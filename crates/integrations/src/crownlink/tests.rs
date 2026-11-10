//! The service against a stand-in crownlink served in-process from a private
//! socket directory, so no test can reach the real daemon or a real phone.

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use crownlink_linux::ipc::proto::{
    self, CallInfo, CallStateChanged, CallStatus, DeviceInfo, DevicesChanged, LinkKind, Topic,
};
use crownos_ipc::{
    Error, EventMsg, Message, MethodMsg, Server, ServiceBuilder, adapter::tokio::serve_with_outbox,
    postcard,
};
use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
    task::JoinHandle,
    time::timeout,
};

use super::*;
use crate::{Wake, bus};

/// Long enough for one reconnect, whose first pause is a second.
const PATIENCE: Duration = Duration::from_secs(5);

const PIXEL: DeviceId = DeviceId([1; 32]);
const ONEPLUS: DeviceId = DeviceId([2; 32]);

type Emit = Box<dyn FnOnce(&mut Server) + Send>;

/// What the stand-in was asked to do.
#[derive(Debug, PartialEq, Eq)]
enum Asked {
    HangUp(String),
    Stop(DeviceId, Feature),
    SetHotspot(DeviceId, bool),
    StartMic(DeviceId),
}

/// A crownlink that lists `devices`, reports every hotspot on and records
/// every command instead of carrying it out.
struct FakeCrownlink {
    events: UnboundedSender<Emit>,
    asked: UnboundedReceiver<Asked>,
    task: JoinHandle<()>,
}

impl FakeCrownlink {
    fn start(directory: &Path, devices: Vec<DeviceInfo>) -> Result<Self, Error> {
        let mut server = ServiceBuilder::new(proto::SERVICE)
            .directory(directory)
            .build()?;
        let (events, mut outbox) = unbounded_channel::<Emit>();
        let (record, asked) = unbounded_channel();
        let task = tokio::spawn(async move {
            let dispatch = move |server: &mut Server, peer, message: Message| {
                let reply = answer(&devices, &record, &message)?;
                server.reply(peer, message.req_id, &reply, Vec::new())?;
                Ok(true)
            };
            let _ = serve_with_outbox(
                &mut server,
                dispatch,
                |cx| outbox.poll_recv(cx),
                |server, emit| emit(server),
            )
            .await;
        });
        Ok(Self {
            events,
            asked,
            task,
        })
    }

    fn emit<E: EventMsg + Send + 'static>(&self, event: E) {
        let _ = self.events.send(Box::new(move |server: &mut Server| {
            let _ = server.emit(&event);
        }));
    }

    async fn next_asked(&mut self) -> Option<Asked> {
        timeout(PATIENCE, self.asked.recv()).await.ok().flatten()
    }

    /// The daemon going away, taking its socket's peers with it.
    async fn stop(self) {
        self.task.abort();
        let _ = self.task.await;
    }
}

fn request<M: MethodMsg<Args = M> + serde::de::DeserializeOwned>(message: &Message) -> Option<M> {
    (message.selector == M::SELECTOR)
        .then(|| postcard::from_bytes(&message.payload).ok())
        .flatten()
}

fn answer(
    devices: &[DeviceInfo],
    record: &UnboundedSender<Asked>,
    message: &Message,
) -> Result<Vec<u8>, Error> {
    if request::<proto::devices>(message).is_some() {
        return Ok(postcard::to_stdvec(devices)?);
    }
    if let Some(asked) = request::<proto::state>(message) {
        let hotspot = postcard::to_stdvec(&(true, None::<&str>))?;
        let document = (asked.topic == Topic::Hotspot).then_some(hotspot);
        return Ok(postcard::to_stdvec(&document)?);
    }
    let asked = request::<proto::hangup_call>(message)
        .map(|asked| Asked::HangUp(asked.call_id))
        .or_else(|| {
            request::<proto::stop>(message).map(|asked| Asked::Stop(asked.id, asked.feature))
        })
        .or_else(|| {
            request::<proto::set_hotspot>(message)
                .map(|asked| Asked::SetHotspot(asked.id, asked.enabled))
        })
        .or_else(|| request::<proto::start_mic>(message).map(|asked| Asked::StartMic(asked.id)));
    if let Some(asked) = asked {
        let _ = record.send(asked);
    }
    Ok(Vec::new())
}

fn phone(id: DeviceId, name: &str, active: &[Feature]) -> DeviceInfo {
    DeviceInfo {
        id,
        name: name.to_owned(),
        class: DeviceClass::Phone,
        trusted: true,
        connected: true,
        link: LinkKind::Lan,
        battery: Some(Battery {
            percent: 82,
            charging: false,
        }),
        features: Feature::ALL.into_iter().collect(),
        active: active.iter().copied().collect(),
    }
}

/// A socket directory no other test, and no real daemon, uses.
fn private_directory() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    std::env::temp_dir().join(format!(
        "crownbar-crownlink-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

fn start_service(directory: &Path) -> Channel {
    let (backend, channel) = bus::connect(PhoneState::default(), &Wake::default());
    tokio::spawn(serve(backend, Some(directory.to_owned())));
    channel
}

async fn until(channel: &Channel, holds: impl Fn(&PhoneState) -> bool) -> Arc<PhoneState> {
    let mut watch = channel.tap().state;
    let reached = timeout(PATIENCE, watch.wait_for(|state| holds(state))).await;
    assert!(
        matches!(reached, Ok(Ok(_))),
        "never reached; last saw {:?}",
        channel.read()
    );
    channel.read()
}

fn names(state: &PhoneState) -> Vec<&str> {
    state.devices.iter().map(|device| &*device.name).collect()
}

#[tokio::test]
async fn no_daemon_means_no_devices() {
    let directory = private_directory();
    let channel = start_service(&directory);
    let state = until(&channel, |state| !state.availability.usable()).await;
    assert!(state.devices.is_empty());
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn every_connection_starts_from_a_fresh_listing() -> Result<(), Error> {
    let directory = private_directory();
    let crownlink = FakeCrownlink::start(
        &directory,
        vec![
            phone(PIXEL, "Pixel 8", &[Feature::Camera]),
            phone(ONEPLUS, "OnePlus 11 5G", &[]),
        ],
    )?;
    let channel = start_service(&directory);

    let state = until(&channel, |state| state.devices.len() == 2).await;
    assert_eq!(names(&state), ["OnePlus 11 5G", "Pixel 8"]);
    assert!(
        state
            .devices
            .iter()
            .all(|device| device.hotspot == Some(true))
    );
    assert!(
        state
            .device(PIXEL)
            .is_some_and(|device| device.is_streaming(Stream::Camera))
    );

    crownlink.stop().await;
    until(&channel, |state| {
        !state.availability.usable() && state.devices.is_empty()
    })
    .await;

    let _crownlink = FakeCrownlink::start(&directory, vec![phone(ONEPLUS, "OnePlus 11 5G", &[])])?;
    let state = until(&channel, |state| state.availability.usable()).await;
    assert_eq!(names(&state), ["OnePlus 11 5G"]);
    let _ = std::fs::remove_dir_all(directory);
    Ok(())
}

#[tokio::test]
async fn controls_reach_the_device_they_name() -> Result<(), Error> {
    let directory = private_directory();
    let mut crownlink = FakeCrownlink::start(
        &directory,
        vec![
            phone(PIXEL, "Pixel 8", &[Feature::Mic]),
            phone(ONEPLUS, "OnePlus 11 5G", &[]),
        ],
    )?;
    let channel = start_service(&directory);
    until(&channel, |state| state.devices.len() == 2).await;

    crownlink.emit(CallStateChanged {
        id: ONEPLUS,
        call: CallInfo {
            call_id: "/org/ofono/call1".to_owned(),
            number: "+447700900123".to_owned(),
            contact_name: Some("Mum".to_owned()),
            status: CallStatus::Active,
            answered_unix_ms: None,
        },
    });
    let state = until(&channel, |state| {
        state
            .device(ONEPLUS)
            .is_some_and(|device| device.call.is_some())
    })
    .await;
    assert!(
        state
            .device(PIXEL)
            .is_some_and(|device| device.call.is_none())
    );

    channel.send(PhoneCommand::HangUp(PIXEL));
    channel.send(PhoneCommand::Stop(DeviceId([9; 32]), Stream::Mic));
    channel.send(PhoneCommand::HangUp(ONEPLUS));
    channel.send(PhoneCommand::Stop(PIXEL, Stream::Mic));
    channel.send(PhoneCommand::SetHotspot(ONEPLUS, false));
    channel.send(PhoneCommand::Start(PIXEL, Stream::Mic));
    assert_eq!(
        crownlink.next_asked().await,
        Some(Asked::HangUp("/org/ofono/call1".to_owned()))
    );
    assert_eq!(
        crownlink.next_asked().await,
        Some(Asked::Stop(PIXEL, Feature::Mic))
    );
    assert_eq!(
        crownlink.next_asked().await,
        Some(Asked::SetHotspot(ONEPLUS, false))
    );
    assert_eq!(crownlink.next_asked().await, Some(Asked::StartMic(PIXEL)));

    crownlink.emit(DevicesChanged {
        devices: vec![phone(PIXEL, "Pixel 8", &[])],
    });
    let state = until(&channel, |state| state.devices.len() == 1).await;
    assert_eq!(names(&state), ["Pixel 8"]);
    assert!(state.availability.usable());

    crownlink.emit(DevicesChanged {
        devices: Vec::new(),
    });
    let state = until(&channel, |state| state.devices.is_empty()).await;
    assert!(state.availability.usable());
    let _ = std::fs::remove_dir_all(directory);
    Ok(())
}

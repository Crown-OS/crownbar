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
    self, CallInfo, CallStateChanged, CallStatus, DeviceInfo, DevicesChanged, LinkKind,
    LiveActivitiesChanged, LiveActivity, LiveProgress, LocalRingChanged, Topic,
};
use crownos_ipc::{
    Error, EventMsg, Kind, Message, MethodMsg, Server, ServiceBuilder,
    adapter::tokio::serve_with_outbox, postcard,
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
    LiveAction(DeviceId, String, String),
    Ring(DeviceId),
    StopRinging(DeviceId),
    StopLocalRing,
    ShareWifi(DeviceId, String),
    RequestWifi(DeviceId, String),
}

/// A crownlink that lists `devices`, reports every hotspot on, an upload
/// going on the first of them and the second ringing this computer, and
/// records every command instead of carrying it out.
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
                if message.kind == Kind::Request {
                    server.reply(peer, message.req_id, &reply, Vec::new())?;
                }
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
    if let Some(asked) = request::<proto::live_activities>(message) {
        let first = devices.first().is_some_and(|device| device.id == asked.id);
        let activities: Vec<_> = first.then(|| upload("photos")).into_iter().collect();
        return Ok(postcard::to_stdvec(&activities)?);
    }
    if request::<proto::local_ring>(message).is_some() {
        return Ok(postcard::to_stdvec(
            &devices.get(1).map(|device| device.id),
        )?);
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
        .or_else(|| request::<proto::start_mic>(message).map(|asked| Asked::StartMic(asked.id)))
        .or_else(|| {
            request::<proto::live_activity_action>(message)
                .map(|asked| Asked::LiveAction(asked.id, asked.activity_id, asked.action_id))
        })
        .or_else(|| request::<proto::ring_device>(message).map(|asked| Asked::Ring(asked.id)))
        .or_else(|| {
            request::<proto::stop_ringing>(message).map(|asked| Asked::StopRinging(asked.id))
        })
        .or_else(|| request::<proto::stop_local_ring>(message).map(|_| Asked::StopLocalRing))
        .or_else(|| {
            request::<proto::share_wifi>(message)
                .map(|asked| Asked::ShareWifi(asked.id, asked.ssid))
        })
        .or_else(|| {
            request::<proto::request_wifi>(message)
                .map(|asked| Asked::RequestWifi(asked.id, asked.ssid))
        });
    if let Some(asked) = asked {
        let _ = record.send(asked);
    }
    Ok(Vec::new())
}

fn upload(id: &str) -> LiveActivity {
    LiveActivity {
        id: id.to_owned(),
        package: "com.google.android.apps.photos".to_owned(),
        app: "Photos".to_owned(),
        title: "Backing up".to_owned(),
        text: "12 of 40".to_owned(),
        short_text: None,
        category: Some("progress".to_owned()),
        progress: Some(LiveProgress {
            value: 12,
            max: 40,
            indeterminate: false,
        }),
        chronometer: None,
        promoted: false,
        color_argb: None,
        actions: vec![proto::LiveAction {
            id: "pause".to_owned(),
            label: "Pause".to_owned(),
            accepts_text: false,
        }],
    }
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

fn activity_ids(state: &PhoneState, id: DeviceId) -> Vec<String> {
    state
        .device(id)
        .map(|device| {
            device
                .activities
                .iter()
                .map(|activity| activity.id.to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn activities_and_a_ring_are_pulled_on_connect_then_followed() -> Result<(), Error> {
    let directory = private_directory();
    let mut crownlink = FakeCrownlink::start(
        &directory,
        vec![
            phone(PIXEL, "Pixel 8", &[]),
            phone(ONEPLUS, "OnePlus 11 5G", &[]),
        ],
    )?;
    let channel = start_service(&directory);

    let state = until(&channel, |state| {
        state.devices.len() == 2 && state.ringing_here.is_some()
    })
    .await;
    assert_eq!(state.ringing_here, Some(ONEPLUS));
    assert_eq!(activity_ids(&state, PIXEL), ["photos"]);
    assert!(activity_ids(&state, ONEPLUS).is_empty());

    crownlink.emit(LiveActivitiesChanged {
        id: ONEPLUS,
        activities: vec![upload("drive")],
    });
    crownlink.emit(LocalRingChanged { by: None });
    let state = until(&channel, |state| {
        state.ringing_here.is_none() && !activity_ids(state, ONEPLUS).is_empty()
    })
    .await;
    assert_eq!(activity_ids(&state, ONEPLUS), ["drive"]);

    channel.send(PhoneCommand::LiveAction(
        ONEPLUS,
        Arc::from("drive"),
        Arc::from("pause"),
    ));
    channel.send(PhoneCommand::RingDevice(DeviceId([9; 32])));
    channel.send(PhoneCommand::RingDevice(PIXEL));
    channel.send(PhoneCommand::StopRinging(PIXEL));
    channel.send(PhoneCommand::StopLocalRing);
    channel.send(PhoneCommand::ShareWifi(PIXEL, Arc::from("Home")));
    channel.send(PhoneCommand::RequestWifi(ONEPLUS, Arc::from("Café")));
    let expected = [
        Asked::LiveAction(ONEPLUS, "drive".to_owned(), "pause".to_owned()),
        Asked::Ring(PIXEL),
        Asked::StopRinging(PIXEL),
        Asked::StopLocalRing,
        Asked::ShareWifi(PIXEL, "Home".to_owned()),
        Asked::RequestWifi(ONEPLUS, "Café".to_owned()),
    ];
    for asked in expected {
        assert_eq!(crownlink.next_asked().await, Some(asked));
    }
    let _ = std::fs::remove_dir_all(directory);
    Ok(())
}

#[tokio::test]
async fn a_crownlink_without_live_activities_still_lists_its_devices() -> Result<(), Error> {
    let directory = private_directory();
    let mut server = ServiceBuilder::new(proto::SERVICE)
        .directory(&directory)
        .build()?;
    let devices = vec![phone(PIXEL, "Pixel 8", &[])];
    let task = tokio::spawn(async move {
        let dispatch = move |server: &mut Server, peer, message: Message| {
            if request::<proto::devices>(&message).is_some() {
                let listing = postcard::to_stdvec(&devices)?;
                server.reply(peer, message.req_id, &listing, Vec::new())?;
            } else if request::<proto::state>(&message).is_some() {
                let document = postcard::to_stdvec(&None::<Vec<u8>>)?;
                server.reply(peer, message.req_id, &document, Vec::new())?;
            } else {
                let refusal = crownos_ipc::RemoteError::unknown_method(message.selector);
                server.reply_err(peer, message.req_id, &refusal)?;
            }
            Ok(true)
        };
        let _ = crownos_ipc::adapter::tokio::serve(&mut server, dispatch).await;
    });
    let channel = start_service(&directory);
    let state = until(&channel, |state| state.devices.len() == 1).await;
    assert!(state.availability.usable());
    assert!(activity_ids(&state, PIXEL).is_empty());
    task.abort();
    let _ = std::fs::remove_dir_all(directory);
    Ok(())
}

//! The BlueZ connection.
//!
//! Event-driven: the adapter reports devices appearing and going, and each
//! paired device reports its own connection and battery changes. What replaced
//! the old `bluetoothctl show` + `devices` + one `info` per device — eight
//! subprocesses every three seconds while the panel was open — is a single
//! `all_properties` round trip for whichever device actually changed.

use std::{collections::HashMap, sync::Arc, time::Duration};

use bluer::{Adapter, Address, AdapterEvent, DeviceEvent, Session};
use futures_util::{stream::SelectAll, StreamExt};
use tokio::time;

use crate::services::{
    bluetooth::{BluetoothCommand, BluetoothState, BtDevice, RadioState},
    bus::{Commands, Publisher},
    rfkill,
    status::{Availability, ErrorKind, Failure, Interest},
};

/// A connect on a headset can take this long before BlueZ gives up; without a
/// bound the command task would hang for the life of the session.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(20);
/// A scan left running past this is a battery drain nobody asked for.
const DISCOVERY_LIMIT: Duration = Duration::from_secs(30);
/// BlueZ mid-restart hands back a stream that ends at once; without a pause
/// that is a hot loop with D-Bus in it.
const RECONNECT_DELAY: Duration = Duration::from_secs(2);

type Events = SelectAll<futures_util::stream::BoxStream<'static, (Address, DeviceEvent)>>;

pub async fn run(publish: Publisher<BluetoothState>, mut commands: Commands<BluetoothCommand>) {
    loop {
        match connect(&publish, &mut commands).await {
            // The command pipe closed: the bar is going away.
            Ok(()) => return,
            Err(reason) => {
                publish.edit(|state| {
                    state.availability = Availability::Unavailable(Arc::from(reason.clone()));
                    state.paired.clear();
                    state.discovered.clear();
                    true
                });
                log::info!("bluetooth: {reason}; retrying");
                time::sleep(RECONNECT_DELAY).await;
            }
        }
    }
}

/// One session, from connect until BlueZ goes away. `Ok` means the bar exited.
async fn connect(
    publish: &Publisher<BluetoothState>,
    commands: &mut Commands<BluetoothCommand>,
) -> Result<(), String> {
    let session = Session::new().await.map_err(|e| e.to_string())?;
    let adapter = session.default_adapter().await.map_err(|e| e.to_string())?;

    let mut adapter_events = adapter.events().await.map_err(|e| e.to_string())?;
    let mut device_events: Events = SelectAll::new();
    let mut discovery = None;
    let mut discovery_deadline = None;
    let mut interest = Interest::Idle;

    refresh(&adapter, publish, &mut device_events).await;

    loop {
        let sleep_until = discovery_deadline.unwrap_or_else(|| time::Instant::now() + DISCOVERY_LIMIT);
        tokio::select! {
            command = commands.recv() => match command {
                // The panel restates its interest on every rebuild, and a
                // rebuild is what a discovered device causes — only a change
                // is worth restarting the scan and re-reading BlueZ.
                Some(BluetoothCommand::Interest(next)) if next == interest => {}
                Some(command) => {
                    if let BluetoothCommand::Interest(next) = command {
                        interest = next;
                    }
                    apply(&adapter, publish, command, &mut discovery, &mut discovery_deadline).await;
                    refresh(&adapter, publish, &mut device_events).await;
                }
                None => return Ok(()),
            },
            event = adapter_events.next() => match event {
                Some(AdapterEvent::PropertyChanged(_) | AdapterEvent::DeviceAdded(_)
                    | AdapterEvent::DeviceRemoved(_)) => {
                    refresh(&adapter, publish, &mut device_events).await;
                }
                None => return Err("the adapter stopped answering".into()),
            },
            Some(_) = device_events.next(), if !device_events.is_empty() => {
                refresh(&adapter, publish, &mut device_events).await;
            }
            // Discovery yields nearby devices as `DeviceAdded` on the adapter
            // stream, so the scan stream itself only has to be kept alive.
            Some(_) = next_discovery(&mut discovery), if discovery.is_some() => {}
            _ = time::sleep_until(sleep_until), if discovery_deadline.is_some() => {
                discovery = None;
                discovery_deadline = None;
                publish.edit(|state| {
                    state.discovering = false;
                    state.discovered.clear();
                    true
                });
            }
        }
    }
}

type Discovery = std::pin::Pin<Box<dyn futures_util::Stream<Item = AdapterEvent> + Send>>;

async fn next_discovery(discovery: &mut Option<Discovery>) -> Option<AdapterEvent> {
    match discovery.as_mut() {
        Some(stream) => stream.next().await,
        None => std::future::pending().await,
    }
}

/// Re-read the adapter and every device it knows.
///
/// The whole list rather than a delta, because BlueZ's per-property events do
/// not say whether a device was added or merely changed, and a list this small
/// — a handful of paired devices — costs one round trip each.
async fn refresh(adapter: &Adapter, publish: &Publisher<BluetoothState>, events: &mut Events) {
    let powered = adapter.is_powered().await.ok();
    let alias = adapter.alias().await.ok();
    let discovering = adapter.is_discovering().await.unwrap_or(false);
    let radio = RadioState::resolve(rfkill::block(rfkill::BLUETOOTH), powered);

    let mut paired = Vec::new();
    let mut discovered = Vec::new();
    let mut watched = HashMap::new();
    for address in adapter.device_addresses().await.unwrap_or_default() {
        let Ok(device) = adapter.device(address) else {
            continue;
        };
        let Ok(properties) = device.all_properties().await else {
            continue;
        };
        let entry = BtDevice::from_properties(address, &properties);
        watched.insert(address, device);
        if entry.paired {
            paired.push(entry);
        } else {
            discovered.push(entry);
        }
    }

    // Resubscribe from scratch: `SelectAll` drops a stream when it ends, so a
    // device that came back needs a fresh one, and rebuilding is cheaper than
    // tracking which of them are still live.
    events.clear();
    for (address, device) in watched {
        if let Ok(stream) = device.events().await {
            events.push(Box::pin(stream.map(move |event| (address, event))));
        }
    }

    publish.edit(|state| {
        let next = BluetoothState {
            availability: Availability::Ready,
            radio,
            adapter: alias.clone(),
            paired: paired.clone(),
            discovered: discovered.clone(),
            discovering,
            // A refresh that agrees with the optimistic change clears it; one
            // that does not is what the failure was there to explain.
            failure: state.failure.clone(),
        };
        let changed = *state != next;
        *state = next;
        state.sort();
        changed
    });
}

async fn apply(
    adapter: &Adapter,
    publish: &Publisher<BluetoothState>,
    command: BluetoothCommand,
    discovery: &mut Option<Discovery>,
    deadline: &mut Option<time::Instant>,
) {
    match command {
        BluetoothCommand::SetPowered(on) => {
            if !publish.read().radio.changeable() {
                return fail(
                    publish,
                    ErrorKind::Unsupported,
                    "a hardware switch is holding the radio off",
                );
            }
            // Move the switch now and let the refresh that follows correct it;
            // waiting on the round trip makes it feel broken.
            publish.edit(|state| {
                let next = if on { RadioState::On } else { RadioState::Off };
                let changed = state.radio != next;
                state.radio = next;
                state.failure = None;
                changed
            });
            report(publish, adapter.set_powered(on).await);
        }
        BluetoothCommand::Connect(address) => {
            busy(publish, address);
            report(publish, link(adapter, address, true).await);
        }
        BluetoothCommand::Disconnect(address) => {
            busy(publish, address);
            report(publish, link(adapter, address, false).await);
        }
        BluetoothCommand::PairAndConnect(address) => {
            busy(publish, address);
            report(publish, pair(adapter, address).await);
        }
        BluetoothCommand::Forget(address) => {
            report(publish, adapter.remove_device(address).await);
        }
        BluetoothCommand::Interest(interest) => {
            match interest {
                Interest::Panel => {
                    // `_with_changes` keeps yielding across a
                    // `Discovering(false)` transition, which the plain stream
                    // does not — and that transition is exactly what a panel
                    // has to survive.
                    match adapter.discover_devices_with_changes().await {
                        Ok(stream) => {
                            *discovery = Some(Box::pin(stream));
                            *deadline = Some(time::Instant::now() + DISCOVERY_LIMIT);
                        }
                        Err(e) => log::info!("could not start discovery: {e}"),
                    }
                }
                // Dropping the stream is what stops the scan; BlueZ has no
                // method for it.
                Interest::Idle => {
                    *discovery = None;
                    *deadline = None;
                    publish.edit(|state| {
                        let changed = !state.discovered.is_empty();
                        state.discovered.clear();
                        changed
                    });
                }
            }
        }
    }
}

async fn link(adapter: &Adapter, address: Address, connect: bool) -> bluer::Result<()> {
    let device = adapter.device(address)?;
    let act = async {
        if connect {
            device.connect().await
        } else {
            device.disconnect().await
        }
    };
    time::timeout(COMMAND_TIMEOUT, act).await.unwrap_or_else(|_| {
        Err(bluer::Error {
            kind: bluer::ErrorKind::Failed,
            message: "timed out".into(),
        })
    })
}

/// Pair, trust, then connect. Trusting is what stops BlueZ asking again on
/// every reconnect, and is what every desktop does on the user's behalf here.
async fn pair(adapter: &Adapter, address: Address) -> bluer::Result<()> {
    let device = adapter.device(address)?;
    time::timeout(COMMAND_TIMEOUT, device.pair())
        .await
        .unwrap_or_else(|_| {
            Err(bluer::Error {
                kind: bluer::ErrorKind::Failed,
                message: "pairing timed out".into(),
            })
        })?;
    device.set_trusted(true).await?;
    device.connect().await
}

fn busy(publish: &Publisher<BluetoothState>, address: Address) {
    publish.edit(|state| {
        state.failure = None;
        state
            .paired
            .iter_mut()
            .chain(state.discovered.iter_mut())
            .find(|device| device.address == address)
            .map(|device| {
                device.busy = true;
                true
            })
            .unwrap_or(false)
    });
}

fn report(publish: &Publisher<BluetoothState>, result: bluer::Result<()>) {
    match result {
        Ok(()) => {
            publish.edit(|state| state.failure.take().is_some());
        }
        Err(e) => {
            let kind = match e.kind {
                bluer::ErrorKind::AuthenticationRejected
                | bluer::ErrorKind::NotAuthorized
                | bluer::ErrorKind::NotPermitted => ErrorKind::NotAuthorized,
                bluer::ErrorKind::NotFound | bluer::ErrorKind::DoesNotExist => ErrorKind::NotFound,
                bluer::ErrorKind::InProgress | bluer::ErrorKind::AlreadyConnected => ErrorKind::Busy,
                bluer::ErrorKind::NotSupported => ErrorKind::Unsupported,
                _ => ErrorKind::Backend,
            };
            log::info!("bluetooth command failed: {e}");
            publish.edit(|state| {
                state.failure = Some(Failure::new(kind, e.to_string()));
                true
            });
        }
    }
}

fn fail(publish: &Publisher<BluetoothState>, kind: ErrorKind, detail: &str) {
    publish.edit(|state| {
        state.failure = Some(Failure::new(kind, detail));
        true
    });
}

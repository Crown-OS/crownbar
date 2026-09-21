//! The NetworkManager connection.
//!
//! Every event NetworkManager emits means "look again" rather than carrying a
//! delta — the stream is deliberately lossy — so the service always re-derives
//! the whole snapshot. A leading debounce keeps a busy band, which reports
//! access-point changes several times a second, from turning that into a
//! stream of D-Bus round trips.

use std::{sync::Arc, time::Duration};

use futures_util::{FutureExt, StreamExt};
use nmrs::{ConnectionError, NetworkManager, WifiSecurity};
use tokio::time;

use crate::services::{
    bus::{Commands, Publisher},
    network::{link, Joined, NetworkCommand, NetworkState, Radio, WifiNetwork, MAX_OTHER},
    status::{Availability, ErrorKind, Failure, Interest},
};

/// Leading, not trailing: a busy radio then delays a refresh by at most this
/// rather than starving it forever.
const DEBOUNCE: Duration = Duration::from_millis(350);
/// A daemon mid-restart hands back a stream that ends at once.
const RECONNECT_DELAY: Duration = Duration::from_secs(2);
/// How often to re-scan while the panel is up. Never on the bar tick.
const SCAN_PERIOD: Duration = Duration::from_secs(10);
/// Joining blocks on association and DHCP.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(45);

pub async fn run(publish: Publisher<NetworkState>, mut commands: Commands<NetworkCommand>) {
    loop {
        match connect(&publish, &mut commands).await {
            Ok(()) => return,
            Err(reason) => {
                publish.edit(|state| {
                    state.availability = Availability::Unavailable(Arc::from(reason.clone()));
                    state.connected = None;
                    state.known.clear();
                    state.others.clear();
                    state.fallback_quality = link::quality().unwrap_or(0.0);
                    true
                });
                log::info!("network: {reason}; retrying");
                time::sleep(RECONNECT_DELAY).await;
            }
        }
    }
}

async fn connect(
    publish: &Publisher<NetworkState>,
    commands: &mut Commands<NetworkCommand>,
) -> Result<(), String> {
    let nm = NetworkManager::new().await.map_err(|e| e.to_string())?;
    let mut events = nm.network_events().await.map_err(|e| e.to_string())?;
    let mut interest = Interest::Idle;
    let mut settled = time::Instant::now();

    refresh(&nm, publish).await;

    loop {
        let scan_at = settled + SCAN_PERIOD;
        tokio::select! {
            command = commands.recv() => match command {
                Some(NetworkCommand::Interest(next)) => {
                    interest = next;
                    if interest == Interest::Panel {
                        scan(&nm, publish).await;
                        settled = time::Instant::now();
                    }
                }
                Some(command) => {
                    apply(&nm, publish, command).await;
                    refresh(&nm, publish).await;
                }
                None => return Ok(()),
            },
            event = events.next() => match event {
                // A failed event is still a signal that something moved; the
                // refresh below is what actually reads the state.
                Some(_) => {
                    // Coalesce the burst a single scan produces, then take one
                    // snapshot of whatever it settled into.
                    time::sleep(DEBOUNCE).await;
                    while events.next().now_or_never().flatten().is_some() {}
                    refresh(&nm, publish).await;
                }
                None => return Err("NetworkManager stopped answering".into()),
            },
            _ = time::sleep_until(scan_at), if interest == Interest::Panel => {
                scan(&nm, publish).await;
                settled = time::Instant::now();
            }
        }
    }
}

async fn scan(nm: &NetworkManager, publish: &Publisher<NetworkState>) {
    publish.edit(|state| {
        let changed = !state.scanning;
        state.scanning = true;
        changed
    });
    if let Err(e) = nm.scan_networks(None).await {
        log::info!("could not start a scan: {e}");
    }
    refresh(nm, publish).await;
}

/// Re-derive the whole snapshot. NetworkManager's events do not say what
/// changed, and nmrs has already grouped access points by SSID, so this is one
/// listing plus two property reads.
async fn refresh(nm: &NetworkManager, publish: &Publisher<NetworkState>) {
    let radio = match nm.wifi_state().await {
        Ok(state) if !state.present => Radio::Absent,
        Ok(state) if !state.hardware_enabled => Radio::HardBlocked,
        Ok(state) if state.enabled => Radio::On,
        Ok(_) => Radio::Off,
        Err(_) => Radio::Unknown,
    };

    let joined = nm.current_network().await.ok().flatten().map(|network| Joined {
        ssid: network.ssid.clone(),
        strength: strength(network.strength),
        secured: network.secured,
        weak: weak(&network),
        ip4: network.ip4_address.clone(),
    });

    let mut known = Vec::new();
    let mut others = Vec::new();
    if radio.on() {
        for network in nm.list_networks(None).await.unwrap_or_default() {
            if network.is_active || joined.as_ref().is_some_and(|j| j.ssid == network.ssid) {
                continue;
            }
            // A hotspot the machine itself is running is not a network to join.
            if network.is_hotspot {
                continue;
            }
            let entry = WifiNetwork {
                strength: strength(network.strength),
                secured: network.secured,
                enterprise: network.is_eap,
                weak: weak(&network),
                known: network.known,
                ssid: network.ssid,
            };
            if entry.known { &mut known } else { &mut others }.push(entry);
        }
        for list in [&mut known, &mut others] {
            list.sort_by(|a, b| b.strength.total_cmp(&a.strength));
        }
        others.truncate(MAX_OTHER);
    }

    publish.edit(|state| {
        let next = NetworkState {
            availability: Availability::Ready,
            radio,
            connected: joined.clone(),
            known: known.clone(),
            others: others.clone(),
            scanning: false,
            hotspot: state.hotspot.clone(),
            failure: state.failure.clone(),
            fallback_quality: state.fallback_quality,
        };
        let changed = *state != next;
        *state = next;
        changed
    });
}

fn strength(value: Option<u8>) -> f32 {
    value.unwrap_or(0) as f32 / 100.0
}

/// WEP or TKIP. Joinable, but every other desktop flags it.
fn weak(network: &nmrs::Network) -> bool {
    let features = &network.security_features;
    features.wep40 || features.wep104 || features.tkip
}

async fn apply(nm: &NetworkManager, publish: &Publisher<NetworkState>, command: NetworkCommand) {
    match command {
        NetworkCommand::SetRadio(on) => {
            if !publish.read().radio.changeable() {
                return fail(
                    publish,
                    ErrorKind::Unsupported,
                    "a hardware switch is holding the radio off",
                );
            }
            // Move the switch now; the refresh that follows corrects it.
            publish.edit(|state| {
                let next = if on { Radio::On } else { Radio::Off };
                let changed = state.radio != next;
                state.radio = next;
                state.failure = None;
                if !on {
                    state.connected = None;
                    state.known.clear();
                    state.others.clear();
                }
                changed
            });
            report(publish, nm.set_wireless_enabled(on).await);
        }
        NetworkCommand::Scan => scan(nm, publish).await,
        NetworkCommand::Connect(ssid) => {
            publish.edit(|state| {
                state.failure = None;
                state.scanning = true;
                true
            });
            // Open, or already saved — the panel never sends anything that
            // would need a password typed here.
            let result = time::timeout(COMMAND_TIMEOUT, nm.connect(&ssid, None, WifiSecurity::Open))
                .await
                .unwrap_or(Err(ConnectionError::Timeout));
            report(publish, result);
        }
        NetworkCommand::Disconnect => report(publish, nm.disconnect(None).await),
        NetworkCommand::Forget(ssid) => report(publish, nm.forget(&ssid).await),
        NetworkCommand::Interest(_) => {}
    }
}

fn report<T>(publish: &Publisher<NetworkState>, result: nmrs::Result<T>) {
    match result {
        Ok(_) => {
            publish.edit(|state| state.failure.take().is_some());
        }
        Err(e) => {
            let kind = match &e {
                ConnectionError::Timeout | ConnectionError::SupplicantTimeout => ErrorKind::Timeout,
                ConnectionError::NotFound
                | ConnectionError::NoWifiDevice
                | ConnectionError::NoSavedConnection
                | ConnectionError::SavedConnectionNotFound(_) => ErrorKind::NotFound,
                ConnectionError::AuthFailed | ConnectionError::MissingPassword => {
                    ErrorKind::NotAuthorized
                }
                ConnectionError::WifiNotReady | ConnectionError::Stuck(_) => ErrorKind::Busy,
                _ => ErrorKind::Backend,
            };
            log::info!("network command failed: {e}");
            publish.edit(|state| {
                state.failure = Some(Failure::new(kind, e.to_string()));
                true
            });
        }
    }
}

fn fail(publish: &Publisher<NetworkState>, kind: ErrorKind, detail: &str) {
    publish.edit(|state| {
        state.failure = Some(Failure::new(kind, detail));
        true
    });
}

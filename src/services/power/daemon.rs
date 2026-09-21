//! power-profiles-daemon, over D-Bus.
//!
//! The daemon pushes `PropertiesChanged`, so the profile the panel shows is
//! never stale and a change made elsewhere — a keyboard shortcut, another
//! desktop's applet — shows up at once. That is what the old
//! `powerprofilesctl list` catch-up poll was standing in for.

use std::collections::HashMap;

use futures_util::StreamExt;
use zbus::{proxy, zvariant::OwnedValue, Connection};

use crate::services::power::profile::{Profile, Profiles};

/// ppd ≥ 0.20 renamed itself; both names are served by the same daemon and
/// older builds answer only to the first.
const SERVICES: [(&str, &str); 2] = [
    ("net.hadess.PowerProfiles", "/net/hadess/PowerProfiles"),
    (
        "org.freedesktop.UPower.PowerProfiles",
        "/org/freedesktop/UPower/PowerProfiles",
    ),
];

#[proxy(interface = "net.hadess.PowerProfiles", gen_blocking = false)]
pub trait PowerProfiles {
    #[zbus(property)]
    fn active_profile(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn set_active_profile(&self, profile: &str) -> zbus::Result<()>;

    #[zbus(property)]
    fn profiles(&self) -> zbus::Result<Vec<HashMap<String, OwnedValue>>>;

    #[zbus(property)]
    fn performance_degraded(&self) -> zbus::Result<String>;
}

/// Connect to whichever name this machine's daemon answers to.
pub async fn connect(bus: &Connection) -> zbus::Result<PowerProfilesProxy<'static>> {
    let mut last = None;
    for (service, path) in SERVICES {
        match PowerProfilesProxy::builder(bus)
            .destination(service)?
            .path(path)?
            .build()
            .await
        {
            // Building a proxy does not talk to the bus, so a read is what
            // actually decides whether the daemon is there.
            Ok(proxy) => match proxy.active_profile().await {
                Ok(_) => return Ok(proxy),
                Err(e) => last = Some(e),
            },
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or(zbus::Error::InvalidReply))
}

pub async fn read(proxy: &PowerProfilesProxy<'_>) -> zbus::Result<Profiles> {
    let supported = proxy
        .profiles()
        .await?
        .iter()
        .filter_map(|entry| entry.get("Profile")?.downcast_ref::<String>().ok())
        .filter_map(|id| Profile::from_id(&id))
        .collect();
    let degraded = proxy.performance_degraded().await?;
    Ok(Profiles {
        supported,
        active: Profile::from_id(&proxy.active_profile().await?),
        degraded: (!degraded.is_empty()).then_some(degraded),
    })
}

/// Anything the daemon changes on its own. Yields once per change; the caller
/// re-reads, because the three properties move together often enough that
/// tracking them separately would only add races.
pub async fn changes<'a>(proxy: &'a PowerProfilesProxy<'_>) -> impl StreamExt<Item = ()> + 'a {
    let active = proxy.receive_active_profile_changed().await;
    let degraded = proxy.receive_performance_degraded_changed().await;
    futures_util::stream::select(active.map(|_| ()), degraded.map(|_| ()))
}

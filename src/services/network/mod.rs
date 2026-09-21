//! Wi-Fi, over NetworkManager.
//!
//! The read model is already in the shape the panel draws — the joined
//! network, then known ones, then strangers — so a widget walks it rather than
//! filtering it. nmrs groups access points by SSID itself, which is what the
//! old hand-rolled dedupe in `util::network` existed to do.

pub mod link;
mod manager;

use std::sync::Arc;

use crate::services::{
    bus::Backend,
    status::{Availability, Failure, Interest},
};

/// How many strangers the panel lists. A café produces dozens; this is a bar.
pub const MAX_OTHER: usize = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Radio {
    #[default]
    Unknown,
    /// No Wi-Fi hardware at all.
    Absent,
    /// rfkill is holding it down; the toggle is drawn but cannot be pressed.
    HardBlocked,
    Off,
    On,
}

impl Radio {
    pub fn on(self) -> bool {
        matches!(self, Self::On)
    }

    pub fn changeable(self) -> bool {
        matches!(self, Self::Off | Self::On)
    }
}

/// A network the panel can list.
#[derive(Clone, Debug, PartialEq)]
pub struct WifiNetwork {
    pub ssid: String,
    /// ∈ [0, 1].
    pub strength: f32,
    pub secured: bool,
    /// WPA-Enterprise. The bar cannot collect a certificate or an identity, so
    /// these rows hand off to settings instead of trying to join.
    pub enterprise: bool,
    /// WEP or TKIP — joinable, but worth saying so.
    pub weak: bool,
    /// A saved profile exists, so joining needs no password from us.
    pub known: bool,
}

/// The network currently joined.
#[derive(Clone, Debug, PartialEq)]
pub struct Joined {
    pub ssid: String,
    pub strength: f32,
    pub secured: bool,
    pub weak: bool,
    pub ip4: Option<String>,
}

/// Personal Hotspot. NetworkManager can do this — an AP-mode profile with
/// shared IPv4 — but owning it belongs to crownconnect, so the surface is here
/// and the body is not.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum HotspotState {
    /// No AP-capable device, or nothing has looked yet.
    #[default]
    Unsupported,
    Off,
    On {
        ssid: String,
        clients: u32,
    },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NetworkState {
    pub availability: Availability,
    pub radio: Radio,
    pub connected: Option<Joined>,
    /// In range, saved, not the joined one. Strongest first.
    pub known: Vec<WifiNetwork>,
    /// In range, never saved. Strongest first, already capped.
    pub others: Vec<WifiNetwork>,
    pub scanning: bool,
    pub hotspot: HotspotState,
    pub failure: Option<Failure>,
    /// Carrier quality straight from the kernel, for the icon before
    /// NetworkManager has answered.
    fallback_quality: f32,
}

impl NetworkState {
    /// What the bar icon draws ∈ [0, 1].
    pub fn quality(&self) -> f32 {
        match &self.connected {
            Some(joined) => joined.strength,
            None if self.availability.usable() => 0.0,
            None => self.fallback_quality,
        }
    }

    /// Joining needs no password from us: the network is open, or a saved
    /// profile already holds one. Anything else goes to settings.
    pub fn joinable(&self, ssid: &str) -> bool {
        self.known
            .iter()
            .chain(self.others.iter())
            .find(|network| network.ssid == ssid)
            .map(|network| network.known || !network.secured)
            .unwrap_or(false)
    }
}

#[derive(Clone, Debug)]
pub enum NetworkCommand {
    SetRadio(bool),
    Scan,
    /// Only for a network [`NetworkState::joinable`] reports; the panel sends
    /// anything else to crownsettings, which has somewhere to type.
    Connect(String),
    Disconnect,
    Forget(String),
    Interest(Interest),
}

pub type Channel = crate::services::bus::Channel<NetworkState, NetworkCommand>;

pub async fn run(backend: Backend<NetworkState, NetworkCommand>) {
    let Backend {
        publish,
        commands,
    } = backend;

    // The kernel answers before NetworkManager does, and keeps answering if it
    // dies, so the icon is never wrong for lack of a daemon.
    let present = link::interface().is_some();
    publish.edit(|state| {
        state.fallback_quality = link::quality().unwrap_or(0.0);
        state.radio = if present { Radio::Unknown } else { Radio::Absent };
        state.availability = if present {
            Availability::Starting
        } else {
            Availability::Unavailable(Arc::from("no wireless device"))
        };
        true
    });
    if !present {
        return;
    }

    manager::run(publish, commands).await;
}

//! The phone link, over crownlink's IPC.
//!
//! crownlink owns every linked device; the bar mirrors the ones connected right
//! now — their battery, their hotspot, what they stream here, an answered
//! call and their ongoing activities — and asks crownlink to start and stop
//! things on them. A ringing call is crownotify's card, not the bar's; a phone
//! ringing this computer to find it is the bar's.
//!
//! crownlink may not be installed, may start after the bar and may be
//! restarted under it, so the loop reconnects for as long as the bar lives and
//! shows no devices meanwhile. Its events are neither replayed nor reliable,
//! so every connection starts from a fresh listing and every new listing is
//! taken as the whole truth. Commands issued while it is down are dropped: a
//! hang-up asked of a daemon that is not there has nothing left to end.
//!
//! Every device is its own entry, keyed by its id, so any number of phones —
//! none included — is just a longer or shorter list.

mod activity;
mod call;
mod device;
mod roster;
mod session;
mod stream;
mod topic;

use std::{path::PathBuf, sync::Arc};

pub use activity::{LiveButton, LiveClock, PhoneActivity};
pub use call::PhoneCall;
pub use crownlink_linux::ipc::proto::{Battery, DeviceClass, DeviceId, Feature, FeatureSet};
pub use device::PhoneDevice;
pub use stream::Stream;

use tokio::time::Instant;

use crate::{
    bus::{Backend, Publisher},
    crownlink::session::Session,
    reconnect::Backoff,
    status::Availability,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PhoneState {
    pub availability: Availability,
    /// Every connected device, by name. Empty whenever crownlink is not
    /// running.
    pub devices: Vec<PhoneDevice>,
    /// The device ringing this computer to find it, which may since have gone.
    pub ringing_here: Option<DeviceId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PhoneCommand {
    Start(DeviceId, Stream),
    Stop(DeviceId, Stream),
    /// End the device's answered call.
    HangUp(DeviceId),
    /// Turn the device's mobile hotspot on or off.
    SetHotspot(DeviceId, bool),
    /// Press a button of one of the device's live activities: the activity's
    /// id, then the button's.
    LiveAction(DeviceId, Arc<str>, Arc<str>),
    /// Ring the device at full volume, even when it is silenced.
    RingDevice(DeviceId),
    StopRinging(DeviceId),
    /// Silence this computer, rung by whichever device.
    StopLocalRing,
    /// Send this computer's saved password for a network to the device.
    ShareWifi(DeviceId, Arc<str>),
    /// Ask the device for a network's password, to save it here.
    RequestWifi(DeviceId, Arc<str>),
}

impl PhoneCommand {
    /// The device the command is about; `None` for one about this computer.
    pub fn device(&self) -> Option<DeviceId> {
        match self {
            Self::Start(id, _)
            | Self::Stop(id, _)
            | Self::HangUp(id)
            | Self::SetHotspot(id, _)
            | Self::LiveAction(id, ..)
            | Self::RingDevice(id)
            | Self::StopRinging(id)
            | Self::ShareWifi(id, _)
            | Self::RequestWifi(id, _) => Some(*id),
            Self::StopLocalRing => None,
        }
    }
}

pub type Channel = crate::bus::Channel<PhoneState, PhoneCommand>;

pub async fn run(backend: Backend<PhoneState, PhoneCommand>) {
    serve(backend, None).await;
}

/// [`run`], against a crownlink listening in `directory` rather than in the
/// session's runtime directory.
async fn serve(backend: Backend<PhoneState, PhoneCommand>, directory: Option<PathBuf>) {
    let Backend {
        publish,
        mut commands,
    } = backend;

    let mut backoff = Backoff::default();
    loop {
        let started = Instant::now();
        if let Some(session) = Session::open(directory.as_deref()).await
            && !session.run(&publish, &mut commands).await
        {
            return;
        }
        backoff.reset_if_held(started);
        offline(&publish);
        if !backoff.wait_discarding(&mut commands).await {
            return;
        }
    }
}

/// crownlink is not answering, so no device is linked as far as the bar can
/// tell: every row and chip goes.
fn offline(publish: &Publisher<PhoneState>) {
    publish.edit(|state| {
        let reason = Availability::Unavailable(Arc::from("crownlink is not running"));
        let changed = state.availability != reason
            || !state.devices.is_empty()
            || state.ringing_here.is_some();
        state.availability = reason;
        state.devices.clear();
        state.ringing_here = None;
        changed
    });
}

#[cfg(test)]
mod tests;

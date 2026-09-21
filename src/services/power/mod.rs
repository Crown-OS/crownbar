//! Power profiles.
//!
//! Split from [`crate::services::battery`] because the two answer to different
//! things — the cell is sysfs, the profile is a daemon — and because caffeine
//! will join this module rather than the battery's.

mod daemon;
mod profile;

use std::sync::Arc;

use futures_util::StreamExt;
use zbus::Connection;

use crate::services::{
    bus::Backend,
    status::{Availability, ErrorKind, Failure},
};

pub use profile::{Profile, Profiles};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PowerState {
    pub availability: Availability,
    pub profiles: Profiles,
    /// The last profile switch the daemon refused. Cleared by the next one
    /// that works, so a panel can say why the cell snapped back.
    pub failure: Option<Failure>,
}

#[derive(Clone, Copy, Debug)]
pub enum PowerCommand {
    SetProfile(Profile),
}

pub type Channel = crate::services::bus::Channel<PowerState, PowerCommand>;

pub async fn run(backend: Backend<PowerState, PowerCommand>) {
    let Backend {
        publish,
        mut commands,
    } = backend;

    let unavailable = |reason: String| {
        publish.edit(|state| {
            state.availability = Availability::Unavailable(Arc::from(reason));
            true
        });
    };

    let bus = match Connection::system().await {
        Ok(bus) => bus,
        Err(e) => return unavailable(e.to_string()),
    };
    let proxy = match daemon::connect(&bus).await {
        Ok(proxy) => proxy,
        Err(e) => return unavailable(e.to_string()),
    };

    let mut changes = std::pin::pin!(daemon::changes(&proxy).await);
    loop {
        match daemon::read(&proxy).await {
            Ok(profiles) => publish.edit(|state| {
                let changed = state.profiles != profiles
                    || !matches!(state.availability, Availability::Ready);
                state.profiles = profiles;
                state.availability = Availability::Ready;
                changed
            }),
            Err(e) => publish.edit(|state| {
                state.availability = Availability::Degraded(Arc::from(e.to_string()));
                true
            }),
        };

        tokio::select! {
            command = commands.recv() => match command {
                Some(PowerCommand::SetProfile(profile)) => set(&proxy, &publish, profile).await,
                None => return,
            },
            change = changes.next() => if change.is_none() {
                // The daemon went away mid-session; without the stream there is
                // nothing left to wait on.
                return unavailable("power-profiles-daemon stopped".into());
            },
        }
    }
}

/// Show the choice at once — the cell starts travelling toward its new colour
/// while the daemon is still being told — then let the reply correct it. This
/// is the optimistic write the widgets used to do by hand, except that here a
/// refusal is visible instead of being dropped on a detached thread.
async fn set(
    proxy: &daemon::PowerProfilesProxy<'_>,
    publish: &crate::services::bus::Publisher<PowerState>,
    profile: Profile,
) {
    publish.edit(|state| {
        let changed = state.profiles.active != Some(profile) || state.failure.is_some();
        state.profiles.active = Some(profile);
        state.failure = None;
        changed
    });

    if let Err(e) = proxy.set_active_profile(profile.id()).await {
        let kind = match &e {
            zbus::Error::MethodError(name, ..) if name.contains("AccessDenied") => {
                ErrorKind::NotAuthorized
            }
            _ => ErrorKind::Backend,
        };
        log::info!("could not switch to the {} profile: {e}", profile.id());
        publish.edit(|state| {
            state.failure = Some(Failure::new(kind, e.to_string()));
            true
        });
    }
}

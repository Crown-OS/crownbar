//! The weather, from Open-Meteo.
//!
//! The only service that leaves the machine. It is therefore the only one that
//! can fail for reasons nothing on the machine can fix — no link, a service
//! that is down, a rate limit — so every failure is reported as a reason on
//! the snapshot rather than retried into the ground: a refresh that fails
//! waits for the next tick like any other.
//!
//! Position is resolved once per run from the public address and then cached,
//! because it is the part a user would least like repeated.

mod api;
mod condition;

use std::{sync::Arc, time::Duration};

pub use api::{Current, Outlook, Place, DAYS};
pub use condition::Condition;

use tokio::time;

use crate::services::{
    bus::{Backend, Publisher},
    status::{Availability, Interest},
};

/// How often to ask while nobody is looking. The sky does not move fast and
/// the service is free and unauthenticated; this is as often as is polite.
const IDLE_REFRESH: Duration = Duration::from_secs(15 * 60);
/// How often while the panel is open, so a reading is never stale in front of
/// somebody who is watching it.
const PANEL_REFRESH: Duration = Duration::from_secs(2 * 60);
/// How long to wait after a failed refresh before trying again.
const RETRY: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WeatherState {
    pub availability: Availability,
    /// Where the reading is for. `None` until the position resolves.
    pub place: Option<Arc<str>>,
    pub current: Option<Current>,
    /// Today first, then the next few days.
    pub outlook: Vec<Outlook>,
}

#[derive(Clone, Copy, Debug)]
pub enum WeatherCommand {
    /// Ask now — the panel opening, or the user asking for it.
    Refresh,
    Interest(Interest),
}

pub type Channel = crate::services::bus::Channel<WeatherState, WeatherCommand>;

pub async fn run(backend: Backend<WeatherState, WeatherCommand>) {
    let Backend {
        publish,
        mut commands,
    } = backend;

    let client = match api::client() {
        Ok(client) => client,
        Err(error) => {
            fail(&publish, format!("no HTTP client: {error}"));
            return;
        }
    };

    let mut place: Option<Place> = None;
    let mut interest = Interest::Idle;
    // Due immediately: the first reading should be on the bar before the
    // first refresh interval has elapsed.
    let mut due = Duration::ZERO;

    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(WeatherCommand::Interest(next)) => {
                    interest = next;
                    continue;
                }
                Some(WeatherCommand::Refresh) => {}
                None => return,
            },
            _ = time::sleep(due) => {}
        }

        due = match refresh(&client, &publish, &mut place).await {
            true if interest == Interest::Panel => PANEL_REFRESH,
            true => IDLE_REFRESH,
            false => RETRY,
        };
    }
}

/// One round trip. Returns whether it landed.
async fn refresh(
    client: &reqwest::Client,
    publish: &Publisher<WeatherState>,
    place: &mut Option<Place>,
) -> bool {
    if place.is_none() {
        match api::locate(client).await {
            Ok(found) => *place = Some(found),
            Err(error) => {
                log::info!("could not work out where this machine is: {error}");
                fail(publish, "cannot determine location".to_string());
                return false;
            }
        }
    }
    let Some(here) = place.as_ref() else {
        return false;
    };

    match api::forecast(client, here).await {
        Ok((current, outlook)) => {
            let name: Arc<str> = Arc::from(here.name.as_str());
            publish.edit(|state| {
                let changed = state.availability != Availability::Ready
                    || state.current != Some(current)
                    || state.outlook != outlook
                    || state.place.as_deref() != Some(name.as_ref());
                state.availability = Availability::Ready;
                state.place = Some(name.clone());
                state.current = Some(current);
                state.outlook = outlook.clone();
                changed
            });
            true
        }
        Err(error) => {
            log::info!("the forecast did not arrive: {error}");
            fail(publish, "no forecast".to_string());
            false
        }
    }
}

/// Report a failure without throwing away a reading that is merely old: a
/// number from twenty minutes ago is worth far more than an empty pill.
fn fail(publish: &Publisher<WeatherState>, why: String) {
    publish.edit(|state| {
        let reason = match state.current.is_some() {
            true => Availability::Degraded(Arc::from(why.as_str())),
            false => Availability::Unavailable(Arc::from(why.as_str())),
        };
        let changed = state.availability != reason;
        state.availability = reason;
        changed
    });
}

//! Keeping the screen awake.
//!
//! Unlike every other service this one has nothing to read: no daemon owns the
//! answer, the bar does. What it owns is the *intent* — on, off, or on until a
//! deadline — and the expiry timer behind it. Turning that intent into a
//! Wayland inhibitor happens on the event loop, in [`reconcile`], because the
//! object has to be created against the bar's own surface.
//!
//! The mechanism is `zwp_idle_inhibit_manager_v1`. It is held by an object
//! rather than by a subprocess or a daemon call, so a bar that crashes cannot
//! leave the machine awake with no way to turn it off.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use crownshell::App;
use tokio::time;

use crate::services::{bus::Backend, status::Availability};

/// How long "keep awake for a while" lasts, for the panel's preset rows.
pub const PRESETS: [(&str, Duration); 3] = [
    ("For 30 Minutes", Duration::from_secs(30 * 60)),
    ("For 1 Hour", Duration::from_secs(60 * 60)),
    ("For 4 Hours", Duration::from_secs(4 * 60 * 60)),
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CaffeineState {
    pub availability: Availability,
    pub active: bool,
    /// When a timed session ends. `None` while off, or on indefinitely.
    pub until: Option<Instant>,
}

impl CaffeineState {
    /// What the panel says under the switch.
    pub fn remaining(&self) -> Option<Duration> {
        self.until
            .map(|until| until.saturating_duration_since(Instant::now()))
    }
}

#[derive(Clone, Copy, Debug)]
pub enum CaffeineCommand {
    Toggle,
    SetActive(bool),
    /// Stay awake for a while, then release without being asked.
    SetActiveFor(Duration),
    /// The event loop reporting whether the compositor implements the
    /// protocol. Nothing else can know.
    Supported(bool),
}

pub type Channel = crate::services::bus::Channel<CaffeineState, CaffeineCommand>;

pub async fn run(backend: Backend<CaffeineState, CaffeineCommand>) {
    let Backend {
        publish,
        mut commands,
    } = backend;

    loop {
        let deadline = publish.read().until;
        tokio::select! {
            command = commands.recv() => match command {
                Some(command) => apply(&publish, command),
                None => return,
            },
            // A timed session running out is the only thing that changes this
            // service without being asked to.
            _ = sleep_until(deadline) => {
                publish.edit(|state| {
                    state.active = false;
                    state.until = None;
                    true
                });
            }
        }
    }
}

async fn sleep_until(deadline: Option<Instant>) {
    match deadline {
        Some(instant) => time::sleep_until(instant.into()).await,
        None => std::future::pending().await,
    }
}

fn apply(publish: &crate::services::bus::Publisher<CaffeineState>, command: CaffeineCommand) {
    publish.edit(|state| match command {
        CaffeineCommand::Supported(true) => {
            let changed = state.availability != Availability::Ready;
            state.availability = Availability::Ready;
            changed
        }
        CaffeineCommand::Supported(false) => {
            let reason = Availability::Unavailable(Arc::from(
                "the compositor does not support idle inhibition",
            ));
            let changed = state.availability != reason;
            state.availability = reason;
            state.active = false;
            state.until = None;
            changed
        }
        CaffeineCommand::Toggle => {
            state.active = !state.active;
            state.until = None;
            true
        }
        CaffeineCommand::SetActive(active) => {
            let changed = state.active != active || state.until.is_some();
            state.active = active;
            state.until = None;
            changed
        }
        CaffeineCommand::SetActiveFor(duration) => {
            state.active = true;
            state.until = Some(Instant::now() + duration);
            true
        }
    });
}

/// Make the compositor agree with the intent.
///
/// Runs on the event loop, where `App` lives. Both halves are idempotent, so
/// this is safe to call on every wake-up however little changed.
pub fn reconcile(channel: &Channel, app: &mut App) {
    let state = channel.read();
    if matches!(state.availability, Availability::Starting) {
        channel.send(CaffeineCommand::Supported(app.supports_idle_inhibit()));
    }
    if state.active != app.is_idle_inhibited() {
        app.set_idle_inhibited(state.active);
    }
}

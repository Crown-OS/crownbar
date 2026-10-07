//! Warming the screen after dark.
//!
//! Like [`super::caffeine`], this service has nothing to read: no daemon owns
//! the answer, the bar does. What it owns is the *intent* — off, or on at a
//! colour temperature — and it holds the gamma ramps that carry it out on a
//! Wayland connection of its own, since they belong to the outputs rather
//! than to any surface.
//!
//! The mechanism is `zwlr_gamma_control_manager_v1`. It is a scanout lookup
//! table, so the tint costs nothing per frame, covers every client on the
//! screen and cannot outlive the process that asked for it.

mod connection;
mod ramp;

use std::{sync::Arc, time::Duration};

use tokio::time::{self, Instant};

use crate::services::{
    bus::{Backend, Commands, Publisher},
    status::Availability,
};
use connection::GammaConnection;
pub use ramp::{NEUTRAL_KELVIN, WARMEST_KELVIN};

/// Least time between two ramps sent to the compositor. Each one is a
/// colour-table commit on every output, so a slider drag is thinned to this
/// rate and always ends on the value it was released at.
const RAMP_INTERVAL: Duration = Duration::from_millis(50);

/// Where the slider starts on a screen that has never been warmed. Around the
/// warmth of an incandescent bulb, which is what most night-lights default to.
const DEFAULT_KELVIN: u16 = 3400;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NightLightState {
    pub availability: Availability,
    pub active: bool,
    /// The temperature the tint is held at while active, and the one it will
    /// resume at when it is turned back on.
    pub kelvin: u16,
}

impl Default for NightLightState {
    fn default() -> Self {
        Self {
            availability: Availability::default(),
            active: false,
            kelvin: DEFAULT_KELVIN,
        }
    }
}

impl NightLightState {
    /// Slider position ∈ [0, 1]: left is neutral daylight, right is warmest.
    pub fn warmth(&self) -> f32 {
        let span = (NEUTRAL_KELVIN - WARMEST_KELVIN) as f32;
        ((NEUTRAL_KELVIN - self.kelvin.clamp(WARMEST_KELVIN, NEUTRAL_KELVIN)) as f32 / span)
            .clamp(0.0, 1.0)
    }

    /// The temperature a slider at `warmth` asks for.
    pub fn kelvin_at(warmth: f32) -> u16 {
        let span = (NEUTRAL_KELVIN - WARMEST_KELVIN) as f32;
        NEUTRAL_KELVIN - (warmth.clamp(0.0, 1.0) * span) as u16
    }

    /// What the outputs should be held at.
    fn target(&self) -> Option<u16> {
        self.active.then_some(self.kelvin)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum NightLightCommand {
    Toggle,
    SetActive(bool),
    /// Warm to this temperature, turning the tint on if it was off — dragging
    /// the slider is how most people will switch it on in the first place.
    SetKelvin(u16),
    /// Whether the compositor implements the protocol, as the service's own
    /// connection found out.
    Supported(bool),
}

pub type Channel = crate::services::bus::Channel<NightLightState, NightLightCommand>;

pub async fn run(backend: Backend<NightLightState, NightLightCommand>) {
    let Backend {
        publish,
        mut commands,
    } = backend;

    let mut gamma = match GammaConnection::connect() {
        Ok(gamma) => Some(gamma),
        Err(error) => {
            log::info!("night light unavailable: {error}");
            None
        }
    };
    apply(&publish, NightLightCommand::Supported(gamma.is_some()));
    let mut ramp_sent = Instant::now() - RAMP_INTERVAL;

    loop {
        let command = match gamma.as_mut() {
            Some(link) => tokio::select! {
                command = commands.recv() => command,
                events = link.dispatch() => {
                    if let Err(error) = events {
                        log::warn!("night light lost its compositor connection: {error}");
                        gamma = None;
                        apply(&publish, NightLightCommand::Supported(false));
                    }
                    continue;
                }
            },
            None => commands.recv().await,
        };
        let Some(command) = command else {
            return;
        };
        apply(&publish, command);
        let Some(link) = gamma.as_mut() else {
            continue;
        };
        time::sleep_until(ramp_sent + RAMP_INTERVAL).await;
        apply_queued(&publish, &mut commands);
        link.hold(publish.read().target());
        ramp_sent = Instant::now();
    }
}

/// Folds in whatever arrived while the last ramp was cooling down, so only
/// the newest intent reaches the compositor.
fn apply_queued(publish: &Publisher<NightLightState>, commands: &mut Commands<NightLightCommand>) {
    while let Ok(command) = commands.try_recv() {
        apply(publish, command);
    }
}

fn apply(publish: &Publisher<NightLightState>, command: NightLightCommand) {
    publish.edit(|state| match command {
        NightLightCommand::Supported(true) => {
            let changed = state.availability != Availability::Ready;
            state.availability = Availability::Ready;
            changed
        }
        NightLightCommand::Supported(false) => {
            let reason = Availability::Unavailable(Arc::from(
                "the compositor does not support gamma control",
            ));
            let changed = state.availability != reason;
            state.availability = reason;
            state.active = false;
            changed
        }
        NightLightCommand::Toggle => {
            state.active = !state.active;
            true
        }
        NightLightCommand::SetActive(active) => {
            let changed = state.active != active;
            state.active = active;
            changed
        }
        NightLightCommand::SetKelvin(kelvin) => {
            let kelvin = kelvin.clamp(WARMEST_KELVIN, NEUTRAL_KELVIN);
            let changed = state.kelvin != kelvin || !state.active;
            state.kelvin = kelvin;
            state.active = true;
            changed
        }
    });
}

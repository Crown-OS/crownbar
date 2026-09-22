//! Warming the screen after dark.
//!
//! Like [`super::caffeine`], this service has nothing to read: no daemon owns
//! the answer, the bar does. What it owns is the *intent* — off, or on at a
//! colour temperature — and turning that into a gamma ramp happens on the
//! event loop, in [`reconcile`], because the Wayland objects belong to the
//! bar's own connection.
//!
//! The mechanism is `zwlr_gamma_control_manager_v1`, by way of
//! [`crownshell::App::set_color_temperature`]. It is a scanout lookup table,
//! so the tint costs nothing per frame, covers every client on the screen and
//! cannot outlive the process that asked for it.

use std::sync::Arc;

use crownshell::{App, NEUTRAL_KELVIN, WARMEST_KELVIN};

use crate::services::{bus::Backend, status::Availability};

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

    /// What the event loop should hold the outputs at.
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
    /// The event loop reporting whether the compositor implements the
    /// protocol. Nothing else can know.
    Supported(bool),
}

pub type Channel = crate::services::bus::Channel<NightLightState, NightLightCommand>;

pub async fn run(backend: Backend<NightLightState, NightLightCommand>) {
    let Backend {
        publish,
        mut commands,
    } = backend;

    while let Some(command) = commands.recv().await {
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
}

/// Make the screen agree with the intent.
///
/// Runs on the event loop, where `App` lives. Both halves are idempotent, so
/// this is safe to call on every wake-up however little changed.
pub fn reconcile(channel: &Channel, app: &mut App) {
    let state = channel.read();
    if matches!(state.availability, Availability::Starting) {
        channel.send(NightLightCommand::Supported(app.supports_gamma_control()));
    }
    let target = state.target();
    if target != app.color_temperature() {
        app.set_color_temperature(target);
    }
}

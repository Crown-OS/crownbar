//! Warming the screen after dark.
//!
//! The compositor warms the screen from `display.ron`, so the night light is
//! two keys of that file: `night_light` and `night_light_warmth`. The service
//! writes them when the user changes them on the bar, and follows them when
//! something else, such as the settings app, changes them.

use std::{sync::Arc, time::Duration};

use crownos_config::{PatchError, Subscription, schema::Display};
use tokio::{
    sync::mpsc,
    task,
    time::{self, Instant},
};

use crate::{
    bus::{Backend, Commands, Publisher},
    status::Availability,
};

/// Least time between two writes of `display.ron`. Each one makes the
/// compositor program the gamma ramp of every output, so a slider drag is
/// thinned to this rate and always ends on the value it was released at.
const WRITE_INTERVAL: Duration = Duration::from_millis(50);

/// The colour temperature at each end of the warmth slider, as the compositor
/// shows it.
pub const NEUTRAL_KELVIN: u16 = 6500;
pub const WARMEST_KELVIN: u16 = 2500;

/// `night_light_warmth` is a percentage.
const WARMTH_PERCENT: f64 = 100.0;

#[derive(Clone, Debug, PartialEq)]
pub struct NightLightState {
    pub availability: Availability,
    pub active: bool,
    /// Slider position ∈ [0, 1]: 0 is neutral daylight, 1 the warmest. The
    /// tint is held at it while active, and resumes at it when turned back on.
    pub warmth: f32,
}

impl Default for NightLightState {
    fn default() -> Self {
        Self::from(&Display::default())
    }
}

impl From<&Display> for NightLightState {
    fn from(display: &Display) -> Self {
        Self {
            availability: Availability::default(),
            active: display.night_light,
            warmth: (display.night_light_warmth / WARMTH_PERCENT).clamp(0.0, 1.0) as f32,
        }
    }
}

impl NightLightState {
    /// The colour temperature the compositor shows at this warmth.
    pub fn kelvin(&self) -> u16 {
        let span = f32::from(NEUTRAL_KELVIN - WARMEST_KELVIN);
        NEUTRAL_KELVIN - (self.warmth.clamp(0.0, 1.0) * span).round() as u16
    }

    /// `display` with this state's night light in it.
    fn written_into(&self, display: &Display) -> Display {
        Display {
            night_light: self.active,
            night_light_warmth: (f64::from(self.warmth) * WARMTH_PERCENT).round(),
            ..display.clone()
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum NightLightCommand {
    SetActive(bool),
    /// Warm to this slider position, turning the tint on if it was off —
    /// dragging the slider is how most people will switch it on in the first
    /// place.
    SetWarmth(f32),
}

pub type Channel = crate::bus::Channel<NightLightState, NightLightCommand>;

pub async fn run(backend: Backend<NightLightState, NightLightCommand>) {
    let Backend {
        publish,
        mut commands,
    } = backend;

    let (edited_sender, mut edited) = mpsc::unbounded_channel();
    let _display_subscription: Subscription =
        crownos_config::subscribe_typed(Display::SECTION, move |display: Display| {
            let _ = edited_sender.send(display);
        });
    adopt(&publish, &read_display());
    let mut written = Instant::now() - WRITE_INTERVAL;

    loop {
        tokio::select! {
            command = commands.recv() => {
                let Some(command) = command else {
                    return;
                };
                apply(&publish, command);
                time::sleep_until(written + WRITE_INTERVAL).await;
                apply_queued(&publish, &mut commands);
                write(publish.read()).await;
                written = Instant::now();
            }
            Some(display) = edited.recv() => adopt(&publish, &display),
        }
    }
}

/// The file as it is, or the defaults when it is missing or does not parse.
fn read_display() -> Display {
    crownos_config::load_strict(Display::SECTION).unwrap_or_default()
}

fn adopt(publish: &Publisher<NightLightState>, display: &Display) {
    let edited = NightLightState {
        availability: Availability::Ready,
        ..NightLightState::from(display)
    };
    publish.edit(|state| {
        let changed = *state != edited;
        *state = edited;
        changed
    });
}

/// Folds in whatever arrived while the last write was cooling down, so only
/// the newest intent reaches the file.
fn apply_queued(publish: &Publisher<NightLightState>, commands: &mut Commands<NightLightCommand>) {
    while let Ok(command) = commands.try_recv() {
        apply(publish, command);
    }
}

fn apply(publish: &Publisher<NightLightState>, command: NightLightCommand) {
    publish.edit(|state| match command {
        NightLightCommand::SetActive(active) => {
            let changed = state.active != active;
            state.active = active;
            changed
        }
        NightLightCommand::SetWarmth(warmth) => {
            let warmth = warmth.clamp(0.0, 1.0);
            let changed = state.warmth != warmth || !state.active;
            state.warmth = warmth;
            state.active = true;
            changed
        }
    });
}

async fn write(state: Arc<NightLightState>) {
    let written = task::spawn_blocking(move || save_into_display(&state)).await;
    match written {
        Ok(Ok(())) => {}
        Ok(Err(error)) => log::warn!("could not write the night light to display.ron: {error}"),
        Err(error) => log::warn!("the night light write stopped: {error}"),
    }
}

/// Writes the two keys and leaves the rest of the file as it is. A file that
/// does not parse is not replaced, since the user may be halfway through an
/// edit of it.
fn save_into_display(state: &NightLightState) -> Result<(), PatchError> {
    let display: Display = crownos_config::load_strict(Display::SECTION)?;
    let edited = state.written_into(&display);
    if edited != display {
        crownos_config::save(Display::SECTION, &edited)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warmth_is_read_and_written_as_the_file_holds_it() {
        let display = Display {
            night_light: true,
            night_light_warmth: 40.0,
            ..Display::default()
        };
        let state = NightLightState::from(&display);
        assert!(state.active);
        assert!((state.warmth - 0.4).abs() < 1e-6);
        assert_eq!(state.written_into(&Display::default()), display);
    }

    #[test]
    fn the_slider_ends_match_the_compositor() {
        let state = |warmth| NightLightState {
            warmth,
            ..NightLightState::default()
        };
        assert_eq!(state(0.0).kelvin(), NEUTRAL_KELVIN);
        assert_eq!(state(1.0).kelvin(), WARMEST_KELVIN);
    }
}

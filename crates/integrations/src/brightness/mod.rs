//! Screen brightness, for every output the compositor can dim.
//!
//! The compositor owns the brightness of each output and says it on
//! `crownos_output_brightness_v1`, on a connection of the service's own. It
//! drives the backlight, DDC/CI or a dim of its own; the bar sees one
//! perceptual level per output and never touches the hardware.

mod display;
mod level;
mod protocol;

use std::sync::Arc;

pub use display::{Control, Display, DisplayId};

use crate::{
    bus::{Backend, Publisher},
    status::Availability,
};
use protocol::BrightnessConnection;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BrightnessState {
    pub availability: Availability,
    /// In the order the compositor announced the outputs.
    pub displays: Vec<Display>,
}

impl BrightnessState {
    /// The display of the bar on `output`, or the first display for a bar on
    /// an output with no name.
    pub fn for_output(&self, output: Option<&str>) -> Option<&Display> {
        match output {
            Some(name) => self
                .displays
                .iter()
                .find(|display| display.id.as_str() == name),
            None => self.displays.first(),
        }
    }

    /// More than one display means the panel grows a row per display.
    pub fn is_multi(&self) -> bool {
        self.displays.len() > 1
    }
}

#[derive(Clone, Debug)]
pub enum BrightnessCommand {
    /// Perceptual level ∈ [0, 1].
    SetLevel { id: DisplayId, level: f32 },
    /// Every display at once, from the panel's slider for all displays.
    SetAll(f32),
    /// Read every display from the hardware again, to catch a change made
    /// with a monitor's own buttons.
    Refresh,
}

pub type Channel = crate::bus::Channel<BrightnessState, BrightnessCommand>;

pub async fn run(backend: Backend<BrightnessState, BrightnessCommand>) {
    let Backend {
        publish,
        mut commands,
    } = backend;

    let mut link = match BrightnessConnection::connect() {
        Ok(link) => link,
        Err(error) => {
            log::info!("brightness unavailable: {error}");
            return unavailable(&publish, "the compositor does not control brightness");
        }
    };

    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(command) => link.command(command),
                None => return,
            },
            heard = link.dispatch() => {
                if let Err(error) = heard {
                    log::warn!("brightness lost its compositor connection: {error}");
                    return unavailable(&publish, "lost the compositor connection");
                }
            }
        }
        if let Some(displays) = link.take_displays() {
            publish.edit(|state| {
                let changed =
                    state.availability != Availability::Ready || state.displays != displays;
                state.availability = Availability::Ready;
                state.displays = displays;
                changed
            });
        }
    }
}

fn unavailable(publish: &Publisher<BrightnessState>, reason: &'static str) {
    publish.edit(|state| {
        state.availability = Availability::Unavailable(Arc::from(reason));
        true
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display(name: &str, level: f32) -> Display {
        Display {
            id: DisplayId::new(name),
            label: name.to_owned(),
            control: Control::Software,
            level,
        }
    }

    #[test]
    fn each_bar_finds_its_own_display() {
        let state = BrightnessState {
            availability: Availability::Ready,
            displays: vec![display("eDP-1", 0.4), display("HDMI-A-1", 0.9)],
        };
        assert_eq!(
            state
                .for_output(Some("HDMI-A-1"))
                .map(|display| display.level),
            Some(0.9)
        );
        assert_eq!(
            state.for_output(None).map(|display| display.level),
            Some(0.4)
        );
        assert!(state.for_output(Some("DP-3")).is_none());
    }
}

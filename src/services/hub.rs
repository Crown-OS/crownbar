//! Every service the bar owns, and the runtime they live on.
//!
//! `Services` is handed to the calloop source callback, which the event loop
//! owns for its whole life — so the runtime, the PipeWire loop and every live
//! subscription are dropped exactly when the loop ends, in that order.

use std::cell::Cell;

use tokio::runtime::Runtime;

use crate::services::{audio, battery, bus, power, runtime, Wake};

pub struct Services {
    pub audio: audio::Channel,
    pub battery: battery::Channel,
    pub power: power::Channel,
    /// Bumped whenever any service publishes. The surfaces compare it in
    /// `needs_redraw`, the way they already compare `theme::epoch`.
    epoch: Cell<u64>,
    /// Dropping this stops every backend.
    _runtime: Runtime,
}

impl Services {
    pub fn start(wake: Wake) -> anyhow::Result<Self> {
        let rt = runtime::build()?;

        let (audio_backend, audio) = bus::connect(audio::AudioState::default(), &wake);
        rt.spawn(audio::run(audio_backend));

        let (battery_backend, battery) = bus::connect(battery::BatteryState::default(), &wake);
        rt.spawn(battery::run(battery_backend));

        let (power_backend, power) = bus::connect(power::PowerState::default(), &wake);
        rt.spawn(power::run(power_backend));

        Ok(Self {
            audio,
            battery,
            power,
            epoch: Cell::new(0),
            _runtime: rt,
        })
    }

    /// A service published something. The snapshot is already in place; this
    /// is only what makes the surfaces notice.
    pub fn woke(&self) {
        self.epoch.set(self.epoch.get().wrapping_add(1));
    }

    pub fn epoch(&self) -> u64 {
        self.epoch.get()
    }
}

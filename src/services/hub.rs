//! Every service the bar owns, and the runtime they live on.
//!
//! `Services` is handed to the calloop source callback, which the event loop
//! owns for its whole life — so the runtime, the PipeWire loop and every live
//! subscription are dropped exactly when the loop ends, in that order.

use std::cell::Cell;

use tokio::runtime::Runtime;

use crate::services::{
    audio, battery, bluetooth, brightness, bus, caffeine, network, power, runtime, Wake,
};

pub struct Services {
    pub audio: audio::Channel,
    pub battery: battery::Channel,
    pub brightness: brightness::Channel,
    pub caffeine: caffeine::Channel,
    pub bluetooth: bluetooth::Channel,
    pub network: network::Channel,
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

        let (brightness_backend, brightness) =
            bus::connect(brightness::BrightnessState::default(), &wake);
        rt.spawn(brightness::run(brightness_backend));

        let (caffeine_backend, caffeine) = bus::connect(caffeine::CaffeineState::default(), &wake);
        rt.spawn(caffeine::run(caffeine_backend));

        let (bluetooth_backend, bluetooth) =
            bus::connect(bluetooth::BluetoothState::default(), &wake);
        rt.spawn(bluetooth::run(bluetooth_backend));

        let (network_backend, network) = bus::connect(network::NetworkState::default(), &wake);
        rt.spawn(network::run(network_backend));

        let (power_backend, power) = bus::connect(power::PowerState::default(), &wake);
        rt.spawn(power::run(power_backend));

        Ok(Self {
            audio,
            battery,
            brightness,
            caffeine,
            bluetooth,
            network,
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

    /// Push whatever a service cannot do from its own thread out to the
    /// compositor. Runs on the event loop, where `App` is reachable.
    pub fn reconcile(&self, app: &mut crownshell::App) {
        caffeine::reconcile(&self.caffeine, app);
    }
}

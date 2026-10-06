//! Every service the bar owns, and the runtime they live on.
//!
//! `Services` lives in the bar's root component for the life of the UI — so the
//! runtime, the PipeWire loop and every live subscription are dropped exactly
//! when the UI goes away, in that order.

use tokio::runtime::Runtime;

use crate::services::{
    Wake, audio, battery, bluetooth, brightness, bus, caffeine, network, nightlight, notifications,
    power, runtime, stats, weather,
};

pub struct Services {
    pub audio: audio::Channel,
    pub battery: battery::Channel,
    pub brightness: brightness::Channel,
    pub caffeine: caffeine::Channel,
    pub bluetooth: bluetooth::Channel,
    pub network: network::Channel,
    pub nightlight: nightlight::Channel,
    pub notifications: notifications::Channel,
    pub power: power::Channel,
    pub stats: stats::Channel,
    pub weather: weather::Channel,
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

        let (nightlight_backend, nightlight) =
            bus::connect(nightlight::NightLightState::default(), &wake);
        rt.spawn(nightlight::run(nightlight_backend));

        let (notifications_backend, notifications) =
            bus::connect(notifications::NotificationsState::default(), &wake);
        rt.spawn(notifications::run(notifications_backend));

        let (power_backend, power) = bus::connect(power::PowerState::default(), &wake);
        rt.spawn(power::run(power_backend));

        let (stats_backend, stats) = bus::connect(stats::StatsState::default(), &wake);
        rt.spawn(stats::run(stats_backend));

        let (weather_backend, weather) = bus::connect(weather::WeatherState::default(), &wake);
        rt.spawn(weather::run(weather_backend));

        Ok(Self {
            audio,
            battery,
            brightness,
            caffeine,
            bluetooth,
            network,
            nightlight,
            notifications,
            power,
            stats,
            weather,
            _runtime: rt,
        })
    }
}

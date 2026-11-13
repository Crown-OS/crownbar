//! Every service the bar owns, and the runtime they live on.
//!
//! `Services` lives in the bar's root component for the life of the UI — so the
//! runtime, the PipeWire loop and every live subscription are dropped exactly
//! when the UI goes away, in that order.

use tokio::runtime::Runtime;

use crate::{
    Wake, assistant, audio, battery, bluetooth, brightness, bus, caffeine, crownlink, live,
    network, nightlight, notifications, plugins, power, runtime, screencast, stats, weather,
    workspace_mode,
};

pub struct Services {
    pub assistant: assistant::Channel,
    pub audio: audio::Channel,
    pub battery: battery::Channel,
    pub brightness: brightness::Channel,
    pub caffeine: caffeine::Channel,
    pub crownlink: crownlink::Channel,
    pub bluetooth: bluetooth::Channel,
    pub live: live::Channel,
    pub network: network::Channel,
    pub nightlight: nightlight::Channel,
    pub notifications: notifications::Channel,
    pub plugins: plugins::Channel,
    pub power: power::Channel,
    pub screencast: screencast::Channel,
    pub stats: stats::Channel,
    pub weather: weather::Channel,
    pub workspace_mode: workspace_mode::Channel,
    /// Dropping this stops every backend.
    _runtime: Runtime,
}

/// What each service holds when the bar is drawn from fixed readings.
#[derive(Default)]
pub struct Snapshots {
    pub assistant: assistant::AssistantSnapshot,
    pub audio: audio::AudioState,
    pub battery: battery::BatteryState,
    pub brightness: brightness::BrightnessState,
    pub caffeine: caffeine::CaffeineState,
    pub crownlink: crownlink::PhoneState,
    pub bluetooth: bluetooth::BluetoothState,
    pub live: live::LiveState,
    pub network: network::NetworkState,
    pub nightlight: nightlight::NightLightState,
    pub notifications: notifications::NotificationsState,
    pub plugins: plugins::PluginsState,
    pub power: power::PowerState,
    pub screencast: screencast::ScreencastState,
    pub stats: stats::StatsState,
    pub weather: weather::WeatherState,
    pub workspace_mode: workspace_mode::WorkspaceModeState,
}

impl Services {
    pub fn start(wake: Wake) -> anyhow::Result<Self> {
        let rt = runtime::build()?;

        let (assistant_backend, assistant) =
            bus::connect(assistant::AssistantSnapshot::default(), &wake);
        rt.spawn(assistant::run(assistant_backend));
        let (audio_backend, audio) = bus::connect(audio::AudioState::default(), &wake);
        audio::run(audio_backend, rt.handle().clone());

        let (battery_backend, battery) = bus::connect(battery::BatteryState::default(), &wake);
        rt.spawn(battery::run(battery_backend));

        let (brightness_backend, brightness) =
            bus::connect(brightness::BrightnessState::default(), &wake);
        rt.spawn(brightness::run(brightness_backend));

        let (caffeine_backend, caffeine) = bus::connect(caffeine::CaffeineState::default(), &wake);
        rt.spawn(caffeine::run(caffeine_backend));

        let (crownlink_backend, crownlink) = bus::connect(crownlink::PhoneState::default(), &wake);
        rt.spawn(crownlink::run(crownlink_backend));

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

        let (plugins_backend, plugins) = bus::connect(plugins::PluginsState::default(), &wake);
        rt.spawn(plugins::run(plugins_backend));

        let (power_backend, power) = bus::connect(power::PowerState::default(), &wake);
        rt.spawn(power::run(power_backend));

        let (screencast_backend, screencast) =
            bus::connect(screencast::ScreencastState::default(), &wake);
        rt.spawn(screencast::run(screencast_backend));

        let (stats_backend, stats) = bus::connect(stats::StatsState::default(), &wake);
        rt.spawn(stats::run(stats_backend));

        let (weather_backend, weather) = bus::connect(weather::WeatherState::default(), &wake);
        rt.spawn(weather::run(weather_backend));

        let (workspace_mode_backend, workspace_mode) =
            bus::connect(workspace_mode::WorkspaceModeState::default(), &wake);
        rt.spawn(workspace_mode::run(workspace_mode_backend));

        let (live_backend, live) = bus::connect(live::LiveState::default(), &wake);
        let bridges = live::Bridges {
            audio: audio.tap(),
            screencast: screencast.tap(),
            caffeine: caffeine.tap(),
            crownlink: crownlink.tap(),
        };
        rt.spawn(live::run(live_backend, bridges));

        Ok(Self {
            assistant,
            audio,
            battery,
            brightness,
            caffeine,
            crownlink,
            bluetooth,
            live,
            network,
            nightlight,
            notifications,
            plugins,
            power,
            screencast,
            stats,
            weather,
            workspace_mode,
            _runtime: rt,
        })
    }

    /// Every service holding `snapshots` for good, with no backend behind it:
    /// the bar as sample readings draw it, never this machine.
    pub fn still(snapshots: Snapshots) -> anyhow::Result<Self> {
        let wake = Wake::default();
        Ok(Self {
            assistant: bus::connect(snapshots.assistant, &wake).1,
            audio: bus::connect(snapshots.audio, &wake).1,
            battery: bus::connect(snapshots.battery, &wake).1,
            brightness: bus::connect(snapshots.brightness, &wake).1,
            caffeine: bus::connect(snapshots.caffeine, &wake).1,
            crownlink: bus::connect(snapshots.crownlink, &wake).1,
            bluetooth: bus::connect(snapshots.bluetooth, &wake).1,
            live: bus::connect(snapshots.live, &wake).1,
            network: bus::connect(snapshots.network, &wake).1,
            nightlight: bus::connect(snapshots.nightlight, &wake).1,
            notifications: bus::connect(snapshots.notifications, &wake).1,
            plugins: bus::connect(snapshots.plugins, &wake).1,
            power: bus::connect(snapshots.power, &wake).1,
            screencast: bus::connect(snapshots.screencast, &wake).1,
            stats: bus::connect(snapshots.stats, &wake).1,
            weather: bus::connect(snapshots.weather, &wake).1,
            workspace_mode: bus::connect(snapshots.workspace_mode, &wake).1,
            _runtime: runtime::build()?,
        })
    }
}

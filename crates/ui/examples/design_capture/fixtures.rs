//! Plausible readings for every service, so captures never show this machine.

use std::sync::Arc;

use chrono::{Days, Local};

use crownbar_integrations::{
    Availability, Snapshots,
    audio::{AudioState, Device, DeviceKind as AudioKind, Stream, Volume},
    battery::{BatteryState, Charge, ChargeStatus},
    bluetooth::{Address, BluetoothState, BtDevice, DeviceKind, RadioState, SignalLevel},
    brightness::{BrightnessState, Display, DisplayId, Transport},
    caffeine::CaffeineState,
    crownlink::{Battery, DeviceClass, DeviceId, Feature, FeatureSet, PhoneDevice, PhoneState},
    network::{Joined, NetworkState, Radio, Uplink, WifiNetwork},
    nightlight::NightLightState,
    notifications::NotificationsState,
    power::{PowerState, Profile, Profiles},
    weather::{Condition, Current, DAYS, Outlook, WeatherState},
    workspace_mode::{WorkspaceMode, WorkspaceModeState},
};

pub const PIXEL: DeviceId = DeviceId([1; 32]);

/// Every service up, with a typical afternoon's readings.
pub fn everyday() -> Snapshots {
    Snapshots {
        audio: audio(),
        battery: battery(0.76, ChargeStatus::Discharging, Some(272)),
        brightness: brightness(),
        caffeine: CaffeineState {
            availability: Availability::Ready,
            ..CaffeineState::default()
        },
        crownlink: phones(),
        bluetooth: bluetooth(),
        network: network(),
        nightlight: NightLightState {
            availability: Availability::Ready,
            active: false,
            kelvin: 4500,
        },
        notifications: NotificationsState {
            availability: Availability::Ready,
            ..NotificationsState::default()
        },
        power: PowerState {
            availability: Availability::Ready,
            profiles: Profiles {
                supported: vec![Profile::Saver, Profile::Balanced, Profile::Performance],
                active: Some(Profile::Balanced),
                degraded: None,
            },
            failure: None,
        },
        weather: weather(),
        workspace_mode: WorkspaceModeState {
            availability: Availability::Ready,
            mode: WorkspaceMode::Floating,
        },
        ..Snapshots::default()
    }
}

pub fn battery(level: f32, status: ChargeStatus, minutes: Option<u32>) -> BatteryState {
    BatteryState {
        availability: Availability::Ready,
        charge: Some(Charge {
            level,
            status,
            minutes,
            health: Some(0.94),
        }),
    }
}

fn output(id: u32, name: &str, description: &str, kind: AudioKind, default: bool) -> Device {
    Device {
        id,
        name: name.into(),
        description: description.into(),
        kind,
        volume: Volume {
            level: 0.62,
            muted: false,
        },
        default,
        card: None,
        card_port: None,
        priority: 0,
    }
}

fn stream(id: u32, app: &str, level: f32) -> Stream {
    Stream {
        id,
        app: app.into(),
        title: None,
        icon_name: None,
        volume: Volume {
            level,
            muted: false,
        },
        active: true,
    }
}

fn audio() -> AudioState {
    let mut audio = AudioState::default();
    audio.availability = Availability::Ready;
    audio.outputs = vec![
        output(
            41,
            "speakers",
            "Built-in Speakers",
            AudioKind::Speakers,
            true,
        ),
        output(42, "headphones", "WH-1000XM5", AudioKind::Bluetooth, false),
        output(43, "hdmi", "LG HDR 4K", AudioKind::Display, false),
    ];
    audio.inputs = vec![
        output(
            51,
            "mic",
            "Built-in Microphone",
            AudioKind::Microphone,
            true,
        ),
        output(52, "webcam", "Logitech C920", AudioKind::Webcam, false),
    ];
    audio.playback = vec![stream(61, "Spotify", 0.8), stream(62, "Firefox", 0.55)];
    audio.recording = vec![stream(71, "Discord", 1.0)];
    audio
}

pub fn muted(mut audio: AudioState) -> AudioState {
    if let Some(speakers) = audio.outputs.first_mut() {
        speakers.volume.muted = true;
    }
    audio
}

fn brightness() -> BrightnessState {
    BrightnessState {
        availability: Availability::Ready,
        displays: vec![
            Display {
                id: DisplayId::backlight("intel_backlight"),
                label: "Built-in Display".into(),
                transport: Transport::Backlight,
                level: 0.68,
                max: 19_200,
            },
            Display {
                id: DisplayId::monitor("GSM", "LG HDR 4K", "205NTAB1C123"),
                label: "LG HDR 4K".into(),
                transport: Transport::Ddc,
                level: 0.45,
                max: 100,
            },
        ],
        failure: None,
    }
}

pub fn single_display(mut brightness: BrightnessState) -> BrightnessState {
    brightness.displays.truncate(1);
    brightness
}

fn phones() -> PhoneState {
    PhoneState {
        availability: Availability::Ready,
        devices: vec![PhoneDevice {
            id: PIXEL,
            name: Arc::from("Pixel 8"),
            class: DeviceClass::Phone,
            battery: Some(Battery {
                percent: 64,
                charging: false,
            }),
            hotspot: Some(false),
            allowed: Feature::ALL.into_iter().collect(),
            active: FeatureSet::EMPTY,
            call: None,
            activities: Vec::new(),
        }],
        ringing_here: None,
    }
}

fn device(last: u8, name: &str, kind: DeviceKind) -> BtDevice {
    BtDevice {
        address: Address::new([0x7c, 0x96, 0xd2, 0x41, 0x0e, last]),
        name: name.into(),
        kind,
        paired: true,
        connected: false,
        busy: false,
        battery: None,
        signal: None,
    }
}

fn connected(mut device: BtDevice, battery: u8) -> BtDevice {
    device.connected = true;
    device.battery = Some(battery);
    device
}

fn nearby(mut device: BtDevice, rssi: i16) -> BtDevice {
    device.paired = false;
    device.signal = Some(SignalLevel::from_rssi(rssi));
    device
}

fn bluetooth() -> BluetoothState {
    BluetoothState {
        availability: Availability::Ready,
        radio: RadioState::On,
        adapter: Some("hci0".into()),
        paired: vec![
            connected(device(1, "WH-1000XM5", DeviceKind::Headphones), 80),
            connected(device(2, "MX Master 3S", DeviceKind::Mouse), 55),
            device(3, "Keychron K2", DeviceKind::Keyboard),
        ],
        discovered: vec![
            nearby(device(4, "JBL Flip 6", DeviceKind::Speaker), -58),
            nearby(
                device(5, "Xbox Wireless Controller", DeviceKind::Gamepad),
                -74,
            ),
        ],
        discovering: false,
        failure: None,
    }
}

fn wifi(ssid: &str, strength: f32, secured: bool, known: bool) -> WifiNetwork {
    WifiNetwork {
        ssid: ssid.into(),
        strength,
        secured,
        enterprise: false,
        weak: strength < 0.3,
        known,
    }
}

fn network() -> NetworkState {
    let mut network = NetworkState::default();
    network.availability = Availability::Ready;
    network.radio = Radio::On;
    network.uplink = Uplink::Wifi;
    network.connected = Some(Joined {
        ssid: "Crown Studio".into(),
        strength: 0.82,
        secured: true,
        weak: false,
        ip4: Some("192.168.1.24".into()),
    });
    network.known = vec![wifi("Crown Studio 2.4G", 0.64, true, true)];
    network.others = vec![
        wifi("Café Lumière", 0.71, false, false),
        wifi("TP-Link_4F2A", 0.48, true, false),
        wifi("Vodafone-B7C1", 0.36, true, false),
        wifi("NETGEAR-Guest", 0.18, true, false),
    ];
    network
}

fn weather() -> WeatherState {
    let today = Local::now().date_naive();
    let days = [
        (Condition::PartlyCloudy, 22.0, 15.0),
        (Condition::Showers, 19.0, 13.0),
        (Condition::Rain, 17.0, 12.0),
        (Condition::Cloudy, 18.0, 11.0),
        (Condition::Clear, 23.0, 14.0),
    ];
    let outlook = days
        .into_iter()
        .take(DAYS)
        .enumerate()
        .map(|(offset, (condition, high, low))| Outlook {
            day: match offset {
                0 => "Today".into(),
                _ => today
                    .checked_add_days(Days::new(offset as u64))
                    .map_or_else(String::new, |day| day.format("%A").to_string()),
            },
            condition,
            high,
            low,
        })
        .collect();
    WeatherState {
        availability: Availability::Ready,
        place: Some(Arc::from("Lisbon")),
        current: Some(Current {
            condition: Condition::PartlyCloudy,
            night: false,
            celsius: 21.4,
            feels_like: 20.0,
            humidity: 58,
            wind_kph: 14.0,
        }),
        outlook,
        refreshing: false,
    }
}

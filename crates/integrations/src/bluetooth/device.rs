//! A Bluetooth device as the panel needs it.

use bluer::{Address, DeviceProperty};

/// What a device is, for picking its glyph.
///
/// The device *name* is deliberately never consulted: "MX Master" says nothing
/// a class code does not say better.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DeviceKind {
    Headphones,
    Headset,
    Speaker,
    Keyboard,
    Mouse,
    Phone,
    Computer,
    Display,
    #[default]
    Other,
}

/// Major device class, bits 8..13 of the BR/EDR class of device.
const MAJOR_SHIFT: u32 = 8;
const MAJOR_MASK: u32 = 0x1f;
/// Minor class, bits 2..8.
const MINOR_SHIFT: u32 = 2;
const MINOR_MASK: u32 = 0x3f;

impl DeviceKind {
    /// BlueZ's `Icon` first — it has already done this work — then the class
    /// of device for anything that has none, then the LE appearance.
    pub fn classify(icon: Option<&str>, class: Option<u32>, appearance: Option<u16>) -> Self {
        if let Some(kind) = icon.and_then(from_icon) {
            return kind;
        }
        if let Some(kind) = class.and_then(from_class) {
            return kind;
        }
        appearance.and_then(from_appearance).unwrap_or(Self::Other)
    }
}

fn from_icon(icon: &str) -> Option<DeviceKind> {
    Some(match icon {
        "audio-headphones" => DeviceKind::Headphones,
        "audio-headset" => DeviceKind::Headset,
        "audio-card" | "audio-speakers" => DeviceKind::Speaker,
        "input-keyboard" => DeviceKind::Keyboard,
        "input-mouse" | "input-tablet" | "input-gaming" => DeviceKind::Mouse,
        "phone" => DeviceKind::Phone,
        "computer" => DeviceKind::Computer,
        "video-display" | "tv" => DeviceKind::Display,
        _ => return None,
    })
}

fn from_class(class: u32) -> Option<DeviceKind> {
    let major = (class >> MAJOR_SHIFT) & MAJOR_MASK;
    let minor = (class >> MINOR_SHIFT) & MINOR_MASK;
    Some(match (major, minor) {
        (0x01, _) => DeviceKind::Computer,
        (0x02, _) => DeviceKind::Phone,
        // Audio/video: the minor class distinguishes a headset from a speaker.
        (0x04, 0x01 | 0x02) => DeviceKind::Headset,
        (0x04, 0x06) => DeviceKind::Headphones,
        (0x04, 0x05 | 0x07 | 0x08) => DeviceKind::Speaker,
        (0x04, 0x0a..=0x0c) => DeviceKind::Display,
        (0x05, 0x10..=0x1f) => DeviceKind::Keyboard,
        (0x05, 0x20..=0x2f) => DeviceKind::Mouse,
        _ => return None,
    })
}

/// GAP appearance values, whose high 10 bits are the category.
fn from_appearance(appearance: u16) -> Option<DeviceKind> {
    Some(match appearance >> 6 {
        0x01 => DeviceKind::Phone,
        0x02 => DeviceKind::Computer,
        0x0f => DeviceKind::Keyboard,
        _ => return None,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BtDevice {
    pub address: Address,
    /// The user's alias, the advertised name, or the address — a row always
    /// has something to say.
    pub name: String,
    pub kind: DeviceKind,
    pub paired: bool,
    pub connected: bool,
    /// A connect or pair is in flight. Shown before BlueZ answers, because a
    /// headset takes seconds to come up.
    pub busy: bool,
    pub battery: Option<u8>,
    /// Only meaningful while discovering; nearby devices sort on it.
    pub rssi: Option<i16>,
}

impl BtDevice {
    /// Fold one `all_properties` read — a single D-Bus round trip — into a
    /// device.
    pub fn from_properties(address: Address, properties: &[DeviceProperty]) -> Self {
        let (mut alias, mut name, mut icon) = (None, None, None);
        let (mut class, mut appearance) = (None, None);
        let mut device = Self {
            address,
            name: String::new(),
            kind: DeviceKind::Other,
            paired: false,
            connected: false,
            busy: false,
            battery: None,
            rssi: None,
        };
        for property in properties {
            match property {
                DeviceProperty::Alias(value) => alias = Some(value.clone()),
                DeviceProperty::Name(value) => name = Some(value.clone()),
                DeviceProperty::Icon(value) => icon = Some(value.clone()),
                DeviceProperty::Class(value) => class = Some(*value),
                DeviceProperty::Appearance(value) => appearance = Some(*value),
                DeviceProperty::Paired(value) => device.paired = *value,
                DeviceProperty::Connected(value) => device.connected = *value,
                DeviceProperty::BatteryPercentage(value) => device.battery = Some(*value),
                DeviceProperty::Rssi(value) => device.rssi = Some(*value),
                _ => {}
            }
        }
        device.name = alias
            .or(name)
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| address.to_string());
        device.kind = DeviceKind::classify(icon.as_deref(), class, appearance);
        device
    }
}

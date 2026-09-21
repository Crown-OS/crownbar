//! Paired Bluetooth devices, read through `bluetoothctl`.
//!
//! The bar's *icon* state still comes from rfkill ([`crate::util::rfkill`]),
//! which needs nothing running. The panel's device list needs BlueZ, so this
//! module talks to it — through the CLI rather than a D-Bus client, so the
//! bar keeps working when BlueZ is absent instead of failing to link.
//!
//! `bluetoothctl power on|off` goes through BlueZ and polkit, which a session
//! user is normally allowed to do. That is why the toggle uses it rather than
//! writing the rfkill soft-block, which wants root.

use crate::util::cmd;

const CTL: &str = "bluetoothctl";

/// What a device is, so the panel can pick a glyph.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DeviceKind {
    Headphones,
    Speaker,
    Input,
    Phone,
    Display,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub address: String,
    pub name: String,
    pub connected: bool,
    /// Reported by BlueZ's battery provider, where the device supports it.
    pub battery: Option<u8>,
    pub kind: DeviceKind,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    /// `None` when BlueZ could not be reached at all — the panel then shows
    /// the toggle alone rather than an empty and misleading device list.
    pub powered: Option<bool>,
    pub devices: Vec<Device>,
}

pub fn available() -> bool {
    cmd::exists(CTL)
}

/// One read of everything the panel shows. Runs several subprocesses, so it
/// belongs on a [`crate::util::worker::Job`].
pub fn snapshot() -> Snapshot {
    if !available() {
        return Snapshot::default();
    }
    let powered = cmd::output(CTL, &["show"])
        .and_then(|out| cmd::field(&out, "Powered").map(|v| v.starts_with("yes")));

    let mut devices: Vec<Device> = paired_addresses()
        .into_iter()
        .filter_map(|(address, fallback_name)| describe(&address, fallback_name))
        .collect();
    // Connected first, then alphabetical — the row people came for is on top.
    devices.sort_by(|a, b| {
        b.connected
            .cmp(&a.connected)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Snapshot { powered, devices }
}

pub fn set_powered(on: bool) -> bool {
    cmd::run(CTL, &["power", if on { "on" } else { "off" }])
}

/// Toggle one device's connection. `bluetoothctl connect` blocks until the
/// link is up or times out, so this is worker-thread work like the rest.
pub fn set_connected(address: &str, connected: bool) -> bool {
    let verb = if connected { "connect" } else { "disconnect" };
    cmd::run(CTL, &[verb, address])
}

pub fn open_settings() -> bool {
    cmd::launch_first(&[
        ("crownos-settings", &["bluetooth"]),
        ("blueman-manager", &[]),
        ("overskride", &[]),
    ])
}

/// `bluetoothctl devices Paired` → `Device AA:BB:CC:DD:EE:FF  Name`.
/// The `Paired` filter needs BlueZ ≥ 5.65; older builds print usage instead,
/// so fall back to the unfiltered list.
fn paired_addresses() -> Vec<(String, String)> {
    let out = cmd::output(CTL, &["devices", "Paired"])
        .filter(|out| out.lines().any(|l| l.trim_start().starts_with("Device ")))
        .or_else(|| cmd::output(CTL, &["devices"]));
    let Some(out) = out else {
        return Vec::new();
    };
    parse_device_list(&out)
}

fn parse_device_list(out: &str) -> Vec<(String, String)> {
    out.lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("Device ")?;
            let (address, name) = rest.split_once(' ')?;
            if !looks_like_address(address) {
                return None;
            }
            Some((address.to_string(), name.trim().to_string()))
        })
        .collect()
}

fn looks_like_address(s: &str) -> bool {
    s.len() == 17 && s.split(':').count() == 6
}

fn describe(address: &str, fallback_name: String) -> Option<Device> {
    let info = cmd::output(CTL, &["info", address])?;
    Some(parse_info(address, fallback_name, &info))
}

fn parse_info(address: &str, fallback_name: String, info: &str) -> Device {
    let name = cmd::field(info, "Alias")
        .or_else(|| cmd::field(info, "Name"))
        .map(str::to_string)
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback_name);
    let connected = cmd::field(info, "Connected")
        .map(|v| v.starts_with("yes"))
        .unwrap_or(false);
    Device {
        address: address.to_string(),
        kind: classify(cmd::field(info, "Icon").unwrap_or(""), &name),
        battery: parse_battery(info),
        connected,
        name,
    }
}

/// `Battery Percentage: 0x46 (70)` — the decimal in parentheses is the one
/// worth reading; the hex before it is the raw attribute value.
fn parse_battery(info: &str) -> Option<u8> {
    let line = info
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("Battery Percentage"))?;
    let (_, rest) = line.split_once('(')?;
    let (value, _) = rest.split_once(')')?;
    value.trim().parse::<u8>().ok()
}

/// BlueZ's `Icon` property is a freedesktop icon name (`audio-headset`,
/// `input-mouse`, …). Fall back to the device name where it is missing.
fn classify(icon: &str, name: &str) -> DeviceKind {
    let hay = format!("{icon} {name}").to_ascii_lowercase();
    if hay.contains("headset") || hay.contains("headphone") || hay.contains("airpod") {
        DeviceKind::Headphones
    } else if hay.contains("speaker") || hay.contains("audio-card") {
        DeviceKind::Speaker
    } else if hay.contains("input") || hay.contains("keyboard") || hay.contains("mouse") {
        DeviceKind::Input
    } else if hay.contains("phone") {
        DeviceKind::Phone
    } else if hay.contains("video") || hay.contains("display") || hay.contains("tv") {
        DeviceKind::Display
    } else {
        DeviceKind::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INFO: &str = "\
Device AA:BB:CC:DD:EE:FF (public)
\tName: soundcore Space One
\tAlias: soundcore Space One
\tClass: 0x00240418
\tIcon: audio-headset
\tPaired: yes
\tConnected: yes
\tBattery Percentage: 0x46 (70)
";

    #[test]
    fn device_list_keeps_only_real_entries() {
        let out = "Device AA:BB:CC:DD:EE:FF soundcore Space One\nAgent registered\n";
        assert_eq!(
            parse_device_list(out),
            vec![(
                "AA:BB:CC:DD:EE:FF".to_string(),
                "soundcore Space One".to_string()
            )]
        );
    }

    #[test]
    fn info_carries_name_battery_and_kind() {
        let device = parse_info("AA:BB:CC:DD:EE:FF", "fallback".into(), INFO);
        assert_eq!(device.name, "soundcore Space One");
        assert!(device.connected);
        assert_eq!(device.battery, Some(70));
        assert_eq!(device.kind, DeviceKind::Headphones);
    }

    #[test]
    fn a_device_without_a_battery_reports_none() {
        let device = parse_info("AA:BB:CC:DD:EE:FF", "fallback".into(), "\tConnected: no\n");
        assert_eq!(device.battery, None);
        assert!(!device.connected);
        assert_eq!(device.name, "fallback");
    }
}

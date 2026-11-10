//! Link quality, and the link in use, with no daemon in the way.
//!
//! NetworkManager's `Strength` is what the panel rows use, because it is the
//! same number the list sorts by. But the bar has to draw a correct icon
//! before NetworkManager is up and while it is restarting, and only the kernel
//! can answer then — so this stays.

use std::{
    fs,
    path::{Path, PathBuf},
};

use super::Uplink;

const NET_CLASS: &str = "/sys/class/net";
/// `/proc/net/route` prints addresses as little-endian hex; this is 0.0.0.0.
const DEFAULT_DESTINATION: &str = "00000000";
/// Most cfg80211 drivers report link quality on a 0-70 scale.
const DRIVER_SCALE: f32 = 70.0;

/// The first interface with a `wireless` subdirectory, which is the kernel's
/// convention for a cfg80211 device.
pub fn interface() -> Option<PathBuf> {
    fs::read_dir(NET_CLASS)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.join("wireless").is_dir())
}

/// Carrier state and quality ∈ [0, 1], or `None` with no wireless device.
pub fn quality() -> Option<f32> {
    let path = interface()?;
    let up = fs::read_to_string(path.join("operstate"))
        .map(|state| state.trim() == "up")
        .unwrap_or(false);
    if !up {
        return Some(0.0);
    }
    let name = path.file_name()?.to_str()?;
    Some(read_quality(name).unwrap_or(0.5))
}

/// The kind of interface the cheapest IPv4 default route leaves by. A tunnel
/// or a bridge is neither, and reads as [`Uplink::None`].
pub fn uplink() -> Uplink {
    let Ok(routes) = fs::read_to_string("/proc/net/route") else {
        return Uplink::None;
    };
    routes
        .lines()
        .skip(1)
        .filter_map(default_route)
        .min_by_key(|(metric, _)| *metric)
        .map_or(Uplink::None, |(_, interface)| kind(interface))
}

/// A route line's metric and interface, when it is a default route.
fn default_route(line: &str) -> Option<(u32, &str)> {
    let mut columns = line.split_whitespace();
    let interface = columns.next()?;
    let destination = columns.next()?;
    let metric = columns.nth(4)?.parse().ok()?;
    (destination == DEFAULT_DESTINATION).then_some((metric, interface))
}

/// Physical hardware has a `device` link; a radio also has `wireless`.
fn kind(interface: &str) -> Uplink {
    let path = Path::new(NET_CLASS).join(interface);
    if path.join("wireless").is_dir() {
        Uplink::Wifi
    } else if path.join("device").exists() {
        Uplink::Ethernet
    } else {
        Uplink::None
    }
}

/// `/proc/net/wireless`: the second column after the interface is the link
/// quality.
fn read_quality(interface: &str) -> Option<f32> {
    let contents = fs::read_to_string("/proc/net/wireless").ok()?;
    for line in contents.lines() {
        let Some((name, rest)) = line.trim_start().split_once(':') else {
            continue;
        };
        if name != interface {
            continue;
        }
        let mut columns = rest.split_whitespace();
        let _status = columns.next()?;
        let link: f32 = columns.next()?.trim_end_matches('.').parse().ok()?;
        return Some((link / DRIVER_SCALE).clamp(0.0, 1.0));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_default_routes_are_read() {
        assert_eq!(
            default_route("enp2s0\t00000000\t0101A8C0\t0003\t0\t0\t100\t00000000\t0\t0\t0"),
            Some((100, "enp2s0"))
        );
        assert_eq!(
            default_route("enp2s0\t0001A8C0\t00000000\t0001\t0\t0\t100\t00FFFFFF\t0\t0\t0"),
            None
        );
    }
}

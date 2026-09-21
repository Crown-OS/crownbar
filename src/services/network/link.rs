//! Link quality with no daemon in the way.
//!
//! NetworkManager's `Strength` is what the panel rows use, because it is the
//! same number the list sorts by. But the bar has to draw a correct icon
//! before NetworkManager is up and while it is restarting, and only the kernel
//! can answer then — so this stays.

use std::{fs, path::PathBuf};

/// Most cfg80211 drivers report link quality on a 0-70 scale.
const DRIVER_SCALE: f32 = 70.0;

/// The first interface with a `wireless` subdirectory, which is the kernel's
/// convention for a cfg80211 device.
pub fn interface() -> Option<PathBuf> {
    fs::read_dir("/sys/class/net")
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

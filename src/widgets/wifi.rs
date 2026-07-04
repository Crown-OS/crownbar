//! Wi-Fi widget. State comes from sysfs (`/sys/class/net/<iface>/wireless`
//! exists ⇒ wireless device, `operstate` ⇒ link up/down) and signal quality
//! from `/proc/net/wireless` (column 3 = link quality on most drivers).
//!
//! No NetworkManager dependency — keeps the bar self-sufficient on any
//! Linux box, including minimal cosmic-comp environments where NM may not
//! be running yet.

use std::fs;
use std::path::PathBuf;

use crate::{
    animation::Spring,
    util::poll::PollGate,
    widgets::{BarWidget, Icon, WidgetSlot},
};

const POLL_PERIOD_TICKS: u32 = 3;

pub struct WifiWidget {
    iface: PathBuf,
    strength: Spring,
    gate: PollGate,
}

impl WifiWidget {
    pub fn try_new() -> Option<Self> {
        let iface = find_wireless_iface()?;
        let mut w = Self {
            iface,
            strength: Spring::new(0.0),
            gate: PollGate::new(POLL_PERIOD_TICKS),
        };
        let target = w.read_strength();
        w.strength.position = target;
        w.strength.set_target(target);
        Some(w)
    }

    fn read_strength(&self) -> f32 {
        let up = read_trimmed(self.iface.join("operstate"))
            .map(|s| s == "up")
            .unwrap_or(false);
        if !up {
            return 0.0;
        }
        let iface_name = self
            .iface
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        read_link_quality(iface_name)
            .map(|q| q as f32 / 100.0)
            .unwrap_or(0.5)
    }
}

impl BarWidget for WifiWidget {
    fn id(&self) -> &'static str {
        "wifi"
    }

    fn slot(&self) -> WidgetSlot {
        WidgetSlot::Right
    }

    fn icon(&self) -> Icon {
        Icon::Wifi {
            strength: self.strength.position,
        }
    }

    fn update(&mut self) -> bool {
        if !self.gate.should_run() {
            return false;
        }
        let next = self.read_strength();
        let delta = (next - self.strength.target).abs();
        if delta > 0.01 {
            self.strength.set_target(next);
            true
        } else {
            false
        }
    }

    fn tick_animation(&mut self, dt: f32) -> bool {
        if self.strength.at_rest() {
            return false;
        }
        self.strength.step(dt);
        !self.strength.at_rest()
    }
}

/// Pick the first interface under /sys/class/net that has a `wireless`
/// subdirectory (the kernel convention for cfg80211-backed devices).
fn find_wireless_iface() -> Option<PathBuf> {
    let entries = fs::read_dir("/sys/class/net").ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.join("wireless").is_dir() {
            return Some(path);
        }
    }
    None
}

fn read_trimmed(path: PathBuf) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

/// Parse `/proc/net/wireless` and return the link-quality percentage for the
/// given interface. The file's "link" column is on a driver-dependent scale
/// (commonly 0-70 or 0-100); we normalize to 0-100 with a 70 cap that matches
/// what most cfg80211 drivers report.
fn read_link_quality(iface: &str) -> Option<u8> {
    let contents = fs::read_to_string("/proc/net/wireless").ok()?;
    for line in contents.lines() {
        let line = line.trim_start();
        let Some((name, rest)) = line.split_once(':') else {
            continue;
        };
        if name != iface {
            continue;
        }
        let mut cols = rest.split_whitespace();
        let _status = cols.next()?;
        let link_raw = cols.next()?;
        let link: f32 = link_raw.trim_end_matches('.').parse().ok()?;
        let pct = ((link / 70.0) * 100.0).clamp(0.0, 100.0);
        return Some(pct.round() as u8);
    }
    None
}

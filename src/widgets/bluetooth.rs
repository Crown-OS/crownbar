//! Bluetooth widget. Reads adapter state from `/sys/class/rfkill/*` so we
//! don't need to depend on BlueZ or D-Bus — this works pre-login, before any
//! desktop daemon is up.
//!
//! rfkill exposes per-radio entries with `type` (bluetooth/wlan/wwan/...) and
//! `state` (0 soft-blocked, 1 unblocked, 2 hard-blocked). We treat unblocked
//! as "on".

use std::fs;

use crate::{
    animation::Spring,
    util::poll::PollGate,
    widgets::{BarWidget, Icon, WidgetSlot},
};

const POLL_PERIOD_TICKS: u32 = 3;

pub struct BluetoothWidget {
    on: bool,
    on_anim: Spring,
    gate: PollGate,
}

impl BluetoothWidget {
    pub fn try_new() -> Option<Self> {
        if !has_bluetooth_radio() {
            return None;
        }
        let on = is_on();
        let init = if on { 1.0 } else { 0.0 };
        let mut anim = Spring::new(init);
        anim.set_target(init);
        Some(Self {
            on,
            on_anim: anim,
            gate: PollGate::new(POLL_PERIOD_TICKS),
        })
    }
}

impl BarWidget for BluetoothWidget {
    fn id(&self) -> &'static str {
        "bluetooth"
    }

    fn slot(&self) -> WidgetSlot {
        WidgetSlot::Right
    }

    fn icon(&self) -> Icon {
        Icon::Bluetooth {
            on: self.on_anim.position,
        }
    }

    fn update(&mut self) -> bool {
        if !self.gate.should_run() {
            return false;
        }
        let next = is_on();
        if next != self.on {
            self.on = next;
            self.on_anim.set_target(if next { 1.0 } else { 0.0 });
            true
        } else {
            false
        }
    }

    fn on_click(&mut self) -> bool {
        // Toggle is local-visual only; flipping the rfkill soft-block needs
        // privileges the bar isn't asking for. The next sysfs poll will
        // re-sync if the real state never moved.
        self.on = !self.on;
        self.on_anim
            .set_target(if self.on { 1.0 } else { 0.0 });
        true
    }

    fn tick_animation(&mut self, dt: f32) -> bool {
        if self.on_anim.at_rest() {
            return false;
        }
        self.on_anim.step(dt);
        !self.on_anim.at_rest()
    }
}

fn has_bluetooth_radio() -> bool {
    rfkill_entries().any(|(ty, _)| ty == "bluetooth")
}

fn is_on() -> bool {
    rfkill_entries()
        .filter(|(ty, _)| ty == "bluetooth")
        .any(|(_, state)| state == 1)
}

fn rfkill_entries() -> impl Iterator<Item = (String, u32)> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/class/rfkill") else {
        return out.into_iter();
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(ty) = read_trim(&path.join("type")) else {
            continue;
        };
        let Some(state) = read_trim(&path.join("state")).and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        out.push((ty, state));
    }
    out.into_iter()
}

fn read_trim(path: &std::path::Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

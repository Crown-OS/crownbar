//! Display brightness widget. Reads the first backlight controller under
//! `/sys/class/backlight/` (firmware backlights take priority when present
//! but we just take the first one — that's what cosmic-applet-battery does
//! in practice too).

use std::fs;
use std::path::PathBuf;

use crate::{
    animation::Spring,
    util::poll::PollGate,
    widgets::{BarWidget, Icon, WidgetSlot},
};

const POLL_PERIOD_TICKS: u32 = 2;

pub struct BrightnessWidget {
    device: PathBuf,
    max: u32,
    level: Spring,
    gate: PollGate,
}

impl BrightnessWidget {
    pub fn try_new() -> Option<Self> {
        let device = find_backlight()?;
        let max = read_int(&device.join("max_brightness"))?.max(1);
        let mut w = Self {
            device,
            max,
            level: Spring::new(0.0),
            gate: PollGate::new(POLL_PERIOD_TICKS),
        };
        let lv = w.read_level();
        w.level.position = lv;
        w.level.set_target(lv);
        Some(w)
    }

    fn read_level(&self) -> f32 {
        let Some(cur) = read_int(&self.device.join("brightness")) else {
            return 0.0;
        };
        (cur as f32 / self.max as f32).clamp(0.0, 1.0)
    }
}

impl BarWidget for BrightnessWidget {
    fn id(&self) -> &'static str {
        "brightness"
    }

    fn slot(&self) -> WidgetSlot {
        WidgetSlot::Right
    }

    fn icon(&self) -> Icon {
        Icon::Brightness {
            level: self.level.position,
        }
    }

    fn update(&mut self) -> bool {
        if !self.gate.should_run() {
            return false;
        }
        let next = self.read_level();
        if (next - self.level.target).abs() > 0.005 {
            self.level.set_target(next);
            true
        } else {
            false
        }
    }

    fn tick_animation(&mut self, dt: f32) -> bool {
        if self.level.at_rest() {
            return false;
        }
        self.level.step(dt);
        !self.level.at_rest()
    }
}

fn find_backlight() -> Option<PathBuf> {
    let entries = fs::read_dir("/sys/class/backlight").ok()?;
    entries.flatten().map(|e| e.path()).next()
}

fn read_int(path: &std::path::Path) -> Option<u32> {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

use chrono::{DateTime, Local, Timelike};

use crate::widgets::{BarWidget, WidgetSlot};

pub struct ClockWidget {
    label: String,
    hours_norm: f32,
    minutes_norm: f32,
}

impl ClockWidget {
    pub fn new() -> Self {
        let mut w = Self {
            label: String::new(),
            hours_norm: 0.0,
            minutes_norm: 0.0,
        };
        w.refresh();
        w
    }

    fn refresh(&mut self) -> bool {
        let now: DateTime<Local> = Local::now();
        let label = now.format("%a %e %b %H:%M").to_string();
        let h = (now.hour() % 12) as f32;
        let m = now.minute() as f32;
        let s = now.second() as f32;
        let hours_norm = ((h + m / 60.0) / 12.0).fract();
        let minutes_norm = (m + s / 60.0) / 60.0;
        let changed = label != self.label;
        self.label = label;
        self.hours_norm = hours_norm;
        self.minutes_norm = minutes_norm;
        changed
    }
}

impl Default for ClockWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl BarWidget for ClockWidget {
    fn id(&self) -> &'static str {
        "clock"
    }

    fn slot(&self) -> WidgetSlot {
        WidgetSlot::Left
    }

    fn update(&mut self) -> bool {
        self.refresh()
    }

    fn label(&self) -> &str {
        &self.label
    }
}

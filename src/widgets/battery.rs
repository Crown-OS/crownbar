use battery::Manager as BatteryManager;

use crate::{
    animation::Spring,
    util::poll::PollGate,
    widgets::{BarWidget, Icon, WidgetSlot},
};

const POLL_PERIOD_TICKS: u32 = 5;

pub struct BatteryWidget {
    manager: BatteryManager,
    percent: f32,
    charging: bool,
    saver: bool,
    pct_anim: Spring,
    charging_anim: Spring,
    saver_anim: Spring,
    gate: PollGate,
}

impl BatteryWidget {
    pub fn try_new() -> Option<Self> {
        let manager = match BatteryManager::new() {
            Ok(m) => m,
            Err(e) => {
                log::warn!("battery manager unavailable: {e}");
                return None;
            }
        };
        let mut w = Self {
            manager,
            percent: 0.0,
            charging: false,
            saver: false,
            pct_anim: Spring::new(0.0),
            charging_anim: Spring::new(0.0),
            saver_anim: Spring::new(0.0),
            gate: PollGate::new(POLL_PERIOD_TICKS),
        };
        w.refresh();
        w.pct_anim.position = w.percent / 100.0;
        w.pct_anim.set_target(w.percent / 100.0);
        Some(w)
    }

    fn refresh(&mut self) {
        let Some(mut battery) = self
            .manager
            .batteries()
            .ok()
            .and_then(|mut it| it.next().and_then(|r| r.ok()))
        else {
            return;
        };
        let _ = self.manager.refresh(&mut battery);
        self.percent = (battery.state_of_charge().value * 100.0).clamp(0.0, 100.0);
        self.charging = matches!(battery.state(), battery::State::Charging);
        self.pct_anim.set_target(self.percent / 100.0);
        self.charging_anim
            .set_target(if self.charging { 1.0 } else { 0.0 });
    }
}

impl BarWidget for BatteryWidget {
    fn id(&self) -> &'static str {
        "battery"
    }

    fn slot(&self) -> WidgetSlot {
        WidgetSlot::Right
    }

    fn icon(&self) -> Icon {
        Icon::Battery {
            pct: self.pct_anim.position.clamp(0.05, 1.0),
            charging: self.charging_anim.position,
            saver: self.saver_anim.position,
        }
    }

    fn update(&mut self) -> bool {
        if !self.gate.should_run() {
            return false;
        }
        let prev_pct = self.percent.round() as i32;
        let prev_charging = self.charging;
        self.refresh();
        prev_pct != self.percent.round() as i32 || prev_charging != self.charging
    }

    fn label(&self) -> String {
        format!("{:.0}%", self.percent)
    }

    fn on_click(&mut self) -> bool {
        self.saver = !self.saver;
        self.saver_anim
            .set_target(if self.saver { 1.0 } else { 0.0 });
        true
    }

    fn tick_animation(&mut self, dt: f32) -> bool {
        let mut alive = false;
        for s in [
            &mut self.pct_anim,
            &mut self.charging_anim,
            &mut self.saver_anim,
        ] {
            if !s.at_rest() {
                s.step(dt);
                if !s.at_rest() {
                    alive = true;
                }
            }
        }
        alive
    }
}

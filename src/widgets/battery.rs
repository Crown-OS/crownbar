//! Battery widget. The charge and the power profile both arrive from
//! [`crate::services`] — the cell from sysfs on the runtime, the profile from
//! power-profiles-daemon over D-Bus — so nothing here reads the machine.
//!
//! Each visual state the cell can be in is a spring: charge, plugged in,
//! saving power, and about to go flat. The icon module is handed their live
//! positions every frame, so a profile change or a plug-in event morphs the
//! glyph rather than switching it.

use std::sync::Arc;

use crate::{
    animation::Spring,
    services::{
        battery::{BatteryState, Charge},
        link::{self, SettingsPane},
        power::{PowerCommand, PowerState, Profile},
        Services,
    },
    widgets::{
        popup::{Item, PanelBuilder, Row},
        AfterAction, BarWidget, Icon, PopupAction, PopupSpec, Rune, WidgetSlot,
    },
};

/// Charge at or below which the cell goes red, while nothing is plugged in.
const LOW_CHARGE: f32 = 0.20;
const PANEL_WIDTH: f32 = 268.0;

pub struct BatteryWidget {
    charge: Arc<BatteryState>,
    power: Arc<PowerState>,
    level: Spring,
    charging: Spring,
    saver: Spring,
    low: Spring,
    targets: Vec<Option<Target>>,
}

#[derive(Clone, Copy)]
enum Target {
    Mode(Profile),
    Settings,
}

impl BatteryWidget {
    /// Always constructed. Whether the machine has a cell is answered per
    /// frame by [`BarWidget::visible`], so a battery that appears later — a
    /// UPS, a hot-plugged pack — brings the pill with it.
    pub fn new() -> Self {
        Self {
            charge: Arc::default(),
            power: Arc::default(),
            level: Spring::new(0.0),
            charging: Spring::new(0.0),
            saver: Spring::new(0.0),
            low: Spring::new(0.0),
            targets: Vec::new(),
        }
    }

    fn springs(&mut self) -> [&mut Spring; 4] {
        [
            &mut self.level,
            &mut self.charging,
            &mut self.saver,
            &mut self.low,
        ]
    }

    /// Point every spring at the state the newest snapshots describe.
    fn retarget(&mut self) {
        let charge = self.charge.charge.unwrap_or(Charge::EMPTY);
        let flat = charge.level <= LOW_CHARGE && !charge.charging();
        self.level.set_target(charge.level);
        self.charging
            .set_target(if charge.charging() { 1.0 } else { 0.0 });
        self.saver
            .set_target(if self.power.profiles.is_saving() { 1.0 } else { 0.0 });
        self.low.set_target(if flat { 1.0 } else { 0.0 });
    }

    fn percent(&self) -> u8 {
        self.charge.charge.map(Charge::percent).unwrap_or(0)
    }
}

impl BarWidget for BatteryWidget {
    fn id(&self) -> &'static str {
        "battery"
    }

    fn slot(&self) -> WidgetSlot {
        WidgetSlot::Right
    }

    fn visible(&self) -> bool {
        self.charge.availability.usable() && self.charge.charge.is_some()
    }

    fn icon(&self) -> Icon {
        Icon::Battery(crate::widgets::BatteryState {
            level: self.level.position,
            readout: self.percent(),
            charging: self.charging.position,
            saver: self.saver.position,
            low: self.low.position,
        })
    }

    fn sync(&mut self, services: &Services) -> bool {
        let charge = services.battery.read();
        let power = services.power.read();
        // Pointer equality: the services hand out the same `Arc` until they
        // publish, so an unrelated wake costs one comparison and no work.
        if Arc::ptr_eq(&charge, &self.charge) && Arc::ptr_eq(&power, &self.power) {
            return false;
        }
        self.charge = charge;
        self.power = power;
        self.retarget();
        true
    }

    fn popup(&mut self, _services: &Services) -> Option<PopupSpec> {
        let charge = self.charge.charge?;

        let mut panel = PanelBuilder::new(PANEL_WIDTH);
        panel.row(Row::Header {
            title: "Battery".into(),
            toggle: None,
        });
        panel.row(
            Item::new(format!("{}%", charge.percent()))
                .plain()
                .icon(self.icon())
                .detail(charge.summary())
                .enabled(false)
                .row(),
        );

        let profiles = &self.power.profiles;
        if !profiles.supported.is_empty() {
            panel.row(Row::Separator);
            panel.row(Row::Section {
                title: "Power Mode".into(),
                chevron: false,
            });
            for mode in &profiles.supported {
                let mut item = Item::new(mode.label())
                    .icon(Icon::Rune(rune_for(*mode)))
                    .selected(profiles.active == Some(*mode));
                // The daemon says High Power is thermally limited; saying so
                // beats leaving the user wondering why it feels like Balanced.
                if let Some(why) = profiles.degraded.as_deref()
                    && *mode == Profile::Performance
                {
                    item = item.detail(why);
                }
                panel.action(item.row(), Target::Mode(*mode));
            }
        }

        if let Some(failure) = self.power.failure.as_ref() {
            panel.row(
                Item::new(failure.kind.summary())
                    .plain()
                    .enabled(false)
                    .row(),
            );
        }

        panel.row(Row::Separator);
        panel.action(
            Row::Action {
                label: "Battery Settings…".into(),
            },
            Target::Settings,
        );

        let (spec, targets) = panel.finish();
        self.targets = targets;
        Some(spec)
    }

    fn on_popup(&mut self, action: PopupAction, services: &Services) -> AfterAction {
        let PopupAction::Activate { row } = action else {
            return AfterAction::Stay;
        };
        match self.targets.get(row).copied().flatten() {
            // The service echoes the choice back before it tells the daemon,
            // so the cell starts travelling immediately — and unlike the old
            // detached write, a refusal comes back and says so.
            Some(Target::Mode(mode)) => {
                services.power.send(PowerCommand::SetProfile(mode));
                AfterAction::Stay
            }
            Some(Target::Settings) => {
                if !link::open(SettingsPane::Battery) {
                    log::info!("no power settings application installed");
                }
                AfterAction::Close
            }
            None => AfterAction::Stay,
        }
    }

    fn tick_animation(&mut self, dt: f32) -> bool {
        let mut busy = false;
        for spring in self.springs() {
            if spring.at_rest() {
                continue;
            }
            spring.step(dt);
            busy |= !spring.at_rest();
        }
        busy
    }
}

fn rune_for(profile: Profile) -> Rune {
    match profile {
        Profile::Saver => Rune::Leaf,
        Profile::Balanced => Rune::Gauge,
        Profile::Performance => Rune::Bolt,
    }
}

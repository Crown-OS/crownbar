//! Battery widget. The charge and the power profile both arrive from
//! [`crownbar_integrations`] — the cell from sysfs on the runtime, the profile from
//! power-profiles-daemon over D-Bus — so nothing here reads the machine.
//!
//! Each visual state the cell can be in is a spring: charge, plugged in,
//! saving power, and about to go flat. The icon module is handed their live
//! positions every frame, so a profile change or a plug-in event morphs the
//! glyph rather than switching it.
//!
//! The panel also lists the charge of every linked phone that shares it.

use std::sync::Arc;

use crate::{
    AfterAction, BarWidget, Icon, PopupAction, PopupSpec, Rune,
    animation::Spring,
    phone,
    popup::{Header, Item, PanelBuilder, Row},
    snapshot::Snapshot,
};
use crownbar_integrations::{
    Services,
    battery::{BatteryState, Charge},
    crownlink::{Feature, PhoneDevice, PhoneState},
    link::{self, SettingsPane},
    power::{PowerCommand, PowerState, Profile},
};

/// Charge at or below which the cell goes red, while nothing is plugged in.
const LOW_CHARGE: f32 = 0.20;

pub struct BatteryWidget {
    charge: Arc<BatteryState>,
    power: Arc<PowerState>,
    phones: Snapshot<PhoneState>,
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
            phones: Snapshot::default(),
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
    fn retarget(&mut self) -> bool {
        let charge = self.charge.charge.unwrap_or(Charge::EMPTY);
        let flat = charge.level <= LOW_CHARGE && !charge.charging();
        self.level.set_target(charge.level)
            | self
                .charging
                .set_target(if charge.charging() { 1.0 } else { 0.0 })
            | self.saver.set_target(if self.power.profiles.is_saving() {
                1.0
            } else {
                0.0
            })
            | self.low.set_target(if flat { 1.0 } else { 0.0 })
    }

    fn percent(&self) -> u8 {
        self.charge.charge.map(Charge::percent).unwrap_or(0)
    }

    fn panel(&mut self) -> Option<PopupSpec> {
        let charge = self.charge.charge?;
        let phones = self.phones.for_panel();

        let mut panel = PanelBuilder::new();
        panel.row(Header::new("Battery").row());
        panel.row(Item::new(charge.summary()).plain().enabled(false).row());

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

        devices(&mut panel, &phones.devices);

        panel.row(Row::Separator);
        panel.action(Row::action("Battery Settings…"), Target::Settings);

        let (spec, targets) = panel.finish();
        self.targets = targets;
        Some(spec)
    }
}

impl BarWidget for BatteryWidget {
    fn id(&self) -> &str {
        "battery"
    }

    fn visible(&self) -> bool {
        self.charge.availability.usable() && self.charge.charge.is_some()
    }

    fn icon(&self) -> Icon {
        Icon::Battery(crate::BatteryState {
            level: self.level.position,
            readout: self.percent(),
            charging: self.charging.position,
            saver: self.saver.position,
            low: self.low.position,
        })
    }

    fn sync(&mut self, services: &Services) -> bool {
        self.phones.replace(services.crownlink.read());
        let charge = services.battery.read();
        let power = services.power.read();
        // Pointer equality: the services hand out the same `Arc` until they
        // publish, so an unrelated wake costs one comparison and no work.
        if Arc::ptr_eq(&charge, &self.charge) && Arc::ptr_eq(&power, &self.power) {
            return false;
        }
        self.charge = charge;
        self.power = power;
        self.retarget()
    }

    fn popup(&mut self, _services: &Services) -> Option<PopupSpec> {
        self.panel()
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
                link::open(SettingsPane::Battery);
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

/// "Devices": every linked device sharing its charge, with the percentage
/// once it has reported one.
fn devices(panel: &mut PanelBuilder<Target>, devices: &[PhoneDevice]) {
    let mut sharing = phone::allowing(devices, Feature::Battery).peekable();
    if sharing.peek().is_none() {
        return;
    }
    phone::section(panel, "Devices");
    for device in sharing {
        let item = phone::item(device, Feature::Battery);
        panel.row(match device.battery {
            Some(battery) => item.detail(format!("{}%", battery.percent)).row(),
            None => item.row(),
        });
    }
}

fn rune_for(profile: Profile) -> Rune {
    match profile {
        Profile::Saver => Rune::GaugeLow,
        Profile::Balanced => Rune::GaugeMid,
        Profile::Performance => Rune::GaugeHigh,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phone::fixture::*;

    fn battery_panel(devices: Vec<PhoneDevice>) -> Option<PopupSpec> {
        let mut widget = BatteryWidget::new();
        widget.charge = Arc::new(BatteryState {
            charge: Some(Charge::EMPTY),
            ..BatteryState::default()
        });
        widget.phones.replace(state(devices));
        widget.panel()
    }

    #[test]
    fn no_phone_means_no_devices_heading() {
        let spec = battery_panel(Vec::new()).expect("a machine with a battery has a panel");
        assert!(!sections(&spec).contains(&"Devices"));
    }

    #[test]
    fn each_phone_lists_its_charge_once_it_has_one() {
        for devices in rosters() {
            let devices: Vec<_> = devices
                .into_iter()
                .enumerate()
                .map(|(index, device)| match index {
                    0 => charged(device, 82),
                    _ => device,
                })
                .collect();
            let spec = battery_panel(devices.clone()).expect("panel");
            let rows = section_items(&spec, "Devices");
            let read: Vec<_> = rows
                .iter()
                .map(|(_, item)| (item.label.as_str(), item.detail.as_deref()))
                .collect();
            let expected: Vec<_> = devices
                .iter()
                .enumerate()
                .map(|(index, device)| (&*device.name, (index == 0).then_some("82%")))
                .collect();
            assert_eq!(read, expected);
        }
    }

    #[test]
    fn a_device_that_keeps_its_charge_private_is_not_listed() {
        let spec = battery_panel(vec![forbidding(
            charged(phone(PIXEL, "Pixel 8"), 50),
            Feature::Battery,
        )])
        .expect("panel");
        assert!(!sections(&spec).contains(&"Devices"));
    }
}

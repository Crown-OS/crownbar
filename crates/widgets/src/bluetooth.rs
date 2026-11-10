//! Bluetooth widget and the panel behind it.
//!
//! The icon follows the radio, which [`crownbar_integrations::bluetooth`] answers
//! from rfkill when BlueZ is not up and from BlueZ when it is — so the pill is
//! right before any daemon starts and stays right if one dies.

use crate::{
    AfterAction, BarWidget, Icon, PopupAction, PopupSpec, Rune,
    animation::Spring,
    popup::{Header, Item, PanelBuilder, Row},
    snapshot::Snapshot,
};
use crownbar_integrations::{
    Interest, Services,
    bluetooth::{Address, BluetoothCommand, BluetoothState, DeviceKind},
    link::{self, SettingsPane},
};

pub struct BluetoothWidget {
    bluetooth: Snapshot<BluetoothState>,
    on: Spring,
    targets: Vec<Option<Target>>,
}

#[derive(Clone, Copy)]
enum Target {
    Refresh,
    Paired(Address),
    Discovered(Address),
    Settings,
}

impl BluetoothWidget {
    pub fn new() -> Self {
        Self {
            bluetooth: Snapshot::default(),
            on: Spring::new(0.0),
            targets: Vec::new(),
        }
    }

    fn device_busy(&self, address: Address) -> bool {
        self.bluetooth
            .device(address)
            .is_some_and(|device| device.busy)
    }
}

impl BarWidget for BluetoothWidget {
    fn id(&self) -> &str {
        "bluetooth"
    }

    /// A radio with no BlueZ behind it still gets a pill; no radio at all does
    /// not.
    fn visible(&self) -> bool {
        self.bluetooth.availability.usable()
    }

    fn icon(&self) -> Icon {
        Icon::Bluetooth {
            on: self.on.position,
        }
    }

    fn sync(&mut self, services: &Services) -> bool {
        if !self.bluetooth.replace(services.bluetooth.read()) {
            return false;
        }
        self.on
            .set_target(if self.bluetooth.radio.on() { 1.0 } else { 0.0 })
    }

    fn popup(&mut self, services: &Services) -> Option<PopupSpec> {
        // Discovery costs radio time, so it runs only while this panel is up.
        services
            .bluetooth
            .send(BluetoothCommand::Interest(Interest::Panel));

        let state = self.bluetooth.for_panel();
        let mut panel = PanelBuilder::new();
        let header = Header::new("Bluetooth").toggle(state.radio.on());
        if state.radio.on() {
            panel.action(
                header.button(Rune::Refresh, state.discovering).row(),
                Target::Refresh,
            );
        } else {
            panel.row(header.row());
        }

        if !state.radio.changeable() {
            panel.row(
                Item::new("Hardware switch is off")
                    .plain()
                    .enabled(false)
                    .row(),
            );
        } else if state.radio.on() {
            for device in &state.paired {
                let mut item = Item::new(&device.name)
                    .icon(Icon::Rune(rune_for(device.kind)))
                    .selected(device.connected)
                    .busy(device.busy);
                if let Some(battery) = device.battery.filter(|_| device.connected) {
                    item = item
                        .detail(format!("{battery}%"))
                        .battery(battery as f32 / 100.0);
                }
                if let Some(signal) = device.signal {
                    item = item.signal(signal.fraction());
                }
                panel.action(item.row(), Target::Paired(device.address));
            }
            if state.paired.is_empty() {
                panel.row(Item::new("No Devices Paired").plain().enabled(false).row());
            }

            if !state.discovered.is_empty() {
                panel.row(Row::Separator);
                panel.row(Row::Section {
                    title: "Other Devices".into(),
                    chevron: false,
                });
                for device in &state.discovered {
                    let mut item = Item::new(&device.name)
                        .icon(Icon::Rune(rune_for(device.kind)))
                        .busy(device.busy);
                    if let Some(signal) = device.signal {
                        item = item.signal(signal.fraction());
                    }
                    panel.action(item.row(), Target::Discovered(device.address));
                }
            }
        }

        if let Some(failure) = state.failure.as_ref() {
            panel.row(
                Item::new(failure.kind.summary())
                    .plain()
                    .enabled(false)
                    .row(),
            );
        }

        panel.row(Row::Separator);
        panel.action(Row::action("Bluetooth Settings…"), Target::Settings);

        let (spec, targets) = panel.finish();
        self.targets = targets;
        Some(spec)
    }

    fn on_popup(&mut self, action: PopupAction, services: &Services) -> AfterAction {
        match action {
            PopupAction::Toggle { on, .. } => {
                services.bluetooth.send(BluetoothCommand::SetPowered(on));
                AfterAction::Stay
            }
            PopupAction::Activate { row } => {
                match self.targets.get(row).copied().flatten() {
                    Some(Target::Refresh) => {
                        if !self.bluetooth.discovering {
                            services.bluetooth.send(BluetoothCommand::Scan);
                        }
                        AfterAction::Stay
                    }
                    Some(Target::Paired(address) | Target::Discovered(address))
                        if self.device_busy(address) =>
                    {
                        AfterAction::Stay
                    }
                    Some(Target::Paired(address)) => {
                        let connected = self
                            .bluetooth
                            .device(address)
                            .is_some_and(|device| device.connected);
                        services.bluetooth.send(if connected {
                            BluetoothCommand::Disconnect(address)
                        } else {
                            BluetoothCommand::Connect(address)
                        });
                        AfterAction::Stay
                    }
                    // Pairing may need a PIN, which the bar has nowhere to
                    // collect; BlueZ's session agent answers for the devices
                    // that pair without one, and the rest go to settings.
                    Some(Target::Discovered(address)) => {
                        services
                            .bluetooth
                            .send(BluetoothCommand::PairAndConnect(address));
                        AfterAction::Stay
                    }
                    Some(Target::Settings) => {
                        link::open(SettingsPane::Bluetooth);
                        AfterAction::Close
                    }
                    None => AfterAction::Stay,
                }
            }
            PopupAction::Slide { .. } => AfterAction::Stay,
        }
    }

    fn popup_closed(&mut self, services: &Services) {
        services
            .bluetooth
            .send(BluetoothCommand::Interest(Interest::Idle));
    }

    fn panel_stale(&self) -> bool {
        self.bluetooth.panel_stale()
    }

    fn tick_animation(&mut self, dt: f32) -> bool {
        if self.on.at_rest() {
            return false;
        }
        self.on.step(dt);
        !self.on.at_rest()
    }
}

fn rune_for(kind: DeviceKind) -> Rune {
    match kind {
        DeviceKind::Headphones | DeviceKind::Headset => Rune::Headphones,
        DeviceKind::Speaker => Rune::Speaker,
        DeviceKind::Keyboard | DeviceKind::Mouse => Rune::Keyboard,
        DeviceKind::Gamepad => Rune::Gamepad,
        DeviceKind::Phone => Rune::Phone,
        DeviceKind::Computer => Rune::Laptop,
        DeviceKind::Display => Rune::Display,
        DeviceKind::Other => Rune::Bluetooth,
    }
}

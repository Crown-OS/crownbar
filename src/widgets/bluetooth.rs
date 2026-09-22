//! Bluetooth widget and the panel behind it.
//!
//! The icon follows the radio, which [`crate::services::bluetooth`] answers
//! from rfkill when BlueZ is not up and from BlueZ when it is — so the pill is
//! right before any daemon starts and stays right if one dies.

use std::sync::Arc;

use crate::{
    animation::Spring,
    services::{
        bluetooth::{Address, BluetoothCommand, BluetoothState, DeviceKind},
        link::{self, SettingsPane},
        Interest, Services,
    },
    widgets::{
        popup::{Item, PanelBuilder, Row},
        AfterAction, BarWidget, Icon, PopupAction, PopupSpec, Rune, WidgetSlot,
    },
};

pub struct BluetoothWidget {
    bluetooth: Arc<BluetoothState>,
    on: Spring,
    targets: Vec<Option<Target>>,
}

#[derive(Clone, Copy)]
enum Target {
    Paired(Address),
    Discovered(Address),
    Settings,
}

impl BluetoothWidget {
    pub fn new() -> Self {
        Self {
            bluetooth: Arc::default(),
            on: Spring::new(0.0),
            targets: Vec::new(),
        }
    }
}

impl BarWidget for BluetoothWidget {
    fn id(&self) -> &'static str {
        "bluetooth"
    }

    fn slot(&self) -> WidgetSlot {
        WidgetSlot::Right
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
        let bluetooth = services.bluetooth.read();
        if Arc::ptr_eq(&bluetooth, &self.bluetooth) {
            return false;
        }
        self.bluetooth = bluetooth;
        self.on
            .set_target(if self.bluetooth.radio.on() { 1.0 } else { 0.0 })
    }

    fn popup(&mut self, services: &Services) -> Option<PopupSpec> {
        // Discovery costs radio time, so it runs only while this panel is up.
        services
            .bluetooth
            .send(BluetoothCommand::Interest(Interest::Panel));

        let state = self.bluetooth.clone();
        let mut panel = PanelBuilder::new();
        panel.row(Row::Header {
            title: "Bluetooth".into(),
            toggle: Some(state.radio.on()),
        });

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
                    .selected(device.connected);
                if let Some(battery) = device.battery.filter(|_| device.connected) {
                    item = item
                        .detail(format!("{battery}%"))
                        .battery(battery as f32 / 100.0);
                } else if device.busy {
                    item = item.detail("…");
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
                    panel.action(
                        Item::new(&device.name)
                            .icon(Icon::Rune(rune_for(device.kind)))
                            .row(),
                        Target::Discovered(device.address),
                    );
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
        panel.action(
            Row::Action {
                label: "Bluetooth Settings…".into(),
            },
            Target::Settings,
        );

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
                    Some(Target::Paired(address)) => {
                        let connected = self
                            .bluetooth
                            .device(address)
                            .map(|device| device.connected)
                            .unwrap_or(false);
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
                        if !link::open(SettingsPane::Bluetooth) {
                            log::info!("no bluetooth settings application installed");
                        }
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
        DeviceKind::Phone => Rune::Phone,
        DeviceKind::Computer => Rune::Laptop,
        DeviceKind::Display => Rune::Display,
        DeviceKind::Other => Rune::Bluetooth,
    }
}

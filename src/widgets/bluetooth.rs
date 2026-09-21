//! Bluetooth widget and the panel behind it.
//!
//! The bar icon's state still comes from rfkill (see [`crate::util::rfkill`]),
//! so it is right before any desktop daemon is up. The panel needs the paired
//! device list, which only BlueZ has, so it reads that through
//! [`crate::util::bluetooth`] on a worker thread and shows the switch alone
//! where BlueZ is not answering.

use crate::{
    services::Services,
    animation::Spring,
    util::{
        bluetooth::{self, DeviceKind, Snapshot},
        poll::PollGate,
        rfkill,
        worker::Job,
    },
    widgets::{
        popup::{Item, PanelBuilder, Row},
        AfterAction, BarWidget, Icon, PopupAction, PopupSpec, Rune, WidgetSlot,
    },
};

const POLL_PERIOD_TICKS: u32 = 3;
const KIND: &str = "bluetooth";
const PANEL_WIDTH: f32 = 300.0;

pub struct BluetoothWidget {
    on: bool,
    on_anim: Spring,
    gate: PollGate,
    /// Adapter and devices, as of the last completed panel read.
    snapshot: Snapshot,
    job: Job<Snapshot>,
    targets: Vec<Option<Target>>,
    panel_open: bool,
}

impl BluetoothWidget {
    pub fn try_new() -> Option<Self> {
        // No radio at all is the one case where the widget does not belong on
        // the bar. A radio with no BlueZ behind it still gets an icon.
        if !rfkill::present(KIND) && !bluetooth::available() {
            return None;
        }
        let on = rfkill::unblocked(KIND);
        Some(Self {
            on,
            on_anim: Spring::new(if on { 1.0 } else { 0.0 }),
            gate: PollGate::new(POLL_PERIOD_TICKS),
            snapshot: Snapshot::default(),
            job: Job::idle(),
            targets: Vec::new(),
            panel_open: false,
        })
    }

    fn set_on(&mut self, on: bool) {
        self.on = on;
        self.on_anim.set_target(if on { 1.0 } else { 0.0 });
    }

    fn read(&mut self) {
        self.job.request(bluetooth::snapshot);
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
        let next = rfkill::unblocked(KIND);
        if next == self.on {
            return false;
        }
        self.set_on(next);
        true
    }

    fn popup(&mut self, services: &Services) -> Option<PopupSpec> {
        if !self.panel_open {
            self.panel_open = true;
            self.read();
        }

        let mut panel = PanelBuilder::new(PANEL_WIDTH);
        panel.row(Row::Header {
            title: "Bluetooth".into(),
            toggle: Some(self.on),
        });

        if self.on {
            for (index, device) in self.snapshot.devices.iter().enumerate() {
                let mut item = Item::new(&device.name)
                    .icon(Icon::Rune(rune_for(device.kind)))
                    .selected(device.connected);
                if let Some(battery) = device.battery.filter(|_| device.connected) {
                    item = item
                        .detail(format!("{battery}%"))
                        .battery(battery as f32 / 100.0);
                }
                panel.action(item.row(), Target::Device(index));
            }
            if self.snapshot.devices.is_empty() {
                let message = if bluetooth::available() {
                    "No Devices Found"
                } else {
                    "Bluetooth Service Unavailable"
                };
                panel.row(Item::new(message).plain().enabled(false).row());
            }
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
                // Animate at once and let the read that follows correct us if
                // BlueZ refused; waiting on the round trip makes the switch
                // feel broken.
                self.set_on(on);
                std::thread::spawn(move || bluetooth::set_powered(on));
                if !on {
                    self.snapshot.devices.clear();
                }
                AfterAction::Stay
            }
            PopupAction::Activate { row } => match self.targets.get(row).copied().flatten() {
                Some(Target::Device(index)) => {
                    let Some(device) = self.snapshot.devices.get_mut(index) else {
                        return AfterAction::Stay;
                    };
                    let address = device.address.clone();
                    let connect = !device.connected;
                    device.connected = connect;
                    // Connecting blocks until the link is up or times out, so
                    // it never runs on the event loop.
                    std::thread::spawn(move || bluetooth::set_connected(&address, connect));
                    AfterAction::Stay
                }
                Some(Target::Settings) => {
                    if !bluetooth::open_settings() {
                        log::info!("no bluetooth settings application installed");
                    }
                    AfterAction::Close
                }
                None => AfterAction::Stay,
            },
            PopupAction::Slide { .. } => AfterAction::Stay,
        }
    }

    fn popup_poll(&mut self, slow: bool) -> bool {
        let mut changed = false;
        if let Some(snapshot) = self.job.take() {
            if let Some(powered) = snapshot.powered
                && powered != self.on
            {
                self.set_on(powered);
                changed = true;
            }
            changed |= snapshot.devices != self.snapshot.devices;
            self.snapshot = snapshot;
        }
        if slow {
            self.read();
        }
        changed
    }

    fn popup_busy(&self) -> bool {
        self.job.in_flight()
    }

    fn popup_closed(&mut self, _services: &Services) {
        self.panel_open = false;
    }

    fn tick_animation(&mut self, dt: f32) -> bool {
        if self.on_anim.at_rest() {
            return false;
        }
        self.on_anim.step(dt);
        !self.on_anim.at_rest()
    }
}

#[derive(Clone, Copy)]
enum Target {
    Device(usize),
    Settings,
}

fn rune_for(kind: DeviceKind) -> Rune {
    match kind {
        DeviceKind::Headphones => Rune::Headphones,
        DeviceKind::Speaker => Rune::Speaker,
        DeviceKind::Input => Rune::Keyboard,
        DeviceKind::Phone => Rune::Phone,
        DeviceKind::Display => Rune::Display,
        DeviceKind::Other => Rune::Bluetooth,
    }
}

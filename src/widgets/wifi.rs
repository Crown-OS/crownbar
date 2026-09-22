//! Wi-Fi widget and the panel behind it.
//!
//! The icon follows [`crate::services::network`], which reads NetworkManager
//! when it is up and falls back to the kernel's own carrier state when it is
//! not — so the pill is right on a cold boot and stays right across a daemon
//! restart.

use std::sync::Arc;

use crate::{
    animation::Spring,
    services::{
        link::{self, SettingsPane},
        network::{NetworkCommand, NetworkState, Radio, WifiNetwork},
        Interest, Services,
    },
    widgets::{
        popup::{Item, PanelBuilder, Row},
        AfterAction, BarWidget, Icon, PopupAction, PopupSpec, Rune, WidgetSlot, WifiState,
    },
};

/// One scanning sweep, in seconds.
const SWEEP_SECS: f32 = 1.4;
pub struct WifiWidget {
    network: Arc<NetworkState>,
    strength: Spring,
    off: Spring,
    searching: Spring,
    phase: f32,
    targets: Vec<Option<Target>>,
}

#[derive(Clone)]
enum Target {
    /// Joinable from here: open, or already saved.
    Join(String),
    /// Needs a password the bar has nowhere to type, so it hands off.
    Handoff,
    Settings,
}

impl WifiWidget {
    pub fn new() -> Self {
        Self {
            network: Arc::default(),
            strength: Spring::new(0.0),
            off: Spring::new(0.0),
            searching: Spring::new(0.0),
            phase: 0.0,
            targets: Vec::new(),
        }
    }

    fn springs(&mut self) -> [&mut Spring; 3] {
        [&mut self.strength, &mut self.off, &mut self.searching]
    }

    /// Anything short of a carrier reads as searching — that covers scanning,
    /// associating and DHCP alike.
    fn retarget(&mut self) -> bool {
        let network = self.network.clone();
        let (strength, off, searching) = match (network.radio, &network.connected) {
            (Radio::Off | Radio::HardBlocked | Radio::Absent, _) => (0.0, 1.0, 0.0),
            (_, Some(joined)) => (joined.strength, 0.0, 0.0),
            (_, None) => (0.0, 0.0, 1.0),
        };
        self.strength.set_target(strength)
            | self.off.set_target(off)
            | self.searching.set_target(searching)
    }

    fn group(&self, panel: &mut PanelBuilder<Target>, title: &str, networks: &[WifiNetwork]) {
        if networks.is_empty() {
            return;
        }
        panel.row(Row::Separator);
        panel.row(Row::Section {
            title: title.into(),
            chevron: false,
        });
        for network in networks {
            panel.action(
                Item::new(&network.ssid)
                    .icon(Icon::Rune(Rune::Wifi))
                    // macOS flags an open network, and weak ciphers deserve
                    // the same warning.
                    .warning(!network.secured || network.weak)
                    .row(),
                match self.network.joinable(&network.ssid) {
                    true => Target::Join(network.ssid.clone()),
                    false => Target::Handoff,
                },
            );
        }
    }
}

impl BarWidget for WifiWidget {
    fn id(&self) -> &'static str {
        "wifi"
    }

    fn slot(&self) -> WidgetSlot {
        WidgetSlot::Right
    }

    fn visible(&self) -> bool {
        self.network.radio != Radio::Absent
    }

    fn icon(&self) -> Icon {
        Icon::Wifi(WifiState {
            strength: self.strength.position,
            off: self.off.position,
            searching: self.searching.position,
            phase: self.phase,
        })
    }

    fn sync(&mut self, services: &Services) -> bool {
        let network = services.network.read();
        if Arc::ptr_eq(&network, &self.network) {
            return false;
        }
        self.network = network;
        // Only the pill's own state is worth a repaint; a change to the list
        // behind it reaches the panel through its own invalidation.
        self.retarget()
    }

    fn popup(&mut self, services: &Services) -> Option<PopupSpec> {
        // Scanning costs radio time, so it runs only while the panel is up.
        services
            .network
            .send(NetworkCommand::Interest(Interest::Panel));

        let state = self.network.clone();
        let mut panel = PanelBuilder::new();
        panel.row(Row::Header {
            title: "Wi-Fi".into(),
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
            if let Some(joined) = state.connected.as_ref() {
                panel.action(
                    Item::new(&joined.ssid)
                        .icon(Icon::Rune(Rune::Wifi))
                        .selected(true)
                        .warning(!joined.secured || joined.weak)
                        .row(),
                    Target::Join(joined.ssid.clone()),
                );
            }
            self.group(&mut panel, "Known Network", &state.known);
            self.group(&mut panel, "Other Networks", &state.others);

            if state.connected.is_none() && state.known.is_empty() && state.others.is_empty() {
                panel.row(
                    Item::new("Looking for Networks…")
                        .plain()
                        .enabled(false)
                        .row(),
                );
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
                label: "Wi-Fi Settings…".into(),
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
                services.network.send(NetworkCommand::SetRadio(on));
                AfterAction::Stay
            }
            PopupAction::Activate { row } => match self.targets.get(row).cloned().flatten() {
                Some(Target::Join(ssid)) => {
                    // Tapping the joined network disconnects, the way the
                    // Bluetooth rows toggle.
                    let joined = self
                        .network
                        .connected
                        .as_ref()
                        .is_some_and(|current| current.ssid == ssid);
                    services.network.send(if joined {
                        NetworkCommand::Disconnect
                    } else {
                        NetworkCommand::Connect(ssid)
                    });
                    AfterAction::Close
                }
                // A new secured network needs a password, and the popup has no
                // keyboard — so settings takes it from here.
                Some(Target::Handoff) | Some(Target::Settings) => {
                    if !link::open(SettingsPane::Network) {
                        log::info!("no network settings application installed");
                    }
                    AfterAction::Close
                }
                None => AfterAction::Stay,
            },
            PopupAction::Slide { .. } | PopupAction::Page { .. } => AfterAction::Stay,
        }
    }

    fn popup_closed(&mut self, services: &Services) {
        services
            .network
            .send(NetworkCommand::Interest(Interest::Idle));
    }

    fn tick_animation(&mut self, dt: f32) -> bool {
        // The sweep runs whenever the searching spring is off its rest, so the
        // animation fades in and out with the state rather than snapping.
        let sweeping = self.searching.position > 0.01;
        if sweeping {
            self.phase = (self.phase + dt / SWEEP_SECS).fract();
        }
        let mut alive = sweeping;
        for spring in self.springs() {
            if spring.at_rest() {
                continue;
            }
            spring.step(dt);
            alive |= !spring.at_rest();
        }
        alive
    }
}

//! Wi-Fi widget and the panel behind it.
//!
//! The icon follows [`crownbar_integrations::network`], which reads NetworkManager
//! when it is up and falls back to the kernel's own carrier state when it is
//! not — so the pill is right on a cold boot and stays right across a daemon
//! restart.

use crate::{
    AfterAction, BarWidget, Icon, PopupAction, PopupSpec, Rune, WifiState,
    animation::Spring,
    popup::{Header, Item, PanelBuilder, Row},
    snapshot::Snapshot,
};
use crownbar_integrations::{
    Interest, Services,
    link::{self, SettingsPane},
    network::{NetworkCommand, NetworkState, Radio, WifiNetwork},
};

/// One scanning sweep, in seconds.
const SWEEP_SECS: f32 = 1.4;
pub struct WifiWidget {
    network: Snapshot<NetworkState>,
    strength: Spring,
    off: Spring,
    searching: Spring,
    phase: f32,
    targets: Vec<Option<Target>>,
}

#[derive(Clone)]
enum Target {
    Refresh,
    /// Joinable from here: open, or already saved.
    Join(String),
    /// Needs a password the bar has nowhere to type, so it hands off.
    Handoff,
    Settings,
}

impl WifiWidget {
    pub fn new() -> Self {
        Self {
            network: Snapshot::default(),
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
        let (strength, off, searching) = match (self.network.radio, &self.network.connected) {
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
                    .signal(network.strength)
                    .busy(self.network.pending.as_deref() == Some(network.ssid.as_str()))
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
    fn id(&self) -> &str {
        "wifi"
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
        if !self.network.replace(services.network.read()) {
            return false;
        }
        // Only the pill's own state is worth a repaint; a change to the list
        // behind it reaches the panel through its own invalidation.
        self.retarget()
    }

    fn popup(&mut self, services: &Services) -> Option<PopupSpec> {
        // Scanning costs radio time, so it runs only while the panel is up.
        services
            .network
            .send(NetworkCommand::Interest(Interest::Panel));

        let state = self.network.for_panel();
        let mut panel = PanelBuilder::new();
        let header = Header::new("Wi-Fi").toggle(state.radio.on());
        if state.radio.on() {
            panel.action(
                header.button(Rune::Refresh, state.scanning).row(),
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
            if let Some(joined) = state.connected.as_ref() {
                panel.action(
                    Item::new(&joined.ssid)
                        .icon(Icon::Rune(Rune::Wifi))
                        .selected(true)
                        .warning(!joined.secured || joined.weak)
                        .signal(joined.strength)
                        .busy(state.pending.as_deref() == Some(joined.ssid.as_str()))
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
                Some(Target::Refresh) => {
                    if !self.network.scanning {
                        services.network.send(NetworkCommand::Scan);
                    }
                    AfterAction::Stay
                }
                // One join or leave at a time; the spinner on its row says
                // which.
                Some(Target::Join(_)) if self.network.pending.is_some() => AfterAction::Stay,
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
                    AfterAction::Stay
                }
                // A new secured network needs a password, and the popup has no
                // keyboard — so settings takes it from here.
                Some(Target::Handoff) | Some(Target::Settings) => {
                    link::open(SettingsPane::Network);
                    AfterAction::Close
                }
                None => AfterAction::Stay,
            },
            PopupAction::Slide { .. } => AfterAction::Stay,
        }
    }

    fn popup_closed(&mut self, services: &Services) {
        services
            .network
            .send(NetworkCommand::Interest(Interest::Idle));
    }

    fn panel_stale(&self) -> bool {
        self.network.panel_stale()
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

//! Network widget and the panel behind it.
//!
//! The icon follows [`crownbar_integrations::network`], which reads NetworkManager
//! when it is up and falls back to the kernel's own carrier state when it is
//! not — so the pill is right on a cold boot and stays right across a daemon
//! restart. It draws whichever link the default route leaves by: the Wi-Fi
//! arcs, or an Ethernet port once a cable carries the traffic.

use crate::{
    AfterAction, BarWidget, Icon, PopupAction, PopupSpec, Rune, WifiState,
    animation::Spring,
    popup::{Header, Item, PanelBuilder, Row},
    snapshot::Snapshot,
};
use crownbar_integrations::{
    Interest, Services,
    link::{self, SettingsPane},
    network::{NetworkCommand, NetworkState, Radio, Uplink, WifiNetwork},
};

/// One scanning sweep, in seconds.
const SWEEP_SECS: f32 = 1.4;
pub struct WifiWidget {
    network: Snapshot<NetworkState>,
    strength: Spring,
    off: Spring,
    searching: Spring,
    wired: Spring,
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
            wired: Spring::new(0.0),
            phase: 0.0,
            targets: Vec::new(),
        }
    }

    fn springs(&mut self) -> [&mut Spring; 4] {
        [
            &mut self.strength,
            &mut self.off,
            &mut self.searching,
            &mut self.wired,
        ]
    }

    fn on_cable(&self) -> bool {
        self.network.uplink == Uplink::Ethernet
    }

    /// Anything short of a carrier reads as searching — that covers scanning,
    /// associating and DHCP alike. A cable outranks all of it: the radio's
    /// pose rests while the port is drawn, so it never sweeps unseen.
    fn retarget(&mut self) -> bool {
        let joined = self
            .network
            .connected
            .as_ref()
            .map(|joined| joined.strength);
        let (strength, off, searching) = match (self.on_cable(), self.network.radio, joined) {
            (true, _, strength) => (strength.unwrap_or(0.0), 0.0, 0.0),
            (_, Radio::Off | Radio::HardBlocked | Radio::Absent, _) => (0.0, 1.0, 0.0),
            (_, _, Some(strength)) => (strength, 0.0, 0.0),
            (_, _, None) => (0.0, 0.0, 1.0),
        };
        let wired = if self.on_cable() { 1.0 } else { 0.0 };
        self.strength.set_target(strength)
            | self.off.set_target(off)
            | self.searching.set_target(searching)
            | self.wired.set_target(wired)
    }

    /// The cable, when one is up or the kernel routes through one.
    fn ethernet(panel: &mut PanelBuilder<Target>, state: &NetworkState) {
        if state.wired.is_none() && state.uplink != Uplink::Ethernet {
            return;
        }
        let item = Item::new("Ethernet")
            .icon(Icon::Rune(Rune::Ethernet))
            .selected(true);
        let speed = state.wired.as_ref().and_then(|wired| wired.speed_mbps);
        panel.row(match speed {
            Some(mbps) => item.detail(link_speed(mbps)).row(),
            None => item.row(),
        });
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
        self.network.radio != Radio::Absent || self.on_cable()
    }

    fn icon(&self) -> Icon {
        Icon::Wifi(WifiState {
            strength: self.strength.position,
            off: self.off.position,
            searching: self.searching.position,
            phase: self.phase,
            wired: self.wired.position,
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
        Self::ethernet(&mut panel, &state);

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
                label: "Network Settings…".into(),
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

/// A negotiated link rate the way a settings pane prints it.
fn link_speed(mbps: u32) -> String {
    match mbps {
        0..1000 => format!("{mbps} Mb/s"),
        _ => format!("{} Gb/s", mbps as f32 / 1000.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_speeds_read_in_the_largest_whole_unit() {
        assert_eq!(link_speed(100), "100 Mb/s");
        assert_eq!(link_speed(1000), "1 Gb/s");
        assert_eq!(link_speed(2500), "2.5 Gb/s");
    }
}

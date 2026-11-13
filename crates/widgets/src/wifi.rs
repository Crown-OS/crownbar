//! Network widget and the panel behind it.
//!
//! The icon follows [`crownbar_integrations::network`], which reads NetworkManager
//! when it is up and falls back to the kernel's own carrier state when it is
//! not — so the pill is right on a cold boot and stays right across a daemon
//! restart. It draws whichever link the default route leaves by: the Wi-Fi
//! arcs, or an Ethernet port once a cable carries the traffic.
//!
//! The panel also lists every linked phone that can share its connection,
//! each with a switch for the phone's own hotspot — not this computer's. A
//! phone that shares Wi-Fi passwords is asked for the password of a locked
//! network this computer has never joined, and can be sent the password of
//! the network this computer is on.

use std::sync::Arc;

use crate::{
    AfterAction, BarWidget, Icon, PopupAction, PopupSpec, Rune, WifiState,
    animation::Spring,
    phone,
    popup::{Header, Item, PanelBuilder, Row},
    snapshot::Snapshot,
};
use crownbar_integrations::{
    Interest, Services,
    crownlink::{DeviceId, Feature, PhoneCommand, PhoneDevice, PhoneState},
    link::{self, SettingsPane},
    network::{NetworkCommand, NetworkState, Radio, Uplink, WifiNetwork},
};

/// One scanning sweep, in seconds.
const SWEEP_SECS: f32 = 1.4;
/// The heading over the phones the joined network's password can be sent to.
const SHARE_SECTION: &str = "Share Wi-Fi Password";
pub struct WifiWidget {
    network: Snapshot<NetworkState>,
    phones: Snapshot<PhoneState>,
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
    /// Needs a password a linked phone may know: ask it.
    AskPhone(DeviceId, String),
    /// Send the joined network's password to a linked phone.
    Share(DeviceId, String),
    /// A linked phone's own hotspot.
    Hotspot(DeviceId),
    Settings,
}

/// What a switch in the panel turns on and off.
#[derive(Debug, PartialEq, Eq)]
enum Switch {
    Radio,
    Hotspot(DeviceId),
}

/// The header, with the radio's switch, opens every panel.
const HEADER_ROW: usize = 0;

impl WifiWidget {
    pub fn new() -> Self {
        Self {
            network: Snapshot::default(),
            phones: Snapshot::default(),
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

    /// `asker` is the phone to ask for a password this computer lacks.
    fn group(
        &self,
        panel: &mut PanelBuilder<Target>,
        title: &str,
        networks: &[WifiNetwork],
        asker: Option<&PhoneDevice>,
    ) {
        if networks.is_empty() {
            return;
        }
        panel.row(Row::Separator);
        panel.row(Row::Section {
            title: title.into(),
            chevron: false,
        });
        for network in networks {
            let item = Item::new(&network.ssid)
                .icon(Icon::Rune(Rune::Wifi))
                // macOS flags an open network, and weak ciphers deserve the
                // same warning.
                .warning(!network.secured || network.weak)
                .signal(network.strength)
                .busy(self.network.pending.as_deref() == Some(network.ssid.as_str()));
            let (item, target) = match (self.network.joinable(&network.ssid), asker) {
                (true, _) => (item, Target::Join(network.ssid.clone())),
                (false, Some(phone)) if !network.enterprise => (
                    item.detail(format!("Ask {}", phone.name)),
                    Target::AskPhone(phone.id, network.ssid.clone()),
                ),
                (false, _) => (item, Target::Handoff),
            };
            panel.action(item.row(), target);
        }
    }

    fn panel(&mut self) -> PopupSpec {
        let state = self.network.for_panel();
        let phones = self.phones.for_panel();
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
            let asker = phone::allowing(&phones.devices, Feature::WifiSharing).next();
            self.group(&mut panel, "Known Network", &state.known, asker);
            self.group(&mut panel, "Other Networks", &state.others, asker);

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

        if let Some(joined) = state.connected.as_ref().filter(|joined| joined.secured) {
            share_targets(&mut panel, &phones.devices, &joined.ssid);
        }
        hotspots(&mut panel, &phones.devices);

        panel.row(Row::Separator);
        panel.action(Row::action("Network Settings…"), Target::Settings);

        let (spec, targets) = panel.finish();
        self.targets = targets;
        spec
    }

    fn target(&self, row: usize) -> Option<Target> {
        self.targets.get(row).cloned().flatten()
    }

    /// The switch at `row`. A row that has since become something else — a
    /// phone gone between the panel's build and the click — has none, so it
    /// can never flip this computer's radio instead.
    fn switch_at(&self, row: usize) -> Option<Switch> {
        match self.target(row) {
            _ if row == HEADER_ROW => Some(Switch::Radio),
            Some(Target::Hotspot(id)) => Some(Switch::Hotspot(id)),
            _ => None,
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
        self.phones.replace(services.crownlink.read());
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
        Some(self.panel())
    }

    fn on_popup(&mut self, action: PopupAction, services: &Services) -> AfterAction {
        match action {
            PopupAction::Toggle { row, on } => {
                match self.switch_at(row) {
                    Some(Switch::Radio) => services.network.send(NetworkCommand::SetRadio(on)),
                    Some(Switch::Hotspot(id)) => {
                        services.crownlink.send(PhoneCommand::SetHotspot(id, on))
                    }
                    None => {}
                }
                AfterAction::Stay
            }
            PopupAction::Activate { row } => match self.target(row) {
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
                // The phone asks its user; the network turns up saved here
                // once they agree.
                Some(Target::AskPhone(id, ssid)) => {
                    services
                        .crownlink
                        .send(PhoneCommand::RequestWifi(id, Arc::from(ssid)));
                    AfterAction::Close
                }
                // crownlink confirms with a notification before anything
                // leaves this computer.
                Some(Target::Share(id, ssid)) => {
                    services
                        .crownlink
                        .send(PhoneCommand::ShareWifi(id, Arc::from(ssid)));
                    AfterAction::Close
                }
                Some(Target::Hotspot(_)) | None => AfterAction::Stay,
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
        self.network.panel_stale() || self.phones.panel_stale()
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

/// "Share Wi-Fi Password": every linked device that takes Wi-Fi passwords,
/// each sent the joined network's on a click.
fn share_targets(panel: &mut PanelBuilder<Target>, devices: &[PhoneDevice], ssid: &str) {
    let mut takers = phone::allowing(devices, Feature::WifiSharing).peekable();
    if takers.peek().is_none() {
        return;
    }
    phone::section(panel, SHARE_SECTION);
    for device in takers {
        panel.action(
            phone::item(device, Feature::WifiSharing).detail(ssid).row(),
            Target::Share(device.id, ssid.to_owned()),
        );
    }
}

/// "Personal Hotspot": every linked device that may share its connection,
/// with a switch once it has said whether its hotspot is up. Until then the
/// row only names it.
fn hotspots(panel: &mut PanelBuilder<Target>, devices: &[PhoneDevice]) {
    let mut sharing = phone::allowing(devices, Feature::Hotspot).peekable();
    if sharing.peek().is_none() {
        return;
    }
    phone::section(panel, "Personal Hotspot");
    for device in sharing {
        let item = phone::item(device, Feature::Hotspot);
        match device.hotspot {
            Some(on) => panel.action(item.switch(on).row(), Target::Hotspot(device.id)),
            None => panel.row(item.enabled(false).row()),
        };
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
    use std::sync::Arc;

    use super::*;
    use crate::phone::fixture::*;

    #[test]
    fn link_speeds_read_in_the_largest_whole_unit() {
        assert_eq!(link_speed(100), "100 Mb/s");
        assert_eq!(link_speed(1000), "1 Gb/s");
        assert_eq!(link_speed(2500), "2.5 Gb/s");
    }

    fn wifi_panel(devices: Vec<PhoneDevice>) -> (WifiWidget, PopupSpec) {
        let mut widget = WifiWidget::new();
        widget.network.replace(Arc::new(NetworkState::default()));
        widget.phones.replace(state(devices));
        let spec = widget.panel();
        (widget, spec)
    }

    fn sharing(device: PhoneDevice, on: bool) -> PhoneDevice {
        PhoneDevice {
            hotspot: Some(on),
            ..device
        }
    }

    #[test]
    fn no_phone_means_no_hotspot_heading() {
        let (_, spec) = wifi_panel(Vec::new());
        assert!(!sections(&spec).contains(&"Personal Hotspot"));
    }

    #[test]
    fn each_phone_gets_a_switch_for_its_own_hotspot() {
        for devices in rosters() {
            let devices: Vec<_> = devices
                .into_iter()
                .enumerate()
                .map(|(index, device)| sharing(device, index == 0))
                .collect();
            let (widget, spec) = wifi_panel(devices.clone());
            assert!(!matches!(widget.target(0), Some(Target::Hotspot(_))));
            let rows = section_items(&spec, "Personal Hotspot");
            assert_eq!(rows.len(), devices.len());
            for ((row, item), device) in rows.iter().zip(&devices) {
                assert_eq!(item.label, &*device.name);
                assert_eq!(item.switch, device.hotspot);
                assert!(
                    matches!(widget.target(*row), Some(Target::Hotspot(id)) if id == device.id)
                );
            }
        }
    }

    #[test]
    fn only_the_header_switch_reaches_the_radio() {
        let (widget, spec) = wifi_panel(vec![
            sharing(phone(ONEPLUS, "OnePlus 11 5G"), false),
            phone(PIXEL, "Pixel 8"),
        ]);
        assert_eq!(widget.switch_at(HEADER_ROW), Some(Switch::Radio));
        let rows = section_items(&spec, "Personal Hotspot");
        assert_eq!(widget.switch_at(rows[0].0), Some(Switch::Hotspot(ONEPLUS)));
        assert_eq!(widget.switch_at(rows[1].0), None);
        assert_eq!(widget.switch_at(spec.rows.len()), None);
    }

    #[test]
    fn a_hotspot_not_yet_reported_is_named_but_has_no_switch() {
        let (widget, spec) = wifi_panel(vec![
            phone(ONEPLUS, "OnePlus 11 5G"),
            forbidding(sharing(phone(PIXEL, "Pixel 8"), true), Feature::Hotspot),
        ]);
        let rows = section_items(&spec, "Personal Hotspot");
        assert_eq!(labels(&rows), ["OnePlus 11 5G"]);
        let (row, item) = rows[0];
        assert_eq!(item.switch, None);
        assert!(!item.enabled);
        assert!(widget.target(row).is_none());
    }

    #[test]
    fn the_hotspot_section_stays_while_the_radio_is_off() {
        let mut widget = WifiWidget::new();
        let mut network = NetworkState::default();
        network.radio = Radio::Off;
        widget.network.replace(Arc::new(network));
        widget
            .phones
            .replace(state(vec![sharing(phone(PIXEL, "Pixel 8"), false)]));
        let spec = widget.panel();
        assert_eq!(
            labels(&section_items(&spec, "Personal Hotspot")),
            ["Pixel 8"]
        );
    }
}

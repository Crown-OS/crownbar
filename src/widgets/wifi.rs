//! Wi-Fi widget. State comes from sysfs (`/sys/class/net/<iface>/wireless`
//! exists ⇒ wireless device, `operstate` ⇒ link up/down), signal quality from
//! `/proc/net/wireless` (column 3 = link quality on most drivers), and the
//! radio kill-switch from rfkill.
//!
//! No NetworkManager dependency for any of that — it keeps the bar
//! self-sufficient on any Linux box, including minimal cosmic-comp
//! environments where NM may not be running yet.
//!
//! Listing and joining networks is the one thing sysfs cannot do, so the panel
//! reads [`crate::util::network`] (which drives `nmcli`) on a worker thread.
//! With no `nmcli` installed the panel degrades to the radio switch and the
//! current link, and the icon is unaffected either way.

use std::fs;
use std::path::PathBuf;

use crate::{
    services::Services,
    animation::Spring,
    util::{network, poll::PollGate, rfkill, worker::Job},
    widgets::{
        popup::{Item, PanelBuilder, Row},
        AfterAction, BarWidget, Icon, PopupAction, PopupSpec, Rune, WidgetSlot, WifiState,
    },
};

const POLL_PERIOD_TICKS: u32 = 3;
const RFKILL_KIND: &str = "wlan";
/// One scanning sweep, in seconds.
const SWEEP_SECS: f32 = 1.4;
/// Link-quality change that's worth re-targeting the spring for.
const STRENGTH_EPSILON: f32 = 0.01;
const PANEL_WIDTH: f32 = 288.0;

/// What the radio is doing. Anything short of a carrier reads as searching —
/// that covers scanning, associating and DHCP.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Link {
    /// Radio soft- or hard-blocked in rfkill.
    Off,
    /// Radio up, no link yet.
    Searching,
    /// Associated, carrying `quality` ∈ [0, 1].
    Up(f32),
}

pub struct WifiWidget {
    iface: PathBuf,
    link: Link,
    strength: Spring,
    off: Spring,
    searching: Spring,
    phase: f32,
    gate: PollGate,
    /// Networks, as of the last completed panel read.
    scan: network::Snapshot,
    job: Job<network::Snapshot>,
    targets: Vec<Option<Target>>,
    panel_open: bool,
}

impl WifiWidget {
    pub fn try_new() -> Option<Self> {
        let iface = find_wireless_iface()?;
        let mut w = Self {
            iface,
            link: Link::Off,
            strength: Spring::new(0.0),
            off: Spring::new(0.0),
            searching: Spring::new(0.0),
            phase: 0.0,
            gate: PollGate::new(POLL_PERIOD_TICKS),
            scan: network::Snapshot::default(),
            job: Job::idle(),
            targets: Vec::new(),
            panel_open: false,
        };
        let link = w.read_link();
        w.retarget(link);
        w.strength.snap_to_target();
        w.off.snap_to_target();
        w.searching.snap_to_target();
        Some(w)
    }

    fn read_link(&self) -> Link {
        // A machine with no wlan rfkill entry never reports blocked, so fall
        // through to operstate there rather than claiming the radio is off.
        if rfkill::present(RFKILL_KIND) && !rfkill::unblocked(RFKILL_KIND) {
            return Link::Off;
        }
        let up = read_trimmed(self.iface.join("operstate"))
            .map(|s| s == "up")
            .unwrap_or(false);
        if !up {
            return Link::Searching;
        }
        let name = self.iface.file_name().and_then(|s| s.to_str()).unwrap_or("");
        Link::Up(
            read_link_quality(name)
                .map(|q| q as f32 / 100.0)
                .unwrap_or(0.5),
        )
    }

    fn read(&mut self) {
        self.job.request(network::snapshot);
    }

    fn retarget(&mut self, link: Link) {
        self.link = link;
        let (strength, off, searching) = match link {
            Link::Off => (0.0, 1.0, 0.0),
            Link::Searching => (0.0, 0.0, 1.0),
            Link::Up(q) => (q, 0.0, 0.0),
        };
        self.strength.set_target(strength);
        self.off.set_target(off);
        self.searching.set_target(searching);
    }
}

impl BarWidget for WifiWidget {
    fn id(&self) -> &'static str {
        "wifi"
    }

    fn slot(&self) -> WidgetSlot {
        WidgetSlot::Right
    }

    fn icon(&self) -> Icon {
        Icon::Wifi(WifiState {
            strength: self.strength.position,
            off: self.off.position,
            searching: self.searching.position,
            phase: self.phase,
        })
    }

    fn update(&mut self) -> bool {
        if !self.gate.should_run() {
            return false;
        }
        let next = self.read_link();
        let settled = match (self.link, next) {
            (Link::Up(a), Link::Up(b)) => (a - b).abs() <= STRENGTH_EPSILON,
            (a, b) => a == b,
        };
        if settled {
            return false;
        }
        self.retarget(next);
        true
    }

    fn popup(&mut self, services: &Services) -> Option<PopupSpec> {
        if !self.panel_open {
            self.panel_open = true;
            // Ask for a fresh sweep as the panel opens; the list we draw now
            // is NetworkManager's cache, and the rescan lands a tick later.
            std::thread::spawn(network::request_scan);
            self.read();
        }

        // rfkill knows the radio is blocked even where NetworkManager is not
        // answering, so it wins when the two disagree about "off".
        let radio_on = self.link != Link::Off && self.scan.radio_on.unwrap_or(true);

        let mut panel = PanelBuilder::new(PANEL_WIDTH);
        panel.row(Row::Header {
            title: "Wi-Fi".into(),
            toggle: Some(radio_on),
        });

        if radio_on {
            if let Some(active) = self.scan.active() {
                panel.action(
                    Item::new(&active.ssid)
                        .icon(Icon::Rune(Rune::Wifi))
                        .selected(true)
                        // macOS flags the joined network when it is open;
                        // there is nothing to do about it from here, but it is
                        // worth saying.
                        .warning(!active.secure)
                        .row(),
                    Target::Network(active.ssid.clone()),
                );
            }

            self.push_group(
                &mut panel,
                "Known Networks",
                |n| n.known && !n.active,
                usize::MAX,
            );
            self.push_group(
                &mut panel,
                "Other Networks",
                |n| !n.known && !n.active,
                network::MAX_OTHER,
            );

            if self.scan.networks.is_empty() {
                let message = if network::available() {
                    "Looking for Networks…"
                } else {
                    "Network Service Unavailable"
                };
                panel.row(Item::new(message).plain().enabled(false).row());
            }
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
                // Move the icon at once: `Searching` is what a radio coming
                // back up actually does, and the next read resolves it.
                self.retarget(if on { Link::Searching } else { Link::Off });
                std::thread::spawn(move || network::set_radio(on));
                if !on {
                    self.scan.networks.clear();
                }
                AfterAction::Stay
            }
            PopupAction::Activate { row } => {
                match self.targets.get(row).cloned().flatten() {
                    Some(Target::Network(ssid)) => {
                        self.retarget(Link::Searching);
                        // Joining blocks on DHCP, so it never runs on the
                        // event loop.
                        std::thread::spawn(move || {
                            if !network::connect(&ssid) {
                                log::info!("could not join {ssid} unattended");
                            }
                        });
                        AfterAction::Close
                    }
                    Some(Target::Settings) => {
                        if !network::open_settings() {
                            log::info!("no network settings application installed");
                        }
                        AfterAction::Close
                    }
                    None => AfterAction::Stay,
                }
            }
            PopupAction::Slide { .. } => AfterAction::Stay,
        }
    }

    fn popup_poll(&mut self, slow: bool) -> bool {
        let mut changed = false;
        if let Some(scan) = self.job.take() {
            changed = scan != self.scan;
            self.scan = scan;
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
        let mut busy = false;
        for spring in [&mut self.strength, &mut self.off, &mut self.searching] {
            if spring.at_rest() {
                continue;
            }
            spring.step(dt);
            busy |= !spring.at_rest();
        }
        // The sweep is a loop, not a transition: keep asking for frames for as
        // long as any of it is still visible.
        if self.searching.target > 0.0 || self.searching.position > 0.0 {
            self.phase = (self.phase + dt / SWEEP_SECS).fract();
            busy = true;
        }
        busy
    }
}

#[derive(Clone)]
enum Target {
    Network(String),
    Settings,
}

impl WifiWidget {
    /// Append one titled group of networks, skipping the heading when the
    /// group turns out to be empty.
    fn push_group(
        &self,
        panel: &mut PanelBuilder<Target>,
        title: &str,
        keep: impl Fn(&network::Network) -> bool,
        limit: usize,
    ) {
        let mut wrote_heading = false;
        for net in self.scan.networks.iter().filter(|n| keep(n)).take(limit) {
            if !wrote_heading {
                panel.row(Row::Section {
                    title: title.to_string(),
                    chevron: false,
                });
                wrote_heading = true;
            }
            panel.action(
                Item::new(&net.ssid)
                    .icon(Icon::Rune(Rune::Wifi))
                    // An open network is worth flagging even before joining.
                    .warning(!net.secure)
                    .row(),
                Target::Network(net.ssid.clone()),
            );
        }
    }
}

/// Pick the first interface under /sys/class/net that has a `wireless`
/// subdirectory (the kernel convention for cfg80211-backed devices).
fn find_wireless_iface() -> Option<PathBuf> {
    let entries = fs::read_dir("/sys/class/net").ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.join("wireless").is_dir() {
            return Some(path);
        }
    }
    None
}

fn read_trimmed(path: PathBuf) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

/// Parse `/proc/net/wireless` and return the link-quality percentage for the
/// given interface. The file's "link" column is on a driver-dependent scale
/// (commonly 0-70 or 0-100); we normalize to 0-100 with a 70 cap that matches
/// what most cfg80211 drivers report.
fn read_link_quality(iface: &str) -> Option<u8> {
    let contents = fs::read_to_string("/proc/net/wireless").ok()?;
    for line in contents.lines() {
        let line = line.trim_start();
        let Some((name, rest)) = line.split_once(':') else {
            continue;
        };
        if name != iface {
            continue;
        }
        let mut cols = rest.split_whitespace();
        let _status = cols.next()?;
        let link_raw = cols.next()?;
        let link: f32 = link_raw.trim_end_matches('.').parse().ok()?;
        let pct = ((link / 70.0) * 100.0).clamp(0.0, 100.0);
        return Some(pct.round() as u8);
    }
    None
}

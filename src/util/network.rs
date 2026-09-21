//! Wi-Fi networks, read through `nmcli`.
//!
//! Everything the Wi-Fi *icon* shows still comes from sysfs and rfkill — the
//! bar does not need NetworkManager to draw a signal strength. Listing and
//! joining networks does, and there is no way around that, so the panel
//! degrades instead: with no `nmcli` on `PATH` it shows the radio toggle and
//! the current link, and nothing else.

use crate::util::cmd;

const NMCLI: &str = "nmcli";
/// How many unknown networks the panel is willing to list.
pub const MAX_OTHER: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Network {
    pub ssid: String,
    /// Signal strength in percent, as NetworkManager reports it.
    pub signal: u8,
    pub secure: bool,
    /// Currently associated.
    pub active: bool,
    /// A saved connection profile exists for this SSID.
    pub known: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    /// `None` when NetworkManager could not be reached.
    pub radio_on: Option<bool>,
    /// Strongest-first, deduplicated by SSID.
    pub networks: Vec<Network>,
}

impl Snapshot {
    pub fn active(&self) -> Option<&Network> {
        self.networks.iter().find(|n| n.active)
    }
}

pub fn available() -> bool {
    cmd::exists(NMCLI)
}

/// One read of everything the panel shows.
pub fn snapshot() -> Snapshot {
    if !available() {
        return Snapshot::default();
    }
    let radio_on = cmd::output(NMCLI, &["radio", "wifi"]).map(|out| out.trim() == "enabled");
    let known = known_ssids();
    let networks = cmd::output(
        NMCLI,
        &["-t", "-f", "ACTIVE,SSID,SIGNAL,SECURITY", "dev", "wifi", "list"],
    )
    .map(|out| parse_wifi_list(&out, &known))
    .unwrap_or_default();
    Snapshot {
        radio_on,
        networks,
    }
}

/// Ask NetworkManager to rescan before the next list. Best-effort: it fails
/// harmlessly when a scan is already running or the radio is down.
pub fn request_scan() {
    let _ = cmd::run(NMCLI, &["dev", "wifi", "rescan"]);
}

pub fn set_radio(on: bool) -> bool {
    cmd::run(NMCLI, &["radio", "wifi", if on { "on" } else { "off" }])
}

/// Join `ssid`. Works unattended for open networks and for ones with a saved
/// profile; a new secured network needs a password we have nowhere to ask
/// for, and NetworkManager reports that as a failure.
pub fn connect(ssid: &str) -> bool {
    cmd::run(NMCLI, &["dev", "wifi", "connect", ssid])
}

pub fn open_settings() -> bool {
    cmd::launch_first(&[
        ("crownos-settings", &["network"]),
        ("nm-connection-editor", &[]),
        ("iwgtk", &[]),
    ])
}

/// Saved connection profiles of type `wifi`, by SSID. The profile's *name* is
/// the SSID for anything NetworkManager created itself, which covers every
/// network the user has joined from a desktop.
fn known_ssids() -> Vec<String> {
    cmd::output(NMCLI, &["-t", "-f", "NAME,TYPE", "connection", "show"])
        .map(|out| {
            out.lines()
                .filter_map(|line| {
                    let fields = cmd::split_escaped(line, ':');
                    let name = fields.first()?;
                    let kind = fields.get(1)?;
                    (kind.contains("wireless") || kind == "wifi").then(|| name.clone())
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_wifi_list(out: &str, known: &[String]) -> Vec<Network> {
    let mut networks: Vec<Network> = Vec::new();
    for line in out.lines() {
        let fields = cmd::split_escaped(line, ':');
        if fields.len() < 4 {
            continue;
        }
        let ssid = fields[1].trim().to_string();
        if ssid.is_empty() {
            continue;
        }
        let signal = fields[2].trim().parse::<u8>().unwrap_or(0);
        let security = fields[3].trim();
        let network = Network {
            active: fields[0].trim() == "yes",
            known: known.iter().any(|k| k == &ssid),
            // nmcli prints an empty security field, or `--`, for open networks.
            secure: !security.is_empty() && security != "--",
            signal,
            ssid,
        };
        // The same SSID shows up once per band and per AP; keep the strongest,
        // and let an active or known entry win over a stronger stranger.
        match networks.iter_mut().find(|n| n.ssid == network.ssid) {
            Some(existing) => {
                existing.active |= network.active;
                existing.known |= network.known;
                existing.signal = existing.signal.max(network.signal);
            }
            None => networks.push(network),
        }
    }
    networks.sort_by(|a, b| {
        b.active
            .cmp(&a.active)
            .then_with(|| b.signal.cmp(&a.signal))
    });
    networks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wifi_list_dedupes_by_ssid_and_sorts_by_signal() {
        let out = "\
yes:UNIWORLD-22:71:WPA2
no:UNIWORLD-22:54:WPA2
no:Cafe Guest:88:
no:Neighbour:40:WPA3
";
        let networks = parse_wifi_list(out, &["UNIWORLD-22".to_string()]);
        assert_eq!(networks.len(), 3);
        assert_eq!(networks[0].ssid, "UNIWORLD-22");
        assert!(networks[0].active && networks[0].known && networks[0].secure);
        assert_eq!(networks[0].signal, 71);
        assert_eq!(networks[1].ssid, "Cafe Guest");
        assert!(!networks[1].secure, "an empty security field is an open network");
    }

    #[test]
    fn escaped_colons_stay_inside_the_ssid() {
        let out = r"no:Guest\:Wi-Fi:60:WPA2";
        let networks = parse_wifi_list(out, &[]);
        assert_eq!(networks[0].ssid, "Guest:Wi-Fi");
        assert_eq!(networks[0].signal, 60);
    }
}

//! Handing a subject off to the settings app.
//!
//! One function instead of the five near-identical `open_settings()` copies
//! the `util` modules each carried. The deep link comes first so that
//! crownsettings can focus an already-running window rather than starting a
//! second one; the direct invocation and the third-party fallbacks are what
//! answer until it ships.

use crate::util::cmd;

/// Where in the settings app to land.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsPane {
    Sound,
    SoundInput,
    Bluetooth,
    Network,
    Display,
    Battery,
}

impl SettingsPane {
    /// The pane's name, which is both the deep-link path and the argument
    /// crownsettings takes.
    fn slug(self) -> &'static str {
        match self {
            Self::Sound => "sound",
            Self::SoundInput => "sound/input",
            Self::Bluetooth => "bluetooth",
            Self::Network => "network",
            Self::Display => "display",
            Self::Battery => "battery",
        }
    }

    /// What to try when crownsettings is not installed. Ordered by how close
    /// each one gets to the pane that was asked for.
    fn fallbacks(self) -> &'static [(&'static str, &'static [&'static str])] {
        match self {
            Self::Sound | Self::SoundInput => {
                &[("pavucontrol", &[]), ("pavucontrol-qt", &[]), ("helvum", &[])]
            }
            Self::Bluetooth => &[("blueman-manager", &[]), ("overskride", &[])],
            Self::Network => &[("nm-connection-editor", &[]), ("iwgtk", &[])],
            Self::Display => &[("wdisplays", &[])],
            Self::Battery => &[],
        }
    }
}

const LINKER: &str = "hyprlink";
const SETTINGS: &str = "crownos-settings";

/// Open `pane`, returning whether anything started.
pub fn open(pane: SettingsPane) -> bool {
    let uri = format!("crown://settings/{}", pane.slug());
    if cmd::exists(LINKER) && cmd::spawn_detached(LINKER, &[&uri]) {
        return true;
    }
    if cmd::spawn_detached(SETTINGS, &[pane.slug()]) {
        return true;
    }
    cmd::launch_first(pane.fallbacks())
}

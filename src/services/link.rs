//! Handing a subject off to the settings app.
//!
//! One function instead of the five near-identical `open_settings()` copies
//! the `util` modules each carried. The deep link comes first so that
//! crownsettings can focus an already-running window rather than starting a
//! second one; the direct invocation and the third-party fallbacks are what
//! answer until it ships.

use std::process::Child;

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
    DateTime,
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
            Self::DateTime => "date-time",
        }
    }

    /// What to try when crownsettings is not installed. Ordered by how close
    /// each one gets to the pane that was asked for.
    fn fallbacks(self) -> &'static [(&'static str, &'static [&'static str])] {
        match self {
            Self::Sound | Self::SoundInput => &[
                ("pavucontrol", &[]),
                ("pavucontrol-qt", &[]),
                ("helvum", &[]),
            ],
            Self::Bluetooth => &[("blueman-manager", &[]), ("overskride", &[])],
            Self::Network => &[("nm-connection-editor", &[]), ("iwgtk", &[])],
            Self::Display => &[("wdisplays", &[])],
            Self::Battery => &[],
            Self::DateTime => &[("gnome-control-center", &["datetime"])],
        }
    }
}

const LINKER: &str = "hyprlink";
const SETTINGS: &str = "crownos-settings";

/// Open `pane` from a thread of its own, which looks the application up,
/// starts it and reaps it once it exits: the `PATH` walk and the fork stay off
/// the frame, and no settings window is left behind as a zombie.
pub fn open(pane: SettingsPane) {
    let launcher = std::thread::Builder::new()
        .name("crownbar-launch".into())
        .spawn(move || match launch(pane) {
            Some(mut child) => {
                let _ = child.wait();
            }
            None => log::info!("no {pane:?} settings application installed"),
        });
    if let Err(error) = launcher {
        log::warn!("could not start the launcher thread: {error}");
    }
}

fn launch(pane: SettingsPane) -> Option<Child> {
    let uri = format!("crown://settings/{}", pane.slug());
    cmd::exists(LINKER)
        .then(|| cmd::spawn(LINKER, &[&uri]))
        .flatten()
        .or_else(|| cmd::spawn(SETTINGS, &[pane.slug()]))
        .or_else(|| cmd::launch_first(pane.fallbacks()))
}

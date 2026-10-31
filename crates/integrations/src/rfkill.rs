//! The kernel's radio switches, straight from sysfs.
//!
//! `/sys/class/rfkill/*` carries one entry per radio with a `type`
//! (bluetooth/wlan/wwan/…) and two independent booleans: `soft`, which
//! userspace can clear, and `hard`, which only a physical switch or the
//! firmware can. No daemon is involved, so this answers before BlueZ or
//! NetworkManager is up and keeps answering if either dies — which is the
//! whole reason it survives the move off subprocesses.

use std::{fs, path::Path};

pub const BLUETOOTH: &str = "bluetooth";
pub const WLAN: &str = "wlan";

/// What the kernel says about a class of radio.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Block {
    /// No radio of this kind exists.
    #[default]
    Absent,
    /// A physical switch or the firmware is holding it down. Userspace cannot
    /// clear this, so a toggle must be drawn disabled rather than failing
    /// silently when it is pressed.
    Hard,
    /// Off, but userspace can turn it back on.
    Soft,
    Unblocked,
}

impl Block {
    pub fn present(self) -> bool {
        !matches!(self, Self::Absent)
    }

    pub fn on(self) -> bool {
        matches!(self, Self::Unblocked)
    }
}

/// The state of a class of radio, taking the least-blocked entry: a machine
/// with two Bluetooth radios is usable while either one is.
pub fn block(kind: &str) -> Block {
    let mut best = Block::Absent;
    let Ok(entries) = fs::read_dir("/sys/class/rfkill") else {
        return best;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if read_trim(&path.join("type")).as_deref() != Some(kind) {
            continue;
        }
        let state = match (flag(&path, "hard"), flag(&path, "soft")) {
            (Some(true), _) => Block::Hard,
            (_, Some(true)) => Block::Soft,
            (Some(false), Some(false)) => Block::Unblocked,
            // Pre-2.6.31 kernels expose only the combined `state` file.
            _ => match read_trim(&path.join("state")).as_deref() {
                Some("1") => Block::Unblocked,
                Some("2") => Block::Hard,
                _ => Block::Soft,
            },
        };
        best = best.max(state);
    }
    best
}

fn flag(path: &Path, name: &str) -> Option<bool> {
    Some(read_trim(&path.join(name))? != "0")
}

fn read_trim(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_owned())
}

impl PartialOrd for Block {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Ordered by how usable the radio is, so `max` picks the best of several.
impl Ord for Block {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        fn rank(block: Block) -> u8 {
            match block {
                Block::Absent => 0,
                Block::Hard => 1,
                Block::Soft => 2,
                Block::Unblocked => 3,
            }
        }
        rank(*self).cmp(&rank(*other))
    }
}

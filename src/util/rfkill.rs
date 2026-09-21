//! rfkill sysfs reader. `/sys/class/rfkill/*` carries one entry per radio with
//! `type` (bluetooth/wlan/wwan/…) and `state` (0 soft-blocked, 1 unblocked,
//! 2 hard-blocked). No BlueZ / NetworkManager / D-Bus dependency, so this
//! works pre-login, before any desktop daemon is up.

use std::fs;
use std::path::Path;

/// Does the machine have a radio of this type at all?
pub fn present(kind: &str) -> bool {
    entries().any(|(ty, _)| ty == kind)
}

/// Is any radio of this type unblocked?
pub fn unblocked(kind: &str) -> bool {
    entries().any(|(ty, state)| ty == kind && state == 1)
}

fn entries() -> impl Iterator<Item = (String, u32)> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/class/rfkill") else {
        return out.into_iter();
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(ty) = read_trim(&path.join("type")) else {
            continue;
        };
        let Some(state) = read_trim(&path.join("state")).and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        out.push((ty, state));
    }
    out.into_iter()
}

fn read_trim(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

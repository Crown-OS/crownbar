//! The internal panel.
//!
//! Reading is world-readable sysfs, so a snapshot never has a permission
//! problem. Writing that file does not: it is root-owned, and the unprivileged
//! path is logind's `SetBrightness`, which systemd grants to the active
//! session with no udev rule and no polkit prompt.

use std::fs;

use zbus::{proxy, Connection};

use crate::services::brightness::display::{Display, DisplayId, Transport};

const SYSFS: &str = "/sys/class/backlight";
const SUBSYSTEM: &str = "backlight";

#[proxy(
    interface = "org.freedesktop.login1.Session",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1/session/auto",
    gen_blocking = false
)]
pub trait Session {
    fn set_brightness(&self, subsystem: &str, name: &str, brightness: u32) -> zbus::Result<()>;
}

/// One panel, or none on a desktop. A machine with two backlights is a laptop
/// with a quirky driver, not two screens.
pub fn find() -> Option<(String, Display)> {
    let entry = fs::read_dir(SYSFS).ok()?.flatten().next()?;
    let device = entry.file_name().to_str()?.to_owned();
    let max = read(&entry.path(), "max_brightness")?;
    let raw = read(&entry.path(), "brightness").unwrap_or(0);
    Some((
        device.clone(),
        Display {
            id: DisplayId::backlight(&device),
            label: "Built-in Display".into(),
            transport: Transport::Backlight,
            level: Transport::Backlight.from_raw(raw, max),
            max,
        },
    ))
}

pub async fn set(bus: &Connection, device: &str, raw: u32) -> zbus::Result<()> {
    SessionProxy::new(bus)
        .await?
        .set_brightness(SUBSYSTEM, device, raw)
        .await
}

fn read(path: &std::path::Path, name: &str) -> Option<u32> {
    fs::read_to_string(path.join(name)).ok()?.trim().parse().ok()
}

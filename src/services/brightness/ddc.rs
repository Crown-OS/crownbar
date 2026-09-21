//! External monitors, over DDC/CI.
//!
//! Blocking and slow — a VCP write is tens of milliseconds and enumeration can
//! take a second — so everything here runs on the runtime's blocking pool, one
//! thread for all monitors. Parallel writes on a shared i2c bus are slower,
//! not faster.

use ddc_hi::{Ddc, Display as DdcDisplay};

use crate::services::brightness::display::{Display, DisplayId, Transport};

/// VCP feature code for luminance, the one every MCCS display implements.
const LUMINANCE: u8 = 0x10;

pub struct Monitor {
    pub display: Display,
    handle: DdcDisplay,
}

/// Every monitor that answers DDC. Called at startup and on output hotplug,
/// never on a tick.
pub fn enumerate() -> Vec<Monitor> {
    DdcDisplay::enumerate()
        .into_iter()
        .filter_map(|mut handle| {
            // A monitor that will not report luminance cannot be controlled —
            // many cheap panels and most TVs — so it is dropped rather than
            // given a slider that does nothing.
            let raw = handle.handle.get_vcp_feature(LUMINANCE).ok()?;
            let info = &handle.info;
            let id = DisplayId::monitor(
                info.manufacturer_id.as_deref().unwrap_or("???"),
                info.model_name.as_deref().unwrap_or(&info.id),
                info.serial_number.as_deref().unwrap_or_default(),
            );
            let max = u32::from(raw.maximum()).max(1);
            let display = Display {
                label: info
                    .model_name
                    .clone()
                    .unwrap_or_else(|| "External Display".into()),
                level: Transport::Ddc.from_raw(u32::from(raw.value()), max),
                id,
                transport: Transport::Ddc,
                max,
            };
            Some(Monitor { display, handle })
        })
        .collect()
}

impl Monitor {
    pub fn set(&mut self, raw: u32) -> Result<(), String> {
        self.handle
            .handle
            .set_vcp_feature(LUMINANCE, raw.min(self.display.max) as u16)
            .map_err(|e| e.to_string())
    }

    /// Re-read from the monitor, to catch a change made with its own buttons.
    pub fn refresh(&mut self) -> Option<f32> {
        let raw = self.handle.handle.get_vcp_feature(LUMINANCE).ok()?;
        Some(Transport::Ddc.from_raw(u32::from(raw.value()), self.display.max))
    }
}

/// Whether any i2c bus can be opened at all.
///
/// Checked by trying rather than by looking at group membership: on a
/// seat-managed system logind puts an ACL on the buses belonging to the active
/// session, so a user who is in no `i2c` group still has access.
pub fn reachable() -> bool {
    let Ok(entries) = std::fs::read_dir("/dev") else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with("i2c-"))
            && std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(entry.path())
                .is_ok()
    })
}

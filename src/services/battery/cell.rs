//! The cell as sysfs has it.

use battery::{Manager, State};

use crate::services::battery::charge::{Charge, ChargeStatus};

/// The first cell the manager lists. A machine with two batteries reports the
/// one the firmware puts first, which is what every other bar does.
///
/// Blocking: `battery::Manager` walks sysfs through udev, so this belongs on
/// the blocking pool.
pub fn read() -> Result<Option<Charge>, battery::Error> {
    let manager = Manager::new()?;
    let Some(cell) = manager.batteries()?.next().transpose()? else {
        return Ok(None);
    };
    let status = match cell.state() {
        State::Charging => ChargeStatus::Charging,
        State::Discharging => ChargeStatus::Discharging,
        State::Full => ChargeStatus::Full,
        State::Empty => ChargeStatus::Empty,
        _ => ChargeStatus::Unknown,
    };
    let remaining = match status {
        ChargeStatus::Charging => cell.time_to_full(),
        _ => cell.time_to_empty(),
    };
    let design = cell.energy_full_design().value;
    Ok(Some(Charge {
        level: cell.state_of_charge().value.clamp(0.0, 1.0),
        status,
        minutes: remaining.map(|t| (t.value / 60.0).round().max(0.0) as u32),
        health: (design > 0.0).then(|| (cell.energy_full().value / design).clamp(0.0, 1.0)),
    }))
}

//! Battery charge.
//!
//! The `battery` crate reads sysfs directly, so there is no daemon to
//! subscribe to and no push source — this is the one service that genuinely
//! polls. It polls on the runtime, not on the event loop, and slowly: a cell
//! that moves a percent every few minutes does not repay a faster cadence.

use std::{sync::Arc, time::Duration};

use battery::{Manager, State};
use tokio::{task, time};

use crate::services::{
    bus::Backend,
    status::{Availability, Interest},
};

const IDLE_PERIOD: Duration = Duration::from_secs(30);
const PANEL_PERIOD: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChargeStatus {
    Charging,
    Discharging,
    Full,
    Empty,
    Unknown,
}

/// One reading of the cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Charge {
    /// ∈ [0, 1].
    pub level: f32,
    pub status: ChargeStatus,
    /// Time to full while charging, to empty otherwise.
    pub minutes: Option<u32>,
    /// Full charge against design capacity, ∈ [0, 1].
    pub health: Option<f32>,
}

impl Charge {
    /// What a machine with no reading shows: a flat cell, so the springs have
    /// somewhere to sit before the first poll lands.
    pub const EMPTY: Self = Self {
        level: 0.0,
        status: ChargeStatus::Unknown,
        minutes: None,
        health: None,
    };

    pub fn percent(self) -> u8 {
        (self.level * 100.0).round().clamp(0.0, 100.0) as u8
    }

    pub fn charging(self) -> bool {
        matches!(self.status, ChargeStatus::Charging)
    }

    pub fn full(self) -> bool {
        matches!(self.status, ChargeStatus::Full)
    }

    /// The line under the reading. Worded here rather than in the widget so
    /// the bar, a future OSD and crownsettings cannot disagree.
    pub fn summary(self) -> String {
        match (self.charging(), self.full(), self.minutes) {
            (_, true, _) => "Charged".into(),
            (true, _, Some(m)) => format!("{} until full", clock(m)),
            (true, _, None) => "Charging".into(),
            (false, _, Some(m)) => format!("{} remaining", clock(m)),
            (false, _, None) => "On Battery".into(),
        }
    }
}

/// Minutes as `h:mm`, or as plain minutes under the hour.
fn clock(minutes: u32) -> String {
    match minutes / 60 {
        0 => format!("{minutes} min"),
        hours => format!("{hours}:{:02}", minutes % 60),
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BatteryState {
    pub availability: Availability,
    /// `None` on a desktop, or before the first reading lands.
    pub charge: Option<Charge>,
}

#[derive(Clone, Copy, Debug)]
pub enum BatteryCommand {
    Interest(Interest),
}

pub type Channel = crate::services::bus::Channel<BatteryState, BatteryCommand>;

/// Poll the cell until the bar exits.
pub async fn run(backend: Backend<BatteryState, BatteryCommand>) {
    let Backend {
        publish,
        mut commands,
    } = backend;

    let mut interest = Interest::Idle;
    loop {
        // `battery::Manager` is not `Send`, so it cannot be held across an
        // await — and it need not be: opening it is the same handful of sysfs
        // reads the reading itself costs.
        let reading = task::spawn_blocking(read).await;
        publish.edit(|state| {
            let (charge, availability) = match &reading {
                Ok(Ok(Some(charge))) => (Some(*charge), Availability::Ready),
                Ok(Ok(None)) => (None, Availability::Unavailable(Arc::from("no battery"))),
                Ok(Err(e)) => (None, Availability::Unavailable(Arc::from(e.to_string()))),
                Err(e) => (None, Availability::Unavailable(Arc::from(e.to_string()))),
            };
            let changed = state.charge != charge || state.availability != availability;
            state.charge = charge;
            state.availability = availability;
            changed
        });

        let period = match interest {
            Interest::Idle => IDLE_PERIOD,
            Interest::Panel => PANEL_PERIOD,
        };
        tokio::select! {
            command = commands.recv() => match command {
                Some(BatteryCommand::Interest(next)) => interest = next,
                // The bar is gone.
                None => return,
            },
            _ = time::sleep(period) => {}
        }
    }
}

/// The first cell the manager lists. A machine with two batteries reports the
/// one the firmware puts first, which is what every other bar does.
fn read() -> Result<Option<Charge>, battery::Error> {
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

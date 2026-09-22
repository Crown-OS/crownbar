//! Battery charge.
//!
//! The cell is read from sysfs, which has no subscription to offer, so the
//! reading itself is a poll — slowly, because a cell moves a percent every few
//! minutes. What must not wait for that poll is the plug: [`upower`] hands the
//! loop a doorbell so charging shows the moment the cable goes in, and the
//! poll stays as the thing that keeps the estimate honest.

mod cell;
mod charge;
mod upower;

use std::{pin::Pin, sync::Arc, time::Duration};

use futures_util::{Stream, StreamExt};
use tokio::{task, time};

use crate::services::{
    bus::Backend,
    status::{Availability, Interest},
};

pub use charge::{Charge, ChargeStatus};

/// With the doorbell wired, the poll only has to keep the estimate fresh.
const IDLE_PERIOD: Duration = Duration::from_secs(60);
/// Open panels get the estimate sooner, but not at the rate it is drawn.
const PANEL_PERIOD: Duration = Duration::from_secs(15);
/// Without UPower there is nothing to ring, and the poll is the only way a
/// plug-in is ever noticed — so it answers within a glance instead.
const DEAF_PERIOD: Duration = Duration::from_secs(5);

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

/// Poll the cell, and re-read it on every ring, until the bar exits.
pub async fn run(backend: Backend<BatteryState, BatteryCommand>) {
    let Backend {
        publish,
        mut commands,
    } = backend;

    let doorbell = upower::display_device().await.inspect_err(|e| {
        log::info!("upower unavailable, battery polls only: {e}");
    });
    let doorbell = doorbell.ok();
    let mut rings: Pin<Box<dyn Stream<Item = ()> + Send>> = match &doorbell {
        Some(device) => Box::pin(upower::rings(device).await),
        None => Box::pin(futures_util::stream::pending()),
    };

    let mut interest = Interest::Idle;
    loop {
        // `battery::Manager` is not `Send`, so it cannot be held across an
        // await — and it need not be: opening it is the same handful of sysfs
        // reads the reading itself costs.
        let reading = task::spawn_blocking(cell::read).await;
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

        let period = match (interest, doorbell.is_some()) {
            (Interest::Panel, _) => PANEL_PERIOD,
            (Interest::Idle, true) => IDLE_PERIOD,
            (Interest::Idle, false) => DEAF_PERIOD,
        };
        tokio::select! {
            command = commands.recv() => match command {
                Some(BatteryCommand::Interest(next)) => interest = next,
                // The bar is gone.
                None => return,
            },
            _ = rings.next() => {}
            _ = time::sleep(period) => {}
        }
    }
}

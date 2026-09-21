//! Screen brightness, across every display the machine can dim.
//!
//! Two very different transports behind one list: the internal panel, where a
//! write is microseconds, and external monitors over DDC/CI, where a write is
//! tens of milliseconds and cannot be parallelised. A slider drag is therefore
//! shown immediately and *coalesced* on the way out — one write in flight per
//! display, newest value wins — so dragging never queues a hundred of them.

mod backlight;
mod ddc;
mod display;

use std::{collections::HashMap, sync::Arc, time::Duration};

pub use display::{Display, DisplayId, Transport};

use tokio::{task, time};
use zbus::Connection;

use crate::services::{
    bus::{Backend, Publisher},
    status::{Availability, ErrorKind, Failure, Interest},
};

/// How often to re-read external monitors while the panel is up, to catch a
/// change made with the monitor's own buttons. Never while it is closed: a DDC
/// read is as slow as a write.
const RECHECK: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BrightnessState {
    pub availability: Availability,
    /// Internal panel first, then external monitors.
    pub displays: Vec<Display>,
    pub failure: Option<Failure>,
}

impl BrightnessState {
    /// What the bar pill shows: the internal panel if there is one, else the
    /// first display. The pill has room for one number.
    pub fn primary(&self) -> Option<&Display> {
        self.displays
            .iter()
            .find(|display| display.transport == Transport::Backlight)
            .or_else(|| self.displays.first())
    }

    pub fn level(&self) -> f32 {
        self.primary().map(|display| display.level).unwrap_or(0.0)
    }

    /// More than one slider means the panel grows a row per display.
    pub fn is_multi(&self) -> bool {
        self.displays.len() > 1
    }
}

#[derive(Clone, Debug)]
pub enum BrightnessCommand {
    /// Perceptual level ∈ [0, 1].
    SetLevel { id: DisplayId, level: f32 },
    /// Every display at once — the brightness keys, and the panel's master
    /// slider when there is more than one screen.
    SetAll(f32),
    /// Re-enumerate. Sent on output hotplug, never on a tick.
    Rescan,
    Interest(Interest),
}

pub type Channel = crate::services::bus::Channel<BrightnessState, BrightnessCommand>;

pub async fn run(backend: Backend<BrightnessState, BrightnessCommand>) {
    let Backend {
        publish,
        mut commands,
    } = backend;

    let bus = Connection::system().await.ok();
    let mut panel = backlight::find();
    let mut monitors: Vec<ddc::Monitor> = Vec::new();
    let mut reachable = true;

    // The internal panel is published before anything probes for monitors.
    // Enumeration walks every i2c bus and takes seconds; waiting on it would
    // leave the pill off the bar for that whole time on a laptop that already
    // knows its own answer — and would queue the user's first drag behind it.
    publish_all(&publish, &panel, &monitors, reachable);
    let mut probe = Some(spawn_probe());

    // One pending target per display: a drag overwrites it rather than
    // queueing, so a five-hundred-sample sweep costs one write per round trip.
    let mut pending: HashMap<DisplayId, f32> = HashMap::new();
    let mut interest = Interest::Idle;

    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(BrightnessCommand::Interest(next)) => interest = next,
                Some(BrightnessCommand::Rescan) => {
                    panel = backlight::find();
                    probe = Some(spawn_probe());
                    publish_all(&publish, &panel, &monitors, reachable);
                }
                Some(BrightnessCommand::SetLevel { id, level }) => {
                    show(&publish, &[(id.clone(), level)]);
                    pending.insert(id, level);
                }
                Some(BrightnessCommand::SetAll(level)) => {
                    let targets: Vec<_> = publish
                        .read()
                        .displays
                        .iter()
                        .map(|display| (display.id.clone(), level))
                        .collect();
                    show(&publish, &targets);
                    pending.extend(targets);
                }
                None => return,
            },
            (found, access) = probed(&mut probe) => {
                probe = None;
                monitors = found;
                reachable = access;
                if panel.is_none() && monitors.is_empty() {
                    publish.edit(|state| {
                        state.availability = Availability::Unavailable(Arc::from(if reachable {
                            "no controllable display"
                        } else {
                            "no backlight, and no access to /dev/i2c-*"
                        }));
                        true
                    });
                    return;
                }
                publish_all(&publish, &panel, &monitors, reachable);
            }
            // A re-read only makes sense while somebody is looking at the
            // number, and only for monitors, which can change behind our back.
            _ = time::sleep(RECHECK), if interest == Interest::Panel && !monitors.is_empty() => {
                monitors = recheck(monitors).await;
                publish_all(&publish, &panel, &monitors, reachable);
            }
        }

        if !pending.is_empty() {
            let writes = std::mem::take(&mut pending);
            monitors = flush(&publish, &bus, &panel, monitors, writes).await;
        }
    }
}

type Probe = task::JoinHandle<(Vec<ddc::Monitor>, bool)>;

/// Look for DDC monitors on the blocking pool. Both halves are blocking: the
/// permission check opens device nodes, and enumeration talks to every bus.
fn spawn_probe() -> Probe {
    task::spawn_blocking(|| {
        let reachable = ddc::reachable();
        let monitors = if reachable {
            ddc::enumerate()
        } else {
            Vec::new()
        };
        (monitors, reachable)
    })
}

/// Resolves once, when the probe finishes; parks forever once it has been
/// taken, so the `select!` arm simply stops being eligible.
async fn probed(probe: &mut Option<Probe>) -> (Vec<ddc::Monitor>, bool) {
    match probe.as_mut() {
        Some(handle) => handle.await.unwrap_or_else(|_| (Vec::new(), false)),
        None => std::future::pending().await,
    }
}

/// Show the new level before writing it. The slider then tracks the pointer at
/// frame rate whatever the transport costs.
fn show(publish: &Publisher<BrightnessState>, targets: &[(DisplayId, f32)]) {
    publish.edit(|state| {
        let mut changed = false;
        for (id, level) in targets {
            if let Some(display) = state.displays.iter_mut().find(|d| d.id == *id) {
                changed |= display.level != *level;
                display.level = *level;
            }
        }
        changed
    });
}

/// Apply the coalesced set-points. Monitors are written in list order on one
/// blocking thread: i2c contention makes parallel writes slower, not faster.
async fn flush(
    publish: &Publisher<BrightnessState>,
    bus: &Option<Connection>,
    panel: &Option<(String, Display)>,
    monitors: Vec<ddc::Monitor>,
    writes: HashMap<DisplayId, f32>,
) -> Vec<ddc::Monitor> {
    if let Some((device, display)) = panel
        && let Some(level) = writes.get(&display.id)
        && let Some(bus) = bus
    {
        let raw = Transport::Backlight.to_raw(*level, display.max);
        if let Err(e) = backlight::set(bus, device, raw).await {
            log::info!("could not set the backlight: {e}");
            report(publish, ErrorKind::NotAuthorized, e.to_string());
        }
    }

    task::spawn_blocking(move || {
        let mut monitors = monitors;
        for monitor in monitors.iter_mut() {
            let Some(level) = writes.get(&monitor.display.id) else {
                continue;
            };
            let raw = Transport::Ddc.to_raw(*level, monitor.display.max);
            if let Err(e) = monitor.set(raw) {
                log::info!("could not set {}: {e}", monitor.display.label);
            }
        }
        monitors
    })
    .await
    .unwrap_or_default()
}

async fn recheck(monitors: Vec<ddc::Monitor>) -> Vec<ddc::Monitor> {
    task::spawn_blocking(move || {
        let mut monitors = monitors;
        for monitor in monitors.iter_mut() {
            if let Some(level) = monitor.refresh() {
                monitor.display.level = level;
            }
        }
        monitors
    })
    .await
    .unwrap_or_default()
}

fn publish_all(
    publish: &Publisher<BrightnessState>,
    panel: &Option<(String, Display)>,
    monitors: &[ddc::Monitor],
    reachable: bool,
) {
    let mut displays: Vec<Display> = panel.iter().map(|(_, display)| display.clone()).collect();
    displays.extend(monitors.iter().map(|monitor| monitor.display.clone()));

    publish.edit(|state| {
        // A machine with a panel but no i2c access is working, just not
        // completely — worth saying once, not worth hiding the widget for.
        let availability = match (monitors.is_empty(), reachable) {
            (true, false) => Availability::Degraded(Arc::from(
                "no access to /dev/i2c-*; external monitors cannot be dimmed",
            )),
            _ => Availability::Ready,
        };
        let changed = state.displays != displays || state.availability != availability;
        state.displays = displays.clone();
        state.availability = availability;
        changed
    });
}

fn report(publish: &Publisher<BrightnessState>, kind: ErrorKind, detail: String) {
    publish.edit(|state| {
        state.failure = Some(Failure::new(kind, detail));
        true
    });
}

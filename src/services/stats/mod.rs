//! What the machine is doing to itself: temperatures, clocks, load, memory.
//!
//! Every reading is a small file under `/proc` or `/sys`, so there is no
//! daemon to talk to and nothing to subscribe to — the service polls. It polls
//! slowly while nobody is looking and quickly while the panel is open, because
//! a temperature nobody is reading is worth no syscalls at all.
//!
//! Where the readings live is worked out once, at startup: hwmon numbering is
//! stable for the life of a boot.

mod hwmon;
mod probe;

use std::{sync::Arc, time::Duration};

use tokio::{task, time};

use crate::services::{
    bus::{Backend, Publisher},
    status::{Availability, Interest},
};

/// Slow enough to be free, often enough that the pill is never obviously
/// wrong. Load is a rate over this interval, so it is also the window the
/// percentage describes.
const IDLE_POLL: Duration = Duration::from_secs(3);
/// While the panel is open, where the numbers are being watched.
const PANEL_POLL: Duration = Duration::from_secs(1);

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StatsState {
    pub availability: Availability,
    /// "AMD Ryzen 7 7840HS", for the panel's heading.
    pub model: Option<Arc<str>>,
    pub cpu: Unit,
    pub gpu: Unit,
    /// Used and total system memory, in bytes.
    pub memory: Option<(u64, u64)>,
    /// Used and total video memory, in bytes.
    pub vram: Option<(u64, u64)>,
    /// Graphics power draw, in watts.
    pub watts: Option<f32>,
}

/// One processor's three readings. Each is absent on a machine — or a driver —
/// that does not publish it, and the panel simply leaves that row out.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Unit {
    pub celsius: Option<f32>,
    pub mhz: Option<f32>,
    /// Utilisation ∈ [0, 1].
    pub load: Option<f32>,
}

impl Unit {
    pub fn is_empty(&self) -> bool {
        self.celsius.is_none() && self.mhz.is_none() && self.load.is_none()
    }
}

#[derive(Clone, Copy, Debug)]
pub enum StatsCommand {
    Interest(Interest),
}

pub type Channel = crate::services::bus::Channel<StatsState, StatsCommand>;

pub async fn run(backend: Backend<StatsState, StatsCommand>) {
    let Backend {
        publish,
        mut commands,
    } = backend;

    let Ok(hardware) = task::spawn_blocking(Hardware::discover).await else {
        return;
    };
    if hardware.is_barren() {
        publish.edit(|state| {
            state.availability =
                Availability::Unavailable(Arc::from("this machine publishes no sensors"));
            true
        });
        return;
    }

    let mut interest = Interest::Idle;
    let mut previous = None;

    loop {
        let sample = match task::spawn_blocking({
            let hardware = hardware.clone();
            move || hardware.sample()
        })
        .await
        {
            Ok(sample) => sample,
            Err(_) => return,
        };
        apply(&publish, &hardware, sample, &mut previous);

        let interval = match interest {
            Interest::Panel => PANEL_POLL,
            Interest::Idle => IDLE_POLL,
        };
        tokio::select! {
            command = commands.recv() => match command {
                Some(StatsCommand::Interest(next)) => interest = next,
                None => return,
            },
            _ = time::sleep(interval) => {}
        }
    }
}

/// Where this machine's readings live. Resolved once.
#[derive(Clone, Debug, Default)]
struct Hardware {
    sensors: hwmon::Sensors,
    node: probe::GpuNode,
    model: Option<String>,
}

/// One pass over all of them.
struct Sample {
    cpu_celsius: Option<f32>,
    cpu_mhz: Option<f32>,
    ticks: Option<probe::CpuTicks>,
    gpu_celsius: Option<f32>,
    gpu_mhz: Option<f32>,
    gpu_load: Option<f32>,
    memory: Option<(u64, u64)>,
    vram: Option<(u64, u64)>,
    watts: Option<f32>,
}

impl Hardware {
    fn discover() -> Self {
        Self {
            sensors: hwmon::discover(),
            node: probe::GpuNode::discover(),
            model: probe::cpu_model(),
        }
    }

    /// Whether there is anything at all worth polling for.
    fn is_barren(&self) -> bool {
        self.sensors.cpu.is_none()
            && self.sensors.gpu.is_none()
            && probe::CpuTicks::read().is_none()
    }

    fn sample(&self) -> Sample {
        Sample {
            cpu_celsius: self.sensors.cpu.as_ref().and_then(hwmon::Sensor::celsius),
            cpu_mhz: probe::cpu_clock_mhz(),
            ticks: probe::CpuTicks::read(),
            gpu_celsius: self.sensors.gpu.as_ref().and_then(hwmon::Sensor::celsius),
            gpu_mhz: probe::gpu_clock_mhz(&self.sensors),
            gpu_load: self.node.load(),
            memory: probe::memory(),
            vram: self.node.vram(),
            watts: probe::gpu_watts(&self.sensors),
        }
    }
}

fn apply(
    publish: &Publisher<StatsState>,
    hardware: &Hardware,
    sample: Sample,
    previous: &mut Option<probe::CpuTicks>,
) {
    // Load is a rate, so it needs two samples. The first tick after start
    // reports everything else and leaves it absent.
    let load = match (sample.ticks, *previous) {
        (Some(now), Some(before)) => now.since(before),
        _ => None,
    };
    if let Some(ticks) = sample.ticks {
        *previous = Some(ticks);
    }
    let model: Option<Arc<str>> = hardware.model.as_deref().map(Arc::from);

    publish.edit(|state| {
        let next = StatsState {
            availability: Availability::Ready,
            model,
            cpu: Unit {
                celsius: sample.cpu_celsius,
                mhz: sample.cpu_mhz,
                // Load barely moves between two ticks of an idle machine, and
                // a pill that rewrote itself for a tenth of a percent would
                // repaint the bar three times a second for nothing.
                load: load.or(state.cpu.load).map(round_load),
            },
            gpu: Unit {
                celsius: sample.gpu_celsius,
                mhz: sample.gpu_mhz,
                load: sample.gpu_load.map(round_load),
            },
            memory: sample.memory,
            vram: sample.vram,
            watts: sample.watts,
        };
        let changed = *state != next;
        *state = next;
        changed
    });
}

/// To the whole percent the panel prints, so a reading that only moved in the
/// noise does not count as a change.
fn round_load(load: f32) -> f32 {
    (load * 100.0).round() / 100.0
}

//! Finding the two temperatures worth showing, among the dozen a laptop has.
//!
//! `/sys/class/hwmon` is a flat list of whatever drivers registered: batteries,
//! USB-C controllers, the Wi-Fi radio, both NVMe drives, the EC. Picking the
//! CPU and the GPU out of it is a matter of knowing which *drivers* own them,
//! and then which of that driver's several sensors is the package rather than
//! an individual rail.
//!
//! The search runs once, at startup: hwmon numbering is stable for the life of
//! a boot, so re-walking it every tick would be pure syscalls.

use std::path::{Path, PathBuf};

/// Drivers that report a package temperature for the processor, by the `name`
/// they publish. AMD first because this is what CrownOS runs on most.
const CPU_CHIPS: [&str; 5] = ["k10temp", "zenpower", "coretemp", "cpu_thermal", "acpitz"];
/// Drivers that report one for the graphics processor.
const GPU_CHIPS: [&str; 5] = ["amdgpu", "nvidia", "i915", "xe", "radeon"];

/// Sensor labels that mean "the whole package", best first. A chip with none
/// of these falls back to its first sensor, which is what a single-sensor
/// driver publishes anyway.
const PACKAGE_LABELS: [&str; 7] = [
    "Tctl",
    "Tdie",
    "Package id 0",
    "edge",
    "junction",
    "GPU Core",
    "Composite",
];

/// One `tempN_input` file, already chosen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sensor(PathBuf);

impl Sensor {
    /// Millidegrees to degrees. `None` once the driver has gone — an eGPU
    /// unplugged, a module removed.
    pub fn celsius(&self) -> Option<f32> {
        read_number(&self.0).map(|milli| milli as f32 / 1000.0)
    }
}

/// Where the processor and the graphics temperatures live on this machine.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sensors {
    pub cpu: Option<Sensor>,
    pub gpu: Option<Sensor>,
    /// `freq1_input` of the graphics chip, in hertz.
    pub gpu_clock: Option<PathBuf>,
    /// `power1_average` of the graphics chip, in microwatts.
    pub gpu_power: Option<PathBuf>,
}

pub fn discover() -> Sensors {
    let mut sensors = Sensors::default();
    let Ok(entries) = std::fs::read_dir("/sys/class/hwmon") else {
        return sensors;
    };

    // Ranked rather than first-wins: `acpitz` is a legitimate last resort for
    // a CPU temperature and a poor answer when `k10temp` is also present.
    let mut best_cpu = usize::MAX;
    let mut best_gpu = usize::MAX;
    for entry in entries.flatten() {
        let chip = entry.path();
        let Some(name) = read_string(&chip.join("name")) else {
            continue;
        };
        if let Some(rank) = CPU_CHIPS.iter().position(|c| *c == name)
            && rank < best_cpu
            && let Some(sensor) = package_sensor(&chip)
        {
            best_cpu = rank;
            sensors.cpu = Some(sensor);
        }
        if let Some(rank) = GPU_CHIPS.iter().position(|c| *c == name)
            && rank < best_gpu
            && let Some(sensor) = package_sensor(&chip)
        {
            best_gpu = rank;
            sensors.gpu = Some(sensor);
            sensors.gpu_clock = exists(chip.join("freq1_input"));
            sensors.gpu_power = exists(chip.join("power1_average"))
                .or_else(|| exists(chip.join("power1_input")));
        }
    }
    sensors
}

/// The package sensor of one chip, by label, else its first.
fn package_sensor(chip: &Path) -> Option<Sensor> {
    let mut inputs: Vec<PathBuf> = std::fs::read_dir(chip)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("temp") && name.ends_with("_input"))
        })
        .collect();
    // `temp10_input` sorts before `temp2_input` as a string, and the fallback
    // wants the lowest-numbered sensor.
    inputs.sort();

    for wanted in PACKAGE_LABELS {
        for input in &inputs {
            let label = PathBuf::from(input.to_string_lossy().replace("_input", "_label"));
            if read_string(&label).as_deref() == Some(wanted) {
                return Some(Sensor(input.clone()));
            }
        }
    }
    inputs.into_iter().next().map(Sensor)
}

fn exists(path: PathBuf) -> Option<PathBuf> {
    path.is_file().then_some(path)
}

pub fn read_string(path: &Path) -> Option<String> {
    Some(std::fs::read_to_string(path).ok()?.trim().to_string())
}

pub fn read_number(path: &Path) -> Option<u64> {
    read_string(path)?.parse().ok()
}

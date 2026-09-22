//! The readings themselves, all of them small files under `/proc` and `/sys`.
//!
//! Nothing here blocks on anything but the page cache, but it is still a
//! dozen file opens per tick, so it runs on the blocking pool like every
//! other reading the bar takes.

use std::path::{Path, PathBuf};

use crate::services::stats::hwmon::{read_number, read_string, Sensors};

const CPUFREQ: &str = "/sys/devices/system/cpu";
const DRM: &str = "/sys/class/drm";

/// A reading of `/proc/stat`'s aggregate line, for differencing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CpuTicks {
    busy: u64,
    total: u64,
}

impl CpuTicks {
    pub fn read() -> Option<Self> {
        let stat = std::fs::read_to_string("/proc/stat").ok()?;
        let line = stat.lines().next()?.strip_prefix("cpu ")?;
        let fields: Vec<u64> = line
            .split_whitespace()
            .filter_map(|field| field.parse().ok())
            .collect();
        // user nice system idle iowait irq softirq steal …
        let total: u64 = fields.iter().sum();
        let idle: u64 = fields.iter().skip(3).take(2).sum();
        Some(Self {
            busy: total.saturating_sub(idle),
            total,
        })
    }

    /// Load over the interval between two readings ∈ [0, 1].
    ///
    /// Load is a rate, so a single sample cannot express it — the first tick
    /// after start has nothing to difference against and reports nothing.
    pub fn since(self, previous: Self) -> Option<f32> {
        let elapsed = self.total.checked_sub(previous.total)?;
        if elapsed == 0 {
            return None;
        }
        let busy = self.busy.saturating_sub(previous.busy);
        Some((busy as f32 / elapsed as f32).clamp(0.0, 1.0))
    }
}

/// Mean current clock across the cores that are online, in MHz.
///
/// The mean rather than core 0: on a machine that parks cores, core 0 is
/// whichever one the scheduler happened to leave busy and reads far higher
/// than the package is actually running at.
pub fn cpu_clock_mhz() -> Option<f32> {
    let cpus = std::fs::read_dir(CPUFREQ).ok()?;
    let mut total = 0u64;
    let mut count = 0u32;
    for entry in cpus.flatten() {
        let path = entry.path().join("cpufreq/scaling_cur_freq");
        if let Some(khz) = read_number(&path) {
            total += khz;
            count += 1;
        }
    }
    (count > 0).then(|| total as f32 / count as f32 / 1000.0)
}

/// Total and available memory, in bytes.
pub fn memory() -> Option<(u64, u64)> {
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
    let mut total = None;
    let mut available = None;
    for line in meminfo.lines() {
        let (key, value) = line.split_once(':')?;
        let kib: Option<u64> = value.split_whitespace().next().and_then(|n| n.parse().ok());
        match key {
            "MemTotal" => total = kib,
            "MemAvailable" => available = kib,
            _ => continue,
        }
        if total.is_some() && available.is_some() {
            break;
        }
    }
    Some((total? * 1024, available? * 1024))
}

/// The render node's own counters, which hwmon does not carry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GpuNode {
    busy: Option<PathBuf>,
    vram_used: Option<PathBuf>,
    vram_total: Option<PathBuf>,
}

impl GpuNode {
    /// The first card that publishes a utilisation counter. Integrated and
    /// discrete parts both appear here; the one that answers is the one the
    /// driver is willing to talk about.
    pub fn discover() -> Self {
        let Ok(cards) = std::fs::read_dir(DRM) else {
            return Self::default();
        };
        for entry in cards.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            // `card1-DP-1` is a connector, not a card.
            if !name.starts_with("card") || name.contains('-') {
                continue;
            }
            let device = entry.path().join("device");
            let busy = exists(device.join("gpu_busy_percent"));
            if busy.is_none() {
                continue;
            }
            return Self {
                busy,
                vram_used: exists(device.join("mem_info_vram_used")),
                vram_total: exists(device.join("mem_info_vram_total")),
            };
        }
        Self::default()
    }

    pub fn load(&self) -> Option<f32> {
        let percent = read_number(self.busy.as_deref()?)?;
        Some((percent as f32 / 100.0).clamp(0.0, 1.0))
    }

    /// Used and total video memory, in bytes.
    pub fn vram(&self) -> Option<(u64, u64)> {
        Some((
            read_number(self.vram_used.as_deref()?)?,
            read_number(self.vram_total.as_deref()?)?,
        ))
    }
}

/// Graphics clock in MHz, from the chip hwmon found.
pub fn gpu_clock_mhz(sensors: &Sensors) -> Option<f32> {
    let hertz = read_number(sensors.gpu_clock.as_deref()?)?;
    Some(hertz as f32 / 1_000_000.0)
}

/// Graphics power draw in watts.
pub fn gpu_watts(sensors: &Sensors) -> Option<f32> {
    let micro = read_number(sensors.gpu_power.as_deref()?)?;
    Some(micro as f32 / 1_000_000.0)
}

/// What the processor calls itself, for the panel's heading.
pub fn cpu_model() -> Option<String> {
    let cpuinfo = read_string(Path::new("/proc/cpuinfo"))?;
    let raw = cpuinfo
        .lines()
        .find_map(|line| line.strip_prefix("model name")?.split_once(':'))
        .map(|(_, value)| value.trim())?;
    // "AMD Ryzen 7 7840HS w/ Radeon 780M Graphics" is wider than the panel;
    // the marketing suffixes are what goes.
    let trimmed = raw
        .split(" w/ ")
        .next()
        .unwrap_or(raw)
        .replace("(R)", "")
        .replace("(TM)", "")
        .replace(" CPU", "")
        .replace(" Processor", "");
    Some(trimmed.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn exists(path: PathBuf) -> Option<PathBuf> {
    path.is_file().then_some(path)
}

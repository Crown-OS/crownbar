//! The readings themselves, all of them small files under `/proc` and `/sys`.
//!
//! Nothing here blocks on anything but the page cache, but it is still a
//! dozen file opens per tick, so it runs on the blocking pool like every
//! other reading the bar takes. Paths are resolved once and every reading
//! lands in a stack buffer, so a tick allocates nothing.

use std::path::{Path, PathBuf};

use crate::{
    services::stats::hwmon::{Sensors, read_string},
    util::sysfs::{read_head, read_number},
};

const CPUFREQ: &str = "/sys/devices/system/cpu";
const DRM: &str = "/sys/class/drm";
/// Holds the aggregate line `/proc/stat` opens with, whatever the counters.
const STAT_HEAD: usize = 512;
/// Holds `MemTotal`, `MemFree` and `MemAvailable`, the lines `/proc/meminfo`
/// opens with.
const MEMINFO_HEAD: usize = 256;

/// A reading of `/proc/stat`'s aggregate line, for differencing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CpuTicks {
    busy: u64,
    total: u64,
}

impl CpuTicks {
    pub fn read() -> Option<Self> {
        let mut buffer = [0; STAT_HEAD];
        let stat = read_head(Path::new("/proc/stat"), &mut buffer)?;
        let line = stat.lines().next()?.strip_prefix("cpu ")?;
        // user nice system idle iowait irq softirq steal …
        let (total, idle) = line
            .split_whitespace()
            .filter_map(|field| field.parse::<u64>().ok())
            .enumerate()
            .fold((0, 0), |(total, idle), (column, ticks)| {
                let idle = if matches!(column, 3 | 4) {
                    idle + ticks
                } else {
                    idle
                };
                (total + ticks, idle)
            });
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

/// Every core's current-clock file. A core taken offline later simply stops
/// answering and drops out of the mean.
pub fn cpu_clock_files() -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(CPUFREQ) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path().join("cpufreq/scaling_cur_freq"))
        .filter(|path| path.is_file())
        .collect()
}

/// Mean current clock across the cores that are online, in MHz.
///
/// The mean rather than core 0: on a machine that parks cores, core 0 is
/// whichever one the scheduler happened to leave busy and reads far higher
/// than the package is actually running at.
pub fn cpu_clock_mhz(files: &[PathBuf]) -> Option<f32> {
    let (total, count) = files
        .iter()
        .filter_map(|path| read_number(path))
        .fold((0u64, 0u32), |(total, count), khz| (total + khz, count + 1));
    (count > 0).then(|| total as f32 / count as f32 / 1000.0)
}

/// Total and available memory, in bytes.
pub fn memory() -> Option<(u64, u64)> {
    let mut buffer = [0; MEMINFO_HEAD];
    let meminfo = read_head(Path::new("/proc/meminfo"), &mut buffer)?;
    let mut total = None;
    let mut available = None;
    for line in meminfo.lines() {
        let Some((key, value)) = line.split_once(':') else {
            break;
        };
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

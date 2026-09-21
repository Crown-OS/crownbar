//! What a controllable display is, and the curve its slider rides.

/// A display the bar can dim. Stable for a session; a replug re-enumerates.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DisplayId(String);

impl DisplayId {
    /// The internal panel, by its `/sys/class/backlight` device name.
    pub fn backlight(device: &str) -> Self {
        Self(format!("backlight:{device}"))
    }

    /// An external monitor, by the EDID fields that survive a replug and a
    /// change of i2c bus — which `ddc-hi`'s own id does not.
    pub fn monitor(manufacturer: &str, model: &str, serial: &str) -> Self {
        Self(format!("ddc:{manufacturer}-{model}-{serial}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport {
    /// `/sys/class/backlight` for reads, logind for writes. Fast and fine
    /// grained.
    Backlight,
    /// DDC/CI over i2c. Coarse, and a write costs tens of milliseconds.
    Ddc,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Display {
    pub id: DisplayId,
    /// "Built-in Display", or the monitor's EDID name.
    pub label: String,
    pub transport: Transport,
    /// Perceptual level ∈ [0, 1] — see [`to_raw`].
    pub level: f32,
    /// Raw scale for the backlight; DDC is always 0-100.
    pub max: u32,
}

/// Backlight hardware is linear in duty cycle and the eye is not, so a linear
/// slider spends its bottom half doing nothing visible.
const GAMMA: f32 = 2.2;
/// Never let the slider reach zero: on most panels a raw 0 is the backlight
/// off, and a brightness control that can blank the screen is a trap.
const FLOOR: f32 = 0.01;

impl Transport {
    /// Perceptual level to the value the hardware takes.
    ///
    /// DDC needs no curve: its scale is a percentage the monitor has already
    /// made roughly perceptual, and applying gamma on top would fight it.
    pub fn to_raw(self, level: f32, max: u32) -> u32 {
        let level = level.clamp(0.0, 1.0);
        let scaled = match self {
            Self::Backlight => level.powf(GAMMA).max(FLOOR),
            Self::Ddc => level,
        };
        (scaled * max as f32).round().clamp(1.0, max.max(1) as f32) as u32
    }

    pub fn from_raw(self, raw: u32, max: u32) -> f32 {
        if max == 0 {
            return 0.0;
        }
        let fraction = (raw as f32 / max as f32).clamp(0.0, 1.0);
        match self {
            Self::Backlight => fraction.powf(1.0 / GAMMA),
            Self::Ddc => fraction,
        }
    }
}

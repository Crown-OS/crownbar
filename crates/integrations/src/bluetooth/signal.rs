//! How well a device is heard, in the bars a row can draw.

/// Received signal strength, quantized so an RSSI that wanders a few dBm
/// neither republishes the snapshot nor reshuffles the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SignalLevel(u8);

impl SignalLevel {
    pub const MAX: u8 = 4;
    /// Below this a device is heard at all, but not well enough for a bar.
    const FLOOR_DBM: i16 = -90;
    const STEP_DBM: i16 = 10;

    pub fn from_rssi(dbm: i16) -> Self {
        let bars = (dbm - Self::FLOOR_DBM).div_euclid(Self::STEP_DBM) + 1;
        Self(bars.clamp(0, i16::from(Self::MAX)) as u8)
    }

    pub fn bars(self) -> u8 {
        self.0
    }

    /// ∈ [0, 1].
    pub fn fraction(self) -> f32 {
        f32::from(self.0) / f32::from(Self::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rssi_maps_onto_four_bars() {
        let bars = |dbm| SignalLevel::from_rssi(dbm).bars();
        assert_eq!(bars(-40), 4);
        assert_eq!(bars(-60), 4);
        assert_eq!(bars(-61), 3);
        assert_eq!(bars(-75), 2);
        assert_eq!(bars(-90), 1);
        assert_eq!(bars(-91), 0);
        assert_eq!(bars(i16::MIN), 0);
    }

    #[test]
    fn a_wandering_rssi_keeps_its_level() {
        assert_eq!(SignalLevel::from_rssi(-62), SignalLevel::from_rssi(-68));
    }
}

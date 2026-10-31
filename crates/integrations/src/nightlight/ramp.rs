//! The gamma tables a colour temperature turns into.

/// The whole range a night-light control offers, in kelvin. 6500 K is D65 —
/// the point at which the ramp is the identity and the screen is untouched.
pub const NEUTRAL_KELVIN: u16 = 6500;
pub const WARMEST_KELVIN: u16 = 2000;

/// The three ramps the protocol wants, back to back: red, then green, then
/// blue, each `size` little-endian `u16`s.
pub(super) fn ramps(size: u32, kelvin: u16) -> Vec<u8> {
    let white = whitepoint(kelvin);
    let last = (size.max(2) - 1) as f32;
    let mut out = Vec::with_capacity(size as usize * 3 * 2);
    for channel in white {
        for step in 0..size {
            let value = (step as f32 / last * channel).clamp(0.0, 1.0);
            out.extend_from_slice(&((value * u16::MAX as f32) as u16).to_le_bytes());
        }
    }
    out
}

/// A blackbody's colour at `kelvin`, normalised so its largest channel is 1.
///
/// Normalising is what keeps this a *tint* rather than a dimmer: the ramp only
/// ever pulls channels down, so the brightest the screen can be is unchanged
/// and the brightness slider stays the only thing that darkens it. The curve
/// is Neil Bartlett's refinement of Tanner Helland's approximation.
fn whitepoint(kelvin: u16) -> [f32; 3] {
    let t = kelvin.clamp(1000, 40_000) as f32 / 100.0;
    let (red, green, blue) = if t <= 66.0 {
        (
            255.0,
            99.470_8 * t.ln() - 161.119_57,
            if t <= 19.0 {
                0.0
            } else {
                138.517_73 * (t - 10.0).ln() - 305.044_8
            },
        )
    } else {
        (
            329.698_73 * (t - 60.0).powf(-0.133_204_76),
            288.122_16 * (t - 60.0).powf(-0.075_514_85),
            255.0,
        )
    };
    let channels = [red, green, blue].map(|c| c.clamp(0.0, 255.0));
    let peak = channels.iter().copied().fold(f32::MIN, f32::max).max(1.0);
    channels.map(|c| c / peak)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_light_leaves_every_channel_alone() {
        assert!(
            whitepoint(NEUTRAL_KELVIN)
                .iter()
                .all(|&channel| channel > 0.97)
        );
    }

    #[test]
    fn warmer_light_only_ever_pulls_blue_and_green_down() {
        let (neutral, warm) = (whitepoint(NEUTRAL_KELVIN), whitepoint(WARMEST_KELVIN));
        assert!(warm[2] < neutral[2]);
        assert!(warm[1] < neutral[1]);
        assert_eq!(warm[0], 1.0);
    }

    #[test]
    fn a_ramp_is_three_channels_of_little_endian_u16() {
        let table = ramps(256, NEUTRAL_KELVIN);
        assert_eq!(table.len(), 256 * 3 * 2);
        assert_eq!(&table[0..2], &[0, 0]);
        assert!(u16::from_le_bytes([table[510], table[511]]) > 64_000);
    }
}

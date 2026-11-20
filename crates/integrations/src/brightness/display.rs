//! What a display the bar can dim is, and how its level maps to the slider.

/// A display, by the name of its `wl_output`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DisplayId(String);

impl DisplayId {
    pub fn new(output_name: impl Into<String>) -> Self {
        Self(output_name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// How the compositor changes a display's brightness.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    /// The level cannot be changed.
    None,
    Backlight,
    Ddc,
    /// The compositor dims the picture.
    Software,
}

impl Control {
    pub fn adjustable(self) -> bool {
        self != Self::None
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Display {
    pub id: DisplayId,
    /// The output's description, or its name when it has none.
    pub label: String,
    pub control: Control,
    /// Perceptual level ∈ [0, 1].
    pub level: f32,
}

/// The top of the protocol's perceptual scale.
const PROTOCOL_MAXIMUM: u32 = 10_000;

/// The slider position for a level on the protocol's perceptual scale. Both
/// scales are perceptual, so no curve is applied.
pub fn slider_level(protocol_level: u32) -> f32 {
    protocol_level.min(PROTOCOL_MAXIMUM) as f32 / PROTOCOL_MAXIMUM as f32
}

/// The protocol level for a slider position, never above the protocol's
/// maximum.
pub fn protocol_level(slider_level: f32) -> u32 {
    (slider_level.clamp(0.0, 1.0) * PROTOCOL_MAXIMUM as f32).round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_level_survives_a_round_trip_through_the_protocol() {
        for level in [0.0, 0.01, 0.5, 0.683, 1.0] {
            assert!((slider_level(protocol_level(level)) - level).abs() < 1e-4);
        }
    }

    #[test]
    fn a_slider_out_of_range_never_asks_for_an_invalid_level() {
        assert_eq!(protocol_level(1.7), PROTOCOL_MAXIMUM);
        assert_eq!(protocol_level(-0.2), 0);
        assert_eq!(protocol_level(f32::NAN), 0);
        assert_eq!(slider_level(u32::MAX), 1.0);
    }
}

//! WCAG 2 relative luminance and contrast ratio, over sRGB colors.

use crownui::prelude::Color;

/// The ambient light WCAG adds to both sides of a contrast ratio.
const FLARE: f32 = 0.05;

/// An sRGB-encoded channel in linear light.
pub fn to_linear(channel: f32) -> f32 {
    if channel <= 0.040_45 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

/// A linear-light channel, sRGB-encoded.
pub fn to_encoded(linear: f32) -> f32 {
    if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

pub fn relative_luminance(color: Color) -> f32 {
    0.2126 * to_linear(color.r) + 0.7152 * to_linear(color.g) + 0.0722 * to_linear(color.b)
}

/// From 1 (the same luminance) to 21 (black on white).
pub fn contrast_ratio(first: Color, second: Color) -> f32 {
    let (first, second) = (relative_luminance(first), relative_luminance(second));
    (first.max(second) + FLARE) / (first.min(second) + FLARE)
}

/// `top` composited over the opaque `under`, as the renderer blends it.
pub fn over(top: Color, under: Color) -> Color {
    let mix = |top_channel: f32, under_channel: f32| {
        under_channel + (top_channel - under_channel) * top.a
    };
    Color::new(
        mix(top.r, under.r),
        mix(top.g, under.g),
        mix(top.b, under.b),
        1.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLACK: Color = Color::new(0.0, 0.0, 0.0, 1.0);
    const WHITE: Color = Color::new(1.0, 1.0, 1.0, 1.0);

    #[test]
    fn black_on_white_is_the_widest_ratio() {
        assert!((contrast_ratio(BLACK, WHITE) - 21.0).abs() < 1e-4);
        assert!((contrast_ratio(WHITE, BLACK) - 21.0).abs() < 1e-4);
        assert!((contrast_ratio(WHITE, WHITE) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn encoding_round_trips() {
        for value in [0.0, 0.002, 0.04, 0.2, 0.5, 0.9, 1.0] {
            assert!(
                (to_encoded(to_linear(value)) - value).abs() < 1e-5,
                "{value}"
            );
        }
    }

    #[test]
    fn a_translucent_color_lands_between_its_ends() {
        let half = over(Color::new(1.0, 1.0, 1.0, 0.5), BLACK);
        assert_eq!(half, Color::new(0.5, 0.5, 0.5, 1.0));
        assert_eq!(over(WHITE, BLACK), WHITE);
    }
}

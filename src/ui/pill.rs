//! Rounded-pill widget background. Designed as a thin layer over Vello — kept
//! standalone so future widgets that want to draw their own decorations can
//! reuse it without going through the bar's slot machinery.

use vello::{
    kurbo::{Affine, RoundedRect, Stroke},
    peniko::{Color, Fill},
    Scene,
};

/// Draw the pill background for one widget at the given bounds.
///
/// `hover` ∈ [0, 1] fades the fill and rim in from invisible to fully on.
/// The corner radius is always half the pill's height — a fully-rounded
/// capsule shape.
pub fn draw(
    scene: &mut Scene,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    hover: f32,
    fill: Color,
    rim: Color,
) {
    if hover <= 0.0 || w <= 0.0 || h <= 0.0 {
        return;
    }
    let radius = (h as f64) * 0.5;
    let rect = RoundedRect::new(x as f64, y as f64, (x + w) as f64, (y + h) as f64, radius);

    // Fade fill alpha by hover.
    let fc = fill.components;
    let fill_c = Color::new([fc[0], fc[1], fc[2], fc[3] * hover]);
    scene.fill(Fill::NonZero, Affine::IDENTITY, fill_c, None, &rect);
}

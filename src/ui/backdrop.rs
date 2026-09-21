//! How the bar meets the desktop: the fade its background ends in.
//!
//! It is not allowed an edge. The bar asks the compositor for no backdrop
//! blur, so [`fill`] is the whole of what sits behind the widgets — densest
//! along the screen edge, thinning to nothing by the bar's lower one — and
//! nothing ever starts or stops at a line.
//!
//! The ramp is a smoothstep sampled into gradient stops rather than a
//! straight interpolation: a linear ramp leaves a Mach band exactly where it
//! is supposed to have disappeared. A wide, straight, screen-width edge is
//! also what a blurred rect would converge to, at a fraction of the cost —
//! the closed form of the thing, not an approximation of it.

use vello::{
    kurbo::{Affine, Point, Rect},
    peniko::{Color, Fill, Gradient},
    Scene,
};

use crate::util::ease::fade_out;

/// Fraction of the bar the background holds at full strength before it starts
/// to thin, so the widgets keep solid ground under them while the edge itself
/// dissolves.
const FILL_HOLD: f32 = 0.4;
/// Samples per ramp. Vello interpolates linearly between stops, so this is how
/// finely the curve is followed.
const STOPS: usize = 8;

/// The bar's background: `color` at the top, gone by `bar_height`.
pub fn fill(scene: &mut Scene, width: f32, bar_height: f32, color: Color) {
    let [.., alpha] = color.components;
    if alpha <= 0.0 || width <= 0.0 || bar_height <= 0.0 {
        return;
    }

    let bar = Rect::new(0.0, 0.0, width as f64, bar_height as f64);
    let ramp = vertical_ramp(0.0, bar_height as f64, color, |t| {
        alpha * fade_out((t - FILL_HOLD) / (1.0 - FILL_HOLD))
    });
    scene.fill(Fill::NonZero, Affine::IDENTITY, &ramp, None, &bar);
}

/// `color` down the column from `y0` to `y1`, its alpha following `alpha_at`.
fn vertical_ramp(y0: f64, y1: f64, color: Color, alpha_at: impl Fn(f32) -> f32) -> Gradient {
    let stops: [(f32, Color); STOPS] = std::array::from_fn(|i| {
        let t = i as f32 / (STOPS - 1) as f32;
        (t, color.with_alpha(alpha_at(t)))
    });
    Gradient::new_linear(Point::new(0.0, y0), Point::new(0.0, y1)).with_stops(stops)
}

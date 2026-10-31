//! How the bar meets the desktop: the fade its background ends in.
//!
//! It is not allowed an edge. The bar asks the compositor for no backdrop
//! blur, so [`paint`] is the whole of what sits behind the widgets — densest
//! along the screen edge and thinning from there across the bar's whole
//! height — and nothing ever starts or stops at a line.
//!
//! The ramp is a smoothstep sampled into gradient stops rather than a
//! straight interpolation: a linear ramp leaves a Mach band exactly where it
//! is supposed to have disappeared.
//!
//! A floating bar is the exception: it is held off the screen edge, so there
//! is no edge to fade from, and it is drawn as a rounded body of its own.

use crownui::{
    ext::{Brush, DrawList, GradientStop, Paint},
    prelude::{Color, CornerRadii, Point, Rect, Size},
};

use crate::util::ease::fade_out;

/// Peak strength of the background, as a fraction of the body color's own
/// alpha: the bar is a hint of ground under the widgets, not a panel.
const FILL_STRENGTH: f32 = 0.45;
/// Samples per ramp. The renderer interpolates linearly between stops, so this
/// is how finely the curve is followed.
const STOPS: usize = 12;

/// What the bar's background is drawn as.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ground {
    /// Docked to the top edge, fading downward.
    Top,
    /// Docked to the bottom edge, fading upward.
    Bottom,
    /// Clear of every edge, with corners of this radius.
    Floating { radius: f32 },
}

/// The bar's background: `color` along its screen edge, gone by the far side
/// of `size` — or, floating, a body filling `size`.
pub fn paint(list: &mut DrawList, size: Size, color: Color, ground: Ground) {
    if color.a <= 0.0 || size.width <= 0.0 || size.height <= 0.0 {
        return;
    }
    let bounds = Rect::new(Point::new(0.0, 0.0), size);
    let (start, end) = match ground {
        Ground::Top => (0.0, size.height),
        Ground::Bottom => (size.height, 0.0),
        Ground::Floating { radius } => {
            list.fill(
                bounds,
                CornerRadii::uniform(radius.min(size.height / 2.0)),
                Paint::solid(color),
            );
            return;
        }
    };
    let stops = (0..STOPS)
        .map(|index| {
            let offset = index as f32 / (STOPS - 1) as f32;
            GradientStop {
                offset,
                color: Color {
                    a: color.a * FILL_STRENGTH * fade_out(offset),
                    ..color
                },
            }
        })
        .collect();
    let ramp = Brush::LinearGradient {
        start: Point::new(0.0, start),
        end: Point::new(0.0, end),
        stops,
    };
    list.fill(
        bounds,
        CornerRadii::uniform(0.0),
        Paint {
            brush: ramp,
            blur: None,
        },
    );
}

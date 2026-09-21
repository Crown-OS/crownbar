//! Shape and depth primitives shared by the bar's controls.
//!
//! Ported from `crownuikit::util::{shapes, shadow}` rather than called: the
//! kit's helpers return `kurbo`/`peniko` values from xilem's versions of those
//! crates, which are not the ones the bar paints with (see [`crate::theme`]).
//! The geometry is the kit's, so a toggle here and a toggle in crownsettings
//! are the same object; only the types are local.

use vello::{
    kurbo::{Point, Rect, RoundedRect},
    peniko::{Color, Gradient},
};

/// A pill centred on `center`, `inflate` px larger on every side. Inflating is
/// how the glow behind a knob gets a matching pill-shaped extent for free.
pub fn inflated_pill(center: Point, inflate: f64, height: f64, width: f64) -> RoundedRect {
    let half_w = width / 2.0 + inflate;
    let half_h = height / 2.0 + inflate;
    Rect::new(
        center.x - half_w,
        center.y - half_h,
        center.x + half_w,
        center.y + half_h,
    )
    .to_rounded_rect(half_h)
}

/// Radial gradient fading from `color` at `inner_radius` out to transparent at
/// `outer_radius`, with a subtle core at the rim itself.
///
/// Painted over an inflated copy of a shape it gives a symmetric drop shadow
/// that also blurs the rim — at these sizes the smooth alpha ramp hides
/// pixelation along a curve better than a stroke does.
pub fn outer_glow(
    center: Point,
    inner_radius: f32,
    outer_radius: f32,
    color: Color,
    rim_strength: f32,
    halo_strength: f32,
) -> Gradient {
    let edge = (inner_radius / outer_radius).clamp(0.0, 1.0);
    Gradient::new_radial(center, outer_radius).with_stops([
        (0.0_f32, color.with_alpha(0.0)),
        ((edge - 0.02).max(0.0), color.with_alpha(rim_strength)),
        (edge, color.with_alpha(halo_strength)),
        (1.0_f32, color.with_alpha(0.0)),
    ])
}

/// Radial gradient that is transparent through most of a shape and ramps up to
/// `color` at the rim. Painted *inside* the shape it reads as an inner shadow
/// (black) or a rim highlight (white), and smooths the shape's fill into
/// whatever [`outer_glow`] put outside it.
///
/// `start` is the fraction of `radius` where the ring begins — higher is
/// tighter.
pub fn inner_ring(center: Point, radius: f32, color: Color, strength: f32, start: f32) -> Gradient {
    Gradient::new_radial(center, radius).with_stops([
        (0.0_f32, color.with_alpha(0.0)),
        (start, color.with_alpha(0.0)),
        (1.0_f32, color.with_alpha(strength)),
    ])
}

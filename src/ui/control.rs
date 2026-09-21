//! The toggle and the slider, as the rest of CrownOS draws them.
//!
//! Both are ports of the corresponding `crownuikit` widgets — same metrics,
//! same gradient recipes, same spring feel — down to the elastic stretch the
//! kit's toggle knob picks up at speed. They are ports rather than uses
//! because a `crownuikit` widget is a masonry `Widget`, and the bar has no
//! masonry: it hands crownshell a bare [`Scene`]. What crosses over is the
//! part that matters, which is the drawing.
//!
//! Everything here takes its colors from a [`Palette`] resolved this frame, so
//! a toggle on the bar and a toggle in crownsettings are the same color at the
//! same moment, accent and all.

use vello::{
    kurbo::{Affine, Point, Rect, RoundedRect},
    peniko::{color::palette::css, Fill},
    Scene,
};

use crate::theme::{lerp_stops, Palette};

use super::shapes::{inflated_pill, inner_ring, outer_glow};

// --- Toggle -----------------------------------------------------------------
// crownuikit::widgets::toggle

pub const TOGGLE_WIDTH: f64 = 48.0;
pub const TOGGLE_HEIGHT: f64 = 24.0;
/// Total horizontal footprint of the knob, padding on both sides included.
const KNOB_WIDTH: f64 = 32.0;
/// Padding inside the track around every side of the knob.
const KNOB_PADDING: f64 = 2.0;
/// How far past the knob the soft glow extends. Bigger = softer edge.
const KNOB_GLOW_RADIUS: f64 = 4.0;
/// Elastic stretch. Multiplied by |spring velocity| to get a dimensionless
/// stretch fraction, which is then scaled by the knob's width — so at
/// [`KNOB_MAX_STRETCH`] the knob briefly grows 10% wider at peak velocity.
const KNOB_STRETCH_PER_VELOCITY: f64 = 0.15;
const KNOB_MAX_STRETCH: f64 = 0.1;

const KNOB_HEIGHT: f64 = TOGGLE_HEIGHT - 2.0 * KNOB_PADDING;
const BASE_KNOB_WIDTH: f64 = KNOB_WIDTH - 2.0 * KNOB_PADDING;

/// Draw a switch whose track's right edge is at `right`, centred on `cy`.
///
/// `t` ∈ [0, 1] is the animated on-ness — it drives both the knob's position
/// and the track's color — and `velocity` is the spring's, which is what the
/// knob stretches with. Pass `0.0` for a switch that is not animating.
pub fn toggle(scene: &mut Scene, right: f64, cy: f64, t: f32, velocity: f32, p: &Palette) {
    let t = t.clamp(0.0, 1.0);
    let left = right - TOGGLE_WIDTH;
    let center = Point::new(left + TOGGLE_WIDTH / 2.0, cy);

    let track = lerp_stops(p.toggle_off, p.accent, t).vertical(
        center.x,
        cy - TOGGLE_HEIGHT / 2.0,
        cy + TOGGLE_HEIGHT / 2.0,
    );
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &track,
        None,
        &inflated_pill(center, 0.0, TOGGLE_HEIGHT, TOGGLE_WIDTH),
    );

    // The knob slides between the ends of the track, always KNOB_PADDING from
    // the outer rim.
    let knob_x = left + KNOB_WIDTH / 2.0 + t as f64 * (TOGGLE_WIDTH - KNOB_WIDTH);
    let knob_center = Point::new(knob_x, cy);
    let stretch =
        ((velocity as f64).abs() * KNOB_STRETCH_PER_VELOCITY).clamp(0.0, KNOB_MAX_STRETCH);
    let knob_w = BASE_KNOB_WIDTH * (1.0 + stretch);
    let knob_radius = (KNOB_HEIGHT / 2.0) as f32;
    let knob = inflated_pill(knob_center, 0.0, KNOB_HEIGHT, knob_w);

    // Soft glow: an inflated pill behind the knob, filled with a radial ramp
    // anchored at its centre. Doubles as the drop shadow.
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &outer_glow(
            knob_center,
            knob_radius,
            knob_radius + KNOB_GLOW_RADIUS as f32,
            css::BLACK,
            0.14,
            0.08,
        ),
        None,
        &inflated_pill(knob_center, KNOB_GLOW_RADIUS, KNOB_HEIGHT, knob_w),
    );
    scene.fill(Fill::NonZero, Affine::IDENTITY, p.knob, None, &knob);
    // Inner rim shading, so the body does not meet the glow at a hard edge.
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &inner_ring(knob_center, knob_radius, css::WHEAT, 0.08, 0.60),
        None,
        &knob,
    );
}

// --- Slider -----------------------------------------------------------------
// crownuikit::widgets::slider

pub const TRACK_HEIGHT: f64 = 8.0;
pub const THUMB_WIDTH: f64 = 26.0;
pub const THUMB_HEIGHT: f64 = 20.0;
pub const THUMB_HALF_WIDTH: f64 = THUMB_WIDTH / 2.0;
const THUMB_CORNER_RADIUS: f64 = THUMB_HEIGHT / 2.0;
const SHADOW_BLUR_RADIUS: f64 = 0.5;
const INNER_SHADOW_STRENGTH: f32 = 1.1;
/// Alpha of the soft white bloom over the thumb's edge — the last pass, which
/// is what lifts it off the track.
const THUMB_BLOOM_ALPHA: f32 = 150.0 / 255.0;

/// Draw a slider along `span`, a rect whose width is the *thumb-centre* travel
/// plus a half-thumb of margin at each end, and whose height is the row's.
///
/// `value` ∈ [0, 1]. The thumb's centre travels between `span.x0 +
/// THUMB_HALF_WIDTH` and `span.x1 - THUMB_HALF_WIDTH`, which is the geometry
/// [`track_span`] and [`value_at`] agree on.
pub fn slider(scene: &mut Scene, span: Rect, value: f32, p: &Palette) {
    let (x0, x1) = track_span(span);
    let width = (x1 - x0).max(0.0);
    let cy = span.center().y;
    let top = cy - TRACK_HEIGHT / 2.0;
    let radius = TRACK_HEIGHT / 2.0;

    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        p.track,
        None,
        &RoundedRect::new(x0, top, x1, top + TRACK_HEIGHT, radius),
    );

    let filled = value.clamp(0.0, 1.0) as f64 * width;
    if filled > 0.0 {
        // The gradient spans the whole track, not the filled part, so a
        // changing value reveals a consistent slice of it rather than
        // restretching the same two stops.
        let fill = p.accent.horizontal(top, x0, x1);
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            &fill,
            None,
            &RoundedRect::new(x0, top, x0 + filled, top + TRACK_HEIGHT, radius),
        );
    }

    let thumb_center = Point::new(x0 + filled, cy);
    let thumb = Rect::from_center_size(thumb_center, (THUMB_WIDTH, THUMB_HEIGHT))
        .to_rounded_rect(THUMB_CORNER_RADIUS);

    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        p.knob_shadow,
        None,
        &inflated_pill(
            thumb_center,
            SHADOW_BLUR_RADIUS * 3.5,
            THUMB_HEIGHT,
            THUMB_WIDTH,
        ),
    );
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        p.knob.with_alpha(0.85),
        None,
        &thumb,
    );
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &inner_ring(
            thumb_center,
            THUMB_HALF_WIDTH as f32,
            css::WHITE_SMOKE,
            INNER_SHADOW_STRENGTH,
            0.78,
        ),
        None,
        &thumb,
    );
    scene.draw_blurred_rounded_rect(
        Affine::IDENTITY,
        thumb.rect(),
        p.knob.with_alpha(THUMB_BLOOM_ALPHA),
        5.0,
        5.0,
    );
}

/// The x range the thumb's centre travels over, inset from `span` by a half
/// thumb at each end so the thumb never overhangs.
pub fn track_span(span: Rect) -> (f64, f64) {
    let x0 = span.x0 + THUMB_HALF_WIDTH;
    let x1 = (span.x1 - THUMB_HALF_WIDTH).max(x0);
    (x0, x1)
}

/// The value an x in the same space as `span` maps onto.
pub fn value_at(span: Rect, x: f64) -> f32 {
    let (x0, x1) = track_span(span);
    let width = x1 - x0;
    if width <= 0.0 {
        return 0.0;
    }
    (((x - x0) / width) as f32).clamp(0.0, 1.0)
}

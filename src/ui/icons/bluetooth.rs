//! Bluetooth rune. `on` ∈ [0, 1] is the progress of the stroke along the
//! rune's single continuous path: it draws itself on as the adapter comes up
//! and un-draws as it goes down, over a dim full-length track.

use std::sync::OnceLock;

use vello::{
    Scene,
    kurbo::{Rect, Stroke},
    peniko::Color,
};

use super::{
    fade,
    glyph::{self, Glyph},
    lerp,
};

/// Single-stroke rune in a 24 × 24 box: down-right diagonal, lower flag, stem,
/// upper flag, down-left diagonal — one unbroken trace.
const GLYPH_SVG: &str = "M7 7 L17 17 L12 22 L12 2 L17 7 L7 17";

const INK_FILL: f64 = 0.84;
const STROKE_PX: f64 = 1.5;

fn glyph() -> &'static Glyph {
    static CACHE: OnceLock<Glyph> = OnceLock::new();
    CACHE.get_or_init(|| Glyph::parse(GLYPH_SVG))
}

pub(super) fn draw(scene: &mut Scene, b: Rect, fg: Color, on: f32) {
    let glyph = glyph();
    if glyph.is_empty() {
        return;
    }
    let progress = on.clamp(0.0, 1.0);

    let (transform, scale) = glyph::fit(glyph.ink(), b, INK_FILL);
    let stroke = Stroke::new(STROKE_PX / scale);

    scene.stroke(
        &stroke,
        transform,
        fade(fg, lerp(0.18, 0.30, progress)),
        None,
        glyph.path(),
    );

    if progress <= 0.0 {
        return;
    }
    if progress >= 1.0 {
        scene.stroke(&stroke, transform, fg, None, glyph.path());
    } else {
        scene.stroke(&stroke, transform, fg, None, &glyph.traced(progress));
    }
}

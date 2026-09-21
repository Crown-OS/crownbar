//! Caffeine cup. `on` ∈ [0, 1] traces the steam on as the machine is told to
//! stay awake, over a dim full-length track — the cup itself never changes, so
//! the pill reads as the same object in both states.

use std::sync::OnceLock;

use vello::{
    kurbo::{Cap, Join, Rect, Stroke},
    peniko::Color,
    Scene,
};

use super::{
    fade,
    glyph::{self, Glyph},
    lerp,
};

/// The box both paths are authored in, so they stay in register with each
/// other however the icon is scaled.
const VIEW_BOX: Rect = Rect::new(0.0, 0.0, 24.0, 24.0);
const INK_FILL: f64 = 0.86;
const STROKE_PX: f64 = 1.5;

const CUP: &str = "M4 9h12v6a4 4 0 0 1-4 4H8a4 4 0 0 1-4-4V9z M16 11h1.6a2.4 2.4 0 0 1 0 4.8H16 M3 22h14";
/// Three wisps, drawn outwards from the cup so the trace grows upwards.
const STEAM: &str = "M8 6.5V3.5 M12 6.5V2.5 M16 6.5V3.5";

fn cup() -> &'static Glyph {
    static CACHE: OnceLock<Glyph> = OnceLock::new();
    CACHE.get_or_init(|| Glyph::parse(CUP))
}

fn steam() -> &'static Glyph {
    static CACHE: OnceLock<Glyph> = OnceLock::new();
    CACHE.get_or_init(|| Glyph::parse(STEAM))
}

pub(super) fn draw(scene: &mut Scene, b: Rect, fg: Color, on: f32) {
    let (cup_glyph, steam_glyph) = (cup(), steam());
    if cup_glyph.is_empty() {
        return;
    }
    let progress = on.clamp(0.0, 1.0);
    let (transform, scale) = glyph::fit(VIEW_BOX, b, INK_FILL);
    let stroke = Stroke::new(STROKE_PX / scale)
        .with_caps(Cap::Round)
        .with_join(Join::Round);

    scene.stroke(&stroke, transform, fg, None, cup_glyph.path());

    // The track keeps the icon the same width in both states, so the pill does
    // not jump as the steam comes and goes.
    scene.stroke(
        &stroke,
        transform,
        fade(fg, lerp(0.0, 0.22, 1.0 - progress)),
        None,
        steam_glyph.path(),
    );
    if progress >= 1.0 {
        scene.stroke(&stroke, transform, fg, None, steam_glyph.path());
    } else if progress > 0.0 {
        scene.stroke(
            &stroke,
            transform,
            fg,
            None,
            &steam_glyph.traced(progress),
        );
    }
}

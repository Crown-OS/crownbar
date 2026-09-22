//! Notification bell. `open` ∈ [0, 1] fills the bell as the notification
//! centre comes out; `silenced` ∈ [0, 1] traces a slash across it and fades
//! the clapper away as Do Not Disturb goes on. The outline never changes, so
//! the pill reads as the same object in all four states.

use std::sync::OnceLock;

use vello::{
    kurbo::{Cap, Join, Rect, Stroke},
    peniko::{Color, Fill},
    Scene,
};

use super::{
    fade,
    glyph::{self, Glyph},
};

/// The box all three paths are authored in, so they stay in register with each
/// other however the icon is scaled.
const VIEW_BOX: Rect = Rect::new(0.0, 0.0, 24.0, 24.0);
const INK_FILL: f64 = 0.86;
const STROKE_PX: f64 = 1.5;

const BELL: &str = "M18 8a6 6 0 0 0-12 0c0 7-3 9-3 9h18s-3-2-3-9z";
const CLAPPER: &str = "M13.73 20.5a2 2 0 0 1-3.46 0";
const SLASH: &str = "M3.5 3.5 L20.5 20.5";

fn cached(svg: &'static str, cache: &'static OnceLock<Glyph>) -> &'static Glyph {
    cache.get_or_init(|| Glyph::parse(svg))
}

pub(super) fn draw(scene: &mut Scene, b: Rect, fg: Color, open: f32, silenced: f32) {
    static BELL_CACHE: OnceLock<Glyph> = OnceLock::new();
    static CLAPPER_CACHE: OnceLock<Glyph> = OnceLock::new();
    static SLASH_CACHE: OnceLock<Glyph> = OnceLock::new();

    let bell = cached(BELL, &BELL_CACHE);
    if bell.is_empty() {
        return;
    }
    let (open, silenced) = (open.clamp(0.0, 1.0), silenced.clamp(0.0, 1.0));
    let (transform, scale) = glyph::fit(VIEW_BOX, b, INK_FILL);
    let stroke = Stroke::new(STROKE_PX / scale)
        .with_caps(Cap::Round)
        .with_join(Join::Round);

    // Filling rather than growing: the pill's width is laid out from the glyph
    // box, and a bell that swelled would shove the widgets beside it along as
    // the centre opened.
    if open > 0.0 {
        scene.fill(
            Fill::NonZero,
            transform,
            fade(fg, open * 0.9),
            None,
            bell.path(),
        );
    }
    scene.stroke(&stroke, transform, fg, None, bell.path());
    scene.stroke(
        &stroke,
        transform,
        fade(fg, 1.0 - silenced),
        None,
        cached(CLAPPER, &CLAPPER_CACHE).path(),
    );

    // The slash draws itself on the way the bluetooth rune does, rather than
    // fading, so silencing reads as an action rather than a state change.
    if silenced > 0.0 {
        let slash = cached(SLASH, &SLASH_CACHE);
        let path = if silenced >= 1.0 {
            slash.path().clone()
        } else {
            slash.traced(silenced)
        };
        scene.stroke(&stroke, transform, fg, None, &path);
    }
}

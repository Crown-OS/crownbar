//! A screen with a recording light in its corner. The screen is the bar's
//! foreground like every other pill; the light is the palette's danger red and
//! breathes with `pulse`, because sharing is the one state the user must not
//! overlook.

use std::sync::OnceLock;

use crate::scene::{Fill, Scene};
use crownui::prelude::Color;
use kurbo::{Cap, Circle, Join, Rect, Stroke};

use super::{
    fade,
    glyph::{self, Glyph},
    lerp,
};

const VIEW_BOX: Rect = Rect::new(0.0, 0.0, 24.0, 24.0);
const INK_FILL: f64 = 0.86;
const STROKE_PX: f64 = 1.5;
/// The screen, open at its top-right corner where the light sits.
const SCREEN: &str = "M13 4H5a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-3 M8 21h8 M12 17v4";
const LIGHT_CENTRE: (f64, f64) = (19.0, 5.0);
const LIGHT_RADIUS: f64 = 3.2;

fn screen() -> &'static Glyph {
    static CACHE: OnceLock<Glyph> = OnceLock::new();
    CACHE.get_or_init(|| Glyph::parse(SCREEN))
}

pub(super) fn draw(scene: &mut Scene<'_>, b: Rect, fg: Color, light: Color, pulse: f32) {
    let screen = screen();
    if screen.is_empty() {
        return;
    }
    let (transform, scale) = glyph::fit(VIEW_BOX, b, INK_FILL);
    let stroke = Stroke::new(STROKE_PX / scale)
        .with_caps(Cap::Round)
        .with_join(Join::Round);
    scene.stroke(&stroke, transform, fg, None, screen.path());

    let glow = Circle::new(
        LIGHT_CENTRE,
        LIGHT_RADIUS * f64::from(lerp(1.0, 1.6, pulse)),
    );
    scene.fill(
        Fill::NonZero,
        transform,
        fade(light, lerp(0.35, 0.0, pulse)),
        None,
        &glow,
    );
    scene.fill(
        Fill::NonZero,
        transform,
        light,
        None,
        &Circle::new(LIGHT_CENTRE, LIGHT_RADIUS),
    );
}

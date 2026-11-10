//! Untitled UI's drawing rules, which every glyph in the set follows: authored
//! in a 20 × 20 box, stroked (never filled) at 2 on the 24 grid it is scaled
//! down from, with round caps and joins.

use std::sync::OnceLock;

use crate::scene::Scene;
use crownui::prelude::Color;
use kurbo::{Affine, Cap, Join, Rect, Stroke};

use super::{
    ICON_BOX,
    glyph::{self, Glyph},
};

/// The box every glyph is authored in.
pub(super) const VIEW_BOX: Rect = Rect::new(0.0, 0.0, 20.0, 20.0);
/// Stroke width in view-box units: 2 on the 24 grid.
pub(super) const STROKE: f64 = 1.666_67;
/// The frame a bar glyph is drawn in, centred in its [`ICON_BOX`]. Untitled's
/// native size, so its stroke lands on its native 1.67 px, the weight of the
/// bar's 14 / 600 labels.
const BAR_FRAME: f64 = 20.0;

/// The pen every glyph is stroked with, `width` in view-box units.
pub(super) fn pen(width: f64) -> Stroke {
    Stroke::new(width)
        .with_caps(Cap::Round)
        .with_join(Join::Round)
}

/// Maps the view box onto `b` as the bar's icons sit in it. The width is left
/// to the transform to scale, so a glyph's weight tracks its size.
pub(super) fn place(b: Rect) -> Affine {
    glyph::fit(VIEW_BOX, b, BAR_FRAME / f64::from(ICON_BOX)).0
}

/// One static glyph: its subpaths, the weight they are stroked at, and their
/// geometry, parsed on first use and kept for the life of the process.
pub(super) struct Art {
    paths: &'static [&'static str],
    stroke: f64,
    glyph: OnceLock<Glyph>,
}

impl Art {
    pub(super) const fn new(paths: &'static [&'static str]) -> Self {
        Self::weighted(paths, STROKE)
    }

    pub(super) const fn weighted(paths: &'static [&'static str], stroke: f64) -> Self {
        Self {
            paths,
            stroke,
            glyph: OnceLock::new(),
        }
    }

    pub(super) fn glyph(&self) -> &Glyph {
        self.glyph
            .get_or_init(|| Glyph::parse(&self.paths.join(" ")))
    }

    pub(super) fn pen(&self) -> Stroke {
        pen(self.stroke)
    }

    /// The whole glyph, its view box covering `fill` of `b`.
    pub(super) fn draw(&self, scene: &mut Scene<'_>, b: Rect, fg: Color, fill: f64) {
        let glyph = self.glyph();
        if glyph.is_empty() {
            return;
        }
        let (transform, _) = glyph::fit(VIEW_BOX, b, fill);
        scene.stroke(&self.pen(), transform, fg, None, glyph.path());
    }
}

#[cfg(test)]
pub(super) mod tests {
    use kurbo::{BezPath, Point, Shape};

    use super::*;

    /// Every subpath of `art` parses, and its ink, stroke included, stays
    /// inside the view box.
    pub(in crate::icons) fn assert_fits(art: &Art, name: &str) {
        for path in art.paths {
            assert!(BezPath::from_svg(path).is_ok(), "{name} does not parse");
        }
        assert!(!art.glyph().is_empty(), "{name} has no ink");
        let ink = art.glyph().path().bounding_box();
        let room = VIEW_BOX.inflate(art.stroke / 2.0, art.stroke / 2.0);
        assert!(
            room.contains(ink.origin()) && room.contains(Point::new(ink.x1, ink.y1)),
            "{name} spills out of the view box: {ink:?}"
        );
    }
}

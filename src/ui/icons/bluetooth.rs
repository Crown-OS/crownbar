//! Bluetooth rune. `on` ∈ [0, 1] is the progress of the stroke along the
//! rune's single continuous path: it draws itself on as the adapter comes up
//! and un-draws as it goes down, over a dim full-length track.

use std::sync::OnceLock;

use vello::{
    Scene,
    kurbo::{Affine, BezPath, ParamCurve, ParamCurveArclen, Point, Rect, Shape, Stroke},
    peniko::Color,
};

use super::{fade, lerp};

/// Single-stroke rune in a 24 × 24 box: up-left diagonal, lower flag, stem,
/// upper flag, down-left diagonal — one unbroken trace.
const GLYPH_SVG: &str = "M7 7 L17 17 L12 22 L12 2 L17 7 L7 17";

const ARCLEN_ACCURACY: f64 = 0.01;
const INK_FILL: f64 = 0.84;
const STROKE_PX: f64 = 1.5;

struct Glyph {
    path: BezPath,
    lens: Vec<f64>,
    total: f64,
    ink: Rect,
}

fn glyph() -> &'static Glyph {
    static CACHE: OnceLock<Glyph> = OnceLock::new();
    CACHE.get_or_init(|| {
        let path = BezPath::from_svg(GLYPH_SVG).unwrap_or_default();
        let lens: Vec<f64> = path
            .segments()
            .map(|seg| seg.arclen(ARCLEN_ACCURACY))
            .collect();
        let total = lens.iter().sum();
        let ink = path.bounding_box();
        Glyph {
            path,
            lens,
            total,
            ink,
        }
    })
}

pub(super) fn draw(scene: &mut Scene, b: Rect, fg: Color, on: f32) {
    let glyph = glyph();
    if glyph.total <= 0.0 || glyph.ink.width() <= 0.0 || glyph.ink.height() <= 0.0 {
        return;
    }
    let progress = on.clamp(0.0, 1.0);

    // Map the ink box into `b`: uniform scale, centered.
    let scale = (b.width() / glyph.ink.width()).min(b.height() / glyph.ink.height()) * INK_FILL;
    let tx = b.x0 + (b.width() - glyph.ink.width() * scale) * 0.5 - glyph.ink.x0 * scale;
    let ty = b.y0 + (b.height() - glyph.ink.height() * scale) * 0.5 - glyph.ink.y0 * scale;
    let transform = Affine::new([scale, 0.0, 0.0, scale, tx, ty]);

    // Path is in glyph units, so undo the scale for a constant screen weight.
    let stroke = Stroke::new(STROKE_PX / scale);

    scene.stroke(
        &stroke,
        transform,
        fade(fg, lerp(0.18, 0.30, progress)),
        None,
        &glyph.path,
    );

    if progress <= 0.0 {
        return;
    }
    if progress >= 1.0 {
        scene.stroke(&stroke, transform, fg, None, &glyph.path);
    } else {
        scene.stroke(&stroke, transform, fg, None, &traced(glyph, progress));
    }
}

/// The leading `progress` fraction of the rune, by arclength. The segment
/// straddling the head is cut with `inv_arclen` + `subsegment` so the stroke
/// ends mid-segment instead of snapping between corners.
fn traced(glyph: &Glyph, progress: f32) -> BezPath {
    let target = glyph.total * progress as f64;
    let mut out = BezPath::new();
    let mut walked = 0.0;
    let mut cursor: Option<Point> = None;

    for (seg, len) in glyph.path.segments().zip(glyph.lens.iter().copied()) {
        let left = target - walked;
        if left <= 0.0 {
            break;
        }
        if cursor != Some(seg.start()) {
            out.move_to(seg.start());
        }
        if left >= len {
            out.push(seg.as_path_el());
            cursor = Some(seg.end());
            walked += len;
        } else {
            let t = seg.inv_arclen(left, ARCLEN_ACCURACY);
            out.push(seg.subsegment(0.0..t).as_path_el());
            break;
        }
    }

    out
}

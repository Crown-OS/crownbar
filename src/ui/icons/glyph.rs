//! SVG-authored icon geometry. A glyph is parsed and measured once, then
//! traced to any progress along its own length so an icon can draw itself on.

use vello::kurbo::{Affine, BezPath, ParamCurve, ParamCurveArclen, Point, Rect, Shape};

const ARCLEN_ACCURACY: f64 = 0.01;

pub(super) struct Glyph {
    path: BezPath,
    /// Arclength of each segment of `path`, in author units.
    lens: Vec<f64>,
    total: f64,
    ink: Rect,
}

impl Glyph {
    pub(super) fn parse(svg: &str) -> Self {
        let path = BezPath::from_svg(svg).unwrap_or_default();
        let lens: Vec<f64> = path
            .segments()
            .map(|seg| seg.arclen(ARCLEN_ACCURACY))
            .collect();
        Self {
            total: lens.iter().sum(),
            ink: path.bounding_box(),
            path,
            lens,
        }
    }

    pub(super) fn path(&self) -> &BezPath {
        &self.path
    }

    pub(super) fn ink(&self) -> Rect {
        self.ink
    }

    pub(super) fn is_empty(&self) -> bool {
        self.total <= 0.0
    }

    /// The leading `progress` fraction of the glyph, by arclength. The segment
    /// straddling the head is cut with `inv_arclen` + `subsegment` so the
    /// stroke ends mid-segment instead of snapping between corners.
    pub(super) fn traced(&self, progress: f32) -> BezPath {
        let target = self.total * progress.clamp(0.0, 1.0) as f64;
        let mut out = BezPath::new();
        let mut walked = 0.0;
        let mut cursor: Option<Point> = None;

        for (seg, len) in self.path.segments().zip(self.lens.iter().copied()) {
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
}

/// Uniform scale + centering mapping `ink` (author space) into `b`, covering
/// `fill` of it. Returns the scale too: stroke widths are authored in screen
/// px and have to be divided back down before being drawn under `transform`.
pub(super) fn fit(ink: Rect, b: Rect, fill: f64) -> (Affine, f64) {
    let w = ink.width().max(1e-6);
    let h = ink.height().max(1e-6);
    let scale = (b.width() / w).min(b.height() / h) * fill;
    let tx = b.x0 + (b.width() - w * scale) * 0.5 - ink.x0 * scale;
    let ty = b.y0 + (b.height() - h * scale) * 0.5 - ink.y0 * scale;
    (Affine::new([scale, 0.0, 0.0, scale, tx, ty]), scale)
}

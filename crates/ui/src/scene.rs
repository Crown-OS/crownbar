//! The drawing surface the bar's glyphs are written against.
//!
//! Every glyph is authored in kurbo geometry — arcs, SVG paths, rounded rects,
//! measured and traced by arclength — and handed here with an affine, exactly
//! as a vello scene would take it. This records the same thing into a crownui
//! [`DrawList`], so a canvas painter is the glyph code and nothing more.

use crownui::{
    ext::{Brush, DrawCommand, DrawList, Paint, Path, Stroke, StrokeCap, StrokeJoin, Transform},
    prelude::{Color, CornerRadii, Point, Rect},
};
use kurbo::{Affine, PathEl, Shape};

use crate::theme::{SHADOW_BLUR, SHADOW_DROP, scale_alpha};

/// Path flattening tolerance, in the glyph's own units, for shapes kurbo
/// describes analytically (circles, arcs).
const TOLERANCE: f64 = 0.05;

/// Fill rule, kept so a glyph reads the way it was written. Paths are always
/// filled non-zero.
#[derive(Clone, Copy, Debug)]
pub enum Fill {
    NonZero,
}

/// Anything a shape can be painted with.
pub trait IntoPaint {
    fn into_paint(self) -> Paint;
}

impl IntoPaint for Color {
    fn into_paint(self) -> Paint {
        Paint::solid(self)
    }
}

impl IntoPaint for Paint {
    fn into_paint(self) -> Paint {
        self
    }
}

impl IntoPaint for &Brush {
    fn into_paint(self) -> Paint {
        Paint {
            brush: self.clone(),
            blur: None,
        }
    }
}

pub struct Scene<'a> {
    list: &'a mut DrawList,
    /// Set while the scene records its glyphs' shadow rather than the glyphs.
    shadow: Option<Color>,
}

impl<'a> Scene<'a> {
    pub fn new(list: &'a mut DrawList) -> Self {
        Self { list, shadow: None }
    }

    /// Records what `draw` paints twice: as a soft shadow in `shadow`, then over
    /// it as itself.
    pub fn shadowed(list: &mut DrawList, shadow: Color, draw: impl Fn(&mut Scene)) {
        if shadow.a > 0.0 {
            draw(&mut Scene {
                list: &mut *list,
                shadow: Some(shadow),
            });
        }
        draw(&mut Scene::new(list));
    }

    pub fn fill(
        &mut self,
        _: Fill,
        transform: Affine,
        paint: impl IntoPaint,
        _: Option<Affine>,
        shape: &impl Shape,
    ) {
        let command = DrawCommand::FillPath(path_of(shape), paint.into_paint());
        self.draw(transform, command);
    }

    pub fn stroke(
        &mut self,
        stroke: &kurbo::Stroke,
        transform: Affine,
        paint: impl IntoPaint,
        _: Option<Affine>,
        shape: &impl Shape,
    ) {
        let command = DrawCommand::StrokePath(path_of(shape), line(stroke), paint.into_paint());
        self.draw(transform, command);
    }

    /// Whatever `draw` paints, kept inside `clip`.
    pub fn clipped(&mut self, clip: kurbo::Rect, draw: impl FnOnce(&mut Self)) {
        self.list.save();
        self.list.clip(
            Rect::from_xywh(
                clip.x0 as f32,
                clip.y0 as f32,
                clip.width() as f32,
                clip.height() as f32,
            ),
            CornerRadii::ZERO,
        );
        draw(self);
        self.list.restore();
    }

    fn draw(&mut self, transform: Affine, command: DrawCommand) {
        let (transform, command) = match self.shadow {
            Some(shadow) => (
                Affine::translate((0.0, f64::from(SHADOW_DROP))) * transform,
                silhouette(command, shadow),
            ),
            None => (transform, command),
        };
        if transform == Affine::IDENTITY {
            self.list.push(command);
            return;
        }
        self.list.save();
        self.list.transform(to_transform(transform));
        self.list.push(command);
        self.list.restore();
    }
}

/// `command` painted as `shadow`, blurred, as strong as its own paint is opaque.
fn silhouette(command: DrawCommand, shadow: Color) -> DrawCommand {
    let cast = |paint: Paint| {
        let coverage = match paint.brush {
            Brush::Solid(color) => color.a,
            _ => 1.0,
        };
        Paint {
            brush: Brush::Solid(scale_alpha(shadow, coverage)),
            blur: Some(SHADOW_BLUR),
        }
    };
    match command {
        DrawCommand::FillPath(path, paint) => DrawCommand::FillPath(path, cast(paint)),
        DrawCommand::StrokePath(path, stroke, paint) => {
            DrawCommand::StrokePath(path, stroke, cast(paint))
        }
        other => other,
    }
}

fn path_of(shape: &impl Shape) -> Path {
    let point = |p: kurbo::Point| Point::new(p.x as f32, p.y as f32);
    shape
        .path_elements(TOLERANCE)
        .fold(Path::new(), |path, element| match element {
            PathEl::MoveTo(to) => path.move_to(point(to)),
            PathEl::LineTo(to) => path.line_to(point(to)),
            PathEl::QuadTo(control, to) => path.quad_to(point(control), point(to)),
            PathEl::CurveTo(first, second, to) => {
                path.cubic_to(point(first), point(second), point(to))
            }
            PathEl::ClosePath => path.close(),
        })
}

fn line(stroke: &kurbo::Stroke) -> Stroke {
    Stroke {
        width: stroke.width as f32,
        cap: match stroke.start_cap {
            kurbo::Cap::Butt => StrokeCap::Butt,
            kurbo::Cap::Round => StrokeCap::Round,
            kurbo::Cap::Square => StrokeCap::Square,
        },
        join: match stroke.join {
            kurbo::Join::Bevel => StrokeJoin::Bevel,
            kurbo::Join::Miter => StrokeJoin::Miter,
            kurbo::Join::Round => StrokeJoin::Round,
        },
    }
}

fn to_transform(affine: Affine) -> Transform {
    Transform {
        matrix: affine.as_coeffs().map(|coefficient| coefficient as f32),
    }
}

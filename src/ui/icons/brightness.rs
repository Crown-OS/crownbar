use vello::{
    Scene,
    kurbo::{Affine, Circle, Line, Point, Rect, Stroke},
    peniko::{Color, Fill},
};

use super::{fade, lerp};

pub(super) fn draw(scene: &mut Scene, b: Rect, fg: Color, level: f32) {
    let cx = (b.x0 + b.x1) * 0.5;
    let cy = (b.y0 + b.y1) * 0.5;
    let r = b.width() * 0.18;
    let core_alpha = lerp(0.55, 1.0, level);
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        fade(fg, core_alpha),
        None,
        &Circle::new(Point::new(cx, cy), r),
    );

    // Eight rays; length scales with level.
    let n = 8;
    let inner = r + 1.5;
    let outer = inner + lerp(1.0, 4.0, level) as f64;
    for i in 0..n {
        let angle = (i as f32 / n as f32) * std::f32::consts::TAU;
        let (s, c) = angle.sin_cos();
        let s = s as f64;
        let c = c as f64;
        let p0 = Point::new(cx + c * inner, cy + s * inner);
        let p1 = Point::new(cx + c * outer, cy + s * outer);
        scene.stroke(
            &Stroke::new(1.3),
            Affine::IDENTITY,
            fade(fg, core_alpha),
            None,
            &Line::new(p0, p1),
        );
    }
}

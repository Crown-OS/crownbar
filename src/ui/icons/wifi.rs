use vello::{
    Scene,
    kurbo::{Affine, BezPath, Circle, Point, Rect, Stroke},
    peniko::{Color, Fill},
};

use super::{fade, lerp};

pub(super) fn draw(scene: &mut Scene, b: Rect, fg: Color, strength: f32) {
    let cx = (b.x0 + b.x1) * 0.5;
    let base_y = b.y1 - 1.5;
    // strength ∈ [0, 1] linearly distributes brightness across the arcs.
    let thresholds = [0.15f32, 0.45, 0.75];
    let dot = Circle::new(Point::new(cx, base_y), 1.1);
    let dot_alpha = (strength * 3.0).clamp(0.0, 1.0);
    scene.fill(Fill::NonZero, Affine::IDENTITY, fade(fg, dot_alpha.max(0.25)), None, &dot);

    for (i, t) in thresholds.iter().enumerate() {
        let r = 3.0 + i as f64 * 3.0;
        let alpha = ((strength - t) / 0.3).clamp(0.0, 1.0);
        let mut path = BezPath::new();
        // Approximate an arc with two cubics.
        let start = Point::new(cx - r, base_y);
        let end = Point::new(cx + r, base_y);
        let top = Point::new(cx, base_y - r);
        path.move_to(start);
        path.curve_to(
            Point::new(cx - r, base_y - r * 0.55),
            Point::new(cx - r * 0.55, base_y - r),
            top,
        );
        path.curve_to(
            Point::new(cx + r * 0.55, base_y - r),
            Point::new(cx + r, base_y - r * 0.55),
            end,
        );
        let stroke = Stroke::new(1.3);
        let color = fade(fg, lerp(0.20, 1.0, alpha));
        scene.stroke(&stroke, Affine::IDENTITY, color, None, &path);
    }
}

use vello::{
    Scene,
    kurbo::{Affine, BezPath, Line, Point, Rect, Stroke},
    peniko::{Color, Fill},
};

use super::fade;

pub(super) fn draw(scene: &mut Scene, b: Rect, fg: Color, level: f32, muted: f32) {
    let cx = (b.x0 + b.x1) * 0.5;
    let cy = (b.y0 + b.y1) * 0.5;
    let size = b.height();

    // Speaker body (trapezoid). Always drawn.
    let x0 = cx - size * 0.40;
    let x1 = cx - size * 0.10;
    let x2 = cx + size * 0.10;
    let mut speaker = BezPath::new();
    speaker.move_to(Point::new(x0, cy - size * 0.10));
    speaker.line_to(Point::new(x1, cy - size * 0.10));
    speaker.line_to(Point::new(x2, cy - size * 0.30));
    speaker.line_to(Point::new(x2, cy + size * 0.30));
    speaker.line_to(Point::new(x1, cy + size * 0.10));
    speaker.line_to(Point::new(x0, cy + size * 0.10));
    speaker.close_path();
    scene.fill(Fill::NonZero, Affine::IDENTITY, fg, None, &speaker);

    // Sound waves — fade with `level`, attenuated by `muted`.
    let unmuted = 1.0 - muted.clamp(0.0, 1.0);
    for (i, threshold) in [0.05f32, 0.4, 0.75].iter().enumerate() {
        let alpha = ((level - threshold) / 0.3).clamp(0.0, 1.0) * unmuted;
        if alpha <= 0.0 {
            continue;
        }
        let r = size * (0.18 + i as f64 * 0.13);
        let start_angle = -0.5f32;
        let mut path = BezPath::new();
        let from = Point::new(
            cx + size * 0.18 + r * start_angle.cos() as f64,
            cy + r * start_angle.sin() as f64,
        );
        let to = Point::new(
            cx + size * 0.18 + r * (-start_angle).cos() as f64,
            cy + r * (-start_angle).sin() as f64,
        );
        path.move_to(from);
        path.curve_to(
            Point::new(cx + size * 0.18 + r * 1.05, cy - r * 0.55),
            Point::new(cx + size * 0.18 + r * 1.05, cy + r * 0.55),
            to,
        );
        scene.stroke(&Stroke::new(1.2), Affine::IDENTITY, fade(fg, alpha), None, &path);
    }

    // Muted: slash overlaid, alpha follows the muted spring.
    let slash_alpha = muted.clamp(0.0, 1.0);
    if slash_alpha > 0.0 {
        let s = size * 0.45;
        let line = Line::new(
            Point::new(cx - s * 0.6, cy - s),
            Point::new(cx + s, cy + s * 0.6),
        );
        scene.stroke(
            &Stroke::new(1.6),
            Affine::IDENTITY,
            fade(fg, slash_alpha),
            None,
            &line,
        );
    }
}

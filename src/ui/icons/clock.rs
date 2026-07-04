use vello::{
    Scene,
    kurbo::{Affine, Circle, Line, Point, Rect, Stroke},
    peniko::Color,
};

pub(super) fn draw(scene: &mut Scene, b: Rect, fg: Color, hours: f32, minutes: f32) {
    let cx = (b.x0 + b.x1) * 0.5;
    let cy = (b.y0 + b.y1) * 0.5;
    let r = (b.width().min(b.height())) * 0.5 - 1.0;
    let face = Circle::new(Point::new(cx, cy), r);
    scene.stroke(&Stroke::new(1.3), Affine::IDENTITY, fg, None, &face);

    // Hands. Hours in [0, 1) = full revolution.
    let hour_angle = hours * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
    let min_angle = minutes * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;

    let hour_end = Point::new(
        cx + (r * 0.55 * hour_angle.cos() as f64),
        cy + (r * 0.55 * hour_angle.sin() as f64),
    );
    let min_end = Point::new(
        cx + (r * 0.85 * min_angle.cos() as f64),
        cy + (r * 0.85 * min_angle.sin() as f64),
    );
    scene.stroke(
        &Stroke::new(1.4),
        Affine::IDENTITY,
        fg,
        None,
        &Line::new(Point::new(cx, cy), hour_end),
    );
    scene.stroke(
        &Stroke::new(1.1),
        Affine::IDENTITY,
        fg,
        None,
        &Line::new(Point::new(cx, cy), min_end),
    );
}

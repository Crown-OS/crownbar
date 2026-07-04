use vello::{
    Scene,
    kurbo::{Affine, Rect, RoundedRect, Stroke, Vec2},
    peniko::Color,
};

pub(super) fn draw(scene: &mut Scene, b: Rect, fg: Color, tiled: f32) {
    let t = tiled.clamp(0.0, 1.0) as f64;
    let stroke = Stroke::new(1.2);

    // Three windows. Each one interpolates between a "floating" pose
    // (rotated, scattered, varying sizes) and a "tiled" pose (snapped grid).
    let floating = [
        // (cx, cy, w, h, rot_rad) — relative to bounds
        (0.30, 0.30, 0.38, 0.38, -0.18),
        (0.65, 0.45, 0.30, 0.34, 0.12),
        (0.45, 0.70, 0.32, 0.30, -0.06),
    ];
    let tiled_poses = [
        (0.30, 0.30, 0.50, 0.50, 0.0),
        (0.75, 0.30, 0.30, 0.50, 0.0),
        (0.75, 0.72, 0.30, 0.34, 0.0),
    ];

    let w = b.width();
    let h = b.height();
    let inset = w * 0.08;

    for (f, g) in floating.iter().zip(tiled_poses.iter()) {
        let cx_f = b.x0 + inset + f.0 * (w - 2.0 * inset);
        let cy_f = b.y0 + inset + f.1 * (h - 2.0 * inset);
        let rw_f = f.2 * (w - 2.0 * inset) * 0.9;
        let rh_f = f.3 * (h - 2.0 * inset) * 0.9;
        let cx_t = b.x0 + inset + g.0 * (w - 2.0 * inset);
        let cy_t = b.y0 + inset + g.1 * (h - 2.0 * inset);
        let rw_t = g.2 * (w - 2.0 * inset) * 0.9;
        let rh_t = g.3 * (h - 2.0 * inset) * 0.9;

        let cx = cx_f + (cx_t - cx_f) * t;
        let cy = cy_f + (cy_t - cy_f) * t;
        let rw = rw_f + (rw_t - rw_f) * t;
        let rh = rh_f + (rh_t - rh_f) * t;
        let rot = f.4 + (g.4 - f.4) * t;

        let rect = Rect::new(-rw * 0.5, -rh * 0.5, rw * 0.5, rh * 0.5);
        let rounded = RoundedRect::from_rect(rect, 1.5);
        let xform = Affine::translate(Vec2::new(cx, cy)) * Affine::rotate(rot);
        scene.stroke(&stroke, xform, fg, None, &rounded);
    }
}

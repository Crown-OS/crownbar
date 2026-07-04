use vello::{
    Scene,
    kurbo::{Affine, BezPath, Line, Point, Rect, RoundedRect, Stroke},
    peniko::{Color, Fill},
};

use super::fade;

pub(super) fn draw(
    scene: &mut Scene,
    b: Rect,
    fg: Color,
    pct: f32,
    charging: f32,
    saver: f32,
) {
    // Body: rounded rect, with a tiny terminal nub on the right.
    let pad_x = b.width() * 0.08;
    let pad_y = b.height() * 0.25;
    let body = RoundedRect::new(
        b.x0 + pad_x,
        b.y0 + pad_y,
        b.x1 - pad_x * 2.5,
        b.y1 - pad_y,
        2.0,
    );
    let stroke = Stroke::new(1.2);
    scene.stroke(&stroke, Affine::IDENTITY, fg, None, &body);

    // Terminal nub.
    let nub = Rect::new(
        b.x1 - pad_x * 2.5,
        b.y0 + pad_y + b.height() * 0.08,
        b.x1 - pad_x,
        b.y1 - pad_y - b.height() * 0.08,
    );
    scene.fill(Fill::NonZero, Affine::IDENTITY, fg, None, &nub);

    // Fill bar inside body. Saver mode shifts color toward a "leaf" tint by
    // tinting the fill alpha less and the leaf overlay more.
    let inner_pad = 1.2;
    let inner_x0 = b.x0 + pad_x + inner_pad;
    let inner_x1 = b.x1 - pad_x * 2.5 - inner_pad;
    let inner_y0 = b.y0 + pad_y + inner_pad;
    let inner_y1 = b.y1 - pad_y - inner_pad;
    let total_w = inner_x1 - inner_x0;
    let fill_w = total_w * pct.clamp(0.05, 1.0) as f64;
    let fill_rect = Rect::new(inner_x0, inner_y0, inner_x0 + fill_w, inner_y1);
    let regular_alpha = 1.0 - saver * 0.55; // dim the bar when saver is "on"
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        fade(fg, regular_alpha),
        None,
        &fill_rect,
    );

    // Saver leaf overlay — fades in with the spring. Two short curves
    // arranged like a leaf glyph sitting on the battery body.
    let leaf_alpha = saver.clamp(0.0, 1.0);
    if leaf_alpha > 0.0 {
        let lcx = (inner_x0 + inner_x1) * 0.5;
        let lcy = (inner_y0 + inner_y1) * 0.5;
        let lw = (inner_y1 - inner_y0) * 0.45;
        let mut leaf = BezPath::new();
        leaf.move_to(Point::new(lcx - lw, lcy));
        leaf.curve_to(
            Point::new(lcx - lw, lcy - lw * 1.2),
            Point::new(lcx + lw, lcy - lw * 1.2),
            Point::new(lcx + lw, lcy),
        );
        leaf.curve_to(
            Point::new(lcx + lw, lcy + lw * 1.2),
            Point::new(lcx - lw, lcy + lw * 1.2),
            Point::new(lcx - lw, lcy),
        );
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            fade(fg, leaf_alpha),
            None,
            &leaf,
        );
        // Stem down the middle.
        scene.stroke(
            &Stroke::new(0.9),
            Affine::IDENTITY,
            fade(fg, leaf_alpha * 0.6),
            None,
            &Line::new(
                Point::new(lcx - lw * 0.4, lcy - lw * 0.8),
                Point::new(lcx + lw * 0.4, lcy + lw * 0.8),
            ),
        );
    }

    // Charging bolt — fades in with `charging` float.
    let bolt_alpha = charging.clamp(0.0, 1.0);
    if bolt_alpha > 0.0 {
        let bcx = (inner_x0 + inner_x1) * 0.5;
        let bcy = (inner_y0 + inner_y1) * 0.5;
        let bs = (inner_y1 - inner_y0) * 0.5;
        let mut bolt = BezPath::new();
        bolt.move_to(Point::new(bcx + bs * 0.1, bcy - bs));
        bolt.line_to(Point::new(bcx - bs * 0.3, bcy + bs * 0.15));
        bolt.line_to(Point::new(bcx - bs * 0.05, bcy + bs * 0.15));
        bolt.line_to(Point::new(bcx - bs * 0.1, bcy + bs));
        bolt.line_to(Point::new(bcx + bs * 0.3, bcy - bs * 0.15));
        bolt.line_to(Point::new(bcx + bs * 0.05, bcy - bs * 0.15));
        bolt.close_path();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            fade(fg, bolt_alpha),
            None,
            &bolt,
        );
    }
}

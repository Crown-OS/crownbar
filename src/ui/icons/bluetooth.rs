use vello::{
    kurbo::{Affine, BezPath, Rect, Stroke},
    peniko::Color,
    Scene,
};

use super::{fade, lerp};

pub(super) fn draw(scene: &mut Scene, b: Rect, fg: Color, on: f32) {
    // Stylized "B" runic shape. Fades between dim (off) and bright (on).
    // let cx = (b.x0 + b.x1) * 0.5;
    // let cy = (b.y0 + b.y1) * 0.5;
    // let h = b.height() * 0.7;
    // let half_h = h * 0.5;
    // let half_w = h * 0.30;
    //
    // let top = Point::new(cx, cy - half_h);
    // let bot = Point::new(cx, cy + half_h);
    // let mid_l = Point::new(cx - half_w * 0.9, cy);
    // let mid_r = Point::new(cx + half_w * 0.9, cy);
    // let upper_join = Point::new(cx + half_w, cy - half_h * 0.5);
    // let lower_join = Point::new(cx + half_w, cy + half_h * 0.5);
    //
    // let mut path = BezPath::new();
    // path.move_to(mid_l);
    // path.line_to(top);
    // path.line_to(upper_join);
    // path.line_to(mid_r);
    // path.line_to(lower_join);
    // path.line_to(bot);
    // path.line_to(mid_l);

    let svg = "m74.168 65.387-19.504-15.387 19.504-15.387c0.69531-0.54688 1.0977-1.3828 1.0977-2.2656 0-0.88672-0.40234-1.7227-1.0977-2.2695l-22.379-17.652c-0.86719-0.6875-2.0547-0.81641-3.0508-0.33203-0.99609 0.48438-1.6289 1.4922-1.6289 2.5977v29.348l-17.699-13.961c-1.25-0.98828-3.0664-0.77344-4.0547 0.48047-0.98828 1.25-0.77344 3.0664 0.47656 4.0547l19.504 15.387-19.504 15.387c-0.60547 0.47266-0.99609 1.168-1.0859 1.9297-0.09375 0.76172 0.125 1.5312 0.60156 2.1328 0.47266 0.60156 1.1719 0.99219 1.9336 1.082 0.76172 0.085938 1.5273-0.13281 2.1289-0.60938l17.699-13.961v29.348c0 1.1055 0.63281 2.1133 1.6289 2.5977 0.99609 0.48438 2.1836 0.35547 3.0508-0.33203l22.379-17.652c0.69531-0.54688 1.0977-1.3828 1.0977-2.2695 0-0.88281-0.40234-1.7188-1.0977-2.2656zm-21.277-44.734 14.824 11.695-14.824 11.691zm0 58.695v-23.387l14.824 11.691z";

    let path = BezPath::from_svg(svg).unwrap();

    // SVG path is authored in a 100×100 coordinate space; map it into `b`.
    let svg_size = 100.0_f64;
    let scale = b.height().min(b.width()) / svg_size;
    let tx = b.x0 + (b.width() - svg_size * scale) * 0.75;
    let ty = b.y0 + (b.height() - svg_size * scale) * 0.75;
    let transform = Affine::new([scale, 0.0, 0.0, scale, tx, ty]);

    let alpha = lerp(1.0, 1.0, on);
    let stroke = Stroke::new(2.4);
    scene.stroke(&stroke, transform, fade(fg, alpha), None, &path);
}

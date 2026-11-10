//! Brightness sun, Untitled's sun: a ring and eight rays. `level` ∈ [0, 1]
//! grows the rays from stubs to their full length and brings the whole sun up
//! from dim, so full brightness is exactly the Untitled glyph.

use std::f64::consts::TAU;

use crate::scene::Scene;
use crownui::prelude::Color;
use kurbo::{BezPath, Circle, Point, Rect, Vec2};

use super::{
    art::{self, STROKE},
    fade, lerp,
};

const CENTRE: Point = Point::new(10.0, 10.0);
const CORE_RADIUS: f64 = 4.166_67;
const RAY_COUNT: usize = 8;
const RAY_START: f64 = 6.666_67;
const RAY_END: f64 = 8.333_33;
/// What is left of a ray with the screen at its dimmest.
const RAY_STUB: f64 = 0.25;
const DIM_ALPHA: f32 = 0.55;

pub(super) fn draw(scene: &mut Scene<'_>, b: Rect, fg: Color, level: f32) {
    let level = level.clamp(0.0, 1.0);
    let transform = art::place(b);
    let pen = art::pen(STROKE);
    let ink = fade(fg, lerp(DIM_ALPHA, 1.0, level));

    scene.stroke(
        &pen,
        transform,
        ink,
        None,
        &Circle::new(CENTRE, CORE_RADIUS),
    );
    scene.stroke(&pen, transform, ink, None, &rays(level));
}

fn rays(level: f32) -> BezPath {
    let shortest = RAY_START + RAY_STUB;
    let end = shortest + (RAY_END - shortest) * f64::from(level);
    let mut rays = BezPath::new();
    for ray in 0..RAY_COUNT {
        let direction = Vec2::from_angle(TAU * ray as f64 / RAY_COUNT as f64);
        rays.move_to(CENTRE + direction * RAY_START);
        rays.line_to(CENTRE + direction * end);
    }
    rays
}

#[cfg(test)]
mod tests {
    use kurbo::Shape;

    use super::*;
    use crate::icons::art::VIEW_BOX;

    #[test]
    fn full_rays_stay_inside_the_view_box() {
        let room = VIEW_BOX.inflate(STROKE / 2.0, STROKE / 2.0);
        let ink = rays(1.0).bounding_box();
        assert!(room.contains(ink.origin()) && room.contains(Point::new(ink.x1, ink.y1)));
    }

    #[test]
    fn rays_grow_with_level() {
        assert!(rays(0.0).bounding_box().width() < rays(1.0).bounding_box().width());
    }
}

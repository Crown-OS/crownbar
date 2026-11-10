//! Window layout, Untitled's layout-grid-01. `tiled` ∈ [0, 1] snaps four
//! windows from a loose, tilted scatter into the grid's quarters, their outer
//! corners growing to the frame's radius and their inner ones squaring off, so
//! the fully tiled pose is the Untitled glyph.

use crate::scene::Scene;
use crownui::prelude::Color;
use kurbo::{Affine, BezPath, Point, Rect, RoundedRect, RoundedRectRadii, Shape, Vec2};

use super::art::{self, STROKE};

/// Each quarter's centre in the tiled grid, in the order kurbo lists corner
/// radii — top-left, top-right, bottom-right, bottom-left — so quarter `i`'s
/// outer corner is radius `i`.
const QUARTERS: [Point; 4] = [
    Point::new(6.25, 6.25),
    Point::new(13.75, 6.25),
    Point::new(13.75, 13.75),
    Point::new(6.25, 13.75),
];
const QUARTER_SIDE: f64 = 7.5;
/// The frame's corner, as large as a quarter allows.
const FRAME_RADIUS: f64 = QUARTER_SIDE / 2.0;
const FLOATING_RADIUS: f64 = 1.5;
const TOLERANCE: f64 = 0.05;

/// Where a window drifts to off the grid, about its own quarter.
struct Drift {
    offset: Vec2,
    scale: f64,
    tilt: f64,
}

const FLOATING: [Drift; 4] = [
    Drift {
        offset: Vec2::new(-0.4, -0.2),
        scale: 0.8,
        tilt: -0.16,
    },
    Drift {
        offset: Vec2::new(0.6, 0.5),
        scale: 0.66,
        tilt: 0.12,
    },
    Drift {
        offset: Vec2::new(0.3, 0.7),
        scale: 0.74,
        tilt: -0.08,
    },
    Drift {
        offset: Vec2::new(-0.5, 0.4),
        scale: 0.7,
        tilt: 0.1,
    },
];

pub(super) fn draw(scene: &mut Scene<'_>, b: Rect, fg: Color, tiled: f32) {
    let windows = windows(f64::from(tiled.clamp(0.0, 1.0)));
    scene.stroke(&art::pen(STROKE), art::place(b), fg, None, &windows);
}

/// All four windows, stroked as one path.
fn windows(tiled: f64) -> BezPath {
    let loose = 1.0 - tiled;
    let half = QUARTER_SIDE / 2.0;
    let mut windows = BezPath::new();
    for (corner, (centre, drift)) in QUARTERS.iter().zip(&FLOATING).enumerate() {
        let mut radii = [FLOATING_RADIUS * loose; 4];
        radii[corner] += FRAME_RADIUS * tiled;
        let window = RoundedRect::from_rect(
            Rect::new(-half, -half, half, half),
            RoundedRectRadii::new(radii[0], radii[1], radii[2], radii[3]),
        );
        let placement = Affine::translate(centre.to_vec2() + drift.offset * loose)
            * Affine::rotate(drift.tilt * loose)
            * Affine::scale(drift.scale + (1.0 - drift.scale) * tiled);
        windows.extend(window.path_elements(TOLERANCE).map(|el| placement * el));
    }
    windows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::art::VIEW_BOX;

    #[test]
    fn every_pose_stays_inside_the_view_box() {
        let room = VIEW_BOX.inflate(STROKE / 2.0, STROKE / 2.0);
        for tiled in [0.0, 0.5, 1.0] {
            let ink = windows(tiled).bounding_box();
            assert!(
                room.contains(ink.origin()) && room.contains(Point::new(ink.x1, ink.y1)),
                "tiled {tiled} spills out: {ink:?}"
            );
        }
    }

    #[test]
    fn tiled_fills_the_untitled_frame() {
        let ink = windows(1.0).bounding_box();
        let frame = Rect::new(2.5, 2.5, 17.5, 17.5);
        let edges = |r: Rect| [r.x0, r.y0, r.x1, r.y1];
        for (got, want) in edges(ink).into_iter().zip(edges(frame)) {
            assert!((got - want).abs() < 1e-6, "{ink:?} is not {frame:?}");
        }
    }
}

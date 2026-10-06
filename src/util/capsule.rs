//! A capsule cut by a vertical line, as a path rather than a clip.
//!
//! A battery's charge is its cell filled up to a line. Drawing that as a
//! rectangle clipped to the rounded cell leans on the renderer's rounded
//! clips; the cell is convex, so cutting its outline at the line is exact on
//! any renderer and costs a few dozen points.

use std::f64::consts::PI;

use kurbo::{BezPath, Point, Rect};

/// Points per semicircular end; enough that the curve reads as round at the
/// sizes a bar draws a battery.
const CAP_STEPS: usize = 16;

/// The fully rounded capsule filling `bounds`, kept left of `edge`.
pub fn capsule_left_of(bounds: Rect, edge: f64) -> BezPath {
    let radius = bounds.height() * 0.5;
    let center_y = bounds.center().y;
    let cap = |center_x: f64, from: f64| {
        (0..=CAP_STEPS).map(move |step| {
            let angle = from + PI * step as f64 / CAP_STEPS as f64;
            Point::new(
                center_x + radius * angle.cos(),
                center_y + radius * angle.sin(),
            )
        })
    };
    let outline: Vec<Point> = cap(bounds.x1 - radius, -PI * 0.5)
        .chain(cap(bounds.x0 + radius, PI * 0.5))
        .collect();
    polygon(&keep_left_of(&outline, edge))
}

/// Sutherland–Hodgman against the half-plane `x <= edge`.
fn keep_left_of(outline: &[Point], edge: f64) -> Vec<Point> {
    let mut kept = Vec::with_capacity(outline.len() + 2);
    for (index, &current) in outline.iter().enumerate() {
        let previous = outline[(index + outline.len() - 1) % outline.len()];
        let (inside, was_inside) = (current.x <= edge, previous.x <= edge);
        if inside != was_inside {
            let t = (edge - previous.x) / (current.x - previous.x);
            kept.push(Point::new(edge, previous.y + (current.y - previous.y) * t));
        }
        if inside {
            kept.push(current);
        }
    }
    kept
}

fn polygon(points: &[Point]) -> BezPath {
    let mut path = BezPath::new();
    let Some((first, rest)) = points.split_first() else {
        return path;
    };
    path.move_to(*first);
    for point in rest {
        path.line_to(*point);
    }
    path.close_path();
    path
}

#[cfg(test)]
mod tests {
    use kurbo::Shape;

    use super::*;

    const CELL: Rect = Rect::new(0.0, 0.0, 36.0, 18.0);

    #[test]
    fn a_full_charge_is_the_whole_capsule() {
        let bounds = capsule_left_of(CELL, CELL.x1).bounding_box();
        assert!(
            (bounds.width() - CELL.width()).abs() < 0.01
                && (bounds.height() - CELL.height()).abs() < 0.01
        );
    }

    #[test]
    fn a_partial_charge_stops_at_the_line_and_keeps_its_rounded_end() {
        let charge = capsule_left_of(CELL, 20.0);
        let bounds = charge.bounding_box();
        assert!((bounds.x1 - 20.0).abs() < 1e-9);
        assert!(
            !charge.contains(Point::new(0.5, 0.5)),
            "the left end is not rounded"
        );
        assert!(charge.contains(Point::new(19.0, 9.0)));
    }

    #[test]
    fn nothing_is_left_of_the_cell() {
        assert!(capsule_left_of(CELL, -1.0).is_empty());
    }
}

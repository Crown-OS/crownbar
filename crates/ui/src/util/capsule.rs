//! Pieces of a capsule, as paths rather than clips.
//!
//! A battery's charge is its cell filled up to a line. Drawing that as a
//! rectangle clipped to the rounded cell leans on the renderer's rounded
//! clips; the cell is convex, so cutting its outline at the line is exact on
//! any renderer and costs a few dozen points. A chip's highlight is the same
//! kind of piece: the sliver of its outline that a copy shifted down leaves
//! uncovered.

use std::f64::consts::{FRAC_PI_2, PI};

use kurbo::{Arc, BezPath, Point, Rect, Vec2};

/// Points per semicircular end; enough that the curve reads as round at the
/// sizes a bar draws a battery.
const CAP_STEPS: usize = 16;
/// How closely a crescent's arcs are followed, in px.
const TOLERANCE: f64 = 0.05;

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

/// The part of the capsule filling `bounds` that a copy of it moved down by
/// `drop` leaves uncovered: a crescent along its top edge, `drop` thick at
/// the middle and tapering to nothing halfway down each end. Nothing for a
/// box taller than it is wide, which has no straight top to run along.
pub fn capsule_crescent(bounds: Rect, drop: f64) -> BezPath {
    let mut path = BezPath::new();
    if drop <= 0.0 || bounds.width() < bounds.height() || bounds.height() <= 0.0 {
        return path;
    }
    let radius = bounds.height() * 0.5;
    let drop = drop.min(bounds.height());
    let meet = (drop * 0.5 / radius).asin();
    let (left, right, centre_y) = (bounds.x0 + radius, bounds.x1 - radius, bounds.center().y);
    let arc = |x: f64, y: f64, start: f64, sweep: f64| {
        Arc::new(
            Point::new(x, y),
            Vec2::new(radius, radius),
            start,
            sweep,
            0.0,
        )
    };
    let edge = arc(left, centre_y, PI - meet, FRAC_PI_2 + meet);
    path.move_to(edge.center + Vec2::from_angle(edge.start_angle) * radius);
    path.extend(edge.append_iter(TOLERANCE));
    path.line_to(Point::new(right, bounds.y0));
    path.extend(arc(right, centre_y, -FRAC_PI_2, FRAC_PI_2 + meet).append_iter(TOLERANCE));
    path.extend(arc(right, centre_y + drop, -meet, meet - FRAC_PI_2).append_iter(TOLERANCE));
    path.line_to(Point::new(left, bounds.y0 + drop));
    path.extend(arc(left, centre_y + drop, -FRAC_PI_2, meet - FRAC_PI_2).append_iter(TOLERANCE));
    path.close_path();
    path
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

    #[test]
    fn a_crescent_runs_along_the_top_edge_only() {
        let crescent = capsule_crescent(CELL, 1.0);
        let bounds = crescent.bounding_box();
        assert!((bounds.y0 - CELL.y0).abs() < 1e-9);
        assert!(
            (bounds.y1 - CELL.center().y - 0.5).abs() < 1e-6,
            "{bounds:?}"
        );
        assert!((bounds.width() - CELL.width()).abs() < 0.01, "{bounds:?}");
        assert!(crescent.contains(Point::new(18.0, 0.5)));
        assert!(!crescent.contains(Point::new(18.0, 1.5)));
        assert!(!crescent.contains(Point::new(18.0, 17.5)));
        assert!((crescent.area().abs() - CELL.width()).abs() < 0.5);
    }

    #[test]
    fn no_crescent_without_a_drop_or_a_straight_top() {
        assert!(capsule_crescent(CELL, 0.0).is_empty());
        assert!(capsule_crescent(Rect::new(0.0, 0.0, 10.0, 18.0), 1.0).is_empty());
    }
}

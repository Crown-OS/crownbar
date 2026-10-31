//! Inter's bold figures as outlines. A reading drawn as text lands on a
//! baseline the renderer rounds to the pixel grid, which at a fractional scale
//! leaves it up to a pixel off the cell's middle; as paths it is centred on its
//! own ink at any scale.

use std::sync::OnceLock;

use crownui::kit;
use kurbo::{Affine, BezPath, Point};
use ttf_parser::{Face, OutlineBuilder, Tag};

/// The weight the bar's numerals are cut at.
const WEIGHT: f32 = 700.0;

/// One figure in ems, y down, its origin on the baseline at its left edge.
struct Figure {
    outline: BezPath,
    advance: f64,
}

fn figures() -> Option<&'static [Figure; 10]> {
    static CACHE: OnceLock<Option<[Figure; 10]>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            let mut face = Face::parse(kit::inter_font(), 0).ok()?;
            face.set_variation(Tag::from_bytes(b"wght"), WEIGHT);
            Some(std::array::from_fn(|digit| figure(&face, digit)))
        })
        .as_ref()
}

fn figure(face: &Face<'_>, digit: usize) -> Figure {
    let units = f64::from(face.units_per_em());
    let mut pen = Pen {
        path: BezPath::new(),
        units,
    };
    let glyph = char::from_digit(digit as u32, 10).and_then(|figure| face.glyph_index(figure));
    let advance = glyph.and_then(|glyph| {
        face.outline_glyph(glyph, &mut pen);
        face.glyph_hor_advance(glyph)
    });
    Figure {
        outline: pen.path,
        advance: advance.map_or(0.0, |advance| f64::from(advance) / units),
    }
}

/// The decimal digits of `value`, most significant first.
pub(super) fn digits(value: u8) -> impl Iterator<Item = usize> {
    let mut place = match value {
        100.. => 100,
        10.. => 10,
        _ => 1,
    };
    std::iter::from_fn(move || {
        (place > 0).then(|| {
            let digit = value / place % 10;
            place /= 10;
            usize::from(digit)
        })
    })
}

/// `value` set in ems, y down, starting at the origin's baseline.
pub(super) fn reading(value: u8) -> BezPath {
    let mut path = BezPath::new();
    let Some(figures) = figures() else {
        return path;
    };
    let mut pen_x = 0.0;
    for figure in digits(value).map(|digit| &figures[digit]) {
        let shift = Affine::translate((pen_x, 0.0));
        path.extend(
            figure
                .outline
                .elements()
                .iter()
                .map(|&element| shift * element),
        );
        pen_x += figure.advance;
    }
    path
}

struct Pen {
    path: BezPath,
    units: f64,
}

impl Pen {
    fn point(&self, x: f32, y: f32) -> Point {
        Point::new(f64::from(x) / self.units, -f64::from(y) / self.units)
    }
}

impl OutlineBuilder for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path.move_to(self.point(x, y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.path.line_to(self.point(x, y));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.path.quad_to(self.point(x1, y1), self.point(x, y));
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.path
            .curve_to(self.point(x1, y1), self.point(x2, y2), self.point(x, y));
    }

    fn close(&mut self) {
        self.path.close_path();
    }
}

#[cfg(test)]
mod tests {
    use kurbo::Shape;

    use super::*;

    #[test]
    fn digits_read_most_significant_first() {
        assert_eq!(digits(7).collect::<Vec<_>>(), [7]);
        assert_eq!(digits(42).collect::<Vec<_>>(), [4, 2]);
        assert_eq!(digits(100).collect::<Vec<_>>(), [1, 0, 0]);
    }

    #[test]
    fn a_reading_sits_on_its_baseline_one_figure_after_another() {
        let one = reading(8).bounding_box();
        let three = reading(188).bounding_box();
        assert!(one.height() > 0.6 && one.y1.abs() < 0.05, "{one:?}");
        assert!(three.width() > one.width() * 2.5, "{three:?}");
    }
}

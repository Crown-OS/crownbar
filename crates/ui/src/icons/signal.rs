//! Signal bars: four rising columns, lit up to the strength and dim past it,
//! so the meter keeps its silhouette at any reading.

use crate::scene::{Fill, Scene};
use crownui::prelude::Color;
use kurbo::{Affine, Rect, RoundedRect};

use super::fade;

const BARS: usize = 4;
/// Gap between bars as a share of a bar's width.
const GAP_RATIO: f64 = 0.6;
/// The shortest bar's height as a share of the tallest.
const FLOOR_HEIGHT: f64 = 0.35;
const TRACK_ALPHA: f32 = 0.25;

pub(super) fn draw(scene: &mut Scene<'_>, b: Rect, fg: Color, strength: f32) {
    let lit = (strength.clamp(0.0, 1.0) * BARS as f32).ceil() as usize;
    let width = b.width() / (BARS as f64 + (BARS - 1) as f64 * GAP_RATIO);
    let step = width * (1.0 + GAP_RATIO);
    let rise = (1.0 - FLOOR_HEIGHT) / (BARS - 1) as f64;
    for bar in 0..BARS {
        let height = b.height() * (FLOOR_HEIGHT + rise * bar as f64);
        let x0 = b.x0 + step * bar as f64;
        let column = Rect::new(x0, b.y1 - height, x0 + width, b.y1);
        let color = if bar < lit { fg } else { fade(fg, TRACK_ALPHA) };
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            color,
            None,
            &RoundedRect::from_rect(column, width * 0.5),
        );
    }
}

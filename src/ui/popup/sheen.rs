//! The light on the panel's glass: a gloss pooled under its top edge, a
//! fainter one along its bottom, and a specular rim that is brightest where
//! the light falls on it.

use crownui::{
    ext::{Brush, DrawList, GradientStop, Paint},
    prelude::{Color, CornerRadii, Point, Rect, Size, Styled, View, canvas},
};

use super::PANEL_RADIUS;
use crate::{theme::Glass, ui::state::Bar};

/// How far down the body the top gloss reaches, in logical px.
const GLOSS_DEPTH: f32 = 56.0;
/// Strength of the bottom glow against the top gloss.
const UNDERGLOW: f32 = 0.6;

pub fn sheen(bar: Bar, rim: f32) -> impl View {
    canvas(move |runtime, size, list| {
        let glass = bar.palette.get(runtime).glass;
        paint(list, size, &glass, PANEL_RADIUS, rim);
    })
    .absolute()
    .inset(0.0)
    .pointer_events_none()
}

fn paint(list: &mut DrawList, size: Size, glass: &Glass, radius: f32, rim: f32) {
    if size.width <= rim * 2.0 || size.height <= rim * 2.0 {
        return;
    }
    let clear = glass.gloss.with_alpha_factor(0.0);
    let reach = (GLOSS_DEPTH / size.height).min(0.5);
    let gloss = gradient(
        Point::ZERO,
        Point::new(0.0, size.height),
        &[
            (0.0, glass.gloss),
            (reach, clear),
            (1.0 - reach, clear),
            (1.0, glass.gloss.with_alpha_factor(UNDERGLOW)),
        ],
    );
    list.fill(
        Rect::new(Point::ZERO, size),
        CornerRadii::uniform(radius),
        gloss,
    );
    let half = rim / 2.0;
    let [lit, shaded, far] = glass.rim;
    let edge = gradient(
        Point::ZERO,
        Point::new(size.width, size.height),
        &[(0.0, lit), (0.5, shaded), (1.0, far)],
    );
    list.stroke(
        Rect::new(
            Point::new(half, half),
            Size::new(size.width - rim, size.height - rim),
        ),
        CornerRadii::uniform((radius - half).max(0.0)),
        rim,
        edge,
    );
}

fn gradient(start: Point, end: Point, stops: &[(f32, Color)]) -> Paint {
    Paint {
        brush: Brush::LinearGradient {
            start,
            end,
            stops: stops
                .iter()
                .map(|&(offset, color)| GradientStop { offset, color })
                .collect(),
        },
        blur: None,
    }
}

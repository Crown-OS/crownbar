//! The battery cell: a pill-shaped shell filled to the charge, a terminal nub
//! bent along the shell's rounded end,
//! the percentage knocked out of the fill, and a bolt or a plus riding beside
//! it when the machine is plugged in or saving power.
//!
//! Every state arrives as a float, so nothing here ever switches between two
//! drawings: the fill travels to its new colour, the charge line slides, and
//! the accessory grows out of the terminal. Its advance grows with it, which
//! is what makes the pill widen around a bolt instead of jumping.
//!
//! The numerals are drawn twice, in two inks, and each copy is clipped to one
//! side of the charge line. That is what keeps a reading legible when it
//! straddles the boundary: over the track it is always knocked out to the
//! surface, while over the charge it takes whichever of the surface and the
//! foreground reads better on the status colour the cell is wearing.

use std::sync::OnceLock;

use crownui::{
    ext::{Brush, Paint},
    prelude::Color,
};
use kurbo::{Affine, Arc, BezPath, Cap, Line, Point, Rect, RoundedRect, Shape, Stroke, Vec2};

use crate::theme::{self, Palette, opaque};
use crownbar_widgets::BatteryState;

use super::{fade, glyph, numerals};
use crate::{
    scene::{Fill, Scene},
    util::capsule::capsule_left_of,
};

// Geometry, in units of the body's height — the one number a caller picks.
const BODY_W: f64 = 1.90;
/// Fully rounded ends: a half-height radius makes the shell a pill.
const RADIUS: f64 = 0.5;
const NUB_GAP: f64 = 0.07;
const NUB_W: f64 = 0.15;
/// Angle, in radians, of the shell's right end that the nub follows.
const NUB_SWEEP: f64 = 0.6;
const ACCESSORY_GAP: f64 = 0.12;
/// Room the accessory takes along the pill; each glyph is centred in it at a
/// size of its own, as the two are not the same weight.
const ACCESSORY_W: f64 = 0.72;
const BOLT_H: f64 = 0.68;
const PLUS_H: f64 = 0.72;
/// The cell without its accessory: body, gap, nub.
const SHELL_W: f64 = BODY_W + NUB_GAP + NUB_W;

/// Height of the cell on the bar. A glyph set beside text rather than a square
/// icon, so it takes a height of its own instead of [`super::ICON_BOX`].
pub(super) const BAR_HEIGHT: f64 = 18.0;

/// How much of the foreground the empty track keeps.
const TRACK_ALPHA: f32 = 0.5;
/// Nominal digit size, and the most of the body's width three of them may
/// take before the size is fitted down to them.
const DIGITS_SIZE: f64 = 0.90;
const DIGITS_MAX_W: f64 = 0.78;
/// Inter's bold figures run about this fraction of an em, which sizes a
/// reading by how many figures it has rather than by which ones, so 11 % and
/// 88 % are set at the same size.
const DIGIT_ADVANCE: f64 = 0.62;
/// Paths only get the renderer's 4x MSAA, which stair-steps figures this
/// small; a blur this many logical pixels wide stands in for the coverage a
/// glyph atlas would give them.
const FIGURE_SOFTEN: f64 = 0.5;

/// Smallest an accessory is drawn as it fades in, so it grows out of the
/// terminal rather than materialising at full size.
const ACCESSORY_MIN_SCALE: f64 = 1.0;
const BOLT: &str = "M0.62 0 L0.10 0.58 L0.44 0.58 L0.36 1 L0.90 0.40 L0.55 0.40 Z";
const PLUS_STROKE: f64 = 0.18;

/// Advance of the whole glyph, accessory included. The accessory's share
/// tracks the springs, so a pill widens as a bolt arrives.
pub(super) fn advance(state: BatteryState) -> f32 {
    let accessory = state.charging.max(state.saver).clamp(0.0, 1.0) as f64;
    (BAR_HEIGHT * (SHELL_W + accessory * (ACCESSORY_GAP + ACCESSORY_W))) as f32
}

/// The cell on its own, monochrome and without a reading — a device row's
/// battery, where the percentage is a column of its own.
pub(super) fn draw(scene: &mut Scene<'_>, b: Rect, fg: Color, state: BatteryState) {
    let height = b.height().min(b.width() / SHELL_W);
    let left = b.x0 + (b.width() - SHELL_W * height) * 0.5;
    Cell::new(left, b.center().y, height).shell(scene, state.level, fg, fade(fg, TRACK_ALPHA));
}

/// The bar's battery: shell, charge, reading and accessory, from `origin`, the
/// glyph's left edge and vertical centre.
pub(super) fn draw_bar(
    scene: &mut Scene<'_>,
    origin: Point,
    state: BatteryState,
    fg: Color,
    p: &Palette,
) {
    let cell = Cell::new(origin.x, origin.y, BAR_HEIGHT);
    let charge = charge_color(state, fg, p);
    cell.shell(scene, state.level, charge, fade(fg, TRACK_ALPHA));
    cell.reading(scene, state, [ink_on(charge, fg, p), opaque(p.bar_fill)]);
    cell.accessory(scene, state, fg);
}

/// The shell's geometry for one placement: everything below works from this
/// rather than recomputing the body from the bounds.
struct Cell {
    body: RoundedRect,
    nub: Arc,
    height: f64,
    cy: f64,
}

impl Cell {
    fn new(left: f64, cy: f64, height: f64) -> Self {
        let radius = RADIUS * height;
        let nub_radius = radius + (NUB_GAP + NUB_W * 0.5) * height;
        Self {
            body: RoundedRect::new(
                left,
                cy - height * 0.5,
                left + BODY_W * height,
                cy + height * 0.5,
                radius,
            ),
            nub: Arc::new(
                Point::new(left + BODY_W * height - radius, cy),
                Vec2::new(nub_radius, nub_radius),
                -NUB_SWEEP * 0.5,
                NUB_SWEEP,
                0.0,
            ),
            height,
            cy,
        }
    }

    /// Where the charge stops, in surface x.
    fn charge_edge(&self, level: f32) -> f64 {
        let body = self.body.rect();
        body.x0 + body.width() * level.clamp(0.0, 1.0) as f64
    }

    /// Track, nub, and the charge clipped to the body — so the charge line is
    /// square while the far end keeps the shell's radius.
    fn shell(&self, scene: &mut Scene<'_>, level: f32, charge: Color, track: Color) {
        scene.fill(Fill::NonZero, Affine::IDENTITY, track, None, &self.body);
        let nub = Stroke::new(NUB_W * self.height).with_caps(Cap::Round);
        scene.stroke(&nub, Affine::IDENTITY, track, None, &self.nub);
        let body = self.body.rect();
        let edge = self.charge_edge(level);
        if edge <= body.x0 {
            return;
        }
        let filled = capsule_left_of(body, edge);
        scene.fill(Fill::NonZero, Affine::IDENTITY, charge, None, &filled);
    }

    /// The percentage, centred on its own ink: over the charge in the first of
    /// `inks`, over the empty track in the second.
    fn reading(&self, scene: &mut Scene<'_>, state: BatteryState, inks: [Color; 2]) {
        let figures = numerals::reading(state.readout);
        let body = self.body.rect();
        let edge = self.charge_edge(state.level);
        let size = self.reading_size(state.readout);
        let transform = self.reading_transform(&figures, size);
        let blur = Some((FIGURE_SOFTEN / size) as f32);
        for (ink, (from, to)) in inks.into_iter().zip([(body.x0, edge), (edge, body.x1)]) {
            if to > from {
                let paint = Paint {
                    brush: Brush::Solid(ink),
                    blur,
                };
                scene.clipped(Rect::new(from, body.y0, to, body.y1), |scene| {
                    scene.fill(Fill::NonZero, transform, paint, None, &figures);
                });
            }
        }
    }

    /// Digit size for `value`, by how many figures it has.
    fn reading_size(&self, value: u8) -> f64 {
        let count = numerals::digits(value).count() as f64;
        (self.height * DIGITS_SIZE)
            .min(self.body.rect().width() * DIGITS_MAX_W / (count * DIGIT_ADVANCE))
    }

    /// Scales `figures` to `size` and centres their ink on the body.
    fn reading_transform(&self, figures: &BezPath, size: f64) -> Affine {
        let body = self.body.rect();
        let ink = figures.bounding_box().center();
        Affine::translate(body.center().to_vec2() - ink.to_vec2() * size) * Affine::scale(size)
    }

    /// The bolt, or the plus that stands for a saving profile. Plugging in
    /// wins: a machine that is charging says so first.
    fn accessory(&self, scene: &mut Scene<'_>, state: BatteryState, fg: Color) {
        let bolt = state.charging.clamp(0.0, 1.0);
        let plus = (state.saver * (1.0 - bolt)).clamp(0.0, 1.0);
        let shell_right = self.body.rect().x1 + (NUB_GAP + NUB_W) * self.height;
        let cx = shell_right + (ACCESSORY_GAP + ACCESSORY_W * 0.5) * self.height;
        let box_of = |size: f64| {
            let half = size * self.height * 0.5;
            Rect::new(cx - half, self.cy - half, cx + half, self.cy + half)
        };
        if bolt > 0.0 {
            let path = bolt_path();
            let (transform, _) = glyph::fit(path.bounding_box(), box_of(BOLT_H * grow(bolt)), 1.0);
            scene.fill(Fill::NonZero, transform, fade(fg, bolt), None, path);
        }
        if plus > 0.0 {
            let (transform, _) = glyph::fit(UNIT, box_of(PLUS_H * grow(plus)), 1.0);
            let stroke = Stroke::new(PLUS_STROKE).with_caps(Cap::Round);
            for arm in [
                Line::new(Point::new(0.5, 0.1), Point::new(0.5, 0.9)),
                Line::new(Point::new(0.1, 0.5), Point::new(0.9, 0.5)),
            ] {
                scene.stroke(&stroke, transform, fade(fg, plus), None, &arm);
            }
        }
    }
}

const UNIT: Rect = Rect::new(0.0, 0.0, 1.0, 1.0);

fn bolt_path() -> &'static BezPath {
    static PATH: OnceLock<BezPath> = OnceLock::new();
    PATH.get_or_init(|| BezPath::from_svg(BOLT).unwrap_or_default())
}

fn grow(alpha: f32) -> f64 {
    ACCESSORY_MIN_SCALE + (1.0 - ACCESSORY_MIN_SCALE) * alpha as f64
}

/// The fill, pulled toward each status the state is in. Saver is applied last
/// because a machine held at low power says so even while it charges.
fn charge_color(state: BatteryState, fg: Color, p: &Palette) -> Color {
    let low = theme::lerp(fg, p.danger, state.low);
    let charging = theme::lerp(low, p.success, state.charging);
    theme::lerp(charging, p.warning, state.saver)
}

/// Whichever of the foreground and the surface reads better on `under` — the
/// numerals are a knockout, and which way round that goes depends on the mode
/// and on which status colour the cell is currently wearing.
fn ink_on(under: Color, fg: Color, p: &Palette) -> Color {
    let surface = opaque(p.bar_fill);
    let distance = |c: Color| (luma(under) - luma(c)).abs();
    if distance(surface) >= distance(fg) {
        surface
    } else {
        fg
    }
}

fn luma(c: Color) -> f32 {
    0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reading_is_centred_on_the_body() {
        let cell = Cell::new(0.0, 0.0, BAR_HEIGHT);
        let centre = cell.body.rect().center();
        for value in [1, 7, 42, 88, 100] {
            let figures = numerals::reading(value);
            let transform = cell.reading_transform(&figures, cell.reading_size(value));
            let ink = (transform * figures).bounding_box();
            assert!((ink.center() - centre).hypot() < 1e-9, "{value}: {ink:?}");
            assert!(
                cell.body.rect().contains_rect(ink),
                "{value} spills: {ink:?}"
            );
        }
    }
}

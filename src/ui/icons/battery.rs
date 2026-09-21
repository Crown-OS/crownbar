//! The battery cell: a rounded shell filled to the charge, a terminal nub,
//! the percentage knocked out of the fill, and a bolt or a plus riding beside
//! it when the machine is plugged in or saving power.
//!
//! Every state arrives as a float, so nothing here ever switches between two
//! drawings: the fill travels to its new colour, the charge line slides, and
//! the accessory grows out of the terminal. Its advance grows with it, which
//! is what makes the pill widen around a bolt instead of jumping.
//!
//! The numerals are shaped twice, in two inks, and each copy is clipped to one
//! side of the charge line. That is what keeps a reading legible when it
//! straddles the boundary — the half over a bright fill is dark, the half over
//! the empty track is light — in either palette, without naming a colour.

use std::sync::OnceLock;

use crownshell::{Text, TextContext, TextStyle};
use vello::{
    kurbo::{Affine, BezPath, Cap, Line, Point, Rect, RoundedRect, Shape, Stroke},
    peniko::{Color, Fill},
    Scene,
};

use crate::{
    theme::{self, Palette},
    widgets::BatteryState,
};

use super::{fade, glyph};

// Geometry, in units of the body's height — the one number a caller picks.
const BODY_W: f64 = 1.81;
/// Fully rounded ends: a half-height radius makes the shell a pill.
const RADIUS: f64 = 0.5;
const NUB_GAP: f64 = 0.09;
const NUB_W: f64 = 0.14;
const NUB_H: f64 = 0.46;
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
pub(super) const BAR_HEIGHT: f64 = 16.0;

/// How much of the foreground the empty track keeps.
const TRACK_ALPHA: f32 = 0.36;
/// Nominal digit size, and the most of the body's width three of them may
/// take before the size is fitted down to them.
const DIGITS_SIZE: f64 = 0.90;
const DIGITS_MAX_W: f64 = 0.78;
const DIGITS_WEIGHT: f32 = 700.0;

/// Smallest an accessory is drawn as it fades in, so it grows out of the
/// terminal rather than materialising at full size.
const ACCESSORY_MIN_SCALE: f64 = 0.6;
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
pub(super) fn draw(scene: &mut Scene, b: Rect, fg: Color, state: BatteryState) {
    let height = b.height().min(b.width() / SHELL_W);
    let left = b.x0 + (b.width() - SHELL_W * height) * 0.5;
    Cell::new(left, b.center().y, height).shell(scene, state.level, fg, fade(fg, TRACK_ALPHA));
}

/// The bar's battery, which prints its reading inside the cell.
///
/// Retained because the two shaped runs are the expensive part and the reading
/// changes once a percent, not once a frame.
pub struct Readout {
    runs: [Text; 2],
    shown: Option<u8>,
}

impl Readout {
    pub fn new() -> Self {
        Self {
            runs: std::array::from_fn(|_| Text::styled("", digit_style(0.0))),
            shown: None,
        }
    }

    /// Draw the cell from `origin`, its left edge and vertical centre. `fg` is
    /// the pill's foreground, which the cell fills with until a state pulls it
    /// toward a status colour.
    pub fn draw(
        &mut self,
        scene: &mut Scene,
        tcx: &mut TextContext,
        origin: Point,
        state: BatteryState,
        fg: Color,
        p: &Palette,
    ) {
        let cy = origin.y;
        let cell = Cell::new(origin.x, cy, BAR_HEIGHT);
        let charge = charge_color(state, fg, p);
        let track = fade(fg, TRACK_ALPHA);
        cell.shell(scene, state.level, charge, track);

        self.shape(state.readout, tcx);
        let width = self.runs[0].width(tcx);
        let height = self.runs[0].height(tcx);
        let body = cell.body.rect();
        let origin = Point::new(
            (body.center().x - width * 0.5).round(),
            (cy - height * 0.5).round(),
        );
        // What each half of the reading sits on: the fill on one side, the
        // track composited over the bar on the other.
        let over_track = theme::lerp(opaque(p.bar_fill), fg, TRACK_ALPHA);
        let edge = cell.charge_edge(state.level);
        let halves = [
            (Rect::new(body.x0, cy - BAR_HEIGHT, edge, cy + BAR_HEIGHT), charge),
            (
                Rect::new(edge, cy - BAR_HEIGHT, body.x1, cy + BAR_HEIGHT),
                over_track,
            ),
        ];
        for (run, (clip, under)) in self.runs.iter_mut().zip(halves) {
            if clip.width() <= 0.0 {
                continue;
            }
            tint(run, ink_on(under, fg, p));
            scene.push_clip_layer(Fill::NonZero, Affine::IDENTITY, &clip);
            run.draw(tcx, scene, origin);
            scene.pop_layer();
        }

        cell.accessory(scene, state, fg);
    }

    /// Point both runs at `pct`, at a size that fits the body.
    fn shape(&mut self, pct: u8, tcx: &mut TextContext) {
        if self.shown == Some(pct) {
            return;
        }
        self.shown = Some(pct);
        let reading = pct.to_string();
        for run in &mut self.runs {
            run.set_text(&reading);
        }
        let size = BAR_HEIGHT * DIGITS_SIZE;
        self.resize(size);
        let max = BAR_HEIGHT * BODY_W * DIGITS_MAX_W;
        let width = self.runs[0].width(tcx);
        if width > max {
            self.resize(size * max / width);
        }
    }

    fn resize(&mut self, size: f64) {
        for run in &mut self.runs {
            let color = run.style().color;
            run.set_style(digit_style(size as f32).with_color(color));
        }
    }
}

impl Default for Readout {
    fn default() -> Self {
        Self::new()
    }
}

/// The shell's geometry for one placement: everything below works from this
/// rather than recomputing the body from the bounds.
struct Cell {
    body: RoundedRect,
    nub: RoundedRect,
    height: f64,
    cy: f64,
}

impl Cell {
    fn new(left: f64, cy: f64, height: f64) -> Self {
        let nub_x = left + (BODY_W + NUB_GAP) * height;
        let nub_h = NUB_H * height;
        Self {
            body: RoundedRect::new(
                left,
                cy - height * 0.5,
                left + BODY_W * height,
                cy + height * 0.5,
                RADIUS * height,
            ),
            nub: RoundedRect::new(
                nub_x,
                cy - nub_h * 0.5,
                nub_x + NUB_W * height,
                cy + nub_h * 0.5,
                NUB_W * height * 0.5,
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
    fn shell(&self, scene: &mut Scene, level: f32, charge: Color, track: Color) {
        scene.fill(Fill::NonZero, Affine::IDENTITY, track, None, &self.body);
        scene.fill(Fill::NonZero, Affine::IDENTITY, track, None, &self.nub);
        let body = self.body.rect();
        let edge = self.charge_edge(level);
        if edge <= body.x0 {
            return;
        }
        scene.push_clip_layer(Fill::NonZero, Affine::IDENTITY, &self.body);
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            charge,
            None,
            &Rect::new(body.x0, body.y0, edge, body.y1),
        );
        scene.pop_layer();
    }

    /// The bolt, or the plus that stands for a saving profile. Plugging in
    /// wins: a machine that is charging says so first.
    fn accessory(&self, scene: &mut Scene, state: BatteryState, fg: Color) {
        let bolt = state.charging.clamp(0.0, 1.0);
        let plus = (state.saver * (1.0 - bolt)).clamp(0.0, 1.0);
        let cx = self.nub.rect().x1 + (ACCESSORY_GAP + ACCESSORY_W * 0.5) * self.height;
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
    let [r, g, b, _] = c.components;
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

fn opaque(c: Color) -> Color {
    let [r, g, b, _] = c.components;
    Color::new([r, g, b, 1.0])
}

/// Re-style only on a real change: `Text` re-lays-out when it is handed a
/// style, and the ink is resolved every frame because a theme change fades.
fn tint(run: &mut Text, color: Color) {
    if run.style().color == color {
        return;
    }
    let mut style = run.style().clone();
    style.color = color;
    run.set_style(style);
}

fn digit_style(size: f32) -> TextStyle {
    TextStyle::new(crate::ui::FONT_FAMILY, size)
        .with_weight(DIGITS_WEIGHT)
        .with_line_height(1.0)
}

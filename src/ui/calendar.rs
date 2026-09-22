//! Layout, hit-testing and drawing for a [`Month`] grid.
//!
//! Lives beside [`super::panel`] rather than in it: a month is the one row
//! whose contents are a grid instead of a line, and it carries its own
//! geometry, its own hit targets and fifty retained text runs of its own.
//!
//! Everything here is panel-local, with the origin at the grid row's top-left
//! corner, so the host can translate it the same way it translates every other
//! row.

use crownshell::{Text, TextContext, TextStyle};
use vello::{
    kurbo::{Affine, Circle, Point, Rect},
    peniko::{Color, Fill},
    Scene,
};

use crate::{
    theme::Palette,
    util::calendar::{self, WEEKDAY_INITIALS},
    widgets::{popup::Month, Icon, Rune},
};

use super::{icons, panel};

/// The month name and its two arrows.
const H_TITLE: f32 = 32.0;
/// The column headings.
const H_WEEKDAYS: f32 = 22.0;
/// One week.
const H_WEEK: f32 = 30.0;

/// Diameter of the disc behind today's number.
const TODAY_D: f64 = 26.0;
const ARROW_D: f32 = 13.0;
/// How far apart the two arrows sit, centre to centre.
const ARROW_GAP: f32 = 26.0;
/// The arrows are small; their hit targets are not.
const ARROW_SLOP: f64 = 9.0;

/// The numbers 1 to 31, so a cell's label costs no allocation. The grid is
/// rebuilt every second a panel is open.
const DAY_LABELS: [&str; 32] = [
    "", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16",
    "17", "18", "19", "20", "21", "22", "23", "24", "25", "26", "27", "28", "29", "30", "31",
];

/// Height of a whole grid row, fixed — see [`calendar::WEEKS`].
pub fn height() -> f32 {
    H_TITLE + H_WEEKDAYS + H_WEEK * calendar::WEEKS as f32
}

/// How many text runs a grid needs: the title, the seven headings and the
/// cells. The panel keeps them retained so paging re-shapes only the numbers
/// that actually changed.
pub const RUNS: usize = 1 + 7 + calendar::DAYS;

/// Centre of column `column` within a grid occupying `bounds`.
fn column_x(bounds: Rect, column: usize) -> f64 {
    let left = bounds.x0 + panel::PAD_X as f64;
    let width = bounds.width() - panel::PAD_X as f64 * 2.0;
    left + width * (column as f64 + 0.5) / 7.0
}

/// Centre of the back (`-1`) and forward (`+1`) arrows.
fn arrow_center(bounds: Rect, months: i32) -> Point {
    let right = bounds.x1 - panel::PAD_X as f64 - (ARROW_D * 0.5) as f64;
    let x = if months < 0 { right - ARROW_GAP as f64 } else { right };
    Point::new(x, bounds.y0 + H_TITLE as f64 * 0.5)
}

/// Which arrow `point` lands on, if either. Panel-local.
pub fn arrow_at(bounds: Rect, point: Point) -> Option<i32> {
    [-1, 1].into_iter().find(|&months| {
        Rect::from_center_size(arrow_center(bounds, months), (ARROW_D as f64, ARROW_D as f64))
            .inflate(ARROW_SLOP, ARROW_SLOP)
            .contains(point)
    })
}

/// Point `runs` at what `month` now says. Kept separate from [`draw`] so the
/// panel can measure on the frame it rebuilds rather than the frame it paints.
pub fn sync(month: &Month, runs: &mut Vec<Text>) {
    runs.resize_with(RUNS, || Text::styled("", label_style()));
    set(&mut runs[0], &month.title, title_style());
    for (run, initial) in runs[1..8].iter_mut().zip(WEEKDAY_INITIALS) {
        set(run, initial, heading_style());
    }
    for (run, day) in runs[8..].iter_mut().zip(month.days.iter()) {
        set(run, label_for(day.day), label_style());
    }
}

pub fn draw(
    scene: &mut Scene,
    shift: Affine,
    bounds: Rect,
    month: &Month,
    runs: &mut [Text],
    p: &Palette,
    tcx: &mut TextContext,
) {
    let Some((title, rest)) = runs.split_first_mut() else {
        return;
    };
    let (headings, cells) = rest.split_at_mut(7);

    let title_y = bounds.y0 + H_TITLE as f64 * 0.5;
    draw_left(title, scene, shift * Point::new(bounds.x0 + panel::PAD_X as f64, title_y), p.fg, tcx);
    for months in [-1, 1] {
        let rune = if months < 0 { Rune::ChevronLeft } else { Rune::ChevronRight };
        let center = shift * arrow_center(bounds, months);
        icons::draw_sized(
            scene,
            Icon::Rune(rune),
            center.x as f32,
            center.y as f32,
            ARROW_D,
            p.fg_dim,
            p,
        );
    }

    let heading_y = bounds.y0 + H_TITLE as f64 + H_WEEKDAYS as f64 * 0.5;
    for (column, run) in headings.iter_mut().enumerate() {
        draw_centered(
            run,
            scene,
            shift * Point::new(column_x(bounds, column), heading_y),
            p.fg_dim,
            tcx,
        );
    }

    let grid_top = bounds.y0 + (H_TITLE + H_WEEKDAYS) as f64;
    for (index, (day, run)) in month.days.iter().zip(cells.iter_mut()).enumerate() {
        let center = Point::new(
            column_x(bounds, index % 7),
            grid_top + H_WEEK as f64 * ((index / 7) as f64 + 0.5),
        );
        // Today is a filled disc rather than a tint: it is the one cell a
        // glance is looking for, and a wash reads as "selected" next to the
        // device rows above it.
        if day.today {
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                p.accent.end,
                None,
                &Circle::new(shift * center, TODAY_D * 0.5),
            );
        }
        let color = match (day.today, day.in_month, day.weekend) {
            (true, _, _) => p.fg_on_accent,
            (_, false, _) => fade(p.fg_dim, 0.45),
            (_, true, true) => p.fg_dim,
            (_, true, false) => p.fg,
        };
        draw_centered(run, scene, shift * center, color, tcx);
    }
}

fn label_for(day: u8) -> &'static str {
    DAY_LABELS.get(day as usize).copied().unwrap_or("")
}

fn title_style() -> TextStyle {
    TextStyle::new(panel::FONT, 14.0).with_weight(700.0).with_line_height(1.2)
}

fn heading_style() -> TextStyle {
    TextStyle::new(panel::FONT, 11.0).with_weight(600.0).with_line_height(1.2)
}

fn label_style() -> TextStyle {
    TextStyle::new(panel::FONT, 12.5).with_weight(500.0).with_line_height(1.2)
}

/// As [`super::panel`] does: the metrics are the spec's business, the colour is
/// resolved as it is painted so a theme change cross-fades.
fn set(run: &mut Text, content: &str, style: TextStyle) {
    run.set_text(content);
    let color = run.style().color;
    run.set_style(style.with_color(color));
}

fn styled(run: &mut Text, color: Color) {
    let mut style = run.style().clone();
    if style.color != color {
        style.color = color;
        run.set_style(style);
    }
}

fn draw_left(run: &mut Text, scene: &mut Scene, origin: Point, color: Color, tcx: &mut TextContext) {
    styled(run, color);
    let height = run.height(tcx);
    run.draw(tcx, scene, (origin.x.round(), (origin.y - height * 0.5).round()));
}

fn draw_centered(
    run: &mut Text,
    scene: &mut Scene,
    center: Point,
    color: Color,
    tcx: &mut TextContext,
) {
    styled(run, color);
    let (width, height) = (run.width(tcx), run.height(tcx));
    run.draw(
        tcx,
        scene,
        (
            (center.x - width * 0.5).round(),
            (center.y - height * 0.5).round(),
        ),
    );
}

fn fade(color: Color, alpha: f32) -> Color {
    let c = color.components;
    Color::new([c[0], c[1], c[2], c[3] * alpha])
}

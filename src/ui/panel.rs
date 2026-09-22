//! Layout and drawing for a popup panel.
//!
//! A [`Panel`] owns one [`PopupSpec`], the retained [`Text`] runs for its rows
//! and the springs for the rows that animate, so reopening or re-hovering a
//! panel costs no text shaping. It is the only place that knows what a row
//! looks like — widgets describe their panel as data and never touch a
//! [`Scene`].
//!
//! The switch and the slider are `crownuikit`'s, by way of
//! [`crate::ui::control`]; everything else is painted from the same palette
//! they are. Colors arrive as a [`Palette`] resolved for the frame, never
//! cached, because a theme change cross-fades.
//!
//! Everything here works in *panel-local* coordinates, with the origin at the
//! panel's top-left corner. The host translates into surface space when it
//! appends the panel's scene, which is also how the open animation is applied
//! without any of this having to know that one is running.

use crownshell::{Text, TextContext, TextStyle};
use vello::{
    kurbo::{Affine, Circle, Point, Rect, RoundedRect, Stroke},
    peniko::{Color, Fill},
    Scene,
};

use crate::{
    animation::Spring,
    theme::Palette,
    widgets::{
        popup::{Item, Row},
        BatteryState, Icon, PopupSpec, Rune,
    },
};

use super::{control, icons};

/// Every panel is this wide, whatever rows it holds.
pub const WIDTH: f32 = 296.0;
pub const RADIUS: f64 = 13.0;

const FONT: &str = "system-ui";
/// Inner horizontal padding of the panel.
const PAD_X: f32 = 14.0;
const PAD_TOP: f32 = 10.0;
const PAD_BOTTOM: f32 = 10.0;
/// How far a row's highlight is inset from the panel edge.
const ROW_INSET: f32 = 5.0;
const ROW_RADIUS: f64 = 7.0;

const H_HEADER: f32 = 32.0;
const H_SLIDER: f32 = 36.0;
const H_SECTION: f32 = 26.0;
const H_ITEM: f32 = 38.0;
const H_ACTION: f32 = 27.0;
const H_SEPARATOR: f32 = 11.0;

const BADGE_D: f32 = 27.0;
const GLYPH_D: f32 = 17.0;
/// Bare (un-badged) rows put their glyph in the same column, smaller.
const PLAIN_GLYPH_D: f32 = 18.0;
const TEXT_GAP: f32 = 10.0;

const SLIDER_ICON_D: f32 = 18.0;
const CHEVRON_D: f32 = 14.0;
const WARNING_D: f32 = 15.0;
const BATTERY_W: f32 = 24.0;
const BATTERY_H: f32 = 13.0;
const TRAILING_GAP: f32 = 6.0;

/// A laid-out panel, ready to hit-test and draw.
pub struct Panel {
    spec: PopupSpec,
    /// Top edge of each row, panel-local, parallel to `spec.rows`.
    tops: Vec<f32>,
    rows: Vec<RowState>,
    height: f32,
}

/// Per-row state the spec does not carry: the retained text runs, and the
/// spring for the rows that animate. Which fields are populated depends on the
/// row's kind; the rest stay `None`.
#[derive(Default)]
struct RowState {
    primary: Option<Text>,
    detail: Option<Text>,
    /// A header switch's on-ness, or a slider's displayed value. Both spring
    /// toward the spec's value, which is what makes a toggle slide and a
    /// volume reading from the audio server glide instead of jumping.
    anim: Option<Spring>,
}

impl Panel {
    pub fn new(spec: PopupSpec, tcx: &mut TextContext) -> Self {
        let mut panel = Self {
            tops: Vec::with_capacity(spec.rows.len()),
            rows: Vec::with_capacity(spec.rows.len()),
            spec,
            height: 0.0,
        };
        panel.rebuild(tcx, true);
        panel
    }

    /// Replace the contents in place, keeping each row's text runs and springs
    /// — a panel that repolls once a second mostly re-renders the same words,
    /// and a value that moved should travel rather than snap.
    pub fn set_spec(&mut self, spec: PopupSpec, tcx: &mut TextContext) {
        self.spec = spec;
        self.rebuild(tcx, false);
    }

    pub fn size(&self) -> (f32, f32) {
        (WIDTH, self.height)
    }

    /// The panel's own rect, given where its top-left corner sits.
    pub fn rect(&self, origin: Point) -> Rect {
        Rect::new(
            origin.x,
            origin.y,
            origin.x + WIDTH as f64,
            origin.y + self.height as f64,
        )
    }

    fn rebuild(&mut self, tcx: &mut TextContext, initial: bool) {
        self.tops.clear();
        // Reuse existing rows positionally: a row keeps its kind across a
        // repoll far more often than not, and `Text`'s setters no-op when
        // nothing changed.
        self.rows
            .resize_with(self.spec.rows.len(), RowState::default);

        let mut y = PAD_TOP;
        for (row, state) in self.spec.rows.iter().zip(self.rows.iter_mut()) {
            self.tops.push(y);
            y += row_height(row);
            state.sync(row, initial);
        }
        self.height = y + PAD_BOTTOM;

        // Measure here so the first frame is off the shaping path.
        for state in self.rows.iter_mut() {
            for text in [state.primary.as_mut(), state.detail.as_mut()]
                .into_iter()
                .flatten()
            {
                let _ = text.size(tcx);
            }
        }
    }

    /// Step every row spring. Returns whether any is still in flight.
    pub fn step(&mut self, dt: f32) -> bool {
        let mut busy = false;
        for state in self.rows.iter_mut() {
            let Some(spring) = state.anim.as_mut() else {
                continue;
            };
            if spring.at_rest() {
                continue;
            }
            spring.step(dt);
            busy |= !spring.at_rest();
        }
        busy
    }

    fn row_rect(&self, index: usize) -> Rect {
        let top = self.tops[index] as f64;
        Rect::new(
            0.0,
            top,
            WIDTH as f64,
            top + row_height(&self.spec.rows[index]) as f64,
        )
    }

    /// Row under a panel-local point, ignoring the ones nothing can be done
    /// with.
    pub fn row_at(&self, point: Point) -> Option<usize> {
        (0..self.spec.rows.len())
            .find(|&i| self.spec.rows[i].is_interactive() && self.row_rect(i).contains(point))
    }

    /// The current state of a header switch, if that row has one.
    pub fn toggle_state(&self, index: usize) -> Option<bool> {
        match self.spec.rows.get(index) {
            Some(Row::Header { toggle, .. }) => *toggle,
            _ => None,
        }
    }

    /// The header switch's hit target, generous enough to be easy to hit.
    pub fn switch_at(&self, point: Point) -> Option<usize> {
        (0..self.spec.rows.len()).find(|&i| {
            self.toggle_state(i).is_some() && self.switch_rect(i).inflate(6.0, 8.0).contains(point)
        })
    }

    /// The slider under a point, with the value that point maps to. The whole
    /// row is a hit target, so a grab does not have to start on the thumb.
    pub fn slider_at(&self, point: Point) -> Option<(usize, f32)> {
        let index = (0..self.spec.rows.len()).find(|&i| {
            matches!(self.spec.rows[i], Row::Slider { .. }) && self.row_rect(i).contains(point)
        })?;
        Some((index, self.slider_value_at(index, point.x)))
    }

    /// Map an x in panel-local space onto a slider's value.
    pub fn slider_value_at(&self, index: usize, x: f64) -> f32 {
        control::value_at(self.slider_span(index), x)
    }

    /// Put a slider where the pointer is, with no spring: mid-drag the thumb
    /// has to be under the finger, not easing toward it.
    pub fn set_slider_value(&mut self, index: usize, value: f32) {
        let value = value.clamp(0.0, 1.0);
        if let Some(Row::Slider { value: slot, .. }) = self.spec.rows.get_mut(index) {
            *slot = value;
        }
        if let Some(spring) = self.rows.get_mut(index).and_then(|s| s.anim.as_mut()) {
            spring.set_target(value);
            spring.snap_to_target();
        }
    }

    fn switch_rect(&self, index: usize) -> Rect {
        let row = self.row_rect(index);
        let x1 = WIDTH as f64 - PAD_X as f64;
        let cy = row.center().y;
        Rect::new(
            x1 - control::TOGGLE_WIDTH,
            cy - control::TOGGLE_HEIGHT / 2.0,
            x1,
            cy + control::TOGGLE_HEIGHT / 2.0,
        )
    }

    /// The band a slider occupies: from just past its icon to the panel's
    /// inner right edge. `control` insets it by a half thumb at each end.
    fn slider_span(&self, index: usize) -> Rect {
        let row = self.row_rect(index);
        Rect::new(
            (PAD_X + SLIDER_ICON_D + TEXT_GAP) as f64,
            row.y0,
            WIDTH as f64 - PAD_X as f64,
            row.y1,
        )
    }

    /// Where a row's hover / selection background goes.
    fn highlight_rect(&self, index: usize) -> Rect {
        let bounds = self.row_rect(index);
        Rect::new(
            ROW_INSET as f64,
            bounds.y0,
            WIDTH as f64 - ROW_INSET as f64,
            bounds.y1,
        )
    }

    // -----------------------------------------------------------------------
    // Drawing
    // -----------------------------------------------------------------------

    /// Encode the panel at `origin`, with `hovered` highlighted.
    pub fn draw(
        &mut self,
        scene: &mut Scene,
        origin: Point,
        hovered: Option<usize>,
        p: &Palette,
        tcx: &mut TextContext,
    ) {
        let panel = self.rect(origin);
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            p.panel_bg,
            None,
            &RoundedRect::from_rect(panel, RADIUS),
        );

        scene.stroke(
            &Stroke::new(1.0),
            Affine::IDENTITY,
            p.panel_rim,
            None,
            &RoundedRect::from_rect(panel.inset(-0.5), RADIUS + 0.5),
        );

        // Lifted out for the pass so a row can hold its own state mutably
        // while still reading the panel's geometry.
        let mut rows = std::mem::take(&mut self.rows);
        let shift = Affine::translate((origin.x, origin.y));
        for (index, state) in rows.iter_mut().enumerate() {
            self.draw_row(scene, shift, index, state, hovered == Some(index), p, tcx);
        }
        self.rows = rows;
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_row(
        &self,
        scene: &mut Scene,
        shift: Affine,
        index: usize,
        state: &mut RowState,
        hovered: bool,
        p: &Palette,
        tcx: &mut TextContext,
    ) {
        let bounds = self.row_rect(index);
        let cy = bounds.center().y;
        // Rows are laid out panel-local and shifted as they are drawn, so one
        // set of geometry serves both hit-testing and painting.
        let at = |x: f64, y: f64| shift * Point::new(x, y);
        let shifted = |r: Rect| shift.transform_rect_bbox(r);

        match &self.spec.rows[index] {
            Row::Separator => {
                let y = cy.round();
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    p.separator,
                    None,
                    &shifted(Rect::new(
                        PAD_X as f64,
                        y,
                        WIDTH as f64 - PAD_X as f64,
                        y + 1.0,
                    )),
                );
            }
            Row::Header { toggle, .. } => {
                if toggle.is_some() {
                    let (on, velocity) = state
                        .anim
                        .as_ref()
                        .map(|s| (s.position, s.velocity))
                        .unwrap_or((0.0, 0.0));
                    let rect = shifted(self.switch_rect(index));
                    control::toggle(scene, rect.x1, rect.center().y, on, velocity, p);
                }
                draw_left(
                    state.primary.as_mut(),
                    scene,
                    at(PAD_X as f64, cy),
                    p.fg,
                    tcx,
                );
            }
            Row::Slider { icon, .. } => {
                let center = at((PAD_X + SLIDER_ICON_D * 0.5) as f64, cy);
                icons::draw_sized(
                    scene,
                    *icon,
                    center.x as f32,
                    center.y as f32,
                    SLIDER_ICON_D,
                    p.fg_muted,
                );
                let value = state.anim.as_ref().map(|s| s.position).unwrap_or(0.0);
                control::slider(scene, shifted(self.slider_span(index)), value, p);
            }
            Row::Section { chevron, .. } => {
                if *chevron {
                    if hovered {
                        draw_highlight(scene, shifted(self.highlight_rect(index)), p.row_hover);
                    }
                    let center = at((WIDTH - PAD_X - CHEVRON_D * 0.5) as f64, cy);
                    draw_rune(scene, center, CHEVRON_D, p.fg_dim, Rune::ChevronRight);
                }
                draw_left(
                    state.primary.as_mut(),
                    scene,
                    at(PAD_X as f64, cy),
                    p.fg_dim,
                    tcx,
                );
            }
            Row::Action { .. } => {
                if hovered {
                    draw_highlight(scene, shifted(self.highlight_rect(index)), p.row_hover);
                }
                draw_left(
                    state.primary.as_mut(),
                    scene,
                    at(PAD_X as f64, cy),
                    p.fg,
                    tcx,
                );
            }
            Row::Item(item) => self.draw_item(scene, shift, index, item, state, hovered, p, tcx),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_item(
        &self,
        scene: &mut Scene,
        shift: Affine,
        index: usize,
        item: &Item,
        state: &mut RowState,
        hovered: bool,
        p: &Palette,
        tcx: &mut TextContext,
    ) {
        let cy = self.row_rect(index).center().y;
        let shifted = |r: Rect| shift.transform_rect_bbox(r);
        let at = |x: f64, y: f64| shift * Point::new(x, y);

        if item.selected {
            draw_highlight(scene, shifted(self.highlight_rect(index)), p.row_selected);
        } else if hovered && item.enabled {
            draw_highlight(scene, shifted(self.highlight_rect(index)), p.row_hover);
        }

        let mut left = PAD_X;
        if !matches!(item.icon, Icon::None) {
            let center = at((left + BADGE_D * 0.5) as f64, cy);
            let glyph_color = if item.badge {
                let (fill, glyph) = if item.selected {
                    (p.badge_active, p.badge_glyph_active)
                } else {
                    (p.badge_idle, p.fg)
                };
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    fill,
                    None,
                    &Circle::new(center, BADGE_D as f64 * 0.5),
                );
                glyph
            } else {
                p.fg
            };
            let glyph_d = if item.badge { GLYPH_D } else { PLAIN_GLYPH_D };
            icons::draw_sized(
                scene,
                item.icon,
                center.x as f32,
                center.y as f32,
                glyph_d,
                glyph_color,
            );
            left += BADGE_D + TEXT_GAP;
        }

        // Trailing furniture, right to left, so the label learns where to stop.
        let mut right = WIDTH - PAD_X;
        if item.chevron {
            draw_rune(
                scene,
                at((right - CHEVRON_D * 0.5) as f64, cy),
                CHEVRON_D,
                p.fg_dim,
                Rune::ChevronRight,
            );
            right -= CHEVRON_D + TRAILING_GAP;
        }
        if item.warning {
            draw_rune(
                scene,
                at((right - WARNING_D * 0.5) as f64, cy),
                WARNING_D,
                p.warning,
                Rune::Warning,
            );
            right -= WARNING_D + TRAILING_GAP;
        }
        if let Some(level) = item.battery {
            let cell = Rect::new(
                (right - BATTERY_W) as f64,
                cy - BATTERY_H as f64 * 0.5,
                right as f64,
                cy + BATTERY_H as f64 * 0.5,
            );
            icons::draw_in(
                scene,
                Icon::Battery(BatteryState {
                    level,
                    ..Default::default()
                }),
                shifted(cell),
                p.fg_dim,
            );
            right -= BATTERY_W + TRAILING_GAP;
        }
        if let Some(detail) = state.detail.as_mut() {
            detail.set_style(detail_style(p));
            let width = detail.width(tcx) as f32;
            draw_left(
                Some(detail),
                scene,
                at((right - width) as f64, cy),
                p.fg_dim,
                tcx,
            );
            right -= width + TRAILING_GAP;
        }

        // Clamp the label to what is left, so a long device name ellipsizes
        // into the gap instead of running under the battery cell.
        if let Some(text) = state.primary.as_mut() {
            text.set_max_width(Some((right - left).max(0.0) as f64));
            text.set_max_lines(Some(1));
        }
        let color = if item.enabled { p.fg } else { p.fg_dim };
        draw_left(
            state.primary.as_mut(),
            scene,
            at(left as f64, cy),
            color,
            tcx,
        );
    }
}

impl RowState {
    /// Point this row's runs at its current strings, and its spring at its
    /// current value. `initial` snaps rather than animating, so a panel does
    /// not play its toggles and sliders in from zero every time it opens.
    fn sync(&mut self, row: &Row, initial: bool) {
        match row {
            Row::Header { title, toggle } => {
                set(&mut self.primary, title, weight(700.0, 14.0));
                unclamp(&mut self.primary);
                self.detail = None;
                self.retarget(toggle.map(|on| if on { 1.0 } else { 0.0 }), initial);
            }
            Row::Section { title, .. } => {
                set(&mut self.primary, title, weight(600.0, 12.0));
                unclamp(&mut self.primary);
                self.detail = None;
                self.anim = None;
            }
            Row::Action { label } => {
                set(&mut self.primary, label, weight(400.0, 13.0));
                unclamp(&mut self.primary);
                self.detail = None;
                self.anim = None;
            }
            Row::Item(item) => {
                set(&mut self.primary, &item.label, weight(400.0, 13.0));
                match item.detail.as_deref() {
                    Some(detail) => set(&mut self.detail, detail, weight(400.0, 12.5)),
                    None => self.detail = None,
                }
                self.anim = None;
            }
            Row::Slider { value, .. } => {
                self.primary = None;
                self.detail = None;
                self.retarget(Some(*value), initial);
            }
            Row::Separator => {
                self.primary = None;
                self.detail = None;
                self.anim = None;
            }
        }
    }

    fn retarget(&mut self, value: Option<f32>, initial: bool) {
        let Some(value) = value else {
            self.anim = None;
            return;
        };
        match self.anim.as_mut() {
            Some(spring) => {
                spring.set_target(value);
                if initial {
                    spring.snap_to_target();
                }
            }
            None => self.anim = Some(Spring::new(value)),
        }
    }
}

/// Draw a run with its left edge at `origin.x`, vertically centred on
/// `origin.y` — how every row in a panel places its text.
///
/// The color is applied here rather than at build time because a theme change
/// cross-fades: the palette is different on every frame of the transition, and
/// `Text` only re-lays-out when the style it is handed actually differs.
fn draw_left(
    text: Option<&mut Text>,
    scene: &mut Scene,
    origin: Point,
    color: Color,
    tcx: &mut TextContext,
) {
    let Some(text) = text else {
        return;
    };
    let mut style = text.style().clone();
    if style.color != color {
        style.color = color;
        text.set_style(style);
    }
    let height = text.height(tcx);
    text.draw(
        tcx,
        scene,
        (origin.x.round(), (origin.y - height * 0.5).round()),
    );
}

fn draw_rune(scene: &mut Scene, center: Point, size: f32, color: Color, rune: Rune) {
    icons::draw_sized(
        scene,
        Icon::Rune(rune),
        center.x as f32,
        center.y as f32,
        size,
        color,
    );
}

fn draw_highlight(scene: &mut Scene, rect: Rect, color: Color) {
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        color,
        None,
        &RoundedRect::from_rect(rect, ROW_RADIUS),
    );
}

fn row_height(row: &Row) -> f32 {
    match row {
        Row::Header { .. } => H_HEADER,
        Row::Slider { .. } => H_SLIDER,
        Row::Section { .. } => H_SECTION,
        Row::Item(_) => H_ITEM,
        Row::Separator => H_SEPARATOR,
        Row::Action { .. } => H_ACTION,
    }
}

/// A run's metrics. The color is not part of it — see [`draw_left`].
fn weight(weight: f32, size: f32) -> TextStyle {
    TextStyle::new(FONT, size)
        .with_weight(weight)
        .with_line_height(1.2)
}

fn detail_style(p: &Palette) -> TextStyle {
    weight(600.0, 12.5).with_color(p.fg_dim)
}

/// Point a slot's run at `content`. `Text`'s setters no-op when nothing
/// changed, so a panel that repolls the same values never re-shapes.
fn set(slot: &mut Option<Text>, content: &str, style: TextStyle) {
    match slot.as_mut() {
        Some(text) => {
            text.set_text(content);
            // Keep whatever color the last paint resolved; only the metrics
            // are the spec's business.
            let color = text.style().color;
            text.set_style(style.with_color(color));
        }
        None => *slot = Some(Text::styled(content, style)),
    }
}

/// Only item labels clamp their width, and they do it as they draw. Clear the
/// clamp on every other kind of row, in case the run was one before.
fn unclamp(slot: &mut Option<Text>) {
    if let Some(text) = slot.as_mut() {
        text.set_max_width(None);
        text.set_max_lines(None);
    }
}

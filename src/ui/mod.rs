pub mod backdrop;
pub mod control;
mod icons;
pub mod panel;
mod pill;
#[cfg(test)]
mod preview;
mod shapes;

use crownshell::{Text, TextContext, TextStyle};
use vello::{kurbo::Point, peniko::Color, Scene};

use crate::{
    theme::{self, Palette},
    widgets::{Icon, WidgetRegistry, WidgetSlot},
};

const ICON_LABEL_GAP: f32 = 6.0;
pub(super) const FONT_FAMILY: &str = "system-ui";

struct Measured {
    idx: usize,
    width: f32,
    slot: WidgetSlot,
}

pub struct BarPainter {
    labels: Vec<Text>,
    /// The battery's shaped reading, retained across frames the way the pill
    /// labels are.
    battery: icons::BatteryReadout,
}

impl BarPainter {
    pub fn new() -> Self {
        Self {
            labels: Vec::new(),
            battery: icons::BatteryReadout::new(),
        }
    }

    /// Place every widget's pill. `size` is the bar's own, which on the live
    /// surface is shorter than the surface — see [`backdrop`].
    pub fn layout_widgets(
        &mut self,
        registry: &mut WidgetRegistry,
        size: (u32, u32),
        tcx: &mut TextContext,
    ) {
        let (bar_w, bar_h) = size;
        let width = bar_w as f32;
        let height = bar_h as f32;
        let pill_h = (height + theme::PILL_PAD_Y).max(0.0);
        let pill_y = theme::PILL_PAD_Y;

        self.sync_labels(registry);

        let mut measured: Vec<Measured> = Vec::with_capacity(registry.widgets.len());
        for (i, rt) in registry.widgets.iter().enumerate() {
            if !rt.widget.visible() {
                continue;
            }
            let label = rt.widget.label();
            let has_icon = !matches!(rt.widget.icon(), Icon::None);
            if label.is_empty() && !has_icon {
                continue;
            }
            let text_w = if label.is_empty() {
                0.0
            } else {
                let text = &mut self.labels[i];
                text.set_text(&label);
                text.width(tcx) as f32
            };
            let mut inner = 0.0;
            if has_icon {
                inner += icons::advance(rt.widget.icon());
            }
            if has_icon && !label.is_empty() {
                inner += ICON_LABEL_GAP;
            }
            inner += text_w;
            measured.push(Measured {
                idx: i,
                width: inner + 2.0 * theme::PILL_PAD_X,
                slot: rt.widget.slot(),
            });
        }

        for rt in registry.widgets.iter_mut() {
            rt.bounds = None;
        }

        place_slot(
            &measured,
            WidgetSlot::Left,
            registry,
            pill_y,
            pill_h,
            |_| theme::BAR_PAD_X,
        );
        place_slot(
            &measured,
            WidgetSlot::Center,
            registry,
            pill_y,
            pill_h,
            |total| (width - total) * 0.5,
        );
        place_slot(
            &measured,
            WidgetSlot::Right,
            registry,
            pill_y,
            pill_h,
            |total| width - theme::BAR_PAD_X - total,
        );
    }

    /// Encode the bar. `active` is the widget whose popup is open, if any —
    /// its pill stays lit for as long as the panel is up.
    pub fn build_scene(
        &mut self,
        scene: &mut Scene,
        registry: &WidgetRegistry,
        active: Option<usize>,
        size: (u32, u32),
        p: &Palette,
        tcx: &mut TextContext,
    ) {
        let width = size.0 as f32;
        let height = size.1 as f32;

        backdrop::fill(scene, width, height, p.bar_fill);

        for (i, rt) in registry.widgets.iter().enumerate() {
            let Some((x, y, w, h)) = rt.bounds else {
                continue;
            };
            let hover = rt.hover.position.clamp(0.0, 1.0);
            // An open panel pins its pill on: the hover spring alone would
            // let it fade out as soon as the pointer moved onto the panel.
            let (fill, lit) = if active == Some(i) {
                (p.pill_active, 1.0)
            } else {
                (p.pill_hover, hover)
            };
            pill::draw(scene, x, y, w, h, lit, fill);

            let icon = rt.widget.icon();
            let label = rt.widget.label();
            let fg = theme::lerp(p.bar_fg, p.bar_fg_hover, lit);
            let cy = y + h * 0.5;

            let mut cursor = x + theme::PILL_PAD_X;
            if !matches!(icon, Icon::None) {
                let advance = icons::advance(icon);
                match icon {
                    // The battery prints its reading inside its own cell, so it
                    // is the one glyph that needs the text context.
                    Icon::Battery(state) => self.battery.draw(
                        scene,
                        tcx,
                        Point::new(cursor as f64, cy as f64),
                        state,
                        fg,
                        p,
                    ),
                    _ => icons::draw(scene, icon, cursor + advance * 0.5, cy, fg),
                }
                cursor += advance;
                if !label.is_empty() {
                    cursor += ICON_LABEL_GAP;
                }
            }
            if !label.is_empty() {
                let text = &mut self.labels[i];
                text.set_style(label_style(fg));
                // `Text` draws from its top-left corner; center it on `cy`.
                let top = cy as f64 - text.height(tcx) * 0.5;
                text.draw(tcx, scene, (cursor as f64, top));
            }
        }
    }

    /// Keep one retained `Text` per widget index.
    fn sync_labels(&mut self, registry: &WidgetRegistry) {
        while self.labels.len() < registry.widgets.len() {
            // The color is resolved every paint; this is only a seed.
            self.labels
                .push(Text::styled("", label_style(Color::TRANSPARENT)));
        }
        self.labels.truncate(registry.widgets.len());
    }
}

impl Default for BarPainter {
    fn default() -> Self {
        Self::new()
    }
}

fn label_style(color: Color) -> TextStyle {
    TextStyle::new(FONT_FAMILY, theme::FONT_SIZE)
        .with_weight(theme::FONT_WEIGHT)
        .with_line_height(1.2)
        .with_color(color)
}

fn place_slot<F: Fn(f32) -> f32>(
    measured: &[Measured],
    slot: WidgetSlot,
    registry: &mut WidgetRegistry,
    y: f32,
    h: f32,
    start_x_for: F,
) {
    let entries: Vec<&Measured> = measured.iter().filter(|m| m.slot == slot).collect();
    if entries.is_empty() {
        return;
    }
    let total: f32 = entries.iter().map(|m| m.width).sum::<f32>()
        + theme::WIDGET_GAP * (entries.len().saturating_sub(1) as f32);
    let mut x = start_x_for(total);
    for entry in entries {
        registry.widgets[entry.idx].bounds = Some((x, y, entry.width, h));
        x += entry.width + theme::WIDGET_GAP;
    }
}

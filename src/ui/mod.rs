mod icons;
mod pill;

use crownshell::{Text, TextContext, TextStyle};
use vello::{
    kurbo::{Affine, Rect},
    peniko::{Color, Fill},
    Scene,
};

use crate::{
    theme::THEME,
    widgets::{Icon, WidgetRegistry, WidgetSlot},
};

const ICON_LABEL_GAP: f32 = 6.0;
const FONT_FAMILY: &str = "system-ui";

struct Measured {
    idx: usize,
    width: f32,
    slot: WidgetSlot,
}

pub struct BarPainter {
    labels: Vec<Text>,
}

impl BarPainter {
    pub fn new() -> Self {
        Self { labels: Vec::new() }
    }

    pub fn layout_widgets(
        &mut self,
        registry: &mut WidgetRegistry,
        size: (u32, u32),
        tcx: &mut TextContext,
    ) {
        let (surface_w, surface_h) = size;
        let width = surface_w as f32;
        let height = surface_h as f32;
        let pill_h = (height + THEME.pill_pad_y).max(0.0);
        let pill_y = THEME.pill_pad_y;

        self.sync_labels(registry);

        let mut measured: Vec<Measured> = Vec::with_capacity(registry.widgets.len());
        for (i, rt) in registry.widgets.iter().enumerate() {
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
                inner += icons::ICON_BOX;
            }
            if has_icon && !label.is_empty() {
                inner += ICON_LABEL_GAP;
            }
            inner += text_w;
            measured.push(Measured {
                idx: i,
                width: inner + 2.0 * THEME.pill_pad_x,
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
            |_| THEME.bar_pad_x,
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
            |total| width - THEME.bar_pad_x - total,
        );
    }

    pub fn build_scene(
        &mut self,
        scene: &mut Scene,
        registry: &WidgetRegistry,
        size: (u32, u32),
        tcx: &mut TextContext,
    ) {
        let width = size.0 as f32;
        let height = size.1 as f32;

        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            THEME.bar_tint,
            None,
            &Rect::new(0.0, 0.0, width as f64, height as f64),
        );

        for (i, rt) in registry.widgets.iter().enumerate() {
            let Some((x, y, w, h)) = rt.bounds else {
                continue;
            };
            let hover = rt.hover.position.clamp(0.0, 1.0);
            pill::draw(scene, x, y, w, h, hover, THEME.pill_hover, THEME.pill_rim);

            let icon = rt.widget.icon();
            let label = rt.widget.label();
            let fg = lerp_color(THEME.fg_muted, THEME.fg, hover);
            let cy = y + h * 0.5;

            let mut cursor = x + THEME.pill_pad_x;
            if !matches!(icon, Icon::None) {
                let icon_cx = cursor + icons::ICON_BOX * 0.5;
                icons::draw(scene, icon, icon_cx, cy, fg);
                cursor += icons::ICON_BOX;
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
            self.labels
                .push(Text::styled("", label_style(THEME.fg_muted)));
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
    TextStyle::new(FONT_FAMILY, THEME.font_size)
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
        + THEME.widget_gap * (entries.len().saturating_sub(1) as f32);
    let mut x = start_x_for(total);
    for entry in entries {
        registry.widgets[entry.idx].bounds = Some((x, y, entry.width, h));
        x += entry.width + THEME.widget_gap;
    }
}

fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let ac = a.components;
    let bc = b.components;
    Color::new([
        ac[0] + (bc[0] - ac[0]) * t,
        ac[1] + (bc[1] - ac[1]) * t,
        ac[2] + (bc[2] - ac[2]) * t,
        ac[3] + (bc[3] - ac[3]) * t,
    ])
}

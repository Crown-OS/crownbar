//! Procedural icon rendering. Each icon is sized to a 16 × 16 logical box;
//! the renderer hands us a center point and the box width, we draw vectors
//! directly into the Vello scene.
//!
//! Several icons accept smoothly-interpolated state floats (∈ [0, 1]) so the
//! widget can spring a value between two extremes (e.g. tiled ↔ floating,
//! battery normal ↔ battery saver) and the icon morphs continuously.

mod battery;
mod bluetooth;
mod brightness;
mod layout;
mod volume;
mod wifi;

use vello::{kurbo::Rect, peniko::Color, Scene};

use crate::widgets::Icon;

pub const ICON_BOX: f32 = 22.0;

/// Dispatch + draw one icon, centered at (cx, cy). `fg` is the foreground
/// stroke/fill color; the icon picks accent colors from it.
pub fn draw(scene: &mut Scene, icon: Icon, cx: f32, cy: f32, fg: Color) {
    let size = ICON_BOX;
    let bounds = Rect::new(
        (cx - size * 0.5) as f64,
        (cy - size * 0.5) as f64,
        (cx + size * 0.5) as f64,
        (cy + size * 0.5) as f64,
    );
    match icon {
        Icon::None => {}
        Icon::Wifi { strength } => wifi::draw(scene, bounds, fg, strength),
        Icon::Bluetooth { on } => bluetooth::draw(scene, bounds, fg, on),
        Icon::Volume { level, muted } => volume::draw(scene, bounds, fg, level, muted),
        Icon::Brightness { level } => brightness::draw(scene, bounds, fg, level),
        Icon::Battery {
            pct,
            charging,
            saver,
        } => battery::draw(scene, bounds, fg, pct, charging, saver),
        Icon::Layout { tiled } => layout::draw(scene, bounds, fg, tiled),
    }
}

pub(super) fn fade(c: Color, alpha: f32) -> Color {
    let comp = c.components;
    Color::new([comp[0], comp[1], comp[2], comp[3] * alpha.clamp(0.0, 1.0)])
}

pub(super) fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

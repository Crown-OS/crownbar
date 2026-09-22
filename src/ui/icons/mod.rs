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
mod caffeine;
mod glyph;
mod layout;
mod notifications;
mod rune;
mod volume;
mod weather;
mod wifi;

use vello::{kurbo::Rect, peniko::Color, Scene};

use crate::{theme::Palette, widgets::Icon};

pub use battery::Readout as BatteryReadout;

pub const ICON_BOX: f32 = 22.0;

/// How much room `icon` takes along a pill. Square for all but the battery,
/// whose accessory grows the glyph as it fades in.
pub fn advance(icon: Icon) -> f32 {
    match icon {
        Icon::Battery(state) => battery::advance(state),
        _ => ICON_BOX,
    }
}

/// Dispatch + draw one icon in the bar's standard [`ICON_BOX`], centered at
/// (cx, cy). `fg` is the foreground stroke/fill color; the icon picks accent
/// colors from it.
pub fn draw(scene: &mut Scene, icon: Icon, cx: f32, cy: f32, fg: Color, p: &Palette) {
    draw_sized(scene, icon, cx, cy, ICON_BOX, fg, p)
}

/// As [`draw`], in a box of `size` rather than [`ICON_BOX`]. The popup panels
/// draw the same icons smaller than the bar does.
pub fn draw_sized(scene: &mut Scene, icon: Icon, cx: f32, cy: f32, size: f32, fg: Color, p: &Palette) {
    let bounds = Rect::new(
        (cx - size * 0.5) as f64,
        (cy - size * 0.5) as f64,
        (cx + size * 0.5) as f64,
        (cy + size * 0.5) as f64,
    );
    draw_in(scene, icon, bounds, fg, p)
}

/// As [`draw`], into an explicit box. Useful where the glyph is not square —
/// the small battery cell on a device row.
pub fn draw_in(scene: &mut Scene, icon: Icon, bounds: Rect, fg: Color, p: &Palette) {
    match icon {
        Icon::None => {}
        Icon::Wifi(state) => wifi::draw(scene, bounds, fg, state),
        Icon::Bluetooth { on } => bluetooth::draw(scene, bounds, fg, on),
        Icon::Caffeine { on } => caffeine::draw(scene, bounds, fg, on),
        Icon::Volume { level, muted } => volume::draw(scene, bounds, fg, level, muted),
        Icon::Brightness { level } => brightness::draw(scene, bounds, fg, level),
        Icon::Battery(state) => battery::draw(scene, bounds, fg, state),
        Icon::Layout { tiled } => layout::draw(scene, bounds, fg, tiled),
        Icon::Notifications { open, silenced } => {
            notifications::draw(scene, bounds, fg, open, silenced)
        }
        Icon::Weather {
            from,
            to,
            blend,
            night,
        } => weather::draw(scene, bounds, from, to, blend, night, &p.weather),
        Icon::Rune(r) => rune::draw(scene, bounds, fg, r),
    }
}

pub(super) fn fade(c: Color, alpha: f32) -> Color {
    let comp = c.components;
    Color::new([comp[0], comp[1], comp[2], comp[3] * alpha.clamp(0.0, 1.0)])
}

pub(super) fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

//! The bar's palette, which is the desktop's palette.
//!
//! No module in this crate names a color but this one. Every slot is derived
//! from crownui's kit [`Theme`] — the same value every other CrownOS app paints
//! from, resolved from `~/.config/crownos/appearance.ron` and `theme.ron` — so
//! the bar follows light/dark mode and the user's accent without being told.
//!
//! The panels' own rows take their colors straight from the kit; what is left
//! here is what the bar paints itself: its ground, its pills, its glyphs, and
//! the body every panel sits on.

use crownconfig::ThemeOverrides;
use crownui::{
    ext::{Brush, GradientStop},
    kit::{self, Theme},
    prelude::{Color, CornerShape, Point},
};

/// Opacity of a popup panel before the user's transparency setting is taken
/// off it. The bar's panels sit on the compositor's blur, and letting some of
/// it through is most of what makes them look like part of the desktop.
const PANEL_OPACITY: f32 = 0.66;
/// Never let the transparency setting take a panel below this — past it the
/// text stops being readable over a bright wallpaper.
const MIN_PANEL_OPACITY: f32 = 0.55;

/// The bar's own foreground, idle then under the pointer, per mode. A panel's
/// text sits on the panel's own body, but the bar's sits over the wallpaper,
/// and the contrast that reads well there is the desktop's choice rather than
/// a derivative of the window palette. `theme.ron` may override them as the
/// `bar.fg` and `bar.fg_hover` tokens.
const BAR_FG_DARK: Color = rgb(0xDD_DD_DD);
const BAR_FG_HOVER_DARK: Color = rgb(0xAA_AA_AA);
const BAR_FG_LIGHT: Color = rgb(0x11_11_11);
const BAR_FG_HOVER_LIGHT: Color = rgb(0x33_33_33);

/// How much stronger the pill under an open panel is than a hovered one.
const PILL_ACTIVE_BOOST: f32 = 2.2;

// -- geometry-free layout tokens ---------------------------------------------

/// Every corner the bar draws — pills, the floating ground, panels and the
/// rows inside them — is the compositor's squircle, so a panel stays
/// concentric with the blur, rim and shadow drawn under it.
pub const CORNER_SHAPE: CornerShape = CornerShape::Squircle;

/// Horizontal inset of the bar contents from the left/right edges.
pub const BAR_PAD_X: f32 = 12.0;
/// Inner padding of a widget pill (left/right).
pub const PILL_PAD_X: f32 = 10.0;
/// Gap between adjacent widgets.
pub const WIDGET_GAP: f32 = 4.0;
/// Font size for widget text.
pub const FONT_SIZE: f32 = 14.0;
/// Font weight for widget text.
pub const FONT_WEIGHT: u16 = 600;

/// Two-stop gradient, projected onto whichever axis a surface catches the
/// light along.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stops {
    pub start: Color,
    pub end: Color,
}

impl Stops {
    const fn of(start: u32, end: u32) -> Self {
        Self {
            start: rgb(start),
            end: rgb(end),
        }
    }

    /// Top→bottom down the column at `x`.
    pub fn vertical(self, x: f64, y0: f64, y1: f64) -> Brush {
        self.between(
            Point::new(x as f32, y0 as f32),
            Point::new(x as f32, y1 as f32),
        )
    }

    fn between(self, start: Point, end: Point) -> Brush {
        Brush::LinearGradient {
            start,
            end,
            stops: [(0.0, self.start), (1.0, self.end)]
                .into_iter()
                .map(|(offset, color)| GradientStop { offset, color })
                .collect(),
        }
    }
}

/// The weather glyphs' own colors: these read as a sun, a cloud and rain
/// rather than as foreground and accent, so they cannot come from the kit's
/// semantic slots.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeatherColors {
    pub sun: Stops,
    pub moon: Stops,
    pub cloud: Stops,
    /// Overcast, and the haze bars behind a night moon.
    pub cloud_dark: Stops,
    /// Rain, snow and wind.
    pub water: Stops,
}

const WEATHER_DARK: WeatherColors = WeatherColors {
    sun: Stops::of(0xFF_C2_4D, 0xFF_7A_18),
    moon: Stops::of(0xFF_D3_7A, 0xFF_A5_2E),
    cloud: Stops::of(0xFF_FF_FF, 0xC2_C7_CF),
    cloud_dark: Stops::of(0xA8_AE_B8, 0x6E_74_7E),
    water: Stops::of(0x7F_B8_F5, 0x3A_82_D9),
};

/// The same hues, darkened where they would otherwise disappear into a light
/// panel — a white cloud on white needs an edge the dark mode does not.
const WEATHER_LIGHT: WeatherColors = WeatherColors {
    sun: Stops::of(0xFF_B0_2E, 0xF0_66_00),
    moon: Stops::of(0xF7_BE_54, 0xE8_8E_10),
    cloud: Stops::of(0xF4_F6_F9, 0xA7_AF_BB),
    cloud_dark: Stops::of(0x9A_A2_AE, 0x5E_65_70),
    water: Stops::of(0x5E_9F_EC, 0x1E_66_C4),
};

/// Every color the bar paints itself, resolved from one theme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    /// The bar's ground at its densest, along the screen edge. The same body
    /// color and transparency as the panels, so the bar and the panels it
    /// opens are one surface in two places.
    pub bar_fill: Color,
    /// Pill fill under the pointer, and while the widget's panel is open.
    pub pill_hover: Color,
    pub pill_active: Color,
    /// Icons and text on the bar, idle and under the pointer.
    pub bar_fg: Color,
    pub bar_fg_hover: Color,
    /// Third-level foreground — the trailing glyphs of a panel row.
    pub fg_dim: Color,
    /// Popup panel body, rim and drop shadow.
    pub panel_bg: Color,
    pub panel_rim: Color,
    pub panel_shadow: Color,
    /// Degraded but working: an open network, a battery held back to save power.
    pub warning: Color,
    /// Healthy: a charging battery.
    pub success: Color,
    /// Failing: a battery about to go flat.
    pub danger: Color,
    pub weather: WeatherColors,
}

impl Palette {
    /// The palette for `theme`, with the user's `transparency` (0..1) taken off
    /// the panels and the bar tokens of `overrides` applied.
    pub fn new(theme: &Theme, transparency: f32, overrides: &ThemeOverrides) -> Self {
        let opacity = panel_opacity(transparency);
        let panel_bg = kit::panel_fill(theme, opacity);
        let dark = theme.mode.is_dark();
        let (bar_fg, bar_fg_hover) = if dark {
            (BAR_FG_DARK, BAR_FG_HOVER_DARK)
        } else {
            (BAR_FG_LIGHT, BAR_FG_HOVER_LIGHT)
        };
        let token = |name: &str, fallback: Color| {
            overrides
                .color(dark, name)
                .map_or(fallback, |[r, g, b, a]| Color::new(r, g, b, a))
        };
        Self {
            bar_fill: panel_bg,
            pill_hover: theme.surface.hover,
            pill_active: scale_alpha(theme.surface.hover, PILL_ACTIVE_BOOST),
            bar_fg: token("bar.fg", bar_fg),
            bar_fg_hover: token("bar.fg_hover", bar_fg_hover),
            fg_dim: theme.popover.muted_text,
            panel_bg,
            panel_rim: kit::panel_rim(theme, opacity),
            panel_shadow: theme.surface.shadow,
            warning: theme.status.warning,
            success: theme.status.success,
            danger: theme.status.danger,
            weather: if theme.mode.is_dark() {
                WEATHER_DARK
            } else {
                WEATHER_LIGHT
            },
        }
    }
}

fn panel_opacity(transparency: f32) -> f32 {
    (PANEL_OPACITY - transparency.clamp(0.0, 1.0)).max(MIN_PANEL_OPACITY)
}

const fn rgb(hex: u32) -> Color {
    Color::from_rgba8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8, 0xFF)
}

/// `color` with its alpha replaced.
pub fn with_alpha(color: Color, alpha: f32) -> Color {
    Color {
        a: alpha.clamp(0.0, 1.0),
        ..color
    }
}

/// `color` with no transparency left in it — for a surface that has to carry
/// its own contrast because the compositor is not blurring what sits behind it.
pub fn opaque(color: Color) -> Color {
    with_alpha(color, 1.0)
}

/// `color` with its alpha multiplied, for a translucent slot shown at more or
/// less than the strength the palette chose.
pub fn scale_alpha(color: Color, factor: f32) -> Color {
    with_alpha(color, color.a * factor)
}

/// Linear interpolation mixed in premultiplied space, so a fade whose
/// endpoints differ in alpha stays monotonic instead of flashing through a
/// dark midpoint.
pub fn lerp(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let alpha = a.a + (b.a - a.a) * t;
    if alpha <= f32::EPSILON {
        return Color::new(0.0, 0.0, 0.0, 0.0);
    }
    let mix = |ca: f32, cb: f32| {
        let (pa, pb) = (ca * a.a, cb * b.a);
        (pa + (pb - pa) * t) / alpha
    };
    Color::new(mix(a.r, b.r), mix(a.g, b.g), mix(a.b, b.b), alpha)
}

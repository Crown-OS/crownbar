//! The bar's palette, which is the desktop's palette.
//!
//! No module in this crate names a color but this one. Every slot is derived
//! from crownui's kit [`Theme`] — the same value every other CrownOS app paints
//! from, resolved from `~/.config/crownos/appearance.ron` and `theme.ron` — so
//! the bar follows light/dark mode and the user's accent without being told.
//!
//! The panels' own rows take their colors straight from the kit; what is left
//! here is what the bar paints itself: its ground, its pills, its glyphs, and
//! the glass every panel is made of.

mod chip;
mod glass;
mod ink;

pub use chip::{ChipLook, Chips, HIGHLIGHT_DROP, boost_opacity};
pub use glass::Glass;
pub use ink::{Behind, Ink, Inks};

use std::ops::Range;

use crownbar_integrations::live::wire::Accent;
use crownconfig::ThemeOverrides;
use crownui::{
    ext::{Brush, GradientStop},
    kit::{Theme, ThemeMode},
    prelude::{Color, CornerShape, Point},
};

/// Opacity of a popup panel before the user's transparency setting is taken
/// off it, in light mode and in dark. The bar's panels sit on the compositor's
/// blur, and letting some of it through is most of what makes them look like
/// part of the desktop; the dark body needs more of itself to stay smoked.
const LIGHT_PANEL_OPACITY: f32 = 0.58;
const DARK_PANEL_OPACITY: f32 = 0.82;
/// Never let the transparency setting take a panel below these — past them the
/// text stops being readable over a bright wallpaper. Each mode keeps the same
/// travel below its own opacity.
const MIN_LIGHT_PANEL_OPACITY: f32 = 0.5;
const MIN_DARK_PANEL_OPACITY: f32 = 0.74;

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
    /// The bar's ground at its densest, along the screen edge. The panels'
    /// glass body, so the bar and the panels it opens are one surface in two
    /// places.
    pub bar_fill: Color,
    /// Pill fill under the pointer, and while the widget's panel is open.
    pub pill_hover: Color,
    pub pill_active: Color,
    /// Icons and text on the bar, idle and under the pointer, as they read
    /// across the whole bar; a pill takes its own from `inks`.
    pub bar_fg: Color,
    pub bar_fg_hover: Color,
    pub inks: Inks,
    /// Both modes' glass bodies, opaque, light then dark: what a glyph cut out
    /// of the bar's ink is knocked out to.
    pub knockouts: [Color; 2],
    /// Live activities' chips, in this mode.
    pub chips: Chips,
    /// Third-level foreground — the trailing glyphs of a panel row.
    pub fg_dim: Color,
    /// What the popup panels are made of, and the theme their rows are drawn in.
    pub glass: Glass,
    pub panel_theme: Theme,
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
    /// the panels, the bar's ink chosen against what is `behind` it, and the bar
    /// tokens of `overrides` applied.
    pub fn new(
        theme: &Theme,
        transparency: f32,
        overrides: &ThemeOverrides,
        behind: Option<Behind>,
    ) -> Self {
        let dark_mode = theme.mode.is_dark();
        let glass = Glass::new(theme, panel_opacity(transparency, dark_mode));
        let inks = Inks::new(behind, glass.body, dark_mode, overrides);
        let opaque_body = |mode| Glass::new(&Theme { mode, ..*theme }, 1.0).body;
        let whole_bar = inks.over(0.0..1.0);
        Self {
            bar_fill: glass.body,
            pill_hover: theme.surface.hover,
            pill_active: scale_alpha(theme.surface.hover, PILL_ACTIVE_BOOST),
            bar_fg: whole_bar.fg,
            bar_fg_hover: whole_bar.fg_hover,
            inks,
            knockouts: [ThemeMode::Light, ThemeMode::Dark].map(opaque_body),
            chips: Chips::new(dark_mode),
            fg_dim: glass.ink_muted,
            glass,
            panel_theme: glass.rows_theme(theme),
            warning: theme.status.warning,
            success: theme.status.success,
            danger: theme.status.danger,
            weather: if dark_mode {
                WEATHER_DARK
            } else {
                WEATHER_LIGHT
            },
        }
    }
}

/// What state a pill is in, for the fill drawn under it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PillState {
    Rest,
    Hovered,
    Active,
}

impl Palette {
    /// `accent` as a solid color.
    pub fn accent(&self, accent: Accent) -> Color {
        chip::hue(accent).unwrap_or(self.bar_fg)
    }

    /// The fill under a pill in `state`. A live activity's chip, tinted or not,
    /// paints its own wash, so it gets none.
    pub fn pill_fill(&self, state: PillState, accent: Option<Accent>) -> Color {
        match (state, accent) {
            (_, Some(_)) | (PillState::Rest, None) => with_alpha(self.pill_hover, 0.0),
            (PillState::Hovered, None) => self.pill_hover,
            (PillState::Active, None) => self.pill_active,
        }
    }

    /// The ink for a pill across `span` of the bar: a tinted chip's own, or
    /// whichever reads over the wallpaper there.
    pub fn pill_ink(&self, span: Range<f32>, accent: Option<Accent>) -> Ink {
        accent
            .and_then(|accent| self.chips.ink(accent))
            .map_or_else(|| self.inks.over(span), Ink::solid)
    }
}

fn panel_opacity(transparency: f32, dark_mode: bool) -> f32 {
    let (opacity, floor) = if dark_mode {
        (DARK_PANEL_OPACITY, MIN_DARK_PANEL_OPACITY)
    } else {
        (LIGHT_PANEL_OPACITY, MIN_LIGHT_PANEL_OPACITY)
    };
    (opacity - transparency.clamp(0.0, 1.0)).max(floor)
}

const fn rgb(hex: u32) -> Color {
    Color::from_rgba8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8, 0xFF)
}

/// `color` with its alpha replaced.
pub const fn with_alpha(color: Color, alpha: f32) -> Color {
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

#[cfg(test)]
mod tests {
    use crownconfig::AccentColor;

    use super::*;
    use crate::wallpaper::{Backdrop, SAMPLES};

    const WHOLE_BAR: Range<f32> = 0.0..1.0;

    fn over_wallpaper(mode: ThemeMode, level: f32) -> Palette {
        let behind = Behind {
            wallpaper: Backdrop([Color::new(level, level, level, 1.0); SAMPLES]),
            ground_strength: 0.0,
        };
        Palette::new(
            &Theme::for_mode(mode, AccentColor::Purple),
            0.0,
            &ThemeOverrides::default(),
            Some(behind),
        )
    }

    #[test]
    fn a_tinted_chip_keeps_its_ink_whatever_the_wallpaper() {
        for (mode, ink) in [
            (ThemeMode::Light, rgb(0x11_11_11)),
            (ThemeMode::Dark, rgb(0xFF_FF_FF)),
        ] {
            for level in [0.05, 0.95] {
                let chip = over_wallpaper(mode, level).pill_ink(WHOLE_BAR, Some(Accent::Blue));
                assert_eq!(
                    (chip.fg, chip.fg_hover),
                    (ink, ink),
                    "{mode:?} over {level}"
                );
            }
        }
    }

    #[test]
    fn untinted_chips_and_plain_pills_read_the_wallpaper() {
        let palette = over_wallpaper(ThemeMode::Dark, 0.95);
        let wallpaper = palette.inks.over(WHOLE_BAR);
        assert_eq!(
            palette.pill_ink(WHOLE_BAR, Some(Accent::Neutral)),
            wallpaper
        );
        assert_eq!(palette.pill_ink(WHOLE_BAR, None), wallpaper);
        assert_eq!(wallpaper.fg, rgb(0x11_11_11));
    }

    #[test]
    fn the_knockouts_are_both_glass_bodies_opaque() {
        let palette = over_wallpaper(ThemeMode::Dark, 0.5);
        assert_eq!(palette.knockouts, [rgb(0xF4_F6_FA), rgb(0x0A_0B_0F)],);
    }
}

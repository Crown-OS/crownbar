//! The bar's ink: whichever of a light and a dark glyph reads better over the
//! wallpaper under a pill, and a shadow that grows as even that one falls
//! short.
//!
//! Each pill is judged over its own stretch of the bar, by its worst contrast
//! there, so a pill over a bright patch can go dark while the rest stay light.

use std::ops::Range;

use crownconfig::ThemeOverrides;
use crownui::prelude::Color;

use super::{rgb, with_alpha};
use crate::{
    util::contrast::{contrast_ratio, over},
    wallpaper::{Backdrop, SAMPLES},
};

/// Light ink, idle then under the pointer, and dark ink likewise. `theme.ron`
/// may override them as `bar.fg` and `bar.fg_hover`: the dark mode's tokens
/// for the light ink, the light mode's for the dark.
const LIGHT: (Color, Color) = (rgb(0xDD_DD_DD), rgb(0xAA_AA_AA));
const DARK: (Color, Color) = (rgb(0x11_11_11), rgb(0x33_33_33));

/// Contrast at which a glyph needs no help: WCAG's AAA level for body text.
const ENOUGH_CONTRAST: f32 = 7.0;
/// The shadow's strength with no contrast left at all.
const MAX_SHADOW_ALPHA: f32 = 0.9;

/// What the bar's glyphs are drawn over: the wallpaper, under the bar's own
/// ground at `ground_strength` of its alpha.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Behind {
    pub wallpaper: Backdrop,
    pub ground_strength: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tone {
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ink {
    pub fg: Color,
    pub fg_hover: Color,
    /// Transparent while the ink holds its own.
    pub shadow: Color,
}

/// Both inks the bar can draw in, and what each stretch of it is drawn over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Inks {
    light: Ink,
    dark: Ink,
    /// The wallpaper with the bar's ground laid over it, column by column;
    /// `None` while the wallpaper is unknown.
    grounds: Option<[Color; SAMPLES]>,
    /// The ink the theme's mode is drawn in, for an unknown wallpaper.
    fallback: Tone,
}

impl Inks {
    pub fn new(
        behind: Option<Behind>,
        ground: Color,
        dark_mode: bool,
        overrides: &ThemeOverrides,
    ) -> Self {
        Self {
            light: Ink::of(Tone::Light, overrides),
            dark: Ink::of(Tone::Dark, overrides),
            grounds: behind.map(|behind| {
                let ground = with_alpha(ground, ground.a * behind.ground_strength);
                behind.wallpaper.0.map(|wallpaper| over(ground, wallpaper))
            }),
            fallback: if dark_mode { Tone::Light } else { Tone::Dark },
        }
    }

    /// The ink for glyphs across `span` of the bar's width, from 0 at its left
    /// end to 1 at its right.
    pub fn over(&self, span: Range<f32>) -> Ink {
        let Some(grounds) = &self.grounds else {
            return self.ink(self.fallback);
        };
        let grounds = &grounds[columns(span)];
        let (tone, contrast) = [Tone::Light, Tone::Dark]
            .map(|tone| (tone, worst_contrast(self.ink(tone).fg, grounds)))
            .into_iter()
            .max_by(|(_, first), (_, second)| first.total_cmp(second))
            .unwrap_or((self.fallback, ENOUGH_CONTRAST));
        Ink {
            shadow: shadow(tone, contrast),
            ..self.ink(tone)
        }
    }

    fn ink(&self, tone: Tone) -> Ink {
        match tone {
            Tone::Light => self.light,
            Tone::Dark => self.dark,
        }
    }
}

impl Ink {
    fn of(tone: Tone, overrides: &ThemeOverrides) -> Self {
        let (dark_tokens, (fg, fg_hover)) = match tone {
            Tone::Light => (true, LIGHT),
            Tone::Dark => (false, DARK),
        };
        let token = |name: &str, fallback: Color| {
            overrides
                .color(dark_tokens, name)
                .map_or(fallback, |[r, g, b, a]| Color::new(r, g, b, a))
        };
        Self {
            fg: token("bar.fg", fg),
            fg_hover: token("bar.fg_hover", fg_hover),
            shadow: with_alpha(fg, 0.0),
        }
    }
}

/// The sampled columns `span` covers, at least one.
fn columns(span: Range<f32>) -> Range<usize> {
    let count = SAMPLES as f32;
    let first = ((span.start * count).floor().max(0.0) as usize).min(SAMPLES - 1);
    let last = ((span.end * count).ceil().max(0.0) as usize).clamp(first + 1, SAMPLES);
    first..last
}

fn worst_contrast(ink: Color, grounds: &[Color]) -> f32 {
    grounds
        .iter()
        .map(|&ground| contrast_ratio(over(ink, ground), ground))
        .fold(f32::INFINITY, f32::min)
}

/// The opposite of the ink, stronger the further `contrast` falls short of
/// [`ENOUGH_CONTRAST`]. Ratios multiply, so the shortfall is measured on a log
/// scale.
fn shadow(tone: Tone, contrast: f32) -> Color {
    let shortfall = (1.0 - contrast.ln() / ENOUGH_CONTRAST.ln()).clamp(0.0, 1.0);
    let opposite = match tone {
        Tone::Light => Color::new(0.0, 0.0, 0.0, 1.0),
        Tone::Dark => Color::new(1.0, 1.0, 1.0, 1.0),
    };
    with_alpha(opposite, MAX_SHADOW_ALPHA * shortfall)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHOLE_BAR: Range<f32> = 0.0..1.0;
    const NO_GROUND: Color = Color::new(0.0, 0.0, 0.0, 0.0);

    fn over_wallpaper(columns: [Color; SAMPLES]) -> Inks {
        let behind = Behind {
            wallpaper: Backdrop(columns),
            ground_strength: 0.0,
        };
        Inks::new(Some(behind), NO_GROUND, true, &ThemeOverrides::default())
    }

    fn grey(level: f32) -> Color {
        Color::new(level, level, level, 1.0)
    }

    fn split() -> Inks {
        over_wallpaper(std::array::from_fn(|column| {
            grey(if column < SAMPLES / 2 { 0.0 } else { 1.0 })
        }))
    }

    #[test]
    fn a_bright_wallpaper_takes_dark_ink_without_a_shadow() {
        let ink = over_wallpaper([grey(0.95); SAMPLES]).over(WHOLE_BAR);
        assert_eq!(ink.fg, DARK.0);
        assert_eq!(ink.shadow.a, 0.0);
    }

    #[test]
    fn a_dark_wallpaper_takes_light_ink_even_in_light_mode() {
        let behind = Behind {
            wallpaper: Backdrop([grey(0.05); SAMPLES]),
            ground_strength: 0.0,
        };
        let ink =
            Inks::new(Some(behind), NO_GROUND, false, &ThemeOverrides::default()).over(WHOLE_BAR);
        assert_eq!(ink.fg, LIGHT.0);
        assert_eq!(ink.shadow.a, 0.0);
    }

    #[test]
    fn each_pill_reads_only_the_wallpaper_under_it() {
        let inks = split();
        assert_eq!(inks.over(0.1..0.3).fg, LIGHT.0);
        assert_eq!(inks.over(0.7..0.9).fg, DARK.0);
        assert_eq!(inks.over(0.1..0.3).shadow.a, 0.0);
    }

    #[test]
    fn the_shadow_deepens_as_contrast_runs_out() {
        let mid = over_wallpaper([grey(0.5); SAMPLES]).over(WHOLE_BAR);
        let straddling = split().over(0.4..0.6);
        assert!(mid.shadow.a > 0.0);
        assert!(straddling.shadow.a > mid.shadow.a);
        assert!(straddling.shadow.a <= MAX_SHADOW_ALPHA);
    }

    #[test]
    fn the_shadow_is_the_opposite_of_the_ink() {
        let light = over_wallpaper([grey(0.35); SAMPLES]).over(WHOLE_BAR);
        let dark = over_wallpaper([grey(0.6); SAMPLES]).over(WHOLE_BAR);
        assert_eq!((light.fg, light.shadow.r), (LIGHT.0, 0.0));
        assert_eq!((dark.fg, dark.shadow.r), (DARK.0, 1.0));
    }

    #[test]
    fn the_bars_own_ground_counts_toward_what_is_behind() {
        let behind = Behind {
            wallpaper: Backdrop([grey(1.0); SAMPLES]),
            ground_strength: 1.0,
        };
        let inks = Inks::new(Some(behind), grey(0.0), false, &ThemeOverrides::default());
        assert_eq!(inks.over(WHOLE_BAR).fg, LIGHT.0);
    }

    #[test]
    fn a_span_off_either_end_still_reads_a_column() {
        assert_eq!(columns(-0.2..-0.1), 0..1);
        assert_eq!(columns(1.1..1.3), SAMPLES - 1..SAMPLES);
        assert_eq!(columns(f32::NAN..f32::NAN), 0..1);
        assert_eq!(columns(0.0..1.0), 0..SAMPLES);
    }
}

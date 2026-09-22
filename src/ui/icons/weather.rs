//! The weather glyphs.
//!
//! The one place in the bar that draws in colour. Everything else is a stroke
//! in the foreground; a sun that was not warm and rain that was not blue would
//! not read as weather at all. The colours are still the palette's — see
//! [`crate::theme::WeatherColors`].
//!
//! Each glyph is assembled from parts rather than authored whole: a luminary,
//! a cloud, and something falling out of it. That is what lets the icon
//! *cross-fade* between two conditions and between day and night instead of
//! switching, which is the only way a bar pill can change weather without the
//! change looking like a glitch.
//!
//! Everything is authored in the same 24 × 24 box the runes use, and the
//! gradients are declared in that space too, so they are carried by the same
//! transform that places the paths.

use std::sync::OnceLock;

use vello::{
    kurbo::{Affine, Cap, Circle, Join, Point, Rect, Stroke},
    peniko::{Color, Fill},
    Scene,
};

use crate::{
    theme::{Stops, WeatherColors},
    widgets::Condition,
};

use super::glyph::{self, Glyph};

const VIEW_BOX: Rect = Rect::new(0.0, 0.0, 24.0, 24.0);
const INK_FILL: f64 = 0.96;

/// The big cloud, filling the box — overcast, rain, thunder.
const CLOUD: &str =
    "M18.6 8.8a6.8 6.8 0 0 0-13.2-1.6A4.6 4.6 0 0 0 6 17h12.2a4.1 4.1 0 0 0 .4-8.2z";
/// The big cloud, sat lower, for the glyphs with nothing under them.
const CLOUD_ALONE: &str =
    "M18.6 10.8a6.8 6.8 0 0 0-13.2-1.6A4.6 4.6 0 0 0 6 19h12.2a4.1 4.1 0 0 0 .4-8.2z";
/// The small cloud that shares the box with a sun or a moon.
const CLOUD_SMALL: &str =
    "M18.4 13.6a4 4 0 0 0-7.8-1.1A3.3 3.3 0 0 0 11.2 19.4h7.4a2.9 2.9 0 0 0-.2-5.8z";
/// The cloud fog sits under.
const CLOUD_HIGH: &str =
    "M18.6 7.8a6.8 6.8 0 0 0-13.2-1.6A4.6 4.6 0 0 0 6 16h12.2a4.1 4.1 0 0 0 .4-8.2z";

const SUN_RAYS: &str = "M12 3.2v2.4 M12 18.4v2.4 M5.7 5.7l1.7 1.7 M16.6 16.6l1.7 1.7 \
                        M3.2 12h2.4 M18.4 12h2.4 M5.7 18.3l1.7-1.7 M16.6 7.4l1.7-1.7";
const SUN_RAYS_SMALL: &str = "M8.4 1.6v1.8 M3.9 3.5l1.3 1.3 M2 8h1.8 M3.9 12.5l1.3-1.3 \
                              M12.9 3.5l-1.3 1.3";
const MOON: &str = "M20 14.2A8.2 8.2 0 0 1 9.8 3.9a6.6 6.6 0 1 0 10.2 10.3z";
const MOON_SMALL: &str = "M14.6 10.4A6.4 6.4 0 0 1 6.7 2.5a5.2 5.2 0 1 0 7.9 7.9z";

const BOLT: &str = "M13.4 14.9l-4.8 5.5h3l-.9 3.9 4.8-5.9h-3z";
const FLAKE: &str = "M12 3v18 M4.2 7.5l15.6 9 M19.8 7.5l-15.6 9 \
                     M12 7.2l-2.2-2.2 M12 7.2l2.2-2.2 M12 16.8l-2.2 2.2 M12 16.8l2.2 2.2";
const DRIZZLE: &str = "M12 18.6l-1 2.6 M16 18.6l-1 2.6";
const RAIN: &str = "M8 18.6l-1 2.6 M12 18.6l-1 2.6 M16 18.6l-1 2.6";
const SHOWERS: &str = "M12.4 20.6l-.8 2.2 M16 20.6l-.8 2.2";
const SLEET_DROPS: &str = "M7.6 18.7l-1 2.6 M16.8 18.7l-1 2.6";
/// Drawn thinner than the drops beside it: at pill size a six-armed star at
/// the drops' weight fills in and reads as a blot.
const SLEET_FLAKE: &str = "M12.2 18.4v4 M10.4 19.4l3.6 2 M14 19.4l-3.6 2";
const FOG: &str = "M4.5 19.2h15 M7 22.2h11";
const HAZE: &str = "M3.5 14h15 M6 18h13 M4.5 22h11";
const WIND: &str = "M3 8.5h11a3 3 0 1 0-3-3.2 M3 13.5h15a3 3 0 1 1-3 3.2 M3 18.5h7";

fn cached(svg: &'static str, cache: &'static OnceLock<Glyph>) -> &'static Glyph {
    cache.get_or_init(|| Glyph::parse(svg))
}

macro_rules! glyphs {
    ($($name:ident => $svg:ident),* $(,)?) => {
        $(fn $name() -> &'static Glyph {
            static CACHE: OnceLock<Glyph> = OnceLock::new();
            cached($svg, &CACHE)
        })*
    };
}

glyphs! {
    cloud => CLOUD,
    cloud_alone => CLOUD_ALONE,
    cloud_small => CLOUD_SMALL,
    cloud_high => CLOUD_HIGH,
    sun_rays => SUN_RAYS,
    sun_rays_small => SUN_RAYS_SMALL,
    moon => MOON,
    moon_small => MOON_SMALL,
    bolt => BOLT,
    flake => FLAKE,
    drizzle => DRIZZLE,
    rain => RAIN,
    showers => SHOWERS,
    sleet_drops => SLEET_DROPS,
    sleet_flake => SLEET_FLAKE,
    fog => FOG,
    haze => HAZE,
    wind => WIND,
}

/// Draw `from` and `to` over one another, weighted by `blend`.
///
/// Two conditions at partial alpha rather than one morphing shape: the parts
/// a pair of conditions share — the cloud, nearly always — sit exactly on top
/// of each other, so what the eye sees is the rain fading out and the sun
/// fading in, which is the change that actually happened.
pub(super) fn draw(
    scene: &mut Scene,
    bounds: Rect,
    from: Condition,
    to: Condition,
    blend: f32,
    night: f32,
    colors: &WeatherColors,
) {
    let (transform, scale) = glyph::fit(VIEW_BOX, bounds, INK_FILL);
    let blend = blend.clamp(0.0, 1.0);
    let night = night.clamp(0.0, 1.0);
    let painter = Painter {
        transform,
        scale,
        night,
        colors,
    };
    if blend < 1.0 {
        painter.condition(scene, from, 1.0 - blend);
    }
    if blend > 0.0 {
        painter.condition(scene, to, blend);
    }
}

struct Painter<'a> {
    transform: Affine,
    scale: f64,
    night: f32,
    colors: &'a WeatherColors,
}

impl Painter<'_> {
    fn condition(&self, scene: &mut Scene, condition: Condition, alpha: f32) {
        use Condition::*;
        match condition {
            Clear => self.luminary(scene, alpha, false),
            PartlyCloudy => {
                self.luminary(scene, alpha, true);
                self.fill(scene, cloud_small(), self.colors.cloud, alpha);
            }
            Cloudy => self.fill(scene, cloud_alone(), self.colors.cloud, alpha),
            Overcast => self.fill(scene, cloud_alone(), self.colors.cloud_dark, alpha),
            Drizzle => self.raining(scene, drizzle(), alpha),
            Rain => self.raining(scene, rain(), alpha),
            Showers => {
                self.luminary(scene, alpha, true);
                self.fill(scene, cloud_small(), self.colors.cloud, alpha);
                self.stroke(scene, showers(), self.colors.water, 1.6, alpha);
            }
            Thunder => {
                self.fill(scene, cloud(), self.colors.cloud, alpha);
                self.fill(scene, bolt(), self.colors.sun, alpha);
            }
            Snow => self.stroke(scene, flake(), self.colors.water, 1.8, alpha),
            Sleet => {
                self.fill(scene, cloud(), self.colors.cloud, alpha);
                self.stroke(scene, sleet_drops(), self.colors.water, 1.8, alpha);
                self.stroke(scene, sleet_flake(), self.colors.water, 1.1, alpha);
            }
            Fog => {
                self.fill(scene, cloud_high(), self.colors.cloud, alpha);
                self.stroke(scene, fog(), self.colors.water, 1.8, alpha);
            }
            Haze => {
                self.moon_only(scene, alpha);
                self.stroke(scene, haze(), self.colors.cloud_dark, 2.2, alpha);
            }
            Wind => self.stroke(scene, wind(), self.colors.water, 2.0, alpha),
        }
    }

    fn raining(&self, scene: &mut Scene, fall: &Glyph, alpha: f32) {
        self.fill(scene, cloud(), self.colors.cloud, alpha);
        self.stroke(scene, fall, self.colors.water, 1.8, alpha);
    }

    /// The sun, the moon, or the point between them. `small` is the pose that
    /// shares its box with a cloud.
    fn luminary(&self, scene: &mut Scene, alpha: f32, small: bool) {
        let day = alpha * (1.0 - self.night);
        let night = alpha * self.night;
        if day > 0.0 {
            let (center, radius, rays, width) = if small {
                (Point::new(8.4, 8.0), 3.5, sun_rays_small(), 1.5)
            } else {
                (Point::new(12.0, 12.0), 4.3, sun_rays(), 1.9)
            };
            self.stroke(scene, rays, self.colors.sun, width, day);
            scene.fill(
                Fill::NonZero,
                self.transform,
                &self.brush(self.colors.sun, day),
                None,
                &Circle::new(center, radius),
            );
        }
        if night > 0.0 {
            let moon = if small { moon_small() } else { moon() };
            self.fill(scene, moon, self.colors.moon, night);
        }
    }

    /// Haze is a night pose whatever the hour, so its moon does not fade.
    fn moon_only(&self, scene: &mut Scene, alpha: f32) {
        self.fill(scene, moon_small(), self.colors.moon, alpha);
    }

    fn fill(&self, scene: &mut Scene, glyph: &Glyph, stops: Stops, alpha: f32) {
        if glyph.is_empty() || alpha <= 0.0 {
            return;
        }
        scene.fill(
            Fill::NonZero,
            self.transform,
            &self.brush(stops, alpha),
            None,
            glyph.path(),
        );
    }

    fn stroke(&self, scene: &mut Scene, glyph: &Glyph, stops: Stops, width: f64, alpha: f32) {
        if glyph.is_empty() || alpha <= 0.0 {
            return;
        }
        // Authored in view-box units: these are drawn beside filled shapes of
        // the same authorship, so the weight has to scale with them rather
        // than stay put in screen pixels the way an outline glyph's does.
        let stroke = Stroke::new(width)
            .with_caps(Cap::Round)
            .with_join(Join::Round);
        let _ = self.scale;
        scene.stroke(
            &stroke,
            self.transform,
            &self.brush(stops, alpha),
            None,
            glyph.path(),
        );
    }

    /// The pair as a top-to-bottom gradient down the view box, faded to
    /// `alpha` so a cross-fade can pass through it.
    fn brush(&self, stops: Stops, alpha: f32) -> vello::peniko::Gradient {
        Stops {
            start: fade(stops.start, alpha),
            end: fade(stops.end, alpha),
        }
        .vertical(12.0, VIEW_BOX.y0 + 1.0, VIEW_BOX.y1 - 1.0)
    }
}

fn fade(color: Color, alpha: f32) -> Color {
    let c = color.components;
    Color::new([c[0], c[1], c[2], c[3] * alpha.clamp(0.0, 1.0)])
}

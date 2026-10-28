//! Static panel glyphs. Unlike the bar's icons these carry no animated state:
//! they are one authored path each, stroked at whatever size the caller asks
//! for.
//!
//! The set is Heroicons' outline style. Every rune is authored in the same
//! 24 × 24 box and fitted from *that* box rather than from its own ink bounds,
//! so a chevron stays a chevron next to a headphone instead of being blown up
//! to fill the same square.

use std::sync::OnceLock;

use crate::ui::scene::Scene;
use crownui::prelude::Color;
use kurbo::{Cap, Join, Rect, Stroke};

use crate::widgets::Rune;

use super::glyph::{self, Glyph};

/// The box every rune below is drawn in.
const VIEW_BOX: Rect = Rect::new(0.0, 0.0, 24.0, 24.0);
/// How much of the target bounds the view box covers.
const INK_FILL: f64 = 1.0;
/// Stroke width in view-box units, Heroicons' outline weight.
const STROKE: f64 = 1.5;

// Heroicons v2 outline.
const DISPLAY: &str = "M9 17.25v1.007a3 3 0 0 1-.879 2.122L7.5 21h9l-.621-.621A3 3 0 0 1 15 18.257V17.25m6-12V15a2.25 2.25 0 0 1-2.25 2.25H5.25A2.25 2.25 0 0 1 3 15V5.25m18 0A2.25 2.25 0 0 0 18.75 3H5.25A2.25 2.25 0 0 0 3 5.25m18 0V12a2.25 2.25 0 0 1-2.25 2.25H5.25A2.25 2.25 0 0 1 3 12V5.25";
const PHONE: &str = "M10.5 1.5H8.25A2.25 2.25 0 0 0 6 3.75v16.5a2.25 2.25 0 0 0 2.25 2.25h7.5A2.25 2.25 0 0 0 18 20.25V3.75a2.25 2.25 0 0 0-2.25-2.25H13.5m-3 0V3h3V1.5m-3 0h3m-3 18.75h3";
const MICROPHONE: &str = "M12 18.75a6 6 0 0 0 6-6v-1.5m-6 7.5a6 6 0 0 1-6-6v-1.5m6 7.5v3.75m-3.75 0h7.5M12 15.75a3 3 0 0 1-3-3V4.5a3 3 0 1 1 6 0v8.25a3 3 0 0 1-3 3Z";
const SUN: &str = "M12 3v2.25m6.364.386-1.591 1.591M21 12h-2.25m-.386 6.364-1.591-1.591M12 18.75V21m-4.773-4.227-1.591 1.591M5.25 12H3m4.227-4.773L5.636 5.636M15.75 12a3.75 3.75 0 1 1-7.5 0 3.75 3.75 0 0 1 7.5 0Z";
const MOON: &str = "M21.752 15.002A9.72 9.72 0 0 1 18 15.75c-5.385 0-9.75-4.365-9.75-9.75 0-1.33.266-2.597.748-3.752A9.753 9.753 0 0 0 3 11.25C3 16.635 7.365 21 12.75 21a9.753 9.753 0 0 0 9.002-5.998Z";
const WIFI: &str = "M8.288 15.038a5.25 5.25 0 0 1 7.424 0M5.106 11.856c3.807-3.808 9.98-3.808 13.788 0M1.924 8.674c5.565-5.565 14.587-5.565 20.152 0M12.53 18.22l-.53.53-.53-.53a.75.75 0 0 1 1.06 0Z";
const WARNING: &str = "M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126ZM12 15.75h.007v.008H12v-.008Z";
const WINDOW: &str = "M3 8.25V18a2.25 2.25 0 0 0 2.25 2.25h13.5A2.25 2.25 0 0 0 21 18V8.25m-18 0V6a2.25 2.25 0 0 1 2.25-2.25h13.5A2.25 2.25 0 0 1 21 6v2.25m-18 0h18M5.25 6h.008v.008H5.25V6ZM7.5 6h.008v.008H7.5V6Zm2.25 0h.008v.008H9.75V6Z";
const WORKSPACE: &str = "M3.75 6A2.25 2.25 0 0 1 6 3.75h2.25A2.25 2.25 0 0 1 10.5 6v2.25a2.25 2.25 0 0 1-2.25 2.25H6a2.25 2.25 0 0 1-2.25-2.25V6ZM3.75 15.75A2.25 2.25 0 0 1 6 13.5h2.25a2.25 2.25 0 0 1 2.25 2.25V18a2.25 2.25 0 0 1-2.25 2.25H6A2.25 2.25 0 0 1 3.75 18v-2.25ZM13.5 6a2.25 2.25 0 0 1 2.25-2.25H18A2.25 2.25 0 0 1 20.25 6v2.25A2.25 2.25 0 0 1 18 10.5h-2.25a2.25 2.25 0 0 1-2.25-2.25V6ZM13.5 15.75a2.25 2.25 0 0 1 2.25-2.25H18a2.25 2.25 0 0 1 2.25 2.25V18A2.25 2.25 0 0 1 18 20.25h-2.25A2.25 2.25 0 0 1 13.5 18v-2.25Z";
const SPARKLES: &str = "M9.813 15.904 9 18.75l-.813-2.846a4.5 4.5 0 0 0-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 0 0 3.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 0 0 3.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 0 0-3.09 3.09ZM18.259 8.715 18 9.75l-.259-1.035a3.375 3.375 0 0 0-2.455-2.456L14.25 6l1.036-.259a3.375 3.375 0 0 0 2.455-2.456L18 2.25l.259 1.035a3.375 3.375 0 0 0 2.456 2.456L21.75 6l-1.035.259a3.375 3.375 0 0 0-2.456 2.456ZM16.894 20.567 16.5 21.75l-.394-1.183a2.25 2.25 0 0 0-1.423-1.423L13.5 18.75l1.183-.394a2.25 2.25 0 0 0 1.423-1.423l.394-1.183.394 1.183a2.25 2.25 0 0 0 1.423 1.423l1.183.394-1.183.394a2.25 2.25 0 0 0-1.423 1.423Z";
const CLOCK: &str = "M12 6v6h4.5m4.5 0a9 9 0 1 1-18 0 9 9 0 0 1 18 0Z";
const MUSICAL_NOTE: &str = "m9 9 10.5-3m0 6.553v3.75a2.25 2.25 0 0 1-1.632 2.163l-1.32.377a1.803 1.803 0 1 1-.99-3.467l2.31-.66a2.25 2.25 0 0 0 1.632-2.163Zm0 0V2.25L9 5.25v10.303m0 0v3.75a2.25 2.25 0 0 1-1.632 2.163l-1.32.377a1.803 1.803 0 0 1-.99-3.467l2.31-.66A2.25 2.25 0 0 0 9 15.553Z";
const COMMAND_LINE: &str = "m6.75 7.5 3 2.25-3 2.25m4.5 0h3m-9 8.25h13.5A2.25 2.25 0 0 0 21 18V6a2.25 2.25 0 0 0-2.25-2.25H5.25A2.25 2.25 0 0 0 3 6v12a2.25 2.25 0 0 0 2.25 2.25Z";
const ARROW_DOWN_TRAY: &str = "M3 16.5v2.25A2.25 2.25 0 0 0 5.25 21h13.5A2.25 2.25 0 0 0 21 18.75V16.5M16.5 12 12 16.5m0 0L7.5 12m4.5 4.5V3";
const CHECK_CIRCLE: &str = "M9 12.75 11.25 15 15 9.75M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0Z";
const X_CIRCLE: &str = "m9.75 9.75 4.5 4.5m0-4.5-4.5 4.5M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0Z";
const HAND_RAISED: &str = "M10.05 4.575a1.575 1.575 0 1 0-3.15 0v3m3.15-3v-1.5a1.575 1.575 0 0 1 3.15 0v1.5m-3.15 0 .075 5.925m3.075.75V4.575m0 0a1.575 1.575 0 0 1 3.15 0V15M6.9 7.575a1.575 1.575 0 1 0-3.15 0v8.175a6.75 6.75 0 0 0 6.75 6.75h2.018a5.25 5.25 0 0 0 3.712-1.538l1.732-1.732a5.25 5.25 0 0 0 1.538-3.712l.003-2.024a.668.668 0 0 1 .198-.471 1.575 1.575 0 1 0-2.228-2.228 3.818 3.818 0 0 0-1.12 2.687M6.9 7.575V12m6.27 4.318A4.49 4.49 0 0 1 16.35 15m.002 0h-.002";
const VIDEO_CAMERA: &str = "m15.75 10.5 4.72-4.72a.75.75 0 0 1 1.28.53v11.38a.75.75 0 0 1-1.28.53l-4.72-4.72M4.5 18.75h9a2.25 2.25 0 0 0 2.25-2.25v-9a2.25 2.25 0 0 0-2.25-2.25h-9A2.25 2.25 0 0 0 2.25 7.5v9a2.25 2.25 0 0 0 2.25 2.25Z";

// Drawn to the same rules where Heroicons has no such icon: 2.25 corner
// radii, ink centred in the box, dots as Heroicons' 0.008 specks.
const HEADPHONES: &str = "M3.75 15.75V12a8.25 8.25 0 0 1 16.5 0v3.75M7.5 13.5H6a2.25 2.25 0 0 0-2.25 2.25V18A2.25 2.25 0 0 0 6 20.25h1.5V13.5Zm9 0H18a2.25 2.25 0 0 1 2.25 2.25V18A2.25 2.25 0 0 1 18 20.25h-1.5V13.5Z";
const SPEAKER: &str = "M6.75 21.75h10.5a2.25 2.25 0 0 0 2.25-2.25v-15a2.25 2.25 0 0 0-2.25-2.25H6.75A2.25 2.25 0 0 0 4.5 4.5v15a2.25 2.25 0 0 0 2.25 2.25ZM15.75 14.25a3.75 3.75 0 1 1-7.5 0 3.75 3.75 0 0 1 7.5 0ZM12 6.75h.008v.008H12V6.75Z";
const LAPTOP: &str = "M4.5 16.5V6.75A2.25 2.25 0 0 1 6.75 4.5h10.5a2.25 2.25 0 0 1 2.25 2.25v9.75m-17.25 0h19.5v.75a2.25 2.25 0 0 1-2.25 2.25h-15a2.25 2.25 0 0 1-2.25-2.25v-.75Z";
const KEYBOARD: &str = "M3.75 18.75h16.5a2.25 2.25 0 0 0 2.25-2.25v-9a2.25 2.25 0 0 0-2.25-2.25H3.75A2.25 2.25 0 0 0 1.5 7.5v9a2.25 2.25 0 0 0 2.25 2.25ZM6 9.75h.008v.008H6V9.75Zm4 0h.008v.008H10V9.75Zm4 0h.008v.008H14V9.75Zm4 0h.008v.008H18V9.75ZM7.5 14.25h9";
const STOPWATCH: &str = "M12 6a7.5 7.5 0 1 1 0 15 7.5 7.5 0 0 1 0-15Zm0 0V3m-2.25 0h4.5M12 13.5l2.25-2.25m3.75-4.5 1.5-1.5";
const BLUETOOTH: &str = "m6.75 7.5 10.5 9-5.25 4.5V3l5.25 4.5-10.5 9";
/// One dial for every power profile, its needle swung from low to high.
const GAUGE_LOW: &str = "M5.636 19.864a9 9 0 1 1 12.728 0M13.5 13.5a1.5 1.5 0 1 1-3 0 1.5 1.5 0 0 1 3 0ZM10.701 12.75 6.804 10.5";
const GAUGE_MID: &str =
    "M5.636 19.864a9 9 0 1 1 12.728 0M13.5 13.5a1.5 1.5 0 1 1-3 0 1.5 1.5 0 0 1 3 0ZM12 12V7.5";
const GAUGE_HIGH: &str = "M5.636 19.864a9 9 0 1 1 12.728 0M13.5 13.5a1.5 1.5 0 1 1-3 0 1.5 1.5 0 0 1 3 0ZM13.299 12.75 17.196 10.5";

fn svg(rune: Rune) -> &'static str {
    match rune {
        Rune::Headphones => HEADPHONES,
        Rune::Speaker => SPEAKER,
        Rune::Laptop => LAPTOP,
        Rune::Display => DISPLAY,
        Rune::Keyboard => KEYBOARD,
        Rune::Phone => PHONE,
        Rune::Microphone => MICROPHONE,
        Rune::Sun => SUN,
        Rune::Moon => MOON,
        Rune::Wifi => WIFI,
        Rune::Bluetooth => BLUETOOTH,
        Rune::Warning => WARNING,
        Rune::GaugeLow => GAUGE_LOW,
        Rune::GaugeMid => GAUGE_MID,
        Rune::GaugeHigh => GAUGE_HIGH,
        Rune::Window => WINDOW,
        Rune::Workspace => WORKSPACE,
        Rune::Sparkles => SPARKLES,
        Rune::Stopwatch => STOPWATCH,
        Rune::Clock => CLOCK,
        Rune::MusicalNote => MUSICAL_NOTE,
        Rune::CommandLine => COMMAND_LINE,
        Rune::ArrowDownTray => ARROW_DOWN_TRAY,
        Rune::CheckCircle => CHECK_CIRCLE,
        Rune::XCircle => X_CIRCLE,
        Rune::HandRaised => HAND_RAISED,
        Rune::VideoCamera => VIDEO_CAMERA,
    }
}

/// Parsed geometry, kept for the life of the process — a rune is authored
/// once and drawn thousands of times.
fn glyph_for(rune: Rune) -> &'static Glyph {
    macro_rules! cached {
        ($($variant:ident),* $(,)?) => {
            match rune {
                $(Rune::$variant => {
                    static CACHE: OnceLock<Glyph> = OnceLock::new();
                    CACHE.get_or_init(|| Glyph::parse(svg(Rune::$variant)))
                })*
            }
        };
    }
    cached!(
        Headphones,
        Speaker,
        Laptop,
        Display,
        Keyboard,
        Phone,
        Microphone,
        Sun,
        Moon,
        Wifi,
        Bluetooth,
        Warning,
        GaugeLow,
        GaugeMid,
        GaugeHigh,
        Window,
        Workspace,
        Sparkles,
        Stopwatch,
        Clock,
        MusicalNote,
        CommandLine,
        ArrowDownTray,
        CheckCircle,
        XCircle,
        HandRaised,
        VideoCamera,
    )
}

pub(super) fn draw(scene: &mut Scene<'_>, b: Rect, fg: Color, rune: Rune) {
    let glyph = glyph_for(rune);
    if glyph.is_empty() {
        return;
    }
    let (transform, _scale) = glyph::fit(VIEW_BOX, b, INK_FILL);
    // The width is authored in view-box units and left to the transform to
    // scale, so a rune's weight tracks its size. Round caps and joins are what
    // make these read as a set rather than as eleven unrelated shapes.
    let stroke = Stroke::new(STROKE)
        .with_caps(Cap::Round)
        .with_join(Join::Round);
    scene.stroke(&stroke, transform, fg, None, glyph.path());
}

#[cfg(test)]
mod tests {
    use kurbo::{BezPath, Point, Shape};

    use super::*;

    const ALL: [Rune; 27] = [
        Rune::Headphones,
        Rune::Speaker,
        Rune::Laptop,
        Rune::Display,
        Rune::Keyboard,
        Rune::Phone,
        Rune::Microphone,
        Rune::Sun,
        Rune::Moon,
        Rune::Wifi,
        Rune::Bluetooth,
        Rune::Warning,
        Rune::GaugeLow,
        Rune::GaugeMid,
        Rune::GaugeHigh,
        Rune::Window,
        Rune::Workspace,
        Rune::Sparkles,
        Rune::Stopwatch,
        Rune::Clock,
        Rune::MusicalNote,
        Rune::CommandLine,
        Rune::ArrowDownTray,
        Rune::CheckCircle,
        Rune::XCircle,
        Rune::HandRaised,
        Rune::VideoCamera,
    ];

    #[test]
    fn every_rune_parses_inside_the_view_box() {
        let stroke_room = VIEW_BOX.inflate(STROKE / 2.0, STROKE / 2.0);
        for rune in ALL {
            let path = BezPath::from_svg(svg(rune));
            assert!(path.is_ok(), "{rune:?} does not parse");
            let ink = path.unwrap_or_default().bounding_box();
            assert!(ink.area() > 0.0, "{rune:?} has no ink");
            assert!(
                stroke_room.contains(ink.origin())
                    && stroke_room.contains(Point::new(ink.x1, ink.y1)),
                "{rune:?} spills out of the view box: {ink:?}"
            );
        }
    }
}

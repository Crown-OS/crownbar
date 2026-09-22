//! Static panel glyphs. Unlike the bar's icons these carry no animated state:
//! they are one authored path each, stroked at whatever size the caller asks
//! for.
//!
//! Every rune is authored in the same 24 × 24 box and fitted from *that* box
//! rather than from its own ink bounds, so a chevron stays a chevron next to a
//! headphone instead of being blown up to fill the same square.

use std::sync::OnceLock;

use vello::{
    kurbo::{Cap, Join, Rect, Stroke},
    peniko::Color,
    Scene,
};

use crate::widgets::Rune;

use super::glyph::{self, Glyph};

/// The box every rune below is drawn in.
const VIEW_BOX: Rect = Rect::new(0.0, 0.0, 24.0, 24.0);
/// How much of the target bounds the view box covers.
const INK_FILL: f64 = 1.0;
/// Stroke width in view-box units.
const STROKE: f64 = 2.0;

const HEADPHONES: &str = "M3 14h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-7a9 9 0 0 1 18 0v7a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3";
const SPEAKER: &str = "M7 2h10a2 2 0 0 1 2 2v16a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2z M12 12a3.5 3.5 0 1 0 0 7a3.5 3.5 0 1 0 0-7z M12 7.5h.01";
const LAPTOP: &str = "M4 5h16v11H4z M2 19h20";
const DISPLAY: &str = "M3 4h18v12H3z M8 21h8 M12 17v4";
const SUN: &str = "M12 7.5a4.5 4.5 0 1 0 0 9a4.5 4.5 0 1 0 0-9z M12 1.5v2.5 M12 20v2.5 M3.2 3.2l1.8 1.8 M19 19l1.8 1.8 M1.5 12h2.5 M20 12h2.5 M3.2 20.8l1.8-1.8 M19 5l1.8-1.8";
const KEYBOARD: &str = "M3 7h18v11H3z M7 11h.01 M11 11h.01 M15 11h.01 M8 15h8";
const PHONE: &str = "M7 2h10a1 1 0 0 1 1 1v18a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1V3a1 1 0 0 1 1-1z M10.5 18.5h3";
const MICROPHONE: &str = "M12 2a3 3 0 0 1 3 3v6a3 3 0 0 1-6 0V5a3 3 0 0 1 3-3z M5 10.5a7 7 0 0 0 14 0 M12 17.5V22 M8.5 22h7";
const WIFI: &str = "M12 20h.01 M8.5 16.429a5 5 0 0 1 7 0 M5 12.859a10 10 0 0 1 14 0 M2 8.82a15 15 0 0 1 20 0";
const BLUETOOTH: &str = "M7 7 L17 17 L12 22 L12 2 L17 7 L7 17";
const WARNING: &str = "M12 3 L22.5 20.5 L1.5 20.5 Z M12 10v4.5 M12 17.5h.01";
const CHEVRON_RIGHT: &str = "M9.5 5.5 L16 12 L9.5 18.5";
const LEAF: &str = "M11 20A7 7 0 0 1 9.8 6.1C15.5 5 17 4.48 19 2c1 2 2 4.18 2 8 0 5.5-4.78 10-10 10z M2 21c0-3 1.85-5.36 5.08-6C9.5 14.52 12 13 13 12";
const GAUGE: &str = "M12 14 L16 10 M3.34 19a10 10 0 1 1 17.32 0";
const BOLT: &str = "M13 2 L3 14h9l-1 8 10-12h-9l1-8z";

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
        Rune::Wifi => WIFI,
        Rune::Bluetooth => BLUETOOTH,
        Rune::Warning => WARNING,
        Rune::ChevronRight => CHEVRON_RIGHT,
        Rune::Leaf => LEAF,
        Rune::Gauge => GAUGE,
        Rune::Bolt => BOLT,
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
        Wifi,
        Bluetooth,
        Warning,
        ChevronRight,
        Leaf,
        Gauge,
        Bolt,
    )
}

pub(super) fn draw(scene: &mut Scene, b: Rect, fg: Color, rune: Rune) {
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

//! Screen sharing, Untitled's chrome-cast. The screen is the bar's foreground
//! like every other pill; the cast mark in its corner is the palette's danger
//! red and ripples out with `pulse`, because sharing is the one state the user
//! must not overlook.

use crate::scene::Scene;
use crownui::prelude::Color;
use kurbo::Rect;

use super::{
    art::{self, Art, STROKE},
    fade, lerp,
};

/// The screen, open at its bottom-left corner where the cast mark sits.
const SCREEN: &str = "M11.6667 17.5H14.3334C15.7335 17.5 16.4335 17.5 16.9684 17.2275C17.4388 16.9878 17.8212 16.6054 18.0609 16.135C18.3334 15.6002 18.3334 14.9002 18.3334 13.5V6.5C18.3334 5.09987 18.3334 4.3998 18.0609 3.86503C17.8212 3.39462 17.4388 3.01217 16.9684 2.77248C16.4335 2.5 15.7335 2.5 14.3334 2.5H5.66669C4.26655 2.5 3.56649 2.5 3.03171 2.77248C2.5613 3.01217 2.17885 3.39462 1.93917 3.86503C1.66669 4.3998 1.66669 5.09987 1.66669 6.5V6.66667";
const CAST: &str = "M1.66669 10.0417C3.35901 10.2299 4.93692 10.9883 6.14095 12.1924C7.34498 13.3964 8.10347 14.9743 8.29169 16.6667M1.66669 13.4167C2.46743 13.58 3.20247 13.9752 3.78034 14.553C4.3582 15.1308 4.75338 15.8659 4.91669 16.6667M1.66669 16.6667H1.67502";

static SCREEN_ART: Art = Art::new(&[SCREEN]);
static CAST_ART: Art = Art::new(&[CAST]);

/// How much wider than the mark its ripple spreads at the height of a pulse,
/// and how strong it starts.
const RIPPLE_SPREAD: f64 = 2.6;
const RIPPLE_ALPHA: f32 = 0.35;

pub(super) fn draw(scene: &mut Scene<'_>, b: Rect, fg: Color, light: Color, pulse: f32) {
    let (screen, cast) = (SCREEN_ART.glyph(), CAST_ART.glyph());
    if screen.is_empty() || cast.is_empty() {
        return;
    }
    let pulse = pulse.clamp(0.0, 1.0);
    let transform = art::place(b);
    let pen = SCREEN_ART.pen();
    scene.stroke(&pen, transform, fg, None, screen.path());

    let ripple = art::pen(STROKE * (1.0 + (RIPPLE_SPREAD - 1.0) * f64::from(pulse)));
    scene.stroke(
        &ripple,
        transform,
        fade(light, lerp(RIPPLE_ALPHA, 0.0, pulse)),
        None,
        cast.path(),
    );
    scene.stroke(&pen, transform, light, None, cast.path());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::art::tests::assert_fits;

    #[test]
    fn every_piece_fits_the_view_box() {
        assert_fits(&SCREEN_ART, "screen");
        assert_fits(&CAST_ART, "cast mark");
    }
}

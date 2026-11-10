//! Notification bell, Untitled's bell-01. `open` ∈ [0, 1] fills the bell as the
//! notification centre comes out; `silenced` ∈ [0, 1] traces a slash across
//! it and cross-fades the body to bell-off-01's, broken where the slash
//! passes, as Do Not Disturb goes on. The clapper never changes, so the pill
//! reads as the same object in all four states.

use crate::scene::{Fill, Scene};
use crownui::prelude::Color;
use kurbo::Rect;

use super::{
    art::{self, Art},
    fade,
};

const BELL: &str = "M15 6.66667C15 5.34059 14.4732 4.06882 13.5356 3.13114C12.5978 2.19346 11.3261 1.66667 9.99998 1.66667C8.6739 1.66667 7.40215 2.19346 6.46447 3.13114C5.52678 4.06882 5 5.34059 5 6.66667C5 9.24184 4.35039 11.005 3.62472 12.1712C3.01261 13.1549 2.70655 13.6468 2.71777 13.784C2.7302 13.9359 2.76238 13.9938 2.88481 14.0847C2.99538 14.1667 3.49382 14.1667 4.49071 14.1667H15.5093C16.5062 14.1667 17.0047 14.1667 17.1152 14.0847C17.2377 13.9938 17.2698 13.9359 17.2822 13.784C17.2935 13.6468 16.9874 13.1549 16.3753 12.1712C15.6496 11.005 15 9.24184 15 6.66667Z";
const BROKEN_BELL: &str = "M7.19422 2.5281C8.01633 1.97076 8.9925 1.66667 10 1.66667C11.3261 1.66667 12.5978 2.19346 13.5355 3.13114C14.4732 4.06882 15 5.34059 15 6.66667C15 8.41734 15.2252 9.79267 15.5403 10.8603M5.21556 5.21437C5.07388 5.68105 5 6.17005 5 6.66667C5 9.24184 4.35039 11.005 3.62472 12.1712C3.01261 13.1549 2.70655 13.6468 2.71777 13.784C2.7302 13.9359 2.76238 13.9938 2.88482 14.0847C2.99538 14.1667 3.49383 14.1667 4.49071 14.1667H14.1667";
const CLAPPER: &str = "M7.79514 17.5C8.38273 18.0187 9.15465 18.3333 9.99998 18.3333C10.8454 18.3333 11.6172 18.0187 12.2048 17.5";
/// Drawn from the top so silencing strikes down across the bell.
const SLASH: &str = "M2.5 2.5L17.5 17.5";

static BELL_ART: Art = Art::new(&[BELL]);
static BROKEN_BELL_ART: Art = Art::new(&[BROKEN_BELL]);
static CLAPPER_ART: Art = Art::new(&[CLAPPER]);
static SLASH_ART: Art = Art::new(&[SLASH]);

const OPEN_FILL_ALPHA: f32 = 0.9;
const MIN_ALPHA: f32 = 0.01;

pub(super) fn draw(scene: &mut Scene<'_>, b: Rect, fg: Color, open: f32, silenced: f32) {
    let bell = BELL_ART.glyph();
    if bell.is_empty() {
        return;
    }
    let (open, silenced) = (open.clamp(0.0, 1.0), silenced.clamp(0.0, 1.0));
    let transform = art::place(b);
    let pen = BELL_ART.pen();

    // Filling rather than growing: the pill's width is laid out from the glyph
    // box, and a bell that swelled would shove the widgets beside it along as
    // the centre opened.
    if open > 0.0 {
        scene.fill(
            Fill::NonZero,
            transform,
            fade(fg, open * OPEN_FILL_ALPHA),
            None,
            bell.path(),
        );
    }
    if silenced < 1.0 - MIN_ALPHA {
        scene.stroke(&pen, transform, fade(fg, 1.0 - silenced), None, bell.path());
    }
    if silenced > MIN_ALPHA {
        let broken = BROKEN_BELL_ART.glyph().path();
        scene.stroke(&pen, transform, fade(fg, silenced), None, broken);
    }
    scene.stroke(&pen, transform, fg, None, CLAPPER_ART.glyph().path());

    // The slash draws itself on the way the bluetooth rune does, rather than
    // fading, so silencing reads as an action rather than a state change.
    let slash = SLASH_ART.glyph();
    if silenced >= 1.0 {
        scene.stroke(&pen, transform, fg, None, slash.path());
    } else if silenced > 0.0 {
        scene.stroke(&pen, transform, fg, None, &slash.traced(silenced));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::{
        art::tests::assert_fits,
        tests::{BOX, INK, commands_drawn},
    };

    #[test]
    fn every_piece_fits_the_view_box() {
        assert_fits(&BELL_ART, "bell");
        assert_fits(&BROKEN_BELL_ART, "broken bell");
        assert_fits(&CLAPPER_ART, "clapper");
        assert_fits(&SLASH_ART, "slash");
    }

    #[test]
    fn every_pose_draws() {
        for (open, silenced) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
            let drawn = commands_drawn(|scene| draw(scene, BOX, INK, open, silenced));
            assert!(drawn >= 2, "open {open}, silenced {silenced}");
        }
    }
}

//! Caffeine cup. `on` ∈ [0, 1] traces the steam on as the machine is told to
//! stay awake, taking Untitled's coffee-off to its coffee; the cup itself never
//! changes, so the pill reads as the same object in both states.

use crate::scene::Scene;
use crownui::prelude::Color;
use kurbo::Rect;

use super::art::{self, Art};

pub(super) const CUP: &str = "M14.1667 7.91666H15.4167C16.5672 7.91666 17.5 8.84941 17.5 10C17.5 11.1506 16.5672 12.0833 15.4167 12.0833H14.1667M2.5 7.08333H14.1667V11.6667C14.1667 14.8883 11.555 17.5 8.33333 17.5C5.11168 17.5 2.5 14.8883 2.5 11.6667V7.08333Z";
/// Three wisps, drawn outwards from the cup so the trace grows upwards.
pub(super) const STEAM: &str =
    "M5.83333 3.74999V2.08333M8.33333 3.74999V2.08333M10.8333 3.74999V2.08333";

static CUP_ART: Art = Art::new(&[CUP]);
static STEAM_ART: Art = Art::new(&[STEAM]);

pub(super) fn draw(scene: &mut Scene<'_>, b: Rect, fg: Color, on: f32) {
    let cup = CUP_ART.glyph();
    if cup.is_empty() {
        return;
    }
    let progress = on.clamp(0.0, 1.0);
    let transform = art::place(b);
    let pen = CUP_ART.pen();

    scene.stroke(&pen, transform, fg, None, cup.path());

    let steam = STEAM_ART.glyph();
    if progress >= 1.0 {
        scene.stroke(&pen, transform, fg, None, steam.path());
    } else if progress > 0.0 {
        scene.stroke(&pen, transform, fg, None, &steam.traced(progress));
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
        assert_fits(&CUP_ART, "cup");
        assert_fits(&STEAM_ART, "steam");
    }

    #[test]
    fn steam_only_rises_while_on() {
        let off = commands_drawn(|scene| draw(scene, BOX, INK, 0.0));
        let on = commands_drawn(|scene| draw(scene, BOX, INK, 1.0));
        assert!(off > 0 && on > off);
    }
}

//! Bluetooth rune, Untitled's bluetooth-on. `on` ∈ [0, 1] is the progress of
//! the stroke along the rune's single continuous path: it draws itself on as
//! the adapter comes up and un-draws as it goes down, over a dim full-length
//! track.

use crate::scene::Scene;
use crownui::prelude::Color;
use kurbo::Rect;

use super::{
    art::{self, Art},
    fade, lerp,
};

/// One unbroken trace: down-right diagonal, lower flag, stem, upper flag,
/// down-left diagonal.
pub(super) const RUNE: &str = "M5 5.83334L15 14.1667L10 18.3333V1.66667L15 5.83334L5 14.1667";

static RUNE_ART: Art = Art::new(&[RUNE]);

pub(super) fn draw(scene: &mut Scene<'_>, b: Rect, fg: Color, on: f32) {
    let rune = RUNE_ART.glyph();
    if rune.is_empty() {
        return;
    }
    let progress = on.clamp(0.0, 1.0);
    let transform = art::place(b);
    let pen = RUNE_ART.pen();

    scene.stroke(
        &pen,
        transform,
        fade(fg, lerp(0.18, 0.30, progress)),
        None,
        rune.path(),
    );

    if progress >= 1.0 {
        scene.stroke(&pen, transform, fg, None, rune.path());
    } else if progress > 0.0 {
        scene.stroke(&pen, transform, fg, None, &rune.traced(progress));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::art::tests::assert_fits;

    #[test]
    fn the_rune_fits_the_view_box() {
        assert_fits(&RUNE_ART, "bluetooth");
    }
}

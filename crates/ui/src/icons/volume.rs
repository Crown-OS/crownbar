//! Volume speaker, Untitled's volume-max. `level` ∈ [0, 1] lights the waves
//! over a dim track, the way link quality lights the Wi-Fi arcs; `muted`
//! ∈ [0, 1] fades the waves away and traces a cross on in their place. The
//! speaker never changes, so the pill reads as the same object in every state.

use crate::scene::Scene;
use crownui::prelude::Color;
use kurbo::Rect;

use super::{
    art::{self, Art},
    fade, lerp,
};

pub(super) const SPEAKER: &str = "M8.0286 4.47134L5.39052 7.10942C5.2464 7.25354 5.17433 7.32561 5.09023 7.37714C5.01567 7.42283 4.93439 7.4565 4.84936 7.47692C4.75345 7.49994 4.65154 7.49994 4.44771 7.49994H3C2.53329 7.49994 2.29993 7.49994 2.12167 7.59077C1.96487 7.67066 1.83739 7.79814 1.75749 7.95495C1.66666 8.13321 1.66666 8.36658 1.66666 8.83325V11.1666C1.66666 11.6333 1.66666 11.8667 1.75749 12.0449C1.83739 12.2018 1.96487 12.3293 2.12167 12.4091C2.29993 12.4999 2.53329 12.4999 3 12.4999H4.44771C4.65154 12.4999 4.75345 12.4999 4.84936 12.523C4.93439 12.5434 5.01567 12.5771 5.09023 12.6228C5.17433 12.6743 5.2464 12.7463 5.39052 12.8905L8.02859 15.5285C8.38558 15.8855 8.56408 16.064 8.71733 16.0761C8.85025 16.0865 8.98025 16.0327 9.06683 15.9313C9.16666 15.8144 9.16666 15.562 9.16666 15.0572V4.94275C9.16666 4.4379 9.16666 4.18548 9.06683 4.06859C8.98025 3.96718 8.85025 3.91335 8.71733 3.92382C8.56408 3.93588 8.38558 4.11437 8.0286 4.47134Z";
/// Inner to outer.
pub(super) const WAVES: [&str; 2] = [
    "M13.1211 6.66661C13.7802 7.61147 14.1667 8.76058 14.1667 9.99992C14.1667 11.2393 13.7802 12.3884 13.1211 13.3333",
    "M16.4566 4.16661C17.6377 5.80847 18.3333 7.82297 18.3333 9.99992C18.3333 12.1769 17.6377 14.1914 16.4566 15.8333",
];
/// Where the waves were, so the pill keeps its width whether muted or not.
const CROSS: &str = "M18.3333 7.5L13.3333 12.5M13.3333 7.5L18.3333 12.5";

static SPEAKER_ART: Art = Art::new(&[SPEAKER]);
static WAVE_ART: [Art; 2] = [Art::new(&[WAVES[0]]), Art::new(&[WAVES[1]])];
static CROSS_ART: Art = Art::new(&[CROSS]);

const MIN_ALPHA: f32 = 0.01;
const TRACK_ALPHA: f32 = 0.2;

/// Level at which each wave starts lighting, and the span it takes to reach
/// full brightness.
const WAVE_LIT_AT: [f32; 2] = [0.05, 0.5];
const WAVE_LIT_SPAN: f32 = 0.3;

pub(super) fn draw(scene: &mut Scene<'_>, b: Rect, fg: Color, level: f32, muted: f32) {
    let speaker = SPEAKER_ART.glyph();
    if speaker.is_empty() {
        return;
    }
    let (level, muted) = (level.clamp(0.0, 1.0), muted.clamp(0.0, 1.0));
    let transform = art::place(b);
    let pen = SPEAKER_ART.pen();

    scene.stroke(&pen, transform, fg, None, speaker.path());

    for (wave, lit_at) in WAVE_ART.iter().zip(WAVE_LIT_AT) {
        let lit = ((level - lit_at) / WAVE_LIT_SPAN).clamp(0.0, 1.0);
        let alpha = lerp(TRACK_ALPHA, 1.0, lit) * (1.0 - muted);
        if alpha > MIN_ALPHA {
            scene.stroke(&pen, transform, fade(fg, alpha), None, wave.glyph().path());
        }
    }

    let cross = CROSS_ART.glyph();
    if muted >= 1.0 {
        scene.stroke(&pen, transform, fg, None, cross.path());
    } else if muted > MIN_ALPHA {
        scene.stroke(&pen, transform, fg, None, &cross.traced(muted));
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
        assert_fits(&SPEAKER_ART, "speaker");
        assert_fits(&WAVE_ART[0], "inner wave");
        assert_fits(&WAVE_ART[1], "outer wave");
        assert_fits(&CROSS_ART, "mute cross");
    }

    #[test]
    fn full_and_muted_both_draw_more_than_the_speaker() {
        let speaker_only = 1;
        let full = commands_drawn(|scene| draw(scene, BOX, INK, 1.0, 0.0));
        let muted = commands_drawn(|scene| draw(scene, BOX, INK, 1.0, 1.0));
        assert!(full > speaker_only && muted > speaker_only);
    }
}

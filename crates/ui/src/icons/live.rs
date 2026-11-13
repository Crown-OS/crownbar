//! A live activity's mark: its glyph, ringed by how far along it is, over a
//! halo in its accent while it asks for attention.

use std::f64::consts::{FRAC_PI_2, TAU};

use crate::scene::{Fill, Scene};
use crownui::prelude::Color;
use kurbo::{Arc, Cap, Circle, Rect, Stroke, Vec2};

use crownbar_integrations::live::Glyph;

use super::{art::Art, fade, lerp, untitled::*};

const RING_STROKE: f64 = 1.5;
const RING_TRACK_ALPHA: f32 = 0.28;
/// How much of the box the glyph keeps inside a ring.
const RINGED_GLYPH: f64 = 0.56;
/// How much of the box the glyph takes with no ring.
const BARE_GLYPH: f64 = 0.82;
const HALO_ALPHA: f32 = 0.45;
/// The glyph's view box fills whatever share of the box it is given.
const GLYPH_FILL: f64 = 1.0;

fn art(glyph: Glyph) -> &'static Art {
    match glyph {
        Glyph::Sparkles => &MAGIC_WAND_02,
        Glyph::Stopwatch => &CLOCK_STOPWATCH,
        Glyph::Timer => &HOURGLASS_01,
        Glyph::Music => &MUSIC_NOTE_01,
        Glyph::Terminal => &TERMINAL_SQUARE,
        Glyph::Download => &ARROW_SQUARE_DOWN,
        Glyph::Check => &CHECK_CIRCLE,
        Glyph::Cross => &X_CIRCLE,
        Glyph::Alert => &ALERT_TRIANGLE,
        Glyph::Hand => &MESSAGE_QUESTION_SQUARE,
        Glyph::Microphone => &MICROPHONE_02,
        Glyph::Camera => &VIDEO_RECORDER,
        Glyph::Screen => &MONITOR_01,
        Glyph::Coffee => &COFFEE,
        Glyph::Call => &PHONE_CALL_01,
        Glyph::Webcam => &WEBCAM_01,
        Glyph::Mirror => &PHONE_01,
        Glyph::ExternalDisplay => &MONITOR_05,
        Glyph::Phone => &PHONE_01,
    }
}

fn centred(b: Rect, share: f64) -> Rect {
    let inset = b.width().min(b.height()) * (1.0 - share) * 0.5;
    b.inset(-inset)
}

pub(super) struct Look {
    pub glyph: Glyph,
    pub progress: Option<f32>,
    pub pulse: f32,
    pub fg: Color,
    pub accent: Color,
}

pub(super) fn draw(scene: &mut Scene<'_>, b: Rect, look: Look) {
    let centre = b.center();
    let radius = b.width().min(b.height()) * 0.5;
    if look.pulse > 0.0 {
        let halo = Circle::new(centre, radius * f64::from(lerp(0.6, 1.0, look.pulse)));
        scene.fill(
            Fill::NonZero,
            kurbo::Affine::IDENTITY,
            fade(look.accent, HALO_ALPHA * look.pulse),
            None,
            &halo,
        );
    }

    let share = match look.progress {
        Some(progress) => {
            ring(scene, centre, radius - RING_STROKE, progress, look.fg);
            RINGED_GLYPH
        }
        None => BARE_GLYPH,
    };
    art(look.glyph).draw(scene, centred(b, share), look.fg, GLYPH_FILL);
}

fn ring(scene: &mut Scene<'_>, centre: kurbo::Point, radius: f64, progress: f32, fg: Color) {
    let stroke = Stroke::new(RING_STROKE).with_caps(Cap::Round);
    let identity = kurbo::Affine::IDENTITY;
    scene.stroke(
        &stroke,
        identity,
        fade(fg, RING_TRACK_ALPHA),
        None,
        &Circle::new(centre, radius),
    );
    if progress <= 0.0 {
        return;
    }
    let sweep = Arc::new(
        centre,
        Vec2::new(radius, radius),
        -FRAC_PI_2,
        TAU * f64::from(progress.min(1.0)),
        0.0,
    );
    scene.stroke(&stroke, identity, fg, None, &sweep);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::tests::{BOX, INK, commands_drawn};

    const ALL: [Glyph; 19] = [
        Glyph::Sparkles,
        Glyph::Stopwatch,
        Glyph::Timer,
        Glyph::Music,
        Glyph::Terminal,
        Glyph::Download,
        Glyph::Check,
        Glyph::Cross,
        Glyph::Alert,
        Glyph::Hand,
        Glyph::Microphone,
        Glyph::Camera,
        Glyph::Screen,
        Glyph::Coffee,
        Glyph::Call,
        Glyph::Webcam,
        Glyph::Mirror,
        Glyph::ExternalDisplay,
        Glyph::Phone,
    ];

    #[test]
    fn every_glyph_draws_ringed_or_bare() {
        for glyph in ALL {
            assert!(!art(glyph).glyph().is_empty(), "{glyph:?} has no ink");
            for progress in [None, Some(0.5)] {
                let look = Look {
                    glyph,
                    progress,
                    pulse: 0.0,
                    fg: INK,
                    accent: INK,
                };
                assert!(
                    commands_drawn(|scene| draw(scene, BOX, look)) > 0,
                    "{glyph:?} drew nothing"
                );
            }
        }
    }
}

//! A live activity's mark: its glyph, ringed by how far along it is, over a
//! halo in its accent while it asks for attention.

use std::f64::consts::{FRAC_PI_2, TAU};

use crate::ui::scene::{Fill, Scene};
use crownui::prelude::Color;
use kurbo::{Arc, Cap, Circle, Rect, Stroke, Vec2};

use crate::{services::live::Glyph, widgets::Rune};

use super::{caffeine, fade, lerp, rune};

const RING_STROKE: f64 = 1.5;
const RING_TRACK_ALPHA: f32 = 0.28;
/// How much of the box the glyph keeps inside a ring.
const RINGED_GLYPH: f64 = 0.56;
/// How much of the box the glyph takes with no ring.
const BARE_GLYPH: f64 = 0.82;
const HALO_ALPHA: f32 = 0.45;

fn rune_for(glyph: Glyph) -> Option<Rune> {
    Some(match glyph {
        Glyph::Sparkles => Rune::Sparkles,
        Glyph::Stopwatch => Rune::Stopwatch,
        Glyph::Timer => Rune::Clock,
        Glyph::Music => Rune::MusicalNote,
        Glyph::Terminal => Rune::CommandLine,
        Glyph::Download => Rune::ArrowDownTray,
        Glyph::Check => Rune::CheckCircle,
        Glyph::Cross => Rune::XCircle,
        Glyph::Alert => Rune::Warning,
        Glyph::Hand => Rune::HandRaised,
        Glyph::Microphone => Rune::Microphone,
        Glyph::Camera => Rune::VideoCamera,
        Glyph::Screen => Rune::Display,
        Glyph::Coffee => return None,
    })
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
    let inner = centred(b, share);
    match rune_for(look.glyph) {
        Some(glyph) => rune::draw(scene, inner, look.fg, glyph),
        None => caffeine::draw(scene, inner, look.fg, 1.0),
    }
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

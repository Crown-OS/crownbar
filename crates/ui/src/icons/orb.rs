use std::f64::consts::TAU;

use crate::scene::{Fill, Scene};
use crownbar_widgets::OrbMood;
use crownui::prelude::Color;
use kurbo::{Affine, Arc, Cap, Circle, Point, Stroke, Vec2};

use super::{fade, lerp};

const CORE_IDLE: f32 = 0.30;
const CORE_LISTENING_MIN: f32 = 0.28;
const CORE_LISTENING_MAX: f32 = 0.46;
const RING_STROKE: f64 = 1.6;
const SPEAK_RINGS: u32 = 2;

pub(super) fn draw(
    scene: &mut Scene<'_>,
    b: kurbo::Rect,
    fg: Color,
    accent: Color,
    level: f32,
    phase: f32,
    mood: OrbMood,
) {
    let centre = b.center();
    let radius = b.width().min(b.height()) * 0.5;
    match mood {
        OrbMood::Idle => core(scene, centre, radius * f64::from(CORE_IDLE), fade(fg, 0.9)),
        OrbMood::Listening => {
            let share = lerp(CORE_LISTENING_MIN, CORE_LISTENING_MAX, level);
            halo(
                scene,
                centre,
                radius * f64::from(lerp(0.6, 1.0, level)),
                fade(accent, 0.25 + 0.35 * level),
            );
            core(scene, centre, radius * f64::from(share), accent);
        }
        OrbMood::Thinking => {
            core(
                scene,
                centre,
                radius * f64::from(CORE_IDLE * 0.8),
                fade(accent, 0.8),
            );
            spinner(
                scene,
                centre,
                radius - RING_STROKE,
                f64::from(phase),
                accent,
            );
        }
        OrbMood::Speaking => {
            core(scene, centre, radius * f64::from(CORE_IDLE), accent);
            for ring in 0..SPEAK_RINGS {
                let t = (f64::from(phase) * 1.4 + f64::from(ring) / f64::from(SPEAK_RINGS)).fract();
                let ring_radius = radius * (0.4 + 0.6 * t);
                ripple(
                    scene,
                    centre,
                    ring_radius,
                    fade(accent, (1.0 - t as f32) * 0.7),
                );
            }
        }
        OrbMood::Tool => {
            core(
                scene,
                centre,
                radius * f64::from(CORE_IDLE * 0.7),
                fade(fg, 0.9),
            );
            dashed_ring(
                scene,
                centre,
                radius - RING_STROKE,
                f64::from(phase),
                accent,
            );
        }
    }
}

fn core(scene: &mut Scene<'_>, centre: Point, radius: f64, color: Color) {
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        color,
        None,
        &Circle::new(centre, radius),
    );
}

fn halo(scene: &mut Scene<'_>, centre: Point, radius: f64, color: Color) {
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        color,
        None,
        &Circle::new(centre, radius),
    );
}

fn ripple(scene: &mut Scene<'_>, centre: Point, radius: f64, color: Color) {
    let stroke = Stroke::new(RING_STROKE * 0.8).with_caps(Cap::Round);
    scene.stroke(
        &stroke,
        Affine::IDENTITY,
        color,
        None,
        &Circle::new(centre, radius),
    );
}

fn spinner(scene: &mut Scene<'_>, centre: Point, radius: f64, phase: f64, color: Color) {
    let stroke = Stroke::new(RING_STROKE).with_caps(Cap::Round);
    let start = (phase * 1.5).fract() * TAU;
    let arc = Arc::new(centre, Vec2::new(radius, radius), start, TAU * 0.3, 0.0);
    scene.stroke(
        &stroke,
        Affine::IDENTITY,
        fade(color, 0.3),
        None,
        &Circle::new(centre, radius),
    );
    scene.stroke(&stroke, Affine::IDENTITY, color, None, &arc);
}

fn dashed_ring(scene: &mut Scene<'_>, centre: Point, radius: f64, phase: f64, color: Color) {
    let stroke = Stroke::new(RING_STROKE).with_caps(Cap::Round);
    let offset = (phase * 0.6).fract() * TAU;
    for dash in 0..4 {
        let start = offset + f64::from(dash) * TAU / 4.0;
        let arc = Arc::new(centre, Vec2::new(radius, radius), start, TAU / 8.0, 0.0);
        scene.stroke(&stroke, Affine::IDENTITY, color, None, &arc);
    }
}

//! Wi-Fi rune: a dot plus three arcs (Lucide geometry). The arcs light by link
//! quality when associated, run an outward sweep while searching, and a slash
//! draws itself on when the radio is blocked.

use std::sync::OnceLock;

use vello::{
    kurbo::{Rect, Stroke},
    peniko::Color,
    Scene,
};

use crate::widgets::WifiState;

use super::{
    fade,
    glyph::{self, Glyph},
    lerp,
};

const DOT_SVG: &str = "M12 20h.01";
const ARC_SVG: [&str; 3] = [
    "M8.5 16.429a5 5 0 0 1 7 0",
    "M5 12.859a10 10 0 0 1 14 0",
    "M2 8.82a15 15 0 0 1 20 0",
];
const SLASH_SVG: &str = "M4.5 19.5 L19.5 4.5";

const INK_FILL: f64 = 0.92;
const STROKE_PX: f64 = 2.0;
const DOT_PX: f64 = 2.0;
const MIN_ALPHA: f32 = 0.01;

/// Link quality at which each arc starts lighting, and the span it takes to
/// reach full brightness.
const ARC_LIT_AT: [f32; 3] = [0.15, 0.45, 0.75];
const ARC_LIT_SPAN: f32 = 0.3;

/// Sweep shape: per-arc delay into the cycle, each arc's own window, and the
/// share of that window spent drawing on (the rest fades out).
const SWEEP_STAGGER: f32 = 0.16;
const SWEEP_SPAN: f32 = 0.55;
const SWEEP_RISE: f32 = 0.42;
const SWEEP_DOT_ALPHA: f32 = 0.6;

struct Rune {
    dot: Glyph,
    arcs: [Glyph; 3],
    slash: Glyph,
    /// Union of dot + arcs, so the slash can overhang without shifting the fit.
    ink: Rect,
}

fn rune() -> &'static Rune {
    static CACHE: OnceLock<Rune> = OnceLock::new();
    CACHE.get_or_init(|| {
        let dot = Glyph::parse(DOT_SVG);
        let arcs = ARC_SVG.map(Glyph::parse);
        let ink = arcs.iter().fold(dot.ink(), |acc, arc| acc.union(arc.ink()));
        Rune {
            dot,
            arcs,
            slash: Glyph::parse(SLASH_SVG),
            ink,
        }
    })
}

pub(super) fn draw(scene: &mut Scene, b: Rect, fg: Color, state: WifiState) {
    let rune = rune();
    if rune.dot.is_empty() || rune.arcs.iter().any(Glyph::is_empty) {
        return;
    }

    let strength = state.strength.clamp(0.0, 1.0);
    let off = state.off.clamp(0.0, 1.0);
    let searching = state.searching.clamp(0.0, 1.0);
    let live = 1.0 - off;

    let (transform, scale) = glyph::fit(rune.ink, b, INK_FILL);
    let stroke = Stroke::new(STROKE_PX / scale);
    let dot_stroke = Stroke::new(DOT_PX / scale);

    // Track: the whole rune, dim, so the icon keeps its silhouette.
    let track = fade(fg, lerp(0.20, 0.12, off));
    scene.stroke(&dot_stroke, transform, track, None, rune.dot.path());
    for arc in &rune.arcs {
        scene.stroke(&stroke, transform, track, None, arc.path());
    }

    // Dot: the device itself — lit whenever the radio is up.
    let dot = lerp(
        (strength * 3.0).clamp(0.0, 1.0).max(0.35),
        SWEEP_DOT_ALPHA,
        searching,
    ) * live;
    if dot > MIN_ALPHA {
        scene.stroke(&dot_stroke, transform, fade(fg, dot), None, rune.dot.path());
    }

    for (i, arc) in rune.arcs.iter().enumerate() {
        let lit = ((strength - ARC_LIT_AT[i]) / ARC_LIT_SPAN).clamp(0.0, 1.0);
        let (sweep_trace, sweep_alpha) = sweep(state.phase, i);
        let alpha = lerp(lit, sweep_alpha, searching) * live;
        let trace = lerp(1.0, sweep_trace, searching);
        if alpha <= MIN_ALPHA || trace <= 0.0 {
            continue;
        }
        let color = fade(fg, alpha);
        if trace >= 1.0 {
            scene.stroke(&stroke, transform, color, None, arc.path());
        } else {
            scene.stroke(&stroke, transform, color, None, &arc.traced(trace));
        }
    }

    if off > MIN_ALPHA {
        if off >= 1.0 {
            scene.stroke(&stroke, transform, fg, None, rune.slash.path());
        } else {
            scene.stroke(&stroke, transform, fg, None, &rune.slash.traced(off));
        }
    }
}

/// Scanning sweep. Once per cycle each arc draws on and fades out in turn,
/// innermost first. `phase` ∈ [0, 1) is the cycle position; returns the arc's
/// trace progress and alpha.
fn sweep(phase: f32, arc: usize) -> (f32, f32) {
    let s = (phase.rem_euclid(1.0) - arc as f32 * SWEEP_STAGGER) / SWEEP_SPAN;
    if s <= 0.0 || s >= 1.0 {
        return (0.0, 0.0);
    }
    let alpha = if s < SWEEP_RISE {
        s / SWEEP_RISE
    } else {
        1.0 - (s - SWEEP_RISE) / (1.0 - SWEEP_RISE)
    };
    ((s / SWEEP_RISE).min(1.0), alpha)
}

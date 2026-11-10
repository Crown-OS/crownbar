//! Wi-Fi rune: Untitled's wedge under two arcs. The arcs light by link quality
//! when associated, run an outward sweep while searching, and a slash draws
//! itself on when the radio is blocked. Once a cable carries the traffic the
//! whole rune cross-fades to an Ethernet port.

use crate::scene::Scene;
use crownui::prelude::Color;
use kurbo::Rect;

use crownbar_widgets::WifiState;

use super::{
    art::{self, Art},
    ethernet, fade, lerp,
};

/// Inner to outer, each drawn left to right so a trace sweeps across.
pub(super) const ARCS: [&str; 2] = [
    "M4.65 11.15C7.7 8.64998 12.3 8.64998 15.35 11.15",
    "M1 7.34998C6.1 3.01668 13.9 3.01668 19 7.34998",
];
pub(super) const WEDGE: &str =
    "M9.05 14.8C9.68333 14.4333 10.3167 14.4333 10.95 14.8L10 15.75L9.05 14.8Z";
/// Untitled draws this one heavier than the rest of the set.
pub(super) const WEIGHT: f64 = 2.5;
const SLASH: &str = "M2.5 17.5L17.5 2.5";

static WEDGE_ART: Art = Art::weighted(&[WEDGE], WEIGHT);
static ARC_ART: [Art; 2] = [
    Art::weighted(&[ARCS[0]], WEIGHT),
    Art::weighted(&[ARCS[1]], WEIGHT),
];
static SLASH_ART: Art = Art::weighted(&[SLASH], WEIGHT);

const MIN_ALPHA: f32 = 0.01;

/// Link quality at which each arc starts lighting, and the span it takes to
/// reach full brightness.
const ARC_LIT_AT: [f32; 2] = [0.2, 0.55];
const ARC_LIT_SPAN: f32 = 0.3;

/// Sweep shape: per-arc delay into the cycle, each arc's own window, and the
/// share of that window spent drawing on (the rest fades out).
const SWEEP_STAGGER: f32 = 0.22;
const SWEEP_SPAN: f32 = 0.6;
const SWEEP_RISE: f32 = 0.42;
const SWEEP_WEDGE_ALPHA: f32 = 0.6;

pub(super) fn draw(scene: &mut Scene<'_>, b: Rect, fg: Color, state: WifiState) {
    let wired = state.wired.clamp(0.0, 1.0);
    if wired > MIN_ALPHA {
        ethernet::draw(scene, b, fade(fg, wired));
    }
    if wired < 1.0 - MIN_ALPHA {
        draw_radio(scene, b, fade(fg, 1.0 - wired), state);
    }
}

fn draw_radio(scene: &mut Scene<'_>, b: Rect, fg: Color, state: WifiState) {
    let wedge = WEDGE_ART.glyph();
    if wedge.is_empty() || ARC_ART.iter().any(|arc| arc.glyph().is_empty()) {
        return;
    }

    let strength = state.strength.clamp(0.0, 1.0);
    let off = state.off.clamp(0.0, 1.0);
    let searching = state.searching.clamp(0.0, 1.0);
    let live = 1.0 - off;

    let transform = art::place(b);
    let pen = WEDGE_ART.pen();

    // Track: the whole rune, dim, so the icon keeps its silhouette.
    let track = fade(fg, lerp(0.20, 0.12, off));
    scene.stroke(&pen, transform, track, None, wedge.path());
    for arc in &ARC_ART {
        scene.stroke(&pen, transform, track, None, arc.glyph().path());
    }

    // Wedge: the device itself — lit whenever the radio is up.
    let wedge_alpha = lerp(
        (strength * 3.0).clamp(0.0, 1.0).max(0.35),
        SWEEP_WEDGE_ALPHA,
        searching,
    ) * live;
    if wedge_alpha > MIN_ALPHA {
        scene.stroke(&pen, transform, fade(fg, wedge_alpha), None, wedge.path());
    }

    for (i, arc) in ARC_ART.iter().enumerate() {
        let lit = ((strength - ARC_LIT_AT[i]) / ARC_LIT_SPAN).clamp(0.0, 1.0);
        let (sweep_trace, sweep_alpha) = sweep(state.phase, i);
        let alpha = lerp(lit, sweep_alpha, searching) * live;
        let trace = lerp(1.0, sweep_trace, searching);
        if alpha <= MIN_ALPHA || trace <= 0.0 {
            continue;
        }
        let color = fade(fg, alpha);
        let arc = arc.glyph();
        if trace >= 1.0 {
            scene.stroke(&pen, transform, color, None, arc.path());
        } else {
            scene.stroke(&pen, transform, color, None, &arc.traced(trace));
        }
    }

    let slash = SLASH_ART.glyph();
    if off >= 1.0 {
        scene.stroke(&pen, transform, fg, None, slash.path());
    } else if off > MIN_ALPHA {
        scene.stroke(&pen, transform, fg, None, &slash.traced(off));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::{
        art::tests::assert_fits,
        tests::{BOX, INK, commands_drawn},
    };

    const POSES: [WifiState; 4] = [
        WifiState {
            strength: 1.0,
            off: 0.0,
            searching: 0.0,
            phase: 0.0,
            wired: 0.0,
        },
        WifiState {
            strength: 0.0,
            off: 1.0,
            searching: 0.0,
            phase: 0.0,
            wired: 0.0,
        },
        WifiState {
            strength: 0.0,
            off: 0.0,
            searching: 1.0,
            phase: 0.5,
            wired: 0.0,
        },
        WifiState {
            strength: 0.0,
            off: 0.0,
            searching: 0.0,
            phase: 0.0,
            wired: 1.0,
        },
    ];

    #[test]
    fn every_piece_fits_the_view_box() {
        assert_fits(&WEDGE_ART, "wifi wedge");
        assert_fits(&ARC_ART[0], "wifi inner arc");
        assert_fits(&ARC_ART[1], "wifi outer arc");
        assert_fits(&SLASH_ART, "wifi slash");
    }

    #[test]
    fn every_pose_draws() {
        for state in POSES {
            assert!(commands_drawn(|scene| draw(scene, BOX, INK, state)) > 0);
        }
    }

    #[test]
    fn a_full_link_lights_both_arcs() {
        assert!(ARC_LIT_AT.iter().all(|at| at + ARC_LIT_SPAN <= 1.0));
    }
}

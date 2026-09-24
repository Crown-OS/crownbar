//! Volume speaker. `level` ∈ [0, 1] lights the waves over a dim track, the way
//! link quality lights the Wi-Fi arcs; `muted` ∈ [0, 1] fades the waves away
//! and traces a cross on in their place. The cone never changes, so the pill
//! reads as the same object in every state.

use std::sync::OnceLock;

use vello::{
    Scene,
    kurbo::{Cap, Join, Rect, Stroke},
    peniko::Color,
};

use super::{
    fade,
    glyph::{self, Glyph},
    lerp,
};

/// The box every path is authored in, so the cross lands where the waves were
/// and the pill keeps its width whether muted or not.
const VIEW_BOX: Rect = Rect::new(0.0, 0.0, 24.0, 24.0);
const INK_FILL: f64 = 0.86;
const STROKE_PX: f64 = 1.5;
const MIN_ALPHA: f32 = 0.01;
const TRACK_ALPHA: f32 = 0.2;

const CONE_SVG: &str = "M11 4.702a.705.705 0 0 0-1.203-.498L6.413 7.587A1.4 1.4 0 0 1 5.416 8H3a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h2.416a1.4 1.4 0 0 1 .997.413l3.383 3.384A.705.705 0 0 0 11 19.298z";
const WAVE_SVG: [&str; 2] = ["M16 9a5 5 0 0 1 0 6", "M19.364 5.636a9 9 0 0 1 0 12.728"];
const CROSS_SVG: &str = "M22 9l-6 6 M16 9l6 6";

/// Level at which each wave starts lighting, and the span it takes to reach
/// full brightness.
const WAVE_LIT_AT: [f32; 2] = [0.05, 0.5];
const WAVE_LIT_SPAN: f32 = 0.3;

struct Speaker {
    cone: Glyph,
    waves: [Glyph; 2],
    cross: Glyph,
}

fn speaker() -> &'static Speaker {
    static CACHE: OnceLock<Speaker> = OnceLock::new();
    CACHE.get_or_init(|| Speaker {
        cone: Glyph::parse(CONE_SVG),
        waves: WAVE_SVG.map(Glyph::parse),
        cross: Glyph::parse(CROSS_SVG),
    })
}

pub(super) fn draw(scene: &mut Scene, b: Rect, fg: Color, level: f32, muted: f32) {
    let speaker = speaker();
    if speaker.cone.is_empty() {
        return;
    }
    let (level, muted) = (level.clamp(0.0, 1.0), muted.clamp(0.0, 1.0));
    let (transform, scale) = glyph::fit(VIEW_BOX, b, INK_FILL);
    let stroke = Stroke::new(STROKE_PX / scale)
        .with_caps(Cap::Round)
        .with_join(Join::Round);

    scene.stroke(&stroke, transform, fg, None, speaker.cone.path());

    for (wave, lit_at) in speaker.waves.iter().zip(WAVE_LIT_AT) {
        let lit = ((level - lit_at) / WAVE_LIT_SPAN).clamp(0.0, 1.0);
        let alpha = lerp(TRACK_ALPHA, 1.0, lit) * (1.0 - muted);
        if alpha > MIN_ALPHA {
            scene.stroke(&stroke, transform, fade(fg, alpha), None, wave.path());
        }
    }

    if muted >= 1.0 {
        scene.stroke(&stroke, transform, fg, None, speaker.cross.path());
    } else if muted > MIN_ALPHA {
        scene.stroke(&stroke, transform, fg, None, &speaker.cross.traced(muted));
    }
}

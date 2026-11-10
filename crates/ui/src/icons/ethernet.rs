//! Ethernet rune: a port, its latch and four pins (Lucide geometry). The
//! network icon cross-fades to it while a cable carries the traffic.

use std::sync::OnceLock;

use crate::scene::Scene;
use crownui::prelude::Color;
use kurbo::{Rect, Stroke};

use super::glyph::{self, Glyph};

const PORT_SVG: &str = "M15 20L18 17h2a2 2 0 0 0 2-2V6a2 2 0 0 0-2-2H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h2l3 3zM6 8v1M10 8v1M14 8v1M18 8v1";

/// Smaller than the Wi-Fi rune's fit: the port is a solid outline where the
/// arcs are open, and at the same width it reads heavier than its neighbours.
const INK_FILL: f64 = 0.82;
const STROKE_PX: f64 = 2.0;

fn port() -> &'static Glyph {
    static CACHE: OnceLock<Glyph> = OnceLock::new();
    CACHE.get_or_init(|| Glyph::parse(PORT_SVG))
}

pub(super) fn draw(scene: &mut Scene<'_>, b: Rect, color: Color) {
    let port = port();
    if port.is_empty() {
        return;
    }
    let (transform, scale) = glyph::fit(port.ink(), b, INK_FILL);
    scene.stroke(
        &Stroke::new(STROKE_PX / scale),
        transform,
        color,
        None,
        port.path(),
    );
}

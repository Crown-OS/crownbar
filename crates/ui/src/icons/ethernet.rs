//! Ethernet rune: a port, its latch and four pins. Untitled has no such glyph,
//! so it is drawn to the set's rules — the 24 grid scaled into the 20 box,
//! 2.5 corner radii, the set's stroke. The network icon cross-fades to it
//! while a cable carries the traffic, and the panels name a wired link by it.

use crate::scene::Scene;
use crownui::prelude::Color;
use kurbo::Rect;

use super::art::{self, Art};

pub(super) static PORT: Art = Art::new(&[
    "M7.5 16.6667H12.5L14.1667 15H16.25A2.0833 2.0833 0 0 0 18.3333 12.9167V5.4167A2.0833 2.0833 0 0 0 16.25 3.3333H3.75A2.0833 2.0833 0 0 0 1.6667 5.4167V12.9167A2.0833 2.0833 0 0 0 3.75 15H5.8333L7.5 16.6667Z",
    "M5.8333 6.6667V8.3333M8.6111 6.6667V8.3333M11.3889 6.6667V8.3333M14.1667 6.6667V8.3333",
]);

pub(super) fn draw(scene: &mut Scene<'_>, b: Rect, color: Color) {
    let port = PORT.glyph();
    if port.is_empty() {
        return;
    }
    scene.stroke(&PORT.pen(), art::place(b), color, None, port.path());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::art::tests::assert_fits;

    #[test]
    fn the_port_fits_the_view_box() {
        assert_fits(&PORT, "ethernet port");
    }
}

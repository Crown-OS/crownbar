//! Glyphs whose ink is not centred in their box look off-centre in a round
//! badge: a Wi-Fi fan is all top, a coffee cup carries its handle to one side.
//! A badge nudges them by what the design measured at its glyph size.

use crownbar_widgets::{Icon, Rune};
use kurbo::Vec2;

/// The glyph size the nudges were measured at.
const MEASURED_AT: f64 = 18.0;

/// Where a glyph sits, which decides whether it is nudged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Seat {
    /// Free-standing: its box is what lines up.
    Bare,
    /// Centred in a round badge, where its ink has to look centred.
    Disc,
}

impl Seat {
    /// How far `icon`, drawn `size` square, moves to look centred here.
    pub fn nudge(self, icon: Icon, size: f32) -> Vec2 {
        match self {
            Self::Bare => Vec2::ZERO,
            Self::Disc => measured(icon) * (f64::from(size) / MEASURED_AT),
        }
    }
}

fn measured(icon: Icon) -> Vec2 {
    match icon {
        Icon::Caffeine { on } => Vec2::new(0.5 * f64::from(1.0 - on.clamp(0.0, 1.0)), 0.0),
        Icon::Wifi(_) | Icon::Rune(Rune::Wifi) => Vec2::new(0.0, 0.7),
        Icon::Rune(Rune::Headphones) => Vec2::new(0.0, -0.5),
        Icon::Rune(Rune::Display) => Vec2::new(0.0, 0.4),
        Icon::Rune(Rune::Moon) => Vec2::new(0.4, -0.2),
        _ => Vec2::ZERO,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_badge_nudges_and_by_its_size() {
        let wifi = Icon::Rune(Rune::Wifi);
        assert_eq!(Seat::Bare.nudge(wifi, 18.0), Vec2::ZERO);
        assert_eq!(Seat::Disc.nudge(wifi, 18.0), Vec2::new(0.0, 0.7));
        assert_eq!(Seat::Disc.nudge(wifi, 36.0), Vec2::new(0.0, 1.4));
        assert_eq!(Seat::Disc.nudge(Icon::Rune(Rune::Laptop), 18.0), Vec2::ZERO);
    }

    #[test]
    fn the_cup_moves_back_as_it_fills() {
        let nudge = |on| Seat::Disc.nudge(Icon::Caffeine { on }, 18.0).x;
        assert_eq!(nudge(0.0), 0.5);
        assert_eq!(nudge(1.0), 0.0);
    }
}

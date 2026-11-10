//! A live activity's chip: a vertical wash in its family's hue over a soft
//! drop shadow, with a one-pixel highlight along its top inside edge, and ink
//! of its own wherever it is tinted.
//!
//! Only the resting chip is designed; under the pointer and while its panel is
//! open its wash is the resting one made stronger by [`HOVER_BOOST`] and
//! [`ACTIVE_BOOST`].

use crownbar_integrations::live::wire::Accent;
use crownui::prelude::{Color, Shadow, Vector};

use super::{PillState, Stops, rgb, with_alpha};

/// How much stronger a chip's wash is under the pointer, and while its panel
/// is open, than at rest.
const HOVER_BOOST: f32 = 1.2;
const ACTIVE_BOOST: f32 = 1.4;

const SHADOW_BLUR: f32 = 2.0;
const SHADOW_DROP: f32 = 1.0;
/// How far below the chip's top edge its highlight reaches.
pub const HIGHLIGHT_DROP: f64 = 1.0;

const WHITE: Color = rgb(0xFF_FF_FF);
const BLACK: Color = rgb(0x00_00_00);
const NEAR_BLACK: Color = rgb(0x11_11_11);

/// A tinted chip's label and glyph, whatever the wallpaper under it.
const LIGHT_INK: Color = NEAR_BLACK;
const DARK_INK: Color = WHITE;

/// Every tinted wash's top and bottom alpha in light mode.
const LIGHT_ALPHAS: [f32; 2] = [0.60, 0.70];
const LIGHT_SHADOW_ALPHA: f32 = 0.14;
const LIGHT_HIGHLIGHT: Color = with_alpha(WHITE, 0.45);
const DARK_SHADOW: Color = with_alpha(BLACK, 0.25);
const DARK_HIGHLIGHT: Color = with_alpha(WHITE, 0.12);

/// The hue an accent tints a chip with: its wash from top to bottom, the
/// darker hue its light-mode shadow is cast in, and the wash's alphas in dark
/// mode, where each hue is weighed by eye.
struct Family {
    top: Color,
    bottom: Color,
    light_shadow: Color,
    dark_alphas: [f32; 2],
}

const CORAL: Family = Family::of(0xFF_B1_99, 0xFF_7A_50, 0x98_53_3D, [0.58, 0.67]);
const BLUE: Family = Family::of(0x93_C5_FD, 0x3B_82_F6, 0x29_5B_AC, [0.59, 0.70]);
const VIOLET: Family = Family::of(0xC4_B5_FD, 0x8B_5C_F6, 0x61_40_AC, [0.59, 0.70]);
const AMBER: Family = Family::of(0xFD_E6_8A, 0xF5_9E_0B, 0xAB_6F_08, [0.49, 0.61]);
const GREEN: Family = Family::of(0x86_EF_AC, 0x22_C5_5E, 0x18_8A_42, [0.52, 0.63]);
const RED: Family = Family::of(0xFC_A5_A5, 0xEF_44_44, 0x88_1B_1B, [0.60, 0.70]);

impl Family {
    const fn of(top: u32, bottom: u32, light_shadow: u32, dark_alphas: [f32; 2]) -> Self {
        Self {
            top: rgb(top),
            bottom: rgb(bottom),
            light_shadow: rgb(light_shadow),
            dark_alphas,
        }
    }

    const fn look(&self, dark_mode: bool) -> ChipLook {
        let ([top, bottom], shadow, highlight) = if dark_mode {
            (self.dark_alphas, DARK_SHADOW, DARK_HIGHLIGHT)
        } else {
            (
                LIGHT_ALPHAS,
                with_alpha(self.light_shadow, LIGHT_SHADOW_ALPHA),
                LIGHT_HIGHLIGHT,
            )
        };
        ChipLook::of(
            with_alpha(self.top, top),
            with_alpha(self.bottom, bottom),
            shadow,
            highlight,
        )
    }
}

const fn family(accent: Accent) -> Option<&'static Family> {
    Some(match accent {
        Accent::Neutral => return None,
        Accent::Coral => &CORAL,
        Accent::Blue => &BLUE,
        Accent::Green => &GREEN,
        Accent::Amber => &AMBER,
        Accent::Red => &RED,
        Accent::Violet => &VIOLET,
    })
}

/// An untinted chip: a faint wash of the mode's ink.
const NEUTRAL_LIGHT: ChipLook = ChipLook::of(
    with_alpha(NEAR_BLACK, 0.04),
    with_alpha(NEAR_BLACK, 0.08),
    with_alpha(NEAR_BLACK, 0.06),
    LIGHT_HIGHLIGHT,
);
const NEUTRAL_DARK: ChipLook = ChipLook::of(
    with_alpha(WHITE, 0.12),
    with_alpha(WHITE, 0.06),
    DARK_SHADOW,
    with_alpha(WHITE, 0.10),
);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChipLook {
    /// The wash at rest, top to bottom.
    pub fill: Stops,
    pub shadow: Shadow,
    /// The crescent along the top inside edge.
    pub highlight: Color,
}

impl ChipLook {
    const fn of(top: Color, bottom: Color, shadow: Color, highlight: Color) -> Self {
        Self {
            fill: Stops {
                start: top,
                end: bottom,
            },
            shadow: Shadow::new(shadow, SHADOW_BLUR, Vector::new(0.0, SHADOW_DROP)),
            highlight,
        }
    }

    /// The wash that, laid over the resting one at [`boost_opacity`], makes
    /// it as strong as the state that opacity was taken for.
    pub fn boost(&self) -> Stops {
        let lift = |color: Color| with_alpha(color, lift_to(color.a, ACTIVE_BOOST));
        Stops {
            start: lift(self.fill.start),
            end: lift(self.fill.end),
        }
    }
}

/// The alpha a layer of the same color needs over `alpha` to bring it to
/// `factor` times itself, as far as full coverage allows.
fn lift_to(alpha: f32, factor: f32) -> f32 {
    let target = (alpha * factor).min(1.0);
    if alpha >= 1.0 {
        return 0.0;
    }
    (target - alpha) / (1.0 - alpha)
}

/// How much of [`ChipLook::boost`] a chip in `state` shows. A layer's
/// contribution grows linearly with its opacity, so this is the state's share
/// of the full boost.
pub fn boost_opacity(state: PillState) -> f32 {
    let factor = match state {
        PillState::Rest => 1.0,
        PillState::Hovered => HOVER_BOOST,
        PillState::Active => ACTIVE_BOOST,
    };
    (factor - 1.0) / (ACTIVE_BOOST - 1.0)
}

/// The chips of one mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chips {
    dark_mode: bool,
}

impl Chips {
    pub const fn new(dark_mode: bool) -> Self {
        Self { dark_mode }
    }

    pub fn look(self, accent: Accent) -> ChipLook {
        match family(accent) {
            Some(family) => family.look(self.dark_mode),
            None if self.dark_mode => NEUTRAL_DARK,
            None => NEUTRAL_LIGHT,
        }
    }

    /// A tinted chip's ink, which its own wash rather than the wallpaper
    /// decides; `None` for an untinted chip, which reads the wallpaper like
    /// any other pill.
    pub fn ink(self, accent: Accent) -> Option<Color> {
        family(accent).map(|_| if self.dark_mode { DARK_INK } else { LIGHT_INK })
    }
}

/// `accent`'s hue at full strength: the bottom of its chip's wash.
pub fn hue(accent: Accent) -> Option<Color> {
    family(accent).map(|family| family.bottom)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TINTED: [Accent; 6] = [
        Accent::Coral,
        Accent::Blue,
        Accent::Violet,
        Accent::Amber,
        Accent::Green,
        Accent::Red,
    ];
    const LIGHT: Chips = Chips::new(false);
    const DARK: Chips = Chips::new(true);

    fn alphas(look: ChipLook) -> [f32; 2] {
        [look.fill.start.a, look.fill.end.a]
    }

    #[test]
    fn every_light_wash_has_the_same_alphas() {
        for accent in TINTED {
            assert_eq!(alphas(LIGHT.look(accent)), [0.60, 0.70], "{accent:?}");
        }
    }

    #[test]
    fn dark_washes_are_weighed_per_hue() {
        let expected = [
            [0.58, 0.67],
            [0.59, 0.70],
            [0.59, 0.70],
            [0.49, 0.61],
            [0.52, 0.63],
            [0.60, 0.70],
        ];
        for (accent, expected) in TINTED.into_iter().zip(expected) {
            assert_eq!(alphas(DARK.look(accent)), expected, "{accent:?}");
        }
    }

    #[test]
    fn every_family_has_the_design_hues_and_light_shadow() {
        let expected = [
            (0xFF_B1_99, 0xFF_7A_50, 0x98_53_3D),
            (0x93_C5_FD, 0x3B_82_F6, 0x29_5B_AC),
            (0xC4_B5_FD, 0x8B_5C_F6, 0x61_40_AC),
            (0xFD_E6_8A, 0xF5_9E_0B, 0xAB_6F_08),
            (0x86_EF_AC, 0x22_C5_5E, 0x18_8A_42),
            (0xFC_A5_A5, 0xEF_44_44, 0x88_1B_1B),
        ];
        for (accent, (top, bottom, shadow)) in TINTED.into_iter().zip(expected) {
            for chips in [LIGHT, DARK] {
                let look = chips.look(accent);
                assert_eq!(with_alpha(look.fill.start, 1.0), rgb(top), "{accent:?}");
                assert_eq!(with_alpha(look.fill.end, 1.0), rgb(bottom), "{accent:?}");
            }
            let light = LIGHT.look(accent).shadow.color;
            assert_eq!(light, with_alpha(rgb(shadow), 0.14), "{accent:?}");
        }
    }

    #[test]
    fn red_is_a_family_of_its_own() {
        let look = LIGHT.look(Accent::Red);
        assert_eq!(with_alpha(look.fill.start, 1.0), rgb(0xFC_A5_A5));
        assert_eq!(with_alpha(look.fill.end, 1.0), rgb(0xEF_44_44));
        assert_eq!(hue(Accent::Red), Some(rgb(0xEF_44_44)));
    }

    #[test]
    fn light_shadows_take_the_hue_and_dark_ones_are_black() {
        let light = LIGHT.look(Accent::Blue).shadow;
        assert_eq!(light.color, with_alpha(rgb(0x29_5B_AC), 0.14));
        assert_eq!((light.blur, light.offset), (2.0, Vector::new(0.0, 1.0)));
        assert_eq!(DARK.look(Accent::Blue).shadow.color, DARK_SHADOW);
        assert_eq!(LIGHT.look(Accent::Blue).highlight.a, 0.45);
        assert_eq!(DARK.look(Accent::Blue).highlight.a, 0.12);
    }

    #[test]
    fn an_untinted_chip_is_a_faint_wash_of_the_ink() {
        assert_eq!(alphas(LIGHT.look(Accent::Neutral)), [0.04, 0.08]);
        assert_eq!(alphas(DARK.look(Accent::Neutral)), [0.12, 0.06]);
        assert_eq!(DARK.look(Accent::Neutral).highlight.a, 0.10);
        assert_eq!(hue(Accent::Neutral), None);
    }

    #[test]
    fn tinted_ink_follows_the_mode_and_untinted_has_none() {
        for accent in TINTED {
            assert_eq!(LIGHT.ink(accent), Some(rgb(0x11_11_11)), "{accent:?}");
            assert_eq!(DARK.ink(accent), Some(rgb(0xFF_FF_FF)), "{accent:?}");
        }
        assert_eq!(LIGHT.ink(Accent::Neutral), None);
        assert_eq!(DARK.ink(Accent::Neutral), None);
    }

    #[test]
    fn the_boost_scales_the_resting_wash() {
        for chips in [LIGHT, DARK] {
            for accent in TINTED.into_iter().chain([Accent::Neutral]) {
                let look = chips.look(accent);
                let boost = look.boost();
                for (state, factor) in [
                    (PillState::Rest, 1.0),
                    (PillState::Hovered, HOVER_BOOST),
                    (PillState::Active, ACTIVE_BOOST),
                ] {
                    let opacity = boost_opacity(state);
                    for (rest, layer) in
                        [(look.fill.start, boost.start), (look.fill.end, boost.end)]
                    {
                        let shown = rest.a + opacity * layer.a * (1.0 - rest.a);
                        let wanted = (rest.a * factor).min(1.0);
                        assert!(
                            (shown - wanted).abs() < 1e-5,
                            "{accent:?} {state:?}: {shown} != {wanted}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn an_opaque_wash_needs_no_boost() {
        assert_eq!(lift_to(1.0, ACTIVE_BOOST), 0.0);
        assert_eq!(lift_to(0.9, ACTIVE_BOOST), 1.0);
    }
}

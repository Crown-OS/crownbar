//! The panels' glass: a smoked body over a deep, vivid blur, a specular rim
//! that catches the light at the top-left and pools again along the bottom,
//! and the kit theme the rows inside are drawn in — white ink, glass bubbles
//! for anything at rest, the desktop's accent for anything on, and a plain
//! white wash for whatever is chosen.

use crownui::{
    kit::{GradientStops, MenuColors, PopoverColors, SurfaceColors, TextColors, Theme},
    prelude::Color,
};

use super::{rgb, with_alpha};

/// A chosen badge's glyph, dark on the white knob in either mode.
const ON_KNOB: Color = rgb(0x10_12_18);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glass {
    /// The tint laid over the blur, at the panel's opacity.
    pub body: Color,
    /// The specular rim, from the lit top-left corner through the shaded
    /// middle to the bottom-right.
    pub rim: [Color; 3],
    /// Light pooled under the top edge, and its fainter echo along the bottom.
    pub gloss: Color,
    pub ink: Color,
    pub ink_muted: Color,
    /// A hovered row's wash, and the rim of a bubble.
    pub wash: Color,
    /// A control at rest: a switch that is off, an idle badge.
    pub bubble: Color,
}

const DARK: Glass = Glass {
    body: rgb(0x0A_0B_0F),
    rim: [
        with_alpha(rgb(0xFF_FF_FF), 0.34),
        with_alpha(rgb(0xF2_D6_E6), 0.08),
        with_alpha(rgb(0xD4_EA_FF), 0.24),
    ],
    gloss: with_alpha(rgb(0xFF_FF_FF), 0.07),
    ink: rgb(0xFF_FF_FF),
    ink_muted: with_alpha(rgb(0xFF_FF_FF), 0.58),
    wash: with_alpha(rgb(0xFF_FF_FF), 0.09),
    bubble: with_alpha(rgb(0xFF_FF_FF), 0.20),
};

const LIGHT: Glass = Glass {
    body: rgb(0xF4_F6_FA),
    rim: [
        with_alpha(rgb(0xFF_FF_FF), 0.90),
        with_alpha(rgb(0xF6_DC_EA), 0.30),
        with_alpha(rgb(0xD8_EC_FF), 0.70),
    ],
    gloss: with_alpha(rgb(0xFF_FF_FF), 0.40),
    ink: rgb(0x10_12_18),
    ink_muted: with_alpha(rgb(0x10_12_18), 0.55),
    wash: with_alpha(rgb(0x10_12_18), 0.06),
    bubble: with_alpha(rgb(0x10_12_18), 0.12),
};

impl Glass {
    /// The glass for `theme`'s mode, its body at `opacity`.
    pub fn new(theme: &Theme, opacity: f32) -> Self {
        let glass = if theme.mode.is_dark() { DARK } else { LIGHT };
        Self {
            body: with_alpha(glass.body, opacity),
            ..glass
        }
    }

    /// `base` with every slot a panel row paints taken from the glass.
    pub fn rows_theme(&self, base: &Theme) -> Theme {
        Theme {
            surface: SurfaceColors {
                hover: self.wash,
                ..base.surface
            },
            text: TextColors {
                primary: self.ink,
                body: self.ink,
                muted: self.ink_muted,
                ..base.text
            },
            toggle_off: GradientStops::new(self.bubble, self.wash),
            popover: PopoverColors {
                bg: self.body,
                text: self.ink,
                muted_text: self.ink_muted,
                hover_bg: self.wash,
                ring: base.accent,
                ..base.popover
            },
            menu: MenuColors {
                separator: self.wash,
                selected_bg: self.ink,
                ..base.menu
            },
            ..*base
        }
    }
}

/// `rows`, with a chosen row's badge inked in place of the accent.
pub fn selection_theme(rows: &Theme) -> Theme {
    Theme {
        accent: GradientStops::new(ON_KNOB, ON_KNOB),
        ..*rows
    }
}

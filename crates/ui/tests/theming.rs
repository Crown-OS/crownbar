//! The bar's palette follows the desktop's.
//!
//! What this guards is the failure that looks fine in whichever mode you
//! developed in: a slot that was written as a constant, or mapped to a kit
//! slot that happens not to move, and so stays put when the desktop flips.

use crownbar_integrations::live::wire::Accent;
use crownbar_ui::theme::{Palette, PillState};
use crownconfig::{AccentColor, ThemeOverrides};
use crownui::{
    kit::{Theme, ThemeMode},
    prelude::Color,
};

/// One named slot, for the tables below.
type Slot = (&'static str, fn(&Palette) -> Color);

fn palette_for(mode: ThemeMode, accent: AccentColor) -> Palette {
    Palette::new(
        &Theme::for_mode(mode, accent),
        0.0,
        &ThemeOverrides::default(),
        None,
    )
}

/// Slots that describe a surface or the text on it have to be different in the
/// two modes. Listed one by one, and named, so a failure says which slot
/// stopped following rather than just "the palettes are equal".
#[test]
fn surface_and_text_slots_follow_the_mode() {
    let dark = palette_for(ThemeMode::Dark, AccentColor::Purple);
    let light = palette_for(ThemeMode::Light, AccentColor::Purple);

    let slots: [Slot; 7] = [
        ("bar_fg", |p| p.bar_fg),
        ("bar_fg_hover", |p| p.bar_fg_hover),
        ("fg_dim", |p| p.fg_dim),
        ("glass.body", |p| p.glass.body),
        ("glass.wash", |p| p.glass.wash),
        ("pill_hover", |p| p.pill_hover),
        ("warning", |p| p.warning),
    ];
    for (name, get) in slots {
        assert_ne!(
            get(&dark),
            get(&light),
            "`{name}` is the same in both modes, so it does not follow the theme",
        );
    }
}

/// A panel body has to be readable over a wallpaper, which means it is never
/// fully transparent and never fully opaque — whatever the transparency
/// setting says.
#[test]
fn the_panel_body_stays_translucent() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        for transparency in [0.0, 0.5, 1.0] {
            let theme = Theme::for_mode(mode, AccentColor::Purple);
            let alpha = Palette::new(&theme, transparency, &ThemeOverrides::default(), None)
                .glass
                .body
                .a;
            assert!(
                (0.5..1.0).contains(&alpha),
                "{mode:?} at {transparency} leaves alpha {alpha}: nothing of the blur, or nothing to read on",
            );
        }
    }
}

/// The dark glass is denser than the light, and the transparency setting
/// thins each only down to a floor of its own.
#[test]
fn each_mode_has_its_own_panel_opacity() {
    let alpha = |mode, transparency| {
        Palette::new(
            &Theme::for_mode(mode, AccentColor::Purple),
            transparency,
            &ThemeOverrides::default(),
            None,
        )
        .glass
        .body
        .a
    };
    assert!((alpha(ThemeMode::Light, 0.0) - 0.58).abs() < 1e-6);
    assert!((alpha(ThemeMode::Dark, 0.0) - 0.82).abs() < 1e-6);
    assert!((alpha(ThemeMode::Dark, 0.04) - 0.78).abs() < 1e-6);
    assert!(alpha(ThemeMode::Dark, 1.0) >= 0.7);
    assert!(alpha(ThemeMode::Dark, 1.0) > alpha(ThemeMode::Light, 1.0));
}

/// A live activity's chip paints its own wash, tinted or not; any other pill
/// is bare at rest and filled only under the pointer.
#[test]
fn chips_paint_their_own_wash_and_plain_pills_none_at_rest() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let palette = palette_for(mode, AccentColor::Purple);
        for state in [PillState::Rest, PillState::Hovered, PillState::Active] {
            for accent in [Accent::Neutral, Accent::Red, Accent::Coral] {
                assert_eq!(palette.pill_fill(state, Some(accent)).a, 0.0, "{mode:?}");
            }
        }
        assert_eq!(palette.pill_fill(PillState::Rest, None).a, 0.0, "{mode:?}");
        assert!(
            palette.pill_fill(PillState::Hovered, None).a > 0.0,
            "{mode:?}"
        );
        let neutral = palette.chips.look(Accent::Neutral);
        assert!(neutral.fill.start.a > 0.0 && neutral.shadow.color.a > 0.0);
    }
}

/// The bar's ground is the panels' body: one surface in two places.
#[test]
fn the_bar_and_its_panels_share_a_body() {
    let palette = palette_for(ThemeMode::Dark, AccentColor::Purple);
    assert_eq!(palette.bar_fill, palette.glass.body);
    assert!(palette.pill_active.a > palette.pill_hover.a);
}

/// `theme.ron` may retint the bar's own ink, per mode: a mode's tokens tint
/// the ink that mode draws in, and leave the other mode's ink alone.
#[test]
fn theme_overrides_retint_the_bar_foreground() {
    let mut overrides = ThemeOverrides::default();
    overrides.dark.insert("bar.fg".into(), "#ff0000".into());
    overrides
        .dark
        .insert("bar.fg_hover".into(), "#00ff0080".into());
    let theme = Theme::for_mode(ThemeMode::Dark, AccentColor::Purple);
    let dark = Palette::new(&theme, 0.0, &overrides, None);
    assert_eq!(dark.bar_fg, Color::new(1.0, 0.0, 0.0, 1.0));
    assert_eq!(dark.bar_fg_hover, Color::new(0.0, 1.0, 0.0, 128.0 / 255.0));

    let light = palette_for(ThemeMode::Light, AccentColor::Purple);
    let light_overridden = Palette::new(
        &Theme::for_mode(ThemeMode::Light, AccentColor::Purple),
        0.0,
        &overrides,
        None,
    );
    assert_eq!(
        (light.bar_fg, light.bar_fg_hover),
        (light_overridden.bar_fg, light_overridden.bar_fg_hover)
    );
}

/// The panels' rows light up in the user's accent, not one of the bar's own.
#[test]
fn panel_rows_follow_the_accent() {
    for accent in [AccentColor::Purple, AccentColor::Blue] {
        let theme = Theme::for_mode(ThemeMode::Dark, accent);
        let palette = Palette::new(&theme, 0.0, &ThemeOverrides::default(), None);
        assert_eq!(palette.panel_theme.accent, theme.accent, "{accent:?}");
        assert_eq!(palette.panel_theme.popover.ring, theme.accent, "{accent:?}");
    }
}

/// A panel's slider is the settings app's slider: the accent fill on the kit's
/// own track and knob.
#[test]
fn panel_sliders_match_the_settings_app() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let theme = Theme::for_mode(mode, AccentColor::Blue);
        let rows = Palette::new(&theme, 0.0, &ThemeOverrides::default(), None).panel_theme;
        assert_eq!(rows.control, theme.control, "{mode:?}");
    }
}

/// What a panel marks as chosen is drawn in the user's accent, in either mode.
#[test]
fn a_chosen_row_follows_the_accent() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let [purple, blue] = [AccentColor::Purple, AccentColor::Blue].map(|accent| {
            let theme = Theme::for_mode(mode, accent);
            let chosen = palette_for(mode, accent).panel_theme.accent;
            assert_eq!(chosen, theme.accent, "{mode:?} {accent:?}");
            chosen
        });
        assert_ne!(purple, blue, "{mode:?}");
    }
}

/// `0xRRGGBB` at `alpha`, as the design file states its colours.
fn design(hex: u32, alpha: f32) -> Color {
    Color {
        a: alpha,
        ..Color::from_rgba8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8, u8::MAX)
    }
}

/// The panel's slots land on the design file's values in each mode: the
/// glass body, the ink and its muted form, the separator hairline, the hover
/// wash, a switch at rest, and the danger ink a hang-up reads in.
#[test]
fn panel_rows_resolve_to_the_design() {
    struct Expected {
        body: Color,
        ink: Color,
        muted: Color,
        separator: Color,
        hover: Color,
        switch_off: [Color; 2],
        danger: Color,
    }
    let cases = [
        (
            ThemeMode::Light,
            Expected {
                body: design(0xF4_F6_FA, 0.58),
                ink: design(0x10_12_18, 1.0),
                muted: design(0x10_12_18, 0.55),
                separator: design(0x10_12_18, 0.06),
                hover: design(0x10_12_18, 0.06),
                switch_off: [design(0x10_12_18, 0.12), design(0x10_12_18, 0.06)],
                danger: design(0xC2_27_27, 1.0),
            },
        ),
        (
            ThemeMode::Dark,
            Expected {
                body: design(0x0A_0B_0F, 0.82),
                ink: design(0xFF_FF_FF, 1.0),
                muted: design(0xFF_FF_FF, 0.66),
                separator: design(0xFF_FF_FF, 0.12),
                hover: design(0xFF_FF_FF, 0.09),
                switch_off: [design(0xFF_FF_FF, 0.20), design(0xFF_FF_FF, 0.09)],
                danger: design(0xF8_71_71, 1.0),
            },
        ),
    ];
    for (mode, expected) in cases {
        let rows = palette_for(mode, AccentColor::Purple).panel_theme;
        assert_eq!(rows.mode, mode);
        assert_eq!(rows.popover.bg, expected.body, "{mode:?} body");
        assert_eq!(rows.popover.text, expected.ink, "{mode:?} ink");
        assert_eq!(rows.text.muted, expected.muted, "{mode:?} muted ink");
        assert_eq!(rows.popover.muted_text, expected.muted, "{mode:?} muted");
        assert_eq!(
            rows.menu.separator, expected.separator,
            "{mode:?} separator"
        );
        assert_eq!(rows.popover.hover_bg, expected.hover, "{mode:?} hover");
        assert_eq!(
            [rows.toggle_off.start, rows.toggle_off.end],
            expected.switch_off,
            "{mode:?} switch at rest",
        );
        assert_eq!(rows.status.danger, expected.danger, "{mode:?} danger");
        assert_eq!(rows.accent.end, design(0x6D_48_E8, 1.0), "{mode:?} accent");
    }
}

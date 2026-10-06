//! The bar's palette follows the desktop's.
//!
//! What this guards is the failure that looks fine in whichever mode you
//! developed in: a slot that was written as a constant, or mapped to a kit
//! slot that happens not to move, and so stays put when the desktop flips.

use crownbar::theme::Palette;
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
        ("panel_bg", |p| p.panel_bg),
        ("panel_rim", |p| p.panel_rim),
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
            let alpha = Palette::new(&theme, transparency, &ThemeOverrides::default())
                .panel_bg
                .a;
            assert!(
                (0.5..1.0).contains(&alpha),
                "{mode:?} at {transparency} leaves alpha {alpha}: nothing of the blur, or nothing to read on",
            );
        }
    }
}

/// The bar's ground is the panels' body: one surface in two places.
#[test]
fn the_bar_and_its_panels_share_a_body() {
    let palette = palette_for(ThemeMode::Dark, AccentColor::Purple);
    assert_eq!(palette.bar_fill, palette.panel_bg);
    assert!(palette.pill_active.a > palette.pill_hover.a);
}

/// `theme.ron` may retint the bar's own ink, per mode, and only that mode.
#[test]
fn theme_overrides_retint_the_bar_foreground() {
    let mut overrides = ThemeOverrides::default();
    overrides.dark.insert("bar.fg".into(), "#ff0000".into());
    overrides
        .dark
        .insert("bar.fg_hover".into(), "#00ff0080".into());
    let theme = Theme::for_mode(ThemeMode::Dark, AccentColor::Purple);
    let dark = Palette::new(&theme, 0.0, &overrides);
    assert_eq!(dark.bar_fg, Color::new(1.0, 0.0, 0.0, 1.0));
    assert_eq!(dark.bar_fg_hover, Color::new(0.0, 1.0, 0.0, 128.0 / 255.0));

    let light = palette_for(ThemeMode::Light, AccentColor::Purple);
    let light_overridden = Palette::new(
        &Theme::for_mode(ThemeMode::Light, AccentColor::Purple),
        0.0,
        &overrides,
    );
    assert_eq!(light, light_overridden);
}

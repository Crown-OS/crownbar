//! The bar's palette follows the desktop's.
//!
//! This is its own test binary rather than a `#[cfg(test)]` module because the
//! palette is process-global, and within the binary it is one `#[test]`
//! calling several functions because cargo runs test *functions* on parallel
//! threads. Both halves of that are why `crownuikit`'s own `tests/theming.rs`
//! is shaped the same way.
//!
//! What it guards is the failure that looks fine in whichever mode you
//! developed in: a slot that was written as a constant, or mapped to a kit
//! slot that happens not to move, and so stays put when the desktop flips.

use crownbar::theme::{self, Palette};
use vello::peniko::Color;

/// One named slot, for the table below.
type Slot = (&'static str, fn(&Palette) -> Color);
use crownos_config::schema::AccentColor;
use crownuikit::config::{set_theme_immediately, Theme, ThemeMode};

fn palette_for(mode: ThemeMode, accent: AccentColor) -> Palette {
    set_theme_immediately(Theme::for_mode(mode, accent));
    theme::palette()
}

#[test]
fn the_palette_follows_the_desktop() {
    surface_and_text_slots_follow_the_mode();
    the_panel_body_stays_translucent();
    accent_slots_follow_the_accent_not_the_mode();
    the_toggle_track_has_somewhere_to_travel();
    a_palette_change_moves_the_epoch();
}

/// Slots that describe a surface or the text on it have to be different in the
/// two modes. Listed one by one, and named, so a failure says which slot
/// stopped following rather than just "the palettes are equal".
fn surface_and_text_slots_follow_the_mode() {
    let dark = palette_for(ThemeMode::Dark, AccentColor::Purple);
    let light = palette_for(ThemeMode::Light, AccentColor::Purple);

    let slots: [Slot; 12] = [
        ("fg", |p| p.fg),
        ("fg_muted", |p| p.fg_muted),
        ("fg_dim", |p| p.fg_dim),
        ("panel_bg", |p| p.panel_bg),
        ("panel_rim", |p| p.panel_rim),
        ("panel_shadow", |p| p.panel_shadow),
        ("pill_hover", |p| p.pill_hover),
        ("row_hover", |p| p.row_hover),
        ("separator", |p| p.separator),
        ("badge_idle", |p| p.badge_idle),
        ("track", |p| p.track),
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
/// fully transparent and never fully opaque.
fn the_panel_body_stays_translucent() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let alpha = palette_for(mode, AccentColor::Purple).panel_bg.components[3];
        assert!(
            (0.5..1.0).contains(&alpha),
            "{mode:?} panel alpha {alpha} leaves nothing of the blur, or nothing to read on",
        );
    }
}

/// The accent-derived slots follow the accent and nothing else: two modes
/// agree on them, two accents do not. This is what makes a toggle on the bar
/// and a toggle in crownsettings the same color.
fn accent_slots_follow_the_accent_not_the_mode() {
    let dark = palette_for(ThemeMode::Dark, AccentColor::Purple);
    let light = palette_for(ThemeMode::Light, AccentColor::Purple);
    assert_eq!(dark.accent.start, light.accent.start);
    assert_eq!(dark.accent.end, light.accent.end);
    assert_eq!(dark.row_selected, light.row_selected);

    let green = palette_for(ThemeMode::Dark, AccentColor::Green);
    assert_ne!(dark.accent.end, green.accent.end);
    assert_ne!(dark.row_selected, green.row_selected);
    assert_ne!(dark.badge_glyph_active, green.badge_glyph_active);
}

/// A toggle's off track has to travel to the accent, so both ends of that
/// interpolation must be real gradients rather than one flat color.
fn the_toggle_track_has_somewhere_to_travel() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let p = palette_for(mode, AccentColor::Purple);
        assert_ne!(p.toggle_off.start, p.accent.start, "{mode:?}");
        assert_ne!(p.toggle_off.start, p.toggle_off.end, "{mode:?} off track is flat");
        assert_ne!(p.accent.start, p.accent.end, "{mode:?} accent is flat");
    }
}

/// The surfaces repaint off [`theme::epoch`]; if it does not move on a change
/// they never notice one.
fn a_palette_change_moves_the_epoch() {
    set_theme_immediately(Theme::for_mode(ThemeMode::Dark, AccentColor::Purple));
    let before = theme::epoch();
    set_theme_immediately(Theme::for_mode(ThemeMode::Light, AccentColor::Orange));
    assert_ne!(before, theme::epoch(), "a palette change went unannounced");
}

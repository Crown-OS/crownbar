//! The bar's palette, which is the desktop's palette.
//!
//! No module in this crate names a color. Every one of them comes from
//! [`crownuikit::config::theme`] — the same value every other CrownOS app
//! paints from, resolved from `~/.config/crownos/appearance.ron` — so the bar
//! follows light/dark mode and the user's accent without being told.
//!
//! # Why this is a bridge and not a re-export
//!
//! crownuikit paints through xilem's masonry, which is on vello 0.6; the bar
//! paints through crownshell, which is on vello 0.9. The two `peniko::Color`
//! types are structurally identical and semantically the same sRGB value, but
//! they are different Rust types and neither crate can name the other's. So
//! the palette crosses the boundary here, once per frame, through [`srgb`]:
//! the components come over as a plain `[f32; 4]`, which belongs to no crate.
//! Nothing downstream has to know there was a boundary at all.
//!
//! # Reading it
//!
//! Call [`palette`] once at the top of a paint and thread the result down.
//! Never cache it in a struct: a mode or accent change *cross-fades*, so the
//! answer is different on every frame for the length of the transition, and
//! [`is_animating`] is what keeps those frames coming.

use std::sync::OnceLock;

use crownos_config::{schema::Appearance, Subscription};
use crownuikit::config as kit;
use vello::{
    kurbo::Point,
    peniko::{Color, Gradient},
};

/// Opacity of a popup panel before the user's transparency setting is taken
/// off it. The kit's popovers are opaque because they sit on a window; the
/// bar's sit on the compositor's blur, and letting some of that through is
/// most of what makes them look like part of the desktop.
const PANEL_OPACITY: f32 = 0.66;
/// Never let the transparency setting take a panel below this — past it the
/// text stops being readable over a bright wallpaper.
const MIN_PANEL_OPACITY: f32 = 0.55;

/// The bar's own foreground, idle then under the pointer, per mode. Unlike
/// every other slot these are named here rather than taken from the kit: a
/// panel's text sits on the panel's own body, but the bar's sits over the
/// wallpaper, and the contrast that reads well there is the desktop's choice
/// rather than a derivative of the window palette.
const BAR_FG_DARK: Color = Color::from_rgb8(0xDD, 0xDD, 0xDD);
const BAR_FG_HOVER_DARK: Color = Color::from_rgb8(0xAA, 0xAA, 0xAA);
const BAR_FG_LIGHT: Color = Color::from_rgb8(0x11, 0x11, 0x11);
const BAR_FG_HOVER_LIGHT: Color = Color::from_rgb8(0x33, 0x33, 0x33);

// -- geometry-free layout tokens ---------------------------------------------
// Spacing is the bar's own business; only color comes from the kit.

/// Horizontal inset of the bar contents from the left/right edges.
pub const BAR_PAD_X: f32 = 12.0;
/// Inner padding of a widget pill (left/right).
pub const PILL_PAD_X: f32 = 12.0;
/// Vertical inset of the pill within the bar.
pub const PILL_PAD_Y: f32 = 0.0;
/// Gap between adjacent widgets.
pub const WIDGET_GAP: f32 = 12.0;
/// Font size for widget text.
pub const FONT_SIZE: f32 = 14.0;
/// Font weight for widget text.
pub const FONT_WEIGHT: f32 = 600.0;

/// Two-stop accent gradient, in the bar's color type.
///
/// The kit's [`GradientStops`] with the geometry helpers the bar actually
/// uses: a toggle track projects them top→bottom, a slider's filled track
/// left→right, so every accent surface catches the light the same way.
///
/// [`GradientStops`]: crownuikit::config::GradientStops
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stops {
    pub start: Color,
    pub end: Color,
}

impl Stops {
    /// Top→bottom down the column at `x`. The toggle track's orientation.
    pub fn vertical(self, x: f64, y0: f64, y1: f64) -> Gradient {
        Gradient::new_linear(Point::new(x, y0), Point::new(x, y1))
            .with_stops([(0.0_f32, self.start), (1.0_f32, self.end)])
    }

    /// Left→right along the row at `y`. The slider's filled-track orientation.
    pub fn horizontal(self, y: f64, x0: f64, x1: f64) -> Gradient {
        Gradient::new_linear(Point::new(x0, y), Point::new(x1, y))
            .with_stops([(0.0_f32, self.start), (1.0_f32, self.end)])
    }

    fn from_kit(stops: kit::GradientStops) -> Self {
        Self {
            start: srgb(stops.start.components),
            end: srgb(stops.end.components),
        }
    }
}

/// Every color the bar paints, resolved for this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    /// The bar's background at its densest, along the screen edge. The bar
    /// asks for no backdrop blur, so this is the whole of what sits behind the
    /// widgets; it takes the same body color the panels do, and the same
    /// transparency setting, so the bar and the panels it opens are one
    /// surface in two places. How it thins out is [`crate::ui::backdrop`]'s.
    pub bar_fill: Color,
    /// Pill fill under the pointer, and while the widget's panel is open.
    pub pill_hover: Color,
    pub pill_active: Color,

    /// Icons and text on the bar, idle and under the pointer.
    pub bar_fg: Color,
    pub bar_fg_hover: Color,

    /// A panel's primary text.
    pub fg: Color,
    /// Secondary text — an idle pill, a device row's label.
    pub fg_muted: Color,
    /// Third level — section headings, trailing detail, chevrons.
    pub fg_dim: Color,

    /// Popup panel body, rim and drop shadow.
    pub panel_bg: Color,
    /// Hairline around a panel. Derived rather than taken from a slot: the
    /// kit's `surface.border` is meant to enclose a surface sitting *on* a
    /// window, so in dark mode it is darker than what it encloses. A panel
    /// floating over the wallpaper needs the opposite, and deriving it from
    /// the body and the foreground gets that right in both modes without
    /// naming a color.
    pub panel_rim: Color,
    pub panel_shadow: Color,

    /// The user's accent, as the kit's two-stop gradient.
    pub accent: Stops,
    /// A toggle's off-state track, also a gradient so it can travel to
    /// [`accent`](Self::accent) as the switch turns on.
    pub toggle_off: Stops,

    /// Panel row backgrounds.
    pub row_hover: Color,
    /// The current output / the joined network. Accent-tinted, so a selected
    /// row follows the accent the way every other active surface does.
    pub row_selected: Color,
    /// Rule between panel sections.
    pub separator: Color,

    /// Circular badge behind a device glyph, and the glyph on a live one.
    pub badge_idle: Color,
    pub badge_active: Color,
    pub badge_glyph_active: Color,

    /// A slider's unfilled track, and the knob shared by slider and toggle.
    pub track: Color,
    pub knob: Color,
    pub knob_shadow: Color,

    /// Degraded but working — the "unsecured network" marker, and a battery
    /// held back by a power-saving profile.
    pub warning: Color,
    /// Healthy: a charging battery.
    pub success: Color,
    /// Failing: a battery about to go flat.
    pub danger: Color,
}

/// Snapshot the palette as of right now.
///
/// Cheap enough to call from paint code, and it must be called *from* paint
/// code: mid-cross-fade every slot is different on every frame.
pub fn palette() -> Palette {
    let t = kit::theme();
    let body = srgb(t.popover.bg.components);
    let opacity = panel_opacity();
    let panel_bg = with_alpha(body, opacity);
    let shadow = srgb(t.surface.shadow.components);
    let (bar_fg, bar_fg_hover) = if t.mode.is_dark() {
        (BAR_FG_DARK, BAR_FG_HOVER_DARK)
    } else {
        (BAR_FG_LIGHT, BAR_FG_HOVER_LIGHT)
    };
    Palette {
        bar_fill: panel_bg,
        pill_hover: srgb(t.surface.hover.components),
        pill_active: scale_alpha(srgb(t.surface.hover.components), 2.2),

        bar_fg,
        bar_fg_hover,

        fg: srgb(t.popover.text.components),
        fg_muted: srgb(t.text.body.components),
        fg_dim: srgb(t.popover.muted_text.components),

        panel_bg,
        panel_rim: with_alpha(lerp(panel_bg, srgb(t.text.primary.components), 0.16), 0.55),
        panel_shadow: shadow,

        accent: Stops::from_kit(t.accent),
        toggle_off: Stops::from_kit(t.toggle_off),

        row_hover: srgb(t.popover.hover_bg.components),
        // `menu.selected_bg` is the accent at full strength, which is right for
        // a menu row that also flips its text to `on_accent`. A panel row keeps
        // its own text, so it takes the same color as a wash instead.
        row_selected: with_alpha(srgb(t.menu.selected_bg.components), 0.30),
        separator: srgb(t.menu.separator.components),

        // The hover wash is tuned to be felt rather than seen; a badge is a
        // container and has to read as one.
        badge_idle: scale_alpha(srgb(t.surface.hover.components), 2.6),
        badge_active: srgb(t.control.knob.components),
        badge_glyph_active: srgb(t.accent.end.components),

        track: srgb(t.control.track.components),
        knob: srgb(t.control.knob.components),
        knob_shadow: srgb(t.control.knob_shadow.components),

        warning: srgb(t.status.warning.components),
        success: srgb(t.status.success.components),
        danger: srgb(t.status.danger.components),
    }
}

/// Whether a palette change is cross-fading, and the surfaces therefore owe
/// the compositor another frame.
pub fn is_animating() -> bool {
    kit::theme_is_animating()
}

/// A number that changes whenever the sampled palette might have. A surface
/// compares the one it last painted against this to know it is stale.
pub fn epoch() -> u64 {
    kit::theme_epoch()
}

/// Install the palette `appearance` describes, cross-fading into it.
pub fn apply(appearance: &Appearance) {
    kit::apply_appearance(appearance);
    set_transparency(appearance.transparency);
}

/// Read `appearance.ron` and install its palette with no fade, so the first
/// frame is already right rather than fading in from the kit's default.
///
/// Returns what it read, because the bar's own settings live in the same
/// section and there is no reason to parse the file twice.
pub fn load() -> Appearance {
    let appearance: Appearance = crownos_config::load(Appearance::SECTION);
    kit::set_theme_immediately(kit::Theme::from_appearance(&appearance));
    set_transparency(appearance.transparency);
    appearance
}

/// Follow `appearance.ron` for the rest of the process.
///
/// The subscription is parked in a `static` rather than returned because
/// dropping one unregisters it, and there is nowhere in a crownshell app to
/// hold a value for the life of the event loop. Calling this twice is a no-op.
///
/// The callback lands on the config watcher's thread; everything it touches is
/// a global behind a lock, and the surfaces notice on their next tick by way
/// of [`epoch`].
pub fn follow_config() {
    static SUBSCRIPTION: OnceLock<Subscription> = OnceLock::new();
    let _ = SUBSCRIPTION.get_or_init(|| {
        crownos_config::subscribe_typed::<Appearance, _>(Appearance::SECTION, |appearance| {
            apply(&appearance);
        })
    });
}

// -- transparency -------------------------------------------------------------

/// `Appearance::transparency`, mirrored here because the kit deliberately does
/// not carry it: for an app window it is the compositor's business, but the
/// bar's panels are the surface it is describing.
static TRANSPARENCY: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn set_transparency(value: f64) {
    let clamped = (value.clamp(0.0, 1.0) as f32).to_bits();
    TRANSPARENCY.store(clamped, std::sync::atomic::Ordering::Relaxed);
}

fn panel_opacity() -> f32 {
    let transparency = f32::from_bits(TRANSPARENCY.load(std::sync::atomic::Ordering::Relaxed));
    (PANEL_OPACITY - transparency).max(MIN_PANEL_OPACITY)
}

// -- the version bridge --------------------------------------------------------

/// One sRGB color, rebuilt from its components.
///
/// The kit's colors and the bar's are both `AlphaColor<Srgb>` over the same
/// four `f32`s and differ only in which version of peniko declared them. Going
/// through the components means this signature names neither one — see the
/// module docs.
fn srgb(components: [f32; 4]) -> Color {
    Color::new(components)
}

fn with_alpha(color: Color, alpha: f32) -> Color {
    let [r, g, b, _] = color.components;
    Color::new([r, g, b, alpha.clamp(0.0, 1.0)])
}

/// `color` with its alpha multiplied — for a translucent palette slot that has
/// to be shown at more or less than the strength the palette chose.
pub fn scale_alpha(color: Color, factor: f32) -> Color {
    let [r, g, b, a] = color.components;
    Color::new([r, g, b, (a * factor).clamp(0.0, 1.0)])
}

/// Straight linear interpolation between two colors, mixed in premultiplied
/// component space so a fade whose endpoints differ in alpha stays monotonic
/// instead of flashing through a dark midpoint.
///
/// The same mix as [`crownuikit::util::lerp_color`], which cannot be called
/// from here for the reason in the module docs.
pub fn lerp(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let [ar, ag, ab, aa] = a.components;
    let [br, bg, bb, ba] = b.components;
    let alpha = aa + (ba - aa) * t;
    if alpha <= f32::EPSILON {
        return Color::new([0.0, 0.0, 0.0, 0.0]);
    }
    let mix = |ca: f32, cb: f32| {
        let (pa, pb) = (ca * aa, cb * ba);
        (pa + (pb - pa) * t) / alpha
    };
    Color::new([mix(ar, br), mix(ag, bg), mix(ab, bb), alpha])
}

/// Interpolate a gradient stop-for-stop, so a surface travelling between two
/// of them is never anything other than a two-stop gradient mid-flight.
pub fn lerp_stops(a: Stops, b: Stops, t: f32) -> Stops {
    Stops {
        start: lerp(a.start, b.start, t),
        end: lerp(a.end, b.end, t),
    }
}

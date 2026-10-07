//! Where the panel is drawn: the area below the bar.
//!
//! The panel is one rounded body whose bounds are sprung (see
//! [`super::motion`]). Everything that has to agree with it follows the same
//! frame: the body laid out at the shape as it lands on screen, the rows
//! stretched onto it from its corner, and the material the compositor renders
//! *under* it — the blur, its vibrancy and the refractive rim, which a
//! translucent surface cannot do for itself because what sits behind it
//! belongs to other clients. A compositor that will not blur gets an opaque
//! body instead: a translucent panel over an unblurred wallpaper is not readable.

use config::BarPosition;
use crownui::{
    kit::INTER_FAMILY,
    prelude::{
        Listeners, Material, MaterialBlur, Runtime, Styled, TRANSPARENT, View, bind, component,
        for_each, keyed, show, vstack, zstack,
    },
};

use super::{PANEL_WIDTH, close, host_offset, rows};
use crate::{
    animation::SpringProfile,
    theme,
    ui::{pill::nothing, state::Bar},
};

/// Concentric with the row highlights.
const RADIUS: f32 = crownui::kit::PANEL_RADIUS;
/// Backdrop blur asked of the compositor, in logical px.
const BLUR_RADIUS: f32 = 32.0;
/// Chroma multiplier on the blurred backdrop: above 1 is the vibrancy that
/// makes a wallpaper's color show through frosted glass rather than grey out.
const BLUR_VIBRANCY: f32 = 1.25;
/// Width of the compositor's refractive rim just inside the panel's edge.
const RIM: f32 = 1.0;
const ROWS_PAD_Y: f32 = 8.0;

/// The area beside the bar, starting where the bar's reserved space ends so a
/// panel thrown out of its pill disappears under the bar's edge rather than
/// over it.
pub fn panel_host(bar: Bar) -> impl View {
    let present = bar.popup.present;
    let open = move || {
        zstack((backdrop(bar), body(bar)))
            .absolute()
            .inset(0.0)
            .pointer_events_none()
    };
    let host = zstack(show(present, open, nothing).absolute().inset(0.0))
        .absolute()
        .left(0.0)
        .right(0.0)
        .overflow_hidden()
        .pointer_events_none();
    match bar.config.position {
        BarPosition::Top => host.top(host_offset(bar)).bottom(0.0),
        BarPosition::Bottom => host.top(0.0).bottom(host_offset(bar)),
    }
}

/// While a panel is up the whole area takes the pointer, so a click outside
/// the panel is a dismissal rather than a click through to what is behind.
fn backdrop(bar: Bar) -> impl View {
    vstack(())
        .absolute()
        .inset(0.0)
        .pointer_events_auto()
        .on_click(move |cx, _| close(cx, bar))
}

fn body(bar: Bar) -> impl View {
    let frame = bar.popup.frame;
    component(move |cx| {
        let capabilities = cx.platform_capabilities();
        let blurred = cx.memo(move |runtime| capabilities.get(runtime).materials);
        let visible = move |runtime: &mut Runtime| frame.with(runtime, |frame| frame.visible());
        zstack(rows_layer(bar))
            .absolute()
            .left(bind(move |runtime| visible(runtime).x0 as f32))
            .top(bind(move |runtime| visible(runtime).y0 as f32))
            .w(bind(move |runtime| visible(runtime).width() as f32))
            .h(bind(move |runtime| visible(runtime).height() as f32))
            .rounded(RADIUS)
            .overflow_hidden()
            .bg(bind(move |runtime| {
                let panel = bar.palette.get(runtime).panel_bg;
                if blurred.get(runtime) {
                    panel
                } else {
                    theme::opaque(panel)
                }
            }))
            .ring(RIM, bind(move |runtime| bar.palette.get(runtime).panel_rim))
            .opacity(bind(move |runtime| {
                frame.with(runtime, |frame| frame.alpha)
            }))
            .material(bind(move |runtime| {
                let alpha = frame.with(runtime, |frame| frame.alpha);
                if blurred.get(runtime) {
                    material(alpha)
                } else {
                    Material::default()
                }
            }))
            .pointer_events_auto()
    })
}

/// The shown panel's rows, stretched from the body's corner onto the shape as
/// it lands. A change of panel mounts the next rows while the last ones fade
/// out where they were.
fn rows_layer(bar: Bar) -> impl View {
    let frame = bar.popup.frame;
    keyed(
        move |runtime| bar.popup.shown.get(runtime),
        move |cx, shown| {
            let rows = cx.signal(Vec::new());
            let declared = cx.signal((0.0, false));
            bar.popup.measured.set(cx, 0.0);
            if let Some(index) = shown {
                cx.effect(move |runtime| {
                    bar.popup.content.get(runtime);
                    let spec = bar
                        .state
                        .update(runtime, |state| state.widgets.popup(index, &state.services))
                        .unwrap_or_default();
                    declared.set(runtime, (rows::height(&spec), rows::sizes_itself(&spec)));
                    rows.set(runtime, rows::keyed(spec));
                });
                cx.effect(move |runtime| {
                    let (height, sizes_itself) = declared.get(runtime);
                    let measured = bar.popup.measured.get(runtime);
                    // Nothing to spring toward until the self-sized rows have
                    // been laid out once.
                    let height = match (sizes_itself, measured > 0.0) {
                        (false, _) => height,
                        (true, true) => height + measured,
                        (true, false) => 0.0,
                    };
                    bar.popup.height.set(runtime, height);
                });
            }
            vstack(for_each(
                rows,
                |row| row.key,
                move |cx, row| rows::view(cx, bar, row),
            ))
            .absolute()
            .top(0.0)
            .left(0.0)
            .w(PANEL_WIDTH)
            .py(ROWS_PAD_Y)
            .font_family(INTER_FAMILY)
            .origin(0.0, 0.0)
            .scale_x(bind(move |runtime| {
                frame.with(runtime, |frame| frame.scale().0 as f32)
            }))
            .scale_y(bind(move |runtime| {
                frame.with(runtime, |frame| frame.scale().1 as f32)
            }))
            .enter(|style| style.opacity(0.0))
            .exit(|style| style.opacity(0.0))
            .transition(SpringProfile::SNAPPY.curve())
        },
    )
    .absolute()
    .inset(0.0)
}

/// What the compositor renders under the panel. The blur comes up with the
/// panel — a radius that tracks the fade keeps the wallpaper from going
/// frosted before there is anything on it — and squaring it lets the blur,
/// and the rim with it, reach zero while the rows are still just visible.
fn material(alpha: f32) -> Material {
    let strength = alpha.clamp(0.0, 1.0);
    Material {
        corner_radius: RADIUS,
        border_width: RIM,
        blur: Some(MaterialBlur {
            radius: BLUR_RADIUS * strength * strength,
            tint: TRANSPARENT,
            saturation: 1.0 + (BLUR_VIBRANCY - 1.0) * strength,
        }),
        shadow: None,
    }
}

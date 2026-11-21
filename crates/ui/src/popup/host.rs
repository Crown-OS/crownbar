//! Where the panel is drawn: the whole surface, above the bar's ground and
//! below its pills, so a panel comes out from behind the pill it belongs to.
//!
//! The panel is one rounded body whose bounds, corners and tilt are sprung
//! (see [`super::motion`]). Everything that has to agree with it follows the
//! same frame: the body laid out at the shape, turned in perspective on the
//! GPU, the rows stretched onto it from its corner, and the material the
//! compositor renders *under* it — the blur, its vibrancy and the refractive
//! rim, which a translucent surface cannot do for itself because what sits
//! behind it belongs to other clients. The body is opaque until the glass
//! comes in, and stays opaque on a compositor that will not blur: a
//! translucent panel over an unblurred wallpaper is not readable.

use crownui::{
    ext::{Modifier, StyleProp},
    kit::INTER_FAMILY,
    prelude::{
        Listeners, Material, MaterialBlur, Runtime, Styled, TRANSPARENT, Vector, View, bind,
        component, for_each, keyed, show, vstack, zstack,
    },
};

use super::{PANEL_WIDTH, close, motion::Frame, rows, sheen::sheen};
use crate::{
    pill::nothing,
    state::Bar,
    theme,
    util::{signal::set_if_changed, theme::provide_derived_theme},
};
use crownbar_widgets::animation::SpringProfile;

/// Backdrop blur asked of the compositor, in logical px: deep enough that
/// what is behind the panel is only its colors.
const BLUR_RADIUS: f32 = 56.0;
/// Chroma multiplier on the blurred backdrop: above 1 is the vibrancy that
/// makes a wallpaper's color glow through the smoked glass rather than grey out.
const BLUR_VIBRANCY: f32 = 1.8;
/// Width of the refractive rim just inside the panel's edge, and of the
/// specular line drawn over it.
const RIM: f32 = 1.5;
/// How far rows slide when the panel moves to another pill, in logical px.
const SLIDE: f32 = PANEL_WIDTH / 3.0;

pub fn panel_host(bar: Bar) -> impl View {
    let present = bar.popup.present;
    let open = move || {
        zstack((backdrop(bar), body(bar)))
            .absolute()
            .inset(0.0)
            .pointer_events_none()
    };
    zstack(show(present, open, nothing).absolute().inset(0.0))
        .absolute()
        .inset(0.0)
        .pointer_events_none()
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
        let read = move |runtime: &mut Runtime, field: fn(&Frame) -> f64| {
            frame.with(runtime, |frame| field(frame) as f32)
        };
        // Whole px: the compositor cuts the blur from the body's shape in
        // whole px, and a body between them would leave a sliver of either
        // along its far edges while it moves.
        let shape = move |runtime: &mut Runtime| frame.with(runtime, |frame| frame.bounds.round());
        provide_derived_theme(cx, move |runtime| bar.palette.get(runtime).panel_theme);
        zstack((sheen(bar, RIM), rows_layer(bar)))
            .absolute()
            .left(bind(move |runtime| shape(runtime).x0 as f32))
            .top(bind(move |runtime| shape(runtime).y0 as f32))
            .w(bind(move |runtime| shape(runtime).width() as f32))
            .h(bind(move |runtime| shape(runtime).height() as f32))
            .rounded(bind(move |runtime| read(runtime, |frame| frame.radius)))
            .overflow_hidden()
            .modifier(Modifier::Bound(Box::new(move |runtime| {
                StyleProp::Origin(Vector::new(read(runtime, |frame| frame.vanishing_x), 0.5))
            })))
            .rotate_x(bind(move |runtime| read(runtime, |frame| frame.pitch)))
            .perspective(bind(move |runtime| {
                read(runtime, |frame| frame.perspective)
            }))
            .bg(bind(move |runtime| {
                let body = bar.palette.get(runtime).glass.body;
                let solid = theme::opaque(body);
                if blurred.get(runtime) {
                    solid.lerp(body, frame.with(runtime, |frame| frame.glass))
                } else {
                    solid
                }
            }))
            .opacity(bind(move |runtime| {
                frame.with(runtime, |frame| frame.alpha)
            }))
            .material(bind(move |runtime| {
                if blurred.get(runtime) {
                    material(frame.with(runtime, |frame| frame.glass))
                } else {
                    Material::default()
                }
            }))
            .pointer_events_auto()
    })
}

/// The shown panel's rows, stretched from the body's corner onto the shape as
/// it lands. The stretch and the fade move one drawn image of the rows, so the
/// text is not drawn again at each new scale and the rows need one layer. A change of panel slides the next rows in from the side the
/// panel is travelling toward while the last ones slide out the other way.
fn rows_layer(bar: Bar) -> impl View {
    let popup = bar.popup;
    let frame = popup.frame;
    keyed(
        move |runtime| popup.shown.get(runtime),
        move |cx, shown| {
            let arrival = Vector::new(popup.slide.with_untracked(cx, |slide| *slide) * SLIDE, 0.0);
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
                    let shape = (rows::height(&spec), rows::sizes_itself(&spec));
                    set_if_changed(runtime, declared, shape);
                    set_if_changed(runtime, rows, rows::keyed(spec));
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
            vstack(
                for_each(rows, |row| row.key, move |cx, row| rows::view(cx, bar, row))
                    .flex_col()
                    .gap(rows::ROW_GAP),
            )
            .absolute()
            .top(0.0)
            .left(0.0)
            .w(PANEL_WIDTH)
            .py(rows::PAD_Y)
            .font_family(INTER_FAMILY)
            .translate(bind(move |runtime| {
                let owner = popup.owner.get(runtime);
                let leaving = owner.is_some() && owner != shown;
                let offset = if leaving {
                    -popup.slide.get(runtime) * SLIDE
                } else {
                    0.0
                };
                Vector::new(offset, 0.0)
            }))
            .enter(move |style| style.opacity(0.0).translate(arrival))
            .exit(|style| style.opacity(0.0))
            .transition(SpringProfile::SNAPPY.curve())
        },
    )
    .absolute()
    .inset(0.0)
    .origin(0.0, 0.0)
    .will_change_transform()
    .scale_x(bind(move |runtime| {
        frame.with(runtime, |frame| frame.scale().0 as f32)
    }))
    .scale_y(bind(move |runtime| {
        frame.with(runtime, |frame| frame.scale().1 as f32)
    }))
    .opacity(bind(move |runtime| frame.with(runtime, |frame| frame.rows)))
}

/// What the compositor renders under the panel. The blur comes up with the
/// glass — a radius that tracks it keeps the wallpaper from going frosted
/// before the panel is flat on it — and squaring it lets the blur, and the
/// rim with it, reach zero while the rows are still just visible.
///
/// The blur takes its corners from the body's own shape. A corner radius here
/// would round the whole layer surface — the bar included — so there is none.
fn material(glass: f32) -> Material {
    let strength = glass.clamp(0.0, 1.0);
    Material {
        corner_radius: 0.0,
        border_width: RIM,
        blur: Some(MaterialBlur {
            radius: BLUR_RADIUS * strength * strength,
            tint: TRANSPARENT,
            saturation: 1.0 + (BLUR_VIBRANCY - 1.0) * strength,
        }),
        shadow: None,
    }
}

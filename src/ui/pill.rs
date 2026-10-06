//! One widget on the bar: a fully rounded capsule around its glyph and label.
//!
//! The capsule's fill fades in under the pointer and stays on while the
//! widget's panel is up; the label brightens with it. Both are transitions the
//! render thread runs, so hovering costs the UI thread one signal write.

use crownplugin_proto::Part;
use crownui::{
    ext::{Kind, Listener, NodeId},
    kit::INTER_FAMILY,
    prelude::{
        AnyView, Cx, Element, Memo, Role, Semantic, Styled, View, bind, canvas, keyed, show, text,
        vstack, zstack,
    },
};
use kurbo::Point;

use crate::{
    animation::SpringProfile,
    theme::{self, FONT_SIZE, FONT_WEIGHT, PILL_PAD_X},
    ui::{
        frames::animate,
        icons::{self, ReadoutPlan},
        popup, remote,
        scene::Scene,
        state::Bar,
    },
    widgets::{Icon, bit},
};

const ICON_LABEL_GAP: f32 = 6.0;
const LABEL_LINE_HEIGHT: f32 = 1.2;

pub struct PillKind {
    bar: Bar,
    index: usize,
}

/// The pill of widget `index`, named `id` for screen readers and agents.
pub fn pill(bar: Bar, index: usize, id: String) -> Element<PillKind> {
    Element::new(PillKind { bar, index })
        .role(Role::Button)
        .label(id)
        .flex_row()
        .items_center()
        .h_full()
        .shrink(0.0)
        .px(PILL_PAD_X)
        .rounded(bar.height / 2.0)
        .pointer_events_auto()
        .transition(SpringProfile::SNAPPY.curve())
}

/// What a glyph needs drawn: nothing, an ordinary glyph, or the battery,
/// which prints its reading over its own cell.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Glyph {
    None,
    Plain,
    Battery,
}

impl Kind for PillKind {
    const NAME: &'static str = "pill";

    fn mount(self, cx: &mut Cx, node: NodeId) {
        let Self { bar, index } = self;
        let hovered = cx.signal(false);
        let active = cx.memo(move |runtime| bar.popup.owner.get(runtime) == Some(index));
        let lit = cx.memo(move |runtime| active.get(runtime) || hovered.get(runtime));
        let icon = cx.memo(move |runtime| bar.read(runtime, index, |widget| widget.icon()));

        for listener in [
            Listener::PointerEnter(Box::new(move |cx, _| hovered.set(cx, true))),
            Listener::PointerLeave(Box::new(move |cx, _| hovered.set(cx, false))),
            Listener::Click(Box::new(move |cx, event| {
                event.stop_propagation();
                press(cx, bar, index, node);
            })),
        ] {
            cx.add_listener(node, listener);
        }
        cx.bind_style(node, move |runtime| {
            let palette = bar.palette.get(runtime);
            let fill = if active.get(runtime) {
                palette.pill_active
            } else if hovered.get(runtime) {
                palette.pill_hover
            } else {
                theme::with_alpha(palette.pill_hover, 0.0)
            };
            crownui::ext::StyleProp::Background(Some(fill))
        });

        let label =
            cx.memo(move |runtime| bar.read(runtime, index, |widget| widget.label().to_owned()));
        cx.set_children(
            node,
            (
                glyph(bar, icon),
                label_view(bar, icon, label, lit),
                plugin(bar, index, lit),
            ),
        );
    }
}

/// A widget with a panel opens it; one without gets the click.
fn press(cx: &mut Cx, bar: Bar, index: usize, node: NodeId) {
    let anchor_x = cx
        .tree()
        .get(node)
        .map(|pill| {
            let rect = pill.geometry().window_rect;
            rect.min_x() + rect.size.width * 0.5
        })
        .unwrap_or_default();
    let open_here = bar
        .popup
        .owner
        .with_untracked(cx, |owner| *owner == Some(index));
    let has_panel = open_here
        || bar
            .state
            .update(cx, |state| state.widgets.has_popup(index, &state.services));
    if has_panel {
        popup::toggle(cx, bar, index, anchor_x);
        return;
    }
    if bar
        .state
        .update(cx, |state| state.widgets.click(index, &state.services))
    {
        bar.touch(cx, bit(index));
        animate(cx, bar, bit(index));
    }
}

fn glyph(bar: Bar, icon: Memo<Icon>) -> impl View {
    keyed(
        move |runtime| match icon.get(runtime) {
            Icon::None => Glyph::None,
            Icon::Battery(_) => Glyph::Battery,
            _ => Glyph::Plain,
        },
        move |_, kind| match kind {
            Glyph::None => nothing(),
            Glyph::Plain => canvas(move |runtime, size, list| {
                let icon = icon.get(runtime);
                let palette = bar.palette.get(runtime);
                let (x, y) = (size.width * 0.5, size.height * 0.5);
                icons::draw(&mut Scene::new(list), icon, x, y, palette.bar_fg, &palette);
            })
            .w(icons::ICON_BOX)
            .h_full()
            .boxed(),
            Glyph::Battery => battery(bar, icon),
        },
    )
    .h_full()
}

/// A plugin's pill: its tree, rebuilt only when a new revision of it lands.
fn plugin(bar: Bar, index: usize, lit: Memo<bool>) -> impl View {
    keyed(
        move |runtime| {
            bar.read(runtime, index, |widget| {
                widget
                    .plugin_surface()
                    .and_then(|surface| surface.version(Part::Main))
            })
        },
        move |cx, version| {
            let surface = version.and_then(|_| {
                bar.state.with(cx, |state| {
                    state
                        .widgets
                        .widget(index)
                        .and_then(|widget| widget.plugin_surface().cloned())
                })
            });
            surface.map_or_else(nothing, |surface| remote::pill(cx, bar, &surface, lit))
        },
    )
    .h_full()
}

/// The battery cell, with its reading laid over it in two inks: each copy is
/// clipped to one side of the charge line, so a reading that straddles it
/// stays legible over the fill and over the empty track alike.
fn battery(bar: Bar, icon: Memo<Icon>) -> AnyView {
    let state = move |icon: Icon| match icon {
        Icon::Battery(state) => state,
        _ => Default::default(),
    };
    let plan = move |runtime: &mut crownui::prelude::Runtime| {
        let battery = state(icon.get(runtime));
        let palette = bar.palette.get(runtime);
        icons::battery_readout(battery, palette.bar_fg, &palette)
    };
    let cell = canvas(move |runtime, size, list| {
        let battery = state(icon.get(runtime));
        let palette = bar.palette.get(runtime);
        let origin = Point::new(0.0, f64::from(size.height) * 0.5);
        icons::draw_battery(
            &mut Scene::new(list),
            origin,
            battery,
            palette.bar_fg,
            &palette,
        );
    })
    .absolute()
    .inset(0.0);
    let digits = move |ink: usize| {
        text(bind(move |runtime| {
            state(icon.get(runtime)).readout.to_string()
        }))
        .font_family(INTER_FAMILY)
        .text_size(bind(move |runtime| plan(runtime).font_size))
        .font_weight(icons::READOUT_WEIGHT)
        .leading(1.0)
        .text_color(bind(move |runtime| plan(runtime).inks[ink]))
    };
    let side = move |ink: usize| {
        let reading = crownui::prelude::hstack(digits(ink))
            .absolute()
            .top(0.0)
            .h_full()
            .items_center()
            .justify_center()
            .left(bind(move |runtime| half(plan(runtime), ink).1))
            .w(bind(move |runtime| plan(runtime).body.width() as f32));
        vstack(reading)
            .absolute()
            .top(0.0)
            .h_full()
            .overflow_hidden()
            .left(bind(move |runtime| half(plan(runtime), ink).0))
            .w(bind(move |runtime| half(plan(runtime), ink).2))
    };
    zstack((cell, side(0), side(1)))
        .relative()
        .h_full()
        .w(bind(move |runtime| icons::advance(icon.get(runtime))))
        .boxed()
}

/// One side of the charge line: where its clip starts, where the reading sits
/// inside that clip so it stays centred on the cell, and how wide the clip is.
fn half(plan: ReadoutPlan, ink: usize) -> (f32, f32, f32) {
    let (start, end) = if ink == 0 {
        (plan.body.x0, plan.edge)
    } else {
        (plan.edge, plan.body.x1)
    };
    (
        start as f32,
        (plan.body.x0 - start) as f32,
        (end - start).max(0.0) as f32,
    )
}

fn label_view(bar: Bar, icon: Memo<Icon>, label: Memo<String>, lit: Memo<bool>) -> impl View {
    show(
        bind(move |runtime| label.with(runtime, |label| !label.is_empty())),
        move || {
            text(label)
                .font_family(INTER_FAMILY)
                .text_size(FONT_SIZE)
                .font_weight(FONT_WEIGHT)
                .leading(LABEL_LINE_HEIGHT)
                .ml(bind(move |runtime| {
                    if matches!(icon.get(runtime), Icon::None) {
                        0.0
                    } else {
                        ICON_LABEL_GAP
                    }
                }))
                .text_color(bind(move |runtime| {
                    let palette = bar.palette.get(runtime);
                    if lit.get(runtime) {
                        palette.bar_fg_hover
                    } else {
                        palette.bar_fg
                    }
                }))
        },
        nothing,
    )
}

/// A branch that takes no space.
pub fn nothing() -> AnyView {
    vstack(()).display_none().boxed()
}

//! One widget on the bar: a fully rounded capsule around its glyph and label.
//!
//! The capsule's fill fades in under the pointer and stays on while the
//! widget's panel is up; the label shifts under the pointer alone. Both are
//! transitions the render thread runs, so hovering costs the UI thread one
//! signal write.

use crownplugin_proto::Part;
use crownui::{
    ext::{Kind, Listener, NodeId},
    kit::INTER_FAMILY,
    prelude::{
        AnyView, Cx, Element, Memo, Role, Semantic, Styled, View, bind, canvas, keyed, show, text,
        vstack,
    },
};
use kurbo::Point;

use crate::{
    animation::SpringProfile,
    theme::{FONT_SIZE, FONT_WEIGHT, PILL_PAD_X, PillState},
    ui::{frames::animate, icons, popup, remote, scene::Scene, state::Bar},
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
        let lit = cx.memo(move |runtime| hovered.get(runtime));
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
        let accent = cx.memo(move |runtime| bar.read(runtime, index, |widget| widget.accent()));
        cx.bind_style(node, move |runtime| {
            let state = if active.get(runtime) {
                PillState::Active
            } else if hovered.get(runtime) {
                PillState::Hovered
            } else {
                PillState::Rest
            };
            let fill = bar
                .palette
                .get(runtime)
                .pill_fill(state, accent.get(runtime));
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

/// The battery cell, its reading drawn into it, as wide as its accessory has
/// grown.
fn battery(bar: Bar, icon: Memo<Icon>) -> AnyView {
    canvas(move |runtime, size, list| {
        let Icon::Battery(battery) = icon.get(runtime) else {
            return;
        };
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
    .h_full()
    .w(bind(move |runtime| icons::advance(icon.get(runtime))))
    .boxed()
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

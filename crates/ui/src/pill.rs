//! One widget on the bar: a fully rounded capsule around its glyph and label.
//!
//! The capsule's fill fades in under the pointer and stays on while the
//! widget's panel is up; the label shifts under the pointer alone. A live
//! activity's capsule is a chip with a wash of its own, strengthened the same
//! way. All of it is transitions the render thread runs, so hovering costs the
//! UI thread one signal write.

mod chip;

use crownplugin_proto::Part;
use crownui::{
    ext::{Kind, Listener, NodeId, StyleProp},
    kit::INTER_FAMILY,
    prelude::{
        AnyView, Cx, Element, Memo, Role, Semantic, Styled, Vector, View, bind, canvas, keyed,
        show, text, vstack, zstack,
    },
};
use kurbo::Point;

use crate::{
    frames::animate,
    icons, popup, remote,
    scene::Scene,
    state::Bar,
    theme::{FONT_SIZE, FONT_WEIGHT, Ink, PILL_PAD_X, PillState, SHADOW_BLUR, SHADOW_DROP},
};
use crownbar_widgets::{Icon, animation::SpringProfile, bit};

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
        let rect = cx.observe_rect(node);
        let accent = cx.memo(move |runtime| bar.read(runtime, index, |widget| widget.accent()));
        let ink = cx.memo(move |runtime| {
            let rect = rect.get(runtime);
            let span = bar.span_of(runtime, rect);
            bar.palette.get(runtime).pill_ink(span, accent.get(runtime))
        });
        let state = cx.memo(move |runtime| {
            if active.get(runtime) {
                PillState::Active
            } else if hovered.get(runtime) {
                PillState::Hovered
            } else {
                PillState::Rest
            }
        });

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
            let fill = bar
                .palette
                .get(runtime)
                .pill_fill(state.get(runtime), accent.get(runtime));
            StyleProp::Background(Some(fill))
        });
        cx.bind_style(node, move |runtime| {
            let chips = bar.palette.get(runtime).chips;
            StyleProp::Shadow(accent.get(runtime).map(|accent| chips.look(accent).shadow))
        });

        let label =
            cx.memo(move |runtime| bar.read(runtime, index, |widget| widget.label().to_owned()));
        cx.set_children(
            node,
            (
                chip::wash(bar, accent, state),
                glyph(bar, icon, ink),
                label_view(icon, label, lit, ink),
                plugin(bar, index, lit, ink),
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

fn glyph(bar: Bar, icon: Memo<Icon>, ink: Memo<Ink>) -> impl View {
    keyed(
        move |runtime| match icon.get(runtime) {
            Icon::None => Glyph::None,
            Icon::Battery(_) => Glyph::Battery,
            _ => Glyph::Plain,
        },
        move |_, kind| match kind {
            Glyph::None => nothing(),
            Glyph::Plain => canvas(move |runtime, size, list| {
                let (icon, ink) = (icon.get(runtime), ink.get(runtime));
                let palette = bar.palette.get(runtime);
                let (x, y) = (size.width * 0.5, size.height * 0.5);
                Scene::shadowed(list, ink.shadow, |scene| {
                    icons::draw(scene, icon, x, y, ink.fg, &palette);
                });
            })
            .w(icons::ICON_BOX)
            .h_full()
            .boxed(),
            Glyph::Battery => battery(bar, icon, ink),
        },
    )
    .h_full()
}

/// A plugin's pill: its tree, rebuilt only when a new revision of it lands.
fn plugin(bar: Bar, index: usize, lit: Memo<bool>, ink: Memo<Ink>) -> impl View {
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
            surface.map_or_else(nothing, |surface| remote::pill(cx, bar, &surface, lit, ink))
        },
    )
    .h_full()
}

/// The battery cell, its reading drawn into it, as wide as its accessory has
/// grown.
fn battery(bar: Bar, icon: Memo<Icon>, ink: Memo<Ink>) -> AnyView {
    canvas(move |runtime, size, list| {
        let Icon::Battery(battery) = icon.get(runtime) else {
            return;
        };
        let (palette, ink) = (bar.palette.get(runtime), ink.get(runtime));
        let origin = Point::new(0.0, f64::from(size.height) * 0.5);
        Scene::shadowed(list, ink.shadow, |scene| {
            icons::draw_battery(scene, origin, battery, ink.fg, &palette);
        });
    })
    .h_full()
    .w(bind(move |runtime| icons::advance(icon.get(runtime))))
    .boxed()
}

fn label_view(icon: Memo<Icon>, label: Memo<String>, lit: Memo<bool>, ink: Memo<Ink>) -> impl View {
    show(
        bind(move |runtime| label.with(runtime, |label| !label.is_empty())),
        move || {
            let shadow = move |runtime: &mut _| ink.get(runtime).shadow;
            zstack((
                label_text(label)
                    .text_color(bind(shadow))
                    .text_blur(SHADOW_BLUR)
                    .translate(Vector::new(0.0, SHADOW_DROP))
                    .hidden(bind(move |runtime| shadow(runtime).a <= 0.0)),
                label_text(label).text_color(bind(move |runtime| {
                    let ink = ink.get(runtime);
                    if lit.get(runtime) {
                        ink.fg_hover
                    } else {
                        ink.fg
                    }
                })),
            ))
            .ml(bind(move |runtime| {
                if matches!(icon.get(runtime), Icon::None) {
                    0.0
                } else {
                    ICON_LABEL_GAP
                }
            }))
        },
        nothing,
    )
}

fn label_text(label: Memo<String>) -> Element<impl Kind> {
    text(label)
        .font_family(INTER_FAMILY)
        .text_size(FONT_SIZE)
        .font_weight(FONT_WEIGHT)
        .leading(LABEL_LINE_HEIGHT)
}

/// A branch that takes no space.
pub fn nothing() -> AnyView {
    vstack(()).display_none().boxed()
}

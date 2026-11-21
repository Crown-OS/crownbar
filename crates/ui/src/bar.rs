//! The bar itself: its ground, and the pills in their three slots. They are
//! two layers so that a panel can come out between them: over the ground,
//! from behind its pill.

use crownbar_config::BarPosition;
use crownui::prelude::{
    Cx, Listeners, Runtime, Styled, View, bind, canvas, hstack, keyed, show, zstack,
};

use crate::{
    backdrop::{self, Ground},
    pill::{nothing, pill},
    popup,
    state::Bar,
    theme::{BAR_PAD_X, WIDGET_GAP},
};
use crownbar_widgets::{Icon, WidgetSlot};

/// The bar's ground. A click on bare bar puts away whatever panel is up, the
/// same way a click anywhere outside it does.
pub fn bar_ground(bar: Bar) -> impl View {
    let config = bar.config;
    along_bar(
        bar,
        canvas(move |runtime, size, list| {
            let ground = Ground::of(&config.get(runtime));
            backdrop::paint(list, size, bar.palette.get(runtime).bar_fill, ground)
        })
        .on_click(move |cx, _| popup::close(cx, bar))
        .pointer_events_auto(),
    )
}

/// The pills, rebuilt whenever the widgets are rearranged. A rebuild leaves
/// the services and the palette alone; only the pills are made again.
pub fn bar_pills(bar: Bar) -> impl View {
    keyed(
        move |runtime| bar.layout.get(runtime),
        move |cx, _| pills(cx, bar),
    )
    .absolute()
    .inset(0.0)
    .pointer_events_none()
}

/// `view` laid along the screen edge the bar sits on, at the bar's size.
fn along_bar<V: Styled>(bar: Bar, view: V) -> V {
    let config = bar.config;
    let margin = move |runtime: &mut Runtime| config.get(runtime).margin as f32;
    let placed = view
        .absolute()
        .left(bind(margin))
        .right(bind(margin))
        .h(bind(move |runtime| config.get(runtime).bar_height as f32));
    match bar.position {
        BarPosition::Top => placed.top(bind(margin)),
        BarPosition::Bottom => placed.bottom(bind(margin)),
    }
}

fn pills(cx: &mut Cx, bar: Bar) -> impl View + use<> {
    let widgets = bar.state.with(cx, |state| {
        state
            .widgets
            .placed()
            .map(|(slot, widget)| (slot, widget.id().to_owned()))
            .collect::<Vec<_>>()
    });
    let in_slot = |slot: WidgetSlot| {
        widgets
            .iter()
            .enumerate()
            .filter(|(_, (candidate, _))| *candidate == slot)
            .map(|(index, (_, id))| (index, id.clone()))
            .collect::<Vec<_>>()
    };
    along_bar(
        bar,
        zstack((
            slot_row(bar, WidgetSlot::Left, in_slot(WidgetSlot::Left)),
            slot_row(bar, WidgetSlot::Center, in_slot(WidgetSlot::Center)),
            slot_row(bar, WidgetSlot::Right, in_slot(WidgetSlot::Right)),
        ))
        .pointer_events_none(),
    )
}

/// A widget whose hardware is absent takes no space and cannot be hit; it
/// starts drawing by itself if the hardware appears.
fn slot_row(bar: Bar, slot: WidgetSlot, widgets: Vec<(usize, String)>) -> impl View {
    let pills = widgets
        .into_iter()
        .map(|(index, id)| {
            show(
                bind(move |runtime| {
                    bar.read(runtime, index, |widget| {
                        widget.visible()
                            && (!widget.label().is_empty()
                                || !matches!(widget.icon(), Icon::None)
                                || widget.plugin_surface().is_some())
                    })
                }),
                move || pill(bar, index, id.clone()),
                nothing,
            )
            .h_full()
            .shrink(0.0)
            .boxed()
        })
        .collect::<Vec<_>>();
    let row = hstack(pills)
        .absolute()
        .inset(0.0)
        .items_center()
        .gap(WIDGET_GAP)
        .pointer_events_none();
    match slot {
        WidgetSlot::Left => row.justify_start().px(BAR_PAD_X),
        WidgetSlot::Center => row.justify_center(),
        WidgetSlot::Right => row.justify_end().px(BAR_PAD_X),
    }
}

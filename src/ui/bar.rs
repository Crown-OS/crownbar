//! The bar itself: its ground, and the pills in their three slots.

use config::BarPosition;
use crownui::prelude::{Cx, Listeners, Styled, View, bind, canvas, hstack, keyed, show, zstack};

use crate::{
    theme::{BAR_PAD_X, WIDGET_GAP},
    ui::{
        backdrop::{self, Ground},
        pill::{nothing, pill},
        popup,
        state::Bar,
    },
    widgets::{Icon, WidgetSlot},
};

/// The strip, rebuilt whenever the widgets are rearranged. A rebuild leaves the
/// services and the palette alone; only the pills are made again.
pub fn bar_strip(bar: Bar) -> impl View {
    keyed(
        move |runtime| bar.layout.get(runtime),
        move |cx, _| strip(cx, bar),
    )
    .absolute()
    .inset(0.0)
    .pointer_events_none()
}

fn strip(cx: &mut Cx, bar: Bar) -> impl View + use<> {
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
    let config = bar.config;
    let ground = match (config.radius, config.position) {
        (Some(radius), _) => Ground::Floating { radius },
        (None, BarPosition::Top) => Ground::Top,
        (None, BarPosition::Bottom) => Ground::Bottom,
    };
    let margin = config.margin as f32;
    let strip = zstack((
        // A click on bare bar puts away whatever panel is up, the same way a
        // click anywhere outside it does.
        canvas(move |runtime, size, list| {
            backdrop::paint(list, size, bar.palette.get(runtime).bar_fill, ground)
        })
        .absolute()
        .inset(0.0)
        .on_click(move |cx, _| popup::close(cx, bar)),
        slot_row(bar, WidgetSlot::Left, in_slot(WidgetSlot::Left)),
        slot_row(bar, WidgetSlot::Center, in_slot(WidgetSlot::Center)),
        slot_row(bar, WidgetSlot::Right, in_slot(WidgetSlot::Right)),
    ))
    .absolute()
    .left(margin)
    .right(margin)
    .h(bar.height)
    .pointer_events_auto();
    match config.position {
        BarPosition::Top => strip.top(margin),
        BarPosition::Bottom => strip.bottom(margin),
    }
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

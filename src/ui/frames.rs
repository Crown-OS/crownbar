//! The bar's frame clock.
//!
//! Every spring on the bar — a pill's glyph morphing, the panel thrown out of
//! its pill — is stepped here, once per display frame, and only while one of
//! them is moving. At rest the bar requests no frames at all.

use std::time::Duration;

use crownui::prelude::Cx;

use crate::{
    ui::{popup, state::Bar},
    widgets::WidgetMask,
};

/// The frame a resting clock pretends the last one was, so the first step of
/// an animation is a whole frame rather than nothing.
const FIRST_FRAME: Duration = Duration::from_micros(16_667);
/// Largest step taken at once: a stall must not fling a spring past its target.
const MAX_FRAME: Duration = Duration::from_micros(33_333);

/// Starts the springs of the widgets in `widgets` and keeps frames coming
/// until they and the panel come to rest.
pub fn animate(cx: &mut Cx, bar: Bar, widgets: WidgetMask) {
    let schedule = bar.clock.update(cx, |clock| {
        clock.active |= widgets;
        !std::mem::replace(&mut clock.scheduled, true)
    });
    if schedule {
        cx.request_animation_frame(move |cx, now| frame(cx, bar, now));
    }
}

fn frame(cx: &mut Cx, bar: Bar, now: Duration) {
    let (active, dt) = bar.clock.update(cx, |clock| {
        clock.scheduled = false;
        let dt = clock
            .last
            .map_or(FIRST_FRAME, |last| now.saturating_sub(last))
            .min(MAX_FRAME);
        clock.last = Some(now);
        (std::mem::take(&mut clock.active), dt.as_secs_f32())
    });
    let moving = bar
        .state
        .update(cx, |state| state.widgets.step_animations(active, dt));
    bar.touch(cx, active);
    let panel_moving = popup::step(cx, bar, dt);
    if moving != 0 || panel_moving {
        animate(cx, bar, moving);
    } else {
        bar.clock.update(cx, |clock| clock.last = None);
    }
}

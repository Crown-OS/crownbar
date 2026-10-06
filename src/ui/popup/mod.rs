//! The panel a pill opens: when it opens, what it shows, and how it moves.
//!
//! There is one panel for the life of the bar. A click on a pill throws it out
//! of that pill; a click on another pill morphs it into the other panel; a
//! click anywhere else, or an action that is done with it, puts it away. The
//! rows are the widget's own [`PopupSpec`](crate::widgets::PopupSpec), rebuilt
//! whenever what they show went stale.

mod host;
pub mod motion;
mod rows;

pub use host::panel_host;

use config::BarPosition;
use crownui::prelude::Cx;
use kurbo::Rect;

use crate::{
    ui::{PANEL_GAP, frames::animate, state::Bar},
    widgets::{AfterAction, PopupAction, bit},
};
use motion::{Opening, PanelMotion};

/// Every panel is this wide, whatever rows it holds.
pub const PANEL_WIDTH: f32 = crownui::kit::PANEL_WIDTH;
/// Smallest distance the panel keeps from the screen's left/right edges.
const SCREEN_MARGIN: f64 = 8.0;

/// Open `idx`'s panel under a pill centred on `anchor_x`, or put it away if it
/// is the one already up.
pub fn toggle(cx: &mut Cx, bar: Bar, idx: usize, anchor_x: f32) {
    if bar
        .popup
        .owner
        .with_untracked(cx, |owner| *owner == Some(idx))
    {
        close(cx, bar);
    } else {
        open(cx, bar, idx, anchor_x);
    }
}

fn open(cx: &mut Cx, bar: Bar, idx: usize, anchor_x: f32) {
    let popup = bar.popup;
    let previous = popup.owner.with_untracked(cx, |owner| *owner);
    if let Some(previous) = previous {
        closed(cx, bar, previous);
    }
    bar.state.update(cx, |state| {
        if let Some(widget) = state.widgets.widget_mut(idx) {
            widget.popup_opened(&state.services);
        }
    });
    popup.owner.set(cx, Some(idx));
    popup.anchor_x.set(cx, anchor_x);
    let shown = popup.shown.with_untracked(cx, |shown| *shown);
    let present = popup.present.with_untracked(cx, |present| *present);
    // One panel handing over to another, rather than one being thrown out of
    // its pill: the shape travels and the rows cross-fade.
    let replacing = previous.is_some() && present;
    if shown != Some(idx) || previous.is_none() {
        popup.motion.update(cx, |motion| motion.open(replacing));
    }
    if shown != Some(idx) {
        popup.shown.set(cx, Some(idx));
    }
    popup.present.set(cx, true);
    animate(cx, bar, 0);
}

/// Put the panel away, if one is up.
pub fn close(cx: &mut Cx, bar: Bar) {
    let Some(owner) = bar.popup.owner.with_untracked(cx, |owner| *owner) else {
        return;
    };
    closed(cx, bar, owner);
    bar.popup.owner.set(cx, None);
    bar.popup.motion.update(cx, |motion| motion.close());
    animate(cx, bar, 0);
}

/// Take the panel down at once, skipping its exit: the widgets it belonged
/// to are being replaced.
pub fn dismiss(cx: &mut Cx, bar: Bar) {
    if let Some(owner) = bar.popup.owner.with_untracked(cx, |owner| *owner) {
        closed(cx, bar, owner);
    }
    let popup = bar.popup;
    popup.owner.set(cx, None);
    popup.shown.set(cx, None);
    popup.present.set(cx, false);
    popup
        .motion
        .update(cx, |motion| *motion = PanelMotion::new());
}

/// A widget that started a poll loop when its panel opened stops it here.
fn closed(cx: &mut Cx, bar: Bar, idx: usize) {
    bar.state.update(cx, |state| {
        if let Some(widget) = state.widgets.widget_mut(idx) {
            widget.popup_closed(&state.services);
        }
    });
}

/// Hand an action to the widget that owns the panel.
pub fn dispatch(cx: &mut Cx, bar: Bar, action: PopupAction) {
    let Some(idx) = bar.popup.owner.with_untracked(cx, |owner| *owner) else {
        return;
    };
    let after = bar.state.update(cx, |state| {
        state
            .widgets
            .widget_mut(idx)
            .map_or(AfterAction::Close, |widget| {
                widget.on_popup(action, &state.services)
            })
    });
    bar.touch(cx, bit(idx));
    animate(cx, bar, bit(idx));
    // A slider mid-drag has already moved under the pointer; restating the
    // rows at pointer rate would rebuild the panel for no visible gain.
    let mid_drag = matches!(action, PopupAction::Slide { commit: false, .. });
    if after == AfterAction::Close {
        close(cx, bar);
    } else if !mid_drag {
        bar.restate_panel(cx);
    }
}

/// The once-a-second tick, which is when the shown panel kicks off a fresh
/// reading.
pub fn poll(cx: &mut Cx, bar: Bar) {
    let Some(idx) = bar.popup.shown.with_untracked(cx, |shown| *shown) else {
        return;
    };
    let (changed, busy) = bar.state.update(cx, |state| {
        state
            .widgets
            .widget_mut(idx)
            .map_or((false, false), |widget| {
                (widget.popup_poll(true), widget.popup_busy())
            })
    });
    if changed {
        bar.restate_panel(cx);
    }
    if busy {
        animate(cx, bar, 0);
    }
}

/// One frame of the panel. Returns whether it still owes another.
pub fn step(cx: &mut Cx, bar: Bar, dt: f32) -> bool {
    let popup = bar.popup;
    if !popup.present.with_untracked(cx, |present| *present) {
        return false;
    }
    let height = popup.height.with_untracked(cx, |height| f64::from(*height));
    if height <= 0.0 {
        return true;
    }
    let anchor_x = popup
        .anchor_x
        .with_untracked(cx, |anchor| f64::from(*anchor));
    let surface = bar.surface.with_untracked(cx, |size| *size);
    let (top, opening) = match bar.config.position {
        BarPosition::Top => (f64::from(PANEL_GAP), Opening::Down),
        BarPosition::Bottom => {
            let host = f64::from(surface.height) - f64::from(host_offset(bar));
            (host - f64::from(PANEL_GAP) - height, Opening::Up)
        }
    };
    let target = target_bounds(anchor_x, top, height, f64::from(surface.width));
    let (moving, frame, gone) = popup.motion.update(cx, |motion| {
        motion.reshape(target);
        let moving = motion.step(dt);
        (moving, motion.frame(anchor_x, opening), motion.invisible())
    });
    popup.frame.set(cx, frame);

    let owner = popup.owner.with_untracked(cx, |owner| *owner);
    if owner.is_none() && gone {
        popup.shown.set(cx, None);
        popup.present.set(cx, false);
        return false;
    }
    let polling = owner.is_some_and(|idx| {
        let (changed, busy) = bar.state.update(cx, |state| {
            state
                .widgets
                .widget_mut(idx)
                .map_or((false, false), |widget| {
                    (widget.popup_poll(false), widget.popup_busy())
                })
        });
        if changed {
            bar.restate_panel(cx);
        }
        busy
    });
    moving || polling
}

/// How far the area panels open into is set back from the bar's screen edge:
/// to the line the bar reserves, which stops [`PANEL_GAP`] short of its far
/// edge.
pub fn host_offset(bar: Bar) -> f32 {
    bar.config.reach() as f32 - PANEL_GAP
}

/// Where the panel wants to be: centred on its pill with its top edge at
/// `top`, then kept on screen. What the shape springs toward rather than where
/// it is drawn.
fn target_bounds(anchor_x: f64, top: f64, height: f64, surface_width: f64) -> Rect {
    let width = f64::from(PANEL_WIDTH);
    let x = (anchor_x - width * 0.5)
        .clamp(
            SCREEN_MARGIN,
            (surface_width - width - SCREEN_MARGIN).max(SCREEN_MARGIN),
        )
        .round();
    Rect::new(x, top, x + width, top + height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panel_hangs_centred_under_its_pill() {
        assert_eq!(
            target_bounds(1000.0, 40.0, 200.0, 2560.0),
            Rect::new(852.0, 40.0, 1148.0, 240.0)
        );
    }

    #[test]
    fn a_panel_near_an_edge_stays_on_screen() {
        assert_eq!(target_bounds(2550.0, 40.0, 100.0, 2560.0).x1, 2552.0);
        assert_eq!(target_bounds(10.0, 40.0, 100.0, 2560.0).x0, SCREEN_MARGIN);
    }
}

use std::time::Instant;

use crate::{
    services::live::{Activity, Control},
    widgets::{
        AfterAction, Icon, PopupAction, PopupSpec,
        popup::{Item, PanelBuilder, Row},
    },
};

/// What a row of a chip's panel does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    Control(Control),
    Seek,
}

/// The panel behind a chip, built from the activity alone: every source gets
/// the same panel without drawing one.
pub fn panel(activity: &Activity, now: Instant) -> (PopupSpec, Vec<Option<Target>>) {
    let mut panel = PanelBuilder::new();
    panel.row(Row::Header {
        title: activity.title.to_string(),
        toggle: None,
    });

    if !activity.caption.is_empty() {
        let mut primary = String::new();
        activity.caption.write_into(&mut primary, now);
        let secondary = match activity.headline == activity.title {
            true => String::new(),
            false => activity.headline.to_string(),
        };
        panel.row(Row::Readout { primary, secondary });
    }

    if let (true, Some(progress)) = (activity.seekable, activity.progress) {
        panel.action(
            Row::Slider {
                icon: Icon::Live {
                    glyph: activity.glyph,
                    accent: activity.accent,
                    progress: None,
                    pulse: 0.0,
                },
                value: progress.at(now),
            },
            Target::Seek,
        );
    }

    for detail in &activity.details {
        panel.row(
            Item::new(detail.label.to_string())
                .plain()
                .detail(detail.value.to_string())
                .enabled(false)
                .row(),
        );
    }

    if !activity.controls.is_empty() {
        panel.row(Row::Separator);
    }
    for control in &activity.controls {
        panel.action(
            Row::Action {
                label: control.label().into(),
            },
            Target::Control(*control),
        );
    }
    panel.finish()
}

pub fn row(action: PopupAction) -> usize {
    match action {
        PopupAction::Toggle { row, .. }
        | PopupAction::Slide { row, .. }
        | PopupAction::Activate { row } => row,
    }
}

/// Whether the panel goes once `control` is done: an activity that ends
/// takes its panel with it.
pub fn after(control: Control) -> AfterAction {
    match control {
        Control::Cancel | Control::Dismiss | Control::Reset | Control::Stop => AfterAction::Close,
        _ => AfterAction::Stay,
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use super::*;
    use crate::services::live::{ActivityKey, Caption, Glyph, Origin};

    #[test]
    fn every_control_gets_a_row_that_maps_back_to_it() {
        let now = Instant::now();
        let key = ActivityKey {
            origin: Origin::Stopwatch,
            id: Arc::from("stopwatch"),
        };
        let activity = Activity::new(key, Glyph::Stopwatch, "Stopwatch")
            .caption(Caption::Frozen(Duration::from_secs(72)))
            .detail("Lap 1", "0:30")
            .controls([Control::Resume, Control::Lap, Control::Reset]);
        let (spec, targets) = panel(&activity, now);
        assert!(matches!(
            &spec.rows[1],
            Row::Readout { primary, .. } if primary == "1:12"
        ));
        let controls: Vec<_> = targets
            .iter()
            .flatten()
            .map(|target| match target {
                Target::Control(control) => Some(*control),
                Target::Seek => None,
            })
            .collect();
        assert_eq!(
            controls,
            [
                Some(Control::Resume),
                Some(Control::Lap),
                Some(Control::Reset)
            ]
        );
    }
}

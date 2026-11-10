use std::time::Instant;

use crate::{
    AfterAction, Icon, PopupAction, PopupSpec, phone,
    popup::{Header, Item, PanelBuilder, Row},
};
use crownbar_integrations::{
    crownlink::{DeviceId, Feature, PhoneDevice, Stream},
    live::{Activity, Control, Glyph, Origin},
};

/// What a row of a chip's panel does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    Control(Control),
    Seek,
    /// Start or stop a linked device's camera as this computer's webcam.
    Webcam(DeviceId),
}

/// The panel behind a chip, built from the activity alone: every source gets
/// the same panel without drawing one. A camera's panel also offers every
/// linked device in `phones` that can stand in as the webcam.
pub fn panel(
    activity: &Activity,
    phones: &[PhoneDevice],
    now: Instant,
) -> (PopupSpec, Vec<Option<Target>>) {
    let mut panel = PanelBuilder::new();
    panel.row(Header::new(activity.title.as_ref()).row());

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
                muted: None,
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

    if is_camera(activity) {
        camera_sources(&mut panel, phones);
    }

    if !activity.controls.is_empty() {
        panel.row(Row::Separator);
    }
    for control in &activity.controls {
        panel.action(action_row(*control), Target::Control(*control));
    }
    panel.finish()
}

/// The camera in use, or a phone's camera standing in for it.
fn is_camera(activity: &Activity) -> bool {
    matches!(
        (activity.key.origin, activity.glyph),
        (Origin::Capture, Glyph::Camera) | (Origin::Phone, Glyph::Webcam)
    )
}

/// "Camera Source": each linked device whose camera may be used, chosen while
/// it streams here.
fn camera_sources(panel: &mut PanelBuilder<Target>, phones: &[PhoneDevice]) {
    let mut cameras = phone::allowing(phones, Feature::Camera).peekable();
    if cameras.peek().is_none() {
        return;
    }
    phone::section(panel, "Camera Source");
    for device in cameras {
        panel.action(
            phone::item(device, Feature::Camera)
                .detail(WEBCAM_LENS)
                .selected(device.is_streaming(Stream::Camera))
                .row(),
            Target::Webcam(device.id),
        );
    }
}

/// The lens crownlink starts a phone's webcam on.
const WEBCAM_LENS: &str = "Front Camera";

fn action_row(control: Control) -> Row {
    match control {
        Control::HangUp => Row::destructive(control.label()),
        _ => Row::action(control.label()),
    }
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
        Control::Cancel | Control::Dismiss | Control::Reset | Control::Stop | Control::HangUp => {
            AfterAction::Close
        }
        _ => AfterAction::Stay,
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use super::*;
    use crate::{phone::fixture::*, popup::Tone};
    use crownbar_integrations::live::{ActivityKey, Caption};

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
        let (spec, targets) = panel(&activity, &[], now);
        assert!(matches!(
            &spec.rows[1],
            Row::Readout { primary, .. } if primary == "1:12"
        ));
        let controls: Vec<_> = targets
            .iter()
            .flatten()
            .map(|target| match target {
                Target::Control(control) => Some(*control),
                Target::Seek | Target::Webcam(_) => None,
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

    fn activity(origin: Origin, id: &str, glyph: Glyph, controls: &[Control]) -> Activity {
        let key = ActivityKey {
            origin,
            id: Arc::from(id),
        };
        Activity::new(key, glyph, "Camera").controls(controls.iter().copied())
    }

    fn camera() -> Activity {
        activity(Origin::Capture, "camera", Glyph::Camera, &[])
    }

    #[test]
    fn a_camera_offers_every_phone_that_can_stand_in_as_the_webcam() {
        for devices in rosters() {
            let (spec, targets) = panel(&camera(), &devices, Instant::now());
            let rows = section_items(&spec, "Camera Source");
            assert_eq!(
                labels(&rows),
                devices.iter().map(|d| &*d.name).collect::<Vec<_>>()
            );
            assert_eq!(
                sections(&spec).contains(&"Camera Source"),
                !devices.is_empty()
            );
            for ((row, item), device) in rows.iter().zip(&devices) {
                assert_eq!(item.detail.as_deref(), Some(WEBCAM_LENS));
                assert_eq!(targets[*row], Some(Target::Webcam(device.id)));
            }
        }
    }

    #[test]
    fn the_phone_streaming_as_the_webcam_is_the_chosen_source() {
        let devices = [
            phone(ONEPLUS, "OnePlus 11 5G"),
            running(phone(PIXEL, "Pixel 8"), Feature::Camera),
        ];
        let chip = activity(
            Origin::Phone,
            "0202/camera",
            Glyph::Webcam,
            &[Control::Stop],
        );
        let (spec, _) = panel(&chip, &devices, Instant::now());
        let chosen: Vec<_> = section_items(&spec, "Camera Source")
            .iter()
            .map(|(_, item)| item.selected)
            .collect();
        assert_eq!(chosen, [false, true]);
        assert!(matches!(
            spec.rows.last(),
            Some(Row::Action { label, tone: Tone::Plain }) if label == "Stop"
        ));
    }

    #[test]
    fn only_a_camera_lists_camera_sources() {
        let [_, _, devices] = rosters();
        let mic = activity(Origin::Capture, "microphone", Glyph::Microphone, &[]);
        let (spec, _) = panel(&mic, &devices, Instant::now());
        assert!(sections(&spec).is_empty());
    }

    #[test]
    fn hanging_up_reads_as_destructive() {
        let call = activity(Origin::Phone, "0202/call", Glyph::Call, &[Control::HangUp]);
        let (spec, targets) = panel(&call, &[], Instant::now());
        assert!(matches!(
            spec.rows.last(),
            Some(Row::Action { label, tone: Tone::Danger }) if label == "Hang Up"
        ));
        assert_eq!(
            targets.last(),
            Some(&Some(Target::Control(Control::HangUp)))
        );
        assert_eq!(after(Control::HangUp), AfterAction::Close);
    }
}

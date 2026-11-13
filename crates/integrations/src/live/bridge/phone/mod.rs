mod live;

use std::{convert::Infallible, sync::Arc};

use crate::{
    crownlink::{DeviceId, PhoneCommand, PhoneDevice, PhoneState, Stream},
    live::{
        Accent, Activity, Caption, Control, Feed, Glyph, Origin, Urgency, activity::HEADLINE_CHARS,
        bridge::Bridge,
    },
    util::text::ellipsize,
};

/// Between a chip's subject and the device it is on.
const DEVICE_SEPARATOR: &str = " · ";
/// The least of a subject a long device name may leave.
const MIN_SUBJECT_CHARS: usize = 8;

/// One chip per answered call, per stream and per live activity of every
/// linked device, and one for a device ringing this computer. Each is keyed by
/// its device, so two phones streaming the same thing are two chips.
#[derive(Default)]
pub struct PhoneBridge {
    /// What each chip on show stands for, for routing its controls back.
    shown: Vec<(Arc<str>, DeviceId, Route)>,
}

/// Where a chip's controls go.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Route {
    Chip(Chip),
    /// One of the device's live activities, with its buttons' ids in the
    /// order of its [`Control::Action`]s.
    Live {
        activity: Arc<str>,
        buttons: Vec<Arc<str>>,
    },
    /// The device ringing this computer.
    Ringing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Chip {
    Call,
    Stream(Stream),
}

impl Chip {
    const fn slug(self) -> &'static str {
        match self {
            Self::Call => "call",
            Self::Stream(Stream::Camera) => "camera",
            Self::Stream(Stream::Mic) => "mic",
            Self::Stream(Stream::Mirror) => "mirror",
            Self::Stream(Stream::Monitor) => "display",
        }
    }

    const fn title(self) -> &'static str {
        match self {
            Self::Call => "Phone Call",
            Self::Stream(Stream::Camera) => "Phone Camera",
            Self::Stream(Stream::Mic) => "Phone Mic",
            Self::Stream(Stream::Mirror) => "Mirroring",
            Self::Stream(Stream::Monitor) => "Extended Display",
        }
    }

    const fn glyph(self) -> Glyph {
        match self {
            Self::Call => Glyph::Call,
            Self::Stream(Stream::Camera) => Glyph::Webcam,
            Self::Stream(Stream::Mic) => Glyph::Microphone,
            Self::Stream(Stream::Mirror) => Glyph::Mirror,
            Self::Stream(Stream::Monitor) => Glyph::ExternalDisplay,
        }
    }

    const fn accent(self) -> Accent {
        match self {
            Self::Call => Accent::Green,
            Self::Stream(Stream::Camera | Stream::Mic) => Accent::Red,
            Self::Stream(Stream::Mirror) => Accent::Blue,
            Self::Stream(Stream::Monitor) => Accent::Violet,
        }
    }

    /// A call, and a phone hearing or seeing the room, are worth a seat over
    /// a screen merely shown elsewhere.
    const fn urgency(self) -> Urgency {
        match self {
            Self::Call | Self::Stream(Stream::Camera | Stream::Mic) => Urgency::Attention,
            Self::Stream(Stream::Mirror | Stream::Monitor) => Urgency::Active,
        }
    }

    const fn control(self) -> Control {
        match self {
            Self::Call => Control::HangUp,
            Self::Stream(_) => Control::Stop,
        }
    }
}

impl Bridge for PhoneBridge {
    type State = PhoneState;
    type Command = PhoneCommand;
    const ORIGIN: Origin = Origin::Phone;

    fn activities(&mut self, state: &PhoneState, feed: &Feed<Infallible>) -> Vec<Activity> {
        let several = state.devices.len() > 1;
        self.shown.clear();
        let mut activities = Vec::new();
        if let Some(by) = state.ringing_here {
            let id = Arc::from(format!("{by}/ringing"));
            activities.push(ringing(feed, &id, state.device(by)));
            self.shown.push((id, by, Route::Ringing));
        }
        for device in &state.devices {
            let chips = device
                .call
                .iter()
                .map(|_| Chip::Call)
                .chain(device.streams().map(Chip::Stream));
            for chip in chips {
                let id = Arc::from(format!("{}/{}", device.id, chip.slug()));
                activities.push(activity(feed, &id, device, chip, several));
                self.shown.push((id, device.id, Route::Chip(chip)));
            }
            for ongoing in &device.activities {
                let id = Arc::from(format!("{}/activity/{}", device.id, ongoing.id));
                activities.push(live::activity(feed, &id, device, ongoing, several));
                let buttons = ongoing.actions.iter().map(|button| button.id.clone());
                let route = Route::Live {
                    activity: ongoing.id.clone(),
                    buttons: buttons.collect(),
                };
                self.shown.push((id, device.id, route));
            }
        }
        activities
    }

    fn command(&self, id: &str, control: Control) -> Option<PhoneCommand> {
        let (_, device, route) = self.shown.iter().find(|(shown, ..)| **shown == *id)?;
        match (route, control) {
            (Route::Chip(Chip::Call), Control::HangUp) => Some(PhoneCommand::HangUp(*device)),
            (Route::Chip(Chip::Stream(stream)), Control::Stop) => {
                Some(PhoneCommand::Stop(*device, *stream))
            }
            (Route::Live { activity, buttons }, Control::Action(index)) => {
                let button = buttons.get(usize::from(index))?;
                Some(PhoneCommand::LiveAction(
                    *device,
                    activity.clone(),
                    button.clone(),
                ))
            }
            (Route::Ringing, Control::Stop) => Some(PhoneCommand::StopLocalRing),
            _ => None,
        }
    }
}

/// "Ringing from Pixel 8", red until stopped: a device is finding this
/// computer. The device may have gone since it started the ring.
fn ringing(feed: &Feed<Infallible>, id: &Arc<str>, device: Option<&PhoneDevice>) -> Activity {
    let name = device.map_or("a linked device", |device| &device.name);
    Activity::new(feed.key(id.clone()), Glyph::Phone, "Find My")
        .headline(format!("Ringing from {name}"))
        .accent(Accent::Red)
        .urgency(Urgency::Attention)
        .controls([Control::Stop])
}

/// With more than one device linked, every chip names its device.
fn activity(
    feed: &Feed<Infallible>,
    id: &Arc<str>,
    device: &PhoneDevice,
    chip: Chip,
    several: bool,
) -> Activity {
    let call = device.call.as_ref().filter(|_| chip == Chip::Call);
    let subject = call.map_or(chip.title(), |call| call.caller());
    let headline = match several {
        true => on_device(subject, &device.name),
        false => subject.to_owned(),
    };
    let activity = Activity::new(feed.key(id.clone()), chip.glyph(), chip.title())
        .headline(headline)
        .accent(chip.accent())
        .urgency(chip.urgency())
        .detail("Device", device.name.clone())
        .controls([chip.control()]);
    let Some(call) = call else {
        return activity;
    };
    let activity = activity.caption(Caption::Since(call.answered));
    let activity = match call.contact {
        Some(_) => activity.detail("Number", call.number.clone()),
        None => activity,
    };
    match call.held {
        true => activity.detail("Status", "On Hold"),
        false => activity,
    }
}

/// `subject` named with its device. The subject is the part cut short, so
/// the device that tells two chips apart survives the headline's limit.
fn on_device(subject: &str, device: &str) -> String {
    let room = HEADLINE_CHARS
        .saturating_sub(device.chars().count() + DEVICE_SEPARATOR.chars().count())
        .max(MIN_SUBJECT_CHARS);
    format!("{}{DEVICE_SEPARATOR}{device}", ellipsize(subject, room))
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, time::Instant};

    use crownlink_linux::ipc::proto::{
        Battery, CallInfo, CallStateChanged, CallStatus, Chronometer, DeviceClass, DeviceInfo,
        Event, Feature, LinkKind, LiveAction, LiveActivitiesChanged, LiveActivity, LiveProgress,
        LocalRingChanged,
    };

    use super::*;
    use crate::live::Progress;

    const PIXEL: DeviceId = DeviceId([1; 32]);
    const ONEPLUS: DeviceId = DeviceId([2; 32]);

    fn listed(id: DeviceId, name: &str, active: &[Feature]) -> DeviceInfo {
        DeviceInfo {
            id,
            name: name.to_owned(),
            class: DeviceClass::Phone,
            trusted: true,
            connected: true,
            link: LinkKind::Lan,
            battery: Some(Battery {
                percent: 50,
                charging: true,
            }),
            features: Feature::ALL.into_iter().collect(),
            active: active.iter().copied().collect(),
        }
    }

    fn answered(state: &mut PhoneState, id: DeviceId, contact: Option<&str>) {
        let call = CallInfo {
            call_id: "/call1".to_owned(),
            number: "+447700900123".to_owned(),
            contact_name: contact.map(str::to_owned),
            status: CallStatus::Active,
            answered_unix_ms: None,
        };
        let _ = state.apply(
            Event::CallStateChanged(CallStateChanged { id, call }),
            Instant::now(),
        );
    }

    fn headlines(activities: &[Activity]) -> Vec<&str> {
        activities
            .iter()
            .map(|activity| &*activity.headline)
            .collect()
    }

    #[test]
    fn no_devices_no_chips() {
        let feed = Feed::detached(Origin::Phone);
        let mut bridge = PhoneBridge::default();
        assert!(bridge.activities(&PhoneState::default(), &feed).is_empty());
    }

    #[test]
    fn one_device_reads_as_the_design_has_it() {
        let feed = Feed::detached(Origin::Phone);
        let mut state = PhoneState::default();
        state.seed(vec![listed(
            PIXEL,
            "Pixel 8",
            &[
                Feature::Camera,
                Feature::Mic,
                Feature::Mirror,
                Feature::Monitor,
            ],
        )]);
        answered(&mut state, PIXEL, Some("Mum"));
        let activities = PhoneBridge::default().activities(&state, &feed);
        assert_eq!(
            headlines(&activities),
            [
                "Mum",
                "Phone Camera",
                "Phone Mic",
                "Mirroring",
                "Extended Display"
            ]
        );
        let looks: Vec<_> = activities
            .iter()
            .map(|activity| (activity.glyph, activity.accent, activity.controls.clone()))
            .collect();
        assert_eq!(
            looks,
            [
                (Glyph::Call, Accent::Green, vec![Control::HangUp]),
                (Glyph::Webcam, Accent::Red, vec![Control::Stop]),
                (Glyph::Microphone, Accent::Red, vec![Control::Stop]),
                (Glyph::Mirror, Accent::Blue, vec![Control::Stop]),
                (Glyph::ExternalDisplay, Accent::Violet, vec![Control::Stop]),
            ]
        );
        assert!(matches!(activities[0].caption, Caption::Since(_)));
    }

    #[test]
    fn several_devices_never_share_a_chip_and_each_names_its_device() {
        let feed = Feed::detached(Origin::Phone);
        let mut state = PhoneState::default();
        state.seed(vec![
            listed(PIXEL, "Pixel 8", &[Feature::Camera]),
            listed(ONEPLUS, "OnePlus", &[Feature::Camera]),
        ]);
        answered(&mut state, PIXEL, None);
        let mut bridge = PhoneBridge::default();
        let activities = bridge.activities(&state, &feed);
        assert_eq!(
            headlines(&activities),
            [
                "Phone Camera · OnePlus",
                "+447700900123 · Pixel 8",
                "Phone Camera · Pixel 8"
            ]
        );
        let keys: HashSet<_> = activities.iter().map(|activity| &activity.key).collect();
        assert_eq!(keys.len(), 3);

        let ids: Vec<_> = activities.iter().map(|a| a.key.id.clone()).collect();
        assert_eq!(
            bridge.command(&ids[0], Control::Stop),
            Some(PhoneCommand::Stop(ONEPLUS, Stream::Camera))
        );
        assert_eq!(
            bridge.command(&ids[1], Control::HangUp),
            Some(PhoneCommand::HangUp(PIXEL))
        );
        assert_eq!(bridge.command(&ids[1], Control::Stop), None);
        assert_eq!(bridge.command("gone/camera", Control::Stop), None);
    }

    #[test]
    fn a_long_headline_keeps_the_device_that_tells_it_apart() {
        let headline = on_device("Extended Display", "OnePlus 11 5G");
        assert_eq!(headline, "Extended Di… · OnePlus 11 5G");
        assert!(headline.chars().count() <= HEADLINE_CHARS);
        assert_eq!(
            on_device("Phone Camera", "Pixel 8"),
            "Phone Camera · Pixel 8"
        );
    }

    #[test]
    fn a_device_that_goes_takes_its_chips_and_their_controls() {
        let feed = Feed::detached(Origin::Phone);
        let mut state = PhoneState::default();
        state.seed(vec![
            listed(PIXEL, "Pixel 8", &[Feature::Mic]),
            listed(ONEPLUS, "OnePlus", &[]),
        ]);
        answered(&mut state, PIXEL, None);
        let mut bridge = PhoneBridge::default();
        let before = bridge.activities(&state, &feed);
        assert_eq!(before.len(), 2);

        state.seed(vec![listed(ONEPLUS, "OnePlus", &[])]);
        assert!(bridge.activities(&state, &feed).is_empty());
        assert_eq!(bridge.command(&before[0].key.id, Control::HangUp), None);
    }

    fn delivery(id: &str) -> LiveActivity {
        LiveActivity {
            id: id.to_owned(),
            package: "com.ubercab.eats".to_owned(),
            app: "Uber Eats".to_owned(),
            title: "Your order is on its way".to_owned(),
            text: "Arriving at 19:40".to_owned(),
            short_text: Some("12 min".to_owned()),
            category: Some("progress".to_owned()),
            progress: Some(LiveProgress {
                value: 3,
                max: 4,
                indeterminate: false,
            }),
            chronometer: Some(Chronometer {
                base_unix_ms: u64::MAX / 2,
                counts_down: true,
            }),
            promoted: true,
            color_argb: None,
            actions: vec![
                LiveAction {
                    id: "track".to_owned(),
                    label: "Track".to_owned(),
                    accepts_text: false,
                },
                LiveAction {
                    id: "chat".to_owned(),
                    label: "Message Courier".to_owned(),
                    accepts_text: true,
                },
                LiveAction {
                    id: "help".to_owned(),
                    label: "Help".to_owned(),
                    accepts_text: false,
                },
            ],
        }
    }

    fn going_on(state: &mut PhoneState, id: DeviceId, activities: Vec<LiveActivity>) {
        let _ = state.apply(
            Event::LiveActivitiesChanged(LiveActivitiesChanged { id, activities }),
            Instant::now(),
        );
    }

    #[test]
    fn a_live_activity_is_a_chip_whose_buttons_are_pressed_on_its_phone() {
        let feed = Feed::detached(Origin::Phone);
        let mut state = PhoneState::default();
        state.seed(vec![listed(PIXEL, "Pixel 8", &[])]);
        going_on(&mut state, PIXEL, vec![delivery("0|eats|7")]);
        let mut bridge = PhoneBridge::default();
        let activities = bridge.activities(&state, &feed);
        assert_eq!(headlines(&activities), ["12 min"]);
        let chip = &activities[0];
        assert_eq!(&*chip.title, "Uber Eats");
        assert_eq!((chip.glyph, chip.accent), (Glyph::Download, Accent::Blue));
        assert_eq!(chip.progress, Some(Progress::Fixed(0.75)));
        assert!(matches!(chip.caption, Caption::Until(_)));
        assert_eq!(chip.controls, [Control::Action(0), Control::Action(1)]);
        assert_eq!(chip.control_label(Control::Action(1)), "Help");

        assert_eq!(
            bridge.command(&chip.key.id, Control::Action(1)),
            Some(PhoneCommand::LiveAction(
                PIXEL,
                Arc::from("0|eats|7"),
                Arc::from("help")
            ))
        );
        assert_eq!(bridge.command(&chip.key.id, Control::Action(2)), None);
        assert_eq!(bridge.command(&chip.key.id, Control::Stop), None);
    }

    #[test]
    fn a_device_that_stops_sharing_its_activities_takes_their_chips() {
        let feed = Feed::detached(Origin::Phone);
        let mut state = PhoneState::default();
        state.seed(vec![listed(PIXEL, "Pixel 8", &[])]);
        going_on(&mut state, PIXEL, vec![delivery("a"), delivery("b")]);
        let mut bridge = PhoneBridge::default();
        assert_eq!(bridge.activities(&state, &feed).len(), 2);

        let mut withheld = listed(PIXEL, "Pixel 8", &[]);
        withheld.features = withheld.features.without(Feature::LiveActivities);
        state.seed(vec![withheld]);
        assert!(bridge.activities(&state, &feed).is_empty());
        going_on(&mut state, PIXEL, vec![delivery("a")]);
        assert!(bridge.activities(&state, &feed).is_empty());
    }

    #[test]
    fn a_device_finding_this_computer_rings_until_stopped_even_once_gone() {
        let feed = Feed::detached(Origin::Phone);
        let mut state = PhoneState::default();
        state.seed(vec![listed(PIXEL, "Pixel 8", &[])]);
        let _ = state.apply(
            Event::LocalRingChanged(LocalRingChanged { by: Some(PIXEL) }),
            Instant::now(),
        );
        let mut bridge = PhoneBridge::default();
        let activities = bridge.activities(&state, &feed);
        assert_eq!(headlines(&activities), ["Ringing from Pixel 8"]);
        assert_eq!(activities[0].urgency, Urgency::Attention);
        assert_eq!(
            bridge.command(&activities[0].key.id, Control::Stop),
            Some(PhoneCommand::StopLocalRing)
        );

        state.seed(Vec::new());
        let activities = bridge.activities(&state, &feed);
        assert_eq!(headlines(&activities), ["Ringing from a linked device"]);

        let _ = state.apply(
            Event::LocalRingChanged(LocalRingChanged { by: None }),
            Instant::now(),
        );
        assert!(bridge.activities(&state, &feed).is_empty());
    }
}

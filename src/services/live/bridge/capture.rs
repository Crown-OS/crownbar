use std::{convert::Infallible, time::Instant};

use crate::services::{
    audio::{AudioCommand, AudioState},
    live::{
        Accent, Activity, Caption, Control, Feed, Glyph, Origin, Urgency,
        bridge::{Bridge, FirstSeen},
    },
};

const MICROPHONE: &str = "microphone";
const CAMERA: &str = "camera";

/// The microphone and the camera, while something is using them.
#[derive(Default)]
pub struct CaptureBridge {
    seen: FirstSeen<&'static str>,
}

impl Bridge for CaptureBridge {
    type State = AudioState;
    type Command = AudioCommand;
    const ORIGIN: Origin = Origin::Capture;

    fn activities(&mut self, state: &AudioState, feed: &Feed<Infallible>) -> Vec<Activity> {
        let mut listeners: Vec<&str> = state
            .recording
            .iter()
            .filter(|stream| stream.active)
            .map(|stream| stream.app.as_str())
            .collect();
        listeners.sort_unstable();
        listeners.dedup();
        let camera = state.cameras.iter().any(|camera| camera.active);

        let present: Vec<&'static str> = [
            (!listeners.is_empty()).then_some(MICROPHONE),
            camera.then_some(CAMERA),
        ]
        .into_iter()
        .flatten()
        .collect();
        let since = self.seen.track(&present, Instant::now());

        present
            .iter()
            .zip(since)
            .map(|(id, since)| {
                let base = |glyph, title| {
                    Activity::new(feed.key(*id), glyph, title)
                        .accent(Accent::Red)
                        .urgency(Urgency::Attention)
                        .caption(Caption::Since(since))
                };
                match *id {
                    MICROPHONE => base(Glyph::Microphone, "Microphone")
                        .headline(listeners.join(", "))
                        .detail("In use by", listeners.join(", "))
                        .controls([Control::MuteMic]),
                    _ => base(Glyph::Camera, "Camera").headline("Camera"),
                }
            })
            .collect()
    }

    fn command(&self, id: &str, control: Control) -> Option<AudioCommand> {
        (id == MICROPHONE && control == Control::MuteMic)
            .then_some(AudioCommand::SetInputMuted(true))
    }
}

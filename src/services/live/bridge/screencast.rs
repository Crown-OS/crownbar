use std::{convert::Infallible, time::Instant};

use crate::services::{
    live::{
        Accent, Activity, Caption, Control, Feed, Glyph, Origin, Urgency,
        bridge::{Bridge, FirstSeen},
    },
    screencast::{ScreencastCommand, ScreencastState},
};

/// Every app watching the screen, with how long it has been.
#[derive(Default)]
pub struct ScreencastBridge {
    seen: FirstSeen<u64>,
}

impl Bridge for ScreencastBridge {
    type State = ScreencastState;
    type Command = ScreencastCommand;
    const ORIGIN: Origin = Origin::Screencast;

    fn activities(&mut self, state: &ScreencastState, feed: &Feed<Infallible>) -> Vec<Activity> {
        let ids: Vec<u64> = state.casts.iter().map(|cast| cast.id).collect();
        let since = self.seen.track(&ids, Instant::now());
        state
            .casts
            .iter()
            .zip(since)
            .map(|(cast, since)| {
                Activity::new(
                    feed.key(cast.id.to_string()),
                    Glyph::Screen,
                    "Screen Sharing",
                )
                .headline(cast.app.as_str())
                .caption(Caption::Since(since))
                .accent(Accent::Red)
                .urgency(Urgency::Attention)
                .detail("App", cast.app.as_str())
                .detail("Sees", cast.label.as_str())
                .controls([Control::Stop])
            })
            .collect()
    }

    fn command(&self, id: &str, control: Control) -> Option<ScreencastCommand> {
        match control {
            Control::Stop => id.parse().ok().map(ScreencastCommand::Stop),
            _ => None,
        }
    }
}

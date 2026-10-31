use std::convert::Infallible;

use crate::{
    caffeine::{CaffeineCommand, CaffeineState},
    live::{Activity, Caption, Control, Feed, Glyph, Origin, Urgency, bridge::Bridge},
};

const ID: &str = "caffeine";

/// A timed "keep awake", counting down. One held indefinitely is the
/// caffeine pill's to show; only the countdown is news.
#[derive(Default)]
pub struct CaffeineBridge;

impl Bridge for CaffeineBridge {
    type State = CaffeineState;
    type Command = CaffeineCommand;
    const ORIGIN: Origin = Origin::Caffeine;

    fn activities(&mut self, state: &CaffeineState, feed: &Feed<Infallible>) -> Vec<Activity> {
        let Some(until) = state.until.filter(|_| state.active) else {
            return Vec::new();
        };
        vec![
            Activity::new(feed.key(ID), Glyph::Coffee, "Keep Awake")
                .headline("Awake")
                .caption(Caption::Until(until))
                .urgency(Urgency::Ambient)
                .controls([Control::Stop]),
        ]
    }

    fn command(&self, _id: &str, control: Control) -> Option<CaffeineCommand> {
        (control == Control::Stop).then_some(CaffeineCommand::SetActive(false))
    }
}

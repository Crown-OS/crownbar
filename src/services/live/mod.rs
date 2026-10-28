//! Live activities: whatever is happening right now, as chips beside the clock.
//!
//! Each kind of activity is a [`LiveSource`] of its own — a stopwatch, a
//! coding agent, a track playing — that shows and retires activities. This
//! module seats them on chips and routes the pointer back to whichever source
//! a chip belongs to; [`ingress`] lets processes outside the bar feed them.

mod activity;
mod agent;
mod board;
mod bridge;
mod card;
mod ingress;
mod media;
mod source;
mod stopwatch;
mod timer;

use std::sync::Arc;

pub use activity::{
    Accent, Activity, ActivityKey, Caption, Control, Detail, Glyph, Origin, Progress, Urgency,
};
pub use board::{Board, MAX_CHIPS};
pub use crownbar_live::wire::{StopwatchRequest, TimerRequest};
pub use source::{BoardUpdate, Feed, LiveSource, Signal, Steer};

use tokio::sync::mpsc::{self, UnboundedReceiver};

use crate::services::{
    audio::{AudioCommand, AudioState},
    bus::{Backend, Publisher, Tap},
    caffeine::{CaffeineCommand, CaffeineState},
    live::{
        bridge::{Bridged, CaffeineBridge, CaptureBridge, ScreencastBridge},
        ingress::Ingress,
        source::Sources,
    },
    screencast::{ScreencastCommand, ScreencastState},
};

/// The services the bridges follow.
pub struct Bridges {
    pub audio: Tap<AudioState, AudioCommand>,
    pub screencast: Tap<ScreencastState, ScreencastCommand>,
    pub caffeine: Tap<CaffeineState, CaffeineCommand>,
}

#[derive(Clone, Debug, Default)]
pub struct LiveState {
    pub board: Board,
}

impl LiveState {
    pub fn chip(&self, rank: usize) -> Option<&Arc<Activity>> {
        self.board.chips.get(rank)?.as_ref()
    }
}

#[derive(Clone, Debug)]
pub enum LiveCommand {
    Steer(Origin, Steer),
    Stopwatch(StopwatchRequest),
    Timer(TimerRequest),
}

impl LiveCommand {
    pub fn control(key: &ActivityKey, control: Control) -> Self {
        Self::Steer(key.origin, Steer::Control(key.id.clone(), control))
    }

    pub fn seek(key: &ActivityKey, fraction: f32) -> Self {
        Self::Steer(key.origin, Steer::Seek(key.id.clone(), fraction))
    }
}

pub type Channel = crate::services::bus::Channel<LiveState, LiveCommand>;

pub async fn run(backend: Backend<LiveState, LiveCommand>, bridges: Bridges) {
    let Backend {
        publish,
        mut commands,
    } = backend;
    let (board, mut updates) = mpsc::unbounded_channel();
    let mut sources = Sources::new(board);
    let ingress = Ingress {
        agent: sources.spawn(agent::AgentSource::default()),
        card: sources.spawn(card::CardSource::default()),
        stopwatch: sources.spawn(stopwatch::StopwatchSource::default()),
        timer: sources.spawn(timer::TimerSource::default()),
    };
    sources.spawn(media::MediaSource);
    sources.spawn(Bridged::new(CaptureBridge::default(), bridges.audio));
    sources.spawn(Bridged::new(
        ScreencastBridge::default(),
        bridges.screencast,
    ));
    sources.spawn(Bridged::new(CaffeineBridge, bridges.caffeine));
    let routes = sources.into_routes();
    tokio::spawn(ingress::listen(ingress.clone()));

    loop {
        tokio::select! {
            Some(update) = updates.recv() => apply(&publish, update, &mut updates),
            command = commands.recv() => match command {
                Some(LiveCommand::Steer(origin, steer)) => routes.steer(origin, steer),
                Some(LiveCommand::Stopwatch(request)) => {
                    let _ = ingress.stopwatch.send(request);
                }
                Some(LiveCommand::Timer(request)) => {
                    let _ = ingress.timer.send(request);
                }
                None => return,
            },
        }
    }
}

/// Fold `first` and every update already queued behind it into one
/// snapshot, so a burst of hooks costs the bar one wake.
fn apply(
    publish: &Publisher<LiveState>,
    first: BoardUpdate,
    queued: &mut UnboundedReceiver<BoardUpdate>,
) {
    publish.edit(|state| {
        let mut changed = false;
        let mut next = Some(first);
        while let Some(update) = next {
            changed |= match update {
                BoardUpdate::Show(activity) => state.board.show(activity),
                BoardUpdate::Retire(key) => state.board.retire(&key),
            };
            next = queued.try_recv().ok();
        }
        if changed {
            state.board.seat();
        }
        changed
    });
}

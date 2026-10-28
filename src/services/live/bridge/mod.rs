//! Activities other services already know about: the microphone, the camera,
//! a screen being shared, the machine held awake. A bridge reads a service's
//! snapshots and says which activities they stand for; following the snapshots
//! and keeping the chips in step is the same for every bridge, and lives here.

mod caffeine;
mod capture;
mod screencast;

use std::{convert::Infallible, sync::Arc, time::Instant};

pub use caffeine::CaffeineBridge;
pub use capture::CaptureBridge;
pub use screencast::ScreencastBridge;

use crate::services::{
    bus::Tap,
    live::{Activity, Control, Feed, LiveSource, Origin, Signal, Steer},
};

pub trait Bridge: Send + 'static {
    type State: Send + Sync + 'static;
    type Command: Send + 'static;
    const ORIGIN: Origin;

    /// The activities `state` stands for, keyed through `feed`.
    fn activities(&mut self, state: &Self::State, feed: &Feed<Infallible>) -> Vec<Activity>;

    /// What the service must be told for `control` on chip `id`.
    fn command(&self, id: &str, control: Control) -> Option<Self::Command>;
}

/// A bridge wired to the service it reads.
pub struct Bridged<B: Bridge> {
    bridge: B,
    tap: Tap<B::State, B::Command>,
}

impl<B: Bridge> Bridged<B> {
    pub fn new(bridge: B, tap: Tap<B::State, B::Command>) -> Self {
        Self { bridge, tap }
    }
}

impl<B: Bridge> LiveSource for Bridged<B> {
    type Input = Infallible;
    const ORIGIN: Origin = B::ORIGIN;

    async fn run(self, mut feed: Feed<Self::Input>) {
        let Self {
            mut bridge,
            mut tap,
        } = self;
        let mut shown: Vec<Arc<str>> = Vec::new();
        loop {
            let state = tap.state.borrow_and_update().clone();
            let activities = bridge.activities(&state, &feed);
            for gone in shown
                .iter()
                .filter(|id| !activities.iter().any(|activity| activity.key.id == **id))
            {
                feed.retire(gone.clone());
            }
            shown = activities
                .iter()
                .map(|activity| activity.key.id.clone())
                .collect();
            for activity in activities {
                feed.show(activity);
            }
            tokio::select! {
                changed = tap.state.changed() => if changed.is_err() {
                    return;
                },
                signal = feed.next() => match signal {
                    Some(Signal::Steer(Steer::Control(id, control))) => {
                        if let Some(command) = bridge.command(&id, control) {
                            let _ = tap.commands.send(command);
                        }
                    }
                    Some(Signal::Steer(Steer::Seek(..))) => {}
                    Some(Signal::Input(never)) => match never {},
                    None => return,
                },
            }
        }
    }
}

/// When each of a bridge's activities was first seen, so its clock counts
/// from when it began rather than from the last snapshot.
pub struct FirstSeen<K>(Vec<(K, Instant)>);

impl<K> Default for FirstSeen<K> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<K: PartialEq + Clone> FirstSeen<K> {
    /// Forget everything not in `present`, and return when each of them was
    /// first seen.
    pub fn track(&mut self, present: &[K], now: Instant) -> Vec<Instant> {
        self.0.retain(|(key, _)| present.contains(key));
        present
            .iter()
            .map(|key| match self.0.iter().find(|(held, _)| held == key) {
                Some((_, since)) => *since,
                None => {
                    self.0.push((key.clone(), now));
                    now
                }
            })
            .collect()
    }
}

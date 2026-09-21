//! The boundary between a service backend and the event loop.
//!
//! A service is two halves joined by this module. The backend half owns a
//! [`Publisher`] and a command receiver and lives on the tokio runtime or on a
//! thread of its own; the bar holds a [`Channel`] and never blocks on it.
//!
//! Snapshots travel in a `watch`, so a slow reader only ever misses
//! intermediate states, never the newest one. A [`Wake`] carries no payload of
//! its own: what changed is already published, and the bar re-reads the
//! services it holds when it wakes.

use std::sync::Arc;

use crownshell::calloop::ping::Ping;
use tokio::sync::{mpsc, watch};

/// The one road from any service thread back onto the event loop.
#[derive(Clone)]
pub struct Wake(Ping);

impl Wake {
    pub fn new(ping: Ping) -> Self {
        Self(ping)
    }

    pub fn wake(&self) {
        self.0.ping();
    }
}

/// A backend's outbound half.
pub struct Publisher<S> {
    state: watch::Sender<Arc<S>>,
    wake: Wake,
}

impl<S: Clone> Publisher<S> {
    /// Edit the published snapshot in place and wake the bar if `change`
    /// reports that it altered anything. Returning `false` from a no-op keeps
    /// a chatty backend — PipeWire re-sends `Props` on every nudge — from
    /// costing a repaint.
    pub fn edit(&self, change: impl FnOnce(&mut S) -> bool) -> bool {
        let changed = self
            .state
            .send_if_modified(|held| change(Arc::make_mut(held)));
        if changed {
            self.wake.wake();
        }
        changed
    }

    /// The snapshot as last published, for a backend that needs to read its
    /// own state back rather than mirror it.
    pub fn read(&self) -> Arc<S> {
        self.state.borrow().clone()
    }
}

/// The receiving end of a service's command pipe.
pub type Commands<C> = mpsc::UnboundedReceiver<C>;

/// A backend's half of a service.
pub struct Backend<S, C> {
    pub publish: Publisher<S>,
    pub commands: Commands<C>,
}

/// The bar's half of a service.
pub struct Channel<S, C> {
    state: watch::Receiver<Arc<S>>,
    commands: mpsc::UnboundedSender<C>,
}

impl<S, C> Channel<S, C> {
    /// The newest snapshot. One atomic increment, so a widget may call this
    /// per frame.
    pub fn read(&self) -> Arc<S> {
        self.state.borrow().clone()
    }

    /// Queue a command. Never blocks; a closed pipe means the backend is gone,
    /// which the snapshot's availability already says.
    pub fn send(&self, command: C) {
        let _ = self.commands.send(command);
    }
}

/// Wire up one service.
pub fn connect<S, C>(initial: S, wake: &Wake) -> (Backend<S, C>, Channel<S, C>) {
    let (state, rx) = watch::channel(Arc::new(initial));
    let (tx, commands) = mpsc::unbounded_channel();
    (
        Backend {
            publish: Publisher {
                state,
                wake: wake.clone(),
            },
            commands,
        },
        Channel {
            state: rx,
            commands: tx,
        },
    )
}

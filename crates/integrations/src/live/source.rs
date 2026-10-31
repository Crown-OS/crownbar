use std::{future::Future, sync::Arc};

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::live::activity::{Activity, ActivityKey, Control, Origin};

/// What the board hears from a source.
pub enum BoardUpdate {
    Show(Activity),
    Retire(ActivityKey),
}

/// The pointer, acting on one of a source's chips.
#[derive(Clone, Debug)]
pub enum Steer {
    Control(Arc<str>, Control),
    /// A seek to ∈ [0, 1] along the chip's progress.
    Seek(Arc<str>, f32),
}

pub enum Signal<I> {
    Input(I),
    Steer(Steer),
}

/// Something that puts activities on the bar.
///
/// A source runs on the services' runtime for the life of the bar. It hears
/// its own `Input` — whatever the IPC ingress or the bar routes to it — and
/// the controls of its own chips, and answers by showing and retiring
/// activities through its [`Feed`]. It knows nothing of chips or seating.
pub trait LiveSource: Send + 'static {
    type Input: Send + 'static;
    const ORIGIN: Origin;

    fn run(self, feed: Feed<Self::Input>) -> impl Future<Output = ()> + Send;
}

pub struct Feed<I> {
    origin: Origin,
    board: UnboundedSender<BoardUpdate>,
    inputs: UnboundedReceiver<I>,
    steers: UnboundedReceiver<Steer>,
}

impl<I> Feed<I> {
    pub fn key(&self, id: impl Into<Arc<str>>) -> ActivityKey {
        ActivityKey {
            origin: self.origin,
            id: id.into(),
        }
    }

    pub fn show(&self, activity: Activity) {
        let _ = self.board.send(BoardUpdate::Show(activity));
    }

    pub fn retire(&self, id: impl Into<Arc<str>>) {
        let _ = self.board.send(BoardUpdate::Retire(self.key(id)));
    }

    /// The next thing to act on; `None` once the bar is gone.
    pub async fn next(&mut self) -> Option<Signal<I>> {
        tokio::select! {
            Some(input) = self.inputs.recv() => Some(Signal::Input(input)),
            Some(steer) = self.steers.recv() => Some(Signal::Steer(steer)),
            else => None,
        }
    }
}

/// Starts sources and remembers where each one's controls go.
pub struct Sources {
    board: UnboundedSender<BoardUpdate>,
    routes: Routes,
}

#[derive(Default)]
pub struct Routes(Vec<(Origin, UnboundedSender<Steer>)>);

impl Routes {
    pub fn steer(&self, origin: Origin, steer: Steer) {
        if let Some((_, route)) = self.0.iter().find(|(held, _)| *held == origin) {
            let _ = route.send(steer);
        }
    }
}

impl Sources {
    pub fn new(board: UnboundedSender<BoardUpdate>) -> Self {
        Self {
            board,
            routes: Routes::default(),
        }
    }

    /// Start `source`; the returned sender is its input.
    pub fn spawn<S: LiveSource>(&mut self, source: S) -> UnboundedSender<S::Input> {
        let (input, inputs) = unbounded_channel();
        let (steer, steers) = unbounded_channel();
        self.routes.0.push((S::ORIGIN, steer));
        let feed = Feed {
            origin: S::ORIGIN,
            board: self.board.clone(),
            inputs,
            steers,
        };
        tokio::spawn(source.run(feed));
        input
    }

    pub fn into_routes(self) -> Routes {
        self.routes
    }
}

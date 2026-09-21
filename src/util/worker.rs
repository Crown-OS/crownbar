//! One-shot background jobs.
//!
//! The panels behind the bar's popups read their contents from subprocesses —
//! `wpctl`, `bluetoothctl`, `nmcli`. A scan can take a hundred milliseconds,
//! which is several dropped frames if it runs on the event-loop thread while
//! the panel is animating open. A [`Job`] runs one of those readings on a
//! thread and hands the result back the next time the loop asks for it.
//!
//! Only one run is ever in flight: a request made while the previous one is
//! still going is dropped, so a 60 Hz poll cannot pile up processes.

use std::sync::mpsc::{self, Receiver, TryRecvError};

pub struct Job<T> {
    rx: Option<Receiver<T>>,
}

impl<T: Send + 'static> Job<T> {
    pub const fn idle() -> Self {
        Self { rx: None }
    }

    /// Whether a run is still going.
    pub fn in_flight(&self) -> bool {
        self.rx.is_some()
    }

    /// Start `f` on a worker thread unless one is already running. Returns
    /// whether it started.
    pub fn request(&mut self, f: impl FnOnce() -> T + Send + 'static) -> bool {
        if self.rx.is_some() {
            return false;
        }
        let (tx, rx) = mpsc::channel();
        match std::thread::Builder::new()
            .name("crownbar-job".into())
            .spawn(move || {
                // A closed channel means the popup went away mid-read; that is
                // the normal way a job ends early, not something to report.
                let _ = tx.send(f());
            }) {
            Ok(_) => {
                self.rx = Some(rx);
                true
            }
            Err(e) => {
                log::warn!("could not spawn worker thread: {e}");
                false
            }
        }
    }

    /// Take the result if the run has finished. Never blocks.
    pub fn take(&mut self) -> Option<T> {
        let rx = self.rx.as_ref()?;
        match rx.try_recv() {
            Ok(value) => {
                self.rx = None;
                Some(value)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.rx = None;
                None
            }
        }
    }
}

impl<T: Send + 'static> Default for Job<T> {
    fn default() -> Self {
        Self::idle()
    }
}

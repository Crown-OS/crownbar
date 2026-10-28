use std::{thread, time::Duration};

use crownbar_live::proto::crownbar::Client;
use crownos_ipc::Error;

const FLUSH_ATTEMPTS: u32 = 50;
const FLUSH_PAUSE: Duration = Duration::from_millis(2);

/// A connection to the bar, held across messages that must arrive in order.
/// A bar that is not running is not an error: a hook must never fail the
/// agent that ran it.
pub struct Bar(Option<Client>);

impl Bar {
    pub fn connect() -> Self {
        Self(Client::connect().ok())
    }

    pub fn send(&mut self, message: impl FnOnce(&mut Client) -> Result<(), Error>) {
        let Some(bar) = &mut self.0 else {
            return;
        };
        if message(bar).is_err() {
            self.0 = None;
            return;
        }
        for _ in 0..FLUSH_ATTEMPTS {
            if !bar.interest().write || bar.handle_writable().is_err() {
                return;
            }
            thread::sleep(FLUSH_PAUSE);
        }
    }
}

/// Deliver one message on a connection of its own.
pub fn send(message: impl FnOnce(&mut Client) -> Result<(), Error>) {
    Bar::connect().send(message);
}

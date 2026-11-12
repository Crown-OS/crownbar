//! A Wayland connection of a service's own, driven from the tokio runtime.
//!
//! libwayland reports a drained socket as a successful read of nothing, so
//! readiness is cleared by asking the socket itself; trusting the read would
//! leave tokio waking the task in a loop forever.

use std::{
    io,
    os::fd::{AsFd, AsRawFd, RawFd},
};

use tokio::io::unix::AsyncFd;
use wayland_client::{Connection, DispatchError, EventQueue, QueueHandle, backend::WaylandError};

use super::fd;

#[derive(Debug, thiserror::Error)]
pub enum PumpError {
    #[error("Wayland dispatch failed: {0}")]
    Dispatch(#[from] DispatchError),
    #[error("Wayland connection failed: {0}")]
    Wayland(#[from] WaylandError),
    #[error(transparent)]
    Io(#[from] io::Error),
}

pub struct Pump<S> {
    connection: Connection,
    queue: EventQueue<S>,
    readable: AsyncFd<RawFd>,
}

impl<S> Pump<S> {
    /// Must be called on a tokio runtime with IO enabled.
    pub fn new(connection: Connection, queue: EventQueue<S>) -> io::Result<Self> {
        let readable = AsyncFd::new(connection.as_fd().as_raw_fd())?;
        Ok(Self {
            connection,
            queue,
            readable,
        })
    }

    pub fn handle(&self) -> QueueHandle<S> {
        self.queue.handle()
    }

    pub fn flush(&self) -> Result<(), WaylandError> {
        self.connection.flush()
    }

    /// Waits for the compositor and hands whatever it said to `state`.
    pub async fn dispatch(&mut self, state: &mut S) -> Result<(), PumpError> {
        self.queue.dispatch_pending(state)?;
        self.connection.flush()?;
        let mut ready = self.readable.readable().await?;
        if let Some(guard) = self.queue.prepare_read() {
            match guard.read() {
                Ok(_) => {}
                Err(WaylandError::Io(error)) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error.into()),
            }
        }
        if !fd::has_input(self.connection.as_fd())? {
            ready.clear_ready();
        }
        self.queue.dispatch_pending(state)?;
        Ok(())
    }
}

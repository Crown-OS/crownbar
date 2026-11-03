//! Every known device's own event stream, each subscribed once.
//!
//! A subscription is a D-Bus match rule, so the set only grows by the devices
//! that are new, and a device whose stream ends is forgotten so it is
//! subscribed afresh if it comes back.

use std::collections::HashSet;

use bluer::{Address, Device};
use futures_util::{
    FutureExt, StreamExt,
    stream::{self, BoxStream, SelectAll},
};

enum Event {
    Changed,
    Ended(Address),
}

#[derive(Default)]
pub(super) struct DeviceWatch {
    streams: SelectAll<BoxStream<'static, Event>>,
    watched: HashSet<Address>,
}

impl DeviceWatch {
    pub(super) async fn follow(&mut self, device: &Device) {
        let address = device.address();
        if self.watched.contains(&address) {
            return;
        }
        match device.events().await {
            Ok(events) => {
                let ended = stream::once(async move { Event::Ended(address) });
                self.streams
                    .push(events.map(|_| Event::Changed).chain(ended).boxed());
                self.watched.insert(address);
            }
            Err(e) => log::debug!("cannot follow {address}: {e}"),
        }
    }

    /// Resolves at the next change to a watched device, having taken every
    /// other change already queued with it: one refresh answers a burst.
    pub(super) async fn changed(&mut self) {
        loop {
            match self.streams.next().await {
                Some(Event::Changed) => break,
                Some(Event::Ended(address)) => {
                    self.watched.remove(&address);
                }
                None => std::future::pending::<()>().await,
            }
        }
        while let Some(Some(event)) = self.streams.next().now_or_never() {
            if let Event::Ended(address) = event {
                self.watched.remove(&address);
            }
        }
    }
}

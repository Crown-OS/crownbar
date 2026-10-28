use crownbar_live::{
    proto::crownbar::{self, Handler},
    wire::{AgentReport, Card, Outcome, StopwatchRequest, TimerRequest},
};
use crownos_ipc::{ServiceBuilder, adapter::tokio::serve};
use tokio::sync::mpsc::UnboundedSender;

use crate::services::live::card::CardInput;

/// Where each kind of message from outside the bar goes.
#[derive(Clone)]
pub struct Ingress {
    pub agent: UnboundedSender<AgentReport>,
    pub card: UnboundedSender<CardInput>,
    pub stopwatch: UnboundedSender<StopwatchRequest>,
    pub timer: UnboundedSender<TimerRequest>,
}

impl Handler for Ingress {
    fn report_agent(&mut self, report: AgentReport) {
        let _ = self.agent.send(report);
    }

    fn post_card(&mut self, card: Card) {
        let _ = self.card.send(CardInput::Post(card));
    }

    fn end_card(&mut self, key: String, outcome: Outcome) {
        let _ = self.card.send(CardInput::End(key, outcome));
    }

    fn stopwatch(&mut self, request: StopwatchRequest) {
        let _ = self.stopwatch.send(request);
    }

    fn timer(&mut self, request: TimerRequest) {
        let _ = self.timer.send(request);
    }
}

/// Answer `crownbar-live` on `$XDG_RUNTIME_DIR/crownos/crownbar.sock` for the
/// life of the bar.
pub async fn listen(mut ingress: Ingress) {
    let mut server = match ServiceBuilder::new(crownbar::SERVICE).build() {
        Ok(server) => server,
        Err(error) => {
            log::warn!("live activities will hear nothing from outside the bar: {error}");
            return;
        }
    };
    let served = serve(&mut server, |server, peer, message| {
        crownbar::dispatch(&mut ingress, server, peer, message)?;
        Ok(true)
    })
    .await;
    if let Err(error) = served {
        log::warn!("the live activities socket closed: {error}");
    }
}

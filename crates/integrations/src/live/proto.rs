use crate::live::wire::{AgentReport, Card, Outcome, StopwatchRequest, TimerRequest};

crownos_ipc::protocol! {
    pub mod crownbar: "crownbar" {
        /// A coding agent's lifecycle, one hook at a time.
        notify report_agent(report: AgentReport);
        /// Show a card, or replace the one already shown under its key.
        notify post_card(card: Card);
        /// Close a card, saying how what it tracked went.
        notify end_card(key: String, outcome: Outcome);
        notify stopwatch(request: StopwatchRequest);
        notify timer(request: TimerRequest);
    }
}

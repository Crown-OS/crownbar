use std::time::Duration;

use clap::Subcommand;
use crownbar_live::wire::TimerRequest;

use crate::{cli::bar, util::duration};

#[derive(Subcommand)]
pub enum TimerVerb {
    /// Count down from `25m`, `90s`, `1h30m`.
    Start {
        #[arg(value_parser = duration::parse)]
        length: Duration,
        #[arg(long)]
        label: Option<String>,
    },
    Cancel,
}

pub fn timer(verb: TimerVerb) {
    let request = match verb {
        TimerVerb::Start { length, label } => TimerRequest::Start {
            seconds: u32::try_from(length.as_secs()).unwrap_or(u32::MAX),
            label,
        },
        TimerVerb::Cancel => TimerRequest::CancelAll,
    };
    bar::send(|bar| bar.timer(request));
}

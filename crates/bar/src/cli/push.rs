use std::time::Duration;

use clap::Args;
use crownbar_integrations::live::wire::{Accent, Card, CardClock, Glyph, Outcome, Urgency};

use crate::{
    cli::bar,
    util::{duration, epoch},
};

#[derive(Args)]
pub struct PushArgs {
    #[arg(long)]
    key: String,
    #[arg(long)]
    title: String,
    #[arg(long)]
    detail: Option<String>,
    #[arg(long, value_enum, default_value_t)]
    glyph: Glyph,
    #[arg(long, value_enum, default_value_t)]
    accent: Accent,
    #[arg(long, value_enum, default_value_t)]
    urgency: Urgency,
    /// ∈ [0, 1].
    #[arg(long)]
    progress: Option<f32>,
    /// Count up from now.
    #[arg(long, conflicts_with = "until")]
    since: bool,
    /// Count down to now plus this.
    #[arg(long, value_parser = duration::parse)]
    until: Option<Duration>,
    #[arg(long, value_parser = duration::parse)]
    ttl: Option<Duration>,
    #[arg(long, value_parser = duration::parse)]
    reveal_after: Option<Duration>,
    /// Retire the card when this process exits.
    #[arg(long)]
    pid: Option<u32>,
}

#[derive(Args)]
pub struct EndArgs {
    key: String,
    /// An exit status: zero ends the card as done, anything else as failed.
    #[arg(long)]
    status: Option<i32>,
}

impl From<PushArgs> for Card {
    fn from(args: PushArgs) -> Self {
        let clock = match (args.since, args.until) {
            (true, _) => Some(CardClock::Since(epoch::millis_from_now(Duration::ZERO))),
            (false, Some(left)) => Some(CardClock::Until(epoch::millis_from_now(left))),
            (false, None) => None,
        };
        Self {
            key: args.key,
            headline: args.title,
            detail: args.detail,
            glyph: args.glyph,
            accent: args.accent,
            urgency: args.urgency,
            clock,
            progress: args.progress.map(|progress| progress.clamp(0.0, 1.0)),
            reveal_after_ms: args.reveal_after.map_or(0, millis),
            ttl_ms: args.ttl.map(millis),
            pid: args.pid,
        }
    }
}

fn millis(duration: Duration) -> u32 {
    u32::try_from(duration.as_millis()).unwrap_or(u32::MAX)
}

pub fn push(args: PushArgs) {
    let card = Card::from(args);
    bar::send(|bar| bar.post_card(card));
}

pub fn end(args: EndArgs) {
    let outcome = args.status.map_or(Outcome::Dropped, Outcome::from_status);
    bar::send(|bar| bar.end_card(args.key, outcome));
}

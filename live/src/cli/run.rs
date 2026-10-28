use std::{
    os::unix::process::ExitStatusExt,
    process::{Command, ExitCode},
    time::Duration,
};

use anyhow::{Context, Result};
use clap::Args;
use crownbar_live::wire::{Card, CardClock, Glyph, Outcome};

use crate::{cli::bar::Bar, util::epoch};

const TITLE_CHARS: usize = 40;
/// How a shell reports a process killed by a signal.
const SIGNAL_STATUS_BASE: i32 = 128;

#[derive(Args)]
pub struct RunArgs {
    #[arg(long)]
    title: Option<String>,
    #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<String>,
}

pub fn run(args: RunArgs) -> Result<ExitCode> {
    let key = format!("run-{}", std::process::id());
    let headline = args
        .title
        .unwrap_or_else(|| args.command.join(" ").chars().take(TITLE_CHARS).collect());
    let card = Card {
        key: key.clone(),
        headline,
        glyph: Glyph::Terminal,
        clock: Some(CardClock::Since(epoch::millis_from_now(Duration::ZERO))),
        pid: Some(std::process::id()),
        ..Card::default()
    };
    let mut bar = Bar::connect();
    bar.send(|bar| bar.post_card(card));

    let (program, rest) = args.command.split_first().context("nothing to run")?;
    let status = Command::new(program)
        .args(rest)
        .status()
        .with_context(|| format!("could not run `{program}`"));
    let code = match &status {
        Ok(status) => status
            .code()
            .or_else(|| status.signal().map(|signal| SIGNAL_STATUS_BASE + signal))
            .unwrap_or(1),
        Err(_) => 127,
    };
    bar.send(|bar| bar.end_card(key, Outcome::from_status(code)));
    status?;
    Ok(ExitCode::from(u8::try_from(code).unwrap_or(1)))
}

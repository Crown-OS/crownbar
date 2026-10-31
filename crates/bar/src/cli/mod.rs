mod bar;
mod clock;
mod hook;
mod install;
mod push;
mod run;

use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};
use crownbar_integrations::live::wire::{AgentTool, StopwatchRequest};

use crate::app;
use install::Tool;

/// The CrownOS bar. Without a subcommand it runs the bar; the subcommands feed
/// its live activities.
#[derive(Parser)]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    verb: Option<Verb>,
}

#[derive(Subcommand)]
enum Verb {
    /// Translate one coding-agent hook, read from stdin (or the last argument).
    Hook {
        tool: AgentTool,
        payload: Option<String>,
    },
    /// Run a command, showing it as a chip until it exits.
    Run(run::RunArgs),
    /// Show a card, or replace the one under the same key.
    Push(push::PushArgs),
    /// Close a card.
    End(push::EndArgs),
    Stopwatch {
        request: StopwatchRequest,
    },
    #[command(subcommand)]
    Timer(clock::TimerVerb),
    /// Wire every detected coding agent and shell to the bar.
    Install {
        #[arg(long)]
        dry_run: bool,
        tools: Vec<Tool>,
    },
    Uninstall {
        #[arg(long)]
        dry_run: bool,
        tools: Vec<Tool>,
    },
}

impl Cli {
    pub fn run(self) -> Result<ExitCode> {
        let Some(verb) = self.verb else {
            app::run()?;
            return Ok(ExitCode::SUCCESS);
        };
        match verb {
            Verb::Hook { tool, payload } => hook::run(tool, payload.as_deref()),
            Verb::Run(args) => return run::run(args),
            Verb::Push(args) => push::push(args),
            Verb::End(args) => push::end(args),
            Verb::Stopwatch { request } => bar::send(|bar| bar.stopwatch(request)),
            Verb::Timer(verb) => clock::timer(verb),
            Verb::Install { dry_run, tools } => install::install(&tools, dry_run)?,
            Verb::Uninstall { dry_run, tools } => install::uninstall(&tools, dry_run)?,
        }
        Ok(ExitCode::SUCCESS)
    }
}

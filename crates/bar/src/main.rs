mod app;
mod cli;
mod util;

use std::process::ExitCode;

use clap::Parser;

fn main() -> anyhow::Result<ExitCode> {
    cli::Cli::parse().run()
}

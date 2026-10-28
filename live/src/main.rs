mod cli;
mod install;
mod util;

use clap::Parser;

fn main() -> anyhow::Result<std::process::ExitCode> {
    cli::Cli::parse().run()
}

mod claude_code;
mod codex;
mod fish;
mod gemini;
mod hooks;
mod opencode;

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::ValueEnum;

pub use hooks::Change;

/// One program the bar can hear from, and how to make it talk.
pub trait Integration {
    fn name(&self) -> &'static str;
    /// Whether the program is set up for this user at all.
    fn detected(&self, home: &Path) -> bool;
    /// What wiring it to `exe` takes. Described rather than done, so a dry run
    /// and a test can read it.
    fn changes(&self, home: &Path, exe: &str) -> Vec<Change>;
    fn removals(&self, home: &Path) -> Vec<Change>;
    /// Anything the user still has to do by hand.
    fn note(&self) -> Option<&'static str> {
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Tool {
    ClaudeCode,
    Codex,
    Gemini,
    #[value(name = "opencode")]
    OpenCode,
    Fish,
}

impl Tool {
    const ALL: [Self; 5] = [
        Self::ClaudeCode,
        Self::Codex,
        Self::Gemini,
        Self::OpenCode,
        Self::Fish,
    ];

    fn integration(self) -> &'static dyn Integration {
        match self {
            Self::ClaudeCode => &claude_code::ClaudeCode,
            Self::Codex => &codex::Codex,
            Self::Gemini => &gemini::Gemini,
            Self::OpenCode => &opencode::OpenCode,
            Self::Fish => &fish::Fish,
        }
    }
}

fn home() -> Result<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .context("$HOME is not set")
}

/// The named tools, or every one this user has when none is named.
fn chosen(tools: &[Tool], home: &Path) -> Vec<&'static dyn Integration> {
    match tools {
        [] => Tool::ALL
            .iter()
            .map(|tool| tool.integration())
            .filter(|integration| integration.detected(home))
            .collect(),
        named => named.iter().map(|tool| tool.integration()).collect(),
    }
}

pub fn install(tools: &[Tool], dry_run: bool) -> Result<()> {
    let home = home()?;
    let exe = fs::canonicalize(env::current_exe()?)?;
    let exe = exe.to_str().context("the binary's path is not UTF-8")?;
    let integrations = chosen(tools, &home);
    if integrations.is_empty() {
        bail!("found no coding agent or shell to wire up");
    }
    for integration in integrations {
        println!("{}:", integration.name());
        for change in integration.changes(&home, exe) {
            change.apply(dry_run)?;
        }
        if let Some(note) = integration.note() {
            println!("  note: {note}");
        }
    }
    Ok(())
}

pub fn uninstall(tools: &[Tool], dry_run: bool) -> Result<()> {
    let home = home()?;
    for integration in chosen(tools, &home) {
        println!("{}:", integration.name());
        for change in integration.removals(&home) {
            change.apply(dry_run)?;
        }
    }
    Ok(())
}

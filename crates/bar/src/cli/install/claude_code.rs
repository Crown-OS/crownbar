use std::path::{Path, PathBuf};

use serde_json::json;

use crate::cli::install::{
    Change, Integration,
    hooks::{command, groups},
};

/// Claude Code loads a plugin it finds here on the next session, with no
/// marketplace in between.
const PLUGIN_DIR: &str = ".claude/skills/crownbar-live";
const EVENTS: [&str; 10] = [
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "Notification",
    "PreCompact",
    "Stop",
    "StopFailure",
    "SessionEnd",
];

pub struct ClaudeCode;

fn plugin_dir(home: &Path) -> PathBuf {
    home.join(PLUGIN_DIR)
}

impl Integration for ClaudeCode {
    fn name(&self) -> &'static str {
        "Claude Code"
    }

    fn detected(&self, home: &Path) -> bool {
        home.join(".claude").is_dir()
    }

    fn changes(&self, home: &Path, exe: &str) -> Vec<Change> {
        let dir = plugin_dir(home);
        let manifest = json!({
            "name": "crownbar-live",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Shows what Claude Code is doing as a live chip on the CrownOS bar.",
        });
        // Asynchronous, so a bar that is down or slow never holds up a turn.
        let hooks = json!({
            "hooks": groups(&EVENTS, &command(exe, "claude-code"), &json!({ "async": true })),
        });
        vec![
            Change::Write {
                path: dir.join(".claude-plugin/plugin.json"),
                contents: manifest.to_string(),
            },
            Change::Write {
                path: dir.join("hooks/hooks.json"),
                contents: hooks.to_string(),
            },
        ]
    }

    fn removals(&self, home: &Path) -> Vec<Change> {
        vec![Change::Remove {
            path: plugin_dir(home),
        }]
    }

    fn note(&self) -> Option<&'static str> {
        Some("loads as crownbar-live@skills-dir from the next Claude Code session")
    }
}

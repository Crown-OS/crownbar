use std::path::Path;

use serde_json::json;

use crate::cli::install::{
    Change, Integration,
    hooks::{command, groups},
};

const HOOKS_FILE: &str = ".codex/hooks.json";
const EVENTS: [&str; 8] = [
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PermissionRequest",
    "PreCompact",
    "Stop",
    "SessionEnd",
];
const TIMEOUT_SECS: u32 = 5;

pub struct Codex;

impl Integration for Codex {
    fn name(&self) -> &'static str {
        "Codex"
    }

    fn detected(&self, home: &Path) -> bool {
        home.join(".codex").is_dir()
    }

    fn changes(&self, home: &Path, exe: &str) -> Vec<Change> {
        vec![Change::MergeHooks {
            path: home.join(HOOKS_FILE),
            hooks: groups(
                &EVENTS,
                &command(exe, "codex"),
                &json!({ "timeout": TIMEOUT_SECS }),
            ),
        }]
    }

    fn removals(&self, home: &Path) -> Vec<Change> {
        vec![Change::StripHooks {
            path: home.join(HOOKS_FILE),
        }]
    }

    fn note(&self) -> Option<&'static str> {
        Some("Codex asks you to trust new hooks once: run /hooks in Codex")
    }
}

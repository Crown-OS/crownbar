use std::path::Path;

use serde_json::json;

use crate::cli::install::{
    Change, Integration,
    hooks::{command, groups},
};

const SETTINGS_FILE: &str = ".gemini/settings.json";
const EVENTS: [&str; 8] = [
    "SessionStart",
    "BeforeAgent",
    "BeforeTool",
    "AfterTool",
    "Notification",
    "PreCompress",
    "AfterAgent",
    "SessionEnd",
];
/// Gemini waits for its hooks, so this is how long a turn can be held up.
const TIMEOUT_MS: u32 = 2000;

pub struct Gemini;

impl Integration for Gemini {
    fn name(&self) -> &'static str {
        "Gemini CLI"
    }

    fn detected(&self, home: &Path) -> bool {
        home.join(".gemini").is_dir()
    }

    fn changes(&self, home: &Path, exe: &str) -> Vec<Change> {
        vec![Change::MergeHooks {
            path: home.join(SETTINGS_FILE),
            hooks: groups(
                &EVENTS,
                &command(exe, "gemini"),
                &json!({ "name": "crownbar-live", "timeout": TIMEOUT_MS }),
            ),
        }]
    }

    fn removals(&self, home: &Path) -> Vec<Change> {
        vec![Change::StripHooks {
            path: home.join(SETTINGS_FILE),
        }]
    }
}

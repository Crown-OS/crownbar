use std::path::{Path, PathBuf};

use crate::install::{Change, Integration};

const PLUGIN: &str = include_str!("../../assets/opencode/crownbar-live.js");
const PLUGIN_FILE: &str = ".config/opencode/plugins/crownbar-live.js";

pub struct OpenCode;

fn plugin_file(home: &Path) -> PathBuf {
    home.join(PLUGIN_FILE)
}

impl Integration for OpenCode {
    fn name(&self) -> &'static str {
        "opencode"
    }

    fn detected(&self, home: &Path) -> bool {
        home.join(".config/opencode").is_dir()
    }

    fn changes(&self, home: &Path, exe: &str) -> Vec<Change> {
        vec![Change::Write {
            path: plugin_file(home),
            contents: PLUGIN.replace("{exe}", exe),
        }]
    }

    fn removals(&self, home: &Path) -> Vec<Change> {
        vec![Change::Remove {
            path: plugin_file(home),
        }]
    }
}

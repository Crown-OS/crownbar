use std::path::{Path, PathBuf};

use crate::cli::install::{Change, Integration};

const SNIPPET: &str = include_str!("../../../assets/fish/crownbar-live.fish");
const SNIPPET_FILE: &str = ".config/fish/conf.d/crownbar-live.fish";

pub struct Fish;

fn snippet_file(home: &Path) -> PathBuf {
    home.join(SNIPPET_FILE)
}

impl Integration for Fish {
    fn name(&self) -> &'static str {
        "fish"
    }

    fn detected(&self, home: &Path) -> bool {
        home.join(".config/fish").is_dir()
    }

    fn changes(&self, home: &Path, exe: &str) -> Vec<Change> {
        vec![Change::Write {
            path: snippet_file(home),
            contents: SNIPPET.replace("{exe}", exe),
        }]
    }

    fn removals(&self, home: &Path) -> Vec<Change> {
        vec![Change::Remove {
            path: snippet_file(home),
        }]
    }

    fn note(&self) -> Option<&'static str> {
        Some("commands running over 10 s show up from the next fish you open")
    }
}

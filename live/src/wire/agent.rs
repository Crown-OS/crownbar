use serde::{Deserialize, Serialize};

use crate::wire::Accent;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum AgentTool {
    ClaudeCode,
    Codex,
    Gemini,
    #[cfg_attr(feature = "cli", value(name = "opencode"))]
    OpenCode,
}

impl AgentTool {
    pub const fn name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude",
            Self::Codex => "Codex",
            Self::Gemini => "Gemini",
            Self::OpenCode => "opencode",
        }
    }

    pub const fn accent(self) -> Accent {
        match self {
            Self::ClaudeCode => Accent::Coral,
            Self::Codex => Accent::Neutral,
            Self::Gemini => Accent::Blue,
            Self::OpenCode => Accent::Violet,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentReport {
    pub tool: AgentTool,
    pub session: String,
    /// The agent's own process, so the bar can tell when it is gone.
    pub pid: u32,
    pub cwd: String,
    pub event: AgentEvent,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentEvent {
    Opened,
    Prompted,
    Tool { verb: ToolVerb, subject: String },
    ToolDone,
    Waiting { reason: String },
    Compacting,
    Finished,
    Failed { reason: String },
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolVerb {
    Read,
    Edit,
    Run,
    Search,
    Fetch,
    Delegate,
    Other,
}

impl ToolVerb {
    /// Every agent's names for its tools, folded into what they do.
    pub fn classify(tool: &str) -> Self {
        match tool.to_ascii_lowercase().as_str() {
            "read" | "read_file" | "read_many_files" | "view" | "notebookread" => Self::Read,
            "edit" | "multiedit" | "write" | "notebookedit" | "apply_patch" | "patch"
            | "replace" | "write_file" => Self::Edit,
            "bash" | "shell" | "run_shell_command" | "exec_command" | "local_shell" => Self::Run,
            "grep"
            | "glob"
            | "ls"
            | "list"
            | "list_directory"
            | "search_file_content"
            | "codesearch" => Self::Search,
            "webfetch" | "websearch" | "web_fetch" | "google_web_search" | "fetch" => Self::Fetch,
            "task" | "agent" => Self::Delegate,
            _ => Self::Other,
        }
    }

    pub const fn gerund(self) -> &'static str {
        match self {
            Self::Read => "Reading",
            Self::Edit => "Editing",
            Self::Run => "Running",
            Self::Search => "Searching",
            Self::Fetch => "Browsing",
            Self::Delegate => "Delegating",
            Self::Other => "Working",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_agents_tool_names_fold_into_one_verb() {
        for (name, verb) in [
            ("Edit", ToolVerb::Edit),
            ("apply_patch", ToolVerb::Edit),
            ("write_file", ToolVerb::Edit),
            ("Bash", ToolVerb::Run),
            ("run_shell_command", ToolVerb::Run),
            ("Grep", ToolVerb::Search),
            ("read_file", ToolVerb::Read),
            ("Task", ToolVerb::Delegate),
            ("mcp__linear__list_issues", ToolVerb::Other),
        ] {
            assert_eq!(ToolVerb::classify(name), verb, "{name}");
        }
    }
}

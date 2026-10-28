use std::{borrow::Cow, io::Read, path::Path};

use crownbar_live::wire::{AgentEvent, AgentReport, AgentTool, ToolVerb};
use serde::Deserialize;
use serde_json::Value;

use crate::{cli::bar, util::ancestry};

const SUBJECT_CHARS: usize = 64;
const SUBJECT_KEYS: [&str; 7] = [
    "file_path",
    "absolute_path",
    "path",
    "command",
    "pattern",
    "url",
    "query",
];

/// The fields every agent's hooks share. Claude Code and Codex name their
/// events alike; Gemini's names differ but never collide, so one table reads
/// them all, and opencode's plugin shim speaks Claude's names.
#[derive(Deserialize)]
struct Hook<'a> {
    #[serde(borrow)]
    session_id: Cow<'a, str>,
    #[serde(borrow, default)]
    cwd: Cow<'a, str>,
    #[serde(borrow)]
    hook_event_name: Cow<'a, str>,
    #[serde(borrow, default)]
    tool_name: Option<Cow<'a, str>>,
    #[serde(default)]
    tool_input: Value,
    #[serde(borrow, default)]
    notification_type: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    message: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    error: Option<Cow<'a, str>>,
}

impl Hook<'_> {
    fn event(&self) -> Option<AgentEvent> {
        Some(match self.hook_event_name.as_ref() {
            "SessionStart" => AgentEvent::Opened,
            "UserPromptSubmit" | "BeforeAgent" => AgentEvent::Prompted,
            "PreToolUse" | "BeforeTool" => self.tool(),
            "PostToolUse" | "PostToolUseFailure" | "AfterTool" => AgentEvent::ToolDone,
            "PermissionRequest" => self.waiting(),
            "Notification" => match self.notification_type.as_deref()? {
                "permission_prompt" | "elicitation_dialog" | "agent_needs_input"
                | "ToolPermission" => self.waiting(),
                _ => return None,
            },
            "PreCompact" | "PreCompress" => AgentEvent::Compacting,
            "Stop" | "AfterAgent" => AgentEvent::Finished,
            "StopFailure" => AgentEvent::Failed {
                reason: self.error.as_deref().unwrap_or("Stopped").to_owned(),
            },
            "SessionEnd" => AgentEvent::Closed,
            _ => return None,
        })
    }

    fn tool(&self) -> AgentEvent {
        let name = self.tool_name.as_deref().unwrap_or_default();
        AgentEvent::Tool {
            verb: ToolVerb::classify(name),
            subject: subject(&self.tool_input).unwrap_or_else(|| name.to_owned()),
        }
    }

    fn waiting(&self) -> AgentEvent {
        AgentEvent::Waiting {
            reason: self
                .message
                .as_deref()
                .unwrap_or("Needs your approval")
                .to_owned(),
        }
    }
}

/// What a tool call is about, short enough for a chip: a file's name, a
/// command's first line.
fn subject(input: &Value) -> Option<String> {
    let value = SUBJECT_KEYS
        .iter()
        .find_map(|key| input.get(key)?.as_str())?;
    let line = value.lines().next()?.trim();
    let short = match Path::new(line).file_name() {
        Some(name) if line.starts_with('/') => name.to_str()?,
        _ => line,
    };
    Some(short.chars().take(SUBJECT_CHARS).collect())
}

fn report(tool: AgentTool, payload: &str, pid: u32) -> Option<AgentReport> {
    let hook: Hook = serde_json::from_str(payload).ok()?;
    Some(AgentReport {
        tool,
        event: hook.event()?,
        session: hook.session_id.into_owned(),
        cwd: hook.cwd.into_owned(),
        pid,
    })
}

/// Hooks that read stdout as JSON get an empty object: silence is not valid
/// there.
const fn reply(tool: AgentTool) -> Option<&'static str> {
    match tool {
        AgentTool::Codex | AgentTool::Gemini => Some("{}"),
        AgentTool::ClaudeCode | AgentTool::OpenCode => None,
    }
}

pub fn run(tool: AgentTool, argument: Option<&str>) {
    let mut stdin = String::new();
    let payload = match argument {
        Some(payload) => payload,
        None => {
            let _ = std::io::stdin().read_to_string(&mut stdin);
            &stdin
        }
    };
    if let Some(report) = report(tool, payload, ancestry::agent_pid()) {
        bar::send(|bar| bar.report_agent(report));
    }
    if let Some(reply) = reply(tool) {
        println!("{reply}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(payload: &str) -> Option<AgentEvent> {
        report(AgentTool::ClaudeCode, payload, 1).map(|report| report.event)
    }

    #[test]
    fn a_claude_edit_names_the_file() {
        let payload = r#"{"session_id":"s1","cwd":"/e/CrownOS/crownbar","hook_event_name":"PreToolUse",
            "tool_name":"Edit","tool_input":{"file_path":"/e/CrownOS/crownbar/src/ui/bar.rs","old_string":"a"}}"#;
        assert_eq!(
            event(payload),
            Some(AgentEvent::Tool {
                verb: ToolVerb::Edit,
                subject: "bar.rs".into()
            })
        );
    }

    #[test]
    fn a_command_keeps_its_first_line() {
        let payload = r#"{"session_id":"s1","hook_event_name":"PreToolUse","tool_name":"Bash",
            "tool_input":{"command":"cargo test\necho done"}}"#;
        assert_eq!(
            event(payload),
            Some(AgentEvent::Tool {
                verb: ToolVerb::Run,
                subject: "cargo test".into()
            })
        );
    }

    #[test]
    fn a_permission_prompt_waits_and_an_idle_prompt_is_ignored() {
        let prompt = r#"{"session_id":"s1","hook_event_name":"Notification",
            "notification_type":"permission_prompt","message":"Claude needs your permission to use Bash"}"#;
        assert!(matches!(event(prompt), Some(AgentEvent::Waiting { .. })));
        let idle = r#"{"session_id":"s1","hook_event_name":"Notification","notification_type":"idle_prompt"}"#;
        assert_eq!(event(idle), None);
    }

    #[test]
    fn gemini_names_map_onto_the_same_lifecycle() {
        for (name, expected) in [
            ("BeforeAgent", AgentEvent::Prompted),
            ("AfterAgent", AgentEvent::Finished),
            ("PreCompress", AgentEvent::Compacting),
            ("SessionEnd", AgentEvent::Closed),
        ] {
            let payload = format!(r#"{{"session_id":"g","hook_event_name":"{name}"}}"#);
            assert_eq!(event(&payload), Some(expected), "{name}");
        }
    }

    #[test]
    fn garbage_reports_nothing() {
        assert_eq!(event("not json"), None);
        assert_eq!(
            event(r#"{"session_id":"s","hook_event_name":"Unheard"}"#),
            None
        );
    }
}

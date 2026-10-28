use std::{fs, io::ErrorKind, path::PathBuf};

use anyhow::{Context, Result};
use serde_json::{Map, Value, json};

/// How the bar's own entries are told apart from the user's.
const MARK: &str = "crownbar-live";

pub enum Change {
    Write {
        path: PathBuf,
        contents: String,
    },
    /// Add `hooks` to the `hooks` object of a JSON settings file, replacing
    /// any of ours already there and keeping everyone else's.
    MergeHooks {
        path: PathBuf,
        hooks: Value,
    },
    StripHooks {
        path: PathBuf,
    },
    Remove {
        path: PathBuf,
    },
}

/// `{ event: [ { hooks: [ { type: command, command, ..extra } ] } ] }` for every
/// event in `events`.
pub fn groups(events: &[&str], command: &str, extra: &Value) -> Value {
    let mut handler = json!({ "type": "command", "command": command });
    if let (Some(handler), Some(extra)) = (handler.as_object_mut(), extra.as_object()) {
        handler.extend(extra.clone());
    }
    let group = json!([{ "hooks": [handler] }]);
    Value::Object(
        events
            .iter()
            .map(|event| ((*event).to_owned(), group.clone()))
            .collect(),
    )
}

/// The hook command, the binary's path quoted for the shell that runs it.
pub fn command(exe: &str, tool: &str) -> String {
    format!("\"{exe}\" hook {tool}")
}

fn ours(group: &Value) -> bool {
    group["hooks"].as_array().is_some_and(|handlers| {
        handlers.iter().any(|handler| {
            handler["command"]
                .as_str()
                .is_some_and(|c| c.contains(MARK))
        })
    })
}

fn strip(settings: &mut Map<String, Value>) {
    let Some(Value::Object(events)) = settings.get_mut("hooks") else {
        return;
    };
    for groups in events.values_mut() {
        if let Value::Array(groups) = groups {
            groups.retain(|group| !ours(group));
        }
    }
    events.retain(|_, groups| groups.as_array().is_none_or(|groups| !groups.is_empty()));
}

fn merge(settings: &mut Map<String, Value>, hooks: &Value) {
    strip(settings);
    let events = settings
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()));
    let (Some(events), Some(hooks)) = (events.as_object_mut(), hooks.as_object()) else {
        return;
    };
    for (event, groups) in hooks {
        let slot = events
            .entry(event.clone())
            .or_insert_with(|| Value::Array(Vec::new()));
        if let (Some(slot), Some(groups)) = (slot.as_array_mut(), groups.as_array()) {
            slot.extend(groups.iter().cloned());
        }
    }
}

fn read_settings(path: &PathBuf) -> Result<Map<String, Value>> {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text)
            .with_context(|| format!("{} is not a JSON object", path.display())),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(Map::new()),
        Err(error) => Err(error).with_context(|| format!("could not read {}", path.display())),
    }
}

fn write(path: &PathBuf, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents).with_context(|| format!("could not write {}", path.display()))
}

impl Change {
    pub fn apply(&self, dry_run: bool) -> Result<()> {
        let (verb, path) = match self {
            Self::Write { path, .. } => ("write", path),
            Self::MergeHooks { path, .. } => ("add hooks to", path),
            Self::StripHooks { path } => ("remove hooks from", path),
            Self::Remove { path } => ("remove", path),
        };
        println!(
            "  {}{verb} {}",
            if dry_run { "would " } else { "" },
            path.display()
        );
        if dry_run {
            return Ok(());
        }
        match self {
            Self::Write { path, contents } => write(path, contents),
            Self::MergeHooks { path, hooks } => {
                let mut settings = read_settings(path)?;
                merge(&mut settings, hooks);
                write(path, &serde_json::to_string_pretty(&settings)?)
            }
            Self::StripHooks { path } if path.exists() => {
                let mut settings = read_settings(path)?;
                strip(&mut settings);
                write(path, &serde_json::to_string_pretty(&settings)?)
            }
            Self::StripHooks { .. } => Ok(()),
            Self::Remove { path } if path.is_dir() => Ok(fs::remove_dir_all(path)?),
            Self::Remove { path } if path.exists() => Ok(fs::remove_file(path)?),
            Self::Remove { .. } => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(value: Value) -> Map<String, Value> {
        match value {
            Value::Object(map) => map,
            _ => Map::new(),
        }
    }

    #[test]
    fn merging_twice_keeps_one_copy_and_the_users_own_hooks() {
        let users = json!({ "hooks": [{ "type": "command", "command": "notify-send hi" }] });
        let mut file = settings(json!({ "theme": "dark", "hooks": { "Stop": [users] } }));
        let ours = groups(
            &["Stop", "SessionStart"],
            &command("/bin/crownbar-live", "codex"),
            &json!({}),
        );
        merge(&mut file, &ours);
        merge(&mut file, &ours);
        assert_eq!(file["theme"], "dark");
        assert_eq!(file["hooks"]["Stop"].as_array().map(Vec::len), Some(2));
        assert_eq!(
            file["hooks"]["SessionStart"].as_array().map(Vec::len),
            Some(1)
        );

        strip(&mut file);
        assert_eq!(file["hooks"]["Stop"].as_array().map(Vec::len), Some(1));
        assert!(file["hooks"].get("SessionStart").is_none());
    }
}

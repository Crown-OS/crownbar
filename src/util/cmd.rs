//! Thin wrappers around one-shot subprocesses.
//!
//! The bar deliberately links no desktop daemon libraries — see
//! [`crate::util::rfkill`]. Where a reading genuinely needs one (the list of
//! audio sinks, of paired Bluetooth devices, of nearby networks) we shell out
//! to the tool that ships with the daemon instead. Every call here is expected
//! to run off the event-loop thread, through [`crate::util::worker::Job`].

use std::{
    path::Path,
    process::{Command, Stdio},
};

/// Run `program args…` and return its stdout, or `None` if it could not be
/// started or exited non-zero.
pub fn output(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

/// Run `program args…` for its side effect. Returns whether it exited zero.
pub fn run(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Start `program args…` and forget about it — for the "… Settings" rows,
/// which hand off to a separate application.
pub fn spawn_detached(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .is_ok()
}

/// Is `program` on `PATH`? Cheaper and quieter than running it with
/// `--version`, and it does not depend on the tool having such a flag.
pub fn exists(program: &str) -> bool {
    if program.contains('/') {
        return Path::new(program).is_file();
    }
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| is_executable(&dir.join(program)))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Launch the first of `candidates` (each a `(program, args)` pair) that is
/// installed. Returns whether anything was started.
pub fn launch_first(candidates: &[(&str, &[&str])]) -> bool {
    for (program, args) in candidates {
        if exists(program) && spawn_detached(program, args) {
            return true;
        }
    }
    false
}

/// `nmcli -t` and friends escape `:` inside fields as `\:`. Split on the
/// unescaped separators and unescape what is left.
pub fn split_escaped(line: &str, sep: char) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut escaped = false;
    for ch in line.chars() {
        if escaped {
            fields.last_mut().expect("seeded above").push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == sep {
            fields.push(String::new());
        } else {
            fields.last_mut().expect("seeded above").push(ch);
        }
    }
    fields
}

/// Value of a `Key: value` line from a tool that prints indented records
/// (`bluetoothctl info`, `bluetoothctl show`).
pub fn field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let line = line.trim();
        let rest = line.strip_prefix(key)?;
        let rest = rest.strip_prefix(':')?;
        Some(rest.trim())
    })
}

//! Starting another application.
//!
//! Nothing in the bar shells out for *data* any more — every reading comes
//! from a daemon library in [`crate::services`]. What is left is handing a
//! subject to the settings app, which [`crate::services::link`] does through
//! these.

use std::{
    path::Path,
    process::{Command, Stdio},
};

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


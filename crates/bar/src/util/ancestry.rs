use std::{fs, os::unix::process::parent_id};

/// Programs an agent wraps a hook in, which die with the hook rather than
/// with the agent.
const SHELLS: [&str; 6] = ["sh", "bash", "dash", "zsh", "fish", "env"];
const MAX_HOPS: usize = 4;

/// The process that ran this hook, past any shell it went through.
pub fn agent_pid() -> u32 {
    let mut pid = parent_id();
    for _ in 0..MAX_HOPS {
        match stat(pid) {
            Some((comm, parent)) if SHELLS.contains(&comm.as_str()) && parent > 1 => pid = parent,
            _ => break,
        }
    }
    pid
}

/// `comm` and parent pid, from `/proc/<pid>/stat`. The name is the text
/// between the first `(` and the last `)`, since it may hold either.
fn stat(pid: u32) -> Option<(String, u32)> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let open = stat.find('(')?;
    let close = stat.rfind(')')?;
    let comm = stat.get(open + 1..close)?.to_owned();
    let parent = stat.get(close + 2..)?.split(' ').nth(1)?.parse().ok()?;
    Some((comm, parent))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_process_reads_its_own_parent() {
        let (_, parent) = stat(std::process::id()).expect("own stat is readable");
        assert_eq!(parent, parent_id());
    }
}

use std::path::Path;

/// Whether process `pid` still exists.
pub fn alive(pid: u32) -> bool {
    Path::new("/proc").join(pid.to_string()).exists()
}

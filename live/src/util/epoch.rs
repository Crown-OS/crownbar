use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Wall-clock milliseconds, `offset` from now.
pub fn millis_from_now(offset: Duration) -> u64 {
    (SystemTime::now() + offset)
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

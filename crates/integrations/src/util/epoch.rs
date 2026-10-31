use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The monotonic instant matching wall-clock `millis` since the epoch.
pub fn instant_at(millis: u64) -> Instant {
    let now = Instant::now();
    let wall = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let at = Duration::from_millis(millis);
    match at.checked_sub(wall) {
        Some(ahead) => now + ahead,
        None => now.checked_sub(wall - at).unwrap_or(now),
    }
}

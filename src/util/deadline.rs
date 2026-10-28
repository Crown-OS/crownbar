use std::time::Instant;

/// Resolves at `deadline`, or never when there is none — the idle arm of a
/// `select!` over a backend's own timers.
pub async fn sleep_until(deadline: Option<Instant>) {
    match deadline {
        Some(instant) => tokio::time::sleep_until(instant.into()).await,
        None => std::future::pending().await,
    }
}

//! Retrying a daemon that is not there.
//!
//! The pause doubles after every failed attempt, so a daemon that is not
//! installed costs a connection attempt every half minute rather than every
//! second, and a restart that is merely in progress is still caught at once.
//! Only a session that held resets it: a daemon that accepts and drops at
//! once would otherwise keep the retries at their fastest.

use std::time::Duration;

use tokio::time::{self, Instant};

use crate::services::bus::Commands;

const FIRST_PAUSE: Duration = Duration::from_secs(1);
const LONGEST_PAUSE: Duration = Duration::from_secs(30);

pub struct Backoff {
    pause: Duration,
}

impl Default for Backoff {
    fn default() -> Self {
        Self { pause: FIRST_PAUSE }
    }
}

impl Backoff {
    /// Ends an attempt made at `started`. One that held for longer than the
    /// longest pause was a working session, and its loss is retried quickly.
    pub fn reset_if_held(&mut self, started: Instant) {
        if started.elapsed() >= LONGEST_PAUSE {
            self.pause = FIRST_PAUSE;
        }
    }

    /// Sleeps out the pause, leaving commands queued for the next connection.
    pub async fn wait(&mut self) {
        time::sleep(self.lengthen()).await;
    }

    /// Sleeps out the pause, discarding whatever is asked for meanwhile.
    /// Returns whether the bar is still there.
    pub async fn wait_discarding<C>(&mut self, commands: &mut Commands<C>) -> bool {
        let deadline = time::sleep(self.lengthen());
        tokio::pin!(deadline);
        loop {
            tokio::select! {
                _ = &mut deadline => return true,
                command = commands.recv() => if command.is_none() {
                    return false;
                },
            }
        }
    }

    fn lengthen(&mut self) -> Duration {
        let pause = self.pause;
        self.pause = (pause * 2).min(LONGEST_PAUSE);
        pause
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_doubles_up_to_the_longest() {
        let mut backoff = Backoff::default();
        let pauses: Vec<_> = (0..7).map(|_| backoff.lengthen().as_secs()).collect();
        assert_eq!(pauses, [1, 2, 4, 8, 16, 30, 30]);
    }

    #[test]
    fn only_a_session_that_held_resets_the_pause() {
        let mut backoff = Backoff::default();
        backoff.lengthen();
        backoff.lengthen();
        backoff.reset_if_held(Instant::now());
        assert_eq!(backoff.pause, Duration::from_secs(4));
        backoff.reset_if_held(Instant::now() - LONGEST_PAUSE);
        assert_eq!(backoff.pause, FIRST_PAUSE);
    }
}

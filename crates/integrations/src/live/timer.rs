use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use crownos_ipc::adapter::tokio::AsyncClient;
use crownotify::proto::protocol;

use crate::{
    live::{
        Accent, Activity, Caption, Control, Feed, Glyph, LiveSource, Origin, Progress, Signal,
        Steer, TimerRequest, Urgency, activity::clock,
    },
    util::deadline::sleep_until,
};

/// How long a finished timer keeps calling for attention.
const RINGING: Duration = Duration::from_secs(60);
const EXTENSION: Duration = Duration::from_secs(60);

#[derive(Default)]
pub struct TimerSource {
    timers: Vec<Timer>,
    next_id: u32,
}

struct Timer {
    id: Arc<str>,
    label: Option<Arc<str>>,
    length: Duration,
    phase: Phase,
}

#[derive(Clone, Copy)]
enum Phase {
    Running { until: Instant },
    Paused { left: Duration },
    Ringing { since: Instant },
}

impl Timer {
    fn deadline(&self) -> Option<Instant> {
        match self.phase {
            Phase::Running { until } => Some(until),
            Phase::Ringing { since } => Some(since + RINGING),
            Phase::Paused { .. } => None,
        }
    }

    fn title(&self) -> Arc<str> {
        self.label.clone().unwrap_or_else(|| Arc::from("Timer"))
    }

    fn activity(&self, feed: &Feed<TimerRequest>) -> Activity {
        let base = Activity::new(feed.key(self.id.clone()), Glyph::Timer, self.title())
            .accent(Accent::Amber);
        match self.phase {
            Phase::Running { until } => base
                .caption(Caption::Until(until))
                .progress(Some(Progress::Draining {
                    until,
                    span: self.length,
                }))
                .controls([Control::Pause, Control::Extend, Control::Cancel]),
            Phase::Paused { left } => base
                .caption(Caption::Frozen(left))
                .progress(Some(Progress::Fixed(
                    left.as_secs_f32() / self.length.as_secs_f32().max(1.0),
                )))
                .controls([Control::Resume, Control::Extend, Control::Cancel]),
            Phase::Ringing { .. } => base
                .glyph(Glyph::Alert)
                .headline("Time's up")
                .urgency(Urgency::Attention)
                .controls([Control::Extend, Control::Dismiss]),
        }
    }
}

impl TimerSource {
    fn start(&mut self, length: Duration, label: Option<String>, now: Instant) {
        self.next_id += 1;
        self.timers.push(Timer {
            id: Arc::from(format!("timer-{}", self.next_id)),
            label: label.map(Arc::from),
            length,
            phase: Phase::Running {
                until: now + length,
            },
        });
    }

    /// Act on timer `id`; returns whether that removed it.
    fn steer(&mut self, id: &str, control: Control, now: Instant) -> bool {
        let Some(index) = self.timers.iter().position(|timer| &*timer.id == id) else {
            return false;
        };
        let timer = &mut self.timers[index];
        timer.phase = match (timer.phase, control) {
            (Phase::Running { until }, Control::Pause) => Phase::Paused {
                left: until.saturating_duration_since(now),
            },
            (Phase::Paused { left }, Control::Resume) => Phase::Running { until: now + left },
            (Phase::Running { until }, Control::Extend) => {
                timer.length += EXTENSION;
                Phase::Running {
                    until: until + EXTENSION,
                }
            }
            (Phase::Paused { left }, Control::Extend) => {
                timer.length += EXTENSION;
                Phase::Paused {
                    left: left + EXTENSION,
                }
            }
            (Phase::Ringing { .. }, Control::Extend) => {
                timer.length = EXTENSION;
                Phase::Running {
                    until: now + EXTENSION,
                }
            }
            (_, Control::Cancel | Control::Dismiss) => {
                self.timers.remove(index);
                return true;
            }
            (phase, _) => phase,
        };
        false
    }

    /// Ring every timer that has run out; drop every one done ringing.
    fn expire(&mut self, feed: &Feed<TimerRequest>, now: Instant) {
        for timer in &mut self.timers {
            if let Phase::Running { until } = timer.phase
                && until <= now
            {
                timer.phase = Phase::Ringing { since: now };
                tokio::spawn(announce(timer.title(), timer.length));
            }
        }
        self.timers.retain(|timer| match timer.phase {
            Phase::Ringing { since } if since + RINGING <= now => {
                feed.retire(timer.id.clone());
                false
            }
            _ => true,
        });
    }

    fn publish(&self, feed: &Feed<TimerRequest>) {
        for timer in &self.timers {
            feed.show(timer.activity(feed));
        }
    }
}

/// Post the "time's up" card through crownotify, if it is running.
async fn announce(title: Arc<str>, length: Duration) {
    let Ok(mut notifications) = AsyncClient::connect(protocol::SERVICE) else {
        return;
    };
    let mut body = String::new();
    clock(&mut body, length);
    body.push_str(" is up");
    let card = protocol::send_general_notification {
        app_name: "Timer".into(),
        summary: title.to_string(),
        body,
        expire_timeout: 0,
        actions: Vec::new(),
    };
    let _ = notifications
        .call::<protocol::send_general_notification>(&card, Vec::new())
        .await;
}

impl LiveSource for TimerSource {
    type Input = TimerRequest;
    const ORIGIN: Origin = Origin::Timer;

    async fn run(mut self, mut feed: Feed<Self::Input>) {
        loop {
            let deadline = self.timers.iter().filter_map(Timer::deadline).min();
            tokio::select! {
                signal = feed.next() => {
                    let now = Instant::now();
                    match signal {
                        Some(Signal::Input(TimerRequest::Start { seconds, label })) => {
                            self.start(Duration::from_secs(seconds.into()), label, now);
                        }
                        Some(Signal::Input(TimerRequest::CancelAll)) => {
                            for timer in self.timers.drain(..) {
                                feed.retire(timer.id);
                            }
                        }
                        Some(Signal::Steer(Steer::Control(id, control))) => {
                            if self.steer(&id, control, now) {
                                feed.retire(id);
                            }
                        }
                        Some(Signal::Steer(Steer::Seek(..))) => {}
                        None => return,
                    }
                }
                _ = sleep_until(deadline) => self.expire(&feed, Instant::now()),
            }
            self.publish(&feed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pausing_keeps_what_was_left_and_extending_adds_a_minute() {
        let now = Instant::now();
        let mut source = TimerSource::default();
        source.start(Duration::from_secs(300), None, now);
        let id = source.timers[0].id.clone();
        source.steer(&id, Control::Pause, now + Duration::from_secs(100));
        source.steer(&id, Control::Extend, now + Duration::from_secs(150));
        assert!(matches!(
            source.timers[0].phase,
            Phase::Paused { left } if left == Duration::from_secs(260)
        ));
        source.steer(&id, Control::Cancel, now);
        assert!(source.timers.is_empty());
    }
}

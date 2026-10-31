use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use crate::live::{
    Accent, Activity, Caption, Control, Feed, Glyph, LiveSource, Origin, Signal, Steer,
    StopwatchRequest, activity::clock,
};

const ID: &str = "stopwatch";
/// Laps the panel lists, newest first.
const SHOWN_LAPS: usize = 5;

#[derive(Default)]
pub struct StopwatchSource {
    /// `None` while there is no stopwatch.
    watch: Option<Watch>,
}

#[derive(Default)]
struct Watch {
    /// Time run before the current stretch.
    banked: Duration,
    running_since: Option<Instant>,
    laps: Vec<Duration>,
}

impl Watch {
    fn elapsed(&self, now: Instant) -> Duration {
        self.banked
            + self
                .running_since
                .map_or(Duration::ZERO, |since| now.saturating_duration_since(since))
    }

    fn pause(&mut self, now: Instant) {
        self.banked = self.elapsed(now);
        self.running_since = None;
    }

    fn resume(&mut self, now: Instant) {
        self.running_since.get_or_insert(now);
    }
}

impl StopwatchSource {
    fn apply(&mut self, request: StopwatchRequest, now: Instant) {
        if request == StopwatchRequest::Reset {
            self.watch = None;
            return;
        }
        let watch = self.watch.get_or_insert_with(Watch::default);
        match request {
            StopwatchRequest::Start | StopwatchRequest::Resume => watch.resume(now),
            StopwatchRequest::Pause => watch.pause(now),
            StopwatchRequest::Toggle if watch.running_since.is_some() => watch.pause(now),
            StopwatchRequest::Toggle => watch.resume(now),
            StopwatchRequest::Lap => watch.laps.push(watch.elapsed(now)),
            StopwatchRequest::Reset => {}
        }
    }

    fn publish(&self, feed: &Feed<StopwatchRequest>, now: Instant) {
        let Some(watch) = &self.watch else {
            feed.retire(ID);
            return;
        };
        let running = watch.running_since.is_some();
        let caption = match running {
            true => Caption::Since(now - watch.elapsed(now)),
            false => Caption::Frozen(watch.banked),
        };
        let pause_or_resume = if running {
            Control::Pause
        } else {
            Control::Resume
        };
        let mut activity = Activity::new(feed.key(ID), Glyph::Stopwatch, "Stopwatch")
            .caption(caption)
            .accent(Accent::Blue)
            .controls([pause_or_resume, Control::Lap, Control::Reset]);
        let mut text = String::new();
        for (index, lap) in watch.laps.iter().enumerate().rev().take(SHOWN_LAPS) {
            text.clear();
            clock(&mut text, *lap);
            activity = activity.detail(format!("Lap {}", index + 1), Arc::from(text.as_str()));
        }
        feed.show(activity);
    }
}

fn request(control: Control) -> Option<StopwatchRequest> {
    Some(match control {
        Control::Pause => StopwatchRequest::Pause,
        Control::Resume => StopwatchRequest::Resume,
        Control::Lap => StopwatchRequest::Lap,
        Control::Reset | Control::Dismiss => StopwatchRequest::Reset,
        _ => return None,
    })
}

impl LiveSource for StopwatchSource {
    type Input = StopwatchRequest;
    const ORIGIN: Origin = Origin::Stopwatch;

    async fn run(mut self, mut feed: Feed<Self::Input>) {
        while let Some(signal) = feed.next().await {
            let request = match signal {
                Signal::Input(request) => Some(request),
                Signal::Steer(Steer::Control(_, control)) => request(control),
                Signal::Steer(Steer::Seek(..)) => None,
            };
            if let Some(request) = request {
                let now = Instant::now();
                self.apply(request, now);
                self.publish(&feed, now);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pausing_banks_the_stretch_and_resuming_continues_it() {
        let start = Instant::now();
        let mut source = StopwatchSource::default();
        source.apply(StopwatchRequest::Start, start);
        source.apply(StopwatchRequest::Pause, start + Duration::from_secs(10));
        source.apply(StopwatchRequest::Toggle, start + Duration::from_secs(100));
        source.apply(StopwatchRequest::Lap, start + Duration::from_secs(105));
        let watch = source.watch.as_ref().expect("started");
        assert_eq!(watch.laps, [Duration::from_secs(15)]);
        source.apply(StopwatchRequest::Reset, start);
        assert!(source.watch.is_none());
    }
}

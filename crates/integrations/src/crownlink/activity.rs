//! A device's ongoing activities: a timer, a delivery, a route.
//!
//! The phone stamps a running clock in wall-clock milliseconds; the bar keeps
//! clocks as instants. Each base is turned into an instant once and kept for as
//! long as the phone keeps reporting that base, so republishing the same
//! activities never nudges its clock or churns the chips.

use std::{sync::Arc, time::Instant};

use crownlink_linux::ipc::proto::{LiveActivity, LiveProgress};

use crate::util::epoch::instant_at;

/// One ongoing activity of a device, as the bar shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhoneActivity {
    /// The phone's key for it, which its actions are pressed by.
    pub id: Arc<str>,
    /// The app's label.
    pub app: Arc<str>,
    pub title: Arc<str>,
    pub text: Arc<str>,
    pub short_text: Option<Arc<str>>,
    /// The platform's category verbatim, such as `navigation` or `stopwatch`.
    pub category: Option<Arc<str>>,
    pub progress: Option<LiveProgress>,
    pub clock: Option<LiveClock>,
    /// The phone ranks it above its other activities.
    pub promoted: bool,
    /// Its buttons, without the ones that take typed text.
    pub actions: Vec<LiveButton>,
}

/// A running clock, counting up from `at` or down to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiveClock {
    base_unix_ms: u64,
    pub counts_down: bool,
    pub at: Instant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveButton {
    pub id: Arc<str>,
    pub label: Arc<str>,
}

impl PhoneActivity {
    /// The words a chip leads with: the phone's short form when it has one.
    pub fn headline(&self) -> &str {
        self.short_text.as_deref().unwrap_or(&self.title)
    }

    /// How far along it is ∈ [0, 1]; `None` when it has no measurable end.
    pub fn fraction(&self) -> Option<f32> {
        let progress = self.progress.filter(|p| !p.indeterminate && p.max > 0)?;
        Some((progress.value as f32 / progress.max as f32).clamp(0.0, 1.0))
    }

    fn new(live: LiveActivity, previous: &[Self]) -> Self {
        let id: Arc<str> = Arc::from(live.id);
        let clock = live.chronometer.map(|chronometer| {
            let kept = previous
                .iter()
                .find(|held| held.id == id)
                .and_then(|held| held.clock)
                .filter(|clock| {
                    clock.base_unix_ms == chronometer.base_unix_ms
                        && clock.counts_down == chronometer.counts_down
                });
            kept.unwrap_or_else(|| LiveClock {
                base_unix_ms: chronometer.base_unix_ms,
                counts_down: chronometer.counts_down,
                at: instant_at(chronometer.base_unix_ms),
            })
        });
        Self {
            id,
            app: Arc::from(live.app),
            title: Arc::from(live.title),
            text: Arc::from(live.text),
            short_text: live.short_text.map(Arc::from),
            category: live.category.map(Arc::from),
            progress: live.progress,
            clock,
            promoted: live.promoted,
            actions: live
                .actions
                .into_iter()
                .filter(|action| !action.accepts_text)
                .map(|action| LiveButton {
                    id: Arc::from(action.id),
                    label: Arc::from(action.label),
                })
                .collect(),
        }
    }
}

/// `held` replaced by everything in `live`, keeping each clock that did not
/// move.
pub(super) fn follow(held: &mut Vec<PhoneActivity>, live: Vec<LiveActivity>) {
    let previous = std::mem::take(held);
    *held = live
        .into_iter()
        .map(|activity| PhoneActivity::new(activity, &previous))
        .collect();
}

#[cfg(test)]
mod tests {
    use crownlink_linux::ipc::proto::{Chronometer, LiveAction};

    use super::*;

    fn timer(id: &str, base_unix_ms: u64) -> LiveActivity {
        LiveActivity {
            id: id.to_owned(),
            package: "com.google.android.deskclock".to_owned(),
            app: "Clock".to_owned(),
            title: "Timer".to_owned(),
            text: "Pasta".to_owned(),
            short_text: Some("4m".to_owned()),
            category: Some("alarm".to_owned()),
            progress: None,
            chronometer: Some(Chronometer {
                base_unix_ms,
                counts_down: true,
            }),
            promoted: true,
            color_argb: None,
            actions: vec![
                LiveAction {
                    id: "pause".to_owned(),
                    label: "Pause".to_owned(),
                    accepts_text: false,
                },
                LiveAction {
                    id: "reply".to_owned(),
                    label: "Reply".to_owned(),
                    accepts_text: true,
                },
            ],
        }
    }

    #[test]
    fn a_clock_that_did_not_move_keeps_its_instant() {
        let mut held = Vec::new();
        follow(&mut held, vec![timer("a", 1_000)]);
        let first = held[0].clock.map(|clock| clock.at);
        follow(&mut held, vec![timer("a", 1_000)]);
        assert_eq!(held[0].clock.map(|clock| clock.at), first);

        follow(&mut held, vec![timer("a", 2_000_000)]);
        assert_ne!(held[0].clock.map(|clock| clock.at), first);
    }

    #[test]
    fn buttons_that_take_text_are_left_out() {
        let mut held = Vec::new();
        follow(&mut held, vec![timer("a", 1)]);
        let labels: Vec<_> = held[0].actions.iter().map(|a| &*a.label).collect();
        assert_eq!(labels, ["Pause"]);
        assert_eq!(held[0].headline(), "4m");
    }

    #[test]
    fn progress_without_an_end_has_no_fraction() {
        let mut held = Vec::new();
        let mut live = timer("a", 1);
        live.progress = Some(LiveProgress {
            value: 3,
            max: 4,
            indeterminate: false,
        });
        follow(&mut held, vec![live.clone()]);
        assert_eq!(held[0].fraction(), Some(0.75));
        for progress in [
            LiveProgress {
                value: 3,
                max: 0,
                indeterminate: false,
            },
            LiveProgress {
                value: 3,
                max: 4,
                indeterminate: true,
            },
        ] {
            live.progress = Some(progress);
            follow(&mut held, vec![live.clone()]);
            assert_eq!(held[0].fraction(), None);
        }
    }
}

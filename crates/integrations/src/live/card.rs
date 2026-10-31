use std::{
    collections::VecDeque,
    sync::Arc,
    time::{Duration, Instant},
};

use crate::live::wire::{Card, CardClock, Outcome};

use crate::{
    live::{
        Accent, Activity, Caption, Control, Feed, Glyph, LiveSource, Origin, Progress, Signal,
        Steer, Urgency,
    },
    util::{deadline::sleep_until, epoch, proc},
};

/// How long a finished card stays to say how it went.
const LINGER: Duration = Duration::from_secs(8);
const PRUNE_EVERY: Duration = Duration::from_secs(5);
/// Keys ended recently, so a post that lost the race with its own end is not
/// shown forever.
const REMEMBERED_ENDS: usize = 32;

pub enum CardInput {
    Post(Card),
    End(String, Outcome),
}

#[derive(Default)]
pub struct CardSource {
    cards: Vec<Held>,
    ended: VecDeque<Arc<str>>,
}

struct Held {
    key: Arc<str>,
    card: Card,
    caption: Caption,
    reveal_at: Instant,
    expires: Option<Instant>,
    finished: Option<(Outcome, Instant)>,
}

impl Held {
    fn new(card: Card, now: Instant) -> Self {
        let caption = match card.clock {
            Some(CardClock::Since(millis)) => Caption::Since(epoch::instant_at(millis)),
            Some(CardClock::Until(millis)) => Caption::Until(epoch::instant_at(millis)),
            None => Caption::None,
        };
        Self {
            key: Arc::from(card.key.as_str()),
            reveal_at: now + Duration::from_millis(card.reveal_after_ms.into()),
            expires: card
                .ttl_ms
                .map(|ttl| now + Duration::from_millis(ttl.into())),
            caption,
            card,
            finished: None,
        }
    }

    fn revealed(&self, now: Instant) -> bool {
        self.reveal_at <= now
    }

    fn deadline(&self, now: Instant) -> Option<Instant> {
        let reveal = (!self.revealed(now)).then_some(self.reveal_at);
        let linger = self.finished.map(|(_, at)| at + LINGER);
        [reveal, linger, self.expires].into_iter().flatten().min()
    }

    fn done(&self, now: Instant) -> bool {
        self.expires.is_some_and(|at| at <= now)
            || self.finished.is_some_and(|(_, at)| at + LINGER <= now)
            || (self.finished.is_none() && self.card.pid.is_some_and(|pid| !proc::alive(pid)))
    }

    fn activity(&self, feed: &Feed<CardInput>, now: Instant) -> Activity {
        let card = &self.card;
        let base = Activity::new(
            feed.key(self.key.clone()),
            card.glyph,
            card.headline.as_str(),
        )
        .headline(card.headline.as_str())
        .accent(card.accent)
        .urgency(card.urgency)
        .controls([Control::Dismiss]);
        let base = match &card.detail {
            Some(detail) => base.detail("Detail", detail.as_str()),
            None => base,
        };
        let elapsed = match self.caption {
            Caption::Since(start) => Caption::Frozen(now.saturating_duration_since(start)),
            _ => Caption::None,
        };
        match self.finished {
            None => base
                .caption(match (&self.caption, card.progress) {
                    (Caption::None, Some(progress)) => {
                        Caption::Text(Arc::from(format!("{:.0}%", progress * 100.0)))
                    }
                    (caption, _) => caption.clone(),
                })
                .progress(card.progress.map(Progress::Fixed)),
            Some((Outcome::Failed(code), _)) => base
                .glyph(Glyph::Cross)
                .accent(Accent::Red)
                .caption(elapsed)
                .detail("Exit status", code.to_string()),
            Some(_) => base
                .glyph(Glyph::Check)
                .accent(Accent::Green)
                .urgency(Urgency::Ambient)
                .caption(elapsed),
        }
    }
}

impl CardSource {
    fn post(&mut self, card: Card, now: Instant) {
        if self.ended.iter().any(|key| **key == *card.key) {
            return;
        }
        match self.cards.iter_mut().find(|held| *held.key == *card.key) {
            Some(held) => {
                let revealed = held.revealed(now);
                *held = Held::new(card, now);
                if revealed {
                    held.reveal_at = now;
                }
            }
            None => self.cards.push(Held::new(card, now)),
        }
    }

    /// Returns the key to retire at once, if the card never showed.
    fn end(&mut self, key: &str, outcome: Outcome, now: Instant) -> Option<Arc<str>> {
        if self.ended.len() == REMEMBERED_ENDS {
            self.ended.pop_front();
        }
        self.ended.push_back(Arc::from(key));
        let index = self.cards.iter().position(|held| &*held.key == key)?;
        let held = &mut self.cards[index];
        if held.revealed(now) && outcome != Outcome::Dropped {
            held.finished = Some((outcome, now));
            return None;
        }
        Some(self.cards.remove(index).key)
    }

    fn publish(&mut self, feed: &Feed<CardInput>, now: Instant) {
        self.cards.retain(|held| {
            let done = held.done(now);
            if done {
                feed.retire(held.key.clone());
            }
            !done
        });
        for held in self.cards.iter().filter(|held| held.revealed(now)) {
            feed.show(held.activity(feed, now));
        }
    }

    fn deadline(&self, now: Instant) -> Option<Instant> {
        let watched = self.cards.iter().any(|held| held.card.pid.is_some());
        let prune = watched.then(|| now + PRUNE_EVERY);
        self.cards
            .iter()
            .filter_map(|held| held.deadline(now))
            .chain(prune)
            .min()
    }
}

impl LiveSource for CardSource {
    type Input = CardInput;
    const ORIGIN: Origin = Origin::Card;

    async fn run(mut self, mut feed: Feed<Self::Input>) {
        loop {
            let deadline = self.deadline(Instant::now());
            tokio::select! {
                signal = feed.next() => {
                    let now = Instant::now();
                    match signal {
                        Some(Signal::Input(CardInput::Post(card))) => self.post(card, now),
                        Some(Signal::Input(CardInput::End(key, outcome))) => {
                            if let Some(key) = self.end(&key, outcome, now) {
                                feed.retire(key);
                            }
                        }
                        Some(Signal::Steer(Steer::Control(key, Control::Dismiss))) => {
                            if let Some(key) = self.end(&key, Outcome::Dropped, now) {
                                feed.retire(key);
                            }
                        }
                        Some(Signal::Steer(_)) => continue,
                        None => return,
                    }
                }
                _ = sleep_until(deadline) => {}
            }
            self.publish(&feed, Instant::now());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(key: &str, reveal_after_ms: u32) -> Card {
        Card {
            key: key.into(),
            headline: "cargo build".into(),
            reveal_after_ms,
            ..Card::default()
        }
    }

    #[test]
    fn a_quick_command_never_shows_and_a_late_post_stays_ended() {
        let now = Instant::now();
        let mut source = CardSource::default();
        source.post(card("fish-1", 10_000), now);
        assert_eq!(
            source.end("fish-1", Outcome::Done, now).as_deref(),
            Some("fish-1")
        );
        assert!(source.cards.is_empty());

        assert_eq!(source.end("fish-2", Outcome::Done, now), None);
        source.post(card("fish-2", 10_000), now);
        assert!(source.cards.is_empty());
    }

    #[test]
    fn a_revealed_card_lingers_with_its_outcome_after_its_process_exits() {
        let now = Instant::now();
        let mut source = CardSource::default();
        source.post(
            Card {
                pid: Some(u32::MAX),
                ..card("run-1", 0)
            },
            now,
        );
        assert_eq!(source.end("run-1", Outcome::Failed(2), now), None);
        assert!(!source.cards[0].done(now));
        assert!(source.cards[0].done(now + LINGER));
    }
}

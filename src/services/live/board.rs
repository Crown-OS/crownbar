use std::sync::Arc;

use crate::services::live::activity::{Activity, ActivityKey};

/// How many chips the bar shows beside the clock at most.
pub const MAX_CHIPS: usize = 4;

/// Every live activity, and which of them hold a chip.
///
/// Seating is sticky: an activity keeps its chip until it retires, so chips
/// never trade places under the pointer. When every chip is taken, a more
/// urgent arrival unseats the least urgent, newest one, which waits in line.
#[derive(Clone, Debug, Default)]
pub struct Board {
    /// In order of arrival.
    pub activities: Vec<Arc<Activity>>,
    pub chips: [Option<Arc<Activity>>; MAX_CHIPS],
}

impl Board {
    /// Returns whether that changed anything: a source re-stating what it
    /// already showed costs the bar no wake.
    pub fn show(&mut self, activity: Activity) -> bool {
        match self
            .activities
            .iter_mut()
            .find(|held| held.key == activity.key)
        {
            Some(held) if **held == activity => false,
            Some(held) => {
                *held = Arc::new(activity);
                true
            }
            None => {
                self.activities.push(Arc::new(activity));
                true
            }
        }
    }

    pub fn retire(&mut self, key: &ActivityKey) -> bool {
        let before = self.activities.len();
        self.activities.retain(|held| held.key != *key);
        before != self.activities.len()
    }

    fn position(&self, key: &ActivityKey) -> Option<usize> {
        self.activities.iter().position(|held| held.key == *key)
    }

    /// Bring the chips in line with the activities.
    pub fn seat(&mut self) {
        for chip in &mut self.chips {
            *chip = chip.take().and_then(|seated| {
                self.activities
                    .iter()
                    .find(|held| held.key == seated.key)
                    .cloned()
            });
        }

        let mut waiting: Vec<_> = self
            .activities
            .iter()
            .filter(|held| {
                !self
                    .chips
                    .iter()
                    .flatten()
                    .any(|seated| seated.key == held.key)
            })
            .cloned()
            .collect();
        let mut free = self.chips.iter_mut().filter(|chip| chip.is_none());
        waiting.retain(|arrival| match free.next() {
            Some(chip) => {
                *chip = Some(arrival.clone());
                false
            }
            None => true,
        });
        waiting.sort_by_key(|held| std::cmp::Reverse(held.urgency));

        for arrival in waiting {
            let weakest = self
                .chips
                .iter()
                .enumerate()
                .filter_map(|(index, chip)| Some((index, chip.as_ref()?)))
                .min_by_key(|(_, seated)| {
                    (
                        seated.urgency,
                        std::cmp::Reverse(self.position(&seated.key)),
                    )
                })
                .map(|(index, seated)| (index, seated.urgency));
            if let Some((index, urgency)) = weakest
                && arrival.urgency > urgency
            {
                self.chips[index] = Some(arrival);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::live::activity::{Glyph, Origin, Urgency};

    fn activity(id: &str, urgency: Urgency) -> Activity {
        let key = ActivityKey {
            origin: Origin::Card,
            id: Arc::from(id),
        };
        Activity::new(key, Glyph::Sparkles, id).urgency(urgency)
    }

    fn seated(board: &Board) -> Vec<Option<&str>> {
        board
            .chips
            .iter()
            .map(|chip| chip.as_ref().map(|held| &*held.key.id))
            .collect()
    }

    #[test]
    fn a_chip_keeps_its_place_when_one_before_it_retires() {
        let mut board = Board::default();
        for id in ["a", "b", "c"] {
            board.show(activity(id, Urgency::Active));
        }
        board.seat();
        board.retire(&board.activities[0].key.clone());
        board.seat();
        assert_eq!(seated(&board), [None, Some("b"), Some("c"), None]);
        board.show(activity("d", Urgency::Active));
        board.seat();
        assert_eq!(seated(&board), [Some("d"), Some("b"), Some("c"), None]);
    }

    #[test]
    fn an_urgent_arrival_unseats_the_newest_least_urgent_chip() {
        let mut board = Board::default();
        for (id, urgency) in [
            ("a", Urgency::Ambient),
            ("b", Urgency::Active),
            ("c", Urgency::Ambient),
            ("d", Urgency::Active),
        ] {
            board.show(activity(id, urgency));
        }
        board.seat();
        board.show(activity("e", Urgency::Ambient));
        board.seat();
        assert_eq!(seated(&board), [Some("a"), Some("b"), Some("c"), Some("d")]);

        board.show(activity("alarm", Urgency::Attention));
        board.seat();
        assert_eq!(
            seated(&board),
            [Some("a"), Some("b"), Some("alarm"), Some("d")]
        );

        board.retire(&board.activities[0].key.clone());
        board.seat();
        assert_eq!(
            seated(&board),
            [Some("c"), Some("b"), Some("alarm"), Some("d")]
        );
    }

    #[test]
    fn an_update_replaces_the_seated_copy() {
        let mut board = Board::default();
        board.show(activity("a", Urgency::Active).headline("one"));
        board.seat();
        board.show(activity("a", Urgency::Active).headline("two"));
        board.seat();
        assert_eq!(board.activities.len(), 1);
        assert_eq!(
            board.chips[0].as_ref().map(|held| &*held.headline),
            Some("two")
        );
    }
}

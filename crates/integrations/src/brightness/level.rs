//! The level of one output while a slider drag is on its way to the
//! compositor.

#[derive(Debug, Default)]
pub struct OutputLevel {
    reported: Option<u32>,
    sent: Option<u32>,
    waiting: Option<u32>,
}

impl OutputLevel {
    /// The compositor's level event.
    pub fn report(&mut self, level: u32) {
        self.reported = Some(level);
    }

    /// The bar wants `level`; it replaces a level that waits to be sent.
    pub fn ask(&mut self, level: u32) {
        self.waiting = Some(level);
    }

    /// The level to send in the round trip that starts now, if one waits.
    pub fn send(&mut self) -> Option<u32> {
        let level = self.waiting.take()?;
        self.sent = Some(level);
        Some(level)
    }

    /// The round trip that carried the sent level ended, so the compositor's
    /// answer to it has arrived. Returns whether the shown level can change.
    pub fn round_trip_ended(&mut self) -> bool {
        self.sent.take().is_some()
    }

    /// What the slider shows: the newest level asked for until the compositor
    /// has answered it, then the level the compositor reports.
    pub fn shown(&self) -> Option<u32> {
        self.waiting.or(self.sent).or(self.reported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drag_sends_only_the_newest_level_of_each_round_trip() {
        let mut level = OutputLevel::default();
        level.report(5000);
        level.ask(5200);
        assert_eq!(level.send(), Some(5200));
        level.ask(5400);
        level.ask(5600);
        assert_eq!(level.send(), Some(5600));
        assert_eq!(level.send(), None);
    }

    #[test]
    fn an_older_echo_does_not_pull_the_slider_back() {
        let mut level = OutputLevel::default();
        level.report(5000);
        level.ask(6000);
        level.send();
        level.ask(7000);
        level.report(6000);
        assert_eq!(level.shown(), Some(7000));
    }

    #[test]
    fn the_clamped_answer_shows_once_the_round_trip_ends() {
        let mut level = OutputLevel::default();
        level.report(5000);
        level.ask(0);
        level.send();
        level.report(500);
        assert_eq!(level.shown(), Some(0));
        assert!(level.round_trip_ended());
        assert_eq!(level.shown(), Some(500));
    }
}

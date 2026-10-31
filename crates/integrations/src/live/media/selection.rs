/// Where an MPRIS player's playback stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PlaybackState {
    Playing,
    Paused,
    Stopped,
}

impl PlaybackState {
    pub(super) fn parse(status: &str) -> Self {
        match status {
            "Playing" => Self::Playing,
            "Paused" => Self::Paused,
            _ => Self::Stopped,
        }
    }
}

/// The player the chip follows: one that is playing, keeping the current
/// choice while it qualifies, and falling back to a paused one.
pub(super) fn choose_active<'p>(
    players: &[(&'p str, PlaybackState)],
    current: Option<&str>,
) -> Option<&'p str> {
    let best_with = |state: PlaybackState| {
        players
            .iter()
            .find(|(name, held)| *held == state && Some(*name) == current)
            .or_else(|| players.iter().find(|(_, held)| *held == state))
            .map(|(name, _)| *name)
    };
    best_with(PlaybackState::Playing).or_else(|| best_with(PlaybackState::Paused))
}

/// The `SetPosition` target for `fraction` of a track, clamped to the track.
pub(super) fn seek_target_us(length_us: u64, fraction: f32) -> i64 {
    let target = length_us as f64 * f64::from(fraction.clamp(0.0, 1.0));
    i64::try_from(target as u64).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::PlaybackState::{Paused, Playing, Stopped};
    use super::*;

    #[test]
    fn a_playing_player_beats_a_paused_one_and_the_choice_sticks() {
        assert_eq!(
            choose_active(&[("a", Paused), ("b", Playing)], Some("a")),
            Some("b")
        );
        assert_eq!(
            choose_active(&[("a", Playing), ("b", Playing)], Some("b")),
            Some("b")
        );
        assert_eq!(
            choose_active(&[("a", Stopped), ("b", Paused)], None),
            Some("b")
        );
        assert_eq!(choose_active(&[("a", Stopped)], Some("a")), None);
    }

    #[test]
    fn seeking_scales_the_track_and_clamps() {
        assert_eq!(seek_target_us(200_000_000, 0.25), 50_000_000);
        assert_eq!(seek_target_us(200_000_000, 2.5), 200_000_000);
    }
}

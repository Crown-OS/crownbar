//! Live activities as the bar's sources would seat them: a track, a timer, a
//! coding agent and a call.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use crownbar_integrations::live::{
    Accent, Activity, ActivityKey, Board, Caption, Control, Glyph, LiveState, Origin, Progress,
    Urgency,
};

fn key(origin: Origin, id: &str) -> ActivityKey {
    ActivityKey {
        origin,
        id: Arc::from(id),
    }
}

fn now_playing(now: Instant) -> Activity {
    Activity::new(key(Origin::Media, "media"), Glyph::Music, "Spotify")
        .headline("Midnight City · M83")
        .accent(Accent::Green)
        .progress(Some(Progress::Filling {
            zero: now - Duration::from_secs(84),
            span: Duration::from_secs(243),
        }))
        .controls([Control::Previous, Control::PlayPause, Control::Next])
        .detail("Track", "Midnight City")
        .detail("Artist", "M83")
        .detail("Album", "Hurry Up, We're Dreaming")
        .seekable()
}

fn timer(now: Instant) -> Activity {
    let until = now + Duration::from_secs(7 * 60 + 42);
    Activity::new(key(Origin::Timer, "tea"), Glyph::Timer, "Tea")
        .accent(Accent::Amber)
        .caption(Caption::Until(until))
        .progress(Some(Progress::Draining {
            until,
            span: Duration::from_secs(10 * 60),
        }))
        .controls([Control::Pause, Control::Extend, Control::Cancel])
}

fn agent(now: Instant) -> Activity {
    Activity::new(key(Origin::Agent, "claude"), Glyph::Sparkles, "Claude Code")
        .accent(Accent::Coral)
        .caption(Caption::Since(now - Duration::from_secs(84)))
        .detail("Agent", "Claude Code")
        .detail("Project", "crownbar")
        .controls([Control::Dismiss])
        .headline("Editing bar.rs")
}

fn waiting_agent(now: Instant) -> Activity {
    Activity::new(key(Origin::Agent, "codex"), Glyph::Sparkles, "Codex")
        .accent(Accent::Violet)
        .caption(Caption::Since(now - Duration::from_secs(5 * 60 + 12)))
        .detail("Agent", "Codex")
        .detail("Project", "crownos-config")
        .controls([Control::Dismiss])
        .headline("Needs input")
        .glyph(Glyph::Hand)
        .accent(Accent::Amber)
        .urgency(Urgency::Attention)
        .detail("Request", "Allow running cargo test?")
}

fn call(now: Instant) -> Activity {
    Activity::new(key(Origin::Phone, "pixel/call"), Glyph::Call, "Phone Call")
        .headline("Mum")
        .accent(Accent::Green)
        .urgency(Urgency::Attention)
        .detail("Device", "Pixel 8")
        .controls([Control::HangUp])
        .caption(Caption::Since(now - Duration::from_secs(3 * 60 + 5)))
        .detail("Number", "+351 912 345 678")
}

fn stopwatch() -> Activity {
    Activity::new(
        key(Origin::Stopwatch, "stopwatch"),
        Glyph::Stopwatch,
        "Stopwatch",
    )
    .caption(Caption::Frozen(Duration::from_secs(12 * 60 + 3)))
    .accent(Accent::Blue)
    .controls([Control::Resume, Control::Lap, Control::Reset])
    .detail("Lap 2", "5:41")
    .detail("Lap 1", "6:22")
}

fn board(activities: impl IntoIterator<Item = Activity>) -> LiveState {
    let mut board = Board::default();
    for activity in activities {
        board.show(activity);
    }
    board.seat();
    LiveState { board }
}

/// Four chips: what the bar seats at most.
pub fn busy() -> LiveState {
    let now = Instant::now();
    board([now_playing(now), timer(now), agent(now), call(now)])
}

/// The activities that only fit on a bar of their own.
pub fn more() -> LiveState {
    board([stopwatch(), waiting_agent(Instant::now())])
}

use std::{
    fmt::Write,
    sync::Arc,
    time::{Duration, Instant},
};

pub use crate::live::wire::{Accent, Control, Glyph, Urgency};

use crate::util::text::ellipsize;

/// The most a chip says ahead of its caption.
pub const HEADLINE_CHARS: usize = 28;

/// Which source an activity came from, and so which one hears its controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Origin {
    Agent,
    Stopwatch,
    Timer,
    Media,
    Card,
    Capture,
    Screencast,
    Caffeine,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ActivityKey {
    pub origin: Origin,
    pub id: Arc<str>,
}

/// The running part of a chip's text, which the bar keeps current by itself
/// so a source never has to re-publish to move a clock on.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Caption {
    #[default]
    None,
    Text(Arc<str>),
    Since(Instant),
    Until(Instant),
    Frozen(Duration),
}

impl Caption {
    pub fn ticks(&self) -> bool {
        matches!(self, Self::Since(_) | Self::Until(_))
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, Self::None)
    }

    /// Append the caption as of `now` to `out`.
    pub fn write_into(&self, out: &mut String, now: Instant) {
        match self {
            Self::None => {}
            Self::Text(text) => out.push_str(text),
            Self::Since(start) => clock(out, now.saturating_duration_since(*start)),
            Self::Until(end) => clock(out, end.saturating_duration_since(now)),
            Self::Frozen(elapsed) => clock(out, *elapsed),
        }
    }
}

/// `0:42`, `12:03`, `1:04:09`.
pub fn clock(out: &mut String, elapsed: Duration) {
    let seconds = elapsed.as_secs();
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    let _ = match hours {
        0 => write!(out, "{minutes}:{seconds:02}"),
        _ => write!(out, "{hours}:{minutes:02}:{seconds:02}"),
    };
}

/// How far along something is, as a ring around the glyph.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Progress {
    Fixed(f32),
    /// Filling from `zero` over `span`: a track playing.
    Filling {
        zero: Instant,
        span: Duration,
    },
    /// Emptying until `until`, from full `span` before it: a timer.
    Draining {
        until: Instant,
        span: Duration,
    },
}

impl Progress {
    pub fn moves(self) -> bool {
        !matches!(self, Self::Fixed(_))
    }

    pub fn at(self, now: Instant) -> f32 {
        let fraction = |part: Duration, span: Duration| match span.as_secs_f32() {
            span if span > 0.0 => part.as_secs_f32() / span,
            _ => 0.0,
        };
        match self {
            Self::Fixed(fraction) => fraction,
            Self::Filling { zero, span } => fraction(now.saturating_duration_since(zero), span),
            Self::Draining { until, span } => fraction(until.saturating_duration_since(now), span),
        }
        .clamp(0.0, 1.0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Detail {
    pub label: Arc<str>,
    pub value: Arc<str>,
}

/// One thing happening right now, as a chip and the panel behind it.
#[derive(Clone, Debug, PartialEq)]
pub struct Activity {
    pub key: ActivityKey,
    /// The panel's title.
    pub title: Arc<str>,
    /// The chip's words ahead of its caption. Empty for a chip that is only a
    /// clock.
    pub headline: Arc<str>,
    pub caption: Caption,
    pub glyph: Glyph,
    pub accent: Accent,
    pub urgency: Urgency,
    pub progress: Option<Progress>,
    pub details: Vec<Detail>,
    pub controls: Vec<Control>,
    /// The progress ring can be dragged: a track's position.
    pub seekable: bool,
}

impl Activity {
    pub fn new(key: ActivityKey, glyph: Glyph, title: impl Into<Arc<str>>) -> Self {
        Self {
            key,
            title: title.into(),
            headline: Arc::from(""),
            caption: Caption::None,
            glyph,
            accent: Accent::Neutral,
            urgency: Urgency::Active,
            progress: None,
            details: Vec::new(),
            controls: Vec::new(),
            seekable: false,
        }
    }

    /// Cut to [`HEADLINE_CHARS`], so no chip crowds the bar.
    pub fn headline(mut self, headline: impl AsRef<str>) -> Self {
        self.headline = Arc::from(ellipsize(headline.as_ref(), HEADLINE_CHARS));
        self
    }

    pub fn glyph(mut self, glyph: Glyph) -> Self {
        self.glyph = glyph;
        self
    }

    pub fn caption(mut self, caption: Caption) -> Self {
        self.caption = caption;
        self
    }

    pub fn accent(mut self, accent: Accent) -> Self {
        self.accent = accent;
        self
    }

    pub fn urgency(mut self, urgency: Urgency) -> Self {
        self.urgency = urgency;
        self
    }

    pub fn progress(mut self, progress: Option<Progress>) -> Self {
        self.progress = progress;
        self
    }

    pub fn detail(mut self, label: impl Into<Arc<str>>, value: impl Into<Arc<str>>) -> Self {
        self.details.push(Detail {
            label: label.into(),
            value: value.into(),
        });
        self
    }

    pub fn controls(mut self, controls: impl IntoIterator<Item = Control>) -> Self {
        self.controls.extend(controls);
        self
    }

    pub fn seekable(mut self) -> Self {
        self.seekable = true;
        self
    }

    /// The chip's text as of `now`, written over `out`.
    pub fn write_label(&self, out: &mut String, now: Instant) {
        out.clear();
        out.push_str(&self.headline);
        if !self.headline.is_empty() && !self.caption.is_empty() {
            out.push_str(" · ");
        }
        self.caption.write_into(out, now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label(activity: &Activity, now: Instant) -> String {
        let mut out = String::new();
        activity.write_label(&mut out, now);
        out
    }

    #[test]
    fn clocks_read_like_a_stopwatch() {
        let mut out = String::new();
        for (seconds, expected) in [(42, "0:42"), (723, "12:03"), (3849, "1:04:09")] {
            out.clear();
            clock(&mut out, Duration::from_secs(seconds));
            assert_eq!(out, expected);
        }
    }

    #[test]
    fn a_chip_joins_its_headline_and_its_running_caption() {
        let start = Instant::now();
        let key = ActivityKey {
            origin: Origin::Agent,
            id: Arc::from("s"),
        };
        let agent = Activity::new(key.clone(), Glyph::Sparkles, "Claude")
            .headline("Editing bar.rs")
            .caption(Caption::Since(start));
        assert_eq!(
            label(&agent, start + Duration::from_secs(84)),
            "Editing bar.rs · 1:24"
        );
        let stopwatch = Activity::new(key, Glyph::Stopwatch, "Stopwatch")
            .caption(Caption::Frozen(Duration::from_secs(5)));
        assert_eq!(label(&stopwatch, start), "0:05");
    }

    #[test]
    fn a_draining_ring_empties_toward_its_deadline() {
        let now = Instant::now();
        let ring = Progress::Draining {
            until: now + Duration::from_secs(15),
            span: Duration::from_secs(60),
        };
        assert!((ring.at(now) - 0.25).abs() < 1e-3);
        assert_eq!(ring.at(now + Duration::from_secs(99)), 0.0);
    }
}

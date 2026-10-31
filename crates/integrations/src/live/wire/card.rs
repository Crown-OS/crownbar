use serde::{Deserialize, Serialize};

use crate::live::wire::{Accent, Glyph, Urgency};

/// A chip anything can post: a download, a build, a deploy.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Card {
    pub key: String,
    pub headline: String,
    pub detail: Option<String>,
    pub glyph: Glyph,
    pub accent: Accent,
    pub urgency: Urgency,
    pub clock: Option<CardClock>,
    /// ∈ [0, 1].
    pub progress: Option<f32>,
    /// Stay hidden this long, so a task that ends quickly never shows.
    pub reveal_after_ms: u32,
    /// Retire unprompted after this long.
    pub ttl_ms: Option<u32>,
    /// Retire when this process exits.
    pub pid: Option<u32>,
}

/// A clock the bar keeps running by itself, in milliseconds since the epoch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardClock {
    Since(u64),
    Until(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Done,
    Failed(i32),
    Dropped,
}

impl Outcome {
    pub const fn from_status(status: i32) -> Self {
        match status {
            0 => Self::Done,
            code => Self::Failed(code),
        }
    }
}

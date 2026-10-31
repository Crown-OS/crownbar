use serde::{Deserialize, Serialize};

/// The mark drawn at the head of a chip.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum Glyph {
    #[default]
    Sparkles,
    Stopwatch,
    Timer,
    Music,
    Terminal,
    Download,
    Check,
    Cross,
    Alert,
    Hand,
    Microphone,
    Camera,
    Screen,
    Coffee,
}

/// The hue a chip is tinted with. The bar resolves it against its palette.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum Accent {
    #[default]
    Neutral,
    Coral,
    Blue,
    Green,
    Amber,
    Red,
    Violet,
}

/// How much a chip asks for attention. Ordered, so the most urgent wins.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum Urgency {
    Ambient,
    #[default]
    Active,
    Attention,
}

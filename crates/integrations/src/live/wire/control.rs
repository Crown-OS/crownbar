use serde::{Deserialize, Serialize};

/// Something a chip's panel offers to do to what the chip shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Control {
    Pause,
    Resume,
    Reset,
    Lap,
    Cancel,
    Extend,
    PlayPause,
    Next,
    Previous,
    MuteMic,
    Stop,
    Dismiss,
}

impl Control {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pause => "Pause",
            Self::Resume => "Resume",
            Self::Reset => "Reset",
            Self::Lap => "Lap",
            Self::Cancel => "Cancel",
            Self::Extend => "Add 1 Minute",
            Self::PlayPause => "Play / Pause",
            Self::Next => "Next",
            Self::Previous => "Previous",
            Self::MuteMic => "Mute Microphone",
            Self::Stop => "Stop",
            Self::Dismiss => "Dismiss",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum StopwatchRequest {
    Start,
    Pause,
    Resume,
    Toggle,
    Lap,
    Reset,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimerRequest {
    Start { seconds: u32, label: Option<String> },
    CancelAll,
}

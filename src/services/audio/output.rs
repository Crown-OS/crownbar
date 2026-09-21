//! The output side of [`super::AudioState`].

use crate::services::audio::{AudioState, Device, Stream, Volume};

impl AudioState {
    pub fn default_output(&self) -> Option<&Device> {
        self.outputs.iter().find(|d| d.default)
    }

    /// What the bar pill and its slider draw. A machine with no default sink
    /// gets a silent one rather than making every caller unwrap.
    pub fn output_volume(&self) -> Volume {
        self.default_output().map(|d| d.volume).unwrap_or_default()
    }

    /// Applications playing right now, newest first.
    pub fn playing(&self) -> &[Stream] {
        &self.playback
    }
}

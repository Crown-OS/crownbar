//! The input side of [`super::AudioState`].
//!
//! There is no level meter: pipewire-native 0.1.4 has no stream API, so audio
//! buffers cannot be received at all. What the bar can answer — and what
//! people actually ask of it — is which applications are holding the
//! microphone, which the registry reports for free.

use crate::services::audio::{AudioState, Device, Volume};

impl AudioState {
    pub fn default_input(&self) -> Option<&Device> {
        self.inputs.iter().find(|d| d.default)
    }

    pub fn input_volume(&self) -> Volume {
        self.default_input().map(|d| d.volume).unwrap_or_default()
    }

    pub fn microphone_in_use(&self) -> bool {
        self.recording.iter().any(|stream| stream.active)
    }

    /// Names of the applications recording, deduplicated in first-seen order.
    pub fn microphone_users(&self) -> Vec<&str> {
        let mut users: Vec<&str> = Vec::with_capacity(self.recording.len());
        for stream in self.recording.iter().filter(|s| s.active) {
            let app = stream.app.as_str();
            if !users.contains(&app) {
                users.push(app);
            }
        }
        users
    }
}

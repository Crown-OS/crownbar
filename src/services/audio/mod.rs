//! Audio devices, volume and streams, over PipeWire.
//!
//! Fully event-driven: the registry announces devices and streams as they
//! appear, and each node pushes its own `Props` whenever its volume or mute
//! changes. Nothing here polls, and the volume slider is live rather than
//! committing on release — a `set_param` costs a socket write, not a
//! subprocess.

mod backend;
mod device;
pub mod input;
pub mod output;
mod props;
mod route;

use std::sync::Arc;

pub use device::{Device, DeviceKind, NodeId, Role, Stream, Volume, MAX_LEVEL};

use crate::services::{bus::Backend, status::Availability};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AudioState {
    pub availability: Availability,
    /// Default first, then by route priority, then description.
    pub outputs: Vec<Device>,
    pub inputs: Vec<Device>,
    /// Applications currently playing.
    pub playback: Vec<Stream>,
    /// Applications currently holding the microphone.
    pub recording: Vec<Stream>,
    /// `node.name` of the session default, as the server resolved it.
    default_output: Option<String>,
    default_input: Option<String>,
}

#[derive(Clone, Debug)]
pub enum AudioCommand {
    /// Perceptual level ∈ [0, [`MAX_LEVEL`]].
    SetOutputVolume(f32),
    SetOutputMuted(bool),
    SetInputVolume(f32),
    SetInputMuted(bool),
    /// Any device or stream by id, for a full mixer panel.
    SetNodeVolume { id: NodeId, level: f32 },
    SetNodeMuted { id: NodeId, muted: bool },
    /// By `node.name`: global ids are not stable across a server restart, and
    /// the name is what the session manager persists.
    SetDefaultOutput(String),
    SetDefaultInput(String),
}

pub type Channel = crate::services::bus::Channel<AudioState, AudioCommand>;

/// Bridge the command pipe to the PipeWire thread.
///
/// PipeWire's loop is not async and its lock blocks until the current
/// iteration finishes, so it gets a thread of its own rather than a runtime
/// worker. This task exists only to carry commands across.
pub async fn run(backend: Backend<AudioState, AudioCommand>) {
    let Backend {
        publish,
        mut commands,
    } = backend;
    let (tx, rx) = std::sync::mpsc::channel();

    if let Err(e) = std::thread::Builder::new()
        .name("crownbar-pw".into())
        .spawn(move || backend::run(publish, rx))
    {
        log::warn!("could not start the pipewire thread: {e}");
        return;
    }

    while let Some(command) = commands.recv().await {
        if tx.send(command).is_err() {
            return;
        }
    }
}

impl AudioState {
    /// Fold the default-device names into the per-device flags and put both
    /// lists in the order a panel draws them.
    pub(crate) fn resolve(&mut self) {
        for (devices, default) in [
            (&mut self.outputs, &self.default_output),
            (&mut self.inputs, &self.default_input),
        ] {
            for device in devices.iter_mut() {
                device.default = default.as_deref() == Some(device.name.as_str());
            }
            devices.sort_by(|a, b| {
                b.default
                    .cmp(&a.default)
                    .then(b.priority.cmp(&a.priority))
                    .then_with(|| a.description.cmp(&b.description))
            });
        }
    }

    pub(crate) fn set_default(&mut self, role: Role, name: Option<String>) -> bool {
        let slot = match role {
            Role::Output => &mut self.default_output,
            _ => &mut self.default_input,
        };
        if *slot == name {
            return false;
        }
        *slot = name;
        self.resolve();
        true
    }

    /// Every device and stream, for a command that addresses one by id.
    pub(crate) fn nodes_mut(&mut self) -> impl Iterator<Item = (NodeId, &mut Volume)> {
        let devices = self
            .outputs
            .iter_mut()
            .chain(self.inputs.iter_mut())
            .map(|d| (d.id, &mut d.volume));
        let streams = self
            .playback
            .iter_mut()
            .chain(self.recording.iter_mut())
            .map(|s| (s.id, &mut s.volume));
        devices.chain(streams)
    }

    /// Apply a node's full property set, and for a stream whether it is
    /// actually moving audio.
    pub(crate) fn describe(
        &mut self,
        id: NodeId,
        properties: Option<&pipewire_native::properties::Properties>,
        active: bool,
    ) -> bool {
        let outputs = self.outputs.len();
        if let Some((index, device)) = self
            .outputs
            .iter_mut()
            .chain(self.inputs.iter_mut())
            .enumerate()
            .find(|(_, d)| d.id == id)
        {
            let role = if index < outputs {
                Role::Output
            } else {
                Role::Input
            };
            let changed = properties
                .map(|properties| device.describe(properties, role))
                .unwrap_or(false);
            if changed {
                self.resolve();
            }
            return changed;
        }
        self.playback
            .iter_mut()
            .chain(self.recording.iter_mut())
            .find(|s| s.id == id)
            .map(|stream| {
                let mut changed = properties
                    .map(|properties| stream.describe(properties))
                    .unwrap_or(false);
                changed |= stream.active != active;
                stream.active = active;
                changed
            })
            .unwrap_or(false)
    }

    pub(crate) fn remove(&mut self, id: NodeId) -> bool {
        let before = self.outputs.len() + self.inputs.len() + self.playback.len() + self.recording.len();
        self.outputs.retain(|d| d.id != id);
        self.inputs.retain(|d| d.id != id);
        self.playback.retain(|s| s.id != id);
        self.recording.retain(|s| s.id != id);
        before != self.outputs.len() + self.inputs.len() + self.playback.len() + self.recording.len()
    }

    pub(crate) fn unavailable(reason: impl Into<Arc<str>>) -> Self {
        Self {
            availability: Availability::Unavailable(reason.into()),
            ..Default::default()
        }
    }
}

//! `crownos_workspace_mode_manager_v1` on a connection of the service's own.

use std::io;

use crownos_protocols::workspace_mode::v1::client::crownos_workspace_mode_manager_v1::{
    self, CrownosWorkspaceModeManagerV1, Mode,
};
use wayland_client::{
    ConnectError, Connection, Dispatch, QueueHandle, WEnum,
    backend::WaylandError,
    globals::{BindError, GlobalError, GlobalListContents, registry_queue_init},
    protocol::wl_registry::{self, WlRegistry},
};

use super::WorkspaceMode;
use crate::util::wayland::{Pump, PumpError};

#[derive(Debug, thiserror::Error)]
pub enum ModeError {
    #[error("no Wayland compositor to talk to: {0}")]
    Connect(#[from] ConnectError),
    #[error("could not read the compositor's globals: {0}")]
    Globals(#[from] GlobalError),
    #[error("the compositor does not offer workspace modes: {0}")]
    Unsupported(#[from] BindError),
    #[error("Wayland connection failed: {0}")]
    Wayland(#[from] WaylandError),
    #[error(transparent)]
    Pump(#[from] PumpError),
    #[error(transparent)]
    Io(#[from] io::Error),
}

pub struct ModeConnection {
    pump: Pump<Heard>,
    manager: CrownosWorkspaceModeManagerV1,
    heard: Heard,
}

/// The newest mode the compositor reported, until the service takes it.
#[derive(Default)]
struct Heard(Option<WorkspaceMode>);

impl ModeConnection {
    /// Must be called on a tokio runtime with IO enabled.
    pub fn connect() -> Result<Self, ModeError> {
        let connection = Connection::connect_to_env()?;
        let (globals, queue) = registry_queue_init::<Heard>(&connection)?;
        let manager = globals.bind(&queue.handle(), 1..=1, ())?;
        connection.flush()?;
        Ok(Self {
            pump: Pump::new(connection, queue)?,
            manager,
            heard: Heard::default(),
        })
    }

    pub fn request(&self, mode: WorkspaceMode) {
        self.manager.set_mode(match mode {
            WorkspaceMode::Tiling => Mode::Tiling,
            WorkspaceMode::Floating => Mode::Floating,
        });
        if let Err(error) = self.pump.flush() {
            log::warn!("could not ask for the workspace mode: {error}");
        }
    }

    /// Waits for the compositor, returning the mode it reported if it did.
    pub async fn dispatch(&mut self) -> Result<Option<WorkspaceMode>, ModeError> {
        self.pump.dispatch(&mut self.heard).await?;
        Ok(self.heard.0.take())
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for Heard {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<CrownosWorkspaceModeManagerV1, ()> for Heard {
    fn event(
        heard: &mut Self,
        _: &CrownosWorkspaceModeManagerV1,
        event: crownos_workspace_mode_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let crownos_workspace_mode_manager_v1::Event::Mode {
            mode: WEnum::Value(mode),
        } = event
        else {
            return;
        };
        heard.0 = match mode {
            Mode::Tiling => Some(WorkspaceMode::Tiling),
            Mode::Floating => Some(WorkspaceMode::Floating),
            _ => return,
        };
    }
}

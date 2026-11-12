//! Whether the focused workspace tiles its windows or lets them float.
//!
//! The compositor owns the answer and says it on `crownos_workspace_mode_v1`,
//! on a connection of the service's own: the mode belongs to a workspace, not
//! to any surface of the bar's. Switching only asks; the mode the snapshot
//! shows is always the one the compositor last reported.

mod connection;

use std::sync::Arc;

use crate::{bus::Backend, status::Availability};
use connection::ModeConnection;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WorkspaceMode {
    #[default]
    Tiling,
    Floating,
}

impl WorkspaceMode {
    pub fn toggled(self) -> Self {
        match self {
            Self::Tiling => Self::Floating,
            Self::Floating => Self::Tiling,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceModeState {
    pub availability: Availability,
    pub mode: WorkspaceMode,
}

/// The mode to switch the focused workspace to.
pub type Channel = crate::bus::Channel<WorkspaceModeState, WorkspaceMode>;

pub async fn run(backend: Backend<WorkspaceModeState, WorkspaceMode>) {
    let Backend {
        publish,
        mut commands,
    } = backend;

    let mut link = match ModeConnection::connect() {
        Ok(link) => link,
        Err(error) => {
            log::info!("workspace mode unavailable: {error}");
            return unavailable(&publish, "the compositor does not report workspace modes");
        }
    };

    loop {
        let wanted = tokio::select! {
            command = commands.recv() => match command {
                Some(mode) => mode,
                None => return,
            },
            heard = link.dispatch() => match heard {
                Ok(Some(mode)) => {
                    publish.edit(|state| {
                        let changed = state.availability != Availability::Ready
                            || state.mode != mode;
                        state.availability = Availability::Ready;
                        state.mode = mode;
                        changed
                    });
                    continue;
                }
                Ok(None) => continue,
                Err(error) => {
                    log::warn!("workspace mode lost its compositor connection: {error}");
                    return unavailable(&publish, "lost the compositor connection");
                }
            },
        };
        link.request(wanted);
    }
}

fn unavailable(publish: &crate::bus::Publisher<WorkspaceModeState>, reason: &'static str) {
    publish.edit(|state| {
        state.availability = Availability::Unavailable(Arc::from(reason));
        true
    });
}

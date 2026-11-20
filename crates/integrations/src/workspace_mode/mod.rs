//! Whether a workspace tiles its windows or lets them float.
//!
//! The compositor owns the answer and says it on `crownos_workspace_mode_v1`,
//! on a connection of the service's own: the mode belongs to a workspace, not
//! to any surface of the bar's. Switching only asks; the mode the snapshot
//! shows is always the one the compositor last reported.

mod connection;

use std::{collections::BTreeMap, sync::Arc};

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
    /// The mode of the focused output's active workspace.
    pub focused: WorkspaceMode,
    /// The mode of each output's active workspace, by `wl_output` name. Empty
    /// when the compositor reports only the focused output.
    pub outputs: BTreeMap<String, WorkspaceMode>,
}

impl WorkspaceModeState {
    /// The mode the bar on `output` shows: its own output's, or the focused
    /// output's when the compositor does not report that output.
    pub fn mode_on(&self, output: Option<&str>) -> WorkspaceMode {
        output
            .and_then(|name| self.outputs.get(name))
            .copied()
            .unwrap_or(self.focused)
    }
}

/// Switch the active workspace of the bar's output to `mode`.
#[derive(Clone, Debug)]
pub struct ModeRequest {
    pub output: Option<Arc<str>>,
    pub mode: WorkspaceMode,
}

pub type Channel = crate::bus::Channel<WorkspaceModeState, ModeRequest>;

pub async fn run(backend: Backend<WorkspaceModeState, ModeRequest>) {
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
                Some(request) => request,
                None => return,
            },
            heard = link.dispatch() => match heard {
                Ok(Some(heard)) => {
                    publish.edit(|state| {
                        let changed = state.availability != Availability::Ready
                            || state.focused != heard.focused
                            || state.outputs != heard.outputs;
                        state.availability = Availability::Ready;
                        state.focused = heard.focused;
                        state.outputs = heard.outputs;
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
        link.request(&wanted);
    }
}

fn unavailable(publish: &crate::bus::Publisher<WorkspaceModeState>, reason: &'static str) {
    publish.edit(|state| {
        state.availability = Availability::Unavailable(Arc::from(reason));
        true
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bar_shows_its_own_output_and_falls_back_to_the_focused_one() {
        let state = WorkspaceModeState {
            availability: Availability::Ready,
            focused: WorkspaceMode::Tiling,
            outputs: BTreeMap::from([("HDMI-A-1".to_owned(), WorkspaceMode::Floating)]),
        };
        assert_eq!(state.mode_on(Some("HDMI-A-1")), WorkspaceMode::Floating);
        assert_eq!(state.mode_on(Some("eDP-1")), WorkspaceMode::Tiling);
        assert_eq!(state.mode_on(None), WorkspaceMode::Tiling);
    }
}

//! Tiling/floating switch for the focused workspace.
//!
//! The pill shows the mode the compositor reports, morphing between a loose
//! scatter of windows and a grid, and a click asks for the other one. Nothing
//! moves until the compositor answers, so the icon never claims a mode the
//! workspace is not in.

use std::sync::Arc;

use crate::{BarWidget, Icon, animation::Spring};
use crownbar_integrations::{
    Services,
    workspace_mode::{WorkspaceMode, WorkspaceModeState},
};

pub struct LayoutWidget {
    workspace: Arc<WorkspaceModeState>,
    morph: Spring,
}

impl LayoutWidget {
    pub fn new() -> Self {
        let workspace = Arc::<WorkspaceModeState>::default();
        Self {
            morph: Spring::new(tiled(workspace.mode)),
            workspace,
        }
    }
}

fn tiled(mode: WorkspaceMode) -> f32 {
    match mode {
        WorkspaceMode::Tiling => 1.0,
        WorkspaceMode::Floating => 0.0,
    }
}

impl BarWidget for LayoutWidget {
    fn id(&self) -> &str {
        "layout"
    }

    /// A compositor that cannot report the mode gets no pill, rather than a
    /// switch that silently does nothing.
    fn visible(&self) -> bool {
        self.workspace.availability.usable()
    }

    fn icon(&self) -> Icon {
        Icon::Layout {
            tiled: self.morph.position,
        }
    }

    /// The first mode heard is drawn as it is: the pill appearing should not
    /// play a switch that never happened.
    fn sync(&mut self, services: &Services) -> bool {
        let workspace = services.workspace_mode.read();
        if Arc::ptr_eq(&workspace, &self.workspace) {
            return false;
        }
        let appearing = !self.visible() && workspace.availability.usable();
        self.workspace = workspace;
        let moved = self.morph.set_target(tiled(self.workspace.mode));
        if appearing {
            self.morph.snap_to_target();
        }
        moved || appearing
    }

    fn on_click(&mut self, services: &Services) -> bool {
        services.workspace_mode.send(self.workspace.mode.toggled());
        false
    }

    fn tick_animation(&mut self, dt: f32) -> bool {
        if self.morph.at_rest() {
            return false;
        }
        self.morph.step(dt);
        !self.morph.at_rest()
    }
}

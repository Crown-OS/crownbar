//! `crownos_workspace_mode_manager_v1` on a connection of the service's own.
//!
//! From version 2 the service also makes a `crownos_workspace_mode_output_v1`
//! for each `wl_output`, so each bar can show its own output's mode.

use std::{collections::BTreeMap, io};

use crownos_protocols::workspace_mode::v1::client::{
    crownos_workspace_mode_manager_v1::{self, CrownosWorkspaceModeManagerV1, Mode},
    crownos_workspace_mode_output_v1::{self, CrownosWorkspaceModeOutputV1},
};
use wayland_client::{
    ConnectError, Connection, Dispatch, Proxy, QueueHandle, WEnum,
    backend::WaylandError,
    globals::{BindError, GlobalError, GlobalListContents, registry_queue_init},
    protocol::{
        wl_output::{self, WlOutput},
        wl_registry::{self, WlRegistry},
    },
};

use super::{ModeRequest, WorkspaceMode};
use crate::util::{
    outputs::Outputs,
    wayland::{Pump, PumpError},
};

/// The first version with an object for each output.
const PER_OUTPUT_VERSION: u32 = 2;

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

/// What the compositor reported, as the service publishes it.
#[derive(Debug, PartialEq, Eq)]
pub struct Heard {
    pub focused: WorkspaceMode,
    pub outputs: BTreeMap<String, WorkspaceMode>,
}

pub struct ModeConnection {
    pump: Pump<Reports>,
    reports: Reports,
}

struct Reports {
    manager: CrownosWorkspaceModeManagerV1,
    focused: Option<WorkspaceMode>,
    outputs: Outputs<OutputMode>,
    changed: bool,
}

struct OutputMode {
    object: CrownosWorkspaceModeOutputV1,
    mode: Option<WorkspaceMode>,
}

impl ModeConnection {
    /// Must be called on a tokio runtime with IO enabled.
    pub fn connect() -> Result<Self, ModeError> {
        let connection = Connection::connect_to_env()?;
        let (globals, queue) = registry_queue_init::<Reports>(&connection)?;
        let handle = queue.handle();
        let manager: CrownosWorkspaceModeManagerV1 =
            globals.bind(&handle, 1..=PER_OUTPUT_VERSION, ())?;
        let mut outputs = Outputs::default();
        if manager.version() >= PER_OUTPUT_VERSION {
            outputs.bind_listed(&globals, &handle, |output, global| {
                OutputMode::new(&manager, output, &handle, global)
            });
        }
        connection.flush()?;
        Ok(Self {
            pump: Pump::new(connection, queue)?,
            reports: Reports {
                manager,
                focused: None,
                outputs,
                changed: false,
            },
        })
    }

    /// Asks for `request.mode` on the bar's own output, or on the focused
    /// output when the compositor reports only that one.
    pub fn request(&self, request: &ModeRequest) {
        let mode = match request.mode {
            WorkspaceMode::Tiling => Mode::Tiling,
            WorkspaceMode::Floating => Mode::Floating,
        };
        let own_output = request
            .output
            .as_deref()
            .and_then(|name| self.reports.outputs.find(name));
        match own_output {
            Some(output) => output.object.set_mode(mode),
            None => self.reports.manager.set_mode(mode),
        }
        if let Err(error) = self.pump.flush() {
            log::warn!("could not ask for the workspace mode: {error}");
        }
    }

    /// Waits for the compositor, returning what it reported if anything
    /// changed.
    pub async fn dispatch(&mut self) -> Result<Option<Heard>, ModeError> {
        self.pump.dispatch(&mut self.reports).await?;
        Ok(self.reports.take_heard())
    }
}

impl Reports {
    fn take_heard(&mut self) -> Option<Heard> {
        if !std::mem::take(&mut self.changed) {
            return None;
        }
        let focused = self.focused?;
        let outputs = self
            .outputs
            .named()
            .filter_map(|(name, output)| Some((name.to_owned(), output.tracked.mode?)))
            .collect();
        Some(Heard { focused, outputs })
    }
}

impl OutputMode {
    fn new(
        manager: &CrownosWorkspaceModeManagerV1,
        output: &WlOutput,
        queue: &QueueHandle<Reports>,
        global: u32,
    ) -> Self {
        Self {
            object: manager.get_output(output, queue, global),
            mode: None,
        }
    }
}

fn workspace_mode(mode: WEnum<Mode>) -> Option<WorkspaceMode> {
    match mode {
        WEnum::Value(Mode::Tiling) => Some(WorkspaceMode::Tiling),
        WEnum::Value(Mode::Floating) => Some(WorkspaceMode::Floating),
        _ => None,
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for Reports {
    fn event(
        reports: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        queue: &QueueHandle<Self>,
    ) {
        let Self {
            manager,
            outputs,
            changed,
            ..
        } = reports;
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } if interface == WlOutput::interface().name
                && manager.version() >= PER_OUTPUT_VERSION =>
            {
                outputs.bind(registry, name, version, queue, |output, global| {
                    OutputMode::new(manager, output, queue, global)
                });
            }
            wl_registry::Event::GlobalRemove { name } => {
                *changed |= outputs.remove(name, |output| output.object.destroy());
            }
            _ => {}
        }
    }
}

impl Dispatch<WlOutput, u32> for Reports {
    fn event(
        reports: &mut Self,
        _: &WlOutput,
        event: wl_output::Event,
        global: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        reports.changed |= reports.outputs.describe(*global, event);
    }
}

impl Dispatch<CrownosWorkspaceModeManagerV1, ()> for Reports {
    fn event(
        reports: &mut Self,
        _: &CrownosWorkspaceModeManagerV1,
        event: crownos_workspace_mode_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let crownos_workspace_mode_manager_v1::Event::Mode { mode } = event else {
            return;
        };
        if let Some(mode) = workspace_mode(mode) {
            reports.focused = Some(mode);
            reports.changed = true;
        }
    }
}

impl Dispatch<CrownosWorkspaceModeOutputV1, u32> for Reports {
    fn event(
        reports: &mut Self,
        _: &CrownosWorkspaceModeOutputV1,
        event: crownos_workspace_mode_output_v1::Event,
        global: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let crownos_workspace_mode_output_v1::Event::Mode { mode } = event else {
            return;
        };
        if let (Some(mode), Some(output)) =
            (workspace_mode(mode), reports.outputs.tracked_mut(*global))
        {
            output.mode = Some(mode);
            reports.changed = true;
        }
    }
}

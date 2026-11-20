//! `crownos_output_brightness_v1` on a connection of the service's own, with
//! one brightness object for each `wl_output`.
//!
//! A slider drag sends many levels. Each output has at most one `set_level`
//! in each round trip to the compositor; a newer level waits for the
//! `wl_display.sync` that ends the round trip, and replaces any older one that
//! waits. Until then the output shows the level that was asked for, so the
//! slider does not jump back to an older level that the compositor echoes.

use std::io;

use crownos_protocols::output_brightness::v1::client::{
    crownos_output_brightness_manager_v1::CrownosOutputBrightnessManagerV1,
    crownos_output_brightness_v1::{self, Control as ProtocolControl, CrownosOutputBrightnessV1},
};
use wayland_client::{
    ConnectError, Connection, Dispatch, Proxy, QueueHandle, WEnum,
    backend::WaylandError,
    globals::{BindError, GlobalError, GlobalListContents, registry_queue_init},
    protocol::{
        wl_callback::{self, WlCallback},
        wl_display::WlDisplay,
        wl_output::{self, WlOutput},
        wl_registry::{self, WlRegistry},
    },
};

use super::{
    BrightnessCommand,
    display::{Control, Display, DisplayId, protocol_level, slider_level},
    level::OutputLevel,
};
use crate::util::{
    outputs::Outputs,
    wayland::{Pump, PumpError},
};

#[derive(Debug, thiserror::Error)]
pub enum BrightnessError {
    #[error("no Wayland compositor to talk to: {0}")]
    Connect(#[from] ConnectError),
    #[error("could not read the compositor's globals: {0}")]
    Globals(#[from] GlobalError),
    #[error("the compositor does not control brightness: {0}")]
    Unsupported(#[from] BindError),
    #[error("Wayland connection failed: {0}")]
    Wayland(#[from] WaylandError),
    #[error(transparent)]
    Pump(#[from] PumpError),
    #[error(transparent)]
    Io(#[from] io::Error),
}

pub struct BrightnessConnection {
    pump: Pump<Reports>,
    reports: Reports,
}

struct Reports {
    display: WlDisplay,
    manager: CrownosOutputBrightnessManagerV1,
    outputs: Outputs<OutputBrightness>,
    round_trip: Option<WlCallback>,
    changed: bool,
}

struct OutputBrightness {
    object: CrownosOutputBrightnessV1,
    control: Option<Control>,
    level: OutputLevel,
}

impl BrightnessConnection {
    /// Must be called on a tokio runtime with IO enabled.
    pub fn connect() -> Result<Self, BrightnessError> {
        let connection = Connection::connect_to_env()?;
        let (globals, queue) = registry_queue_init::<Reports>(&connection)?;
        let handle = queue.handle();
        let manager: CrownosOutputBrightnessManagerV1 = globals.bind(&handle, 1..=1, ())?;
        let mut outputs = Outputs::default();
        outputs.bind_listed(&globals, &handle, |output, global| {
            OutputBrightness::new(&manager, output, &handle, global)
        });
        connection.flush()?;
        Ok(Self {
            reports: Reports {
                display: connection.display(),
                manager,
                outputs,
                round_trip: None,
                changed: false,
            },
            pump: Pump::new(connection, queue)?,
        })
    }

    pub fn command(&mut self, command: BrightnessCommand) {
        let queue = self.pump.handle();
        match command {
            BrightnessCommand::SetLevel { id, level } => {
                self.reports.ask(Some(&id), protocol_level(level));
            }
            BrightnessCommand::SetAll(level) => self.reports.ask(None, protocol_level(level)),
            BrightnessCommand::Refresh => self.reports.refresh(),
        }
        self.reports.send_waiting(&queue);
        if let Err(error) = self.pump.flush() {
            log::warn!("could not send the brightness: {error}");
        }
    }

    /// Waits for the compositor and hands it whatever waited for the round
    /// trip to end.
    pub async fn dispatch(&mut self) -> Result<(), BrightnessError> {
        Ok(self.pump.dispatch(&mut self.reports).await?)
    }

    /// Every display whose first state is complete, if anything changed since
    /// the last call.
    pub fn take_displays(&mut self) -> Option<Vec<Display>> {
        std::mem::take(&mut self.reports.changed).then(|| self.reports.displays())
    }
}

impl Reports {
    /// Asks for `level` on the display called `id`, or on every display.
    fn ask(&mut self, id: Option<&DisplayId>, level: u32) {
        let asked = self.outputs.named_tracked_mut().filter(|(name, output)| {
            output.adjustable() && id.is_none_or(|id| id.as_str() == *name)
        });
        for (_, output) in asked {
            output.level.ask(level);
            self.changed = true;
        }
    }

    fn refresh(&self) {
        for (_, output) in self.outputs.named() {
            output.tracked.object.refresh();
        }
    }

    /// Sends the newest level of each output that has one waiting, unless the
    /// round trip that carries the last ones has not ended.
    fn send_waiting(&mut self, queue: &QueueHandle<Self>) {
        if self.round_trip.is_some() {
            return;
        }
        let mut sent_any = false;
        for (_, output) in self.outputs.named_tracked_mut() {
            if let Some(level) = output.level.send() {
                output.object.set_level(level);
                sent_any = true;
            }
        }
        if sent_any {
            self.round_trip = Some(self.display.sync(queue, ()));
        }
    }

    fn round_trip_ended(&mut self, queue: &QueueHandle<Self>) {
        self.round_trip = None;
        for (_, output) in self.outputs.named_tracked_mut() {
            self.changed |= output.level.round_trip_ended();
        }
        self.send_waiting(queue);
    }

    fn displays(&self) -> Vec<Display> {
        self.outputs
            .named()
            .filter_map(|(name, bound)| {
                let output = &bound.tracked;
                Some(Display {
                    id: DisplayId::new(name),
                    label: bound.description.as_deref().unwrap_or(name).to_owned(),
                    control: output.control?,
                    level: slider_level(output.level.shown()?),
                })
            })
            .collect()
    }
}

impl OutputBrightness {
    fn new(
        manager: &CrownosOutputBrightnessManagerV1,
        output: &WlOutput,
        queue: &QueueHandle<Reports>,
        global: u32,
    ) -> Self {
        Self {
            object: manager.get_output(output, queue, global),
            control: None,
            level: OutputLevel::default(),
        }
    }

    fn adjustable(&self) -> bool {
        self.control.is_some_and(Control::adjustable)
    }
}

fn control(control: WEnum<ProtocolControl>) -> Control {
    match control {
        WEnum::Value(ProtocolControl::Backlight) => Control::Backlight,
        WEnum::Value(ProtocolControl::Ddc) => Control::Ddc,
        WEnum::Value(ProtocolControl::Software) => Control::Software,
        _ => Control::None,
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
            } if interface == WlOutput::interface().name => {
                outputs.bind(registry, name, version, queue, |output, global| {
                    OutputBrightness::new(manager, output, queue, global)
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

impl Dispatch<CrownosOutputBrightnessManagerV1, ()> for Reports {
    fn event(
        _: &mut Self,
        _: &CrownosOutputBrightnessManagerV1,
        _: <CrownosOutputBrightnessManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<CrownosOutputBrightnessV1, u32> for Reports {
    fn event(
        reports: &mut Self,
        _: &CrownosOutputBrightnessV1,
        event: crownos_output_brightness_v1::Event,
        global: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(output) = reports.outputs.tracked_mut(*global) else {
            return;
        };
        match event {
            crownos_output_brightness_v1::Event::Control { control: reported } => {
                output.control = Some(control(reported));
            }
            crownos_output_brightness_v1::Event::Level { level } => {
                output.level.report(level);
            }
            _ => return,
        }
        reports.changed = true;
    }
}

impl Dispatch<WlCallback, ()> for Reports {
    fn event(
        reports: &mut Self,
        _: &WlCallback,
        event: wl_callback::Event,
        _: &(),
        _: &Connection,
        queue: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event {
            reports.round_trip_ended(queue);
        }
    }
}

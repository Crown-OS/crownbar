//! `zwlr_gamma_control_manager_v1` on a connection of the service's own.
//!
//! One control is held per output while the tint is on. The compositor answers
//! each with the size of that output's ramp, and nothing can be written until
//! it has. Releasing the tint destroys the controls, which hands the outputs
//! back to the compositor rather than pinning them to a flat ramp.

use std::{
    collections::HashMap,
    fs::File,
    io::{self, Write},
    os::fd::{AsFd, AsRawFd, RawFd},
};

use rustix::fs::{MemfdFlags, memfd_create};
use tokio::io::unix::AsyncFd;
use wayland_client::{
    ConnectError, Connection, Dispatch, DispatchError, EventQueue, Proxy, QueueHandle,
    backend::WaylandError,
    globals::{BindError, GlobalError, GlobalListContents, registry_queue_init},
    protocol::{
        wl_output::{self, WlOutput},
        wl_registry::{self, WlRegistry},
    },
};
use wayland_protocols_wlr::gamma_control::v1::client::{
    zwlr_gamma_control_manager_v1::{self, ZwlrGammaControlManagerV1},
    zwlr_gamma_control_v1::{self, ZwlrGammaControlV1},
};

use super::ramp::ramps;
use crate::util::fd;

const OUTPUT_VERSION: u32 = 4;

#[derive(Debug, thiserror::Error)]
pub enum GammaError {
    #[error("no Wayland compositor to talk to: {0}")]
    Connect(#[from] ConnectError),
    #[error("could not read the compositor's globals: {0}")]
    Globals(#[from] GlobalError),
    #[error("the compositor does not offer gamma control: {0}")]
    Unsupported(#[from] BindError),
    #[error("Wayland dispatch failed: {0}")]
    Dispatch(#[from] DispatchError),
    #[error("Wayland connection failed: {0}")]
    Wayland(#[from] WaylandError),
    #[error(transparent)]
    Io(#[from] io::Error),
}

pub struct GammaConnection {
    connection: Connection,
    queue: EventQueue<Outputs>,
    outputs: Outputs,
    readable: AsyncFd<RawFd>,
}

struct Outputs {
    manager: ZwlrGammaControlManagerV1,
    /// Keyed by the output's registry name, so a replug cannot collide with
    /// the control it replaces.
    controls: HashMap<u32, Control>,
    kelvin: Option<u16>,
}

struct Control {
    output: WlOutput,
    gamma: Option<ZwlrGammaControlV1>,
    /// Entries per channel, from the `gamma_size` event.
    size: Option<u32>,
}

impl GammaConnection {
    /// Must be called on a tokio runtime with IO enabled.
    pub fn connect() -> Result<Self, GammaError> {
        let connection = Connection::connect_to_env()?;
        let (globals, queue) = registry_queue_init::<Outputs>(&connection)?;
        let qh = queue.handle();
        let mut outputs = Outputs {
            manager: globals.bind(&qh, 1..=1, ())?,
            controls: HashMap::new(),
            kelvin: None,
        };
        globals.contents().with_list(|list| {
            for global in list
                .iter()
                .filter(|global| global.interface == WlOutput::interface().name)
            {
                outputs.add(globals.registry(), global.name, global.version, &qh);
            }
        });
        connection.flush()?;
        let readable = AsyncFd::new(connection.as_fd().as_raw_fd())?;
        Ok(Self {
            connection,
            queue,
            outputs,
            readable,
        })
    }

    /// Hold every output at `kelvin`, or release them all with `None`.
    /// Idempotent, so it can follow every snapshot however little changed.
    pub fn hold(&mut self, kelvin: Option<u16>) {
        let qh = self.queue.handle();
        self.outputs.hold(kelvin, &qh);
        if let Err(error) = self.connection.flush() {
            log::warn!("could not send the gamma ramps: {error}");
        }
    }

    /// Waits for the compositor and handles whatever it said: ramp sizes,
    /// outputs coming and going.
    ///
    /// libwayland reports a drained socket as a successful read of nothing,
    /// so readiness is cleared by asking the socket itself; trusting the read
    /// would leave tokio waking this task in a loop forever.
    pub async fn dispatch(&mut self) -> Result<(), GammaError> {
        self.queue.dispatch_pending(&mut self.outputs)?;
        self.connection.flush()?;
        let mut ready = self.readable.readable().await?;
        if let Some(guard) = self.queue.prepare_read() {
            match guard.read() {
                Ok(_) => {}
                Err(WaylandError::Io(error)) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error.into()),
            }
        }
        if !fd::has_input(self.connection.as_fd())? {
            ready.clear_ready();
        }
        self.queue.dispatch_pending(&mut self.outputs)?;
        Ok(())
    }
}

impl Outputs {
    fn add(&mut self, registry: &WlRegistry, name: u32, version: u32, qh: &QueueHandle<Self>) {
        let output = registry.bind(name, version.min(OUTPUT_VERSION), qh, ());
        let gamma = self
            .kelvin
            .map(|_| self.manager.get_gamma_control(&output, qh, name));
        self.controls.insert(
            name,
            Control {
                output,
                gamma,
                size: None,
            },
        );
    }

    fn remove(&mut self, name: u32) {
        if let Some(mut control) = self.controls.remove(&name) {
            control.release();
            if control.output.version() >= 3 {
                control.output.release();
            }
        }
    }

    fn hold(&mut self, kelvin: Option<u16>, qh: &QueueHandle<Self>) {
        if self.kelvin == kelvin {
            return;
        }
        self.kelvin = kelvin;
        for (name, control) in &mut self.controls {
            match kelvin {
                None => control.release(),
                Some(kelvin) => match &control.gamma {
                    Some(_) => control.apply(kelvin),
                    None => {
                        control.gamma =
                            Some(self.manager.get_gamma_control(&control.output, qh, *name))
                    }
                },
            }
        }
    }
}

impl Control {
    fn release(&mut self) {
        if let Some(gamma) = self.gamma.take() {
            gamma.destroy();
        }
        self.size = None;
    }

    /// The table goes through a memory file the compositor reads from offset
    /// zero; the file can be dropped as soon as its descriptor is queued.
    fn apply(&self, kelvin: u16) {
        let (Some(gamma), Some(size)) = (&self.gamma, self.size) else {
            return;
        };
        let written = memfd_create("crownbar-gamma", MemfdFlags::CLOEXEC)
            .map(File::from)
            .map_err(io::Error::from)
            .and_then(|mut file| file.write_all(&ramps(size, kelvin)).map(|()| file));
        match written {
            Ok(file) => gamma.set_gamma(file.as_fd()),
            Err(error) => log::warn!("could not write the gamma ramp: {error}"),
        }
    }
}

impl Drop for Outputs {
    fn drop(&mut self) {
        for control in self.controls.values_mut() {
            control.release();
        }
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for Outputs {
    fn event(
        outputs: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } if interface == WlOutput::interface().name => {
                outputs.add(registry, name, version, qh);
            }
            wl_registry::Event::GlobalRemove { name } => outputs.remove(name),
            _ => {}
        }
    }
}

impl Dispatch<WlOutput, ()> for Outputs {
    fn event(
        _: &mut Self,
        _: &WlOutput,
        _: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrGammaControlManagerV1, ()> for Outputs {
    fn event(
        _: &mut Self,
        _: &ZwlrGammaControlManagerV1,
        _: zwlr_gamma_control_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrGammaControlV1, u32> for Outputs {
    fn event(
        outputs: &mut Self,
        _: &ZwlrGammaControlV1,
        event: zwlr_gamma_control_v1::Event,
        name: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(control) = outputs.controls.get_mut(name) else {
            return;
        };
        match event {
            zwlr_gamma_control_v1::Event::GammaSize { size } => {
                control.size = Some(size);
                if let Some(kelvin) = outputs.kelvin {
                    control.apply(kelvin);
                }
            }
            zwlr_gamma_control_v1::Event::Failed => {
                log::info!("another client owns an output's gamma");
                control.release();
            }
            _ => {}
        }
    }
}

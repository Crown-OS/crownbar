//! The `wl_output`s on a service's own Wayland connection, each with the
//! protocol object the service made for it, keyed by registry name.

use std::collections::BTreeMap;

use wayland_client::{
    Dispatch, Proxy, QueueHandle,
    globals::GlobalList,
    protocol::{
        wl_output::{self, WlOutput},
        wl_registry::WlRegistry,
    },
};

/// Version 4 is the first that sends the output's name.
const NAMED_VERSION: u32 = 4;
/// Version 3 is the first with `release`.
const RELEASE_VERSION: u32 = 3;

pub struct BoundOutput<T> {
    pub name: Option<String>,
    pub description: Option<String>,
    pub tracked: T,
    output: WlOutput,
}

pub struct Outputs<T> {
    bound: BTreeMap<u32, BoundOutput<T>>,
}

impl<T> Default for Outputs<T> {
    fn default() -> Self {
        Self {
            bound: BTreeMap::new(),
        }
    }
}

impl<T> Outputs<T> {
    /// Binds every `wl_output` the registry listed at connection time.
    pub fn bind_listed<State>(
        &mut self,
        globals: &GlobalList,
        queue: &QueueHandle<State>,
        mut track: impl FnMut(&WlOutput, u32) -> T,
    ) where
        State: Dispatch<WlOutput, u32> + 'static,
    {
        let listed: Vec<(u32, u32)> = globals.contents().with_list(|list| {
            list.iter()
                .filter(|global| global.interface == WlOutput::interface().name)
                .map(|global| (global.name, global.version))
                .collect()
        });
        for (global, version) in listed {
            self.bind(globals.registry(), global, version, queue, &mut track);
        }
    }

    /// Binds the `wl_output` the registry announced as `global`, and makes the
    /// service's object for it with `track`.
    pub fn bind<State>(
        &mut self,
        registry: &WlRegistry,
        global: u32,
        version: u32,
        queue: &QueueHandle<State>,
        track: impl FnOnce(&WlOutput, u32) -> T,
    ) where
        State: Dispatch<WlOutput, u32> + 'static,
    {
        let output: WlOutput = registry.bind(global, version.min(NAMED_VERSION), queue, global);
        let tracked = track(&output, global);
        self.bound.insert(
            global,
            BoundOutput {
                name: None,
                description: None,
                tracked,
                output,
            },
        );
    }

    /// Forgets the output the registry removed: `retire` destroys the
    /// service's object, then the `wl_output` is released. Returns whether
    /// `global` was an output.
    pub fn remove(&mut self, global: u32, retire: impl FnOnce(T)) -> bool {
        let Some(removed) = self.bound.remove(&global) else {
            return false;
        };
        retire(removed.tracked);
        if removed.output.version() >= RELEASE_VERSION {
            removed.output.release();
        }
        true
    }

    /// Takes the name and description the compositor sent for `global`.
    /// Returns whether either changed.
    pub fn describe(&mut self, global: u32, event: wl_output::Event) -> bool {
        let Some(bound) = self.bound.get_mut(&global) else {
            return false;
        };
        match event {
            wl_output::Event::Name { name } => replace(&mut bound.name, name),
            wl_output::Event::Description { description } => {
                replace(&mut bound.description, description)
            }
            _ => false,
        }
    }

    pub fn tracked_mut(&mut self, global: u32) -> Option<&mut T> {
        self.bound.get_mut(&global).map(|bound| &mut bound.tracked)
    }

    /// The outputs whose name has arrived, in registry order.
    pub fn named(&self) -> impl Iterator<Item = (&str, &BoundOutput<T>)> {
        self.bound
            .values()
            .filter_map(|bound| Some((bound.name.as_deref()?, bound)))
    }

    pub fn named_tracked_mut(&mut self) -> impl Iterator<Item = (&str, &mut T)> {
        self.bound
            .values_mut()
            .filter_map(|bound| Some((bound.name.as_deref()?, &mut bound.tracked)))
    }

    pub fn find(&self, name: &str) -> Option<&T> {
        self.named()
            .find(|(bound_name, _)| *bound_name == name)
            .map(|(_, bound)| &bound.tracked)
    }
}

fn replace(held: &mut Option<String>, received: String) -> bool {
    let changed = held.as_deref() != Some(received.as_str());
    *held = Some(received);
    changed
}

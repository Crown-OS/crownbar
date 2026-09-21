//! The PipeWire connection.
//!
//! Runs on a thread of its own: `ThreadLoop::lock` blocks until the current
//! loop iteration finishes, so it must never be taken from the event loop or a
//! runtime worker. Registry and node callbacks land on the loop thread and
//! publish straight from there; commands arrive on this thread and take the
//! lock around each call.

use std::{
    collections::HashMap,
    sync::{mpsc::Receiver, Arc, Mutex},
};

use pipewire_native::{
    context::Context,
    keys,
    properties::Properties,
    proxy::{
        device::{Device as PwDevice, DeviceEvents},
        metadata::{Metadata, MetadataEvents},
        node::{Node, NodeChangeMask, NodeEvents, NodeState},
        registry::{Registry, RegistryEvents},
    },
    some_closure,
    thread_loop::ThreadLoop,
    types,
};
use pipewire_native_spa::param::ParamType;

use crate::services::{
    audio::{
        device::{Device, NodeId, Role, Stream},
        props::{self, PropsUpdate},
        route, AudioCommand, AudioState,
    },
    bus::Publisher,
    status::Availability,
};

/// The session-wide metadata object, the only place the default sink and
/// source are written.
const DEFAULT_METADATA: &str = "default";
const AUDIO_DEVICE: &str = "Audio/Device";
const DEFAULT_SINK_KEY: &str = "default.audio.sink";
const DEFAULT_SOURCE_KEY: &str = "default.audio.source";
const JSON: &str = "Spa:String:JSON";

/// Bound proxies, keyed by global id. Held apart from the published snapshot:
/// a widget has no use for a proxy, and the snapshot has to stay `Clone`.
type Bound = Arc<Mutex<Tracked>>;

#[derive(Default)]
struct Tracked {
    nodes: HashMap<NodeId, Node>,
    cards: HashMap<NodeId, Card>,
    metadata: Option<Metadata>,
}

/// One sound card and the ports it currently offers.
struct Card {
    device: PwDevice,
    ports: Vec<route::Port>,
}

impl Card {
    /// The port a node plays out of, by its `card.profile.device`.
    fn port(&self, card_port: i32, output: bool) -> Option<route::Port> {
        self.ports
            .iter()
            .find(|port| port.device == card_port && port.output == output)
            .copied()
    }
}

pub fn run(publish: Publisher<AudioState>, commands: Receiver<AudioCommand>) {
    // Sets up the crate's global support libraries and logging. Every other
    // call in the crate, `ThreadLoop::new` included, assumes it has run.
    pipewire_native::init();

    let mut props = Properties::new();
    props.set(keys::APP_NAME, "crownbar".into());

    let Some(loop_) = ThreadLoop::new(&props) else {
        return fail(&publish, "could not create a pipewire loop");
    };
    let context = match Context::new(loop_.main_loop(), Properties::new()) {
        Ok(context) => context,
        Err(e) => return fail(&publish, e),
    };
    let core = match context.connect(None) {
        Ok(core) => core,
        Err(e) => return fail(&publish, e),
    };
    let registry = match core.registry() {
        Ok(registry) => registry,
        Err(e) => return fail(&publish, e),
    };

    let tracked: Bound = Bound::default();
    let publish = Arc::new(publish);
    watch(&registry, &tracked, &publish);

    publish.edit(|state| {
        state.availability = Availability::Ready;
        true
    });

    loop_.run();
    while let Ok(command) = commands.recv() {
        let _guard = loop_.lock();
        apply(command, &tracked, &publish);
    }
    loop_.quit();
}

fn fail(publish: &Publisher<AudioState>, reason: impl ToString) {
    let reason = reason.to_string();
    log::info!("audio unavailable: {reason}");
    publish.edit(|state| {
        *state = AudioState::unavailable(reason);
        true
    });
}

/// Subscribe to the registry. Every audio node and the default-metadata object
/// are bound as they appear; anything else is ignored without binding, so the
/// client holds no proxies it will never read.
fn watch(registry: &Registry, tracked: &Bound, publish: &Arc<Publisher<AudioState>>) {
    registry.add_listener(RegistryEvents {
        global: some_closure!([registry ^(tracked, publish)] id, _permissions, type_, version, properties, {
            match type_ {
                types::interface::NODE => {
                    let Some(role) = Role::classify(properties) else { return };
                    let Ok(bound) = registry.bind(id, type_, version) else { return };
                    let Some(node) = bound.downcast::<Node>() else { return };
                    listen(&node, id, publish);
                    tracked.lock().unwrap_or_else(|e| e.into_inner()).nodes.insert(id, node);
                    let properties = properties.clone();
                    publish.edit(|state| {
                        match role {
                            Role::Output => state.outputs.push(Device::from_props(id, &properties, role)),
                            Role::Input => state.inputs.push(Device::from_props(id, &properties, role)),
                            Role::PlaybackStream => state.playback.push(Stream::from_props(id, &properties)),
                            Role::RecordStream => state.recording.push(Stream::from_props(id, &properties)),
                        }
                        state.resolve();
                        true
                    });
                }
                types::interface::DEVICE if properties.get("media.class") == Some(AUDIO_DEVICE) => {
                    let Ok(bound) = registry.bind(id, type_, version) else { return };
                    let Some(device) = bound.downcast::<PwDevice>() else { return };
                    follow_routes(&device, id, tracked);
                    tracked.lock().unwrap_or_else(|e| e.into_inner())
                        .cards.insert(id, Card { device, ports: Vec::new() });
                }
                types::interface::METADATA if properties.get("metadata.name") == Some(DEFAULT_METADATA) => {
                    let Ok(bound) = registry.bind(id, type_, version) else { return };
                    let Some(metadata) = bound.downcast::<Metadata>() else { return };
                    follow_defaults(&metadata, publish);
                    tracked.lock().unwrap_or_else(|e| e.into_inner()).metadata = Some(metadata);
                }
                _ => {}
            }
        }),
        global_remove: some_closure!([^(tracked, publish)] id, {
            let mut guard = tracked.lock().unwrap_or_else(|e| e.into_inner());
            guard.nodes.remove(&id);
            guard.cards.remove(&id);
            drop(guard);
            publish.edit(|state| state.remove(id));
        }),
    });
}

/// Follow one node's volume and mute. `subscribe_params` is what makes this
/// push rather than poll — the server re-sends `Props` on every change,
/// including ones made by other applications.
fn listen(node: &Node, id: NodeId, publish: &Arc<Publisher<AudioState>>) {
    node.add_listener(NodeEvents {
        // The registry global carries only a summary of a node's properties;
        // the full set — the card's icon, bus and ALSA keys, which is what
        // classification needs — arrives here.
        info: some_closure!([^(publish)] info, {
            // `info` also arrives for a bare state or params change, carrying
            // no properties at all. Taking those as the node's property set
            // would blank its name — and a device whose name no longer matches
            // `default.audio.sink` stops being the default.
            let properties = info
                .mask
                .contains(NodeChangeMask::PROPS)
                .then(|| info.props.clone());
            let active = info.state == NodeState::Running;
            publish.edit(|state| state.describe(id, properties.as_ref(), active));
        }),
        param: some_closure!([^(publish)] _seq, param, _index, _next, pod, {
            if param != ParamType::Props {
                return;
            }
            let update = props::decode(pod);
            publish.edit(|state| {
                state
                    .nodes_mut()
                    .find(|(node_id, _)| *node_id == id)
                    .map(|(_, volume)| update.apply(volume))
                    .unwrap_or(false)
            });
        }),
    });
    if let Err(e) = node.subscribe_params(&[ParamType::Props]) {
        log::info!("could not follow volume on node {id}: {e}");
    }
}

/// Follow a card's routes. A route carries the volume stage the rest of the
/// desktop reads and writes, so this is what keeps the bar's slider and the
/// machine's volume keys showing the same number.
fn follow_routes(device: &PwDevice, card: NodeId, tracked: &Bound) {
    device.add_listener(DeviceEvents {
        info: None,
        param: some_closure!([^(tracked)] _seq, param, _index, _next, pod, {
            if param != ParamType::Route {
                return;
            }
            let Some(port) = route::decode(pod) else { return };
            let mut guard = tracked.lock().unwrap_or_else(|e| e.into_inner());
            let Some(card) = guard.cards.get_mut(&card) else { return };
            match card.ports.iter_mut().find(|p| p.index == port.index && p.device == port.device) {
                Some(existing) => *existing = port,
                None => card.ports.push(port),
            }
        }),
    });
    if let Err(e) = device.subscribe_params(&[ParamType::Route]) {
        log::info!("could not follow routes on card {card}: {e}");
    }
}

fn follow_defaults(metadata: &Metadata, publish: &Arc<Publisher<AudioState>>) {
    metadata.add_listener(MetadataEvents {
        property: some_closure!([^(publish)] subject, key, _type, value, {
            // Subject 0 is the session itself; per-node metadata is something
            // else entirely and must not be read as a default.
            if subject != 0 {
                return;
            }
            let role = match key {
                Some(DEFAULT_SINK_KEY) => Role::Output,
                Some(DEFAULT_SOURCE_KEY) => Role::Input,
                _ => return,
            };
            let name = value.and_then(node_name);
            publish.edit(|state| state.set_default(role, name));
        }),
    });
}

/// The value is `{"name":"alsa_output.pci-0000_00_1f.3.analog-stereo"}`. It is
/// not worth a JSON dependency to read one string out of one shape the session
/// manager has written the same way for years.
fn node_name(value: &str) -> Option<String> {
    let rest = value.split_once("\"name\"")?.1;
    let rest = rest.split_once(':')?.1;
    let rest = rest.trim_start().strip_prefix('"')?;
    Some(rest.split_once('"')?.0.to_owned())
}

fn apply(command: AudioCommand, tracked: &Bound, publish: &Publisher<AudioState>) {
    let state = publish.read();
    let (id, change) = match command {
        AudioCommand::SetOutputVolume(level) => (
            state.default_output().map(|d| d.id),
            PropsUpdate::gain(level),
        ),
        AudioCommand::SetOutputMuted(muted) => (
            state.default_output().map(|d| d.id),
            PropsUpdate::mute(muted),
        ),
        AudioCommand::SetInputVolume(level) => (
            state.default_input().map(|d| d.id),
            PropsUpdate::gain(level),
        ),
        AudioCommand::SetInputMuted(muted) => (
            state.default_input().map(|d| d.id),
            PropsUpdate::mute(muted),
        ),
        AudioCommand::SetNodeVolume { id, level } => (Some(id), PropsUpdate::gain(level)),
        AudioCommand::SetNodeMuted { id, muted } => (Some(id), PropsUpdate::mute(muted)),
        AudioCommand::SetDefaultOutput(name) => {
            return set_default(tracked, DEFAULT_SINK_KEY, &name);
        }
        AudioCommand::SetDefaultInput(name) => {
            return set_default(tracked, DEFAULT_SOURCE_KEY, &name);
        }
    };

    let Some(id) = id else { return };

    // Show it before writing it. The server echoes the change back a moment
    // later, which is what confirms it; a write that failed simply never
    // produces that echo and the next one corrects us.
    publish.edit(|state| {
        state
            .nodes_mut()
            .find(|(node_id, _)| *node_id == id)
            .map(|(_, volume)| change.apply(volume))
            .unwrap_or(false)
    });

    let guard = tracked.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(result) = write_route(&state, &guard, id, change) {
        if let Err(e) = result {
            log::info!("could not write the route for node {id}: {e}");
        }
        return;
    }
    let Some(node) = guard.nodes.get(&id) else { return };
    let result = match (change.gain, change.muted) {
        (Some(gain), _) => props::set_volume(node, gain),
        (_, Some(muted)) => props::set_muted(node, muted),
        _ => return,
    };
    if let Err(e) = result {
        log::info!("could not write props to node {id}: {e}");
    }
}

/// `None` when this node has no card port — a Bluetooth sink or an application
/// stream — in which case the node's own `Props` is the only stage there is.
fn write_route(
    state: &AudioState,
    tracked: &Tracked,
    id: NodeId,
    change: PropsUpdate,
) -> Option<std::io::Result<()>> {
    let (device, output) = state
        .outputs
        .iter()
        .map(|d| (d, true))
        .chain(state.inputs.iter().map(|d| (d, false)))
        .find(|(d, _)| d.id == id)?;
    let card = tracked.cards.get(&device.card?)?;
    let port = card.port(device.card_port?, output)?;
    Some(route::set(&card.device, port, change))
}

fn set_default(tracked: &Bound, key: &str, name: &str) {
    let guard = tracked.lock().unwrap_or_else(|e| e.into_inner());
    let Some(metadata) = guard.metadata.as_ref() else {
        return;
    };
    let value = format!("{{\"name\":\"{name}\"}}");
    if let Err(e) = metadata.set_property(0, Some(key), Some(JSON), Some(&value)) {
        log::info!("could not set {key} to {name}: {e}");
    }
}

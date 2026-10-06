//! Plugin widgets, over crownos-ipc from crownplugind.
//!
//! The daemon runs the plugins and sends every tree the bar draws as one
//! snapshot, again in full whenever any of them changes; the bar mirrors the
//! newest and sends back what the user did to them.
//!
//! crownplugind may be absent, may start after the bar and may be restarted
//! under it. While it is down the bar shows no plugin pills and retries with a
//! growing pause; on reconnecting it attaches afresh, since a restarted daemon
//! knows nothing of the last session. Events for a daemon that is not there
//! are dropped: a click on a pill that no longer exists asks for nothing.

use std::{sync::Arc, time::Duration};

use crownos_ipc::adapter::tokio::AsyncClient;
use crownplugin_proto::{Host, RemoteSurface, Snapshot, SurfaceKey, UiEvent, plugind};
use tokio::time;

use crate::services::bus::{Backend, Commands, Publisher};

/// First pause before retrying a connection that failed.
const RECONNECT_MIN: Duration = Duration::from_secs(1);
/// Longest pause: an absent daemon costs a syscall this often, and one that
/// comes up is on the bar within this long.
const RECONNECT_MAX: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Default)]
pub struct PluginsState {
    /// Counts connections. Tree revisions only compare within one: a
    /// restarted daemon numbers them from the start again.
    pub session: u64,
    /// What the daemon has for the bar; empty while it is down.
    pub surfaces: Vec<Arc<RemoteSurface>>,
}

impl PluginsState {
    pub fn surface(&self, plugin: &str, widget: &str) -> Option<&Arc<RemoteSurface>> {
        self.surfaces
            .iter()
            .find(|surface| surface.plugin == plugin && surface.widget == widget)
    }
}

#[derive(Clone, Debug)]
pub enum PluginsCommand {
    /// The user acted on revision `rev` of a tree.
    Event {
        surface: SurfaceKey,
        rev: u32,
        event: UiEvent,
    },
    /// A plugin's panel opened or closed.
    Popup { surface: SurfaceKey, open: bool },
}

pub type Channel = crate::services::bus::Channel<PluginsState, PluginsCommand>;

pub async fn run(backend: Backend<PluginsState, PluginsCommand>) {
    let Backend {
        publish,
        mut commands,
    } = backend;

    let mut pause = RECONNECT_MIN;
    loop {
        if let Some((client, snapshot)) = connect().await {
            pause = RECONNECT_MIN;
            if !session(&publish, &mut commands, client, snapshot).await {
                return;
            }
        }
        offline(&publish);
        if !backoff(&mut commands, pause).await {
            return;
        }
        pause = (pause * 2).min(RECONNECT_MAX);
    }
}

/// Subscribe first, then attach, so no change lands between the two unseen.
async fn connect() -> Option<(AsyncClient, Snapshot)> {
    let mut client = AsyncClient::new(plugind::Client::connect().ok()?.into_inner()).ok()?;
    client.subscribe::<plugind::BarChanged>().await.ok()?;
    let snapshot = client
        .call::<plugind::attach>(&plugind::attach { host: Host::Bar }, Vec::new())
        .await
        .ok()?;
    Some((client, snapshot))
}

/// Pump one connection until it breaks. Returns whether the bar is still there.
async fn session(
    publish: &Publisher<PluginsState>,
    commands: &mut Commands<PluginsCommand>,
    mut client: AsyncClient,
    attached: Snapshot,
) -> bool {
    let mut revision = attached.revision;
    publish.edit(|state| {
        state.session = state.session.wrapping_add(1);
        state.surfaces = shared(attached.surfaces);
        true
    });

    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(command) => if send(&mut client, command).await.is_err() {
                    return true;
                },
                None => return false,
            },
            pumped = client.pump() => {
                if pumped.is_err() {
                    return true;
                }
                drain(publish, &mut client, &mut revision);
            }
        }
    }
}

/// Only the newest snapshot matters, and only if it is newer than what the
/// bar holds — an event queued before `attach` answered may be older.
fn drain(publish: &Publisher<PluginsState>, client: &mut AsyncClient, revision: &mut u64) {
    let inbox = client.client_mut();
    let mut newest = None;
    while let Some(changed) = inbox.next_event::<plugind::BarChanged>() {
        newest = Some(changed.snapshot);
    }
    let Some(snapshot) = newest.filter(|snapshot| snapshot.revision > *revision) else {
        return;
    };
    *revision = snapshot.revision;
    publish.edit(|state| {
        state.surfaces = shared(snapshot.surfaces);
        true
    });
}

fn shared(surfaces: Vec<RemoteSurface>) -> Vec<Arc<RemoteSurface>> {
    surfaces.into_iter().map(Arc::new).collect()
}

async fn send(client: &mut AsyncClient, command: PluginsCommand) -> Result<(), crownos_ipc::Error> {
    match command {
        PluginsCommand::Event {
            surface,
            rev,
            event,
        } => {
            client
                .notify::<plugind::ui_event>(
                    &plugind::ui_event {
                        surface,
                        rev,
                        event,
                    },
                    Vec::new(),
                )
                .await
        }
        PluginsCommand::Popup { surface, open } => {
            client
                .notify::<plugind::popup>(&plugind::popup { surface, open }, Vec::new())
                .await
        }
    }
}

/// Sleep out `pause`, discarding whatever is asked for meanwhile. Returns
/// whether the bar is still there.
async fn backoff(commands: &mut Commands<PluginsCommand>, pause: Duration) -> bool {
    let deadline = time::sleep(pause);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            _ = &mut deadline => return true,
            command = commands.recv() => if command.is_none() {
                return false;
            },
        }
    }
}

/// The daemon is gone, and its pills with it.
fn offline(publish: &Publisher<PluginsState>) {
    publish.edit(|state| !std::mem::take(&mut state.surfaces).is_empty());
}

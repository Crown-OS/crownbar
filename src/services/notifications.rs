//! The notification centre, over crownos-ipc.
//!
//! The one service with no reading to take: crownotify owns the state and the
//! bar mirrors it. Everything published here arrives as an event, so the pill
//! is never guessing — flipping Do Not Disturb from a keybinding moves the
//! bell exactly as clicking it does.
//!
//! crownotify may not be installed, may start after the bar, and may be
//! restarted under it, so the loop reconnects for as long as the bar is alive.
//! Commands issued while it is down are dropped rather than queued: a toggle
//! the user asked for a minute ago is not one they still want.

use std::{sync::Arc, time::Duration};

use crownos_ipc::adapter::tokio::AsyncClient;
use crownotify::proto::{protocol, CenterVisibility, DoNotDisturbChanged};
use tokio::time;

use crate::services::{
    bus::{Backend, Commands, Publisher},
    status::Availability,
};

/// How long to wait before retrying a connection that failed. Long enough that
/// an absent crownotify costs a syscall a second, short enough that a restart
/// is picked up before the pill looks wrong.
const RECONNECT: Duration = Duration::from_secs(1);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NotificationsState {
    pub availability: Availability,
    /// Whether the notification centre is on screen.
    pub center_open: bool,
    pub do_not_disturb: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum NotificationsCommand {
    ToggleCenter,
    SetDoNotDisturb(bool),
    DismissAll,
}

pub type Channel = crate::services::bus::Channel<NotificationsState, NotificationsCommand>;

pub async fn run(backend: Backend<NotificationsState, NotificationsCommand>) {
    let Backend {
        publish,
        mut commands,
    } = backend;

    loop {
        if let Some(client) = connect().await
            && !session(&publish, &mut commands, client).await
        {
            return;
        }
        offline(&publish);
        if !backoff(&mut commands).await {
            return;
        }
    }
}

/// Connect and subscribe. Both events or neither: a client that heard about
/// one half of the state would show the other half stale forever.
async fn connect() -> Option<AsyncClient> {
    let mut client = AsyncClient::new(protocol::Client::connect().ok()?.into_inner()).ok()?;
    client.subscribe::<CenterVisibility>().await.ok()?;
    client.subscribe::<DoNotDisturbChanged>().await.ok()?;
    Some(client)
}

/// Pump one connection until it breaks. Returns whether the bar is still there.
async fn session(
    publish: &Publisher<NotificationsState>,
    commands: &mut Commands<NotificationsCommand>,
    mut client: AsyncClient,
) -> bool {
    publish.edit(|state| {
        let changed = state.availability != Availability::Ready;
        state.availability = Availability::Ready;
        changed
    });

    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(command) => if send(&mut client, command).await.is_err() {
                    return true;
                },
                None => return false,
            },
            // Awaiting one event type would starve the other, so the socket is
            // pumped and both queues drained — see `AsyncClient::pump`.
            pumped = client.pump() => {
                if pumped.is_err() {
                    return true;
                }
                drain(publish, &mut client);
            }
        }
    }
}

/// Sleep out the retry interval, discarding whatever is asked for meanwhile.
/// Returns whether the bar is still there.
async fn backoff(commands: &mut Commands<NotificationsCommand>) -> bool {
    let deadline = time::sleep(RECONNECT);
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

fn drain(publish: &Publisher<NotificationsState>, client: &mut AsyncClient) {
    let inbox = client.client_mut();
    while let Some(event) = inbox.next_event::<CenterVisibility>() {
        publish.edit(|state| {
            let changed = state.center_open != event.open;
            state.center_open = event.open;
            changed
        });
    }
    while let Some(event) = inbox.next_event::<DoNotDisturbChanged>() {
        publish.edit(|state| {
            let changed = state.do_not_disturb != event.enabled;
            state.do_not_disturb = event.enabled;
            changed
        });
    }
}

async fn send(
    client: &mut AsyncClient,
    command: NotificationsCommand,
) -> Result<(), crownos_ipc::Error> {
    match command {
        NotificationsCommand::ToggleCenter => {
            client
                .notify::<protocol::toggle_notification_center>(
                    &protocol::toggle_notification_center {},
                    Vec::new(),
                )
                .await
        }
        NotificationsCommand::SetDoNotDisturb(enabled) => {
            client
                .notify::<protocol::set_do_not_disturb>(
                    &protocol::set_do_not_disturb { enabled },
                    Vec::new(),
                )
                .await
        }
        NotificationsCommand::DismissAll => {
            client
                .notify::<protocol::dismiss_all>(&protocol::dismiss_all {}, Vec::new())
                .await
        }
    }
}

/// crownotify is not answering. The pill leaves the bar rather than offering a
/// bell that rings nothing.
fn offline(publish: &Publisher<NotificationsState>) {
    publish.edit(|state| {
        let reason = Availability::Unavailable(Arc::from("crownotify is not running"));
        let changed = state.availability != reason;
        state.availability = reason;
        state.center_open = false;
        state.do_not_disturb = false;
        changed
    });
}

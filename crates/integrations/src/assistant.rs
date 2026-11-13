//! The bar's link to `sujad`, the voice assistant: its state, microphone level
//! and the words of the current exchange, for the companion orb.

use crownos_ipc::adapter::tokio::AsyncClient;
use suja_proto::protocol::{self, Event};
use suja_proto::{AssistantState, decode_event};
use tokio::time::Instant;

use crate::{
    bus::{Backend, Commands, Publisher},
    reconnect::Backoff,
};

#[derive(Clone, Debug, Default)]
pub struct AssistantSnapshot {
    pub connected: bool,
    pub state: AssistantState,
    pub level: f32,
    pub heard: String,
    pub reply: String,
    pub tool: Option<ToolActivity>,
    pub last_first_audio_ms: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct ToolActivity {
    pub name: String,
    pub outcome: Option<(bool, String)>,
}

#[derive(Clone, Copy, Debug)]
pub enum AssistantCommand {
    TalkToggle,
    Cancel,
}

pub type Channel = crate::bus::Channel<AssistantSnapshot, AssistantCommand>;

pub async fn run(backend: Backend<AssistantSnapshot, AssistantCommand>) {
    let Backend {
        publish,
        mut commands,
    } = backend;
    let mut backoff = Backoff::default();
    loop {
        let started = Instant::now();
        if let Some(client) = connect().await
            && !session(&publish, &mut commands, client).await
        {
            return;
        }
        backoff.reset_if_held(started);
        offline(&publish);
        if !backoff.wait_discarding(&mut commands).await {
            return;
        }
    }
}

async fn connect() -> Option<AsyncClient> {
    let mut client = AsyncClient::connect(protocol::SERVICE).ok()?;
    client.subscribe::<protocol::StateChanged>().await.ok()?;
    client.subscribe::<protocol::MicLevel>().await.ok()?;
    client
        .subscribe::<protocol::PartialTranscript>()
        .await
        .ok()?;
    client.subscribe::<protocol::FinalTranscript>().await.ok()?;
    client.subscribe::<protocol::AssistantText>().await.ok()?;
    client.subscribe::<protocol::ToolCall>().await.ok()?;
    client.subscribe::<protocol::ToolResult>().await.ok()?;
    client.subscribe::<protocol::TurnFinished>().await.ok()?;
    client.subscribe::<protocol::Failure>().await.ok()?;
    Some(client)
}

async fn session(
    publish: &Publisher<AssistantSnapshot>,
    commands: &mut Commands<AssistantCommand>,
    mut client: AsyncClient,
) -> bool {
    publish.edit(|snapshot| {
        *snapshot = AssistantSnapshot {
            connected: true,
            ..AssistantSnapshot::default()
        };
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
                drain(publish, &mut client);
            }
        }
    }
}

fn drain(publish: &Publisher<AssistantSnapshot>, client: &mut AsyncClient) {
    let inbox = client.client_mut();
    while let Some(message) = inbox.next_event_raw() {
        if let Some(event) = decode_event(&message) {
            publish.edit(|snapshot| apply(snapshot, event));
        }
    }
}

fn apply(snapshot: &mut AssistantSnapshot, event: Event) -> bool {
    match event {
        Event::StateChanged(e) => {
            if e.state == AssistantState::Listening {
                snapshot.heard.clear();
                snapshot.reply.clear();
                snapshot.tool = None;
            }
            snapshot.state = e.state;
        }
        Event::MicLevel(e) => snapshot.level = e.level,
        Event::PartialTranscript(e) => snapshot.heard = e.text,
        Event::FinalTranscript(e) => snapshot.heard = e.text,
        Event::AssistantText(e) => snapshot.reply.push_str(&e.delta),
        Event::ToolCall(e) => {
            snapshot.tool = Some(ToolActivity {
                name: e.name,
                outcome: None,
            })
        }
        Event::ToolResult(e) => {
            snapshot.tool = Some(ToolActivity {
                name: e.name,
                outcome: Some((e.ok, e.summary)),
            })
        }
        Event::TurnFinished(e) => {
            snapshot.last_first_audio_ms = Some(e.timings.endpoint_to_first_audio_ms());
            snapshot.level = 0.0;
        }
        Event::Failure(e) => snapshot.reply = e.message,
    }
    true
}

async fn send(
    client: &mut AsyncClient,
    command: AssistantCommand,
) -> Result<(), crownos_ipc::Error> {
    match command {
        AssistantCommand::TalkToggle => {
            client
                .notify::<protocol::talk_toggle>(&protocol::talk_toggle {}, Vec::new())
                .await
        }
        AssistantCommand::Cancel => {
            client
                .notify::<protocol::cancel>(&protocol::cancel {}, Vec::new())
                .await
        }
    }
}

fn offline(publish: &Publisher<AssistantSnapshot>) {
    publish.edit(|snapshot| {
        let was_connected = snapshot.connected;
        *snapshot = AssistantSnapshot::default();
        was_connected
    });
}

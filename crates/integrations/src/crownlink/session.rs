//! One connection to crownlink, from subscribing to the socket closing.

use std::{path::Path, time::Instant};

use crownlink_linux::ipc::proto::{self, DeviceId, Event, Topic};
use crownos_ipc::{
    Error, EventMsg, Message, MethodMsg, Pending, adapter::tokio::AsyncClient, postcard,
};

use crate::{
    bus::{Commands, Publisher},
    crownlink::{
        PhoneCommand, PhoneState, Stream,
        roster::Followup,
        stream::{CAMERA, MIRROR, MONITOR},
        topic,
    },
    status::Availability,
};

pub(super) struct Session {
    client: AsyncClient,
    state: PhoneState,
    /// Commands sent and not yet answered. Their replies are only read to be
    /// retired, so a slow phone never holds up the loop.
    unanswered: Vec<Pending<()>>,
}

impl Session {
    /// Connect and subscribe to everything the state is made of. A client
    /// that missed one kind of event would show that part stale forever.
    /// `directory` overrides crownlink's socket directory.
    pub(super) async fn open(directory: Option<&Path>) -> Option<Self> {
        let mut client = match directory {
            Some(directory) => AsyncClient::connect_in(directory, proto::SERVICE),
            None => AsyncClient::connect(proto::SERVICE),
        }
        .ok()?;
        client.subscribe::<proto::DevicesChanged>().await.ok()?;
        client.subscribe::<proto::DeviceConnected>().await.ok()?;
        client.subscribe::<proto::CallStateChanged>().await.ok()?;
        client.subscribe::<proto::BatteryChanged>().await.ok()?;
        client.subscribe::<proto::HotspotChanged>().await.ok()?;
        client.subscribe::<proto::FeatureChanged>().await.ok()?;
        client.subscribe::<proto::FeatureFailed>().await.ok()?;
        client
            .subscribe::<proto::LiveActivitiesChanged>()
            .await
            .ok()?;
        client.subscribe::<proto::LocalRingChanged>().await.ok()?;
        Some(Self {
            client,
            state: PhoneState {
                availability: Availability::Ready,
                ..PhoneState::default()
            },
            unanswered: Vec::new(),
        })
    }

    /// Serve until the connection breaks. Returns whether the bar is still
    /// there.
    pub(super) async fn run(
        mut self,
        publish: &Publisher<PhoneState>,
        commands: &mut Commands<PhoneCommand>,
    ) -> bool {
        if self.relist().await.is_err() || self.settle().await.is_err() {
            return true;
        }
        self.publish(publish);
        loop {
            tokio::select! {
                command = commands.recv() => match command {
                    Some(command) => if self.send(command).is_err() {
                        return true;
                    },
                    None => return false,
                },
                pumped = self.client.pump() => {
                    if pumped.is_err() || self.settle().await.is_err() {
                        return true;
                    }
                    self.retire_answered();
                    self.publish(publish);
                    if self.client.client().is_closed() {
                        return true;
                    }
                }
            }
        }
    }

    fn publish(&self, publish: &Publisher<PhoneState>) {
        publish.edit(|held| {
            let changed = *held != self.state;
            if changed {
                held.clone_from(&self.state);
            }
            changed
        });
    }

    /// Apply every queued event in the order crownlink sent them, fetching
    /// whatever one of them leaves unknown before the next is applied.
    async fn settle(&mut self) -> Result<(), Error> {
        while let Some(message) = self.client.client_mut().next_event_raw() {
            let Some(event) = decode(&message) else {
                continue;
            };
            match self.state.apply(event, Instant::now()) {
                Followup::Settled => {}
                Followup::Probe(arrived) => self.probe(arrived).await?,
                Followup::Relist => self.relist().await?,
            }
        }
        Ok(())
    }

    /// Events are not replayed and may be dropped, so the listing is the
    /// ground truth after every (re)connect.
    async fn relist(&mut self) -> Result<(), Error> {
        let listing = self
            .client
            .call::<proto::devices>(&proto::devices {}, Vec::new())
            .await?;
        let arrived = self.state.seed(listing);
        self.state.ringing_here = self
            .pull(&proto::local_ring {})
            .await?
            .flatten()
            .or(self.state.ringing_here);
        self.probe(arrived).await
    }

    async fn probe(&mut self, arrived: Vec<DeviceId>) -> Result<(), Error> {
        for id in arrived {
            let hotspot = self.topic(id, Topic::Hotspot).await?;
            let calls = self.topic(id, Topic::Calls).await?;
            self.state.seed_topics(
                id,
                topic::hotspot(hotspot.as_deref()),
                &topic::calls(calls.as_deref()),
                Instant::now(),
            );
            let activities = self.pull(&proto::live_activities { id }).await?;
            self.state
                .seed_activities(id, activities.unwrap_or_default());
        }
        Ok(())
    }

    /// What a pull answers; `None` when crownlink refuses it — a device that
    /// left between the listing and the probe, or a feature this crownlink
    /// does not serve — which is no reason to drop the connection.
    async fn pull<M: MethodMsg<Args = M>>(
        &mut self,
        message: &M,
    ) -> Result<Option<M::Reply>, Error> {
        match self.client.call::<M>(message, Vec::new()).await {
            Err(Error::Remote(_)) => Ok(None),
            outcome => outcome.map(Some),
        }
    }

    async fn topic(&mut self, id: DeviceId, topic: Topic) -> Result<Option<Vec<u8>>, Error> {
        Ok(self.pull(&proto::state { id, topic }).await?.flatten())
    }

    /// Commands about a device that is no longer linked are dropped here
    /// rather than refused by crownlink.
    fn send(&mut self, command: PhoneCommand) -> Result<(), Error> {
        let Some(id) = command.device() else {
            return self
                .client
                .client_mut()
                .notify::<proto::stop_local_ring>(&proto::stop_local_ring {}, Vec::new());
        };
        if self.state.device(id).is_none() {
            return Ok(());
        }
        match command {
            PhoneCommand::Start(_, Stream::Camera) => self.request(&proto::start_camera {
                id,
                options: CAMERA,
            }),
            PhoneCommand::Start(_, Stream::Mic) => self.request(&proto::start_mic { id }),
            PhoneCommand::Start(_, Stream::Mirror) => self.request(&proto::start_mirror {
                id,
                options: MIRROR,
            }),
            PhoneCommand::Start(_, Stream::Monitor) => self.request(&proto::start_monitor {
                id,
                options: MONITOR,
            }),
            PhoneCommand::Stop(_, stream) => self.request(&proto::stop {
                id,
                feature: stream.feature(),
            }),
            PhoneCommand::SetHotspot(_, enabled) => {
                self.request(&proto::set_hotspot { id, enabled })
            }
            PhoneCommand::HangUp(_) => {
                let call_id = self
                    .state
                    .device(id)
                    .and_then(|device| device.call.as_ref())
                    .map(|call| call.id().to_owned());
                match call_id {
                    Some(call_id) => self.request(&proto::hangup_call { call_id }),
                    None => Ok(()),
                }
            }
            PhoneCommand::LiveAction(_, activity_id, action_id) => {
                self.request(&proto::live_activity_action {
                    id,
                    activity_id: activity_id.to_string(),
                    action_id: action_id.to_string(),
                })
            }
            PhoneCommand::RingDevice(_) => self.request(&proto::ring_device { id }),
            PhoneCommand::StopRinging(_) => self.request(&proto::stop_ringing { id }),
            PhoneCommand::ShareWifi(_, ssid) => self.request(&proto::share_wifi {
                id,
                ssid: ssid.to_string(),
            }),
            PhoneCommand::RequestWifi(_, ssid) => self.request(&proto::request_wifi {
                id,
                ssid: ssid.to_string(),
            }),
            PhoneCommand::StopLocalRing => Ok(()),
        }
    }

    fn request<M: MethodMsg<Args = M, Reply = ()>>(&mut self, message: &M) -> Result<(), Error> {
        let pending = self
            .client
            .client_mut()
            .start_request::<M>(message, Vec::new())?;
        self.unanswered.push(pending);
        Ok(())
    }

    fn retire_answered(&mut self) {
        let client = self.client.client_mut();
        self.unanswered
            .retain(|pending| match client.take(pending) {
                Ok(None) => true,
                Ok(Some(())) => false,
                Err(error) => {
                    log::debug!("crownlink did not carry out a command: {error}");
                    false
                }
            });
    }
}

/// The events the session subscribed to, in a single queue so that their
/// order survives.
fn decode(message: &Message) -> Option<Event> {
    fn as_event<E: EventMsg>(message: &Message, wrap: fn(E) -> Event) -> Option<Event> {
        (message.selector == E::SELECTOR)
            .then(|| postcard::from_bytes(&message.payload).ok())
            .flatten()
            .map(wrap)
    }
    as_event(message, Event::DevicesChanged)
        .or_else(|| as_event(message, Event::DeviceConnected))
        .or_else(|| as_event(message, Event::CallStateChanged))
        .or_else(|| as_event(message, Event::BatteryChanged))
        .or_else(|| as_event(message, Event::HotspotChanged))
        .or_else(|| as_event(message, Event::FeatureChanged))
        .or_else(|| as_event(message, Event::FeatureFailed))
        .or_else(|| as_event(message, Event::LiveActivitiesChanged))
        .or_else(|| as_event(message, Event::LocalRingChanged))
}

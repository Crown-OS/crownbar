//! What is playing, from MPRIS: one chip for the player in front, ringed by
//! the track's progress, with the transport in its panel.

mod metadata;
mod player;
mod selection;

use std::{
    convert::Infallible,
    time::{Duration, Instant},
};

use futures_util::{StreamExt, stream::select};
use zbus::{
    MatchRule, MessageStream,
    fdo::{DBusProxy, NameOwnerChanged},
    names::OwnedBusName,
};

use crate::{
    services::live::{
        Accent, Activity, Control, Feed, Glyph, LiveSource, Origin, Progress, Signal, Steer,
        Urgency,
    },
    util::deadline::sleep_until,
};
use player::{MprisPlayer, Track};
use selection::choose_active;

const MPRIS_PREFIX: &str = "org.mpris.MediaPlayer2.";
const MPRIS_PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";
/// playerctld mirrors whichever player is active; following it too would
/// count every track twice.
const PLAYERCTLD: &str = "org.mpris.MediaPlayer2.playerctld";
const SIGNAL_QUEUE_DEPTH: usize = 32;
const ID: &str = "media";
/// How long a paused track keeps its chip.
const PAUSED_LINGER: Duration = Duration::from_secs(5 * 60);

pub struct MediaSource;

impl LiveSource for MediaSource {
    type Input = Infallible;
    const ORIGIN: Origin = Origin::Media;

    async fn run(self, mut feed: Feed<Self::Input>) {
        if let Err(error) = follow(&mut feed).await {
            log::warn!("the media chip cannot reach the session bus: {error}");
        }
    }
}

async fn follow(feed: &mut Feed<Infallible>) -> zbus::Result<()> {
    let connection = zbus::Connection::session().await?;
    let bus = DBusProxy::new(&connection).await?;
    let mut owner_changes = bus.receive_name_owner_changed().await?;
    let mut signals = select(
        watch(
            &connection,
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
        )
        .await?,
        watch(&connection, PLAYER_INTERFACE, "Seeked").await?,
    );
    let mut players = Players::new(connection);
    for name in bus.list_names().await? {
        players.add(name).await;
    }
    let mut paused_since: Option<Instant> = None;
    loop {
        let track = players.active_track().await;
        let now = Instant::now();
        paused_since = match &track {
            Some(track) if !track.playing => paused_since.or(Some(now)),
            _ => None,
        };
        let stale = paused_since.is_some_and(|since| since + PAUSED_LINGER <= now);
        match track.filter(|_| !stale) {
            Some(track) => feed.show(activity(feed, &track)),
            None => feed.retire(ID),
        }
        let linger = paused_since
            .filter(|_| !stale)
            .map(|since| since + PAUSED_LINGER);
        tokio::select! {
            Some(change) = owner_changes.next() => players.apply(&change).await,
            Some(_) = signals.next() => {}
            signal = feed.next() => match signal {
                Some(Signal::Steer(Steer::Control(_, control))) => players.control(control).await,
                Some(Signal::Steer(Steer::Seek(_, fraction))) => players.seek(fraction).await,
                Some(Signal::Input(never)) => match never {},
                None => return Ok(()),
            },
            _ = sleep_until(linger) => {}
        }
    }
}

fn activity(feed: &Feed<Infallible>, track: &Track) -> Activity {
    let headline = match (track.title.is_empty(), track.artist.is_empty()) {
        (true, _) => track.app.clone(),
        (false, true) => track.title.clone(),
        (false, false) => format!("{} · {}", track.title, track.artist),
    };
    let progress = track.length.map(|span| match track.playing {
        true => Progress::Filling {
            zero: track
                .read_at
                .checked_sub(track.position)
                .unwrap_or(track.read_at),
            span,
        },
        false => Progress::Fixed(track.position.as_secs_f32() / span.as_secs_f32().max(1.0)),
    });
    let mut activity = Activity::new(feed.key(ID), Glyph::Music, track.app.as_str())
        .headline(headline)
        .accent(Accent::Green)
        .urgency(match track.playing {
            true => Urgency::Active,
            false => Urgency::Ambient,
        })
        .progress(progress)
        .controls([Control::Previous, Control::PlayPause, Control::Next]);
    for (label, value) in [
        ("Track", &track.title),
        ("Artist", &track.artist),
        ("Album", &track.album),
    ] {
        if !value.is_empty() {
            activity = activity.detail(label, value.as_str());
        }
    }
    match track.length {
        Some(_) => activity.seekable(),
        None => activity,
    }
}

async fn watch(
    connection: &zbus::Connection,
    interface: &'static str,
    member: &'static str,
) -> zbus::Result<MessageStream> {
    let rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface(interface)?
        .member(member)?
        .path(MPRIS_PATH)?
        .build();
    MessageStream::for_match_rule(rule, connection, Some(SIGNAL_QUEUE_DEPTH)).await
}

fn is_player(name: &str) -> bool {
    name.starts_with(MPRIS_PREFIX) && name != PLAYERCTLD
}

struct Players {
    connection: zbus::Connection,
    players: Vec<MprisPlayer>,
    active: Option<OwnedBusName>,
}

impl Players {
    const fn new(connection: zbus::Connection) -> Self {
        Self {
            connection,
            players: Vec::new(),
            active: None,
        }
    }

    async fn add(&mut self, name: OwnedBusName) {
        if !is_player(name.as_str()) {
            return;
        }
        self.remove(name.as_str());
        match MprisPlayer::connect(&self.connection, name).await {
            Ok(player) => self.players.push(player),
            Err(error) => log::debug!("skipping an MPRIS player: {error}"),
        }
    }

    fn remove(&mut self, name: &str) {
        self.players.retain(|player| player.name.as_str() != name);
    }

    async fn apply(&mut self, change: &NameOwnerChanged) {
        let Ok(args) = change.args() else {
            return;
        };
        let name = args.name().as_str();
        if !is_player(name) {
            return;
        }
        match args.new_owner().as_ref() {
            Some(_) => self.add(OwnedBusName::from(args.name().to_owned())).await,
            None => self.remove(name),
        }
    }

    async fn active_track(&mut self) -> Option<Track> {
        let mut states = Vec::with_capacity(self.players.len());
        for player in &self.players {
            states.push((player.name.as_str(), player.state().await));
        }
        let active = choose_active(&states, self.active.as_ref().map(|name| name.as_str()))?;
        let (index, state) = states
            .iter()
            .enumerate()
            .find_map(|(index, (name, state))| (*name == active).then_some((index, *state)))?;
        let player = self.players.get(index)?;
        self.active = Some(player.name.clone());
        player.track(state).await.ok()
    }

    fn active(&self) -> Option<&MprisPlayer> {
        let active = self.active.as_ref()?;
        self.players.iter().find(|player| player.name == *active)
    }

    async fn control(&self, control: Control) {
        if let Some(player) = self.active()
            && let Err(error) = player.control(control).await
        {
            log::warn!("{control:?} failed on {}: {error}", player.name);
        }
    }

    async fn seek(&self, fraction: f32) {
        if let Some(player) = self.active()
            && let Err(error) = player.seek(fraction).await
        {
            log::warn!("seeking failed on {}: {error}", player.name);
        }
    }
}

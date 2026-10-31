use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use zbus::{
    names::OwnedBusName,
    proxy::CacheProperties,
    zvariant::{ObjectPath, OwnedValue},
};

use crate::live::{
    Control,
    media::{
        MPRIS_PREFIX,
        metadata::TrackMetadata,
        selection::{PlaybackState, seek_target_us},
    },
};

#[zbus::proxy(
    interface = "org.mpris.MediaPlayer2.Player",
    default_path = "/org/mpris/MediaPlayer2",
    gen_blocking = false
)]
trait Player {
    fn play_pause(&self) -> zbus::Result<()>;
    fn next(&self) -> zbus::Result<()>;
    fn previous(&self) -> zbus::Result<()>;
    fn set_position(&self, track_id: &ObjectPath<'_>, position: i64) -> zbus::Result<()>;
    #[zbus(property)]
    fn playback_status(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn metadata(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
    #[zbus(property)]
    fn position(&self) -> zbus::Result<i64>;
}

#[zbus::proxy(
    interface = "org.mpris.MediaPlayer2",
    default_path = "/org/mpris/MediaPlayer2",
    gen_blocking = false
)]
trait Application {
    #[zbus(property)]
    fn identity(&self) -> zbus::Result<String>;
}

/// What one player is playing, as of `read_at`.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Track {
    pub(super) app: String,
    pub(super) title: String,
    pub(super) artist: String,
    pub(super) album: String,
    pub(super) position: Duration,
    pub(super) length: Option<Duration>,
    pub(super) playing: bool,
    pub(super) read_at: Instant,
}

/// One MPRIS player on the session bus.
pub(super) struct MprisPlayer {
    pub(super) name: OwnedBusName,
    player: PlayerProxy<'static>,
    identity: String,
}

impl MprisPlayer {
    pub(super) async fn connect(
        connection: &zbus::Connection,
        name: OwnedBusName,
    ) -> zbus::Result<Self> {
        let player = PlayerProxy::builder(connection)
            .destination(name.clone())?
            .cache_properties(CacheProperties::No)
            .build()
            .await?;
        let application = ApplicationProxy::builder(connection)
            .destination(name.clone())?
            .cache_properties(CacheProperties::No)
            .build()
            .await?;
        let identity = match application.identity().await {
            Ok(identity) => identity,
            Err(_) => fallback_identity(name.as_str()).to_owned(),
        };
        Ok(Self {
            name,
            player,
            identity,
        })
    }

    pub(super) async fn state(&self) -> PlaybackState {
        self.player
            .playback_status()
            .await
            .map_or(PlaybackState::Stopped, |status| {
                PlaybackState::parse(&status)
            })
    }

    pub(super) async fn track(&self, state: PlaybackState) -> zbus::Result<Track> {
        let track = TrackMetadata::from_map(&self.player.metadata().await?);
        let position_us = self.player.position().await.unwrap_or_default();
        Ok(Track {
            app: self.identity.clone(),
            title: track.title,
            artist: track.artist,
            album: track.album,
            position: Duration::from_micros(u64::try_from(position_us).unwrap_or_default()),
            length: track.length_us.map(Duration::from_micros),
            playing: state == PlaybackState::Playing,
            read_at: Instant::now(),
        })
    }

    pub(super) async fn control(&self, control: Control) -> zbus::Result<()> {
        match control {
            Control::PlayPause | Control::Pause | Control::Resume => self.player.play_pause().await,
            Control::Next => self.player.next().await,
            Control::Previous => self.player.previous().await,
            _ => Ok(()),
        }
    }

    pub(super) async fn seek(&self, fraction: f32) -> zbus::Result<()> {
        let track = TrackMetadata::from_map(&self.player.metadata().await?);
        match (track.track_id, track.length_us) {
            (Some(track_id), Some(length_us)) => {
                self.player
                    .set_position(&track_id, seek_target_us(length_us, fraction))
                    .await
            }
            _ => Ok(()),
        }
    }
}

fn fallback_identity(bus_name: &str) -> &str {
    bus_name
        .strip_prefix(MPRIS_PREFIX)
        .and_then(|rest| rest.split('.').next())
        .unwrap_or(bus_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bus_name_names_a_player_without_an_identity() {
        assert_eq!(
            fallback_identity("org.mpris.MediaPlayer2.chromium.instance3155"),
            "chromium"
        );
        assert_eq!(fallback_identity("odd"), "odd");
    }
}

//! Screen casts the portal is running, and what each could show instead.
//!
//! xdg-desktop-portal-crownos owns every cast, so it is the one to ask: it
//! lists them on `org.crownos.ScreenCast1` and pushes `PropertiesChanged` when
//! one starts, switches or ends. A portal that is not running simply means
//! nothing is being shared.

use std::sync::Arc;

use futures_util::StreamExt;
use zbus::{Connection, proxy};

use crate::services::{
    bus::{Backend, Publisher},
    status::Availability,
};

#[proxy(
    interface = "org.crownos.ScreenCast1",
    default_service = "org.crownos.ScreenCast",
    default_path = "/org/crownos/ScreenCast",
    gen_blocking = false
)]
trait ScreenCast {
    #[zbus(property)]
    fn casts(&self) -> zbus::Result<Vec<(u64, String, String, String, String)>>;

    fn sources(&self) -> zbus::Result<Vec<(String, String, String, String)>>;

    fn switch(&self, cast: u64, kind: &str, source: &str) -> zbus::Result<()>;

    fn stop(&self, cast: u64) -> zbus::Result<()>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    Screen,
    Window,
    Workspace,
}

impl SourceKind {
    fn parse(kind: &str) -> Option<Self> {
        match kind {
            "output" => Some(Self::Screen),
            "window" => Some(Self::Window),
            "workspace" => Some(Self::Workspace),
            _ => None,
        }
    }

    fn wire(self) -> &'static str {
        match self {
            Self::Screen => "output",
            Self::Window => "window",
            Self::Workspace => "workspace",
        }
    }
}

/// Something a cast can show, as the portal names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub kind: SourceKind,
    pub id: String,
    pub label: String,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cast {
    pub id: u64,
    pub app: String,
    pub kind: SourceKind,
    pub source: String,
    pub label: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScreencastState {
    pub availability: Availability,
    pub casts: Vec<Cast>,
    /// Fetched when a panel asks, since only the panel shows them.
    pub sources: Vec<Source>,
}

#[derive(Clone, Debug)]
pub enum ScreencastCommand {
    RefreshSources,
    Switch {
        cast: u64,
        kind: SourceKind,
        source: String,
    },
    Stop(u64),
}

pub type Channel = crate::services::bus::Channel<ScreencastState, ScreencastCommand>;

pub async fn run(backend: Backend<ScreencastState, ScreencastCommand>) {
    let Backend {
        publish,
        mut commands,
    } = backend;
    let unavailable = |reason: String| {
        publish.edit(|state| {
            state.availability = Availability::Unavailable(Arc::from(reason));
            true
        });
    };

    let bus = match Connection::session().await {
        Ok(bus) => bus,
        Err(e) => return unavailable(e.to_string()),
    };
    let proxy = match ScreenCastProxy::new(&bus).await {
        Ok(proxy) => proxy,
        Err(e) => return unavailable(e.to_string()),
    };
    let mut changes = proxy.receive_casts_changed().await;
    let mut owners = match proxy.inner().receive_owner_changed().await {
        Ok(owners) => owners,
        Err(e) => return unavailable(e.to_string()),
    };

    loop {
        publish_casts(&proxy, &publish).await;
        tokio::select! {
            command = commands.recv() => match command {
                Some(command) => apply(&proxy, &publish, command).await,
                None => return,
            },
            Some(_) = changes.next() => {}
            Some(_) = owners.next() => {}
        }
    }
}

/// No portal on the bus is not an error: it only means nothing is shared.
async fn publish_casts(proxy: &ScreenCastProxy<'_>, publish: &Publisher<ScreencastState>) {
    let casts: Vec<Cast> = proxy
        .casts()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(id, app, kind, source, label)| {
            Some(Cast {
                id,
                app,
                kind: SourceKind::parse(&kind)?,
                source,
                label,
            })
        })
        .collect();
    publish.edit(|state| {
        let changed = state.casts != casts || !matches!(state.availability, Availability::Ready);
        state.casts = casts;
        state.availability = Availability::Ready;
        changed
    });
}

async fn apply(
    proxy: &ScreenCastProxy<'_>,
    publish: &Publisher<ScreencastState>,
    command: ScreencastCommand,
) {
    let result = match command {
        ScreencastCommand::RefreshSources => match proxy.sources().await {
            Ok(sources) => {
                let sources: Vec<Source> = sources
                    .into_iter()
                    .filter_map(|(kind, id, label, detail)| {
                        Some(Source {
                            kind: SourceKind::parse(&kind)?,
                            id,
                            label,
                            detail,
                        })
                    })
                    .collect();
                publish.edit(|state| {
                    let changed = state.sources != sources;
                    state.sources = sources;
                    changed
                });
                Ok(())
            }
            Err(e) => Err(e),
        },
        ScreencastCommand::Switch { cast, kind, source } => {
            proxy.switch(cast, kind.wire(), &source).await
        }
        ScreencastCommand::Stop(cast) => proxy.stop(cast).await,
    };
    if let Err(e) = result {
        log::info!("screen cast request failed: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_kinds_round_trip_the_wire() {
        for kind in [
            SourceKind::Screen,
            SourceKind::Window,
            SourceKind::Workspace,
        ] {
            assert_eq!(SourceKind::parse(kind.wire()), Some(kind));
        }
        assert_eq!(SourceKind::parse("tablet"), None);
    }
}

//! The screen-sharing indicator: present while an app sees the screen, a
//! window or a workspace, and the place to point it at something else or end
//! it.
//!
//! The light flashes once when a cast starts and then holds still: an
//! indicator that animated for the whole call would keep the bar repainting
//! for nothing.

use std::sync::Arc;

use crate::{
    AfterAction, BarWidget, Icon, PopupAction, PopupSpec, Rune,
    animation::Spring,
    popup::{Item, PanelBuilder, Row},
};
use crownbar_integrations::{
    Services,
    screencast::{Cast, ScreencastCommand, ScreencastState, SourceKind},
};

pub struct ScreencastWidget {
    screencast: Arc<ScreencastState>,
    flash: Spring,
    targets: Vec<Option<Target>>,
}

#[derive(Clone)]
enum Target {
    Switch {
        cast: u64,
        kind: SourceKind,
        source: String,
    },
    Stop(u64),
}

impl ScreencastWidget {
    pub fn new() -> Self {
        Self {
            screencast: Arc::default(),
            flash: Spring::new(0.0),
            targets: Vec::new(),
        }
    }
}

fn rune(kind: SourceKind) -> Rune {
    match kind {
        SourceKind::Screen => Rune::Display,
        SourceKind::Window => Rune::Window,
        SourceKind::Workspace => Rune::Workspace,
    }
}

impl BarWidget for ScreencastWidget {
    fn id(&self) -> &str {
        "screencast"
    }

    fn visible(&self) -> bool {
        !self.screencast.casts.is_empty()
    }

    fn icon(&self) -> Icon {
        Icon::ScreenShare {
            pulse: self.flash.position,
        }
    }

    fn sync(&mut self, services: &Services) -> bool {
        let screencast = services.screencast.read();
        if Arc::ptr_eq(&screencast, &self.screencast) {
            return false;
        }
        let started = screencast.casts.iter().any(|cast| {
            !self
                .screencast
                .casts
                .iter()
                .any(|known| known.id == cast.id)
        });
        self.screencast = screencast;
        if started {
            self.flash.position = 1.0;
        }
        self.flash.set_target(0.0) || started
    }

    fn popup(&mut self, _services: &Services) -> Option<PopupSpec> {
        let (spec, targets) = panel(&self.screencast)?;
        self.targets = targets;
        Some(spec)
    }

    fn popup_opened(&mut self, services: &Services) {
        services.screencast.send(ScreencastCommand::RefreshSources);
    }

    fn on_popup(&mut self, action: PopupAction, services: &Services) -> AfterAction {
        let PopupAction::Activate { row } = action else {
            return AfterAction::Stay;
        };
        match self.targets.get(row).cloned().flatten() {
            Some(Target::Switch { cast, kind, source }) => {
                services
                    .screencast
                    .send(ScreencastCommand::Switch { cast, kind, source });
                AfterAction::Close
            }
            Some(Target::Stop(cast)) => {
                services.screencast.send(ScreencastCommand::Stop(cast));
                AfterAction::Close
            }
            None => AfterAction::Stay,
        }
    }

    fn tick_animation(&mut self, dt: f32) -> bool {
        if self.flash.at_rest() {
            return false;
        }
        self.flash.step(dt);
        !self.flash.at_rest()
    }
}

/// Who is watching what, what the newest cast could show instead, and how to
/// stop each one. `None` while nothing is shared.
fn panel(state: &ScreencastState) -> Option<(PopupSpec, Vec<Option<Target>>)> {
    let latest = state.casts.iter().max_by_key(|cast| cast.id)?;
    let mut panel = PanelBuilder::new();
    panel.row(Row::Header {
        title: "Screen Sharing".into(),
        toggle: None,
    });
    for cast in &state.casts {
        panel.row(
            Item::new(cast.app.clone())
                .icon(Icon::Rune(rune(cast.kind)))
                .detail(format!("Sees {}", cast.label))
                .enabled(false)
                .row(),
        );
    }

    if !state.sources.is_empty() {
        panel.row(Row::Section {
            title: share_instead_title(&state.casts, latest),
            chevron: false,
        });
        for source in &state.sources {
            panel.action(
                Item::new(source.label.clone())
                    .icon(Icon::Rune(rune(source.kind)))
                    .detail(source.detail.clone())
                    .selected(source.kind == latest.kind && source.id == latest.source)
                    .row(),
                Target::Switch {
                    cast: latest.id,
                    kind: source.kind,
                    source: source.id.clone(),
                },
            );
        }
    }

    panel.row(Row::Separator);
    for cast in &state.casts {
        let label = match state.casts.len() {
            1 => "Stop Sharing".to_owned(),
            _ => format!("Stop Sharing with {}", cast.app),
        };
        panel.action(Row::Action { label }, Target::Stop(cast.id));
    }
    Some(panel.finish())
}

/// Which cast the list switches, when there is more than one to choose from.
fn share_instead_title(casts: &[Cast], latest: &Cast) -> String {
    match casts.len() {
        1 => "Share Instead".into(),
        _ => format!("Switch {} To", latest.app),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crownbar_integrations::screencast::Source;

    fn state() -> ScreencastState {
        ScreencastState {
            casts: vec![Cast {
                id: 4,
                app: "Firefox".into(),
                kind: SourceKind::Window,
                source: "w1".into(),
                label: "Inbox".into(),
            }],
            sources: vec![
                Source {
                    kind: SourceKind::Screen,
                    id: "DP-1".into(),
                    label: "Entire Screen".into(),
                    detail: "2560 × 1440".into(),
                },
                Source {
                    kind: SourceKind::Window,
                    id: "w1".into(),
                    label: "Inbox".into(),
                    detail: "Mail".into(),
                },
            ],
            ..ScreencastState::default()
        }
    }

    #[test]
    fn nothing_shared_means_no_panel() {
        assert!(panel(&ScreencastState::default()).is_none());
    }

    #[test]
    fn the_current_source_is_selected_and_every_row_switches_the_cast() {
        let (spec, targets) = panel(&state()).expect("a cast is running");
        let selected: Vec<_> = spec
            .rows
            .iter()
            .filter_map(|row| match row {
                Row::Item(item) if item.selected => Some(item.label.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(selected, ["Inbox"]);
        let switches = targets
            .iter()
            .flatten()
            .filter(|target| matches!(target, Target::Switch { cast: 4, .. }))
            .count();
        assert_eq!(switches, 2);
        assert!(matches!(targets.last(), Some(Some(Target::Stop(4)))));
        assert!(matches!(
            spec.rows.last(),
            Some(Row::Action { label }) if label == "Stop Sharing"
        ));
    }
}

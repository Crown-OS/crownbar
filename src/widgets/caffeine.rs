//! Caffeine widget and the panel behind it.
//!
//! Clicking the pill toggles indefinitely, which is what the icon promises;
//! the panel is there for "stay awake for a while", which is the case where
//! forgetting to turn it off actually costs something.

use std::{sync::Arc, time::Duration};

use crate::{
    animation::Spring,
    services::{
        caffeine::{CaffeineCommand, CaffeineState, PRESETS},
        Services,
    },
    widgets::{
        popup::{Item, PanelBuilder, Row},
        AfterAction, BarWidget, Icon, PopupAction, PopupSpec, WidgetSlot,
    },
};

pub struct CaffeineWidget {
    caffeine: Arc<CaffeineState>,
    on: Spring,
    targets: Vec<Option<Target>>,
}

#[derive(Clone, Copy)]
enum Target {
    Indefinitely,
    For(Duration),
}

impl CaffeineWidget {
    pub fn new() -> Self {
        Self {
            caffeine: Arc::default(),
            on: Spring::new(0.0),
            targets: Vec::new(),
        }
    }
}

impl BarWidget for CaffeineWidget {
    fn id(&self) -> &'static str {
        "caffeine"
    }

    fn slot(&self) -> WidgetSlot {
        WidgetSlot::Right
    }

    /// A compositor with no idle-inhibit protocol gets no pill, rather than a
    /// switch that silently does nothing.
    fn visible(&self) -> bool {
        self.caffeine.availability.usable()
    }

    fn icon(&self) -> Icon {
        Icon::Caffeine {
            on: self.on.position,
        }
    }

    fn sync(&mut self, services: &Services) -> bool {
        let caffeine = services.caffeine.read();
        if Arc::ptr_eq(&caffeine, &self.caffeine) {
            return false;
        }
        self.caffeine = caffeine;
        self.on
            .set_target(if self.caffeine.active { 1.0 } else { 0.0 })
    }

    fn popup(&mut self, _services: &Services) -> Option<PopupSpec> {
        let state = self.caffeine.clone();
        let mut panel = PanelBuilder::new();
        panel.row(Row::Header {
            title: "Keep Awake".into(),
            toggle: Some(state.active),
        });

        let detail = match state.remaining() {
            Some(left) => remaining(left),
            None if state.active => "On".into(),
            None => "Off".into(),
        };
        panel.action(
            Item::new("Indefinitely")
                .icon(Icon::Caffeine { on: self.on.position })
                .detail(detail)
                .selected(state.active && state.until.is_none())
                .row(),
            Target::Indefinitely,
        );

        panel.row(Row::Separator);
        for (label, duration) in PRESETS {
            panel.action(Item::new(label).plain().row(), Target::For(duration));
        }

        let (spec, targets) = panel.finish();
        self.targets = targets;
        Some(spec)
    }

    fn on_popup(&mut self, action: PopupAction, services: &Services) -> AfterAction {
        match action {
            PopupAction::Toggle { on, .. } => {
                services.caffeine.send(CaffeineCommand::SetActive(on));
                AfterAction::Stay
            }
            PopupAction::Activate { row } => {
                match self.targets.get(row).copied().flatten() {
                    Some(Target::Indefinitely) => {
                        services.caffeine.send(CaffeineCommand::Toggle);
                        AfterAction::Stay
                    }
                    Some(Target::For(duration)) => {
                        services
                            .caffeine
                            .send(CaffeineCommand::SetActiveFor(duration));
                        AfterAction::Close
                    }
                    None => AfterAction::Stay,
                }
            }
            PopupAction::Slide { .. } | PopupAction::Page { .. } => AfterAction::Stay,
        }
    }

    fn tick_animation(&mut self, dt: f32) -> bool {
        if self.on.at_rest() {
            return false;
        }
        self.on.step(dt);
        !self.on.at_rest()
    }
}

/// "43 min left" / "3:12 left" — the same shape the battery uses for its own
/// countdown, so the two panels read alike.
fn remaining(left: Duration) -> String {
    let minutes = left.as_secs().div_ceil(60);
    match minutes / 60 {
        0 => format!("{minutes} min left"),
        hours => format!("{hours}:{:02} left", minutes % 60),
    }
}

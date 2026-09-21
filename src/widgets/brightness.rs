//! Brightness widget and the Display panel behind it.
//!
//! The pill shows the internal panel; the popup carries one slider per screen
//! the machine can dim, internal and DDC/CI alike. Levels are perceptual —
//! [`crate::services::brightness`] owns the curve — so the slider travels
//! evenly instead of doing nothing across its bottom half.

use std::sync::Arc;

use crate::{
    animation::Spring,
    services::{
        brightness::{BrightnessCommand, BrightnessState, DisplayId},
        link::{self, SettingsPane},
        Interest, Services,
    },
    widgets::{
        popup::{PanelBuilder, Row},
        AfterAction, BarWidget, Icon, PopupAction, PopupSpec, Rune, WidgetSlot,
    },
};

const PANEL_WIDTH: f32 = 288.0;

pub struct BrightnessWidget {
    brightness: Arc<BrightnessState>,
    level: Spring,
    targets: Vec<Option<Target>>,
}

#[derive(Clone)]
enum Target {
    Display(DisplayId),
    Settings,
}

impl BrightnessWidget {
    pub fn new() -> Self {
        Self {
            brightness: Arc::default(),
            level: Spring::new(0.0),
            targets: Vec::new(),
        }
    }
}

impl BarWidget for BrightnessWidget {
    fn id(&self) -> &'static str {
        "brightness"
    }

    fn slot(&self) -> WidgetSlot {
        WidgetSlot::Right
    }

    fn visible(&self) -> bool {
        self.brightness.availability.usable() && !self.brightness.displays.is_empty()
    }

    fn icon(&self) -> Icon {
        Icon::Brightness {
            level: self.level.position,
        }
    }

    fn sync(&mut self, services: &Services) -> bool {
        let brightness = services.brightness.read();
        if Arc::ptr_eq(&brightness, &self.brightness) {
            return false;
        }
        self.brightness = brightness;
        self.level.set_target(self.brightness.level());
        true
    }

    fn popup(&mut self, services: &Services) -> Option<PopupSpec> {
        if self.brightness.displays.is_empty() {
            return None;
        }
        // Re-reading a monitor over DDC is as slow as writing one, so it
        // happens only while this panel is up.
        services
            .brightness
            .send(BrightnessCommand::Interest(Interest::Panel));

        let state = self.brightness.clone();
        let mut panel = PanelBuilder::new(PANEL_WIDTH);
        panel.row(Row::Header {
            title: "Display".into(),
            toggle: None,
        });

        for display in &state.displays {
            // One screen needs no heading; several do, or the sliders are
            // indistinguishable.
            if state.is_multi() {
                panel.row(Row::Section {
                    title: display.label.clone(),
                    chevron: false,
                });
            }
            panel.action(
                Row::Slider {
                    icon: Icon::Rune(Rune::Sun),
                    value: display.level,
                },
                Target::Display(display.id.clone()),
            );
        }

        if let Some(why) = state.availability.reason() {
            panel.row(
                crate::widgets::popup::Item::new(why)
                    .plain()
                    .enabled(false)
                    .row(),
            );
        }

        panel.row(Row::Separator);
        panel.action(
            Row::Action {
                label: "Display Settings…".into(),
            },
            Target::Settings,
        );

        let (spec, targets) = panel.finish();
        self.targets = targets;
        Some(spec)
    }

    fn on_popup(&mut self, action: PopupAction, services: &Services) -> AfterAction {
        match action {
            // Sent as it happens: the service coalesces, so a drag costs one
            // write per round trip however fast the pointer moves.
            PopupAction::Slide { row, value, .. } => {
                if let Some(Target::Display(id)) = self.targets.get(row).cloned().flatten() {
                    services
                        .brightness
                        .send(BrightnessCommand::SetLevel { id, level: value });
                }
                AfterAction::Stay
            }
            PopupAction::Activate { row } => {
                if let Some(Target::Settings) = self.targets.get(row).cloned().flatten() {
                    if !link::open(SettingsPane::Display) {
                        log::info!("no display settings application installed");
                    }
                    return AfterAction::Close;
                }
                AfterAction::Stay
            }
            PopupAction::Toggle { .. } => AfterAction::Stay,
        }
    }

    fn popup_closed(&mut self, services: &Services) {
        services
            .brightness
            .send(BrightnessCommand::Interest(Interest::Idle));
    }

    fn tick_animation(&mut self, dt: f32) -> bool {
        if self.level.at_rest() {
            return false;
        }
        self.level.step(dt);
        !self.level.at_rest()
    }
}

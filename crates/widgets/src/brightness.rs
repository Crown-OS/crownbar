//! Brightness widget and the Display panel behind it.
//!
//! The pill shows the display of the bar's own output; the panel carries one
//! slider per display the compositor can dim, and one for all of them when
//! there are several. Levels are perceptual on both sides, so the slider
//! travels evenly instead of doing nothing across its bottom half.

use std::sync::Arc;

use crate::{
    AfterAction, BarWidget, Icon, PopupAction, PopupSpec, Rune,
    animation::Spring,
    popup::{Header, Item, PanelBuilder, Row},
};
use crownbar_integrations::{
    Services,
    brightness::{BrightnessCommand, BrightnessState, Display, DisplayId},
    link::{self, SettingsPane},
    nightlight::{NightLightCommand, NightLightState},
};

pub struct BrightnessWidget {
    brightness: Arc<BrightnessState>,
    nightlight: Arc<NightLightState>,
    output: Option<Arc<str>>,
    level: Spring,
    targets: Vec<Option<Target>>,
}

#[derive(Clone)]
enum Target {
    Display(DisplayId),
    AllDisplays,
    NightLight,
    Settings,
}

impl BrightnessWidget {
    pub fn new(output: Option<&str>) -> Self {
        Self {
            brightness: Arc::default(),
            nightlight: Arc::default(),
            output: output.map(Arc::from),
            level: Spring::new(0.0),
            targets: Vec::new(),
        }
    }

    fn own_display(&self) -> Option<&Display> {
        self.brightness.for_output(self.output.as_deref())
    }
}

impl BarWidget for BrightnessWidget {
    fn id(&self) -> &str {
        "brightness"
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
        // The night light changes nothing about the pill, so it never reports
        // a repaint of its own — but the panel is rebuilt from it, so the
        // widget has to be holding the newest one when that happens.
        self.nightlight = services.nightlight.read();
        let brightness = services.brightness.read();
        if Arc::ptr_eq(&brightness, &self.brightness) {
            return false;
        }
        self.brightness = brightness;
        let level = self.own_display().map_or(0.0, |display| display.level);
        self.level.set_target(level)
    }

    fn popup(&mut self, _services: &Services) -> Option<PopupSpec> {
        if self.brightness.displays.is_empty() {
            return None;
        }
        let state = self.brightness.clone();
        let mut panel = PanelBuilder::new();
        panel.row(Header::new("Display").row());

        let adjustable = state
            .displays
            .iter()
            .filter(|display| display.control.adjustable())
            .count();
        if adjustable > 1 {
            panel.row(Row::Section {
                title: "All Displays".into(),
                chevron: false,
            });
            panel.action(
                Row::Slider {
                    icon: Icon::Rune(Rune::Sun),
                    value: self.own_display().map_or(0.0, |display| display.level),
                    muted: None,
                },
                Target::AllDisplays,
            );
        }

        for display in &state.displays {
            // One screen needs no heading; several do, or the sliders are
            // indistinguishable.
            if state.is_multi() {
                panel.row(Row::Section {
                    title: display.label.clone(),
                    chevron: false,
                });
            }
            if display.control.adjustable() {
                panel.action(
                    Row::Slider {
                        icon: Icon::Rune(Rune::Sun),
                        value: display.level,
                        muted: None,
                    },
                    Target::Display(display.id.clone()),
                );
            } else {
                panel.row(
                    Item::new("Brightness cannot be changed")
                        .plain()
                        .enabled(false)
                        .row(),
                );
            }
        }

        if let Some(why) = state.availability.reason() {
            panel.row(crate::popup::Item::new(why).plain().enabled(false).row());
        }

        // A compositor with no gamma protocol gets no night-light rows, rather
        // than a switch that silently does nothing.
        if self.nightlight.availability.usable() {
            let night = self.nightlight.clone();
            panel.row(Row::Separator);
            panel.row(Header::new("Night Light").toggle(night.active).row());
            panel.action(
                Row::Slider {
                    icon: Icon::Rune(Rune::Moon),
                    value: night.warmth(),
                    muted: None,
                },
                Target::NightLight,
            );
            panel.row(
                Item::new("Colour Temperature")
                    .plain()
                    .detail(format!("{} K", night.kelvin))
                    .enabled(false)
                    .row(),
            );
        }

        panel.row(Row::Separator);
        panel.action(Row::action("Display Settings…"), Target::Settings);

        let (spec, targets) = panel.finish();
        self.targets = targets;
        Some(spec)
    }

    fn on_popup(&mut self, action: PopupAction, services: &Services) -> AfterAction {
        match action {
            // Sent as it happens: the service coalesces, so a drag costs one
            // write per round trip however fast the pointer moves.
            PopupAction::Slide { row, value, .. } => {
                match self.targets.get(row).cloned().flatten() {
                    Some(Target::Display(id)) => services
                        .brightness
                        .send(BrightnessCommand::SetLevel { id, level: value }),
                    Some(Target::AllDisplays) => {
                        services.brightness.send(BrightnessCommand::SetAll(value))
                    }
                    // Dragging the warmth slider turns the tint on, which is
                    // how most people will switch it on in the first place.
                    Some(Target::NightLight) => services.nightlight.send(
                        NightLightCommand::SetKelvin(NightLightState::kelvin_at(value)),
                    ),
                    _ => {}
                }
                AfterAction::Stay
            }
            PopupAction::Activate { row } => {
                if let Some(Target::Settings) = self.targets.get(row).cloned().flatten() {
                    link::open(SettingsPane::Display);
                    return AfterAction::Close;
                }
                AfterAction::Stay
            }
            PopupAction::Toggle { on, .. } => {
                services.nightlight.send(NightLightCommand::SetActive(on));
                AfterAction::Stay
            }
        }
    }

    /// A monitor's own buttons change its brightness behind the compositor's
    /// back, so the panel asks for a fresh reading each time it opens.
    fn popup_opened(&mut self, services: &Services) {
        services.brightness.send(BrightnessCommand::Refresh);
    }

    fn tick_animation(&mut self, dt: f32) -> bool {
        if self.level.at_rest() {
            return false;
        }
        self.level.step(dt);
        !self.level.at_rest()
    }
}

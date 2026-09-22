//! Weather widget and the forecast panel behind it.
//!
//! The pill is the temperature and the sky. Both *travel*: the glyph
//! cross-fades from the condition it was showing to the one that arrived, and
//! the day/night pose crosses over at dusk, so a change that happens while
//! nobody is looking is never caught as a jump when they look back.

use std::sync::Arc;

use crate::{
    animation::Spring,
    services::{
        weather::{Condition, WeatherCommand, WeatherState},
        Interest, Services,
    },
    widgets::{
        popup::{Item, PanelBuilder, Row},
        AfterAction, BarWidget, Icon, PopupAction, PopupSpec, WidgetSlot,
    },
};

pub struct WeatherWidget {
    weather: Arc<WeatherState>,
    label: String,
    /// What the glyph is fading *from*, and what it is fading *to*. `blend`
    /// carries it across; when it arrives the two are the same.
    from: Condition,
    to: Condition,
    blend: Spring,
    night: Spring,
    targets: Vec<Option<Target>>,
}

#[derive(Clone, Copy)]
enum Target {
    Refresh,
}

impl WeatherWidget {
    pub fn new() -> Self {
        Self {
            weather: Arc::default(),
            label: String::new(),
            from: Condition::default(),
            to: Condition::default(),
            blend: Spring::new(1.0),
            night: Spring::new(0.0),
            targets: Vec::new(),
        }
    }

    /// Start a fade towards `condition`.
    ///
    /// A change arriving mid-fade drops what was left of the old one rather
    /// than queueing it: the pill should show where the weather *is*, and two
    /// readings in a row this close together means the first was already
    /// wrong.
    fn travel_to(&mut self, condition: Condition) -> bool {
        if self.to == condition {
            return false;
        }
        self.from = self.to;
        self.to = condition;
        self.blend.position = 0.0;
        self.blend.velocity = 0.0;
        self.blend.set_target(1.0);
        true
    }
}

impl Default for WeatherWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl BarWidget for WeatherWidget {
    fn id(&self) -> &'static str {
        "weather"
    }

    fn slot(&self) -> WidgetSlot {
        WidgetSlot::Right
    }

    /// No reading, no pill. A degraded service is still showing the last one,
    /// so it keeps its place.
    fn visible(&self) -> bool {
        self.weather.current.is_some()
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn icon(&self) -> Icon {
        Icon::Weather {
            from: self.from,
            to: self.to,
            blend: self.blend.position,
            night: self.night.position,
        }
    }

    fn sync(&mut self, services: &Services) -> bool {
        let weather = services.weather.read();
        if Arc::ptr_eq(&weather, &self.weather) {
            return false;
        }
        self.weather = weather;
        let Some(current) = self.weather.current else {
            return false;
        };
        let label = degrees(current.celsius);
        let mut dirty = label != self.label;
        self.label = label;
        dirty |= self.travel_to(current.condition);
        dirty |= self
            .night
            .set_target(if current.night { 1.0 } else { 0.0 });
        dirty
    }

    fn popup(&mut self, services: &Services) -> Option<PopupSpec> {
        let state = self.weather.clone();
        let current = state.current?;
        // A panel that is being looked at is worth a faster refresh than one
        // nobody has open.
        services.weather.send(WeatherCommand::Interest(Interest::Panel));

        let mut panel = PanelBuilder::new();
        panel.row(Row::Header {
            title: state.place.as_deref().unwrap_or("Weather").to_string(),
            toggle: None,
        });
        panel.row(Row::Readout {
            primary: degrees(current.celsius),
            secondary: format!(
                "{} · Feels like {}",
                current.condition.label(),
                degrees(current.feels_like)
            ),
        });
        panel.row(Row::Separator);

        for day in &state.outlook {
            panel.row(
                Item::new(&day.day)
                    // Bare, not badged: the disc behind a panel glyph is there
                    // to give a monochrome outline a ground, and these carry
                    // their own colour.
                    .plain()
                    .icon(Icon::Weather {
                        from: day.condition,
                        to: day.condition,
                        blend: 1.0,
                        // A forecast is for a whole day, so it is always the
                        // daylit pose however late it is being read.
                        night: 0.0,
                    })
                    .detail(format!("{}  {}", degrees(day.high), degrees(day.low)))
                    .row(),
            );
        }

        panel.row(Row::Separator);
        panel.row(
            Item::new(format!(
                "Humidity {}%  ·  Wind {:.0} km/h",
                current.humidity, current.wind_kph
            ))
            .plain()
            .enabled(false)
            .row(),
        );
        if let Some(why) = state.availability.reason() {
            panel.row(Item::new(why).plain().enabled(false).row());
        }
        panel.action(
            Row::Action {
                label: "Refresh".into(),
            },
            Target::Refresh,
        );

        let (spec, targets) = panel.finish();
        self.targets = targets;
        Some(spec)
    }

    fn on_popup(&mut self, action: PopupAction, services: &Services) -> AfterAction {
        if let PopupAction::Activate { row } = action
            && let Some(Target::Refresh) = self.targets.get(row).copied().flatten()
        {
            services.weather.send(WeatherCommand::Refresh);
        }
        AfterAction::Stay
    }

    fn popup_closed(&mut self, services: &Services) {
        services.weather.send(WeatherCommand::Interest(Interest::Idle));
    }

    fn tick_animation(&mut self, dt: f32) -> bool {
        let mut alive = false;
        for spring in [&mut self.blend, &mut self.night] {
            if !spring.at_rest() {
                spring.step(dt);
                alive |= !spring.at_rest();
            }
        }
        // Once the fade lands the two ends are the same condition, so the
        // next change has somewhere to come from.
        if self.blend.at_rest() && self.from != self.to {
            self.from = self.to;
        }
        alive
    }
}

/// Rounded to the whole degree a pill has room for, and never "-0°".
fn degrees(celsius: f32) -> String {
    let rounded = celsius.round();
    format!("{}°", if rounded == 0.0 { 0.0 } else { rounded } as i32)
}

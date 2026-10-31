//! Clock widget and the calendar panel behind it.
//!
//! The pill is the date and time. The panel is the same reading written large,
//! over a month grid that can be paged — which is the one thing the pill
//! cannot say and the reason to click it. The grid pages itself, and starts
//! over on today's month each time the panel opens.

use chrono::{Datelike, Local};
use crownui::kit::CalendarDate;

use crate::{
    AfterAction, BarWidget, Icon, PopupAction, PopupSpec, Rune,
    popup::{Item, PanelBuilder, Row},
};
use crownbar_integrations::{
    Services,
    link::{self, SettingsPane},
    live::{LiveCommand, StopwatchRequest, TimerRequest},
};

pub struct ClockWidget {
    label: String,
    targets: Vec<Option<Target>>,
}

#[derive(Clone, Copy)]
enum Target {
    Stopwatch,
    Timer(u32),
    Settings,
}

/// The countdowns the panel offers in one click, in minutes.
const TIMER_PRESETS: [u32; 3] = [5, 15, 25];

impl ClockWidget {
    pub fn new() -> Self {
        let mut widget = Self {
            label: String::new(),
            targets: Vec::new(),
        };
        widget.refresh();
        widget
    }

    fn refresh(&mut self) -> bool {
        let label = Local::now().format("%a %e %b %H:%M").to_string();
        let changed = label != self.label;
        self.label = label;
        changed
    }
}

impl Default for ClockWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl BarWidget for ClockWidget {
    fn id(&self) -> &str {
        "clock"
    }

    fn update(&mut self) -> bool {
        self.refresh()
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn popup(&mut self, _services: &Services) -> Option<PopupSpec> {
        let now = Local::now();
        let mut panel = PanelBuilder::new();
        panel.row(Row::Readout {
            primary: now.format("%H:%M").to_string(),
            secondary: now.format("%A, %e %B %Y").to_string(),
        });
        panel.row(Row::Separator);
        let today = now.date_naive();
        panel.row(Row::Calendar(CalendarDate::new(
            today.year(),
            today.month() as u8,
            today.day() as u8,
        )));
        panel.row(Row::Separator);
        panel.action(
            Item::new("Start Stopwatch")
                .icon(Icon::Rune(Rune::Stopwatch))
                .row(),
            Target::Stopwatch,
        );
        for minutes in TIMER_PRESETS {
            panel.action(
                Item::new(format!("Timer for {minutes} Minutes"))
                    .icon(Icon::Rune(Rune::Clock))
                    .row(),
                Target::Timer(minutes),
            );
        }
        panel.row(Row::Separator);
        panel.action(
            Row::Action {
                label: "Date & Time Settings…".into(),
            },
            Target::Settings,
        );

        let (spec, targets) = panel.finish();
        self.targets = targets;
        Some(spec)
    }

    fn on_popup(&mut self, action: PopupAction, services: &Services) -> AfterAction {
        let PopupAction::Activate { row } = action else {
            return AfterAction::Stay;
        };
        match self.targets.get(row).copied().flatten() {
            Some(Target::Stopwatch) => services
                .live
                .send(LiveCommand::Stopwatch(StopwatchRequest::Start)),
            Some(Target::Timer(minutes)) => {
                services.live.send(LiveCommand::Timer(TimerRequest::Start {
                    seconds: minutes * 60,
                    label: None,
                }))
            }
            Some(Target::Settings) => link::open(SettingsPane::DateTime),
            None => return AfterAction::Stay,
        }
        AfterAction::Close
    }

    /// The panel restates itself every second so the readout is the time and
    /// not the time it opened at.
    fn popup_poll(&mut self, slow: bool) -> bool {
        slow
    }
}

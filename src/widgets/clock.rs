//! Clock widget and the calendar panel behind it.
//!
//! The pill is the date and time. The panel is the same reading written large,
//! over a month grid that can be paged — which is the one thing the pill
//! cannot say and the reason to click it.

use chrono::{DateTime, Datelike, Local, NaiveDate};

use crate::{
    services::{
        link::{self, SettingsPane},
        Services,
    },
    util::calendar,
    widgets::{
        popup::{Day, Month, PanelBuilder, Row},
        AfterAction, BarWidget, PopupAction, PopupSpec, WidgetSlot,
    },
};

pub struct ClockWidget {
    label: String,
    /// Months away from today the panel is showing. Reset when it closes: a
    /// panel reopened a week later should not still be in March.
    page: i32,
    targets: Vec<Option<Target>>,
}

#[derive(Clone, Copy)]
enum Target {
    Calendar,
    Settings,
}

impl ClockWidget {
    pub fn new() -> Self {
        let mut widget = Self {
            label: String::new(),
            page: 0,
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
    fn id(&self) -> &'static str {
        "clock"
    }

    fn slot(&self) -> WidgetSlot {
        WidgetSlot::Left
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
        panel.action(
            Row::Calendar(Box::new(month_of(now, self.page))),
            Target::Calendar,
        );
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

    fn on_popup(&mut self, action: PopupAction, _services: &Services) -> AfterAction {
        match action {
            PopupAction::Page { row, months } => {
                if let Some(Target::Calendar) = self.targets.get(row).copied().flatten() {
                    self.page += months;
                }
                AfterAction::Stay
            }
            PopupAction::Activate { row } => {
                if let Some(Target::Settings) = self.targets.get(row).copied().flatten() {
                    if !link::open(SettingsPane::DateTime) {
                        log::info!("no date and time settings application installed");
                    }
                    return AfterAction::Close;
                }
                AfterAction::Stay
            }
            PopupAction::Toggle { .. } | PopupAction::Slide { .. } => AfterAction::Stay,
        }
    }

    /// The panel restates itself every second so the readout is the time and
    /// not the time it opened at.
    fn popup_poll(&mut self, slow: bool) -> bool {
        slow
    }

    fn popup_closed(&mut self, _services: &Services) {
        self.page = 0;
    }
}

/// The month `page` months from `now`, with today marked wherever it falls.
fn month_of(now: DateTime<Local>, page: i32) -> Month {
    let today = now.date_naive();
    let anchor = calendar::shift_months(today.with_day(1).unwrap_or(today), page);
    Month {
        title: anchor.format("%B %Y").to_string(),
        days: calendar::grid(anchor).map(|date| cell(date, anchor, today)).collect(),
    }
}

fn cell(date: NaiveDate, anchor: NaiveDate, today: NaiveDate) -> Day {
    Day {
        day: date.day() as u8,
        in_month: date.month() == anchor.month() && date.year() == anchor.year(),
        today: date == today,
        weekend: calendar::is_weekend(date),
    }
}

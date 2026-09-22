//! Notifications widget and the panel behind it.
//!
//! Clicking the pill toggles crownotify's notification centre, which is what
//! the bell promises. The panel is there for the two things a centre cannot
//! ask for itself — silencing everything, and clearing everything — and it is
//! reached the way every other panel is.

use std::sync::Arc;

use crate::{
    animation::Spring,
    services::{
        notifications::{NotificationsCommand, NotificationsState},
        Services,
    },
    widgets::{
        popup::{Item, PanelBuilder, Row},
        AfterAction, BarWidget, Icon, PopupAction, PopupSpec, WidgetSlot,
    },
};

pub struct NotificationsWidget {
    notifications: Arc<NotificationsState>,
    open: Spring,
    silenced: Spring,
    targets: Vec<Option<Target>>,
}

#[derive(Clone, Copy)]
enum Target {
    Center,
    DismissAll,
}

impl NotificationsWidget {
    pub fn new() -> Self {
        Self {
            notifications: Arc::default(),
            open: Spring::new(0.0),
            silenced: Spring::new(0.0),
            targets: Vec::new(),
        }
    }
}

impl Default for NotificationsWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl BarWidget for NotificationsWidget {
    fn id(&self) -> &'static str {
        "notifications"
    }

    fn slot(&self) -> WidgetSlot {
        WidgetSlot::Right
    }

    /// With crownotify absent the pill leaves the bar rather than offering a
    /// bell that rings nothing.
    fn visible(&self) -> bool {
        self.notifications.availability.usable()
    }

    fn icon(&self) -> Icon {
        Icon::Notifications {
            open: self.open.position,
            silenced: self.silenced.position,
        }
    }

    fn sync(&mut self, services: &Services) -> bool {
        let notifications = services.notifications.read();
        if Arc::ptr_eq(&notifications, &self.notifications) {
            return false;
        }
        self.notifications = notifications;
        self.open
            .set_target(if self.notifications.center_open { 1.0 } else { 0.0 })
            | self.silenced.set_target(if self.notifications.do_not_disturb {
                1.0
            } else {
                0.0
            })
    }

    fn popup(&mut self, _services: &Services) -> Option<PopupSpec> {
        let state = self.notifications.clone();
        let mut panel = PanelBuilder::new();
        panel.row(Row::Header {
            title: "Notifications".into(),
            toggle: Some(!state.do_not_disturb),
        });

        panel.action(
            Item::new("Notification Center")
                .icon(Icon::Notifications {
                    open: self.open.position,
                    silenced: self.silenced.position,
                })
                .detail(if state.center_open { "Open" } else { "Closed" })
                .selected(state.center_open)
                .row(),
            Target::Center,
        );

        panel.row(Row::Separator);
        panel.action(
            Row::Action {
                label: "Clear All Notifications".into(),
            },
            Target::DismissAll,
        );

        let (spec, targets) = panel.finish();
        self.targets = targets;
        Some(spec)
    }

    fn on_popup(&mut self, action: PopupAction, services: &Services) -> AfterAction {
        match action {
            // The switch reads "notifications on", so it is Do Not Disturb
            // inverted — a switch the user turns *off* to go quiet.
            PopupAction::Toggle { on, .. } => {
                services
                    .notifications
                    .send(NotificationsCommand::SetDoNotDisturb(!on));
                AfterAction::Stay
            }
            PopupAction::Activate { row } => match self.targets.get(row).copied().flatten() {
                Some(Target::Center) => {
                    services.notifications.send(NotificationsCommand::ToggleCenter);
                    AfterAction::Close
                }
                Some(Target::DismissAll) => {
                    services.notifications.send(NotificationsCommand::DismissAll);
                    AfterAction::Close
                }
                None => AfterAction::Stay,
            },
            PopupAction::Slide { .. } | PopupAction::Page { .. } => AfterAction::Stay,
        }
    }

    fn tick_animation(&mut self, dt: f32) -> bool {
        let mut alive = false;
        for spring in [&mut self.open, &mut self.silenced] {
            if !spring.at_rest() {
                spring.step(dt);
                alive |= !spring.at_rest();
            }
        }
        alive
    }
}

//! Notifications widget.
//!
//! Clicking the pill toggles crownotify's notification centre, which is what
//! the bell promises. Silencing and clearing live in the centre itself, so the
//! pill has no panel of its own.

use std::sync::Arc;

use crate::{
    animation::Spring,
    services::{
        Services,
        notifications::{NotificationsCommand, NotificationsState},
    },
    widgets::{BarWidget, Icon},
};

pub struct NotificationsWidget {
    notifications: Arc<NotificationsState>,
    open: Spring,
    silenced: Spring,
}

impl NotificationsWidget {
    pub fn new() -> Self {
        Self {
            notifications: Arc::default(),
            open: Spring::new(0.0),
            silenced: Spring::new(0.0),
        }
    }
}

impl Default for NotificationsWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl BarWidget for NotificationsWidget {
    fn id(&self) -> &str {
        "notifications"
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
        self.open.set_target(if self.notifications.center_open {
            1.0
        } else {
            0.0
        }) | self
            .silenced
            .set_target(if self.notifications.do_not_disturb {
                1.0
            } else {
                0.0
            })
    }

    /// The bell only moves once crownotify reports the centre's new state, so
    /// the click itself changes nothing on the bar.
    fn on_click(&mut self, services: &Services) -> bool {
        services
            .notifications
            .send(NotificationsCommand::ToggleCenter);
        false
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

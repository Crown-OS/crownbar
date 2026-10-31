//! How a service reports on itself.
//!
//! Every snapshot carries an [`Availability`]. It is a runtime value, not a
//! startup one: BlueZ can come up after the bar, a dongle can be plugged in,
//! and a widget that was hidden starts drawing without a restart.

use std::sync::Arc;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Availability {
    /// The backend is connecting. Widgets stay hidden rather than flash an
    /// empty panel during the first few milliseconds.
    #[default]
    Starting,
    Ready,
    /// Working, but not completely — no i2c permission for external monitors,
    /// BlueZ with no battery provider.
    Degraded(Arc<str>),
    Unavailable(Arc<str>),
}

impl Availability {
    /// Whether the service can answer at all. Drives `BarWidget::visible`.
    pub fn usable(&self) -> bool {
        matches!(self, Self::Ready | Self::Degraded(_))
    }

    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::Degraded(why) | Self::Unavailable(why) => Some(why),
            _ => None,
        }
    }
}

/// Why a command did not take effect, in the terms a panel row can render.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    NotAuthorized,
    NotFound,
    Busy,
    Timeout,
    Unsupported,
    Backend,
}

impl ErrorKind {
    /// Wording for the panel. The detail goes to the log, not to the pixels.
    pub fn summary(self) -> &'static str {
        match self {
            Self::NotAuthorized => "Not allowed",
            Self::NotFound => "No longer available",
            Self::Busy => "Busy, try again",
            Self::Timeout => "Timed out",
            Self::Unsupported => "Not supported",
            Self::Backend => "Something went wrong",
        }
    }
}

/// The last command that failed, kept in the snapshot so the panel can say so
/// instead of silently snapping a switch back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub kind: ErrorKind,
    pub detail: Arc<str>,
}

impl Failure {
    pub fn new(kind: ErrorKind, detail: impl Into<Arc<str>>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }
}

/// Whether the bar is currently showing what a service produces.
///
/// Backends poll faster, scan, and hold subscriptions only while a panel is
/// up; an idle service should cost nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Interest {
    #[default]
    Idle,
    Panel,
}

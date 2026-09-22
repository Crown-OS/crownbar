//! Everything the bar knows about the machine.
//!
//! Each service is a concrete handle, not an implementation of a shared trait:
//! the domains have little in common beyond how they are wired, and that part
//! lives in [`bus`]. A service is a snapshot type, a command type, and a
//! backend future — nothing else is required of it.
//!
//! The shape is always the same. A backend runs on the tokio runtime (or on a
//! thread of its own, for PipeWire), publishes snapshots into a `watch`, and
//! wakes the event loop; the bar reads the newest snapshot when it repaints
//! and pushes commands back down an unbounded pipe. No service call ever runs
//! on the event loop thread.

pub mod audio;
pub mod battery;
pub mod brightness;
pub mod caffeine;
pub mod bluetooth;
pub mod network;
pub mod nightlight;
pub mod notifications;
pub mod power;
pub mod stats;
pub mod weather;

pub mod link;

mod bus;
pub mod rfkill;
mod hub;
mod runtime;
mod status;

pub use bus::Wake;
pub use hub::Services;
pub use link::SettingsPane;
pub use status::{Availability, ErrorKind, Failure, Interest};

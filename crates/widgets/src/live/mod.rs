//! One chip beside the clock per live activity.
//!
//! The bar holds [`MAX_CHIPS`] of these; chip `rank` shows whatever the
//! board seated there, and takes no room while nothing is. The text and the
//! progress ring follow the clock on the bar's once-a-second tick, so a
//! running activity costs no frames between ticks. The one animation is the
//! halo, raised for a few beats when an activity starts asking for attention.

mod popup;

use std::{f32::consts::TAU, sync::Arc, time::Instant};

use crate::{AfterAction, BarWidget, Icon, PopupAction, PopupSpec};
use crownbar_integrations::{
    Services,
    live::{Accent, Activity, LiveCommand, MAX_CHIPS, Urgency},
};

pub use popup::Target;

/// One accessible name per seat, so each chip can be told apart.
const NAMES: [&str; MAX_CHIPS] = ["live-1", "live-2", "live-3", "live-4"];

/// How long the halo beats on entering attention, and how fast.
const PULSE_SECONDS: f32 = 3.0;
const PULSE_HZ: f32 = 1.0;

pub struct LiveChip {
    rank: usize,
    activity: Option<Arc<Activity>>,
    label: String,
    scratch: String,
    /// Seconds into the halo's beat, while it is beating.
    pulse_clock: Option<f32>,
    targets: Vec<Option<Target>>,
}

impl LiveChip {
    pub fn new(rank: usize) -> Self {
        Self {
            rank,
            activity: None,
            label: String::new(),
            scratch: String::new(),
            pulse_clock: None,
            targets: Vec::new(),
        }
    }

    /// One chip for every seat the board has.
    pub fn all() -> impl Iterator<Item = Self> {
        (0..NAMES.len()).map(Self::new)
    }

    /// Rewrite the label as of now; returns whether it changed. Written into
    /// a second buffer and swapped, so a tick allocates nothing.
    fn relabel(&mut self) -> bool {
        match &self.activity {
            Some(activity) => activity.write_label(&mut self.scratch, Instant::now()),
            None => self.scratch.clear(),
        }
        let changed = self.scratch != self.label;
        std::mem::swap(&mut self.scratch, &mut self.label);
        changed
    }

    /// Whether the caption and the ring move by themselves.
    fn running(&self) -> (bool, bool) {
        self.activity.as_ref().map_or((false, false), |activity| {
            (
                activity.caption.ticks(),
                activity.progress.is_some_and(|progress| progress.moves()),
            )
        })
    }

    fn pulse(&self) -> f32 {
        self.pulse_clock
            .map_or(0.0, |seconds| 0.5 - 0.5 * (seconds * PULSE_HZ * TAU).cos())
    }
}

impl BarWidget for LiveChip {
    fn id(&self) -> &str {
        NAMES[self.rank]
    }

    fn sync(&mut self, services: &Services) -> bool {
        let live = services.live.read();
        let next = live.chip(self.rank).cloned();
        let same = match (&self.activity, &next) {
            (Some(held), Some(next)) => Arc::ptr_eq(held, next),
            (None, None) => true,
            _ => false,
        };
        if same {
            return false;
        }
        let was_urgent = self
            .activity
            .as_ref()
            .is_some_and(|held| held.urgency == Urgency::Attention);
        let urgent = next
            .as_ref()
            .is_some_and(|next| next.urgency == Urgency::Attention);
        if urgent && !was_urgent {
            self.pulse_clock = Some(0.0);
        }
        self.activity = next;
        self.relabel();
        true
    }

    fn update(&mut self) -> bool {
        let (caption, ring) = self.running();
        (caption && self.relabel()) || ring
    }

    fn visible(&self) -> bool {
        self.activity.is_some()
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn icon(&self) -> Icon {
        let Some(activity) = &self.activity else {
            return Icon::None;
        };
        Icon::Live {
            glyph: activity.glyph,
            accent: activity.accent,
            progress: activity
                .progress
                .map(|progress| progress.at(Instant::now())),
            pulse: self.pulse(),
        }
    }

    fn accent(&self) -> Option<Accent> {
        self.activity.as_ref().map(|activity| activity.accent)
    }

    fn popup(&mut self, _services: &Services) -> Option<PopupSpec> {
        let (spec, targets) = popup::panel(self.activity.as_deref()?, Instant::now());
        self.targets = targets;
        Some(spec)
    }

    fn on_popup(&mut self, action: PopupAction, services: &Services) -> AfterAction {
        let Some(activity) = &self.activity else {
            return AfterAction::Close;
        };
        let key = &activity.key;
        match (
            action,
            self.targets.get(popup::row(action)).copied().flatten(),
        ) {
            (PopupAction::Activate { .. }, Some(Target::Control(control))) => {
                services.live.send(LiveCommand::control(key, control));
                popup::after(control)
            }
            (
                PopupAction::Slide {
                    value,
                    commit: true,
                    ..
                },
                Some(Target::Seek),
            ) => {
                services.live.send(LiveCommand::seek(key, value));
                AfterAction::Stay
            }
            _ => AfterAction::Stay,
        }
    }

    fn popup_poll(&mut self, slow: bool) -> bool {
        let (caption, ring) = self.running();
        slow && (caption || ring)
    }

    fn tick_animation(&mut self, dt: f32) -> bool {
        let Some(seconds) = self.pulse_clock.as_mut() else {
            return false;
        };
        *seconds += dt;
        if *seconds >= PULSE_SECONDS {
            self.pulse_clock = None;
        }
        true
    }
}

//! Spring-driven scalar interpolation. A damped harmonic oscillator per value,
//! integrated with fixed sub-steps so the integrator stays stable independent
//! of the frame rate the compositor wakes us at.
//!
//! Each animated value carries its own position + velocity, a target, and the
//! [`SpringProfile`] that decides how it gets there. Step the value with
//! [`Spring::step`] when the frame ticks; check [`Spring::at_rest`] to know
//! when to stop requesting frames.
//!
//! The same profiles drive the transitions crownui runs on its render thread
//! (a pill's hover wash, rows cross-fading), through [`SpringProfile::curve`].

/// Fixed integrator sub-step.
const SUBSTEP: f32 = 1.0 / 240.0;
/// Largest dt the integrator will accept in one tick.
const MAX_DT: f32 = 1.0 / 30.0;
/// Settle thresholds.
const EPSILON_POS: f32 = 0.0005;
const EPSILON_VEL: f32 = 0.01;

/// Stiffness and damping, as a named feel.
///
/// Damping is spelled out rather than derived because `sqrt` is not const; the
/// ratio each one works out to is in its doc comment, since that — not the
/// absolute number — is what decides whether a value overshoots.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpringProfile {
    pub stiffness: f32,
    pub damping: f32,
}

impl SpringProfile {
    /// The profile as crownui describes a spring: by its period and how much
    /// of critical damping it has, rather than by stiffness and damping.
    pub fn curve(self) -> crownui::prelude::Spring {
        let natural = self.stiffness.sqrt();
        crownui::prelude::Spring::new(
            std::f32::consts::TAU / natural,
            self.damping / (2.0 * natural),
        )
    }

    /// Critically damped, settles in roughly 200 ms. What a toggle, a slider
    /// or a hover wash wants: no overshoot, no delay.
    pub const SNAPPY: Self = Self {
        stiffness: 320.0,
        damping: 35.78,
    };
    /// Ratio 0.9 over a longer travel — a shape changing size arrives with the
    /// barest settle rather than stopping dead at its new dimensions.
    pub const GLIDE: Self = Self {
        stiffness: 260.0,
        damping: 29.0,
    };
    /// Ratio 0.68: one visible overshoot, then still. A light object thrown
    /// against its stop and caught by it.
    pub const BOUNCE: Self = Self {
        stiffness: 300.0,
        damping: 23.6,
    };
    /// Ratio 1.1 and stiff: a fall that is over before it can bounce, which is
    /// what a thing being put away does and a thing being taken out does not.
    pub const DROP: Self = Self {
        stiffness: 420.0,
        damping: 45.1,
    };
}

#[derive(Debug, Clone, Copy)]
pub struct Spring {
    pub position: f32,
    pub velocity: f32,
    pub target: f32,
    pub profile: SpringProfile,
}

impl Spring {
    pub const fn new(value: f32) -> Self {
        Self::with_profile(value, SpringProfile::SNAPPY)
    }

    pub const fn with_profile(value: f32, profile: SpringProfile) -> Self {
        Self {
            position: value,
            velocity: 0.0,
            target: value,
            profile,
        }
    }

    /// Returns whether the target actually moved — what a caller uses to
    /// decide a repaint is due.
    pub fn set_target(&mut self, target: f32) -> bool {
        let moved = self.target != target;
        self.target = target;
        moved
    }

    pub fn set_profile(&mut self, profile: SpringProfile) {
        self.profile = profile;
    }

    /// Throw the value at its target with a starting velocity. A spring
    /// released from rest covers a few percent of its travel in the first
    /// frame, which reads as a hesitation; a hand that pushes something away
    /// has already let go of it by then.
    pub fn nudge(&mut self, velocity: f32) {
        self.velocity = velocity;
    }

    /// Integrate the spring forward by `dt` seconds (clamped to MAX_DT).
    pub fn step(&mut self, dt: f32) {
        let mut remaining = dt.min(MAX_DT);
        while remaining > 0.0 {
            let h = remaining.min(SUBSTEP);
            let accel = -self.profile.stiffness * (self.position - self.target)
                - self.profile.damping * self.velocity;
            self.velocity += accel * h;
            self.position += self.velocity * h;
            remaining -= h;
        }
    }

    pub fn at_rest(&self) -> bool {
        (self.position - self.target).abs() < EPSILON_POS && self.velocity.abs() < EPSILON_VEL
    }

    pub fn snap_to_target(&mut self) {
        self.position = self.target;
        self.velocity = 0.0;
    }
}

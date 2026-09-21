//! Spring-driven scalar interpolation. A single critically-damped harmonic
//! oscillator, integrated with fixed sub-steps so the integrator stays stable
//! independent of the frame rate the compositor wakes us at.
//!
//! Each animated value carries its own position + velocity, and a target. Step
//! the value with [`Spring::step`] when the frame ticks; check
//! [`Spring::at_rest`] to know when to stop requesting frames.

use std::time::Instant;

/// Stiffness — higher = snappier. 320 settles in roughly 200 ms.
const STIFFNESS: f32 = 320.0;
/// Damping ratio of ~1.0 (critically damped) — no overshoot, gentle ease-out.
/// damping = 2 * sqrt(STIFFNESS) ≈ 35.78
const DAMPING: f32 = 35.78;
/// Fixed integrator sub-step.
const SUBSTEP: f32 = 1.0 / 240.0;
/// Largest dt the integrator will accept in one tick.
const MAX_DT: f32 = 1.0 / 30.0;
/// Settle thresholds.
const EPSILON_POS: f32 = 0.0005;
const EPSILON_VEL: f32 = 0.01;

#[derive(Debug, Clone, Copy)]
pub struct Spring {
    pub position: f32,
    pub velocity: f32,
    pub target: f32,
}

impl Spring {
    pub const fn new(value: f32) -> Self {
        Self {
            position: value,
            velocity: 0.0,
            target: value,
        }
    }

    pub fn set_target(&mut self, target: f32) {
        self.target = target;
    }

    /// Integrate the spring forward by `dt` seconds (clamped to MAX_DT).
    pub fn step(&mut self, dt: f32) {
        let mut remaining = dt.min(MAX_DT);
        while remaining > 0.0 {
            let h = remaining.min(SUBSTEP);
            let accel = -STIFFNESS * (self.position - self.target) - DAMPING * self.velocity;
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

/// Wall-clock dt source for animation loops. `tick` returns the delta since
/// the previous call (or a 60-Hz frame on first call), clamped to MAX_DT.
pub struct Clock {
    last: Option<Instant>,
}

impl Clock {
    pub const fn new() -> Self {
        Self { last: None }
    }

    pub fn reset(&mut self) {
        self.last = None;
    }

    pub fn tick(&mut self) -> f32 {
        let now = Instant::now();
        let dt = self
            .last
            .map(|t| now.duration_since(t).as_secs_f32())
            .unwrap_or(1.0 / 60.0)
            .min(MAX_DT);
        self.last = Some(now);
        dt
    }
}

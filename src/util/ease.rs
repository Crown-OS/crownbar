//! Easing curves, for the ramps that are shaped rather than sprung.

/// Hermite smoothstep: flat at both ends, steepest in the middle.
///
/// What a straight ramp is missing. The eye finds the corner where a linear
/// ramp meets its endpoint and reads it as a line — a Mach band — which is
/// exactly what a background fading out cannot afford.
pub fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// [`smoothstep`] the other way up: 1 at `t` = 0, 0 by `t` = 1.
pub fn fade_out(t: f32) -> f32 {
    1.0 - smoothstep(t)
}

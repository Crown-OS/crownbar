//! How a popup panel moves: out of its pill, between one panel and the next,
//! and back out of sight.
//!
//! Everything here is a spring, and every spring is integrated rather than
//! sampled off a curve — so a panel interrupted halfway out carries the
//! velocity it already had into wherever it is sent next, the way a real thing
//! does and a keyframed one cannot.
//!
//! Three motions, each modelled on something physical:
//!
//! * **Out of the pill** — the panel is thrown from the icon it belongs to at
//!   [`BIRTH_SCALE`], falls the last [`DROP`] px onto its resting place and
//!   overshoots once ([`SpringProfile::BOUNCE`]) before settling, like a drawer
//!   pushed open against its stop.
//! * **Squash and stretch** — while it is moving it stretches along its travel
//!   and narrows across it, in proportion to the spring's own velocity, and
//!   gives that back as it stops. A body under acceleration deforms; a body at
//!   rest does not, which is why this falls out of the velocity for free.
//! * **Away** — [`SpringProfile::DROP`] with a shove ([`CLOSE_KICK`]): stiff,
//!   over-damped and already moving on the first frame, so it leaves rather
//!   than deflates.
//!
//! Switching panels never replays any of that. The shape itself travels to the
//! new one's bounds while the two sets of rows cross-fade inside it, so a click
//! on the next pill reads as the same object changing rather than one object
//! being destroyed and another built.

use vello::kurbo::{Affine, Point, Rect};

use crate::animation::{Spring, SpringProfile};

/// Fraction of its final size the panel is thrown from.
const BIRTH_SCALE: f64 = 0.88;
/// How far above its resting place it starts, in px.
const DROP: f64 = 10.0;
/// Velocity the close is shoved off with, in units of the 0..1 progress/second.
const CLOSE_KICK: f32 = -2.2;
/// Seconds of the open spring's velocity taken as stretch along the travel.
const STRETCH: f32 = 0.010;
/// Ceiling on that, so a spring caught at full speed deforms rather than tears.
const MAX_STRETCH: f64 = 0.035;
/// How much of the stretch comes back off the other axis. Not 1.0: the panel
/// keeps most of its area the way a dropped ball does, not all of it.
const VOLUME_PRESERVATION: f64 = 0.55;
/// Opacity leads the motion — solid by the time it is half out, which reads as
/// quicker than it is.
const ALPHA_LEAD: f32 = 2.2;
/// Below this the panel is gone: nothing drawn, no input region, no material.
const HIDDEN: f32 = 0.001;

/// What a frame of the animation works out to.
pub struct Frame {
    /// The morphing shape, in surface px, before the spring-out transform.
    pub bounds: Rect,
    /// Spring-out, drop and squash, about the pill the panel hangs from.
    pub transform: Affine,
    pub alpha: f32,
    /// How much of the incoming panel's rows show; the outgoing ones take the
    /// rest. 1.0 whenever nothing is being replaced.
    pub crossfade: f32,
}

impl Frame {
    /// The shape as it lands on screen, transform included.
    pub fn visible(&self) -> Rect {
        self.transform.transform_rect_bbox(self.bounds)
    }
}

/// The springs behind one popup surface.
pub struct PanelMotion {
    /// 0 = dismissed, 1 = fully out, and past 1 while it overshoots.
    open: Spring,
    /// The shape's four edges, so a panel of another size is travelled to
    /// rather than cut to.
    edges: [Spring; 4],
    /// Whether the next [`reshape`](Self::reshape) lands outright. A shape
    /// with nothing on screen has nothing to travel from.
    unplaced: bool,
    fade: Spring,
}

impl PanelMotion {
    pub const fn new() -> Self {
        Self {
            open: Spring::with_profile(0.0, SpringProfile::BOUNCE),
            edges: [Spring::with_profile(0.0, SpringProfile::GLIDE); 4],
            unplaced: true,
            fade: Spring::new(1.0),
        }
    }

    /// Throw the panel out of its pill. `replacing` is whether another panel
    /// is already on screen — then the shape morphs and the rows cross-fade
    /// instead of the whole thing being launched again.
    pub fn open(&mut self, replacing: bool) {
        self.unplaced = !replacing;
        if replacing {
            self.fade.reset(0.0);
            self.fade.set_target(1.0);
            return;
        }
        self.open.set_profile(SpringProfile::BOUNCE);
        self.open.set_target(1.0);
        self.fade.reset(1.0);
    }

    pub fn close(&mut self) {
        self.open.set_profile(SpringProfile::DROP);
        self.open.set_target(0.0);
        self.open.nudge(CLOSE_KICK);
    }

    /// Aim the shape at `bounds`.
    pub fn reshape(&mut self, bounds: Rect) {
        let target = [bounds.x0, bounds.y0, bounds.x1, bounds.y1];
        for (edge, value) in self.edges.iter_mut().zip(target) {
            edge.set_target(value as f32);
            if self.unplaced {
                edge.snap_to_target();
            }
        }
        self.unplaced = false;
    }

    pub fn step(&mut self, dt: f32) -> bool {
        let mut busy = false;
        for spring in [&mut self.open, &mut self.fade].into_iter().chain(&mut self.edges) {
            if !spring.at_rest() {
                spring.step(dt);
                busy = true;
            }
        }
        busy
    }

    /// Nothing on screen and nothing on its way there.
    pub fn invisible(&self) -> bool {
        self.open.position <= HIDDEN && self.open.at_rest()
    }

    /// Whether the panel has stopped moving, and so is worth hit-testing.
    pub fn at_rest(&self) -> bool {
        self.open.at_rest() && self.edges.iter().all(Spring::at_rest)
    }

    /// Resolve the frame, hanging from the pill centred on `anchor_x`.
    pub fn frame(&self, anchor_x: f64) -> Frame {
        let [x0, y0, x1, y1] = self.edges.map(|edge| edge.position as f64);
        let bounds = Rect::new(x0, y0, x1, y1);

        let progress = self.open.position as f64;
        let scale = BIRTH_SCALE + (1.0 - BIRTH_SCALE) * progress;
        // Positive while it is coming out, negative while it is settling back
        // against its own overshoot — so the panel stretches into the motion
        // and squashes as the motion stops.
        let stretch =
            ((self.open.velocity * STRETCH) as f64).clamp(-MAX_STRETCH, MAX_STRETCH);
        let pivot = Point::new(anchor_x.clamp(bounds.x0, bounds.x1), bounds.y0);

        Frame {
            bounds,
            transform: Affine::translate((0.0, -DROP * (1.0 - progress)))
                * about(
                    pivot,
                    Affine::scale_non_uniform(
                        scale * (1.0 - stretch * VOLUME_PRESERVATION),
                        scale * (1.0 + stretch),
                    ),
                ),
            alpha: (self.open.position * ALPHA_LEAD).clamp(0.0, 1.0),
            crossfade: self.fade.position.clamp(0.0, 1.0),
        }
    }
}

/// `transform` applied about `pivot` rather than the surface origin.
fn about(pivot: Point, transform: Affine) -> Affine {
    Affine::translate(pivot.to_vec2()) * transform * Affine::translate(-pivot.to_vec2())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: f32 = 1.0 / 60.0;
    const FIRST: Rect = Rect::new(100.0, 4.0, 396.0, 200.0);
    const SECOND: Rect = Rect::new(400.0, 4.0, 696.0, 320.0);

    /// Steps until nothing is moving, returning the furthest the panel ever
    /// got past its resting size.
    fn settle(motion: &mut PanelMotion) -> f64 {
        let mut peak: f64 = 0.0;
        for _ in 0..600 {
            peak = peak.max(motion.frame(0.0).bounds.height());
            if !motion.step(FRAME) {
                break;
            }
        }
        peak
    }

    fn shown() -> PanelMotion {
        let mut motion = PanelMotion::new();
        motion.open(false);
        motion.reshape(FIRST);
        settle(&mut motion);
        motion
    }

    #[test]
    fn the_first_panel_takes_its_bounds_outright() {
        let mut motion = PanelMotion::new();
        motion.open(false);
        motion.reshape(FIRST);
        // Nothing to travel from: the shape is there before the first frame,
        // and only the spring-out transform moves.
        assert_eq!(motion.frame(0.0).bounds, FIRST);
    }

    #[test]
    fn it_is_thrown_out_of_its_pill_and_overshoots_once() {
        let mut motion = PanelMotion::new();
        motion.open(false);
        motion.reshape(FIRST);

        let birth = motion.frame(0.0);
        assert!(birth.visible().height() < FIRST.height(), "started full size");
        assert!(birth.alpha < 1.0, "started solid");

        let mut peak: f64 = 0.0;
        for _ in 0..600 {
            peak = peak.max(motion.frame(0.0).visible().height());
            if !motion.step(FRAME) {
                break;
            }
        }
        assert!(peak > FIRST.height(), "never overshot: peaked at {peak}");
        assert!(motion.at_rest());
        assert!((motion.frame(0.0).visible().height() - FIRST.height()).abs() < 1.0);
    }

    #[test]
    fn switching_panels_travels_to_the_new_bounds() {
        let mut motion = shown();
        motion.open(true);
        motion.reshape(SECOND);

        let midway = motion.frame(0.0);
        assert!(
            midway.bounds.x0 > FIRST.x0 - 1.0 && midway.bounds.x0 < SECOND.x0,
            "jumped to {:?}",
            midway.bounds
        );
        // Full size and solid throughout: this is one shape changing, not a
        // second one being opened.
        assert_eq!(midway.alpha, 1.0);
        assert!(midway.crossfade < 1.0, "rows did not cross-fade");

        settle(&mut motion);
        assert!(!motion.invisible());
        assert!((motion.frame(0.0).bounds.height() - SECOND.height()).abs() < 1.0);
    }

    #[test]
    fn a_dismissal_falls_away_without_bouncing() {
        let mut motion = shown();
        motion.close();
        motion.step(FRAME);
        // A spring released from rest covers a couple of percent of its travel
        // in the first frame, which reads as a hesitation before a dismissal.
        let shrink = 1.0 - motion.frame(0.0).visible().height() / FIRST.height();
        assert!(shrink > 0.02, "crept off by {shrink} on the first frame");

        let peak = settle(&mut motion);
        assert!(peak <= FIRST.height(), "a dismissal bounced to {peak}");
        assert!(motion.invisible());
    }

    #[test]
    fn a_panel_at_rest_is_not_deformed() {
        let motion = shown();
        let settled = motion.frame(200.0).visible();
        // Springs settle within their epsilon rather than exactly on it, so
        // what matters is that nothing is left a pixel out of place.
        assert!(
            (settled.x0 - FIRST.x0).abs() < 0.1
                && (settled.y0 - FIRST.y0).abs() < 0.1
                && (settled.x1 - FIRST.x1).abs() < 0.1
                && (settled.y1 - FIRST.y1).abs() < 0.1,
            "settled at {settled:?}"
        );
    }
}

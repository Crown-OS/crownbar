//! How a popup panel moves: out of its pill, between one panel and the next,
//! and back out of sight.
//!
//! Everything here is a spring, and every spring is integrated rather than
//! sampled off a curve — so a panel interrupted halfway out carries the
//! velocity it already had into wherever it is sent next.
//!
//! **Out** is one critically damped spring ([`SpringProfile::SNAPPY`]): the
//! panel grows from [`BIRTH_SCALE`] about the pill it hangs from, settles the
//! last [`DROP`] px onto its resting place and fades in, arriving without
//! overshoot.
//!
//! **Away** runs the same spring back on [`SpringProfile::DROP`] with a shove
//! ([`CLOSE_KICK`]): over-damped and already moving on the first frame, so it
//! leaves rather than deflates, and never bounces back into view.
//!
//! Switching panels never replays any of that. The shape itself travels to the
//! new one's bounds while the two sets of rows cross-fade inside it, so a click
//! on the next pill reads as the same object changing rather than one object
//! being destroyed and another built.

use kurbo::{Affine, Point, Rect};

use crate::{
    animation::{Spring, SpringProfile},
    ui::PANEL_GAP,
};

/// Fraction of its final size the panel grows from.
const BIRTH_SCALE: f64 = 0.94;
/// How far above its resting place it starts, in px: no further than the gap
/// to the bar, so the body never slides under the edge it is clipped to while
/// the compositor's blur, which nothing clips, carries on past it.
const DROP: f64 = PANEL_GAP as f64;
/// Velocity the close is shoved off with, in units of the 0..1 progress/second.
const CLOSE_KICK: f32 = -2.2;
/// Opacity leads the motion — solid by the time it is half out, which reads as
/// quicker than it is.
const ALPHA_LEAD: f32 = 2.2;
/// Below this the panel is gone: nothing drawn, no input region, no material.
const HIDDEN: f32 = 0.001;

/// Which way a panel opens out of the bar: down from a bar along the top of
/// the screen, up from one along the bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opening {
    Down,
    Up,
}

/// What a frame of the animation works out to.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Frame {
    /// The morphing shape, in surface px, before the spring-out transform.
    pub bounds: Rect,
    /// Spring-out and drop, about the pill the panel hangs from.
    pub transform: Affine,
    pub alpha: f32,
}

impl Frame {
    /// The shape as it lands on screen, transform included.
    pub fn visible(&self) -> Rect {
        self.transform.transform_rect_bbox(self.bounds)
    }

    /// How much the rows are scaled along each axis on their way to
    /// [`visible`](Self::visible), from its top-left corner.
    pub fn scale(&self) -> (f64, f64) {
        let [scale_x, _, _, scale_y, _, _] = self.transform.as_coeffs();
        (scale_x, scale_y)
    }
}

/// The springs behind one popup surface.
pub struct PanelMotion {
    /// Size, drop and fade: 0 = dismissed, 1 = fully out.
    reveal: Spring,
    /// The shape's four edges, so a panel of another size is travelled to
    /// rather than cut to.
    edges: [Spring; 4],
    /// Whether the next [`reshape`](Self::reshape) lands outright. A shape
    /// with nothing on screen has nothing to travel from.
    unplaced: bool,
}

impl PanelMotion {
    pub const fn new() -> Self {
        Self {
            reveal: Spring::with_profile(0.0, SpringProfile::SNAPPY),
            edges: [Spring::with_profile(0.0, SpringProfile::SNAPPY); 4],
            unplaced: true,
        }
    }

    /// Bring the panel out of its pill. `replacing` is whether another panel
    /// is already on screen — then the shape morphs instead of the whole thing
    /// being brought out again.
    pub fn open(&mut self, replacing: bool) {
        self.unplaced = !replacing;
        if replacing {
            return;
        }
        self.reveal.set_profile(SpringProfile::SNAPPY);
        self.reveal.set_target(1.0);
    }

    pub fn close(&mut self) {
        self.reveal.set_profile(SpringProfile::DROP);
        self.reveal.set_target(0.0);
        self.reveal.nudge(CLOSE_KICK);
    }

    fn springs(&mut self) -> impl Iterator<Item = &mut Spring> {
        std::iter::once(&mut self.reveal).chain(&mut self.edges)
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
        self.springs().fold(false, |busy, spring| {
            let moving = !spring.at_rest();
            if moving {
                spring.step(dt);
            }
            busy | moving
        })
    }

    /// Nothing on screen and nothing on its way there.
    pub fn invisible(&self) -> bool {
        self.reveal.position <= HIDDEN && self.reveal.at_rest()
    }

    #[cfg(test)]
    fn at_rest(&mut self) -> bool {
        self.springs().all(|spring| spring.at_rest())
    }

    /// Resolve the frame, hanging from the pill centred on `anchor_x`.
    pub fn frame(&self, anchor_x: f64, opening: Opening) -> Frame {
        let [x0, y0, x1, y1] = self.edges.map(|edge| edge.position as f64);
        let bounds = Rect::new(x0, y0, x1, y1);

        let revealed = f64::from(self.reveal.position);
        let scale = BIRTH_SCALE + (1.0 - BIRTH_SCALE) * revealed;
        let (edge, drop) = match opening {
            Opening::Down => (bounds.y0, -DROP),
            Opening::Up => (bounds.y1, DROP),
        };
        let pivot = Point::new(anchor_x.clamp(bounds.x0, bounds.x1), edge);

        Frame {
            bounds,
            transform: Affine::translate((0.0, drop * (1.0 - revealed)))
                * about(pivot, Affine::scale(scale)),
            alpha: (self.reveal.position * ALPHA_LEAD).clamp(0.0, 1.0),
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
            peak = peak.max(motion.frame(0.0, Opening::Down).bounds.height());
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
        assert_eq!(motion.frame(0.0, Opening::Down).bounds, FIRST);
    }

    #[test]
    fn it_grows_out_of_its_pill_without_overshooting() {
        let mut motion = PanelMotion::new();
        motion.open(false);
        motion.reshape(FIRST);

        let birth = motion.frame(0.0, Opening::Down);
        assert!(
            birth.visible().height() < FIRST.height(),
            "started full size"
        );
        assert!(birth.alpha < 1.0, "started solid");

        let mut peak: f64 = 0.0;
        for _ in 0..600 {
            peak = peak.max(motion.frame(0.0, Opening::Down).visible().height());
            if !motion.step(FRAME) {
                break;
            }
        }
        assert!(peak <= FIRST.height() + 0.5, "overshot to {peak}");
        assert!(motion.at_rest());
        assert!((motion.frame(0.0, Opening::Down).visible().height() - FIRST.height()).abs() < 1.0);
    }

    #[test]
    fn switching_panels_travels_to_the_new_bounds() {
        let mut motion = shown();
        motion.open(true);
        motion.reshape(SECOND);

        let midway = motion.frame(0.0, Opening::Down);
        assert!(
            midway.bounds.x0 > FIRST.x0 - 1.0 && midway.bounds.x0 < SECOND.x0,
            "jumped to {:?}",
            midway.bounds
        );
        // Full size and solid throughout: this is one shape changing, not a
        // second one being opened.
        assert_eq!(midway.alpha, 1.0);

        let mut furthest = f64::MIN;
        for _ in 0..600 {
            furthest = furthest.max(motion.frame(0.0, Opening::Down).bounds.x0);
            if !motion.step(FRAME) {
                break;
            }
        }
        assert!(furthest <= SECOND.x0 + 0.5, "overshot to {furthest}");
        assert!(!motion.invisible());
        assert!((motion.frame(0.0, Opening::Down).bounds.height() - SECOND.height()).abs() < 1.0);
    }

    #[test]
    fn a_dismissal_falls_away_without_bouncing() {
        let mut motion = shown();
        motion.close();
        motion.step(FRAME);
        // A spring released from rest covers a couple of percent of its travel
        // in the first frame, which reads as a hesitation before a dismissal.
        let shrunk = 1.0 - motion.frame(0.0, Opening::Down).visible().height() / FIRST.height();
        let travelled = shrunk / (1.0 - BIRTH_SCALE);
        assert!(
            travelled > 0.02,
            "crept off by {travelled} on the first frame"
        );

        let peak = settle(&mut motion);
        assert!(peak <= FIRST.height(), "a dismissal bounced to {peak}");
        assert!(motion.invisible());
    }

    #[test]
    fn it_never_falls_past_its_resting_place() {
        let mut motion = PanelMotion::new();
        motion.open(false);
        motion.reshape(FIRST);
        let mut deepest = f64::MIN;
        for _ in 0..600 {
            deepest = deepest.max(motion.frame(0.0, Opening::Down).transform.as_coeffs()[5]);
            if !motion.step(FRAME) {
                break;
            }
        }
        assert!(deepest <= 0.01, "fell past its place by {deepest}");
    }

    #[test]
    fn a_panel_never_leaves_the_area_it_opens_into() {
        let resting = Rect::new(100.0, PANEL_GAP as f64, 396.0, 200.0);
        let mut motion = PanelMotion::new();
        motion.open(false);
        motion.reshape(resting);
        for _ in 0..600 {
            let top = motion.frame(0.0, Opening::Down).visible().y0;
            assert!(top >= -1e-9, "rose to {top} past the bar's edge");
            if !motion.step(FRAME) {
                break;
            }
        }
    }

    #[test]
    fn a_panel_opening_up_rises_out_of_a_bottom_bar() {
        let mut motion = PanelMotion::new();
        motion.open(false);
        motion.reshape(FIRST);
        let birth = motion.frame(0.0, Opening::Up).visible();
        assert!(birth.y1 > FIRST.y1, "started above the bar: {birth:?}");
        assert!(birth.y0 > FIRST.y0, "grew downward: {birth:?}");
    }

    #[test]
    fn a_panel_at_rest_is_not_deformed() {
        let motion = shown();
        let settled = motion.frame(200.0, Opening::Down).visible();
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

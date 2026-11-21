//! How a popup panel moves: out from behind its pill, between one panel and
//! the next, and back behind its pill.
//!
//! The panel starts as the shape of its pill, behind the pill. Its leading
//! edge, the one away from the bar, springs out first. A moment later the
//! shape grows to its full size, and it turns flat from a tilt that keeps its
//! trailing edge back toward the pill. The corners go from the pill's round
//! ends to the panel's radius, and the rows show when the shape is nearly in
//! place. A dismissal plays the same springs back, the size first.
//!
//! The springs, the delay and the tilt are fitted to a frame-by-frame
//! measurement of the reference recording (see the tests), then played
//! [`SPEED`] times as fast.
//!
//! A change from one panel to the next does not play any of that. The shape
//! travels to the bounds of the next panel while the two sets of rows
//! cross-fade inside it.

use kurbo::Rect;

use super::PANEL_RADIUS;
use crate::util::ease::smoothstep;
use crownbar_widgets::animation::{Spring, SpringProfile};

/// How many times faster than the reference recording the panel moves.
const SPEED: f32 = 1.5;

/// `profile` played [`SPEED`] times as fast, with the same curve.
const fn quickened(profile: SpringProfile) -> SpringProfile {
    SpringProfile {
        stiffness: profile.stiffness * SPEED * SPEED,
        damping: profile.damping * SPEED,
    }
}

/// The leading edge on the way out: damping ratio 0.74, a 1.6 % overshoot.
const OPEN_LEAD: SpringProfile = quickened(SpringProfile {
    stiffness: 85.6,
    damping: 13.7,
});
/// The size along the axis the panel opens on, on the way out.
const OPEN_LENGTH: SpringProfile = quickened(SpringProfile {
    stiffness: 144.0,
    damping: 20.2,
});
/// The size across that axis, on the way out: critically damped.
const OPEN_BREADTH: SpringProfile = quickened(SpringProfile {
    stiffness: 232.6,
    damping: 30.5,
});
const CLOSE_LEAD: SpringProfile = quickened(SpringProfile {
    stiffness: 64.0,
    damping: 12.5,
});
const CLOSE_LENGTH: SpringProfile = quickened(SpringProfile {
    stiffness: 203.1,
    damping: 28.5,
});
const CLOSE_BREADTH: SpringProfile = quickened(SpringProfile {
    stiffness: 210.2,
    damping: 26.1,
});
/// How long the size waits for the leading edge on the way out, in seconds.
const GROWTH_DELAY: f32 = 0.1 / SPEED;
/// Distance from the eye to the panel, in resting panel heights. In the
/// reference the far edge narrows in step with the growth still to come, which
/// at this distance is a turn whose sine is this times that growth.
const PERSPECTIVE_DEPTH: f64 = 0.75;
/// The panel is solid once its leading edge is this far out of the pill.
const BODY_FADE: f64 = 0.15;
/// The rows fade in over this part of the growth.
const ROWS_FADE: (f64, f64) = (0.72, 0.96);
/// The glass (blur and translucency) comes in over this part of the growth,
/// when the tilt is less than 9 degrees. The compositor blurs a flat rounded
/// rectangle, which a strongly tilted panel does not cover.
const GLASS_FADE: (f64, f64) = (0.8, 1.0);
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
    /// The shape in surface px, before the tilt.
    pub bounds: Rect,
    /// Where the shape rests, which the rows are laid out for.
    pub resting: Rect,
    pub radius: f64,
    /// The turn about the horizontal axis, in degrees.
    pub pitch: f64,
    /// Distance from the eye the turn is seen from, in px.
    pub perspective: f64,
    /// Where the far side converges, as a fraction of the width: the centre of
    /// the pill.
    pub vanishing_x: f64,
    /// Opacity of the shape.
    pub alpha: f32,
    /// Opacity of the rows.
    pub rows: f32,
    /// How much of the glass is in: 0 is an opaque body without blur.
    pub glass: f32,
}

impl Frame {
    /// How much the rows are scaled along each axis to fill the shape, from
    /// its top-left corner.
    pub fn scale(&self) -> (f64, f64) {
        let ratio = |now: f64, resting: f64| if resting > 0.0 { now / resting } else { 1.0 };
        (
            ratio(self.bounds.width(), self.resting.width()),
            ratio(self.bounds.height(), self.resting.height()),
        )
    }
}

/// The springs behind one popup surface. Each runs from 0, the pill, to 1,
/// the panel at rest.
pub struct PanelMotion {
    /// The leading edge.
    lead: Spring,
    /// The size along the axis the panel opens on.
    length: Spring,
    /// The size across it.
    breadth: Spring,
    /// Time left before the size follows the leading edge out.
    growth_delay: f32,
    /// The resting shape's four edges, so a panel of another size is
    /// travelled to rather than cut to.
    edges: [Spring; 4],
    /// Whether the next [`reshape`](Self::reshape) lands outright. A shape
    /// with nothing on screen has nothing to travel from.
    unplaced: bool,
}

impl PanelMotion {
    pub const fn new() -> Self {
        Self {
            lead: Spring::with_profile(0.0, OPEN_LEAD),
            length: Spring::with_profile(0.0, OPEN_LENGTH),
            breadth: Spring::with_profile(0.0, OPEN_BREADTH),
            growth_delay: 0.0,
            edges: [Spring::with_profile(0.0, SpringProfile::SNAPPY); 4],
            unplaced: true,
        }
    }

    /// Bring the panel out from behind its pill. `replacing` is whether
    /// another panel is already on screen; then the shape travels instead.
    pub fn open(&mut self, replacing: bool) {
        self.unplaced = !replacing;
        if replacing {
            return;
        }
        self.lead.set_profile(OPEN_LEAD);
        self.length.set_profile(OPEN_LENGTH);
        self.breadth.set_profile(OPEN_BREADTH);
        self.lead.set_target(1.0);
        self.growth_delay = GROWTH_DELAY;
    }

    pub fn close(&mut self) {
        self.growth_delay = 0.0;
        for (spring, profile) in self
            .progress()
            .zip([CLOSE_LEAD, CLOSE_LENGTH, CLOSE_BREADTH])
        {
            spring.set_profile(profile);
            spring.set_target(0.0);
        }
    }

    fn progress(&mut self) -> impl Iterator<Item = &mut Spring> {
        [&mut self.lead, &mut self.length, &mut self.breadth].into_iter()
    }

    fn springs(&mut self) -> impl Iterator<Item = &mut Spring> {
        [&mut self.lead, &mut self.length, &mut self.breadth]
            .into_iter()
            .chain(&mut self.edges)
    }

    /// Aim the resting shape at `bounds`.
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
        let growing = self.growth_delay > 0.0;
        if growing {
            self.growth_delay -= dt;
            if self.growth_delay <= 0.0 {
                self.length.set_target(1.0);
                self.breadth.set_target(1.0);
            }
        }
        self.springs().fold(growing, |busy, spring| {
            let moving = !spring.at_rest();
            if moving {
                spring.step(dt);
                if spring.at_rest() {
                    spring.snap_to_target();
                }
            }
            busy | moving
        })
    }

    /// Nothing on screen and nothing on its way there.
    pub fn invisible(&self) -> bool {
        self.lead.position <= HIDDEN
            && self.growth_delay <= 0.0
            && [&self.lead, &self.length, &self.breadth]
                .iter()
                .all(|spring| spring.at_rest())
    }

    fn resting(&self) -> Rect {
        let [x0, y0, x1, y1] = self.edges.map(|edge| f64::from(edge.position));
        Rect::new(x0, y0, x1, y1)
    }

    /// Resolve the frame of a panel that opens out of `pill`.
    pub fn frame(&self, pill: Rect, opening: Opening) -> Frame {
        let resting = self.resting();
        let [lead, length, breadth] =
            [&self.lead, &self.length, &self.breadth].map(|spring| f64::from(spring.position));
        let height = lerp(pill.height(), resting.height(), length);
        let (y0, y1) = match opening {
            Opening::Down => {
                let leading = lerp(pill.y1, resting.y1, lead);
                (leading - height, leading)
            }
            Opening::Up => {
                let leading = lerp(pill.y0, resting.y0, lead);
                (leading, leading + height)
            }
        };
        let bounds = Rect::new(
            lerp(pill.x0, resting.x0, breadth),
            y0,
            lerp(pill.x1, resting.x1, breadth),
            y1,
        )
        .abs();
        let pill_radius = pill.width().min(pill.height()) / 2.0;
        let radius = lerp(pill_radius, f64::from(PANEL_RADIUS), length)
            .min(bounds.width().min(bounds.height()) / 2.0)
            .max(0.0);
        let turn = ((1.0 - length) * PERSPECTIVE_DEPTH)
            .clamp(-1.0, 1.0)
            .asin()
            .to_degrees();
        Frame {
            bounds,
            resting,
            radius,
            pitch: match opening {
                Opening::Down => turn,
                Opening::Up => -turn,
            },
            perspective: PERSPECTIVE_DEPTH * resting.height(),
            vanishing_x: (pill.center().x - bounds.x0) / bounds.width().max(1.0),
            alpha: fade(lead, (0.0, BODY_FADE)),
            rows: fade(length, ROWS_FADE),
            glass: fade(length, GLASS_FADE),
        }
    }
}

fn lerp(from: f64, to: f64, progress: f64) -> f64 {
    from + (to - from) * progress
}

fn fade(progress: f64, (from, to): (f64, f64)) -> f32 {
    smoothstep(((progress - from) / (to - from)) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: f32 = 1.0 / 144.0;
    const PILL: Rect = Rect::new(380.0, 4.0, 420.0, 32.0);
    const FIRST: Rect = Rect::new(230.0, 40.0, 570.0, 340.0);
    const SECOND: Rect = Rect::new(400.0, 40.0, 740.0, 420.0);
    /// The reference recording's frame rate.
    const REFERENCE_FPS: f32 = 30_000.0 / 1001.0;
    /// Frames from the click to the first frame the reference moved in, on
    /// the way out and on the way back.
    const OPEN_LATENCY: f32 = 0.75;
    const CLOSE_LATENCY: f32 = -0.25;

    /// Measured from the reference, one value per frame after the click: how
    /// far its leading edge had travelled and how far it had grown.
    const REFERENCE_LEAD: [f64; 19] = [
        0.05, 0.08, 0.17, 0.28, 0.36, 0.51, 0.65, 0.78, 0.87, 0.93, 0.976, 1.0, 1.012, 1.016,
        1.016, 1.012, 1.008, 1.004, 1.0,
    ];
    const REFERENCE_LENGTH: [f64; 19] = [
        0.0, 0.0, 0.026, 0.077, 0.121, 0.210, 0.360, 0.533, 0.662, 0.765, 0.849, 0.908, 0.949,
        0.974, 0.993, 1.0, 1.004, 1.004, 1.0,
    ];
    const REFERENCE_CLOSE_LEAD: [f64; 15] = [
        0.026, 0.094, 0.218, 0.342, 0.451, 0.545, 0.635, 0.722, 0.801, 0.861, 0.910, 0.944, 0.966,
        0.992, 1.0,
    ];

    fn opened() -> PanelMotion {
        let mut motion = PanelMotion::new();
        motion.open(false);
        motion.reshape(FIRST);
        motion
    }

    fn shown() -> PanelMotion {
        let mut motion = opened();
        settle(&mut motion);
        motion
    }

    fn settle(motion: &mut PanelMotion) {
        for _ in 0..2000 {
            if !motion.step(FRAME) {
                break;
            }
        }
    }

    /// Steps `motion` to each reference frame in turn, played [`SPEED`] times
    /// as fast, and reads `progress` there.
    fn sampled(
        motion: &mut PanelMotion,
        latency: f32,
        frames: usize,
        progress: impl Fn(&PanelMotion) -> f64,
    ) -> Vec<f64> {
        let step = FRAME / SPEED;
        let mut elapsed = 0.0;
        (1..=frames)
            .map(|frame| {
                let due = (frame as f32 - latency) / REFERENCE_FPS / SPEED;
                while elapsed + step <= due {
                    motion.step(step);
                    elapsed += step;
                }
                progress(motion)
            })
            .collect()
    }

    fn worst_error(sampled: &[f64], reference: &[f64]) -> f64 {
        sampled
            .iter()
            .zip(reference)
            .map(|(sampled, reference)| (sampled - reference).abs())
            .fold(0.0, f64::max)
    }

    #[test]
    fn it_starts_as_its_pill_behind_it() {
        let birth = opened().frame(PILL, Opening::Down);
        assert_eq!(birth.bounds, PILL);
        assert_eq!(birth.radius, PILL.height() / 2.0);
        assert_eq!((birth.alpha, birth.rows, birth.glass), (0.0, 0.0, 0.0));
    }

    #[test]
    fn it_opens_like_the_reference() {
        let mut motion = opened();
        let lead = sampled(&mut motion, OPEN_LATENCY, REFERENCE_LEAD.len(), |motion| {
            f64::from(motion.lead.position)
        });
        assert!(
            worst_error(&lead, &REFERENCE_LEAD) < 0.09,
            "leading edge {lead:.3?}"
        );
        let mut motion = opened();
        let length = sampled(
            &mut motion,
            OPEN_LATENCY,
            REFERENCE_LENGTH.len(),
            |motion| f64::from(motion.length.position),
        );
        assert!(
            worst_error(&length, &REFERENCE_LENGTH) < 0.09,
            "growth {length:.3?}"
        );
    }

    #[test]
    fn it_closes_like_the_reference() {
        let mut motion = shown();
        motion.close();
        let travelled = sampled(
            &mut motion,
            CLOSE_LATENCY,
            REFERENCE_CLOSE_LEAD.len(),
            |motion| 1.0 - f64::from(motion.lead.position),
        );
        assert!(
            worst_error(&travelled, &REFERENCE_CLOSE_LEAD) < 0.06,
            "leading edge {travelled:.3?}"
        );
    }

    #[test]
    fn it_leaves_its_pill_before_it_grows() {
        let mut motion = opened();
        for _ in 0..(GROWTH_DELAY / FRAME) as usize {
            motion.step(FRAME);
        }
        let early = motion.frame(PILL, Opening::Down);
        assert!(
            early.bounds.y1 > PILL.y1 + 1.0,
            "still at the pill: {early:?}"
        );
        assert!(
            (early.bounds.height() - PILL.height()).abs() < 1.0,
            "grew before it left: {early:?}"
        );
    }

    #[test]
    fn it_turns_flat_from_its_far_side_tilted_back() {
        let mut motion = opened();
        let mut pitches = Vec::new();
        for _ in 0..2000 {
            pitches.push(motion.frame(PILL, Opening::Down).pitch);
            if !motion.step(FRAME) {
                break;
            }
        }
        assert!(pitches[0] > 45.0, "started at {}", pitches[0]);
        assert!(pitches.last().is_some_and(|pitch| pitch.abs() < 0.1));
        let up = opened().frame(PILL, Opening::Up).pitch;
        assert!(up < -45.0, "a panel opening up turned the other way: {up}");
    }

    #[test]
    fn its_corners_go_from_the_pill_to_the_panel() {
        let mut motion = opened();
        let mut radii = Vec::new();
        for _ in 0..2000 {
            radii.push(motion.frame(PILL, Opening::Down).radius);
            if !motion.step(FRAME) {
                break;
            }
        }
        assert_eq!(radii[0], PILL.height() / 2.0);
        assert!(
            radii
                .last()
                .is_some_and(|radius| (radius - f64::from(PANEL_RADIUS)).abs() < 0.1)
        );
    }

    #[test]
    fn the_rows_show_only_when_the_shape_is_nearly_in_place() {
        let mut motion = opened();
        for _ in 0..2000 {
            let frame = motion.frame(PILL, Opening::Down);
            if motion.length.position < 0.7 {
                assert_eq!(frame.rows, 0.0, "rows showed at {}", motion.length.position);
            }
            if !motion.step(FRAME) {
                break;
            }
        }
        assert_eq!(motion.frame(PILL, Opening::Down).rows, 1.0);
    }

    #[test]
    fn switching_panels_travels_to_the_new_bounds_flat() {
        let mut motion = shown();
        motion.open(true);
        motion.reshape(SECOND);
        motion.step(FRAME);
        let midway = motion.frame(PILL, Opening::Down);
        assert!(
            midway.bounds.x0 > FIRST.x0 && midway.bounds.x0 < SECOND.x0,
            "jumped to {:?}",
            midway.bounds
        );
        assert_eq!((midway.alpha, midway.rows), (1.0, 1.0));
        assert!(midway.pitch.abs() < 0.1);
        settle(&mut motion);
        assert!((motion.frame(PILL, Opening::Down).bounds.height() - SECOND.height()).abs() < 1.0);
    }

    #[test]
    fn a_dismissal_goes_back_behind_its_pill() {
        let mut motion = shown();
        motion.close();
        settle(&mut motion);
        assert!(motion.invisible());
        let gone = motion.frame(PILL, Opening::Down);
        assert_eq!(gone.alpha, 0.0);
        assert!(
            (gone.bounds.center() - PILL.center()).hypot() < 1.0,
            "ended at {:?}",
            gone.bounds
        );
    }

    #[test]
    fn a_panel_at_rest_is_not_deformed() {
        let settled = shown().frame(PILL, Opening::Down);
        assert!(
            (settled.bounds.x0 - FIRST.x0).abs() < 0.1
                && (settled.bounds.y0 - FIRST.y0).abs() < 0.1
                && (settled.bounds.x1 - FIRST.x1).abs() < 0.1
                && (settled.bounds.y1 - FIRST.y1).abs() < 0.1,
            "settled at {settled:?}"
        );
        assert!(settled.pitch.abs() < 0.1);
        assert_eq!(
            (settled.alpha, settled.rows, settled.glass),
            (1.0, 1.0, 1.0)
        );
    }
}

//! The surface every widget popup is drawn on, and the state the bar and that
//! surface share.
//!
//! There is exactly one popup surface for the life of the process: an overlay
//! layer anchored to all four edges, so the compositor sizes it to the area
//! below the bar. Covering that whole area is what lets a panel be drawn
//! anywhere in it, animate without a resize round trip, and dismiss from a
//! click outside itself — that click is an ordinary pointer event on the same
//! surface. While no panel is up the surface draws nothing and drops both its
//! input region and its material, so it costs the compositor nothing while it
//! waits.
//!
//! The bar and the popup are separate [`SurfaceHandler`]s and only a surface
//! can repaint itself, so a click on a pill reaches the panel through
//! [`PopupState`]: the bar bumps a version, and [`SurfaceHandler::needs_redraw`]
//! on the popup picks it up at the end of the same event-loop iteration.
//!
//! # The shape, and what the compositor puts behind it
//!
//! The panel is one rounded rectangle whose bounds are sprung — see
//! [`PanelMotion`]. Everything that has to agree with it is derived from the
//! same rect on the same frame: the body this surface paints, the rows clipped
//! inside it, and the [`MaterialSpec`] the compositor renders *under* it.
//!
//! That last one cannot be done here at all. What sits behind this surface
//! belongs to other clients and a translucent surface never gets to see it, so
//! the blur, its vibrancy, the refractive rim and the drop shadow are asked of
//! the compositor through `crownos-background-effects`, as a rounded rect
//! rather than a `wl_region` — a region is a list of integer rectangles and
//! would square the corners off, which is the one place the difference shows.
//! A compositor may withhold any of it ([`Capability`]), so when the blur is
//! not coming the panel body is painted opaque instead: a translucent panel
//! over an unblurred wallpaper is not readable.

use std::{cell::RefCell, rc::Rc};

use crownshell::{Blur, Capability, MaterialSpec, Scene, Shadow, SurfaceCtx, SurfaceHandler};
use vello::{
    kurbo::{Affine, Point, Rect, RoundedRect, Vec2},
    peniko::{Color, Fill, Mix},
};

use crate::{
    animation::Clock,
    services::Services,
    theme,
    ui::{
        morph::PanelMotion,
        panel::{self, Panel},
    },
    widgets::{AfterAction, PopupAction, WidgetRegistry},
};

/// Gap between the bar's lower edge — the top of this surface — and the panel.
const PANEL_GAP: f64 = 4.0;
/// Smallest distance the panel keeps from the screen's left/right edges.
const SCREEN_MARGIN: f64 = 8.0;
/// Backdrop blur asked of the compositor, in logical px.
const BLUR_RADIUS: f64 = 32.0;
/// Chroma multiplier on the blurred backdrop. Above 1 is the vibrancy that
/// makes a wallpaper's color show through frosted glass rather than grey out.
const BLUR_VIBRANCY: f64 = 1.25;
/// Width of the compositor's refractive rim just inside the panel's edge.
const RIM: f64 = 1.0;
const SHADOW_DY: f64 = 8.0;
const SHADOW_BLUR: f64 = 24.0;

/// What the bar and the popup surface both read.
///
/// Everything in crownshell runs on one thread, so `Rc<RefCell<_>>` is all the
/// sharing this needs.
#[derive(Default)]
pub struct PopupState {
    owner: Option<usize>,
    anchor_x: f32,
    version: u64,
    /// Bumped when the open panel's *contents* went stale, which must not
    /// replay the open animation the way `version` does.
    content: u64,
}

impl PopupState {
    pub fn shared() -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self::default()))
    }

    /// Index of the widget whose panel is up, if any. The bar reads this to
    /// keep that widget's pill lit.
    pub fn owner(&self) -> Option<usize> {
        self.owner
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn content(&self) -> u64 {
        self.content
    }

    /// A service published something the open panel is showing. With nothing
    /// up there is nothing to restate, and the surface stays asleep.
    pub fn invalidate(&mut self) {
        if self.owner.is_some() {
            self.content += 1;
        }
    }

    /// Show `idx`'s panel, hanging from a pill centred on `anchor_x`.
    pub fn open(&mut self, idx: usize, anchor_x: f32) {
        self.owner = Some(idx);
        self.anchor_x = anchor_x;
        self.version += 1;
    }

    pub fn close(&mut self) {
        if self.owner.is_none() {
            return;
        }
        self.owner = None;
        self.version += 1;
    }

    /// Open `idx`'s panel, or dismiss it if it is the one already up.
    pub fn toggle(&mut self, idx: usize, anchor_x: f32) {
        if self.owner == Some(idx) {
            self.close();
        } else {
            self.open(idx, anchor_x);
        }
    }
}

pub struct PopupHandler {
    widgets: Rc<RefCell<WidgetRegistry>>,
    state: Rc<RefCell<PopupState>>,
    services: Rc<Services>,
    /// Service generation this surface last painted.
    seen_services: u64,
    /// Panel-contents generation this surface last rebuilt for.
    seen_content: u64,
    /// Version of the shared state this surface last painted.
    seen: u64,
    /// Palette generation this surface last painted — see [`crate::theme`].
    seen_theme: u64,
    /// Widget the built panel belongs to. Trails `state.owner` by one paint,
    /// which is how the closing animation keeps something to draw.
    shown: Option<usize>,
    panel: Option<Panel>,
    /// Where the panel's top-left corner sits this frame, in surface px. It
    /// follows the morphing shape rather than the resting one, so a pointer
    /// lands on the row it is over.
    origin: Point,
    motion: PanelMotion,
    clock: Clock,
    hovered: Option<usize>,
    /// Slider row the pointer is currently dragging.
    dragging: Option<usize>,
    /// The widget's contents changed and the panel has to be rebuilt.
    stale: bool,
    /// Scratch scene for the panel's rows, appended under the animation's
    /// transform so a frame costs re-encoding and no text shaping.
    content: Scene,
    /// The rows of the panel being replaced, kept encoded so they can fade out
    /// inside the shape that is on its way to the new one's bounds.
    outgoing: Scene,
}

impl PopupHandler {
    pub fn new(
        widgets: Rc<RefCell<WidgetRegistry>>,
        state: Rc<RefCell<PopupState>>,
        services: Rc<Services>,
    ) -> Self {
        Self {
            widgets,
            state,
            services,
            seen_services: 0,
            seen_content: 0,
            seen: 0,
            seen_theme: 0,
            shown: None,
            panel: None,
            origin: Point::ZERO,
            motion: PanelMotion::new(),
            clock: Clock::new(),
            hovered: None,
            dragging: None,
            stale: false,
            content: Scene::new(),
            outgoing: Scene::new(),
        }
    }

    /// Adopt whatever the shared state now says, building or retiring the
    /// panel as needed. Returns the anchor the panel should hang from.
    fn sync(&mut self, tcx: &mut crownshell::TextContext) -> f32 {
        let (owner, anchor_x, version, content) = {
            let state = self.state.borrow();
            (state.owner, state.anchor_x, state.version, state.content)
        };
        self.seen = version;
        // A snapshot landing while the panel is up restates its rows without
        // replaying the open animation, which `owner != self.shown` would.
        let fresh_content = content != self.seen_content;
        self.seen_content = content;

        if owner != self.shown {
            // One panel handing over to another, rather than one being thrown
            // out of its pill: the shape travels and the rows cross-fade.
            let replacing = self.shown.is_some() && !self.motion.invisible();
            if let Some(prev) = self.shown
                && let Some(rt) = self.widgets.borrow_mut().widgets.get_mut(prev)
            {
                rt.widget.popup_closed(&self.services);
            }
            self.shown = owner;
            self.hovered = None;
            self.dragging = None;
            self.clock.reset();
            match owner {
                Some(idx) => {
                    if replacing {
                        // Last frame's rows, already encoded, become the ones
                        // fading out — no second build, no second shaping.
                        std::mem::swap(&mut self.content, &mut self.outgoing);
                    }
                    self.rebuild(idx, tcx);
                    self.motion.open(replacing);
                }
                None => self.motion.close(),
            }
        } else if (self.stale || fresh_content)
            && let Some(idx) = self.shown
        {
            self.rebuild(idx, tcx);
        }
        self.stale = false;
        anchor_x
    }

    fn rebuild(&mut self, idx: usize, tcx: &mut crownshell::TextContext) {
        let spec = self
            .widgets
            .borrow_mut()
            .popup(idx, &self.services)
            .unwrap_or_default();
        match self.panel.as_mut() {
            Some(panel) => panel.set_spec(spec, tcx),
            None => self.panel = Some(Panel::new(spec, tcx)),
        }
    }

    /// Where the panel wants to be: centred under its pill, then kept on
    /// screen. What the shape springs toward rather than where it is drawn.
    fn target_bounds(&self, anchor_x: f64, surface_w: f64) -> Option<Rect> {
        let (width, height) = self.panel.as_ref()?.size();
        let (width, height) = (width as f64, height as f64);
        let x = (anchor_x - width * 0.5)
            .clamp(SCREEN_MARGIN, (surface_w - width - SCREEN_MARGIN).max(SCREEN_MARGIN))
            .round();
        Some(Rect::new(x, PANEL_GAP, x + width, PANEL_GAP + height))
    }

    /// Hand an action to the widget that owns the panel. Returns whether the
    /// surface should repaint.
    fn dispatch(&mut self, action: PopupAction) -> bool {
        let Some(idx) = self.shown else {
            return false;
        };
        let after = {
            let mut registry = self.widgets.borrow_mut();
            match registry.widgets.get_mut(idx) {
                Some(rt) => rt.widget.on_popup(action, &self.services),
                None => AfterAction::Close,
            }
        };
        // A slider mid-drag has already been updated in place; rebuilding the
        // spec on every pointer sample would allocate a fresh panel at pointer
        // rate for no visible gain.
        let mid_drag = matches!(action, PopupAction::Slide { commit: false, .. });
        self.stale |= !mid_drag;
        if after == AfterAction::Close {
            self.state.borrow_mut().close();
        }
        true
    }

    fn set_hovered(&mut self, hovered: Option<usize>) -> bool {
        if self.hovered == hovered {
            return false;
        }
        self.hovered = hovered;
        true
    }
}

impl SurfaceHandler for PopupHandler {
    fn paint(&mut self, scene: &mut Scene, ctx: SurfaceCtx<'_>) {
        let size = ctx.size;
        self.seen_theme = theme::epoch();
        self.seen_services = self.services.epoch();
        let anchor_x = self.sync(&mut *ctx.text) as f64;

        if self.shown.is_none() && self.motion.invisible() {
            // Nothing drawn: the buffer is fully transparent. Dropping the
            // input region and the material is what makes a mapped surface
            // that is holding its GPU state cost nothing while it waits.
            self.panel = None;
            ctx.set_input_region(&[]);
            ctx.set_material(&[], &MaterialSpec::default());
            return;
        }

        let Some(target) = self.target_bounds(anchor_x, size.0 as f64) else {
            return;
        };
        self.motion.reshape(target);
        let frame = self.motion.frame(anchor_x);
        self.origin = frame.bounds.origin();

        let mut palette = theme::palette();
        if !ctx.material_capabilities().contains(Capability::Blur) {
            palette.panel_bg = theme::opaque(palette.panel_bg);
        }

        // Open to any degree: the whole surface takes pointer input, so a
        // click outside the panel is a dismissal rather than a click through
        // to whatever is behind it.
        ctx.set_input_region(&[Rect::new(0.0, 0.0, size.0 as f64, size.1 as f64)]);
        ctx.set_material(
            &[RoundedRect::from_rect(frame.visible(), panel::RADIUS)],
            &material(&palette, frame.alpha),
        );

        let Some(panel) = self.panel.as_mut() else {
            return;
        };
        // Encoded from zero, so the rows can be pinned to whichever corner the
        // shape has this frame rather than the one it will come to rest at.
        self.content.reset();
        panel.draw(
            &mut self.content,
            Point::ZERO,
            self.hovered,
            &palette,
            &mut *ctx.text,
        );

        let rows = frame.transform * Affine::translate(frame.bounds.origin().to_vec2());
        if self.motion.at_rest() && frame.crossfade >= 1.0 {
            panel::body(scene, frame.transform, frame.bounds, &palette);
            scene.append(&self.content, Some(rows));
            return;
        }

        // Clipped to the shape rather than the surface: a full-screen layer
        // would make the compositor's cheapest frame its dearest.
        let clip = Clip {
            shape: RoundedRect::from_rect(frame.bounds, panel::RADIUS),
            transform: frame.transform,
        };
        scene.push_layer(Fill::NonZero, Mix::Normal, frame.alpha, clip.transform, &clip.shape);
        panel::body(scene, frame.transform, frame.bounds, &palette);
        if frame.crossfade < 1.0 {
            append_faded(scene, &self.outgoing, rows, 1.0 - frame.crossfade, &clip);
        }
        append_faded(scene, &self.content, rows, frame.crossfade, &clip);
        scene.pop_layer();
    }

    fn on_frame(&mut self, _ctx: SurfaceCtx<'_>) -> bool {
        let dt = self.clock.tick();
        let mut busy = theme::is_animating() | self.motion.step(dt);
        if let Some(panel) = self.panel.as_mut() {
            // The header switch and the volume slider spring toward whatever
            // the last rebuild put in the spec.
            busy |= panel.step(dt);
        }
        if let Some(idx) = self.shown {
            let mut registry = self.widgets.borrow_mut();
            if let Some(rt) = registry.widgets.get_mut(idx) {
                // Collect a reading that landed since the last frame, so one
                // that finishes mid-animation shows up straight away rather
                // than waiting for the next tick.
                self.stale |= rt.widget.popup_poll(false);
                busy |= rt.widget.popup_busy();
            }
        }
        busy || self.stale
    }

    fn on_tick(&mut self, _ctx: SurfaceCtx<'_>) -> bool {
        let Some(idx) = self.shown else {
            return false;
        };
        let mut registry = self.widgets.borrow_mut();
        let Some(rt) = registry.widgets.get_mut(idx) else {
            return false;
        };
        // The slow tick is where a widget kicks off a fresh reading. Report
        // a started read as worth a frame too: that puts the surface back on
        // the frame clock, where `on_frame` collects the result as soon as it
        // lands instead of at the following tick.
        let changed = rt.widget.popup_poll(true);
        let busy = rt.widget.popup_busy();
        drop(registry);
        self.stale |= changed;
        changed || busy
    }

    fn on_pointer_press(&mut self, x: f64, y: f64, _ctx: SurfaceCtx<'_>) -> bool {
        let Some(panel) = self.panel.as_mut() else {
            return false;
        };
        if !panel.rect(self.origin).contains(Point::new(x, y)) {
            self.state.borrow_mut().close();
            return true;
        }
        let point = Point::new(x - self.origin.x, y - self.origin.y);

        let action = if let Some(row) = panel.switch_at(point) {
            panel
                .toggle_state(row)
                .map(|on| PopupAction::Toggle { row, on: !on })
        } else if let Some((row, months)) = panel.page_at(point) {
            Some(PopupAction::Page { row, months })
        } else if let Some((row, value)) = panel.slider_at(point) {
            // Move the knob to the pointer straight away; the reading that
            // comes back from the audio server is a second behind at best.
            panel.set_slider_value(row, value);
            self.dragging = Some(row);
            Some(PopupAction::Slide {
                row,
                value,
                commit: false,
            })
        } else {
            // A separator or a header: not something to act on, and not a
            // dismissal either.
            panel.row_at(point).map(|row| PopupAction::Activate { row })
        };

        match action {
            Some(action) => self.dispatch(action),
            None => false,
        }
    }

    fn on_pointer_release(&mut self, x: f64, _y: f64, _ctx: SurfaceCtx<'_>) -> bool {
        let Some(row) = self.dragging.take() else {
            return false;
        };
        let origin = self.origin;
        let Some(panel) = self.panel.as_mut() else {
            return false;
        };
        let value = panel.slider_value_at(row, x - origin.x);
        panel.set_slider_value(row, value);
        self.dispatch(PopupAction::Slide {
            row,
            value,
            commit: true,
        })
    }

    fn on_pointer_motion(&mut self, x: f64, y: f64, _ctx: SurfaceCtx<'_>) -> bool {
        let point = Point::new(x - self.origin.x, y - self.origin.y);
        if let Some(row) = self.dragging {
            let Some(panel) = self.panel.as_mut() else {
                return false;
            };
            let value = panel.slider_value_at(row, point.x);
            panel.set_slider_value(row, value);
            return self.dispatch(PopupAction::Slide {
                row,
                value,
                commit: false,
            });
        }
        // Only hit-test once the panel has stopped moving: hovering a target
        // that is still sliding under the pointer is worse than not hovering.
        let hovered = match (self.panel.as_ref(), self.motion.at_rest()) {
            (Some(panel), true) => panel.row_at(point),
            _ => None,
        };
        self.set_hovered(hovered)
    }

    fn on_pointer_enter(&mut self, x: f64, y: f64, ctx: SurfaceCtx<'_>) -> bool {
        self.on_pointer_motion(x, y, ctx)
    }

    fn on_pointer_leave(&mut self, _ctx: SurfaceCtx<'_>) -> bool {
        self.dragging = None;
        self.set_hovered(None)
    }

    fn needs_redraw(&self) -> bool {
        self.state.borrow().version != self.seen
            || self.stale
            || theme::epoch() != self.seen_theme
            || self.state.borrow().content() != self.seen_content
    }
}

/// What the compositor renders under the panel: the blurred, vibrant backdrop
/// the body's transparency shows through, the refractive rim along its edge,
/// and the shadow it casts. The blur comes up with the panel — a radius that
/// tracks the fade is what keeps the wallpaper from going frosted before there
/// is anything sitting on it.
fn material(palette: &theme::Palette, alpha: f32) -> MaterialSpec {
    MaterialSpec {
        blur: Some(Blur {
            radius: BLUR_RADIUS * alpha as f64,
            saturation: BLUR_VIBRANCY,
            ..Default::default()
        }),
        shadow: Some(Shadow {
            radius: SHADOW_BLUR,
            offset: Vec2::new(0.0, SHADOW_DY),
            color: fade(palette.panel_shadow, alpha),
        }),
        border: RIM,
    }
}

/// The panel's shape as vello takes a clip: the rect and the transform it is
/// seen through, which every layer of one frame shares.
struct Clip {
    shape: RoundedRect,
    transform: Affine,
}

/// One panel's rows at `alpha`, inside the shape they belong to.
fn append_faded(scene: &mut Scene, rows: &Scene, place: Affine, alpha: f32, clip: &Clip) {
    scene.push_layer(Fill::NonZero, Mix::Normal, alpha, clip.transform, &clip.shape);
    scene.append(rows, Some(place));
    scene.pop_layer();
}

fn fade(color: Color, alpha: f32) -> Color {
    let c = color.components;
    Color::new([c[0], c[1], c[2], c[3] * alpha.clamp(0.0, 1.0)])
}

//! The surface every widget popup is drawn on, and the state the bar and that
//! surface share.
//!
//! There is exactly one popup surface for the life of the process: an overlay
//! layer anchored to all four edges, so the compositor sizes it to the area
//! below the bar. Covering that whole area is what lets a panel be drawn
//! anywhere in it, animate without a resize round trip, and dismiss from a
//! click outside itself — that click is an ordinary pointer event on the same
//! surface. While no panel is up the surface draws nothing and drops both its
//! input and blur regions, so it costs the compositor nothing while it waits.
//!
//! The bar and the popup are separate [`SurfaceHandler`]s and only a surface
//! can repaint itself, so a click on a pill reaches the panel through
//! [`PopupState`]: the bar bumps a version, and [`SurfaceHandler::needs_redraw`]
//! on the popup picks it up at the end of the same event-loop iteration.

use std::{cell::RefCell, rc::Rc};

use crownshell::{Scene, SurfaceCtx, SurfaceHandler};
use vello::{
    kurbo::{Affine, Point, Rect},
    peniko::{Fill, Mix},
};

use crate::{
    services::Services,
    animation::{Clock, Spring},
    theme,
    ui::panel::{self, Panel},
    widgets::{AfterAction, PopupAction, WidgetRegistry},
};

/// Gap between the bar's lower edge — the top of this surface — and the panel.
const PANEL_GAP: f64 = 4.0;
/// Smallest distance the panel keeps from the screen's left/right edges.
const SCREEN_MARGIN: f64 = 8.0;
/// How much of its final size the panel starts at.
const START_SCALE: f64 = 0.94;
/// How far above its resting place the panel starts.
const START_RISE: f64 = 6.0;
const SHADOW_DY: f64 = 6.0;
const SHADOW_BLUR: f64 = 16.0;
/// Below this the panel is treated as gone: nothing drawn, no regions set.
const HIDDEN: f32 = 0.001;

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
    /// Where the panel's top-left corner sits, in surface px.
    origin: Point,
    /// 0 = dismissed, 1 = fully out. Springs between the two.
    open: Spring,
    clock: Clock,
    hovered: Option<usize>,
    /// Slider row the pointer is currently dragging.
    dragging: Option<usize>,
    /// The widget's contents changed and the panel has to be rebuilt.
    stale: bool,
    /// Scratch scene for the panel, appended under the animation's transform
    /// so a frame costs re-encoding and no text shaping.
    content: Scene,
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
            open: Spring::new(0.0),
            clock: Clock::new(),
            hovered: None,
            dragging: None,
            stale: false,
            content: Scene::new(),
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
                    self.rebuild(idx, tcx);
                    self.open.set_target(1.0);
                }
                None => {
                    self.open.set_target(0.0);
                }
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

    /// Left-align the panel under its pill, then keep it on screen.
    fn place(&mut self, anchor_x: f32, surface_w: f64) {
        let Some(panel) = self.panel.as_ref() else {
            return;
        };
        let width = panel.size().0 as f64;
        let x = (anchor_x as f64 - width * 0.5).clamp(
            SCREEN_MARGIN,
            (surface_w - width - SCREEN_MARGIN).max(SCREEN_MARGIN),
        );
        self.origin = Point::new(x.round(), PANEL_GAP);
    }

    fn hidden(&self) -> bool {
        self.shown.is_none() && self.open.position <= HIDDEN && self.open.at_rest()
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
        let palette = theme::palette();
        let anchor_x = self.sync(&mut *ctx.text);

        if self.hidden() {
            // Nothing drawn: the buffer is fully transparent. Dropping both
            // regions is what makes a mapped surface that is holding its GPU
            // state cost nothing while it waits.
            self.panel = None;
            ctx.set_input_region(&[]);
            ctx.set_blur_region(&[]);
            return;
        }

        self.place(anchor_x, size.0 as f64);
        let Some(rect) = self.panel.as_ref().map(|p| p.rect(self.origin)) else {
            return;
        };

        let progress = self.open.position.clamp(0.0, 1.0) as f64;
        // Grow from the point on the panel's top edge nearest the pill it
        // belongs to, so it reads as coming out of that icon.
        let pivot = Point::new((anchor_x as f64).clamp(rect.x0, rect.x1), rect.y0);
        let transform = Affine::translate((0.0, -START_RISE * (1.0 - progress)))
            * Affine::scale_about(START_SCALE + (1.0 - START_SCALE) * progress, pivot);
        let visible = transform.transform_rect_bbox(rect);
        // Alpha leads the scale: by the time the panel is halfway out it is
        // already solid, which reads as quicker than it is.
        let alpha = (progress * 1.8).min(1.0) as f32;

        // Open to any degree: the whole surface takes pointer input, so a
        // click outside the panel is a dismissal rather than a click through
        // to whatever is behind it.
        ctx.set_input_region(&[Rect::new(0.0, 0.0, size.0 as f64, size.1 as f64)]);
        ctx.set_blur_region(&rounded_bands(visible, panel::RADIUS));

        scene.draw_blurred_rounded_rect(
            Affine::translate((0.0, SHADOW_DY)) * transform,
            rect,
            fade(palette.panel_shadow, alpha),
            panel::RADIUS,
            SHADOW_BLUR,
        );

        let Some(panel) = self.panel.as_mut() else {
            return;
        };
        self.content.reset();
        panel.draw(
            &mut self.content,
            self.origin,
            self.hovered,
            &palette,
            &mut *ctx.text,
        );
        if alpha < 1.0 {
            // Clipped to the panel rather than the surface: a full-screen
            // layer would make the compositor's cheapest frame its dearest.
            let clip = visible.inflate(1.0, 1.0);
            scene.push_layer(Fill::NonZero, Mix::Normal, alpha, Affine::IDENTITY, &clip);
            scene.append(&self.content, Some(transform));
            scene.pop_layer();
        } else {
            scene.append(&self.content, Some(transform));
        }
    }

    fn on_frame(&mut self, _ctx: SurfaceCtx<'_>) -> bool {
        let dt = self.clock.tick();
        let mut busy = theme::is_animating();
        if !self.open.at_rest() {
            self.open.step(dt);
            busy = true;
        }
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
        let hovered = match (self.panel.as_ref(), self.open.at_rest()) {
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

fn fade(color: vello::peniko::Color, alpha: f32) -> vello::peniko::Color {
    let c = color.components;
    vello::peniko::Color::new([c[0], c[1], c[2], c[3] * alpha.clamp(0.0, 1.0)])
}

/// A `wl_region` is a set of rectangles, so a rounded corner cannot be
/// described exactly. Three bands inset at the ends keep the blur from
/// squaring off the corners, which is the only place the difference shows.
fn rounded_bands(rect: Rect, radius: f64) -> [Rect; 3] {
    let radius = radius.min(rect.width() / 2.0).min(rect.height() / 2.0);
    [
        Rect::new(
            rect.x0 + radius,
            rect.y0,
            rect.x1 - radius,
            rect.y0 + radius,
        ),
        Rect::new(rect.x0, rect.y0 + radius, rect.x1, rect.y1 - radius),
        Rect::new(
            rect.x0 + radius,
            rect.y1 - radius,
            rect.x1 - radius,
            rect.y1,
        ),
    ]
}

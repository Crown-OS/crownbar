use std::{cell::RefCell, rc::Rc};

use crownshell::{Scene, SurfaceCtx, SurfaceHandler};

use crate::{
    animation::Clock,
    popup_handler::PopupState,
    services::Services,
    theme,
    ui::BarPainter,
    widgets::WidgetRegistry,
};

pub struct BarHandler {
    /// Shared with the popup surface, which reads the open widget's panel out
    /// of it and dispatches the pointer back into the same widget.
    widgets: Rc<RefCell<WidgetRegistry>>,
    popup: Rc<RefCell<PopupState>>,
    services: Rc<Services>,
    /// Service generation this surface last painted. A snapshot landing on the
    /// event loop is not an event on any surface, so this is what notices it.
    seen_services: u64,
    /// Version of the popup state this surface last painted, so the pill can
    /// come back off when the panel dismisses itself from a click outside it.
    seen: u64,
    /// Palette generation this surface last painted. A theme change is not an
    /// event on any surface, so this is what notices it — see [`crate::theme`].
    seen_theme: u64,
    painter: BarPainter,
    anim_clock: Clock,
}

impl BarHandler {
    pub fn new(
        widgets: Rc<RefCell<WidgetRegistry>>,
        popup: Rc<RefCell<PopupState>>,
        services: Rc<Services>,
    ) -> Self {
        Self {
            widgets,
            popup,
            services,
            seen_services: 0,
            seen: 0,
            seen_theme: 0,
            painter: BarPainter::new(),
            anim_clock: Clock::new(),
        }
    }

    fn hover_at(&mut self, x: f64, y: f64) -> bool {
        let mut widgets = self.widgets.borrow_mut();
        let hit = widgets.hit_test(x as f32, y as f32);
        if widgets.set_hovered(hit) {
            drop(widgets);
            self.anim_clock.reset();
            true
        } else {
            false
        }
    }
}

impl SurfaceHandler for BarHandler {
    fn paint(&mut self, scene: &mut Scene, ctx: SurfaceCtx<'_>) {
        let bar = ctx.size;

        let popup = self.popup.borrow();
        self.seen = popup.version();
        let active = popup.owner();
        drop(popup);

        // Sampled once per frame and threaded down: mid-cross-fade every slot
        // is different on every frame, so nothing below may cache it.
        self.seen_theme = theme::epoch();
        self.seen_services = self.services.epoch();
        let palette = theme::palette();

        let text = &mut *ctx.text;
        let mut widgets = self.widgets.borrow_mut();
        self.painter.layout_widgets(&mut widgets, bar, text);
        self.painter
            .build_scene(scene, &widgets, active, bar, &palette, text);
    }

    fn on_pointer_enter(&mut self, x: f64, y: f64, _ctx: SurfaceCtx<'_>) -> bool {
        self.hover_at(x, y)
    }

    fn on_pointer_leave(&mut self, _ctx: SurfaceCtx<'_>) -> bool {
        if self.widgets.borrow_mut().clear_hover() {
            self.anim_clock.reset();
            true
        } else {
            false
        }
    }

    fn on_pointer_motion(&mut self, x: f64, y: f64, _ctx: SurfaceCtx<'_>) -> bool {
        self.hover_at(x, y)
    }

    fn on_pointer_press(&mut self, x: f64, y: f64, _ctx: SurfaceCtx<'_>) -> bool {
        let mut widgets = self.widgets.borrow_mut();
        let Some(idx) = widgets.hit_test(x as f32, y as f32) else {
            // A click on bare bar dismisses whatever panel is up, the same way
            // a click anywhere outside it does.
            drop(widgets);
            let mut popup = self.popup.borrow_mut();
            let was_open = popup.owner().is_some();
            popup.close();
            return was_open;
        };

        // A widget with a panel opens it; one without gets the click. Whether
        // it has one is answered by asking for it, so there is no separate
        // "do you have one" method to keep in step with `popup`.
        let anchor_x = widgets.anchor_x(idx).unwrap_or(x as f32);
        let open_here = self.popup.borrow().owner() == Some(idx);
        if open_here || widgets.has_popup(idx, &self.services) {
            drop(widgets);
            self.popup.borrow_mut().toggle(idx, anchor_x);
            return true;
        }

        let changed = widgets.click(idx, &self.services);
        drop(widgets);
        if changed {
            self.anim_clock.reset();
        }
        changed
    }

    fn on_tick(&mut self, _ctx: SurfaceCtx<'_>) -> bool {
        self.widgets.borrow_mut().tick()
    }

    fn on_frame(&mut self, _ctx: SurfaceCtx<'_>) -> bool {
        let dt = self.anim_clock.tick();
        let busy = self.widgets.borrow_mut().step_animations(dt);
        // A cross-fade has no events of its own; keeping the surface on the
        // frame clock for its duration is what makes it a fade.
        busy || theme::is_animating()
    }

    fn needs_redraw(&self) -> bool {
        self.popup.borrow().version() != self.seen
            || theme::epoch() != self.seen_theme
            || self.services.epoch() != self.seen_services
    }
}

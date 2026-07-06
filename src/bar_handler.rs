use crownshell::{Scene, SurfaceCtx, SurfaceHandler};

use crate::{
    animation::Clock,
    ui::BarPainter,
    widgets::WidgetRegistry,
};

pub struct BarHandler {
    widgets: WidgetRegistry,
    painter: BarPainter,
    anim_clock: Clock,
}

impl BarHandler {
    pub fn new(widgets: WidgetRegistry) -> Self {
        Self {
            widgets,
            painter: BarPainter::new(),
            anim_clock: Clock::new(),
        }
    }
}

impl SurfaceHandler for BarHandler {
    fn paint(&mut self, scene: &mut Scene, ctx: SurfaceCtx<'_>) {
        self.painter.layout_widgets(&mut self.widgets, ctx.size);
        self.painter.build_scene(scene, &self.widgets, ctx.size);
    }

    fn on_pointer_enter(&mut self, x: f64, y: f64, _ctx: SurfaceCtx<'_>) -> bool {
        let hit = self.widgets.hit_test(x as f32, y as f32);
        if self.widgets.set_hovered(hit) {
            self.anim_clock.reset();
            true
        } else {
            false
        }
    }

    fn on_pointer_leave(&mut self, _ctx: SurfaceCtx<'_>) -> bool {
        if self.widgets.clear_hover() {
            self.anim_clock.reset();
            true
        } else {
            false
        }
    }

    fn on_pointer_motion(&mut self, x: f64, y: f64, _ctx: SurfaceCtx<'_>) -> bool {
        let hit = self.widgets.hit_test(x as f32, y as f32);
        if self.widgets.set_hovered(hit) {
            self.anim_clock.reset();
            true
        } else {
            false
        }
    }

    fn on_pointer_press(&mut self, x: f64, y: f64, _ctx: SurfaceCtx<'_>) -> bool {
        let Some(idx) = self.widgets.hit_test(x as f32, y as f32) else {
            return false;
        };
        if self.widgets.click(idx) {
            self.anim_clock.reset();
            true
        } else {
            false
        }
    }

    fn on_tick(&mut self, _ctx: SurfaceCtx<'_>) -> bool {
        self.widgets.tick()
    }

    fn on_frame(&mut self, _ctx: SurfaceCtx<'_>) -> bool {
        let dt = self.anim_clock.tick();
        self.widgets.step_animations(dt)
    }
}

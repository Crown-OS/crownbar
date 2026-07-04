use std::time::Duration;

use calloop::{LoopHandle, RegistrationToken, timer::Timer};
use smithay_client_toolkit::{
    compositor::{CompositorState, Region},
    output::OutputState,
    registry::RegistryState,
    seat::SeatState,
    shell::{
        WaylandSurface,
        wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell, LayerSurface},
    },
};
use wayland_client::{
    Connection, EventQueue, QueueHandle, globals::registry_queue_init,
    protocol::{wl_output::WlOutput, wl_pointer::WlPointer},
};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;

use crate::{
    animation::Clock,
    config::{BAR_HEIGHT, BAR_NAMESPACE},
    ui::Ui,
    wayland::background_effect::BackgroundEffect,
    widgets::WidgetRegistry,
};

const TICK_INTERVAL: Duration = Duration::from_millis(1000);

pub struct Window {
    // Ui (which owns the wgpu Surface) is dropped first because it holds raw
    // pointers into the wl_surface owned by `layer`.
    pub ui: Option<Ui>,
    pub registry_state: RegistryState,
    pub output_state: OutputState,
    pub seat_state: SeatState,
    pub compositor_state: CompositorState,
    // Kept alive so the LayerSurface (which holds a reference to it) stays valid.
    #[allow(dead_code)]
    pub layer_shell: LayerShell,
    pub layer: LayerSurface,
    pub pointer: Option<WlPointer>,
    pub qh: QueueHandle<Window>,
    pub loop_handle: LoopHandle<'static, Window>,
    pub width: u32,
    pub height: u32,
    pub exit: bool,
    pub first_configure: bool,
    pub frame_pending: bool,
    pub widgets: WidgetRegistry,
    pub tick_timer: Option<RegistrationToken>,
    pub anim_clock: Clock,
    pub background_effect: Option<BackgroundEffect>,
    pub bg_effect_surface: Option<ExtBackgroundEffectSurfaceV1>,
    /// Last known pointer position in surface-local px.
    pub pointer_pos: Option<(f64, f64)>,
}

impl Window {
    pub fn new(
        loop_handle: LoopHandle<'static, Window>,
        widgets: WidgetRegistry,
    ) -> anyhow::Result<(Connection, EventQueue<Window>, Window)> {
        let connection = Connection::connect_to_env()?;
        let (globals, event_queue) = registry_queue_init(&connection)?;
        let qh: QueueHandle<Window> = event_queue.handle();

        let compositor_state = CompositorState::bind(&globals, &qh)?;
        let layer_shell = LayerShell::bind(&globals, &qh)?;
        let seat_state = SeatState::new(&globals, &qh);

        let layer = Self::build_layer(&compositor_state, &layer_shell, &qh, None);

        let background_effect = BackgroundEffect::bind(&globals, &qh);
        let bg_effect_surface = background_effect
            .as_ref()
            .map(|bg| bg.manager.get_background_effect(layer.wl_surface(), &qh, ()));

        let state = Self {
            ui: None,
            registry_state: RegistryState::new(&globals),
            output_state: OutputState::new(&globals, &qh),
            seat_state,
            compositor_state,
            layer_shell,
            layer,
            pointer: None,
            qh,
            loop_handle,
            width: 0,
            height: BAR_HEIGHT,
            exit: false,
            first_configure: true,
            frame_pending: false,
            widgets,
            tick_timer: None,
            anim_clock: Clock::new(),
            background_effect,
            bg_effect_surface,
            pointer_pos: None,
        };

        Ok((connection, event_queue, state))
    }

    fn build_layer(
        compositor_state: &CompositorState,
        layer_shell: &LayerShell,
        qh: &QueueHandle<Window>,
        output: Option<&WlOutput>,
    ) -> LayerSurface {
        let surface = compositor_state.create_surface(qh);
        let layer = layer_shell.create_layer_surface(
            qh,
            surface,
            Layer::Top,
            Some(BAR_NAMESPACE),
            output,
        );

        layer.set_anchor(Anchor::TOP | Anchor::LEFT | Anchor::RIGHT);
        layer.set_size(0, BAR_HEIGHT);
        layer.set_exclusive_zone(BAR_HEIGHT as i32);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.commit();

        layer
    }

    /// Tell the compositor to apply its background blur to the entire bar
    /// surface, so the strip behind us is frosted rather than transparent.
    pub fn apply_blur_region(&self) {
        let Some(effect_surface) = self.bg_effect_surface.as_ref() else {
            return;
        };
        let Some(bg) = self.background_effect.as_ref() else {
            return;
        };
        if !bg.supports_blur() {
            return;
        }
        let Ok(region) = Region::new(&self.compositor_state) else {
            return;
        };
        region.add(0, 0, self.width as i32, self.height as i32);
        effect_surface.set_blur_region(Some(region.wl_region()));
        self.layer.commit();
    }

    pub fn paint(&mut self) {
        let Some(ui) = self.ui.as_mut() else {
            return;
        };
        ui.layout_widgets(&mut self.widgets);
        if let Err(e) = ui.render(&self.widgets) {
            log::error!("render failed: {e}");
        }
    }

    pub fn request_frame(&mut self) {
        if self.frame_pending {
            return;
        }
        self.frame_pending = true;
        self.layer
            .wl_surface()
            .frame(&self.qh, self.layer.wl_surface().clone());
        self.paint();
    }

    pub fn on_frame(&mut self) {
        self.frame_pending = false;
        let dt = self.anim_clock.tick();
        let in_flight = self.widgets.step_animations(dt);
        if in_flight {
            self.layer
                .wl_surface()
                .frame(&self.qh, self.layer.wl_surface().clone());
            self.frame_pending = true;
            self.paint();
        } else {
            self.anim_clock.reset();
            self.paint();
        }
    }

    pub fn arm_tick_timer(&mut self) {
        if self.tick_timer.is_some() {
            return;
        }
        let timer = Timer::from_duration(TICK_INTERVAL);
        let token = self
            .loop_handle
            .insert_source(timer, |_deadline, _, window: &mut Window| {
                window.on_tick();
                calloop::timer::TimeoutAction::ToDuration(TICK_INTERVAL)
            });
        match token {
            Ok(t) => self.tick_timer = Some(t),
            Err(e) => log::warn!("failed to install tick timer: {e}"),
        }
    }

    fn on_tick(&mut self) {
        if self.widgets.tick() {
            self.request_frame();
        }
    }

    // ---- Pointer hover ----

    pub fn on_pointer_enter(&mut self, x: f64, y: f64) {
        self.pointer_pos = Some((x, y));
        self.update_hover(x as f32, y as f32);
    }

    pub fn on_pointer_leave(&mut self) {
        self.pointer_pos = None;
        if self.widgets.clear_hover() {
            self.anim_clock.reset();
            self.request_frame();
        }
    }

    pub fn on_pointer_motion(&mut self, x: f64, y: f64) {
        self.pointer_pos = Some((x, y));
        self.update_hover(x as f32, y as f32);
    }

    fn update_hover(&mut self, x: f32, y: f32) {
        let idx = self.widgets.hit_test(x, y);
        if self.widgets.set_hovered(idx) {
            self.anim_clock.reset();
            self.request_frame();
        }
    }

    pub fn on_pointer_click(&mut self, x: f64, y: f64) {
        self.pointer_pos = Some((x, y));
        let Some(idx) = self.widgets.hit_test(x as f32, y as f32) else {
            return;
        };
        if self.widgets.click(idx) {
            self.anim_clock.reset();
            self.request_frame();
        }
    }
}

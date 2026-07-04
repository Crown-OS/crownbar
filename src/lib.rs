mod animation;
mod app;
mod config;
mod renderer;
mod theme;
mod ui;
mod util;
mod wayland;
mod widgets;
mod window;

use anyhow::{anyhow, Result};
use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;

use renderer::Renderer;
use ui::Ui;
use widgets::{
    battery::BatteryWidget, bluetooth::BluetoothWidget, brightness::BrightnessWidget,
    clock::ClockWidget, layout::LayoutWidget, volume::VolumeWidget, wifi::WifiWidget, BarWidget,
    WidgetRegistry,
};
use window::Window;

pub fn app() -> Result<()> {
    let mut event_loop: EventLoop<'static, Window> = EventLoop::try_new()?;
    let loop_handle = event_loop.handle();

    // Right-slot widgets render in registration order, left-to-right. This
    // ordering mirrors the conventional "status indicators → clock" layout.
    let mut widgets = WidgetRegistry::new();
    widgets.register(Box::new(LayoutWidget::new(false)) as Box<dyn BarWidget>);
    if let Some(w) = BluetoothWidget::try_new() {
        widgets.register(Box::new(w) as Box<dyn BarWidget>);
    }
    if let Some(w) = VolumeWidget::try_new() {
        widgets.register(Box::new(w) as Box<dyn BarWidget>);
    }
    if let Some(w) = BrightnessWidget::try_new() {
        widgets.register(Box::new(w) as Box<dyn BarWidget>);
    }
    if let Some(w) = WifiWidget::try_new() {
        widgets.register(Box::new(w) as Box<dyn BarWidget>);
    }
    if let Some(w) = BatteryWidget::try_new() {
        widgets.register(Box::new(w) as Box<dyn BarWidget>);
    }
    widgets.register(Box::new(ClockWidget::new()));

    let (connection, mut event_queue, mut window) = Window::new(loop_handle.clone(), widgets)?;

    while window.first_configure {
        event_queue.blocking_dispatch(&mut window)?;
    }

    let renderer = Renderer::new(&connection, &window)?;
    window.ui = Some(Ui::new(renderer)?);
    window.apply_blur_region();
    window.paint();
    window.arm_tick_timer();

    WaylandSource::new(connection, event_queue)
        .insert(loop_handle.clone())
        .map_err(|e| anyhow!("register wayland source: {}", e.error))?;

    let signal = event_loop.get_signal();
    event_loop.run(None, &mut window, move |window| {
        if window.exit {
            signal.stop();
        }
    })?;

    Ok(())
}

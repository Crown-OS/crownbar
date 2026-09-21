mod animation;
mod bar_handler;
mod ipc;
mod popup_handler;
pub mod services;
pub mod theme;
mod ui;
mod util;
mod widgets;

use std::{cell::RefCell, rc::Rc};

use anyhow::{anyhow, Result};
use config::{BarConfig, BAR_NAMESPACE, POPUP_NAMESPACE};
use crownshell::{calloop, Anchor, KeyboardInteractivity, Layer, WindowConfig};

use bar_handler::BarHandler;
use services::{Services, Wake};
use popup_handler::{PopupHandler, PopupState};
use widgets::{
    battery::BatteryWidget, bluetooth::BluetoothWidget, brightness::BrightnessWidget,
    caffeine::CaffeineWidget, clock::ClockWidget, layout::LayoutWidget, volume::VolumeWidget,
    wifi::WifiWidget, BarWidget, WidgetRegistry,
};

pub fn app() -> Result<()> {
    // Right-slot widgets render in registration order, left-to-right.
    let mut widgets = WidgetRegistry::new();
    widgets.register(Box::new(LayoutWidget::new(false)) as Box<dyn BarWidget>);

    widgets.register(Box::new(CaffeineWidget::new()) as Box<dyn BarWidget>);
    widgets.register(Box::new(VolumeWidget::new()) as Box<dyn BarWidget>);
    widgets.register(Box::new(BrightnessWidget::new()) as Box<dyn BarWidget>);
    widgets.register(Box::new(BluetoothWidget::new()) as Box<dyn BarWidget>);
    widgets.register(Box::new(WifiWidget::new()) as Box<dyn BarWidget>);
    widgets.register(Box::new(BatteryWidget::new()) as Box<dyn BarWidget>);
    widgets.register(Box::new(ClockWidget::new()));

    // The registry is shared: the popup surface reads the open widget's panel
    // out of it and dispatches the pointer back into the same widget.
    let widgets = Rc::new(RefCell::new(widgets));
    let popup = PopupState::shared();

    // The palette, before the first frame, so nothing fades in from the kit's
    // default — and then for the life of the process, so it follows the file.
    let appearance = theme::load();
    theme::follow_config();
    let bar_config = BarConfig::from_appearance(&appearance);

    crownshell::run(move |app| {
        // One ping for every service. A snapshot is already published by the
        // time this fires, so the wake carries no payload of its own — it only
        // moves the epoch the two surfaces compare in `needs_redraw`.
        let (ping, wakeups) = calloop::ping::make_ping()?;
        let services = Rc::new(Services::start(Wake::new(ping))?);

        let window_config = WindowConfig {
            namespace: BAR_NAMESPACE.to_string(),
            layer: Layer::Top,
            anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
            size: (0, bar_config.bar_height),
            exclusive_zone: bar_config.bar_height as i32,
            keyboard_interactivity: KeyboardInteractivity::None,
            // No backdrop blur: the bar paints its own gradient, which is
            // the whole of what sits behind the widgets.
            blur: false,
            ..Default::default()
        };
        app.create_window(
            window_config,
            BarHandler::new(widgets.clone(), popup.clone(), services.clone()),
        );

        // One popup surface for the life of the process, created now so its
        // wgpu surface, Vello pipelines and shaped text all exist before the
        // first click. Anchored on all four sides at size (0, 0) it fills the
        // output; leaving `exclusive_zone` at 0 means the compositor shrinks
        // it clear of the bar, so it starts immediately below it however much
        // space other clients have reserved as well.
        app.create_window(
            WindowConfig {
                namespace: POPUP_NAMESPACE.to_string(),
                layer: Layer::Overlay,
                anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
                size: (0, 0),
                keyboard_interactivity: KeyboardInteractivity::None,
                blur: true,
                // A panel is a small part of a full-screen surface, so the
                // handler drives the blur region per frame instead.
                auto_blur_region: false,
                ..Default::default()
            },
            PopupHandler::new(widgets.clone(), popup.clone(), services.clone()),
        );

        // The loop owns this closure until `run` returns, and the closure owns
        // the last `Rc<Services>` — so the tokio runtime, the PipeWire loop and
        // every live subscription are torn down exactly when the loop ends.
        app.loop_handle()
            .insert_source(wakeups, move |_, _, app: &mut crownshell::App| {
                services.woke();
                // Caffeine's inhibitor is a Wayland object on the bar's own
                // surface, so the intent the service holds is turned into one
                // here rather than on the runtime.
                services.reconcile(app);
                if widgets.borrow_mut().sync(&services) {
                    popup.borrow_mut().invalidate();
                }
            })
            .map_err(|e| anyhow!("could not watch for service updates: {}", e.error))?;
        Ok(())
    })
}

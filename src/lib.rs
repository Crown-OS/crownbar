mod animation;
mod bar_handler;
mod config;
mod theme;
mod ui;
mod util;
mod widgets;

use anyhow::Result;
use crownos_config::{schema::appearance, subscribe_key};
use crownshell::{Anchor, KeyboardInteractivity, Layer, SurfaceHandler, WindowConfig};

use bar_handler::BarHandler;
use config::{BAR_HEIGHT, BAR_NAMESPACE};
use widgets::{
    battery::BatteryWidget, bluetooth::BluetoothWidget, brightness::BrightnessWidget,
    clock::ClockWidget, layout::LayoutWidget, volume::VolumeWidget, wifi::WifiWidget, BarWidget,
    WidgetRegistry,
};

use crate::config::BarConfig;

pub fn app() -> Result<()> {
    // Right-slot widgets render in registration order, left-to-right.
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

    crownshell::run(move |app| {
        let bar_config = BarConfig::load();

        let window_config = WindowConfig {
            namespace: BAR_NAMESPACE.to_string(),
            layer: Layer::Top,
            anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
            size: (0, bar_config.bar_height),
            exclusive_zone: BAR_HEIGHT as i32,
            keyboard_interactivity: KeyboardInteractivity::None,
            blur: true,
            ..Default::default()
        };
        let bar = BarHandler::new(widgets);
        // subscribe_key(appearance::BarHeight, |config| {
        //     app.flush_redraws();
        // });
        app.create_window(window_config, bar);
        Ok(())
    })
}

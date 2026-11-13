//! How the bar's layer surface is described to the runner.

use crownbar_config::{BAR_NAMESPACE, BarConfig, BarPosition};
use crownui::{
    AppConfig, kit,
    prelude::{Anchor, Layer, LayerExtent, LayerOptions, TRANSPARENT},
};

use crate::{PANEL_GAP, theme};

/// The bar's window: a transparent layer surface along its edge, only the
/// strip tall until a panel opens (the surface then grows to the whole output).
pub fn app_config(bar_config: &BarConfig) -> AppConfig {
    let edge = match bar_config.position {
        BarPosition::Top => Anchor::TOP,
        BarPosition::Bottom => Anchor::BOTTOM,
    };
    let layer = LayerOptions::new(BAR_NAMESPACE, edge | Anchor::LEFT | Anchor::RIGHT)
        .layer(Layer::Top)
        .height(LayerExtent::Fixed(bar_config.reach()))
        .exclusive_zone(exclusive_zone(bar_config));
    AppConfig::new("CrownBar")
        .app_id(BAR_NAMESPACE)
        .size(1280, bar_config.bar_height.max(1) * 18)
        .clear_color(TRANSPARENT)
        .font(kit::inter_font().to_vec(), Some(kit::INTER_FAMILY))
        .accessibility(false)
        .corner_shape(theme::CORNER_SHAPE)
        .layer(layer)
}

/// The space the bar keeps windows out of: its reach, short of the gap its
/// panels hang in.
pub fn exclusive_zone(bar_config: &BarConfig) -> i32 {
    bar_config.reach() as i32 - PANEL_GAP as i32
}

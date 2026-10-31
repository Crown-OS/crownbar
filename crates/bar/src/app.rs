use anyhow::{Context, Result};
use crownbar_config::{BAR_NAMESPACE, BarConfig, BarPosition, BarSettings};
use crownbar_integrations::{Services, Wake};
use crownbar_ui::{PANEL_GAP, theme};
use crownui::{
    AppConfig, kit,
    prelude::{Anchor, Layer, LayerExtent, LayerOptions, TRANSPARENT},
};

pub fn run() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let appearance = crownconfig::load_appearance();
    let settings: BarSettings = crownconfig::load_section(BarSettings::SECTION);
    let bar_config = BarConfig::new(&settings, appearance.bar_height);

    // One wake for every service. A snapshot is already published by the time
    // it fires, so it carries no payload of its own.
    let wake = Wake::default();
    let services = Services::start(wake.clone())?;

    let edge = match bar_config.position {
        BarPosition::Top => Anchor::TOP,
        BarPosition::Bottom => Anchor::BOTTOM,
    };
    let layer = LayerOptions::new(BAR_NAMESPACE, edge | Anchor::LEFT | Anchor::RIGHT)
        .layer(Layer::Top)
        // The surface reaches across the whole output so a panel can open
        // anywhere beside the bar without a resize round trip; only the bar's
        // own extent is reserved from other windows.
        .height(LayerExtent::Output)
        .exclusive_zone(bar_config.reach() as i32 - PANEL_GAP as i32);
    let config = AppConfig::new("CrownBar")
        .app_id(BAR_NAMESPACE)
        .size(1280, bar_config.bar_height.max(1) * 18)
        .clear_color(TRANSPARENT)
        .font(kit::inter_font().to_vec(), Some(kit::INTER_FAMILY))
        .accessibility(false)
        .corner_shape(theme::CORNER_SHAPE)
        .layer(layer);

    crownui::run(
        crownbar_ui::root(services, wake, appearance, settings),
        config,
    )
    .context("the bar stopped")
}

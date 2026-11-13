use anyhow::{Context, Result};
use crownbar_config::{BarConfig, BarSettings};
use crownbar_integrations::{Services, Wake};

pub fn run() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let appearance = crownconfig::load_appearance();
    let settings: BarSettings = crownconfig::load_section(BarSettings::SECTION);
    let bar_config = BarConfig::new(&settings, appearance.bar_height);

    // One wake for every service. A snapshot is already published by the time
    // it fires, so it carries no payload of its own.
    let wake = Wake::default();
    let services = Services::start(wake.clone())?;

    let config = crownbar_ui::app_config(&bar_config);

    crownui::run(
        crownbar_ui::root(services, wake, appearance, settings),
        config,
    )
    .context("the bar stopped")
}

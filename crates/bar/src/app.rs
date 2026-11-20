mod surfaces;

use anyhow::{Context, Result};
use crownbar_integrations::{Services, Wake};

use surfaces::BarSurfaces;

pub fn run() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // One wake for every service and every bar. A snapshot is already
    // published by the time it fires, so it carries no payload of its own.
    let wake = Wake::default();
    let services = Services::start(wake.clone())?;

    crownui::run_surfaces(BarSurfaces::new(services, wake)).context("the bar stopped")
}

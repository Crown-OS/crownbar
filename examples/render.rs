//! Renders the bar through Impeller, offscreen, to PNGs: the bar at rest and
//! with each panel open. `cargo run --example render -- <dir> [widget ids…]`.
//!
//! The services are the real ones, so the panels show this machine.

use std::{path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use crownbar::{
    BarSettings,
    services::{Services, Wake},
};
use crownui::ext::TextSystem;
use crownui::{kit, prelude::Size};
use impeller::{ImpellerBackend, Offscreen};
use runtime::InlineApp;
use semantics::UiAction;
use taffy_layout::TaffyEngine;

const SURFACE: Size = Size::new(1440.0, 640.0);
const SCALE: f32 = 2.0;
const SETTLE: Duration = Duration::from_millis(2500);

fn main() -> Result<()> {
    let mut arguments = std::env::args().skip(1);
    let directory = PathBuf::from(arguments.next().unwrap_or_else(|| ".".into()));
    let panels: Vec<String> = arguments.collect();

    let appearance = crownconfig::load_appearance();
    let wake = Wake::default();
    let services = Services::start(wake.clone())?;
    std::thread::sleep(SETTLE);

    let surface = Offscreen {
        width: (SURFACE.width * SCALE) as u32,
        height: (SURFACE.height * SCALE) as u32,
        scale: SCALE,
    };
    let view = crownbar::view(services, wake, appearance, BarSettings::default());
    let mut app = InlineApp::<ImpellerBackend, TaffyEngine>::with_surface(
        view,
        TaffyEngine::new(),
        SURFACE,
        SCALE,
        &surface,
    )
    .context("creating the offscreen renderer")?;
    app.set_corner_shape(crownbar::theme::CORNER_SHAPE);
    app.text_cache()
        .system_mut()
        .register_font(kit::inter_font().to_vec(), Some(kit::INTER_FAMILY))?;
    app.settle()?;
    write(&mut app, &directory.join("bar.png"))?;

    for id in panels {
        let pill = semantics::role(semantics::Role::Button)
            .name(&id)
            .find_one(app.ui().semantic_tree());
        let Ok(pill) = pill else {
            eprintln!("no pill named {id:?} on this machine");
            continue;
        };
        app.perform(pill, &UiAction::Click)?;
        app.settle()?;
        write(&mut app, &directory.join(format!("{id}.png")))?;
        app.perform(pill, &UiAction::Click)?;
        app.settle()?;
    }
    Ok(())
}

fn write(app: &mut InlineApp<ImpellerBackend, TaffyEngine>, path: &std::path::Path) -> Result<()> {
    let pixels = app.renderer_mut().read_pixels()?;
    std::fs::write(path, impeller::encode_png(&pixels)?)
        .with_context(|| format!("writing {}", path.display()))?;
    println!("{}", path.display());
    Ok(())
}

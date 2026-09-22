//! Offline render of the popup panels, in both palettes.
//!
//! The panels are the one part of the bar whose correctness is "does it look
//! right", and a compositor is a poor place to iterate on that: it needs a
//! session, real hardware, and a Bluetooth adapter with something paired to
//! it. This renders them over a backdrop with fixed sample data and writes the
//! pixels out, so the layout can be checked from a terminal.
//!
//! It renders **light and dark**, for the reason `crownuikit`'s own
//! `tests/theming.rs` exists: a color that does not follow the mode looks
//! perfectly fine in whichever mode you developed in.
//!
//! ```text
//! cargo test --lib render_panels -- --ignored --nocapture
//! ```

use std::num::NonZeroUsize;

use crownos_config::schema::AccentColor;
use crownshell::TextContext;
use crownuikit::config::{set_theme_immediately, Theme, ThemeMode};
use vello::{
    kurbo::{Affine, Point, Rect},
    peniko::{color::palette, Color, Fill},
    wgpu, AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene,
};

use crate::{
    theme::{self, Palette},
    ui::{panel::Panel, BarPainter},
    widgets::{
        clock::ClockWidget, layout::LayoutWidget, popup::{Item, PanelBuilder, Row}, BarWidget,
        BatteryState, Icon, PopupSpec, Rune, WidgetRegistry, WidgetSlot,
    },
};

const WIDTH: u32 = 1320;
/// One band per palette.
const BAND: u32 = 320;
const HEIGHT: u32 = BAND * 2;
const MARGIN: f64 = 24.0;
const BAR_H: u32 = 36;

/// The Sound panel with a laptop and a pair of headphones attached.
fn sound_panel() -> PopupSpec {
    let mut panel = PanelBuilder::<()>::new();
    panel.row(Row::Header {
        title: "Sound".into(),
        toggle: None,
    });
    panel.row(Row::Slider {
        icon: Icon::Rune(Rune::Headphones),
        value: 0.62,
    });
    panel.row(Row::Separator);
    panel.row(Row::Section {
        title: "Output".into(),
        chevron: false,
    });
    panel.row(
        Item::new("MacBook Air Speakers")
            .icon(Icon::Rune(Rune::Laptop))
            .row(),
    );
    panel.row(
        Item::new("AirPods Max")
            .icon(Icon::Rune(Rune::Headphones))
            .selected(true)
            .detail("100%")
            .battery(1.0)
            .row(),
    );
    panel.row(Row::Separator);
    panel.row(Row::Action {
        label: "Sound Settings…".into(),
    });
    panel.finish().0
}

fn wifi_panel() -> PopupSpec {
    let mut panel = PanelBuilder::<()>::new();
    panel.row(Row::Header {
        title: "Wi-Fi".into(),
        toggle: Some(true),
    });
    panel.row(
        Item::new("Unsecured Network")
            .icon(Icon::Rune(Rune::Wifi))
            .selected(true)
            .warning(true)
            .row(),
    );
    panel.row(Row::Section {
        title: "Known Networks".into(),
        chevron: false,
    });
    panel.row(
        Item::new("UNIWORLD-22")
            .icon(Icon::Rune(Rune::Wifi))
            .row(),
    );
    panel.row(Row::Section {
        title: "Other Networks".into(),
        chevron: false,
    });
    panel.row(Item::new("Cafe Guest").icon(Icon::Rune(Rune::Wifi)).row());
    panel.row(Row::Separator);
    panel.row(Row::Action {
        label: "Wi-Fi Settings…".into(),
    });
    panel.finish().0
}

fn bluetooth_panel() -> PopupSpec {
    let mut panel = PanelBuilder::<()>::new();
    panel.row(Row::Header {
        title: "Bluetooth".into(),
        toggle: Some(true),
    });
    panel.row(
        Item::new("soundcore Space One")
            .icon(Icon::Rune(Rune::Headphones))
            .selected(true)
            .detail("70%")
            .battery(0.7)
            .row(),
    );
    // A name long enough to need clamping, so the ellipsis is in the picture.
    panel.row(
        Item::new("Magic Keyboard with Touch ID and Numeric Keypad")
            .icon(Icon::Rune(Rune::Keyboard))
            .row(),
    );
    panel.row(
        Item::new("Pixel 9 Pro")
            .icon(Icon::Rune(Rune::Phone))
            .detail("18%")
            .battery(0.18)
            .row(),
    );
    panel.row(Row::Separator);
    panel.row(Row::Action {
        label: "Bluetooth Settings…".into(),
    });
    panel.finish().0
}

/// The Battery panel, on a machine power-profiles-daemon knows about.
fn battery_panel() -> PopupSpec {
    let mut panel = PanelBuilder::<()>::new();
    panel.row(Row::Header {
        title: "Battery".into(),
        toggle: None,
    });
    panel.row(
        Item::new("64%")
            .plain()
            .icon(Icon::Battery(BatteryState {
                level: 0.64,
                readout: 64,
                ..Default::default()
            }))
            .detail("2:40 remaining")
            .enabled(false)
            .row(),
    );
    panel.row(Row::Separator);
    panel.row(Row::Section {
        title: "Power Mode".into(),
        chevron: false,
    });
    panel.row(Item::new("Low Power").icon(Icon::Rune(Rune::Leaf)).row());
    panel.row(
        Item::new("Balanced")
            .icon(Icon::Rune(Rune::Gauge))
            .selected(true)
            .row(),
    );
    panel.row(Item::new("High Power").icon(Icon::Rune(Rune::Bolt)).row());
    panel.row(Row::Separator);
    panel.row(Row::Action {
        label: "Battery Settings…".into(),
    });
    panel.finish().0
}

/// A battery frozen mid-state, so the strip can show every pose the cell has
/// to tell apart through the real painter rather than a private path.
struct BatteryPose(BatteryState);

impl BarWidget for BatteryPose {
    fn id(&self) -> &'static str {
        "battery-pose"
    }

    fn slot(&self) -> WidgetSlot {
        WidgetSlot::Right
    }

    fn icon(&self) -> Icon {
        Icon::Battery(self.0)
    }
}

/// Saving, full, full and charging, flat, half, half and charging.
fn battery_poses() -> [BatteryState; 6] {
    [
        (0.20, 20, 0.0, 1.0, 0.0),
        (1.00, 100, 0.0, 0.0, 0.0),
        (1.00, 100, 1.0, 0.0, 0.0),
        (0.20, 20, 0.0, 0.0, 1.0),
        (0.50, 50, 0.0, 0.0, 0.0),
        (0.50, 50, 1.0, 0.0, 0.0),
    ]
    .map(|(level, readout, charging, saver, low)| BatteryState {
        level,
        readout,
        charging,
        saver,
        low,
    })
}

/// Something for the panels' translucency to sit on: a wallpaper-ish wash per
/// band, dark under the dark palette and bright under the light one.
fn backdrop(scene: &mut Scene, band: u32, dark: bool) {
    let y0 = (band * BAND) as f64;
    for step in 0..BAND / 8 {
        let t = step as f32 / (BAND / 8) as f32;
        let color = if dark {
            Color::new([0.06 + 0.20 * t, 0.14 + 0.26 * t, 0.30 + 0.26 * t, 1.0])
        } else {
            Color::new([0.62 + 0.22 * t, 0.72 + 0.18 * t, 0.86 + 0.10 * t, 1.0])
        };
        let y = y0 + step as f64 * 8.0;
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            color,
            None,
            &Rect::new(0.0, y, WIDTH as f64, y + 8.0),
        );
    }
}

/// Ignored by default: it needs a GPU, and it flips the process-global palette
/// while it runs. Nothing else in this binary reads the theme, so running it
/// on its own with `--ignored` is safe.
#[test]
#[ignore = "renders a PNG for visual review; needs a GPU"]
fn render_panels() {
    let out = std::env::var("CROWNBAR_PREVIEW")
        .unwrap_or_else(|_| "target/panel-preview.rgba".to_string());

    let mut tcx = TextContext::new();
    let mut scene = Scene::new();

    for (band, mode) in [ThemeMode::Dark, ThemeMode::Light].into_iter().enumerate() {
        // The palette is process-global, so each band is rendered under the
        // mode it is showing rather than all three panels built up front.
        set_theme_immediately(Theme::for_mode(mode, AccentColor::Purple));
        let palette = theme::palette();
        let top = (band as u32 * BAND) as f64;
        backdrop(&mut scene, band as u32, mode.is_dark());
        bar_strip(&mut scene, top, &palette, &mut tcx);

        // The middle panel is drawn with a row hovered, so both row states
        // show up in each mode.
        let panels = [
            (sound_panel(), None),
            (wifi_panel(), Some(3usize)),
            (bluetooth_panel(), None),
            (battery_panel(), Some(5usize)),
        ];
        let mut x = MARGIN;
        let y = top + BAR_H as f64 + 6.0;
        for (spec, hovered) in panels {
            let mut panel = Panel::new(spec, &mut tcx);
            panel.draw(&mut scene, Point::new(x, y), hovered, &palette, &mut tcx);
            x += panel.size().0 as f64 + MARGIN;
        }
    }

    let pixels = render(&scene);
    std::fs::write(&out, &pixels).expect("write preview");
    println!("wrote {WIDTH}x{HEIGHT} RGBA to {out}");
}

/// The bar itself, through the real painter, so its pills and icons are
/// checked against the palette alongside the panels they open.
fn bar_strip(scene: &mut Scene, top: f64, palette: &Palette, tcx: &mut TextContext) {
    let mut registry = WidgetRegistry::new();
    registry.register(Box::new(LayoutWidget::new(true)) as Box<dyn BarWidget>);
    for pose in battery_poses() {
        registry.register(Box::new(BatteryPose(pose)) as Box<dyn BarWidget>);
    }
    registry.register(Box::new(ClockWidget::new()));

    let mut painter = BarPainter::new();
    let size = (WIDTH, BAR_H);
    painter.layout_widgets(&mut registry, size, tcx);

    // `build_scene` paints in surface coordinates, so the strip is built on
    // its own and appended where the band wants it.
    let mut strip = Scene::new();
    painter.build_scene(&mut strip, &registry, Some(0), size, palette, tcx);
    scene.append(&strip, Some(Affine::translate((0.0, top))));
}

/// Render `scene` headlessly and read the texture back as RGBA8.
fn render(scene: &Scene) -> Vec<u8> {
    let mut context = vello::util::RenderContext::new();
    let dev_id = pollster::block_on(context.device(None)).expect("no wgpu device");
    let device = &context.devices[dev_id].device;
    let queue = &context.devices[dev_id].queue;

    let mut renderer = Renderer::new(
        device,
        RendererOptions {
            use_cpu: false,
            antialiasing_support: AaSupport::all(),
            num_init_threads: NonZeroUsize::new(1),
            pipeline_cache: None,
        },
    )
    .expect("vello renderer");

    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("preview"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    renderer
        .render_to_texture(
            device,
            queue,
            scene,
            &view,
            &RenderParams {
                base_color: palette::css::BLACK,
                width: WIDTH,
                height: HEIGHT,
                antialiasing_method: AaConfig::Msaa16,
            },
        )
        .expect("render_to_texture");

    // wgpu wants each copied row aligned to 256 bytes, so the readback is
    // padded and unpacked below.
    let unpadded = WIDTH as usize * 4;
    let padded = unpadded.div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (padded * HEIGHT as usize) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded as u32),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);

    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .expect("poll");
    rx.recv().expect("map").expect("map ok");

    let data = slice.get_mapped_range();
    let mut out = Vec::with_capacity(unpadded * HEIGHT as usize);
    for row in 0..HEIGHT as usize {
        out.extend_from_slice(&data[row * padded..row * padded + unpadded]);
    }
    out
}

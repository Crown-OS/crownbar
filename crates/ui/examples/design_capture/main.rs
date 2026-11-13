//! Captures the bar and every panel it opens for the CrownOS-Design Figma file, from sample
//! readings. Run both appearances with `../../../crownui/capture/scripts/capture-app.sh .`;
//! output lands in `$CROWN_DESIGN_DIR`.

mod activities;
mod fixtures;
mod plugin;

use std::time::{Duration, Instant};

use anyhow::Result;
use capture::{CaptureApp, Carrier};
use crownbar_config::{BarConfig, BarSettings};
use crownbar_integrations::{
    Availability, ErrorKind, Failure, Services, Snapshots, Wake,
    battery::ChargeStatus,
    bluetooth::RadioState,
    caffeine::CaffeineState,
    network::{Radio, Uplink, Wired},
    nightlight::NightLightState,
    screencast::{Cast, ScreencastState, Source, SourceKind},
};
use crowntest::{Role, role};
use crownui::prelude::Capabilities;

const WIDTH: f32 = 1920.0;
const OUTPUT_HEIGHT: f32 = 1080.0;
const LIVE: &str = "live";

const BAR: &str = "Current build · Bar";
const PANELS: &str = "Current build · Panels";
const VARIANTS: &str = "Current build · Panel states";
const LIVE_GROUP: &str = "Current build · Live activities";
const PLUGINS: &str = "Current build · Plugins";

/// What the CrownOS compositor offers a layer surface: glass, idle inhibition.
fn crownos_capabilities() -> Capabilities {
    Capabilities {
        materials: true,
        popups: true,
        layer_shell: true,
        idle_inhibit: true,
        ..Capabilities::default()
    }
}

/// The bar as `settings` arranges it over `snapshots`, in a surface of the bar's own reach,
/// or of the whole output while a panel is open.
fn mount(settings: &BarSettings, snapshots: Snapshots, panel_open: bool) -> Result<CaptureApp> {
    let appearance = crownconfig::load_appearance();
    let bar_config = BarConfig::new(settings, appearance.bar_height);
    let height = match panel_open {
        true => OUTPUT_HEIGHT,
        false => bar_config.reach() as f32,
    };
    let root = crownbar_ui::root(
        Services::still(snapshots)?,
        Wake::default(),
        appearance,
        settings.clone(),
    );
    let mut app = capture::mount_app(
        root,
        &crownbar_ui::app_config(&bar_config),
        Some((WIDTH, height)),
    )?;
    app.ui().set_platform_capabilities(crownos_capabilities());
    app.settle();
    Ok(app)
}

/// The panel behind `pill`, open over the whole output.
fn open(settings: &BarSettings, snapshots: Snapshots, pill: &str) -> Result<CaptureApp> {
    let mut app = mount(settings, snapshots, true)?;
    app.click(role(Role::Button).name(pill));
    app.settle();
    Ok(app)
}

struct Session<'a> {
    carrier: Carrier,
    settings: &'a BarSettings,
}

impl Session<'_> {
    fn bar(
        &mut self,
        group: &str,
        name: &str,
        description: &str,
        snapshots: Snapshots,
    ) -> Result<()> {
        let mut app = mount(self.settings, snapshots, false)?;
        self.carrier
            .capture_state(&mut app, group, name, description)?;
        Ok(())
    }

    fn panel(
        &mut self,
        group: &str,
        name: &str,
        description: &str,
        snapshots: Snapshots,
        pill: &str,
    ) -> Result<()> {
        let mut app = open(self.settings, snapshots, pill)?;
        self.carrier
            .capture_state(&mut app, group, name, description)?;
        Ok(())
    }
}

fn with_live(settings: &BarSettings) -> BarSettings {
    let mut settings = settings.clone();
    if !settings.left.iter().any(|entry| entry == LIVE) {
        let after_clock = settings
            .left
            .iter()
            .position(|entry| entry == "clock")
            .map_or(0, |clock| clock + 1);
        settings.left.insert(after_clock, LIVE.to_owned());
    }
    settings
}

fn casting(apps: &[(&str, SourceKind, &str, &str)]) -> ScreencastState {
    ScreencastState {
        availability: Availability::Ready,
        casts: apps
            .iter()
            .enumerate()
            .map(|(index, (app, kind, source, label))| Cast {
                id: index as u64 + 1,
                app: (*app).into(),
                kind: *kind,
                source: (*source).into(),
                label: (*label).into(),
            })
            .collect(),
        sources: vec![
            Source {
                kind: SourceKind::Screen,
                id: "eDP-1".into(),
                label: "Entire Screen".into(),
                detail: "2560 × 1600".into(),
            },
            Source {
                kind: SourceKind::Window,
                id: "w12".into(),
                label: "Design review".into(),
                detail: "Firefox".into(),
            },
            Source {
                kind: SourceKind::Workspace,
                id: "ws2".into(),
                label: "Workspace 2".into(),
                detail: "3 windows".into(),
            },
        ],
    }
}

fn capture_bar(session: &mut Session) -> Result<()> {
    session.bar(
        BAR,
        "Bar",
        "The bar at rest, as bar.ron arranges it",
        fixtures::everyday(),
    )?;

    let mut app = mount(session.settings, fixtures::everyday(), false)?;
    let volume = app.find(role(Role::Button).name("volume")).bounds.center();
    app.pointer_move(volume.x, volume.y);
    session.carrier.capture_state(
        &mut app,
        BAR,
        "Hovered pill",
        "The pointer over the volume pill",
    )?;
    drop(app);

    let mut toggled = fixtures::everyday();
    toggled.caffeine.active = true;
    toggled.audio = fixtures::muted(toggled.audio);
    toggled.notifications.do_not_disturb = true;
    toggled.battery = fixtures::battery(0.54, ChargeStatus::Charging, Some(48));
    toggled.power.profiles.active = Some(crownbar_integrations::power::Profile::Performance);
    toggled.workspace_mode.mode = crownbar_integrations::workspace_mode::WorkspaceMode::Tiling;
    let mut app = mount(session.settings, toggled, false)?;
    session.carrier.capture_state(
        &mut app,
        BAR,
        "Toggled",
        "Tiled layout, keeping awake, muted, charging, Do Not Disturb",
    )?;
    drop(app);

    let mut low = fixtures::everyday();
    low.battery = fixtures::battery(0.12, ChargeStatus::Discharging, Some(24));
    low.power.profiles.active = Some(crownbar_integrations::power::Profile::Saver);
    low.network.radio = Radio::Off;
    low.network.connected = None;
    low.bluetooth.radio = RadioState::Off;
    low.notifications.center_open = true;
    session.bar(
        BAR,
        "Low battery, radios off",
        "Battery saver at 12%, Wi-Fi and Bluetooth off, notification centre open",
        low,
    )?;

    let mut searching = fixtures::everyday();
    searching.network.connected = None;
    searching.network.scanning = true;
    searching.screencast = casting(&[("Firefox", SourceKind::Window, "w12", "Design review")]);
    session.bar(
        BAR,
        "Searching, sharing screen",
        "Wi-Fi looking for a network while a window is being shared",
        searching,
    )?;

    let mut wired = fixtures::everyday();
    wired.network.uplink = Uplink::Ethernet;
    wired.network.wired = Some(Wired {
        speed_mbps: Some(1000),
    });
    session.bar(BAR, "On Ethernet", "Traffic leaves by cable", wired)?;

    let floating = BarSettings {
        floating: true,
        margin: 8,
        ..session.settings.clone()
    };
    let mut app = mount(&floating, fixtures::everyday(), false)?;
    session.carrier.capture_state(
        &mut app,
        BAR,
        "Floating",
        "bar.ron floating: true with an 8 px margin and round ends",
    )?;
    Ok(())
}

fn capture_panels(session: &mut Session) -> Result<()> {
    let panels = [
        (
            "Calendar",
            "clock",
            "The time written large over a month grid",
        ),
        (
            "Weather",
            "weather",
            "Current conditions and the five-day outlook",
        ),
        (
            "Keep Awake",
            "caffeine",
            "Hold the machine awake, indefinitely or for a while",
        ),
        (
            "Sound",
            "volume",
            "Output level, devices, apps playing and inputs",
        ),
        (
            "Display",
            "brightness",
            "One slider per screen, and Night Light",
        ),
        (
            "Bluetooth",
            "bluetooth",
            "Paired devices with battery, and others nearby",
        ),
        (
            "Wi-Fi",
            "wifi",
            "The joined network, known and other networks, phone hotspot",
        ),
        (
            "Battery",
            "battery",
            "Charge, power modes and a linked phone's battery",
        ),
    ];
    for (name, pill, description) in panels {
        session.panel(PANELS, name, description, fixtures::everyday(), pill)?;
    }
    let mut shared = fixtures::everyday();
    shared.screencast = casting(&[("Firefox", SourceKind::Window, "w12", "Design review")]);
    session.panel(
        PANELS,
        "Screen Sharing",
        "What is being shared, and what to share instead",
        shared,
        "screencast",
    )
}

fn capture_panel_states(session: &mut Session) -> Result<()> {
    let mut awake = fixtures::everyday();
    awake.caffeine = CaffeineState {
        availability: Availability::Ready,
        active: true,
        until: Some(Instant::now() + Duration::from_secs(43 * 60)),
    };
    session.panel(
        VARIANTS,
        "Keep Awake · timed",
        "Keeping awake with 43 minutes left",
        awake,
        "caffeine",
    )?;

    let mut muted = fixtures::everyday();
    muted.audio = fixtures::muted(muted.audio);
    session.panel(
        VARIANTS,
        "Sound · muted",
        "The output muted from its slider's icon",
        muted,
        "volume",
    )?;

    let mut single = fixtures::everyday();
    single.brightness = fixtures::single_display(single.brightness);
    single.nightlight = NightLightState {
        availability: Availability::Ready,
        active: true,
        kelvin: 3400,
    };
    session.panel(
        VARIANTS,
        "Display · one screen",
        "A single screen with Night Light on",
        single,
        "brightness",
    )?;

    let mut wifi_off = fixtures::everyday();
    wifi_off.network.radio = Radio::Off;
    wifi_off.network.connected = None;
    session.panel(
        VARIANTS,
        "Wi-Fi · off",
        "The radio switched off",
        wifi_off,
        "wifi",
    )?;

    let mut blocked = fixtures::everyday();
    blocked.network.radio = Radio::HardBlocked;
    blocked.network.connected = None;
    session.panel(
        VARIANTS,
        "Wi-Fi · hardware switch",
        "A physical switch holds the radio off",
        blocked,
        "wifi",
    )?;

    let mut looking = fixtures::everyday();
    looking.network.connected = None;
    looking.network.known.clear();
    looking.network.others.clear();
    looking.network.scanning = true;
    session.panel(
        VARIANTS,
        "Wi-Fi · looking",
        "Scanning with nothing found yet",
        looking,
        "wifi",
    )?;

    let mut joining = fixtures::everyday();
    joining.network.connected = None;
    joining.network.pending = Some("TP-Link_4F2A".into());
    joining.network.failure = Some(Failure::new(
        ErrorKind::NotAuthorized,
        "secrets were required",
    ));
    session.panel(
        VARIANTS,
        "Wi-Fi · joining",
        "A join in flight after a refused one",
        joining,
        "wifi",
    )?;

    let mut ethernet = fixtures::everyday();
    ethernet.network.uplink = Uplink::Ethernet;
    ethernet.network.wired = Some(Wired {
        speed_mbps: Some(1000),
    });
    session.panel(
        VARIANTS,
        "Wi-Fi · Ethernet",
        "A cable plugged in beside Wi-Fi",
        ethernet,
        "wifi",
    )?;

    let mut bt_off = fixtures::everyday();
    bt_off.bluetooth.radio = RadioState::Off;
    session.panel(
        VARIANTS,
        "Bluetooth · off",
        "The radio switched off",
        bt_off,
        "bluetooth",
    )?;

    let mut pairing = fixtures::everyday();
    pairing.bluetooth.paired.clear();
    pairing.bluetooth.discovering = true;
    if let Some(speaker) = pairing.bluetooth.discovered.first_mut() {
        speaker.busy = true;
    }
    session.panel(
        VARIANTS,
        "Bluetooth · pairing",
        "Nothing paired, scanning, pairing a speaker",
        pairing,
        "bluetooth",
    )?;

    let mut failed = fixtures::everyday();
    failed.bluetooth.failure = Some(Failure::new(ErrorKind::Timeout, "connect timed out"));
    session.panel(
        VARIANTS,
        "Bluetooth · failed",
        "A connection that timed out",
        failed,
        "bluetooth",
    )?;

    let mut charging = fixtures::everyday();
    charging.battery = fixtures::battery(0.54, ChargeStatus::Charging, Some(48));
    charging.power.profiles.degraded = Some("Too hot".into());
    session.panel(
        VARIANTS,
        "Battery · charging",
        "Charging, with High Power held back by heat",
        charging,
        "battery",
    )?;

    let mut stale = fixtures::everyday();
    stale.weather.availability =
        Availability::Degraded("Offline · showing the last reading".into());
    stale.weather.refreshing = true;
    session.panel(
        VARIANTS,
        "Weather · offline",
        "Refreshing an out-of-date reading",
        stale,
        "weather",
    )?;

    let mut two = fixtures::everyday();
    two.screencast = casting(&[
        ("Firefox", SourceKind::Window, "w12", "Design review"),
        ("OBS Studio", SourceKind::Screen, "eDP-1", "Entire Screen"),
    ]);
    session.panel(
        VARIANTS,
        "Screen Sharing · two apps",
        "Two casts, each with its own stop",
        two,
        "screencast",
    )
}

fn capture_live(session: &mut Session) -> Result<()> {
    let settings = with_live(session.settings);
    let busy = || Snapshots {
        live: activities::busy(),
        ..fixtures::everyday()
    };
    let mut app = mount(&settings, busy(), false)?;
    session.carrier.capture_state(
        &mut app,
        LIVE_GROUP,
        "Bar with activities",
        "A track, a timer, a coding agent and a call beside the clock",
    )?;
    drop(app);

    let chips = [
        (
            "Now Playing",
            "live-1",
            "A seekable track with transport controls",
        ),
        ("Timer", "live-2", "A countdown draining its ring"),
        ("Agent", "live-3", "A coding agent at work"),
        ("Call", "live-4", "A phone call with a destructive hang-up"),
    ];
    for (name, pill, description) in chips {
        let mut app = open(&settings, busy(), pill)?;
        session
            .carrier
            .capture_state(&mut app, LIVE_GROUP, name, description)?;
    }

    let more = || Snapshots {
        live: activities::more(),
        ..fixtures::everyday()
    };
    let mut app = mount(&settings, more(), false)?;
    session.carrier.capture_state(
        &mut app,
        LIVE_GROUP,
        "Bar asking for attention",
        "A stopwatch, and an agent waiting on the user",
    )?;
    drop(app);
    for (name, pill, description) in [
        ("Stopwatch", "live-1", "A paused stopwatch with its laps"),
        ("Agent waiting", "live-2", "An agent that needs an answer"),
    ] {
        let mut app = open(&settings, more(), pill)?;
        session
            .carrier
            .capture_state(&mut app, LIVE_GROUP, name, description)?;
    }
    Ok(())
}

fn capture_plugins(session: &mut Session) -> Result<()> {
    let mut settings = session.settings.clone();
    settings.right.insert(0, plugin::ENTRY.to_owned());
    let with_plugin = || -> Result<Snapshots> {
        Ok(Snapshots {
            plugins: plugin::cpu_graph()?,
            ..fixtures::everyday()
        })
    };
    let mut app = mount(&settings, with_plugin()?, false)?;
    session.carrier.capture_state(
        &mut app,
        PLUGINS,
        "Plugin pill",
        "The cpu-graph example plugin's pill, drawn in the bar's ink",
    )?;
    drop(app);
    let mut app = open(&settings, with_plugin()?, plugin::ENTRY)?;
    session.carrier.capture_state(
        &mut app,
        PLUGINS,
        "Plugin panel",
        "The cpu-graph plugin's card as a panel",
    )?;
    Ok(())
}

fn main() -> Result<()> {
    let settings: BarSettings = crownconfig::load_section(BarSettings::SECTION);
    let mut session = Session {
        carrier: Carrier::new(
            "Bar",
            "crownbar: the top bar, every panel and live activity it opens, captured headlessly from the current source with sample readings.",
        ),
        settings: &settings,
    };
    capture_bar(&mut session)?;
    capture_panels(&mut session)?;
    capture_panel_states(&mut session)?;
    capture_live(&mut session)?;
    capture_plugins(&mut session)?;
    let path = session.carrier.write(None)?;
    println!("{}", path.display());
    capture::finish()
}

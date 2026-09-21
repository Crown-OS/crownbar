//! The service layer against whatever daemons this machine is running.
//!
//! This is its own test binary because starting [`Services`] starts a tokio
//! runtime and a PipeWire thread, and within the binary it is one `#[test]`
//! because cargo runs test *functions* on parallel threads — two runtimes
//! racing for the same sockets proves nothing.
//!
//! Where a daemon is absent the corresponding check reports and moves on: the
//! point is to catch a service that connects and then reads the wrong thing,
//! which is the failure a unit test cannot see.

use std::{thread::sleep, time::Duration};

use crownbar::services::{Availability, Services, Wake};

/// How long to let a service settle before believing its snapshot.
const SETTLE: Duration = Duration::from_millis(2500);
/// A Wi-Fi sweep is seconds of radio time, not milliseconds.
const SCAN: Duration = Duration::from_secs(6);

#[test]
fn the_services_describe_this_machine() {
    let (ping, _source) = crownshell::calloop::ping::make_ping().expect("a ping");
    let services = Services::start(Wake::new(ping)).expect("services start");
    sleep(SETTLE);

    audio_finds_its_devices(&services);
    the_power_daemon_offers_profiles(&services);
    the_battery_reads_or_says_why(&services);
}

fn audio_finds_its_devices(services: &Services) {
    let audio = services.audio.read();
    match &audio.availability {
        Availability::Ready => {
            assert!(
                !audio.outputs.is_empty(),
                "pipewire answered but listed no output devices, so either the \
                 media.class filter or the registry walk is wrong"
            );
            for device in &audio.outputs {
                assert!(
                    !device.name.is_empty(),
                    "an output has no `node.name`, which is the key a \
                     default-device write quotes"
                );
                assert!(
                    (0.0..=1.5).contains(&device.volume.level),
                    "{} reports a level of {}, which is outside the slider's \
                     range — the gain is probably not being cube-rooted",
                    device.description,
                    device.volume.level
                );
            }
            for device in &audio.outputs {
                eprintln!(
                    "output {}{} {:?} level {:.2}{}",
                    device.description,
                    if device.default { " (default)" } else { "" },
                    device.kind,
                    device.volume.level,
                    if device.volume.muted { " muted" } else { "" }
                );
            }
            for device in &audio.inputs {
                eprintln!("input  {} {:?}", device.description, device.kind);
            }
            eprintln!("playing: {:?}", audio.microphone_users());
            assert!(
                audio.default_output().is_some(),
                "no output is marked default, so the `default.audio.sink` \
                 metadata was not matched against any `node.name`"
            );
        }
        other => eprintln!("skipping audio: {other:?}"),
    }
}

fn the_power_daemon_offers_profiles(services: &Services) {
    let power = services.power.read();
    match &power.availability {
        Availability::Ready => {
            assert!(
                !power.profiles.supported.is_empty(),
                "power-profiles-daemon answered but advertised no profiles"
            );
            assert!(
                power.profiles.active.is_some(),
                "no active profile, so `ActiveProfile` did not map onto a \
                 known profile id"
            );
            eprintln!("profiles {:?}", power.profiles);
            assert!(
                power.failure.is_none(),
                "a failure is recorded before any command was sent"
            );
        }
        other => eprintln!("skipping power profiles: {other:?}"),
    }
}

fn the_battery_reads_or_says_why(services: &Services) {
    let battery = services.battery.read();
    match (&battery.availability, battery.charge) {
        (Availability::Ready, Some(charge)) => {
            assert!(
                (0.0..=1.0).contains(&charge.level),
                "charge level {} is outside [0, 1]",
                charge.level
            );
            eprintln!("charge {:?}  {}", charge, charge.summary());
            assert!(
                charge.percent() <= 100,
                "charge percent {} is over 100",
                charge.percent()
            );
        }
        (Availability::Ready, None) => {
            panic!("the battery service is ready but has no reading")
        }
        (other, _) => eprintln!("skipping battery: {other:?}"),
    }
}

/// Writing volume touches the machine's actual output, so this is opt-in:
/// `cargo test --test services -- --ignored`. It is the only check that the
/// SPA pod a `Props` write is built from is one the server accepts — every
/// other assertion here would pass against a read-only client.
#[test]
#[ignore = "changes the machine's volume"]
fn a_volume_write_comes_back() {
    use crownbar::services::audio::AudioCommand;

    let (ping, _source) = crownshell::calloop::ping::make_ping().expect("a ping");
    let services = Services::start(Wake::new(ping)).expect("services start");
    sleep(SETTLE);

    let before = services.audio.read().output_volume();
    if !services.audio.read().availability.usable() {
        eprintln!("no pipewire; skipping");
        return;
    }

    // Far enough from the starting point that a no-op write cannot pass.
    let target = if before.level > 0.5 { 0.3 } else { 0.7 };
    services.audio.send(AudioCommand::SetOutputVolume(target));
    sleep(Duration::from_millis(600));

    let after = services.audio.read().output_volume();
    services
        .audio
        .send(AudioCommand::SetOutputVolume(before.level));
    sleep(Duration::from_millis(400));

    assert!(
        (after.level - target).abs() < 0.02,
        "asked for {target} and the server reported back {}, so either the \
         `channelVolumes` pod was rejected or the cube law is not symmetric",
        after.level
    );
}

/// Bluetooth and Wi-Fi share a shape: both have a kernel-level radio answer
/// that must be right with no daemon, and a daemon-level device list on top.
#[test]
fn the_radios_agree_with_the_kernel() {
    use crownbar::services::{bluetooth::RadioState, network::Radio, rfkill};

    let (ping, _source) = crownshell::calloop::ping::make_ping().expect("a ping");
    let services = Services::start(Wake::new(ping)).expect("services start");
    sleep(SETTLE);

    let bluetooth = services.bluetooth.read();
    let kernel = rfkill::block(rfkill::BLUETOOTH);
    eprintln!(
        "bluetooth {:?} rfkill={kernel:?} adapter={:?} paired={}",
        bluetooth.radio,
        bluetooth.adapter,
        bluetooth.paired.len()
    );
    assert_eq!(
        kernel.present(),
        bluetooth.radio != RadioState::Unknown || !bluetooth.availability.usable(),
        "the service and rfkill disagree about whether a bluetooth radio exists"
    );
    if kernel == rfkill::Block::Hard {
        assert_eq!(
            bluetooth.radio,
            RadioState::HardBlocked,
            "rfkill reports a hardware block, so the toggle must be drawn inert"
        );
    }
    for device in &bluetooth.paired {
        assert!(
            !device.name.is_empty(),
            "a paired device has no name, so its row would be blank"
        );
        assert!(device.paired, "an unpaired device reached the paired list");
    }

    let network = services.network.read();
    eprintln!(
        "wifi {:?} joined={:?} known={} others={} quality={:.2}",
        network.radio,
        network.connected.as_ref().map(|j| &j.ssid),
        network.known.len(),
        network.others.len(),
        network.quality()
    );
    assert!(
        (0.0..=1.0).contains(&network.quality()),
        "link quality {} is outside [0, 1]",
        network.quality()
    );
    for entry in network.known.iter().chain(network.others.iter()) {
        assert!(!entry.ssid.is_empty(), "a network row has no SSID");
        assert!(
            (0.0..=1.0).contains(&entry.strength),
            "{} reports a strength of {}, which is not a fraction",
            entry.ssid,
            entry.strength
        );
    }
    assert!(
        network.others.len() <= crownbar::services::network::MAX_OTHER,
        "the stranger list was not capped"
    );
    assert!(
        network
            .connected
            .as_ref()
            .is_none_or(|joined| !network.known.iter().any(|n| n.ssid == joined.ssid)),
        "the joined network is also listed under Known Network, so it would \
         appear twice in the panel"
    );

    // Scanning only runs while a panel is up, so an idle service listing no
    // networks is correct. Opening one has to produce some.
    if network.radio == Radio::On {
        drop(network);
        services
            .network
            .send(crownbar::services::network::NetworkCommand::Interest(
                crownbar::services::Interest::Panel,
            ));
        sleep(SCAN);
        let network = services.network.read();
        eprintln!(
            "after scan: known={} others={}",
            network.known.len(),
            network.others.len()
        );
        assert!(
            network.connected.is_some() || !network.others.is_empty(),
            "a scan finished and found nothing at all, so either the scan was \
             never started or the listing is being filtered away"
        );
    }
}

#[test]
fn brightness_finds_every_screen_it_can_dim() {
    use crownbar::services::brightness::Transport;

    let (ping, _source) = crownshell::calloop::ping::make_ping().expect("a ping");
    let services = Services::start(Wake::new(ping)).expect("services start");
    // DDC enumeration probes every i2c bus and is slower than everything else
    // here.
    sleep(SCAN);

    let brightness = services.brightness.read();
    eprintln!("brightness {:?}", brightness.availability);
    for display in &brightness.displays {
        eprintln!(
            "  {} {:?} level {:.2} max {}",
            display.label, display.transport, display.level, display.max
        );
    }
    if !brightness.availability.usable() {
        eprintln!("skipping brightness: nothing controllable");
        return;
    }

    assert!(
        !brightness.displays.is_empty(),
        "brightness reports itself usable but lists no display"
    );
    for display in &brightness.displays {
        assert!(
            (0.0..=1.0).contains(&display.level),
            "{} reports a level of {}, which is not a fraction",
            display.label,
            display.level
        );
        assert!(display.max > 0, "{} has a zero raw range", display.label);
        // The curve has to survive a round trip, or the slider would drift a
        // little further from the truth on every drag.
        let raw = display.transport.to_raw(display.level, display.max);
        let back = display.transport.from_raw(raw, display.max);
        assert!(
            (back - display.level).abs() < 0.02,
            "{} level {} became {} after a round trip through raw {}",
            display.label,
            display.level,
            back,
            raw
        );
    }
    assert!(
        brightness
            .primary()
            .is_some_and(|d| d.transport == Transport::Backlight)
            || brightness
                .displays
                .iter()
                .all(|d| d.transport == Transport::Ddc),
        "an internal panel exists but is not what the pill shows"
    );
}

/// Caffeine is the one service the event loop has to finish: it holds its
/// intent, but only `App` can turn that into a Wayland object. Without a
/// compositor connection there is no `App`, so this checks the half that runs
/// on the runtime — the toggle and the expiry timer.
#[test]
fn caffeine_holds_and_releases_its_intent() {
    use crownbar::services::caffeine::CaffeineCommand;
    use std::time::Duration;

    let (ping, _source) = crownshell::calloop::ping::make_ping().expect("a ping");
    let services = Services::start(Wake::new(ping)).expect("services start");
    sleep(Duration::from_millis(300));

    assert!(
        !services.caffeine.read().active,
        "caffeine starts held, so the machine would never sleep"
    );

    services.caffeine.send(CaffeineCommand::Toggle);
    sleep(Duration::from_millis(200));
    let state = services.caffeine.read();
    assert!(state.active, "toggle did not take");
    assert!(
        state.until.is_none(),
        "an untimed toggle set a deadline, so it would release on its own"
    );

    services.caffeine.send(CaffeineCommand::Toggle);
    sleep(Duration::from_millis(200));
    assert!(!services.caffeine.read().active, "toggle did not release");

    // A timed hold has to release itself; that timer is the only thing in this
    // service that changes state without being asked.
    services
        .caffeine
        .send(CaffeineCommand::SetActiveFor(Duration::from_millis(600)));
    sleep(Duration::from_millis(200));
    let state = services.caffeine.read();
    assert!(state.active && state.until.is_some(), "timed hold did not take");
    assert!(
        state.remaining().is_some_and(|left| left.as_millis() > 0),
        "a timed hold reports no time left the moment it starts"
    );

    sleep(Duration::from_millis(900));
    let state = services.caffeine.read();
    assert!(
        !state.active && state.until.is_none(),
        "the timed hold never expired, so the machine would stay awake forever"
    );
}

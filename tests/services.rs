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

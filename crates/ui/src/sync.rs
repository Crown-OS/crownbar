//! Keeping the bar current: service snapshots as they land, the clock as it
//! ticks, the appearance, theme and bar settings as they are edited, and the
//! wallpaper as it changes.

use std::{
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crownbar_config::{BarConfig, BarSettings};
use crownconfig::{Appearance, SectionWatch, ThemeOverrides};
use crownui::prelude::{Cx, Signal, Size};
use tokio::sync::watch;

use crate::{
    frames::animate,
    popup,
    state::Bar,
    wallpaper::{self, Backdrop},
};
use crownbar_integrations::{Wake, caffeine};
use crownbar_widgets::{Arrangement, WidgetRegistry};

pub fn start(cx: &mut Cx, bar: Bar, wake: Wake) {
    services_changed(cx, bar);
    follow_services(cx, bar, wake);
    tick(cx, bar);
}

/// Waits for the next service snapshot, takes it, and waits again.
fn follow_services(cx: &mut Cx, bar: Bar, wake: Wake) {
    cx.spawn(wake.clone().woken(), move |cx, ()| {
        services_changed(cx, bar);
        follow_services(cx, bar, wake);
    });
}

fn services_changed(cx: &mut Cx, bar: Bar) {
    let changed = bar
        .state
        .update(cx, |state| state.widgets.sync(&state.services));
    bar.touch(cx, changed);
    animate(cx, bar, changed);
    bar.restate_stale_panel(cx);
    // Caffeine's inhibitor is a Wayland object on the bar's own surface, so
    // the intent the service holds is turned into one here.
    let supported = cx
        .platform_capabilities()
        .with_untracked(cx, |capabilities| capabilities.idle_inhibit);
    let inhibit = bar.state.with(cx, |state| {
        caffeine::reconcile(&state.services.caffeine, supported)
    });
    cx.set_idle_inhibited(inhibit);
}

/// Once a second, on the second, so the clock turns over with the minute.
fn tick(cx: &mut Cx, bar: Bar) {
    let wait = until_next_second();
    cx.spawn(
        async move { tokio::time::sleep(wait).await },
        move |cx, ()| {
            let changed = bar.state.update(cx, |state| state.widgets.tick());
            bar.touch(cx, changed);
            popup::poll(cx, bar);
            tick(cx, bar);
        },
    );
}

fn until_next_second() -> Duration {
    let into_second = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(Duration::ZERO, |now| {
            Duration::from_nanos(u64::from(now.subsec_nanos()))
        });
    Duration::from_secs(1).saturating_sub(into_second)
}

/// The panels' transparency and the wallpaper's path, following
/// `appearance.ron`.
pub fn appearance(cx: &mut Cx, appearance: &Appearance) -> (Signal<f32>, Signal<String>) {
    let transparency = cx.signal(appearance.transparency as f32);
    let wallpaper = cx.signal(appearance.wallpaper.clone());
    let watch = crownconfig::watch_section(Appearance::SECTION, appearance.clone());
    follow(cx, watch, move |cx, appearance: Appearance| {
        transparency.set(cx, appearance.transparency as f32);
        if wallpaper.with_untracked(cx, |path| *path != appearance.wallpaper) {
            wallpaper.set(cx, appearance.wallpaper);
        }
    });
    (transparency, wallpaper)
}

/// A wallpaper to sample, and the output it fills.
type SampleRequest = Option<(PathBuf, Size)>;

/// The wallpaper at `path` as it lies under the bar, sampled off the UI thread
/// again whenever it or the output changes. `None` until then, or for a
/// wallpaper that is not a still image.
pub fn backdrop(cx: &mut Cx, path: Signal<String>, config: BarConfig) -> Signal<Option<Backdrop>> {
    let backdrop = cx.signal(None);
    let output = cx.output_size();
    let (requests, receiver) = watch::channel(None);
    cx.effect(move |runtime| {
        let screen = output.get(runtime);
        let request = path.with(runtime, |path| {
            screen
                .filter(|_| !path.is_empty())
                .map(|screen| (PathBuf::from(path), screen))
        });
        requests.send_replace(request);
    });
    sample_next(cx, receiver, backdrop, config);
    backdrop
}

/// Samples the latest request, publishes it unless a newer one has already
/// come in, and waits again.
fn sample_next(
    cx: &mut Cx,
    mut requests: watch::Receiver<SampleRequest>,
    backdrop: Signal<Option<Backdrop>>,
    config: BarConfig,
) {
    let next = async move {
        requests.changed().await.ok()?;
        let request = requests.borrow_and_update().clone();
        let sampled = match request {
            Some((path, screen)) => tokio::task::spawn_blocking(move || {
                wallpaper::sample(&path, screen, wallpaper::strip(&config, screen))
                    .inspect_err(|error| {
                        log::info!(
                            "the bar's ink follows the theme: {}: {error}",
                            path.display()
                        )
                    })
                    .ok()
            })
            .await
            .ok()
            .flatten(),
            None => None,
        };
        let superseded = requests.has_changed().unwrap_or(true);
        Some((requests, sampled, superseded))
    };
    cx.spawn(next, move |cx, next| {
        let Some((requests, sampled, superseded)) = next else {
            return;
        };
        if !superseded {
            backdrop.set(cx, sampled);
        }
        sample_next(cx, requests, backdrop, config);
    });
}

/// `theme.ron`'s palette edits, following the file.
pub fn theme_overrides(cx: &mut Cx) -> Signal<ThemeOverrides> {
    let overrides = crownconfig::load_theme_overrides();
    let signal = cx.signal(overrides.clone());
    let watch = crownconfig::watch_section(ThemeOverrides::SECTION, overrides);
    follow(cx, watch, move |cx, overrides| signal.set(cx, overrides));
    signal
}

/// `bar.ron`, following the file. A new arrangement rebuilds the strip at
/// once; a new shape or place waits for a restart, since the layer surface is
/// described to the compositor only when it is made.
pub fn bar_settings(cx: &mut Cx, bar: Bar, settings: BarSettings, fallback_height: u32) {
    let watch = crownconfig::watch_section(BarSettings::SECTION, settings);
    follow(cx, watch, move |cx, settings: BarSettings| {
        let arrangement = Arrangement::from(&settings);
        if bar.state.with(cx, |state| state.arrangement != arrangement) {
            popup::dismiss(cx, bar);
            bar.rearrange(cx, WidgetRegistry::arranged(&arrangement), arrangement);
        }
        if BarConfig::new(&settings, fallback_height) != bar.config {
            log::info!("bar.ron moves or reshapes the bar; restart crownbar to apply it");
        }
    });
}

/// Waits for the next edit of a section, applies it, and waits again.
fn follow<T>(cx: &mut Cx, mut watch: SectionWatch<T>, apply: impl Fn(&mut Cx, T) + Copy + 'static)
where
    T: Clone + Send + Sync + 'static,
{
    let next = async move {
        let value = watch.changed().await;
        (watch, value)
    };
    cx.spawn(next, move |cx, (watch, value)| {
        apply(cx, value);
        follow(cx, watch, apply);
    });
}

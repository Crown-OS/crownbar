//! One bar on each output the compositor gives a bar, all in one process.

use std::sync::{Arc, OnceLock};

use crownbar_config::{BarConfig, BarPosition, BarSettings};
use crownbar_integrations::{Services, Wake};
use crownos_config::Subscription;
use crownui::{
    Surfaces,
    prelude::{AnyView, AppConfig, View},
    surfaces::{Output, Waker},
};
use tokio::sync::watch;

pub struct BarSurfaces {
    services: Arc<Services>,
    wake: Wake,
    settings: Arc<watch::Sender<BarSettings>>,
    runner_waker: Arc<OnceLock<Waker>>,
    _settings_subscription: Subscription,
}

impl BarSurfaces {
    pub fn new(services: Arc<Services>, wake: Wake) -> Self {
        let settings = Arc::new(watch::Sender::new(
            crownconfig::load_section::<BarSettings>(BarSettings::SECTION),
        ));
        let runner_waker = Arc::<OnceLock<Waker>>::default();
        let settings_subscription = follow_settings(settings.clone(), runner_waker.clone());
        Self {
            services,
            wake,
            settings,
            runner_waker,
            _settings_subscription: settings_subscription,
        }
    }
}

/// Keeps `settings` on what `bar.ron` says, and wakes the runner when the bar
/// moves to the other edge, so it builds every bar again on that edge.
fn follow_settings(
    settings: Arc<watch::Sender<BarSettings>>,
    runner_waker: Arc<OnceLock<Waker>>,
) -> Subscription {
    crownos_config::subscribe_typed(BarSettings::SECTION, move |edited: BarSettings| {
        let moved = settings.send_if_modified(|held| {
            let moved = held.position != edited.position;
            *held = edited;
            moved
        });
        if moved && let Some(wake_runner) = runner_waker.get() {
            wake_runner();
        }
    })
}

fn placement_key(position: BarPosition) -> u64 {
    match position {
        BarPosition::Top => 0,
        BarPosition::Bottom => 1,
    }
}

impl Surfaces for BarSurfaces {
    fn attach(&mut self, waker: Waker) {
        let _ = self.runner_waker.set(waker);
    }

    fn placement(&mut self, output: &Output) -> Option<u64> {
        let wanted = output.crown.as_ref().is_none_or(|crown| crown.shell.bar());
        wanted.then(|| placement_key(self.settings.borrow().position))
    }

    fn build(&mut self, output: &Output) -> Vec<(AnyView, AppConfig)> {
        let settings = self.settings.borrow().clone();
        let appearance = crownconfig::load_appearance();
        let config = crownbar_ui::app_config(&BarConfig::new(&settings, appearance.bar_height));
        let root = crownbar_ui::root(
            self.services.clone(),
            self.wake.subscribe(),
            appearance,
            settings,
            output.name.as_deref().map(Arc::from),
        );
        vec![(root.boxed(), config)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_edge_has_its_own_placement() {
        assert_ne!(
            placement_key(BarPosition::Top),
            placement_key(BarPosition::Bottom)
        );
    }
}

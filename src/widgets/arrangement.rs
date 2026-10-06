//! What is on the bar and where: the three lists of `bar.ron`.
//!
//! A built-in is named by its own id; a plugin's widget by
//! `plugin:<plugin>/<widget>`. A built-in left out of every list is not built
//! at all, and a name nothing answers to is skipped with a warning.

use config::BarSettings;

use crate::widgets::{
    BarWidget, WidgetRegistry, WidgetSlot, battery::BatteryWidget, bluetooth::BluetoothWidget,
    brightness::BrightnessWidget, caffeine::CaffeineWidget, clock::ClockWidget,
    layout::LayoutWidget, notifications::NotificationsWidget, remote::RemoteWidget,
    volume::VolumeWidget, weather::WeatherWidget, wifi::WifiWidget,
};

/// The entries of each slot, left-to-right.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Arrangement {
    pub left: Vec<String>,
    pub center: Vec<String>,
    pub right: Vec<String>,
}

impl From<&BarSettings> for Arrangement {
    fn from(settings: &BarSettings) -> Self {
        Self {
            left: settings.left.clone(),
            center: settings.center.clone(),
            right: settings.right.clone(),
        }
    }
}

impl Arrangement {
    /// Every entry with its slot, in the order the bar registers them.
    pub fn entries(&self) -> impl Iterator<Item = (WidgetSlot, &str)> {
        in_slot(WidgetSlot::Left, &self.left)
            .chain(in_slot(WidgetSlot::Center, &self.center))
            .chain(in_slot(WidgetSlot::Right, &self.right))
    }
}

fn in_slot(slot: WidgetSlot, entries: &[String]) -> impl Iterator<Item = (WidgetSlot, &str)> {
    entries.iter().map(move |entry| (slot, entry.as_str()))
}

/// The built-in widget called `name`.
pub fn builtin(name: &str) -> Option<Box<dyn BarWidget>> {
    let widget: Box<dyn BarWidget> = match name {
        "clock" => Box::new(ClockWidget::new()),
        "layout" => Box::new(LayoutWidget::new(false)),
        "weather" => Box::new(WeatherWidget::new()),
        "caffeine" => Box::new(CaffeineWidget::new()),
        "volume" => Box::new(VolumeWidget::new()),
        "brightness" => Box::new(BrightnessWidget::new()),
        "bluetooth" => Box::new(BluetoothWidget::new()),
        "wifi" => Box::new(WifiWidget::new()),
        "battery" => Box::new(BatteryWidget::new()),
        "notifications" => Box::new(NotificationsWidget::new()),
        _ => return None,
    };
    Some(widget)
}

fn widget(entry: &str) -> Option<Box<dyn BarWidget>> {
    match BarSettings::plugin_entry(entry) {
        Some((plugin, widget)) => Some(Box::new(RemoteWidget::new(entry, plugin, widget))),
        None => builtin(entry),
    }
}

impl WidgetRegistry {
    /// Every widget `arrangement` names, in its slot.
    pub fn arranged(arrangement: &Arrangement) -> Self {
        let mut registry = Self::new();
        for (slot, entry) in arrangement.entries() {
            match widget(entry) {
                Some(widget) => registry.register(widget, slot),
                None => log::warn!("bar.ron names `{entry}`, which is no widget"),
            }
        }
        registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(registry: &WidgetRegistry) -> Vec<(WidgetSlot, String)> {
        registry
            .placed()
            .map(|(slot, widget)| (slot, widget.id().to_owned()))
            .collect()
    }

    #[test]
    fn the_default_settings_are_the_stock_bar() {
        let registry = WidgetRegistry::arranged(&Arrangement::from(&BarSettings::default()));
        let placed = layout(&registry);
        assert_eq!(placed[0], (WidgetSlot::Left, "clock".to_owned()));
        let right: Vec<_> = placed[1..]
            .iter()
            .map(|(slot, id)| {
                assert_eq!(*slot, WidgetSlot::Right);
                id.as_str()
            })
            .collect();
        assert_eq!(
            right,
            [
                "layout",
                "weather",
                "caffeine",
                "volume",
                "brightness",
                "bluetooth",
                "wifi",
                "battery",
                "notifications"
            ]
        );
    }

    #[test]
    fn built_ins_left_out_are_hidden_and_plugins_take_their_slot() {
        let arrangement = Arrangement {
            left: vec!["plugin:crown.example.cpu-graph/status".into()],
            center: vec!["clock".into(), "no-such-widget".into()],
            right: vec!["battery".into()],
        };
        let registry = WidgetRegistry::arranged(&arrangement);
        assert_eq!(
            layout(&registry),
            [
                (
                    WidgetSlot::Left,
                    "plugin:crown.example.cpu-graph/status".to_owned()
                ),
                (WidgetSlot::Center, "clock".to_owned()),
                (WidgetSlot::Right, "battery".to_owned()),
            ]
        );
        let plugin = registry
            .widget(0)
            .and_then(|widget| widget.plugin_surface());
        assert!(plugin.is_none(), "no tree has arrived for the plugin yet");
    }

    #[test]
    fn every_built_in_answers_to_its_own_name() {
        for name in BarSettings::default()
            .left
            .iter()
            .chain(&BarSettings::default().right)
        {
            let widget = builtin(name).map(|widget| widget.id().to_owned());
            assert_eq!(widget.as_deref(), Some(name.as_str()));
        }
    }

    #[test]
    fn a_bar_never_holds_more_widgets_than_its_mask() {
        let arrangement = Arrangement {
            right: vec!["clock".to_owned(); 80],
            ..Arrangement::default()
        };
        assert_eq!(
            WidgetRegistry::arranged(&arrangement).len(),
            crate::widgets::MAX_WIDGETS
        );
    }
}

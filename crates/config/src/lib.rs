pub use crownos_config::schema::{Bar as BarSettings, BarPosition};

pub static BAR_NAMESPACE: &str = "crownbar";
pub static BAR_HEIGHT: u32 = 40;

/// Where the bar sits and how it is shaped. Followed live, except for
/// `position`: the layer surface's anchor is described only when it is made.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarConfig {
    pub bar_height: u32,
    pub position: BarPosition,
    /// Distance from the screen's edges; zero unless the bar floats.
    pub margin: u32,
    /// Corner radius of a floating bar's ground; `None` for a docked bar.
    pub radius: Option<f32>,
}

impl BarConfig {
    /// The bar's own settings, out of `bar.ron`, falling back to
    /// `fallback_height` — `appearance.bar_height`.
    pub fn new(settings: &BarSettings, fallback_height: u32) -> Self {
        let bar_height = settings.height_or(fallback_height);
        let floating = settings.floating;
        BarConfig {
            bar_height,
            position: settings.position,
            margin: if floating {
                u32::from(settings.margin)
            } else {
                0
            },
            radius: floating.then(|| settings.radius.map_or(bar_height as f32 / 2.0, f32::from)),
        }
    }

    /// The bar's extent from its screen edge, margin included.
    pub fn reach(&self) -> u32 {
        self.margin + self.bar_height
    }

    /// `next` as it can be applied to a running bar: on this bar's edge.
    pub fn reshaped_to(&self, next: Self) -> Self {
        Self {
            position: self.position,
            ..next
        }
    }
}

#[cfg(test)]
mod tests {
    use crownconfig::Appearance;

    use super::*;

    #[test]
    fn a_docked_bar_ignores_its_margin_and_radius() {
        let settings = BarSettings {
            height: Some(32),
            margin: 10,
            radius: Some(6),
            ..BarSettings::default()
        };
        let config = BarConfig::new(&settings, Appearance::default().bar_height);
        assert_eq!(
            (config.bar_height, config.margin, config.radius),
            (32, 0, None)
        );
    }

    #[test]
    fn a_floating_bar_is_inset_and_rounded() {
        let settings = BarSettings {
            floating: true,
            margin: 8,
            position: BarPosition::Bottom,
            ..BarSettings::default()
        };
        let appearance = Appearance::default();
        let config = BarConfig::new(&settings, appearance.bar_height);
        assert_eq!(config.margin, 8);
        assert_eq!(config.radius, Some(appearance.bar_height as f32 / 2.0));
        assert_eq!(config.reach(), appearance.bar_height + 8);
    }

    #[test]
    fn a_running_bar_reshapes_but_stays_on_its_edge() {
        let running = BarConfig::new(&BarSettings::default(), 32);
        let moved = BarConfig::new(
            &BarSettings {
                height: Some(44),
                position: BarPosition::Bottom,
                ..BarSettings::default()
            },
            32,
        );
        let reshaped = running.reshaped_to(moved);
        assert_eq!(reshaped.bar_height, 44);
        assert_eq!(reshaped.position, running.position);
    }
}

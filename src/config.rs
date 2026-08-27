use crownos_config::{load, Appearance};

pub static BAR_NAMESPACE: &str = "crownbar";
pub static BAR_HEIGHT: u32 = 40;

pub struct BarConfig {
    pub bar_height: u32,
}

impl BarConfig {
    pub fn load() -> Self {
        let appearance: Appearance = load(Appearance::SECTION);
        BarConfig {
            bar_height: appearance.bar_height,
        }
    }
}

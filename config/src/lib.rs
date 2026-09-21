use crownos_config::Appearance;

pub static BAR_NAMESPACE: &str = "crownbar";
pub static POPUP_NAMESPACE: &str = "crownbar-popup";
pub static BAR_HEIGHT: u32 = 40;

pub struct BarConfig {
    pub bar_height: u32,
}

impl BarConfig {
    /// The bar's own settings, out of the same `appearance.ron` the palette
    /// came from — [`crate::theme::load`] returns it so the file is read once.
    pub fn from_appearance(appearance: &Appearance) -> Self {
        BarConfig {
            bar_height: appearance.bar_height,
        }
    }
}

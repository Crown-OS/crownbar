pub mod global_menu;

use iced_layershell::reexport::core::Svg;

pub struct BarWidgets {
    widgets: Vec<Box<dyn BarWidget>>,
}

pub enum BarWidgetMenuState {
    Checkbox(bool),
    Toggle(bool),
    Slider(i8),
}

pub struct BarWidgetMenu {
    name: String,
    state: BarWidgetMenuState,
    action: fn(BarWidgetMenuState),
}

pub trait BarWidget {
    fn get_icon(&self) -> Svg;
    fn get_menus(&self) -> Vec<BarWidgetMenu>;
}

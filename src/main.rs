mod ui;
mod widgets;

use iced::widget::{button, column, row, text, text_input};
use iced::{event, Alignment, Color, Element, Event, Length, Subscription, Task};
use iced_layershell::application;
use iced_layershell::reexport::Anchor;
use iced_layershell::settings::{LayerShellSettings, Settings, StartMode};
use iced_layershell::to_layer_message;

fn namespace() -> String {
    "Crownbar".to_string()
}

#[derive(Default, Debug, Clone)]
pub struct State {
    current_app_name: String,
    app_menus: String,
}

pub fn main() -> Result<(), iced_layershell::Error> {
    let binded_output_name = std::env::args().nth(1);
    let start_mode = match binded_output_name {
        Some(output) => StartMode::TargetScreen(output),
        None => StartMode::Active,
    };

    application(|| State::default(), namespace, update, view)
        .settings(Settings {
            layer_settings: LayerShellSettings {
                size: Some((0, 400)),
                exclusive_zone: 400,
                anchor: Anchor::Top | Anchor::Left | Anchor::Right,
                start_mode,
                ..Default::default()
            },
            ..Default::default()
        })
        .run()
}

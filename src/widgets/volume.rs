//! Volume widget and the Sound panel behind it.
//!
//! Everything comes from [`crate::services::audio`], which follows PipeWire's
//! own `Props` events — so the level is right the instant anything changes it,
//! and the slider writes continuously instead of only on release.

use std::sync::Arc;

use crate::{
    animation::Spring,
    services::{
        audio::{AudioCommand, AudioState, Device, DeviceKind, MAX_LEVEL},
        link::{self, SettingsPane},
        Services,
    },
    widgets::{
        popup::{Item, PanelBuilder, Row},
        AfterAction, BarWidget, Icon, PopupAction, PopupSpec, Rune, WidgetSlot,
    },
};

pub struct VolumeWidget {
    audio: Arc<AudioState>,
    level: Spring,
    muted: Spring,
    targets: Vec<Option<Target>>,
}

/// What a clickable or draggable row of the Sound panel stands for.
#[derive(Clone, Copy)]
enum Target {
    OutputLevel,
    InputLevel,
    Output(usize),
    Input(usize),
    Settings,
}

impl VolumeWidget {
    pub fn new() -> Self {
        Self {
            audio: Arc::default(),
            level: Spring::new(0.0),
            muted: Spring::new(0.0),
            targets: Vec::new(),
        }
    }

    fn retarget(&mut self) -> bool {
        let volume = self.audio.output_volume();
        self.level.set_target(volume.level)
            | self.muted.set_target(if volume.muted { 1.0 } else { 0.0 })
    }

    /// The device the slider is controlling, for the glyph beside it.
    fn output_rune(&self) -> Rune {
        self.audio
            .default_output()
            .map(|device| rune_for(device.kind))
            .unwrap_or(Rune::Speaker)
    }

    fn device_rows(
        panel: &mut PanelBuilder<Target>,
        title: &str,
        devices: &[Device],
        target: impl Fn(usize) -> Target,
    ) {
        if devices.is_empty() {
            return;
        }
        panel.row(Row::Separator);
        panel.row(Row::Section {
            title: title.into(),
            chevron: false,
        });
        for (index, device) in devices.iter().enumerate() {
            panel.action(
                Item::new(&device.description)
                    .icon(Icon::Rune(rune_for(device.kind)))
                    .selected(device.default)
                    .row(),
                target(index),
            );
        }
    }
}

impl BarWidget for VolumeWidget {
    fn id(&self) -> &'static str {
        "volume"
    }

    fn slot(&self) -> WidgetSlot {
        WidgetSlot::Right
    }

    fn visible(&self) -> bool {
        self.audio.availability.usable()
    }

    fn icon(&self) -> Icon {
        Icon::Volume {
            level: self.level.position,
            muted: self.muted.position,
        }
    }

    fn sync(&mut self, services: &Services) -> bool {
        let audio = services.audio.read();
        if Arc::ptr_eq(&audio, &self.audio) {
            return false;
        }
        self.audio = audio;
        self.retarget()
    }

    fn popup(&mut self, _services: &Services) -> Option<PopupSpec> {
        let mut panel = PanelBuilder::new();
        panel.row(Row::Header {
            title: "Sound".into(),
            toggle: None,
        });

        let output = self.audio.output_volume();
        panel.action(
            Row::Slider {
                icon: Icon::Rune(self.output_rune()),
                value: if output.muted { 0.0 } else { output.level },
            },
            Target::OutputLevel,
        );
        Self::device_rows(&mut panel, "Output", &self.audio.outputs, Target::Output);

        if !self.audio.inputs.is_empty() {
            panel.row(Row::Separator);
            panel.row(Row::Section {
                title: "Input".into(),
                chevron: false,
            });
            let input = self.audio.input_volume();
            panel.action(
                Row::Slider {
                    icon: Icon::Rune(Rune::Microphone),
                    value: if input.muted { 0.0 } else { input.level },
                },
                Target::InputLevel,
            );
            for (index, device) in self.audio.inputs.iter().enumerate() {
                panel.action(
                    Item::new(&device.description)
                        .icon(Icon::Rune(rune_for(device.kind)))
                        .selected(device.default)
                        .row(),
                    Target::Input(index),
                );
            }
            // No level meter is possible — pipewire-native has no stream API —
            // but who is holding the microphone is the question people
            // actually ask of a bar, and the registry answers it for free.
            let users = self.audio.microphone_users();
            if !users.is_empty() {
                panel.row(
                    Item::new(format!("In use by {}", users.join(", ")))
                        .plain()
                        .enabled(false)
                        .row(),
                );
            }
        }

        panel.row(Row::Separator);
        panel.action(
            Row::Action {
                label: "Sound Settings…".into(),
            },
            Target::Settings,
        );

        let (spec, targets) = panel.finish();
        self.targets = targets;
        Some(spec)
    }

    fn on_popup(&mut self, action: PopupAction, services: &Services) -> AfterAction {
        let target = match action {
            PopupAction::Slide { row, .. } | PopupAction::Activate { row } => {
                self.targets.get(row).copied().flatten()
            }
            PopupAction::Toggle { .. } => None,
        };
        let Some(target) = target else {
            return AfterAction::Stay;
        };

        match (target, action) {
            // A write is a socket message now, not a subprocess, so the drag
            // is sent as it happens rather than held back until release.
            (Target::OutputLevel, PopupAction::Slide { value, .. }) => {
                let level = value.clamp(0.0, MAX_LEVEL);
                self.level.set_target(level);
                self.level.snap_to_target();
                if self.audio.output_volume().muted && level > 0.0 {
                    services.audio.send(AudioCommand::SetOutputMuted(false));
                }
                services.audio.send(AudioCommand::SetOutputVolume(level));
                AfterAction::Stay
            }
            (Target::InputLevel, PopupAction::Slide { value, .. }) => {
                services
                    .audio
                    .send(AudioCommand::SetInputVolume(value.clamp(0.0, MAX_LEVEL)));
                AfterAction::Stay
            }
            (Target::Output(index), PopupAction::Activate { .. }) => {
                if let Some(device) = self.audio.outputs.get(index) {
                    services
                        .audio
                        .send(AudioCommand::SetDefaultOutput(device.name.clone()));
                }
                AfterAction::Close
            }
            (Target::Input(index), PopupAction::Activate { .. }) => {
                if let Some(device) = self.audio.inputs.get(index) {
                    services
                        .audio
                        .send(AudioCommand::SetDefaultInput(device.name.clone()));
                }
                AfterAction::Close
            }
            (Target::Settings, PopupAction::Activate { .. }) => {
                if !link::open(SettingsPane::Sound) {
                    log::info!("no sound settings application installed");
                }
                AfterAction::Close
            }
            _ => AfterAction::Stay,
        }
    }

    fn tick_animation(&mut self, dt: f32) -> bool {
        let mut alive = false;
        for spring in [&mut self.level, &mut self.muted] {
            if !spring.at_rest() {
                spring.step(dt);
                alive |= !spring.at_rest();
            }
        }
        alive
    }
}

fn rune_for(kind: DeviceKind) -> Rune {
    match kind {
        DeviceKind::Headphones | DeviceKind::Headset => Rune::Headphones,
        DeviceKind::Bluetooth => Rune::Bluetooth,
        DeviceKind::Display => Rune::Display,
        DeviceKind::Microphone | DeviceKind::Webcam => Rune::Microphone,
        DeviceKind::Speakers | DeviceKind::Unknown => Rune::Laptop,
    }
}

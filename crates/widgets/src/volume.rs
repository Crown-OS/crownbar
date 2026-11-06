//! Volume widget and the Sound panel behind it.
//!
//! Everything comes from [`crownbar_integrations::audio`], which follows PipeWire's
//! own `Props` events — so the level is right the instant anything changes it,
//! and the slider writes continuously instead of only on release.
//!
//! Every slider addresses its node by id, the way pavucontrol does: the
//! default output, the default input, and each application playing. Pressing
//! a slider's icon mutes what it controls.

use crate::{
    AfterAction, BarWidget, Icon, PopupAction, PopupSpec, Rune,
    animation::Spring,
    popup::{Header, Item, PanelBuilder, Row},
    snapshot::Snapshot,
};
use crownbar_integrations::{
    Services,
    audio::{AudioCommand, AudioState, Device, DeviceKind, MAX_LEVEL, NodeId, Stream, Volume},
    link::{self, SettingsPane},
};

pub struct VolumeWidget {
    audio: Snapshot<AudioState>,
    level: Spring,
    muted: Spring,
    targets: Vec<Option<Target>>,
}

/// What a clickable or draggable row of the Sound panel stands for. Devices
/// are held by id rather than position: the lists re-sort whenever the
/// default changes, which can land between building the panel and a click.
#[derive(Clone, Copy)]
enum Target {
    Level(NodeId),
    Output(NodeId),
    Input(NodeId),
    Settings,
}

impl VolumeWidget {
    pub fn new() -> Self {
        Self {
            audio: Snapshot::default(),
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

    /// A drag on any slider. Moving a muted one unmutes it, as every mixer does.
    fn slide(&mut self, id: NodeId, value: f32, services: &Services) {
        let level = value.clamp(0.0, MAX_LEVEL);
        if self.audio.default_output().is_some_and(|d| d.id == id) {
            self.level.set_target(level);
            self.level.snap_to_target();
        }
        if level > 0.0 && self.audio.volume(id).is_some_and(|v| v.muted) {
            services
                .audio
                .send(AudioCommand::SetNodeMuted { id, muted: false });
        }
        services
            .audio
            .send(AudioCommand::SetNodeVolume { id, level });
    }

    fn toggle_mute(&self, id: NodeId, services: &Services) {
        if let Some(volume) = self.audio.volume(id) {
            services.audio.send(AudioCommand::SetNodeMuted {
                id,
                muted: !volume.muted,
            });
        }
    }
}

impl BarWidget for VolumeWidget {
    fn id(&self) -> &str {
        "volume"
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
        if !self.audio.replace(services.audio.read()) {
            return false;
        }
        self.retarget()
    }

    fn popup(&mut self, _services: &Services) -> Option<PopupSpec> {
        let audio = self.audio.for_panel();
        let mut panel = PanelBuilder::new();
        panel.row(Header::new("Sound").row());

        let output = audio.default_output();
        let output_rune = output.map_or(Rune::Speaker, |device| rune_for(device.kind));
        level_row(&mut panel, output_rune, output.map(|d| (d.id, d.volume)));
        if !audio.outputs.is_empty() {
            section(&mut panel, "Output");
            device_rows(&mut panel, &audio.outputs, Target::Output);
        }

        application_rows(&mut panel, audio.playing());

        if !audio.inputs.is_empty() {
            section(&mut panel, "Input");
            let input = audio.default_input().map(|d| (d.id, d.volume));
            level_row(&mut panel, Rune::Microphone, input);
            device_rows(&mut panel, &audio.inputs, Target::Input);
            // No level meter is possible — pipewire-native has no stream API —
            // but who is holding the microphone is the question people
            // actually ask of a bar, and the registry answers it for free.
            let users = audio.microphone_users();
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
            (Target::Level(id), PopupAction::Slide { value, .. }) => {
                self.slide(id, value, services);
                AfterAction::Stay
            }
            (Target::Level(id), PopupAction::Activate { .. }) => {
                self.toggle_mute(id, services);
                AfterAction::Stay
            }
            (Target::Output(id), PopupAction::Activate { .. }) => {
                if let Some(name) = device_name(&self.audio.outputs, id) {
                    services.audio.send(AudioCommand::SetDefaultOutput(name));
                }
                AfterAction::Close
            }
            (Target::Input(id), PopupAction::Activate { .. }) => {
                if let Some(name) = device_name(&self.audio.inputs, id) {
                    services.audio.send(AudioCommand::SetDefaultInput(name));
                }
                AfterAction::Close
            }
            (Target::Settings, PopupAction::Activate { .. }) => {
                link::open(SettingsPane::Sound);
                AfterAction::Close
            }
            _ => AfterAction::Stay,
        }
    }

    fn panel_stale(&self) -> bool {
        self.audio.panel_stale()
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

fn section(panel: &mut PanelBuilder<Target>, title: &str) {
    panel.row(Row::Separator);
    panel.row(Row::Section {
        title: title.into(),
        chevron: false,
    });
}

/// A level slider for `node`, or an inert empty one while there is none.
fn level_row(panel: &mut PanelBuilder<Target>, icon: Rune, node: Option<(NodeId, Volume)>) {
    let icon = Icon::Rune(icon);
    let Some((id, volume)) = node else {
        panel.row(Row::Slider {
            icon,
            value: 0.0,
            muted: None,
        });
        return;
    };
    panel.action(
        Row::Slider {
            icon,
            value: volume.audible_level(),
            muted: Some(volume.muted),
        },
        Target::Level(id),
    );
}

fn device_rows(
    panel: &mut PanelBuilder<Target>,
    devices: &[Device],
    target: impl Fn(NodeId) -> Target,
) {
    for device in devices {
        panel.action(
            Item::new(&device.description)
                .icon(Icon::Rune(rune_for(device.kind)))
                .selected(device.default)
                .row(),
            target(device.id),
        );
    }
}

/// One slider per application stream, each under the application's name.
fn application_rows(panel: &mut PanelBuilder<Target>, streams: &[Stream]) {
    if streams.is_empty() {
        return;
    }
    panel.row(Row::Separator);
    for stream in streams {
        panel.row(Row::Section {
            title: stream.app.clone(),
            chevron: false,
        });
        level_row(panel, Rune::MusicalNote, Some((stream.id, stream.volume)));
    }
}

fn device_name(devices: &[Device], id: NodeId) -> Option<String> {
    devices
        .iter()
        .find(|device| device.id == id)
        .map(|device| device.name.clone())
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

//! Volume widget and the Sound panel behind it.
//!
//! Everything comes from [`crownbar_integrations::audio`], which follows PipeWire's
//! own `Props` events — so the level is right the instant anything changes it,
//! and the slider writes continuously instead of only on release.
//!
//! Every slider addresses its node by id, the way pavucontrol does: the
//! default output, the default input, and each application playing. Pressing
//! a slider's icon mutes what it controls.
//!
//! A linked phone that can lend its microphone joins the inputs; picking it
//! asks crownlink to start, or stop, streaming it here.

use crate::{
    AfterAction, BarWidget, Icon, PopupAction, PopupSpec, Rune,
    animation::Spring,
    phone,
    popup::{Header, Item, PanelBuilder, Row},
    snapshot::Snapshot,
};
use crownbar_integrations::{
    Services,
    audio::{AudioCommand, AudioState, Device, DeviceKind, MAX_LEVEL, NodeId, Stream, Volume},
    crownlink::{self, DeviceId, Feature, PhoneDevice, PhoneState},
    link::{self, SettingsPane},
};

pub struct VolumeWidget {
    audio: Snapshot<AudioState>,
    phones: Snapshot<PhoneState>,
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
    PhoneMic(DeviceId),
    Settings,
}

impl VolumeWidget {
    pub fn new() -> Self {
        Self {
            audio: Snapshot::default(),
            phones: Snapshot::default(),
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

    fn panel(&mut self) -> PopupSpec {
        let audio = self.audio.for_panel();
        let phones = self.phones.for_panel();
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

        let mut phone_mics = phone::allowing(&phones.devices, Feature::Mic).peekable();
        if !audio.inputs.is_empty() || phone_mics.peek().is_some() {
            section(&mut panel, "Input");
        }
        if !audio.inputs.is_empty() {
            let input = audio.default_input().map(|d| (d.id, d.volume));
            level_row(&mut panel, Rune::Microphone, input);
            device_rows(&mut panel, &audio.inputs, Target::Input);
        }
        phone_mics.for_each(|device| phone_mic_row(&mut panel, device));
        // No level meter is possible — pipewire-native has no stream API —
        // but who is holding the microphone is the question people actually
        // ask of a bar, and the registry answers it for free.
        let users = audio.microphone_users();
        if !audio.inputs.is_empty() && !users.is_empty() {
            panel.row(
                Item::new(format!("In use by {}", users.join(", ")))
                    .plain()
                    .enabled(false)
                    .row(),
            );
        }

        panel.row(Row::Separator);
        panel.action(Row::action("Sound Settings…"), Target::Settings);

        let (spec, targets) = panel.finish();
        self.targets = targets;
        spec
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
        self.phones.replace(services.crownlink.read());
        if !self.audio.replace(services.audio.read()) {
            return false;
        }
        self.retarget()
    }

    fn popup(&mut self, _services: &Services) -> Option<PopupSpec> {
        Some(self.panel())
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
            (Target::PhoneMic(id), PopupAction::Activate { .. }) => {
                phone::toggle(services, id, crownlink::Stream::Mic);
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
        self.audio.panel_stale() || self.phones.panel_stale()
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

/// A linked device's microphone, chosen while it streams here.
fn phone_mic_row(panel: &mut PanelBuilder<Target>, device: &PhoneDevice) {
    panel.action(
        phone::item(device, Feature::Mic)
            .detail(phone::kind(device.class))
            .selected(device.is_streaming(crownlink::Stream::Mic))
            .row(),
        Target::PhoneMic(device.id),
    );
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::phone::fixture::*;

    fn sound_panel(devices: Vec<PhoneDevice>) -> (VolumeWidget, PopupSpec) {
        let mut widget = VolumeWidget::new();
        widget.audio.replace(Arc::new(AudioState::default()));
        widget.phones.replace(state(devices));
        let spec = widget.panel();
        (widget, spec)
    }

    #[test]
    fn no_phone_and_no_microphone_means_no_input_heading() {
        let (_, spec) = sound_panel(Vec::new());
        assert!(!sections(&spec).contains(&"Input"));
    }

    #[test]
    fn each_phone_that_lends_its_mic_is_an_input() {
        for devices in rosters() {
            let count = devices.len();
            let (widget, spec) = sound_panel(devices.clone());
            let inputs = section_items(&spec, "Input");
            assert_eq!(inputs.len(), count);
            for ((row, item), device) in inputs.iter().zip(&devices) {
                assert_eq!(item.label, &*device.name);
                assert_eq!(item.detail.as_deref(), Some("Phone"));
                assert_eq!(item.icon, Icon::Rune(Rune::Phone));
                assert!(item.badge && !item.selected);
                assert!(
                    matches!(widget.targets[*row], Some(Target::PhoneMic(id)) if id == device.id)
                );
            }
        }
    }

    #[test]
    fn a_streaming_phone_mic_is_the_chosen_input() {
        let (_, spec) = sound_panel(vec![
            running(phone(ONEPLUS, "OnePlus 11 5G"), Feature::Mic),
            phone(PIXEL, "Pixel 8"),
            forbidding(phone(DeviceId([3; 32]), "Tablet"), Feature::Mic),
        ]);
        let chosen: Vec<_> = section_items(&spec, "Input")
            .iter()
            .map(|(_, item)| (item.label.as_str(), item.selected))
            .collect();
        assert_eq!(chosen, [("OnePlus 11 5G", true), ("Pixel 8", false)]);
    }

    #[test]
    fn a_phone_that_leaves_takes_only_its_own_row() {
        let [_, _, both] = rosters();
        let (_, before) = sound_panel(both.clone());
        let (_, after) = sound_panel(both[1..].to_vec());
        let identity = |spec: &PopupSpec, name: &str| {
            section_items(spec, "Input")
                .into_iter()
                .find(|(_, item)| item.label == name)
                .and_then(|(_, item)| item.identity)
        };
        assert_eq!(labels(&section_items(&after, "Input")), ["Pixel 8"]);
        assert_eq!(identity(&before, "Pixel 8"), identity(&after, "Pixel 8"));
        assert!(identity(&before, "Pixel 8").is_some());
    }
}

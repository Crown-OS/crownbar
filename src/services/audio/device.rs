//! What an audio device is, in the bar's terms.

use pipewire_native::properties::Properties;

/// A PipeWire global, identified the way the server does. Not stable across a
/// server restart, which is why a default-device write quotes [`Device::name`].
pub type NodeId = u32;

/// Perceptual volume — the number the slider draws — and the mute bit.
///
/// This is deliberately *not* PipeWire's gain: `gain = level³`. A linear
/// slider sounds broken, because half travel is already near-silent. Doing the
/// mapping here rather than in the widget is what keeps the bar, a future OSD
/// and crownsettings from disagreeing about what 40% means.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Volume {
    pub level: f32,
    pub muted: bool,
}

impl Volume {
    pub fn from_gain(gain: f32, muted: bool) -> Self {
        Self {
            level: gain.max(0.0).cbrt().clamp(0.0, MAX_LEVEL),
            muted,
        }
    }

    pub fn gain(self) -> f32 {
        self.level.clamp(0.0, MAX_LEVEL).powi(3)
    }
}

/// Over-amplification ceiling, matching what every other mixer allows.
pub const MAX_LEVEL: f32 = 1.5;

/// What a device physically is, for picking its glyph.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DeviceKind {
    Speakers,
    Headphones,
    Headset,
    Bluetooth,
    Display,
    Microphone,
    Webcam,
    #[default]
    Unknown,
}

impl DeviceKind {
    /// Classify from the global's own properties, best evidence first.
    ///
    /// The node carries the card's `device.*` keys but not
    /// `device.form-factor`, which lives on the `Device` global — so a
    /// built-in codec offers only the direction-free `audio-card-analog`, and
    /// the `media.class` the caller already knows is what settles playback
    /// against capture. Guessing it from the node name would classify a
    /// microphone as speakers, because both are `…analog-stereo`.
    pub fn classify(props: &Properties, role: Role) -> Self {
        if let Some(kind) = props.get("device.form-factor").and_then(form_factor) {
            return kind;
        }
        if let Some(kind) = props
            .get("device.icon-name")
            .or_else(|| props.get("node.icon-name"))
            .and_then(icon_name)
        {
            return kind;
        }
        if props.get("api.bluez5.address").is_some() || props.get("device.bus") == Some("bluetooth")
        {
            return Self::Bluetooth;
        }
        // A card's profile names its port: `hdmi-stereo`, `analog-stereo`.
        let profile = props.get("device.profile.name").unwrap_or_default();
        if profile.contains("hdmi") || profile.contains("iec958") {
            return Self::Display;
        }
        let name = props
            .get("node.name")
            .unwrap_or_default()
            .to_ascii_lowercase();
        match () {
            _ if name.contains("hdmi") || name.contains("displayport") => Self::Display,
            _ if name.contains("headphone") => Self::Headphones,
            _ if name.contains("headset") => Self::Headset,
            _ if name.contains("bluez") => Self::Bluetooth,
            _ => match role {
                Role::Input | Role::RecordStream => Self::Microphone,
                Role::Output | Role::PlaybackStream => Self::Speakers,
            },
        }
    }
}

fn form_factor(value: &str) -> Option<DeviceKind> {
    Some(match value {
        "internal" | "speaker" => DeviceKind::Speakers,
        "headphone" => DeviceKind::Headphones,
        "headset" | "handset" => DeviceKind::Headset,
        "microphone" => DeviceKind::Microphone,
        "webcam" => DeviceKind::Webcam,
        _ => return None,
    })
}

/// Freedesktop icon names, matched by prefix. `audio-card*` is deliberately
/// absent: every node on a built-in codec reports it, capture and playback
/// alike, so it would answer the one question it cannot.
fn icon_name(value: &str) -> Option<DeviceKind> {
    Some(match value {
        _ if value.starts_with("audio-headphones") => DeviceKind::Headphones,
        _ if value.starts_with("audio-headset") => DeviceKind::Headset,
        _ if value.starts_with("audio-input-microphone") => DeviceKind::Microphone,
        _ if value.starts_with("audio-speakers") => DeviceKind::Speakers,
        _ if value.starts_with("video-display") => DeviceKind::Display,
        _ if value.starts_with("camera-web") => DeviceKind::Webcam,
        _ => return None,
    })
}

/// An output or input device.
#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    pub id: NodeId,
    /// `node.name` — the stable key, and what a default-device write sends.
    pub name: String,
    /// `node.description` — what a panel row says.
    pub description: String,
    pub kind: DeviceKind,
    pub volume: Volume,
    pub default: bool,
    /// The card behind it, for route switching.
    pub card: Option<NodeId>,
    /// `card.profile.device` — which of that card's ports this node plays out
    /// of, and the key that picks its route.
    pub card_port: Option<i32>,
    /// Route priority; a panel sorts on it so Speakers outranks HDMI 3.
    pub priority: i32,
}

/// One application playing or recording.
#[derive(Clone, Debug, PartialEq)]
pub struct Stream {
    pub id: NodeId,
    /// `application.name`, falling back to `node.name`.
    pub app: String,
    /// `media.name` — the track or tab title, when the app sets one.
    pub title: Option<String>,
    pub icon_name: Option<String>,
    pub volume: Volume,
    /// The node is `Running` rather than merely `Idle` — actually moving
    /// audio. What makes the microphone indicator mean something.
    pub active: bool,
}

impl Device {
    pub fn from_props(id: NodeId, props: &Properties, role: Role) -> Self {
        Self {
            id,
            name: props.get("node.name").unwrap_or_default().to_owned(),
            description: props
                .get("node.description")
                .or_else(|| props.get("node.nick"))
                .or_else(|| props.get("node.name"))
                .unwrap_or_default()
                .to_owned(),
            kind: DeviceKind::classify(props, role),
            volume: Volume::default(),
            default: false,
            card: props.get_u32("device.id"),
            card_port: props.get_i32("card.profile.device"),
            priority: props.get_i32("priority.session").unwrap_or(0),
        }
    }

    /// Re-read from the full property set that arrives with the node's `info`
    /// event. The registry global carries only a summary — no
    /// `device.icon-name`, no ALSA keys — so a device classified at bind time
    /// is always `Unknown`.
    pub fn describe(&mut self, props: &Properties, role: Role) -> bool {
        let next = Self {
            volume: self.volume,
            default: self.default,
            ..Self::from_props(self.id, props, role)
        };
        let changed = next != *self;
        *self = next;
        changed
    }
}

impl Stream {
    pub fn from_props(id: NodeId, props: &Properties) -> Self {
        Self {
            id,
            app: props
                .get("application.name")
                .or_else(|| props.get("node.name"))
                .unwrap_or_default()
                .to_owned(),
            title: props.get("media.name").map(ToOwned::to_owned),
            icon_name: props
                .get("application.icon-name")
                .or_else(|| props.get("application.process.binary"))
                .map(ToOwned::to_owned),
            volume: Volume::default(),
            active: false,
        }
    }

    /// Re-read from the full property set that arrives with the node's `info`
    /// event. The registry global carries only a summary.
    pub fn describe(&mut self, props: &Properties) -> bool {
        let next = Self {
            volume: self.volume,
            active: self.active,
            ..Self::from_props(self.id, props)
        };
        let changed = next != *self;
        *self = next;
        changed
    }
}

/// Which side of the server a node sits on, from `media.class`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Output,
    Input,
    PlaybackStream,
    RecordStream,
}

impl Role {
    pub fn classify(props: &Properties) -> Option<Self> {
        match props.get("media.class")? {
            "Audio/Sink" => Some(Self::Output),
            // A source whose name ends `.monitor` is a sink's loopback, not a
            // microphone; listing it would offer the user their own speakers
            // as a recording device.
            "Audio/Source" | "Audio/Source/Virtual" => props
                .get("node.name")
                .map(|name| !name.ends_with(".monitor"))
                .unwrap_or(true)
                .then_some(Self::Input),
            "Stream/Output/Audio" => Some(Self::PlaybackStream),
            "Stream/Input/Audio" => Some(Self::RecordStream),
            _ => None,
        }
    }
}

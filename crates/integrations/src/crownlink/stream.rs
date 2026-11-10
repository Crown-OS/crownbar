use crownlink_linux::ipc::proto::{
    CameraLens, CameraOptions, Direction, Edge, Feature, MirrorOptions, MonitorOptions, VideoLimits,
};

/// What a linked device can stream to this computer. The other [`Feature`]s
/// have nothing to start or stop, so a command can only ever name one of these.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Stream {
    /// The device's camera, as a webcam.
    Camera,
    /// The device's microphone, as an input.
    Mic,
    /// The device's screen, mirrored in a window here.
    Mirror,
    /// The device as an extra monitor for this computer.
    Monitor,
}

impl Stream {
    pub const ALL: [Self; 4] = [Self::Camera, Self::Mic, Self::Mirror, Self::Monitor];

    pub const fn feature(self) -> Feature {
        match self {
            Self::Camera => Feature::Camera,
            Self::Mic => Feature::Mic,
            Self::Mirror => Feature::Mirror,
            Self::Monitor => Feature::Monitor,
        }
    }
}

/// A webcam's usual ceiling; the daemon settles on what the phone can do
/// within it.
pub(super) const CAMERA: CameraOptions = CameraOptions {
    lens: CameraLens::Front,
    limits: VideoLimits {
        max_width: 1920,
        max_height: 1080,
        max_fps: 30,
    },
};

/// Square, so a portrait phone screen is not scaled down to fit a landscape
/// box.
pub(super) const MIRROR: MirrorOptions = MirrorOptions {
    direction: Direction::FromPeer,
    limits: VideoLimits {
        max_width: 2560,
        max_height: 2560,
        max_fps: 60,
    },
    remote_input: true,
};

pub(super) const MONITOR: MonitorOptions = MonitorOptions {
    placement: Edge::Right,
    limits: VideoLimits {
        max_width: 1920,
        max_height: 1080,
        max_fps: 60,
    },
};

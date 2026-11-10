use std::sync::Arc;

use crownlink_linux::ipc::proto::{
    Battery, DeviceClass, DeviceId, DeviceInfo, Feature, FeatureSet,
};

use crate::crownlink::{PhoneCall, Stream};

/// One device linked to this computer right now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhoneDevice {
    pub id: DeviceId,
    pub name: Arc<str>,
    pub class: DeviceClass,
    /// `None` until the device has reported it.
    pub battery: Option<Battery>,
    /// The device's own mobile hotspot; `None` until the device has said.
    pub hotspot: Option<bool>,
    /// What the user allows this device to do.
    pub allowed: FeatureSet,
    /// What is running with it right now.
    pub active: FeatureSet,
    /// An answered call on the device's phone line.
    pub call: Option<PhoneCall>,
}

impl PhoneDevice {
    pub(super) fn new(info: DeviceInfo) -> Self {
        Self {
            id: info.id,
            name: Arc::from(info.name),
            class: info.class,
            battery: info.battery,
            hotspot: None,
            allowed: info.features,
            active: info.active,
            call: None,
        }
    }

    /// The listing's word on everything it carries; what only events carry —
    /// the hotspot and the call — is kept.
    pub(super) fn refresh(&mut self, info: DeviceInfo) {
        if *self.name != *info.name {
            self.name = Arc::from(info.name);
        }
        self.class = info.class;
        self.battery = info.battery.or(self.battery);
        self.allowed = info.features;
        self.active = info.active;
    }

    pub fn allows(&self, feature: Feature) -> bool {
        self.allowed.contains(feature)
    }

    pub fn is_streaming(&self, stream: Stream) -> bool {
        self.active.contains(stream.feature())
    }

    /// Everything streaming from the device, in [`Stream::ALL`] order.
    pub fn streams(&self) -> impl Iterator<Item = Stream> + '_ {
        Stream::ALL
            .into_iter()
            .filter(|stream| self.is_streaming(*stream))
    }

    pub(super) fn set_active(&mut self, feature: Feature, active: bool) {
        self.active = match active {
            true => self.active.with(feature),
            false => self.active.without(feature),
        };
    }
}

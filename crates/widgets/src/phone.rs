//! A linked device's rows in the panels it reaches into: its microphone in
//! Sound, its hotspot in Wi-Fi, its charge in Battery, its camera in Camera.
//!
//! [`crownbar_integrations::crownlink`] lists only the devices connected right
//! now, none while crownlink is down, so every section here is built from that
//! list and vanishes with it. Each row is keyed by its device rather than by
//! its place, so a phone leaving takes its own rows and no one else's.

use crownbar_integrations::{
    Services,
    crownlink::{DeviceClass, DeviceId, Feature, PhoneCommand, PhoneDevice, Stream},
};

use crate::{
    Icon, Rune,
    popup::{Item, PanelBuilder, Row},
};

/// The devices that let this computer use `feature`, in the service's order.
pub(crate) fn allowing(
    devices: &[PhoneDevice],
    feature: Feature,
) -> impl Iterator<Item = &PhoneDevice> {
    devices.iter().filter(move |device| device.allows(feature))
}

/// `device`'s row in the section about `feature`: its name beside its badge.
pub(crate) fn item(device: &PhoneDevice, feature: Feature) -> Item {
    Item::new(&*device.name)
        .icon(Icon::Rune(rune(device.class)))
        .identity((feature, device.id))
}

/// Start `stream` from the device, or stop it if it is already streaming. A
/// device that has gone since the panel was built is left alone.
pub(crate) fn toggle(services: &Services, id: DeviceId, stream: Stream) {
    if let Some(command) = flip(&services.crownlink.read().devices, id, stream) {
        services.crownlink.send(command);
    }
}

fn flip(devices: &[PhoneDevice], id: DeviceId, stream: Stream) -> Option<PhoneCommand> {
    let device = devices.iter().find(|device| device.id == id)?;
    Some(match device.is_streaming(stream) {
        true => PhoneCommand::Stop(id, stream),
        false => PhoneCommand::Start(id, stream),
    })
}

/// A separator and a heading, for a section that has at least one row.
pub(crate) fn section<T>(panel: &mut PanelBuilder<T>, title: &str) {
    panel.row(Row::Separator);
    panel.row(Row::Section {
        title: title.into(),
        chevron: false,
    });
}

/// What the device is, as a row's detail names it.
pub(crate) const fn kind(class: DeviceClass) -> &'static str {
    match class {
        DeviceClass::Phone => "Phone",
        DeviceClass::Tablet => "Tablet",
        DeviceClass::Watch => "Watch",
        DeviceClass::Computer => "Computer",
    }
}

const fn rune(class: DeviceClass) -> Rune {
    match class {
        DeviceClass::Computer => Rune::Laptop,
        DeviceClass::Phone | DeviceClass::Tablet | DeviceClass::Watch => Rune::Phone,
    }
}

#[cfg(test)]
pub(crate) mod fixture {
    use std::sync::Arc;

    use crownbar_integrations::crownlink::{
        Battery, DeviceClass, DeviceId, Feature, FeatureSet, PhoneDevice, PhoneState,
    };

    use crate::{
        PopupSpec,
        popup::{Item, Row},
    };

    pub(crate) const PIXEL: DeviceId = DeviceId([1; 32]);
    pub(crate) const ONEPLUS: DeviceId = DeviceId([2; 32]);

    /// A connected phone that allows everything and is running nothing.
    pub(crate) fn phone(id: DeviceId, name: &str) -> PhoneDevice {
        PhoneDevice {
            id,
            name: Arc::from(name),
            class: DeviceClass::Phone,
            battery: None,
            hotspot: None,
            allowed: Feature::ALL.into_iter().collect(),
            active: FeatureSet::EMPTY,
            call: None,
            activities: Vec::new(),
        }
    }

    pub(crate) fn charged(mut device: PhoneDevice, percent: u8) -> PhoneDevice {
        device.battery = Some(Battery {
            percent,
            charging: false,
        });
        device
    }

    pub(crate) fn running(mut device: PhoneDevice, feature: Feature) -> PhoneDevice {
        device.active = device.active.with(feature);
        device
    }

    pub(crate) fn forbidding(mut device: PhoneDevice, feature: Feature) -> PhoneDevice {
        device.allowed = device.allowed.without(feature);
        device
    }

    /// No devices, one, and two, as the service would list them.
    pub(crate) fn rosters() -> [Vec<PhoneDevice>; 3] {
        [
            Vec::new(),
            vec![phone(ONEPLUS, "OnePlus 11 5G")],
            vec![phone(ONEPLUS, "OnePlus 11 5G"), phone(PIXEL, "Pixel 8")],
        ]
    }

    pub(crate) fn state(devices: Vec<PhoneDevice>) -> Arc<PhoneState> {
        Arc::new(PhoneState {
            devices,
            ..PhoneState::default()
        })
    }

    pub(crate) fn sections(spec: &PopupSpec) -> Vec<&str> {
        spec.rows
            .iter()
            .filter_map(|row| match row {
                Row::Section { title, .. } => Some(title.as_str()),
                _ => None,
            })
            .collect()
    }

    /// The items under the heading `title`, up to the next separator, with
    /// their row indices.
    pub(crate) fn section_items<'a>(spec: &'a PopupSpec, title: &str) -> Vec<(usize, &'a Item)> {
        spec.rows
            .iter()
            .enumerate()
            .skip_while(|(_, row)| !matches!(row, Row::Section { title: t, .. } if t == title))
            .skip(1)
            .take_while(|(_, row)| !matches!(row, Row::Separator))
            .filter_map(|(index, row)| match row {
                Row::Item(item) => Some((index, &**item)),
                _ => None,
            })
            .collect()
    }

    pub(crate) fn labels<'a>(items: &[(usize, &'a Item)]) -> Vec<&'a str> {
        items.iter().map(|(_, item)| item.label.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{fixture::*, *};

    #[test]
    fn rows_are_keyed_by_device_and_section_not_by_name() {
        let pixel = phone(PIXEL, "Phone");
        let twin = phone(ONEPLUS, "Phone");
        let key = |device, feature| item(device, feature).identity;
        assert_ne!(key(&pixel, Feature::Mic), key(&twin, Feature::Mic));
        assert_ne!(key(&pixel, Feature::Mic), key(&pixel, Feature::Hotspot));
        assert_eq!(
            key(&pixel, Feature::Mic),
            key(&phone(PIXEL, "Renamed"), Feature::Mic)
        );
    }

    #[test]
    fn a_stream_is_started_when_idle_stopped_when_running_and_left_when_gone() {
        let devices = [
            running(phone(PIXEL, "Pixel 8"), Feature::Mic),
            phone(ONEPLUS, "OnePlus"),
        ];
        assert_eq!(
            flip(&devices, PIXEL, Stream::Mic),
            Some(PhoneCommand::Stop(PIXEL, Stream::Mic))
        );
        assert_eq!(
            flip(&devices, ONEPLUS, Stream::Mic),
            Some(PhoneCommand::Start(ONEPLUS, Stream::Mic))
        );
        assert_eq!(
            flip(&devices, PIXEL, Stream::Camera),
            Some(PhoneCommand::Start(PIXEL, Stream::Camera))
        );
        assert_eq!(flip(&devices, DeviceId([9; 32]), Stream::Mic), None);
    }

    #[test]
    fn only_devices_that_allow_a_feature_are_offered_it() {
        let devices = [
            forbidding(phone(PIXEL, "Pixel 8"), Feature::Mic),
            phone(ONEPLUS, "OnePlus"),
        ];
        let names: Vec<_> = allowing(&devices, Feature::Mic)
            .map(|device| &*device.name)
            .collect();
        assert_eq!(names, ["OnePlus"]);
    }
}

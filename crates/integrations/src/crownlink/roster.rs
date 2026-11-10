//! Folding what crownlink says into the devices the bar shows.
//!
//! Pure, so every edge case is a unit test: the session feeds it listings and
//! events and does whatever it asks for next.

use std::{collections::HashSet, time::Instant};

use crownlink_linux::ipc::proto::{Battery, CallInfo, DeviceId, DeviceInfo, Event, FeatureState};

use crate::crownlink::{PhoneDevice, PhoneState, call};

/// What the session must fetch before the state is whole again.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Followup {
    Settled,
    /// Devices that just arrived, whose hotspot and call no listing carries.
    Probe(Vec<DeviceId>),
    /// A device the bar has never been told about came online.
    Relist,
}

impl PhoneState {
    pub fn device(&self, id: DeviceId) -> Option<&PhoneDevice> {
        self.devices.iter().find(|device| device.id == id)
    }

    fn device_mut(&mut self, id: DeviceId) -> Option<&mut PhoneDevice> {
        self.devices.iter_mut().find(|device| device.id == id)
    }

    /// Take `listing` as the whole truth about which devices are linked, and
    /// return the ones that just arrived. A device missing from it, or listed
    /// but no longer connected, goes with its call and its streams.
    pub(crate) fn seed(&mut self, listing: Vec<DeviceInfo>) -> Vec<DeviceId> {
        let mut previous = std::mem::take(&mut self.devices);
        let mut arrived = Vec::new();
        let mut listed = HashSet::with_capacity(listing.len());
        self.devices = listing
            .into_iter()
            .filter(|info| info.connected && listed.insert(info.id))
            .map(
                |info| match previous.iter().position(|held| held.id == info.id) {
                    Some(index) => {
                        let mut device = previous.swap_remove(index);
                        device.refresh(info);
                        device
                    }
                    None => {
                        arrived.push(info.id);
                        PhoneDevice::new(info)
                    }
                },
            )
            .collect();
        self.devices
            .sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
        arrived
    }

    /// What a probe of a newly arrived device found. Events that beat the
    /// probe here are newer, so they win.
    pub(super) fn seed_topics(
        &mut self,
        id: DeviceId,
        hotspot: Option<bool>,
        calls: &[CallInfo],
        now: Instant,
    ) {
        let Some(device) = self.device_mut(id) else {
            return;
        };
        device.hotspot = device.hotspot.or(hotspot);
        if device.call.is_none() {
            for info in calls {
                call::follow(&mut device.call, info, now);
            }
        }
    }

    pub(crate) fn apply(&mut self, event: Event, now: Instant) -> Followup {
        match event {
            Event::DevicesChanged(changed) => return Followup::Probe(self.seed(changed.devices)),
            Event::DeviceConnected(event) if !event.connected => {
                self.devices.retain(|device| device.id != event.id);
            }
            Event::DeviceConnected(event) if self.device(event.id).is_none() => {
                return Followup::Relist;
            }
            Event::CallStateChanged(event) => {
                if let Some(device) = self.device_mut(event.id) {
                    call::follow(&mut device.call, &event.call, now);
                }
            }
            Event::BatteryChanged(event) => {
                if let Some(device) = self.device_mut(event.id) {
                    device.battery = Some(Battery {
                        percent: event.percent,
                        charging: event.charging,
                    });
                }
            }
            Event::HotspotChanged(event) => {
                if let Some(device) = self.device_mut(event.id) {
                    device.hotspot = Some(event.enabled);
                }
            }
            Event::FeatureChanged(event) => {
                if let Some(device) = self.device_mut(event.id) {
                    device.set_active(event.feature, event.state == FeatureState::Active);
                }
            }
            Event::FeatureFailed(event) => {
                log::debug!(
                    "crownlink: {:?} failed on {}: {}",
                    event.feature,
                    event.id,
                    event.reason
                );
                if let Some(device) = self.device_mut(event.id) {
                    device.set_active(event.feature, false);
                }
            }
            Event::DeviceConnected(_)
            | Event::PairingRequest(_)
            | Event::PairingResult(_)
            | Event::MediaChanged(_) => {}
        }
        Followup::Settled
    }
}

#[cfg(test)]
mod tests {
    use crownlink_linux::ipc::proto::{
        BatteryChanged, CallStateChanged, CallStatus, DeviceClass, DeviceConnected, DevicesChanged,
        Feature, FeatureChanged, FeatureFailed, HotspotChanged, LinkKind,
    };

    use super::*;
    use crate::crownlink::Stream;

    const PIXEL: DeviceId = DeviceId([1; 32]);
    const ONEPLUS: DeviceId = DeviceId([2; 32]);

    fn listed(id: DeviceId, name: &str, active: &[Feature]) -> DeviceInfo {
        DeviceInfo {
            id,
            name: name.to_owned(),
            class: DeviceClass::Phone,
            trusted: true,
            connected: true,
            link: LinkKind::Lan,
            battery: Some(Battery {
                percent: 80,
                charging: false,
            }),
            features: Feature::ALL.into_iter().collect(),
            active: active.iter().copied().collect(),
        }
    }

    fn call(id: &str, number: &str, status: CallStatus) -> CallInfo {
        CallInfo {
            call_id: id.to_owned(),
            number: number.to_owned(),
            contact_name: None,
            status,
            answered_unix_ms: None,
        }
    }

    fn call_event(id: DeviceId, call: CallInfo) -> Event {
        Event::CallStateChanged(CallStateChanged { id, call })
    }

    fn streams(state: &PhoneState, id: DeviceId) -> Vec<Stream> {
        state
            .device(id)
            .map(|device| device.streams().collect())
            .unwrap_or_default()
    }

    #[test]
    fn no_devices_is_an_empty_roster() {
        let mut state = PhoneState::default();
        assert!(state.seed(Vec::new()).is_empty());
        assert!(state.devices.is_empty());
    }

    #[test]
    fn every_device_is_tracked_on_its_own_in_a_stable_order() {
        let mut state = PhoneState::default();
        let arrived = state.seed(vec![
            listed(PIXEL, "Pixel 8", &[Feature::Camera]),
            listed(ONEPLUS, "OnePlus 11 5G", &[Feature::Mic, Feature::Mirror]),
        ]);
        assert_eq!(arrived, [PIXEL, ONEPLUS]);
        let names: Vec<_> = state.devices.iter().map(|d| &*d.name).collect();
        assert_eq!(names, ["OnePlus 11 5G", "Pixel 8"]);
        assert_eq!(streams(&state, PIXEL), [Stream::Camera]);
        assert_eq!(streams(&state, ONEPLUS), [Stream::Mic, Stream::Mirror]);

        let now = Instant::now();
        state.apply(
            Event::HotspotChanged(HotspotChanged {
                id: ONEPLUS,
                enabled: true,
            }),
            now,
        );
        state.apply(call_event(PIXEL, call("1", "123", CallStatus::Active)), now);
        let pixel = state.device(PIXEL).map(|d| (d.hotspot, d.call.is_some()));
        let oneplus = state.device(ONEPLUS).map(|d| (d.hotspot, d.call.is_some()));
        assert_eq!(pixel, Some((None, true)));
        assert_eq!(oneplus, Some((Some(true), false)));
    }

    #[test]
    fn a_device_that_goes_mid_call_takes_its_call_and_streams_with_it() {
        let now = Instant::now();
        let mut state = PhoneState::default();
        state.seed(vec![
            listed(PIXEL, "Pixel 8", &[Feature::Camera]),
            listed(ONEPLUS, "OnePlus", &[]),
        ]);
        state.apply(call_event(PIXEL, call("1", "123", CallStatus::Active)), now);
        let mut gone = listed(PIXEL, "Pixel 8", &[Feature::Camera]);
        gone.connected = false;
        let followup = state.apply(
            Event::DevicesChanged(DevicesChanged {
                devices: vec![gone, listed(ONEPLUS, "OnePlus", &[])],
            }),
            now,
        );
        assert_eq!(followup, Followup::Probe(Vec::new()));
        assert!(state.device(PIXEL).is_none());
        assert!(state.device(ONEPLUS).is_some());

        state.apply(
            Event::DeviceConnected(DeviceConnected {
                id: ONEPLUS,
                name: "OnePlus".to_owned(),
                connected: false,
            }),
            now,
        );
        assert!(state.devices.is_empty());
    }

    #[test]
    fn a_device_listed_twice_is_one_device() {
        let mut state = PhoneState::default();
        let arrived = state.seed(vec![
            listed(PIXEL, "Pixel 8", &[]),
            listed(ONEPLUS, "OnePlus", &[]),
            listed(PIXEL, "Pixel 8 (renamed)", &[]),
        ]);
        assert_eq!(arrived, [PIXEL, ONEPLUS]);
        let ids: Vec<_> = state.devices.iter().map(|device| device.id).collect();
        assert_eq!(ids, [ONEPLUS, PIXEL]);
    }

    #[test]
    fn a_relisting_keeps_what_only_events_carry() {
        let now = Instant::now();
        let mut state = PhoneState::default();
        state.seed(vec![listed(PIXEL, "Pixel 8", &[])]);
        state.apply(call_event(PIXEL, call("1", "123", CallStatus::Active)), now);
        state.apply(
            Event::HotspotChanged(HotspotChanged {
                id: PIXEL,
                enabled: true,
            }),
            now,
        );
        let arrived = state.seed(vec![listed(PIXEL, "Pixel 8 Pro", &[Feature::Mic])]);
        assert!(arrived.is_empty());
        let pixel = state.device(PIXEL);
        assert_eq!(pixel.map(|d| &*d.name), Some("Pixel 8 Pro"));
        assert_eq!(pixel.and_then(|d| d.hotspot), Some(true));
        assert!(pixel.is_some_and(|d| d.call.is_some() && d.is_streaming(Stream::Mic)));
    }

    #[test]
    fn a_failed_feature_stops_even_if_its_state_change_is_lost() {
        let now = Instant::now();
        let mut state = PhoneState::default();
        state.seed(vec![listed(PIXEL, "Pixel 8", &[])]);
        state.apply(
            Event::FeatureChanged(FeatureChanged {
                id: PIXEL,
                feature: Feature::Monitor,
                state: FeatureState::Active,
            }),
            now,
        );
        assert_eq!(streams(&state, PIXEL), [Stream::Monitor]);
        state.apply(
            Event::FeatureFailed(FeatureFailed {
                id: PIXEL,
                feature: Feature::Monitor,
                reason: "the phone went to sleep".to_owned(),
            }),
            now,
        );
        assert!(streams(&state, PIXEL).is_empty());
    }

    #[test]
    fn events_about_unknown_devices_change_nothing_but_a_new_one_asks_for_a_listing() {
        let now = Instant::now();
        let mut state = PhoneState::default();
        state.seed(vec![listed(PIXEL, "Pixel 8", &[])]);
        let before = state.clone();
        state.apply(call_event(ONEPLUS, call("1", "1", CallStatus::Active)), now);
        state.apply(
            Event::BatteryChanged(BatteryChanged {
                id: ONEPLUS,
                percent: 5,
                charging: true,
            }),
            now,
        );
        assert_eq!(state, before);
        let followup = state.apply(
            Event::DeviceConnected(DeviceConnected {
                id: ONEPLUS,
                name: "OnePlus".to_owned(),
                connected: true,
            }),
            now,
        );
        assert_eq!(followup, Followup::Relist);
    }

    #[test]
    fn a_probe_fills_gaps_but_never_overrides_an_event() {
        let now = Instant::now();
        let mut state = PhoneState::default();
        state.seed(vec![listed(PIXEL, "Pixel 8", &[])]);
        state.apply(
            Event::HotspotChanged(HotspotChanged {
                id: PIXEL,
                enabled: false,
            }),
            now,
        );
        state.seed_topics(
            PIXEL,
            Some(true),
            &[
                call("r", "999", CallStatus::Ringing),
                call("a", "123", CallStatus::Active),
            ],
            now,
        );
        let pixel = state.device(PIXEL);
        assert_eq!(pixel.and_then(|d| d.hotspot), Some(false));
        assert_eq!(
            pixel.and_then(|d| d.call.as_ref()).map(|c| c.caller()),
            Some("123")
        );
    }
}

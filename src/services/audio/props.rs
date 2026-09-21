//! Reading and writing a node's `Props` param.
//!
//! Volume goes through the node's own mixer rather than the card's hardware
//! route: it is a flat pod, it is what `wpctl` writes so behaviour does not
//! change under the bar, and it is the only stage that works uniformly for
//! Bluetooth sinks, HDMI and per-application streams — which a route cannot
//! touch at all.

use pipewire_native::proxy::node::Node;
use pipewire_native_spa::{
    param::{props::Prop, ParamType},
    pod::{
        builder::ObjectBuilder,
        parser::Parser,
        types::{PropertyFlags, Type},
        RawPodOwned,
    },
};

use crate::services::audio::device::Volume;

/// What one `Props` param said. Both fields are optional because the server
/// sends whichever subset changed.
#[derive(Clone, Copy, Debug, Default)]
pub struct PropsUpdate {
    pub gain: Option<f32>,
    pub muted: Option<bool>,
}

impl PropsUpdate {
    /// From a perceptual slider level. The cube law is applied here so no
    /// caller can forget it.
    pub fn gain(level: f32) -> Self {
        Self {
            gain: Some(
                Volume {
                    level,
                    muted: false,
                }
                .gain(),
            ),
            muted: None,
        }
    }

    pub fn mute(muted: bool) -> Self {
        Self {
            gain: None,
            muted: Some(muted),
        }
    }

    pub fn apply(self, volume: &mut Volume) -> bool {
        let next = Volume {
            level: self
                .gain
                .map(|gain| Volume::from_gain(gain, false).level)
                .unwrap_or(volume.level),
            muted: self.muted.unwrap_or(volume.muted),
        };
        let changed = next != *volume;
        *volume = next;
        changed
    }
}

/// Decode a `Props` object.
///
/// This walks raw `u32` keys rather than the typed `Prop` parser on purpose:
/// the typed parser errors on any key its 0.1 enum does not list, and its
/// `Iterator` impl reports that error as end-of-object — so a real `Props`
/// carrying an unknown key would silently truncate, losing a `Mute` that
/// happened to sit after it.
pub fn decode(pod: &RawPodOwned) -> PropsUpdate {
    let mut update = PropsUpdate::default();
    let mut parser = Parser::new(pod.data());
    let _ = parser.pop_object_raw(|object, _type, _id: u32| {
        for (key, _flags, value) in object {
            match Prop::try_from(key) {
                Ok(Prop::Mute) => update.muted = value.decode::<bool>().ok(),
                Ok(Prop::Volume) if update.gain.is_none() => {
                    update.gain = value.decode::<f32>().ok()
                }
                // Per-channel volumes win over the single `Volume`, which the
                // server sends as a legacy summary of them.
                Ok(Prop::ChannelVolumes) => {
                    if value.type_() == Type::Array
                        && let Ok(channels) = value.decode::<Vec<f32>>()
                        && !channels.is_empty()
                    {
                        // One number for a stereo pair: the loudest channel,
                        // so dragging the slider never quietly rebalances.
                        update.gain = channels.iter().copied().reduce(f32::max);
                    }
                }
                _ => {}
            }
        }
        Ok(())
    });
    update
}

/// How many channels to write. The server takes however many it is given and
/// spreads the last value across the rest, so two covers mono and stereo and
/// anything wider keeps its own balance.
pub(super) const CHANNELS: usize = 2;

pub fn set_volume(node: &Node, gain: f32) -> std::io::Result<()> {
    node.set_param(
        ParamType::Props,
        object_type(),
        0,
        Box::new(move |builder: ObjectBuilder<'_>| {
            builder.push_property(
                Prop::ChannelVolumes,
                PropertyFlags::empty(),
                vec![gain; CHANNELS],
            )
        }),
    )
}

pub fn set_muted(node: &Node, muted: bool) -> std::io::Result<()> {
    node.set_param(
        ParamType::Props,
        object_type(),
        0,
        Box::new(move |builder: ObjectBuilder<'_>| {
            builder.push_property(Prop::Mute, PropertyFlags::empty(), muted)
        }),
    )
}

fn object_type() -> pipewire_native_spa::pod::types::ObjectType {
    // `ParamType::Props` always maps to `ObjectType::Props`; the `Option` is
    // only `None` for `Invalid`.
    ParamType::Props
        .object_type()
        .unwrap_or(pipewire_native_spa::pod::types::ObjectType::Props)
}

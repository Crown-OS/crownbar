//! A card's ports, and the volume stage the rest of the desktop agrees on.
//!
//! A node's own `Props` is a second, independent gain: writing it works, but
//! `wpctl`, the volume keys and pavucontrol all drive the card's *route*, so a
//! bar that wrote only the node would move a slider nothing else could see and
//! leave two gains stacked on top of each other. Devices that have no route —
//! Bluetooth sinks, application streams — still take the node path.

use pipewire_native::proxy::device::Device;
use pipewire_native_spa::{
    param::{props::Prop, route::Route, ParamType},
    pod::{
        builder::{Builder, ObjectBuilder},
        parser::Parser,
        types::{Id, ObjectType, PropertyFlags, Type},
        RawPodOwned,
    },
};

use crate::services::audio::{device::Volume, props::PropsUpdate};

/// Which way a route carries audio. The pod stores this as an id, and SPA
/// numbers directions input-first.
const DIRECTION_OUTPUT: u32 = 1;

/// One port of one card.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Port {
    pub index: i32,
    /// The card's `card.profile.device`, which is what a node quotes to say
    /// which port it plays out of.
    pub device: i32,
    pub output: bool,
    pub volume: Volume,
}

/// Decode one `Route` param. `None` for a pod that carries no index — an
/// enumeration entry rather than an active route.
pub fn decode(pod: &RawPodOwned) -> Option<Port> {
    let mut index = None;
    let mut device = None;
    let mut output = true;
    let mut volume = Volume::default();

    let mut parser = Parser::new(pod.data());
    parser
        .pop_object_raw(|object, _type, _id: u32| {
            for (key, _flags, value) in object {
                match Route::try_from(key) {
                    Ok(Route::Index) => index = value.decode::<i32>().ok(),
                    Ok(Route::Device) => device = value.decode::<i32>().ok(),
                    Ok(Route::Direction) => {
                        output = value
                            .decode::<Id<u32>>()
                            .map(|Id(direction)| direction == DIRECTION_OUTPUT)
                            .unwrap_or(true)
                    }
                    // The route's volume is a whole `Props` object nested
                    // inside this one.
                    Ok(Route::Props) if value.type_() == Type::Object => {
                        let nested = RawPodOwned::wrap(value.data().to_vec());
                        if let Ok(nested) = nested {
                            crate::services::audio::props::decode(&nested).apply(&mut volume);
                        }
                    }
                    _ => {}
                }
            }
            Ok(())
        })
        .ok()?;

    Some(Port {
        index: index?,
        device: device?,
        output,
        volume,
    })
}

/// Enough for a `Props` object holding channel volumes and a mute.
const SCRATCH: usize = 256;

pub fn set(device: &Device, port: Port, change: PropsUpdate) -> std::io::Result<()> {
    // `ObjectBuilder` can only push leaf values, so the inner `Props` object is
    // built into a scratch buffer first and pushed as an already-encoded pod —
    // `RawPodOwned` re-encodes verbatim.
    let mut scratch = [0u8; SCRATCH];
    let encoded = Builder::new(&mut scratch)
        .push_object(ObjectType::Props, ParamType::Props, |mut props| {
            if let Some(gain) = change.gain {
                props = props.push_property(
                    Prop::ChannelVolumes,
                    PropertyFlags::empty(),
                    vec![gain; super::props::CHANNELS],
                );
            }
            if let Some(muted) = change.muted {
                props = props.push_property(Prop::Mute, PropertyFlags::empty(), muted);
            }
            props
        })
        .build()
        .map_err(|e| std::io::Error::other(format!("could not build route props: {e:?}")))?;
    let props = RawPodOwned::wrap(encoded.to_vec())
        .map_err(|e| std::io::Error::other(format!("could not wrap route props: {e:?}")))?;

    device.set_param(
        ParamType::Route,
        ObjectType::ParamRoute,
        0,
        Box::new(move |builder: ObjectBuilder<'_>| {
            builder
                .push_property(Route::Index, PropertyFlags::empty(), port.index)
                .push_property(Route::Device, PropertyFlags::empty(), port.device)
                .push_property(Route::Props, PropertyFlags::empty(), props)
                // Persist it, so the level survives a replug the way every
                // other mixer's does.
                .push_property(Route::Save, PropertyFlags::empty(), true)
        }),
    )
}

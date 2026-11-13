//! A phone's live activity as a chip: its clock running here, its progress as
//! the ring, its buttons pressed on the phone.

use std::{convert::Infallible, sync::Arc};

use crate::{
    crownlink::{PhoneActivity, PhoneDevice},
    live::{Accent, Activity, Caption, Feed, Glyph, Progress, Urgency},
};

use super::on_device;

pub(super) fn activity(
    feed: &Feed<Infallible>,
    id: &Arc<str>,
    device: &PhoneDevice,
    ongoing: &PhoneActivity,
    several: bool,
) -> Activity {
    let (glyph, accent) = look(ongoing);
    let headline = match several {
        true => on_device(ongoing.headline(), &device.name),
        false => ongoing.headline().to_owned(),
    };
    let caption = ongoing
        .clock
        .map_or(Caption::None, |clock| match clock.counts_down {
            true => Caption::Until(clock.at),
            false => Caption::Since(clock.at),
        });
    let urgency = match ongoing.promoted {
        true => Urgency::Active,
        false => Urgency::Ambient,
    };
    let activity = Activity::new(feed.key(id.clone()), glyph, ongoing.app.clone())
        .headline(headline)
        .caption(caption)
        .accent(accent)
        .urgency(urgency)
        .progress(ongoing.fraction().map(Progress::Fixed));
    let activity = match ongoing.text.is_empty() {
        true => activity,
        false => activity.detail(ongoing.title.clone(), ongoing.text.clone()),
    };
    activity
        .detail("Device", device.name.clone())
        .actions(ongoing.actions.iter().map(|button| button.label.clone()))
}

/// The glyph and hue for an activity, by the category its app gave it and,
/// failing that, by its shape.
fn look(ongoing: &PhoneActivity) -> (Glyph, Accent) {
    match ongoing.category.as_deref() {
        Some("stopwatch") => (Glyph::Stopwatch, Accent::Amber),
        Some("alarm" | "reminder") => (Glyph::Timer, Accent::Amber),
        Some("progress") => (Glyph::Download, Accent::Blue),
        Some("transport") => (Glyph::Music, Accent::Coral),
        Some("navigation" | "location_sharing") => (Glyph::Phone, Accent::Blue),
        Some("workout") => (Glyph::Phone, Accent::Green),
        _ => match ongoing.clock {
            Some(clock) if clock.counts_down => (Glyph::Timer, Accent::Amber),
            Some(_) => (Glyph::Stopwatch, Accent::Amber),
            None if ongoing.progress.is_some() => (Glyph::Download, Accent::Blue),
            None => (Glyph::Phone, Accent::Neutral),
        },
    }
}

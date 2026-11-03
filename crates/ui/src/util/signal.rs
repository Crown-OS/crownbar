use crownui::prelude::{Runtime, Signal};

/// Write `value` only if it differs, so an unchanged restatement wakes none of
/// the signal's subscribers.
pub fn set_if_changed<T: PartialEq + 'static>(runtime: &mut Runtime, signal: Signal<T>, value: T) {
    if signal.with_untracked(runtime, |held| *held != value) {
        signal.set(runtime, value);
    }
}

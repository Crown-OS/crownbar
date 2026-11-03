//! A service snapshot as a widget holds it.

use std::{ops::Deref, sync::Arc};

/// The newest snapshot of a service, and whether the panel built from it has
/// seen it yet — so a wake from some other service costs the open panel no
/// rebuild.
pub(crate) struct Snapshot<T> {
    held: Arc<T>,
    panel_stale: bool,
}

impl<T: Default> Default for Snapshot<T> {
    fn default() -> Self {
        Self {
            held: Arc::default(),
            panel_stale: true,
        }
    }
}

impl<T> Snapshot<T> {
    /// Take `fresh`; `false` when it is the snapshot already held, which is
    /// what a service that has not published hands back.
    pub(crate) fn replace(&mut self, fresh: Arc<T>) -> bool {
        if Arc::ptr_eq(&fresh, &self.held) {
            return false;
        }
        self.held = fresh;
        self.panel_stale = true;
        true
    }

    /// The snapshot a panel is being built from, which it has now seen.
    pub(crate) fn for_panel(&mut self) -> Arc<T> {
        self.panel_stale = false;
        Arc::clone(&self.held)
    }

    pub(crate) fn panel_stale(&self) -> bool {
        self.panel_stale
    }
}

impl<T> Deref for Snapshot<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.held
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_new_snapshot_makes_the_panel_stale() {
        let mut snapshot = Snapshot::<u8>::default();
        let first = snapshot.for_panel();
        assert!(!snapshot.panel_stale());
        assert!(!snapshot.replace(first));
        assert!(!snapshot.panel_stale());
        assert!(snapshot.replace(Arc::new(7)));
        assert!(snapshot.panel_stale());
        assert_eq!(*snapshot, 7);
    }
}

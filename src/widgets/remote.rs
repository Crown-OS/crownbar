//! A plugin's widget: a pill and a panel drawn from trees crownplugind sends.
//!
//! The widget holds no state of its own beyond the newest surface. It is on
//! the bar only while the plugin has a pill to show, so a plugin that is
//! disabled, crashed, or whose daemon is down takes no space.

use std::sync::Arc;

use crownplugin_proto::{Part, RemoteSurface, SurfaceKey, Tree};

use crate::{
    services::{
        Services,
        plugins::{PluginsCommand, PluginsState},
    },
    widgets::{BarWidget, PopupSpec, popup::Row},
};

/// One plugin surface as a daemon session sent it.
#[derive(Clone, Debug, PartialEq)]
pub struct Surface {
    pub session: u64,
    pub remote: Arc<RemoteSurface>,
}

/// Which tree a view was built from. A view stays current exactly as long as
/// this does.
pub type TreeVersion = (u64, u32);

impl Surface {
    pub fn tree(&self, part: Part) -> Option<&Tree> {
        match part {
            Part::Main => self.remote.main.as_ref(),
            Part::Popup => self.remote.popup.as_ref(),
        }
    }

    pub fn version(&self, part: Part) -> Option<TreeVersion> {
        self.tree(part).map(|tree| (self.session, tree.rev))
    }

    pub fn key(&self, part: Part) -> SurfaceKey {
        self.remote.key(part)
    }
}

pub struct RemoteWidget {
    entry: String,
    plugin: String,
    widget: String,
    surface: Option<Surface>,
}

impl RemoteWidget {
    /// The widget listed as `entry`, which names `widget` of `plugin`.
    pub fn new(entry: &str, plugin: &str, widget: &str) -> Self {
        Self {
            entry: entry.to_owned(),
            plugin: plugin.to_owned(),
            widget: widget.to_owned(),
            surface: None,
        }
    }

    /// Take this widget's surface out of `state`. Returns whether the pill
    /// changed.
    pub fn adopt(&mut self, state: &PluginsState) -> bool {
        let next = state
            .surface(&self.plugin, &self.widget)
            .map(|remote| Surface {
                session: state.session,
                remote: Arc::clone(remote),
            });
        let pill = |surface: &Option<Surface>| {
            surface
                .as_ref()
                .and_then(|surface| surface.version(Part::Main))
        };
        let changed = pill(&self.surface) != pill(&next);
        self.surface = next;
        changed
    }

    fn popup_key(&self) -> SurfaceKey {
        SurfaceKey {
            plugin: self.plugin.clone(),
            widget: self.widget.clone(),
            part: Part::Popup,
        }
    }
}

impl BarWidget for RemoteWidget {
    fn id(&self) -> &str {
        &self.entry
    }

    fn sync(&mut self, services: &Services) -> bool {
        self.adopt(&services.plugins.read())
    }

    fn visible(&self) -> bool {
        self.surface
            .as_ref()
            .is_some_and(|surface| surface.remote.main.is_some())
    }

    fn plugin_surface(&self) -> Option<&Surface> {
        self.surface.as_ref()
    }

    fn popup(&mut self, _services: &Services) -> Option<PopupSpec> {
        let surface = self
            .surface
            .as_ref()
            .filter(|surface| surface.remote.popup.is_some())?;
        Some(PopupSpec {
            rows: vec![Row::Remote(surface.clone())],
        })
    }

    fn popup_opened(&mut self, services: &Services) {
        services.plugins.send(PluginsCommand::Popup {
            surface: self.popup_key(),
            open: true,
        });
    }

    fn popup_closed(&mut self, services: &Services) {
        services.plugins.send(PluginsCommand::Popup {
            surface: self.popup_key(),
            open: false,
        });
    }
}

#[cfg(test)]
mod tests {
    use crownplugin_proto::Node;

    use super::*;

    fn tree(rev: u32) -> Tree {
        Tree {
            rev,
            root: Node::Spacer,
        }
    }

    fn state(session: u64, main: Option<u32>, popup: Option<u32>) -> PluginsState {
        PluginsState {
            session,
            surfaces: vec![Arc::new(RemoteSurface {
                plugin: "crown.example.cpu-graph".into(),
                widget: "status".into(),
                main: main.map(tree),
                popup: popup.map(tree),
                lock_slot: None,
            })],
        }
    }

    fn widget() -> RemoteWidget {
        RemoteWidget::new(
            "plugin:crown.example.cpu-graph/status",
            "crown.example.cpu-graph",
            "status",
        )
    }

    #[test]
    fn it_shows_only_while_its_plugin_has_a_pill() {
        let mut widget = widget();
        assert!(!widget.visible());

        assert!(widget.adopt(&state(1, Some(1), None)));
        assert!(widget.visible());

        assert!(widget.adopt(&state(1, None, Some(1))));
        assert!(!widget.visible(), "a panel alone puts nothing on the bar");

        assert!(!widget.adopt(&PluginsState::default()));
        assert!(!widget.visible());
        assert_eq!(widget.id(), "plugin:crown.example.cpu-graph/status");
    }

    #[test]
    fn the_pill_changes_with_its_tree_and_not_with_the_panel() {
        let mut widget = widget();
        widget.adopt(&state(1, Some(1), Some(1)));
        assert!(!widget.adopt(&state(1, Some(1), Some(2))));
        assert!(widget.adopt(&state(1, Some(2), Some(2))));
        assert!(
            widget.adopt(&state(2, Some(2), Some(2))),
            "a restarted daemon's revision 2 is another tree"
        );
    }

    #[test]
    fn other_plugins_surfaces_are_not_its_own() {
        let mut widget = RemoteWidget::new("plugin:a/b", "a", "b");
        assert!(!widget.adopt(&state(1, Some(1), Some(1))));
        assert!(widget.plugin_surface().is_none());
    }
}

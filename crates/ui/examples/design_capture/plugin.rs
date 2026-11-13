//! The cpu-graph example plugin as crownplugind would send it: its reading for the pill, its card
//! for the panel.

use std::sync::Arc;

use anyhow::Result;
use crownbar_integrations::plugins::PluginsState;
use crownplugin_proto::{Node, RemoteSurface, Tree};
use crownplugin_view::parse_class;

const PLUGIN: &str = "crown.example.cpu-graph";
const WIDGET: &str = "status";
pub const ENTRY: &str = "plugin:crown.example.cpu-graph/status";
const CPU_LOAD: f32 = 0.41;

fn text(text: &str, class: &str) -> Result<Node> {
    Ok(Node::Text {
        style: parse_class(class)?,
        text: text.into(),
    })
}

fn pill() -> Result<Node> {
    Ok(Node::HStack {
        style: parse_class("gap-1 items-center")?,
        children: vec![
            Node::Icon {
                style: parse_class("text-bar.fg")?,
                name: "cpu".into(),
            },
            text("41%", "text-sm font-semibold text-bar.fg")?,
        ],
    })
}

fn card() -> Result<Node> {
    Ok(Node::Card {
        style: parse_class("p-3 gap-2 w-[220]")?,
        children: vec![
            text("CPU", "text-sm font-semibold text-text.primary")?,
            Node::Progress {
                style: Vec::new(),
                value: CPU_LOAD,
            },
            text("30 samples", "text-xs text-text.muted")?,
        ],
    })
}

pub fn cpu_graph() -> Result<PluginsState> {
    Ok(PluginsState {
        session: 1,
        surfaces: vec![Arc::new(RemoteSurface {
            plugin: PLUGIN.into(),
            widget: WIDGET.into(),
            main: Some(Tree {
                rev: 1,
                root: pill()?,
            }),
            popup: Some(Tree {
                rev: 1,
                root: card()?,
            }),
            lock_slot: None,
        })],
    })
}

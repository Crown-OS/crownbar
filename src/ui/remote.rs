//! Plugin trees on the bar, drawn by crownplugin-render in the bar's own ink.
//!
//! A tree is rendered once and never patched: the view showing it is keyed by
//! its revision, so a new tree is a new view and an unchanged one costs
//! nothing. What the user does to it goes back to crownplugind tagged with the
//! revision it was done to.

use std::rc::Rc;

use crownplugin_proto::{Part, SurfaceKey};
use crownplugin_render::{EventSink, Resolver, TokenSource, render};
use crownui::prelude::{AnyView, Cx, Memo, Size, Styled, View, hstack, vstack};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    services::plugins::PluginsCommand,
    ui::{pill::nothing, probe::size_probe, state::Bar},
    widgets::Surface,
};

/// Inset of a plugin's panel from the panel's sides, as a row's is.
const PANEL_PAD_X: f32 = 12.0;

/// What a plugin tree is rendered against: the bar's palette, and a pipe back
/// to the daemon.
struct BarHost {
    surface: SurfaceKey,
    rev: u32,
    commands: UnboundedSender<PluginsCommand>,
    palette: Memo<crate::theme::Palette>,
    /// Whether the pill is under the pointer or has its panel open; `None` in
    /// a panel.
    lit: Option<Memo<bool>>,
}

impl Resolver for BarHost {
    fn events(&self) -> Option<EventSink> {
        let (surface, rev, commands) = (self.surface.clone(), self.rev, self.commands.clone());
        Some(Rc::new(move |event| {
            let _ = commands.send(PluginsCommand::Event {
                surface: surface.clone(),
                rev,
                event,
            });
        }))
    }

    /// `bar.fg` brightens with the pill, the way a built-in pill's label does.
    fn tokens(&self) -> Option<TokenSource> {
        let (palette, lit) = (self.palette, self.lit);
        Some(Rc::new(move |runtime, token| {
            let lit = lit.is_some_and(|lit| lit.get(runtime));
            let palette = palette.get(runtime);
            match token {
                "bar.fg" if lit => Some(palette.bar_fg_hover),
                "bar.fg" => Some(palette.bar_fg),
                "bar.fg_hover" => Some(palette.bar_fg_hover),
                _ => None,
            }
        }))
    }
}

fn draw(
    cx: &mut Cx,
    bar: Bar,
    surface: &Surface,
    part: Part,
    lit: Option<Memo<bool>>,
) -> Option<AnyView> {
    let tree = surface.tree(part)?;
    let host = BarHost {
        surface: surface.key(part),
        rev: tree.rev,
        commands: bar.state.with(cx, |state| state.services.plugins.sender()),
        palette: bar.palette,
        lit,
    };
    Some(render(cx, tree, &host))
}

/// What a plugin's pill holds, lit with the pill.
pub fn pill(cx: &mut Cx, bar: Bar, surface: &Surface, lit: Memo<bool>) -> AnyView {
    match draw(cx, bar, surface, Part::Main, Some(lit)) {
        Some(content) => hstack(content).h_full().items_center().boxed(),
        None => nothing(),
    }
}

/// A plugin's panel. Its height is whatever the tree lays out to, which it
/// reports so the panel's shape can spring to it.
pub fn panel(cx: &mut Cx, bar: Bar, surface: &Surface) -> AnyView {
    let Some(content) = draw(cx, bar, surface, Part::Popup, None) else {
        return nothing();
    };
    let size = cx.signal(Size::ZERO);
    let measured = bar.popup.measured;
    cx.effect(move |runtime| {
        let height = size.get(runtime).height;
        measured.set(runtime, height);
    });
    vstack((size_probe(size).absolute().inset(0.0), content))
        .w_full()
        .px(PANEL_PAD_X)
        .boxed()
}

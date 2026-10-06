//! What every view on the bar shares.
//!
//! The widgets and the services behind them sit in one untracked store: they
//! are plain Rust objects, read and mutated in place. What makes a view notice
//! a change is a revision signal per widget, bumped only for the widgets that
//! actually moved — so a snapshot that changes the Wi-Fi pill re-runs the
//! Wi-Fi pill's bindings and nothing else.

use std::time::Duration;

use config::BarConfig;
use crownui::prelude::{Cx, Memo, Runtime, Signal, Size, Store};

use crate::{
    services::Services,
    theme::Palette,
    ui::popup::motion::{Frame, PanelMotion},
    widgets::{Arrangement, BarWidget, WidgetMask, WidgetRegistry, bit},
};

pub struct BarState {
    pub widgets: WidgetRegistry,
    /// What `widgets` was built from.
    pub arrangement: Arrangement,
    pub services: Services,
}

/// The popup panel's state, as signals the panel host binds to.
#[derive(Clone, Copy)]
pub struct Popup {
    /// The widget whose panel is up. `None` while it is going away, too.
    pub owner: Signal<Option<usize>>,
    /// The widget whose rows are on screen. Trails `owner` through a
    /// dismissal, which is how the closing panel keeps something to draw.
    pub shown: Signal<Option<usize>>,
    /// Centre of the pill the panel hangs from, in surface px.
    pub anchor_x: Signal<f32>,
    /// Bumped when the shown panel's contents went stale; never replays the
    /// open animation the way a change of `shown` does.
    pub content: Signal<u64>,
    /// Height of the shown panel's rows, which the shape springs toward.
    pub height: Signal<f32>,
    /// Height of the rows that size themselves — a plugin's panel — as laid
    /// out.
    pub measured: Signal<f32>,
    /// This frame of the open, morph and close animation.
    pub frame: Signal<Frame>,
    /// Whether anything of the panel is on screen.
    pub present: Signal<bool>,
    pub motion: Store<PanelMotion>,
}

/// Animation bookkeeping for the frame driver.
#[derive(Default)]
pub struct FrameClock {
    /// Widgets whose springs are in flight.
    pub active: WidgetMask,
    pub scheduled: bool,
    pub last: Option<Duration>,
}

/// The handle every view of the bar is built with. `Copy`, so closures take
/// it by value.
#[derive(Clone, Copy)]
pub struct Bar {
    pub state: Store<BarState>,
    revisions: Store<Vec<Signal<u64>>>,
    /// Bumped when the widgets were rearranged; the strip is rebuilt on it.
    pub layout: Signal<u64>,
    pub palette: Memo<Palette>,
    pub popup: Popup,
    pub clock: Store<FrameClock>,
    /// The surface's size, for keeping a panel on screen.
    pub surface: Signal<Size>,
    /// The bar's own height; panels hang from its lower edge.
    pub height: f32,
    /// Where the bar sits, which is where its panels open from.
    pub config: BarConfig,
}

impl Bar {
    pub fn new(cx: &mut Cx, state: BarState, palette: Memo<Palette>, config: BarConfig) -> Self {
        let revisions = (0..state.widgets.len()).map(|_| cx.signal(0_u64)).collect();
        let popup = Popup {
            owner: cx.signal(None),
            shown: cx.signal(None),
            anchor_x: cx.signal(0.0),
            content: cx.signal(0),
            height: cx.signal(0.0),
            measured: cx.signal(0.0),
            frame: cx.signal(Frame::default()),
            present: cx.signal(false),
            motion: cx.store(PanelMotion::new()),
        };
        Self {
            state: cx.store(state),
            revisions: cx.store(revisions),
            layout: cx.signal(0),
            palette,
            popup,
            clock: cx.store(FrameClock::default()),
            surface: cx.signal(Size::ZERO),
            height: config.bar_height as f32,
            config,
        }
    }

    /// Reads widget `index`, re-running the caller whenever that widget changes.
    /// A widget a rearrangement has since taken away reads as `R::default()`.
    pub fn read<R: Default>(
        self,
        runtime: &mut Runtime,
        index: usize,
        read: impl FnOnce(&dyn BarWidget) -> R,
    ) -> R {
        if let Some(revision) = self
            .revisions
            .with(runtime, |revisions| revisions.get(index).copied())
        {
            revision.get(runtime);
        }
        self.state.with(runtime, |state| {
            state.widgets.widget(index).map_or_else(R::default, read)
        })
    }

    /// Replaces the widgets with `widgets`, built from `arrangement`, and
    /// rebuilds the strip around them. The services stay as they are.
    pub fn rearrange(self, cx: &mut Cx, mut widgets: WidgetRegistry, arrangement: Arrangement) {
        let count = self.state.update(cx, |state| {
            widgets.sync(&state.services);
            state.widgets = widgets;
            state.arrangement = arrangement;
            state.widgets.len()
        });
        let missing = count.saturating_sub(self.revisions.with(cx, Vec::len));
        let added: Vec<_> = (0..missing).map(|_| cx.signal(0_u64)).collect();
        self.revisions
            .update(cx, |revisions| revisions.extend(added));
        self.layout
            .update(cx, |layout| *layout = layout.wrapping_add(1));
    }

    /// Tells the views of every widget in `changed` to look again.
    pub fn touch(self, runtime: &mut Runtime, changed: WidgetMask) {
        if changed == 0 {
            return;
        }
        let revisions = self.revisions.with(runtime, Clone::clone);
        for (index, revision) in revisions.into_iter().enumerate() {
            if changed & bit(index) != 0 {
                revision.update(runtime, |revision| *revision = revision.wrapping_add(1));
            }
        }
    }

    /// The shown panel's rows went stale.
    pub fn restate_panel(self, runtime: &mut Runtime) {
        if self.popup.shown.with_untracked(runtime, Option::is_some) {
            self.popup
                .content
                .update(runtime, |content| *content = content.wrapping_add(1));
        }
    }
}

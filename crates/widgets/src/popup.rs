//! The panel a widget shows when its pill is clicked, described as data.
//!
//! A widget never draws its own panel. It returns a [`PopupSpec`] — a flat
//! list of rows — and gets back a [`PopupAction`] naming the row the pointer
//! landed on. That keeps every panel visually identical without each widget
//! reimplementing a toggle, and it means a new widget gets a popup by
//! overriding two defaulted trait methods and nothing else.
//!
//! Rows are addressed by their index in [`PopupSpec::rows`], so a widget that
//! builds its spec from a list can map an action straight back onto it.

use std::hash::{DefaultHasher, Hash, Hasher};

use crownui::kit::CalendarDate;

use crate::{Icon, Rune, Surface};

/// A whole panel: an anchored, rounded card of rows. Every panel is
/// `crownbar_ui::popup::PANEL_WIDTH` wide, so only its rows vary.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PopupSpec {
    pub rows: Vec<Row>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    /// Panel title, with an optional button and master switch on its right.
    Header(Header),
    /// A continuous value with an icon at its left — the volume slider.
    Slider {
        icon: Icon,
        value: f32,
        /// `Some` makes the icon a mute button, dimmed while `Some(true)`.
        /// Pressing it activates the row.
        muted: Option<bool>,
    },
    /// Small muted heading above a group ("Output", "Known Networks").
    Section {
        title: String,
        /// Draws a disclosure chevron on the right, as macOS does for
        /// "Other Networks".
        chevron: bool,
    },
    /// A device or network: badge, label, and optional trailing detail.
    Item(Box<Item>),
    Separator,
    /// Footer row that hands off elsewhere ("Sound Settings…").
    Action {
        label: String,
        tone: Tone,
    },
    /// The clock's readout: the time, large, with the date beneath it.
    Readout {
        primary: String,
        secondary: String,
    },
    /// A month grid opened on `today`, which pages itself.
    Calendar(CalendarDate),
    /// A plugin's panel, drawn from the tree crownplugind sent. It sizes
    /// itself.
    Remote(Surface),
}

impl Row {
    pub fn action(label: impl Into<String>) -> Self {
        Self::Action {
            label: label.into(),
            tone: Tone::Plain,
        }
    }

    /// An action that ends something for good, set in the danger colour.
    pub fn destructive(label: impl Into<String>) -> Self {
        Self::Action {
            label: label.into(),
            tone: Tone::Danger,
        }
    }
}

/// How an action row reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Tone {
    #[default]
    Plain,
    /// It cannot be undone: a hang-up.
    Danger,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Header {
    pub title: String,
    /// The master switch, when the panel has one.
    pub toggle: Option<bool>,
    /// A round glyph button before the switch. Pressing it activates the
    /// header's row.
    pub button: Option<HeaderButton>,
}

/// A header's glyph button, e.g. "look again".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeaderButton {
    pub icon: Rune,
    /// Work the button started is still running: it shows a spinner and
    /// ignores presses until it is done.
    pub busy: bool,
}

impl Header {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Default::default()
        }
    }

    pub fn toggle(mut self, on: bool) -> Self {
        self.toggle = Some(on);
        self
    }

    pub fn button(mut self, icon: Rune, busy: bool) -> Self {
        self.button = Some(HeaderButton { icon, busy });
        self
    }

    pub fn row(self) -> Row {
        Row::Header(self)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Item {
    pub icon: Icon,
    /// Draw the icon inside a filled circle, the way macOS marks a device.
    /// Plain rows (a bare "Unsecured Network…" line) leave this off.
    pub badge: bool,
    pub label: String,
    /// Right-aligned detail before the battery/chevron ("70%").
    pub detail: Option<String>,
    /// Battery level ∈ [0, 1] drawn as a small cell after `detail`.
    pub battery: Option<f32>,
    /// Signal strength ∈ [0, 1] drawn as bars at the row's right edge.
    pub signal: Option<f32>,
    pub chevron: bool,
    /// The current output / the joined network: highlighted row, filled badge.
    pub selected: bool,
    /// Amber warning triangle on the right — an open network, say.
    pub warning: bool,
    /// A connect or disconnect is in flight: a spinner at the right edge.
    pub busy: bool,
    /// A switch at the row's right edge. Flipping it, or clicking the row,
    /// sends [`PopupAction::Toggle`] for the row.
    pub switch: Option<bool>,
    /// What the row stands for when that outlives its place in the panel —
    /// a device that keeps its row while the ones above it come and go.
    pub identity: Option<u64>,
    pub enabled: bool,
}

impl Item {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            badge: true,
            enabled: true,
            ..Default::default()
        }
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = icon;
        self
    }

    pub fn plain(mut self) -> Self {
        self.badge = false;
        self
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn battery(mut self, level: f32) -> Self {
        self.battery = Some(level.clamp(0.0, 1.0));
        self
    }

    pub fn signal(mut self, strength: f32) -> Self {
        self.signal = Some(strength.clamp(0.0, 1.0));
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn warning(mut self, warning: bool) -> Self {
        self.warning = warning;
        self
    }

    pub fn busy(mut self, busy: bool) -> Self {
        self.busy = busy;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn switch(mut self, on: bool) -> Self {
        self.switch = Some(on);
        self
    }

    /// Key the row by `identity` rather than by its place, which must then be
    /// unique within the panel.
    pub fn identity(mut self, identity: impl Hash) -> Self {
        let mut hasher = DefaultHasher::new();
        identity.hash(&mut hasher);
        self.identity = Some(hasher.finish());
        self
    }

    pub fn row(self) -> Row {
        Row::Item(Box::new(self))
    }
}

/// What the pointer did to a panel, addressed by row index.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PopupAction {
    /// The header's or an item's switch was flipped to `on`.
    Toggle { row: usize, on: bool },
    /// A slider was dragged to `value` ∈ [0, 1]. Fires continuously while the
    /// pointer is down, with `commit` set on release.
    Slide {
        row: usize,
        value: f32,
        commit: bool,
    },
    /// An item, section chevron or action row was clicked.
    Activate { row: usize },
}

/// Builds a [`PopupSpec`] while recording what each row *means* to the widget
/// that built it.
///
/// An incoming [`PopupAction`] names a row by index. Mapping that back by
/// arithmetic ("row 4 is the first sink") breaks the moment a separator moves,
/// so a widget pushes each row together with the target it stands for and
/// looks the target up when the action comes back.
pub struct PanelBuilder<T> {
    spec: PopupSpec,
    targets: Vec<Option<T>>,
}

impl<T> Default for PanelBuilder<T> {
    fn default() -> Self {
        Self {
            spec: PopupSpec::default(),
            targets: Vec::new(),
        }
    }
}

impl<T> PanelBuilder<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a row nothing can be done with — a header, a separator, a section
    /// heading.
    pub fn row(&mut self, row: Row) -> &mut Self {
        self.spec.rows.push(row);
        self.targets.push(None);
        self
    }

    /// Push a row that activates `target`.
    pub fn action(&mut self, row: Row, target: T) -> &mut Self {
        self.spec.rows.push(row);
        self.targets.push(Some(target));
        self
    }

    /// The finished spec, and the row → target table to keep beside it.
    pub fn finish(self) -> (PopupSpec, Vec<Option<T>>) {
        (self.spec, self.targets)
    }
}

/// How the panel should behave after an action was handled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AfterAction {
    /// Keep the panel up and redraw it — a toggle, a slider.
    #[default]
    Stay,
    /// Dismiss, the way picking an output device does.
    Close,
}

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

use crate::widgets::Icon;

/// A whole panel: an anchored, rounded card of rows. Every panel is
/// [`crate::ui::panel::WIDTH`] wide, so only its rows vary.
#[derive(Clone, Debug, Default)]
pub struct PopupSpec {
    pub rows: Vec<Row>,
}

#[derive(Clone, Debug)]
pub enum Row {
    /// Panel title, with the optional master switch on its right.
    Header { title: String, toggle: Option<bool> },
    /// A continuous value with an icon at its left — the volume slider.
    Slider { icon: Icon, value: f32 },
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
    Action { label: String },
    /// The clock's readout: the time, large, with the date beneath it.
    Readout { primary: String, secondary: String },
    /// A month grid. Boxed for the same reason [`Item`] is — it is an order of
    /// magnitude larger than every other variant, and a `Vec<Row>`'s slot is
    /// the size of its largest.
    Calendar(Box<Month>),
}

/// One month, laid out as the six weeks [`crate::util::calendar::grid`]
/// produces. The panel owns how it looks; this is only what it says.
#[derive(Clone, Debug, Default)]
pub struct Month {
    /// "September 2026".
    pub title: String,
    /// Six weeks of cells, Monday first.
    pub days: Vec<Day>,
}

/// One cell of the grid. `Copy` and free of allocation: the grid is rebuilt
/// every second the panel is open, and forty-two `String`s a second to print
/// the numbers 1 to 31 would be forty-two allocations too many.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Day {
    pub day: u8,
    /// False for the lead-in and lead-out days of the neighbouring months.
    pub in_month: bool,
    pub today: bool,
    pub weekend: bool,
}

impl Row {
    /// Whether the pointer can activate this row at all.
    pub fn is_interactive(&self) -> bool {
        match self {
            Row::Item(item) => item.enabled,
            Row::Action { .. } => true,
            Row::Section { chevron, .. } => *chevron,
            // A calendar is acted on through its arrows, which are hit-tested
            // separately — the grid itself is not a row target.
            Row::Header { .. }
            | Row::Slider { .. }
            | Row::Separator
            | Row::Readout { .. }
            | Row::Calendar(_) => false,
        }
    }
}

#[derive(Clone, Debug, Default)]
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
    pub chevron: bool,
    /// The current output / the joined network: highlighted row, filled badge.
    pub selected: bool,
    /// Amber warning triangle on the right — an open network, say.
    pub warning: bool,
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

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn warning(mut self, warning: bool) -> Self {
        self.warning = warning;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn row(self) -> Row {
        Row::Item(Box::new(self))
    }
}

/// What the pointer did to a panel, addressed by row index.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PopupAction {
    /// The header switch was flipped to `on`.
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
    /// A calendar's back or forward arrow was clicked. `months` is -1 or +1.
    Page { row: usize, months: i32 },
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

pub mod battery;
pub mod bluetooth;
pub mod brightness;
pub mod caffeine;
pub mod clock;
pub mod layout;
pub mod notifications;
pub mod popup;
pub mod remote;
pub mod volume;
pub mod weather;
pub mod wifi;

mod arrangement;

pub use arrangement::Arrangement;
pub use popup::{AfterAction, PopupAction, PopupSpec};
pub use remote::Surface;

use crate::services::Services;

/// The sky, as [`crate::services::weather`] reports it. Re-exported so the
/// icon layer takes its vocabulary from `widgets` like every other glyph's.
pub use crate::services::weather::Condition;

/// Which of `bar.ron`'s three lists a widget is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetSlot {
    Left,
    Center,
    Right,
}

/// Glyph the widget exposes to the renderer. Variants carry smoothly-
/// interpolated state floats (∈ [0, 1] typically) so the icon module can
/// crossfade / morph between two visual extremes without the widget caring
/// about the rendering math.
///
/// Widgets that toggle a state (battery saver, window layout, mute, …)
/// drive a spring against a 0↔1 target and pass the spring's live position
/// in here every frame; the result is a continuous, physical transition.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum Icon {
    #[default]
    None,
    Wifi(WifiState),
    Bluetooth {
        on: f32,
    },
    /// 0 = letting the machine sleep, 1 = holding it awake.
    Caffeine {
        on: f32,
    },
    Volume {
        level: f32,
        muted: f32,
    },
    Brightness {
        level: f32,
    },
    Battery(BatteryState),
    Layout {
        tiled: f32,
    },
    /// `open` is how far the notification centre is out, `silenced` how far
    /// Do Not Disturb is on. Both are spring positions.
    Notifications {
        open: f32,
        silenced: f32,
    },
    /// The sky, cross-fading. `blend` travels 0 → 1 as the weather changes
    /// from one condition to the next, and `night` 0 → 1 across dusk, so the
    /// pill never switches between two drawings.
    Weather {
        from: Condition,
        to: Condition,
        blend: f32,
        night: f32,
    },
    /// A fixed, SVG-authored glyph with no animated state. Panels are full of
    /// these — a headphone, a laptop, a chevron — and they would each need a
    /// variant of their own otherwise.
    Rune(Rune),
}

/// Static glyphs shared by the popup panels. Geometry lives in
/// [`crate::ui::icons`]; this is only the name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rune {
    Headphones,
    Speaker,
    Laptop,
    Display,
    Keyboard,
    Phone,
    Microphone,
    Sun,
    Moon,
    Wifi,
    Bluetooth,
    Warning,
    /// The three power profiles, in the order the panel lists them.
    Leaf,
    Gauge,
    Bolt,
}

/// Battery visual state. Everything but the readout is spring-smoothed, so a
/// profile change or a plug-in event *morphs* the cell — its fill travels to
/// the new colour and the accessory beside it grows out of the terminal —
/// rather than switching between two drawings.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BatteryState {
    /// How much of the cell is filled ∈ [0, 1].
    pub level: f32,
    /// Percentage printed inside the cell.
    pub readout: u8,
    /// 0 = on battery, 1 = plugged in.
    pub charging: f32,
    /// 0 = normal profile, 1 = saving power.
    pub saver: f32,
    /// 0 = healthy, 1 = about to go flat.
    pub low: f32,
}

/// Wi-Fi visual state. `off` / `searching` are spring-smoothed crossfades
/// between the icon's poses; `phase` is the position in the scanning sweep's
/// cycle, advanced only while the sweep is running.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WifiState {
    /// Link quality ∈ [0, 1].
    pub strength: f32,
    /// 0 = radio up, 1 = radio blocked.
    pub off: f32,
    /// 0 = associated, 1 = scanning.
    pub searching: f32,
    /// Sweep cycle position ∈ [0, 1).
    pub phase: f32,
}

/// Lean, extensible widget surface.
///
/// New trait methods always carry a default impl so existing widgets keep
/// compiling — adoption is opt-in per widget. The trait stays cheap to
/// implement: an inert label-only widget overrides nothing but `id` and
/// `label`. Where it sits is the layout's business, not the widget's.
pub trait BarWidget {
    /// Stable name — its entry in `bar.ron` — which is also the pill's
    /// accessible name.
    fn id(&self) -> &str;

    /// Take whatever the services now hold. Called after a service publishes
    /// and once per tick. Return `true` if the pill's appearance changed.
    fn sync(&mut self, services: &Services) -> bool {
        let _ = services;
        false
    }

    /// Refresh state that has no service behind it — the clock.
    fn update(&mut self) -> bool {
        false
    }

    /// Whether the pill belongs on the bar at all. A widget whose hardware is
    /// absent takes no space and cannot be hit; it starts drawing by itself if
    /// the hardware appears.
    fn visible(&self) -> bool {
        true
    }

    /// Primary label. Empty = no text in the pill.
    fn label(&self) -> &str {
        ""
    }

    /// Icon glyph to draw inside the pill.
    fn icon(&self) -> Icon {
        Icon::None
    }

    /// A plugin's surface, drawn from the trees crownplugind sent rather than
    /// from a label and a glyph.
    fn plugin_surface(&self) -> Option<&Surface> {
        None
    }

    /// Panel to show when the pill is clicked.
    ///
    /// `None` — the default — means the widget has no popup and a click goes
    /// to [`on_click`](Self::on_click) instead. Building the spec allocates,
    /// so it is only called when the panel is opened or has gone stale, never
    /// per frame.
    fn popup(&mut self, services: &Services) -> Option<PopupSpec> {
        let _ = services;
        None
    }

    /// The pointer acted on a row of this widget's panel. Return whether the
    /// panel should stay up or dismiss; the panel is rebuilt either way.
    fn on_popup(&mut self, action: PopupAction, services: &Services) -> AfterAction {
        let _ = (action, services);
        AfterAction::Stay
    }

    /// Called while this widget's panel is open. `slow` marks the once-a-
    /// second tick, which is when a widget should kick off a fresh reading;
    /// the fast calls are for collecting one that has landed. Return `true`
    /// if the panel's contents changed and it needs rebuilding.
    fn popup_poll(&mut self, slow: bool) -> bool {
        let _ = slow;
        false
    }

    /// Whether background work for the panel is still in flight. While this
    /// is `true` the popup surface stays on the frame clock, so a reading
    /// that lands mid-animation shows up immediately rather than at the next
    /// tick.
    fn popup_busy(&self) -> bool {
        false
    }

    /// The panel was opened.
    fn popup_opened(&mut self, services: &Services) {
        let _ = services;
    }

    /// The panel was dismissed. A widget that started a poll loop on open
    /// stops it here.
    fn popup_closed(&mut self, services: &Services) {
        let _ = services;
    }

    /// Pointer clicked the widget, and the widget has no popup. Return `true`
    /// if the click changed state (so the bar schedules a repaint /
    /// animation frame).
    fn on_click(&mut self, services: &Services) -> bool {
        let _ = services;
        false
    }

    /// Step any internal animation springs by `dt` seconds. Return `true` if
    /// any spring is still in flight (more frames needed).
    fn tick_animation(&mut self, dt: f32) -> bool {
        let _ = dt;
        false
    }
}

/// Every widget on the bar, in registration order — which is left-to-right
/// within each slot. Changes are reported as a bitmask over that order, so the
/// UI repaints only the pills that moved.
#[derive(Default)]
pub struct WidgetRegistry {
    widgets: Vec<Box<dyn BarWidget>>,
    slots: Vec<WidgetSlot>,
}

/// One bit per widget index.
pub type WidgetMask = u64;

/// How many widgets one bar holds: one bit of a [`WidgetMask`] each.
pub const MAX_WIDGETS: usize = WidgetMask::BITS as usize;

impl WidgetRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends `widget` to `slot`. A bar already holding [`MAX_WIDGETS`] drops
    /// it.
    pub fn register(&mut self, widget: Box<dyn BarWidget>, slot: WidgetSlot) {
        if self.widgets.len() == MAX_WIDGETS {
            log::warn!(
                "the bar holds {MAX_WIDGETS} widgets; `{}` left off",
                widget.id()
            );
            return;
        }
        self.widgets.push(widget);
        self.slots.push(slot);
    }

    pub fn len(&self) -> usize {
        self.widgets.len()
    }

    pub fn widget(&self, idx: usize) -> Option<&dyn BarWidget> {
        self.widgets.get(idx).map(AsRef::as_ref)
    }

    pub fn widget_mut(&mut self, idx: usize) -> Option<&mut (dyn BarWidget + 'static)> {
        self.widgets.get_mut(idx).map(AsMut::as_mut)
    }

    /// Every widget with the slot it was placed in, in registration order.
    pub fn placed(&self) -> impl Iterator<Item = (WidgetSlot, &dyn BarWidget)> {
        self.slots
            .iter()
            .copied()
            .zip(self.widgets.iter().map(AsRef::as_ref))
    }

    /// Tick every widget's clock; returns the ones whose pill changed.
    pub fn tick(&mut self) -> WidgetMask {
        mask(self.widgets.iter_mut().map(|widget| widget.update()))
    }

    /// Hand every widget the newest snapshots. Returns the ones whose pill
    /// changed appearance — a snapshot that only moves what a panel shows costs
    /// no repaint of the bar.
    pub fn sync(&mut self, services: &Services) -> WidgetMask {
        mask(self.widgets.iter_mut().map(|widget| {
            let was_visible = widget.visible();
            widget.sync(services) | (widget.visible() != was_visible)
        }))
    }

    /// Step the springs of the widgets in `active`; returns the ones still in
    /// flight.
    pub fn step_animations(&mut self, active: WidgetMask, dt: f32) -> WidgetMask {
        mask(
            self.widgets
                .iter_mut()
                .enumerate()
                .map(|(index, widget)| active & bit(index) != 0 && widget.tick_animation(dt)),
        )
    }

    /// Forward a click to the widget at `idx`. Returns `true` if the widget
    /// reports a state change.
    pub fn click(&mut self, idx: usize, services: &Services) -> bool {
        self.widgets
            .get_mut(idx)
            .is_some_and(|widget| widget.on_click(services))
    }

    /// Build the panel for the widget at `idx`, if it has one.
    pub fn popup(&mut self, idx: usize, services: &Services) -> Option<PopupSpec> {
        self.widgets.get_mut(idx)?.popup(services)
    }

    /// Whether clicking the widget at `idx` opens a panel rather than acting
    /// on the widget directly. Answered by building the panel and dropping it,
    /// which keeps [`BarWidget::popup`] the single source of truth.
    pub fn has_popup(&mut self, idx: usize, services: &Services) -> bool {
        self.popup(idx, services).is_some()
    }
}

pub const fn bit(index: usize) -> WidgetMask {
    1 << index
}

fn mask(changed: impl Iterator<Item = bool>) -> WidgetMask {
    changed
        .enumerate()
        .filter(|(_, changed)| *changed)
        .fold(0, |mask, (index, _)| mask | bit(index))
}

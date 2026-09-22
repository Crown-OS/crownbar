pub mod battery;
pub mod bluetooth;
pub mod brightness;
pub mod caffeine;
pub mod clock;
pub mod layout;
pub mod notifications;
pub mod popup;
pub mod volume;
pub mod weather;
pub mod wifi;

pub use popup::{AfterAction, PopupAction, PopupSpec};

use crate::{animation::Spring, services::Services};

/// The sky, as [`crate::services::weather`] reports it. Re-exported so the
/// icon layer takes its vocabulary from `widgets` like every other glyph's.
pub use crate::services::weather::Condition;

/// Where a widget anchors itself on the bar.
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
#[derive(Debug, Clone, Copy, Default)]
pub enum Icon {
    #[default]
    None,
    Wifi(WifiState),
    Bluetooth { on: f32 },
    /// 0 = letting the machine sleep, 1 = holding it awake.
    Caffeine { on: f32 },
    Volume { level: f32, muted: f32 },
    Brightness { level: f32 },
    Battery(BatteryState),
    Layout { tiled: f32 },
    /// `open` is how far the notification centre is out, `silenced` how far
    /// Do Not Disturb is on. Both are spring positions.
    Notifications { open: f32, silenced: f32 },
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
    ChevronLeft,
    ChevronRight,
    /// The three power profiles, in the order the panel lists them.
    Leaf,
    Gauge,
    Bolt,
}

/// Battery visual state. Everything but the readout is spring-smoothed, so a
/// profile change or a plug-in event *morphs* the cell — its fill travels to
/// the new colour and the accessory beside it grows out of the terminal —
/// rather than switching between two drawings.
#[derive(Debug, Clone, Copy, Default)]
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
#[derive(Debug, Clone, Copy, Default)]
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
/// implement: an inert label-only widget overrides nothing but `id`, `slot`
/// and `label`.
pub trait BarWidget {
    #[allow(dead_code)]
    fn id(&self) -> &'static str;
    fn slot(&self) -> WidgetSlot;

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

/// Per-widget runtime state owned by the registry — animation springs, last
/// computed bounds for hit-testing. Kept separate from the widget impl so
/// implementors don't have to thread animation state through their own types.
pub struct WidgetRuntime {
    pub widget: Box<dyn BarWidget>,
    /// 0 = idle, 1 = fully hovered. Driven by a spring.
    pub hover: Spring,
    /// Last laid-out bounds (x, y, w, h) in surface px. Used for hit-testing.
    pub bounds: Option<(f32, f32, f32, f32)>,
}

impl WidgetRuntime {
    pub fn new(widget: Box<dyn BarWidget>) -> Self {
        Self {
            widget,
            hover: Spring::new(0.0),
            bounds: None,
        }
    }
}

pub struct WidgetRegistry {
    pub widgets: Vec<WidgetRuntime>,
    /// Index of the widget the pointer is currently over, if any.
    pub hovered: Option<usize>,
}

impl WidgetRegistry {
    pub fn new() -> Self {
        Self {
            widgets: Vec::new(),
            hovered: None,
        }
    }

    /// Centre of a widget's pill on the bar, for anchoring its popup under it.
    pub fn anchor_x(&self, idx: usize) -> Option<f32> {
        let (x, _, w, _) = self.widgets.get(idx)?.bounds?;
        Some(x + w * 0.5)
    }

    pub fn register(&mut self, widget: Box<dyn BarWidget>) {
        self.widgets.push(WidgetRuntime::new(widget));
    }

    /// Tick every widget; return whether any of their labels changed.
    pub fn tick(&mut self) -> bool {
        let mut dirty = false;
        for rt in self.widgets.iter_mut() {
            if rt.widget.update() {
                dirty = true;
            }
        }
        dirty
    }

    /// Hand every widget the newest snapshots. Returns whether any pill
    /// changed appearance — a snapshot that only moves what a panel shows
    /// costs no repaint of the bar.
    pub fn sync(&mut self, services: &Services) -> bool {
        let mut dirty = false;
        for rt in self.widgets.iter_mut() {
            let was_visible = rt.widget.visible();
            dirty |= rt.widget.sync(services) | (rt.widget.visible() != was_visible);
        }
        dirty
    }

    /// Step every per-widget hover spring AND each widget's internal springs.
    /// Returns whether any animation is still in flight (i.e. another frame
    /// is needed).
    pub fn step_animations(&mut self, dt: f32) -> bool {
        let mut in_flight = false;
        for rt in self.widgets.iter_mut() {
            if !rt.hover.at_rest() {
                rt.hover.step(dt);
                if !rt.hover.at_rest() {
                    in_flight = true;
                }
            }
            if rt.widget.tick_animation(dt) {
                in_flight = true;
            }
        }
        in_flight
    }

    /// Hit-test against the most recently laid-out widget bounds.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<usize> {
        self.widgets.iter().position(|rt| {
            rt.bounds
                .map(|(bx, by, bw, bh)| x >= bx && x <= bx + bw && y >= by && y <= by + bh)
                .unwrap_or(false)
        })
    }

    /// Update hover state — returns `true` if the hovered widget changed and
    /// a repaint should be scheduled.
    pub fn set_hovered(&mut self, idx: Option<usize>) -> bool {
        if self.hovered == idx {
            return false;
        }
        if let Some(prev) = self.hovered
            && let Some(rt) = self.widgets.get_mut(prev)
        {
            rt.hover.set_target(0.0);
        }
        if let Some(new) = idx
            && let Some(rt) = self.widgets.get_mut(new)
        {
            rt.hover.set_target(1.0);
        }
        self.hovered = idx;
        true
    }

    pub fn clear_hover(&mut self) -> bool {
        self.set_hovered(None)
    }

    /// Forward a click to the widget at `idx`. Returns `true` if the widget
    /// reports state change (so the bar should request a frame).
    pub fn click(&mut self, idx: usize, services: &Services) -> bool {
        match self.widgets.get_mut(idx) {
            Some(rt) => rt.widget.on_click(services),
            None => false,
        }
    }

    /// Build the panel for the widget at `idx`, if it has one.
    pub fn popup(&mut self, idx: usize, services: &Services) -> Option<PopupSpec> {
        self.widgets.get_mut(idx)?.widget.popup(services)
    }

    /// Whether clicking the widget at `idx` opens a panel rather than acting
    /// on the widget directly. Answered by building the panel and dropping it,
    /// which happens once per click and keeps [`BarWidget::popup`] the single
    /// source of truth.
    pub fn has_popup(&mut self, idx: usize, services: &Services) -> bool {
        self.popup(idx, services).is_some()
    }
}

impl Default for WidgetRegistry {
    fn default() -> Self {
        Self::new()
    }
}

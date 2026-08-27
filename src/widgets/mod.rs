pub mod battery;
pub mod bluetooth;
pub mod brightness;
pub mod clock;
pub mod layout;
pub mod volume;
pub mod wifi;

use crate::animation::Spring;

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
#[derive(Debug, Clone, Copy)]
pub enum Icon {
    None,
    Wifi { strength: f32 },
    Bluetooth { on: f32 },
    Volume { level: f32, muted: f32 },
    Brightness { level: f32 },
    Battery { pct: f32, charging: f32, saver: f32 },
    Layout { tiled: f32 },
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

    /// Refresh internal state. Return `true` if the rendered label changed.
    fn update(&mut self) -> bool {
        false
    }

    /// Primary label. Empty = no text in the pill.
    fn label(&self) -> String {
        String::new()
    }

    /// Icon glyph to draw inside the pill.
    fn icon(&self) -> Icon {
        Icon::None
    }

    /// Pointer clicked the widget. Return `true` if the click changed state
    /// (so the bar schedules a repaint / animation frame).
    fn on_click(&mut self) -> bool {
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
        if let Some(prev) = self.hovered {
            if let Some(rt) = self.widgets.get_mut(prev) {
                rt.hover.set_target(0.0);
            }
        }
        if let Some(new) = idx {
            if let Some(rt) = self.widgets.get_mut(new) {
                rt.hover.set_target(1.0);
            }
        }
        self.hovered = idx;
        true
    }

    pub fn clear_hover(&mut self) -> bool {
        self.set_hovered(None)
    }

    /// Forward a click to the widget at `idx`. Returns `true` if the widget
    /// reports state change (so the bar should request a frame).
    pub fn click(&mut self, idx: usize) -> bool {
        match self.widgets.get_mut(idx) {
            Some(rt) => rt.widget.on_click(),
            None => false,
        }
    }
}

impl Default for WidgetRegistry {
    fn default() -> Self {
        Self::new()
    }
}

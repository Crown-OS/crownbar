//! Window-layout toggle. Flips between "floating" (windows can overlap and
//! be repositioned freely) and "tiled" (windows snap to a grid). The actual
//! compositor switch is out of scope for the bar — for now the widget just
//! owns the local UI state and surfaces it as a smoothly-animated icon so
//! the visual treatment is in place when the compositor protocol lands.

use crate::{
    animation::Spring,
    widgets::{BarWidget, Icon, WidgetSlot},
};

pub struct LayoutWidget {
    tiled: bool,
    morph: Spring,
}

impl LayoutWidget {
    pub fn new(initial_tiled: bool) -> Self {
        let value = if initial_tiled { 1.0 } else { 0.0 };
        Self {
            tiled: initial_tiled,
            morph: Spring::new(value),
        }
    }
}

impl Default for LayoutWidget {
    fn default() -> Self {
        Self::new(false)
    }
}

impl BarWidget for LayoutWidget {
    fn id(&self) -> &'static str {
        "layout"
    }

    fn slot(&self) -> WidgetSlot {
        WidgetSlot::Right
    }

    fn icon(&self) -> Icon {
        Icon::Layout {
            tiled: self.morph.position,
        }
    }

    fn on_click(&mut self) -> bool {
        self.tiled = !self.tiled;
        self.morph.set_target(if self.tiled { 1.0 } else { 0.0 });
        true
    }

    fn tick_animation(&mut self, dt: f32) -> bool {
        if self.morph.at_rest() {
            return false;
        }
        self.morph.step(dt);
        !self.morph.at_rest()
    }
}

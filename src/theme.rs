//! Visual design tokens for the bar. Centralized so widgets and the bar
//! renderer share the same palette/spacing without each module redefining it.

use vello::peniko::Color;

#[allow(dead_code)]
pub struct Theme {
    /// Tint applied behind the entire bar. Kept very dark + low alpha so the
    /// blurred backdrop dominates.
    pub bar_tint: Color,
    /// Pill fill color when a widget is idle (very faint).
    pub pill_idle: Color,
    /// Pill fill color when a widget is hovered (more visible).
    pub pill_hover: Color,
    /// Hairline rim around a hovered pill.
    pub pill_rim: Color,
    /// Foreground color for icons/text.
    pub fg: Color,
    /// Muted foreground (secondary text).
    pub fg_muted: Color,
    /// Vertical inset of the bar contents from the top/bottom edges.
    pub bar_pad_y: f32,
    /// Horizontal inset of the bar contents from the left/right edges.
    pub bar_pad_x: f32,
    /// Inner padding of a widget pill (left/right).
    pub pill_pad_x: f32,
    /// Vertical inset of the pill within the bar.
    pub pill_pad_y: f32,
    /// Gap between adjacent widgets.
    pub widget_gap: f32,
    /// Font size for widget text.
    pub font_size: f32,
}

impl Theme {
    pub const fn default() -> Self {
        Self {
            bar_tint: Color::from_rgba8(0, 0, 0, 90),
            pill_idle: Color::from_rgba8(255, 255, 255, 0),
            pill_hover: Color::from_rgba8(255, 255, 255, 28),
            pill_rim: Color::from_rgba8(255, 255, 255, 38),
            fg: Color::from_rgba8(245, 245, 247, 255),
            fg_muted: Color::from_rgba8(210, 210, 215, 255),
            bar_pad_y: 0.0,
            bar_pad_x: 12.0,
            pill_pad_x: 12.0,
            pill_pad_y: 6.0,
            widget_gap: 6.0,
            font_size: 14.0,
        }
    }
}

pub const THEME: Theme = Theme::default();

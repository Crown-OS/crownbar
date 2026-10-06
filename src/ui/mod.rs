//! The bar as crownui views.
//!
//! One layer surface holds everything: the bar strip along the top (or
//! bottom) edge and, beside it, the area a panel opens into. The root takes no
//! pointer input of its own, so the surface's input region is the bar — and,
//! while a panel is up, the whole area beside it, which is what makes a click
//! outside a panel a dismissal rather than a click on the window underneath.

mod backdrop;
mod bar;
mod frames;
mod icons;
mod pill;
mod popup;
mod probe;
mod remote;
mod scene;
mod state;
mod sync;

use config::{BarConfig, BarSettings};
use crownconfig::Appearance;
use crownui::{
    kit::{INTER_FAMILY, provide_configured_theme},
    prelude::{Styled, View, component, zstack},
};

use crate::{
    services::{Services, Wake},
    theme::Palette,
    widgets::{Arrangement, WidgetRegistry},
};
use state::{Bar, BarState};

/// How far the bar's reserved space stops short of its own lower edge. Panels
/// open into the area below that line, hanging this far below it — flush with
/// the bar's edge.
pub const PANEL_GAP: f32 = 4.0;

pub fn root(
    services: Services,
    wake: Wake,
    appearance: Appearance,
    settings: BarSettings,
) -> impl View {
    component(move |cx| {
        let theme = provide_configured_theme(cx);
        let transparency = sync::transparency(cx, &appearance);
        let overrides = sync::theme_overrides(cx);
        let palette = cx.memo(move |runtime| {
            let transparency = transparency.get(runtime);
            let overrides = overrides.get(runtime);
            theme.with(runtime, |theme| {
                Palette::new(theme, transparency, &overrides)
            })
        });
        let config = BarConfig::new(&settings, appearance.bar_height);
        let arrangement = Arrangement::from(&settings);
        let state = BarState {
            widgets: WidgetRegistry::arranged(&arrangement),
            arrangement,
            services,
        };
        let bar = Bar::new(cx, state, palette, config);
        sync::start(cx, bar, wake);
        sync::bar_settings(cx, bar, settings, appearance.bar_height);
        zstack((
            probe::size_probe(bar.surface).absolute().inset(0.0),
            bar::bar_strip(bar),
            popup::panel_host(bar),
        ))
        .font_family(INTER_FAMILY)
        .pointer_events_none()
    })
}

//! The bar, driven headlessly: pills, the panel they open, and what the
//! surface lets through to the windows below.
//!
//! Its own test binary because it starts the real services, which describe
//! whatever machine runs it; nothing here depends on what they find.

use crownbar::{
    BarPosition, BarSettings,
    services::{Services, Wake},
};
use crownconfig::Appearance;
use crowntest::{Role, TestApp, role};

const SURFACE: (f32, f32) = (1440.0, 900.0);

fn bar() -> (TestApp, f32) {
    bar_with(BarSettings::default())
}

fn bar_with(settings: BarSettings) -> (TestApp, f32) {
    let appearance = Appearance::default();
    let height = settings.height_or(appearance.bar_height) as f32;
    let wake = Wake::default();
    let services = Services::start(wake.clone()).expect("services start");
    let mut app = TestApp::with_size(
        crownbar::view(services, wake, appearance, settings),
        SURFACE.0,
        SURFACE.1,
    );
    app.settle();
    (app, height)
}

fn input_region(app: &mut TestApp) -> Vec<crownui::prelude::Rect> {
    app.inline()
        .last_surface()
        .and_then(|surface| surface.input_region.clone())
        .unwrap_or_default()
}

#[test]
fn the_clock_opens_its_panel_and_a_click_outside_puts_it_away() {
    let (mut app, height) = bar();
    assert!(app.exists(role(Role::Button).name("clock")));

    let closed = input_region(&mut app);
    assert!(
        closed.iter().all(|rect| rect.max_y() <= height),
        "with no panel up only the bar takes the pointer: {closed:?}"
    );

    app.click(role(Role::Button).name("clock"));
    app.settle();
    assert!(
        app.exists(role(Role::Group).name("Calendar")),
        "{}",
        app.snapshot()
    );
    assert!(app.exists(role(Role::Button).name("Date & Time Settings…")));
    let open = input_region(&mut app);
    assert!(
        open.iter().any(|rect| rect.max_y() >= SURFACE.1),
        "an open panel takes the whole area below the bar: {open:?}"
    );

    app.pointer_down(SURFACE.0 - 20.0, SURFACE.1 - 20.0)
        .pointer_up(SURFACE.0 - 20.0, SURFACE.1 - 20.0);
    app.settle();
    assert!(!app.exists(role(Role::Group).name("Calendar")));
    assert_eq!(input_region(&mut app), closed);
}

#[test]
fn a_second_click_on_the_pill_puts_its_panel_away() {
    let (mut app, _) = bar();
    app.click(role(Role::Button).name("clock"));
    app.settle();
    assert!(app.exists(role(Role::Group).name("Calendar")));
    app.click(role(Role::Button).name("clock"));
    app.settle();
    assert!(!app.exists(role(Role::Group).name("Calendar")));
}

#[test]
fn the_panel_hangs_from_the_bar_under_its_pill() {
    let (mut app, height) = bar();
    let pill = app.find(role(Role::Button).name("clock")).bounds;
    app.click(role(Role::Button).name("clock"));
    app.settle();
    let calendar = app.find(role(Role::Group).name("Calendar")).bounds;
    assert!(
        (calendar.min_y() - height).abs() < 120.0,
        "panel starts at {}",
        calendar.min_y()
    );
    let pill_centre = pill.min_x() + pill.size.width / 2.0;
    let panel_centre = calendar.min_x() + calendar.size.width / 2.0;
    assert!(
        (pill_centre - panel_centre).abs() < 1.0 || calendar.min_x() <= 8.5,
        "panel centred at {panel_centre}, pill at {pill_centre}"
    );
}

#[test]
fn a_floating_bar_at_the_bottom_opens_its_panels_upward() {
    const MARGIN: f32 = 8.0;
    let (mut app, height) = bar_with(BarSettings {
        position: BarPosition::Bottom,
        floating: true,
        margin: MARGIN as u16,
        ..BarSettings::default()
    });
    let pill = app.find(role(Role::Button).name("clock")).bounds;
    assert!(
        (pill.max_y() - (SURFACE.1 - MARGIN)).abs() < 0.5 && (pill.min_x() - MARGIN) < 16.0,
        "the pill sits inside the floating bar: {pill:?}"
    );
    let closed = input_region(&mut app);
    assert!(
        closed
            .iter()
            .all(|rect| rect.min_y() >= SURFACE.1 - MARGIN - height),
        "with no panel up only the bar takes the pointer: {closed:?}"
    );

    app.click(role(Role::Button).name("clock"));
    app.settle();
    let calendar = app.find(role(Role::Group).name("Calendar")).bounds;
    assert!(
        calendar.max_y() < pill.min_y(),
        "the panel opens above the bar: {calendar:?}"
    );
    app.pointer_down(20.0, 20.0).pointer_up(20.0, 20.0);
    app.settle();
    assert!(!app.exists(role(Role::Group).name("Calendar")));
}

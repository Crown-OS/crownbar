//! The bar, driven headlessly: pills, the panel they open, and what the
//! surface lets through to the windows below.
//!
//! Its own test binary because it starts the real services, which describe
//! whatever machine runs it; nothing here depends on what they find.

use crownbar_config::{BarPosition, BarSettings};
use crownbar_integrations::{Services, Wake};
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
        crownbar_ui::root(services, wake.subscribe(), appearance, settings, None),
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

/// The turn of every tilted layer in `frame`: the panel while it comes out
/// from behind its pill.
fn tilts(frame: &str) -> Vec<f32> {
    frame
        .lines()
        .filter_map(|line| {
            line.split_once(" tilt ")?
                .1
                .split("deg")
                .next()?
                .parse()
                .ok()
        })
        .collect()
}

fn opening_tilts(app: &mut TestApp) -> Vec<Vec<f32>> {
    let pill = app.find(role(Role::Button).name("clock")).bounds.center();
    app.pointer_down(pill.x, pill.y).pointer_up(pill.x, pill.y);
    let tilts = (0..4)
        .map(|_| tilts(&app.advance_ms(16).frame_snapshot()))
        .collect();
    app.settle();
    tilts
}

#[test]
fn the_first_panel_comes_out_of_its_pill_like_every_later_one() {
    let (mut app, _) = bar();
    let first = opening_tilts(&mut app);
    app.pointer_down(SURFACE.0 - 20.0, SURFACE.1 - 20.0)
        .pointer_up(SURFACE.0 - 20.0, SURFACE.1 - 20.0);
    app.settle();
    let second = opening_tilts(&mut app);
    assert!(
        first.iter().flatten().any(|pitch| *pitch > 10.0),
        "the panel did not come out tilted back: {first:?}"
    );
    assert_eq!(first, second);
}

/// Layers drawn as a stretched image: their text is soft.
fn stretched(frame: &str) -> Vec<&str> {
    frame
        .lines()
        .filter(|line| line.contains(" as image"))
        .collect()
}

#[test]
fn a_panel_at_rest_is_flat_and_sharp() {
    let (mut app, _) = bar();
    app.click(role(Role::Button).name("clock"));
    let moving = app.advance_ms(200).frame_snapshot();
    assert!(
        !stretched(&moving).is_empty(),
        "the rows do not stretch as an image while the panel grows: {moving}"
    );
    app.settle();
    let frame = app.frame_snapshot();
    assert!(tilts(&frame).is_empty(), "{frame}");
    assert!(stretched(&frame).is_empty(), "{frame}");

    // Only a machine with Wi-Fi has a second pill to switch to.
    if app.exists(role(Role::Button).name("wifi")) {
        app.click(role(Role::Button).name("wifi"));
        app.settle();
        let switched = app.frame_snapshot();
        assert!(stretched(&switched).is_empty(), "{switched}");
    }
}

fn requested_height(app: &mut TestApp) -> Option<crownui::prelude::LayerExtent> {
    app.ui().layer_height_request()
}

#[test]
fn the_surface_is_ready_for_a_panel_while_the_pointer_is_on_the_bar() {
    use crownui::prelude::LayerExtent;
    let (mut app, height) = bar();
    let pill = app.find(role(Role::Button).name("clock")).bounds.center();
    app.pointer_move(4.0, height / 2.0);
    app.settle();
    assert_eq!(requested_height(&mut app), Some(LayerExtent::Output));

    app.pointer_move(pill.x, pill.y);
    app.settle();
    assert_eq!(
        requested_height(&mut app),
        Some(LayerExtent::Output),
        "moving onto a pill is still on the bar"
    );

    app.pointer_leave(pill.x, pill.y);
    app.settle();
    assert!(
        matches!(requested_height(&mut app), Some(LayerExtent::Fixed(_))),
        "with no panel up the surface goes back to the strip"
    );
}

#[test]
fn a_panel_put_away_with_the_pointer_gone_gives_the_room_back() {
    use crownui::prelude::LayerExtent;
    let (mut app, _) = bar();
    app.click(role(Role::Button).name("clock"));
    app.settle();
    app.pointer_down(SURFACE.0 - 20.0, SURFACE.1 - 20.0)
        .pointer_up(SURFACE.0 - 20.0, SURFACE.1 - 20.0)
        .pointer_leave(SURFACE.0 - 20.0, SURFACE.1 - 20.0);
    app.settle();
    assert!(!app.exists(role(Role::Group).name("Calendar")));
    assert!(matches!(
        requested_height(&mut app),
        Some(LayerExtent::Fixed(_))
    ));
}

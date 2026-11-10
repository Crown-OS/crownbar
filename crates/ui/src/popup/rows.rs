//! A widget's [`PopupSpec`] as rows on screen.
//!
//! Rows are keyed by their place and their shape — kind, label, which
//! furniture they carry — so a panel restated once a second keeps every row
//! that is still the same row: a switch keeps its spring, a slider stays under
//! the pointer, a calendar stays on the month it was paged to. Only what a row
//! *says* flows through its signal.

use std::hash::{DefaultHasher, Hash, Hasher};

use crownui::{
    kit::{
        PanelHeaderKind, PanelHeaderOptions, PanelItemKind, PanelItemOptions, PanelSectionOptions,
        PanelSliderOptions, Theme, calendar, panel_action, panel_header, panel_item, panel_readout,
        panel_section, panel_separator, panel_slider, spinner, toggle, use_theme,
    },
    prelude::{
        AnyView, Color, CursorIcon, Cx, Element, Listeners, Memo, Role, Semantic, Signal, Styled,
        TRANSPARENT, View, bind, canvas, hstack, show,
    },
};
use kurbo::Rect;

use crownplugin_proto::Part;

use super::{dispatch, optical::Seat};
use crate::{icons, remote, scene::Scene, state::Bar, util::theme::provide_derived_theme};
use crownbar_widgets::{
    BatteryState, Icon, PopupAction, PopupSpec, Rune,
    popup::{Header, Item, Row, Tone},
};

/// Space above the first row and below the last, the same as the kit's inset
/// of a row highlight from the panel's sides.
pub const PAD_Y: f32 = ROW_INSET;
pub const ROW_INSET: f32 = 8.0;
/// Space between rows, so neighbouring highlights never touch.
pub const ROW_GAP: f32 = 4.0;
pub const ITEM_HEIGHT: f32 = 40.0;
const ACTION_HEIGHT: f32 = 32.0;
const SECTION_HEIGHT: f32 = 28.0;
const BADGE_GLYPH: f32 = 18.0;
const PLAIN_GLYPH: f32 = 18.0;
const BATTERY_CELL: (f32, f32) = (24.0, 12.0);
const WARNING_GLYPH: f32 = 15.0;
const SIGNAL_METER: (f32, f32) = (16.0, 12.0);
const SPINNER: f32 = 16.0;
const HEADER_BUTTON: f32 = 28.0;
const HEADER_BUTTON_GLYPH: f32 = 16.0;

/// One row of the shown panel, with the key that decides whether it is still
/// the row it was.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelRow {
    pub index: usize,
    pub key: u64,
    pub row: Row,
}

pub fn keyed(spec: PopupSpec) -> Vec<PanelRow> {
    spec.rows
        .into_iter()
        .enumerate()
        .map(|(index, row)| PanelRow {
            index,
            key: shape_key(index, &row),
            row,
        })
        .collect()
}

/// What the rows add up to, less any that size themselves; the shape springs
/// toward it.
pub fn height(spec: &PopupSpec) -> f32 {
    let gaps = spec.rows.len().saturating_sub(1) as f32 * ROW_GAP;
    PAD_Y * 2.0 + gaps + spec.rows.iter().map(row_height).sum::<f32>()
}

/// Whether some row's height is only known once it is laid out.
pub fn sizes_itself(spec: &PopupSpec) -> bool {
    spec.rows.iter().any(|row| matches!(row, Row::Remote(_)))
}

fn row_height(row: &Row) -> f32 {
    match row {
        Row::Header(_) => 36.0,
        Row::Slider { .. } | Row::Item(_) => ITEM_HEIGHT,
        Row::Section { .. } => SECTION_HEIGHT,
        Row::Separator => 16.0,
        Row::Action { .. } => ACTION_HEIGHT,
        Row::Readout { .. } => 64.0,
        Row::Calendar(_) => 252.0,
        Row::Remote(_) => 0.0,
    }
}

fn shape_key(index: usize, row: &Row) -> u64 {
    let mut hasher = DefaultHasher::new();
    match row {
        Row::Item(item) if item.identity.is_some() => item.identity.hash(&mut hasher),
        _ => index.hash(&mut hasher),
    }
    std::mem::discriminant(row).hash(&mut hasher);
    match row {
        Row::Header(header) => (
            &header.title,
            header.toggle.is_some(),
            header.button.map(|button| button.icon),
        )
            .hash(&mut hasher),
        Row::Section { title, chevron } => (title, chevron).hash(&mut hasher),
        Row::Action { label, tone } => (label, tone).hash(&mut hasher),
        Row::Item(item) => (
            &item.label,
            item.badge,
            item.enabled,
            item.chevron,
            item.warning,
            item.battery.is_some(),
            item.signal.is_some(),
            item.busy,
            item.detail.is_some(),
            item.switch.is_some(),
            matches!(item.icon, Icon::None),
        )
            .hash(&mut hasher),
        // A plugin's tree is drawn once per revision; a new one is a new row.
        Row::Remote(surface) => surface.version(Part::Popup).hash(&mut hasher),
        Row::Slider { muted, .. } => muted.is_some().hash(&mut hasher),
        Row::Separator | Row::Readout { .. } | Row::Calendar(_) => {}
    }
    hasher.finish()
}

/// The view for `row`, whose shape its key pins for as long as it is mounted.
pub fn view(cx: &mut Cx, bar: Bar, row: Signal<PanelRow>) -> AnyView {
    let current = row.with_untracked(cx, |row| row.row.clone());
    match current {
        Row::Separator => panel_separator().boxed(),
        Row::Header(header) => header_view(cx, bar, row, header),
        Row::Slider { value, muted, .. } => slider(cx, bar, row, value, muted.is_some()),
        Row::Section { title, chevron } => {
            let section = panel_section(title);
            if chevron {
                section
                    .disclosure()
                    .on_click(move |cx, _| activate(cx, bar, row))
                    .boxed()
            } else {
                section.boxed()
            }
        }
        Row::Action { label, tone } => {
            if tone == Tone::Danger {
                let theme = use_theme(cx);
                provide_derived_theme(cx, move |runtime| theme.with(runtime, danger));
            }
            panel_action(label)
                .on_click(move |cx, _| activate(cx, bar, row))
                .boxed()
        }
        Row::Readout { .. } => panel_readout(
            bind(move |runtime| row.with(runtime, |row| readout(&row.row).0)),
            bind(move |runtime| row.with(runtime, |row| readout(&row.row).1)),
        )
        .boxed(),
        Row::Calendar(today) => {
            let shown = cx.signal(today);
            cx.effect(move |runtime| {
                if let Row::Calendar(today) = row.with(runtime, |row| row.row.clone()) {
                    shown.set(runtime, today);
                }
            });
            calendar(shown).boxed()
        }
        Row::Item(item) => item_view(cx, bar, row, *item),
        Row::Remote(surface) => remote::panel(cx, bar, &surface),
    }
}

/// The panel's theme with its ink swapped for the danger colour, for an
/// action that cannot be undone.
fn danger(theme: &Theme) -> Theme {
    let mut theme = *theme;
    theme.popover.text = theme.status.danger;
    theme
}

fn readout(row: &Row) -> (String, String) {
    match row {
        Row::Readout { primary, secondary } => (primary.clone(), secondary.clone()),
        _ => (String::new(), String::new()),
    }
}

fn index_of(cx: &mut Cx, row: Signal<PanelRow>) -> usize {
    row.with_untracked(cx, |row| row.index)
}

fn activate(cx: &mut Cx, bar: Bar, row: Signal<PanelRow>) {
    let row = index_of(cx, row);
    dispatch(cx, bar, PopupAction::Activate { row });
}

fn header_view(cx: &mut Cx, bar: Bar, row: Signal<PanelRow>, header: Header) -> AnyView {
    let mut view = panel_header(header.title);
    if header.button.is_some() {
        view = view.trailing(header_button(cx, bar, row));
    }
    match header.toggle {
        Some(on) => header_switch(cx, bar, row, view, on),
        None => view.boxed(),
    }
}

fn stated_header(row: &Row) -> Option<&Header> {
    match row {
        Row::Header(header) => Some(header),
        _ => None,
    }
}

/// The header's master switch. Kit's switch flips its own signal; the flip
/// bubbles here, where it becomes the widget's action.
fn header_switch(
    cx: &mut Cx,
    bar: Bar,
    row: Signal<PanelRow>,
    header: Element<PanelHeaderKind>,
    on: bool,
) -> AnyView {
    let switch = cx.signal(on);
    cx.effect(move |runtime| {
        if let Some(on) = row.with(runtime, |row| {
            stated_header(&row.row).and_then(|h| h.toggle)
        }) {
            switch.set(runtime, on);
        }
    });
    header
        .switch(switch)
        .on_click(move |cx, _| {
            let on = switch.with_untracked(cx, |on| *on);
            let (index, stated) = row.with_untracked(cx, |row| {
                let stated = stated_header(&row.row).and_then(|h| h.toggle);
                (row.index, stated.unwrap_or_default())
            });
            if on != stated {
                dispatch(cx, bar, PopupAction::Toggle { row: index, on });
            }
        })
        .boxed()
}

/// The header's round glyph button. Pressing it activates the header's row.
fn header_button(cx: &mut Cx, bar: Bar, row: Signal<PanelRow>) -> AnyView {
    let theme = use_theme(cx);
    let hovered = cx.signal(false);
    let tint = cx.memo(move |runtime| theme.with(runtime, |theme| theme.popover.text));
    let busy = cx.memo(move |runtime| {
        row.with(runtime, |row| {
            stated_header(&row.row)
                .and_then(|header| header.button)
                .is_some_and(|button| button.busy)
        })
    });
    let icon_of = |row: &Row| {
        stated_header(row)
            .and_then(|header| header.button)
            .map_or(Icon::None, |button| Icon::Rune(button.icon))
    };
    let face = show(
        busy,
        || spinner().size(HEADER_BUTTON_GLYPH),
        move || glyph(bar, row, tint, HEADER_BUTTON_GLYPH, Seat::Bare, icon_of),
    );
    hstack(face)
        .size(HEADER_BUTTON)
        .rounded(HEADER_BUTTON / 2.0)
        .items_center()
        .justify_center()
        .cursor(CursorIcon::Pointer)
        .role(Role::Button)
        .label("Refresh")
        .bg(bind(move |runtime| {
            let hovered = hovered.get(runtime);
            theme.with(runtime, |theme| match hovered {
                true => theme.popover.hover_bg,
                false => TRANSPARENT,
            })
        }))
        .on_pointer_enter(move |cx, _| hovered.set(cx, true))
        .on_pointer_leave(move |cx, _| hovered.set(cx, false))
        .on_click(move |cx, _| activate(cx, bar, row))
        .boxed()
}

/// A continuous value. The knob follows the pointer at once; what the
/// service reads back glides it the rest of the way.
fn slider(cx: &mut Cx, bar: Bar, row: Signal<PanelRow>, value: f32, mutable: bool) -> AnyView {
    let shown = cx.signal(value.clamp(0.0, 1.0));
    let sent = cx.signal(value.clamp(0.0, 1.0));
    cx.effect(move |runtime| {
        if let Row::Slider { value, .. } = row.with(runtime, |row| row.row.clone()) {
            // A level boosted past 100 % still shows as a full track.
            let value = value.clamp(0.0, 1.0);
            shown.set(runtime, value);
            sent.set(runtime, value);
        }
    });
    let slide = move |cx: &mut Cx, commit: bool| {
        let value = shown.with_untracked(cx, |value| *value);
        let moved = sent.with_untracked(cx, |sent| *sent != value);
        if moved || commit {
            sent.set(cx, value);
            let row = index_of(cx, row);
            dispatch(cx, bar, PopupAction::Slide { row, value, commit });
        }
    };
    let glyph = move |tint: Memo<Color>| {
        let face = slider_glyph(bar, row, tint);
        if !mutable {
            return face;
        }
        hstack(face)
            .cursor(CursorIcon::Pointer)
            .role(Role::Button)
            .label("Mute")
            .on_click(move |cx, event| {
                event.stop_propagation();
                activate(cx, bar, row);
            })
            .boxed()
    };
    panel_slider("Level", shown)
        .glyph(glyph)
        .on_drag(move |cx, _| slide(cx, false))
        .on_drag_end(move |cx, _| slide(cx, true))
        .on_click(move |cx, _| slide(cx, true))
        .on_wheel(move |cx, _| slide(cx, true))
        .on_key_down(move |cx, _| slide(cx, true))
        .boxed()
}

/// A slider's icon on its badge, dimmed while what it controls is muted.
fn slider_glyph(bar: Bar, row: Signal<PanelRow>, tint: Memo<Color>) -> AnyView {
    canvas(move |runtime, size, list| {
        let (icon, muted) = row.with(runtime, |row| match row.row {
            Row::Slider { icon, muted, .. } => (icon, muted == Some(true)),
            _ => (Icon::None, false),
        });
        let palette = bar.palette.get(runtime);
        let color = if muted {
            palette.fg_dim
        } else {
            tint.get(runtime)
        };
        let bounds = Rect::new(0.0, 0.0, f64::from(size.width), f64::from(size.height))
            + Seat::Disc.nudge(icon, size.width);
        icons::draw_in(&mut Scene::new(list), icon, bounds, color, &palette);
    })
    .size(PLAIN_GLYPH)
    .boxed()
}

fn item_view(cx: &mut Cx, bar: Bar, row: Signal<PanelRow>, item: Item) -> AnyView {
    let mut view = panel_item(item.label.clone()).selected(bind(move |runtime| {
        row.with(
            runtime,
            |row| matches!(&row.row, Row::Item(item) if item.selected),
        )
    }));
    let icon_of = |row: &Row| match row {
        Row::Item(item) => item.icon,
        _ => Icon::None,
    };
    if !matches!(item.icon, Icon::None) {
        view = if item.badge {
            view.badge(move |tint| glyph(bar, row, tint, BADGE_GLYPH, Seat::Disc, icon_of))
        } else {
            view.glyph(move |tint| glyph(bar, row, tint, PLAIN_GLYPH, Seat::Bare, icon_of))
        };
    }
    if item.detail.is_some() {
        view = view.detail(bind(move |runtime| {
            row.with(runtime, |row| match &row.row {
                Row::Item(item) => item.detail.clone().unwrap_or_default(),
                _ => String::new(),
            })
        }));
    }
    if item.battery.is_some() {
        view = view.trailing(battery_cell(bar, row));
    }
    if item.warning {
        view = view.trailing(warning(bar));
    }
    if item.signal.is_some() {
        view = view.trailing(signal_meter(bar, row));
    }
    if item.busy {
        view = view.trailing(spinner().size(SPINNER));
    }
    if item.chevron {
        view = view.chevron();
    }
    if !item.enabled {
        return view.inert().boxed();
    }
    match item.switch {
        Some(on) => item_switch(cx, bar, row, view, &item.label, on),
        None => view.on_click(move |cx, _| activate(cx, bar, row)).boxed(),
    }
}

fn stated_switch(row: &Row) -> Option<bool> {
    match row {
        Row::Item(item) => item.switch,
        _ => None,
    }
}

/// An item's trailing switch, which a click anywhere on the row flips. It
/// flips away from the position last asked for, so a second click before the
/// service answers takes the first back rather than repeating it.
fn item_switch(
    cx: &mut Cx,
    bar: Bar,
    row: Signal<PanelRow>,
    item: Element<PanelItemKind>,
    label: &str,
    on: bool,
) -> AnyView {
    let switch = cx.signal(on);
    let sent = cx.signal(on);
    let stated = cx.memo(move |runtime| row.with(runtime, |row| stated_switch(&row.row)));
    cx.effect(move |runtime| {
        if let Some(on) = stated.get(runtime) {
            switch.set(runtime, on);
            sent.set(runtime, on);
        }
    });
    item.trailing(toggle(switch).label(label.to_owned()).self_center())
        .on_click(move |cx, _| {
            let on = !sent.with_untracked(cx, |on| *on);
            switch.set(cx, on);
            sent.set(cx, on);
            let row = index_of(cx, row);
            dispatch(cx, bar, PopupAction::Toggle { row, on });
        })
        .boxed()
}

/// A row's glyph, `size` square and nudged to look centred in its `seat`,
/// redrawn when the row's icon or the palette changes.
fn glyph(
    bar: Bar,
    row: Signal<PanelRow>,
    tint: Memo<Color>,
    size: f32,
    seat: Seat,
    icon: fn(&Row) -> Icon,
) -> AnyView {
    canvas(move |runtime, size, list| {
        let icon = row.with(runtime, |row| icon(&row.row));
        let color = tint.get(runtime);
        let palette = bar.palette.get(runtime);
        let bounds = Rect::new(0.0, 0.0, f64::from(size.width), f64::from(size.height))
            + seat.nudge(icon, size.width);
        icons::draw_in(&mut Scene::new(list), icon, bounds, color, &palette);
    })
    .size(size)
    .boxed()
}

fn battery_cell(bar: Bar, row: Signal<PanelRow>) -> AnyView {
    canvas(move |runtime, size, list| {
        let level = row.with(runtime, |row| match &row.row {
            Row::Item(item) => item.battery.unwrap_or_default(),
            _ => 0.0,
        });
        let palette = bar.palette.get(runtime);
        let bounds = Rect::new(0.0, 0.0, f64::from(size.width), f64::from(size.height));
        let state = BatteryState {
            level,
            ..Default::default()
        };
        icons::draw_in(
            &mut Scene::new(list),
            Icon::Battery(state),
            bounds,
            palette.fg_dim,
            &palette,
        );
    })
    .w(BATTERY_CELL.0)
    .h(BATTERY_CELL.1)
    .boxed()
}

fn signal_meter(bar: Bar, row: Signal<PanelRow>) -> AnyView {
    canvas(move |runtime, size, list| {
        let strength = row.with(runtime, |row| match &row.row {
            Row::Item(item) => item.signal.unwrap_or_default(),
            _ => 0.0,
        });
        let palette = bar.palette.get(runtime);
        let bounds = Rect::new(0.0, 0.0, f64::from(size.width), f64::from(size.height));
        icons::draw_in(
            &mut Scene::new(list),
            Icon::Signal { strength },
            bounds,
            palette.fg_dim,
            &palette,
        );
    })
    .w(SIGNAL_METER.0)
    .h(SIGNAL_METER.1)
    .boxed()
}

fn warning(bar: Bar) -> AnyView {
    canvas(move |runtime, size, list| {
        let palette = bar.palette.get(runtime);
        let bounds = Rect::new(0.0, 0.0, f64::from(size.width), f64::from(size.height));
        icons::draw_in(
            &mut Scene::new(list),
            Icon::Rune(Rune::Warning),
            bounds,
            palette.warning,
            &palette,
        );
    })
    .size(WARNING_GLYPH)
    .boxed()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(rows: Vec<Row>) -> PopupSpec {
        PopupSpec { rows }
    }

    #[test]
    fn the_height_is_what_the_rows_add_up_to() {
        let rows = vec![
            Header::new("Sound").row(),
            Row::Separator,
            Item::new("Speakers").row(),
        ];
        assert_eq!(
            height(&spec(rows)),
            8.0 + 36.0 + 4.0 + 16.0 + 4.0 + 40.0 + 8.0
        );
    }

    fn plugin_panel(session: u64, rev: u32) -> Row {
        use std::sync::Arc;

        use crownplugin_proto::{Node, RemoteSurface, Tree};

        Row::Remote(crownbar_widgets::Surface {
            session,
            remote: Arc::new(RemoteSurface {
                plugin: "p".into(),
                widget: "w".into(),
                main: None,
                popup: Some(Tree {
                    rev,
                    root: Node::Spacer,
                }),
                lock_slot: None,
            }),
        })
    }

    #[test]
    fn a_plugin_panel_sizes_itself_and_is_rebuilt_per_tree() {
        let panel = spec(vec![plugin_panel(1, 3)]);
        assert!(sizes_itself(&panel));
        assert!(!sizes_itself(&spec(vec![Row::Separator])));
        assert_eq!(height(&panel), 16.0);

        let key = |row| keyed(spec(vec![row]))[0].key;
        assert_eq!(key(plugin_panel(1, 3)), key(plugin_panel(1, 3)));
        assert_ne!(key(plugin_panel(1, 3)), key(plugin_panel(1, 4)));
        assert_ne!(key(plugin_panel(1, 3)), key(plugin_panel(2, 3)));
    }

    #[test]
    fn a_switch_keeps_its_row_while_it_flips() {
        let row = |on| keyed(spec(vec![Item::new("Pixel").switch(on).row()]))[0].key;
        assert_eq!(row(false), row(true));
        assert_ne!(
            row(true),
            keyed(spec(vec![Item::new("Pixel").row()]))[0].key
        );
    }

    #[test]
    fn an_identified_row_keeps_its_key_wherever_it_lands() {
        let pixel = || Item::new("Pixel").identity(7).row();
        let alone = keyed(spec(vec![pixel()]));
        let moved = keyed(spec(vec![Item::new("OnePlus").identity(8).row(), pixel()]));
        assert_eq!(alone[0].key, moved[1].key);
        assert_ne!(moved[0].key, moved[1].key);
    }

    #[test]
    fn a_destructive_action_is_a_different_row() {
        let key = |row| keyed(spec(vec![row]))[0].key;
        assert_ne!(
            key(Row::action("Hang Up")),
            key(Row::destructive("Hang Up"))
        );
    }

    #[test]
    fn a_row_keeps_its_key_while_only_its_reading_changes() {
        let before = keyed(spec(vec![Item::new("AirPods").detail("70%").row()]));
        let after = keyed(spec(vec![
            Item::new("AirPods").detail("65%").selected(true).row(),
        ]));
        assert_eq!(before[0].key, after[0].key);
        let renamed = keyed(spec(vec![Item::new("Speakers").detail("65%").row()]));
        assert_ne!(before[0].key, renamed[0].key);
    }
}

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
        PanelHeaderOptions, PanelItemOptions, PanelSectionOptions, PanelSliderOptions, calendar,
        panel_action, panel_header, panel_item, panel_readout, panel_section, panel_separator,
        panel_slider,
    },
    prelude::{AnyView, Color, Cx, Listeners, Memo, Signal, Styled, View, bind, canvas},
};
use kurbo::Rect;

use crownplugin_proto::Part;

use super::dispatch;
use crate::{
    ui::{icons, remote, scene::Scene, state::Bar},
    widgets::{
        BatteryState, Icon, PopupAction, PopupSpec, Rune,
        popup::{Item, Row},
    },
};

const PAD_Y: f32 = 8.0;
const BADGE_GLYPH: f32 = 16.0;
const PLAIN_GLYPH: f32 = 18.0;
const BATTERY_CELL: (f32, f32) = (24.0, 12.0);
const WARNING_GLYPH: f32 = 16.0;

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
    PAD_Y * 2.0 + spec.rows.iter().map(row_height).sum::<f32>()
}

/// Whether some row's height is only known once it is laid out.
pub fn sizes_itself(spec: &PopupSpec) -> bool {
    spec.rows.iter().any(|row| matches!(row, Row::Remote(_)))
}

fn row_height(row: &Row) -> f32 {
    match row {
        Row::Header { .. } => 36.0,
        Row::Slider { .. } | Row::Item(_) => 40.0,
        Row::Section { .. } => 28.0,
        Row::Separator => 16.0,
        Row::Action { .. } => 32.0,
        Row::Readout { .. } => 64.0,
        Row::Calendar(_) => 252.0,
        Row::Remote(_) => 0.0,
    }
}

fn shape_key(index: usize, row: &Row) -> u64 {
    let mut hasher = DefaultHasher::new();
    index.hash(&mut hasher);
    std::mem::discriminant(row).hash(&mut hasher);
    match row {
        Row::Header { title, toggle } => (title, toggle.is_some()).hash(&mut hasher),
        Row::Section { title, chevron } => (title, chevron).hash(&mut hasher),
        Row::Action { label } => label.hash(&mut hasher),
        Row::Item(item) => (
            &item.label,
            item.badge,
            item.enabled,
            item.chevron,
            item.warning,
            item.battery.is_some(),
            item.detail.is_some(),
            matches!(item.icon, Icon::None),
        )
            .hash(&mut hasher),
        // A plugin's tree is drawn once per revision; a new one is a new row.
        Row::Remote(surface) => surface.version(Part::Popup).hash(&mut hasher),
        Row::Slider { .. } | Row::Separator | Row::Readout { .. } | Row::Calendar(_) => {}
    }
    hasher.finish()
}

/// The view for `row`, whose shape its key pins for as long as it is mounted.
pub fn view(cx: &mut Cx, bar: Bar, row: Signal<PanelRow>) -> AnyView {
    let current = row.with_untracked(cx, |row| row.row.clone());
    match current {
        Row::Separator => panel_separator().boxed(),
        Row::Header {
            title,
            toggle: None,
        } => panel_header(title).boxed(),
        Row::Header {
            title,
            toggle: Some(on),
        } => header_switch(cx, bar, row, title, on),
        Row::Slider { value, .. } => slider(cx, bar, row, value),
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
        Row::Action { label } => panel_action(label)
            .on_click(move |cx, _| activate(cx, bar, row))
            .boxed(),
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
        Row::Item(item) => item_view(bar, row, *item),
        Row::Remote(surface) => remote::panel(cx, bar, &surface),
    }
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

/// The header's master switch. Kit's switch flips its own signal; the flip
/// bubbles here, where it becomes the widget's action.
fn header_switch(cx: &mut Cx, bar: Bar, row: Signal<PanelRow>, title: String, on: bool) -> AnyView {
    let switch = cx.signal(on);
    cx.effect(move |runtime| {
        if let Row::Header {
            toggle: Some(on), ..
        } = row.with(runtime, |row| row.row.clone())
        {
            switch.set(runtime, on);
        }
    });
    panel_header(title)
        .switch(switch)
        .on_click(move |cx, _| {
            let on = switch.with_untracked(cx, |on| *on);
            let (index, stated) = row.with_untracked(cx, |row| {
                let stated = matches!(row.row, Row::Header { toggle: Some(stated), .. } if stated);
                (row.index, stated)
            });
            if on != stated {
                dispatch(cx, bar, PopupAction::Toggle { row: index, on });
            }
        })
        .boxed()
}

/// A continuous value. The knob follows the pointer at once; what the
/// service reads back glides it the rest of the way.
fn slider(cx: &mut Cx, bar: Bar, row: Signal<PanelRow>, value: f32) -> AnyView {
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
        glyph(bar, row, tint, PLAIN_GLYPH, |row| match row {
            Row::Slider { icon, .. } => *icon,
            _ => Icon::None,
        })
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

fn item_view(bar: Bar, row: Signal<PanelRow>, item: Item) -> AnyView {
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
            view.badge(move |tint| glyph(bar, row, tint, BADGE_GLYPH, icon_of))
        } else {
            view.glyph(move |tint| glyph(bar, row, tint, PLAIN_GLYPH, icon_of))
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
    if item.chevron {
        view = view.chevron();
    }
    if !item.enabled {
        return view.inert().boxed();
    }
    view.on_click(move |cx, _| activate(cx, bar, row)).boxed()
}

/// A row's glyph, redrawn when the row's icon or the palette changes.
fn glyph(
    bar: Bar,
    row: Signal<PanelRow>,
    tint: Memo<Color>,
    size: f32,
    icon: fn(&Row) -> Icon,
) -> AnyView {
    canvas(move |runtime, size, list| {
        let icon = row.with(runtime, |row| icon(&row.row));
        let color = tint.get(runtime);
        let palette = bar.palette.get(runtime);
        let bounds = Rect::new(0.0, 0.0, f64::from(size.width), f64::from(size.height));
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
    use crownui::kit;

    fn spec(rows: Vec<Row>) -> PopupSpec {
        PopupSpec { rows }
    }

    #[test]
    fn the_height_is_what_the_rows_add_up_to() {
        let rows = vec![
            Row::Header {
                title: "Sound".into(),
                toggle: None,
            },
            Row::Separator,
            Item::new("Speakers").row(),
        ];
        assert_eq!(height(&spec(rows)), 8.0 + 36.0 + 16.0 + 40.0 + 8.0);
        assert_eq!(kit::PANEL_WIDTH, 296.0);
    }

    fn plugin_panel(session: u64, rev: u32) -> Row {
        use std::sync::Arc;

        use crownplugin_proto::{Node, RemoteSurface, Tree};

        Row::Remote(crate::widgets::Surface {
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

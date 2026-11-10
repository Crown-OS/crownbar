//! A live activity's chip, under its glyph and label: the resting wash, the
//! boost that strengthens it under the pointer and while its panel is open,
//! and the highlight over both. The boost is a layer whose opacity the render
//! thread fades, so the chip brightens as smoothly as a plain pill's fill.

use crownbar_integrations::live::wire::Accent;
use crownui::{
    ext::{DrawList, Paint},
    prelude::{
        Color, CornerRadii, Memo, Point, Rect, Runtime, Size, Styled, View, bind, canvas, show,
        zstack,
    },
};
use kurbo::Affine;

use super::nothing;
use crate::{
    scene::{Fill, Scene},
    state::Bar,
    theme::{ChipLook, HIGHLIGHT_DROP, PillState, Stops, boost_opacity},
    util::capsule::capsule_crescent,
};

pub fn wash(bar: Bar, accent: Memo<Option<Accent>>, state: Memo<PillState>) -> impl View {
    let look = move |runtime: &mut Runtime| {
        let accent = accent.get(runtime)?;
        Some(bar.palette.get(runtime).chips.look(accent))
    };
    show(
        bind(move |runtime| accent.get(runtime).is_some()),
        move || {
            zstack((
                canvas(move |runtime, size, list| {
                    if let Some(look) = look(runtime) {
                        fill(list, size, look.fill);
                    }
                })
                .w_full()
                .h_full(),
                canvas(move |runtime, size, list| {
                    if let Some(look) = look(runtime) {
                        fill(list, size, look.boost());
                    }
                })
                .opacity(bind(move |runtime| boost_opacity(state.get(runtime)))),
                canvas(move |runtime, size, list| {
                    if let Some(ChipLook { highlight, .. }) = look(runtime) {
                        rim(list, size, highlight);
                    }
                }),
            ))
            .w_full()
            .h_full()
        },
        nothing,
    )
    .absolute()
    .inset(0.0)
    .pointer_events_none()
}

fn fill(list: &mut DrawList, size: Size, stops: Stops) {
    let paint = Paint {
        brush: stops.vertical(0.0, 0.0, f64::from(size.height)),
        blur: None,
    };
    let radii = CornerRadii::uniform(size.height * 0.5);
    list.fill(Rect::new(Point::ZERO, size), radii, paint);
}

fn rim(list: &mut DrawList, size: Size, color: Color) {
    let bounds = kurbo::Rect::new(0.0, 0.0, f64::from(size.width), f64::from(size.height));
    let crescent = capsule_crescent(bounds, HIGHLIGHT_DROP);
    Scene::new(list).fill(Fill::NonZero, Affine::IDENTITY, color, None, &crescent);
}

#[cfg(test)]
mod tests {
    use crownui::ext::{Brush, DrawCommand};

    use super::*;

    const CHIP: Size = Size::new(96.0, 32.0);

    #[test]
    fn the_wash_is_a_capsule_graded_top_to_bottom() {
        let stops = Stops {
            start: Color::new(1.0, 0.0, 0.0, 0.6),
            end: Color::new(1.0, 0.0, 0.0, 0.7),
        };
        let mut list = DrawList::new();
        fill(&mut list, CHIP, stops);
        let [DrawCommand::FillRoundedRect(rect, radii, paint)] = list.commands() else {
            panic!("{:?}", list.commands());
        };
        assert_eq!(*rect, Rect::new(Point::ZERO, CHIP));
        assert_eq!(*radii, CornerRadii::uniform(16.0));
        let Brush::LinearGradient { start, end, stops } = &paint.brush else {
            panic!("{paint:?}");
        };
        assert_eq!((start.y, end.y), (0.0, 32.0));
        assert_eq!((stops[0].color.a, stops[1].color.a), (0.6, 0.7));
    }

    #[test]
    fn the_highlight_is_one_path_along_the_top() {
        let mut list = DrawList::new();
        rim(&mut list, CHIP, Color::new(1.0, 1.0, 1.0, 0.45));
        let [DrawCommand::FillPath(path, _)] = list.commands() else {
            panic!("{:?}", list.commands());
        };
        let bounds = path.bounds();
        assert!(
            bounds.origin.y.abs() < 1e-4 && bounds.size.height <= 16.5,
            "{bounds:?}"
        );
    }
}

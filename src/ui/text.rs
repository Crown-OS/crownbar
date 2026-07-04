use parley::{
    Alignment, AlignmentOptions, FontContext, GenericFamily, Layout, LayoutContext,
    PositionedLayoutItem, StyleProperty,
};
use vello::{
    Glyph, Scene,
    peniko::{Brush, Color, Fill},
};

const DISPLAY_SCALE: f32 = 1.0;

fn build_layout(
    font_ctx: &mut FontContext,
    layout_ctx: &mut LayoutContext<Brush>,
    text: &str,
    font_size: f32,
) -> Layout<Brush> {
    let mut builder = layout_ctx.ranged_builder(font_ctx, text, DISPLAY_SCALE, true);
    builder.push_default(StyleProperty::FontStack(parley::FontStack::Single(
        parley::FontFamily::Generic(GenericFamily::SystemUi),
    )));
    builder.push_default(StyleProperty::FontSize(font_size));
    builder.push_default(StyleProperty::LineHeight(parley::LineHeight::FontSizeRelative(1.2)));
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    layout.align(None, Alignment::Start, AlignmentOptions::default());
    layout
}

/// Measure the rendered width of a single-line string at `font_size`.
pub fn measure_text(
    font_ctx: &mut FontContext,
    layout_ctx: &mut LayoutContext<Brush>,
    text: &str,
    font_size: f32,
) -> f32 {
    let layout = build_layout(font_ctx, layout_ctx, text, font_size);
    layout.width()
}

/// Draw `text` with the baseline at `y_center` (vertically centered). Returns
/// the advance width consumed.
pub fn draw_text(
    scene: &mut Scene,
    font_ctx: &mut FontContext,
    layout_ctx: &mut LayoutContext<Brush>,
    text: &str,
    x: f32,
    y_center: f32,
    font_size: f32,
    color: Color,
) -> f32 {
    let layout = build_layout(font_ctx, layout_ctx, text, font_size);
    let total_height = layout.height();
    let y_top = y_center - total_height * 0.5;
    let brush = Brush::Solid(color);

    for line in layout.lines() {
        let line_y = y_top + line.metrics().baseline;
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };
            let run = glyph_run.run();
            let font = run.font().clone();
            let font_size = run.font_size();
            let mut pen_x = x + glyph_run.offset();
            let pen_y = line_y;
            let glyphs = glyph_run.glyphs().map(|g| {
                let gx = pen_x + g.x;
                let gy = pen_y - g.y;
                pen_x += g.advance;
                Glyph {
                    id: g.id as u32,
                    x: gx,
                    y: gy,
                }
            });
            scene
                .draw_glyphs(&font)
                .font_size(font_size)
                .brush(&brush)
                .draw(Fill::NonZero, glyphs);
        }
    }

    layout.width()
}

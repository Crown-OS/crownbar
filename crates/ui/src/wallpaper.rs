//! The wallpaper under the bar, reduced to the colors its glyphs are read against.
//!
//! crownpaper fills the output with the wallpaper cover-fit and centred, so the
//! strip of screen the bar covers is one band of the image. That band is
//! averaged in linear light into [`SAMPLES`] columns: few enough to be free,
//! enough to see a bright patch under one end of the bar.

use std::path::Path;

use crownui::prelude::{Color, Point, Rect, Size};
use image::{ImageReader, ImageResult, RgbImage};

use crate::util::contrast::{to_encoded, to_linear};
use crownbar_config::{BarConfig, BarPosition};

pub const SAMPLES: usize = 16;

/// The wallpaper's average color in each of [`SAMPLES`] columns of the bar,
/// left to right.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Backdrop(pub [Color; SAMPLES]);

/// The wallpaper at `path` inside `strip` of a `screen`, both in logical px.
pub fn sample(path: &Path, screen: Size, strip: Rect) -> ImageResult<Backdrop> {
    let image = ImageReader::open(path)?.with_guessed_format()?.decode()?;
    let (iw, ih) = (image.width(), image.height());
    let scale = (screen.width / iw as f32).max(screen.height / ih as f32);
    let crop_x = (iw as f32 - screen.width / scale) * 0.5;
    let crop_y = (ih as f32 - screen.height / scale) * 0.5;
    let (x, width) = span(
        crop_x + strip.min_x() / scale,
        crop_x + strip.max_x() / scale,
        iw,
    );
    let (y, height) = span(
        crop_y + strip.origin.y / scale,
        crop_y + (strip.origin.y + strip.size.height) / scale,
        ih,
    );
    Ok(Backdrop::average(
        &image.crop_imm(x, y, width, height).into_rgb8(),
    ))
}

/// Where the bar's strip lies on a `screen`.
pub fn strip(config: &BarConfig, screen: Size) -> Rect {
    let (margin, height) = (config.margin as f32, config.bar_height as f32);
    let top = match config.position {
        BarPosition::Top => margin,
        BarPosition::Bottom => screen.height - margin - height,
    };
    Rect::new(
        Point::new(margin, top),
        Size::new(screen.width - 2.0 * margin, height),
    )
}

/// The whole pixels from `start` to `end`, at least one, inside `0..limit`.
fn span(start: f32, end: f32, limit: u32) -> (u32, u32) {
    let first = (start.floor().max(0.0) as u32).min(limit.saturating_sub(1));
    let last = (end.ceil().max(0.0) as u32).min(limit).max(first + 1);
    (first, last - first)
}

impl Backdrop {
    fn average(band: &RgbImage) -> Self {
        let linear: [f32; 256] = std::array::from_fn(|value| to_linear(value as f32 / 255.0));
        let width = band.width() as usize;
        Self(std::array::from_fn(|column| {
            let start = column * width / SAMPLES;
            let end = ((column + 1) * width / SAMPLES).max(start + 1).min(width);
            let (sum, count) = band
                .rows()
                .flat_map(|row| row.skip(start).take(end.saturating_sub(start)))
                .fold(([0.0_f32; 3], 0_u32), |(sum, count), pixel| {
                    let [r, g, b] = pixel.0.map(|channel| linear[usize::from(channel)]);
                    ([sum[0] + r, sum[1] + g, sum[2] + b], count + 1)
                });
            let [r, g, b] = sum.map(|channel| to_encoded(channel / count.max(1) as f32));
            Color::new(r, g, b, 1.0)
        }))
    }
}

#[cfg(test)]
mod tests {
    use image::Rgb;

    use super::*;

    #[test]
    fn each_column_averages_its_own_part_of_the_band() {
        let band = RgbImage::from_fn(64, 4, |x, _| {
            if x < 32 {
                Rgb([0, 0, 0])
            } else {
                Rgb([255, 255, 255])
            }
        });
        let Backdrop(columns) = Backdrop::average(&band);
        assert_eq!(columns[0], Color::new(0.0, 0.0, 0.0, 1.0));
        assert!((columns[SAMPLES - 1].r - 1.0).abs() < 1e-5);
    }

    #[test]
    fn a_band_narrower_than_the_samples_still_fills_every_column() {
        let band = RgbImage::from_pixel(3, 2, Rgb([255, 255, 255]));
        let Backdrop(columns) = Backdrop::average(&band);
        assert!(columns.iter().all(|color| (color.g - 1.0).abs() < 1e-5));
    }

    #[test]
    fn a_span_never_leaves_the_image_or_comes_up_empty() {
        assert_eq!(span(10.4, 20.2, 100), (10, 11));
        assert_eq!(span(-5.0, 2.0, 100), (0, 2));
        assert_eq!(span(120.0, 130.0, 100), (99, 1));
    }

    #[test]
    fn the_strip_follows_the_bar_to_its_edge() {
        let screen = Size::new(1536.0, 960.0);
        let mut config = BarConfig {
            bar_height: 40,
            position: BarPosition::Top,
            margin: 0,
            radius: None,
        };
        assert_eq!(
            strip(&config, screen),
            Rect::new(Point::new(0.0, 0.0), Size::new(1536.0, 40.0))
        );
        config.position = BarPosition::Bottom;
        config.margin = 8;
        assert_eq!(
            strip(&config, screen),
            Rect::new(Point::new(8.0, 912.0), Size::new(1520.0, 40.0))
        );
    }
}

//! Laying shapes on the device pixel grid, so their straight edges come out
//! crisp at a fractional scale instead of one sharp and one soft.

/// A span of `length` around `center`, both in logical px of a space that
/// starts `offset` logical px into the surface, moved and shortened so both of
/// its ends fall on whole device pixels. Returns the snapped centre and
/// length. The length only ever rounds down, so the span keeps to the room
/// laid out for it.
pub fn snap_span(center: f64, length: f64, offset: f64, scale: f64) -> (f64, f64) {
    if scale <= 0.0 {
        return (center, length);
    }
    let device_length = (length * scale).floor();
    let device_start = ((offset + center) * scale - device_length * 0.5).round();
    let snapped_length = device_length / scale;
    (
        device_start / scale - offset + snapped_length * 0.5,
        snapped_length,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device_edges(center: f64, length: f64, offset: f64, scale: f64) -> (f64, f64) {
        let start = (offset + center - length * 0.5) * scale;
        (start, start + length * scale)
    }

    #[test]
    fn both_ends_land_on_whole_device_pixels() {
        for scale in [1.0, 1.25, 1.5, 1.75, 2.0] {
            for offset in [0.0, 3.0, 6.0, 7.5] {
                let (center, length) = snap_span(19.0, 18.0, offset, scale);
                let (top, bottom) = device_edges(center, length, offset, scale);
                assert!((top - top.round()).abs() < 1e-9, "{scale} {offset}: {top}");
                assert!(
                    (bottom - bottom.round()).abs() < 1e-9,
                    "{scale} {offset}: {bottom}"
                );
                assert!(length <= 18.0, "{scale}: {length}");
                assert!(
                    (center - 19.0).abs() * scale <= 0.5 + 1e-9,
                    "{scale}: {center}"
                );
            }
        }
    }

    #[test]
    fn a_span_already_on_the_grid_is_left_alone() {
        assert_eq!(snap_span(19.0, 18.0, 0.0, 2.0), (19.0, 18.0));
    }
}

//! The month grid behind the clock's panel.
//!
//! Pure date arithmetic, so the shape of a month is testable without a
//! surface. The grid is always six weeks: a panel that grew a row for a month
//! starting on a Sunday would resize under the pointer as it was paged.

use chrono::{Datelike, Duration, NaiveDate, Weekday};

/// Weeks in the grid, fixed so the panel's height never changes.
pub const WEEKS: usize = 6;
pub const DAYS: usize = WEEKS * 7;

/// The day each cell of `anchor`'s month grid stands for, Monday first.
///
/// Six weeks from the Monday on or before the first of the month covers every
/// month there is: the worst case is 31 days starting on a Sunday, which is
/// six days of lead-in and 37 cells.
pub fn grid(anchor: NaiveDate) -> impl Iterator<Item = NaiveDate> {
    let first = anchor.with_day(1).unwrap_or(anchor);
    let lead = first.weekday().num_days_from_monday() as i64;
    let start = first - Duration::days(lead);
    (0..DAYS as i64).map(move |offset| start + Duration::days(offset))
}

/// `date` moved by whole months, clamped to the last day of the month it lands
/// in — stepping back from the 31st must not skip February.
pub fn shift_months(date: NaiveDate, months: i32) -> NaiveDate {
    let zero_based = date.year() as i64 * 12 + date.month0() as i64 + months as i64;
    let (year, month0) = (zero_based.div_euclid(12) as i32, zero_based.rem_euclid(12) as u32);
    let day = date.day().min(days_in_month(year, month0 + 1));
    NaiveDate::from_ymd_opt(year, month0 + 1, day).unwrap_or(date)
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = match month {
        12 => (year + 1, 1),
        _ => (year, month + 1),
    };
    match (
        NaiveDate::from_ymd_opt(year, month, 1),
        NaiveDate::from_ymd_opt(next_year, next_month, 1),
    ) {
        (Some(first), Some(next)) => (next - first).num_days() as u32,
        _ => 31,
    }
}

/// Column headings, Monday first, in the order [`grid`] lays cells out.
pub const WEEKDAY_INITIALS: [&str; 7] = ["M", "T", "W", "T", "F", "S", "S"];

/// Whether `date` falls on a Saturday or Sunday, which the grid dims.
pub fn is_weekend(date: NaiveDate) -> bool {
    matches!(date.weekday(), Weekday::Sat | Weekday::Sun)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_grid_always_covers_six_weeks_from_a_monday() {
        for (year, month) in [(2026, 2), (2026, 3), (2026, 8), (2024, 2), (2027, 5)] {
            let anchor = NaiveDate::from_ymd_opt(year, month, 1).expect("a first of the month");
            let cells: Vec<_> = grid(anchor).collect();
            assert_eq!(cells.len(), DAYS);
            assert_eq!(cells[0].weekday(), Weekday::Mon);
            assert!(
                cells.contains(&anchor),
                "{year}-{month} does not contain its own first"
            );
            let last = anchor.with_day(days_in_month(year, month)).expect("a last");
            assert!(cells.contains(&last), "{year}-{month} loses its last day");
        }
    }

    #[test]
    fn stepping_back_from_a_long_month_does_not_skip_a_short_one() {
        let march = NaiveDate::from_ymd_opt(2026, 3, 31).expect("the 31st");
        assert_eq!(
            shift_months(march, -1),
            NaiveDate::from_ymd_opt(2026, 2, 28).expect("the 28th"),
            "the 31st of March must land in February, not in March again"
        );
    }

    #[test]
    fn stepping_crosses_the_year_in_both_directions() {
        let january = NaiveDate::from_ymd_opt(2026, 1, 15).expect("a date");
        assert_eq!(
            shift_months(january, -1),
            NaiveDate::from_ymd_opt(2025, 12, 15).expect("a date")
        );
        assert_eq!(
            shift_months(january, 12),
            NaiveDate::from_ymd_opt(2027, 1, 15).expect("a date")
        );
    }
}

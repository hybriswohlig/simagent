//! The calendar (ADR-0002): 1 year = 364 days = 52 weeks = 13 months of 28 days = 4
//! seasons of 13 weeks, no leap years; every date falls on the same weekday each year.
//! Everything counts from 0; names of days and months are player texts and live
//! outside the core (B16).

use super::SimTime;

/// A point in time broken down into calendar units. `year` is negative before the
/// scenario start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CalendarDate {
    pub year: i64,
    pub day_of_year: u16,
    pub month: u16,
    pub day_of_month: u16,
    pub week: u16,
    pub weekday: u16,
    pub season: u16,
    pub second_of_day: u32,
}

/// Breaks `t` down into calendar units.
pub fn calendar(_t: SimTime) -> CalendarDate {
    CalendarDate {
        year: 0,
        day_of_year: 0,
        month: 0,
        day_of_month: 0,
        week: 0,
        weekday: 0,
        season: 0,
        second_of_day: 0,
    }
}

/// The first second of `year`. Panics with `time.out_of_range` outside the range.
pub fn year_start(_year: i64) -> SimTime {
    SimTime::START
}

/// Days since scenario start (absolute index for random draws, ADR-0001).
pub fn day_index(_t: SimTime) -> i64 {
    0
}

/// Weeks since scenario start (absolute index).
pub fn week_index(_t: SimTime) -> i64 {
    0
}

/// 28-day months since scenario start (absolute index, e.g. the partner market round).
pub fn month_index(_t: SimTime) -> i64 {
    0
}

#[cfg(test)]
mod tests {
    use super::super::{DAY, SimDuration, WEEK, YEAR};
    use super::*;

    fn at(secs: i64) -> SimTime {
        SimTime::from_secs(secs)
    }

    #[allow(clippy::too_many_arguments)]
    fn date(
        year: i64,
        day_of_year: u16,
        month: u16,
        day_of_month: u16,
        week: u16,
        weekday: u16,
        season: u16,
        second_of_day: u32,
    ) -> CalendarDate {
        CalendarDate {
            year,
            day_of_year,
            month,
            day_of_month,
            week,
            weekday,
            season,
            second_of_day,
        }
    }

    #[test]
    fn calendar_known_answers() {
        let y = YEAR.as_secs();
        let d = DAY.as_secs();
        assert_eq!(calendar(at(0)), date(0, 0, 0, 0, 0, 0, 0, 0));
        assert_eq!(calendar(at(-1)), date(-1, 363, 12, 27, 51, 6, 3, 86_399));
        assert_eq!(calendar(at(-y)), date(-1, 0, 0, 0, 0, 0, 0, 0));
        assert_eq!(
            calendar(at(-y - 1)),
            date(-2, 363, 12, 27, 51, 6, 3, 86_399)
        );
        assert_eq!(calendar(at(y - 1)), date(0, 363, 12, 27, 51, 6, 3, 86_399));
        assert_eq!(calendar(at(y)), date(1, 0, 0, 0, 0, 0, 0, 0));
        // Month, week and season boundaries.
        assert_eq!(
            calendar(at(28 * d - 1)),
            date(0, 27, 0, 27, 3, 6, 0, 86_399)
        );
        assert_eq!(calendar(at(28 * d)), date(0, 28, 1, 0, 4, 0, 0, 0));
        assert_eq!(calendar(at(7 * d)), date(0, 7, 0, 7, 1, 0, 0, 0));
        assert_eq!(calendar(at(91 * d)), date(0, 91, 3, 7, 13, 0, 1, 0));
        assert_eq!(
            calendar(at(91 * d - 1)),
            date(0, 90, 3, 6, 12, 6, 0, 86_399)
        );
    }

    #[test]
    fn every_date_keeps_its_weekday() {
        for day in [0, 17, 363] {
            let this_year = calendar(at(day * DAY.as_secs()));
            let next_year = calendar(at(day * DAY.as_secs() + YEAR.as_secs()));
            assert_eq!(this_year.weekday, next_year.weekday);
            assert_eq!(this_year.day_of_year, next_year.day_of_year);
        }
    }

    #[test]
    fn year_start_is_the_inverse_of_the_year() {
        assert_eq!(year_start(0), SimTime::START);
        assert_eq!(year_start(1), SimTime::START + YEAR);
        assert_eq!(year_start(-1), SimTime::START - YEAR);
        for year in [-5, -1, 0, 1, 200] {
            assert_eq!(calendar(year_start(year)).year, year);
            assert_eq!(
                calendar(year_start(year) - SimDuration::from_secs(1)).year,
                year - 1
            );
        }
    }

    #[test]
    #[should_panic(expected = "invariant time.out_of_range")]
    fn year_start_panics_outside_the_range() {
        let _ = year_start(1_000_001);
    }

    #[test]
    fn indices_count_from_the_start_and_floor_below_it() {
        assert_eq!(day_index(at(-1)), -1);
        assert_eq!(day_index(at(DAY.as_secs())), 1);
        assert_eq!(week_index(at(WEEK.as_secs())), 1);
        assert_eq!(week_index(at(-1)), -1);
        assert_eq!(month_index(at(28 * DAY.as_secs() - 1)), 0);
        assert_eq!(month_index(at(28 * DAY.as_secs())), 1);
        assert_eq!(month_index(at(-1)), -1);
        assert_eq!(month_index(at(YEAR.as_secs())), 13);
    }
}

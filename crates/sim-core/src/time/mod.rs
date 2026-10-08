//! Simulation time (ADR-0002): whole game seconds since scenario start on one calendar
//! of 364 days. All arithmetic is checked; leaving the range of one million years is the
//! invariant break `time.out_of_range` (B21).

mod calendar;
mod clock;
mod scheduler;

pub use calendar::{CalendarDate, calendar, day_index, month_index, week_index, year_start};
pub use clock::NEVER_YEARS;
pub use scheduler::{
    AdvanceOutcome, Ctx, EventClass, EventHandle, EventKey, EventKind, EventSubject, Fired,
    Handler, LoadError, MAX_MICRO_STEP, Payload, RunStats, Scheduler, SchedulerSnapshot,
    SnapshotEntry,
};

use core::fmt;
use core::ops::{Add, Mul, Sub};

use serde::{Deserialize, Serialize};

/// A point in simulation time: whole seconds since scenario start. Founders are born
/// before the start, so negative values are normal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "i64", into = "i64")]
pub struct SimTime(i64);

/// A span of simulation time in whole seconds; negative for differences.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "i64", into = "i64")]
pub struct SimDuration(i64);

/// A number of seconds outside +-`MAX_ABS_SECS` where a time was expected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeOutOfRange;

impl fmt::Display for TimeOutOfRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("time.out_of_range")
    }
}

impl TryFrom<i64> for SimTime {
    type Error = TimeOutOfRange;
    fn try_from(secs: i64) -> Result<SimTime, TimeOutOfRange> {
        if in_range(secs) {
            Ok(SimTime(secs))
        } else {
            Err(TimeOutOfRange)
        }
    }
}

impl From<SimTime> for i64 {
    fn from(t: SimTime) -> i64 {
        t.0
    }
}

impl TryFrom<i64> for SimDuration {
    type Error = TimeOutOfRange;
    fn try_from(secs: i64) -> Result<SimDuration, TimeOutOfRange> {
        if in_range(secs) {
            Ok(SimDuration(secs))
        } else {
            Err(TimeOutOfRange)
        }
    }
}

impl From<SimDuration> for i64 {
    fn from(d: SimDuration) -> i64 {
        d.0
    }
}

pub const MINUTE: SimDuration = SimDuration(60);
pub const HOUR: SimDuration = SimDuration(60 * MINUTE.0);
pub const DAY: SimDuration = SimDuration(24 * HOUR.0);
pub const WEEK: SimDuration = SimDuration(7 * DAY.0);
pub const MONTH: SimDuration = SimDuration(28 * DAY.0);
pub const SEASON: SimDuration = SimDuration(91 * DAY.0);
pub const YEAR: SimDuration = SimDuration(364 * DAY.0);

/// Largest magnitude of a time or duration in seconds: one million years.
pub const MAX_ABS_SECS: i64 = 1_000_000 * YEAR.0;

const fn in_range(secs: i64) -> bool {
    secs >= -MAX_ABS_SECS && secs <= MAX_ABS_SECS
}

fn checked(secs: Option<i64>) -> Option<i64> {
    secs.filter(|&s| in_range(s))
}

#[track_caller]
fn out_of_range() -> ! {
    panic!("invariant time.out_of_range")
}

impl SimTime {
    pub const START: SimTime = SimTime(0);

    /// Panics with `time.out_of_range` outside +-`MAX_ABS_SECS`.
    #[track_caller]
    pub const fn from_secs(secs: i64) -> SimTime {
        assert!(in_range(secs), "invariant time.out_of_range");
        SimTime(secs)
    }

    pub const fn as_secs(self) -> i64 {
        self.0
    }

    pub fn checked_add(self, d: SimDuration) -> Option<SimTime> {
        checked(self.0.checked_add(d.0)).map(SimTime)
    }

    pub fn checked_sub(self, d: SimDuration) -> Option<SimTime> {
        checked(self.0.checked_sub(d.0)).map(SimTime)
    }

    pub fn checked_since(self, earlier: SimTime) -> Option<SimDuration> {
        checked(self.0.checked_sub(earlier.0)).map(SimDuration)
    }
}

impl SimDuration {
    pub const ZERO: SimDuration = SimDuration(0);

    /// Panics with `time.out_of_range` outside +-`MAX_ABS_SECS`.
    #[track_caller]
    pub const fn from_secs(secs: i64) -> SimDuration {
        assert!(in_range(secs), "invariant time.out_of_range");
        SimDuration(secs)
    }

    pub const fn as_secs(self) -> i64 {
        self.0
    }

    pub fn checked_add(self, d: SimDuration) -> Option<SimDuration> {
        checked(self.0.checked_add(d.0)).map(SimDuration)
    }

    pub fn checked_sub(self, d: SimDuration) -> Option<SimDuration> {
        checked(self.0.checked_sub(d.0)).map(SimDuration)
    }

    pub fn checked_mul(self, factor: i64) -> Option<SimDuration> {
        checked(self.0.checked_mul(factor)).map(SimDuration)
    }
}

impl Add<SimDuration> for SimTime {
    type Output = SimTime;
    #[track_caller]
    fn add(self, d: SimDuration) -> SimTime {
        self.checked_add(d).unwrap_or_else(|| out_of_range())
    }
}

impl Sub<SimDuration> for SimTime {
    type Output = SimTime;
    #[track_caller]
    fn sub(self, d: SimDuration) -> SimTime {
        self.checked_sub(d).unwrap_or_else(|| out_of_range())
    }
}

impl Sub<SimTime> for SimTime {
    type Output = SimDuration;
    #[track_caller]
    fn sub(self, earlier: SimTime) -> SimDuration {
        self.checked_since(earlier)
            .unwrap_or_else(|| out_of_range())
    }
}

impl Add for SimDuration {
    type Output = SimDuration;
    #[track_caller]
    fn add(self, d: SimDuration) -> SimDuration {
        self.checked_add(d).unwrap_or_else(|| out_of_range())
    }
}

impl Sub for SimDuration {
    type Output = SimDuration;
    #[track_caller]
    fn sub(self, d: SimDuration) -> SimDuration {
        self.checked_sub(d).unwrap_or_else(|| out_of_range())
    }
}

impl Mul<i64> for SimDuration {
    type Output = SimDuration;
    #[track_caller]
    fn mul(self, factor: i64) -> SimDuration {
        self.checked_mul(factor).unwrap_or_else(|| out_of_range())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    /// The error has a stable code; serde reports it for a time out of range (serde_json
    /// may add a position after it).
    #[test]
    fn time_out_of_range_has_a_stable_code() {
        assert_eq!(TimeOutOfRange.to_string(), "time.out_of_range");
        let too_late = (MAX_ABS_SECS + 1).to_string();
        let error = serde_json::from_str::<SimTime>(&too_late).expect_err("out of range");
        assert!(
            error.to_string().starts_with("time.out_of_range"),
            "{error}"
        );
        let error = serde_json::from_str::<SimDuration>(&too_late).expect_err("out of range");
        assert!(
            error.to_string().starts_with("time.out_of_range"),
            "{error}"
        );
    }

    #[test]
    fn the_calendar_units_add_up() {
        assert_eq!(YEAR, WEEK * 52);
        assert_eq!(YEAR, MONTH * 13);
        assert_eq!(YEAR, SEASON * 4);
        assert_eq!(SEASON, WEEK * 13);
        assert_eq!(MAX_ABS_SECS, YEAR.as_secs() * 1_000_000);
    }

    #[test]
    fn the_range_is_one_million_years_on_both_sides() {
        assert_eq!(SimTime::from_secs(MAX_ABS_SECS).as_secs(), MAX_ABS_SECS);
        assert_eq!(SimTime::from_secs(-MAX_ABS_SECS).as_secs(), -MAX_ABS_SECS);
        assert!(
            SimTime::from_secs(MAX_ABS_SECS)
                .checked_add(SimDuration::from_secs(1))
                .is_none()
        );
        assert!(
            SimTime::from_secs(-MAX_ABS_SECS)
                .checked_sub(SimDuration::from_secs(1))
                .is_none()
        );
        assert!(
            SimDuration::from_secs(MAX_ABS_SECS)
                .checked_mul(2)
                .is_none()
        );
        assert!(SimDuration::from_secs(1).checked_mul(i64::MAX).is_none());
    }

    #[test]
    fn arithmetic_is_exact_inside_the_range() {
        let t = SimTime::START - YEAR * 30;
        assert_eq!(t.as_secs(), -30 * YEAR.as_secs());
        assert_eq!((t + YEAR) - t, YEAR);
        assert_eq!(DAY - HOUR * 24, SimDuration::ZERO);
    }

    #[test]
    #[should_panic(expected = "invariant time.out_of_range")]
    fn from_secs_panics_outside_the_range() {
        let _ = SimTime::from_secs(MAX_ABS_SECS + 1);
    }

    #[test]
    #[should_panic(expected = "invariant time.out_of_range")]
    fn operators_panic_outside_the_range() {
        let _ = SimTime::from_secs(MAX_ABS_SECS) + SimDuration::from_secs(1);
    }
}

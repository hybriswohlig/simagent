//! A need as an exact integer sample (ADR-0026 section 1): a value from 0 to 1000 at a second
//! and a rate in points per hour. The value at a later second and the second at which a
//! falling value reaches a threshold are integer arithmetic, so a day in the house is the same
//! on every target.

use crate::time::{SimDuration, SimTime};

/// A full need.
pub const FULL: u16 = 1000;

/// The levels at which a falling need is planned as an event, highest first (ADR-0026
/// section 1): could, low, critical, emergency.
pub const THRESHOLDS: [u16; 4] = [700, 400, 150, 50];

const HOUR_SECS: i128 = 3600;

/// One need: `value` at second `at`, changing by `rate` per hour until the next sample.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NeedSample {
    value: u16,
    at: SimTime,
    rate: i32,
}

impl NeedSample {
    /// Panics with `house.need_value` above `FULL`.
    #[track_caller]
    pub fn new(value: u16, at: SimTime, rate: i32) -> NeedSample {
        assert!(value <= FULL, "invariant house.need_value");
        NeedSample { value, at, rate }
    }

    pub fn value(self) -> u16 {
        self.value
    }

    pub fn at(self) -> SimTime {
        self.at
    }

    pub fn rate(self) -> i32 {
        self.rate
    }

    /// `clamp(value + rate · s / 3600, 0, FULL)` with `s` the seconds since `at` and the
    /// division truncating towards zero. In i128, so even a rate of `i32::MIN` over the whole
    /// time range cannot overflow; where i64 would not overflow the result is the same. Panics
    /// with `house.need_time` before `at`.
    #[track_caller]
    pub fn value_at(self, t: SimTime) -> u16 {
        let secs = (t - self.at).as_secs();
        assert!(secs >= 0, "invariant house.need_time");
        let moved = i128::from(self.rate) * i128::from(secs) / HOUR_SECS;
        let value = (i128::from(self.value) + moved).clamp(0, i128::from(FULL));
        u16::try_from(value).unwrap_or(FULL)
    }

    /// The sample from `t` on with a new rate: the value is written down at `t` first.
    #[track_caller]
    pub fn with_rate(self, t: SimTime, rate: i32) -> NeedSample {
        NeedSample {
            value: self.value_at(t),
            at: t,
            rate,
        }
    }

    /// The first second at which the value is at or below `level`:
    /// `at + ⌈3600 · (value − level) / |rate|⌉`. None if the value is already there or does
    /// not fall.
    pub fn crossing(self, level: u16) -> Option<SimTime> {
        if self.rate >= 0 || self.value <= level {
            return None;
        }
        let distance = i64::from(self.value - level) * 3600;
        let speed = i64::from(self.rate).abs();
        let secs = (distance + speed - 1) / speed;
        Some(self.at + SimDuration::from_secs(secs))
    }

    /// The crossings of every threshold below the value, earliest first.
    pub fn crossings(self) -> impl Iterator<Item = (u16, SimTime)> {
        THRESHOLDS
            .into_iter()
            .filter_map(move |level| self.crossing(level).map(|at| (level, at)))
    }
}

#[cfg(test)]
mod tests {
    // The expected values come from ADR-0026 section 1, by hand: the value
    // `clamp(value + rate · s / 3600, 0, 1000)` with the division truncating towards zero, and
    // the crossing `at + ⌈3600 · (value − level) / |rate|⌉`.

    use alloc::vec::Vec;

    use super::*;

    const fn t(secs: i64) -> SimTime {
        SimTime::from_secs(secs)
    }

    #[test]
    fn the_pin_of_the_adr() {
        // 1000 at −42 per hour is still 401 after 51 428 s and 400 after 51 429 s.
        let need = NeedSample::new(1000, SimTime::START, -42);
        assert_eq!(need.crossing(400), Some(t(51_429)));
        assert_eq!(need.value_at(t(51_428)), 401);
        assert_eq!(need.value_at(t(51_429)), 400);
    }

    #[test]
    fn exact_crossings_by_hand() {
        // −40: 300 down in exactly 27 000 s.
        let energy = NeedSample::new(1000, SimTime::START, -40);
        assert_eq!(energy.crossing(700), Some(t(27_000)));
        assert_eq!(energy.value_at(t(26_999)), 701);
        assert_eq!(energy.value_at(t(27_000)), 700);
        // −120 from 500 at 100 s: 100 down in 3 000 s, 350 down in 10 500 s.
        let bladder = NeedSample::new(500, t(100), -120);
        assert_eq!(bladder.crossing(400), Some(t(3_100)));
        assert_eq!(bladder.crossing(150), Some(t(10_600)));
        // At the threshold or under it there is no crossing; rising or still neither.
        assert_eq!(NeedSample::new(400, t(0), -10).crossing(400), None);
        assert_eq!(NeedSample::new(300, t(0), -10).crossing(400), None);
        assert_eq!(NeedSample::new(800, t(0), 0).crossing(400), None);
        assert_eq!(NeedSample::new(800, t(0), 100).crossing(400), None);
    }

    #[test]
    fn the_crossings_below_the_value() {
        // 800 at −100: 700 after 1 h, 400 after 4 h, 150 after 6.5 h, 50 after 7.5 h.
        let hunger = NeedSample::new(800, SimTime::START, -100);
        let all: Vec<(u16, SimTime)> = hunger.crossings().collect();
        assert_eq!(
            all,
            [
                (700, t(3_600)),
                (400, t(14_400)),
                (150, t(23_400)),
                (50, t(27_000))
            ]
        );
        // A value at 400 crosses only 150 and 50.
        let at_low = NeedSample::new(400, SimTime::START, -10);
        let levels: Vec<u16> = at_low.crossings().map(|(level, _)| level).collect();
        assert_eq!(levels, [150, 50]);
        assert_eq!(
            NeedSample::new(800, SimTime::START, 5).crossings().count(),
            0
        );
    }

    #[test]
    fn a_new_rate_writes_the_value_down_first() {
        // 800 at −100 for 1 800 s is 750; then +1 800 for 1 200 s would be 1 350, held at 1 000.
        let eating = NeedSample::new(800, SimTime::START, -100).with_rate(t(1_800), 1_800);
        assert_eq!(eating, NeedSample::new(750, t(1_800), 1_800));
        assert_eq!(eating.value_at(t(3_000)), 1000);
        assert_eq!(eating.crossing(400), None);
        // Falling below 0 holds at 0.
        assert_eq!(
            NeedSample::new(100, SimTime::START, -1_000).value_at(t(7_200)),
            0
        );
        // A whole year later the product does not overflow.
        assert_eq!(
            NeedSample::new(1000, SimTime::START, i32::MIN).value_at(t(31_449_600)),
            0
        );
        assert_eq!(
            NeedSample::new(0, SimTime::START, i32::MAX).value_at(t(31_449_600)),
            1000
        );
    }

    #[test]
    fn every_crossing_is_the_first_second_at_or_below() {
        // The defining property over a grid of values and falling rates: at the crossing the
        // value is at or below the level, one second earlier still above it.
        for value in (0..=1000u16).step_by(37) {
            for magnitude in (1..=13_000i32).step_by(331) {
                let need = NeedSample::new(value, t(5), -magnitude);
                for level in THRESHOLDS {
                    match need.crossing(level) {
                        Some(at) => {
                            assert!(value > level);
                            assert!(need.value_at(at) <= level, "{value} {magnitude} {level}");
                            assert!(need.value_at(at - SimDuration::from_secs(1)) > level);
                        }
                        None => assert!(value <= level),
                    }
                }
            }
        }
    }

    #[test]
    #[should_panic(expected = "invariant house.need_value")]
    fn a_value_above_full_breaks_an_invariant() {
        let _ = NeedSample::new(1001, SimTime::START, 0);
    }

    #[test]
    #[should_panic(expected = "invariant house.need_time")]
    fn a_second_before_the_sample_breaks_an_invariant() {
        let _ = NeedSample::new(500, t(10), -10).value_at(t(9));
    }
}

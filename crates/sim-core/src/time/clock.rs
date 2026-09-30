//! The only two conversions between simulation time and years as `f64` (ADR-0002).
//! Formulas (Gompertz, growth curves) think in years; the rest of the core counts
//! seconds. The year length as a literal lives in this file only.

use super::SimDuration;

/// Seconds per year (364 days) as `f64`; a test pins it to `YEAR`.
const YEAR_SECONDS: f64 = 31_449_600.0;

/// Durations of this many years or more mean "never": `from_years_ceil` returns `None`
/// and the caller schedules nothing. Leaves room below the time range so that
/// `reference + duration` stays in range for any reference below 900 000 years.
pub const NEVER_YEARS: f64 = 100_000.0;

impl SimDuration {
    /// The duration in years (exact seconds divided by the year length).
    pub fn as_years(self) -> f64 {
        let _ = YEAR_SECONDS;
        0.0
    }

    /// Whole seconds for `years`, always rounded up, so a computed point in time never
    /// lies before the value computed in `f64`. `None` means "never" (at least
    /// `NEVER_YEARS`). NaN or infinity is the invariant break `time.non_finite`, at most
    /// `-NEVER_YEARS` is `time.out_of_range`.
    pub fn from_years_ceil(_years: f64) -> Option<SimDuration> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::super::{DAY, SimDuration, YEAR};
    use super::*;

    fn secs(s: i64) -> SimDuration {
        SimDuration::from_secs(s)
    }

    #[test]
    fn the_year_literal_matches_the_calendar() {
        assert_eq!(
            sim_math::i64_to_f64_exact(YEAR.as_secs()),
            Some(YEAR_SECONDS)
        );
    }

    #[test]
    fn as_years_is_exact_on_whole_days() {
        assert_eq!(YEAR.as_years(), 1.0);
        assert_eq!((DAY * 182).as_years(), 0.5);
        assert_eq!((YEAR * -30).as_years(), -30.0);
        assert_eq!(SimDuration::ZERO.as_years(), 0.0);
    }

    #[test]
    fn from_years_rounds_up_to_whole_seconds() {
        assert_eq!(SimDuration::from_years_ceil(1.0), Some(YEAR));
        assert_eq!(SimDuration::from_years_ceil(0.5), Some(DAY * 182));
        assert_eq!(SimDuration::from_years_ceil(1e-9), Some(secs(1)));
        assert_eq!(SimDuration::from_years_ceil(-1e-9), Some(SimDuration::ZERO));
        assert_eq!(SimDuration::from_years_ceil(-1.0), Some(YEAR * -1));
        assert_eq!(SimDuration::from_years_ceil(-0.0), Some(SimDuration::ZERO));
    }

    #[test]
    fn never_is_none() {
        assert_eq!(SimDuration::from_years_ceil(NEVER_YEARS), None);
        assert_eq!(SimDuration::from_years_ceil(f64::MAX), None);
        assert!(SimDuration::from_years_ceil(99_999.0).is_some());
        assert!(SimDuration::from_years_ceil(-99_999.0).is_some());
    }

    #[test]
    #[should_panic(expected = "invariant time.non_finite")]
    fn nan_is_an_invariant_break() {
        let _ = SimDuration::from_years_ceil(f64::NAN);
    }

    #[test]
    #[should_panic(expected = "invariant time.non_finite")]
    fn infinity_is_an_invariant_break() {
        let _ = SimDuration::from_years_ceil(f64::INFINITY);
    }

    #[test]
    #[should_panic(expected = "invariant time.non_finite")]
    fn negative_infinity_is_an_invariant_break() {
        let _ = SimDuration::from_years_ceil(f64::NEG_INFINITY);
    }

    #[test]
    #[should_panic(expected = "invariant time.out_of_range")]
    fn minus_never_is_out_of_range() {
        let _ = SimDuration::from_years_ceil(-NEVER_YEARS);
    }

    /// For |d| below `NEVER_YEARS` the round trip gives `d` or `d + 1 s`, never less.
    #[test]
    fn the_round_trip_never_goes_back() {
        let limit = YEAR.as_secs() * 99_999;
        let mut samples = std::vec![0, 1, -1, 59, YEAR.as_secs(), -YEAR.as_secs(), limit, -limit];
        let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
        for _ in 0..20_000 {
            state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^= z >> 31;
            let span = u64::try_from(2 * limit + 1).expect("positive span");
            let offset = i64::try_from(z % span).expect("fits");
            samples.push(offset - limit);
        }
        for s in samples {
            let d = secs(s);
            let back = SimDuration::from_years_ceil(d.as_years()).expect("below never");
            assert!(
                back == d || back == d + secs(1),
                "round trip of {s} s gave {} s",
                back.as_secs()
            );
        }
    }
}

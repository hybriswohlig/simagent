#[cfg(test)]
mod tests {
    // The expected values come from ADR-0026 section 1, by hand: the value
    // `clamp(value + rate · s / 3600, 0, 1000)` with the division truncating towards zero, and
    // the crossing `at + ⌈3600 · (value − level) / |rate|⌉`.

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
        assert_eq!(all, [(700, t(3_600)), (400, t(14_400)), (150, t(23_400)), (50, t(27_000))]);
    // A value at 400 crosses only 150 and 50.
        let at_low = NeedSample::new(400, SimTime::START, -10);
        let levels: Vec<u16> = at_low.crossings().map(|(level, _)| level).collect();
        assert_eq!(levels, [150, 50]);
        assert_eq!(NeedSample::new(800, SimTime::START, 5).crossings().count(), 0);
    }

    #[test]
    fn a_new_rate_writes_the_value_down_first() {
    // 800 at −100 for 1 800 s is 750; then +1 800 for 1 200 s would be 1 350, held at 1 000.
        let eating = NeedSample::new(800, SimTime::START, -100).with_rate(t(1_800), 1_800);
        assert_eq!(eating, NeedSample::new(750, t(1_800), 1_800));
        assert_eq!(eating.value_at(t(3_000)), 1000);
        assert_eq!(eating.crossing(400), None);
    // Falling below 0 holds at 0.
        assert_eq!(NeedSample::new(100, SimTime::START, -1_000).value_at(t(7_200)), 0);
    // A whole year later the product does not overflow.
        assert_eq!(NeedSample::new(1000, SimTime::START, i32::MIN).value_at(t(31_449_600)), 0);
        assert_eq!(NeedSample::new(0, SimTime::START, i32::MAX).value_at(t(31_449_600)), 1000);
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

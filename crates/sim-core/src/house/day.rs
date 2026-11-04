//! A person's duty of a day (ADR-0024 section 4): school or work on a weekday by the age at
//! the day's plan at 5:00 and, for work, by the person's work plan, a draw once in its life.
//! Pure functions of the day's rules, so the handlers, the loader and the page agree.

use crate::house::data::DayRules;
use crate::house::state::Duty;
use crate::lineage::LineageKey;
use crate::rng::{DrawKey, Draws, SubjectKey, WorldSeed, tags};
use crate::time::{DAY, SimDuration, SimTime, YEAR, calendar};

/// The weekdays 0 to 4 have duties (ADR-0024 section 4).
const WEEKDAYS: u16 = 5;

/// A weekday's duty: where to, when the person wakes, leaves and is back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DayDuty {
    pub place: Duty,
    pub wake: SimTime,
    pub depart: SimTime,
    pub back: SimTime,
}

/// The work plan of the person with this lineage key (ADR-0024 section 4, ADR-0001 section 5):
/// `uniform_below(100)` of `WORK_PLAN` with the world seed, occasion 0, detail 0.
pub fn work_draw(seed: u64, key: LineageKey) -> u64 {
    let address = DrawKey {
        tag: tags::WORK_PLAN,
        subject: SubjectKey::from(key),
        occasion: 0,
        detail: 0,
    };
    Draws::new(WorldSeed::new(seed)).uniform_below(address, 100)
}

/// The second `second` of the day with the index `day`; `None` beyond the end of time.
fn of_day(day: i64, second: u32) -> Option<SimTime> {
    let start = DAY.checked_mul(day)?.as_secs();
    SimTime::try_from(start.checked_add(i64::from(second))?).ok()
}

/// The first plan of a day after `after` (ADR-0024 sections 1 and 2): a series set up in a
/// second begins with the next day's plan, unless the plan of that second's day is still to
/// come.
pub fn next_plan(rules: &DayRules, after: SimTime) -> Option<SimTime> {
    let since = i64::from(calendar(after).second_of_day);
    let wait = (i64::from(rules.day_plan_at) - since - 1).rem_euclid(DAY.as_secs()) + 1;
    after.checked_add(SimDuration::from_secs(wait))
}

/// The day whose plan is the last at or before `at`; its duties are those still to come.
pub fn plan_day(rules: &DayRules, at: SimTime) -> i64 {
    let since = at.as_secs() - i64::from(rules.day_plan_at);
    since.div_euclid(DAY.as_secs())
}

/// The duty of the day `day` of a person born at `born`: school from `school.from_age` to the
/// day before the birthday of `school.until_age`, work in the same way by `draw` (asked only
/// then); the age counts at the day's plan. Nothing on the weekend, at other ages, at home by
/// the draw, or beyond the end of time.
pub fn duty(
    rules: &DayRules,
    day: i64,
    born: SimTime,
    draw: impl FnOnce() -> u64,
) -> Option<DayDuty> {
    let plan = of_day(day, rules.day_plan_at)?;
    if calendar(plan).weekday >= WEEKDAYS {
        return None;
    }
    let years = (plan.as_secs() - born.as_secs()).div_euclid(YEAR.as_secs());
    let (school, work) = (&rules.school, &rules.work);
    let between = |from: u8, until: u8| (i64::from(from)..i64::from(until)).contains(&years);
    let (place, hours) = if between(school.from_age, school.until_age) {
        (Duty::School, (school.depart, school.back))
    } else if between(work.from_age, work.until_age) {
        let drawn = draw();
        if drawn < u64::from(work.full_time_below) {
            (Duty::Work, (work.full_time.depart, work.full_time.back))
        } else if drawn < u64::from(work.part_time_below) {
            (Duty::Work, (work.part_time.depart, work.part_time.back))
        } else {
            return None;
        }
    } else {
        return None;
    };
    // The data put the wake after the plan and before the departure.
    let wake = hours.0.checked_sub(rules.wake_before)?;
    Some(DayDuty {
        place,
        wake: of_day(day, wake)?,
        depart: of_day(day, hours.0)?,
        back: of_day(day, hours.1)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> DayRules {
        serde_json::from_str(include_str!("../../../../data/house/day.json")).expect("the day")
    }

    fn t(secs: i64) -> SimTime {
        SimTime::from_secs(secs)
    }

    /// 5:00 is the first plan after any second before it; at and after 5:00 the next day's.
    #[test]
    fn the_next_plan_comes_after_the_second() {
        let day = DAY.as_secs();
        let rules = rules();
        assert_eq!(next_plan(&rules, t(0)), Some(t(18_000)));
        assert_eq!(next_plan(&rules, t(17_999)), Some(t(18_000)));
        assert_eq!(next_plan(&rules, t(18_000)), Some(t(day + 18_000)));
        assert_eq!(next_plan(&rules, t(day - 1)), Some(t(day + 18_000)));
        assert_eq!(next_plan(&rules, t(-1)), Some(t(18_000)));
        assert_eq!(plan_day(&rules, t(17_999)), -1);
        assert_eq!(plan_day(&rules, t(18_000)), 0);
        assert_eq!(plan_day(&rules, t(day + 17_999)), 0);
    }

    /// A child under 6 by the clock, at the edges of `day.json`: the kindergarten from 8:00 to
    /// before 15:00 on Monday (day 0) to Friday (day 4), bed from 19:00 to before 7:00 every
    /// night, else the day room. Day -3 is a Friday, day -2 a Saturday, day -1 a Sunday.
    #[test]
    fn a_young_child_by_the_clock() {
        let day = DAY.as_secs();
        let rules = rules();
        let kita = |until: i64| YoungPlace::Kita { until: t(until) };
        let cases = [
            (28_799, YoungPlace::DayRoom),
            (28_800, kita(54_000)),
            (53_999, kita(54_000)),
            (54_000, YoungPlace::DayRoom),
            (68_399, YoungPlace::DayRoom),
            (68_400, YoungPlace::Bed),
            (day + 25_199, YoungPlace::Bed),
            (day + 25_200, YoungPlace::DayRoom),
            (4 * day + 28_800, kita(4 * day + 54_000)),
            (4 * day + 86_399, YoungPlace::Bed),
            (5 * day + 25_199, YoungPlace::Bed),
            (5 * day + 28_800, YoungPlace::DayRoom),
            (5 * day + 53_999, YoungPlace::DayRoom),
            (5 * day + 68_400, YoungPlace::Bed),
            (-1, YoungPlace::Bed),
            (-3 * day + 32_400, kita(-3 * day + 54_000)),
            (-2 * day + 36_000, YoungPlace::DayRoom),
        ];
        for (at, place) in cases {
            assert_eq!(young_place(&rules, t(at)), place, "{at}");
        }
    }
}

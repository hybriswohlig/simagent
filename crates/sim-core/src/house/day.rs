//! A person's duty of a day (ADR-0024 section 4): school or work on a weekday by the age at
//! the day's plan at 5:00 and, for work, by the person's work plan, a draw once in its life;
//! its night (ADR-0026, addendum of 2026-11-04) by the same age; and where a child under 6
//! is. Pure functions of the day's rules, so the handlers, the loader and the page agree.

use crate::house::data::{DayRules, age_band};
use crate::house::state::Duty;
use crate::lineage::LineageKey;
use crate::rng::{DrawKey, Draws, SubjectKey, WorldSeed, tags};
use crate::time::{DAY, MAX_ABS_SECS, SimDuration, SimTime, YEAR, calendar, day_index};

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

/// The departure still to come after `at` of a person born at `born`: that of its duty of the
/// day whose plan came last at or before `at`, if it lies after `at` (ADR-0024 section 4); the
/// work plan's draw `draw` asked only at working age. What closes an offer that would run into it
/// (ADR-0026, addendum of 2026-11-04 on comfort and the time-filler).
pub fn departure(
    rules: &DayRules,
    born: SimTime,
    draw: impl FnOnce() -> u64,
    at: SimTime,
) -> Option<SimTime> {
    let today = duty(rules, plan_day(rules, at), born, draw)?;
    (today.depart > at).then_some(today.depart)
}

/// A person's night (ADR-0026, addendum of 2026-11-04): from `from` included to `until`
/// excluded; `until` is `None` for the last night of time, whose end lies beyond the end of
/// time: it lasts to the end, its last second included, and a sleep in it has no end, as an
/// action that would end beyond time has none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Night {
    pub from: SimTime,
    pub until: Option<SimTime>,
}

impl Night {
    /// Whether `at` lies in the night.
    pub fn holds(&self, at: SimTime) -> bool {
        self.from <= at && self.until.is_none_or(|until| at < until)
    }

    /// The seconds of the night from `at` on, `at` included: to its end, for the last night of
    /// time to the end of time.
    pub fn left(&self, at: SimTime) -> i64 {
        match self.until {
            Some(until) => (until - at).as_secs(),
            None => MAX_ABS_SECS + 1 - at.as_secs(),
        }
    }
}

/// The night of the day `day` of a person born at `born`: from `night.from` of that day to
/// `night.until` of the next, or to `night.until_weekend` when the next is a Saturday or a
/// Sunday (weekday 5 or 6; ADR-0026, addendum of 2026-11-04 after the playtest), of the age
/// band at the day's plan (as its duty). `None` at an age without needs (under 6, above
/// `OLDEST`) or if the day's plan or the night's start lies beyond the end of time; a night that
/// would end beyond it ends with time. Time begins outside every night: a night before the
/// first second has no plan in time, and nobody is 6 there, as nobody is born before it.
pub fn night_of(rules: &DayRules, day: i64, born: SimTime) -> Option<Night> {
    let plan = of_day(day, rules.day_plan_at)?;
    let years = (plan.as_secs() - born.as_secs()).div_euclid(YEAR.as_secs());
    let band = u8::try_from(years).ok().and_then(age_band)?;
    let until = |next: i64| {
        let morning = of_day(next, 0)?;
        let ends = match calendar(morning).weekday >= WEEKDAYS {
            true => rules.night.until_weekend[band],
            false => rules.night.until[band],
        };
        of_day(next, ends)
    };
    Some(Night {
        from: of_day(day, rules.night.from[band])?,
        until: day.checked_add(1).and_then(until),
    })
}

/// The night `at` lies in for a person born at `born`: that of its day once it has begun, else
/// that of the day before until it ends; `None` between two nights. The data put every end
/// before every start, so no second lies in two.
pub fn night(rules: &DayRules, born: SimTime, at: SimTime) -> Option<Night> {
    let day = day_index(at);
    let inside = |night: &Night| night.holds(at);
    let today = night_of(rules, day, born).filter(inside);
    today.or_else(|| night_of(rules, day.checked_sub(1)?, born).filter(inside))
}

/// The night that follows `at` for a person born at `born`: that of its day if it has not
/// begun, else that of the next day.
pub fn next_night(rules: &DayRules, born: SimTime, at: SimTime) -> Option<Night> {
    let day = day_index(at);
    let today = night_of(rules, day, born).filter(|night| night.from > at);
    today.or_else(|| night_of(rules, day.checked_add(1)?, born))
}

/// Where a child under 6 is (ADR-0024 section 4): at the kindergarten until the second it
/// ends, in bed in the house's `child_night_room`, or in its `child_day_room`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YoungPlace {
    Kita { until: SimTime },
    Bed,
    DayRoom,
}

/// Where a child under 6 is at `at`, a pure function of the second (ADR-0024 sections 2 and
/// 4 and its addendum of 2026-11-04, slice 8c): on a weekday from `kita_from` to before
/// `kita_to` at the kindergarten, every night from `bed_from` to before `bed_to` of the next
/// morning in bed, else in the day room. The loader keeps the bed's end before the
/// kindergarten and its start after it. Who is a child is the caller's: in a house, whoever
/// has no needs.
pub fn young_place(rules: &DayRules, at: SimTime) -> YoungPlace {
    let young = &rules.young;
    let date = calendar(at);
    let second = date.second_of_day;
    if date.weekday < WEEKDAYS && (young.kita_from..young.kita_to).contains(&second) {
        // The end of time is a midnight, so the end of a kindergarten day that began is in it.
        let left = SimDuration::from_secs(i64::from(young.kita_to - second));
        return YoungPlace::Kita { until: at + left };
    }
    if second >= young.bed_from || second < young.bed_to {
        return YoungPlace::Bed;
    }
    YoungPlace::DayRoom
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

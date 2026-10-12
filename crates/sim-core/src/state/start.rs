//! The input `Start` (ADR-0004 section 4): the resolved scenario, only as the first
//! input and only before the clock moves.

use alloc::vec::Vec;

use super::{DomainState, Sim, Slot, death, disease, household, partnership, years_between};
use crate::data::Data;
use crate::genome::{AlleleError, Genome};
use crate::ids::HouseholdId;
use crate::input::{Founder, InputRejected, RejectCode, Start};
use crate::lineage::LineageKey;
use crate::log::{Causes, LogPayload};
use crate::params::NutritionYear;
use crate::time::{MONTH, SimDuration, SimTime, YEAR, year_start};
use crate::trace::TracePayload;
use crate::world::{EventPayload, SimCtx};

pub(crate) fn check_start(
    domain: &DomainState,
    data: &Data,
    closed_through: SimTime,
    start: &Start,
) -> Result<(), InputRejected> {
    let rejection = start_rejection(
        domain.world_seed.is_some(),
        closed_through,
        start.founders.len(),
        start.founders.iter().any(|f| f.born >= SimTime::START),
    )
    .or_else(|| alleles_rejection(&start.founders, data))
    .or_else(|| {
        let age = super::market::market_age(data);
        partnership::couples_rejection(&start.founders, &start.couples, age)
    })
    .or_else(|| nutrition_rejection(&start.world_params.nutrition))
    .or_else(|| (start.capacity == Some(0)).then_some(RejectCode::CapacityZero));
    match rejection {
        Some(code) => Err(InputRejected { code }),
        None => Ok(()),
    }
}

/// Why a `Start` with these facts is rejected, if at all. A function of plain facts,
/// so the founder limit is testable without building a list of that size.
fn start_rejection(
    started: bool,
    closed_through: SimTime,
    founders: usize,
    founder_not_born: bool,
) -> Option<RejectCode> {
    if started {
        return Some(RejectCode::AlreadyStarted);
    }
    // The year end series begins in year 0; after the clock moved it could lie in the
    // past, and an input must never break an invariant.
    if closed_through != SimTime::START - SimDuration::from_secs(1) {
        return Some(RejectCode::ClockNotAtStart);
    }
    if founder_count(founders).is_none() {
        return Some(RejectCode::TooManyFounders);
    }
    if founder_not_born {
        return Some(RejectCode::FounderNotBornBeforeStart);
    }
    None
}

/// Why the nutrition index of `Start` is rejected, if at all (ADR-0013 section 1): its
/// years do not rise strictly, or a year or an index is out of range.
fn nutrition_rejection(nutrition: &[NutritionYear]) -> Option<RejectCode> {
    if nutrition.windows(2).any(|pair| match pair {
        [before, after] => before.year >= after.year,
        _ => false,
    }) {
        return Some(RejectCode::NutritionOrder);
    }
    nutrition
        .iter()
        .any(|entry| !entry.in_range())
        .then_some(RejectCode::NutritionOutOfRange)
}

/// Why the alleles of these founders are rejected, if at all (genetics specification
/// 2.1): the first founder whose alleles do not fit the genome map names the code.
fn alleles_rejection(founders: &[Founder], data: &Data) -> Option<RejectCode> {
    founders.iter().find_map(|founder| {
        Genome::with_alleles(founder.sex, &founder.alleles, data)
            .err()
            .map(|error| match error {
                AlleleError::UnknownLocus => RejectCode::FounderUnknownLocus,
                AlleleError::LocusTwice => RejectCode::FounderLocusTwice,
                AlleleError::Copies => RejectCode::FounderCopies,
            })
    })
}

/// The number of founders as it stands in `core:started`.
fn founder_count(founders: usize) -> Option<u32> {
    u32::try_from(founders).ok()
}

/// Writes `core:started` first: it is the `by` of the system series (ADR-0004 section
/// 7). Then one creation entry per founder, in list order; the position in the list
/// is the founder's index in its lineage key (ADR-0001 section 4). Per founder the first
/// birthday is planned, then the deaths in the order of the causes; the causes act on a
/// founder from the start on, a lethal disease from its onset and only with the switch
/// on. Then the couples are paired, in list order.
pub(crate) fn apply_start(
    domain: &mut DomainState,
    data: &Data,
    start: &Start,
    ctx: &mut SimCtx<'_, '_>,
) {
    domain.world_seed = Some(start.world_seed);
    domain.world_params = Some(start.world_params.clone());
    let founders = founder_count(start.founders.len())
        .unwrap_or_else(|| panic!("invariant start.founder_count_range"));
    let started = ctx.emit(
        LogPayload::Started { founders },
        Vec::new(),
        Causes::new(ctx.trigger()),
    );
    ctx.schedule_periodic(
        year_start(1) - SimDuration::from_secs(1),
        YEAR,
        EventPayload::YearEnd,
        started,
    );
    // The rounds of the partner market, from the first month start after the start
    // (ADR-0018 section 1).
    ctx.schedule_periodic(
        SimTime::START + MONTH,
        MONTH,
        EventPayload::MarketRound,
        started,
    );
    // The soft capacity, in the last second of each year after its births and deaths
    // (ADR-0020 section 6).
    if let Some(capacity) = start.capacity {
        ctx.schedule_periodic(
            year_start(1) - SimDuration::from_secs(1),
            YEAR,
            EventPayload::Emigration { capacity },
            started,
        );
    }
    let mut ids = Vec::with_capacity(start.founders.len());
    for (index, founder) in (0_u64..).zip(&start.founders) {
        let born = founder.born;
        let genome = Genome::with_alleles(founder.sex, &founder.alleles, data)
            .unwrap_or_else(|_| panic!("invariant start.founder_alleles"));
        let lineage_key = LineageKey::founder(&start.founder_tag, index);
        domain.lineage.insert(lineage_key);
        let (id, added) = ctx.create_sim(
            LogPayload::FounderAdded { born, lineage_key },
            Causes::new(ctx.trigger()),
        );
        domain.sims.insert(
            id,
            Sim {
                born,
                lineage_key,
                sex: founder.sex,
                genome,
                afflictions: disease::afflictions(
                    data,
                    start.world_seed,
                    (lineage_key, None),
                    founder.sex,
                    &genome,
                    start.world_params.hereditary_diseases,
                    SimTime::START,
                ),
                conceptions: 0,
                births: 0,
                partnerships: 0,
                // The households come after the founders and the couples
                // (`household::start_households`), which sets each founder's.
                household: HouseholdId::try_from(1)
                    .unwrap_or_else(|_| panic!("invariant start.household")),
                moved_out: false,
                carer: None,
                parents: None,
                social_parents: Vec::new(),
            },
        );
        let next = first_birthday(born, ctx.now());
        let handle = ctx.schedule(next, EventPayload::Birthday { sim: id }, added);
        domain.pending.insert((id, Slot::Birthday), handle);
        ctx.trace(id, || TracePayload::BirthdayPlanned { next });
        death::plan_deaths(domain, data, id, added, ctx);
        ids.push((id, added));
    }
    let place = |place: u32| {
        usize::try_from(place)
            .ok()
            .filter(|place| ids.get(*place).is_some())
            .unwrap_or_else(|| panic!("invariant start.couple_place"))
    };
    let couples: Vec<(usize, usize)> = start
        .couples
        .iter()
        .map(|couple| (place(couple.woman), place(couple.man)))
        .collect();
    for &(woman, man) in &couples {
        partnership::pair_up(domain, data, ids[woman].0, ids[man].0, ctx);
    }
    household::start_households(domain, data, &ids, &couples, ctx);
}

/// The first anniversary of `born` at or after `now` (`born <= now`).
pub(crate) fn first_birthday(born: SimTime, now: SimTime) -> SimTime {
    let secs = next_anniversary_secs(born, now.as_secs())
        .unwrap_or_else(|| panic!("invariant age.out_of_range"));
    SimTime::from_secs(secs)
}

/// The second of the first anniversary of `born` at or after `now_secs`, `None` if
/// `born` lies after it. In plain seconds: the span can be longer than the longest
/// `SimDuration`, and neither `now_secs` nor the result need lie in the time range (the
/// loader compares seconds, it builds no time and must not panic).
pub(crate) fn next_anniversary_secs(born: SimTime, now_secs: i64) -> Option<i64> {
    let full = years_between(born.as_secs(), now_secs)?;
    let after = |years: u32| born.as_secs() + i64::from(years) * YEAR.as_secs();
    let on_the_day = after(full) == now_secs;
    let age = if on_the_day && full >= 1 {
        full
    } else {
        full + 1
    };
    Some(after(age))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::DAY;

    #[test]
    fn founder_count_known_answers() {
        let limit = usize::try_from(u32::MAX).expect("64-bit test host");
        assert_eq!(founder_count(0), Some(0));
        assert_eq!(founder_count(3), Some(3));
        assert_eq!(founder_count(limit), Some(u32::MAX));
        assert_eq!(founder_count(limit + 1), None);
        assert_eq!(founder_count(usize::MAX), None);
    }

    /// Every rejection of `Start` and their order: the state of the world first, then
    /// the input itself.
    #[test]
    fn start_rejections_and_their_order() {
        let at_start = SimTime::from_secs(-1);
        let moved = SimTime::START;
        let limit = usize::try_from(u32::MAX).expect("64-bit test host");
        let cases = [
            (false, at_start, 3, false, None),
            (false, at_start, 0, false, None),
            (false, at_start, limit, false, None),
            (true, at_start, 3, false, Some(RejectCode::AlreadyStarted)),
            (false, moved, 3, false, Some(RejectCode::ClockNotAtStart)),
            (
                false,
                at_start,
                limit + 1,
                false,
                Some(RejectCode::TooManyFounders),
            ),
            (
                false,
                at_start,
                3,
                true,
                Some(RejectCode::FounderNotBornBeforeStart),
            ),
            (
                true,
                moved,
                limit + 1,
                true,
                Some(RejectCode::AlreadyStarted),
            ),
            (
                false,
                moved,
                limit + 1,
                true,
                Some(RejectCode::ClockNotAtStart),
            ),
            (
                false,
                at_start,
                limit + 1,
                true,
                Some(RejectCode::TooManyFounders),
            ),
        ];
        for (started, closed_through, founders, not_born, expected) in cases {
            assert_eq!(
                start_rejection(started, closed_through, founders, not_born),
                expected,
                "started {started}, closed {closed_through:?}, {founders} founders, unborn {not_born}"
            );
        }
    }

    #[test]
    fn no_anniversary_before_the_birth() {
        assert_eq!(next_anniversary_secs(SimTime::from_secs(5), 4), None);
        let year = YEAR.as_secs();
        assert_eq!(
            next_anniversary_secs(SimTime::from_secs(5), 5),
            Some(5 + year)
        );
        // The second after the end of the time range is no valid time and still a
        // valid `now` here.
        let end = crate::time::MAX_ABS_SECS;
        assert_eq!(
            next_anniversary_secs(SimTime::from_secs(end), end + 1),
            Some(end + year)
        );
    }

    /// By hand: the smallest `born + n * YEAR` with `n >= 1` that is not before `now`.
    #[test]
    fn first_birthday_known_answers() {
        let year = YEAR.as_secs();
        let cases = [
            // (born, now, first birthday)
            (-30 * year, 0, 0),
            (-year, 0, 0),
            (-year - 1, 0, year - 1),
            (-year + 1, 0, 1),
            (-1, 0, year - 1),
            (-25 * year - 100 * DAY.as_secs(), 0, 22_809_600),
            (10, 11, 10 + year),
            (10, 10 + 3 * year, 10 + 3 * year),
            (10, 11 + 3 * year, 10 + 4 * year),
            (0, 0, year),
            // The longest span of the time range: 2 000 000 years less 5 seconds.
            (-1_000_000 * year, 1_000_000 * year - 5, 1_000_000 * year),
        ];
        for (born, now, expected) in cases {
            assert_eq!(
                first_birthday(SimTime::from_secs(born), SimTime::from_secs(now)),
                SimTime::from_secs(expected),
                "born {born}, now {now}"
            );
        }
    }
}

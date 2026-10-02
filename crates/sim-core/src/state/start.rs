//! The input `Start` (ADR-0004 section 4): the resolved scenario, only as the first
//! input and only before the clock moves.

use alloc::vec::Vec;

use super::{DomainState, Sim, Slot, death, years_between};
use crate::input::{DeathCause, InputRejected, MIN_DEATH_RATE_PER_YEAR, RejectCode, Start};
use crate::lineage::LineageKey;
use crate::log::{Causes, LogPayload};
use crate::time::{SimDuration, SimTime, YEAR, year_start};
use crate::trace::TracePayload;
use crate::world::{EventPayload, SimCtx};

pub(crate) fn check_start(
    domain: &DomainState,
    closed_through: SimTime,
    start: &Start,
) -> Result<(), InputRejected> {
    let rejection = start_rejection(
        domain.world_seed.is_some(),
        closed_through,
        start.founders.len(),
        start.founders.iter().any(|f| f.born >= SimTime::START),
    )
    .or_else(|| death_causes_rejection(&start.death_causes));
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

/// Why these causes of death are rejected, if at all: two with the same key, or a hazard
/// so small that the years until death would not be a finite number.
pub(crate) fn death_causes_rejection(causes: &[DeathCause]) -> Option<RejectCode> {
    let twice = causes.iter().enumerate().any(|(index, cause)| {
        causes[..index]
            .iter()
            .any(|earlier| earlier.cause.key() == cause.cause.key())
    });
    let too_small = |cause: &DeathCause| cause.per_year.get() < MIN_DEATH_RATE_PER_YEAR;
    let _ = (twice, causes.iter().any(too_small));
    let _ = (RejectCode::DeathCauseTwice, RejectCode::DeathRateOutOfRange);
    None
}

/// The number of founders as it stands in `core:started`.
fn founder_count(founders: usize) -> Option<u32> {
    u32::try_from(founders).ok()
}

/// Writes `core:started` first: it is the `by` of the system series (ADR-0004 section
/// 7). Then one creation entry per founder, in list order; the position in the list
/// is the founder's index in its lineage key (ADR-0001 section 4). Per founder the first
/// birthday is planned, then the deaths in the order of the causes; the causes act on a
/// founder from the start on.
pub(crate) fn apply_start(domain: &mut DomainState, start: &Start, ctx: &mut SimCtx<'_, '_>) {
    domain.world_seed = Some(start.world_seed);
    domain.world_params = Some(start.world_params);
    domain.death_causes.clone_from(&start.death_causes);
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
    for (index, founder) in (0_u64..).zip(&start.founders) {
        let born = founder.born;
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
                genotype: founder.genotype,
            },
        );
        let next = first_birthday(born, ctx.now());
        let handle = ctx.schedule(next, EventPayload::Birthday { sim: id }, added);
        domain.pending.insert((id, Slot::Birthday), handle);
        ctx.trace(id, || TracePayload::BirthdayPlanned { next });
        death::plan_deaths(domain, id, lineage_key, SimTime::START, added, ctx);
    }
}

/// The first anniversary of `born` at or after `now` (`born <= now`).
fn first_birthday(born: SimTime, now: SimTime) -> SimTime {
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

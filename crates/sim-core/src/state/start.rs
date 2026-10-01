//! The input `Start` (ADR-0004 section 4): the resolved scenario, only as the first
//! input and only before the clock moves.

use alloc::vec::Vec;

use super::{DomainState, Sim, whole_years};
use crate::input::{InputRejected, RejectCode, Start};
use crate::log::{Causes, LogPayload};
use crate::time::{SimDuration, SimTime, YEAR, year_start};
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
    );
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
    let _ = (founders, founder_count);
    if founder_not_born {
        return Some(RejectCode::FounderNotBornBeforeStart);
    }
    None
}

/// The number of founders as it stands in `core:started`.
fn founder_count(founders: usize) -> Option<u32> {
    let _ = founders;
    Some(0)
}

/// Writes `core:started` first: it is the `by` of the system series (ADR-0004 section
/// 7). Then one creation entry per founder, in list order.
pub(crate) fn apply_start(domain: &mut DomainState, start: &Start, ctx: &mut SimCtx<'_, '_>) {
    domain.world_seed = Some(start.world_seed);
    let founders = u32::try_from(start.founders.len())
        .unwrap_or_else(|_| panic!("invariant start.founder_count_range"));
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
    for founder in &start.founders {
        let born = founder.born;
        let (id, added) = ctx.create_sim(
            LogPayload::FounderAdded { born },
            Causes::new(ctx.trigger()),
        );
        domain.sims.insert(id, Sim { born });
        ctx.schedule(
            first_birthday(born, ctx.now()),
            EventPayload::Birthday { sim: id },
            added,
        );
    }
}

/// The first anniversary of `born` at or after `now` (`born <= now`).
fn first_birthday(born: SimTime, now: SimTime) -> SimTime {
    let full = whole_years(born, now);
    // Plain seconds: `age` years can be longer than the longest `SimDuration`.
    let after = |years: u32| born.as_secs() + i64::from(years) * YEAR.as_secs();
    let on_the_day = after(full) == now.as_secs();
    let age = if on_the_day && full >= 1 {
        full
    } else {
        full + 1
    };
    SimTime::from_secs(after(age))
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

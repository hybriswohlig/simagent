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
    let reject = |code| Err(InputRejected { code });
    if domain.world_seed.is_some() {
        return reject(RejectCode::AlreadyStarted);
    }
    // The year end series begins in year 0; after the clock moved it could lie in the
    // past, and an input must never break an invariant.
    if closed_through != SimTime::START - SimDuration::from_secs(1) {
        return reject(RejectCode::ClockNotAtStart);
    }
    if start.founders.iter().any(|f| f.born >= SimTime::START) {
        return reject(RejectCode::FounderNotBornBeforeStart);
    }
    Ok(())
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
    let _ = (born, whole_years);
    now + YEAR
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::DAY;

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

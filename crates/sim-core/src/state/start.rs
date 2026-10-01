//! The input `Start` (ADR-0004 section 4): the resolved scenario, only as the first
//! input and only before the clock moves.

use alloc::vec::Vec;

use super::{DomainState, Sim};
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
        let (id, _) = ctx.create_sim(
            LogPayload::FounderAdded { born },
            Causes::new(ctx.trigger()),
        );
        domain.sims.insert(id, Sim { born });
    }
}

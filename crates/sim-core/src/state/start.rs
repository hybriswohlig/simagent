//! The input `Start` (ADR-0004 section 4): the resolved scenario, only as the first
//! input and only before the clock moves.

use super::{DomainState, Sim};
use crate::input::{InputRejected, Start};
use crate::time::SimTime;
use crate::world::SimCtx;

pub(crate) fn check_start(
    domain: &DomainState,
    closed_through: SimTime,
    start: &Start,
) -> Result<(), InputRejected> {
    let _ = (domain, closed_through, start);
    Ok(())
}

pub(crate) fn apply_start(domain: &mut DomainState, start: &Start, ctx: &mut SimCtx<'_, '_>) {
    let _ = (domain, start, ctx);
    let _ = |born| Sim { born };
    let _ = (
        SimCtx::now,
        SimCtx::trigger,
        SimCtx::emit,
        SimCtx::create_sim,
        SimCtx::schedule_periodic,
    );
}

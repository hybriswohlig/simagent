//! Dummy domain event of step 2: one `core:birthday` entry per sim and year.

use alloc::vec;

use super::{DomainState, Slot, whole_years};
use crate::ids::SimId;
use crate::log::{Causes, LogPayload, Subject};
use crate::time::YEAR;
use crate::trace::TracePayload;
use crate::world::{EventPayload, SimCtx};

/// Writes the entry and plans the next birthday with this entry as its `by`. The age
/// follows from the birth time; at 18 the carer ends (ADR-0018 section 4).
pub(crate) fn on_birthday(domain: &mut DomainState, sim: SimId, ctx: &mut SimCtx<'_, '_>) {
    let Some(person) = domain.sims.get(&sim) else {
        panic!("invariant unknown_entity_id: sim {}", sim.get());
    };
    let age_years = whole_years(person.born, ctx.now());
    // From the 18th birthday on nobody is dependent (ADR-0018 section 4).
    if age_years == 18
        && let Some(person) = domain.sims.get_mut(&sim)
    {
        person.carer = None;
    }
    let entry = ctx.emit(
        LogPayload::Birthday { age_years },
        vec![Subject::Sim(sim)],
        Causes::new(ctx.trigger()),
    );
    let next = ctx.now() + YEAR;
    let handle = ctx.schedule(next, EventPayload::Birthday { sim }, entry);
    domain.pending.insert((sim, Slot::Birthday), handle);
    ctx.trace(sim, || TracePayload::BirthdayPlanned { next });
}

//! Death through hazard budgets (ADR-0001, C4 simplified for the Durchstich). A person
//! has one budget per cause of death, drawn exponentially with rate 1 at an address
//! that depends only on the person and the cause. A cause with a constant hazard uses
//! the budget up in `budget / hazard` years; then the person dies of it. The draw is a
//! pure function of seed and key, so the budget is never stored.

use alloc::vec;
use alloc::vec::Vec;

use super::{ArchivedSim, DomainState, LeftBy, Slot};
use crate::ids::{EventId, SimId};
use crate::input::DeathCause;
use crate::lineage::LineageKey;
use crate::log::{Cause, Causes, DrawRef, LogPayload, Subject};
use crate::rng::{DrawKey, Draws, SubjectKey, WorldSeed, tags};
use crate::time::{SimDuration, SimTime};
use crate::world::{EventPayload, SimCtx};

/// The address of the budget of a person for a cause.
pub(crate) fn budget_key(lineage_key: LineageKey, cause: &DeathCause) -> DrawKey {
    DrawKey {
        tag: tags::DEATH_BUDGET,
        subject: SubjectKey::from(lineage_key),
        occasion: cause.cause.key(),
        detail: 0,
    }
}

/// How long `cause` must act on a person until the person dies of it; `None` is never.
pub(crate) fn needed(
    seed: u64,
    lineage_key: LineageKey,
    cause: &DeathCause,
) -> Option<SimDuration> {
    let budget = Draws::new(WorldSeed::new(seed)).exponential(budget_key(lineage_key, cause), 1.0);
    SimDuration::from_years_ceil(budget / cause.per_year.get())
}

/// When a person on whom `cause` acts from `origin` on dies of it: never, if the budget
/// lasts 100 000 years or the time lies beyond the range of the clock.
pub(crate) fn due(
    seed: u64,
    lineage_key: LineageKey,
    cause: &DeathCause,
    origin: SimTime,
) -> Option<SimTime> {
    origin.checked_add(needed(seed, lineage_key, cause)?)
}

/// Plans the death of `sim` for every cause, in the order of the list. The causes act
/// from `origin` on: the start for a founder. `by` is the creation entry of the sim.
pub(crate) fn plan_deaths(
    domain: &mut DomainState,
    sim: SimId,
    lineage_key: LineageKey,
    origin: SimTime,
    by: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let seed = domain
        .world_seed
        .unwrap_or_else(|| panic!("invariant death.before_start"));
    for cause in &domain.death_causes {
        let Some(at) = due(seed, lineage_key, cause, origin) else {
            continue;
        };
        let key = cause.cause.key();
        let handle = ctx.schedule(at, EventPayload::Death { sim, cause: key }, by);
        domain.pending.insert((sim, Slot::Death(key)), handle);
    }
}

/// The budget of `sim` for the cause with the key `cause` has run out: the sim dies. It
/// leaves for the archive, and what else was planned for it is cancelled. The entry
/// names the plan and the draw of the budget as its causes.
pub(crate) fn on_death(domain: &mut DomainState, sim: SimId, cause: u64, ctx: &mut SimCtx<'_, '_>) {
    let Some(person) = domain.sims.remove(&sim) else {
        panic!("invariant unknown_entity_id: sim {}", sim.get());
    };
    let Some(definition) = domain
        .death_causes
        .iter()
        .find(|definition| definition.cause.key() == cause)
    else {
        panic!("invariant death.unknown_cause");
    };
    let draw = budget_key(person.lineage_key, definition);
    ctx.emit(
        LogPayload::Death {
            cause: definition.cause.clone(),
        },
        vec![Subject::Sim(sim)],
        Causes::new(ctx.trigger()).with(Cause::Random(DrawRef::from(draw))),
    );
    let slots: Vec<(SimId, Slot)> = domain
        .pending
        .range((sim, Slot::Birthday)..=(sim, Slot::Death(u64::MAX)))
        .map(|(slot, _)| *slot)
        .collect();
    for slot in slots {
        if let Some(handle) = domain.pending.remove(&slot) {
            // The event that runs right now is no longer pending.
            ctx.cancel(handle);
        }
    }
    domain.archive.insert(
        sim,
        ArchivedSim {
            born: person.born,
            left: ctx.now(),
            left_by: LeftBy::Death { cause },
            lineage_key: person.lineage_key,
            sex: person.sex,
            genotype: person.genotype,
            exposure: person.exposure,
        },
    );
}

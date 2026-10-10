//! Death through hazard budgets (ADR-0001, C4, ADR-0017 section 2). A person has one
//! budget per cause of death, drawn exponentially with rate 1 at an address that depends
//! only on the person and the cause. The draw is a pure function of seed and key, so the
//! budget is never stored.
//!
//! The causes of a sim, in this order: the causes of `Start`, for everyone, with a
//! constant hazard from its origin on (a founder from the start); then its lethal
//! hereditary diseases in the order of the data (`disease`), which act from their onset
//! while the switch is on. When the switch flips, the deaths of the diseases are planned
//! again from what is left of the budget, without a new draw (ADR-0001 section 6).

use alloc::vec;
use alloc::vec::Vec;

use super::{ArchivedSim, DomainState, LeftBy, Slot, disease, partnership};
use crate::data::Data;
use crate::data_id::Code;
use crate::ids::{EventId, SimId};
use crate::input::DeathCause;
use crate::lineage::LineageKey;
use crate::log::{Cause, Causes, DrawRef, LogPayload, Subject};
use crate::rng::{DrawKey, Draws, SubjectKey, WorldSeed, tags};
use crate::time::{SimDuration, SimTime};
use crate::world::{EventPayload, SimCtx};

/// The address of the budget of a person for the cause with the key `cause`.
pub(crate) fn budget_key(lineage_key: LineageKey, cause: u64) -> DrawKey {
    DrawKey {
        tag: tags::DEATH_BUDGET,
        subject: SubjectKey::from(lineage_key),
        occasion: cause,
        detail: 0,
    }
}

/// How long a cause for everyone must act on a person until the person dies of it;
/// `None` is never.
pub(crate) fn needed(
    seed: u64,
    lineage_key: LineageKey,
    cause: &DeathCause,
) -> Option<SimDuration> {
    let budget = Draws::new(WorldSeed::new(seed))
        .exponential(budget_key(lineage_key, cause.cause.key()), 1.0);
    SimDuration::from_years_ceil(budget / cause.per_year.get())
}

/// When a person dies of a cause for everyone if nothing else comes first: it acts from
/// `origin` on (the start for a founder). Never if the budget lasts 100 000 years or the
/// time lies beyond the range of the clock.
pub(crate) fn due(
    seed: u64,
    lineage_key: LineageKey,
    cause: &DeathCause,
    origin: SimTime,
) -> Option<SimTime> {
    origin.checked_add(needed(seed, lineage_key, cause)?)
}

/// From when the causes for everyone act on a sim: from its birth, on a founder from the
/// start.
pub(crate) fn origin(born: SimTime) -> SimTime {
    born.max(SimTime::START)
}

/// Plans the deaths of `sim`: the causes for everyone in the order of `Start`, then its
/// lethal diseases. `by` is the entry that plans them.
pub(crate) fn plan_deaths(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    by: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let seed = domain
        .world_seed
        .unwrap_or_else(|| panic!("invariant death.before_start"));
    let Some(person) = domain.sims.get(&sim) else {
        panic!("invariant unknown_entity_id: sim {}", sim.get());
    };
    let (lineage_key, origin) = (person.lineage_key, origin(person.born));
    for cause in &domain.death_causes {
        let Some(at) = due(seed, lineage_key, cause, origin) else {
            continue;
        };
        let key = cause.cause.key();
        let handle = ctx.schedule(at, EventPayload::Death { sim, cause: key }, by);
        domain.pending.insert((sim, Slot::Death(key)), handle);
    }
    plan_disease_deaths(domain, data, sim, by, ctx);
}

/// Plans the deaths of the lethal diseases of `sim` whose exposure runs, in the order of
/// the data.
fn plan_disease_deaths(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    by: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let seed = domain
        .world_seed
        .unwrap_or_else(|| panic!("invariant death.before_start"));
    let Some(person) = domain.sims.get(&sim) else {
        panic!("invariant unknown_entity_id: sim {}", sim.get());
    };
    let mut planned = Vec::new();
    for affliction in &person.afflictions {
        let definition = disease::of_key(data, affliction.disease)
            .unwrap_or_else(|| panic!("invariant death.unknown_disease"));
        if let Some(at) = disease::due(
            seed,
            person.born,
            person.lineage_key,
            affliction,
            definition,
        ) {
            planned.push((affliction.disease, at));
        }
    }
    for (key, at) in planned {
        let handle = ctx.schedule(at, EventPayload::Death { sim, cause: key }, by);
        domain.pending.insert((sim, Slot::Death(key)), handle);
    }
}

/// The switch of the hereditary diseases flipped to `on` at the second of the entry `by`.
/// Every living sim with a lethal disease (by ID) opens or closes the exposure of each
/// such disease; then the deaths of its diseases are cancelled and planned again by `by`.
/// The causes for everyone come first in the list, so they keep their times and their
/// order; nobody else is touched.
pub(crate) fn switch_diseases(
    domain: &mut DomainState,
    data: &Data,
    on: bool,
    by: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let now = ctx.now();
    let exposed: Vec<SimId> = domain
        .sims
        .iter()
        .filter(|(_, person)| person.afflictions.iter().any(|a| a.exposure.is_some()))
        .map(|(id, _)| *id)
        .collect();
    for sim in exposed {
        let person = domain
            .sims
            .get_mut(&sim)
            .unwrap_or_else(|| panic!("invariant unknown_entity_id: sim {}", sim.get()));
        let born = person.born;
        let mut keys = Vec::new();
        for affliction in &mut person.afflictions {
            let Some(exposure) = affliction.exposure.as_mut() else {
                continue;
            };
            keys.push(affliction.disease);
            match (on, exposure.since) {
                (true, None) => exposure.since = Some(now),
                (false, Some(_)) => {
                    let definition = disease::of_key(data, affliction.disease)
                        .unwrap_or_else(|| panic!("invariant death.unknown_disease"));
                    disease::close(affliction, definition, born, now);
                }
                // The switch flips, so every exposure runs exactly when it was on.
                (true, Some(_)) | (false, None) => panic!("invariant exposure.switch"),
            }
        }
        for key in keys {
            if let Some(handle) = domain.pending.remove(&(sim, Slot::Death(key))) {
                ctx.cancel(handle);
            }
        }
        plan_disease_deaths(domain, data, sim, by, ctx);
    }
}

/// The code and the budget address of the cause with the key `cause`: a cause of `Start`
/// or a disease of the data.
fn cause_of(domain: &DomainState, data: &Data, cause: u64) -> Code {
    if let Some(definition) = domain
        .death_causes
        .iter()
        .find(|definition| definition.cause.key() == cause)
    {
        return definition.cause.clone();
    }
    disease::of_key(data, cause)
        .map(|definition| Code::from(definition.code.clone()))
        .unwrap_or_else(|| panic!("invariant death.unknown_cause"))
}

/// The budget of `sim` for the cause with the key `cause` has run out: the sim dies. It
/// leaves for the archive, and what else was planned for it is cancelled. The entry
/// names the plan and the draw of the budget as its causes.
pub(crate) fn on_death(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    cause: u64,
    ctx: &mut SimCtx<'_, '_>,
) {
    let code = cause_of(domain, data, cause);
    let Some(person) = domain.sims.remove(&sim) else {
        panic!("invariant unknown_entity_id: sim {}", sim.get());
    };
    let draw = budget_key(person.lineage_key, cause);
    ctx.emit(
        LogPayload::Death { cause: code },
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
            genome: person.genome,
            afflictions: person.afflictions,
            conceptions: person.conceptions,
            parents: person.parents,
        },
    );
    // Her pregnancy ends with her; her conception stays counted.
    if let Some(pregnancy) = domain.pregnancies.remove(&sim) {
        domain.lineage.remove(pregnancy.lineage_key);
    }
    partnership::end_with_death(domain, sim, ctx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_id::{Code, DataId};
    use crate::finite::FiniteF64;
    use crate::input::AppliesTo;
    use crate::time::MAX_ABS_SECS;

    /// A death lies at most at the end of the clock; beyond it, it is never due
    /// (ADR-0002). The cause acts from `origin` on for the budget of founder 1 of
    /// `test:mortal` with 0.5 per year.
    #[test]
    fn a_death_beyond_the_clock_is_never_due() {
        let cause = DeathCause {
            cause: Code::new("test:base").expect("valid"),
            per_year: FiniteF64::try_new(0.5).expect("finite"),
            applies_to: AppliesTo::Everyone,
        };
        let key = LineageKey::founder(&DataId::new("test:mortal").expect("valid"), 1);
        let needed = needed(0xff, key, &cause).expect("finite").as_secs();
        let due = |origin: i64| due(0xff, key, &cause, SimTime::from_secs(origin));
        assert_eq!(due(0), Some(SimTime::from_secs(needed)));
        assert_eq!(
            due(MAX_ABS_SECS - needed),
            Some(SimTime::from_secs(MAX_ABS_SECS))
        );
        assert_eq!(due(MAX_ABS_SECS - needed + 1), None);
    }
}

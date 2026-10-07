//! Death through hazard budgets (ADR-0001, C4 simplified for the Durchstich). A person
//! has one budget per cause of death, drawn exponentially with rate 1 at an address
//! that depends only on the person and the cause. A cause with a constant hazard uses
//! the budget up in `budget / hazard` years of acting; then the person dies of it. The
//! draw is a pure function of seed and key, so the budget is never stored.
//!
//! A cause for everyone acts on a founder from the start on. A hereditary cause acts
//! only on a sim with the dummy allele twice while the switch is on (C5); the exposure
//! of the sim says how long it has acted. When the switch flips, its deaths are planned
//! again from what is left of the budget, without a new draw (ADR-0001 section 6).

use alloc::vec;
use alloc::vec::Vec;

use super::{ArchivedSim, DomainState, Exposure, LeftBy, Slot, partnership};
use crate::data::Data;
use crate::data_id::DataId;
use crate::genome::Genome;
use crate::ids::{EventId, SimId};
use crate::input::{AppliesTo, DeathCause};
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

/// When a person dies of `cause` if nothing else comes first. A cause for everyone acts
/// from `origin` on (the start for a founder). A hereditary cause acts only while the
/// exposure runs, for what is left of the budget; without an exposure, or with one that
/// does not run, never. Never also if the budget lasts 100 000 years or the time lies
/// beyond the range of the clock.
pub(crate) fn due(
    seed: u64,
    lineage_key: LineageKey,
    exposure: Option<Exposure>,
    cause: &DeathCause,
    origin: SimTime,
) -> Option<SimTime> {
    let needed = needed(seed, lineage_key, cause)?;
    match cause.applies_to {
        AppliesTo::Everyone => origin.checked_add(needed),
        AppliesTo::DummyHomozygous => {
            let exposure = exposure?;
            exposure
                .since?
                .checked_add(needed.checked_sub(exposure.before)?)
        }
    }
}

/// The dummy allele twice (C5): dose 2 at the locus `core:dummy` of the genome map. A map
/// without that locus has nobody with the dummy allele twice.
pub(crate) fn dummy_homozygous(genome: &Genome, data: &Data) -> bool {
    let id = DataId::new("core:dummy").unwrap_or_else(|_| panic!("invariant death.dummy_id"));
    data.locus(&id).is_some_and(|locus| genome.dose(locus) == 2)
}

/// The exposure a sim starts with at `origin` (the start for a founder, the birth for a
/// child): one for the dummy allele twice, running from then on if the switch is on.
pub(crate) fn first_exposure(
    homozygous: bool,
    switch_on: bool,
    origin: SimTime,
) -> Option<Exposure> {
    homozygous.then_some(Exposure {
        before: SimDuration::ZERO,
        since: switch_on.then_some(origin),
    })
}

/// From when the causes for everyone act on a sim: from its birth, on a founder from the
/// start.
pub(crate) fn origin(born: SimTime) -> SimTime {
    born.max(SimTime::START)
}

/// Plans the deaths of `sim` for the causes from the list position `from` on, in the
/// order of the list; the causes for everyone act from its origin on. `by` is the entry
/// that plans them.
pub(crate) fn plan_deaths(
    domain: &mut DomainState,
    sim: SimId,
    from: usize,
    by: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let seed = domain
        .world_seed
        .unwrap_or_else(|| panic!("invariant death.before_start"));
    let Some(person) = domain.sims.get(&sim) else {
        panic!("invariant unknown_entity_id: sim {}", sim.get());
    };
    let (lineage_key, exposure) = (person.lineage_key, person.exposure);
    let origin = origin(person.born);
    for cause in domain.death_causes.iter().skip(from) {
        let Some(at) = due(seed, lineage_key, exposure, cause, origin) else {
            continue;
        };
        let key = cause.cause.key();
        let handle = ctx.schedule(at, EventPayload::Death { sim, cause: key }, by);
        domain.pending.insert((sim, Slot::Death(key)), handle);
    }
}

/// The switch of the hereditary diseases flipped to `on`. Every living sim with an
/// exposure (by ID) opens or closes its period now. If a hereditary cause exists, its
/// deaths from the first hereditary cause in the list on are cancelled and planned
/// again by `by`, the entry of the change: a later cause keeps its time but is planned
/// after the hereditary one, so that of two deaths due in the same second the cause
/// earlier in the list still runs first. Nobody else is touched.
pub(crate) fn switch_diseases(
    domain: &mut DomainState,
    on: bool,
    by: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let now = ctx.now();
    let first = domain
        .death_causes
        .iter()
        .position(|cause| cause.applies_to == AppliesTo::DummyHomozygous);
    let exposed: Vec<SimId> = domain
        .sims
        .iter()
        .filter(|(_, person)| person.exposure.is_some())
        .map(|(id, _)| *id)
        .collect();
    for sim in exposed {
        let exposure = domain
            .sims
            .get_mut(&sim)
            .and_then(|person| person.exposure.as_mut())
            .unwrap_or_else(|| panic!("invariant exposure.lost"));
        match (on, exposure.since) {
            (true, None) => exposure.since = Some(now),
            (false, Some(since)) => {
                exposure.before = now
                    .checked_since(since)
                    .and_then(|period| exposure.before.checked_add(period))
                    .unwrap_or_else(|| panic!("invariant time.out_of_range"));
                exposure.since = None;
            }
            // The switch flips, so every exposure runs exactly when it was on.
            (true, Some(_)) | (false, None) => panic!("invariant exposure.switch"),
        }
        let Some(first) = first else {
            continue;
        };
        for cause in domain.death_causes.iter().skip(first) {
            if let Some(handle) = domain
                .pending
                .remove(&(sim, Slot::Death(cause.cause.key())))
            {
                ctx.cancel(handle);
            }
        }
        plan_deaths(domain, sim, first, by, ctx);
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
            genome: person.genome,
            exposure: person.exposure,
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
    use crate::time::MAX_ABS_SECS;

    /// The death of a hereditary cause lies at most at the end of the clock; beyond it,
    /// it is never due (ADR-0002). He is founder 1 of `test:sick`; the disease needs
    /// 101 053 347 seconds of exposure to kill him (tools/state_reference.py).
    #[test]
    fn a_death_beyond_the_clock_is_never_due() {
        let his_disease = 101_053_347;
        let disease = DeathCause {
            cause: Code::new("test:dummy_disease").expect("valid"),
            per_year: FiniteF64::try_new(0.3).expect("finite"),
            applies_to: AppliesTo::DummyHomozygous,
        };
        let key = LineageKey::founder(&DataId::new("test:sick").expect("valid"), 1);
        let running = |before: i64, since: i64| {
            Some(Exposure {
                before: SimDuration::from_secs(before),
                since: Some(SimTime::from_secs(since)),
            })
        };
        let due = |exposure| due(0xff, key, exposure, &disease, SimTime::START);
        assert_eq!(due(running(0, 0)), Some(SimTime::from_secs(his_disease)));
        assert_eq!(
            due(running(0, MAX_ABS_SECS - his_disease)),
            Some(SimTime::from_secs(MAX_ABS_SECS))
        );
        assert_eq!(due(running(0, MAX_ABS_SECS - his_disease + 1)), None);
        assert_eq!(due(running(0, MAX_ABS_SECS - 1000)), None);
        // Used up: due in the second it runs again, at the very end too.
        assert_eq!(
            due(running(his_disease, MAX_ABS_SECS)),
            Some(SimTime::from_secs(MAX_ABS_SECS))
        );
    }
}

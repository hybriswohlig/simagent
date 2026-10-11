//! Death through hazard budgets (ADR-0001, C4, ADR-0017 section 2). A person has one
//! budget per cause of death, drawn exponentially with rate 1 at an address that depends
//! only on the person and the cause. The draw is a pure function of seed and key, so the
//! budget is never stored.
//!
//! The causes of a sim, in this order (ADR-0017 section 2): senescence (Gompertz with the
//! genetic factor of longevity) and accident (Makeham), for everyone from its origin on (a
//! founder from the start), with the values of the data and the closed forms of
//! `mortality`; then its lethal hereditary diseases in the order of the data (`disease`),
//! which act from their onset while the switch is on. When the switch flips, the deaths of the diseases are planned
//! again from what is left of the budget, without a new draw (ADR-0001 section 6).

use alloc::vec;
use alloc::vec::Vec;

use super::{ArchivedSim, DomainState, LeftBy, Slot, disease, partnership};
use crate::data::Data;
use crate::data_id::Code;
use crate::genome::{Genome, Sex};
use crate::ids::{EventId, SimId};
use crate::lineage::LineageKey;
use crate::log::{Cause, Causes, DrawRef, LogPayload, Subject};
use crate::mortality;
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

/// The code of senescence (genetics specification 7.1, ADR-0017 section 2).
pub(crate) fn senescence() -> Code {
    Code::new("core:senescence").unwrap_or_else(|_| panic!("invariant death.code"))
}

/// The code of accident (genetics specification 7.1, ADR-0017 section 2).
pub(crate) fn accident() -> Code {
    Code::new("core:accident").unwrap_or_else(|_| panic!("invariant death.code"))
}

/// When a sim dies of senescence and of accident if nothing else comes first, with the
/// key of each cause, in this order (ADR-0017 section 2). Both act from the origin on (a
/// founder from the start, with the rest of its budget, 7.1): the age at death in the
/// closed form of `mortality`, the time counted from the origin second and rounded up;
/// never beyond the clock. Senescence carries the genetic factor of the trait of
/// longevity, held at ±6 (11.4), 1 without one (11.1).
pub(crate) fn everyone_dues(
    data: &Data,
    seed: u64,
    facts: (SimTime, LineageKey, Sex, &Genome),
) -> [(u64, Option<SimTime>); 2] {
    let (born, lineage_key, sex, genome) = facts;
    let genetics = data.genetics();
    let (gompertz, per_year) = match sex {
        Sex::Female => (genetics.senescence_female, genetics.accident_female),
        Sex::Male => (genetics.senescence_male, genetics.accident_male),
    };
    let origin = origin(born);
    let start = disease::age(born, origin);
    let draws = Draws::new(WorldSeed::new(seed));
    let budget = |key: u64| draws.exponential(budget_key(lineage_key, key), 1.0);
    let longevity = data.longevity_trait().map(|t| {
        genome
            .genetic_value(t, data)
            .unwrap_or_else(|| panic!("invariant death.longevity"))
    });
    let m = mortality::multiplier(longevity, genetics.longevity_sigma_g);
    let due = |age: f64| origin.checked_add(SimDuration::from_years_ceil(age - start)?);
    let (old, hurt) = (senescence().key(), accident().key());
    [
        (
            old,
            due(mortality::senescence_age(start, budget(old), gompertz, m)),
        ),
        (
            hurt,
            due(mortality::accident_age(start, budget(hurt), per_year)),
        ),
    ]
}

/// From when the causes for everyone act on a sim: from its birth, on a founder from the
/// start.
pub(crate) fn origin(born: SimTime) -> SimTime {
    born.max(SimTime::START)
}

/// The code of the cause with the key `cause`: senescence, accident, or a disease of the
/// data.
pub(crate) fn cause_code(data: &Data, cause: u64) -> Option<Code> {
    [senescence(), accident()]
        .into_iter()
        .find(|code| code.key() == cause)
        .or_else(|| disease::of_key(data, cause).map(|d| Code::from(d.code.clone())))
}

/// Plans the deaths of `sim`: senescence, accident, then its lethal diseases. `by` is the
/// entry that plans them.
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
    let facts = (person.born, person.lineage_key, person.sex, &person.genome);
    for (key, due) in everyone_dues(data, seed, facts) {
        let Some(at) = due else {
            continue;
        };
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
/// Senescence and accident come first in the list, so they keep their times and their
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
    let code = cause_code(data, cause).unwrap_or_else(|| panic!("invariant death.unknown_cause"));
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
            births: person.births,
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

    /// The codes of senescence and accident and their keys, as the log and the queue name
    /// them; a key of neither and of no disease has no code.
    #[test]
    fn the_causes_for_everyone_have_their_codes() {
        let data = crate::data::testing::data();
        assert_eq!(senescence().as_str(), "core:senescence");
        assert_eq!(accident().as_str(), "core:accident");
        assert_eq!(cause_code(&data, senescence().key()), Some(senescence()));
        assert_eq!(cause_code(&data, accident().key()), Some(accident()));
        assert_eq!(cause_code(&data, 7), None);
    }
}

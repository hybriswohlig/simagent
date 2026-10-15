//! Birth (ADR-0004 sections 1 and 7, B18): the pregnancy ends after its months; the
//! child enters the world with the next SimId, its parents and the genome fixed at its
//! conception.

use super::{
    DomainState, Parents, Sim, Slot, conception, death, disease, household, partnership, start,
};
use crate::data::Data;
use crate::data_id::Code;
use crate::ids::SimId;
use crate::log::{Cause, Causes, LogPayload};
use crate::trace::TracePayload;
use crate::world::{EventPayload, SimCtx};

/// The pregnancy of `mother` ends: the child or the twins are born now, in the order of
/// their index (spec 11.3). Each creation entry `core:birth` names child, mother and
/// father, with the plan of the conception and the rule `core:gestation` as causes. Each
/// child's first birthday comes a year later, its causes of death act from now on, its
/// diseases are fixed now (ADR-0017 section 3). Her count of live births grows by the
/// number of children; if she is in an open partnership, her attempts begin again after
/// the pause (ADR-0017 section 4), planned by the entry of the first child.
pub(crate) fn on_birth(
    domain: &mut DomainState,
    data: &Data,
    mother: SimId,
    ctx: &mut SimCtx<'_, '_>,
) {
    // The index keeps the handle of this birth until her next conception or her death:
    // handles are never used again, so cancelling it then changes nothing.
    let Some(pregnancy) = domain.pregnancies.remove(&mother) else {
        panic!("invariant birth.without_pregnancy: sim {}", mother.get());
    };
    let switch_on = domain
        .world_params
        .as_ref()
        .unwrap_or_else(|| panic!("invariant birth.before_start"))
        .hereditary_diseases;
    let seed = domain
        .world_seed
        .unwrap_or_else(|| panic!("invariant birth.before_start"));
    let now = ctx.now();
    // The rule of a gestation, the cause of the birth next to the plan.
    let rule = Code::new("core:gestation").unwrap_or_else(|_| panic!("invariant birth.rule_code"));
    let mut first = None;
    let mother_household = domain
        .sims
        .get(&mother)
        .map(|her| her.household)
        .unwrap_or_else(|| panic!("invariant unknown_entity_id: sim {}", mother.get()));
    // The lineage keys of the parents, for the phenotype of a trait of liability; the
    // father may have died during the pregnancy, or never come from the pool.
    let father_key = pregnancy
        .pool_father
        .or_else(|| pregnancy.father.map(|father| domain.key_of(father)))
        .unwrap_or_else(|| panic!("invariant birth.father"));
    let parent_keys = crate::phenotype::Parents {
        mother: domain.key_of(mother),
        father: father_key,
    };
    // A town child of two pool persons draws `C` with the pool seed (design note of the
    // arrival, section 6.2).
    let pool_seed = data.pool().map(|held| held.pool().pool_seed);
    let town_seeds = match (pregnancy.father, pool_seed) {
        (Some(father), Some(pool))
            if domain.pool_origin(mother).is_some() && domain.pool_origin(father).is_some() =>
        {
            super::Seeds {
                individual: seed,
                shared: pool,
                life: seed,
            }
        }
        _ => super::Seeds::one(seed),
    };
    let year = u64::try_from(now.as_secs().div_euclid(crate::time::YEAR.as_secs()))
        .unwrap_or_else(|_| panic!("invariant time.out_of_range"));
    for unborn in &pregnancy.children {
        let (child, born) = ctx.create_child(
            LogPayload::Birth {
                lineage_key: unborn.lineage_key,
            },
            (mother, pregnancy.father),
            Causes::new(ctx.trigger()).with(Cause::Rule(rule.clone())),
        );
        first.get_or_insert(born);
        // A child of a pregnancy from the pool is a pool person: its afflictions and its
        // genome key are the pool's (design note of the arrival, section 6.3).
        let (afflictions, parents, pool) = match pregnancy.pool_father {
            Some(pool_father) => {
                let record = data
                    .pool()
                    .and_then(|held| held.unborn(unborn.lineage_key))
                    .unwrap_or_else(|| panic!("invariant pool.unborn"));
                (
                    super::arrival::afflictions(data, &record.afflictions, now, switch_on),
                    None,
                    Some(super::PoolOrigin {
                        genome_key: record.genome_key,
                        born_pool: None,
                        arrived: now,
                        arrival_year: year,
                        mother: Some(parent_keys.mother),
                        father: Some(pool_father),
                        social_parents: alloc::vec::Vec::new(),
                    }),
                )
            }
            None => (
                disease::afflictions(
                    data,
                    town_seeds,
                    (unborn.lineage_key, Some(parent_keys)),
                    unborn.sex,
                    &unborn.genome,
                    switch_on,
                    now,
                ),
                Some(Parents {
                    mother,
                    father: pregnancy
                        .father
                        .unwrap_or_else(|| panic!("invariant birth.father")),
                    conception: pregnancy.conception,
                }),
                None,
            ),
        };
        domain.keys.insert(unborn.lineage_key, child);
        domain.sims.insert(
            child,
            Sim {
                born: now,
                lineage_key: unborn.lineage_key,
                sex: unborn.sex,
                genome: unborn.genome,
                afflictions,
                conceptions: 0,
                births: 0,
                partnerships: 0,
                // Set by `household::born_into` right after.
                household: mother_household,
                moved_out: false,
                carer: Some(mother),
                parents,
                social_parents: alloc::vec::Vec::new(),
                pool,
            },
        );
        household::born_into(domain, child, mother, ctx);
        let next = start::first_birthday(now, now);
        let handle = ctx.schedule(next, EventPayload::Birthday { sim: child }, born);
        domain.pending.insert((child, Slot::Birthday), handle);
        ctx.trace(child, || TracePayload::BirthdayPlanned { next });
        death::plan_deaths(domain, data, child, born, ctx);
        household::plan_leave(domain, data, child, born, ctx);
    }
    let born = first.unwrap_or_else(|| panic!("invariant birth.children"));
    let Some(her) = domain.sims.get_mut(&mother) else {
        panic!("invariant unknown_entity_id: sim {}", mother.get());
    };
    let count = u32::try_from(pregnancy.children.len()).unwrap_or(u32::MAX);
    her.births = her
        .births
        .checked_add(count)
        .unwrap_or_else(|| panic!("invariant birth.count_range"));
    if partnership::open_partnership(domain, mother).is_some() {
        let pause = data.demography().fertility.pause_months;
        let start = conception::series_after_birth(now, pause);
        conception::plan_attempts(domain, data, mother, born, start, ctx);
    }
}

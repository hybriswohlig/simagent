//! Persons of the pool arriving in the town (ADR-0020 section 5; design note of the
//! arrival, section 5): the founders of a town at `Start` and the immigrants in the first
//! second of every year. Each household of the pool comes whole, with its open partnership,
//! its pregnancies and its plans, shifted by `Δ = A − E_p`.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use super::{
    Affliction, DomainState, Exposure, Partnership, PoolOrigin, Pregnancy, Sim, Slot, Unborn,
    disease, household,
};
use crate::data::Data;
use crate::finite::FiniteF64;
use crate::ids::{EventId, HouseholdId, SimId};
use crate::lineage::LineageKey;
use crate::log::{Cause, Causes, LogPayload};
use crate::pool::{PoolHousehold, PoolPerson, take_households};
use crate::time::{MONTH, SimTime, YEAR};
use crate::world::{EventPayload, SimCtx};

/// `t + Δ`, a second of the pool moved into the town.
///
/// # Panics
///
/// Invariant `pool.carried_before_arrival` if it falls before the arrival `now`;
/// `time.out_of_range` beyond the clock.
fn shifted(t: SimTime, delta: i64, now: SimTime) -> SimTime {
    let at = t
        .as_secs()
        .checked_add(delta)
        .map(SimTime::from_secs)
        .unwrap_or_else(|| panic!("invariant time.out_of_range"));
    assert!(at >= now, "invariant pool.carried_before_arrival");
    at
}

/// The households of the data's pool with these indices arrive now, in this order, in
/// year `year` (0 for the founders).
pub(crate) fn arrive(
    domain: &mut DomainState,
    data: &Data,
    households: &[usize],
    year: u64,
    ctx: &mut SimCtx<'_, '_>,
) {
    let pool = data
        .pool()
        .unwrap_or_else(|| panic!("invariant pool.missing"))
        .pool();
    let now = ctx.now();
    let delta = now
        .as_secs()
        .checked_sub(pool.frozen_at.as_secs())
        .unwrap_or_else(|| panic!("invariant time.out_of_range"));
    for &index in households {
        domain.pool_used.push(
            u32::try_from(index).unwrap_or_else(|_| panic!("invariant pool.household_index")),
        );
        arrive_household(domain, data, &pool.households[index], delta, year, ctx);
    }
}

/// The persons of a village arrive now, at the start, each in the household of their own
/// that `pool::village_households` built (ADR-0020 section 8, G4; Grok V2): no household of
/// the pool is used.
pub(crate) fn arrive_alone(
    domain: &mut DomainState,
    data: &Data,
    households: &[PoolHousehold],
    ctx: &mut SimCtx<'_, '_>,
) {
    let pool = data
        .pool()
        .unwrap_or_else(|| panic!("invariant pool.missing"))
        .pool();
    let delta = ctx
        .now()
        .as_secs()
        .checked_sub(pool.frozen_at.as_secs())
        .unwrap_or_else(|| panic!("invariant time.out_of_range"));
    for household in households {
        arrive_household(domain, data, household, delta, 0, ctx);
    }
}

/// One household of the pool arrives (design note, section 5, steps 1 to 5).
fn arrive_household(
    domain: &mut DomainState,
    data: &Data,
    household: &PoolHousehold,
    delta: i64,
    year: u64,
    ctx: &mut SimCtx<'_, '_>,
) {
    let now = ctx.now();
    let switch_on = domain
        .world_params
        .as_ref()
        .is_some_and(|params| params.hereditary_diseases);
    // 1. The members, each with its creation entry.
    let mut arrived: Vec<(SimId, EventId)> = Vec::with_capacity(household.members.len());
    let mut by_key: BTreeMap<LineageKey, SimId> = BTreeMap::new();
    for member in &household.members {
        let (id, entry) = ctx.create_sim(
            LogPayload::Arrived {
                lineage_key: member.lineage_key,
            },
            Causes::new(ctx.trigger()),
        );
        domain.lineage.insert(member.lineage_key);
        domain.keys.insert(member.lineage_key, id);
        domain
            .sims
            .insert(id, arrival_state(data, member, delta, now, year, switch_on));
        super::onset::on_creation(domain, data, id, entry, switch_on, ctx);
        by_key.insert(member.lineage_key, id);
        arrived.push((id, entry));
    }
    for (member, &(id, _)) in household.members.iter().zip(&arrived) {
        let carer = member.carer.map(|key| {
            *by_key
                .get(&key)
                .unwrap_or_else(|| panic!("invariant pool.carer_not_member"))
        });
        living_mut(domain, id).carer = carer;
    }
    // 2. The household, formed now; the members join it without `core:moved`.
    let ids: Vec<SimId> = arrived.iter().map(|(id, _)| *id).collect();
    let causes = Causes::new(ctx.trigger()).with(Cause::Rule(household::rule()));
    let home: HouseholdId = household::create(domain, &ids, causes, ctx);
    for id in &ids {
        household::join(domain, *id, home);
    }
    // 3. The open partnership, as it was formed in the pool.
    let member_id = |key: LineageKey| {
        *by_key
            .get(&key)
            .unwrap_or_else(|| panic!("invariant pool.partnership_not_members"))
    };
    let entry_of = |id: SimId| {
        arrived
            .iter()
            .find(|(sim, _)| *sim == id)
            .map(|(_, entry)| *entry)
            .unwrap_or_else(|| panic!("invariant pool.arrival_entry"))
    };
    let partnership = household.partnership.map(|pair| {
        let (woman, man) = (member_id(pair.woman), member_id(pair.man));
        let (id, _) =
            ctx.create_partnership(woman, man, Causes::new(Cause::Event(entry_of(woman))));
        domain.partnerships.insert(
            id,
            Partnership {
                woman,
                man,
                formed: SimTime::from_secs(
                    pair.formed
                        .as_secs()
                        .checked_add(delta)
                        .unwrap_or_else(|| panic!("invariant time.out_of_range")),
                ),
                ended: None,
            },
        );
        domain.open.insert(woman, id);
        domain.open.insert(man, id);
        id
    });
    // 4. The pregnancies; the father is a person of the town only if he is one now.
    for pregnancy in &household.pregnancies {
        let mother = member_id(pregnancy.mother);
        let father = domain.keys.get(&pregnancy.father).copied();
        domain.pregnancies.insert(
            mother,
            Pregnancy {
                father,
                pool_father: Some(pregnancy.father),
                conception: pregnancy.conception,
                children: pregnancy
                    .children
                    .iter()
                    .map(|child| Unborn {
                        lineage_key: child.lineage_key,
                        sex: child.sex,
                        genome: child.genome,
                    })
                    .collect(),
                conceived: SimTime::from_secs(
                    pregnancy
                        .conceived
                        .as_secs()
                        .checked_add(delta)
                        .unwrap_or_else(|| panic!("invariant time.out_of_range")),
                ),
            },
        );
    }
    // 5. Per member: its carried plans, its next birthday, its deaths.
    for (member, &(id, entry)) in household.members.iter().zip(&arrived) {
        if let Some(t) = member.conception_at {
            let at = shifted(t, delta, now);
            let handle = ctx.schedule_periodic(
                at,
                MONTH,
                EventPayload::ConceptionAttempt { woman: id },
                entry,
            );
            domain.pending.insert((id, Slot::Conception), handle);
        }
        if let Some(t) = member.birth_at {
            let at = shifted(t, delta, now);
            let handle = ctx.schedule(at, EventPayload::Birth { mother: id }, entry);
            domain.pending.insert((id, Slot::Birth), handle);
        }
        if let Some(t) = member.separation_at {
            let at = shifted(t, delta, now);
            let partnership = partnership
                .unwrap_or_else(|| panic!("invariant pool.separation_without_partnership"));
            let handle = ctx.schedule(at, EventPayload::Separation { partnership }, entry);
            domain.pending.insert((id, Slot::Separation), handle);
        }
        if let Some(t) = member.leave_home_at {
            let at = shifted(t, delta, now);
            let handle = ctx.schedule(at, EventPayload::LeaveHome { sim: id }, entry);
            domain.pending.insert((id, Slot::LeaveHome), handle);
        }
        let born = living_mut(domain, id).born;
        let next = super::start::first_birthday(born, now);
        let handle = ctx.schedule(next, EventPayload::Birthday { sim: id }, entry);
        domain.pending.insert((id, Slot::Birthday), handle);
        super::death::plan_deaths(domain, data, id, entry, ctx);
    }
}

/// The state of an arriving member (design note, section 6.1).
fn arrival_state(
    data: &Data,
    member: &PoolPerson,
    delta: i64,
    now: SimTime,
    year: u64,
    switch_on: bool,
) -> Sim {
    let born = SimTime::from_secs(
        member
            .born
            .as_secs()
            .checked_add(delta)
            .unwrap_or_else(|| panic!("invariant time.out_of_range")),
    );
    Sim {
        born,
        lineage_key: member.lineage_key,
        sex: member.sex,
        genome: member.genome,
        afflictions: afflictions(data, &member.afflictions, now, switch_on),
        conceptions: member.conceptions,
        births: member.births,
        partnerships: member.partnerships,
        household: HouseholdId::try_from(1).unwrap_or_else(|_| panic!("invariant household")),
        moved_out: member.moved_out,
        carer: None,
        parents: None,
        social_parents: Vec::new(),
        pool: Some(PoolOrigin {
            genome_key: member.genome_key,
            born_pool: Some(member.born),
            arrived: now,
            arrival_year: year,
            mother: member.mother,
            father: member.father,
            social_parents: member.social_parents.clone(),
        }),
    }
}

/// The pool's afflictions with the exposure of a lethal one from `since` if the switch is
/// on (ADR-0017 section 3).
pub(crate) fn afflictions(
    data: &Data,
    pooled: &[crate::pool::PoolAffliction],
    since: SimTime,
    switch_on: bool,
) -> Vec<Affliction> {
    pooled
        .iter()
        .map(|affliction| {
            let definition = disease::of_key(data, affliction.disease)
                .unwrap_or_else(|| panic!("invariant pool.unknown_disease"));
            Affliction {
                disease: affliction.disease,
                onset: affliction.onset,
                exposure: disease::hazard(definition).map(|_| Exposure {
                    used: FiniteF64::try_new(0.0).unwrap_or_else(|_| panic!("invariant finite")),
                    since: switch_on.then_some(since),
                }),
            }
        })
        .collect()
}

fn living_mut(domain: &mut DomainState, sim: SimId) -> &mut Sim {
    domain
        .sims
        .get_mut(&sim)
        .unwrap_or_else(|| panic!("invariant unknown_entity_id: sim {}", sim.get()))
}

/// The yearly immigration (ADR-0020 section 5): the households not yet used, in the order
/// of the draw for this year, until they bring `persons`.
///
/// # Panics
///
/// Invariant `pool.empty` if the pool runs out first; nobody of this year arrives.
pub(crate) fn on_immigration(
    domain: &mut DomainState,
    data: &Data,
    persons: u64,
    ctx: &mut SimCtx<'_, '_>,
) {
    let seed = domain
        .world_seed
        .unwrap_or_else(|| panic!("invariant pool.before_start"));
    let year = u64::try_from(ctx.now().as_secs() / YEAR.as_secs())
        .unwrap_or_else(|_| panic!("invariant time.out_of_range"));
    let pool = data
        .pool()
        .unwrap_or_else(|| panic!("invariant pool.missing"))
        .pool();
    let used = domain
        .pool_used
        .iter()
        .filter_map(|index| usize::try_from(*index).ok())
        .collect();
    let (taken, enough) = take_households(pool, seed, year, &used, persons);
    assert!(enough, "invariant pool.empty");
    arrive(domain, data, &taken, year, ctx);
}

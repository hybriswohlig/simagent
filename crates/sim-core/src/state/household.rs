//! Households, part 1 (ADR-0018 section 4): every living person lives in one household.
//! The couples of `Start` and the adult founders found theirs at the start, a child is born
//! into its mother's, a couple founds a new one when it pairs and takes the dependent
//! children each of them cares for, the man founds a new one at a separation and takes the
//! children he cares for, and a person founds his own when he leaves home at the age drawn
//! from the table of the data. A dependent child (under 18, not moved out) has a carer in
//! its household; when the carer dies, the other parent or the dead carer's partner in the
//! household takes over, else the child gets a household of its own (the last stage of the
//! orphan rule; part 2 adds relatives and foster homes).

use alloc::collections::BTreeSet;
use alloc::vec::Vec;

use super::{DomainState, Household, Sim, Slot, market};
use crate::data::Data;
use crate::data_id::Code;
use crate::ids::{EventId, HouseholdId, SimId};
use crate::lineage::LineageKey;
use crate::log::{Cause, Causes, LogPayload, Subject};
use crate::rng::{DrawKey, Draws, SubjectKey, WorldSeed, tags};
use crate::time::{SimTime, YEAR};
use crate::world::{EventPayload, SimCtx};

/// The age of adulthood: from the 18th birthday on a person is no longer dependent.
const ADULT_YEARS: i64 = 18;
/// The youngest age of the table of leaving home.
const FIRST_LEAVE_AGE: i64 = 16;

/// The rule every household entry names.
pub(crate) fn rule() -> Code {
    Code::new("core:household").unwrap_or_else(|_| panic!("invariant household.rule_code"))
}

/// Whether a person is dependent at `at`: under 18 and not moved out.
pub(crate) fn dependent(person: &Sim, at: SimTime) -> bool {
    !person.moved_out && !market::of_age(person.born, at, ADULT_YEARS)
}

/// The address of the draw of a person's age of leaving home.
pub(crate) fn leave_key(key: LineageKey) -> DrawKey {
    DrawKey {
        tag: tags::LEAVE_HOME,
        subject: SubjectKey::from(key),
        occasion: 0,
        detail: 0,
    }
}

/// The age at which a person leaves home: `16 + n` for the smallest `n` whose share of the
/// table, summed from 16 on and divided by the sum of the table, lies above the draw.
pub(crate) fn leave_age(data: &Data, seed: u64, key: LineageKey) -> i64 {
    let weights = &data.demography().leave_home;
    let u = Draws::new(WorldSeed::new(seed)).unit(leave_key(key));
    let total: f64 = weights.iter().sum();
    let mut sum = 0.0;
    let mut last = 0;
    for (n, weight) in (0_i64..).zip(weights) {
        sum += weight;
        last = n;
        if u < sum / total {
            return FIRST_LEAVE_AGE + n;
        }
    }
    FIRST_LEAVE_AGE + last
}

/// The second at which a person born at `born` leaves home: the birthday of its drawn age.
pub(crate) fn leave_second(data: &Data, seed: u64, born: SimTime, key: LineageKey) -> SimTime {
    YEAR.checked_mul(leave_age(data, seed, key))
        .and_then(|age| born.checked_add(age))
        .unwrap_or_else(|| panic!("invariant household.leave_range"))
}

/// Plans `sim`'s leaving home by `by`.
pub(crate) fn plan_leave(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    by: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let seed = domain
        .world_seed
        .unwrap_or_else(|| panic!("invariant household.before_start"));
    let person = living(domain, sim);
    let at = leave_second(data, seed, person.born, person.lineage_key);
    let handle = ctx.schedule(at, EventPayload::LeaveHome { sim }, by);
    domain.pending.insert((sim, Slot::LeaveHome), handle);
}

fn living(domain: &DomainState, sim: SimId) -> &Sim {
    domain
        .sims
        .get(&sim)
        .unwrap_or_else(|| panic!("invariant unknown_entity_id: sim {}", sim.get()))
}

fn living_mut(domain: &mut DomainState, sim: SimId) -> &mut Sim {
    domain
        .sims
        .get_mut(&sim)
        .unwrap_or_else(|| panic!("invariant unknown_entity_id: sim {}", sim.get()))
}

/// A new household, formed now with `members` as its subjects (by SimId).
fn create(
    domain: &mut DomainState,
    members: &[SimId],
    causes: Causes,
    ctx: &mut SimCtx<'_, '_>,
) -> HouseholdId {
    let mut subjects: Vec<SimId> = members.to_vec();
    subjects.sort_unstable();
    let id = ctx.create_household(subjects.into_iter().map(Subject::Sim).collect(), causes);
    domain.households.insert(
        id,
        Household {
            formed: ctx.now(),
            ended: None,
        },
    );
    domain.members.insert(id, BTreeSet::new());
    id
}

/// `sim` leaves its household: the household ends now if it was its last member.
pub(crate) fn leave(domain: &mut DomainState, sim: SimId, household: HouseholdId, now: SimTime) {
    let empty = domain.members.get_mut(&household).is_some_and(|members| {
        members.remove(&sim);
        members.is_empty()
    });
    if empty {
        domain.members.remove(&household);
        if let Some(ended) = domain.households.get_mut(&household) {
            ended.ended = Some(now);
        }
    }
}

/// `sim` joins `household` (without an entry).
fn join(domain: &mut DomainState, sim: SimId, household: HouseholdId) {
    living_mut(domain, sim).household = household;
    domain.members.entry(household).or_default().insert(sim);
}

/// `sim` moves from its household into `household`, with its entry `core:moved`.
fn move_into(
    domain: &mut DomainState,
    sim: SimId,
    household: HouseholdId,
    causes: Causes,
    ctx: &mut SimCtx<'_, '_>,
) {
    let from = living(domain, sim).household;
    leave(domain, sim, from, ctx.now());
    join(domain, sim, household);
    ctx.emit(
        LogPayload::Moved { household },
        alloc::vec![Subject::Sim(sim)],
        causes,
    );
}

/// A new household: its creation entry, then the members move in, in this order.
fn found(
    domain: &mut DomainState,
    members: &[SimId],
    causes: &Causes,
    ctx: &mut SimCtx<'_, '_>,
) -> HouseholdId {
    let id = create(domain, members, causes.clone(), ctx);
    for sim in members {
        move_into(domain, *sim, id, causes.clone(), ctx);
    }
    id
}

/// The households of `Start` (ADR-0018 section 4), after the founders and the couples:
/// each couple in list order, then each other founder of 18 or more, then each other
/// founder under 18 (alone; moved out if the drawn age of leaving is reached at the start,
/// else with his leaving home planned by his creation entry). `founders` holds each
/// founder's SimId and creation entry; `couples` the couples as places in the list.
pub(crate) fn start_households(
    domain: &mut DomainState,
    data: &Data,
    founders: &[(SimId, EventId)],
    couples: &[(usize, usize)],
    ctx: &mut SimCtx<'_, '_>,
) {
    let seed = domain
        .world_seed
        .unwrap_or_else(|| panic!("invariant household.before_start"));
    let now = ctx.now();
    let causes = Causes::new(ctx.trigger()).with(Cause::Rule(rule()));
    let mut placed = BTreeSet::new();
    let settle = |domain: &mut DomainState, members: &[SimId], ctx: &mut SimCtx<'_, '_>| {
        let id = create(domain, members, causes.clone(), ctx);
        for sim in members {
            join(domain, *sim, id);
        }
    };
    for &(woman, man) in couples {
        let pair = [founders[woman].0, founders[man].0];
        for sim in pair {
            living_mut(domain, sim).moved_out = true;
            placed.insert(sim);
        }
        settle(domain, &pair, ctx);
    }
    for &(sim, _) in founders {
        if !placed.contains(&sim) && market::of_age(living(domain, sim).born, now, ADULT_YEARS) {
            living_mut(domain, sim).moved_out = true;
            placed.insert(sim);
            settle(domain, &[sim], ctx);
        }
    }
    for &(sim, added) in founders {
        if placed.contains(&sim) {
            continue;
        }
        let person = living(domain, sim);
        let leaves = leave_second(data, seed, person.born, person.lineage_key);
        let moved_out = leaves <= now;
        living_mut(domain, sim).moved_out = moved_out;
        settle(domain, &[sim], ctx);
        if !moved_out {
            plan_leave(domain, data, sim, added, ctx);
        }
    }
}

/// A child is born into its mother's household, cared for by her: its entry `core:moved`
/// after its `core:birth`, caused by the birth's plan and the rule.
pub(crate) fn born_into(
    domain: &mut DomainState,
    child: SimId,
    mother: SimId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let household = living(domain, mother).household;
    join(domain, child, household);
    let causes = Causes::new(ctx.trigger()).with(Cause::Rule(rule()));
    ctx.emit(
        LogPayload::Moved { household },
        alloc::vec![Subject::Sim(child)],
        causes,
    );
}

/// The dependent children `carers` care for, by SimId, without the carers themselves.
fn cared_for(domain: &DomainState, carers: &[SimId], now: SimTime) -> Vec<SimId> {
    domain
        .sims
        .iter()
        .filter(|(id, person)| {
            !carers.contains(id)
                && dependent(person, now)
                && person.carer.is_some_and(|carer| carers.contains(&carer))
        })
        .map(|(id, _)| *id)
        .collect()
}

/// A couple paired now (not by `Start`): it founds a new household; she moves, then he,
/// then each dependent child either of them cares for (by SimId). Both are moved out from
/// now on, without a carer; their leaving home is cancelled.
pub(crate) fn pair_moves(
    domain: &mut DomainState,
    (woman, man): (SimId, SimId),
    partnered: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let now = ctx.now();
    let mut members = alloc::vec![woman, man];
    members.extend(cared_for(domain, &[woman, man], now));
    for sim in [woman, man] {
        // Moved out, so no longer dependent: no carer from now on.
        let person = living_mut(domain, sim);
        person.moved_out = true;
        person.carer = None;
        if let Some(handle) = domain.pending.remove(&(sim, Slot::LeaveHome)) {
            ctx.cancel(handle);
        }
    }
    let causes = Causes::new(Cause::Event(partnered)).with(Cause::Rule(rule()));
    found(domain, &members, &causes, ctx);
}

/// A couple separated now: the man founds a new household and moves, with each dependent
/// child of their household he cares for (by SimId); a dependent child there whom neither
/// cares for stays and the woman becomes its carer (Dario: the children stay with the
/// mother).
pub(crate) fn separation_moves(
    domain: &mut DomainState,
    (woman, man): (SimId, SimId),
    separated: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let now = ctx.now();
    let household = living(domain, woman).household;
    let mut members = alloc::vec![man];
    let mut stay = Vec::new();
    for (id, person) in &domain.sims {
        if *id == woman || *id == man || person.household != household || !dependent(person, now) {
            continue;
        }
        if person.carer == Some(man) {
            members.push(*id);
        } else if person.carer != Some(woman) {
            stay.push(*id);
        }
    }
    for child in stay {
        living_mut(domain, child).carer = Some(woman);
    }
    let causes = Causes::new(Cause::Event(separated)).with(Cause::Rule(rule()));
    found(domain, &members, &causes, ctx);
}

/// `sim` leaves home now: a household of its own, moved out, no carer.
pub(crate) fn on_leave_home(domain: &mut DomainState, sim: SimId, ctx: &mut SimCtx<'_, '_>) {
    // The event that runs right now is no longer pending.
    domain.pending.remove(&(sim, Slot::LeaveHome));
    let person = living_mut(domain, sim);
    person.moved_out = true;
    person.carer = None;
    let causes = Causes::new(ctx.trigger()).with(Cause::Rule(rule()));
    found(domain, &[sim], &causes, ctx);
}

/// `dead` died now (already in the archive, its partnership ended): it leaves its
/// household, and each dependent child it cared for gets the other biological parent, else
/// the partner of the partnership the death ended (`partner`), if he lives in the child's
/// household and is fit to care; the others get a household of their own together, the
/// eldest moving first (part 1 of the orphan rule), without a carer.
pub(crate) fn death_moves(
    domain: &mut DomainState,
    dead: SimId,
    household: HouseholdId,
    partner: Option<SimId>,
    death: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let now = ctx.now();
    leave(domain, dead, household, now);
    let children: Vec<SimId> = domain
        .sims
        .iter()
        .filter(|(_, person)| person.carer == Some(dead) && dependent(person, now))
        .map(|(id, _)| *id)
        .collect();
    let fit = |domain: &DomainState, carer: SimId, child_home: HouseholdId| {
        domain
            .sims
            .get(&carer)
            .is_some_and(|p| p.household == child_home && !dependent(p, now))
    };
    let mut orphans = Vec::new();
    for child in children {
        let person = living(domain, child);
        let home = person.household;
        let parents: Vec<SimId> = person
            .parents
            .map(|p| alloc::vec![p.mother, p.father])
            .unwrap_or_default();
        let parent = parents
            .into_iter()
            .find(|&p| p != dead && fit(domain, p, home));
        let successor = parent.or_else(|| partner.filter(|&p| fit(domain, p, home)));
        match successor {
            Some(carer) => living_mut(domain, child).carer = Some(carer),
            None => orphans.push(child),
        }
    }
    if orphans.is_empty() {
        return;
    }
    orphans.sort_by_key(|sim| {
        let person = living(domain, *sim);
        (person.born, person.lineage_key)
    });
    for orphan in &orphans {
        living_mut(domain, *orphan).carer = None;
    }
    let causes = Causes::new(Cause::Event(death)).with(Cause::Rule(rule()));
    found(domain, &orphans, &causes, ctx);
}

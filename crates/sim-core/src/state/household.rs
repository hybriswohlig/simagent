//! Households (ADR-0018 section 4): every living person lives in one household. The
//! couples of `Start` and the adult founders found theirs at the start, a child is born
//! into its mother's, a couple founds a new one when it pairs and takes the dependent
//! children each of them cares for, the man founds a new one at a separation and takes the
//! children he cares for, and a person founds his own when he leaves home at the age drawn
//! from the table of the data. A dependent child (under 18, not moved out) has a carer in
//! its household; when the carer dies or leaves home without it, the other parent or the
//! carer's partner in the household takes over, else the orphan rule places the children:
//! with their nearest relative of 18 or more, else with a foster mother, else in a household
//! of their own.

use alloc::collections::BTreeSet;
use alloc::vec::Vec;

use super::conception::month_index;
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

/// The age at which a person leaves home, from its draw.
pub(crate) fn leave_age(data: &Data, seed: u64, key: LineageKey) -> i64 {
    let u = Draws::new(WorldSeed::new(seed)).unit(leave_key(key));
    age_of(u, &data.demography().leave_home)
}

/// `16 + n` for the smallest `n` whose share of the table, summed from 16 on and divided by
/// the sum of the table, lies above `u`; the last age if none does.
fn age_of(u: f64, weights: &[f64]) -> i64 {
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
/// founder under 18: moved out and alone if the drawn age of leaving is reached at the
/// start, else placed by the orphan rule (founders are not related: a foster mother, else
/// a household of his own), with his leaving home planned by his creation entry after
/// that. `founders` holds each founder's SimId and creation entry; `couples` the couples
/// as places in the list.
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
        if leaves <= now {
            living_mut(domain, sim).moved_out = true;
            settle(domain, &[sim], ctx);
            continue;
        }
        match placement(domain, sim, now) {
            Placement::Alone => settle(domain, &[sim], ctx),
            taken => {
                let home = take_in(domain, &[sim], &taken);
                join(domain, sim, home);
                ctx.emit(
                    LogPayload::Moved { household: home },
                    alloc::vec![Subject::Sim(sim)],
                    causes.clone(),
                );
            }
        }
        plan_leave(domain, data, sim, added, ctx);
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

/// `sim` leaves home now: a household of its own, moved out, no carer. Each dependent
/// child it cared for, left behind, gets a successor or the orphan rule.
pub(crate) fn on_leave_home(domain: &mut DomainState, sim: SimId, ctx: &mut SimCtx<'_, '_>) {
    // The event that runs right now is no longer pending.
    domain.pending.remove(&(sim, Slot::LeaveHome));
    let person = living_mut(domain, sim);
    person.moved_out = true;
    person.carer = None;
    let causes = Causes::new(ctx.trigger()).with(Cause::Rule(rule()));
    found(domain, &[sim], &causes, ctx);
    // Moved out only now, so never in a partnership: no partner to take over.
    let orphans = succeed(domain, sim, None, ctx.now());
    place(domain, &orphans, &causes, ctx);
}

/// `dead` died now (already in the archive, its partnership ended): it leaves its
/// household, and each dependent child it cared for gets a successor or the orphan rule.
/// `partner` is the partner of the partnership the death ended.
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
    let orphans = succeed(domain, dead, partner, now);
    let causes = Causes::new(Cause::Event(death)).with(Cause::Rule(rule()));
    place(domain, &orphans, &causes, ctx);
}

/// The succession for the dependent children `gone` cared for (ADR-0018 section 4): the
/// other biological parent, else `partner`, if he lives in the child's household and is
/// fit to care. Returns the children without a successor, the eldest first (born, then
/// lineage key), for the orphan rule.
fn succeed(
    domain: &mut DomainState,
    gone: SimId,
    partner: Option<SimId>,
    now: SimTime,
) -> Vec<SimId> {
    let children: Vec<SimId> = domain
        .sims
        .iter()
        .filter(|(_, person)| person.carer == Some(gone) && dependent(person, now))
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
        // `gone` is never fit here: the dead is in the archive already, and who left home
        // lives in a new household.
        let parent = parents.into_iter().find(|&p| fit(domain, p, home));
        let successor = parent.or_else(|| partner.filter(|&p| fit(domain, p, home)));
        match successor {
            Some(carer) => living_mut(domain, child).carer = Some(carer),
            None => orphans.push(child),
        }
    }
    orphans.sort_by_key(|sim| {
        let person = living(domain, *sim);
        (person.born, person.lineage_key)
    });
    orphans
}

/// Where the orphan rule puts a group (ADR-0018 section 4).
enum Placement {
    /// With the nearest relative of 18 or more, its carer and social parent.
    Relative(SimId),
    /// With the foster mother the draw chooses (the carer) and the man of her open
    /// partnership; both become social parents.
    Foster(SimId, SimId),
    /// In a household of their own, without a carer.
    Alone,
}

/// The placement of a group whose eldest child is `representative`, at `now`.
fn placement(domain: &mut DomainState, representative: SimId, now: SimTime) -> Placement {
    if let Some(relative) = nearest_relative(domain, representative, now) {
        return Placement::Relative(relative);
    }
    match foster_mother(domain, representative, now) {
        Some((woman, man)) => Placement::Foster(woman, man),
        None => Placement::Alone,
    }
}

/// Among the living of 18 or more related to `representative` (φ above 0), the one with
/// the greatest φ; tie: the smaller lineage key.
fn nearest_relative(
    domain: &mut DomainState,
    representative: SimId,
    now: SimTime,
) -> Option<SimId> {
    let adults: Vec<(SimId, LineageKey)> = domain
        .sims
        .iter()
        .filter(|(id, person)| {
            **id != representative && market::of_age(person.born, now, ADULT_YEARS)
        })
        .map(|(id, person)| (*id, person.lineage_key))
        .collect();
    let mut nearest: Option<(f64, LineageKey, SimId)> = None;
    for (id, key) in adults {
        let phi = market::phi(domain, representative, id);
        if phi <= 0.0 {
            continue;
        }
        if nearest.is_none_or(|(best, best_key, _)| closer((phi, key), (best, best_key))) {
            nearest = Some((phi, key, id));
        }
    }
    nearest.map(|(_, _, id)| id)
}

/// Whether a relative `(φ, lineage key)` comes before the best so far: the greater φ, and
/// at the same φ the smaller key.
fn closer((phi, key): (f64, LineageKey), (best, best_key): (f64, LineageKey)) -> bool {
    phi > best || (phi == best && key < best_key)
}

/// Among the living women of 18 or more in an open partnership, the one `FOSTER_RANK`
/// ranks first for `representative` in the month of `now`, with her partner.
fn foster_mother(
    domain: &DomainState,
    representative: SimId,
    now: SimTime,
) -> Option<(SimId, SimId)> {
    let seed = domain
        .world_seed
        .unwrap_or_else(|| panic!("invariant household.before_start"));
    let month = month_index(now).unwrap_or_else(|| panic!("invariant household.before_start"));
    let candidates: Vec<(LineageKey, SimId, SimId)> =
        domain
            .open
            .iter()
            .filter_map(|(sim, id)| {
                let partnership = domain.partnerships.get(id)?;
                let woman = domain.sims.get(sim)?;
                (partnership.woman == *sim && market::of_age(woman.born, now, ADULT_YEARS))
                    .then_some((woman.lineage_key, *sim, partnership.man))
            })
            .collect();
    let key = living(domain, representative).lineage_key;
    let draws = Draws::new(WorldSeed::new(seed));
    let chosen = draws.k_smallest(
        tags::FOSTER_RANK,
        key,
        month,
        candidates.iter().map(|(key, _, _)| *key),
        1,
    );
    let first = chosen.first()?;
    candidates
        .iter()
        .find(|(key, _, _)| key == first)
        .map(|&(_, woman, man)| (woman, man))
}

/// Gives each child of `group` the carer and the social parents of `placement` (a relative
/// or a foster home); returns the carer's household.
fn take_in(domain: &mut DomainState, group: &[SimId], placement: &Placement) -> HouseholdId {
    let (carer, social): (SimId, Vec<SimId>) = match *placement {
        Placement::Relative(relative) => (relative, alloc::vec![relative]),
        Placement::Foster(woman, man) => (woman, alloc::vec![woman, man]),
        Placement::Alone => panic!("invariant household.take_in_alone"),
    };
    for child in group {
        let person = living_mut(domain, *child);
        person.carer = Some(carer);
        let parents = person.parents.map(|p| [p.mother, p.father]);
        for adult in &social {
            let biological = parents.is_some_and(|both| both.contains(adult));
            if !biological && !person.social_parents.contains(adult) {
                person.social_parents.push(*adult);
            }
        }
    }
    living(domain, carer).household
}

/// The orphan rule for `group` (the eldest first): its placement decides once for all;
/// each child moves into the carer's household unless it lives there already, the eldest
/// first; alone, they found a household of their own together, without a carer.
fn place(domain: &mut DomainState, group: &[SimId], causes: &Causes, ctx: &mut SimCtx<'_, '_>) {
    let Some(&representative) = group.first() else {
        return;
    };
    match placement(domain, representative, ctx.now()) {
        Placement::Alone => {
            for child in group {
                living_mut(domain, *child).carer = None;
            }
            found(domain, group, causes, ctx);
        }
        taken => {
            let home = take_in(domain, group, &taken);
            for child in group {
                if living(domain, *child).household != home {
                    move_into(domain, *child, home, causes.clone(), ctx);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The nearest relative: the greater φ first, then the smaller lineage key; one that
    /// is not closer does not replace the best so far.
    #[test]
    fn the_nearest_relative_has_the_greater_kinship_then_the_smaller_key() {
        let tag = crate::data_id::DataId::new("test:kin").expect("id");
        let [low, high] = [0, 1].map(|n| LineageKey::founder(&tag, n));
        let (low, high) = if low < high { (low, high) } else { (high, low) };
        assert!(closer((0.25, high), (0.125, low)));
        assert!(!closer((0.125, low), (0.25, high)));
        assert!(closer((0.125, low), (0.125, high)));
        assert!(!closer((0.125, high), (0.125, low)));
        assert!(!closer((0.125, low), (0.125, low)));
    }

    /// Dario's table and the ages at and around its thresholds (`leave_home_age` of
    /// `tools/world_reference.py`); no share lies above 1, so the last age.
    #[test]
    fn the_age_of_leaving_home_from_the_draw() {
        let table = [0.02, 0.03, 0.3, 0.2, 0.15, 0.1, 0.08, 0.05, 0.04, 0.03];
        for (u, age) in [
            (0.0, 16),
            (0.019_999_999_999_999_997, 16),
            (0.02, 17),
            (0.0499, 17),
            (0.05, 18),
            (0.3499, 18),
            (0.35, 19),
            (0.5, 19),
            (0.700_000_000_000_000_1, 21),
            (0.9699, 24),
            (0.970_000_000_000_000_1, 25),
            (0.999_999_999_999_999_9, 25),
            (1.0, 25),
        ] {
            assert_eq!(age_of(u, &table), age, "u {u}");
        }
        // The shares, not the weights: the same table at half the scale (halving is exact,
        // so the shares are the same numbers).
        let half = table.map(|w| w / 2.0);
        assert_eq!(age_of(0.02, &half), 17);
        assert_eq!(age_of(0.3, &half), 18);
    }
}

//! The bonds of a person in a moved-in household (ADR-0030 section 2): one directed bond per
//! other person, formed when a household moves in and when a child is born in the house, with
//! the start values of the first matching role. A full list drops the weakest unprotected
//! bond. On moving out the bonds stay and are settled; on a death they go. Nothing here
//! changes a choice. The values and the eviction are `social::bond`.

use alloc::collections::BTreeSet;
use alloc::vec::Vec;

use super::{DomainState, Parents, market, partnership};
use crate::canonical::LoadedState;
use crate::data::Data;
use crate::ids::{EventId, SimId};
use crate::log::{Cause, Causes, LogPayload, Subject};
use crate::social::bond::{self, Bond, Formation, Role};
use crate::time::SimTime;
use crate::world::SimCtx;

/// φ from which a bond is kin, and protected from eviction (ADR-0030 section 2). The core's
/// kinship is the Malécot coefficient as an `f64`; 1/8 is exact in that representation.
const KIN_PHI: f64 = 0.125;

/// The living members of `household`, by SimId.
fn members(domain: &DomainState, household: crate::ids::HouseholdId) -> Vec<SimId> {
    (domain.members.get(&household))
        .map_or_else(Vec::new, |members| members.iter().copied().collect())
}

/// Every ordered pair of `members`, the holder then the other, both in SimId order.
fn all_pairs(members: &[SimId]) -> Vec<(SimId, SimId)> {
    let mut pairs = Vec::new();
    for &holder in members {
        for &other in members {
            if holder != other {
                pairs.push((holder, other));
            }
        }
    }
    pairs
}

/// The ordered pairs of `members` that include `who`, both directions.
fn touching(members: &[SimId], who: SimId) -> Vec<(SimId, SimId)> {
    (all_pairs(members).into_iter())
        .filter(|(holder, other)| *holder == who || *other == who)
        .collect()
}

/// The biological parents of a person, living or archived.
fn parents_of(domain: &DomainState, sim: SimId) -> Option<Parents> {
    (domain.sims.get(&sim).and_then(|person| person.parents))
        .or_else(|| domain.archive.get(&sim).and_then(|person| person.parents))
}

/// Full siblings: both parents known and the same two persons. A half sibling is kin.
fn full_siblings(a: Option<Parents>, b: Option<Parents>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.mother == b.mother && a.father == b.father,
        (None, _) | (_, None) => false,
    }
}

/// Whether `a` and `b` are the two of an open partnership.
fn open_partners(domain: &DomainState, a: SimId, b: SimId) -> bool {
    let Some(id) = partnership::open_partnership(domain, a) else {
        return false;
    };
    (domain.partnerships.get(&id)).is_some_and(|pair| {
        (pair.woman == a && pair.man == b) || (pair.woman == b && pair.man == a)
    })
}

/// The first matching role of `other` for `holder` (ADR-0030 section 2).
fn role_of(domain: &mut DomainState, data: &Data, holder: SimId, other: SimId) -> Role {
    let partner = open_partners(domain, holder, other);
    let mine = parents_of(domain, holder);
    let theirs = parents_of(domain, other);
    let other_is_my_parent = mine.is_some_and(|parents| parents.mother == other || parents.father == other);
    let other_is_my_child = theirs.is_some_and(|parents| parents.mother == holder || parents.father == holder);
    let sibling = full_siblings(mine, theirs);
    let kin = market::phi(domain, data, holder, other) >= KIN_PHI;
    bond::start_role(partner, other_is_my_parent, other_is_my_child, sibling, kin)
}

/// The others `owner`'s present bonds must not drop: a current housemate, or kin from φ = 1/8.
fn protected(domain: &mut DomainState, data: &Data, owner: SimId) -> BTreeSet<SimId> {
    let household = domain.sims.get(&owner).map(|person| person.household);
    let others: Vec<SimId> = (domain.house.social.bonds.get(&owner))
        .map_or_else(Vec::new, |list| list.iter().map(|bond| bond.other).collect());
    let mut kept = BTreeSet::new();
    for other in others {
        let mate = household.is_some_and(|household| {
            (domain.sims.get(&other)).is_some_and(|person| person.household == household)
        });
        if mate || market::phi(domain, data, owner, other) >= KIN_PHI {
            kept.insert(other);
        }
    }
    kept
}

/// Whether `owner` already has a bond to `other`.
fn has_bond(domain: &DomainState, owner: SimId, other: SimId) -> bool {
    (domain.house.social.bonds.get(&owner)).is_some_and(|list| list.iter().any(|bond| bond.other == other))
}

/// Forms each missing bond of `pairs`, in the order given. `cause` is the entry
/// `core:bond_formed` names; a bond that fell out follows as `core:bond_dropped` caused by
/// that entry. An empty list is not stored. Without social data, nothing is formed.
fn form_pairs(
    domain: &mut DomainState,
    data: &Data,
    pairs: Vec<(SimId, SimId)>,
    cause: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let Some(social) = data.social() else {
        return;
    };
    let now = ctx.now();
    let cap = usize::from(social.limits.bonds);
    let familiarity_fade = social.bonds.familiarity_fade;
    let grudge_fade = social.bonds.grudge_fade;
    let starts = social.bonds.start.clone();
    for (holder, other) in pairs {
        if has_bond(domain, holder, other) {
            continue;
        }
        let role = role_of(domain, data, holder, other);
        let new = Bond {
            other,
            values: bond::values_for(&starts, role),
            since: now,
        };
        let kept = protected(domain, data, holder);
        let outcome = {
            let list = domain.house.social.bonds.entry(holder).or_default();
            bond::insert(list, new, now, familiarity_fade, grudge_fade, cap, |other| {
                kept.contains(&other)
            })
        };
        if (domain.house.social.bonds.get(&holder)).is_some_and(Vec::is_empty) {
            domain.house.social.bonds.remove(&holder);
        }
        let Formation::Formed { dropped } = outcome else {
            continue;
        };
        let wrote = ctx.emit(
            LogPayload::BondFormed {
                other,
                values: new.values,
            },
            alloc::vec![Subject::Sim(holder)],
            Causes::new(Cause::Event(cause)),
        );
        if let Some(other) = dropped {
            ctx.emit(
                LogPayload::BondDropped { other },
                alloc::vec![Subject::Sim(holder)],
                Causes::new(Cause::Event(wrote)),
            );
        }
    }
}

/// A household moved into its house, caused by its `core:moved_in`: every ordered pair of
/// its members forms the missing bond. Call it after [`resume`] of each member.
pub(super) fn on_move_in(
    domain: &mut DomainState,
    data: &Data,
    members: &[SimId],
    entry: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    form_pairs(domain, data, all_pairs(members), entry, ctx);
}

/// `sim` is in the moved-in household `household` (a birth, or a person who joined), caused
/// by `by`: the missing bonds between `sim` and each housemate, both directions. A bond that
/// already exists, from a birth this second or from before a move-out, is left as it is.
pub(super) fn on_join(
    domain: &mut DomainState,
    data: &Data,
    household: crate::ids::HouseholdId,
    sim: SimId,
    by: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    if !domain.house.households.contains_key(&household) {
        return;
    }
    let members = members(domain, household);
    form_pairs(domain, data, touching(&members, sim), by, ctx);
}

/// The child `child` was born to `mother` in this second (`born`): if her household is in a
/// house, the child and each housemate form both bonds (ADR-0030 section 2). A child under 6
/// has them. The father has one only when he lives in that household.
pub(super) fn child_born(
    domain: &mut DomainState,
    data: &Data,
    mother: SimId,
    child: SimId,
    born: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let Some(person) = domain.sims.get(&mother) else {
        return;
    };
    on_join(domain, data, person.household, child, born, ctx);
}

/// A person leaves a moved-in household now (ADR-0030 section 2, "Auszug und Tod"): one who
/// lives on keeps every bond, familiarity and grudge written down at `now` (`since` is this
/// second until the person moves in again); one who died or left the town loses all of them.
/// Nothing is written per bond.
pub(super) fn leave(domain: &mut DomainState, data: &Data, sim: SimId, now: SimTime) {
    let Some(mut list) = domain.house.social.bonds.remove(&sim) else {
        return;
    };
    if !domain.sims.contains_key(&sim) {
        return;
    }
    let Some(social) = data.social() else {
        return;
    };
    let (familiarity_fade, grudge_fade) = (social.bonds.familiarity_fade, social.bonds.grudge_fade);
    for bond in &mut list {
        bond.settle(now, familiarity_fade, grudge_fade);
    }
    if !list.is_empty() {
        domain.house.social.bonds.insert(sim, list);
    }
}

/// A person with bonds comes into a moved-in household (ADR-0030 section 2): what was settled
/// on moving out did not fall while the person was outside, so the values stay and `since`
/// is `now`. No entry.
pub(super) fn resume(domain: &mut DomainState, sim: SimId, now: SimTime) {
    let Some(list) = domain.house.social.bonds.get_mut(&sim) else {
        return;
    };
    for bond in list {
        bond.since = now;
    }
}

/// Whether the bonds of a loaded state are ones the core can reach (ADR-0030 section 5):
/// only with social data; each list belongs to a living person and is not empty, sorted by
/// the other person without a repeat, at most `limits.bonds` long; each bond is to someone
/// else who exists (living or archived), each value at most 1000, written down by the open
/// second. There is no bond event.
pub(crate) fn consistent(loaded: &LoadedState, data: &Data) -> bool {
    let bonds = &loaded.house.social.bonds;
    if bonds.is_empty() {
        return true;
    }
    let Some(social) = data.social() else {
        return false;
    };
    let clock = loaded.closed_through.as_secs();
    let cap = usize::from(social.limits.bonds);
    let known = |sim: SimId| loaded.sims.contains_key(&sim) || loaded.archive.contains_key(&sim);
    bonds.iter().all(|(&owner, list)| {
        loaded.sims.contains_key(&owner)
            && !list.is_empty()
            && list.len() <= cap
            && list.windows(2).all(|pair| pair[0].other < pair[1].other)
            && list.iter().all(|bond| {
                bond.other != owner
                    && known(bond.other)
                    && bond.values.iter().all(|value| *value <= bond::MOST)
                    && bond.since.as_secs() <= clock + 1
            })
    })
}

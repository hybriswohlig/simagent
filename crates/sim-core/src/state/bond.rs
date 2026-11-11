//! The bonds of a person in a moved-in household (ADR-0030 section 2): one directed bond per
//! other person, formed when a household moves in and when a child is born in the house, with
//! the start values of the first matching role. A full list drops the weakest unprotected
//! bond. On moving out the bonds stay and are settled. On a death or an emigration the
//! owner's own list goes (`left_world`), wherever it lived, read like the memories. Nothing
//! here changes a choice. The values and the eviction are `social::bond`.

use alloc::collections::BTreeSet;
use alloc::vec::Vec;

use super::{DomainState, Parents, market, partnership};
use crate::canonical::LoadedState;
use crate::data::Data;
use crate::ids::{EventId, SimId};
use crate::log::{Cause, Causes, LogPayload, Subject};
use crate::social::bond::{self, Bond, Formation, Role, TRUST};
use crate::time::SimTime;
use crate::world::SimCtx;

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
    let other_is_my_parent =
        mine.is_some_and(|parents| parents.mother == other || parents.father == other);
    let other_is_my_child =
        theirs.is_some_and(|parents| parents.mother == holder || parents.father == holder);
    let sibling = full_siblings(mine, theirs);
    let kin = market::kin_from_eighths(domain, data, holder, other, 1);
    bond::start_role(partner, other_is_my_parent, other_is_my_child, sibling, kin)
}

/// The others `owner`'s present bonds must not drop: a current housemate, or kin from φ = 1/8.
fn protected(domain: &mut DomainState, data: &Data, owner: SimId) -> BTreeSet<SimId> {
    let household = domain.sims.get(&owner).map(|person| person.household);
    let others: Vec<SimId> = (domain.house.social.bonds.get(&owner))
        .map_or_else(Vec::new, |list| {
            list.iter().map(|bond| bond.other).collect()
        });
    let mut kept = BTreeSet::new();
    for other in others {
        let mate = household.is_some_and(|household| {
            (domain.sims.get(&other)).is_some_and(|person| person.household == household)
        });
        if mate || market::kin_from_eighths(domain, data, owner, other, 1) {
            kept.insert(other);
        }
    }
    kept
}

/// Whether `owner` already has a bond to `other`.
fn has_bond(domain: &DomainState, owner: SimId, other: SimId) -> bool {
    (domain.house.social.bonds.get(&owner))
        .is_some_and(|list| list.iter().any(|bond| bond.other == other))
}

/// Whether the missing bond of `holder` to `other` would be refused (ADR-0030 section 2).
/// The list is full and every bond in it is protected, so none can be dropped. An existing
/// bond, no social data, a shorter list, or a list that can drop an unprotected bond is not
/// a refusal. Nothing is written.
pub(super) fn formation_refused(
    domain: &mut DomainState,
    data: &Data,
    holder: SimId,
    other: SimId,
) -> bool {
    let Some(social) = data.social() else {
        return false;
    };
    if has_bond(domain, holder, other) {
        return false;
    }
    let list_len = (domain.house.social.bonds.get(&holder))
        .map(|list| list.len())
        .unwrap_or(0);
    let cap = usize::from(social.limits.bonds);
    list_len >= cap && protected(domain, data, holder).len() == list_len
}

/// What forming one bond wrote.
pub(super) enum Formed {
    /// `core:bond_formed`, and `core:bond_dropped` when one fell out. `last` is the later
    /// entry; `entries` is how many named the holder.
    Wrote { last: EventId, entries: u16 },
    /// The list was full and every bond in it is protected.
    Refused,
}

/// Forms the missing bond of `holder` to `other`. `None` without social data, or when the
/// bond already exists. `cause` is what `core:bond_formed` names; a bond that fell out
/// follows as `core:bond_dropped` caused by that entry. An empty list is not stored.
pub(super) fn form_one(
    domain: &mut DomainState,
    data: &Data,
    holder: SimId,
    other: SimId,
    cause: Cause,
    ctx: &mut SimCtx<'_, '_>,
) -> Option<Formed> {
    let social = data.social()?;
    if has_bond(domain, holder, other) {
        return None;
    }
    let now = ctx.now();
    let cap = usize::from(social.limits.bonds);
    let familiarity_fade = social.bonds.familiarity_fade;
    let grudge_fade = social.bonds.grudge_fade;
    let starts = social.bonds.start.clone();
    let role = role_of(domain, data, holder, other);
    let new = Bond {
        other,
        values: bond::values_for(&starts, role),
        since: now,
    };
    let kept = protected(domain, data, holder);
    let outcome = {
        let list = domain.house.social.bonds.entry(holder).or_default();
        bond::insert(
            list,
            new,
            now,
            familiarity_fade,
            grudge_fade,
            cap,
            |other| kept.contains(&other),
        )
    };
    if (domain.house.social.bonds.get(&holder)).is_some_and(Vec::is_empty) {
        domain.house.social.bonds.remove(&holder);
    }
    match outcome {
        Formation::Refused => Some(Formed::Refused),
        Formation::Formed { dropped } => {
            let wrote = ctx.emit(
                LogPayload::BondFormed {
                    other,
                    values: new.values,
                },
                alloc::vec![Subject::Sim(holder)],
                Causes::new(cause),
            );
            let (last, entries) = match dropped {
                Some(other) => {
                    let dropped_id = ctx.emit(
                        LogPayload::BondDropped { other },
                        alloc::vec![Subject::Sim(holder)],
                        Causes::new(Cause::Event(wrote)),
                    );
                    (dropped_id, 2)
                }
                None => (wrote, 1),
            };
            Some(Formed::Wrote { last, entries })
        }
    }
}

/// Forms each missing bond of `pairs`, in the order given. `cause` is the entry
/// `core:bond_formed` names. Without social data, nothing is formed.
fn form_pairs(
    domain: &mut DomainState,
    data: &Data,
    pairs: Vec<(SimId, SimId)>,
    cause: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    for (holder, other) in pairs {
        let _ = form_one(domain, data, holder, other, Cause::Event(cause), ctx);
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

/// A person leaves a moved-in household now (ADR-0030 section 2): one who lives on keeps
/// every bond, familiarity and grudge written down at `now` (`since` is this second until
/// the person moves in again). A plain move-out is this case. Nothing is written per bond.
/// A death or an emigration drops the list in [`left_world`], including one frozen here; if
/// this runs after the person has already left the world, the list is not put back.
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

/// A person who leaves the world loses every bond of their own, wherever the list lived
/// (ADR-0030 section 2). The section keeps a leaver's own bonds "frozen like the memories";
/// memories are lost at death and at emigration, so these bonds are too. A plain move-out
/// still freezes them ([`leave`]). Bonds of others to this person stay and keep fading.
/// Nothing is written per bond.
pub(super) fn left_world(domain: &mut DomainState, sim: SimId) {
    domain.house.social.bonds.remove(&sim);
}

/// The trust a new bond of `holder` to `other` would start at (ADR-0031 section 2: the
/// start value of the role when there is no edge). Hearing uses this only when the edge
/// is missing. It does not form the bond.
pub(super) fn start_trust(
    domain: &mut DomainState,
    data: &Data,
    holder: SimId,
    other: SimId,
) -> u16 {
    let social = data
        .social()
        .unwrap_or_else(|| panic!("invariant belief.social"));
    let role = role_of(domain, data, holder, other);
    bond::values_for(&social.bonds.start, role)[TRUST]
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
/// only with social data. The owner of a list is a living sim. The `other` of a bond is any
/// known person, living or archived, and never the owner. Each list is not empty, sorted by
/// the other person without a repeat, at most `limits.bonds` long; each value is at most
/// 1000, written down by the open second. There is no bond event.
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

/// A holder, the bonds on that list, and one housemate the list does not name.
#[cfg(test)]
pub(crate) struct Listed {
    pub domain: DomainState,
    pub data: Data,
    pub holder: SimId,
    /// A housemate with no bond from the holder, and not kin (φ = 0).
    pub newcomer: SimId,
}

/// `bonds` housemate bonds, every one protected, plus `newcomer`. `drop_last` moves the
/// last bonded person into another household, so that bond is neither a housemate nor kin
/// and the list can drop it (ADR-0030 section 2). Housemates are cheaper to build than kin
/// of φ = 1/8: a shared household, and founders with no parents so φ is 0.
/// The larger id holds `limits.bonds` protected housemate bonds. The smaller id has an
/// empty list and no bond to the larger one, so a talk is refused on the larger side
/// while the smaller side still has room (ADR-0030 section 2).
#[cfg(test)]
pub(crate) fn listed_larger_full() -> Listed {
    listed_with(34, 1, 32, false)
}

#[cfg(test)]
pub(super) fn listed(bonds: u64, drop_last: bool) -> Listed {
    listed_with(1, bonds + 2, bonds, drop_last)
}

#[cfg(test)]
fn listed_with(holder_n: u64, newcomer_n: u64, bonds: u64, drop_last: bool) -> Listed {
    use alloc::collections::BTreeMap;
    use alloc::vec::Vec;

    use super::Sim;
    use crate::data_id::DataId;
    use crate::genome::{Genome, Sex};
    use crate::house::state::MovedIn;
    use crate::ids::HouseholdId;
    use crate::lineage::LineageKey;
    use crate::social::bond::Bond;
    use crate::social::data::SocialData;
    use crate::time::{SimTime, YEAR};

    fn sid(n: u64) -> SimId {
        SimId::try_from(n).expect("valid")
    }

    fn hid(n: u64) -> HouseholdId {
        HouseholdId::try_from(n).expect("valid")
    }

    let home = hid(1);
    let away = hid(2);
    let holder = sid(holder_n);
    let newcomer = sid(newcomer_n);
    let mut domain = DomainState::new();

    let add = |domain: &mut DomainState, id: SimId, household: HouseholdId| {
        let sex = Sex::Female;
        let lineage_key = LineageKey::founder(&DataId::new("test:talk").expect("tag"), id.get());
        domain.keys.insert(lineage_key, id);
        domain.sims.insert(
            id,
            Sim {
                born: SimTime::from_secs(-30 * YEAR.as_secs()),
                lineage_key,
                sex,
                genome: Genome::reference(sex),
                afflictions: Vec::new(),
                conceptions: 0,
                births: 0,
                partnerships: 0,
                household,
                moved_out: false,
                carer: None,
                parents: None,
                social_parents: Vec::new(),
                pool: None,
            },
        );
    };

    add(&mut domain, holder, home);
    let mut list = Vec::new();
    for n in 2..=bonds + 1 {
        let id = sid(n);
        let household = if drop_last && n == bonds + 1 {
            away
        } else {
            home
        };
        add(&mut domain, id, household);
        list.push(Bond {
            other: id,
            values: [0; 6],
            since: SimTime::from_secs(0),
        });
    }
    if !list.is_empty() {
        domain.house.social.bonds.insert(holder, list);
    }
    add(&mut domain, newcomer, home);

    let moved = MovedIn {
        house: DataId::new("test:talk").expect("tag"),
        morning: true,
        since: SimTime::from_secs(0),
        last_talk: BTreeMap::new(),
    };
    domain.house.households.insert(home, moved.clone());
    if drop_last {
        domain.house.households.insert(away, moved);
    }

    let raw: SocialData = serde_json::from_str(include_str!("../../../../data/social/social.json"))
        .expect("social json");
    let data = crate::data::testing::data().with_social(raw.checked().expect("social checks"));
    Listed {
        domain,
        data,
        holder,
        newcomer,
    }
}

#[cfg(test)]
mod tests {
    use super::market;
    use super::{formation_refused, has_bond, listed};

    /// The cap in ADR-0030 section 2, and in `data/social/social.json`.
    const BONDS: u64 = 32;

    fn cap(case: &super::Listed) -> u64 {
        u64::from(case.data.social().expect("social").limits.bonds)
    }

    /// Thirty-two bonds, each a housemate, and no bond to a new housemate who is not kin.
    /// The missing bond is refused. Her own list is empty, so that direction is not.
    #[test]
    fn a_full_list_of_protected_bonds_refuses_a_new_housemate() {
        let mut case = listed(BONDS, false);
        assert_eq!(cap(&case), BONDS);
        let home = case.domain.sims[&case.holder].household;
        let bonded: alloc::vec::Vec<_> = case.domain.house.social.bonds[&case.holder]
            .iter()
            .map(|bond| bond.other)
            .collect();
        assert_eq!(u64::try_from(bonded.len()).expect("fits"), BONDS);
        assert!(
            bonded
                .iter()
                .all(|other| case.domain.sims[other].household == home)
        );
        let (holder, newcomer) = (case.holder, case.newcomer);
        assert!(!has_bond(&case.domain, holder, newcomer));
        assert_eq!(
            case.domain.sims[&newcomer].household,
            case.domain.sims[&holder].household
        );
        assert!(!market::kin_from_eighths(
            &mut case.domain,
            &case.data,
            holder,
            newcomer,
            1
        ));
        assert!(formation_refused(
            &mut case.domain,
            &case.data,
            holder,
            newcomer
        ));
        assert!(!formation_refused(
            &mut case.domain,
            &case.data,
            newcomer,
            holder
        ));
    }

    /// One of the thirty-two is neither a housemate nor kin, so the list can drop it.
    #[test]
    fn a_full_list_with_one_unprotected_bond_does_not_refuse() {
        let mut case = listed(BONDS, true);
        assert_eq!(cap(&case), BONDS);
        let (holder, newcomer) = (case.holder, case.newcomer);
        let last = case.domain.house.social.bonds[&holder]
            .last()
            .expect("a bond")
            .other;
        assert_ne!(
            case.domain.sims[&last].household,
            case.domain.sims[&holder].household
        );
        assert!(!market::kin_from_eighths(
            &mut case.domain,
            &case.data,
            holder,
            last,
            1
        ));
        assert!(!formation_refused(
            &mut case.domain,
            &case.data,
            holder,
            newcomer
        ));
    }

    /// Thirty-one bonds are under the cap, protected or not.
    #[test]
    fn a_list_shorter_than_the_cap_does_not_refuse() {
        let mut case = listed(BONDS - 1, false);
        assert_eq!(cap(&case), BONDS);
        let (holder, newcomer) = (case.holder, case.newcomer);
        assert_eq!(
            case.domain.house.social.bonds[&holder].len(),
            usize::try_from(BONDS - 1).expect("fits")
        );
        assert!(!formation_refused(
            &mut case.domain,
            &case.data,
            holder,
            newcomer
        ));
    }
}

//! The feelings of a person in a moved-in household (ADR-0032): what writes them (the occasions of
//! the table: a birth, a death, a partnership that began or ended, a need at 0, the outcome of a
//! talk), the contagion of anger on entering a room, what a person keeps on moving out and what
//! it loses by leaving the world. The values are in `house::state::HouseState::social`; the
//! analytic fade, the change by an occasion and the contagion are `social::feeling`. Nothing
//! here plans an event: every value is read at the second it is needed. Nothing feels without
//! social data in the world's data.

use alloc::vec;
use alloc::vec::Vec;

use super::{DomainState, memory};
use crate::canonical::LoadedState;
use crate::data::Data;
use crate::house::state::Place;
use crate::ids::{EventId, SimId};
use crate::log::{Cause, Causes, LogPayload, Subject};
use crate::social::bond::AFFECTION;
use crate::social::data::{Occasion, SocialData};
use crate::social::feeling::{self, ANGER, Feel, Row, read};
use crate::time::SimTime;
use crate::world::SimCtx;

/// The affection of `owner` to `other` at `now`, if `owner` has a bond to `other` (ADR-0032
/// section 3: without a bond the factor is 1).
fn affection_to(
    domain: &DomainState,
    social: &SocialData,
    owner: SimId,
    other: SimId,
    now: SimTime,
) -> Option<u16> {
    let (familiarity, grudge) = (social.bonds.familiarity_fade, social.bonds.grudge_fade);
    (domain.house.social.bonds.get(&owner))
        .and_then(|list| list.iter().find(|bond| bond.other == other))
        .map(|bond| bond.values_at(now, familiarity, grudge)[AFFECTION])
}

/// One row of an occasion before its band is read: the occasion and the person the band names.
/// The affection is read where the occasion is applied, at its second (ADR-0032 section 3).
#[derive(Clone, Copy, Debug)]
pub(super) struct Pending {
    pub occasion: Occasion,
    pub band: Option<SimId>,
}

/// Applies the rows of one occasion to `sim` and writes `core:felt` with the values after, caused
/// by `cause` (ADR-0032 sections 1 to 3, 6). Nothing is written, and nothing stored changes, if
/// `sim` has no feelings (not from `min_age` in a moved-in household), the data has none, or
/// no clamped channel and not the temper changes. Returns the entry.
pub(super) fn feel(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    rows: &[Pending],
    cause: Cause,
    ctx: &mut SimCtx<'_, '_>,
) -> Option<EventId> {
    let social = data.social()?;
    let now = ctx.now();
    if !memory::remembers(domain, social, sim, now) {
        return None;
    }
    let weighed: Vec<Row> = rows
        .iter()
        .map(|row| Row {
            steps: social.feelings.steps(row.occasion),
            affection: row
                .band
                .and_then(|other| affection_to(domain, social, sim, other, now)),
        })
        .collect();
    write(domain, data, sim, &weighed, cause, ctx)
}

/// `rows` applied to `sim` as they are (`feel` has weighed their bands) and written.
fn write(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    rows: &[Row],
    cause: Cause,
    ctx: &mut SimCtx<'_, '_>,
) -> Option<EventId> {
    let social = data.social()?;
    let now = ctx.now();
    let stored = domain.house.social.feelings.get(&sim);
    let after = feeling::apply(stored, now, rows, &social.feelings)?;
    if after.is_zero() {
        domain.house.social.feelings.remove(&sim);
    } else {
        domain.house.social.feelings.insert(sim, after);
    }
    let [joy, anger, sadness, fear] = after.channels;
    let felt = LogPayload::Felt {
        joy,
        anger,
        sadness,
        fear,
        temper: after.temper,
    };
    Some(ctx.emit(felt, vec![Subject::Sim(sim)], Causes::new(cause)))
}

/// The own child `child`, born now with the entry `born`, is a joy for its mother and its father
/// (ADR-0032 section 3): each who may receive the memory of it, the same entitlement as
/// `memory::child_born`, the mother first. No band.
pub(super) fn child_born(
    domain: &mut DomainState,
    data: &Data,
    (mother, father): (SimId, Option<SimId>),
    born: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let row = [Pending {
        occasion: Occasion::Birth,
        band: None,
    }];
    let _ = feel(domain, data, mother, &row, Cause::Event(born), ctx);
    if let Some(father) = father {
        let _ = feel(domain, data, father, &row, Cause::Event(born), ctx);
    }
}

/// The woman and the man of a partnership that began or, by separation, ended now (`by` its
/// entry) feel it, the woman first (ADR-0032 section 3): each who may receive the memory of it.
/// No band.
pub(super) fn partnership(
    domain: &mut DomainState,
    data: &Data,
    (woman, man): (SimId, SimId),
    began: bool,
    by: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let occasion = if began {
        Occasion::PartnershipBegan
    } else {
        Occasion::Separated
    };
    let row = [Pending {
        occasion,
        band: None,
    }];
    let _ = feel(domain, data, woman, &row, Cause::Event(by), ctx);
    let _ = feel(domain, data, man, &row, Cause::Event(by), ctx);
}

/// Who feels what at a death (ADR-0032 section 3): the partner who lives on, if there is one,
/// the row of the partner's death, and each relative who may receive the memory, in the order
/// given, the row of a relative's death. The partner has the first alone, though it was a
/// relative too.
fn death_rows(partner: Option<SimId>, relatives: &[SimId]) -> Vec<(SimId, Occasion)> {
    let mut rows = Vec::new();
    if let Some(partner) = partner {
        rows.push((partner, Occasion::PartnerLost));
    }
    for &relative in relatives {
        if Some(relative) != partner {
            rows.push((relative, Occasion::RelativeDied));
        }
    }
    rows
}

/// The death of `dead` (its entry `by`) grieves its partner, if it had one who lives on, and then
/// everyone who may receive the memory of a relative's death, by ascending SimId (ADR-0032
/// section 3, `death_rows`). The band of each reads its affection to the dead, whose own bonds the
/// archive dropped but not theirs. Call it after the memories.
pub(super) fn death(
    domain: &mut DomainState,
    data: &Data,
    dead: SimId,
    partner: Option<SimId>,
    by: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let relatives = memory::relatives_of(domain, data, dead, ctx.now());
    for (who, occasion) in death_rows(partner, &relatives) {
        let row = [Pending {
            occasion,
            band: Some(dead),
        }];
        let _ = feel(domain, data, who, &row, Cause::Event(by), ctx);
    }
}

/// A person's needs reached 0 in one `settle`, the last `core:need_empty` of the call `empty`
/// (ADR-0032 section 3): the row counts once, however many needs reached 0. No band.
pub(super) fn need_empty(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    empty: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let row = [Pending {
        occasion: Occasion::NeedEmpty,
        band: None,
    }];
    let _ = feel(domain, data, sim, &row, Cause::Event(empty), ctx);
}

/// `arriver` came into the room it stands in now (ADR-0032 section 4), the entry of the arrival
/// `entry` (`core:reached`, the planning `core:idle` or `core:came_home`): with an anger of at
/// least `min_anger` at this second it pushes every other person of its household from 6 whose
/// place is that room, in the order of their SimId. Each gets what `social::feeling::infected`
/// says; the temper follows by the rule of an occasion. Read before a `zero::settle` of the same
/// return, and the arriver's anger before anyone's write: a push is not passed on.
pub(super) fn arrived(
    domain: &mut DomainState,
    data: &Data,
    arriver: SimId,
    entry: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let Some(social) = data.social() else {
        return;
    };
    let rules = &social.feelings;
    let now = ctx.now();
    let anger = read(domain.house.social.feelings.get(&arriver), now, rules).0[ANGER];
    if anger < rules.contagion.min_anger {
        return;
    }
    let Some(Place::Room(room)) = domain.house.sims.get(&arriver).map(|person| person.place) else {
        return;
    };
    let Some(household) = domain.sims.get(&arriver).map(|person| person.household) else {
        return;
    };
    let there: Vec<SimId> = (domain.members.get(&household).into_iter().flatten())
        .copied()
        .filter(|&sim| sim != arriver)
        .filter(|sim| {
            let in_room = (domain.house.sims.get(sim))
                .is_some_and(|person| person.place == Place::Room(room));
            in_room && memory::remembers(domain, social, *sim, now)
        })
        .collect();
    // The members are a set in ascending order of SimId.
    for sim in there {
        let own = read(domain.house.social.feelings.get(&sim), now, rules).0[ANGER];
        let Some(new) = feeling::infected(anger, own, &rules.contagion) else {
            continue;
        };
        let delta = i16::try_from(i32::from(new) - i32::from(own))
            .unwrap_or_else(|_| panic!("invariant feeling.contagion"));
        let row = [Row {
            steps: [0, delta, 0, 0],
            affection: None,
        }];
        let _ = write(domain, data, sim, &row, Cause::Event(entry), ctx);
    }
}

/// A person leaves a moved-in household now (ADR-0032 section 1, "Auszug"): one who lives on has
/// its channels and its temper written down at their value now, `since` this second until it moves
/// in again; no entry (the entry of the move is the cause). Outside they do not fade, not even
/// on a load. One who died or left the town loses them in `left_world`; if this runs after the
/// person has already left the world, nothing is put back.
pub(super) fn leave(domain: &mut DomainState, data: &Data, sim: SimId, now: SimTime) {
    let Some(stored) = domain.house.social.feelings.remove(&sim) else {
        return;
    };
    if !domain.sims.contains_key(&sim) {
        return;
    }
    let Some(social) = data.social() else {
        return;
    };
    let (channels, temper) = stored.at(now, &social.feelings);
    let frozen = Feel {
        channels,
        since: now,
        temper,
        temper_since: now,
    };
    if !frozen.is_zero() {
        domain.house.social.feelings.insert(sim, frozen);
    }
}

/// A person who leaves the world loses its channels and its temper, wherever they lived
/// (ADR-0032 section 1). Nothing is written.
pub(super) fn left_world(domain: &mut DomainState, sim: SimId) {
    domain.house.social.feelings.remove(&sim);
}

/// A person with feelings comes into a moved-in household (ADR-0032 section 1): what was written
/// down on moving out did not fade while it was outside, so the values stay and `since` is now,
/// the gap not faded. No entry.
pub(super) fn resume(domain: &mut DomainState, sim: SimId, now: SimTime) {
    let Some(feel) = domain.house.social.feelings.get_mut(&sim) else {
        return;
    };
    feel.since = now;
    feel.temper_since = now;
}

/// Whether the feelings of a loaded state are ones the core can reach (ADR-0032 section 6): only
/// with social data. Each owner is a living person of `min_age` or older, in a moved-in household
/// or frozen after moving out; the four channels are at most 1000, the temper in -1000 to 1000,
/// not all five at 0 (such a person has no entry), both seconds written down by the open second.
/// There is no event and nothing fades on a load.
pub(crate) fn consistent(loaded: &LoadedState, data: &Data) -> bool {
    let feelings = &loaded.house.social.feelings;
    if feelings.is_empty() {
        return true;
    }
    let Some(social) = data.social() else {
        return false;
    };
    let clock = loaded.closed_through;
    feelings.iter().all(|(owner, feel)| {
        let Some(person) = loaded.sims.get(owner) else {
            return false;
        };
        let old_enough = super::years_between(person.born.as_secs(), clock.as_secs())
            .is_some_and(|years| years >= u32::from(social.limits.min_age));
        old_enough
            && feel.channels.iter().all(|&value| value <= feeling::MOST)
            && (-feeling::TEMPER_MOST..=feeling::TEMPER_MOST).contains(&feel.temper)
            && !feel.is_zero()
            && feel.since.as_secs() <= clock.as_secs() + 1
            && feel.temper_since.as_secs() <= clock.as_secs() + 1
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u64) -> SimId {
        SimId::try_from(n).expect("valid")
    }

    /// Acceptance 6 ("auch wenn er verwandt war"): a partner who is also a relative has the
    /// partner's row alone; the others have the relative's, by their order; without a partner
    /// everyone has the relative's.
    #[test]
    fn a_partner_who_is_a_relative_has_the_partner_row_alone() {
        let kin = [id(2), id(5), id(9)];
        assert_eq!(
            death_rows(Some(id(5)), &kin),
            [
                (id(5), Occasion::PartnerLost),
                (id(2), Occasion::RelativeDied),
                (id(9), Occasion::RelativeDied),
            ]
        );
        // A partner who is not kin has the one row, and the relatives theirs.
        assert_eq!(
            death_rows(Some(id(7)), &kin),
            [
                (id(7), Occasion::PartnerLost),
                (id(2), Occasion::RelativeDied),
                (id(5), Occasion::RelativeDied),
                (id(9), Occasion::RelativeDied),
            ]
        );
        assert_eq!(
            death_rows(None, &kin),
            [
                (id(2), Occasion::RelativeDied),
                (id(5), Occasion::RelativeDied),
                (id(9), Occasion::RelativeDied),
            ]
        );
        assert_eq!(death_rows(None, &[]), []);
    }
}

//! Beliefs and own deeds in the house state (ADR-0031 sections 1 to 3 and 7). A person of
//! `min_age` or older in a moved-in household gains them. Moving out freezes both lists.
//! Death and emigration drop them. A deed outside the window stays until a handler that
//! records a deed, or a move-in, cuts the list. The clock does not. Certainty does not fade,
//! so a freeze does not rewrite `since`. The pure rules are `social::belief`.

use alloc::vec;
use alloc::vec::Vec;

use super::{DomainState, whole_years, years_between};
use crate::canonical::LoadedState;
use crate::data::Data;
use crate::ids::{EventId, SimId};
use crate::log::{Cause, Causes, LogPayload, Subject};
use crate::social::belief::{
    Basis, Belief, BeliefInsertion, Statement, StatementKey, Taken, find, hear, insert, see,
};
use crate::social::data::SocialData;
use crate::social::talk::house_day;
use crate::time::SimTime;
use crate::world::SimCtx;

/// Whether `sim` lives in a moved-in household and is old enough to hold a belief or a deed
/// (ADR-0031 section 1, with the age of ADR-0030 section 3).
fn eligible(domain: &DomainState, social: &SocialData, sim: SimId, now: SimTime) -> bool {
    let Some(person) = domain.sims.get(&sim) else {
        return false;
    };
    domain.house.households.contains_key(&person.household)
        && whole_years(person.born, now) >= u32::from(social.limits.min_age)
}

/// Living and archived children of `holder` (ADR-0031 section 2: a `VaterVon` of an own
/// child is never dropped). Unborn children have lineage keys, not [`SimId`]s.
fn own_children(domain: &DomainState, holder: SimId) -> Vec<SimId> {
    let mut children = Vec::new();
    let is_child = |parents: Option<super::Parents>| {
        parents.is_some_and(|parents| parents.mother == holder || parents.father == holder)
    };
    for (id, person) in &domain.sims {
        if is_child(person.parents) {
            children.push(*id);
        }
    }
    for (id, person) in &domain.archive {
        if is_child(person.parents) {
            children.push(*id);
        }
    }
    children.sort();
    children
}

fn held(domain: &DomainState, holder: SimId, key: StatementKey) -> Option<Belief> {
    let list = domain.house.social.beliefs.get(&holder)?;
    find(list, key).copied()
}

/// What storing a belief wrote. `entries` counts `core:believed` and, when a different key
/// fell out, `core:unbelieved`.
pub(super) enum Store {
    Unchanged,
    Wrote { last: EventId, entries: u16 },
}

/// What hearing wrote. A confirmation is not a new deed for the bond step.
pub(super) enum HeardStore {
    Unchanged,
    Confirmed { last: EventId, entries: u16 },
    Stored { last: EventId, entries: u16 },
}

/// What the hearer is offered (ADR-0031 section 2).
pub(super) struct Heard {
    pub incoming: Statement,
    pub speaker: SimId,
    pub speaker_certainty: u16,
    pub speaker_belief: Option<Belief>,
    pub trust: u16,
}

fn emit_believed(
    holder: SimId,
    belief: Belief,
    replaced: bool,
    cause: Cause,
    ctx: &mut SimCtx<'_, '_>,
) -> EventId {
    ctx.emit(
        LogPayload::Believed {
            statement: belief.statement,
            certainty: belief.certainty,
            basis: belief.basis,
            source: belief.source,
            origin: belief.origin,
            replaced,
        },
        vec![Subject::Sim(holder)],
        Causes::new(cause),
    )
}

/// Puts `belief` on `holder`'s list. A key already held is replaced in place. A new key goes
/// through [`insert`]; a refused newcomer writes nothing, and an empty list is not stored.
/// `core:believed` comes first, then `core:unbelieved` of a different key that fell out.
#[allow(clippy::too_many_arguments)]
fn write(
    domain: &mut DomainState,
    data: &Data,
    holder: SimId,
    belief: Belief,
    replaced: bool,
    fresh_key: bool,
    cause: Cause,
    ctx: &mut SimCtx<'_, '_>,
) -> Option<(EventId, u16)> {
    if !fresh_key {
        let list = (domain.house.social.beliefs.get_mut(&holder))
            .unwrap_or_else(|| panic!("invariant belief.key"));
        let at = (list.binary_search_by_key(&belief.key(), Belief::key))
            .unwrap_or_else(|_| panic!("invariant belief.key"));
        list[at] = belief;
        return Some((emit_believed(holder, belief, replaced, cause, ctx), 1));
    }
    let social = data
        .social()
        .unwrap_or_else(|| panic!("invariant belief.social"));
    let cap = usize::from(social.limits.beliefs);
    let children = own_children(domain, holder);
    let outcome = {
        let list = domain.house.social.beliefs.entry(holder).or_default();
        insert(list, belief, &children, cap)
    };
    if matches!(outcome, BeliefInsertion::Refused) {
        if (domain.house.social.beliefs.get(&holder)).is_some_and(Vec::is_empty) {
            domain.house.social.beliefs.remove(&holder);
        }
        return None;
    }
    let id = emit_believed(holder, belief, false, cause, ctx);
    match outcome {
        BeliefInsertion::Displaced(old) => {
            let lost = ctx.emit(
                LogPayload::Unbelieved {
                    statement: old.statement,
                },
                vec![Subject::Sim(holder)],
                Causes::new(Cause::Event(id)),
            );
            Some((lost, 2))
        }
        BeliefInsertion::Added => Some((id, 1)),
        BeliefInsertion::Refused => None,
    }
}

/// `holder` sees `statement` (ADR-0031 section 2). A heard sentence of the same key is
/// replaced. Any other basis, and a person who may not hold beliefs, writes nothing.
pub(super) fn store_seen(
    domain: &mut DomainState,
    data: &Data,
    holder: SimId,
    statement: Statement,
    cause: Cause,
    ctx: &mut SimCtx<'_, '_>,
) -> Store {
    let Some(social) = data.social() else {
        return Store::Unchanged;
    };
    let now = ctx.now();
    if !eligible(domain, social, holder, now) {
        return Store::Unchanged;
    }
    let previous = held(domain, holder, statement.key());
    match see(previous, statement, now) {
        Taken::Unchanged => Store::Unchanged,
        Taken::Confirmed(_) => panic!("invariant belief.seen"),
        Taken::Stored { belief, replaced } => {
            match write(
                domain,
                data,
                holder,
                belief,
                replaced,
                previous.is_none(),
                cause,
                ctx,
            ) {
                Some((last, entries)) => Store::Wrote { last, entries },
                None => Store::Unchanged,
            }
        }
    }
}

/// `hearer` hears `heard` (ADR-0031 section 2). Confirmation keeps source and origin. A
/// `widerlegt` belief is left as it is. A refused newcomer writes nothing.
pub(super) fn store_heard(
    domain: &mut DomainState,
    data: &Data,
    hearer: SimId,
    heard: Heard,
    cause: Cause,
    ctx: &mut SimCtx<'_, '_>,
) -> HeardStore {
    let Some(social) = data.social() else {
        return HeardStore::Unchanged;
    };
    let now = ctx.now();
    if !eligible(domain, social, hearer, now) {
        return HeardStore::Unchanged;
    }
    let previous = held(domain, hearer, heard.incoming.key());
    let taken = hear(
        previous,
        heard.incoming,
        heard.speaker,
        heard.speaker_belief,
        heard.speaker_certainty,
        heard.trust,
        now,
        &social.beliefs,
    );
    let (belief, replaced, fresh, confirmed) = match taken {
        Taken::Unchanged => return HeardStore::Unchanged,
        Taken::Confirmed(belief) => (belief, false, false, true),
        Taken::Stored { belief, replaced } => (belief, replaced, previous.is_none(), false),
    };
    let Some((last, entries)) = write(domain, data, hearer, belief, replaced, fresh, cause, ctx)
    else {
        return HeardStore::Unchanged;
    };
    if confirmed {
        HeardStore::Confirmed { last, entries }
    } else {
        HeardStore::Stored { last, entries }
    }
}

/// Replaces the belief of this key in place. A refutation is not `core:believed`
/// (ADR-0031 section 5): the talk writes `core:refuted`. The key is already held.
pub(super) fn store_refuted(domain: &mut DomainState, holder: SimId, belief: Belief) {
    let list = (domain.house.social.beliefs.get_mut(&holder))
        .unwrap_or_else(|| panic!("invariant belief.key"));
    let at = (list.binary_search_by_key(&belief.key(), Belief::key))
        .unwrap_or_else(|_| panic!("invariant belief.key"));
    list[at] = belief;
}

/// Appends an own deed of today. Days outside the calendar window fall here, because this
/// handler already touches the list (ADR-0031 section 1, addendum 2026-11-11). The clock
/// does not cut them. The cap may still close a day inside that window (`dropped_day`).
/// An empty list is not stored. The cut writes nothing.
pub(super) fn record_deed(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    kind: u16,
    other: SimId,
    today: i64,
    now: SimTime,
) {
    let Some(social) = data.social() else {
        return;
    };
    if !eligible(domain, social, sim, now) {
        return;
    }
    let window = social.limits.deed_window_days;
    let cap = usize::from(social.limits.deeds);
    {
        let deeds = domain.house.social.deeds.entry(sim).or_default();
        deeds.prune_calendar(today, window);
        deeds.add(kind, other, today, cap);
    }
    let empty = (domain.house.social.deeds.get(&sim))
        .is_some_and(|deeds| deeds.items.is_empty() && deeds.dropped_day.is_none());
    if empty {
        domain.house.social.deeds.remove(&sim);
    }
}

/// Cuts deeds the new house day leaves outside the window (ADR-0031 section 1, addendum
/// 2026-11-11). A move-in touches the lists: each member's frozen deeds are judged against
/// the house day counted from the new `since`, which is `now`, so that day is 0. No entry.
/// Days are not renumbered. While the household is outside there is no house day, and the
/// clock never cuts the list. `dropped_day` stays: only the cap closes a day inside the window.
pub(super) fn judge_move_in(
    domain: &mut DomainState,
    data: &Data,
    members: &[SimId],
    now: SimTime,
) {
    let Some(social) = data.social() else {
        return;
    };
    let window = social.limits.deed_window_days;
    let today = house_day(now, now);
    for &sim in members {
        let empty = {
            let Some(deeds) = domain.house.social.deeds.get_mut(&sim) else {
                continue;
            };
            deeds.prune_calendar(today, window);
            deeds.items.is_empty() && deeds.dropped_day.is_none()
        };
        if empty {
            domain.house.social.deeds.remove(&sim);
        }
    }
}

/// A person leaves a moved-in household (ADR-0031 section 1). One who lives on keeps every
/// belief and every deed, unchanged. One who has already left the world does not get them
/// back: death archives before the house forgets.
pub(super) fn leave(domain: &mut DomainState, sim: SimId) {
    let beliefs = domain.house.social.beliefs.remove(&sim);
    let deeds = domain.house.social.deeds.remove(&sim);
    if !domain.sims.contains_key(&sim) {
        return;
    }
    if let Some(list) = beliefs.filter(|list| !list.is_empty()) {
        domain.house.social.beliefs.insert(sim, list);
    }
    if let Some(deeds) =
        deeds.filter(|deeds| !deeds.items.is_empty() || deeds.dropped_day.is_some())
    {
        domain.house.social.deeds.insert(sim, deeds);
    }
}

/// A person who leaves the world loses every belief and every deed (ADR-0031 section 1).
/// Nothing is written per item.
pub(super) fn left_world(domain: &mut DomainState, sim: SimId) {
    domain.house.social.beliefs.remove(&sim);
    domain.house.social.deeds.remove(&sim);
}

/// Whether the beliefs and deeds of a loaded state are ones the core can reach (ADR-0031
/// section 7). Only with social data. The owner of a list is a living person of at least
/// `min_age` at the open second (a child under 6 holds neither; the ADR names beliefs, and
/// deeds use the same age gate because nothing writes them earlier). A belief list is not
/// empty, sorted by key without a repeat, at most `limits.beliefs` long. Certainty is at most
/// 1000. `widerlegt` is only 0 or 1000. `secret` is only a `VaterVon` whose holder is the
/// child's mother. A deed list is not empty, at most `limits.deeds` long, in creation order
/// (the days do not go backwards). Each day is at least 0 and not before `dropped_day`.
/// While the owner lives in a moved-in household, no day is after that household's house
/// day (ADR-0031 section 7, addendum 2026-11-11). An older day is allowed and inert. A list
/// frozen outside a house is not judged against a house day: there is none until the next
/// move-in.
pub(crate) fn consistent(loaded: &LoadedState, data: &Data) -> bool {
    let beliefs = &loaded.house.social.beliefs;
    let deeds = &loaded.house.social.deeds;
    if beliefs.is_empty() && deeds.is_empty() {
        return true;
    }
    let Some(social) = data.social() else {
        return false;
    };
    let Some(now) = loaded.closed_through.as_secs().checked_add(1) else {
        return false;
    };
    let known = |sim: SimId| loaded.sims.contains_key(&sim) || loaded.archive.contains_key(&sim);
    let of_age = |owner: SimId| {
        let Some(person) = loaded.sims.get(&owner) else {
            return false;
        };
        let Some(age) = years_between(person.born.as_secs(), now) else {
            return false;
        };
        age >= u32::from(social.limits.min_age)
    };
    let kinds = social.beliefs.deeds.len();
    let belief_cap = usize::from(social.limits.beliefs);
    let deed_cap = usize::from(social.limits.deeds);
    let beliefs_ok = beliefs.iter().all(|(&owner, list)| {
        let fits = |belief: &Belief| {
            let secret_ok = !belief.secret
                || matches!(
                    belief.statement,
                    Statement::VaterVon { child, .. } if mother_of(loaded, child) == Some(owner)
                );
            let certainty_ok = belief.certainty <= 1000
                && (belief.basis != Basis::Widerlegt
                    || belief.certainty == 0
                    || belief.certainty == 1000);
            let statement_ok = match belief.statement {
                Statement::VaterVon { child, man } => known(child) && known(man),
                Statement::Tat {
                    doer, kind, other, ..
                } => known(doer) && known(other) && usize::from(kind) < kinds,
            };
            let heard_from = belief.source.is_none_or(&known) && belief.origin.is_none_or(&known);
            secret_ok && certainty_ok && statement_ok && heard_from && belief.since.as_secs() <= now
        };
        of_age(owner)
            && !list.is_empty()
            && list.len() <= belief_cap
            && list.windows(2).all(|pair| pair[0].key() < pair[1].key())
            && list.iter().all(fits)
    });
    let deeds_ok = deeds.iter().all(|(&owner, record)| {
        let kept = |day: i64| record.dropped_day.is_none_or(|dropped| day >= dropped);
        let not_after_today = |day: i64| match loaded_house_day(loaded, owner) {
            None => true,
            Some(today) => day <= today,
        };
        of_age(owner)
            && !(record.items.is_empty() && record.dropped_day.is_none())
            && record.items.len() <= deed_cap
            && record
                .items
                .windows(2)
                .all(|pair| pair[0].day <= pair[1].day)
            && record.items.iter().all(|deed| {
                usize::from(deed.kind) < kinds
                    && deed.other != owner
                    && known(deed.other)
                    && deed.day >= 0
                    && kept(deed.day)
                    && not_after_today(deed.day)
            })
    });
    beliefs_ok && deeds_ok
}

/// The house day a loaded deed list is judged against, when its owner lives in a moved-in
/// household. `None` while that list is frozen outside a house.
///
/// A `MoveIn` acts on the open second, so `since` can be one second after the clock. The
/// window is that stay's house day. A save still closed on the previous second keeps the
/// old day: the open second would already be the next morning.
fn loaded_house_day(loaded: &LoadedState, owner: SimId) -> Option<i64> {
    let person = loaded.sims.get(&owner)?;
    let since = loaded.house.households.get(&person.household)?.since;
    Some(house_day(loaded.closed_through.max(since), since))
}

/// The mother of `child`, living or archived.
fn mother_of(loaded: &LoadedState, child: SimId) -> Option<SimId> {
    let parents = loaded
        .sims
        .get(&child)
        .and_then(|person| person.parents)
        .or_else(|| loaded.archive.get(&child).and_then(|person| person.parents))?;
    Some(parents.mother)
}

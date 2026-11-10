//! Beliefs and own deeds in the house state (ADR-0031 sections 1 to 3 and 7). A person of
//! `min_age` or older in a moved-in household gains them. Moving out freezes both lists.
//! Death and emigration drop them. Certainty does not fade, so a freeze does not rewrite
//! `since`. The pure rules are `social::belief`.

use alloc::vec;
use alloc::vec::Vec;

use super::{DomainState, whole_years};
use crate::data::Data;
use crate::ids::{EventId, SimId};
use crate::log::{Cause, Causes, LogPayload, Subject};
use crate::social::belief::{
    Belief, BeliefInsertion, Statement, StatementKey, Taken, find, hear, insert, see,
};
use crate::social::data::SocialData;
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

/// Appends an own deed of today, after dropping days outside the calendar window. The cap
/// may still close a day inside that window (`dropped_day`). An empty list is not stored.
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

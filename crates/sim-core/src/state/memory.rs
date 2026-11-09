//! The memory of a person in a moved-in household (ADR-0030 section 1): what is written down
//! (the birth of an own child, the death of a relative from φ = 1/8, the start and the end of the
//! own partnership), the one open `Forget` per person at the earliest second a strength is 0,
//! what a person keeps on moving out and what it loses by leaving the world. The lists are in
//! `house::state::HouseState::social`; the strengths and the order of falling are
//! `social::memory`. Nothing remembers without social data in the world's data.

use alloc::vec;
use alloc::vec::Vec;

use super::{DomainState, Slot, market, whole_years};
use crate::canonical::LoadedState;
use crate::data::Data;
use crate::ids::{EventId, SimId};
use crate::log::{Cause, Causes, ForgetReason, LogPayload, Subject};
use crate::social::data::{
    CHILD_BORN, PARTNERSHIP_BEGAN, PARTNERSHIP_ENDED, RELATIVE_DIED, SocialData,
};
use crate::social::memory::{Insertion, Memory, insert, keep_on_move_out, next_forget};
use crate::time::SimTime;
use crate::world::{EventPayload, SimCtx};

/// The others of a memory in the order the list keeps them: ascending, a single one first.
pub(crate) fn others(a: Option<SimId>, b: Option<SimId>) -> [Option<SimId>; 2] {
    match (a, b) {
        (Some(a), Some(b)) => [Some(a.min(b)), Some(a.max(b))],
        (Some(only), None) | (None, Some(only)) => [Some(only), None],
        (None, None) => [None, None],
    }
}

/// Whether `sim` lives in a moved-in household and is old enough to remember (ADR-0030 section
/// 3, "Wer"). Judged where the person lives now: a handler's moves settle after it.
fn remembers(domain: &DomainState, social: &SocialData, sim: SimId, now: SimTime) -> bool {
    let Some(person) = domain.sims.get(&sim) else {
        return false;
    };
    domain.house.households.contains_key(&person.household)
        && whole_years(person.born, now) >= u32::from(social.limits.min_age)
}

/// `owner` remembers a memory of `kind` with the others `with`, happening now, if it lives in a
/// moved-in household and is old enough (ADR-0030 section 1, "Neu"): the kind gives the strength,
/// the fade and the valence; a full list lets the weakest of its entries and the new one fall,
/// and if that is the new one nothing is written. `core:remembered` names `cause`, an entry of
/// this handler; a displaced memory follows as `core:forgot` caused by it. Then the person's
/// `Forget` is planned anew.
pub(super) fn remember(
    domain: &mut DomainState,
    data: &Data,
    owner: SimId,
    (kind, with): (u16, [Option<SimId>; 2]),
    cause: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let Some(social) = data.social() else {
        return;
    };
    let now = ctx.now();
    if !remembers(domain, social, owner, now) {
        return;
    }
    let Some(rule) = social.kind(kind) else {
        panic!("invariant memory.kind");
    };
    let new = Memory {
        kind,
        with,
        at: now,
        valence: rule.valence,
        strength: rule.strength,
        since: now,
        fade: rule.fade,
    };
    let cap = usize::from(social.limits.memories);
    let list = domain.house.social.memories.entry(owner).or_default();
    let outcome = insert(list, new, now, cap);
    if outcome == Insertion::Refused {
        return;
    }
    let subjects = vec![Subject::Sim(owner)];
    let wrote = LogPayload::Remembered {
        kind,
        with,
        at: now,
        strength: new.strength,
    };
    let mut last = ctx.emit(wrote, subjects.clone(), Causes::new(Cause::Event(cause)));
    if let Insertion::Displaced(old) = outcome {
        let lost = LogPayload::Forgot {
            kind: old.kind,
            with: old.with,
            at: old.at,
            reason: ForgetReason::Displaced,
        };
        last = ctx.emit(lost, subjects, Causes::new(Cause::Event(last)));
    }
    plan_forget(domain, owner, last, ctx);
}

/// The person's one `Forget` anew after a change of its list (ADR-0030 section 1): the open one
/// is cancelled, and one is planned at the earliest second a strength is 0, by the entry `by`;
/// none for a list without a fading memory or without a list. A zero of this very second that
/// has not run yet is planned in this second, after the handler.
fn plan_forget(domain: &mut DomainState, sim: SimId, by: EventId, ctx: &mut SimCtx<'_, '_>) {
    if let Some(handle) = domain.pending.remove(&(sim, Slot::Forget)) {
        ctx.cancel(handle);
    }
    let due = (domain.house.social.memories.get(&sim)).and_then(|list| next_forget(list));
    if let Some(at) = due {
        let handle = ctx.schedule(at, EventPayload::Forget { sim }, by);
        domain.pending.insert((sim, Slot::Forget), handle);
    }
}

/// Removes every memory of `sim` whose strength is 0 now, each with its `core:forgot` (reason
/// `faded`) caused by `cause`; returns the last entry. An emptied list is gone.
fn fade(
    domain: &mut DomainState,
    sim: SimId,
    cause: &Causes,
    ctx: &mut SimCtx<'_, '_>,
) -> Option<EventId> {
    let now = ctx.now();
    let Some(list) = domain.house.social.memories.get_mut(&sim) else {
        panic!("invariant memory.orphan_event");
    };
    let (gone, kept): (Vec<Memory>, Vec<Memory>) =
        list.iter().partition(|memory| memory.strength_at(now) == 0);
    *list = kept;
    if list.is_empty() {
        domain.house.social.memories.remove(&sim);
    }
    let mut last = None;
    for memory in gone {
        let lost = LogPayload::Forgot {
            kind: memory.kind,
            with: memory.with,
            at: memory.at,
            reason: ForgetReason::Faded,
        };
        last = Some(ctx.emit(lost, vec![Subject::Sim(sim)], cause.clone()));
    }
    last
}

/// The earliest zero of a person's memories is now: every memory at 0 goes, then the next
/// `Forget` is planned (ADR-0030 section 1).
pub(crate) fn on_forget(domain: &mut DomainState, sim: SimId, ctx: &mut SimCtx<'_, '_>) {
    // The event that runs right now is no longer pending.
    domain.pending.remove(&(sim, Slot::Forget));
    let cause = Causes::new(ctx.trigger());
    let by = fade(domain, sim, &cause, ctx).or_else(|| ctx.planned_by());
    let Some(by) = by else {
        panic!("invariant memory.forget_planned");
    };
    plan_forget(domain, sim, by, ctx);
}

/// A person leaves a moved-in household now (ADR-0030 section 1, "Auszug und Tod"): one who lives
/// on keeps its `keep_on_move_out` strongest memories, frozen at their strength now (they do not
/// fade while it is outside; `since` is this second until it moves in again), and no event (its `Forget` is cancelled with the other events of the house); one
/// who died or left the town loses all of them. Nothing is written per memory: the entry of the
/// move or the death is the cause.
pub(super) fn leave(domain: &mut DomainState, data: &Data, sim: SimId, now: SimTime) {
    let Some(list) = domain.house.social.memories.remove(&sim) else {
        return;
    };
    if !domain.sims.contains_key(&sim) {
        return;
    }
    let keep = data
        .social()
        .map_or(0, |social| usize::from(social.limits.keep_on_move_out));
    let kept = keep_on_move_out(&list, keep, now);
    if !kept.is_empty() {
        domain.house.social.memories.insert(sim, kept);
    }
}

/// A person who leaves the world loses every memory, wherever it lived (ADR-0030 section 1).
/// Nothing is written per memory. The open `Forget` is cancelled once: a list frozen on moving
/// out has none left, and one still in a house loses the one it had.
pub(super) fn left_world(domain: &mut DomainState, sim: SimId, ctx: &mut SimCtx<'_, '_>) {
    domain.house.social.memories.remove(&sim);
    if let Some(handle) = domain.pending.remove(&(sim, Slot::Forget)) {
        ctx.cancel(handle);
    }
}

/// A person with memories comes into a moved-in household, caused by `by` (its `core:moved_in`):
/// what it kept on moving out did not fade while it was outside, so each strength stands as it
/// was frozen and `since` is now; then its `Forget` is planned (ADR-0030 section 1, "Auszug und
/// Tod"). A frozen strength of 0 is forgotten in this second.
pub(super) fn resume(domain: &mut DomainState, sim: SimId, by: EventId, ctx: &mut SimCtx<'_, '_>) {
    let now = ctx.now();
    let Some(list) = domain.house.social.memories.get_mut(&sim) else {
        return;
    };
    for memory in list.iter_mut() {
        memory.since = now;
    }
    plan_forget(domain, sim, by, ctx);
}

/// The mother and the father of the child `child`, born now with the entry `born`, remember its
/// birth, each with the other parent and the child as the others (ADR-0030 section 1); the
/// father only if he lives in a moved-in household.
pub(super) fn child_born(
    domain: &mut DomainState,
    data: &Data,
    (mother, father): (SimId, Option<SimId>),
    child: SimId,
    born: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let kind = |other: Option<SimId>| (CHILD_BORN, others(other, Some(child)));
    remember(domain, data, mother, kind(father), born, ctx);
    if let Some(father) = father {
        remember(domain, data, father, kind(Some(mother)), born, ctx);
    }
}

/// The woman and the man of a partnership that began or ended now remember it, each with the
/// other as the one involved: the woman first (ADR-0030 section 1), `began` for the start, else
/// the end; `by` is the entry of the partnership or of its end.
pub(super) fn partnership(
    domain: &mut DomainState,
    data: &Data,
    (woman, man): (SimId, SimId),
    began: bool,
    by: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let kind = if began {
        PARTNERSHIP_BEGAN
    } else {
        PARTNERSHIP_ENDED
    };
    remember(
        domain,
        data,
        woman,
        (kind, others(Some(man), None)),
        by,
        ctx,
    );
    remember(
        domain,
        data,
        man,
        (kind, others(Some(woman), None)),
        by,
        ctx,
    );
}

/// The partner of `dead`, who lives on, remembers the end of the partnership by this death
/// (`by` its `core:death`): ADR-0030 section 1, "Ende der eigenen Partnerschaft".
pub(super) fn partner_lost(
    domain: &mut DomainState,
    data: &Data,
    (dead, partner): (SimId, SimId),
    by: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let kind = (PARTNERSHIP_ENDED, others(Some(dead), None));
    remember(domain, data, partner, kind, by, ctx);
}

/// Everyone in a moved-in household who is old enough and a relative of `dead`, by ascending
/// SimId, remembers its death (`by` its `core:death`; ADR-0030 section 1): a relative is a person
/// with φ of at least `relative_phi_eighths` eighths to it. The dead have left the town; the
/// others are judged where they live now.
pub(super) fn relative_died(
    domain: &mut DomainState,
    data: &Data,
    dead: SimId,
    by: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let Some(social) = data.social() else {
        return;
    };
    let now = ctx.now();
    let mut candidates: Vec<SimId> = (domain.house.households.keys())
        .filter_map(|household| domain.members.get(household))
        .flatten()
        .copied()
        .filter(|&sim| remembers(domain, social, sim, now))
        .collect();
    candidates.sort_unstable();
    for sim in candidates {
        if market::kin_from_eighths(
            domain,
            data,
            dead,
            sim,
            u32::from(social.memory.relative_phi_eighths),
        ) {
            let kind = (RELATIVE_DIED, others(Some(dead), None));
            remember(domain, data, sim, kind, by, ctx);
        }
    }
}

/// The second the open `Forget` of `sim` must stand at in a loaded state: the earliest zero of the
/// list of a person who lives in a moved-in household; `None` for anyone else (a frozen list has
/// no event) and for a list that does not fade within time (ADR-0030 section 5).
pub(crate) fn forget_due(loaded: &LoadedState, sim: SimId) -> Option<SimTime> {
    let person = loaded.sims.get(&sim)?;
    loaded
        .house
        .households
        .contains_key(&person.household)
        .then_some(())?;
    loaded
        .house
        .social
        .memories
        .get(&sim)
        .and_then(|list| next_forget(list))
}

/// Whether the memories of a loaded state are ones the core can reach (ADR-0030 section 5):
/// only with social data; each list belongs to a living person and is not empty, sorted by key
/// without a repeated one, at most `memories` long for a person in a moved-in household and
/// `keep_on_move_out` for one who is not; each memory of a known kind with that kind's fade and
/// valence, a strength up to 1000, the others ascending, known and not the owner, written down
/// by now; in a live list no memory is at 0 already (its `Forget` is at the earliest zero,
/// `forget_due`, checked with the queue).
pub(crate) fn consistent(loaded: &LoadedState, data: &Data) -> bool {
    let memories = &loaded.house.social.memories;
    if memories.is_empty() {
        return true;
    }
    let Some(social) = data.social() else {
        return false;
    };
    let clock = loaded.closed_through;
    let known = |sim: SimId| loaded.sims.contains_key(&sim) || loaded.archive.contains_key(&sim);
    memories.iter().all(|(&owner, list)| {
        let Some(person) = loaded.sims.get(&owner) else {
            return false;
        };
        let live = loaded.house.households.contains_key(&person.household);
        let cap = match live {
            true => social.limits.memories,
            false => social.limits.keep_on_move_out,
        };
        let fits = |memory: &Memory| {
            let kind = social.kind(memory.kind);
            let others = match memory.with {
                [Some(first), Some(second)] => first < second && known(first) && known(second),
                [Some(only), None] => known(only),
                [None, _] => false,
            };
            let written =
                memory.at <= memory.since && memory.since.as_secs() <= clock.as_secs() + 1;
            let faded = live && memory.zero_second().is_some_and(|zero| zero <= clock);
            kind.is_some_and(|kind| kind.fade == memory.fade && kind.valence == memory.valence)
                && memory.strength <= crate::social::memory::MOST
                && others
                && !memory.with.contains(&Some(owner))
                && written
                && !faded
        };
        !list.is_empty()
            && list.len() <= usize::from(cap)
            && list.windows(2).all(|pair| pair[0].key() < pair[1].key())
            && list.iter().all(fits)
    })
}

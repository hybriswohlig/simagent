//! A talk of two people at one sociable furniture (ADR-0030 section 3). Planning follows an
//! arrival; the run is the event, or the same-second end that must happen first. Klatsch is
//! never allowed (`social::talk::gossip_allowed`). The seen deed of Necken, gossip, refutation
//! and the memory "Widerlegung" wait for beliefs (ADR-0031).

use alloc::vec;
use alloc::vec::Vec;

use super::action::person_mut;
use super::bond::{Formed, form_one};
use super::memory::{WroteMemory, others, plan_forget, remember};
use super::{DomainState, Slot, whole_years};
use crate::canonical::LoadedState;
use crate::data::Data;
use crate::data_id::DataId;
use crate::house::data::Need;
use crate::house::state::Place;
use crate::ids::{EventId, HouseholdId, MAX_JS_SAFE, SimId};
use crate::log::{Cause, Causes, LogPayload, Subject};
use crate::rng::WorldSeed;
use crate::social::data::TALK;
use crate::social::memory::Memory;
use crate::social::talk::{
    TalkFacts, TalkPair, choose_topic, house_day, interests, recall_strength, talk_conditions,
    topic_actor, topic_outcome,
};
use crate::time::{SimDuration, SimTime};
use crate::world::{EventPayload, SimCtx};

/// The furniture the two share, and the household it stands in.
struct Seat {
    household: HouseholdId,
    object: u16,
    kind: DataId,
}

/// Where `sim` sits, if it is at an object in a moved-in household.
fn seat_of(domain: &DomainState, data: &Data, sim: SimId) -> Option<Seat> {
    let person = domain.house.sims.get(&sim)?;
    if !matches!(person.place, Place::Room(_)) {
        return None;
    }
    let action = person.action?;
    let household = domain.sims.get(&sim)?.household;
    let moved = domain.house.households.get(&household)?;
    let houses = data.houses()?;
    let house = houses.house(&moved.house)?;
    let kind = house
        .def()
        .objects
        .get(usize::from(action.object))?
        .kind
        .clone();
    Some(Seat {
        household,
        object: action.object,
        kind,
    })
}

/// Both at the same sociable object of one household: the object and its kind.
fn together(domain: &DomainState, data: &Data, a: SimId, b: SimId) -> Option<(u16, DataId)> {
    let left = seat_of(domain, data, a)?;
    let right = seat_of(domain, data, b)?;
    if left.household != right.household || left.object != right.object {
        return None;
    }
    let social = data.social()?;
    social
        .sociable(&left.kind)
        .then_some((left.object, left.kind))
}

fn ordered(a: SimId, b: SimId) -> (SimId, SimId) {
    if a < b { (a, b) } else { (b, a) }
}

fn same_home(domain: &DomainState, a: SimId, b: SimId) -> Option<HouseholdId> {
    let left = domain.sims.get(&a)?.household;
    let right = domain.sims.get(&b)?.household;
    (left == right && domain.house.households.contains_key(&left)).then_some(left)
}

fn years(domain: &DomainState, sim: SimId, at: SimTime) -> u32 {
    let born = (domain.sims.get(&sim))
        .map(|person| person.born)
        .unwrap_or_else(|| panic!("invariant unknown_entity_id: sim {}", sim.get()));
    whole_years(born, at)
}

/// Talks and social entries of `sim` on house day `day`. A stamp from another day reads as
/// zero: that is the reset at 06:00.
fn counts(domain: &DomainState, sim: SimId, day: i64) -> (u16, u16) {
    match domain.house.sims.get(&sim) {
        Some(person) if person.talk_day == day => (person.talks, person.entries),
        _ => (0, 0),
    }
}

/// Whether the pair may talk at `planned`. Missing household, or two households, is no.
fn may(
    domain: &DomainState,
    data: &Data,
    a: SimId,
    b: SimId,
    planned: SimTime,
    check_open: bool,
) -> bool {
    if a >= b {
        return false;
    }
    let Some(social) = data.social() else {
        return false;
    };
    let Some(home) = same_home(domain, a, b) else {
        return false;
    };
    let moved = &domain.house.households[&home];
    let day = house_day(planned, moved.since);
    let (talks_a, entries_a) = counts(domain, a, day);
    let (talks_b, entries_b) = counts(domain, b, day);
    let facts = TalkFacts {
        open_talk: domain.pending.contains_key(&(a, Slot::Talk(b))),
        last_talk: moved.last_talk.get(&(a, b)).copied(),
        talks_a,
        talks_b,
        entries_a,
        entries_b,
        age_a: years(domain, a, planned),
        age_b: years(domain, b, planned),
    };
    talk_conditions(&social.limits, planned, &facts, check_open)
}

fn clear(domain: &mut DomainState, a: SimId, b: SimId) {
    domain.pending.remove(&(a, Slot::Talk(b)));
    domain.pending.remove(&(b, Slot::Talk(a)));
}

fn talk_handles(domain: &DomainState, sim: SimId) -> Vec<(SimId, crate::time::EventHandle)> {
    let first = SimId::try_from(1).unwrap_or_else(|_| panic!("invariant talk.pair"));
    let last = SimId::try_from(MAX_JS_SAFE).unwrap_or_else(|_| panic!("invariant talk.pair"));
    (domain
        .pending
        .range((sim, Slot::Talk(first))..=(sim, Slot::Talk(last))))
    .filter_map(|((_, slot), handle)| match slot {
        Slot::Talk(other) => Some((*other, *handle)),
        _ => None,
    })
    .collect()
}

fn slot_due(
    domain: &DomainState,
    sim: SimId,
    slot: Slot,
    now: SimTime,
    ctx: &SimCtx<'_, '_>,
) -> bool {
    (domain.pending.get(&(sim, slot))).is_some_and(|handle| ctx.pending_time(*handle) == Some(now))
}

fn range_due(
    domain: &DomainState,
    sim: SimId,
    first: Slot,
    last: Slot,
    now: SimTime,
    ctx: &SimCtx<'_, '_>,
) -> bool {
    (domain.pending.range((sim, first)..=(sim, last)))
        .any(|(_, handle)| ctx.pending_time(*handle) == Some(now))
}

/// An end or an abort of `sim` is still pending in this second (ADR-0030 section 3, "Gleiche
/// Sekunde"). A crossing or a reserve that does not stand the person up is included: deferring
/// it one micro changes nothing, and the end that does stand them up then runs the talk.
fn abort_due(domain: &DomainState, sim: SimId, now: SimTime, ctx: &SimCtx<'_, '_>) -> bool {
    let one = [Slot::End, Slot::Wake, Slot::Depart, Slot::StandUp]
        .into_iter()
        .any(|slot| slot_due(domain, sim, slot, now, ctx));
    let crossing = range_due(
        domain,
        sim,
        Slot::Crossing(Need::Hunger, 0),
        Slot::Crossing(Need::Fun, u16::MAX),
        now,
        ctx,
    );
    let reserve = range_due(
        domain,
        sim,
        Slot::Reserve(0),
        Slot::Reserve(u16::MAX),
        now,
        ctx,
    );
    one || crossing || reserve
}

/// A death of `sim` is pending in this second. The slot is the signal: a death that was put
/// straight on the queue is not seen here, and a deferral must not pass it (class orders a
/// later micro after every death of the second).
fn death_due(domain: &DomainState, sim: SimId, now: SimTime, ctx: &SimCtx<'_, '_>) -> bool {
    range_due(domain, sim, Slot::Death(0), Slot::Death(u64::MAX), now, ctx)
}

fn defer(domain: &mut DomainState, a: SimId, b: SimId, ctx: &mut SimCtx<'_, '_>) {
    let by = ctx
        .planned_by()
        .unwrap_or_else(|| panic!("invariant talk.by"));
    let handle = ctx.schedule(ctx.now(), EventPayload::Talk { a, b }, by);
    domain.pending.insert((a, Slot::Talk(b)), handle);
    domain.pending.insert((b, Slot::Talk(a)), handle);
}

/// `sim` reached a sociable seat. For each household member already sitting there, in
/// ascending id, plan one Talk at `now` plus the delay when section 3 "Beginn" allows it.
pub(super) fn on_seated(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    by: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let Some(social) = data.social() else {
        return;
    };
    let Some(seat) = seat_of(domain, data, sim) else {
        return;
    };
    if !social.sociable(&seat.kind) {
        return;
    }
    let delay = i64::from(social.limits.talk_delay_seconds);
    let Some(at) = ctx.now().checked_add(SimDuration::from_secs(delay)) else {
        return;
    };
    let Some(members) = domain.members.get(&seat.household) else {
        return;
    };
    let others: Vec<SimId> = members
        .iter()
        .copied()
        .filter(|&other| other != sim)
        .filter(|&other| {
            seat_of(domain, data, other).is_some_and(|theirs| {
                theirs.household == seat.household && theirs.object == seat.object
            })
        })
        .collect();
    for other in others {
        let (a, b) = ordered(sim, other);
        if !may(domain, data, a, b, at, true) {
            continue;
        }
        let handle = ctx.schedule(at, EventPayload::Talk { a, b }, by);
        domain.pending.insert((a, Slot::Talk(b)), handle);
        domain.pending.insert((b, Slot::Talk(a)), handle);
    }
}

/// `sim` stops sitting now (ADR-0030 section 3, "Gleiche Sekunde" and "Abbruch"). A Talk due
/// this second whose partner is still at the same seat runs first, and plans no successor.
/// Every other open Talk of `sim` is cancelled. A partner who is not there is not an orphan:
/// the event simply does not run.
pub(super) fn on_stand_up(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let now = ctx.now();
    let due: Vec<_> = talk_handles(domain, sim)
        .into_iter()
        .filter(|(_, handle)| ctx.pending_time(*handle) == Some(now))
        .collect();
    for (other, handle) in due {
        let (a, b) = ordered(sim, other);
        clear(domain, a, b);
        if let Some((object, kind)) = together(domain, data, a, b) {
            run(domain, data, (a, b), object, &kind, false, ctx);
        }
        ctx.cancel(handle);
    }
    for (other, handle) in talk_handles(domain, sim) {
        let (a, b) = ordered(sim, other);
        clear(domain, a, b);
        ctx.cancel(handle);
    }
}

/// The Talk event. An abort still pending in this second, and no death, defers it one micro
/// so the end runs it first. Otherwise the two must still sit together (`talk.orphan` if not).
/// Conditions that fail write nothing and plan nothing.
pub(crate) fn on_talk(
    domain: &mut DomainState,
    data: &Data,
    a: SimId,
    b: SimId,
    ctx: &mut SimCtx<'_, '_>,
) {
    if a >= b {
        panic!("invariant talk.pair");
    }
    let now = ctx.now();
    let abort = abort_due(domain, a, now, ctx) || abort_due(domain, b, now, ctx);
    let dying = death_due(domain, a, now, ctx) || death_due(domain, b, now, ctx);
    if abort && !dying {
        defer(domain, a, b, ctx);
        return;
    }
    clear(domain, a, b);
    let Some((object, kind)) = together(domain, data, a, b) else {
        panic!("invariant talk.orphan");
    };
    if !may(domain, data, a, b, now, false) {
        return;
    }
    run(domain, data, (a, b), object, &kind, true, ctx);
}

/// Drops every last-talk pair that names `sim`. The person may already be gone from the
/// sims (a death archives before the house forgets), so this does not look the household up.
pub(super) fn drop_last_talk(domain: &mut DomainState, sim: SimId) {
    for moved in domain.house.households.values_mut() {
        moved.last_talk.retain(|&(a, b), _| a != sim && b != sim);
    }
}

fn charge(total: &mut u16, n: u16) {
    *total = total
        .checked_add(n)
        .unwrap_or_else(|| panic!("invariant talk.entries"));
}

fn add_entries(domain: &mut DomainState, sim: SimId, day: i64, n: u16) {
    if n == 0 {
        return;
    }
    let person = person_mut(domain, sim);
    if person.talk_day != day {
        person.talk_day = day;
        person.talks = 0;
        person.entries = 0;
    }
    person.entries =
        (person.entries.checked_add(n)).unwrap_or_else(|| panic!("invariant talk.entries"));
}

fn add_talk(domain: &mut DomainState, sim: SimId, day: i64) {
    let person = person_mut(domain, sim);
    if person.talk_day != day {
        person.talk_day = day;
        person.talks = 0;
        person.entries = 0;
    }
    person.talks = person
        .talks
        .checked_add(1)
        .unwrap_or_else(|| panic!("invariant talk.talks"));
}

fn edge_at(
    domain: &DomainState,
    owner: SimId,
    other: SimId,
    now: SimTime,
    fade: (u16, u16),
) -> [u16; 6] {
    let (familiarity, grudge) = fade;
    (domain.house.social.bonds.get(&owner))
        .and_then(|list| list.iter().find(|bond| bond.other == other))
        .map(|bond| bond.values_at(now, familiarity, grudge))
        .unwrap_or([0; 6])
}

/// The strongest common memory: same `at` and kind, each list naming the other. The largest
/// sum of strengths, then the younger `at`, then the smaller kind.
fn shared_memory(
    domain: &DomainState,
    a: SimId,
    b: SimId,
    now: SimTime,
) -> Option<(Memory, Memory)> {
    let list_a = domain.house.social.memories.get(&a)?;
    let list_b = domain.house.social.memories.get(&b)?;
    let mut best: Option<(i32, i64, u16, Memory, Memory)> = None;
    for left in list_a {
        if !left.with.contains(&Some(b)) {
            continue;
        }
        for right in list_b {
            if right.at != left.at || right.kind != left.kind || !right.with.contains(&Some(a)) {
                continue;
            }
            let sum = i32::from(left.strength_at(now)) + i32::from(right.strength_at(now));
            let rank = (-sum, -left.at.as_secs(), left.kind);
            if best.as_ref().is_none_or(|old| rank < (old.0, old.1, old.2)) {
                best = Some((rank.0, rank.1, rank.2, *left, *right));
            }
        }
    }
    best.map(|(_, _, _, left, right)| (left, right))
}

fn change_bond(
    domain: &mut DomainState,
    owner: SimId,
    other: SimId,
    steps: &[(usize, i16)],
    now: SimTime,
    fade: (u16, u16),
) -> ([u16; 6], [u16; 6]) {
    let (familiarity, grudge) = fade;
    let list = (domain.house.social.bonds.get_mut(&owner))
        .unwrap_or_else(|| panic!("invariant talk.bond"));
    let bond = (list.iter_mut().find(|bond| bond.other == other))
        .unwrap_or_else(|| panic!("invariant talk.bond"));
    let before = bond.apply(now, familiarity, grudge, steps);
    (before, bond.values)
}

fn formed_edge(
    domain: &mut DomainState,
    data: &Data,
    holder: SimId,
    other: SimId,
    cause: &mut Cause,
    ctx: &mut SimCtx<'_, '_>,
) -> u16 {
    match form_one(domain, data, holder, other, cause.clone(), ctx) {
        Some(Formed::Wrote { last, entries }) => {
            *cause = Cause::Event(last);
            entries
        }
        Some(Formed::Refused) => panic!("invariant talk.bond"),
        None => 0,
    }
}

fn earlier(cause: &Cause, ctx: &SimCtx<'_, '_>) -> EventId {
    match cause {
        Cause::Event(id) => *id,
        _ => ctx
            .planned_by()
            .unwrap_or_else(|| panic!("invariant talk.by")),
    }
}

fn recall(
    domain: &mut DomainState,
    owner: SimId,
    other: SimId,
    memory: Memory,
    step: u16,
    now: SimTime,
) {
    let strength = recall_strength(memory.strength_at(now), step);
    let Some(list) = domain.house.social.memories.get_mut(&owner) else {
        return;
    };
    let Some(found) = list.iter_mut().find(|item| {
        item.at == memory.at && item.kind == memory.kind && item.with.contains(&Some(other))
    }) else {
        return;
    };
    found.strength = strength;
    found.since = now;
}

/// Steps 0, 1, 2, 6, 7 and 8 of section 3 "Ablauf". `plan_next` is false when an end of this
/// second runs the talk: the two do not both stay, so no successor is planned.
fn run(
    domain: &mut DomainState,
    data: &Data,
    pair: (SimId, SimId),
    object: u16,
    kind: &DataId,
    plan_next: bool,
    ctx: &mut SimCtx<'_, '_>,
) {
    let (a, b) = pair;
    let social = data
        .social()
        .unwrap_or_else(|| panic!("invariant talk.social"));
    let home = same_home(domain, a, b).unwrap_or_else(|| panic!("invariant talk.house"));
    let now = ctx.now();
    let since = domain.house.households[&home].since;
    let day = house_day(now, since);
    let mut cause = ctx.trigger();
    let mut entries_a = formed_edge(domain, data, a, b, &mut cause, ctx);
    let mut entries_b = formed_edge(domain, data, b, a, &mut cause, ctx);
    let fade = (social.bonds.familiarity_fade, social.bonds.grudge_fade);
    let seed = (domain.world_seed).unwrap_or_else(|| panic!("invariant seeds.before_start"));
    let interest_a = interests(WorldSeed::new(seed), domain.key_of(a));
    let interest_b = interests(WorldSeed::new(seed), domain.key_of(b));
    let shared = shared_memory(domain, a, b, now);
    let view = TalkPair {
        a,
        b,
        age_a: years(domain, a, now),
        age_b: years(domain, b, now),
        edge_ab: edge_at(domain, a, b, now, fade),
        edge_ba: edge_at(domain, b, a, now, fade),
        interest_a: &interest_a,
        interest_b: &interest_b,
        common_memory: shared.is_some(),
    };
    let topic = choose_topic(social, &view, kind);
    let actor = topic_actor(social, topic, &view);
    let outcomes = topic_outcome(social, topic, &view);
    for outcome in outcomes {
        let (before, after) = change_bond(
            domain,
            outcome.owner,
            outcome.other,
            &outcome.steps,
            now,
            fade,
        );
        if before == after {
            continue;
        }
        let id = ctx.emit(
            LogPayload::BondChanged {
                other: outcome.other,
                before,
                after,
            },
            vec![Subject::Sim(outcome.owner)],
            Causes::new(cause.clone()),
        );
        cause = Cause::Event(id);
        if outcome.owner == a {
            charge(&mut entries_a, 1);
        } else {
            charge(&mut entries_b, 1);
        }
    }
    // Step 3, the seen deed `neckt` (`core:believed`), waits for beliefs (ADR-0031).
    // Steps 4 and 5, gossip and refutation, and the memory "Widerlegung", wait for them too.
    let recall_topic =
        (social.topics.get(topic)).is_some_and(|row| row.id.as_str() == "core:recall");
    if recall_topic && let Some((left, right)) = shared {
        let step = social.memory.recall_step;
        recall(domain, a, b, left, step, now);
        recall(domain, b, a, right, step, now);
        let by = earlier(&cause, ctx);
        plan_forget(domain, a, by, ctx);
        plan_forget(domain, b, by, ctx);
    }
    let (cause, kept_a) = wrote_memory(domain, data, a, b, cause, ctx);
    charge(&mut entries_a, kept_a);
    let (cause, kept_b) = wrote_memory(domain, data, b, a, cause, ctx);
    charge(&mut entries_b, kept_b);
    let topic_u8 = u8::try_from(topic).unwrap_or_else(|_| panic!("invariant talk.topic"));
    let talked = ctx.emit(
        LogPayload::Talked {
            other: b,
            object,
            topic: topic_u8,
            actor,
        },
        vec![Subject::Sim(a), Subject::Sim(b)],
        Causes::new(cause),
    );
    charge(&mut entries_a, 1);
    charge(&mut entries_b, 1);
    add_entries(domain, a, day, entries_a);
    add_entries(domain, b, day, entries_b);
    add_talk(domain, a, day);
    add_talk(domain, b, day);
    domain
        .house
        .households
        .get_mut(&home)
        .unwrap_or_else(|| panic!("invariant talk.house"))
        .last_talk
        .insert((a, b), now);
    if !plan_next {
        return;
    }
    let gap = i64::from(social.limits.pair_gap_seconds);
    let Some(at) = now.checked_add(SimDuration::from_secs(gap)) else {
        return;
    };
    if together(domain, data, a, b).is_none() || !may(domain, data, a, b, at, false) {
        return;
    }
    let handle = ctx.schedule(at, EventPayload::Talk { a, b }, talked);
    domain.pending.insert((a, Slot::Talk(b)), handle);
    domain.pending.insert((b, Slot::Talk(a)), handle);
}

/// The memory "Gespräch" for `owner`, about `other`. Returns the cause of the next entry and
/// how many entries named `owner`.
fn wrote_memory(
    domain: &mut DomainState,
    data: &Data,
    owner: SimId,
    other: SimId,
    cause: Cause,
    ctx: &mut SimCtx<'_, '_>,
) -> (Cause, u16) {
    let with = others(Some(other), None);
    match remember(domain, data, owner, (TALK, with), cause.clone(), ctx) {
        Some(WroteMemory::Kept(id)) => (Cause::Event(id), 1),
        Some(WroteMemory::Displaced(id)) => (Cause::Event(id), 2),
        None => (cause, 0),
    }
}

/// An open Talk of a loaded queue: `a` smaller, both seated at one sociable object of their
/// household. One event per pair is the caller's check.
pub(crate) fn open_talk_ok(loaded: &LoadedState, data: &Data, a: SimId, b: SimId) -> bool {
    if a >= b {
        return false;
    }
    let Some(social) = data.social() else {
        return false;
    };
    let (Some(left), Some(right)) = (loaded.house.sims.get(&a), loaded.house.sims.get(&b)) else {
        return false;
    };
    let Some(home) = loaded.sims.get(&a).map(|person| person.household) else {
        return false;
    };
    let Some(other) = loaded.sims.get(&b).map(|person| person.household) else {
        return false;
    };
    if home != other || !loaded.house.households.contains_key(&home) {
        return false;
    }
    let (Place::Room(_), Some(seat_a)) = (left.place, left.action) else {
        return false;
    };
    let (Place::Room(_), Some(seat_b)) = (right.place, right.action) else {
        return false;
    };
    if seat_a.object != seat_b.object {
        return false;
    }
    let Some(houses) = data.houses() else {
        return false;
    };
    let Some(moved) = loaded.house.households.get(&home) else {
        return false;
    };
    let Some(house) = houses.house(&moved.house) else {
        return false;
    };
    let Some(object) = house.def().objects.get(usize::from(seat_a.object)) else {
        return false;
    };
    social.sociable(&object.kind)
}

/// Day counters at or under their caps, and last-talk pairs of the household at or before the
/// clock. A stamp from another day is kept: it reads as zero. Without social data every
/// counter is zero and no pair is stored.
///
/// READING: "unter ihren Grenzen" on a reachable state is at most the cap. A talk that used
/// the last place sits at the cap until the next house day.
pub(crate) fn counters_ok(loaded: &LoadedState, data: &Data) -> bool {
    let Some(social) = data.social() else {
        let clear =
            (loaded.house.sims.values()).all(|person| person.talks == 0 && person.entries == 0);
        let pairs = (loaded.house.households.values()).all(|moved| moved.last_talk.is_empty());
        return clear && pairs;
    };
    let over = |talks: u16, entries: u16| {
        talks > social.limits.talks_per_day || entries > social.limits.entries_per_day
    };
    if (loaded.house.sims.values()).any(|person| over(person.talks, person.entries)) {
        return false;
    }
    for (home, moved) in &loaded.house.households {
        for (&(a, b), &when) in &moved.last_talk {
            if a >= b || when > loaded.closed_through {
                return false;
            }
            let member =
                |id: SimId| (loaded.sims.get(&id)).is_some_and(|person| person.household == *home);
            if !member(a) || !member(b) {
                return false;
            }
        }
    }
    true
}

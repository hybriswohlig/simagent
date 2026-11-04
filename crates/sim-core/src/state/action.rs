//! What a person in a house does (ADR-0026 sections 3 and 4): it chooses when it is idle in a
//! room, walks to the chosen object holding one of its places, arrives and takes the offer's
//! rates, and at the end takes the awake rates again and chooses anew, or leaves if its
//! departure waited for the end (ADR-0024 section 4). A need turning critical ends what does
//! not raise it.

use alloc::collections::BTreeSet;
use alloc::vec;
use alloc::vec::Vec;

use super::house::plan_crossings;
use super::{DomainState, Slot};
use crate::data::Data;
use crate::house::choice::{CRITICAL, Choice, choose, mood, score};
use crate::house::data::{House, HouseData, NEEDS, Need, OfferDef, age_band};
use crate::house::day::night;
use crate::house::need::FULL;
use crate::house::state::{Action, Decisions, InHouse, MAX_DECISIONS, Place, action_end};
use crate::ids::{EventId, HouseholdId, SimId};
use crate::log::{Cause, Causes, EndReason, LogPayload, Subject};
use crate::time::{SimDuration, SimTime, day_index};
use crate::world::{EventPayload, SimCtx};

/// A person's household, the house data and its house.
pub(super) fn home<'a>(
    domain: &DomainState,
    data: &'a Data,
    sim: SimId,
) -> (HouseholdId, &'a HouseData, &'a House) {
    let Some(person) = domain.sims.get(&sim) else {
        panic!("invariant unknown_entity_id: sim {}", sim.get());
    };
    let houses = data
        .houses()
        .unwrap_or_else(|| panic!("invariant house.unknown"));
    let house = (domain.house.households.get(&person.household))
        .and_then(|moved| houses.house(&moved.house))
        .unwrap_or_else(|| panic!("invariant house.unknown"));
    (person.household, houses, house)
}

/// A person's birth.
pub(super) fn born(domain: &DomainState, sim: SimId) -> SimTime {
    let Some(person) = domain.sims.get(&sim) else {
        panic!("invariant unknown_entity_id: sim {}", sim.get());
    };
    person.born
}

/// A person's age in whole years at `now`.
fn age(domain: &DomainState, sim: SimId, now: SimTime) -> u8 {
    crate::house::choice::age(born(domain, sim), now)
        .unwrap_or_else(|| panic!("invariant age.out_of_range"))
}

/// The age band of a person with needs at `now`.
pub(super) fn band(domain: &DomainState, sim: SimId, now: SimTime) -> usize {
    age_band(age(domain, sim, now)).unwrap_or_else(|| panic!("invariant house.age"))
}

pub(super) fn person_mut(domain: &mut DomainState, sim: SimId) -> &mut InHouse {
    let Some(person) = domain.house.sims.get_mut(&sim) else {
        panic!("invariant house.orphan_event");
    };
    person
}

fn offer_of<'a>(house: &House, houses: &'a HouseData, action: Action) -> &'a OfferDef {
    (house.def().objects.get(usize::from(action.object)))
        .and_then(|object| houses.kind(&object.kind))
        .and_then(|kind| kind.offers.get(usize::from(action.offer)))
        .unwrap_or_else(|| panic!("invariant house.unknown"))
}

fn need_index(need: Need) -> usize {
    (NEEDS.iter().position(|&each| each == need)).unwrap_or_else(|| panic!("invariant house.need"))
}

/// Whether a person's walk or action raises `need` (ADR-0026 section 4): at the object the
/// need rises, on the way the offer's rate for it is positive in the age band of the arrival.
fn raises(domain: &DomainState, data: &Data, sim: SimId, need: Need) -> bool {
    let Some(person) = domain.house.sims.get(&sim) else {
        panic!("invariant house.orphan_event");
    };
    match (person.place, person.action) {
        (Place::Walking { arrive, .. }, Some(action)) => {
            let (_, houses, house) = home(domain, data, sim);
            let band = band(domain, sim, arrive);
            offer_of(house, houses, action).rate(houses.rates(), need, band) > 0
        }
        (Place::Room(_), Some(_)) => person.needs[need_index(need)].rate() > 0,
        _ => false,
    }
}

/// Whether a person's walk or action raises a need that is critical now: then its departure
/// waits for the end (ADR-0024 section 4).
pub(super) fn for_critical(domain: &DomainState, data: &Data, sim: SimId, now: SimTime) -> bool {
    let Some(person) = domain.house.sims.get(&sim) else {
        panic!("invariant house.orphan_event");
    };
    (NEEDS.into_iter().zip(person.needs))
        .any(|(need, sample)| sample.value_at(now) <= CRITICAL && raises(domain, data, sim, need))
}

/// Whether a person sleeps: at the object of an offer of sleep (ADR-0024 section 4).
pub(super) fn asleep(domain: &DomainState, data: &Data, sim: SimId) -> bool {
    let Some(person) = domain.house.sims.get(&sim) else {
        panic!("invariant house.orphan_event");
    };
    let (Place::Room(_), Some(action)) = (person.place, person.action) else {
        return false;
    };
    let (_, houses, house) = home(domain, data, sim);
    offer_of(house, houses, action).sleep
}

/// The places of the household's objects taken by an action or held for another than `sim`,
/// as (object, place): in its choice a place held for `sim` counts as free (ADR-0026 section 5).
pub(super) fn taken(
    domain: &DomainState,
    household: HouseholdId,
    sim: SimId,
) -> BTreeSet<(u16, u8)> {
    let members = domain.members.get(&household).into_iter().flatten();
    let actions = members.filter_map(|sim| domain.house.sims.get(sim)?.action);
    let mut taken: BTreeSet<(u16, u8)> = actions.map(|a| (a.object, a.slot)).collect();
    let held = (domain.house.holds.range(household_places(household)))
        .filter(|&(_, &holder)| holder != sim)
        .map(|(&(_, object, slot), _)| (object, slot));
    taken.extend(held);
    taken
}

/// The keys of all places of a household's objects.
fn household_places(household: HouseholdId) -> core::ops::RangeInclusive<(HouseholdId, u16, u8)> {
    (household, 0, 0)..=(household, u16::MAX, u8::MAX)
}

/// The place held for `sim`, if any.
pub(crate) fn held_by(
    domain: &DomainState,
    household: HouseholdId,
    sim: SimId,
) -> Option<(u16, u8)> {
    (domain.house.holds.range(household_places(household)))
        .find(|&(_, &holder)| holder == sim)
        .map(|(&(_, object, slot), _)| (object, slot))
}

/// A place of `object` is free again (ADR-0026 section 5): the first who waits for the object
/// (earliest start, then smaller SimId) leaves the list and the place is held for it, with a
/// choice planned by `by` in this second; with nobody waiting the place is simply free.
pub(crate) fn release(
    domain: &mut DomainState,
    household: HouseholdId,
    (object, slot): (u16, u8),
    by: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let Some(waiters) = domain.house.waits.get_mut(&(household, object)) else {
        return;
    };
    let (_, waiter) = waiters.remove(0);
    if waiters.is_empty() {
        domain.house.waits.remove(&(household, object));
    }
    domain.house.holds.insert((household, object, slot), waiter);
    let handle = ctx.schedule(ctx.now(), EventPayload::Decide { sim: waiter }, by);
    if let Some(stale) = domain.pending.insert((waiter, Slot::Decide), handle) {
        ctx.cancel(stale);
    }
}

/// The objects of `house` with no free place.
fn busy(house: &House, houses: &HouseData, taken: &BTreeSet<(u16, u8)>) -> Vec<u16> {
    (0_u16..)
        .zip(&house.def().objects)
        .filter(|(object, def)| {
            let slots = houses.kind(&def.kind).map_or(0, |kind| kind.slots);
            (0..slots).all(|slot| taken.contains(&(*object, slot)))
        })
        .map(|(object, _)| object)
        .collect()
}

/// A person idle in a room chooses now (ADR-0026 section 3, and in its night as its addendum
/// of 2026-11-04 says, `choose`), with its mood for each offer in this second (the addendum
/// of 2026-11-04 on the mood): it walks to the chosen offer, holding the smallest free place
/// of its object; or it idles, waiting for an object if the choice says so. Each choice counts
/// against the day's budget and cancels a pending `Decide`.
pub(crate) fn decide(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    causes: Causes,
    ctx: &mut SimCtx<'_, '_>,
) {
    let now = ctx.now();
    let (household, houses, house) = home(domain, data, sim);
    let age = age(domain, sim, now);
    let tonight = night(houses.day(), born(domain, sim), now);
    let seed = (domain.world_seed).unwrap_or_else(|| panic!("invariant seeds.before_start"));
    let Some(key) = domain.sims.get(&sim).map(|person| person.lineage_key) else {
        panic!("invariant unknown_entity_id: sim {}", sim.get());
    };
    // Who chooses leaves every waiting list (ADR-0026 section 5).
    let held = stop_waiting(domain, household, sim, ctx);
    let taken = taken(domain, household, sim);
    let busy = busy(house, houses, &taken);
    let person = person_mut(domain, sim);
    let (Place::Room(room), None) = (person.place, person.action) else {
        panic!("invariant house.choice_busy");
    };
    let today = day_index(now);
    if person.decisions.day != today {
        person.decisions = Decisions {
            day: today,
            count: 0,
        };
    }
    if person.decisions.count >= MAX_DECISIONS {
        panic!("invariant house.decision_budget");
    }
    person.decisions.count += 1;
    let needs = person.needs.map(|need| need.value_at(now));
    let left = tonight.map(|night| night.left(now));
    let moods = |object, offer| mood(seed, key, now, object, offer);
    let scored = score(house, houses, age, needs, room, left, moods);
    let choice = choose(
        (house, houses),
        (age, needs),
        &scored,
        &busy,
        tonight.is_some(),
    );
    let chosen = match choice {
        Choice::Take(chosen) => Some(chosen),
        Choice::Wait(_) | Choice::Idle => None,
    };
    let walk = chosen.map(|chosen| house.walk_to(room, chosen.object));
    let arrive = walk.and_then(|walk| now.checked_add(SimDuration::from_secs(i64::from(walk))));
    let (Some(chosen), Some(walk_secs), Some(arrive)) = (chosen, walk, arrive) else {
        let waits = match choice {
            Choice::Wait(object) => Some(object),
            Choice::Take(_) | Choice::Idle => None,
        };
        if let Some(object) = waits {
            let waiters = domain.house.waits.entry((household, object)).or_default();
            let at = waiters.partition_point(|&waiter| waiter < (now, sim));
            waiters.insert(at, (now, sim));
        }
        let idle = LogPayload::Idle {
            house: house.id().clone(),
            room,
            needs,
            busy,
            waits,
        };
        let entry = ctx.emit(idle, vec![Subject::Sim(sim)], causes);
        if let Some(place) = held {
            release(domain, household, place, entry, ctx);
        }
        return;
    };
    let walk = Walk {
        object: chosen.object,
        offer: chosen.offer,
        room,
        walk_secs,
        arrive,
    };
    let seen = (needs, &taken, busy);
    let (entry, slot) = begin(domain, house, sim, walk, seen, (false, causes), ctx);
    // A held place the choice did not take goes on to the next who waits.
    if let Some(place) = held.filter(|&place| place != (chosen.object, slot)) {
        release(domain, household, place, entry, ctx);
    }
}

/// The person stops waiting before what it does next (ADR-0026 sections 3 and 5): its planned
/// choice is cancelled, it leaves every waiting list, and a place held for it is held no more.
/// Returns that place.
fn stop_waiting(
    domain: &mut DomainState,
    household: HouseholdId,
    sim: SimId,
    ctx: &mut SimCtx<'_, '_>,
) -> Option<(u16, u8)> {
    if let Some(handle) = domain.pending.remove(&(sim, Slot::Decide)) {
        ctx.cancel(handle);
    }
    for waiters in domain.house.waits.values_mut() {
        waiters.retain(|&(_, waiter)| waiter != sim);
    }
    domain.house.waits.retain(|_, waiters| !waiters.is_empty());
    let held = held_by(domain, household, sim);
    if let Some((object, slot)) = held {
        domain.house.holds.remove(&(household, object, slot));
    }
    held
}

/// A walk that begins now (ADR-0026 section 4): to `object` for `offer` from `room`,
/// `walk_secs` long, arriving at `arrive`.
struct Walk {
    object: u16,
    offer: u16,
    room: u8,
    walk_secs: u32,
    arrive: SimTime,
}

/// The person walks as `walk` says, holding the object's free place with the smallest number,
/// `taken` the places taken or held for others: its entry `core:action_started` by `causes`
/// with the needs and the objects without a free place it saw and whether a command chose it,
/// then its arrival planned by that entry. Returns the entry and the place.
fn begin(
    domain: &mut DomainState,
    house: &House,
    sim: SimId,
    walk: Walk,
    (needs, taken, busy): ([u16; 5], &BTreeSet<(u16, u8)>, Vec<u16>),
    (commanded, causes): (bool, Causes),
    ctx: &mut SimCtx<'_, '_>,
) -> (EventId, u8) {
    let Walk {
        object,
        offer,
        room,
        walk_secs,
        arrive,
    } = walk;
    let slot = (0..u8::MAX)
        .find(|slot| !taken.contains(&(object, *slot)))
        .unwrap_or_else(|| panic!("invariant house.slot_twice"));
    let started = LogPayload::ActionStarted {
        house: house.id().clone(),
        object,
        offer,
        slot,
        room,
        walk_secs,
        needs,
        busy,
        commanded,
    };
    let entry = ctx.emit(started, vec![Subject::Sim(sim)], causes);
    let depart = ctx.now();
    let person = person_mut(domain, sim);
    person.place = Place::Walking {
        from: room,
        to: house.def().objects[usize::from(object)].room,
        depart,
        arrive,
    };
    person.action = Some(Action {
        object,
        offer,
        slot,
    });
    let handle = ctx.schedule(arrive, EventPayload::Arrive { sim }, entry);
    domain.pending.insert((sim, Slot::Arrive), handle);
    (entry, slot)
}

/// The commanded walk (ADR-0024 section 5): the person, in a room once what it did has ended,
/// stops waiting and walks to offer `offer` of `object` as after a choice, on the place held
/// for it or a free one, its entry `core:action_started` by `causes` with `commanded`; a held
/// place it does not take goes on to the next who waits. A command is no choice and counts
/// against no budget of choices.
pub(super) fn obey(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    (object, offer): (u16, u16),
    causes: Causes,
    ctx: &mut SimCtx<'_, '_>,
) {
    let now = ctx.now();
    let (household, houses, house) = home(domain, data, sim);
    let held = stop_waiting(domain, household, sim, ctx);
    let taken = taken(domain, household, sim);
    let busy = busy(house, houses, &taken);
    let person = person_mut(domain, sim);
    let (Place::Room(room), None) = (person.place, person.action) else {
        panic!("invariant house.command_busy");
    };
    let needs = person.needs.map(|need| need.value_at(now));
    let walk_secs = house.walk_to(room, object);
    let arrive = (now.checked_add(SimDuration::from_secs(i64::from(walk_secs))))
        .unwrap_or_else(|| panic!("invariant house.command_checked"));
    let walk = Walk {
        object,
        offer,
        room,
        walk_secs,
        arrive,
    };
    let seen = (needs, &taken, busy);
    let (entry, slot) = begin(domain, house, sim, walk, seen, (true, causes), ctx);
    if let Some(place) = held.filter(|&place| place != (object, slot)) {
        release(domain, household, place, entry, ctx);
    }
}

/// A planned choice: the first morning, or coming in after it (ADR-0026 section 3).
pub(crate) fn on_decide(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    ctx: &mut SimCtx<'_, '_>,
) {
    // The event that runs right now is no longer pending.
    domain.pending.remove(&(sim, Slot::Decide));
    decide(domain, data, sim, Causes::new(ctx.trigger()), ctx);
}

/// A falling need of a person reaches a threshold (ADR-0026 sections 1 and 4). At 700 and 400
/// a person idle in a room chooses, otherwise nothing happens. At 150 and 50 the need is
/// critical: an entry, then a person idle in a room chooses, and a walk to an offer that will
/// not raise the need, or an action under way, ends with reason `critical` and the person
/// chooses again.
pub(crate) fn on_crossing(
    domain: &mut DomainState,
    data: &Data,
    (sim, need, level): (SimId, Need, u16),
    ctx: &mut SimCtx<'_, '_>,
) {
    // The event that runs right now is no longer pending.
    domain.pending.remove(&(sim, Slot::Crossing(need, level)));
    let Some(person) = domain.house.sims.get(&sim) else {
        panic!("invariant house.orphan_event");
    };
    let (place, action) = (person.place, person.action);
    let mut causes = Causes::new(ctx.trigger());
    if level <= CRITICAL {
        let critical = LogPayload::NeedCritical { need, level };
        let entry = ctx.emit(critical, vec![Subject::Sim(sim)], causes);
        causes = Causes::new(Cause::Event(entry));
    }
    match (place, action) {
        (Place::Room(_), None) => decide(domain, data, sim, causes, ctx),
        // At the object the need falls, so the action does not raise it. On the way the
        // offer's rate is still to come, in the age band of the arrival.
        (_, Some(_)) if level <= CRITICAL && !raises(domain, data, sim, need) => {
            let ended = interrupt(domain, data, sim, EndReason::Critical, causes, ctx);
            next(domain, data, sim, ended, ctx);
        }
        _ => {}
    }
}

/// A person's walk or action ends now for `reason` (ADR-0026 section 4): its arrival or end
/// is cancelled. Returns the entry `core:action_ended`.
pub(super) fn interrupt(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    reason: EndReason,
    causes: Causes,
    ctx: &mut SimCtx<'_, '_>,
) -> EventId {
    for slot in [Slot::Arrive, Slot::End] {
        if let Some(handle) = domain.pending.remove(&(sim, slot)) {
            ctx.cancel(handle);
        }
    }
    end(domain, data, sim, |_, _| reason, causes, ctx)
}

/// What comes after an action that ended with `entry` (ADR-0026 section 4): a departure that
/// waited for it leaves now, late against its plan (ADR-0024 section 4); otherwise a choice.
pub(super) fn next(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    entry: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let causes = Causes::new(Cause::Event(entry));
    match domain.house.sims.get(&sim).and_then(|person| person.due) {
        Some(due) => {
            let late = (ctx.now() - due.depart).as_secs();
            let late = u32::try_from(late).unwrap_or_else(|_| panic!("invariant house.late"));
            super::day::leave(domain, data, sim, (due.place, late), causes, ctx);
        }
        None => decide(domain, data, sim, causes, ctx),
    }
}

/// A person reaches the object of its action (ADR-0026 section 4): it begins, the needs the
/// offer sets take its rates, the crossings are planned anew and the end is planned, for a
/// sleep at the end of its night. At the end of the walk out the person is away (ADR-0024
/// section 4).
pub(crate) fn on_arrive(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    ctx: &mut SimCtx<'_, '_>,
) {
    // The event that runs right now is no longer pending.
    domain.pending.remove(&(sim, Slot::Arrive));
    let now = ctx.now();
    let (_, houses, house) = home(domain, data, sim);
    let band = band(domain, sim, now);
    let born = born(domain, sim);
    let person = person_mut(domain, sim);
    let (Place::Walking { to, .. }, action) = (person.place, person.action) else {
        panic!("invariant house.orphan_event");
    };
    // Out of the door the person is away; its needs stand still since it left (ADR-0024
    // section 4).
    let Some(action) = action else {
        person.place = Place::Away;
        return;
    };
    let offer = offer_of(house, houses, action);
    let reached = LogPayload::Reached {
        object: action.object,
    };
    let entry = ctx.emit(reached, vec![Subject::Sim(sim)], Causes::new(ctx.trigger()));
    person.place = Place::Room(to);
    for (sample, need) in person.needs.iter_mut().zip(NEEDS) {
        if offer.sets(need) {
            *sample = sample.with_rate(now, offer.rate(houses.rates(), need, band));
        }
    }
    let end = action_end(person, offer, houses.day(), born);
    plan_crossings(domain, sim, entry, ctx);
    if let Some(end) = end {
        let handle = ctx.schedule(end, EventPayload::ActionEnd { sim }, entry);
        domain.pending.insert((sim, Slot::End), handle);
    }
}

/// A person's action ends (ADR-0026 section 4): `full` if the need it fills is full, else
/// `done`; then the person chooses again, or leaves if its departure waited.
pub(crate) fn on_action_end(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    ctx: &mut SimCtx<'_, '_>,
) {
    // The event that runs right now is no longer pending.
    domain.pending.remove(&(sim, Slot::End));
    let at_object = (domain.house.sims.get(&sim))
        .is_some_and(|person| matches!(person.place, Place::Room(_)) && person.action.is_some());
    if !at_object {
        panic!("invariant house.orphan_event");
    }
    let full = |offer: &OfferDef, needs: [u16; 5]| {
        if (offer.until_full).is_some_and(|need| needs[need_index(need)] == FULL) {
            EndReason::Full
        } else {
            EndReason::Done
        }
    };
    let entry = end(domain, data, sim, full, Causes::new(ctx.trigger()), ctx);
    next(domain, data, sim, entry, ctx);
}

/// A person's walk or action ends for the reason `reason` gives for the offer and the needs of
/// now (ADR-0026 section 4), the rates having worked until now: on the way the person is in the
/// room it came from again and the walk counts as not gone (ADR-0025 section 5); at the object
/// the needs the offer set take the awake rates again and the crossings are planned anew. Its
/// place is free. Returns the entry `core:action_ended`.
fn end(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    reason: impl FnOnce(&OfferDef, [u16; 5]) -> EndReason,
    causes: Causes,
    ctx: &mut SimCtx<'_, '_>,
) -> EventId {
    let now = ctx.now();
    let (household, houses, house) = home(domain, data, sim);
    let band = band(domain, sim, now);
    let person = person_mut(domain, sim);
    let Some(action) = person.action.take() else {
        panic!("invariant house.orphan_event");
    };
    let offer = offer_of(house, houses, action);
    let needs = person.needs.map(|need| need.value_at(now));
    let ended = LogPayload::ActionEnded {
        object: action.object,
        offer: action.offer,
        reason: reason(offer, needs),
        needs,
    };
    let entry = ctx.emit(ended, vec![Subject::Sim(sim)], causes);
    let place = person.place;
    match place {
        Place::Walking { from, .. } => person.place = Place::Room(from),
        Place::Room(_) => {
            for (sample, need) in person.needs.iter_mut().zip(NEEDS) {
                if offer.sets(need) {
                    *sample = sample.with_rate(now, houses.rates().awake.rate(need, band));
                }
            }
            plan_crossings(domain, sim, entry, ctx);
        }
        Place::Away => panic!("invariant house.orphan_event"),
    }
    release(domain, household, (action.object, action.slot), entry, ctx);
    entry
}

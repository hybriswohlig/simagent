//! What a person in a house does (ADR-0026 sections 3 and 4): it chooses when it is idle in a
//! room, walks to the chosen object holding one of its places, arrives and takes the offer's
//! rates, and at the end takes the awake rates again and chooses anew.

use alloc::collections::BTreeSet;
use alloc::vec;
use alloc::vec::Vec;

use super::house::plan_crossings;
use super::{DomainState, Slot, whole_years};
use crate::data::Data;
use crate::house::choice::{ENOUGH, best, score};
use crate::house::data::{House, HouseData, NEEDS, Need, OfferDef, age_band};
use crate::house::need::FULL;
use crate::house::state::{Action, Decisions, InHouse, MAX_DECISIONS, Place, action_end};
use crate::ids::{HouseholdId, SimId};
use crate::log::{Cause, Causes, EndReason, LogPayload, Subject};
use crate::time::{SimDuration, SimTime, day_index};
use crate::world::{EventPayload, SimCtx};

/// A person's household, the house data and its house.
fn home<'a>(
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

/// A person's age in whole years at `now`.
fn age(domain: &DomainState, sim: SimId, now: SimTime) -> u8 {
    let Some(person) = domain.sims.get(&sim) else {
        panic!("invariant unknown_entity_id: sim {}", sim.get());
    };
    u8::try_from(whole_years(person.born, now)).unwrap_or(u8::MAX)
}

/// The age band of a person with needs at `now`.
fn band(domain: &DomainState, sim: SimId, now: SimTime) -> usize {
    age_band(age(domain, sim, now)).unwrap_or_else(|| panic!("invariant house.age"))
}

fn person_mut(domain: &mut DomainState, sim: SimId) -> &mut InHouse {
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

/// The places of the household's objects taken by an action or held, as (object, place).
fn taken(domain: &DomainState, household: HouseholdId) -> BTreeSet<(u16, u8)> {
    let members = domain.members.get(&household).into_iter().flatten();
    let actions = members.filter_map(|sim| domain.house.sims.get(sim)?.action);
    let mut taken: BTreeSet<(u16, u8)> = actions.map(|a| (a.object, a.slot)).collect();
    let held = (domain
        .house
        .holds
        .range((household, 0, 0)..=(household, u16::MAX, u8::MAX)))
    .map(|(&(_, object, slot), _)| (object, slot));
    taken.extend(held);
    taken
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

/// A person idle in a room chooses now (ADR-0026 section 3): it walks to the offer with the
/// most points among the objects with a free place, holding the smallest free place, if that
/// brings enough; otherwise it idles. Each choice counts against the day's budget and
/// cancels a pending `Decide`.
pub(crate) fn decide(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    causes: Causes,
    ctx: &mut SimCtx<'_, '_>,
) {
    let now = ctx.now();
    if let Some(handle) = domain.pending.remove(&(sim, Slot::Decide)) {
        ctx.cancel(handle);
    }
    let (household, houses, house) = home(domain, data, sim);
    let age = age(domain, sim, now);
    let taken = taken(domain, household);
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
    let scored = score(house, houses, age, needs, room);
    let chosen = best(&scored, &busy).filter(|chosen| chosen.points >= ENOUGH);
    let walk = chosen.map(|chosen| house.walk_to(room, chosen.object));
    let arrive = walk.and_then(|walk| now.checked_add(SimDuration::from_secs(i64::from(walk))));
    let (Some(chosen), Some(walk_secs), Some(arrive)) = (chosen, walk, arrive) else {
        let idle = LogPayload::Idle {
            house: house.id().clone(),
            room,
            needs,
            busy,
            waits: None,
        };
        ctx.emit(idle, vec![Subject::Sim(sim)], causes);
        return;
    };
    let object = chosen.object;
    let slot = (0..u8::MAX)
        .find(|slot| !taken.contains(&(object, *slot)))
        .unwrap_or_else(|| panic!("invariant house.slot_twice"));
    let started = LogPayload::ActionStarted {
        house: house.id().clone(),
        object,
        offer: chosen.offer,
        slot,
        room,
        walk_secs,
        needs,
        busy,
        commanded: false,
    };
    let entry = ctx.emit(started, vec![Subject::Sim(sim)], causes);
    person.place = Place::Walking {
        from: room,
        to: house.def().objects[usize::from(object)].room,
        depart: now,
        arrive,
    };
    person.action = Some(Action {
        object,
        offer: chosen.offer,
        slot,
    });
    let handle = ctx.schedule(arrive, EventPayload::Arrive { sim }, entry);
    domain.pending.insert((sim, Slot::Arrive), handle);
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

/// A falling need of a person reaches a threshold (ADR-0026 section 1): idle in a room, the
/// person chooses; otherwise nothing happens.
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
    if matches!(person.place, Place::Room(_)) && person.action.is_none() {
        decide(domain, data, sim, Causes::new(ctx.trigger()), ctx);
    }
}

/// A person reaches the object of its action (ADR-0026 section 4): it begins, the needs the
/// offer sets take its rates, the crossings are planned anew and the end is planned.
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
    let person = person_mut(domain, sim);
    let (Place::Walking { to, .. }, Some(action)) = (person.place, person.action) else {
        panic!("invariant house.orphan_event");
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
    let end = action_end(person, offer);
    plan_crossings(domain, sim, entry, ctx);
    if let Some(end) = end {
        let handle = ctx.schedule(end, EventPayload::ActionEnd { sim }, entry);
        domain.pending.insert((sim, Slot::End), handle);
    }
}

/// A person's action ends (ADR-0026 section 4): `full` if the need it fills is full, else
/// `done`; the needs the offer set go back to the awake rates, the crossings are planned
/// anew, the place is free and the person chooses again.
pub(crate) fn on_action_end(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    ctx: &mut SimCtx<'_, '_>,
) {
    // The event that runs right now is no longer pending.
    domain.pending.remove(&(sim, Slot::End));
    let now = ctx.now();
    let (_, houses, house) = home(domain, data, sim);
    let band = band(domain, sim, now);
    let person = person_mut(domain, sim);
    let (Place::Room(_), Some(action)) = (person.place, person.action) else {
        panic!("invariant house.orphan_event");
    };
    let offer = offer_of(house, houses, action);
    let needs = person.needs.map(|need| need.value_at(now));
    let full = (offer.until_full).is_some_and(|need| needs[need_index(need)] == FULL);
    let ended = LogPayload::ActionEnded {
        object: action.object,
        offer: action.offer,
        reason: if full {
            EndReason::Full
        } else {
            EndReason::Done
        },
        needs,
    };
    let entry = ctx.emit(ended, vec![Subject::Sim(sim)], Causes::new(ctx.trigger()));
    for (sample, need) in person.needs.iter_mut().zip(NEEDS) {
        if offer.sets(need) {
            *sample = sample.with_rate(now, houses.rates().awake.rate(need, band));
        }
    }
    person.action = None;
    plan_crossings(domain, sim, entry, ctx);
    decide(domain, data, sim, Causes::new(Cause::Event(entry)), ctx);
}

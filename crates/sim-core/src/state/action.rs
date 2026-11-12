//! What a person in a house does (ADR-0026 sections 3 and 4): it chooses when it is idle in a
//! room, walks to the chosen object holding one of its places, arrives and takes the offer's
//! rates, and at the end takes the awake rates again and chooses anew, or leaves if its
//! departure waited for the end (ADR-0024 section 4). A need turning critical ends what does
//! not raise it; one reaching 0 has its consequence (`state::zero`).

use alloc::collections::BTreeSet;
use alloc::vec;
use alloc::vec::Vec;

use super::house::{plan_crossings, planned_in};
use super::{DomainState, Slot};
use crate::data::Data;
use crate::house::choice::{Bonus, CRITICAL, Choice, choose, filler, in_tier, mood, score, tier};
use crate::house::data::{BedFor, House, HouseData, NEEDS, Need, OfferDef, age_band};
use crate::house::day::{departure, night, work_draw};
use crate::house::need::{EMERGENCY, EMPTY, FULL};
use crate::house::state::{
    Action, Decisions, InHouse, MAX_DECISIONS, Place, action_end, child_held, dwell_walk,
    filler_end_if,
};
use crate::ids::{EventId, HouseholdId, SimId};
use crate::log::{Cause, Causes, Company, EndReason, LogPayload, Subject};
use crate::social::bond::choice_bonus;
use crate::social::feeling::{read, scaled_company};
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

pub(super) fn need_index(need: Need) -> usize {
    (NEEDS.iter().position(|&each| each == need)).unwrap_or_else(|| panic!("invariant house.need"))
}

/// Whether a person's walk or action raises `need` (ADR-0026 section 4): at the object the
/// need rises, on the way the offer's rate for it is positive in the age band of the arrival.
pub(super) fn raises(domain: &DomainState, data: &Data, sim: SimId, need: Need) -> bool {
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
/// A small child in its bed holds one at `now` as well (ADR-0026, addendum of 2026-11-04 after
/// the playtest).
pub(super) fn taken(
    domain: &DomainState,
    data: &Data,
    household: HouseholdId,
    sim: SimId,
    now: SimTime,
) -> BTreeSet<(u16, u8)> {
    let members = domain.members.get(&household).into_iter().flatten();
    let actions = members.filter_map(|sim| domain.house.sims.get(sim)?.action);
    let mut taken: BTreeSet<(u16, u8)> = actions.map(|a| (a.object, a.slot)).collect();
    let held = (domain.house.holds.range(household_places(household)))
        .filter(|&(_, &holder)| holder != sim)
        .map(|(&(_, object, slot), _)| (object, slot));
    taken.extend(held);
    taken.extend(young_held(domain, data, household, now));
    taken
}

/// The places the household's small children hold in their beds at `now` (ADR-0026, addendum
/// of 2026-11-04 after the playtest): after its first morning, its living members without
/// needs, by SimId, each the next place of `House::child_beds`, while the clock puts them in bed.
pub(crate) fn young_held(
    domain: &DomainState,
    data: &Data,
    household: HouseholdId,
    now: SimTime,
) -> Vec<(u16, u8)> {
    let Some(moved) = domain.house.households.get(&household) else {
        return Vec::new();
    };
    let (Some(houses), true) = (data.houses(), moved.morning) else {
        return Vec::new();
    };
    let house = (houses.house(&moved.house)).unwrap_or_else(|| panic!("invariant house.unknown"));
    let members = domain.members.get(&household).into_iter().flatten();
    let young = members
        .filter(|sim| !domain.house.sims.contains_key(sim))
        .count();
    child_held(house, houses, young, now)
}

/// The objects with a place the household's small children hold at `now`, in their order.
fn held_objects(
    domain: &DomainState,
    data: &Data,
    household: HouseholdId,
    now: SimTime,
) -> Vec<u16> {
    let mut objects: Vec<u16> = (young_held(domain, data, household, now).into_iter())
        .map(|(object, _)| object)
        .collect();
    objects.dedup();
    objects
}

/// The partner of `sim` in an open partnership who lives in the same household (ADR-0026,
/// addendum of 2026-11-04 after the playtest).
pub(crate) fn partner(domain: &DomainState, sim: SimId) -> Option<SimId> {
    let id = super::partnership::open_partnership(domain, sim)?;
    let pair = domain.partnerships.get(&id)?;
    let other = if pair.woman == sim {
        pair.man
    } else {
        pair.woman
    };
    let household = |sim: SimId| domain.sims.get(&sim).map(|person| person.household);
    (household(other).is_some() && household(other) == household(sim)).then_some(other)
}

/// The beds of the tier of `sim` at `age` in its night (`house::choice::tier`): its partner's
/// bed for couples if the partner holds a place on one.
pub(crate) fn tier_beds(
    domain: &DomainState,
    house: &House,
    houses: &HouseData,
    sim: SimId,
    age: u8,
) -> Vec<u16> {
    let partner = partner(domain, sim);
    let couples = crate::house::choice::Tier::For(BedFor::Couples);
    let partner_bed = (partner.and_then(|partner| domain.house.sims.get(&partner)?.action))
        .map(|action| action.object)
        .filter(|&object| in_tier(house, houses, couples, object));
    let tier = tier(partner.is_some(), partner_bed, age);
    (0_u16..)
        .zip(&house.def().objects)
        .map(|(object, _)| object)
        .filter(|&object| in_tier(house, houses, tier, object))
        .collect()
}

/// The household's free places changed in this handler, last by the entry `by`: the idle
/// persons' next choices are planned anew when it ends (`house::settle`, ADR-0026, addendum of
/// 2026-11-04 after the playtest).
pub(super) fn changed(domain: &mut DomainState, household: HouseholdId, by: EventId) {
    domain.house_changed.insert(household, by);
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
    changed(domain, household, by);
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
/// of 2026-11-04 on the mood), an offer that would run into today's departure closed: it walks
/// to the chosen offer, holding the smallest free place of its object; or it idles, waiting for
/// an object if the choice says so. By day one idle and waiting for nothing walks to its
/// time-filler instead, if one is free (the addendum on comfort and the time-filler). For a
/// sociable furniture where someone of the household sits or is on the way, it adds the data's
/// company and subtracts its avoid, outside the mood, and writes `core:company` before the choice
/// when either counts (ADR-0030 section 4). Each choice counts against the day's budget and
/// cancels a pending `Decide`.
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
    let birth = born(domain, sim);
    let tonight = night(houses.day(), birth, now);
    let seed = (domain.world_seed).unwrap_or_else(|| panic!("invariant seeds.before_start"));
    let Some(key) = domain.sims.get(&sim).map(|person| person.lineage_key) else {
        panic!("invariant unknown_entity_id: sim {}", sim.get());
    };
    // Who chooses leaves every waiting list (ADR-0026 section 5).
    let held = stop_waiting(domain, household, sim, ctx);
    let taken = taken(domain, data, household, sim, now);
    let busy = busy(house, houses, &taken);
    // In the night the beds of its tier, and those with a place a small child holds.
    let (beds, children) = match tonight {
        Some(_) => (
            tier_beds(domain, house, houses, sim, age),
            held_objects(domain, data, household, now),
        ),
        None => (Vec::new(), Vec::new()),
    };
    let person = person_mut(domain, sim);
    let (Place::Room(room), None, false) = (person.place, person.action, person.collapsed) else {
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
    let percent = houses.day().choice_mood_percent;
    let moods = |object, offer| mood(seed, key, now, (object, offer), percent);
    let leaves = departure(houses.day(), birth, || work_draw(seed, key), now);
    let depart_in = leaves.map(|leaves| (leaves - now).as_secs());
    let (bonuses, rows, temper) = company_of(domain, data, household, house, sim, now);
    let scored = score(
        house,
        houses,
        (age, needs, room),
        (left, depart_in),
        moods,
        temper,
        |object| (bonuses.get(usize::from(object)).copied()).unwrap_or(Bonus::NONE),
    );
    let choice = choose(
        (house, houses),
        (age, needs),
        &scored,
        &busy,
        tonight.map(|_| beds.as_slice()),
    );
    // By day one idle and waiting for nothing takes its time-filler, the one with the highest
    // mood, unless it would end under `fallback_min_seconds` from now (ADR-0026, addendum of
    // 2026-11-05 on the time-filler's length): then it stays idle, and the entry says when that
    // filler would have ended.
    let picked = match (choice, tonight) {
        (Choice::Idle, None) => filler((house, houses), (age, room), &busy, moods),
        _ => None,
    };
    let mut too_short = None;
    let filled = picked.filter(|&(object, offer)| {
        let walk_secs = super::reserve::walk(domain, data, sim, house.walk_to(room, object), now);
        let who = (birth, key, seed);
        let person =
            (domain.house.sims.get(&sim)).unwrap_or_else(|| panic!("invariant house.orphan_event"));
        let fill = (object, offer, walk_secs);
        let end = filler_end_if((house, houses), who, (person, &taken), fill, now);
        let shortest = i64::from(houses.day().fallback_min_seconds);
        let short = end.filter(|end| (*end - now).as_secs() < shortest);
        too_short = short;
        short.is_none()
    });
    let chosen = match (choice, filled) {
        (Choice::Take(chosen), _) => Some((chosen.object, chosen.offer)),
        (_, Some(filled)) => Some(filled),
        (Choice::Wait(_) | Choice::Idle, None) => None,
    };
    // Weak, a walk takes longer (ADR-0026, addendum of 2026-11-06 on hunger at 0); the choice
    // weighed the house's.
    let walk = chosen.map(|(object, _)| {
        super::reserve::walk(domain, data, sim, house.walk_to(room, object), now)
    });
    let arrive = walk.and_then(|walk| now.checked_add(SimDuration::from_secs(i64::from(walk))));
    if !rows.is_empty() {
        ctx.emit(
            LogPayload::Company { objects: rows },
            vec![Subject::Sim(sim)],
            causes.clone(),
        );
    }
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
            held: children,
            tier: beds,
            too_short,
        };
        let entry = ctx.emit(idle, vec![Subject::Sim(sim)], causes);
        // One idle and waiting for nothing goes from a room one does not stay in to the room
        // one does, without an action (ADR-0026, addendum of 2026-11-04 after the playtest); its
        // next choice is planned when the handler ends, for that room.
        if waits.is_none() {
            if !house.dwells(room) {
                let to = house.def().dwell_room;
                let secs = super::reserve::walk(domain, data, sim, house.walk_to_dwell(room), now);
                let walked = SimDuration::from_secs(i64::from(secs));
                if let Some(arrive) = now.checked_add(walked) {
                    let person = person_mut(domain, sim);
                    person.place = Place::Walking {
                        from: room,
                        to,
                        depart: now,
                        arrive,
                    };
                    let handle = ctx.schedule(arrive, EventPayload::Arrive { sim }, entry);
                    domain.pending.insert((sim, Slot::Arrive), handle);
                }
            }
            changed(domain, household, entry);
        }
        if let Some(place) = held {
            release(domain, household, place, entry, ctx);
        }
        return;
    };
    let walk = Walk {
        object: chosen.0,
        offer: chosen.1,
        room,
        walk_secs,
        arrive,
    };
    let seen = (needs, &taken, busy, (children, beds));
    let how = (
        Begun::Chosen {
            filler: filled.is_some(),
        },
        causes,
    );
    let (entry, slot) = begin(domain, house, sim, walk, seen, how, ctx);
    // A held place the choice did not take goes on to the next who waits.
    if let Some(place) = held.filter(|&place| place != (chosen.0, slot)) {
        release(domain, household, place, entry, ctx);
    }
}

/// Company and avoid for this choice, the log rows of the furniture where one of them counts
/// (ADR-0030 section 4) and the chooser's temper at `now`, with which `score` scales the company
/// and the rows show it scaled (ADR-0032 section 5). Company counts another member of the
/// household with an action on that object, sitting or still on the way. Avoid counts only one
/// who has arrived and sits. A bare hold is not enough, and a person with no bond is skipped.
/// Without social data the bonus is 0, the temper 0 and there is no row.
fn company_of(
    domain: &DomainState,
    data: &Data,
    household: HouseholdId,
    house: &House,
    sim: SimId,
    now: SimTime,
) -> (Vec<Bonus>, Vec<Company>, i16) {
    let Some(social) = data.social() else {
        return (Vec::new(), Vec::new(), 0);
    };
    let temper = read(
        domain.house.social.feelings.get(&sim),
        now,
        &social.feelings,
    )
    .1;
    let Some(members) = domain.members.get(&household) else {
        return (Vec::new(), Vec::new(), temper);
    };
    let rules = &social.choice;
    let familiarity = social.bonds.familiarity_fade;
    let grudge = social.bonds.grudge_fade;
    let bonds = domain.house.social.bonds.get(&sim);
    let objects = &house.def().objects;
    let mut bonuses = vec![Bonus::NONE; objects.len()];
    let mut rows = Vec::new();
    for (object, def) in (0_u16..).zip(objects) {
        if !social.sociable(&def.kind) {
            continue;
        }
        let mut for_company = Vec::new();
        let mut for_avoid = Vec::new();
        for other in members {
            if *other == sim {
                continue;
            }
            let Some(person) = domain.house.sims.get(other) else {
                continue;
            };
            let Some(action) = person.action else {
                continue;
            };
            if action.object != object {
                continue;
            }
            // On the way the place is reserved, so company counts. Avoid waits for arrival.
            for_company.push(*other);
            if matches!(person.place, Place::Room(_)) {
                for_avoid.push(*other);
            }
        }
        let edge = |other| {
            let list = bonds?;
            let at = list.binary_search_by_key(&other, |bond| bond.other).ok()?;
            Some(list[at].values_at(now, familiarity, grudge))
        };
        let points = (
            i64::from(rules.company),
            i64::from(rules.avoid),
            rules.company_min_sum,
            rules.avoid_below_trust,
        );
        let liked = choice_bonus(&for_company, edge, points);
        let avoided = choice_bonus(&for_avoid, edge, points);
        if liked.liked.is_none() && avoided.avoided.is_none() {
            continue;
        }
        bonuses[usize::from(object)] = Bonus {
            company: liked.company,
            avoid: avoided.avoid,
        };
        rows.push(Company {
            object,
            liked: liked.liked,
            company: scaled_company(liked.company, temper),
            avoided: avoided.avoided,
            avoid: avoided.avoid,
        });
    }
    (bonuses, rows, temper)
}

/// How a walk begins: by a choice, as a time-filler or not, or by a command (ADR-0024 section 5).
#[derive(Clone, Copy)]
enum Begun {
    Chosen { filler: bool },
    Commanded,
}

/// The next choices of the idle persons of each household whose free places changed in this
/// handler (ADR-0026, addendum of 2026-11-04 after the playtest), in the order of the
/// households and their members: each idle in a room or on its walk to the room one stays in,
/// not lying and waiting for nothing, chooses at `next_choice_of` from now on, planned by the
/// household's last entry that changed it; anyone else has no choice planned. A choice already
/// due in this second stays: it comes anyway, and plans the next. The end of a time-filler at
/// its object is its next choice, planned anew in the same way (`filler_end_of`; ADR-0026,
/// addendum of 2026-11-04 on comfort and the time-filler); in an input at the clock even one due
/// in its second, which has not come yet, as a loaded world plans it from the clock. Each plan is
/// made in `planned_in`.
pub(crate) fn plan_idle(domain: &mut DomainState, data: &Data, ctx: &mut SimCtx<'_, '_>) {
    for (household, by) in core::mem::take(&mut domain.house_changed) {
        if !domain.house.households.contains_key(&household) {
            continue;
        }
        let members: Vec<SimId> = (domain.members.get(&household).into_iter().flatten())
            .copied()
            .collect();
        for sim in members {
            plan_next(domain, data, (household, sim), by, ctx);
        }
    }
}

/// The next choice of `sim` of `household` anew (`plan_idle`).
fn plan_next(
    domain: &mut DomainState,
    data: &Data,
    (household, sim): (HouseholdId, SimId),
    by: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let now = ctx.now();
    let Some(person) = domain.house.sims.get(&sim) else {
        return;
    };
    let filling =
        matches!((person.place, person.action), (Place::Room(_), Some(action)) if action.filler);
    if filling {
        plan_filler_end(domain, data, (household, sim), by, ctx);
        return;
    }
    let pending = domain.pending.get(&(sim, Slot::Decide)).copied();
    let due = pending.and_then(|handle| ctx.pending_time(handle));
    if due == Some(now) {
        return;
    }
    let (_, houses, house) = home(domain, data, sim);
    let waits = (domain.house.waits.values().flatten()).any(|&(_, waiter)| waiter == sim);
    let stands = matches!(person.place, Place::Room(_)) || dwell_walk(person, house);
    let idle = stands && person.action.is_none() && !person.collapsed && !waits;
    let next = match idle {
        true => {
            let seed =
                (domain.world_seed).unwrap_or_else(|| panic!("invariant seeds.before_start"));
            let Some(key) = domain.sims.get(&sim).map(|person| person.lineage_key) else {
                panic!("invariant unknown_entity_id: sim {}", sim.get());
            };
            let taken = taken(domain, data, household, sim, now);
            let who = (born(domain, sim), key, seed);
            let from = (now, planned_in(ctx));
            crate::house::state::next_choice_of((house, houses), who, person, &taken, from)
        }
        false => None,
    };
    if next == due {
        return;
    }
    if let Some(handle) = domain.pending.remove(&(sim, Slot::Decide)) {
        ctx.cancel(handle);
    }
    if let Some(at) = next {
        let handle = ctx.schedule(at, EventPayload::Decide { sim }, by);
        domain.pending.insert((sim, Slot::Decide), handle);
    }
}

/// The end of the time-filler of `sim` of `household` at its object anew (`plan_idle`): at the
/// second `filler_end_of` gives from now on, planned by `by`. One due in this second stays in a
/// handler; an input acts before the events of its second, so there it is planned anew too.
pub(super) fn plan_filler_end(
    domain: &mut DomainState,
    data: &Data,
    (household, sim): (HouseholdId, SimId),
    by: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let now = ctx.now();
    let pending = domain.pending.get(&(sim, Slot::End)).copied();
    let due = pending.and_then(|handle| ctx.pending_time(handle));
    if due == Some(now) && !ctx.in_input() {
        return;
    }
    let next = filler_end(domain, data, (household, sim), (now, planned_in(ctx)));
    if next == due {
        return;
    }
    if let Some(handle) = domain.pending.remove(&(sim, Slot::End)) {
        ctx.cancel(handle);
    }
    if let Some(at) = next {
        let handle = ctx.schedule(at, EventPayload::ActionEnd { sim }, by);
        domain.pending.insert((sim, Slot::End), handle);
    }
}

/// The second the time-filler of `sim` of `household` at its object ends from `now` on, planned
/// in the second `after` (`house::state::filler_end_of`), with the places of the household taken
/// or held for others.
fn filler_end(
    domain: &DomainState,
    data: &Data,
    (household, sim): (HouseholdId, SimId),
    (now, after): (SimTime, SimTime),
) -> Option<SimTime> {
    let (_, houses, house) = home(domain, data, sim);
    let seed = (domain.world_seed).unwrap_or_else(|| panic!("invariant seeds.before_start"));
    let Some(key) = domain.sims.get(&sim).map(|person| person.lineage_key) else {
        panic!("invariant unknown_entity_id: sim {}", sim.get());
    };
    let person = domain.house.sims.get(&sim)?;
    let taken = taken(domain, data, household, sim, now);
    let who = (born(domain, sim), key, seed);
    crate::house::state::filler_end_of((house, houses), who, person, &taken, (now, after))
}

/// The person stops waiting before what it does next (ADR-0026 sections 3 and 5): its planned
/// choice is cancelled, it leaves every waiting list, and a place held for it is held no more.
/// Returns that place.
pub(super) fn stop_waiting(
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

/// What a choice saw (`core:action_started`): the needs, the places taken or held for others,
/// the objects without a free place, and in the night the objects with a place a small child
/// holds and the beds of the person's tier.
type Seen<'a> = (
    [u16; 5],
    &'a BTreeSet<(u16, u8)>,
    Vec<u16>,
    (Vec<u16>, Vec<u16>),
);

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
/// with the needs and the objects without a free place it saw, whether a command chose it and
/// whether it is a time-filler, then its arrival planned by that entry. Returns the entry and the
/// place.
fn begin(
    domain: &mut DomainState,
    house: &House,
    sim: SimId,
    walk: Walk,
    (needs, taken, busy, (held, tier)): Seen<'_>,
    (begun, causes): (Begun, Causes),
    ctx: &mut SimCtx<'_, '_>,
) -> (EventId, u8) {
    let (commanded, filler) = match begun {
        Begun::Chosen { filler } => (false, filler),
        Begun::Commanded => (true, false),
    };
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
        held,
        tier,
        filler,
    };
    let entry = ctx.emit(started, vec![Subject::Sim(sim)], causes);
    let depart = ctx.now();
    if let Some(household) = domain.sims.get(&sim).map(|person| person.household) {
        changed(domain, household, entry);
    }
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
        filler,
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
    let taken = taken(domain, data, household, sim, now);
    let busy = busy(house, houses, &taken);
    let children = match night(houses.day(), born(domain, sim), now) {
        Some(_) => held_objects(domain, data, household, now),
        None => Vec::new(),
    };
    let person = person_mut(domain, sim);
    let (Place::Room(room), None) = (person.place, person.action) else {
        panic!("invariant house.command_busy");
    };
    let needs = person.needs.map(|need| need.value_at(now));
    let walk_secs = super::reserve::walk(domain, data, sim, house.walk_to(room, object), now);
    let arrive = (now.checked_add(SimDuration::from_secs(i64::from(walk_secs))))
        .unwrap_or_else(|| panic!("invariant house.command_checked"));
    let walk = Walk {
        object,
        offer,
        room,
        walk_secs,
        arrive,
    };
    // A command is no choice: it has no tier.
    let seen = (needs, &taken, busy, (children, Vec::new()));
    let (entry, slot) = begin(
        domain,
        house,
        sim,
        walk,
        seen,
        (Begun::Commanded, causes),
        ctx,
    );
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
/// a person idle in a room chooses, unless it waits for an object (ADR-0026, addendum of
/// 2026-11-04 on comfort and the time-filler), otherwise nothing happens. At 150 and 50 the need is
/// critical: an entry, then a person idle in a room chooses, and a walk to an offer that will
/// not raise the need, or an action under way, ends with reason `critical` and the person
/// chooses again. At 0 every need of the person at 0 in this second has its consequence
/// (`state::zero::settle`): those whose crossing at 0 is still to run in it too, and the need of
/// this one. One lying on the floor chooses at none of them: at 150 the entry is all, at 50 it
/// gets up if its energy is above 0 again (ADR-0026, addendum of 2026-11-04 on a need at 0).
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
    let (place, action, lying) = (person.place, person.action, person.collapsed);
    let now = ctx.now();
    let rested = person.needs[need_index(Need::Energy)].value_at(now) > EMPTY;
    if level == EMPTY {
        let at_zero = person.needs[need_index(need)].value_at(now) == EMPTY;
        let mut reached = super::zero::empty_now(domain, sim, ctx);
        reached[need_index(need)] |= at_zero;
        let causes = Causes::new(ctx.trigger());
        super::zero::settle(domain, data, sim, (reached, true), causes, ctx);
        return;
    }
    let mut causes = Causes::new(ctx.trigger());
    let mut critical = None;
    if level <= CRITICAL {
        let payload = LogPayload::NeedCritical { need, level };
        let entry = ctx.emit(payload, vec![Subject::Sim(sim)], causes);
        causes = Causes::new(Cause::Event(entry));
        critical = Some(entry);
    }
    if lying {
        // Lying raises energy alone, and energy that rises crosses nothing; with energy still at
        // 0 the person cannot get up.
        if let Some(entry) = critical.filter(|_| level == EMERGENCY && rested) {
            super::zero::rise(domain, data, sim, entry, causes, ctx);
        }
        return;
    }
    // A waiter keeps its place at 700 and 400; 150, 50 and 0 still make it choose (ADR-0026,
    // addendum of 2026-11-04 on comfort and the time-filler).
    let waits = (domain.house.waits.values().flatten()).any(|&(_, waiter)| waiter == sim);
    match (place, action) {
        (Place::Room(_), None) if waits && level > CRITICAL => {}
        (Place::Room(_), None) => decide(domain, data, sim, causes, ctx),
        // On the way to the room one stays in, 700 and 400 do nothing (the planned choice is
        // the wakeup); 150 and 50 end the walk back in the room it came from, and it chooses.
        (Place::Walking { .. }, None) if level <= CRITICAL => {
            if halt(domain, data, sim, ctx) {
                decide(domain, data, sim, causes, ctx);
            }
        }
        // At the object the need falls, so the action does not raise it. On the way the
        // offer's rate is still to come, in the age band of the arrival.
        (_, Some(_)) if level <= CRITICAL && !raises(domain, data, sim, need) => {
            let ended = interrupt(domain, data, sim, EndReason::Critical, causes, ctx);
            next(domain, data, sim, ended, ctx);
        }
        _ => {}
    }
}

/// A person on its walk without an action to the room one stays in (ADR-0026, addendum of
/// 2026-11-04 after the playtest) stops it: the walk counts as not gone, so it stands in the
/// room it came from again (ADR-0025 section 5), and its arrival and planned choice are
/// cancelled. Whether it was on such a walk; nothing happens to anyone else.
pub(super) fn halt(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    ctx: &mut SimCtx<'_, '_>,
) -> bool {
    let (_, _, house) = home(domain, data, sim);
    let person = person_mut(domain, sim);
    let Place::Walking { from, .. } = person.place else {
        return false;
    };
    if !dwell_walk(person, house) {
        return false;
    }
    person.place = Place::Room(from);
    for slot in [Slot::Arrive, Slot::Decide] {
        if let Some(handle) = domain.pending.remove(&(sim, slot)) {
            ctx.cancel(handle);
        }
    }
    true
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
    super::talk::on_stand_up(domain, data, sim, ctx);
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
    follow(domain, data, sim, Causes::new(Cause::Event(entry)), ctx);
}

/// What comes next for a person idle in a room, by `causes`: a departure that waited leaves
/// now, late against its plan (ADR-0024 section 4); otherwise a choice.
pub(super) fn follow(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    causes: Causes,
    ctx: &mut SimCtx<'_, '_>,
) {
    match domain.house.sims.get(&sim).and_then(|person| person.due) {
        // A need at 0 in this second comes first; lying, the departure waits on.
        Some(_) if super::zero::settle_first(domain, data, sim, ctx) => {}
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
    let (Place::Walking { from, to, .. }, action) = (person.place, person.action) else {
        panic!("invariant house.orphan_event");
    };
    // Out of the door the person is away; its needs stand still since it left (ADR-0024
    // section 4). In the room one stays in it stands idle, its rates as they were, and waits for
    // its planned choice; in its night it chooses now (ADR-0026, addendum of 2026-11-04 after
    // the playtest), as the bedtime it walked through made nobody choose.
    let Some(action) = action else {
        if to == house.def().entrance {
            person.place = Place::Away;
            return;
        }
        person.place = Place::Room(to);
        // The walk to the room one stays in ends: the arrival, caused by the idle that planned
        // it (ADR-0032 section 4). The room before the walk decides whether this is an entering.
        if let Some(by) = ctx.planned_by() {
            super::feeling::arrived(domain, data, sim, Some(from), by, ctx);
        }
        if night(houses.day(), born, now).is_some() {
            decide(domain, data, sim, Causes::new(ctx.trigger()), ctx);
        }
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
    // The change of rates first: it may give energy its extra (ADR-0026, addendum of 2026-11-06
    // on hunger at 0), which the end reckons with.
    plan_crossings(domain, data, sim, entry, ctx);
    // The arrival in a room pushes the others there, if the anger is high and the room is a
    // new one (ADR-0032 section 4, addendum of 2026-11-12).
    super::feeling::arrived(domain, data, sim, Some(from), entry, ctx);
    let end = match action.filler {
        // A time-filler ends at the next choice from here (ADR-0026, addendum of 2026-11-04 on
        // comfort and the time-filler).
        true => {
            let household = (domain.sims.get(&sim)).map(|person| person.household);
            let household = household.unwrap_or_else(|| panic!("invariant house.orphan_event"));
            filler_end(domain, data, (household, sim), (now, now))
        }
        false => {
            let person = (domain.house.sims.get(&sim))
                .unwrap_or_else(|| panic!("invariant house.orphan_event"));
            action_end(person, offer, houses.day(), born)
        }
    };
    if let Some(end) = end {
        let handle = ctx.schedule(end, EventPayload::ActionEnd { sim }, entry);
        domain.pending.insert((sim, Slot::End), handle);
    }
    super::talk::on_seated(domain, data, sim, entry, ctx);
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
    super::talk::on_stand_up(domain, data, sim, ctx);
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
            plan_crossings(domain, data, sim, entry, ctx);
        }
        Place::Away => panic!("invariant house.orphan_event"),
    }
    release(domain, household, (action.object, action.slot), entry, ctx);
    entry
}

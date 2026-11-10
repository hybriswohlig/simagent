//! Households in their houses (ADR-0024 sections 1 and 2): moving in and out, the first
//! morning at 6:00, and members who come and go. Every change of membership leaves a
//! `HouseMove` in the domain state; after each handler and input `settle` turns those of
//! households in houses into their entries, so the rules of phase 1 stay as they were.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use super::{DomainState, Slot, whole_years};
use crate::data::Data;
use crate::house::data::{NEEDS, Need, age_band};
use crate::house::need::NeedSample;
use crate::house::state::Action;
use crate::house::state::{Decisions, InHouse, MAX_HOUSEHOLDS, MovedIn, Place, crossings};
use crate::ids::{EventId, HouseholdId, SimId};
use crate::input::{InputRejected, MoveIn, MoveOut, RejectCode};
use crate::log::{Cause, Causes, EndReason, LogPayload, Subject};
use crate::time::{DAY, EventHandle, HOUR, SimDuration, SimTime, calendar, day_index};
use crate::world::{EventPayload, SimCtx};

/// The needs at the first morning and when someone comes after it (ADR-0026 section 1):
/// hunger, energy, bladder, hygiene, fun.
pub(crate) const MORNING_VALUES: [u16; 5] = [600, 900, 500, 700, 800];

/// The first morning comes at 6:00 (ADR-0024 section 1).
const MORNING: SimDuration = SimDuration::from_secs(6 * HOUR.as_secs());

/// A change of a person's household in this handler: from one, into one, or both, with
/// the entry that made it (ADR-0024 section 2).
#[derive(Clone, Copy, Debug)]
pub(crate) struct HouseMove {
    pub(crate) sim: SimId,
    pub(crate) from: Option<HouseholdId>,
    pub(crate) to: Option<HouseholdId>,
    pub(crate) cause: EventId,
}

/// The first second at or after `at` that is 6:00; `None` beyond the end of time.
pub(crate) fn next_morning(at: SimTime) -> Option<SimTime> {
    let since = i64::from(calendar(at).second_of_day);
    let wait = (MORNING.as_secs() - since).rem_euclid(DAY.as_secs());
    at.checked_add(SimDuration::from_secs(wait))
}

/// A started world with a second after the clock: that second, in which an input acts.
pub(super) fn running(
    domain: &DomainState,
    closed_through: SimTime,
) -> Result<SimTime, RejectCode> {
    let next = closed_through.checked_add(SimDuration::from_secs(1));
    match (domain.world_seed, next) {
        (None, _) => Err(RejectCode::NotStarted),
        (Some(_), None) => Err(RejectCode::EndOfTime),
        (Some(_), Some(next)) => Ok(next),
    }
}

/// The living members of a household, by SimId; none once it has ended.
fn members(domain: &DomainState, household: HouseholdId) -> Vec<SimId> {
    (domain.members.get(&household))
        .map_or_else(Vec::new, |members| members.iter().copied().collect())
}

fn rejected(code: RejectCode) -> InputRejected {
    InputRejected { code }
}

/// Pure check of `MoveIn` (ADR-0024 section 1).
pub(crate) fn check_move_in(
    domain: &DomainState,
    data: &Data,
    closed_through: SimTime,
    input: &MoveIn,
) -> Result<(), InputRejected> {
    running(domain, closed_through).map_err(rejected)?;
    let members = members(domain, input.household);
    if members.is_empty() {
        return Err(rejected(RejectCode::HouseholdUnknown));
    }
    if domain.house.households.contains_key(&input.household) {
        return Err(rejected(RejectCode::AlreadyInHouse));
    }
    if domain.house.households.len() >= MAX_HOUSEHOLDS {
        return Err(rejected(RejectCode::HouseLimit));
    }
    match data
        .houses()
        .and_then(|houses| houses.house_for(members.len()))
    {
        Some(_) => Ok(()),
        None => Err(rejected(RejectCode::NoHouse)),
    }
}

/// The household gets its house now and its first morning at the next 6:00; its entry
/// names all members.
pub(crate) fn apply_move_in(
    domain: &mut DomainState,
    data: &Data,
    input: &MoveIn,
    ctx: &mut SimCtx<'_, '_>,
) {
    let household = input.household;
    let members = members(domain, household);
    let house = (data
        .houses()
        .and_then(|houses| houses.house_for(members.len())))
    .unwrap_or_else(|| panic!("invariant house.checked"))
    .id()
    .clone();
    let subjects = members.iter().copied().map(Subject::Sim).collect();
    let entry = ctx.emit(
        LogPayload::MovedIn {
            household,
            house: house.clone(),
        },
        subjects,
        Causes::new(ctx.trigger()),
    );
    let moved = MovedIn {
        house,
        morning: false,
        since: ctx.now(),
        last_talk: BTreeMap::new(),
    };
    domain.house.households.insert(household, moved);
    // A morning beyond the end of time never comes.
    if let Some(at) = next_morning(ctx.now()) {
        let morning = ctx.schedule(at, EventPayload::HouseMorning { household }, entry);
        domain.house_mornings.insert(household, morning);
    }
    // Every member has its plan at 5:00 from its `core:moved_in` (ADR-0024 section 2 and its
    // addendum of 2026-11-04).
    for &sim in &members {
        super::day::plan_series(domain, data, sim, entry, ctx);
        super::memory::resume(domain, sim, entry, ctx);
        super::bond::resume(domain, sim, ctx.now());
    }
    super::bond::on_move_in(domain, data, &members, entry, ctx);
}

/// Pure check of `MoveOut`.
pub(crate) fn check_move_out(
    domain: &DomainState,
    closed_through: SimTime,
    input: &MoveOut,
) -> Result<(), InputRejected> {
    running(domain, closed_through).map_err(rejected)?;
    if members(domain, input.household).is_empty() {
        return Err(rejected(RejectCode::HouseholdUnknown));
    }
    if !domain.house.households.contains_key(&input.household) {
        return Err(rejected(RejectCode::NotInHouse));
    }
    Ok(())
}

/// The household leaves its house now, with all its members; what they were doing ends.
pub(crate) fn apply_move_out(
    domain: &mut DomainState,
    data: &Data,
    input: &MoveOut,
    ctx: &mut SimCtx<'_, '_>,
) {
    let household = input.household;
    let members = members(domain, household);
    let out = ctx.emit(
        LogPayload::MovedOut { household },
        members.iter().copied().map(Subject::Sim).collect(),
        Causes::new(ctx.trigger()),
    );
    for sim in members {
        let doing = forget(domain, data, sim, ctx);
        left(sim, doing, out, ctx);
    }
    end_house(domain, household, ctx);
}

/// The first morning of a household: each member of 6 or more starts the day.
pub(crate) fn on_morning(
    domain: &mut DomainState,
    data: &Data,
    household: HouseholdId,
    ctx: &mut SimCtx<'_, '_>,
) {
    // The event that runs right now is no longer pending.
    domain.house_mornings.remove(&household);
    let Some(moved) = domain.house.households.get_mut(&household) else {
        panic!("invariant house.orphan_event");
    };
    moved.morning = true;
    for sim in members(domain, household) {
        start_day(
            domain,
            data,
            sim,
            household,
            Causes::new(ctx.trigger()),
            ctx,
        );
    }
}

/// A member's needs from now, if it is 6 or older: its entry `core:day_started`, the
/// morning values with the awake rates of its age, in the entrance (ADR-0025 section 5); by
/// that entry its crossings, what is still to come of its duty today, its bedtime if tonight
/// is still to come (ADR-0026, addendum of 2026-11-04) and a choice now (ADR-0024 sections 1,
/// 2 and 4). Its series of plans at 5:00 came with its moving in.
pub(super) fn start_day(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    household: HouseholdId,
    causes: Causes,
    ctx: &mut SimCtx<'_, '_>,
) {
    let now = ctx.now();
    let Some(person) = domain.sims.get(&sim) else {
        panic!("invariant unknown_entity_id: sim {}", sim.get());
    };
    let age = whole_years(person.born, now);
    let Some(band) = u8::try_from(age).ok().and_then(age_band) else {
        return;
    };
    let house = (domain.house.households.get(&household))
        .and_then(|moved| data.houses()?.house(&moved.house))
        .unwrap_or_else(|| panic!("invariant house.unknown"));
    let entrance = house.def().entrance;
    let rates = (data.houses().map(|houses| &houses.rates().awake))
        .unwrap_or_else(|| panic!("invariant house.unknown"));
    let entry = ctx.emit(
        LogPayload::DayStarted {
            needs: MORNING_VALUES,
        },
        alloc::vec![Subject::Sim(sim)],
        causes,
    );
    let needs = core::array::from_fn(|at| {
        NeedSample::new(MORNING_VALUES[at], now, rates.rate(NEEDS[at], band))
    });
    let houses = data
        .houses()
        .unwrap_or_else(|| panic!("invariant house.unknown"));
    let person = InHouse {
        needs,
        place: Place::Room(entrance),
        action: None,
        due: None,
        decisions: Decisions {
            day: day_index(now),
            count: 0,
        },
        collapsed: false,
        reserve: super::reserve::begun(domain, data, houses, (sim, now)),

        talk_day: 0,
        talks: 0,
        entries: 0,
    };
    domain.house.sims.insert(sim, person);
    plan_crossings(domain, data, sim, entry, ctx);
    super::day::plan_today(domain, data, sim, Causes::new(Cause::Event(entry)), ctx);
    super::day::plan_bedtime(domain, data, sim, entry, ctx);
    let decide = ctx.schedule(now, EventPayload::Decide { sim }, entry);
    domain.pending.insert((sim, Slot::Decide), decide);
}

/// The person's crossings anew after a change of its rates in the handler that wrote `by`
/// (ADR-0026 section 1): those still planned are cancelled, then each threshold below a
/// falling value is planned. An input, a command, acts before the events of its second, so a
/// crossing in that second of a need it did not write down still comes after it and is planned
/// again (ADR-0024, addendum of 2026-11-04); a handler plans those after its second. A crossing
/// of the handler's own second that has not run yet stays planned while its need is still at or
/// below its level: the change wrote the value of that second down as it was, so the need has
/// reached the level all the same, and no other need's crossing of a second is lost to the
/// handler that runs first in it (ADR-0026, addendum of 2026-11-04 on a need at 0). Such a
/// change is the moment the reserve takes its rate and energy its extra, first, and the
/// reserve's crossings are planned anew, last (`state::reserve`, the addendum of 2026-11-06 on
/// hunger at 0).
pub(super) fn plan_crossings(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    by: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let now = ctx.now();
    super::reserve::write(domain, data, sim, now);
    let Some(person) = domain.house.sims.get(&sim) else {
        panic!("invariant house.orphan_event");
    };
    let first = (sim, Slot::Crossing(Need::Hunger, 0));
    let last = (sim, Slot::Crossing(Need::Fun, u16::MAX));
    let still_due = |slot: &Slot, handle: EventHandle| match slot {
        Slot::Crossing(need, level) if !ctx.in_input() => {
            let reached = person.needs[super::action::need_index(*need)].value_at(now) <= *level;
            reached && ctx.pending_time(handle) == Some(now)
        }
        _ => false,
    };
    let gone: Vec<(SimId, Slot)> = (domain.pending.range(first..=last))
        .filter(|&(&(_, slot), &handle)| !still_due(&slot, handle))
        .map(|(key, _)| *key)
        .collect();
    for key in gone {
        if let Some(handle) = domain.pending.remove(&key) {
            ctx.cancel(handle);
        }
    }
    let Some(person) = domain.house.sims.get(&sim) else {
        panic!("invariant house.orphan_event");
    };
    let planned: Vec<(Need, u16, SimTime)> = crossings(person, planned_in(ctx)).collect();
    for (need, level, at) in planned {
        let crossing = EventPayload::NeedCrossing { sim, need, level };
        let handle = ctx.schedule(at, crossing, by);
        domain
            .pending
            .insert((sim, Slot::Crossing(need, level)), handle);
    }
    super::reserve::plan(domain, sim, by, ctx);
}

/// The second a plan made now is made in, whose later seconds it plans: a handler's own; for an
/// input, which acts before the events of its second, the one before, the clock, from which a
/// loaded world plans the same (ADR-0024, addendum of 2026-11-04; ADR-0026, addendum of
/// 2026-11-04 on comfort and the time-filler).
pub(super) fn planned_in(ctx: &SimCtx<'_, '_>) -> SimTime {
    let now = ctx.now();
    if ctx.in_input() {
        now - SimDuration::from_secs(1)
    } else {
        now
    }
}

/// Cancels the person's pending events with a slot from `first` to `last`.
fn cancel(
    domain: &mut DomainState,
    sim: SimId,
    (first, last): (Slot, Slot),
    ctx: &mut SimCtx<'_, '_>,
) {
    let slots: Vec<(SimId, Slot)> = (domain.pending.range((sim, first)..=(sim, last)))
        .map(|(slot, _)| *slot)
        .collect();
    for (who, slot) in slots {
        let partner = match slot {
            Slot::Talk(other) => Some(other),
            _ => None,
        };
        if let Some(handle) = domain.pending.remove(&(who, slot)) {
            if let Some(other) = partner {
                domain.pending.remove(&(other, Slot::Talk(who)));
            }
            ctx.cancel(handle);
        }
    }
}

/// A person leaves the house part: its pending house events, its needs, its waiting
/// positions and its holds, its memories keep to the strongest (`memory::leave`) and its
/// beliefs and deeds freeze (`belief::leave`). Returns
/// what it was doing, with its needs now.
fn forget(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    ctx: &mut SimCtx<'_, '_>,
) -> Option<(Action, [u16; 5])> {
    let house_events = (Slot::DayPlan, Slot::Forget);
    cancel(domain, sim, house_events, ctx);
    let person = domain.house.sims.remove(&sim);
    for waiters in domain.house.waits.values_mut() {
        waiters.retain(|(_, waiter)| *waiter != sim);
    }
    domain.house.waits.retain(|_, waiters| !waiters.is_empty());
    domain.house.holds.retain(|_, holder| *holder != sim);
    let now = ctx.now();
    super::memory::leave(domain, data, sim, now);
    super::bond::leave(domain, data, sim, now);
    super::belief::leave(domain, sim);
    super::talk::drop_last_talk(domain, sim);
    person.and_then(|person| Some((person.action?, person.needs.map(|need| need.value_at(now)))))
}

/// What a person who left the house was doing ends, caused by its `core:moved_out` (ADR-0026
/// section 4, reason `left`).
fn left(sim: SimId, doing: Option<(Action, [u16; 5])>, out: EventId, ctx: &mut SimCtx<'_, '_>) {
    if let Some((action, needs)) = doing {
        let ended = LogPayload::ActionEnded {
            object: action.object,
            offer: action.offer,
            reason: EndReason::Left,
            needs,
        };
        ctx.emit(
            ended,
            alloc::vec![Subject::Sim(sim)],
            Causes::new(Cause::Event(out)),
        );
    }
}

/// A household's house ends: its entry, its morning if it has not come, its lists.
fn end_house(domain: &mut DomainState, household: HouseholdId, ctx: &mut SimCtx<'_, '_>) {
    domain.house.households.remove(&household);
    if let Some(morning) = domain.house_mornings.remove(&household) {
        ctx.cancel(morning);
    }
    domain.house.waits.retain(|(at, _), _| *at != household);
    domain.house.holds.retain(|(at, _, _), _| *at != household);
}

/// The moves of this handler, in their order (ADR-0024 section 2): one who leaves a
/// household in a house moves out of it, one who comes into one moves in with its plan at
/// 5:00, and from 6 after the first morning starts the day at once; a household that has
/// ended leaves its house without an entry. Then the idle persons of each household whose free
/// places changed get their next choice (`action::plan_idle`).
pub(crate) fn settle(domain: &mut DomainState, data: &Data, ctx: &mut SimCtx<'_, '_>) {
    for change in core::mem::take(&mut domain.house_moves) {
        let because = || Causes::new(Cause::Event(change.cause));
        if let Some(from) = change.from
            && domain.house.households.contains_key(&from)
        {
            let held = super::action::held_by(domain, from, change.sim);
            let doing = forget(domain, data, change.sim, ctx);
            let out = ctx.emit(
                LogPayload::MovedOut { household: from },
                alloc::vec![Subject::Sim(change.sim)],
                because(),
            );
            let used = doing.map(|(action, _)| (action.object, action.slot));
            left(change.sim, doing, out, ctx);
            // The places it used or held go to the next who waits (ADR-0026 section 5).
            for place in used.into_iter().chain(held) {
                super::action::release(domain, from, place, out, ctx);
            }
        }
        if let Some(to) = change.to
            && let Some(moved) = domain.house.households.get(&to)
        {
            let (house, morning) = (moved.house.clone(), moved.morning);
            let entry = ctx.emit(
                LogPayload::MovedIn {
                    household: to,
                    house,
                },
                alloc::vec![Subject::Sim(change.sim)],
                because(),
            );
            super::day::plan_series(domain, data, change.sim, entry, ctx);
            super::memory::resume(domain, change.sim, entry, ctx);
            super::bond::resume(domain, change.sim, ctx.now());
            super::bond::on_join(domain, data, to, change.sim, entry, ctx);
            if morning {
                let causes = Causes::new(Cause::Event(entry));
                start_day(domain, data, change.sim, to, causes, ctx);
            }
        }
    }
    let ended: Vec<HouseholdId> = (domain.house.households.keys())
        .filter(|household| !domain.members.contains_key(household))
        .copied()
        .collect();
    for household in ended {
        end_house(domain, household, ctx);
    }
    super::action::plan_idle(domain, data, ctx);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The next 6:00: the same second at 6:00, the next day just after it.
    #[test]
    fn the_next_morning_is_the_first_six_at_or_after() {
        let t = |secs: i64| SimTime::from_secs(secs);
        let day = DAY.as_secs();
        assert_eq!(next_morning(t(0)), Some(t(21_600)));
        assert_eq!(next_morning(t(21_600)), Some(t(21_600)));
        assert_eq!(next_morning(t(21_601)), Some(t(day + 21_600)));
        assert_eq!(next_morning(t(day - 1)), Some(t(day + 21_600)));
        assert_eq!(next_morning(t(-1)), Some(t(21_600)));
    }
}

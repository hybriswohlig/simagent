//! The day of a person in a house (ADR-0024 section 4): its plan at 5:00 every day, on a
//! weekday its duty by age and work plan, the wake that ends sleep, the departure that ends
//! what it does or waits for an action for a critical need, away with every need still, and
//! the return at the entrance with the jumps of the data and a choice; every day its bedtime
//! at the start of its night (ADR-0026, addendum of 2026-11-04).

use alloc::vec;

use super::action::{self, home, person_mut};
use super::house::{plan_crossings, start_day};
use super::{DomainState, Slot, whole_years};
use crate::data::Data;
use crate::house::data::{AGE_BANDS, DayRules, HouseData, NEEDS};
use crate::house::day::{DayDuty, duty, next_plan, night_of, plan_day, work_draw};
use crate::house::need::{EMPTY, FULL, NeedSample};
use crate::house::state::{Due, Duty, Place};
use crate::ids::{EventId, SimId};
use crate::log::{Cause, Causes, EndReason, LogPayload, Subject};
use crate::time::{DAY, SimDuration, SimTime};
use crate::world::{EventPayload, SimCtx};

fn rules(data: &Data) -> &DayRules {
    (data.houses().map(HouseData::day)).unwrap_or_else(|| panic!("invariant house.unknown"))
}

/// Whether a person is under the age of needs at `now` (ADR-0024 section 2).
fn young(domain: &DomainState, sim: SimId, now: SimTime) -> bool {
    let Some(person) = domain.sims.get(&sim) else {
        panic!("invariant unknown_entity_id: sim {}", sim.get());
    };
    whole_years(person.born, now) < u32::from(AGE_BANDS[0].0)
}

/// A person's duty on the day `day`, its work plan drawn only at working age.
fn duty_of(domain: &DomainState, data: &Data, sim: SimId, day: i64) -> Option<DayDuty> {
    let Some(person) = domain.sims.get(&sim) else {
        panic!("invariant unknown_entity_id: sim {}", sim.get());
    };
    let seed = (domain.world_seed).unwrap_or_else(|| panic!("invariant seeds.before_start"));
    let key = person.lineage_key;
    duty(rules(data), day, person.born, || work_draw(seed, key))
}

/// The person's series of plans from the next 5:00 on, set up by `by`, its `core:moved_in`
/// (ADR-0024 section 2 and its addendum of 2026-11-04): leaving the house cancels it, so a
/// person in a house has exactly one.
pub(crate) fn plan_series(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    by: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    // A plan beyond the end of time never comes.
    if let Some(at) = next_plan(rules(data), ctx.now()) {
        let handle = ctx.schedule_periodic(at, DAY, EventPayload::DayPlan { sim }, by);
        if domain
            .pending
            .insert((sim, Slot::DayPlan), handle)
            .is_some()
        {
            panic!("invariant house.day_plan_twice");
        }
    }
}

/// What is still to come of a person's duty of the day (ADR-0024 section 4): its entry
/// `core:day_planned`, and with it as `by` the wake if it comes after now, the departure and
/// the return. Nothing without a duty or once its departure has come.
pub(crate) fn plan_today(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    causes: Causes,
    ctx: &mut SimCtx<'_, '_>,
) {
    let now = ctx.now();
    let day = plan_day(rules(data), now);
    let Some(duty) = duty_of(domain, data, sim, day).filter(|duty| duty.depart > now) else {
        return;
    };
    let planned = LogPayload::DayPlanned {
        place: duty.place,
        depart: duty.depart,
        back: duty.back,
    };
    let entry = ctx.emit(planned, vec![Subject::Sim(sim)], causes);
    if duty.wake > now {
        let handle = ctx.schedule(duty.wake, EventPayload::Wake { sim }, entry);
        domain.pending.insert((sim, Slot::Wake), handle);
    }
    let depart = EventPayload::Depart {
        sim,
        place: duty.place,
        back: duty.back,
    };
    let handle = ctx.schedule(duty.depart, depart, entry);
    domain.pending.insert((sim, Slot::Depart), handle);
    let back = EventPayload::Return {
        sim,
        place: duty.place,
    };
    let handle = ctx.schedule(duty.back, back, entry);
    domain.pending.insert((sim, Slot::Return), handle);
}

/// The bedtime of a person with needs (ADR-0026, addendum of 2026-11-04): at the start of the
/// night of the day whose plan came last, if that has not begun before now, planned by `by`.
/// One planned for now runs after the choice its day begins with, which is in the night then.
pub(super) fn plan_bedtime(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    by: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let now = ctx.now();
    let rules = rules(data);
    let tonight = night_of(rules, plan_day(rules, now), action::born(domain, sim));
    let Some(night) = tonight.filter(|night| night.from >= now) else {
        return;
    };
    let handle = ctx.schedule(night.from, EventPayload::Bedtime { sim }, by);
    if domain
        .pending
        .insert((sim, Slot::Bedtime), handle)
        .is_some()
    {
        panic!("invariant house.bedtime_twice");
    }
}

/// The start of a person's night (ADR-0026, addendum of 2026-11-04): one idle in a room
/// chooses, in its night now; anyone else goes on, and chooses in the night when what it does
/// ends or it gets up from the floor. No entry of its own: the choice's has this event as its
/// cause.
pub(crate) fn on_bedtime(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    ctx: &mut SimCtx<'_, '_>,
) {
    // The event that runs right now is no longer pending.
    domain.pending.remove(&(sim, Slot::Bedtime));
    let Some(person) = domain.house.sims.get(&sim) else {
        panic!("invariant house.orphan_event");
    };
    if let (Place::Room(_), None, false) = (person.place, person.action, person.collapsed) {
        action::decide(domain, data, sim, Causes::new(ctx.trigger()), ctx);
    }
}

/// The plan of the day at 5:00 (ADR-0024 sections 2 and 4): nothing before the household's
/// first morning; a person with needs plans its duty and its bedtime, the latter by the
/// series' own reason, its `core:moved_in` (ADR-0026, addendum of 2026-11-04); a child who is 6
/// now gets its needs and its day, its series running on.
pub(crate) fn on_day_plan(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    ctx: &mut SimCtx<'_, '_>,
) {
    // A series: its next plan is already pending.
    let Some(household) = domain.sims.get(&sim).map(|person| person.household) else {
        panic!("invariant house.orphan_event");
    };
    let Some(moved) = domain.house.households.get(&household) else {
        panic!("invariant house.orphan_event");
    };
    if !moved.morning {
        return;
    }
    let causes = Causes::new(ctx.trigger());
    if domain.house.sims.contains_key(&sim) {
        plan_today(domain, data, sim, causes, ctx);
        let by = (ctx.planned_by()).unwrap_or_else(|| panic!("invariant house.day_plan_planned"));
        plan_bedtime(domain, data, sim, by, ctx);
    } else if !young(domain, sim, ctx.now()) {
        start_day(domain, data, sim, household, causes, ctx);
    }
}

/// The wake before a departure (ADR-0024 section 4, ADR-0026 section 3): it ends sleep with
/// reason `obligation`, and the person chooses; one idle in a room chooses too; anyone else
/// goes on, one lying on the floor too (ADR-0026, addendum of 2026-11-04 on a need at 0).
pub(crate) fn on_wake(domain: &mut DomainState, data: &Data, sim: SimId, ctx: &mut SimCtx<'_, '_>) {
    // The event that runs right now is no longer pending.
    domain.pending.remove(&(sim, Slot::Wake));
    let Some(person) = domain.house.sims.get(&sim) else {
        panic!("invariant house.orphan_event");
    };
    let causes = Causes::new(ctx.trigger());
    match (person.place, person.action) {
        _ if person.collapsed => {}
        (Place::Room(_), None) => action::decide(domain, data, sim, causes, ctx),
        (Place::Room(_), Some(_)) if action::asleep(domain, data, sim) => {
            let ended = action::interrupt(domain, data, sim, EndReason::Obligation, causes, ctx);
            action::next(domain, data, sim, ended, ctx);
        }
        _ => {}
    }
}

/// The departure (ADR-0024 section 4): a walk or an action that raises a critical need makes
/// it wait, the person due at `back`, and so does lying on the floor (ADR-0026, addendum of
/// 2026-11-04 on a need at 0) until the person gets up; otherwise the person leaves now. A need
/// whose crossing at 0 falls into this second is settled first: the departure runs before it.
pub(crate) fn on_depart(
    domain: &mut DomainState,
    data: &Data,
    (sim, place, back): (SimId, Duty, SimTime),
    ctx: &mut SimCtx<'_, '_>,
) {
    // The event that runs right now is no longer pending.
    domain.pending.remove(&(sim, Slot::Depart));
    let now = ctx.now();
    let lying = super::zero::settle_first(domain, data, sim, ctx);
    if lying || action::for_critical(domain, data, sim, now) {
        let depart = now;
        person_mut(domain, sim).due = Some(Due {
            place,
            depart,
            back,
        });
        return;
    }
    leave(
        domain,
        data,
        sim,
        (place, 0),
        Causes::new(ctx.trigger()),
        ctx,
    );
}

/// A person leaves the house for `place`, `late_secs` after its plan (ADR-0024 section 4,
/// ADR-0025 section 5): what it does ends with reason `obligation` and no choice follows, a walk
/// to the room one stays in ends where it began (ADR-0026, addendum of 2026-11-04 after the
/// playtest); it
/// leaves every waiting list, passes on a held place and has no choice planned; its entry
/// `core:left_home`; from now every need stands still and it walks out through the entrance.
pub(crate) fn leave(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    (place, late_secs): (Duty, u32),
    causes: Causes,
    ctx: &mut SimCtx<'_, '_>,
) {
    let mut causes = causes;
    if (domain.house.sims.get(&sim)).is_some_and(|person| person.action.is_some()) {
        let ended = action::interrupt(domain, data, sim, EndReason::Obligation, causes, ctx);
        causes = Causes::new(Cause::Event(ended));
    }
    // On its walk to the room one stays in, it leaves from the room it came from.
    action::halt(domain, data, sim, ctx);
    let (household, _, house) = home(domain, data, sim);
    for waiters in domain.house.waits.values_mut() {
        waiters.retain(|&(_, waiter)| waiter != sim);
    }
    domain.house.waits.retain(|_, waiters| !waiters.is_empty());
    let held = action::held_by(domain, household, sim);
    if let Some((object, slot)) = held {
        domain.house.holds.remove(&(household, object, slot));
    }
    if let Some(handle) = domain.pending.remove(&(sim, Slot::Decide)) {
        ctx.cancel(handle);
    }
    let left = LogPayload::LeftHome { place, late_secs };
    let entry = ctx.emit(left, vec![Subject::Sim(sim)], causes);
    let now = ctx.now();
    let person = person_mut(domain, sim);
    let Place::Room(room) = person.place else {
        panic!("invariant house.orphan_event");
    };
    person.due = None;
    person.needs = person.needs.map(|need| need.with_rate(now, 0));
    let walk = SimDuration::from_secs(i64::from(house.walk_out(room)));
    match now.checked_add(walk) {
        Some(arrive) => {
            person.place = Place::Walking {
                from: room,
                to: house.def().entrance,
                depart: now,
                arrive,
            };
            let handle = ctx.schedule(arrive, EventPayload::Arrive { sim }, entry);
            domain.pending.insert((sim, Slot::Arrive), handle);
        }
        // A walk that would end beyond the end of time is over at once.
        None => person.place = Place::Away,
    }
    // Still needs cross nothing: this only cancels the crossings planned.
    plan_crossings(domain, sim, entry, ctx);
    if let Some(place) = held {
        action::release(domain, household, place, entry, ctx);
    }
}

/// The return (ADR-0024 section 4): a person whose departure still waits has missed its duty
/// and goes on; one away or on its way out comes in at the entrance, each need jumps by the
/// data, clamped to 0 to 1000, the awake rates hold again, and it chooses; a need the jump puts
/// at 0 has its consequence first (ADR-0026, addendum of 2026-11-04 on a need at 0), and one
/// lying on the floor then does not choose.
pub(crate) fn on_return(
    domain: &mut DomainState,
    data: &Data,
    (sim, place): (SimId, Duty),
    ctx: &mut SimCtx<'_, '_>,
) {
    // The event that runs right now is no longer pending.
    domain.pending.remove(&(sim, Slot::Return));
    let now = ctx.now();
    let (_, houses, house) = home(domain, data, sim);
    let jumps = &houses.day().return_jumps;
    let jumps = [
        jumps.hunger,
        jumps.energy,
        jumps.bladder,
        jumps.hygiene,
        jumps.fun,
    ];
    let band = action::band(domain, sim, now);
    let person = person_mut(domain, sim);
    let subject = vec![Subject::Sim(sim)];
    if person.due.take().is_some() {
        let missed = LogPayload::Missed { place };
        ctx.emit(missed, subject, Causes::new(ctx.trigger()));
        return;
    }
    if !matches!(
        (person.place, person.action),
        (Place::Away | Place::Walking { .. }, None)
    ) {
        panic!("invariant house.orphan_event");
    }
    let came = LogPayload::CameHome { place };
    let entry = ctx.emit(came, subject, Causes::new(ctx.trigger()));
    person.place = Place::Room(house.def().entrance);
    let mut emptied = [false; 5];
    let samples = person.needs.iter_mut().zip(NEEDS).zip(jumps);
    for (((sample, need), jump), emptied) in samples.zip(&mut emptied) {
        let before = sample.value_at(now);
        let value = (i32::from(before) + jump).clamp(0, i32::from(FULL));
        let value = u16::try_from(value).unwrap_or_else(|_| panic!("invariant house.need_value"));
        *emptied = before > EMPTY && value == EMPTY;
        *sample = NeedSample::new(value, now, houses.rates().awake.rate(need, band));
    }
    if let Some(handle) = domain.pending.remove(&(sim, Slot::Arrive)) {
        ctx.cancel(handle);
    }
    plan_crossings(domain, sim, entry, ctx);
    let causes = Causes::new(Cause::Event(entry));
    if emptied.contains(&true) {
        super::zero::settle(domain, data, sim, (emptied, true), causes, ctx);
    } else {
        action::decide(domain, data, sim, causes, ctx);
    }
}

//! A need at 0 (ADR-0026, addendum of 2026-11-04 on a need at 0): one function settles every
//! need of a person that reaches 0 in a second, in the order of the needs, from its crossing at
//! 0 or from a jump on coming home. Hunger has no consequence yet. Energy lays the person on the
//! floor of its room until it gets up; the bladder empties itself and leaves hygiene low;
//! hygiene reaching 0 lowers fun once. Fun at 0 has no state: a command that does not raise fun
//! is refused while it lasts (`state::command`).

use alloc::vec;

use super::action::{self, home, person_mut};
use super::house::plan_crossings;
use super::{DomainState, Slot};
use crate::data::Data;
use crate::house::data::{House, HouseData, NEEDS, Need, OfferDef};
use crate::house::need::{EMPTY, FULL, NeedSample};
use crate::house::state::{Action, InHouse, Place, lying_rate, stand_up};
use crate::ids::{EventId, SimId};
use crate::log::{Cause, Causes, EndReason, LogPayload, Subject};
use crate::world::{EventPayload, SimCtx};

/// The needs of `sim` at 0 now whose crossing at 0 in this second has not run yet.
pub(super) fn empty_now(domain: &DomainState, sim: SimId, ctx: &SimCtx<'_, '_>) -> [bool; 5] {
    let now = ctx.now();
    let Some(person) = domain.house.sims.get(&sim) else {
        panic!("invariant house.orphan_event");
    };
    let mut empty = [false; 5];
    for ((at, need), sample) in NEEDS.into_iter().enumerate().zip(person.needs) {
        let planned = domain.pending.get(&(sim, Slot::Crossing(need, EMPTY)));
        let due = planned.is_some_and(|&handle| ctx.pending_time(handle) == Some(now));
        empty[at] = due && sample.value_at(now) == EMPTY;
    }
    empty
}

/// The needs whose crossing at 0 falls into this second are settled before what would leave
/// the house in it (a departure runs before the crossings of its second), and nothing is chosen
/// (ADR-0026, addendum of 2026-11-04 on a need at 0). Returns whether the person lies on the
/// floor then, so that the departure waits.
pub(super) fn settle_first(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    ctx: &mut SimCtx<'_, '_>,
) -> bool {
    let empty = empty_now(domain, sim, ctx);
    if empty.contains(&true) {
        settle(
            domain,
            data,
            sim,
            (empty, false),
            Causes::new(ctx.trigger()),
            ctx,
        );
    }
    (domain.house.sims.get(&sim)).is_some_and(|person| person.collapsed)
}

/// The needs of `sim` marked in `reached` reach 0 now, each with its entry `core:need_empty` by
/// `causes` and then its consequence, in the order of the needs; fun reaching 0 by the change
/// that hygiene at 0 brings is settled as well. The crossing at 0 still planned for this second
/// of a need settled here is cancelled, so no need is settled twice. Then the crossings are
/// planned anew if a need was written down, and with `choose` one idle in a room and not lying
/// chooses, or leaves if its departure waited, by the last entry.
pub(super) fn settle(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    (mut reached, choose): ([bool; 5], bool),
    causes: Causes,
    ctx: &mut SimCtx<'_, '_>,
) {
    let mut last = None;
    // A need written down after the crossings were planned last.
    let mut written = false;
    for (at, need) in NEEDS.into_iter().enumerate() {
        if !reached[at] {
            continue;
        }
        if let Some(handle) = domain.pending.remove(&(sim, Slot::Crossing(need, EMPTY))) {
            ctx.cancel(handle);
        }
        let empty = LogPayload::NeedEmpty { need };
        let entry = ctx.emit(empty, vec![Subject::Sim(sim)], causes.clone());
        last = Some(entry);
        match need {
            // Hunger at 0 starts the reserve's drain: a change of rates (ADR-0026, addendum of
            // 2026-11-06 on hunger at 0).
            Need::Hunger => written = true,
            Need::Fun => {}
            Need::Energy => {
                last = Some(collapse(domain, data, sim, entry, ctx));
                written = true;
            }
            Need::Bladder => {
                let (after, ended) = accident(domain, data, sim, entry, ctx);
                last = Some(after);
                // An action that ended planned the crossings with what the accident wrote.
                written = written || !ended;
            }
            Need::Hygiene => {
                if let Some((before, after)) = sting(domain, data, sim, ctx) {
                    written = true;
                    reached[action::need_index(Need::Fun)] |= before > EMPTY && after == EMPTY;
                }
            }
        }
    }
    let Some(last) = last else {
        return;
    };
    if written {
        plan_crossings(domain, data, sim, last, ctx);
    }
    // A walk to the room one stays in ends where it began, as at 150 and 50 (ADR-0026, addendum
    // of 2026-11-04 after the playtest).
    if choose {
        action::halt(domain, data, sim, ctx);
    }
    let Some(person) = domain.house.sims.get(&sim) else {
        panic!("invariant house.orphan_event");
    };
    if let (Place::Room(_), None, false) = (person.place, person.action, person.collapsed)
        && choose
    {
        action::next(domain, data, sim, last, ctx);
    }
}

/// The offer of a person at the object of its action, none on the way or without one.
fn doing<'a>(person: &InHouse, house: &House, houses: &'a HouseData) -> Option<&'a OfferDef> {
    let (Place::Room(_), Some(Action { object, offer, .. })) = (person.place, person.action) else {
        return None;
    };
    (house.def().objects.get(usize::from(object)))
        .and_then(|object| houses.kind(&object.kind))
        .and_then(|kind| kind.offers.get(usize::from(offer)))
}

/// The rate a consequence writes a need down with: lying on the floor that of `lying_rate`,
/// else the awake one, by the age band of now. It writes no need that an action going on at its
/// object sets: that action's end is reckoned from the second those were written down.
fn rate_now(person: &InHouse, houses: &HouseData, need: Need, band: usize) -> i32 {
    if person.collapsed {
        lying_rate(houses.rates(), houses.day(), need, band)
    } else {
        houses.rates().awake.rate(need, band)
    }
}

/// Energy at 0, with `empty` its entry: what the person does ends with reason `collapse`, on a
/// walk back in the room it came from; it stops waiting and passes on a place held for it; its
/// entry `core:collapsed`; it lies on the floor of that room, every need from now at the rate of
/// `lying_rate`, its getting up planned by that entry (none beyond the end of time). The
/// mattress stays free. Returns the entry.
fn collapse(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    empty: EventId,
    ctx: &mut SimCtx<'_, '_>,
) -> EventId {
    let now = ctx.now();
    let mut causes = Causes::new(Cause::Event(empty));
    if (domain.house.sims.get(&sim)).is_some_and(|person| person.action.is_some()) {
        let ended = action::interrupt(domain, data, sim, EndReason::Collapse, causes, ctx);
        causes = Causes::new(Cause::Event(ended));
    }
    // On its walk to the room one stays in, it falls in the room it came from.
    action::halt(domain, data, sim, ctx);
    let (household, houses, _) = home(domain, data, sim);
    let held = action::stop_waiting(domain, household, sim, ctx);
    let band = action::band(domain, sim, now);
    let born = action::born(domain, sim);
    let person = person_mut(domain, sim);
    let Place::Room(room) = person.place else {
        panic!("invariant house.orphan_event");
    };
    let collapsed = LogPayload::Collapsed { room };
    let entry = ctx.emit(collapsed, vec![Subject::Sim(sim)], causes);
    person.collapsed = true;
    for (sample, need) in person.needs.iter_mut().zip(NEEDS) {
        let rate = lying_rate(houses.rates(), houses.day(), need, band);
        *sample = sample.with_rate(now, rate);
    }
    if let Some(up) = stand_up(person, houses.day(), born) {
        let handle = ctx.schedule(up, EventPayload::StandUp { sim }, entry);
        domain.pending.insert((sim, Slot::StandUp), handle);
    }
    if let Some(place) = held {
        action::release(domain, household, place, entry, ctx);
    }
    entry
}

/// The bladder at 0, with `empty` its entry: the bladder is full at once and hygiene at most the
/// data's value, its entry `core:accident` with the needs after; then, for one not lying whose
/// walk or action does not raise hygiene, what it does ends with reason `critical`, as at 150.
/// Returns the last entry and whether an action ended.
fn accident(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    empty: EventId,
    ctx: &mut SimCtx<'_, '_>,
) -> (EventId, bool) {
    let now = ctx.now();
    let (_, houses, house) = home(domain, data, sim);
    let band = action::band(domain, sim, now);
    let goes_on = action::raises(domain, data, sim, Need::Hygiene);
    let Some(person) = domain.house.sims.get(&sim) else {
        panic!("invariant house.orphan_event");
    };
    let going = (doing(person, house, houses)).filter(|_| goes_on && !person.collapsed);
    let (bladder, hygiene) = (
        action::need_index(Need::Bladder),
        action::need_index(Need::Hygiene),
    );
    let mut needs = person.needs.map(|need| need.value_at(now));
    let written = |need: Need| going.is_none_or(|offer| !offer.sets(need));
    if written(Need::Bladder) {
        needs[bladder] = FULL;
    }
    if written(Need::Hygiene) {
        needs[hygiene] = needs[hygiene].min(houses.day().zero.accident_hygiene_at_most);
    }
    let rates = [Need::Bladder, Need::Hygiene].map(|need| rate_now(person, houses, need, band));
    let ends = person.action.is_some() && !person.collapsed && !goes_on;
    let payload = LogPayload::Accident { needs };
    let entry = ctx.emit(
        payload,
        vec![Subject::Sim(sim)],
        Causes::new(Cause::Event(empty)),
    );
    let person = person_mut(domain, sim);
    for ((need, at), rate) in [(Need::Bladder, bladder), (Need::Hygiene, hygiene)]
        .into_iter()
        .zip(rates)
    {
        if written(need) {
            person.needs[at] = NeedSample::new(needs[at], now, rate);
        }
    }
    if !ends {
        return (entry, false);
    }
    let causes = Causes::new(Cause::Event(entry));
    let ended = action::interrupt(domain, data, sim, EndReason::Critical, causes, ctx);
    (ended, true)
}

/// Hygiene at 0: fun changes once by the data's value, held at 0, and both are written down in
/// this second, hygiene at its 0, so that a save shows the sting; nothing where an action going
/// on at its object sets either. Returns fun before and after, `None` if nothing was written.
fn sting(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    ctx: &mut SimCtx<'_, '_>,
) -> Option<(u16, u16)> {
    let now = ctx.now();
    let (_, houses, house) = home(domain, data, sim);
    let band = action::band(domain, sim, now);
    let fun = action::need_index(Need::Fun);
    let person = person_mut(domain, sim);
    let sets = |offer: &OfferDef| offer.sets(Need::Fun) || offer.sets(Need::Hygiene);
    if (doing(person, house, houses)).is_some_and(sets) {
        return None;
    }
    let before = person.needs[fun].value_at(now);
    let changed = i32::from(before) + houses.day().zero.hygiene_empty_fun;
    let after = u16::try_from(changed.clamp(0, i32::from(FULL)))
        .unwrap_or_else(|_| panic!("invariant house.need_value"));
    let hygiene = action::need_index(Need::Hygiene);
    let rates = [Need::Hygiene, Need::Fun].map(|need| rate_now(person, houses, need, band));
    person.needs[hygiene] = person.needs[hygiene].with_rate(now, rates[0]);
    person.needs[fun] = NeedSample::new(after, now, rates[1]);
    Some((before, after))
}

/// One lying on the floor gets up now: its getting up, if still to come, is cancelled; every
/// need takes the awake rate of its age band again, the crossings are planned anew by `by`; a
/// departure that waited leaves now, late against its plan, otherwise it chooses, by `causes`
/// (in its night the night's rule takes a bed).
pub(super) fn rise(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    by: EventId,
    causes: Causes,
    ctx: &mut SimCtx<'_, '_>,
) {
    if let Some(handle) = domain.pending.remove(&(sim, Slot::StandUp)) {
        ctx.cancel(handle);
    }
    let now = ctx.now();
    let (_, houses, _) = home(domain, data, sim);
    let band = action::band(domain, sim, now);
    let person = person_mut(domain, sim);
    if !person.collapsed {
        panic!("invariant house.orphan_event");
    }
    person.collapsed = false;
    for (sample, need) in person.needs.iter_mut().zip(NEEDS) {
        *sample = sample.with_rate(now, houses.rates().awake.rate(need, band));
    }
    plan_crossings(domain, data, sim, by, ctx);
    action::follow(domain, data, sim, causes, ctx);
}

/// The second one lying on the floor gets up (`house::state::stand_up`). No entry of its own:
/// the crossings planned anew have the entry of the collapse as their reason, which planned this
/// event, and what follows has this event as its cause.
pub(crate) fn on_stand_up(
    domain: &mut DomainState,
    data: &Data,
    sim: SimId,
    ctx: &mut SimCtx<'_, '_>,
) {
    // The event that runs right now is no longer pending.
    domain.pending.remove(&(sim, Slot::StandUp));
    let by = (ctx.planned_by()).unwrap_or_else(|| panic!("invariant house.stand_up_planned"));
    rise(domain, data, sim, by, Causes::new(ctx.trigger()), ctx);
}

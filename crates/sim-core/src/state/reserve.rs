//! The reserve at hunger 0 in the house (ADR-0026, addendum of 2026-11-06 on hunger at 0): its
//! size when the needs begin, its rate and the energy's extra at every change of a person's
//! rates (`state::house::plan_crossings`), its crossings of half and of 0 with their entries, and
//! the band a walk and a command read.

use alloc::vec;
use alloc::vec::Vec;

use super::action::{band, home};
use super::house::planned_in;
use super::{DomainState, Slot};
use crate::data::Data;
use crate::house::data::{House, HouseData, Need};
use crate::house::need::EMPTY;
use crate::house::reserve::{Band, Doing, Reserve};
use crate::house::state::{InHouse, Place, dwell_walk, lying_rate};
use crate::ids::{EventId, SimId};
use crate::log::{Causes, LogPayload, Subject};
use crate::time::{EventHandle, SimTime};
use crate::world::{EventPayload, SimCtx};

/// The full reserve of `sim` whose needs begin now: its capacity by the `body_fat_z` of its body
/// in this second (`body_contract::reserve_size`), read once.
pub(super) fn begun(
    domain: &DomainState,
    data: &Data,
    houses: &HouseData,
    (sim, now): (SimId, SimTime),
) -> Reserve {
    let size = crate::body_contract::reserve_size_of(domain, data, (sim, now));
    Reserve::full(houses.reserve().capacity_of(size), now)
}

/// Whether a person is away or on its way out of the house.
fn out(person: &InHouse, house: &House) -> bool {
    let walking_out = matches!(person.place, Place::Walking { .. }) && person.action.is_none();
    person.place == Place::Away || (walking_out && !dwell_walk(person, house))
}

/// The offer of a person's action at its object; none on the way or without one.
fn at_object<'a>(
    person: &InHouse,
    (house, houses): (&House, &'a HouseData),
) -> Option<&'a crate::house::data::OfferDef> {
    let (Place::Room(_), Some(action)) = (person.place, person.action) else {
        return None;
    };
    (house.def().objects.get(usize::from(action.object)))
        .and_then(|object| houses.kind(&object.kind))
        .and_then(|kind| kind.offers.get(usize::from(action.offer)))
}

/// What the person does now, as far as the reserve's rate goes (`house::reserve::Doing`).
fn doing(person: &InHouse, (house, houses): (&House, &HouseData), now: SimTime) -> Doing {
    let hunger = person.needs[0];
    let sleeps = at_object(person, (house, houses)).is_some_and(|offer| offer.sleep);
    Doing {
        out: out(person, house),
        eating: hunger.rate() > 0,
        asleep: person.collapsed || sleeps,
        empty: hunger.value_at(now) == EMPTY,
    }
}

/// The rate the energy of `person` has without an extra, by the age band `band`: lying on the
/// floor the collapse's, away and on the way out 0, at an object whose offer sets energy the
/// offer's, else the awake one.
fn base_energy(person: &InHouse, (house, houses): (&House, &HouseData), band: usize) -> i32 {
    if person.collapsed {
        return lying_rate(houses.rates(), houses.day(), Need::Energy, band);
    }
    if out(person, house) {
        return 0;
    }
    match at_object(person, (house, houses)) {
        Some(offer) if offer.sets(Need::Energy) => offer.rate(houses.rates(), Need::Energy, band),
        _ => houses.rates().awake.rate(Need::Energy, band),
    }
}

/// A change of the person's rates in this second (ADR-0026, addendum of 2026-11-06 on hunger at
/// 0): the reserve takes the rate of what the person does now if that differs, written down at
/// `now` first; then the energy takes the extra of the band now. A handler that wrote energy in
/// this second wrote its rate without one: it gets its base and the extra. Otherwise energy is
/// written down anew only if the extra changes, not while the person lies on the floor or is out
/// (the extra is 0 there), and not while an action at its object sets energy, which keeps the
/// rate it began with.
pub(super) fn write(domain: &mut DomainState, data: &Data, sim: SimId, now: SimTime) {
    let (_, houses, house) = home(domain, data, sim);
    let band = band(domain, sim, now);
    let Some(person) = domain.house.sims.get_mut(&sim) else {
        panic!("invariant house.orphan_event");
    };
    let rules = houses.reserve();
    let reserve = person.reserve;
    let rate = rules.rate(
        doing(person, (house, houses), now),
        band,
        (reserve.value_at(now), reserve.capacity()),
    );
    if rate != reserve.rate() {
        person.reserve = reserve.with_rate(now, rate);
    }
    let held = (at_object(person, (house, houses))).is_some_and(|offer| offer.sets(Need::Energy));
    let without = person.collapsed || out(person, house);
    let extra = match without {
        true => 0,
        false => rules.energy(person.reserve.band(person.needs[0], now)),
    };
    let energy = person.needs[1];
    let base = base_energy(person, (house, houses), band);
    let changes = extra != person.reserve.energy() && !held && !without;
    if energy.at() == now || changes {
        person.needs[1] = energy.with_rate(now, base + extra);
        person.reserve = person.reserve.with_energy(extra);
    }
}

/// The crossings of the reserve anew after a change of the person's rates in the handler that
/// wrote `by`: those still planned are cancelled, but one of this second that has not run yet
/// while the reserve falls and is at or below its level (as for a need, `plan_crossings`); then,
/// while the reserve falls, half and 0 below its value, each after the second the plan is made in.
pub(super) fn plan(domain: &mut DomainState, sim: SimId, by: EventId, ctx: &mut SimCtx<'_, '_>) {
    let now = ctx.now();
    let Some(person) = domain.house.sims.get(&sim) else {
        panic!("invariant house.orphan_event");
    };
    let reserve = person.reserve;
    let still_due = |level: u16, handle: EventHandle| {
        let reached = reserve.rate() < 0 && reserve.value_at(now) <= level;
        !ctx.in_input() && reached && ctx.pending_time(handle) == Some(now)
    };
    let first = (sim, Slot::Reserve(0));
    let last = (sim, Slot::Reserve(u16::MAX));
    let gone: Vec<(SimId, Slot)> = (domain.pending.range(first..=last))
        .filter(|&(&(_, slot), &handle)| match slot {
            Slot::Reserve(level) => !still_due(level, handle),
            _ => true,
        })
        .map(|(key, _)| *key)
        .collect();
    for key in gone {
        if let Some(handle) = domain.pending.remove(&key) {
            ctx.cancel(handle);
        }
    }
    let after = planned_in(ctx);
    let planned: Vec<(u16, SimTime)> = reserve.crossings().filter(|&(_, at)| at > after).collect();
    for (level, at) in planned {
        let handle = ctx.schedule(at, EventPayload::ReserveCrossing { sim, level }, by);
        domain.pending.insert((sim, Slot::Reserve(level)), handle);
    }
}

/// The reserve of a person reaches `level`, half or 0 (ADR-0026, addendum of 2026-11-06 on
/// hunger at 0): its entry, `core:weakened` at half and `core:wasted` at 0, with the capacity;
/// nothing else changes here. The refusal of commands and the word on the page follow the
/// reserve from this second; a slower walk and the energy's extra the next walk and the next
/// change of rates.
pub(crate) fn on_crossing(
    domain: &mut DomainState,
    sim: SimId,
    level: u16,
    ctx: &mut SimCtx<'_, '_>,
) {
    // The event that runs right now is no longer pending.
    domain.pending.remove(&(sim, Slot::Reserve(level)));
    let Some(person) = domain.house.sims.get(&sim) else {
        panic!("invariant house.orphan_event");
    };
    let capacity = person.reserve.capacity();
    let payload = match level {
        EMPTY => LogPayload::Wasted { capacity },
        _ => LogPayload::Weakened { capacity },
    };
    ctx.emit(payload, vec![Subject::Sim(sim)], Causes::new(ctx.trigger()));
}

/// The band of `sim` now (`house::reserve::Reserve::band`).
pub(super) fn band_now(domain: &DomainState, sim: SimId, now: SimTime) -> Option<Band> {
    let Some(person) = domain.house.sims.get(&sim) else {
        panic!("invariant house.orphan_event");
    };
    person.reserve.band(person.needs[0], now)
}

/// The seconds a walk of `secs` that `sim` begins now takes (`ReserveRules::walk`).
pub(super) fn walk(domain: &DomainState, data: &Data, sim: SimId, secs: u32, now: SimTime) -> u32 {
    let (_, houses, _) = home(domain, data, sim);
    houses.reserve().walk(secs, band_now(domain, sim, now))
}

/// Whether every reserve in a house has the capacity its person's body gives at `t`
/// (`begun`): the loader's check (ADR-0026, addendum of 2026-11-06 on hunger at 0). The size
/// does not change in a life, so the clock serves for the second the needs began.
pub(crate) fn sized(domain: &DomainState, data: &Data, t: SimTime) -> bool {
    let Some(houses) = data.houses() else {
        return domain.house.sims.is_empty();
    };
    (domain.house.sims.iter()).all(|(&sim, person)| {
        let size = crate::body_contract::reserve_size_of(domain, data, (sim, t));
        person.reserve.capacity() == houses.reserve().capacity_of(size)
    })
}

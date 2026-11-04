//! Commands (ADR-0024 section 5): a person with needs at home takes an offer of an object of
//! its house now. The check is a pure function of the state for the second the command acts
//! in; the effect ends what the person does with reason `command` and begins the commanded
//! walk, both caused by the input. A command never waits: what cannot be done is rejected.

use super::{DomainState, action};
use crate::data::Data;
use crate::house::state::Place;
use crate::input::{Command, InputRejected, RejectCode};
use crate::log::{Causes, EndReason};
use crate::time::{SimDuration, SimTime};
use crate::world::SimCtx;

/// Pure check of a command at the stop `closed_through` (ADR-0024 section 5 and its addendum
/// of 2026-11-04): first who and whether it lives in its house with needs, then what (object,
/// offer, age), then the person's state (away, a critical need), then the place, and last the
/// walk itself, which must arrive before the end of time.
pub(crate) fn check(
    domain: &DomainState,
    data: &Data,
    closed_through: SimTime,
    input: &Command,
) -> Result<(), InputRejected> {
    rejection(domain, data, closed_through, input).map_err(|code| InputRejected { code })
}

fn rejection(
    domain: &DomainState,
    data: &Data,
    closed_through: SimTime,
    input: &Command,
) -> Result<(), RejectCode> {
    let now = super::house::running(domain, closed_through)?;
    let sim = input.sim;
    let Some(born) = domain.sims.get(&sim).map(|person| person.born) else {
        return Err(if domain.archive.contains_key(&sim) {
            RejectCode::SimNotAlive
        } else {
            RejectCode::UnknownSim
        });
    };
    let Some(person) = domain.house.sims.get(&sim) else {
        return Err(RejectCode::NotInHouse);
    };
    let (household, houses, house) = action::home(domain, data, sim);
    let object =
        (house.def().objects.get(usize::from(input.object))).ok_or(RejectCode::ObjectUnknown)?;
    let kind = (houses.kind(&object.kind)).unwrap_or_else(|| panic!("invariant house.unknown"));
    let offer = (kind.offers.get(usize::from(input.offer))).ok_or(RejectCode::OfferUnknown)?;
    let age = crate::house::choice::age(born, now)
        .unwrap_or_else(|| panic!("invariant age.out_of_range"));
    if !(offer.min_age..=offer.max_age).contains(&age) {
        return Err(RejectCode::WrongAge);
    }
    // Away, on the way out, or with a departure waiting: the departure stays first. On the
    // way to an object the walk ends back in the room it came from.
    let room = match (person.place, person.action) {
        (Place::Room(room), _) | (Place::Walking { from: room, .. }, Some(_))
            if person.due.is_none() =>
        {
            room
        }
        _ => return Err(RejectCode::Away),
    };
    if action::for_critical(domain, data, sim, now) {
        return Err(RejectCode::CriticalNeed);
    }
    // Its own place frees when what it does ends, and goes to the first who waits for that
    // object (ADR-0026 section 5); a place held for it is free for it.
    let mut taken = action::taken(domain, household, sim);
    if let Some(own) = person.action
        && !domain.house.waits.contains_key(&(household, own.object))
    {
        taken.remove(&(own.object, own.slot));
    }
    if (0..kind.slots).all(|slot| taken.contains(&(input.object, slot))) {
        return Err(RejectCode::SlotBusy);
    }
    // A walk that would end beyond the end of time cannot begin.
    let walk = SimDuration::from_secs(i64::from(house.walk_to(room, input.object)));
    now.checked_add(walk).map(drop).ok_or(RejectCode::EndOfTime)
}

/// What the person does ends now with reason `command`, its rates having worked until now and
/// no choice following, then the commanded walk begins (ADR-0024 section 5, ADR-0026 section
/// 4); both entries are caused by the input.
pub(crate) fn apply(
    domain: &mut DomainState,
    data: &Data,
    input: &Command,
    ctx: &mut SimCtx<'_, '_>,
) {
    let sim = input.sim;
    if (domain.house.sims.get(&sim)).is_some_and(|person| person.action.is_some()) {
        let causes = Causes::new(ctx.trigger());
        action::interrupt(domain, data, sim, EndReason::Command, causes, ctx);
    }
    let what = (input.object, input.offer);
    action::obey(domain, data, sim, what, Causes::new(ctx.trigger()), ctx);
}

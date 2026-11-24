//! The practice of skills (ADR-0033 section 6): hearing two adults talk is language, reading at
//! the bookshelf and a day at school are reading, playing at the toy chest is play. Each
//! occasion adds effective seconds to a raised person whose household lives in a house; each
//! 10 effective hours crossed write a `core:practiced`. Written before the occasion's steps of
//! `D`, so talent and diligence are those before them (section 10).

use alloc::vec;
use alloc::vec::Vec;

use crate::data::Data;
use crate::data_id::DataId;
use crate::development::{Axis, RAISED_UNTIL};
use crate::house::day::{YoungPlace, young_place};
use crate::house::state::{Action, Duty, Place};
use crate::ids::{EventId, HouseholdId, SimId};
use crate::log::{Cause, Causes, EndReason, LogPayload, Subject};
use crate::skill::{Skill, TALENT_AGE, diligence, effective, milestones};
use crate::time::SimDuration;
use crate::world::SimCtx;

use super::DomainState;
use super::conception::{Person, person};

/// The raw seconds of reading a day at school counts (ADR-0033 section 6).
const SCHOOL_SECONDS: u64 = 3600;

/// A talk of `a` and `b` at the furniture `object` of the house of `home`, logged as `talked`:
/// if both are 18 or older, each person under 18 of `home` other than the two who is in the
/// furniture's room hears it, in the order of ids, and practises language for the talk's
/// length (`talk_delay_seconds`). A person with needs is in its room (on a walk in none); one
/// without, a small child, in its night room in bed, its day room by day, at the kindergarten in
/// none.
pub(super) fn talked(
    domain: &mut DomainState,
    data: &Data,
    ((a, b), home, object): ((SimId, SimId), HouseholdId, u16),
    talked: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let now = ctx.now();
    let age = |sim: SimId| (domain.sims.get(&sim)).and_then(|p| now.checked_since(p.born));
    let adult = |sim| age(sim).is_some_and(|age| age >= RAISED_UNTIL);
    let (Some(houses), Some(social)) = (data.houses(), data.social()) else {
        return;
    };
    let Some(moved) = domain.house.households.get(&home) else {
        return;
    };
    if !adult(a) || !adult(b) {
        return;
    }
    let def = (houses.house(&moved.house))
        .unwrap_or_else(|| panic!("invariant practice.house"))
        .def();
    let room = (def.objects.get(usize::from(object)))
        .unwrap_or_else(|| panic!("invariant practice.object"))
        .room;
    let young_room = match young_place(houses.day(), now) {
        YoungPlace::Bed => Some(def.child_night_room),
        YoungPlace::DayRoom => Some(def.child_day_room),
        YoungPlace::Kita { .. } => None,
    };
    let in_room = |sim: SimId| match domain.house.sims.get(&sim) {
        Some(person) => matches!(person.place, Place::Room(at) if at == room),
        None => young_room == Some(room),
    };
    let hearers: Vec<SimId> = (domain.sims.iter())
        .filter(|(sim, person)| person.household == home && ![a, b].contains(sim))
        .map(|(sim, _)| *sim)
        .filter(|sim| age(*sim).is_some_and(|age| age < RAISED_UNTIL) && in_room(*sim))
        .collect();
    let raw = u64::from(social.limits.talk_delay_seconds);
    for sim in hearers {
        practise(domain, data, (sim, Skill::Language, raw), talked, ctx);
    }
}

/// `sim` came home from `place`, logged as `came`: a day at school is an hour of reading.
pub(super) fn came_home(
    domain: &mut DomainState,
    data: &Data,
    (sim, place): (SimId, Duty),
    came: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    if place == Duty::School {
        practise(
            domain,
            data,
            (sim, Skill::Reading, SCHOOL_SECONDS),
            came,
            ctx,
        );
    }
}

/// `sim` ended its action at furniture of `kind` for `reason`, logged as `ended`; `commanded` if
/// a command chose it, `ran` the seconds since it reached its object (`None` if it ended on the
/// way). Reading is the offer of the bookshelf and playing that of the toy chest; neither begun
/// nor ended by a command, the seconds it ran practise.
pub(super) fn finished(
    domain: &mut DomainState,
    data: &Data,
    (sim, kind, reason, commanded, ran): (SimId, &DataId, EndReason, bool, Option<SimDuration>),
    ended: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let skill = match kind.as_str() {
        "core:bookshelf" => Skill::Reading,
        "core:toy_chest" => Skill::Play,
        _ => return,
    };
    let Some(ran) = ran else {
        return;
    };
    if commanded || reason == EndReason::Command {
        return;
    }
    let raw = u64::try_from(ran.as_secs()).unwrap_or_else(|_| panic!("invariant practice.ran"));
    practise(domain, data, (sim, skill, raw), ended, ctx);
}

/// `sim` left the house of `home` doing `action`, its end (reason left) logged as `ended`: as
/// any other end, the seconds it ran at its object practise, by the furniture of that house.
pub(super) fn left(
    domain: &mut DomainState,
    data: &Data,
    (sim, home, action): (SimId, HouseholdId, Action),
    ended: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let (Some(houses), Some(moved)) = (data.houses(), domain.house.households.get(&home)) else {
        panic!("invariant practice.house");
    };
    let def = (houses.house(&moved.house))
        .unwrap_or_else(|| panic!("invariant practice.house"))
        .def();
    let kind = (def.objects.get(usize::from(action.object)))
        .unwrap_or_else(|| panic!("invariant practice.object"))
        .kind
        .clone();
    let ran = (action.reached).map(|reached| ctx.now() - reached);
    let done = (sim, &kind, EndReason::Left, action.commanded, ran);
    finished(domain, data, done, ended, ctx);
}

/// `sim` practises `skill` for `raw` seconds at this second, by `cause`, an occasion in its house:
/// if it is living and raised, its practice grows by the effective seconds, and each 10 effective
/// hours crossed write a `core:practiced` with the level of its talent there. Only an occasion in
/// a house reaches here (a talk, a return home, an action's end there, leaving it in the middle of
/// one), so practice happens only while its household lives in a house.
fn practise(
    domain: &mut DomainState,
    data: &Data,
    (sim, skill, raw): (SimId, Skill, u64),
    cause: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let now = ctx.now();
    if raw == 0 || !practises(domain, sim) {
        return;
    }
    let Some(who) = facts(domain, data, sim) else {
        panic!("invariant practice.person");
    };
    let Some(age) = now.checked_since(who.born) else {
        panic!("invariant practice.age");
    };
    let z = |t: &DataId, at: SimDuration| who.phenotype(data, t, at).unwrap_or(0.0);
    let f = diligence(z(&Axis::Conscientiousness.trait_id(), age));
    let talent = z(&skill.talent(), TALENT_AGE);
    let added = effective(raw, skill.window(age), f);
    let Some(raised) = domain.house.raised.get_mut(&sim) else {
        panic!("invariant practice.raised");
    };
    let before = raised.practice[skill.index()];
    let after = before.saturating_add(added);
    raised.practice[skill.index()] = after;
    let learning = &data.demography().learning;
    for milestone in milestones(talent, (before, after), learning) {
        let practiced = LogPayload::Practiced {
            skill,
            hours: milestone.hours,
            level_milli: milestone.level_milli,
        };
        ctx.emit(
            practiced,
            vec![Subject::Sim(sim)],
            Causes::new(Cause::Event(cause)),
        );
    }
}

/// Whether `sim` practises: raised, and living or, for the last seconds of an action its death or
/// emigration ended, archived in this second.
fn practises(domain: &DomainState, sim: SimId) -> bool {
    let known = domain.sims.contains_key(&sim) || domain.archive.contains_key(&sim);
    known && domain.house.raised.contains_key(&sim)
}

/// What the phenotype reads of `sim`, living or archived.
fn facts<'a>(domain: &'a DomainState, data: &Data, sim: SimId) -> Option<Person<'a>> {
    if domain.sims.contains_key(&sim) {
        return Some(person(domain, data, sim));
    }
    let gone = domain.archive.get(&sim)?;
    Some(Person {
        born: gone.born,
        lineage_key: gone.lineage_key,
        genome: &gone.genome,
        parents: domain.parent_keys(sim),
        seeds: domain.seeds(data, sim),
        lived: domain.lived(sim),
    })
}

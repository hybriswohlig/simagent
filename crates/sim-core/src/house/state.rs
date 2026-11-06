//! The house part of the state (ADR-0024 section 7 and its addendum of 2026-11-02): the
//! households moved in, the persons with needs, the waiting lists and the holds, all in the
//! projection. A run without `MoveIn` holds them empty. `consistent` is the loader's check.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::data_id::DataId;
use crate::house::choice::{CRITICAL, next_choice};
use crate::house::data::{
    AGE_BANDS, DayRules, House, HouseData, NEEDS, Need, NeedRates, OfferDef, age_band,
};
use crate::house::day::{YoungPlace, night, young_place};
use crate::house::day::{day_departure, next_night, work_draw};
use crate::house::need::{EMPTY, FULL, NeedSample};
use crate::house::reserve::{Band, Doing, Reserve};
use crate::ids::{HouseholdId, SimId};
use crate::time::{SimDuration, SimTime, YEAR, day_index};

/// At most this many households live in a house at once (ADR-0024 section 6).
pub const MAX_HOUSEHOLDS: usize = 64;

/// At most this many choices per person and calendar day (ADR-0024 section 6).
pub const MAX_DECISIONS: u16 = 200;

/// A household in its house; `morning` once its first `HouseMorning` has come.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MovedIn {
    pub house: DataId,
    pub morning: bool,
}

/// A person with needs (ADR-0024 sections 1, 2 and 4, ADR-0026), in the order hunger,
/// energy, bladder, hygiene, fun; `collapsed` while it lies on the floor of its room after its
/// energy reached 0 (ADR-0026, addendum of 2026-11-04 on a need at 0); its reserve at hunger 0
/// (`house::reserve`, the addendum of 2026-11-06 on hunger at 0).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InHouse {
    pub needs: [NeedSample; 5],
    pub place: Place,
    pub action: Option<Action>,
    pub due: Option<Due>,
    pub decisions: Decisions,
    pub collapsed: bool,
    pub reserve: Reserve,
}

/// Where a person with needs is (ADR-0025 section 5). A walk with an action leads to its
/// object, one without leads out through the entrance, after which the person is away, or from
/// a room one does not stay in to the house's `dwell_room` (ADR-0026, addendum of 2026-11-04
/// after the playtest).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Place {
    Room(u8),
    Walking {
        from: u8,
        to: u8,
        depart: SimTime,
        arrive: SimTime,
    },
    Away,
}

/// The offer a person chose, on the way or doing it; it holds the place `slot` of the
/// object (ADR-0026 sections 4 and 5). A time-filler, `filler`, ends at the person's next
/// choice (ADR-0026, addendum of 2026-11-04 on comfort and the time-filler), at the latest after
/// its length (its addendum of 2026-11-05 on the time-filler's length).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Action {
    pub object: u16,
    pub offer: u16,
    pub slot: u8,
    pub filler: bool,
}

/// A departure that waits for the end of an action for a critical need (ADR-0024 section
/// 4): where to, when it was planned, when the person is due back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Due {
    pub place: Duty,
    pub depart: SimTime,
    pub back: SimTime,
}

/// Where a person goes on a weekday (ADR-0024 section 4); a code in the log.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Duty {
    School,
    Work,
}

/// The choices of the calendar day `day` (`day_index`); the count starts again with the
/// first choice of a later day.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decisions {
    pub day: i64,
    pub count: u16,
}

/// The house part of the domain state, each map in the projection after `pool_used`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HouseState {
    pub households: BTreeMap<HouseholdId, MovedIn>,
    pub sims: BTreeMap<SimId, InHouse>,
    /// Per object of a household's house the waiters with the second they began to wait,
    /// by that second, then by SimId.
    pub waits: BTreeMap<(HouseholdId, u16), Vec<(SimTime, SimId)>>,
    /// The places held for a waiter, per object and place.
    pub holds: BTreeMap<(HouseholdId, u16, u8), SimId>,
}

impl HouseState {
    pub const fn new() -> HouseState {
        HouseState {
            households: BTreeMap::new(),
            sims: BTreeMap::new(),
            waits: BTreeMap::new(),
            holds: BTreeMap::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.households.is_empty()
            && self.sims.is_empty()
            && self.waits.is_empty()
            && self.holds.is_empty()
    }
}

/// Whether a person walks without an action to the room one stays in (ADR-0026, addendum of
/// 2026-11-04 after the playtest): every other walk without one leads out to the entrance.
pub fn dwell_walk(person: &InHouse, house: &House) -> bool {
    matches!(person.place, Place::Walking { to, .. } if person.action.is_none() && to != house.def().entrance)
}

/// The places `young` small children hold in their beds at `now` (ADR-0026, addendum of
/// 2026-11-04 after the playtest): while the clock puts them in bed (`young_place`), the
/// first `young` of `House::child_beds`, one each; none otherwise.
pub fn child_held(house: &House, data: &HouseData, young: usize, now: SimTime) -> Vec<(u16, u8)> {
    match young_place(data.day(), now) {
        YoungPlace::Bed => house.child_beds(data).take(young).collect(),
        YoungPlace::Kita { .. } | YoungPlace::DayRoom => Vec::new(),
    }
}

/// The second the idle person `person`, born at `born` with the lineage key `key` in the world
/// of `seed`, chooses next from `start` on, planned in the second `after`
/// (`house::choice::next_choice`), `taken` the places of the house taken or held for others: it
/// weighs the objects with a free place from the room it stands in, or on its walk to the room
/// one stays in from that room, no earlier than its arrival (ADR-0026, addendum of 2026-11-04
/// after the playtest), each before today's departure closes it (its addendum on comfort and the
/// time-filler). A handler plans in its second, an input at the clock and the loader in the
/// clock, before the second they act in.
pub fn next_choice_of(
    (house, data): (&House, &HouseData),
    (born, key, seed): (SimTime, crate::lineage::LineageKey, u64),
    person: &InHouse,
    taken: &BTreeSet<(u16, u8)>,
    (start, after): (SimTime, SimTime),
) -> Option<SimTime> {
    let (room, start) = match person.place {
        Place::Room(room) => (room, start),
        Place::Walking { to, arrive, .. } => (to, start.max(arrive)),
        Place::Away => return None,
    };
    let free = |object: u16| {
        let def = &house.def().objects[usize::from(object)];
        let slots = data.kind(&def.kind).map_or(0, |kind| kind.slots);
        (0..slots).any(|slot| !taken.contains(&(object, slot)))
    };
    let percent = data.day().choice_mood_percent;
    let moods = |at: SimTime, object: u16, offer: u16| {
        crate::house::choice::mood(seed, key, at, (object, offer), percent)
    };
    let draw = || work_draw(seed, key);
    let leaves = |at: SimTime| day_departure(data.day(), born, draw, at);
    next_choice(
        (house, data),
        (born, &person.needs, room),
        free,
        (start, after),
        moods,
        leaves,
    )
}

/// The second the time-filler of `person` at its object ends (ADR-0026, addendum of 2026-11-04
/// on comfort and the time-filler, and of 2026-11-05 on its length), from `start` on, planned in
/// the second `after`, with the needs at the filler's rates and `taken` the places taken (its own
/// among them) or held for others: the earliest of its length from when it began, as any action
/// (`action_end`), its next choice (`next_choice_of` from the object's room), its bedtime, the
/// start of the night after it began, today's departure if that comes after `after`
/// (`house::day::day_departure` of the day planned last at or before `start`), and at the latest
/// `fallback_max_seconds` after it began; at once if it began in a night. `None` for anyone not
/// at a time-filler, or if the end lies beyond the end of time.
pub fn filler_end_of(
    (house, data): (&House, &HouseData),
    who: (SimTime, crate::lineage::LineageKey, u64),
    person: &InHouse,
    taken: &BTreeSet<(u16, u8)>,
    (start, after): (SimTime, SimTime),
) -> Option<SimTime> {
    let (Place::Room(_), Some(action)) = (person.place, person.action) else {
        return None;
    };
    if !action.filler {
        return None;
    }
    let (born, key, seed) = who;
    let rules = data.day();
    let object = house.def().objects.get(usize::from(action.object))?;
    let offer = (data.kind(&object.kind)?.offers).get(usize::from(action.offer))?;
    let (_, sample) = (NEEDS.into_iter().zip(person.needs)).find(|(need, _)| offer.sets(*need))?;
    let begun = sample.at();
    if night(rules, born, begun).is_some() {
        return Some(begun);
    }
    let bedtime = next_night(rules, born, begun).map(|night| night.from);
    let longest = SimDuration::from_secs(i64::from(rules.fallback_max_seconds));
    let cap = begun.checked_add(longest);
    let length = action_end(person, offer, rules, born);
    let draw = || work_draw(seed, key);
    let depart = day_departure(rules, born, draw, start).filter(|&depart| depart > after);
    let next = match bedtime.is_none_or(|bedtime| start < bedtime) {
        true => next_choice_of((house, data), who, person, taken, (start, after)),
        false => None,
    };
    [next, bedtime, cap, length, depart]
        .into_iter()
        .flatten()
        .min()
}

/// The end the time-filler `(object, offer)` would have if the idle `person`, born at `who.0`,
/// chose it now, `walk_secs` away (ADR-0026, addendum of 2026-11-05 on the time-filler's length):
/// as `filler_end_of` plans it at the arrival, the needs it sets written down there with its
/// rates of the age band then and its place among `taken`, but planned in the second of the
/// choice, `now`, so that a departure on the way bounds it too. `None` beyond the end of time.
pub fn filler_end_if(
    (house, data): (&House, &HouseData),
    who: (SimTime, crate::lineage::LineageKey, u64),
    (person, taken): (&InHouse, &BTreeSet<(u16, u8)>),
    (object, offer, walk_secs): (u16, u16, u32),
    now: SimTime,
) -> Option<SimTime> {
    let Place::Room(_) = person.place else {
        return None;
    };
    let def = house.def().objects.get(usize::from(object))?;
    let kind = data.kind(&def.kind)?;
    let offered = kind.offers.get(usize::from(offer))?;
    let arrive = now.checked_add(SimDuration::from_secs(i64::from(walk_secs)))?;
    let years = (arrive.as_secs() - who.0.as_secs()).div_euclid(YEAR.as_secs());
    let band = u8::try_from(years).ok().and_then(age_band)?;
    let slot = (0..kind.slots).find(|slot| !taken.contains(&(object, *slot)))?;
    let mut there = person.clone();
    there.place = Place::Room(def.room);
    there.action = Some(Action {
        object,
        offer,
        slot,
        filler: true,
    });
    for (sample, need) in there.needs.iter_mut().zip(NEEDS) {
        if offered.sets(need) {
            *sample = sample.with_rate(arrive, offered.rate(data.rates(), need, band));
        }
    }
    let mut taken = taken.clone();
    taken.insert((object, slot));
    filler_end_of((house, data), who, &there, &taken, (arrive, now))
}

/// The end a time-filler at `offer` would have at most, chosen in the second `chosen` and there at
/// `arrive`, by the bounds of `filler_end_of` that do not depend on the places (ADR-0026, addendum
/// of 2026-11-05 on the time-filler's length): the arrival itself if that lies in a night, as
/// `filler_end_of` ends it there at once whatever else would; else the earliest of its length
/// from the arrival, its bedtime, today's departure if it comes after the choice and
/// `fallback_max_seconds` after the arrival. The next choice can only make the end earlier.
/// `None` if every bound lies beyond the end of time.
fn filler_bound(
    data: &HouseData,
    (born, key, seed): (SimTime, crate::lineage::LineageKey, u64),
    offer: &OfferDef,
    (chosen, arrive): (SimTime, SimTime),
) -> Option<SimTime> {
    let rules = data.day();
    if night(rules, born, arrive).is_some() {
        return Some(arrive);
    }
    let length =
        (offer.seconds).and_then(|secs| arrive.checked_add(SimDuration::from_secs(secs.into())));
    let bedtime = next_night(rules, born, arrive).map(|night| night.from);
    let draw = || work_draw(seed, key);
    let depart = day_departure(rules, born, draw, arrive).filter(|&depart| depart > chosen);
    let longest = SimDuration::from_secs(i64::from(rules.fallback_max_seconds));
    let cap = arrive.checked_add(longest);
    [length, bedtime, depart, cap].into_iter().flatten().min()
}

/// Whether a choice in the second `chosen` could have taken a time-filler at `offer` there at
/// `arrive` (ADR-0026, addendum of 2026-11-05 on the time-filler's length): by day, and its end by
/// the fixed bounds (`filler_bound`) at least `fallback_min_seconds` after the choice. The choice
/// takes none whose end, which those bounds only can make later, comes sooner.
pub fn filler_possible(
    data: &HouseData,
    who: (SimTime, crate::lineage::LineageKey, u64),
    offer: &OfferDef,
    (chosen, arrive): (SimTime, SimTime),
) -> bool {
    let rules = data.day();
    if night(rules, who.0, chosen).is_some() {
        return false;
    }
    let shortest = i64::from(rules.fallback_min_seconds);
    filler_bound(data, who, offer, (chosen, arrive))
        .is_none_or(|end| (end - chosen).as_secs() >= shortest)
}

/// The loader's check of the time-fillers in a house part (ADR-0026, addendum of 2026-11-05 on
/// the time-filler's length), `seed` the world's, `member` a living sim's household, birth and
/// lineage key: on its walk a time-filler was chosen when the walk began; at its object it
/// arrived when the needs its offer sets were written down, and was chosen its walk from some
/// room of the house before, a weak one's walk twice as long (ADR-0026, addendum of 2026-11-06 on
/// hunger at 0) where the samples allow that band at its start. A time-filler no such choice could have taken (`filler_possible`)
/// is refused (`Broken::Filler`). Which offer a choice took is not checked otherwise.
pub fn fillers_consistent(
    house: &HouseState,
    data: Option<&HouseData>,
    seed: u64,
    member: impl Fn(SimId) -> Option<(HouseholdId, SimTime, crate::lineage::LineageKey)>,
) -> Result<(), Broken> {
    for (&sim, person) in &house.sims {
        let Some(action) = person.action.filter(|action| action.filler) else {
            continue;
        };
        let (household, born, key) = member(sim).ok_or(Broken::NotMember)?;
        let data = data.ok_or(Broken::HouseUnknown)?;
        let moved = house.households.get(&household).ok_or(Broken::NotMember)?;
        let home = data.house(&moved.house).ok_or(Broken::HouseUnknown)?;
        let object =
            (home.def().objects.get(usize::from(action.object))).ok_or(Broken::ObjectUnknown)?;
        let kind = data.kind(&object.kind).ok_or(Broken::ObjectUnknown)?;
        let offer = (kind.offers.get(usize::from(action.offer))).ok_or(Broken::OfferUnknown)?;
        let who = (born, key, seed);
        let possible = match person.place {
            Place::Walking { depart, arrive, .. } => {
                filler_possible(data, who, offer, (depart, arrive))
            }
            Place::Room(_) => {
                let set = (NEEDS.into_iter().zip(person.needs)).find(|(need, _)| offer.sets(*need));
                let begun = set.map(|(_, sample)| sample.at()).ok_or(Broken::Filler)?;
                // A weak walk took longer (ADR-0026, addendum of 2026-11-06 on hunger at 0): each
                // length only from a second at which the samples allow its band.
                (0..home.def().rooms).any(|room| {
                    let secs = home.walk_to(room, action.object);
                    (BANDS.iter()).any(|&band| {
                        let walked = data.reserve().walk(secs, band);
                        let walk = SimDuration::from_secs(i64::from(walked));
                        (begun.checked_sub(walk)).is_some_and(|chosen| {
                            bands_then(person, chosen)[depth(band)]
                                && filler_possible(data, who, offer, (chosen, begun))
                        })
                    })
                })
            }
            Place::Away => true,
        };
        if !possible {
            return Err(Broken::Filler);
        }
    }
    Ok(())
}

/// The crossings a change of the person's rates at `after` plans (ADR-0026 section 1): per
/// need and threshold below its value the second it gets there, if that comes after
/// `after`, in the order of the needs and then of the thresholds.
pub fn crossings(person: &InHouse, after: SimTime) -> impl Iterator<Item = (Need, u16, SimTime)> {
    (NEEDS.into_iter().zip(person.needs))
        .flat_map(|(need, sample)| sample.crossings().map(move |(level, at)| (need, level, at)))
        .filter(move |&(_, _, at)| at > after)
}

/// The shortest action (ADR-0026 section 4).
const SHORTEST_SECS: i64 = 60;

/// The second the action of a person at its object ends (ADR-0026 section 4). It began when
/// the needs its offer sets were written down: a fixed length after that, or when the need it
/// fills is full, at least 60 s and at most `max_seconds` after it. A sleep ends with the
/// night it began in, of a person born at `born` by `rules`, and at once if it began after
/// the night it was chosen in had ended (ADR-0026, addendum of 2026-11-04). None if that comes
/// after the end of time (the last night of time has no end), or for an offer that sets no
/// need.
pub fn action_end(
    person: &InHouse,
    offer: &OfferDef,
    rules: &DayRules,
    born: SimTime,
) -> Option<SimTime> {
    let (_, sample) = (NEEDS.into_iter().zip(person.needs)).find(|(need, _)| offer.sets(*need))?;
    let begun = sample.at();
    if offer.sleep {
        return night(rules, born, begun).map_or(Some(begun), |night| night.until);
    }
    let secs = match (offer.seconds, offer.until_full, offer.max_seconds) {
        (Some(seconds), _, _) => i64::from(seconds),
        (None, Some(need), Some(longest)) => {
            let longest = i64::from(longest);
            let at = NEEDS.iter().position(|&each| each == need)?;
            match person.needs[at].full() {
                Some(full) => (full - begun).as_secs().max(SHORTEST_SECS).min(longest),
                None => longest,
            }
        }
        _ => return None,
    };
    begun.checked_add(SimDuration::from_secs(secs))
}

/// The rates of a person lying on the floor in the age band `band` (ADR-0026, addendum of
/// 2026-11-04 on a need at 0): the sleeping ones, energy at the collapse's rate of the day.
pub fn lying_rate(rates: &NeedRates, rules: &DayRules, need: Need, band: usize) -> i32 {
    match need {
        Need::Energy => rules.zero.collapse_energy_rate,
        _ => rates.asleep.rate(need, band),
    }
}

/// The second a person lying on the floor gets up (ADR-0026, addendum of 2026-11-04 on a need
/// at 0), of a person born at `born` by `rules`: when its energy, written down at 0 in the
/// second it fell there and rising since, is at `stand_up_at`, or when the night it fell in
/// ends, whichever comes first; the night's end only with its energy above 0 by then, as nothing
/// gets up one with none. None if both lie beyond the end of time.
pub fn stand_up(person: &InHouse, rules: &DayRules, born: SimTime) -> Option<SimTime> {
    let energy = person.needs[1];
    let up = energy.rises_to(rules.zero.stand_up_at);
    let ends = (night(rules, born, energy.at()).and_then(|night| night.until))
        .filter(|&until| energy.value_at(until) > EMPTY);
    match (up, ends) {
        (Some(up), Some(ends)) => Some(up.min(ends)),
        (up, ends) => up.or(ends),
    }
}

/// The rule a loaded house part breaks (the addendum's list of checks).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Broken {
    TooManyHouseholds,
    HouseholdGone,
    HouseUnknown,
    /// Not living, or not a member of a household moved in whose first morning has come.
    NotMember,
    TooYoung,
    NeedValue,
    NeedTime,
    RoomUnknown,
    /// A walk not begun before the clock (an input at the clock begins one to an object in the
    /// next second, never one out), not as long as the house makes it, or that leads elsewhere
    /// than its object's room or, without an action, the entrance; or an action done outside
    /// its object's room.
    Walk,
    ObjectUnknown,
    OfferUnknown,
    SlotUnknown,
    /// A place of two actions, or taken and held at once.
    SlotTaken,
    /// Away or on the way out with a rate, an action or a duty.
    AwayBusy,
    /// Away or on the way out with needs not all written down at one second, on the way out
    /// not at the start of the walk: leaving writes them down together (ADR-0024 section 4).
    Left,
    /// A rate other than the awake one, or at an object other than the offer's, by the age
    /// band of its second; or the needs an action sets written down at different seconds.
    Rate,
    DueOrder,
    /// A departure waiting without a walk or an action that raised a need critical at the
    /// departure (ADR-0024 section 4).
    Due,
    /// A walk to an offer of sleep begun outside the person's night, or a sleep begun outside
    /// it (ADR-0026, addendum of 2026-11-04).
    Sleep,
    Decisions,
    /// A waiter or holder without needs in that household, away, walking out, busy or lying
    /// on the floor.
    Waiter,
    WaitOrder,
    /// A person on two waiting lists, or waiting and holding.
    QueuedTwice,
    /// Waiting for an object with a place neither taken nor held.
    WaitFree,
    /// A held place without its holder's choice due in the next second after an input at the
    /// clock, or a choice due for one who waits (ADR-0026 section 5).
    Choice,
    /// Lying on the floor elsewhere than in a room, with an action, or with its energy not at
    /// 0 when it was last written down (it fell there in that second and only rises since).
    Collapse,
    /// Idle in a room one does not stay in, waiting for nothing, holding nothing and with no
    /// choice due in the next second after an input at the clock; or a walk to the room one
    /// stays in from a room one stays in (ADR-0026, addendum of 2026-11-04 after the playtest).
    Dwell,
    /// An action on a place a small child holds in its bed at the clock or in the next second
    /// (ADR-0026, addendum of 2026-11-04 after the playtest).
    ChildBed,
    /// A time-filler at an offer that is none (ADR-0026, addendum of 2026-11-04 on comfort and
    /// the time-filler), or one no choice could have taken: chosen in a night or ending too soon
    /// by the bounds the save shows (`fillers_consistent`, its addendum of 2026-11-05).
    Filler,
    /// A reserve (ADR-0026, addendum of 2026-11-06 on hunger at 0) of a capacity the rules do
    /// not have or other than the person's body gives, a value above its capacity, written
    /// down after the clock, with a rate other than what the person does gives it, or an energy
    /// extra the rules do not have or one lying on the floor or out carries.
    Reserve,
}

/// The checks on load of the house part (ADR-0024, addendum of 2026-11-02) at `now`, the
/// second the loaded world runs next. `choices` are the persons whose choice is due at `now`
/// if an input acted at the clock, `None` if none did: an advance runs every event of a
/// second, so only such an input leaves a walk begun, a place held or a choice due in the
/// second after the clock (ADR-0024, addendum of 2026-11-04). `alive` says whether a household
/// has not ended; `member` gives a living sim's household and birth.
pub fn consistent(
    house: &HouseState,
    data: Option<&HouseData>,
    now: SimTime,
    choices: Option<&BTreeSet<SimId>>,
    alive: impl Fn(HouseholdId) -> bool,
    member: impl Fn(SimId) -> Option<(HouseholdId, SimTime)>,
) -> Result<(), Broken> {
    if house.households.len() > MAX_HOUSEHOLDS {
        return Err(Broken::TooManyHouseholds);
    }
    let mut homes: BTreeMap<HouseholdId, &House> = BTreeMap::new();
    for (&id, moved) in &house.households {
        if !alive(id) {
            return Err(Broken::HouseholdGone);
        }
        let home = data.and_then(|data| data.house(&moved.house));
        homes.insert(id, home.ok_or(Broken::HouseUnknown)?);
    }
    let mut taken: BTreeSet<(HouseholdId, u16, u8)> = BTreeSet::new();
    let mut households: BTreeMap<SimId, HouseholdId> = BTreeMap::new();
    for (&sim, person) in &house.sims {
        let (household, born) = member(sim).ok_or(Broken::NotMember)?;
        let morning = house.households.get(&household).is_some_and(|m| m.morning);
        if !morning {
            return Err(Broken::NotMember);
        }
        let six = born.checked_add(YEAR * i64::from(AGE_BANDS[0].0));
        if six.is_none_or(|six| six > now) {
            return Err(Broken::TooYoung);
        }
        households.insert(sim, household);
        let home = homes[&household];
        person_consistent(person, home, data, (now, choices.is_some()))?;
        collapse_consistent(person)?;
        rates_consistent(person, home, data, born)?;
        due_consistent(person, home, data, born)?;
        sleep_consistent(person, home, data, born)?;
        reserve_consistent(person, home, data, (born, now, choices.is_some()))?;
        if let Some(action) = person.action
            && !taken.insert((household, action.object, action.slot))
        {
            return Err(Broken::SlotTaken);
        }
    }
    let mut queued: BTreeSet<SimId> = BTreeSet::new();
    let fit = |sim: SimId, household: HouseholdId| {
        let person = house
            .sims
            .get(&sim)
            .filter(|_| households[&sim] == household);
        person.is_some_and(|person| {
            let walking_out =
                matches!(person.place, Place::Walking { .. }) && person.action.is_none();
            let free = person.action.is_none() && !person.collapsed;
            person.place != Place::Away && !walking_out && free
        })
    };
    for (&(household, object, slot), &sim) in &house.holds {
        let home = homes.get(&household).ok_or(Broken::Waiter)?;
        let slots = slots_of(home, object, data)?;
        if slot >= slots {
            return Err(Broken::SlotUnknown);
        }
        if taken.contains(&(household, object, slot)) {
            return Err(Broken::SlotTaken);
        }
        if !fit(sim, household) {
            return Err(Broken::Waiter);
        }
        if !queued.insert(sim) {
            return Err(Broken::QueuedTwice);
        }
    }
    for (&(household, object), waiters) in &house.waits {
        let home = homes.get(&household).ok_or(Broken::Waiter)?;
        slots_of(home, object, data)?;
        if waiters.is_empty() || waiters.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(Broken::WaitOrder);
        }
        for &(since, sim) in waiters {
            if since > now || !fit(sim, household) {
                return Err(Broken::Waiter);
            }
            if !queued.insert(sim) {
                return Err(Broken::QueuedTwice);
            }
        }
    }
    for &(household, object) in house.waits.keys() {
        let slots = slots_of(homes[&household], object, data)?;
        let busy = |slot: u8| {
            taken.contains(&(household, object, slot))
                || house.holds.contains_key(&(household, object, slot))
        };
        if !(0..slots).all(busy) {
            return Err(Broken::WaitFree);
        }
    }
    // A place is held for one whose choice is due, and whoever chooses has left every list.
    let due = |sim: &SimId| choices.is_some_and(|due| due.contains(sim));
    let mut waiters = house.waits.values().flatten().map(|(_, waiter)| waiter);
    if !house.holds.values().all(due) || waiters.any(due) {
        return Err(Broken::Choice);
    }
    // Idle and waiting for nothing, one walks from a room one does not stay in to the room one
    // does in the second it chooses, unless that walk would end beyond the end of time.
    for (&sim, person) in &house.sims {
        let home = homes[&households[&sim]];
        let Place::Room(room) = person.place else {
            continue;
        };
        let stays = home.dwells(room) || person.action.is_some() || person.collapsed;
        // Weak, that walk takes longer (ADR-0026, addendum of 2026-11-06 on hunger at 0): any
        // length the bands the samples allow at the clock give.
        let secs = home.walk_to_dwell(room);
        let clock = now.checked_sub(SimDuration::from_secs(1)).unwrap_or(now);
        let lengths = match data {
            Some(data) => walks_then(person, data.reserve(), (secs, clock)),
            None => alloc::vec![secs],
        };
        let beyond = (lengths.iter()).any(|&walked| {
            now.checked_add(SimDuration::from_secs(i64::from(walked)))
                .is_none()
        });
        if !(stays || queued.contains(&sim) || due(&sim) || beyond) {
            return Err(Broken::Dwell);
        }
    }
    Ok(())
}

/// The check on load of the places small children hold in their beds (ADR-0026, addendum of
/// 2026-11-04 after the playtest): no action is on one at the clock or in the next second, `now`,
/// `young` the number of living members without needs of a household moved in.
pub fn children_consistent(
    house: &HouseState,
    data: Option<&HouseData>,
    now: SimTime,
    young: impl Fn(HouseholdId) -> usize,
    member: impl Fn(SimId) -> Option<(HouseholdId, SimTime)>,
) -> Result<(), Broken> {
    let clock = now.checked_sub(SimDuration::from_secs(1)).unwrap_or(now);
    for (&sim, person) in &house.sims {
        let (Some(action), Some((household, _))) = (person.action, member(sim)) else {
            continue;
        };
        let Some(moved) = house
            .households
            .get(&household)
            .filter(|moved| moved.morning)
        else {
            continue;
        };
        let data = data.ok_or(Broken::HouseUnknown)?;
        let home = data.house(&moved.house).ok_or(Broken::HouseUnknown)?;
        let place = (action.object, action.slot);
        let held = |at: SimTime| child_held(home, data, young(household), at).contains(&place);
        if held(clock) || held(now) {
            return Err(Broken::ChildBed);
        }
    }
    Ok(())
}

/// The checks of one person with needs in its house; `opened` if an input acted at the clock.
fn person_consistent(
    person: &InHouse,
    home: &House,
    data: Option<&HouseData>,
    (now, opened): (SimTime, bool),
) -> Result<(), Broken> {
    for need in &person.needs {
        if need.value() > FULL {
            return Err(Broken::NeedValue);
        }
        if need.at() > now {
            return Err(Broken::NeedTime);
        }
    }
    let rooms = home.def().rooms;
    let room = |room: u8| {
        if room < rooms {
            Ok(())
        } else {
            Err(Broken::RoomUnknown)
        }
    };
    if let Some(action) = person.action {
        let object = home.def().objects.get(usize::from(action.object));
        let kind = object.and_then(|object| data?.kind(&object.kind));
        let kind = kind.ok_or(Broken::ObjectUnknown)?;
        let offer = kind.offers.get(usize::from(action.offer));
        let offer = offer.ok_or(Broken::OfferUnknown)?;
        if action.slot >= kind.slots {
            return Err(Broken::SlotUnknown);
        }
        // Only an offer the data name a time-filler is one (ADR-0026, addendum of 2026-11-04 on
        // comfort and the time-filler).
        if action.filler && !offer.fallback {
            return Err(Broken::Filler);
        }
    }
    let dwelling = dwell_walk(person, home);
    let goal = match (person.action, dwelling) {
        (Some(action), _) => home.def().objects[usize::from(action.object)].room,
        (None, true) => home.def().dwell_room,
        (None, false) => home.def().entrance,
    };
    match person.place {
        Place::Room(at) => {
            room(at)?;
            if person.action.is_some() && at != goal {
                return Err(Broken::Walk);
            }
        }
        Place::Walking {
            from,
            to,
            depart,
            arrive,
        } => {
            room(from)?;
            room(to)?;
            // The walk the house gives, begun before the clock or, by an input at it (a
            // command, ADR-0024 section 5), in the second the loaded world runs next; no input
            // begins a walk out (leaving is an event) or one to the room one stays in (only a
            // choice does, from a room one does not stay in).
            let secs = match (person.action, dwelling) {
                (Some(action), _) => home.walk_to(from, action.object),
                (None, true) => home.walk_to_dwell(from),
                (None, false) => home.walk_out(from),
            };
            let walked = arrive.as_secs() - depart.as_secs();
            let begun = depart < now || (depart == now && opened && person.action.is_some());
            let rules = data.ok_or(Broken::HouseUnknown)?.reserve();
            // Weak, a walk takes longer (ADR-0026, addendum of 2026-11-06 on hunger at 0): by the
            // bands the samples allow at its start.
            let lengths = walks_then(person, rules, (secs, depart));
            if !begun || !lengths.contains(&u32::try_from(walked).unwrap_or(u32::MAX)) || to != goal
            {
                return Err(Broken::Walk);
            }
            if dwelling && home.dwells(from) {
                return Err(Broken::Dwell);
            }
        }
        Place::Away => {}
    }
    // Away and on the way out every need stands still since the second of leaving, the start
    // of the walk out (ADR-0024 section 4).
    if out(person, home) {
        let still = person.needs.iter().all(|need| need.rate() == 0);
        if !still || person.action.is_some() || person.due.is_some() {
            return Err(Broken::AwayBusy);
        }
        let left = person.needs[0].at();
        let together = person.needs.iter().all(|need| need.at() == left);
        let walked = match person.place {
            Place::Walking { depart, .. } => depart == left,
            _ => true,
        };
        if !together || !walked {
            return Err(Broken::Left);
        }
    }
    if let Some(due) = person.due
        && due.depart >= due.back
    {
        return Err(Broken::DueOrder);
    }
    let decisions = person.decisions;
    if decisions.count > MAX_DECISIONS || decisions.day > day_index(now) {
        return Err(Broken::Decisions);
    }
    Ok(())
}

/// Whether a person is away or on its way out: a walk without an action leads out, unless it
/// leads to the room one stays in.
fn out(person: &InHouse, home: &House) -> bool {
    let walking_out = matches!(person.place, Place::Walking { .. }) && person.action.is_none();
    person.place == Place::Away || (walking_out && !dwell_walk(person, home))
}

/// The age band at `at` of a person born at `born`.
fn band_at(born: SimTime, at: SimTime) -> Option<usize> {
    let years = (at.as_secs() - born.as_secs()).div_euclid(YEAR.as_secs());
    u8::try_from(years).ok().and_then(age_band)
}

/// A departure waits only for a walk or an action that raised a need critical at the
/// departure (ADR-0024 section 4): on the way one the offer raises in the age band of the
/// arrival, at the object one that rises. After a waiting departure no walk begins, so on the
/// way every need was written down before it; at the object only the needs the offer sets
/// may have been written down after it, at the arrival. Such a need fell on the way at the
/// awake rate: where that never rises, it was at least its value at the arrival when the
/// person was due to leave; where it may rise, its value then is open. A need at 0 (ADR-0026,
/// addendum of 2026-11-04 on a need at 0) may write down others later only as a run does while
/// the walk or action goes on: an accident only where it raises hygiene, the bladder full and,
/// on the way, hygiene at most the cap in the same second; hygiene reaching 0 its 0 and fun in
/// one second. Their values at the departure are open.
fn due_consistent(
    person: &InHouse,
    home: &House,
    data: Option<&HouseData>,
    born: SimTime,
) -> Result<(), Broken> {
    let Some(due) = person.due else {
        return Ok(());
    };
    // A departure waits for one lying on the floor too, until it gets up (ADR-0026, addendum
    // of 2026-11-04 on a need at 0); or it waited for an action the collapse ended.
    if person.collapsed {
        return Ok(());
    }
    let action = person.action.ok_or(Broken::Due)?;
    let rates = data.ok_or(Broken::HouseUnknown)?.rates();
    let object = &home.def().objects[usize::from(action.object)];
    let kind = data.and_then(|data| data.kind(&object.kind));
    let offer = kind.and_then(|kind| kind.offers.get(usize::from(action.offer)));
    let offer = offer.ok_or(Broken::OfferUnknown)?;
    let walking = matches!(person.place, Place::Walking { .. });
    let after = |sample: NeedSample| sample.at() > due.depart;
    let written_late =
        |need: Need, sample: NeedSample| after(sample) && (walking || !offer.sets(need));
    let raises = |need: Need, sample: NeedSample| match person.place {
        Place::Walking { arrive, .. } => {
            band_at(born, arrive).is_some_and(|band| offer.rate(rates, need, band) > 0)
        }
        _ => sample.rate() > 0,
    };
    let cap = data
        .ok_or(Broken::HouseUnknown)?
        .day()
        .zero
        .accident_hygiene_at_most;
    let [_, _, bladder, hygiene, fun] = person.needs;
    // The accident goes on only where the walk or the action raises hygiene, the shower: the
    // bladder full and, on the way, hygiene at most the cap in the same second.
    let wet = written_late(Need::Bladder, bladder)
        && bladder.value() == FULL
        && raises(Need::Hygiene, hygiene)
        && (!walking || (hygiene.at() == bladder.at() && hygiene.value() <= cap));
    // The sting writes hygiene's 0 and fun in one second; on the way to the shower an accident
    // may write hygiene's 0 again after it.
    let stung = written_late(Need::Fun, fun)
        && hygiene.value() == EMPTY
        && (hygiene.at() == fun.at() || (wet && hygiene.at() > fun.at()));
    let explained = |need: Need| match need {
        Need::Bladder => wet,
        Need::Hygiene => (walking && wet) || (stung && hygiene.at() == fun.at()),
        Need::Fun => stung,
        // Energy takes the reserve's deeper extra at a change of rates while the walk or the
        // action goes on (ADR-0026, addendum of 2026-11-06 on hunger at 0).
        Need::Energy => person.reserve.energy() != 0,
        Need::Hunger => false,
    };
    let unexplained =
        |need: Need, sample: NeedSample| written_late(need, sample) && !explained(need);
    if (NEEDS.into_iter().zip(person.needs)).any(|(need, sample)| unexplained(need, sample)) {
        return Err(Broken::Due);
    }
    let falls_awake =
        |need: Need| (0..AGE_BANDS.len()).all(|band| rates.awake.rate(need, band) <= 0);
    let critical = |need: Need, sample: NeedSample| {
        if written_late(need, sample) {
            true
        } else if after(sample) {
            !falls_awake(need) || sample.value() <= CRITICAL
        } else {
            sample.value_at(due.depart) <= CRITICAL
        }
    };
    let waits = (NEEDS.into_iter().zip(person.needs))
        .any(|(need, sample)| raises(need, sample) && critical(need, sample));
    if waits { Ok(()) } else { Err(Broken::Due) }
}

/// The rates of a person with needs (ADR-0026 sections 1 and 4): at its object those the
/// offer sets are the offer's, written down together when it arrived; every other is the
/// awake rate; lying on the floor all are those of `lying_rate`; each by the age band at the
/// second it was written down. Away and on the way out, all are 0 (checked with the place).
fn rates_consistent(
    person: &InHouse,
    home: &House,
    data: Option<&HouseData>,
    born: SimTime,
) -> Result<(), Broken> {
    if out(person, home) {
        return Ok(());
    }
    let houses = data.ok_or(Broken::HouseUnknown)?;
    let rates = houses.rates();
    if person.collapsed {
        for (need, sample) in NEEDS.into_iter().zip(person.needs) {
            let band = band_at(born, sample.at()).ok_or(Broken::Rate)?;
            if sample.rate() != lying_rate(rates, houses.day(), need, band) {
                return Err(Broken::Rate);
            }
        }
        return Ok(());
    }
    let doing = match (person.place, person.action) {
        (Place::Room(_), Some(action)) => {
            let object = &home.def().objects[usize::from(action.object)];
            let kind = data.and_then(|data| data.kind(&object.kind));
            kind.and_then(|kind| kind.offers.get(usize::from(action.offer)))
        }
        _ => None,
    };
    let mut begun = None;
    for (need, sample) in NEEDS.into_iter().zip(person.needs) {
        let band = band_at(born, sample.at()).ok_or(Broken::Rate)?;
        let rate = match doing {
            Some(offer) if offer.sets(need) => {
                if begun.is_some_and(|begun| begun != sample.at()) {
                    return Err(Broken::Rate);
                }
                begun = Some(sample.at());
                offer.rate(rates, need, band)
            }
            _ => rates.awake.rate(need, band),
        };
        // Energy carries the reserve's extra (ADR-0026, addendum of 2026-11-06 on hunger at 0).
        let extra = match need {
            Need::Energy => person.reserve.energy(),
            _ => 0,
        };
        if sample.rate().checked_sub(extra) != Some(rate) {
            return Err(Broken::Rate);
        }
    }
    Ok(())
}

/// One lying on the floor (ADR-0026, addendum of 2026-11-04 on a need at 0) is in a room without
/// an action, its energy written down at 0 when it fell there: nothing writes it down again
/// before it gets up.
fn collapse_consistent(person: &InHouse) -> Result<(), Broken> {
    if !person.collapsed {
        return Ok(());
    }
    let in_room = matches!(person.place, Place::Room(_)) && person.action.is_none();
    if in_room && person.needs[1].value() == EMPTY {
        Ok(())
    } else {
        Err(Broken::Collapse)
    }
}

/// The bands of `Reserve::band` in the order of their depth: none, drawing on the reserve, weak,
/// wasted.
const BANDS: [Option<Band>; 4] = [
    None,
    Some(Band::Draws),
    Some(Band::Weak),
    Some(Band::Wasted),
];

/// The depth of a band in `BANDS`.
fn depth(band: Option<Band>) -> usize {
    BANDS.iter().position(|&each| each == band).unwrap_or(0)
}

/// The bands a person may have had at `t` (ADR-0026, addendum of 2026-11-06 on hunger at 0), by
/// the evidence its samples keep, in the order of `BANDS`. Hunger written down at or before `t`
/// above 0 or rising then: none. With hunger at 0 then and the reserve written down at or before
/// `t` too: exactly its band. With the reserve written down later, while hunger stayed at 0 and
/// so the reserve only fell: any band no deeper than its value now gives, none excepted. With
/// hunger written down after `t`, the samples no longer show it: every band.
fn bands_then(person: &InHouse, t: SimTime) -> [bool; 4] {
    let (hunger, reserve) = (person.needs[0], person.reserve);
    if hunger.at() > t {
        return [true; 4];
    }
    if hunger.rate() > 0 || hunger.value_at(t) != EMPTY {
        return [true, false, false, false];
    }
    if reserve.at() <= t {
        let exact = depth(reserve.band(hunger, t));
        return core::array::from_fn(|at| at == exact);
    }
    let deepest = depth(Some(reserve.band_of(reserve.value())));
    core::array::from_fn(|at| (1..=deepest).contains(&at))
}

/// The seconds a walk of `secs` may have taken a person who began it at `t`: one length per band
/// it may have had then (`bands_then`, `ReserveRules::walk`).
fn walks_then(
    person: &InHouse,
    rules: &crate::house::reserve::ReserveRules,
    (secs, t): (u32, SimTime),
) -> Vec<u32> {
    let possible = bands_then(person, t);
    let mut lengths: Vec<u32> = (BANDS.iter().zip(possible))
        .filter(|&(_, possible)| possible)
        .map(|(&band, _)| rules.walk(secs, band))
        .collect();
    lengths.dedup();
    lengths
}

/// The reserve (ADR-0026, addendum of 2026-11-06 on hunger at 0): a capacity of the rules and the
/// value at most it; written down at or before the clock, or in the open second `now` only by an
/// input that changed the person's rates there, so that some need was written down then too
/// (`opened`). Its rate is what the person does gives it at the last change of rates, in the age
/// band of the second it was written down: out 0; eating the refill while it was below its
/// capacity then, else 0; hunger at 0 the drain, asleep or awake; else 0. Hunger's 0 counts from
/// the clock: a crossing of 0 in the open second has not run, unless an input changed the
/// person's rates in that second, which reckoned with it. A drain began with hunger at 0, which
/// the hunger sample shows if it was written down by then; a refill began when the person reached
/// its food, with hunger's rate. The energy's extra is that of the band in the second energy was
/// written down last (one `bands_then` allows; 0 for one lying on the floor or out), and, unless an
/// action at its object holds energy's rate, that of the band at a later change of rates, when
/// hunger or the reserve were written down after energy: else that change would have written
/// energy down anew.
fn reserve_consistent(
    person: &InHouse,
    home: &House,
    data: Option<&HouseData>,
    (born, now, opened): (SimTime, SimTime, bool),
) -> Result<(), Broken> {
    let houses = data.ok_or(Broken::HouseUnknown)?;
    let rules = houses.reserve();
    let reserve = person.reserve;
    let capacity = reserve.capacity();
    if !rules.capacity.contains(&capacity) || reserve.value() > capacity || reserve.at() > now {
        return Err(Broken::Reserve);
    }
    let changed_now = person.needs.iter().any(|need| need.at() == now);
    if reserve.at() == now && !(opened && changed_now) {
        return Err(Broken::Reserve);
    }
    let clock = now.checked_sub(SimDuration::from_secs(1)).unwrap_or(now);
    let then = if changed_now { now } else { clock };
    let hunger = person.needs[0];
    if hunger.at() > then {
        return Err(Broken::Reserve);
    }
    let offer = match (person.place, person.action) {
        (Place::Room(_), Some(action)) => (home.def().objects.get(usize::from(action.object)))
            .and_then(|object| houses.kind(&object.kind))
            .and_then(|kind| kind.offers.get(usize::from(action.offer))),
        _ => None,
    };
    let doing = Doing {
        out: out(person, home),
        eating: hunger.rate() > 0,
        asleep: person.collapsed || offer.is_some_and(|offer| offer.sleep),
        empty: hunger.value_at(then) == EMPTY,
    };
    let band = band_at(born, reserve.at()).ok_or(Broken::Reserve)?;
    if reserve.rate() != rules.rate(doing, band, (reserve.value(), capacity)) {
        return Err(Broken::Reserve);
    }
    let drain_before_hunger_at_0 =
        reserve.rate() < 0 && hunger.at() <= reserve.at() && hunger.value_at(reserve.at()) != EMPTY;
    let refill_not_at_the_food = reserve.rate() > 0 && reserve.at() != hunger.at();
    if drain_before_hunger_at_0 || refill_not_at_the_food {
        return Err(Broken::Reserve);
    }
    // The energy's extra.
    let still = person.collapsed || out(person, home);
    let extra = |band: Option<Band>| if still { 0 } else { rules.energy(band) };
    let energy = person.needs[1];
    let at_its_write = (BANDS.iter().zip(bands_then(person, energy.at())))
        .any(|(&band, possible)| possible && extra(band) == reserve.energy());
    let held = offer.is_some_and(|offer| offer.sets(Need::Energy));
    let last = hunger.at().max(reserve.at());
    let at_the_last_change = last <= energy.at()
        || held
        || still
        || extra(reserve.band(hunger, last)) == reserve.energy();
    if at_its_write && at_the_last_change {
        Ok(())
    } else {
        Err(Broken::Reserve)
    }
}

/// A sleep only in the person's night (ADR-0026, addendum of 2026-11-04): a walk to an offer of
/// sleep began inside one, and so did a sleep at its object, when the needs it sets were
/// written down. A sleep whose walk ended after the night it was chosen in ends in the second
/// it begins, so no save holds one.
fn sleep_consistent(
    person: &InHouse,
    home: &House,
    data: Option<&HouseData>,
    born: SimTime,
) -> Result<(), Broken> {
    let Some(action) = person.action else {
        return Ok(());
    };
    let data = data.ok_or(Broken::HouseUnknown)?;
    let object = &home.def().objects[usize::from(action.object)];
    let kind = data.kind(&object.kind).ok_or(Broken::ObjectUnknown)?;
    let offer = (kind.offers.get(usize::from(action.offer))).ok_or(Broken::OfferUnknown)?;
    if !offer.sleep {
        return Ok(());
    }
    let begun = match person.place {
        Place::Walking { depart, .. } => depart,
        Place::Room(_) => person.needs[0].at(),
        Place::Away => return Ok(()),
    };
    match night(data.day(), born, begun) {
        Some(_) => Ok(()),
        None => Err(Broken::Sleep),
    }
}

/// The places of an object of the house.
fn slots_of(home: &House, object: u16, data: Option<&HouseData>) -> Result<u8, Broken> {
    let object = home.def().objects.get(usize::from(object));
    let kind = object.and_then(|object| data?.kind(&object.kind));
    Ok(kind.ok_or(Broken::ObjectUnknown)?.slots)
}

//! The house part of the state (ADR-0024 section 7 and its addendum of 2026-11-02): the
//! households moved in, the persons with needs, the waiting lists and the holds, all in the
//! projection. A run without `MoveIn` holds them empty. `consistent` is the loader's check.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::data_id::DataId;
use crate::house::data::{AGE_BANDS, House, HouseData};
use crate::house::need::{FULL, NeedSample};
use crate::ids::{HouseholdId, SimId};
use crate::time::{SimTime, YEAR, day_index};

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
/// energy, bladder, hygiene, fun.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InHouse {
    pub needs: [NeedSample; 5],
    pub place: Place,
    pub action: Option<Action>,
    pub due: Option<Due>,
    pub decisions: Decisions,
}

/// Where a person with needs is (ADR-0025 section 5). A walk with an action leads to its
/// object, one without leads out through the entrance; after it the person is away.
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
/// object (ADR-0026 sections 4 and 5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Action {
    pub object: u16,
    pub offer: u16,
    pub slot: u8,
}

/// A departure that waits for the end of an action for a critical need (ADR-0024 section
/// 4): where to, when it was planned, when the person is due back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Due {
    pub place: Duty,
    pub depart: SimTime,
    pub back: SimTime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    /// A walk that ends before it begins, or that leads elsewhere than its object's room or,
    /// without an action, the entrance; or an action done outside its object's room.
    Walk,
    ObjectUnknown,
    OfferUnknown,
    SlotUnknown,
    /// A place of two actions, or taken and held at once.
    SlotTaken,
    /// Away with a rate, an action or a duty.
    AwayBusy,
    DueOrder,
    Decisions,
    /// A waiter or holder without needs in that household, away, walking out or busy.
    Waiter,
    WaitOrder,
    /// A person on two waiting lists, or waiting and holding.
    QueuedTwice,
    /// Waiting for an object with a place neither taken nor held.
    WaitFree,
}

/// The checks on load of the house part (ADR-0024, addendum of 2026-11-02) at `now`, the
/// second the loaded world runs next. `alive` says whether a household has not ended;
/// `member` gives a living sim's household and birth.
pub fn consistent(
    house: &HouseState,
    data: Option<&HouseData>,
    now: SimTime,
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
        person_consistent(person, home, data, now)?;
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
            person.place != Place::Away && !walking_out && person.action.is_none()
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
    Ok(())
}

/// The checks of one person with needs in its house.
fn person_consistent(
    person: &InHouse,
    home: &House,
    data: Option<&HouseData>,
    now: SimTime,
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
        if usize::from(action.offer) >= kind.offers.len() {
            return Err(Broken::OfferUnknown);
        }
        if action.slot >= kind.slots {
            return Err(Broken::SlotUnknown);
        }
    }
    let goal = match person.action {
        Some(action) => home.def().objects[usize::from(action.object)].room,
        None => home.def().entrance,
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
            if depart > arrive || to != goal {
                return Err(Broken::Walk);
            }
        }
        Place::Away => {
            let still = person.needs.iter().all(|need| need.rate() == 0);
            if !still || person.action.is_some() || person.due.is_some() {
                return Err(Broken::AwayBusy);
            }
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

/// The places of an object of the house.
fn slots_of(home: &House, object: u16, data: Option<&HouseData>) -> Result<u8, Broken> {
    let object = home.def().objects.get(usize::from(object));
    let kind = object.and_then(|object| data?.kind(&object.kind));
    Ok(kind.ok_or(Broken::ObjectUnknown)?.slots)
}

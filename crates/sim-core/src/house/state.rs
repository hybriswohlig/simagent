//! The house part of the state (ADR-0024 section 7 and its addendum of 2026-11-02): the
//! households moved in, the persons with needs, the waiting lists and the holds, all in the
//! projection. A run without `MoveIn` holds them empty. `consistent` is the loader's check.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::data_id::DataId;
use crate::house::choice::CRITICAL;
use crate::house::data::{AGE_BANDS, DayRules, House, HouseData, NEEDS, Need, OfferDef, age_band};
use crate::house::day::night;
use crate::house::need::{FULL, NeedSample};
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
/// after the end of time, or for an offer that sets no need.
pub fn action_end(
    person: &InHouse,
    offer: &OfferDef,
    rules: &DayRules,
    born: SimTime,
) -> Option<SimTime> {
    let (_, sample) = (NEEDS.into_iter().zip(person.needs)).find(|(need, _)| offer.sets(*need))?;
    let begun = sample.at();
    if offer.sleep {
        return Some(night(rules, born, begun).map_or(begun, |night| night.until));
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
    /// A walk begun after the second the loaded world runs next (an input at the clock begins
    /// one in it), not as long as the house makes it, or that leads elsewhere than its
    /// object's room or, without an action, the entrance; or an action done outside its
    /// object's room.
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
        rates_consistent(person, home, data, born)?;
        due_consistent(person, home, data, born)?;
        sleep_consistent(person, home, data, born)?;
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
            // The walk the house gives, begun before the clock or, by an input at it (a
            // command, ADR-0024 section 5), in the second the loaded world runs next.
            let secs = match person.action {
                Some(action) => home.walk_to(from, action.object),
                None => home.walk_out(from),
            };
            let walked = arrive.as_secs() - depart.as_secs();
            if depart > now || walked != i64::from(secs) || to != goal {
                return Err(Broken::Walk);
            }
        }
        Place::Away => {}
    }
    // Away and on the way out every need stands still since the second of leaving, the start
    // of the walk out (ADR-0024 section 4).
    if out(person) {
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

/// Whether a person is away or on its way out: a walk without an action leads out.
fn out(person: &InHouse) -> bool {
    matches!(
        (person.place, person.action),
        (Place::Away, _) | (Place::Walking { .. }, None)
    )
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
/// person was due to leave; where it may rise, its value then is open.
fn due_consistent(
    person: &InHouse,
    home: &House,
    data: Option<&HouseData>,
    born: SimTime,
) -> Result<(), Broken> {
    let Some(due) = person.due else {
        return Ok(());
    };
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
    if (NEEDS.into_iter().zip(person.needs)).any(|(need, sample)| written_late(need, sample)) {
        return Err(Broken::Due);
    }
    let raises = |need: Need, sample: NeedSample| match person.place {
        Place::Walking { arrive, .. } => {
            band_at(born, arrive).is_some_and(|band| offer.rate(rates, need, band) > 0)
        }
        _ => sample.rate() > 0,
    };
    let falls_awake =
        |need: Need| (0..AGE_BANDS.len()).all(|band| rates.awake.rate(need, band) <= 0);
    let critical = |need: Need, sample: NeedSample| {
        if after(sample) {
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
/// awake rate; each by the age band at the second it was written down. Away and on the way
/// out, all are 0 (checked with the place).
fn rates_consistent(
    person: &InHouse,
    home: &House,
    data: Option<&HouseData>,
    born: SimTime,
) -> Result<(), Broken> {
    if out(person) {
        return Ok(());
    }
    let rates = data.ok_or(Broken::HouseUnknown)?.rates();
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
        if sample.rate() != rate {
            return Err(Broken::Rate);
        }
    }
    Ok(())
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

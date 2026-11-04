//! The houses as data (ADR-0025 sections 1 to 4) with the kinds of furniture and their offers
//! (ADR-0026 section 2), the rates of the needs (ADR-0026 section 1) and the rules of the day
//! (ADR-0024 section 4): their types, their checks on load, the walking table of each house
//! and the house for a household of a size.
//! The files name rooms and offers; their reader turns the names into numbers, so the core
//! holds no text (ADR-0004 section 7) and knows a room by its number (ADR-0025 section 5).

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
use core::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

use crate::data_id::DataId;
use crate::house::need::FULL;
use crate::time::DAY;

/// The five needs, in the order of the log's arrays (ADR-0026 section 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Need {
    Hunger,
    Energy,
    Bladder,
    Hygiene,
    Fun,
}

pub const NEEDS: [Need; 5] = [
    Need::Hunger,
    Need::Energy,
    Need::Bladder,
    Need::Hygiene,
    Need::Fun,
];

/// The oldest age an offer may name, the oldest age of the body contract (ADR-0014).
pub const OLDEST: u8 = 130;

/// The age bands of the rates in years, both ends included (ADR-0026 section 1).
pub const AGE_BANDS: [(u8, u8); 4] = [(6, 12), (13, 17), (18, 66), (67, OLDEST)];

const SLOTS: RangeInclusive<u8> = 1..=8;
const LENGTH_SECS: RangeInclusive<u32> = 60..=43_200;
const DOOR_SECS: RangeInclusive<u32> = 1..=120;
const MAX_CAPACITY: u8 = 8;
const MAX_OBJECTS: usize = 32;
const MAX_OFFERS: usize = 64;
/// A jump on coming home only lowers a need, at most by its whole scale (ADR-0024 section 4).
const JUMPS: RangeInclusive<i32> = -1000..=0;
/// The mood moves the points of a choice at most by this many percent either way.
const MOST_MOOD: u8 = 50;

/// Walking to an object in one's own room, and through the entrance out of the house
/// (ADR-0025 section 4).
const WITHIN_ROOM_SECS: u32 = 5;
const OUT_SECS: u32 = 5;

/// The band of an age, `None` under 6 and above `OLDEST`.
pub fn age_band(age: u8) -> Option<usize> {
    AGE_BANDS
        .iter()
        .position(|&(low, high)| (low..=high).contains(&age))
}

/// The rates of `data/house/needs.json` (ADR-0026 section 1), in points per hour.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NeedRates {
    pub awake: BandRates,
    pub asleep: BandRates,
}

/// Per need one rate for each age band.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BandRates {
    pub hunger: [i32; 4],
    pub energy: [i32; 4],
    pub bladder: [i32; 4],
    pub hygiene: [i32; 4],
    pub fun: [i32; 4],
}

impl BandRates {
    /// Panics for a band outside `AGE_BANDS`.
    pub fn rate(&self, need: Need, band: usize) -> i32 {
        let rates = match need {
            Need::Hunger => &self.hunger,
            Need::Energy => &self.energy,
            Need::Bladder => &self.bladder,
            Need::Hygiene => &self.hygiene,
            Need::Fun => &self.fun,
        };
        rates[band]
    }
}

/// The rules of the day of `data/house/day.json` (ADR-0024 section 4), times in seconds of
/// the day: the day plan at `day_plan_at`, the wake `wake_before` a departure; the night
/// (ADR-0026, addendum of 2026-11-04), what a need at 0 does and the most the mood moves the
/// points of a choice, in percent either way (its addenda of 2026-11-04 on a need at 0 and on
/// the mood), last.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DayRules {
    pub day_plan_at: u32,
    pub wake_before: u32,
    pub school: SchoolRules,
    pub work: WorkRules,
    pub return_jumps: NeedJumps,
    pub young: YoungRules,
    pub night: NightRules,
    pub zero: ZeroRules,
    pub choice_mood_percent: u8,
}

/// School from `from_age` to the day before the birthday of `until_age`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchoolRules {
    pub from_age: u8,
    pub until_age: u8,
    pub depart: u32,
    pub back: u32,
}

/// Work from `from_age` to the day before the birthday of `until_age`, by the work plan's
/// draw out of 100: full time below `full_time_below`, part time below `part_time_below`,
/// else at home.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkRules {
    pub from_age: u8,
    pub until_age: u8,
    pub full_time_below: u8,
    pub part_time_below: u8,
    pub full_time: Hours,
    pub part_time: Hours,
}

/// When one leaves the house and comes back.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hours {
    pub depart: u32,
    pub back: u32,
}

/// The fixed jump of each need on coming home.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NeedJumps {
    pub hunger: i32,
    pub energy: i32,
    pub bladder: i32,
    pub hygiene: i32,
    pub fun: i32,
}

/// Children under `until_age` on a weekday in the kindergarten from `kita_from` to
/// `kita_to`, in bed from `bed_from` to `bed_to` of the next morning.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct YoungRules {
    pub until_age: u8,
    pub kita_from: u32,
    pub kita_to: u32,
    pub bed_from: u32,
    pub bed_to: u32,
}

/// The night per age band of `AGE_BANDS` (ADR-0026, addendum of 2026-11-04): the night of a
/// day runs from `from` of that day to `until` of the next, for the band of the age at the
/// day's plan; to `until_weekend` when the next morning is a Saturday or a Sunday (weekday 5
/// or 6), the lie-in of its addendum after the playtest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NightRules {
    pub from: [u32; 4],
    pub until: [u32; 4],
    pub until_weekend: [u32; 4],
}

/// What a need at 0 does (ADR-0026, addendum of 2026-11-04 on a need at 0): energy at 0 lays the
/// person on the floor, its energy rising `collapse_energy_rate` an hour until it is at
/// `stand_up_at`; the bladder at 0 leaves hygiene at most `accident_hygiene_at_most`; hygiene
/// reaching 0 changes fun once by `hygiene_empty_fun`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZeroRules {
    pub collapse_energy_rate: i32,
    pub stand_up_at: u16,
    pub accident_hygiene_at_most: u16,
    pub hygiene_empty_fun: i32,
}

impl ZeroRules {
    /// One lying gets up, rising to a level it has not reached at 0; the accident's hygiene is
    /// a value; the change of fun only lowers it, at most by its whole scale.
    fn checked(&self) -> Result<(), HouseError> {
        let rises = self.collapse_energy_rate > 0;
        let up = (1..=FULL).contains(&self.stand_up_at);
        let hygiene = self.accident_hygiene_at_most <= FULL;
        let fun = JUMPS.contains(&self.hygiene_empty_fun);
        if rises && up && hygiene && fun {
            Ok(())
        } else {
            Err(HouseError::DayZero)
        }
    }
}

impl DayRules {
    /// The checks of the day, in this order: the times, the wake, the ages, the shares of
    /// the work plan, the jumps, the night, a need at 0, the mood.
    fn checked(&self) -> Result<(), HouseError> {
        let (school, work, young) = (&self.school, &self.work, &self.young);
        let hours = [
            (school.depart, school.back),
            (work.full_time.depart, work.full_time.back),
            (work.part_time.depart, work.part_time.back),
        ];
        let young_times = [young.bed_to, young.kita_from, young.kita_to, young.bed_from];
        let mut times = (hours.iter().flat_map(|&(depart, back)| [depart, back]))
            .chain(young_times)
            .chain([self.day_plan_at]);
        let in_day = times.all(|time| i64::from(time) < DAY.as_secs());
        let ordered = hours.iter().all(|&(depart, back)| depart < back)
            && young_times.windows(2).all(|pair| pair[0] < pair[1]);
        if !in_day || !ordered {
            return Err(HouseError::DayTime);
        }
        // The wake comes after the day plan that sets it, so on the day of the departure, and
        // before the departure (review: at the same second the departure would come first).
        let wakes_after_plan = |&(depart, _): &(u32, u32)| {
            depart
                .checked_sub(self.wake_before)
                .is_some_and(|wake| wake > self.day_plan_at)
        };
        if self.wake_before == 0 || !hours.iter().all(wakes_after_plan) {
            return Err(HouseError::DayWake);
        }
        if !(young.until_age <= school.from_age
            && school.from_age < school.until_age
            && school.until_age <= work.from_age
            && work.from_age < work.until_age)
        {
            return Err(HouseError::DayAge);
        }
        if !(0 < work.full_time_below
            && work.full_time_below < work.part_time_below
            && work.part_time_below <= 100)
        {
            return Err(HouseError::DayShare);
        }
        let jumps = &self.return_jumps;
        let jumps = [
            jumps.hunger,
            jumps.energy,
            jumps.bladder,
            jumps.hygiene,
            jumps.fun,
        ];
        if !jumps.iter().all(|jump| JUMPS.contains(jump)) {
            return Err(HouseError::DayJump);
        }
        self.night_checked(&hours)?;
        self.zero.checked()?;
        // The mood moves the points at most by half, so an offer keeps them and its mood fits
        // the log.
        if self.choice_mood_percent > MOST_MOOD {
            return Err(HouseError::DayMood);
        }
        Ok(())
    }

    /// The night (ADR-0026, addendum of 2026-11-04), each bound below 24 h. Each end at or
    /// before the earliest wake, so that no wake falls into a night and every morning's
    /// choices are awake ones; at or after the day plan, which sets up the next bedtime. A
    /// weekend morning has no wake, so its end (`until_weekend`, the addendum after the
    /// playtest) only comes at or after the plan. Each start after the last return, so that
    /// everyone with a duty is home at bedtime. The wake comes before its departure and that
    /// before its return, so each start then lies after every end and after the plan of its
    /// day: the plan at 5:00 always plans a bedtime still to come, and no second lies in two
    /// nights. The small children's bed holds its places from `bed_from` (the addendum after
    /// the playtest), so every night starts at or after it and every end comes at or before
    /// it: nobody sleeps on a place when a child's hold of it begins.
    fn night_checked(&self, hours: &[(u32, u32); 3]) -> Result<(), HouseError> {
        let night = &self.night;
        // The wake check before keeps each wake after the plan, so nothing saturates here.
        let wakes = hours
            .iter()
            .map(|&(depart, _)| depart.saturating_sub(self.wake_before));
        let returns = hours.iter().map(|&(_, back)| back);
        let bounds = || {
            night
                .from
                .iter()
                .chain(&night.until)
                .chain(&night.until_weekend)
        };
        let in_day = bounds().all(|&time| i64::from(time) < DAY.as_secs());
        let ends = (night.until.iter())
            .all(|&until| self.day_plan_at <= until && wakes.clone().all(|wake| until <= wake));
        let weekend = (night.until_weekend.iter()).all(|&until| self.day_plan_at <= until);
        let begins = (night.from.iter()).all(|&from| returns.clone().all(|back| from > back));
        let bed = self.young.bed_from;
        let young = (night.from.iter()).all(|&from| bed <= from)
            && (night.until.iter().chain(&night.until_weekend)).all(|&until| until <= bed);
        if !(in_day && ends && weekend && begins && young) {
            return Err(HouseError::DayNight);
        }
        Ok(())
    }
}

/// A kind of furniture (ADR-0025 section 1); an offer's number is its place in `offers`. A bed
/// may say whom it is for (ADR-0026, addendum of 2026-11-04 after the playtest): a couple sleeps
/// in a bed for couples, a child in one for children.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct KindDef {
    pub id: DataId,
    pub slots: u8,
    pub offers: Vec<OfferDef>,
    pub bed_for: Option<BedFor>,
}

/// Whom a bed is for in the night's order (ADR-0026, addendum of 2026-11-04 after the
/// playtest).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BedFor {
    Couples,
    Children,
}

impl KindDef {
    /// Whether the kind is the sofa: it sleeps too, but its places are no beds (ADR-0025
    /// section 3).
    pub fn is_sofa(&self) -> bool {
        self.id.as_str() == "core:sofa"
    }

    /// Whether the kind is a bed: a kind with an offer of sleep other than the sofa.
    pub fn is_bed(&self) -> bool {
        !self.is_sofa() && self.offers.iter().any(|offer| offer.sleep)
    }
}

/// An offer of a kind (ADR-0026 section 2): a fixed length in `seconds`, or until the need
/// `until_full` is full but at most `max_seconds`. A sleeping offer has neither: it lasts
/// the night (ADR-0026, addendum of 2026-11-04). It takes the rates of all needs from the
/// sleep column and `rates` names only what differs; any other sets the rates it names and
/// leaves the others awake.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OfferDef {
    pub rates: BTreeMap<Need, i32>,
    pub seconds: Option<u32>,
    pub until_full: Option<Need>,
    pub max_seconds: Option<u32>,
    pub sleep: bool,
    pub min_age: u8,
    pub max_age: u8,
}

impl OfferDef {
    /// The rate of `need` during the offer in the age band `band`.
    pub fn rate(&self, rates: &NeedRates, need: Need, band: usize) -> i32 {
        match self.rates.get(&need) {
            Some(&rate) => rate,
            None if self.sleep => rates.asleep.rate(need, band),
            None => rates.awake.rate(need, band),
        }
    }

    /// Whether the offer sets the rate of `need`: a sleeping offer sets all, any other those
    /// it names. A set need's sample is written down when the action begins and ends.
    pub fn sets(&self, need: Need) -> bool {
        self.sleep || self.rates.contains_key(&need)
    }

    /// Whether the offer is open at `age` and raises `need` there.
    pub fn raises(&self, rates: &NeedRates, need: Need, age: u8) -> bool {
        let open = (self.min_age..=self.max_age).contains(&age);
        open && age_band(age).is_some_and(|band| self.rate(rates, need, band) > 0)
    }

    fn checked(&self, rates: &NeedRates) -> Result<(), HouseError> {
        let length = match (self.sleep, self.seconds, self.until_full, self.max_seconds) {
            (true, None, None, None) => None,
            (false, Some(seconds), None, None) => Some(seconds),
            (false, None, Some(_), Some(max_seconds)) => Some(max_seconds),
            _ => return Err(HouseError::OfferLength),
        };
        if length.is_some_and(|length| !LENGTH_SECS.contains(&length)) {
            return Err(HouseError::OfferLength);
        }
        if self.min_age > self.max_age || self.max_age > OLDEST {
            return Err(HouseError::OfferAges);
        }
        if !self.sleep && self.rates.is_empty() {
            return Err(HouseError::OfferRates);
        }
        // Full only by a need that the offer raises at every age with needs it is open to.
        if let Some(need) = self.until_full {
            let ages = self.min_age.max(AGE_BANDS[0].0)..=self.max_age;
            if !ages.into_iter().all(|age| self.raises(rates, need, age)) {
                return Err(HouseError::OfferFull);
            }
        }
        Ok(())
    }
}

/// A house (ADR-0025 section 1) with its rooms numbered from 0 to `rooms − 1`; an object's
/// number is its place in `objects`. Per room whether one stays there idle (`dwell`), and the
/// room one goes to from a room one does not (`dwell_room`, ADR-0026, addendum of 2026-11-04
/// after the playtest).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HouseDef {
    pub id: DataId,
    pub capacity: u8,
    pub rooms: u8,
    pub entrance: u8,
    pub child_day_room: u8,
    pub child_night_room: u8,
    pub dwell_room: u8,
    pub dwell: Vec<bool>,
    pub doors: Vec<DoorDef>,
    pub objects: Vec<ObjectDef>,
}

/// A door between the rooms `a` and `b`, walked in `seconds` either way.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DoorDef {
    pub a: u8,
    pub b: u8,
    pub seconds: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ObjectDef {
    pub kind: DataId,
    pub room: u8,
}

/// Why house data are not loaded; the code is stable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HouseError {
    /// A time of the day not below 24 h, a departure not before its return, or the small
    /// children's day out of the order bed, kindergarten, bed.
    DayTime,
    /// A wake before a departure that is not after the day plan, or that would fall before
    /// midnight.
    DayWake,
    /// Ages that do not follow each other: small children, school, work.
    DayAge,
    /// Shares of the work plan outside `0 < full time < part time <= 100`.
    DayShare,
    /// A jump on coming home outside −1000 to 0.
    DayJump,
    /// A night with a bound not below 24 h, an end before the day plan or after the earliest
    /// wake, or a start not after the last return.
    DayNight,
    /// What a need at 0 does, out of range: energy that does not rise on the floor, a level to
    /// get up at outside 1 to 1000, an accident's hygiene above 1000, or a change of fun
    /// outside −1000 to 0.
    DayZero,
    /// A mood of more than 50 percent either way.
    DayMood,
    /// Two kinds with one id.
    KindDuplicate,
    /// A kind with no place or more than eight.
    KindSlots,
    /// An offer that does not sleep with neither a fixed length nor a full need with its
    /// longest, or with both, or a length outside 60 s to 12 h; a sleeping offer with any of
    /// them (it lasts the night).
    OfferLength,
    /// An offer whose youngest age is above its oldest, or whose oldest is above 130.
    OfferAges,
    /// An offer full by a need it does not raise at every age it is open to.
    OfferFull,
    /// An offer that sets no rate: not sleeping and naming none.
    OfferRates,
    /// Two houses with one id.
    Duplicate,
    /// A house for nobody, two houses for as many, or one for more than eight.
    Capacity,
    /// An entrance that is no room of the house.
    Entrance,
    /// A door from a room into itself or into a room the house does not have.
    DoorRoom,
    /// Two doors between one pair of rooms.
    DoorDuplicate,
    /// A door of no second or of more than two minutes.
    DoorSeconds,
    /// A room that cannot be walked to from the entrance.
    Disconnected,
    /// A room for small children that the house does not have.
    ChildRoom,
    /// An object of a kind the data do not have.
    ObjectKind,
    /// An object in a room the house does not have.
    ObjectRoom,
    /// More than 32 objects or 64 offers in a house.
    Limit,
    /// A house without a dining table, fridge, toilet, shower or sofa.
    Essentials,
    /// Fewer places in beds than the house's capacity.
    SleepPlaces,
    /// A need at an age with needs that no offer of the house raises.
    NeedUnserved,
    /// A kind that names whom it is for and is no bed, or a bed that offers more than sleep
    /// (ADR-0026, addendum of 2026-11-04 after the playtest).
    BedFor,
    /// A room to go to from a room one does not stay in that the house does not have, that one
    /// does not stay in itself, or that is the entrance; or not one word per room whether one
    /// stays there.
    DwellRoom,
}

impl fmt::Display for HouseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            HouseError::DayTime => "house.day_time",
            HouseError::DayWake => "house.day_wake",
            HouseError::DayAge => "house.day_age",
            HouseError::DayShare => "house.day_share",
            HouseError::DayJump => "house.day_jump",
            HouseError::DayNight => "house.day_night",
            HouseError::DayZero => "house.day_zero",
            HouseError::DayMood => "house.day_mood",
            HouseError::KindDuplicate => "house.kind_duplicate",
            HouseError::KindSlots => "house.kind_slots",
            HouseError::OfferLength => "house.offer_length",
            HouseError::OfferAges => "house.offer_ages",
            HouseError::OfferFull => "house.offer_full",
            HouseError::OfferRates => "house.offer_rates",
            HouseError::Duplicate => "house.duplicate",
            HouseError::Capacity => "house.capacity",
            HouseError::Entrance => "house.entrance",
            HouseError::DoorRoom => "house.door_room",
            HouseError::DoorDuplicate => "house.door_duplicate",
            HouseError::DoorSeconds => "house.door_seconds",
            HouseError::Disconnected => "house.disconnected",
            HouseError::ChildRoom => "house.child_room",
            HouseError::ObjectKind => "house.object_kind",
            HouseError::ObjectRoom => "house.object_room",
            HouseError::Limit => "house.limit",
            HouseError::Essentials => "house.essentials",
            HouseError::SleepPlaces => "house.sleep_places",
            HouseError::NeedUnserved => "house.need_unserved",
            HouseError::BedFor => "house.bed_for",
            HouseError::DwellRoom => "house.dwell_room",
        })
    }
}

/// A checked house with its walking table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct House {
    def: HouseDef,
    /// `seconds[from][to]` and the route between, both ends included (ADR-0025 section 4).
    seconds: Vec<Vec<u32>>,
    routes: Vec<Vec<Vec<u8>>>,
}

impl House {
    pub fn id(&self) -> &DataId {
        &self.def.id
    }

    pub fn capacity(&self) -> u8 {
        self.def.capacity
    }

    pub fn def(&self) -> &HouseDef {
        &self.def
    }

    /// The shortest walk between two rooms; 0 within one.
    pub fn seconds(&self, from: u8, to: u8) -> u32 {
        self.seconds[usize::from(from)][usize::from(to)]
    }

    /// The rooms walked through from `from` to `to`, both included.
    pub fn route(&self, from: u8, to: u8) -> &[u8] {
        &self.routes[usize::from(from)][usize::from(to)]
    }

    /// The walk from a room to an object: 5 s within its room.
    pub fn walk_to(&self, from: u8, object: u16) -> u32 {
        match self.def.objects[usize::from(object)].room {
            to if to == from => WITHIN_ROOM_SECS,
            to => self.seconds(from, to),
        }
    }

    /// The walk from a room out of the house: to the entrance, then 5 s.
    pub fn walk_out(&self, from: u8) -> u32 {
        self.seconds(from, self.def.entrance) + OUT_SECS
    }

    /// Whether one stays idle in `room` (ADR-0026, addendum of 2026-11-04 after the playtest).
    pub fn dwells(&self, room: u8) -> bool {
        self.def.dwell[usize::from(room)]
    }

    /// The walk from a room one does not stay in to the house's `dwell_room`.
    pub fn walk_to_dwell(&self, from: u8) -> u32 {
        self.seconds(from, self.def.dwell_room)
    }

    /// The places of beds in the house's `child_night_room`, by object and then place: the
    /// `n`-th is held by the `n`-th small child in its bed (ADR-0026, addendum of 2026-11-04
    /// after the playtest).
    pub fn child_beds<'a>(&'a self, data: &'a HouseData) -> impl Iterator<Item = (u16, u8)> + 'a {
        (0_u16..)
            .zip(&self.def.objects)
            .filter(|(_, object)| object.room == self.def.child_night_room)
            .filter_map(|(at, object)| Some((at, data.kind(&object.kind)?)))
            .filter(|(_, kind)| kind.is_bed())
            .flat_map(|(at, kind)| (0..kind.slots).map(move |slot| (at, slot)))
    }
}

/// The checked house data: the rates, the rules of the day, the kinds in the order of their
/// ids, the houses from the smallest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HouseData {
    rates: NeedRates,
    day: DayRules,
    kinds: Vec<KindDef>,
    houses: Vec<House>,
}

impl HouseData {
    /// The checks of the day (ADR-0024 section 4), of ADR-0025 section 2 and of the offers'
    /// format (ADR-0026 section 2), then the walking tables.
    pub fn new(
        rates: NeedRates,
        day: DayRules,
        mut kinds: Vec<KindDef>,
        mut defs: Vec<HouseDef>,
    ) -> Result<HouseData, HouseError> {
        day.checked()?;
        kinds.sort_by(|a, b| a.id.cmp(&b.id));
        if kinds.windows(2).any(|pair| pair[0].id == pair[1].id) {
            return Err(HouseError::KindDuplicate);
        }
        for kind in &kinds {
            if !SLOTS.contains(&kind.slots) {
                return Err(HouseError::KindSlots);
            }
            for offer in &kind.offers {
                offer.checked(&rates)?;
            }
            // A bed only sleeps, so a small child's hold of its place in the night meets no
            // other action on it; only a bed says whom it is for.
            let sleeps_only = kind.offers.iter().all(|offer| offer.sleep);
            if (kind.is_bed() && !sleeps_only) || (kind.bed_for.is_some() && !kind.is_bed()) {
                return Err(HouseError::BedFor);
            }
        }
        defs.sort_by(|a, b| a.id.cmp(&b.id));
        if defs.windows(2).any(|pair| pair[0].id == pair[1].id) {
            return Err(HouseError::Duplicate);
        }
        let mut capacities: Vec<u8> = defs.iter().map(|def| def.capacity).collect();
        capacities.sort_unstable();
        let shared = capacities.windows(2).any(|pair| pair[0] == pair[1]);
        let outside = (capacities.iter()).any(|&capacity| capacity == 0 || capacity > MAX_CAPACITY);
        if shared || outside {
            return Err(HouseError::Capacity);
        }
        let mut houses = (defs.into_iter())
            .map(|def| checked_house(def, &kinds, &rates))
            .collect::<Result<Vec<House>, HouseError>>()?;
        houses.sort_by_key(House::capacity);
        Ok(HouseData {
            rates,
            day,
            kinds,
            houses,
        })
    }

    pub fn rates(&self) -> &NeedRates {
        &self.rates
    }

    pub fn day(&self) -> &DayRules {
        &self.day
    }

    /// The kinds in the order of their ids.
    pub fn kinds(&self) -> &[KindDef] {
        &self.kinds
    }

    /// The houses in the order of their ids.
    pub fn defs(&self) -> Vec<&HouseDef> {
        let mut defs: Vec<&HouseDef> = self.houses.iter().map(House::def).collect();
        defs.sort_by(|a, b| a.id.cmp(&b.id));
        defs
    }

    pub fn kind(&self, id: &DataId) -> Option<&KindDef> {
        let at = self.kinds.binary_search_by(|kind| kind.id.cmp(id)).ok()?;
        Some(&self.kinds[at])
    }

    pub fn house(&self, id: &DataId) -> Option<&House> {
        self.houses.iter().find(|house| house.id() == id)
    }

    /// The house with the smallest capacity that holds `members` (ADR-0025 section 3).
    pub fn house_for(&self, members: usize) -> Option<&House> {
        (self.houses.iter()).find(|house| usize::from(house.capacity()) >= members)
    }
}

/// One house checked against the kinds, with its walking table.
fn checked_house(def: HouseDef, kinds: &[KindDef], rates: &NeedRates) -> Result<House, HouseError> {
    let is_room = |room: u8| room < def.rooms;
    if !is_room(def.entrance) {
        return Err(HouseError::Entrance);
    }
    let mut adjacent: Vec<Vec<(usize, u32, usize)>> = vec![Vec::new(); usize::from(def.rooms)];
    let mut pairs = Vec::new();
    for (number, door) in def.doors.iter().enumerate() {
        if door.a == door.b || !is_room(door.a) || !is_room(door.b) {
            return Err(HouseError::DoorRoom);
        }
        if !DOOR_SECS.contains(&door.seconds) {
            return Err(HouseError::DoorSeconds);
        }
        pairs.push((door.a.min(door.b), door.a.max(door.b)));
        let (a, b) = (usize::from(door.a), usize::from(door.b));
        adjacent[a].push((b, door.seconds, number));
        adjacent[b].push((a, door.seconds, number));
    }
    pairs.sort_unstable();
    if pairs.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(HouseError::DoorDuplicate);
    }
    let walks: Vec<Vec<(u32, Vec<u8>)>> = (0..adjacent.len())
        .map(|from| walks_from(from, &adjacent))
        .collect::<Option<_>>()
        .ok_or(HouseError::Disconnected)?;
    if !is_room(def.child_day_room) || !is_room(def.child_night_room) {
        return Err(HouseError::ChildRoom);
    }
    // One goes from a room one does not stay in to a room one stays in, other than the entrance:
    // a walk without an action leads out of the house only to the entrance.
    let one_each = def.dwell.len() == usize::from(def.rooms);
    let stays = one_each && is_room(def.dwell_room) && def.dwell[usize::from(def.dwell_room)];
    if !stays || def.dwell_room == def.entrance {
        return Err(HouseError::DwellRoom);
    }

    let mut objects: Vec<&KindDef> = Vec::new();
    for object in &def.objects {
        let at = kinds.binary_search_by(|kind| kind.id.cmp(&object.kind));
        objects.push(&kinds[at.map_err(|_| HouseError::ObjectKind)?]);
        if !is_room(object.room) {
            return Err(HouseError::ObjectRoom);
        }
    }
    let offers: usize = objects.iter().map(|kind| kind.offers.len()).sum();
    if objects.len() > MAX_OBJECTS || offers > MAX_OFFERS {
        return Err(HouseError::Limit);
    }
    let essentials = [
        "core:dining_table",
        "core:fridge",
        "core:toilet",
        "core:shower",
        "core:sofa",
    ];
    let has = |id| objects.iter().any(|kind| kind.id.as_str() == id);
    if !essentials.into_iter().all(has) {
        return Err(HouseError::Essentials);
    }
    // The sofa sleeps too, but its places are no beds.
    let beds: usize = (objects.iter())
        .filter(|kind| kind.is_bed())
        .map(|kind| usize::from(kind.slots))
        .sum();
    if beds < usize::from(def.capacity) {
        return Err(HouseError::SleepPlaces);
    }
    let all_offers = || objects.iter().flat_map(|kind| kind.offers.iter());
    let served = |need: Need, age: u8| all_offers().any(|offer| offer.raises(rates, need, age));
    let ages = AGE_BANDS[0].0..=OLDEST;
    if !(NEEDS.iter()).all(|&need| ages.clone().all(|age| served(need, age))) {
        return Err(HouseError::NeedUnserved);
    }

    let (seconds, routes) = walks.into_iter().map(|row| row.into_iter().unzip()).unzip();
    Ok(House {
        def,
        seconds,
        routes,
    })
}

/// The shortest walks from the room `from` to every room (ADR-0025 section 4): the seconds
/// and the rooms walked through, a tie going to the lexicographically smaller sequence of
/// door numbers; `None` if a room cannot be reached. `adjacent` holds per room (at most 255)
/// its neighbours with the door's seconds and number.
fn walks_from(from: usize, adjacent: &[Vec<(usize, u32, usize)>]) -> Option<Vec<(u32, Vec<u8>)>> {
    // Dijkstra on the label (seconds, doors): appending one door keeps two labels of a room
    // in their order, so the smallest label is final when it is taken.
    let rooms = adjacent.len();
    let mut best: Vec<Option<(u32, Vec<usize>)>> = vec![None; rooms];
    let mut routes: Vec<Vec<usize>> = vec![Vec::new(); rooms];
    let mut done = vec![false; rooms];
    best[from] = Some((0, Vec::new()));
    routes[from] = vec![from];
    while let Some(at) = (0..rooms)
        .filter(|&at| !done[at])
        .filter_map(|at| best[at].as_ref().map(|label| (label, at)))
        .min_by(|a, b| a.0.cmp(b.0))
        .map(|(_, at)| at)
    {
        done[at] = true;
        let (seconds, doors) = best[at].clone()?;
        for &(next, door_seconds, door) in &adjacent[at] {
            let mut through = doors.clone();
            through.push(door);
            let label = (seconds + door_seconds, through);
            if !done[next] && best[next].as_ref().is_none_or(|old| label < *old) {
                best[next] = Some(label);
                routes[next] = routes[at].iter().copied().chain([next]).collect();
            }
        }
    }
    (best.into_iter().zip(routes))
        .map(|(label, route)| {
            let route = route.into_iter().map(|at| u8::try_from(at).ok());
            Some((label?.0, route.collect::<Option<Vec<u8>>>()?))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Four rooms with a tie, hall to bath through the kitchen (doors 0 and 3) or the living
    /// room (doors 1 and 2), 7 s either way: one walk ends in the room it began in.
    #[test]
    fn a_tie_goes_to_the_smaller_doors() {
        let mut adjacent = vec![Vec::new(); 4];
        for (number, (a, b, seconds)) in [(0, 1, 5), (0, 2, 3), (2, 3, 4), (1, 3, 2)]
            .into_iter()
            .enumerate()
        {
            adjacent[a].push((b, seconds, number));
            adjacent[b].push((a, seconds, number));
        }
        let from_hall = walks_from(0, &adjacent).expect("connected");
        assert_eq!(from_hall[3], (7, vec![0, 1, 3]));
        assert_eq!(from_hall[0], (0, vec![0]));
        let from_bath = walks_from(3, &adjacent).expect("connected");
        assert_eq!(from_bath[0], (7, vec![3, 2, 0]));
        adjacent[3].clear();
        adjacent[1].retain(|&(next, _, _)| next != 3);
        adjacent[2].retain(|&(next, _, _)| next != 3);
        assert_eq!(walks_from(0, &adjacent), None);
    }
}

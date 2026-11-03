//! The houses as data (ADR-0025 sections 1 to 4) with the kinds of furniture and their offers
//! (ADR-0026 section 2) and the rates of the needs (ADR-0026 section 1): their types, their
//! checks on load, the walking table of each house and the house for a household of a size.
//! The files name rooms and offers; their reader turns the names into numbers, so the core
//! holds no text (ADR-0004 section 7) and knows a room by its number (ADR-0025 section 5).

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
use core::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

use crate::data_id::DataId;

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

/// A kind of furniture (ADR-0025 section 1); an offer's number is its place in `offers`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct KindDef {
    pub id: DataId,
    pub slots: u8,
    pub offers: Vec<OfferDef>,
}

/// An offer of a kind (ADR-0026 section 2): a fixed length in `seconds`, or until the need
/// `until_full` is full but at most `max_seconds`. A sleeping offer takes the rates of all
/// needs from the sleep column and `rates` names only what differs; any other sets the rates
/// it names and leaves the others awake.
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
        let length = match (self.seconds, self.until_full, self.max_seconds) {
            (Some(seconds), None, None) => seconds,
            (None, Some(_), Some(max_seconds)) => max_seconds,
            _ => return Err(HouseError::OfferLength),
        };
        if !LENGTH_SECS.contains(&length) {
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
/// number is its place in `objects`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HouseDef {
    pub id: DataId,
    pub capacity: u8,
    pub rooms: u8,
    pub entrance: u8,
    pub child_day_room: u8,
    pub child_night_room: u8,
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
    /// Two kinds with one id.
    KindDuplicate,
    /// A kind with no place or more than eight.
    KindSlots,
    /// An offer with neither a fixed length nor a full need with its longest, or with both,
    /// or a length outside 60 s to 12 h.
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
}

impl fmt::Display for HouseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
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
}

/// The checked house data: the rates, the kinds in the order of their ids, the houses from
/// the smallest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HouseData {
    rates: NeedRates,
    kinds: Vec<KindDef>,
    houses: Vec<House>,
}

impl HouseData {
    /// The checks of ADR-0025 section 2 and of the offers' format (ADR-0026 section 2), then
    /// the walking tables.
    pub fn new(
        rates: NeedRates,
        mut kinds: Vec<KindDef>,
        mut defs: Vec<HouseDef>,
    ) -> Result<HouseData, HouseError> {
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
            kinds,
            houses,
        })
    }

    pub fn rates(&self) -> &NeedRates {
        &self.rates
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
        .filter(|kind| kind.id.as_str() != "core:sofa" && kind.offers.iter().any(|o| o.sleep))
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

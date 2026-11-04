//! The house files (ADR-0025 section 1, ADR-0026 sections 1 and 2, ADR-0024 section 4):
//! `needs.json`, `day.json`, `kinds.json` and one file per house. They name rooms and offers;
//! this reader checks the names (each once, each named one there, one entrance) and hands the
//! core numbers, which checks the rest.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sim_core::data_id::DataId;
use sim_core::house::data::{
    BedFor, DayRules, DoorDef, HouseData, HouseDef, KindDef, Need, NeedRates, ObjectDef, OfferDef,
};

use crate::error::CliError;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KindFile {
    id: DataId,
    slots: u8,
    offers: Vec<OfferFile>,
    #[serde(default)]
    bed_for: Option<BedFor>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OfferFile {
    id: String,
    #[serde(default)]
    rates: BTreeMap<Need, i32>,
    seconds: Option<u32>,
    until_full: Option<Need>,
    max_seconds: Option<u32>,
    #[serde(default)]
    sleep: bool,
    min_age: u8,
    max_age: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HouseFile {
    id: DataId,
    capacity: u8,
    rooms: Vec<RoomFile>,
    child_day_room: String,
    child_night_room: String,
    dwell_room: String,
    doors: Vec<DoorFile>,
    objects: Vec<ObjectFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RoomFile {
    id: String,
    #[serde(default)]
    entrance: bool,
    /// Whether one stays idle in the room; one does unless it says not.
    #[serde(default = "stays")]
    dwell: bool,
}

fn stays() -> bool {
    true
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DoorFile {
    a: String,
    b: String,
    seconds: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ObjectFile {
    kind: DataId,
    room: String,
}

/// The checked house data from the text of `needs.json`, of `day.json`, of `kinds.json` (a
/// list of kinds) and of each house file with its name.
pub fn house_data_from(
    needs: &str,
    day: &str,
    kinds: &str,
    houses: &[(&str, &str)],
) -> Result<HouseData, CliError> {
    let rates: NeedRates = parsed("needs.json", needs)?;
    let day: DayRules = parsed("day.json", day)?;
    let kinds: Vec<KindFile> = parsed("kinds.json", kinds)?;
    let kinds = (kinds.into_iter())
        .map(|kind| {
            let mut names: Vec<&str> = kind.offers.iter().map(|o| o.id.as_str()).collect();
            names.sort_unstable();
            if names.windows(2).any(|pair| pair[0] == pair[1]) {
                let detail = format!("kinds.json: {}: house.offer_duplicate", kind.id.as_str());
                return Err(rejected(&detail));
            }
            let offers = (kind.offers.into_iter())
                .map(|offer| OfferDef {
                    rates: offer.rates,
                    seconds: offer.seconds,
                    until_full: offer.until_full,
                    max_seconds: offer.max_seconds,
                    sleep: offer.sleep,
                    min_age: offer.min_age,
                    max_age: offer.max_age,
                })
                .collect();
            Ok(KindDef {
                id: kind.id,
                slots: kind.slots,
                offers,
                bed_for: kind.bed_for,
            })
        })
        .collect::<Result<Vec<_>, CliError>>()?;
    let houses = (houses.iter())
        .map(|&(name, text)| house_def(name, parsed(name, text)?))
        .collect::<Result<Vec<_>, CliError>>()?;
    HouseData::new(rates, day, kinds, houses)
        .map_err(|error| rejected(&format!("house data: {error}")))
}

/// The house with its rooms numbered in the file's order.
fn house_def(name: &str, file: HouseFile) -> Result<HouseDef, CliError> {
    let refused = |code: &str| rejected(&format!("{name}: {code}"));
    let mut names: Vec<&str> = file.rooms.iter().map(|room| room.id.as_str()).collect();
    names.sort_unstable();
    if names.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(refused("house.room_duplicate"));
    }
    let rooms = u8::try_from(file.rooms.len()).map_err(|_| refused("house.limit"))?;
    let room = |id: &str, code: &str| {
        let at = file.rooms.iter().position(|room| room.id == id);
        at.and_then(|at| u8::try_from(at).ok())
            .ok_or_else(|| refused(code))
    };
    let mut entrances = (0..file.rooms.len()).filter(|&at| file.rooms[at].entrance);
    let entrance = match (entrances.next(), entrances.next()) {
        (Some(at), None) => u8::try_from(at).map_err(|_| refused("house.limit"))?,
        _ => return Err(refused("house.entrance")),
    };
    let doors = (file.doors.iter())
        .map(|door| {
            Ok(DoorDef {
                a: room(&door.a, "house.door_room")?,
                b: room(&door.b, "house.door_room")?,
                seconds: door.seconds,
            })
        })
        .collect::<Result<_, CliError>>()?;
    let objects = (file.objects.iter())
        .map(|object| {
            Ok(ObjectDef {
                kind: object.kind.clone(),
                room: room(&object.room, "house.object_room")?,
            })
        })
        .collect::<Result<_, CliError>>()?;
    Ok(HouseDef {
        child_day_room: room(&file.child_day_room, "house.child_room")?,
        child_night_room: room(&file.child_night_room, "house.child_room")?,
        dwell_room: room(&file.dwell_room, "house.dwell_room")?,
        dwell: file.rooms.iter().map(|room| room.dwell).collect(),
        id: file.id,
        capacity: file.capacity,
        rooms,
        entrance,
        doors,
        objects,
    })
}

fn parsed<T: serde::de::DeserializeOwned>(name: &str, text: &str) -> Result<T, CliError> {
    serde_json::from_str(text).map_err(|error| rejected(&format!("{name}: {error}")))
}

fn rejected(detail: &str) -> CliError {
    CliError::new("data_rejected", detail)
}

/// The drawing of a house (ADR-0025 section 1), `<name>.layout.json`: its rooms in the order
/// of the house file, its doors, its objects in the order of the house file, with rectangles
/// and points in metres from the top left. The core never reads it and it is not in the data
/// hash; `simagent view` hands it to the house page.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    pub house: DataId,
    pub rooms: Vec<RoomRect>,
    pub doors: Vec<DoorPoint>,
    pub objects: Vec<ObjectRect>,
}

/// A room on its floor (0 the ground floor).
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoomRect {
    pub id: String,
    pub floor: u8,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// A door: on the shared edge of its rooms, or a staircase between two floors, its point in
/// both.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DoorPoint {
    pub a: String,
    pub b: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectRect {
    pub kind: DataId,
    pub room: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// How far a drawing may miss an edge, in metres.
const SLACK: f64 = 0.001;
/// Every number of a drawing lies from 0 to this, in metres (review: far out, a rectangle's
/// end rounds back onto its start).
const FARTHEST: f64 = 100.0;
/// The smallest side of a room or an object, in metres (review: a side far below the slack
/// adds nothing to its start).
const SMALLEST: f64 = 0.01;
/// How far a door between two rooms of a floor stays from either end of their shared wall.
const DOOR_CLEARANCE: f64 = 0.4;

/// The wall two rooms of one floor share, as (constant x, at, from, to): at `at` from `from`
/// to `to` along the other axis, longer than the slack.
fn shared_wall(a: &RoomRect, b: &RoomRect) -> Option<(bool, f64, f64, f64)> {
    let near = |p: f64, q: f64| (p - q).abs() <= SLACK;
    let along = |a0: f64, a1: f64, b0: f64, b1: f64| {
        let from = if a0 > b0 { a0 } else { b0 };
        let to = if a1 < b1 { a1 } else { b1 };
        (to - from > SLACK).then_some((from, to))
    };
    if near(a.x + a.w, b.x) || near(b.x + b.w, a.x) {
        let at = if near(a.x + a.w, b.x) { b.x } else { a.x };
        along(a.y, a.y + a.h, b.y, b.y + b.h).map(|(from, to)| (true, at, from, to))
    } else if near(a.y + a.h, b.y) || near(b.y + b.h, a.y) {
        let at = if near(a.y + a.h, b.y) { b.y } else { a.y };
        along(a.x, a.x + a.w, b.x, b.x + b.w).map(|(from, to)| (false, at, from, to))
    } else {
        None
    }
}

/// The drawing `text` (named `name` in messages) checked against the text of its house file:
/// the same house, the same rooms in order, each side at least 1 cm; every number from 0 to
/// 100 m;
/// no two rooms of a floor overlap; the same doors, each between two rooms of a floor on
/// their shared wall, at least 0.4 m from its ends, or a staircase between floors with its
/// point in both rooms; the same objects in order, each with an area inside its room
/// (ADR-0025 section 2, the test in `sim-cli`).
pub fn layout_from(name: &str, text: &str, house: &str) -> Result<Layout, CliError> {
    let refused = |code: &str| rejected(&format!("{name}: {code}"));
    let layout: Layout = parsed(name, text)?;
    let house: HouseFile = parsed(name, house)?;
    if layout.house != house.id {
        return Err(refused("layout.house"));
    }
    let names = |rooms: &[RoomRect]| rooms.iter().map(|room| room.id.clone()).collect::<Vec<_>>();
    let house_rooms: Vec<String> = house.rooms.iter().map(|room| room.id.clone()).collect();
    if names(&layout.rooms) != house_rooms {
        return Err(refused("layout.rooms"));
    }
    let sizes = (layout.rooms.iter().map(|room| (room.w, room.h)))
        .chain(layout.objects.iter().map(|object| (object.w, object.h)));
    if sizes.into_iter().any(|(w, h)| w < SMALLEST || h < SMALLEST) {
        return Err(refused("layout.size"));
    }
    let ends = (layout
        .rooms
        .iter()
        .map(|r| (r.x, r.y, r.x + r.w, r.y + r.h)))
    .chain(
        layout
            .objects
            .iter()
            .map(|o| (o.x, o.y, o.x + o.w, o.y + o.h)),
    )
    .chain(layout.doors.iter().map(|d| (d.x, d.y, d.x, d.y)));
    let inside = |v: f64| (0.0..=FARTHEST).contains(&v);
    if !ends
        .into_iter()
        .all(|(x0, y0, x1, y1)| [x0, y0, x1, y1].into_iter().all(inside))
    {
        return Err(refused("layout.range"));
    }
    for (at, a) in layout.rooms.iter().enumerate() {
        for b in &layout.rooms[at + 1..] {
            let apart = a.x + a.w <= b.x + SLACK
                || b.x + b.w <= a.x + SLACK
                || a.y + a.h <= b.y + SLACK
                || b.y + b.h <= a.y + SLACK;
            if a.floor == b.floor && !apart {
                return Err(refused("layout.overlap"));
            }
        }
    }
    let pair = |a: &str, b: &str| {
        if a <= b {
            (a.to_owned(), b.to_owned())
        } else {
            (b.to_owned(), a.to_owned())
        }
    };
    let mut drawn: Vec<_> = layout
        .doors
        .iter()
        .map(|door| pair(&door.a, &door.b))
        .collect();
    let mut joined: Vec<_> = house
        .doors
        .iter()
        .map(|door| pair(&door.a, &door.b))
        .collect();
    drawn.sort();
    joined.sort();
    if drawn != joined {
        return Err(refused("layout.doors"));
    }
    let room = |id: &str| layout.rooms.iter().find(|room| room.id == id);
    let holds = |room: &RoomRect, x: f64, y: f64| {
        room.x - SLACK <= x
            && x <= room.x + room.w + SLACK
            && room.y - SLACK <= y
            && y <= room.y + room.h + SLACK
    };
    for door in &layout.doors {
        let placed = match (room(&door.a), room(&door.b)) {
            (Some(a), Some(b)) if a.floor == b.floor => {
                shared_wall(a, b).is_some_and(|(constant_x, at, from, to)| {
                    let (across, along) = if constant_x {
                        (door.x, door.y)
                    } else {
                        (door.y, door.x)
                    };
                    (across - at).abs() <= SLACK
                        && from + DOOR_CLEARANCE <= along + SLACK
                        && along <= to - DOOR_CLEARANCE + SLACK
                })
            }
            (Some(a), Some(b)) => holds(a, door.x, door.y) && holds(b, door.x, door.y),
            _ => false,
        };
        if !placed {
            return Err(refused("layout.door_place"));
        }
    }
    let objects = |objects: &[ObjectRect]| {
        objects
            .iter()
            .map(|object| (object.kind.clone(), object.room.clone()))
            .collect::<Vec<_>>()
    };
    let house_objects: Vec<_> = (house.objects.iter())
        .map(|object| (object.kind.clone(), object.room.clone()))
        .collect();
    if objects(&layout.objects) != house_objects {
        return Err(refused("layout.objects"));
    }
    for object in &layout.objects {
        let fits = room(&object.room).is_some_and(|room| {
            holds(room, object.x, object.y) && holds(room, object.x + object.w, object.y + object.h)
        });
        if !fits {
            return Err(refused("layout.object_place"));
        }
    }
    Ok(layout)
}

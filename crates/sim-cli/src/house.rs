//! The house files (ADR-0025 section 1, ADR-0026 sections 1 and 2): `needs.json`,
//! `kinds.json` and one file per house. They name rooms and offers; this reader checks the
//! names (each once, each named one there, one entrance) and hands the core numbers, which
//! checks the rest.

use std::collections::BTreeMap;

use serde::Deserialize;
use sim_core::data_id::DataId;
use sim_core::house::data::{
    DoorDef, HouseData, HouseDef, KindDef, Need, NeedRates, ObjectDef, OfferDef,
};

use crate::error::CliError;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KindFile {
    id: DataId,
    slots: u8,
    offers: Vec<OfferFile>,
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
    doors: Vec<DoorFile>,
    objects: Vec<ObjectFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RoomFile {
    id: String,
    #[serde(default)]
    entrance: bool,
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

/// The checked house data from the text of `needs.json`, of `kinds.json` (a list of kinds)
/// and of each house file with its name.
pub fn house_data_from(
    needs: &str,
    kinds: &str,
    houses: &[(&str, &str)],
) -> Result<HouseData, CliError> {
    let rates: NeedRates = parsed("needs.json", needs)?;
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
            })
        })
        .collect::<Result<Vec<_>, CliError>>()?;
    let houses = (houses.iter())
        .map(|&(name, text)| house_def(name, parsed(name, text)?))
        .collect::<Result<Vec<_>, CliError>>()?;
    HouseData::new(rates, kinds, houses).map_err(|error| rejected(&format!("house data: {error}")))
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

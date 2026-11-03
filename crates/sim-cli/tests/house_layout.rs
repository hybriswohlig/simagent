//! The drawings of the houses (ADR-0025 sections 1 and 2): `<name>.layout.json` names each
//! room of its house in the house file's order, each door once and each object in the house
//! file's order, with rectangles and points in metres from 0 to 100. Rooms of one floor do not
//! overlap, a door lies on the shared wall of its two rooms of a floor at least 0.4 m from its
//! ends, or is a staircase between floors with its point in both, and an object lies in its
//! room.

use serde_json::{Value, json};
use sim_cli::data::layouts;
use sim_cli::house::layout_from;
use sim_core::data_id::DataId;

const FLAT: &str = include_str!("../../../data/house/houses/flat.json");
const FLAT_LAYOUT: &str = include_str!("../../../data/house/houses/flat.layout.json");

/// The repository's drawings fit their houses (the planned test `layout_covers_house`).
#[test]
fn layout_covers_house() {
    let layouts = layouts().expect("the drawings fit");
    let ids: Vec<&str> = layouts.keys().map(DataId::as_str).collect();
    assert_eq!(ids, ["core:flat", "core:house", "core:terraced"]);
    let flat = &layouts[&DataId::new("core:flat").expect("id")];
    assert_eq!(flat.rooms.len(), 6);
    assert_eq!(flat.doors.len(), 5);
    assert_eq!(flat.objects.len(), 10);
    // The terraced house and the house have two floors.
    for id in ["core:terraced", "core:house"] {
        let layout = &layouts[&DataId::new(id).expect("id")];
        let floors: Vec<u8> = layout.rooms.iter().map(|room| room.floor).collect();
        assert!(floors.contains(&0) && floors.contains(&1), "{id}");
    }
}

/// The flat's drawing changed by `change`: "ok" or the code of its refusal.
fn checked(change: impl FnOnce(&mut Value)) -> String {
    let mut layout: Value = serde_json::from_str(FLAT_LAYOUT).expect("json");
    change(&mut layout);
    match layout_from("flat.layout.json", &layout.to_string(), FLAT) {
        Ok(_) => "ok".to_owned(),
        Err(error) => {
            let code = error.detail.rsplit(": ").next().unwrap_or_default();
            code.to_owned()
        }
    }
}

#[test]
fn a_drawing_is_checked() {
    assert_eq!(checked(|_| {}), "ok");
    // Another house, a room missing, the rooms in another order.
    assert_eq!(
        checked(|l| l["house"] = json!("core:house")),
        "layout.house"
    );
    let missing = checked(|l| {
        l["rooms"].as_array_mut().expect("rooms").pop();
    });
    assert_eq!(missing, "layout.rooms");
    assert_eq!(
        checked(|l| l["rooms"].as_array_mut().expect("rooms").swap(1, 2)),
        "layout.rooms"
    );
    // A room or an object without area.
    assert_eq!(checked(|l| l["rooms"][1]["w"] = json!(0)), "layout.size");
    assert_eq!(checked(|l| l["objects"][0]["h"] = json!(-1)), "layout.size");
    // Two rooms of one floor overlapping: the kitchen 1 cm into the hall; on another floor
    // the same rectangle is fine, but then its door is no longer on a shared edge.
    assert_eq!(
        checked(|l| l["rooms"][1]["w"] = json!(4.01)),
        "layout.overlap"
    );
    assert_eq!(
        checked(|l| {
            l["rooms"][1]["floor"] = json!(1);
            l["doors"][0]["x"] = json!(4.5);
        }),
        "layout.door_place"
    );
    // A door missing, doubled, or between rooms the house does not join.
    let doors = |l: &mut Value| l["doors"].as_array_mut().expect("doors").clone();
    assert_eq!(
        checked(|l| {
            l["doors"].as_array_mut().expect("doors").pop();
        }),
        "layout.doors"
    );
    assert_eq!(
        checked(|l| {
            let first = doors(l)[0].clone();
            l["doors"].as_array_mut().expect("doors").push(first);
        }),
        "layout.doors"
    );
    assert_eq!(
        checked(|l| l["doors"][0]["b"] = json!("bath")),
        "layout.doors"
    );
    // The same door with its rooms swapped is the same door.
    assert_eq!(
        checked(|l| {
            l["doors"][0]["a"] = json!("kitchen");
            l["doors"][0]["b"] = json!("hall");
        }),
        "ok"
    );
    // A door off the shared wall, at its end (review: a corner), or closer than 0.4 m to it;
    // 0.4 m is fine.
    let door_at = |y: f64| checked(move |l| l["doors"][0]["y"] = json!(y));
    assert_eq!(door_at(4.5), "layout.door_place");
    assert_eq!(door_at(0.0), "layout.door_place");
    assert_eq!(door_at(3.7), "layout.door_place");
    assert_eq!(door_at(3.6), "ok");
    // Two rooms that touch only at a corner share no wall: the bath moved below and right of
    // the hall, its door to the hall at their corner.
    assert_eq!(
        checked(|l| {
            l["rooms"][3]["x"] = json!(6);
            l["rooms"][3]["y"] = json!(10);
            l["doors"][2]["x"] = json!(6);
            l["doors"][2]["y"] = json!(10);
        }),
        "layout.door_place"
    );
    // Every number from 0 to 100 m (review: at 1e20 a rectangle's end rounds back onto its
    // start, and every check would pass).
    assert_eq!(checked(|l| l["rooms"][0]["x"] = json!(-1)), "layout.range");
    assert_eq!(checked(|l| l["rooms"][2]["w"] = json!(95)), "layout.range");
    assert_eq!(
        checked(|l| l["doors"][1]["x"] = json!(1e20)),
        "layout.range"
    );
    // An object missing, of another kind, in another room, or outside its room.
    assert_eq!(
        checked(|l| {
            l["objects"].as_array_mut().expect("objects").pop();
        }),
        "layout.objects"
    );
    assert_eq!(
        checked(|l| l["objects"][0]["kind"] = json!("core:fridge")),
        "layout.objects"
    );
    assert_eq!(
        checked(|l| l["objects"][0]["room"] = json!("living")),
        "layout.objects"
    );
    assert_eq!(
        checked(|l| l["objects"][0]["x"] = json!(3)),
        "layout.object_place"
    );
    // Unknown fields are refused like in every data file.
    let unknown = checked(|l| l["rooms"][0]["colour"] = json!("red"));
    assert!(unknown.contains("unknown field"), "{unknown}");
}

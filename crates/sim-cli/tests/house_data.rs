//! ADR-0025 sections 1 to 4 with the offers of ADR-0026 section 2 and the day of ADR-0024
//! section 4, through the reader of the house files: three houses load, each check of the
//! reader and of the core rejects a broken file with its code, the walking table of a test
//! house by hand, and the house by the size of a household.

use serde_json::{Value, json};
use sim_cli::house::house_data_from;
use sim_core::data_id::DataId;
use sim_core::house::data::{
    DayRules, Hours, HouseData, HouseError, NeedJumps, NightRules, SchoolRules, WorkRules,
    YoungRules, ZeroRules,
};
use sim_core::house::reserve::{Drain, ReserveRules};

/// The repository's `day.json` (ADR-0024 section 4).
const DAY: &str = include_str!("../../../data/house/day.json");

fn day() -> Value {
    serde_json::from_str(DAY).expect("the day")
}

/// The rates of ADR-0026 section 1 per age band (6 to 12, 13 to 17, 18 to 66, from 67), the
/// sleeping energy of its addendum of 2026-11-04, and each need's comfort of its addendum of
/// 2026-11-04 on comfort and the time-filler.
fn rates() -> Value {
    json!({
        "awake": {
            "hunger": [-100, -100, -100, -100],
            "energy": [-50, -45, -40, -45],
            "bladder": [-120, -120, -120, -120],
            "hygiene": [-30, -30, -30, -30],
            "fun": [-60, -60, -60, -60]
        },
        "asleep": {
            "hunger": [-40, -40, -40, -40],
            "energy": [80, 70, 85, 90],
            "bladder": [-40, -40, -40, -40],
            "hygiene": [-10, -10, -10, -10],
            "fun": [0, 0, 0, 0]
        },
        "comfort": [700, 1000, 700, 900, 1000]
    })
}

/// The kinds of ADR-0025 section 1 with the offers of ADR-0026 section 2; sleep lasts the
/// night (its addendum of 2026-11-04).
fn kinds() -> Value {
    let sleep = |energy: Option<i32>| {
        let mut offer = json!({
            "id": "sleep", "sleep": true, "min_age": 0, "max_age": 130
        });
        if let Some(energy) = energy {
            offer["rates"] = json!({ "energy": energy });
        }
        offer
    };
    json!([
        { "id": "core:dining_table", "slots": 4, "offers": [
            { "id": "eat", "rates": { "hunger": 1800 }, "seconds": 1200,
              "min_age": 0, "max_age": 130 } ] },
        { "id": "core:fridge", "slots": 1, "offers": [
            { "id": "snack", "rates": { "hunger": 2400 }, "seconds": 300,
              "min_age": 0, "max_age": 130 } ] },
        { "id": "core:bed", "slots": 2, "offers": [sleep(None)] },
        { "id": "core:single_bed", "slots": 1, "offers": [sleep(None)] },
        { "id": "core:sofa", "slots": 3, "offers": [sleep(Some(50))] },
        { "id": "core:toilet", "slots": 1, "offers": [
            { "id": "use", "rates": { "bladder": 12_000 }, "until_full": "bladder",
              "max_seconds": 300, "min_age": 0, "max_age": 130 } ] },
        { "id": "core:shower", "slots": 1, "offers": [
            { "id": "shower", "rates": { "hygiene": 2800 }, "seconds": 900,
              "min_age": 0, "max_age": 130 } ] },
        { "id": "core:tv", "slots": 3, "offers": [
            { "id": "watch", "rates": { "fun": 400 }, "seconds": 2700,
              "min_age": 0, "max_age": 130 } ] },
        { "id": "core:bookshelf", "slots": 2, "offers": [
            { "id": "read", "rates": { "fun": 300 }, "seconds": 3600,
              "min_age": 0, "max_age": 130 } ] },
        { "id": "core:toy_chest", "slots": 2, "offers": [
            { "id": "play", "rates": { "fun": 600 }, "seconds": 1800,
              "min_age": 6, "max_age": 12 } ] }
    ])
}

/// A house of six rooms whose sleeping places are a double bed and `capacity − 2` single
/// beds.
fn house(id: &str, capacity: u8) -> Value {
    let object = |kind: &str, room: &str| json!({ "kind": kind, "room": room });
    let mut objects = vec![
        object("core:dining_table", "kitchen"),
        object("core:fridge", "kitchen"),
        object("core:sofa", "living"),
        object("core:tv", "living"),
        object("core:bookshelf", "living"),
        object("core:toy_chest", "kids"),
        object("core:toilet", "bath"),
        object("core:shower", "bath"),
        object("core:bed", "bedroom"),
    ];
    objects.extend((2..capacity).map(|_| object("core:single_bed", "kids")));
    json!({
        "id": id,
        "capacity": capacity,
        "rooms": [
            { "id": "hall", "entrance": true }, { "id": "kitchen" }, { "id": "living" },
            { "id": "bath" }, { "id": "bedroom" }, { "id": "kids" }
        ],
        "child_day_room": "kids",
        "child_night_room": "kids",
        "dwell_room": "living",
        "doors": [
            { "a": "hall", "b": "kitchen", "seconds": 4 },
            { "a": "hall", "b": "living", "seconds": 3 },
            { "a": "hall", "b": "bath", "seconds": 3 },
            { "a": "living", "b": "bedroom", "seconds": 4 },
            { "a": "living", "b": "kids", "seconds": 5 }
        ],
        "objects": objects
    })
}

/// The house data of these files, or the code of their rejection (`house.…`), or the whole
/// detail of another (a field the format does not know).
fn load(rates: Value, day: Value, kinds: Value, houses: Vec<Value>) -> Result<HouseData, String> {
    let texts: Vec<(String, String)> = (houses.iter())
        .map(|house| {
            (
                format!("{}.json", house["id"].as_str().unwrap_or_default()),
                house.to_string(),
            )
        })
        .collect();
    let named: Vec<(&str, &str)> = (texts.iter())
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect();
    let (rates, day, kinds) = (rates.to_string(), day.to_string(), kinds.to_string());
    house_data_from(&rates, &day, &kinds, &named).map_err(|error| {
        let code = error.detail.rsplit(": ").next().unwrap_or_default();
        match code.starts_with("house.") {
            true => code.to_owned(),
            false => error.detail,
        }
    })
}

/// The check of the three houses after `change` of the kinds and the flat, as its error
/// code, another rejection's detail or "ok".
fn checked(change: impl FnOnce(&mut Value, &mut Value)) -> String {
    let (mut kinds, mut flat) = (kinds(), house("core:flat", 3));
    change(&mut kinds, &mut flat);
    let houses = vec![flat, house("core:terraced", 5), house("core:house", 8)];
    match load(rates(), day(), kinds, houses) {
        Ok(_) => "ok".to_owned(),
        Err(error) => error,
    }
}

/// The kind with this id in the kinds' list.
fn kind<'a>(kinds: &'a mut Value, id: &str) -> &'a mut Value {
    let list = kinds.as_array_mut().expect("a list");
    list.iter_mut()
        .find(|kind| kind["id"] == id)
        .expect("the kind")
}

fn objects(house: &mut Value) -> &mut Vec<Value> {
    house["objects"].as_array_mut().expect("a list")
}

/// Weak or wasted, energy's rate carries an extra (ADR-0026, addendum of 2026-11-06 on hunger at
/// 0): every energy rate it may be added to, in the needs' columns and the offers' own, takes the
/// wasted −80 within an `i32`, so at least −2 147 483 568; checked after the kinds, before the
/// houses.
#[test]
fn every_energy_rate_takes_the_wasted_extra() {
    let lowest = i64::from(i32::MIN) + 80;
    let column = |column: &'static str, rate: i64| {
        let mut changed = rates();
        changed[column]["energy"][2] = json!(rate);
        let houses = vec![
            house("core:flat", 3),
            house("core:terraced", 5),
            house("core:house", 8),
        ];
        match load(changed, day(), kinds(), houses) {
            Ok(_) => "ok".to_owned(),
            Err(error) => error,
        }
    };
    assert_eq!(column("awake", lowest), "ok");
    assert_eq!(column("awake", lowest - 1), "house.day_reserve");
    assert_eq!(column("asleep", lowest - 1), "house.day_reserve");
    let sofa = |rate: i64| {
        checked(move |k, _| kind(k, "core:sofa")["offers"][0]["rates"]["energy"] = json!(rate))
    };
    assert_eq!(sofa(lowest), "ok");
    assert_eq!(sofa(lowest - 1), "house.day_reserve");
}

#[test]
fn house_data_loads_and_rejects() {
    assert_eq!(checked(|_, _| {}), "ok");

    // Kinds and their offers (ADR-0025 section 1, ADR-0026 section 2).
    let duplicate_kind = checked(|kinds, _| {
        let fridge = kind(kinds, "core:fridge").clone();
        kinds.as_array_mut().expect("a list").push(fridge);
    });
    assert_eq!(duplicate_kind, "house.kind_duplicate");
    assert_eq!(
        checked(|k, _| kind(k, "core:tv")["slots"] = json!(0)),
        "house.kind_slots"
    );
    assert_eq!(
        checked(|k, _| kind(k, "core:tv")["slots"] = json!(9)),
        "house.kind_slots"
    );
    let duplicate_offer = checked(|kinds, _| {
        let tv = kind(kinds, "core:tv");
        let watch = tv["offers"][0].clone();
        tv["offers"].as_array_mut().expect("a list").push(watch);
    });
    assert_eq!(duplicate_offer, "house.offer_duplicate");
    let shower = |change: fn(&mut Value)| {
        checked(move |kinds, _| change(&mut kind(kinds, "core:shower")["offers"][0]))
    };
    let toilet = |change: fn(&mut Value)| {
        checked(move |kinds, _| change(&mut kind(kinds, "core:toilet")["offers"][0]))
    };
    assert_eq!(shower(|o| o["seconds"] = json!(59)), "house.offer_length");
    assert_eq!(shower(|o| o["seconds"] = json!(60)), "ok");
    assert_eq!(shower(|o| o["seconds"] = json!(43_200)), "ok");
    assert_eq!(
        shower(|o| o["seconds"] = json!(43_201)),
        "house.offer_length"
    );
    let no_length = shower(|o| {
        o.as_object_mut().expect("an offer").remove("seconds");
    });
    assert_eq!(no_length, "house.offer_length");
    assert_eq!(
        shower(|o| o["max_seconds"] = json!(900)),
        "house.offer_length"
    );
    let both = shower(|o| {
        o["until_full"] = json!("hygiene");
        o["max_seconds"] = json!(900);
    });
    assert_eq!(both, "house.offer_length");
    let without_max = toilet(|o| {
        o.as_object_mut().expect("an offer").remove("max_seconds");
    });
    assert_eq!(without_max, "house.offer_length");
    assert_eq!(
        toilet(|o| o["max_seconds"] = json!(59)),
        "house.offer_length"
    );
    assert_eq!(
        toilet(|o| o["max_seconds"] = json!(43_201)),
        "house.offer_length"
    );
    assert_eq!(shower(|o| o["min_age"] = json!(131)), "house.offer_ages");
    assert_eq!(shower(|o| o["max_age"] = json!(131)), "house.offer_ages");
    let crossed = shower(|o| {
        o["min_age"] = json!(13);
        o["max_age"] = json!(12);
    });
    assert_eq!(crossed, "house.offer_ages");
    // Full only by a need that the offer raises at every age it is open to: the toilet does
    // not feed, and the shower does not entertain.
    assert_eq!(
        toilet(|o| o["until_full"] = json!("hunger")),
        "house.offer_full"
    );
    let full_of_fun = shower(|o| {
        o.as_object_mut().expect("an offer").remove("seconds");
        o["until_full"] = json!("fun");
        o["max_seconds"] = json!(900);
    });
    assert_eq!(full_of_fun, "house.offer_full");
    // A sleep lasts the night: no length of its own, neither fixed nor until full.
    let bed = |change: fn(&mut Value)| {
        checked(move |kinds, _| change(&mut kind(kinds, "core:bed")["offers"][0]))
    };
    assert_eq!(bed(|o| o["seconds"] = json!(3_600)), "house.offer_length");
    let until_full = bed(|o| {
        o["until_full"] = json!("energy");
        o["max_seconds"] = json!(43_200);
    });
    assert_eq!(until_full, "house.offer_length");
    assert_eq!(
        bed(|o| o["max_seconds"] = json!(43_200)),
        "house.offer_length"
    );
    assert_eq!(
        bed(|o| o["until_full"] = json!("energy")),
        "house.offer_length"
    );
    // Without `sleep` an offer of no length is refused as before.
    assert_eq!(bed(|o| o["sleep"] = json!(false)), "house.offer_length");
    // An offer must set a rate, so its action has a second it began at; sleeping sets all.
    let nothing = shower(|o| {
        o.as_object_mut().expect("an offer").remove("rates");
    });
    assert_eq!(nothing, "house.offer_rates");
    // A time-filler passes the day (ADR-0026, addendum of 2026-11-04 on comfort and the
    // time-filler): fun rises at every age with needs it is open to, and it does not sleep. The
    // television is one; the shower raises no fun, the toy chest only from 6, the bed sleeps.
    let tv = |change: fn(&mut Value)| {
        checked(move |kinds, _| change(&mut kind(kinds, "core:tv")["offers"][0]))
    };
    assert_eq!(tv(|o| o["fallback"] = json!(true)), "ok");
    assert_eq!(tv(|o| o["fallback"] = json!(false)), "ok");
    assert_eq!(
        shower(|o| o["fallback"] = json!(true)),
        "house.offer_fallback"
    );
    let chest =
        checked(|kinds, _| kind(kinds, "core:toy_chest")["offers"][0]["fallback"] = json!(true));
    assert_eq!(chest, "ok");
    let little = checked(|kinds, _| {
        let play = &mut kind(kinds, "core:toy_chest")["offers"][0];
        play["fallback"] = json!(true);
        play["rates"]["hygiene"] = json!(100);
        play["rates"]["fun"] = json!(0);
    });
    assert_eq!(little, "house.offer_fallback");
    assert_eq!(bed(|o| o["fallback"] = json!(true)), "house.offer_fallback");
    assert!(tv(|o| o["fallback"] = json!(1)).contains("invalid type"));
    // A time-filler has a fixed length of its own (ADR-0026, addendum of 2026-11-05 on the
    // time-filler's length): the television until fun is full, at most 45 minutes, is refused;
    // the same as an ordinary offer passes.
    let filling = tv(|o| {
        let offer = o.as_object_mut().expect("an offer");
        offer.remove("seconds");
        offer.insert("until_full".to_owned(), json!("fun"));
        offer.insert("max_seconds".to_owned(), json!(2_700));
        offer.insert("fallback".to_owned(), json!(true));
    });
    let ordinary = tv(|o| {
        let offer = o.as_object_mut().expect("an offer");
        offer.remove("seconds");
        offer.insert("until_full".to_owned(), json!("fun"));
        offer.insert("max_seconds".to_owned(), json!(2_700));
        offer.insert("fallback".to_owned(), json!(false));
    });
    assert_eq!(filling, "house.fallback_length");
    assert_eq!(ordinary, "ok");
    // A need weighs before it is critical: its comfort above 150, at most 1000.
    let comfort = |at: usize, value: i64| {
        let mut changed = rates();
        changed["comfort"][at] = json!(value);
        match load(changed, day(), kinds(), vec![house("core:flat", 3)]) {
            Ok(_) => "ok".to_owned(),
            Err(error) => error,
        }
    };
    assert_eq!(comfort(0, 151), "ok");
    assert_eq!(comfort(0, 150), "house.comfort");
    assert_eq!(comfort(4, 1000), "ok");
    assert_eq!(comfort(4, 1001), "house.comfort");
    assert_eq!(comfort(2, 0), "house.comfort");
    let mut four = rates();
    four["comfort"] = json!([700, 1000, 700, 900]);
    let short = load(four, day(), kinds(), vec![house("core:flat", 3)]);
    assert!(short.expect_err("rejected").contains("invalid length"));
    let mut without = rates();
    without
        .as_object_mut()
        .expect("the rates")
        .remove("comfort");
    let missing = load(without, day(), kinds(), vec![house("core:flat", 3)]);
    assert!(missing.expect_err("rejected").contains("missing field"));
    assert_eq!(HouseError::Comfort.to_string(), "house.comfort");
    assert_eq!(
        HouseError::OfferFallback.to_string(),
        "house.offer_fallback"
    );
    assert_eq!(
        HouseError::FallbackLength.to_string(),
        "house.fallback_length"
    );

    // The houses (ADR-0025 section 2).
    let twice = load(
        rates(),
        day(),
        kinds(),
        vec![house("core:flat", 3), house("core:flat", 5)],
    );
    assert_eq!(twice.err().as_deref(), Some("house.duplicate"));
    let same = load(
        rates(),
        day(),
        kinds(),
        vec![house("core:flat", 3), house("core:big", 3)],
    );
    assert_eq!(same.err().as_deref(), Some("house.capacity"));
    let nine = load(
        rates(),
        day(),
        kinds(),
        vec![house("core:flat", 3), house("core:big", 9)],
    );
    assert_eq!(nine.err().as_deref(), Some("house.capacity"));
    assert_eq!(
        checked(|_, flat| flat["capacity"] = json!(0)),
        "house.capacity"
    );

    let rooms = |change: fn(&mut Vec<Value>)| {
        checked(move |_, flat| change(flat["rooms"].as_array_mut().expect("a list")))
    };
    assert_eq!(
        rooms(|r| r.push(json!({ "id": "kitchen" }))),
        "house.room_duplicate"
    );
    assert_eq!(rooms(|r| r[0]["entrance"] = json!(false)), "house.entrance");
    assert_eq!(rooms(|r| r[1]["entrance"] = json!(true)), "house.entrance");

    let doors = |change: fn(&mut Vec<Value>)| {
        checked(move |_, flat| change(flat["doors"].as_array_mut().expect("a list")))
    };
    assert_eq!(doors(|d| d[0]["b"] = json!("hall")), "house.door_room");
    assert_eq!(doors(|d| d[0]["b"] = json!("garden")), "house.door_room");
    let again = doors(|d| d.push(json!({ "a": "kitchen", "b": "hall", "seconds": 9 })));
    assert_eq!(again, "house.door_duplicate");
    assert_eq!(doors(|d| d[0]["seconds"] = json!(0)), "house.door_seconds");
    assert_eq!(
        doors(|d| d[0]["seconds"] = json!(121)),
        "house.door_seconds"
    );
    assert_eq!(doors(|d| d[0]["seconds"] = json!(120)), "ok");
    assert_eq!(doors(|d| drop(d.remove(4))), "house.disconnected");

    let child_day = checked(|_, flat| flat["child_day_room"] = json!("garden"));
    assert_eq!(child_day, "house.child_room");
    let child_night = checked(|_, flat| flat["child_night_room"] = json!("garden"));
    assert_eq!(child_night, "house.child_room");

    // The room one goes to from a room one does not stay in (ADR-0026, addendum of 2026-11-04
    // after the playtest): a room of the house, one stays in it, not the entrance.
    let dwell = |room: &'static str| checked(move |_, flat| flat["dwell_room"] = json!(room));
    assert_eq!(dwell("garden"), "house.dwell_room");
    assert_eq!(dwell("hall"), "house.dwell_room");
    assert_eq!(dwell("kitchen"), "ok");
    let wet_living = checked(|_, flat| flat["rooms"][2]["dwell"] = json!(false));
    assert_eq!(wet_living, "house.dwell_room");
    assert_eq!(rooms(|r| r[3]["dwell"] = json!(false)), "ok");
    assert!(rooms(|r| r[3]["dwell"] = json!(0)).contains("invalid type"));
    let no_dwell = checked(|_, flat| {
        flat.as_object_mut().expect("a house").remove("dwell_room");
    });
    assert!(no_dwell.contains("missing field"), "{no_dwell}");

    // Whom a bed is for: only a bed names it, and a bed only sleeps.
    let bed_kind = |id: &'static str, change: fn(&mut Value)| {
        checked(move |kinds, _| {
            let kinds = kinds.as_array_mut().expect("the kinds");
            let kind = kinds
                .iter_mut()
                .find(|kind| kind["id"] == id)
                .expect("a kind");
            change(kind);
        })
    };
    assert_eq!(
        bed_kind("core:bed", |k| k["bed_for"] = json!("couples")),
        "ok"
    );
    assert_eq!(
        bed_kind("core:single_bed", |k| k["bed_for"] = json!("children")),
        "ok"
    );
    assert_eq!(
        bed_kind("core:sofa", |k| k["bed_for"] = json!("couples")),
        "house.bed_for"
    );
    assert_eq!(
        bed_kind("core:tv", |k| k["bed_for"] = json!("children")),
        "house.bed_for"
    );
    assert!(bed_kind("core:bed", |k| k["bed_for"] = json!("guests")).contains("unknown variant"));
    let reading_in_bed = bed_kind("core:bed", |k| {
        let read = json!({ "id": "read", "rates": { "fun": 300 }, "seconds": 3600,
            "min_age": 0, "max_age": 130 });
        k["offers"].as_array_mut().expect("the offers").push(read);
    });
    assert_eq!(reading_in_bed, "house.bed_for");

    let piano = checked(|_, flat| objects(flat)[3]["kind"] = json!("core:piano"));
    assert_eq!(piano, "house.object_kind");
    let garden = checked(|_, flat| objects(flat)[3]["room"] = json!("garden"));
    assert_eq!(garden, "house.object_room");

    // At most 32 objects and 64 offers in a house.
    let shelf = json!({ "kind": "core:bookshelf", "room": "living" });
    let shelves = |count: usize| {
        let shelf = shelf.clone();
        checked(move |_, flat| objects(flat).extend((0..count).map(|_| shelf.clone())))
    };
    assert_eq!(shelves(32 - 10), "ok");
    assert_eq!(shelves(33 - 10), "house.limit");
    let consoles = |count: usize| {
        checked(move |kinds, flat| {
            let offer = |id: &str| {
                json!({ "id": id, "rates": { "fun": 500 }, "seconds": 1800,
                        "min_age": 0, "max_age": 130 })
            };
            let console = json!({ "id": "core:console", "slots": 1,
                                  "offers": [offer("race"), offer("jump"), offer("build")] });
            kinds.as_array_mut().expect("a list").push(console);
            let placed = json!({ "kind": "core:console", "room": "kids" });
            objects(flat).extend((0..count).map(|_| placed.clone()));
        })
    };
    // Ten objects with one offer each, then three per console.
    assert_eq!(consoles(18), "ok");
    assert_eq!(consoles(19), "house.limit");

    // Each house can feed, wash, rest and entertain.
    for (at, name) in [
        (0, "table"),
        (1, "fridge"),
        (2, "sofa"),
        (6, "toilet"),
        (7, "shower"),
    ] {
        let without = checked(|_, flat| drop(objects(flat).remove(at)));
        assert_eq!(without, "house.essentials", "{name}");
    }
    // A bed for each member; the sofa does not count.
    assert_eq!(
        checked(|_, flat| drop(objects(flat).remove(9))),
        "house.sleep_places"
    );
    // Fun at 13 needs more than the toy chest of 6 to 12; a television from 20 on leaves
    // 13 to 19 without.
    let no_fun = checked(|_, flat| {
        objects(flat).remove(4);
        objects(flat).remove(3);
    });
    assert_eq!(no_fun, "house.need_unserved");
    let late_tv = checked(|kinds, flat| {
        kind(kinds, "core:tv")["offers"][0]["min_age"] = json!(20);
        objects(flat).remove(4);
    });
    assert_eq!(late_tv, "house.need_unserved");

    // Unknown fields everywhere.
    let unknown = |text: String| text.contains("unknown field");
    assert!(unknown(checked(
        |k, _| kind(k, "core:tv")["colour"] = json!("red")
    )));
    assert!(unknown(checked(
        |k, _| kind(k, "core:tv")["offers"][0]["x"] = json!(1)
    )));
    assert!(unknown(checked(|_, flat| flat["garden"] = json!(true))));
    assert!(unknown(checked(
        |_, flat| flat["rooms"][0]["size"] = json!(9)
    )));
    assert!(unknown(checked(
        |_, flat| flat["doors"][0]["width"] = json!(1)
    )));
    assert!(unknown(checked(|_, flat| objects(flat)[0]["x"] = json!(1))));
    let mut extra = rates();
    extra["awake"]["thirst"] = json!([-1, -1, -1, -1]);
    let thirst = load(extra, day(), kinds(), vec![house("core:flat", 3)]);
    assert!(unknown(thirst.expect_err("rejected")));

    assert_eq!(HouseError::Disconnected.to_string(), "house.disconnected");
}

/// The check of the three houses with the repository's day after `change`, as its error
/// code, another rejection's detail or "ok".
fn day_checked(change: impl FnOnce(&mut Value)) -> String {
    let mut changed = day();
    change(&mut changed);
    let houses = vec![
        house("core:flat", 3),
        house("core:terraced", 5),
        house("core:house", 8),
    ];
    match load(rates(), changed, kinds(), houses) {
        Ok(_) => "ok".to_owned(),
        Err(error) => error,
    }
}

/// The repository's day is that of ADR-0024 section 4, by hand.
#[test]
fn the_repository_day_loads() {
    let at = |hours: u32, minutes: u32| hours * 3600 + minutes * 60;
    let houses = vec![house("core:flat", 3)];
    let data = load(rates(), day(), kinds(), houses).expect("loads");
    let expected = DayRules {
        // The day plan at 5:00, the wake 45 minutes before leaving.
        day_plan_at: at(5, 0),
        wake_before: at(0, 45),
        // School from 6 to the day before the 18th birthday, 7:40 to 15:20.
        school: SchoolRules {
            from_age: 6,
            until_age: 18,
            depart: at(7, 40),
            back: at(15, 20),
        },
        // Work from 18 to the day before the 67th: a draw below 70 full time, 8:40 to
        // 17:20, below 90 part time, 7:40 to 13:20, else at home.
        work: WorkRules {
            from_age: 18,
            until_age: 67,
            full_time_below: 70,
            part_time_below: 90,
            full_time: Hours {
                depart: at(8, 40),
                back: at(17, 20),
            },
            part_time: Hours {
                depart: at(7, 40),
                back: at(13, 20),
            },
        },
        // Coming home lowers hunger no more (ADR-0026, addendum of 2026-11-05 on the
        // time-filler's length): the away time freezes the needs, lunch was eaten there.
        return_jumps: NeedJumps {
            hunger: 0,
            energy: -200,
            bladder: -400,
            hygiene: -100,
            fun: -150,
        },
        // Under 6 in the kindergarten 8:00 to 15:00, in bed 19:00 to 7:00.
        young: YoungRules {
            until_age: 6,
            kita_from: at(8, 0),
            kita_to: at(15, 0),
            bed_from: at(19, 0),
            bed_to: at(7, 0),
        },
        // The night (ADR-0026, addendum of 2026-11-04): from 20:00 for 6 to 17, from 22:00 for
        // 18 to 66, from 21:30 from 67 on, each until 6:00; before a Saturday or a Sunday
        // morning until 7:00, from 67 on 6:30 (its addendum after the playtest).
        night: NightRules {
            from: [at(20, 0), at(20, 0), at(22, 0), at(21, 30)],
            until: [at(6, 0); 4],
            until_weekend: [at(7, 0), at(7, 0), at(7, 0), at(6, 30)],
        },
        // A need at 0 (ADR-0026, addendum of 2026-11-04 on a need at 0): on the floor energy
        // rises 50 an hour, the sofa's poor sleep, up at 250; an accident leaves hygiene at most
        // 50; hygiene reaching 0 takes 150 from fun once.
        zero: ZeroRules {
            collapse_energy_rate: 50,
            stand_up_at: 250,
            accident_hygiene_at_most: 50,
            hygiene_empty_fun: -150,
        },
        // The mood of a choice (ADR-0026, addendum of 2026-11-04 on the mood): at most 10
        // percent either way.
        choice_mood_percent: 10,
        // A time-filler lasts at most 4 h (ADR-0026, addendum of 2026-11-04 on comfort and the
        // time-filler).
        fallback_max_seconds: 14_400,
        // A time-filler is taken only with 10 minutes left (ADR-0026, addendum of 2026-11-05 on
        // the time-filler's length).
        fallback_min_seconds: 600,
        // The reserve at hunger 0 (ADR-0026, addendum of 2026-11-06 on hunger at 0): 500, 1 000
        // or 1 500 by the body's fat; at hunger 0 it falls 80, 60, 40, 50 an hour awake and two
        // fifths of that asleep, eating fills it 960 an hour; weak, energy falls 40 an hour more,
        // wasted 80, and a walk takes twice as long.
        reserve: ReserveRules {
            capacity: [500, 1000, 1500],
            drain: Drain {
                awake: [-80, -60, -40, -50],
                asleep: [-32, -24, -16, -20],
            },
            refill: 960,
            weak_energy: -40,
            wasted_energy: -80,
            walk_factor: 2,
        },
    };
    assert_eq!(*data.day(), expected);
}

#[test]
fn the_day_rejects() {
    assert_eq!(day_checked(|_| {}), "ok");

    // Times within the day, each departure before its return, the small children's day in
    // order: in bed until 7:00, the kindergarten, in bed again.
    assert_eq!(
        day_checked(|d| d["school"]["depart"] = json!(55_200)),
        "house.day_time"
    );
    assert_eq!(day_checked(|d| d["school"]["depart"] = json!(55_199)), "ok");
    assert_eq!(
        day_checked(|d| d["work"]["part_time"]["back"] = json!(27_600)),
        "house.day_time"
    );
    assert_eq!(
        day_checked(|d| d["work"]["full_time"]["back"] = json!(86_400)),
        "house.day_time"
    );
    assert_eq!(
        day_checked(|d| d["day_plan_at"] = json!(86_400)),
        "house.day_time"
    );
    assert_eq!(
        day_checked(|d| d["young"]["bed_from"] = json!(86_400)),
        "house.day_time"
    );
    // Within the day, but after the children's night begins at 20:00, which the small children's
    // bed must not (ADR-0026, addendum of 2026-11-04 after the playtest).
    assert_eq!(
        day_checked(|d| d["young"]["bed_from"] = json!(86_399)),
        "house.day_night"
    );
    assert_eq!(
        day_checked(|d| d["young"]["bed_to"] = json!(28_800)),
        "house.day_time"
    );
    assert_eq!(
        day_checked(|d| d["young"]["kita_to"] = json!(28_800)),
        "house.day_time"
    );
    assert_eq!(
        day_checked(|d| d["young"]["bed_from"] = json!(54_000)),
        "house.day_time"
    );

    // The wake comes after the day plan: 7:40 less 2 h 40 min is 5:00, the plan's second.
    assert_eq!(
        day_checked(|d| d["wake_before"] = json!(10_000)),
        "house.day_wake"
    );
    assert_eq!(
        day_checked(|d| d["wake_before"] = json!(9_600)),
        "house.day_wake"
    );
    // (With a night that ends at 5:00, before that wake.)
    let wake_at_five = day_checked(|d| {
        d["wake_before"] = json!(9_599);
        d["night"]["until"] = json!([18_000, 18_000, 18_000, 18_000]);
    });
    assert_eq!(wake_at_five, "ok");
    // No wake in the second of the departure (review: the departure would come first); one
    // second before is fine.
    assert_eq!(
        day_checked(|d| d["wake_before"] = json!(0)),
        "house.day_wake"
    );
    assert_eq!(day_checked(|d| d["wake_before"] = json!(1)), "ok");
    // Longer than the night before a departure.
    assert_eq!(
        day_checked(|d| d["wake_before"] = json!(30_000)),
        "house.day_wake"
    );
    let early_work = day_checked(|d| d["work"]["full_time"]["depart"] = json!(20_700));
    assert_eq!(early_work, "house.day_wake");

    // The ages follow each other: the repository's sit on the bounds (under 6, school from
    // 6; school until 18, work from 18).
    assert_eq!(
        day_checked(|d| d["school"]["until_age"] = json!(19)),
        "house.day_age"
    );
    assert_eq!(day_checked(|d| d["school"]["until_age"] = json!(17)), "ok");
    assert_eq!(
        day_checked(|d| d["young"]["until_age"] = json!(7)),
        "house.day_age"
    );
    assert_eq!(
        day_checked(|d| d["school"]["from_age"] = json!(18)),
        "house.day_age"
    );
    assert_eq!(
        day_checked(|d| d["work"]["until_age"] = json!(18)),
        "house.day_age"
    );

    // The shares of the work plan: 0 < full time < part time <= 100.
    assert_eq!(
        day_checked(|d| d["work"]["full_time_below"] = json!(90)),
        "house.day_share"
    );
    assert_eq!(
        day_checked(|d| d["work"]["full_time_below"] = json!(0)),
        "house.day_share"
    );
    assert_eq!(
        day_checked(|d| d["work"]["full_time_below"] = json!(1)),
        "ok"
    );
    assert_eq!(
        day_checked(|d| d["work"]["part_time_below"] = json!(100)),
        "ok"
    );
    assert_eq!(
        day_checked(|d| d["work"]["part_time_below"] = json!(101)),
        "house.day_share"
    );

    // Coming home only lowers a need, by at most 1000.
    assert_eq!(
        day_checked(|d| d["return_jumps"]["fun"] = json!(1)),
        "house.day_jump"
    );
    assert_eq!(day_checked(|d| d["return_jumps"]["fun"] = json!(0)), "ok");
    assert_eq!(
        day_checked(|d| d["return_jumps"]["hunger"] = json!(-1000)),
        "ok"
    );
    assert_eq!(
        day_checked(|d| d["return_jumps"]["hunger"] = json!(-1001)),
        "house.day_jump"
    );

    // The night (ADR-0026, addendum of 2026-11-04): each bound within the day; each end at or
    // after the day plan (5:00) and at or before the earliest wake (6:55: school and part time
    // leave at 7:40); each start after the last return (17:20), so after every end.
    let night = |band: usize, bound: &'static str, second: u32| {
        day_checked(move |d| d["night"][bound][band] = json!(second))
    };
    assert_eq!(night(2, "from", 86_400), "house.day_night");
    assert_eq!(night(2, "from", 86_399), "ok");
    assert_eq!(night(0, "until", 17_999), "house.day_night");
    assert_eq!(night(0, "until", 18_000), "ok");
    assert_eq!(night(3, "until", 24_900), "ok");
    assert_eq!(night(3, "until", 24_901), "house.day_night");
    // With the small children's bed from the same second, as every night begins at or after it
    // (ADR-0026, addendum of 2026-11-04 after the playtest); without, 17:20:01 is before 19:00.
    let early = |second: u32| {
        day_checked(move |d| {
            d["night"]["from"][1] = json!(second);
            d["young"]["bed_from"] = json!(second);
        })
    };
    assert_eq!(early(62_400), "house.day_night");
    assert_eq!(early(62_401), "ok");
    assert_eq!(night(1, "from", 62_401), "house.day_night");
    // An earlier departure moves the earliest wake (6:40 to 5:55), a later return the last.
    let earlier_wake = day_checked(|d| d["work"]["part_time"]["depart"] = json!(24_000));
    assert_eq!(earlier_wake, "house.day_night");
    let later_return = day_checked(|d| d["work"]["full_time"]["back"] = json!(72_000));
    assert_eq!(later_return, "house.day_night");
    assert!(
        day_checked(|d| d["night"]["from"] = json!([72_000, 72_000, 79_200]))
            .contains("invalid length")
    );
    // A weekend morning has no wake, so its end only comes at or after the day plan (ADR-0026,
    // addendum of 2026-11-04 after the playtest); and the small children's bed holds its places
    // from 19:00, so every night starts at or after it and every end comes at or before it.
    assert_eq!(night(1, "until_weekend", 17_999), "house.day_night");
    assert_eq!(night(1, "until_weekend", 18_000), "ok");
    assert_eq!(night(2, "until_weekend", 43_200), "ok");
    assert_eq!(night(2, "until_weekend", 68_400), "ok");
    assert_eq!(night(2, "until_weekend", 68_401), "house.day_night");
    assert_eq!(night(0, "until_weekend", 86_400), "house.day_night");
    let bedtime = |second: u32| day_checked(move |d| d["young"]["bed_from"] = json!(second));
    assert_eq!(bedtime(72_000), "ok");
    assert_eq!(bedtime(72_001), "house.day_night");
    assert!(
        day_checked(|d| d["night"]["until_weekend"] = json!([25_200, 25_200, 25_200]))
            .contains("invalid length")
    );

    // A need at 0 (ADR-0026, addendum of 2026-11-04 on a need at 0): on the floor energy rises,
    // and the level to get up at is above 0 and at most full; the accident's hygiene is a
    // value; hygiene at 0 lowers fun by at most 1000 or leaves it.
    let zero =
        |field: &'static str, value: i64| day_checked(move |d| d["zero"][field] = json!(value));
    assert_eq!(zero("collapse_energy_rate", 0), "house.day_zero");
    assert_eq!(zero("collapse_energy_rate", -50), "house.day_zero");
    assert_eq!(zero("collapse_energy_rate", 1), "ok");
    assert_eq!(zero("stand_up_at", 0), "house.day_zero");
    assert_eq!(zero("stand_up_at", 1), "ok");
    assert_eq!(zero("stand_up_at", 1000), "ok");
    assert_eq!(zero("stand_up_at", 1001), "house.day_zero");
    assert_eq!(zero("accident_hygiene_at_most", 0), "ok");
    assert_eq!(zero("accident_hygiene_at_most", 1000), "ok");
    assert_eq!(zero("accident_hygiene_at_most", 1001), "house.day_zero");
    assert_eq!(zero("hygiene_empty_fun", 0), "ok");
    assert_eq!(zero("hygiene_empty_fun", -1000), "ok");
    assert_eq!(zero("hygiene_empty_fun", -1001), "house.day_zero");
    assert_eq!(zero("hygiene_empty_fun", 1), "house.day_zero");
    assert!(zero("stand_up_at", -1).contains("invalid value"));
    // The mood moves the points at most by half.
    let mood = |value: i64| day_checked(move |d| d["choice_mood_percent"] = json!(value));
    assert_eq!(mood(0), "ok");
    assert_eq!(mood(50), "ok");
    assert_eq!(mood(51), "house.day_mood");
    assert!(mood(256).contains("invalid value"));
    assert!(zero("stand_up_at", 65_536).contains("invalid value"));
    // A time-filler lasts at most as long as an offer may, 60 s to 12 h; with the least time
    // left for one at 0 (ADR-0026, addendum of 2026-11-05 on the time-filler's length).
    let fallback = |value: i64| {
        day_checked(move |d| {
            d["fallback_max_seconds"] = json!(value);
            d["fallback_min_seconds"] = json!(0);
        })
    };
    assert_eq!(fallback(60), "ok");
    assert_eq!(fallback(43_200), "ok");
    assert_eq!(fallback(59), "house.day_fallback");
    assert_eq!(fallback(43_201), "house.day_fallback");
    // The least time left for a time-filler is at most its longest, the repository's 600 s
    // below its 14 400; 0 takes every one.
    let longest = |value: i64| day_checked(move |d| d["fallback_max_seconds"] = json!(value));
    assert_eq!(longest(600), "ok");
    assert_eq!(longest(599), "house.day_fallback");
    let least = |value: i64| day_checked(move |d| d["fallback_min_seconds"] = json!(value));
    assert_eq!(least(0), "ok");
    assert_eq!(least(14_400), "ok");
    assert_eq!(least(14_401), "house.day_fallback");
    let shorter = day_checked(|d| {
        d["fallback_max_seconds"] = json!(600);
        d["fallback_min_seconds"] = json!(601);
    });
    assert_eq!(shorter, "house.day_fallback");
    assert!(least(-1).contains("invalid value"));
    // The reserve (ADR-0026, addendum of 2026-11-06 on hunger at 0): capacities rising from 2,
    // a drain falling in every band, a refill rising, extras at most 0 with the wasted one at
    // most the weak one, a walk factor from 1 to 10.
    let reserve = |change: fn(&mut Value)| day_checked(move |d| change(&mut d["reserve"]));
    assert_eq!(reserve(|r| r["capacity"] = json!([2, 3, 4])), "ok");
    assert_eq!(
        reserve(|r| r["capacity"] = json!([1, 3, 4])),
        "house.day_reserve"
    );
    assert_eq!(
        reserve(|r| r["capacity"] = json!([500, 500, 1500])),
        "house.day_reserve"
    );
    assert_eq!(
        reserve(|r| r["capacity"] = json!([500, 1500, 1000])),
        "house.day_reserve"
    );
    assert_eq!(
        reserve(|r| r["drain"]["awake"][1] = json!(0)),
        "house.day_reserve"
    );
    assert_eq!(
        reserve(|r| r["drain"]["asleep"][3] = json!(1)),
        "house.day_reserve"
    );
    assert_eq!(reserve(|r| r["refill"] = json!(0)), "house.day_reserve");
    assert_eq!(reserve(|r| r["weak_energy"] = json!(0)), "ok");
    assert_eq!(
        reserve(|r| r["weak_energy"] = json!(1)),
        "house.day_reserve"
    );
    assert_eq!(reserve(|r| r["wasted_energy"] = json!(-40)), "ok");
    assert_eq!(
        reserve(|r| r["wasted_energy"] = json!(-39)),
        "house.day_reserve"
    );
    assert_eq!(reserve(|r| r["walk_factor"] = json!(1)), "ok");
    assert_eq!(reserve(|r| r["walk_factor"] = json!(10)), "ok");
    assert_eq!(
        reserve(|r| r["walk_factor"] = json!(0)),
        "house.day_reserve"
    );
    assert_eq!(
        reserve(|r| r["walk_factor"] = json!(11)),
        "house.day_reserve"
    );
    // A drain, the refill and the extras move at most 10 000 an hour either way.
    assert_eq!(reserve(|r| r["drain"]["awake"][0] = json!(-10_000)), "ok");
    assert_eq!(
        reserve(|r| r["drain"]["awake"][0] = json!(-10_001)),
        "house.day_reserve"
    );
    assert_eq!(
        reserve(|r| r["drain"]["asleep"][2] = json!(i32::MIN)),
        "house.day_reserve"
    );
    assert_eq!(reserve(|r| r["refill"] = json!(10_000)), "ok");
    assert_eq!(
        reserve(|r| r["refill"] = json!(10_001)),
        "house.day_reserve"
    );
    assert_eq!(reserve(|r| r["wasted_energy"] = json!(-10_000)), "ok");
    assert_eq!(
        reserve(|r| r["wasted_energy"] = json!(-10_001)),
        "house.day_reserve"
    );
    let lowest = reserve(|r| {
        r["weak_energy"] = json!(i32::MIN);
        r["wasted_energy"] = json!(i32::MIN);
    });
    assert_eq!(lowest, "house.day_reserve");

    // The first check that fails decides, in the order time, wake, age, share, jump, night,
    // a need at 0, the mood, the time-filler, the reserve.
    let broken = |checks: usize| {
        day_checked(move |d| {
            let breaks: [fn(&mut Value); 10] = [
                |d| d["school"]["depart"] = json!(55_200),
                |d| d["wake_before"] = json!(10_000),
                |d| d["school"]["until_age"] = json!(19),
                |d| d["work"]["full_time_below"] = json!(90),
                |d| d["return_jumps"]["fun"] = json!(1),
                |d| d["night"]["until"][2] = json!(0),
                |d| d["zero"]["stand_up_at"] = json!(0),
                |d| d["choice_mood_percent"] = json!(51),
                |d| d["fallback_max_seconds"] = json!(0),
                |d| d["reserve"]["refill"] = json!(0),
            ];
            breaks.iter().skip(10 - checks).for_each(|change| change(d));
        })
    };
    let codes: Vec<String> = (1..=10).map(broken).collect();
    assert_eq!(
        codes,
        [
            "house.day_reserve",
            "house.day_fallback",
            "house.day_mood",
            "house.day_zero",
            "house.day_night",
            "house.day_jump",
            "house.day_share",
            "house.day_age",
            "house.day_wake",
            "house.day_time"
        ]
    );

    // Unknown and missing fields.
    let unknown = |text: String| text.contains("unknown field");
    assert!(unknown(day_checked(|d| d["night_at"] = json!(0))));
    assert!(unknown(day_checked(|d| d["school"]["grade"] = json!(1))));
    assert!(unknown(day_checked(
        |d| d["work"]["full_time"]["x"] = json!(1)
    )));
    assert!(unknown(day_checked(
        |d| d["return_jumps"]["thirst"] = json!(-1)
    )));
    assert!(unknown(day_checked(|d| d["young"]["x"] = json!(1))));
    assert!(unknown(day_checked(|d| d["night"]["x"] = json!(1))));
    assert!(unknown(day_checked(|d| d["zero"]["x"] = json!(1))));
    assert!(unknown(day_checked(|d| d["reserve"]["x"] = json!(1))));
    assert!(unknown(day_checked(
        |d| d["reserve"]["drain"]["x"] = json!(1)
    )));
    let missing = day_checked(|d| {
        d.as_object_mut().expect("the day").remove("wake_before");
    });
    assert!(missing.contains("missing field"), "{missing}");

    let no_night = day_checked(|d| {
        d.as_object_mut().expect("the day").remove("night");
    });
    assert!(no_night.contains("missing field"), "{no_night}");
    let no_zero = day_checked(|d| {
        d.as_object_mut().expect("the day").remove("zero");
    });
    assert!(no_zero.contains("missing field"), "{no_zero}");
    let no_mood = day_checked(|d| {
        d.as_object_mut()
            .expect("the day")
            .remove("choice_mood_percent");
    });
    assert!(no_mood.contains("missing field"), "{no_mood}");
    let no_fallback = day_checked(|d| {
        d.as_object_mut()
            .expect("the day")
            .remove("fallback_max_seconds");
    });
    assert!(no_fallback.contains("missing field"), "{no_fallback}");
    let no_least = day_checked(|d| {
        d.as_object_mut()
            .expect("the day")
            .remove("fallback_min_seconds");
    });
    assert!(no_least.contains("missing field"), "{no_least}");
    let no_reserve = day_checked(|d| {
        d.as_object_mut().expect("the day").remove("reserve");
    });
    assert!(no_reserve.contains("missing field"), "{no_reserve}");

    assert_eq!(HouseError::DayTime.to_string(), "house.day_time");
    assert_eq!(HouseError::DayJump.to_string(), "house.day_jump");
    assert_eq!(HouseError::DayNight.to_string(), "house.day_night");
    assert_eq!(HouseError::DayZero.to_string(), "house.day_zero");
    assert_eq!(HouseError::DayMood.to_string(), "house.day_mood");
    assert_eq!(HouseError::DayFallback.to_string(), "house.day_fallback");
    assert_eq!(HouseError::DayReserve.to_string(), "house.day_reserve");
    assert_eq!(HouseError::BedFor.to_string(), "house.bed_for");
    assert_eq!(HouseError::DwellRoom.to_string(), "house.dwell_room");
}

/// Four rooms with a tie: hall to bath through the kitchen (doors 0 and 3) or through the
/// living room (doors 1 and 2), 7 s either way.
fn test_house() -> Value {
    let object = |kind: &str, room: &str| json!({ "kind": kind, "room": room });
    json!({
        "id": "core:test",
        "capacity": 3,
        "rooms": [
            { "id": "hall", "entrance": true }, { "id": "kitchen" }, { "id": "living" },
            { "id": "bath" }
        ],
        "child_day_room": "living",
        "child_night_room": "living",
        "dwell_room": "living",
        "doors": [
            { "a": "hall", "b": "kitchen", "seconds": 5 },
            { "a": "hall", "b": "living", "seconds": 3 },
            { "a": "living", "b": "bath", "seconds": 4 },
            { "a": "kitchen", "b": "bath", "seconds": 2 }
        ],
        "objects": [
            object("core:dining_table", "kitchen"),
            object("core:fridge", "kitchen"),
            object("core:sofa", "living"),
            object("core:tv", "living"),
            object("core:bed", "living"),
            object("core:single_bed", "living"),
            object("core:toilet", "bath"),
            object("core:shower", "bath")
        ]
    })
}

#[test]
fn walking_seconds() {
    let data = load(rates(), day(), kinds(), vec![test_house()]).expect("loads");
    let house = data
        .house(&DataId::new("core:test").expect("id"))
        .expect("the house");
    // The rooms by their place in the file.
    let [hall, kitchen, living, bath] = [0, 1, 2, 3];
    // By hand: kitchen to living through the bath (2 + 4) beats the hall (5 + 3); living to
    // bath directly (4) beats the hall and the kitchen (3 + 5 + 2).
    let by_hand = [[0, 5, 3, 7], [5, 0, 6, 2], [3, 6, 0, 4], [7, 2, 4, 0]];
    for from in 0..4u8 {
        for to in 0..4u8 {
            let expected = by_hand[usize::from(from)][usize::from(to)];
            assert_eq!(house.seconds(from, to), expected, "{from} to {to}");
        }
    }
    // The tie goes to the smaller sequence of doors: hall to bath [0, 3] before [1, 2], bath
    // to hall [2, 1] before [3, 0].
    assert_eq!(house.route(hall, bath), [hall, kitchen, bath]);
    assert_eq!(house.route(bath, hall), [bath, living, hall]);
    assert_eq!(house.route(kitchen, living), [kitchen, bath, living]);
    assert_eq!(house.route(living, living), [living]);
    // To an object: 5 s within its room, else the table; out: to the entrance plus 5 s.
    assert_eq!(house.walk_to(kitchen, 0), 5);
    assert_eq!(house.walk_to(hall, 0), 5);
    assert_eq!(house.walk_to(living, 6), 4);
    assert_eq!(house.walk_to(hall, 7), 7);
    assert_eq!(house.walk_out(bath), 12);
    assert_eq!(house.walk_out(hall), 5);
}

#[test]
fn house_by_size() {
    let houses = vec![
        house("core:house", 8),
        house("core:flat", 3),
        house("core:terraced", 5),
    ];
    let data = load(rates(), day(), kinds(), houses).expect("loads");
    let chosen: Vec<Option<&str>> = (1..=9)
        .map(|members| data.house_for(members).map(|house| house.id().as_str()))
        .collect();
    let (flat, terraced, house) = (Some("core:flat"), Some("core:terraced"), Some("core:house"));
    assert_eq!(
        chosen,
        [
            flat, flat, flat, terraced, terraced, house, house, house, None
        ]
    );
}

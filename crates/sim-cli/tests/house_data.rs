//! ADR-0025 sections 1 to 4 with the offers of ADR-0026 section 2 and the day of ADR-0024
//! section 4, through the reader of the house files: three houses load, each check of the
//! reader and of the core rejects a broken file with its code, the walking table of a test
//! house by hand, and the house by the size of a household.

use serde_json::{Value, json};
use sim_cli::house::house_data_from;
use sim_core::data_id::DataId;
use sim_core::house::data::{
    DayRules, Hours, HouseData, HouseError, NeedJumps, SchoolRules, WorkRules, YoungRules,
};

/// The repository's `day.json` (ADR-0024 section 4).
const DAY: &str = include_str!("../../../data/house/day.json");

fn day() -> Value {
    serde_json::from_str(DAY).expect("the day")
}

/// The rates of ADR-0026 section 1 per age band (6 to 12, 13 to 17, 18 to 66, from 67).
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
            "energy": [60, 70, 85, 80],
            "bladder": [-40, -40, -40, -40],
            "hygiene": [-10, -10, -10, -10],
            "fun": [0, 0, 0, 0]
        }
    })
}

/// The kinds of ADR-0025 section 1 with the offers of ADR-0026 section 2.
fn kinds() -> Value {
    let sleep = |energy: Option<i32>| {
        let mut offer = json!({
            "id": "sleep", "sleep": true, "until_full": "energy", "max_seconds": 43_200,
            "min_age": 0, "max_age": 130
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
    // not feed, and sleep leaves fun at 0.
    assert_eq!(
        toilet(|o| o["until_full"] = json!("hunger")),
        "house.offer_full"
    );
    let full_of_fun = checked(|kinds, _| {
        kind(kinds, "core:bed")["offers"][0]["until_full"] = json!("fun");
    });
    assert_eq!(full_of_fun, "house.offer_full");
    // An offer must set a rate, so its action has a second it began at; sleeping sets all.
    let nothing = shower(|o| {
        o.as_object_mut().expect("an offer").remove("rates");
    });
    assert_eq!(nothing, "house.offer_rates");

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
        return_jumps: NeedJumps {
            hunger: -350,
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
    assert_eq!(
        day_checked(|d| d["young"]["bed_from"] = json!(86_399)),
        "ok"
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
    assert_eq!(day_checked(|d| d["wake_before"] = json!(9_599)), "ok");
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

    // The first check that fails decides, in the order time, wake, age, share, jump.
    let broken = |checks: usize| {
        day_checked(move |d| {
            let breaks: [fn(&mut Value); 5] = [
                |d| d["school"]["depart"] = json!(55_200),
                |d| d["wake_before"] = json!(10_000),
                |d| d["school"]["until_age"] = json!(19),
                |d| d["work"]["full_time_below"] = json!(90),
                |d| d["return_jumps"]["fun"] = json!(1),
            ];
            breaks.iter().skip(5 - checks).for_each(|change| change(d));
        })
    };
    let codes: Vec<String> = (1..=5).map(broken).collect();
    assert_eq!(
        codes,
        [
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
    let missing = day_checked(|d| {
        d.as_object_mut().expect("the day").remove("wake_before");
    });
    assert!(missing.contains("missing field"), "{missing}");

    assert_eq!(HouseError::DayTime.to_string(), "house.day_time");
    assert_eq!(HouseError::DayJump.to_string(), "house.day_jump");
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

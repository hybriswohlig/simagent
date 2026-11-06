//! Comfort and the time-filler after the second playtest of 2026-11-04 (ADR-0026, addendum of
//! 2026-11-04 on comfort and the time-filler): a need weighs `max(0, comfort − value)` in the
//! points of a choice, the fill still capped at `1000 − value`; one idle by day with nothing worth
//! it and nobody to wait for takes a free `fallback` offer (since the addendum of 2026-11-05 on
//! the time-filler's length the one with the highest mood, then the shortest walk, then the
//! smaller object), walks to it and does it until its next choice, at the latest after its length,
//! `fallback_max_seconds` after it began, at today's departure and at its bedtime; not in the
//! night. The tune house takes a time-filler however soon it ends (`tune_data`). A waiter
//! keeps its place at 700 and 400; an offer that would still run at today's departure is closed,
//! unless it raises a critical need. The values by hand from the rates, the offers and the walks,
//! the moods from `mood` (their values from `tools/rng_reference.py`), the draw the rules name.

mod common;

use std::collections::BTreeSet;

use common::day::{self, MORNING, Run, number, places_hold};
use serde_json::json;
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::house::choice::{ENOUGH, crosses_departure, filler, lasts, mood, next_choice, score};
use sim_core::house::data::{House, HouseData};
use sim_core::house::need::NeedSample;
use sim_core::house::reserve::Reserve;
use sim_core::house::state::{
    Action, Broken, InHouse, Place, action_end, consistent, filler_end_of, next_choice_of,
};
use sim_core::ids::SimId;
use sim_core::input::{Command, Input};
use sim_core::lineage::LineageKey;
use sim_core::log::{Cause, EndReason, LogEntry, LogPayload, Subject};
use sim_core::time::{DAY, HOUR, SimTime, YEAR};
use sim_core::world::{World, YearOut};

const D: i64 = DAY.as_secs();
const H: i64 = HOUR.as_secs();
const Y: i64 = YEAR.as_secs();

/// The mood of the repository's day, `choice_mood_percent` of `data/house/day.json`.
const MOOD: u8 = 10;

const MORNING_VALUES: [u16; 5] = [600, 900, 500, 700, 800];

fn t(secs: i64) -> SimTime {
    SimTime::from_secs(secs)
}

fn sim(id: u64) -> SimId {
    SimId::try_from(id).expect("valid")
}

fn no_mood(_: u16, _: u16) -> i8 {
    0
}

/// The points of `object`'s first offer among `scored`.
fn points_of(scored: &[sim_core::house::choice::Scored], object: u16) -> Option<i64> {
    (scored.iter())
        .find(|scored| scored.object == object && scored.offer == 0)
        .map(|scored| scored.points)
}

fn terraced(houses: &HouseData) -> &House {
    houses
        .house(&DataId::new("core:terraced").expect("id"))
        .expect("the terraced house")
}

// ---------------------------------------------------------------------------------------
// Comfort (point 1).

/// The pin of the design, in the repository's terraced house (rooms hall 0, kitchen 1, living
/// 2, wc 3, landing 4, bath 5; objects table 0, fridge 1, sofa 2, television 3, bookshelf 4, the
/// WC's toilet 5, the bath's toilet 6, shower 7), an adult of 30, every other need full, mood 0.
/// Hunger weighs from its comfort 700 on: at 600 the table (hunger +1 800 for 20 min, 600, but
/// 400 to full) brings 100 · 400 = 40 000, at 599 101 · 401 = 40 501, less its 5 s in the kitchen;
/// the fridge (200) 100 · 200 and 101 · 200. At 750 hunger weighs 0, so the table brings nothing
/// but its walk. The toilet the same square: bladder 600 from the hall, 100 · 400 less the 2 s to
/// the WC. Hygiene from 900: the shower (+2 800 for 15 min, 700) from the living room, 18 s (3 to
/// the hall, 12 to the landing, 3 to the bath), brings 159 · 259 − 900 = 40 281 at 741 and 158 ·
/// 258 − 900 = 39 864 at 742. Energy and fun keep their weight from full: fun 800 at the
/// television (+400 for 45 min, 300, but 200 to full) 200 · 200 less 3 s from the hall.
#[test]
fn a_need_weighs_from_its_comfort() {
    let houses = common::repository_houses();
    let house = terraced(&houses);
    let at = |values: [u16; 5], room: u8| {
        score(house, &houses, (30, values, room), (None, None), no_mood)
    };
    let hungry = |hunger: u16| [hunger, 1000, 1000, 1000, 1000];
    assert_eq!(points_of(&at(hungry(600), 1), 0), Some(40_000 - 250));
    assert_eq!(points_of(&at(hungry(599), 1), 0), Some(40_501 - 250));
    assert_eq!(points_of(&at(hungry(600), 1), 1), Some(20_000 - 250));
    assert_eq!(points_of(&at(hungry(599), 1), 1), Some(20_200 - 250));
    assert_eq!(points_of(&at(hungry(750), 1), 0), Some(-250));
    assert_eq!(points_of(&at(hungry(700), 1), 0), Some(-250));
    let bladder = [1000, 1000, 600, 1000, 1000];
    assert_eq!(points_of(&at(bladder, 0), 5), Some(40_000 - 100));
    let hygiene = |value: u16| [1000, 1000, 1000, value, 1000];
    assert_eq!(points_of(&at(hygiene(741), 2), 7), Some(40_281));
    assert_eq!(points_of(&at(hygiene(742), 2), 7), Some(39_864));
    assert!(points_of(&at(hygiene(741), 2), 7) >= Some(ENOUGH));
    let fun = [1000, 1000, 1000, 1000, 800];
    assert_eq!(points_of(&at(fun, 0), 3), Some(40_000 - 150));
    // Critical still reads the real value: at hunger 150 the table's weight is 700 − 150 = 550,
    // its fill 600 (850 to full), and (151 − 150) · 10⁹ for the critical need.
    let critical = hungry(150);
    assert_eq!(
        points_of(&at(critical, 1), 0),
        Some(550 * 600 - 250 + 1_000_000_000)
    );
}

// ---------------------------------------------------------------------------------------
// The time-filler (point 2).

/// A loo for one in the hall filling the bladder by 60 (+720 an hour for 5 min), a screen for two
/// in the den, a time-filler with fun +100 an hour for 45 min (75), and a bed for two in the den;
/// the five essential kinds of `common::day` in the hall, each filling 0.
const TUNE_KINDS: &str = r#"
  {"id": "test:loo", "slots": 1, "offers": [
    {"id": "use", "rates": {"bladder": 720}, "seconds": 300, "min_age": 0, "max_age": 130}]},
  {"id": "test:screen", "slots": 2, "offers": [
    {"id": "watch", "rates": {"fun": 100}, "seconds": 2700, "min_age": 0, "max_age": 130,
     "fallback": true}]},
  {"id": "test:bed", "slots": 2, "offers": [
    {"id": "sleep", "sleep": true, "min_age": 0, "max_age": 130}]}"#;

/// Rooms hall 0 (the entrance) and den 1, 3 s apart. Objects: the loo 0 in the hall, the screen
/// 1 and the bed 2 in the den, the essential ones 3 to 7 (table, fridge, toilet, shower, sofa) in
/// the hall.
const TUNE: &str = r#"{
  "id": "test:tune", "capacity": 2,
  "rooms": [{"id": "hall", "entrance": true}, {"id": "den"}],
  "child_day_room": "den", "child_night_room": "den", "dwell_room": "den",
  "doors": [{"a": "hall", "b": "den", "seconds": 3}],
  "objects": [{"kind": "test:loo", "room": "hall"}, {"kind": "test:screen", "room": "den"},
    {"kind": "test:bed", "room": "den"}, ESSENTIALS]
}"#;

const LOO: u16 = 0;
const SCREEN: u16 = 1;

/// Awake only the bladder falls, `bladder` an hour; asleep energy rises 100; every need weighs
/// from full on (the test houses' comfort 1000).
fn bladder_rates(bladder: i32) -> String {
    let bands = |rate: i32| [rate; 4];
    json!({
        "awake": {"hunger": bands(0), "energy": bands(0), "bladder": bands(bladder),
                  "hygiene": bands(0), "fun": bands(0)},
        "asleep": {"hunger": bands(0), "energy": bands(100), "bladder": bands(0),
                   "hygiene": bands(0), "fun": bands(0)},
        "comfort": [1000, 1000, 1000, 1000, 1000]
    })
    .to_string()
}

/// The tune house with the bladder falling `bladder` an hour, a time-filler taken however soon it
/// ends (`common::repository_day_any_filler`): these are the tests of a time-filler once taken;
/// `house_filler.rs` tests the least time left for one (ADR-0026, addendum of 2026-11-05 on the
/// time-filler's length).
fn tune_data(bladder: i32) -> Data {
    let any = common::repository_day_any_filler();
    day::house_data_on(&any, &bladder_rates(bladder), TUNE_KINDS, TUNE)
}

fn tune_house(data: &Data) -> (&HouseData, &House) {
    let houses = data.houses().expect("houses");
    let house = houses
        .house(&DataId::new("test:tune").expect("id"))
        .expect("the house");
    (houses, house)
}

/// What a person does from `origin` on, as (seconds after it, payload): its choices, arrivals,
/// ends, idling and critical needs.
fn doings(run: &Run, id: u64, origin: i64, until: i64) -> Vec<(i64, LogPayload)> {
    (run.log.iter())
        .filter(|entry| entry.subjects == [Subject::Sim(sim(id))])
        .filter(|entry| (origin..until).contains(&entry.t.as_secs()))
        .filter(|entry| {
            matches!(
                entry.payload,
                LogPayload::ActionStarted { .. }
                    | LogPayload::Reached { .. }
                    | LogPayload::ActionEnded { .. }
                    | LogPayload::Idle { .. }
                    | LogPayload::NeedCritical { .. }
            )
        })
        .map(|entry| (entry.t.as_secs() - origin, entry.payload.clone()))
        .collect()
}

/// The entry of `id` at `at` with `payload`.
fn entry_of<'a>(run: &'a Run, id: u64, at: i64, payload: &LogPayload) -> &'a LogEntry {
    (run.log.iter())
        .find(|entry| {
            entry.subjects == [Subject::Sim(sim(id))]
                && entry.t.as_secs() == at
                && entry.payload == *payload
        })
        .expect("the entry")
}

/// A walk to `object`'s first offer as its entry writes it.
fn started(
    (object, slot): (u16, u8),
    (room, walk_secs): (u8, u32),
    needs: [u16; 5],
    filler: bool,
) -> LogPayload {
    LogPayload::ActionStarted {
        house: DataId::new("test:tune").expect("id"),
        object,
        offer: 0,
        slot,
        room,
        walk_secs,
        needs,
        busy: Vec::new(),
        commanded: false,
        held: Vec::new(),
        tier: Vec::new(),
        filler,
    }
}

fn ended(object: u16, reason: EndReason, needs: [u16; 5]) -> LogPayload {
    LogPayload::ActionEnded {
        object,
        offer: 0,
        reason,
        needs,
    }
}

/// The mood of founder `index` of `test:house` (the couple of `common::day`, world seed 7) for
/// offer 0 of `object` at `second`, only read.
fn couple_mood(index: u64, second: i64, object: u16) -> i64 {
    let key = LineageKey::founder(&DataId::new("test:house").expect("id"), index);
    i64::from(mood(7, key, t(second), (object, 0), MOOD))
}

/// The loo's points from the den at the bladder deficit `d` with the mood `mood`: the deficit
/// times its fill of 60, scaled, less its walk of 3 s.
fn loo(d: i64, mood: i64) -> i64 {
    d * 60 * (100 + mood) / 100 - 150
}

/// The bladder's deficit `s` seconds after 6:00, at 500 then and falling 1 000 an hour.
fn deficit(s: i64) -> i64 {
    500 + 1_000 * s / 3_600
}

/// The couple of `common::day` in the tune house, the bladder falling 1 000 an hour. At 6:00
/// nothing brings 40 000 (the loo 500 · 60 · 110 / 100 − 250 = 32 750 at most, the screen 200 ·
/// 75, the essential ones only their walk) and nobody is there to wait for: each takes the screen
/// as its time-filler, the woman its first place, the man the second, and walks 3 s to the den.
/// From there each fills its time until its next choice: the loo brings 40 000 at the most mood
/// from the deficit 609, 393 s on (6:06:33), and surely at the least from 744, 879 s on
/// (6:14:39); the whole minutes between are 6:07 to 6:14 with the deficits 616, 633, 650, 666,
/// 683, 700, 716, 733. Her moods for the loo there are +5, −1, +10: at 6:09:00 650 · 60 · 110 /
/// 100 − 150 = 42 750, so her filler ends then and she walks to the loo, bladder 350, fun 814
/// (+100 an hour for 537 s). His are +6, +1, +1, −1, +3: his end planned at his arrival is 6:11:00
/// (683 · 60 · 103 / 100 − 150 = 42 059), but her walk to the loo takes its one place, so his end
/// is planned anew, and anew when her 5 minutes there end at 6:14:03 with the bladder 410: the loo
/// is free again, the deficit 734 already above 609 and no whole minute before 6:14:39, so his
/// filler ends at 6:14:39, the deficit 744 (40 026 at the least mood). She, with 590, takes the
/// screen again. The 400 of both at 6:06:00 changes nothing for one at its filler.
#[test]
fn a_time_filler_lasts_until_the_next_choice() {
    let (most, least) = (393, 879);
    assert!(loo(deficit(most - 1), 10) < ENOUGH && loo(deficit(most), 10) >= ENOUGH);
    assert!(loo(deficit(least - 1), -10) < ENOUGH && loo(deficit(least), -10) >= ENOUGH);
    let minutes: Vec<i64> = (420..least).step_by(60).collect();
    assert_eq!(
        minutes.iter().map(|&s| deficit(s)).collect::<Vec<_>>(),
        [616, 633, 650, 666, 683, 700, 716, 733]
    );
    let moods = |index: u64| -> Vec<i64> {
        minutes
            .iter()
            .map(|&s| couple_mood(index, MORNING + s, LOO))
            .collect()
    };
    assert_eq!(moods(0), [5, -1, 10, -4, -6, -7, 10, -5]);
    assert_eq!(moods(1), [6, 1, 1, -1, 3, 0, 0, -4]);
    let first = |index: u64| {
        let reaches =
            |s: &&i64| loo(deficit(**s), couple_mood(index, MORNING + **s, LOO)) >= ENOUGH;
        minutes.iter().find(reaches).copied().unwrap_or(least)
    };
    assert_eq!((first(0), first(1)), (540, 660));
    assert_eq!(
        (loo(650, 10), loo(683, 3), loo(744, -10)),
        (42_750, 42_059, 40_026)
    );
    let (mut run, _) = Run::moved_in(tune_data(-1_000));
    run.advance(MORNING + 900);
    let at_six = MORNING_VALUES;
    let hers = [
        (0, started((SCREEN, 0), (0, 3), at_six, true)),
        (3, LogPayload::Reached { object: SCREEN }),
        (
            540,
            ended(SCREEN, EndReason::Done, [600, 900, 350, 700, 814]),
        ),
        (
            540,
            started((LOO, 0), (1, 3), [600, 900, 350, 700, 814], false),
        ),
        (543, LogPayload::Reached { object: LOO }),
        (843, ended(LOO, EndReason::Done, [600, 900, 410, 700, 814])),
        (
            843,
            started((SCREEN, 0), (0, 3), [600, 900, 410, 700, 814], true),
        ),
        (846, LogPayload::Reached { object: SCREEN }),
    ];
    assert_eq!(doings(&run, 1, MORNING, MORNING + 900), hers);
    let his = [
        (0, started((SCREEN, 1), (0, 3), at_six, true)),
        (3, LogPayload::Reached { object: SCREEN }),
        (
            879,
            ended(SCREEN, EndReason::Done, [600, 900, 256, 700, 824]),
        ),
        (
            879,
            started((LOO, 0), (1, 3), [600, 900, 256, 700, 824], false),
        ),
        (882, LogPayload::Reached { object: LOO }),
    ];
    assert_eq!(doings(&run, 2, MORNING, MORNING + 900), his);
    // His end was planned anew by her return to the screen, the last change of the places.
    let end = entry_of(&run, 2, MORNING + 879, &his[2].1);
    let [Cause::Plan(plan)] = end.causes.as_slice() else {
        panic!("{:?}", end.causes);
    };
    let back = entry_of(&run, 1, MORNING + 843, &hers[6].1);
    assert_eq!(number(&plan.by), number(&back.id));
}

/// The time-filler is the free `fallback` offer with the highest mood (ADR-0026, addendum of
/// 2026-11-05 on the time-filler's length), then the one with the shortest walk, then the smaller
/// object and offer; how much fun it brings no longer ranks it, and it takes no object without a
/// free place. In the repository's terraced house, the moods given by hand (a choice gives the
/// draw `mood`): all at 0, an adult in the hall takes the sofa's `sit` (object 2, offer 1), the
/// television and the bookshelf lying as near in the living room, 3 s away; so does a child of 9
/// there, the toy chest 16 s away upstairs; with the bookshelf at +5 the bookshelf, with it taken
/// the sofa again; with the chest at +1 the child goes up to it, an adult cannot (6 to 12); with
/// the sofa, the television and the bookshelf taken nothing is left for an adult.
#[test]
fn the_time_filler_is_the_one_with_the_highest_mood() {
    let houses = common::repository_houses();
    let house = terraced(&houses);
    let (sofa, tv, bookshelf, chest) = (2, 3, 4, 11);
    let level = |_: u16, _: u16| 0;
    let reading = |object: u16, _: u16| if object == bookshelf { 5 } else { 0 };
    let playing = |object: u16, _: u16| if object == chest { 1 } else { 0 };
    let pick = |age: u8, busy: &[u16], moods: &dyn Fn(u16, u16) -> i8| {
        filler((house, &houses), (age, 0), busy, |object, offer| {
            moods(object, offer)
        })
    };
    assert_eq!(pick(30, &[], &level), Some((sofa, 1)));
    assert_eq!(pick(9, &[], &level), Some((sofa, 1)));
    assert_eq!(pick(30, &[], &reading), Some((bookshelf, 0)));
    assert_eq!(pick(30, &[bookshelf], &reading), Some((sofa, 1)));
    assert_eq!(pick(9, &[], &playing), Some((chest, 0)));
    assert_eq!(pick(30, &[], &playing), Some((sofa, 1)));
    assert_eq!(pick(9, &[sofa], &level), Some((tv, 0)));
    assert_eq!(pick(30, &[sofa, tv, bookshelf], &level), None);
}

/// The end of a time-filler by hand (`filler_end_of`; ADR-0026, addendum of 2026-11-05 on the
/// time-filler's length), the woman of 30 of the tune house, who works full time (8:40 to 17:20,
/// her draw 30), at the screen in the den with fun +100 an hour since `begun`, every other need
/// still: nothing ever brings 40 000. On day 2, a Wednesday: begun at 7:00 it ends after its 45
/// minutes, at 7:45; begun at 8:10 at her departure, 8:40, before its 8:55, unless the plan is
/// made after the departure (one that waited for it), then at 8:55; begun at 21:30 at her
/// bedtime, 22:00, before its 22:15; begun in her night (at 22:30), at once. A screen of 12 h
/// ends at the departure, begun at 7:00, and `fallback_max_seconds`, 4 h, after it began, begun
/// at 17:30 (at 21:30, before the bedtime).
#[test]
fn a_time_filler_ends_at_its_length_the_departure_bedtime_or_after_four_hours() {
    let data = tune_data(0);
    // A screen of 12 h whose fun rises 1 an hour, so it never brings 40 000 itself.
    let screen = "\"rates\": {\"fun\": 100}, \"seconds\": 2700";
    assert!(TUNE_KINDS.contains(screen));
    let long = TUNE_KINDS.replace(screen, "\"rates\": {\"fun\": 1}, \"seconds\": 43200");
    let any = common::repository_day_any_filler();
    let long = day::house_data_on(&any, &bladder_rates(0), &long, TUNE);
    let born = t(-30 * Y);
    let key = LineageKey::founder(&DataId::new("test:house").expect("id"), 0);
    let at_screen = |begun: i64| InHouse {
        needs: [600, 900, 500, 700, 800].map(|value| NeedSample::new(value, t(begun), 0)),
        place: Place::Room(1),
        action: Some(Action {
            object: SCREEN,
            offer: 0,
            slot: 0,
            filler: true,
        }),
        due: None,
        decisions: sim_core::house::state::Decisions { day: 1, count: 1 },
        collapsed: false,
        reserve: Reserve::full(1000, t(begun)),
    };
    let fun = |mut person: InHouse, begun: i64| {
        person.needs[4] = NeedSample::new(800, t(begun), 100);
        person
    };
    let taken = BTreeSet::from([(SCREEN, 0)]);
    let end_on = |(data, rate): (&Data, i32), begun: i64, start: i64| {
        let (houses, house) = tune_house(data);
        let mut person = at_screen(begun);
        person.needs[4] = NeedSample::new(800, t(begun), rate);
        filler_end_of(
            (house, houses),
            (born, key, 7),
            &person,
            &taken,
            (t(start), t(start)),
        )
    };
    let end = |begun: i64, start: i64| end_on((&data, 100), begun, start);
    let day = 2 * D;
    assert_eq!(end(day + 7 * H, day + 7 * H), Some(t(day + 7 * H + 2_700)));
    assert_eq!(
        end(day + 7 * H, day + 7 * H + 1_200),
        Some(t(day + 7 * H + 2_700))
    );
    let ten_past_eight = day + 8 * H + 600;
    let departure = day + 8 * H + 2_400;
    assert_eq!(end(ten_past_eight, ten_past_eight), Some(t(departure)));
    assert_eq!(end(ten_past_eight, departure - 1), Some(t(departure)));
    assert_eq!(
        end(ten_past_eight, departure),
        Some(t(ten_past_eight + 2_700))
    );
    assert_eq!(
        end(day + 21 * H + 1_800, day + 21 * H + 1_800),
        Some(t(day + 22 * H))
    );
    assert_eq!(
        end(day + 22 * H + 1_800, day + 22 * H + 1_800),
        Some(t(day + 22 * H + 1_800))
    );
    assert_eq!(
        end_on((&long, 1), day + 7 * H, day + 7 * H),
        Some(t(departure))
    );
    let half_past_five = day + 17 * H + 1_800;
    assert_eq!(
        end_on((&long, 1), half_past_five, half_past_five),
        Some(t(half_past_five + 4 * H))
    );
    // Not a time-filler: no such end.
    let (houses, house) = tune_house(&data);
    let mut chosen = fun(at_screen(day + 7 * H), day + 7 * H);
    chosen.action = chosen.action.map(|action| Action {
        filler: false,
        ..action
    });
    let none = filler_end_of(
        (house, houses),
        (born, key, 7),
        &chosen,
        &taken,
        (t(day + 7 * H), t(day + 7 * H)),
    );
    assert_eq!(none, None);
}

/// A bed only for children under 6 (a cot for two in the den), a coffee in the hall for the
/// energy of everyone else (+600 an hour for 5 min), the screen of the tune house; the essential
/// kinds in the hall.
const COT_KINDS: &str = r#"
  {"id": "test:cot", "slots": 2, "offers": [
    {"id": "sleep", "sleep": true, "min_age": 0, "max_age": 5}]},
  {"id": "test:coffee", "slots": 1, "offers": [
    {"id": "drink", "rates": {"energy": 600}, "seconds": 300, "min_age": 0, "max_age": 130}]},
  {"id": "test:screen", "slots": 2, "offers": [
    {"id": "watch", "rates": {"fun": 100}, "seconds": 2700, "min_age": 0, "max_age": 130,
     "fallback": true}]}"#;

/// Rooms hall 0 and den 1, 3 s apart; objects the cot 0 and the screen 1 in the den, the coffee 2
/// and the essential ones 3 to 7 in the hall.
const COT: &str = r#"{
  "id": "test:cot", "capacity": 2,
  "rooms": [{"id": "hall", "entrance": true}, {"id": "den"}],
  "child_day_room": "den", "child_night_room": "den", "dwell_room": "den",
  "doors": [{"a": "hall", "b": "den", "seconds": 3}],
  "objects": [{"kind": "test:cot", "room": "den"}, {"kind": "test:screen", "room": "den"},
    {"kind": "test:coffee", "room": "hall"}, ESSENTIALS]
}"#;

/// No time-filler in the night: the couple's first morning is 6:00 of a Saturday (moved in at
/// 23:59:59 of day 4, a Friday), which lies in their night until 7:00, and no bed is open to them
/// (the cot is for children under 6), so each idles in the hall though the screen is free; by
/// day, at 6:00 of a Tuesday, the same couple takes the screen as its time-filler.
#[test]
fn no_time_filler_in_the_night() {
    let data = day::house_data(&bladder_rates(0), COT_KINDS, COT);
    let born = [-30 * Y, -32 * Y];
    let (mut run, _) = Run::family(data.clone(), 7, "test:house", &born, 4);
    let saturday = 5 * D + 6 * H;
    run.advance(saturday + 1_800);
    let idle = LogPayload::Idle {
        house: DataId::new("test:cot").expect("id"),
        room: 0,
        needs: MORNING_VALUES,
        busy: Vec::new(),
        waits: None,
        held: Vec::new(),
        tier: Vec::new(),
        too_short: None,
    };
    for id in [1, 2] {
        assert_eq!(
            doings(&run, id, saturday, saturday + 1_800),
            [(0, idle.clone())],
            "{id}"
        );
    }
    let (mut run, _) = Run::moved_in(data);
    run.advance(MORNING + 10);
    let watch = |slot: u8| LogPayload::ActionStarted {
        house: DataId::new("test:cot").expect("id"),
        object: 1,
        offer: 0,
        slot,
        room: 0,
        walk_secs: 3,
        needs: MORNING_VALUES,
        busy: Vec::new(),
        commanded: false,
        held: Vec::new(),
        tier: Vec::new(),
        filler: true,
    };
    for (id, slot) in [(1, 0), (2, 1)] {
        let walked = doings(&run, id, MORNING, MORNING + 10);
        assert_eq!(walked.first(), Some(&(0, watch(slot))), "{id}");
    }
}

// ---------------------------------------------------------------------------------------
// The waiter keeps its place (point 3).

/// A tub for one in the hall that fills hygiene (+2 800 an hour) and the bladder (+12 000) for
/// 15 min, and a bed for two in the den; the essential kinds in the hall.
const TUB_KINDS: &str = r#"
  {"id": "test:tub", "slots": 1, "offers": [
    {"id": "bathe", "rates": {"hygiene": 2800, "bladder": 12000}, "seconds": 900,
     "min_age": 0, "max_age": 130}]},
  {"id": "test:bed", "slots": 2, "offers": [
    {"id": "sleep", "sleep": true, "min_age": 0, "max_age": 130}]}"#;

/// Rooms hall 0 and den 1, 3 s apart; the tub 0 in the hall, the bed 1 in the den, the essential
/// ones 2 to 6 (table, fridge, toilet 4, shower, sofa) in the hall.
const TUB: &str = r#"{
  "id": "test:tub", "capacity": 2,
  "rooms": [{"id": "hall", "entrance": true}, {"id": "den"}],
  "child_day_room": "den", "child_night_room": "den", "dwell_room": "den",
  "doors": [{"a": "hall", "b": "den", "seconds": 3}],
  "objects": [{"kind": "test:tub", "room": "hall"}, {"kind": "test:bed", "room": "den"},
    ESSENTIALS]
}"#;

/// The couple of `common::day` in the tub house, awake the bladder falling 1 400 an hour and fun
/// 600, the rest still. At 6:00 the tub brings far more than 40 000 (300 · 300 for hygiene, 500 ·
/// 500 for the bladder); the woman takes it, until 6:15:05, and the man waits for it from 6:00:00.
/// His bladder reaches 400 at ⌈3 600 · 100 / 1 400⌉ = 258 s, his fun 700 at 600 s: he keeps his
/// place, no choice and no entry. At 150, 900 s on (6:15:00), he chooses: his bladder is critical,
/// the tub still taken, so he walks to the free toilet (it fills nothing, but raises the
/// critical bladder), and leaves the tub's list; his needs then [600, 900, 150, 700, 650].
#[test]
fn a_waiter_keeps_its_place() {
    let bands = |rate: i32| [rate; 4];
    let rates = json!({
        "awake": {"hunger": bands(0), "energy": bands(0), "bladder": bands(-1_400),
                  "hygiene": bands(0), "fun": bands(-600)},
        "asleep": {"hunger": bands(0), "energy": bands(100), "bladder": bands(0),
                   "hygiene": bands(0), "fun": bands(0)},
        "comfort": [1000, 1000, 1000, 1000, 1000]
    })
    .to_string();
    let (mut run, home) = Run::moved_in(day::house_data(&rates, TUB_KINDS, TUB));
    let house = DataId::new("test:tub").expect("id");
    run.advance(MORNING + 899);
    let waits = &run.world.state().house().waits;
    assert_eq!(waits.get(&(home, 0)), Some(&vec![(t(MORNING), sim(2))]));
    run.advance(MORNING + 960);
    let toilet = LogPayload::ActionStarted {
        house: house.clone(),
        object: 4,
        offer: 0,
        slot: 0,
        room: 0,
        walk_secs: 5,
        needs: [600, 900, 150, 700, 650],
        busy: vec![0],
        commanded: false,
        held: Vec::new(),
        tier: Vec::new(),
        filler: false,
    };
    let expected = [
        (
            0,
            LogPayload::Idle {
                house: house.clone(),
                room: 0,
                needs: MORNING_VALUES,
                busy: vec![0],
                waits: Some(0),
                held: Vec::new(),
                tier: Vec::new(),
                too_short: None,
            },
        ),
        (
            900,
            LogPayload::NeedCritical {
                need: sim_core::house::data::Need::Bladder,
                level: 150,
            },
        ),
        (900, toilet),
        (905, LogPayload::Reached { object: 4 }),
    ];
    assert_eq!(doings(&run, 2, MORNING, MORNING + 906), expected);
    assert!(run.world.state().house().waits.is_empty());
}

// ---------------------------------------------------------------------------------------
// No scored offer across the departure (point 4).

/// Which offers of the repository's flat an adult of 30 at the morning values in the hall still
/// has 600 s before its departure (rooms hall 0, kitchen 1, living 2, bath 3; objects table 0,
/// fridge 1, sofa 2, television 3, bookshelf 4, toilet 5, shower 6): the table (4 s and 20 min),
/// the sofa's `sit` (3 s and 30 min), the television (3 s and 45 min), the bookshelf (3 s and 60
/// min) and the shower (3 s and 15 min) would still run then and are closed; the fridge (4 s and
/// 5 min) and the toilet (3 s and the 150 s its 12 000 an hour need from 500 to full) are open.
/// The toilet closes 153 s before the departure, from 500; from 900, 30 s to full, its shortest
/// 60 s count, so 63 s. The shower with hygiene critical (150) stays open. Without a departure
/// nothing is closed (the sofa's sleep is closed by day).
#[test]
fn an_offer_that_runs_into_the_departure_is_closed() {
    let houses = common::repository_houses();
    let flat = houses
        .house(&DataId::new("core:flat").expect("id"))
        .expect("the flat");
    let open = |values: [u16; 5], depart_in: Option<i64>| -> Vec<u16> {
        (score(flat, &houses, (30, values, 0), (None, depart_in), no_mood).iter())
            .map(|scored| scored.object)
            .collect()
    };
    assert_eq!(open(MORNING_VALUES, Some(600)), [1, 5]);
    assert_eq!(open(MORNING_VALUES, None), [0, 1, 2, 3, 4, 5, 6]);
    let toilet = |bladder: u16, depart_in: i64| {
        let kind = houses
            .kind(&DataId::new("core:toilet").expect("id"))
            .expect("a kind");
        let values = [1000, 1000, bladder, 1000, 1000];
        crosses_departure(
            &kind.offers[0],
            houses.rates(),
            (30, values),
            3,
            Some(depart_in),
        )
    };
    // Bladder 500: on the walk of 3 s it falls at most ⌈120 · 3 / 3 600⌉ = 1, to 499, and from
    // there the toilet (+12 000 an hour until full) needs ⌈501 · 3 600 / 12 000⌉ = 151 s: it
    // still runs 154 s after the choice. At 900 the shortest action, 60 s, outlasts the 31 s.
    assert!(!toilet(500, 155) && toilet(500, 154));
    assert!(!toilet(900, 64) && toilet(900, 63));
    let dirty = [600, 900, 500, 150, 800];
    assert_eq!(open(dirty, Some(600)), [1, 5, 6]);
}

/// The departure closes an offer by the end it really has, the walk included (ADR-0026, addendum of
/// 2026-11-04 on comfort and the time-filler): an adult's bladder written down at 6:00 at 501,
/// falling 120 an hour (the repository's awake rate), is 500 at 6:00:57 (501 − ⌊120 · 57 / 3 600⌋).
/// From that value the repository's toilet (+12 000 an hour until full, at most 5 min) would need
/// 500 · 3 600 / 12 000 = 150 s, 153 s with its walk of 3 s, and end a second before a departure
/// 154 s away. But on the walk the bladder falls to 499 by 6:01:00 (501 − 120 · 60 / 3 600), so the
/// action written down at its arrival ends ⌈501 · 3 600 / 12 000⌉ = 151 s later, at the departure
/// itself (`action_end`). The walk lowers the need it fills by at most ⌈120 · 3 / 3 600⌉ = 1, the
/// fastest awake rate of any age band over its seconds: the toilet is closed 154 s before the
/// departure and open 155 s before it.
#[test]
fn the_departure_closes_a_toilet_by_its_end_after_the_walk() {
    let houses = common::repository_houses();
    let kind = houses
        .kind(&DataId::new("core:toilet").expect("id"))
        .expect("a kind");
    let offer = &kind.offers[0];
    let (written, chosen, arrived) = (t(MORNING), t(MORNING + 57), t(MORNING + 60));
    let falling = NeedSample::new(501, written, -120);
    assert_eq!(
        (falling.value_at(chosen), falling.value_at(arrived)),
        (500, 499)
    );
    let values = [1000, 1000, 500, 1000, 1000];
    assert_eq!(lasts(offer, houses.rates(), 2, values), Some(150));
    let mut person = InHouse {
        needs: [1000, 1000, 0, 1000, 1000].map(|value| NeedSample::new(value, arrived, 0)),
        place: Place::Room(3),
        action: Some(Action {
            object: 5,
            offer: 0,
            slot: 0,
            filler: false,
        }),
        due: None,
        decisions: sim_core::house::state::Decisions { day: 1, count: 1 },
        collapsed: false,
        reserve: Reserve::full(1000, arrived),
    };
    person.needs[2] = falling.with_rate(arrived, 12_000);
    let born = t(-30 * Y);
    let ends = action_end(&person, offer, houses.day(), born);
    assert_eq!(ends, Some(t(MORNING + 57 + 154)));
    let closed =
        |depart_in: i64| crosses_departure(offer, houses.rates(), (30, values), 3, Some(depart_in));
    assert!(closed(154) && !closed(155));
}

/// The couple in the tune house with the bladder falling only 44 an hour: at 6:00 each takes the
/// screen as its time-filler, 3 s away, and again each time its 45 minutes are over (ADR-0026,
/// addendum of 2026-11-05 on the time-filler's length): at 6:45:03, 7:30:08 and 8:15:13, each
/// time 5 s within the den, she on her place 0, he on his 1; the bladder at 467, 434 and 401
/// (500 − 44 · s / 3 600), fun from 800 up 75 a time to 875, 950 and full. The loo would bring
/// 40 000 at the most mood from the deficit 609,
/// 8 919 s on (8:28:39), and surely only from 744, 19 964 s on; from 8:34:57 on it would still
/// run at their departure at 8:40 (3 s to it and its 5 min), so it is weighed only before. The
/// whole minutes 8:29 to 8:34 bring at most 609 to 612 · 60 · 110 / 100 − 150, enough only at +10:
/// her moods there are below that (her first such minute would be 8:45), his are +3, +7, +6, −3,
/// −7, +6. So no choice is planned, the last filler runs until the departure ends it (its 45
/// minutes would end at 9:00:18), and both leave on time. Without the departure his next choice from 8:00 would be 8:39:00, his mood +9 with the
/// deficit 616 (616 · 60 · 109 / 100 − 150 = 40 136), a loo of 5 min the departure would cut.
#[test]
fn no_choice_runs_into_the_departure() {
    let data = tune_data(-44);
    let (houses, house) = tune_house(&data);
    let (mut run, _) = Run::moved_in(data.clone());
    let depart = D + 8 * H + 2_400;
    run.advance(depart + 1);
    for (id, slot) in [(1, 0), (2, 1)] {
        let again = |at: i64, needs: [u16; 5]| {
            [
                (at, ended(SCREEN, EndReason::Done, needs)),
                (at, started((SCREEN, slot), (1, 5), needs, true)),
                (at + 5, LogPayload::Reached { object: SCREEN }),
            ]
        };
        let mut expected = vec![
            (0, started((SCREEN, slot), (0, 3), MORNING_VALUES, true)),
            (3, LogPayload::Reached { object: SCREEN }),
        ];
        expected.extend(again(2_703, [600, 900, 467, 700, 875]));
        expected.extend(again(5_408, [600, 900, 434, 700, 950]));
        expected.extend(again(8_113, [600, 900, 401, 700, 1000]));
        expected.push((
            depart - MORNING,
            ended(SCREEN, EndReason::Obligation, [600, 900, 383, 700, 1000]),
        ));
        assert_eq!(doings(&run, id, MORNING, depart + 1), expected, "{id}");
        let left = (run.log.iter()).find(|entry| {
            entry.subjects == [Subject::Sim(sim(id))]
                && matches!(entry.payload, LogPayload::LeftHome { .. })
        });
        let left = left.map(|entry| (entry.t.as_secs(), entry.payload.clone()));
        assert_eq!(
            left,
            Some((
                depart,
                LogPayload::LeftHome {
                    place: sim_core::house::state::Duty::Work,
                    late_secs: 0
                }
            )),
            "{id}"
        );
    }
    let d = |at: i64| 500 + 44 * (at - MORNING) / 3_600;
    let minutes: Vec<i64> = (29..=39).map(|minute| D + 8 * H + minute * 60).collect();
    let his: Vec<i64> = minutes.iter().map(|&at| couple_mood(1, at, LOO)).collect();
    assert_eq!(his, [3, 7, 6, -3, -7, 6, -5, -8, 5, 0, 9]);
    let reaches = |index: u64| {
        (minutes.iter()).find(|&&at| loo(d(at), couple_mood(index, at, LOO)) >= ENOUGH)
    };
    assert_eq!((reaches(0), reaches(1)), (None, Some(&(D + 8 * H + 2_340))));
    assert_eq!(loo(d(D + 8 * H + 2_340), 9), 40_136);
    // His next choice from the den at 8:00, his bladder written down at 6:00 at 500 falling 44
    // an hour, with and without the departure.
    let key = LineageKey::founder(&DataId::new("test:house").expect("id"), 1);
    let mut needs = MORNING_VALUES.map(|value| NeedSample::new(value, t(MORNING), 0));
    needs[2] = NeedSample::new(500, t(MORNING), -44);
    let born = t(-32 * Y);
    let free = |object: u16| object != SCREEN;
    let moods = |at: SimTime, object: u16, offer: u16| mood(7, key, at, (object, offer), MOOD);
    let eight = t(D + 8 * H);
    let leaves = |_: SimTime| Some(t(depart));
    let with = next_choice(
        (house, houses),
        (born, &needs, 1),
        free,
        (eight, eight),
        moods,
        leaves,
    );
    assert_eq!(with, None);
    let without = next_choice(
        (house, houses),
        (born, &needs, 1),
        free,
        (eight, eight),
        moods,
        |_| None,
    );
    assert_eq!(without, Some(t(D + 8 * H + 2_340)));
}

// ---------------------------------------------------------------------------------------
// Saves and the loader (point 6).

/// Saved on the walk to the screen (6:00:01), at it (6:05:00), while the woman is at the loo and
/// the man's end was planned anew (6:10:00) and when it stands at 6:14:39 (6:14:20), each save of
/// the tune house of `a_time_filler_lasts_until_the_next_choice` loads, saves to the same bytes
/// and runs on to 7:00 as the uninterrupted morning, with the same state hash and entries; the
/// places hold after every second with events.
#[test]
fn a_save_during_a_time_filler_runs_on_the_same() {
    let data = tune_data(-1_000);
    let mut whole = Run::moved_in(data.clone()).0;
    whole.step(MORNING + H, places_hold);
    for saved_at in [MORNING + 1, MORNING + 300, MORNING + 600, MORNING + 860] {
        let (mut run, _) = Run::moved_in(data.clone());
        run.advance(saved_at);
        let bytes = run.world.save(&run.out.year_hashes).expect("saves");
        let (mut loaded, _) = World::load(&bytes, data.clone()).expect("loads");
        assert_eq!(
            loaded.save(&run.out.year_hashes).expect("saves"),
            bytes,
            "{saved_at}"
        );
        loaded.advance_until(&mut YearOut::default(), t(MORNING + H), &mut |_| false);
        let entries = |log: &[LogEntry]| serde_json::to_value(log).expect("json");
        let after: Vec<LogEntry> = (whole.log.iter())
            .filter(|entry| entry.t.as_secs() > saved_at)
            .cloned()
            .collect();
        assert_eq!(entries(&loaded.take_log()), entries(&after), "{saved_at}");
        assert_eq!(loaded.state_hash(), whole.world.state_hash(), "{saved_at}");
    }
}

/// The run `at_save` gives, saved there at the clock `saved_at`, loads, saves to the same bytes
/// and runs on to `until` as the same run uninterrupted: the same entries after the save and the
/// same state hash.
fn runs_on_the_same(data: &Data, saved_at: i64, until: i64, at_save: impl Fn() -> Run) {
    let mut whole = at_save();
    let before = whole.log.len();
    whole.advance(until);
    let run = at_save();
    let bytes = run.world.save(&run.out.year_hashes).expect("saves");
    let (mut loaded, _) = World::load(&bytes, data.clone()).expect("loads");
    assert_eq!(
        loaded.save(&run.out.year_hashes).expect("saves"),
        bytes,
        "{saved_at}"
    );
    loaded.advance_until(&mut YearOut::default(), t(until), &mut |_| false);
    let entries = |log: &[LogEntry]| serde_json::to_value(log).expect("json");
    assert_eq!(
        entries(&loaded.take_log()),
        entries(&whole.log[before..]),
        "{saved_at}"
    );
    assert_eq!(loaded.state_hash(), whole.world.state_hash(), "{saved_at}");
}

/// A bound of the next choice at the second after the clock is the run's bound too (ADR-0026,
/// addendum of 2026-11-04 on comfort and the time-filler): the loader recomputes a plan from the
/// clock's next second, with the bounds after the clock, as the run made it with those after its
/// second. The couple of `no_choice_runs_into_the_departure` at the screen until their departure at
/// 8:40, which closes the loo from 8:34:57 on, so that their time-fillers end only at it: saved at
/// 8:39:59, the departure of the next second still closes it, and the loaded morning runs on as the
/// whole.
#[test]
fn a_save_the_second_before_the_departure_runs_on_the_same() {
    let data = tune_data(-44);
    let depart = D + 8 * H + 2_400;
    runs_on_the_same(&data, depart - 1, depart + 600, || {
        let (mut run, _) = Run::moved_in(data.clone());
        run.advance(depart - 1);
        run
    });
}

/// The couple of `no_choice_runs_into_the_departure`, the woman born so that she turns 31 at
/// 7:00 of day 1 (her work by the same draw, full time, as her age at the day's plan, 5:00, is
/// 30). At the screen from 6:00:03 nothing brings 40 000 before 7:00 (the loo only from 8:28:39 at
/// the most mood), and her birthday, where her offers and rates may change, comes before her
/// bedtime: her time-filler ends then, with the bladder 456 (500 − 44) and fun 899 (800 +
/// ⌊100 · 3 597 / 3 600⌋), and she takes the screen again, 5 s within the den. Saved at 6:59:59,
/// the loaded morning runs on as the whole.
#[test]
fn a_save_the_second_before_a_birthday_that_ends_a_time_filler_runs_on_the_same() {
    let data = tune_data(-44);
    let birthday = D + 7 * H;
    let born = [birthday - 31 * Y, -32 * Y];
    let family = || Run::family(data.clone(), 7, "test:house", &born, 0).0;
    let mut whole = family();
    whole.advance(birthday + 1);
    let at_seven = [600, 900, 456, 700, 899];
    assert_eq!(
        doings(&whole, 1, birthday - 1, birthday + 1),
        [
            (1, ended(SCREEN, EndReason::Done, at_seven)),
            (1, started((SCREEN, 0), (1, 5), at_seven, true)),
        ]
    );
    runs_on_the_same(&data, birthday - 1, D + 9 * H, || {
        let mut run = family();
        run.advance(birthday - 1);
        run
    });
}

/// As `a_save_the_second_before_a_birthday_that_ends_a_time_filler_runs_on_the_same` with a
/// screen that is no time-filler: at 6:00 the woman stands idle, and her next choice is her
/// birthday at 7:00, where she is idle again. Saved at 6:59:59, the loaded morning runs on as the
/// whole.
#[test]
fn a_save_the_second_before_a_birthday_that_is_a_choice_runs_on_the_same() {
    let kinds = TUNE_KINDS.replace("\"fallback\": true", "\"fallback\": false");
    let data = day::house_data(&bladder_rates(-44), &kinds, TUNE);
    let birthday = D + 7 * H;
    let born = [birthday - 31 * Y, -32 * Y];
    let family = || Run::family(data.clone(), 7, "test:house", &born, 0).0;
    let mut whole = family();
    whole.advance(birthday + 1);
    let idle: Vec<i64> = (doings(&whole, 1, MORNING, birthday + 1).into_iter())
        .filter(|(_, payload)| matches!(payload, LogPayload::Idle { .. }))
        .map(|(at, _)| at)
        .collect();
    assert_eq!(idle, [0, H]);
    runs_on_the_same(&data, birthday - 1, D + 9 * H, || {
        let mut run = family();
        run.advance(birthday - 1);
        run
    });
}

/// A search that would start in a night that begins after the second it is planned in finds
/// nothing: that night's start, the bedtime, still bounds it and makes the person choose
/// (ADR-0026, addendum of 2026-11-04 on comfort and the time-filler). The woman of 30 in the den
/// of the tune house, her night from 22:00 of day 2, a Wednesday, to 6:00 of the Thursday; her
/// bladder 400 at 21:59:59, falling 30 an hour, every other need still. The loo brings 40 000 at
/// the most mood only from the deficit 609 (609 · 66 − 150 = 40 044; 608: 39 978), 1 080 s on
/// (600 + ⌊30 · 1 080 / 3 600⌋), at 22:17:59: planned at 21:59:59 from then, nothing comes before
/// her bedtime. Planned in that second from 22:00:00, as a world saved at 21:59:59 plans it,
/// nothing either. Planned in her night, at 22:00:00 from then or from 22:00:01 as a world saved
/// then, the search looks from the night's end on: at 6:00:00 her bladder is 400 − ⌊30 · 28 801 /
/// 3 600⌋ = 160, and the loo brings 840 · 60 · 90 / 100 − 150 = 45 210 even at the least mood.
#[test]
fn a_plan_made_before_the_night_ends_at_its_bedtime() {
    let data = tune_data(-30);
    let (houses, house) = tune_house(&data);
    let born = t(-30 * Y);
    let day = 2 * D;
    let bedtime = day + 22 * H;
    let mut needs = MORNING_VALUES.map(|value| NeedSample::new(value, t(bedtime - 1), 0));
    needs[2] = NeedSample::new(400, t(bedtime - 1), -30);
    let planned = |start: i64, after: i64| {
        next_choice(
            (house, houses),
            (born, &needs, 1),
            |_| true,
            (t(start), t(after)),
            |_, _, _| 0,
            |_| None,
        )
    };
    assert!(loo(608, 10) < ENOUGH && loo(609, 10) >= ENOUGH);
    assert_eq!(
        (600 + 30 * 1_079 / 3_600, 600 + 30 * 1_080 / 3_600),
        (608, 609)
    );
    assert_eq!((400 - 30 * 28_801 / 3_600, loo(840, -10)), (160, 45_210));
    assert_eq!(planned(bedtime - 1, bedtime - 1), None);
    assert_eq!(planned(bedtime, bedtime - 1), None);
    let morning = Some(t(day + D + 6 * H));
    assert_eq!(planned(bedtime, bedtime), morning);
    assert_eq!(planned(bedtime + 1, bedtime), morning);
}

/// The couple of 70 and 72 (retired: no duty takes them out) in the tune house without its
/// time-filler (the screen no `fallback`) and with its hall a room one does not stay in, so that
/// one idle there walks the 3 s to the den; their bladder falling 7 an hour, every other need
/// still; their night from 21:30 to 6:00 of a weekday.
fn retired_data() -> Data {
    let kinds = TUNE_KINDS.replace("\"fallback\": true", "\"fallback\": false");
    let hall = "\"entrance\": true}";
    assert!(TUNE.contains(hall));
    let house = TUNE.replace(hall, "\"entrance\": true, \"dwell\": false}");
    day::house_data(&bladder_rates(-7), &kinds, &house)
}

fn retired(data: &Data) -> Run {
    Run::family(data.clone(), 7, "test:house", &[-70 * Y, -72 * Y], 0).0
}

/// Their bedtime on day 1, a Tuesday: 21:30, 55 800 s after their first morning.
const RETIRED_BEDTIME: i64 = D + 21 * H + 1_800;

/// Idle in `room` of the tune house with `needs`, waiting for nothing, by day.
fn idle(room: u8, needs: [u16; 5]) -> LogPayload {
    LogPayload::Idle {
        house: DataId::new("test:tune").expect("id"),
        room,
        needs,
        busy: Vec::new(),
        waits: None,
        held: Vec::new(),
        tier: Vec::new(),
        too_short: None,
    }
}

/// One idle whose search finds nothing before its bedtime has no choice planned: the bedtime
/// makes it choose. Saved in the second before it, the loaded world plans the same (ADR-0026,
/// addendum of 2026-11-04 on comfort and the time-filler): its search would start in the night,
/// which begins after the clock, so the bedtime still bounds it and nothing is planned, where from
/// the night's end on it would find the loo the next morning, a choice the run never planned. The
/// couple of `retired_data`: at 6:00 of day 1 nothing brings 40 000 (the loo 500 · 66 − 250 =
/// 32 750 at most), so each idles in the hall and walks the 3 s to the den. Their bladder reaches
/// the deficit 609 the loo needs from the den at the most mood only ⌈109 · 3 600 / 7⌉ = 56 058 s
/// on, at 21:34:18, after their bedtime at 21:30:00: no choice is planned. At 400, ⌈100 · 3 600 /
/// 7⌉ = 51 429 s on (20:17:09), each chooses again and idles in the den, with nothing planned
/// again. At 21:30:00, with the bladder 392 (500 − ⌊7 · 55 800 / 3 600⌋), under its comfort (1000
/// in a test house), she first takes the loo, 3 s away, which fills it by 60 (ADR-0026, addendum
/// of 2026-11-05 on the time-filler's length: in the night what fills hunger or the bladder under
/// its comfort comes before a bed), and he, the loo taken, the bed's first place, 5 s within the
/// den. Saved at 21:29:59, the world loads, saves to the same bytes and runs on to 9:00 of the
/// next day as the whole.
#[test]
fn an_idle_person_saved_before_its_bedtime_runs_on_the_same() {
    let data = retired_data();
    let bedtime = RETIRED_BEDTIME;
    assert_eq!(bedtime - MORNING, 55_800);
    assert_eq!((109 * 3_600 + 6) / 7, 56_058);
    assert_eq!(MORNING + 56_058, D + 21 * H + 34 * 60 + 18);
    assert_eq!((100 * 3_600 + 6) / 7, 51_429);
    assert_eq!(MORNING + 51_429, D + 20 * H + 17 * 60 + 9);
    assert_eq!(500 - 7 * 55_800 / 3_600, 392);
    assert!(loo(608, 10) < ENOUGH && 500 * 66 - 250 < ENOUGH);
    let mut whole = retired(&data);
    whole.advance(bedtime + 1);
    let low = [600, 900, 400, 700, 800];
    let night = [600, 900, 392, 700, 800];
    let loo_taken = match started((2, 0), (1, 5), night, false) {
        LogPayload::ActionStarted {
            house,
            object,
            offer,
            slot,
            room,
            walk_secs,
            needs,
            commanded,
            held,
            tier,
            filler,
            ..
        } => LogPayload::ActionStarted {
            house,
            object,
            offer,
            slot,
            room,
            walk_secs,
            needs,
            busy: vec![LOO],
            commanded,
            held,
            tier,
            filler,
        },
        other => other,
    };
    for (id, first) in [(1, started((LOO, 0), (1, 3), night, false)), (2, loo_taken)] {
        assert_eq!(
            doings(&whole, id, MORNING, bedtime + 1),
            [
                (0, idle(0, MORNING_VALUES)),
                (51_429, idle(1, low)),
                (55_800, first),
            ],
            "{id}"
        );
    }
    runs_on_the_same(&data, bedtime - 1, 2 * D + 9 * H, || {
        let mut run = retired(&data);
        run.advance(bedtime - 1);
        run
    });
}

/// A walk's search starts at its arrival, and its bounds are those after the second before it,
/// whenever the walk's plan is made (ADR-0026, addendum of 2026-11-04 on comfort and the
/// time-filler): a walk to the room one stays in that arrives in the night looks from the night's
/// end on, planned before the night, in the clock before the night or in it, as a loaded world
/// plans it; one that arrives at the bedtime itself plans nothing, as the bedtime comes first. The
/// couple of `retired_data`: at the clock 21:24:54, 306 s before their bedtime, she is told to use
/// the loo and he to watch the screen in the den for 45 min. She walks the 3 s from the den, and
/// the loo's 5 minutes end at 21:29:58 (21:24:55 + 3 + 300) with her bladder 453: 393 at her
/// arrival (500 − ⌊7 · 55 498 / 3 600⌋) and 60 more. Nothing brings her 40 000 (the loo 547 · 66
/// − 250 = 35 852 at most), so she idles in the hall and walks the 3 s to the den, through the
/// bedtime, which changes nothing for either; she arrives at 21:30:01, in her night, and takes the
/// loo again, the bladder still under its comfort (ADR-0026, addendum of 2026-11-05 on the
/// time-filler's length). Saved at 21:29:59 and at 21:30:00, while she walks, the world loads, saves
/// to the same bytes and runs on to 9:00 of the next day as the whole. By hand, on that walk with
/// her bladder 453 at 21:29:58 falling 30 an hour, so that from the night's end the loo surely
/// brings enough at 6:00:00 (453 − ⌊30 · 30 602 / 3 600⌋ = 198, 802 · 54 − 150 = 43 158 at the
/// least mood): planned in her second, in the clock before the night and in it, her next choice
/// is 6:00:00; arriving at 21:30:00, none.
#[test]
fn a_walk_into_the_night_saved_on_its_way_runs_on_the_same() {
    let data = retired_data();
    let bedtime = RETIRED_BEDTIME;
    assert_eq!(500 - 7 * (55_800 - 302) / 3_600, 393);
    assert_eq!(393 + 720 * 300 / 3_600, 453);
    assert_eq!(547 * 66 - 250, 35_852);
    assert_eq!((453 - 30 * 30_602 / 3_600, loo(802, -10)), (198, 43_158));
    let clock = bedtime - 306;
    let told = || {
        let mut run = retired(&data);
        run.advance(clock);
        for (id, object) in [(1, LOO), (2, SCREEN)] {
            run.submit(Input::Command(Command {
                sim: sim(id),
                object,
                offer: 0,
            }));
        }
        run
    };
    let mut whole = told();
    whole.advance(bedtime + 2);
    let washed = [600, 900, 453, 700, 800];
    assert_eq!(
        doings(&whole, 1, bedtime - 2, bedtime + 2),
        [
            (0, ended(LOO, EndReason::Done, washed)),
            (0, idle(0, washed)),
            (3, started((LOO, 0), (1, 3), washed, false)),
        ]
    );
    assert!(doings(&whole, 2, bedtime - 2, bedtime + 2).is_empty());
    // Her next choice on her walk, planned in her second, in the clock before the night and in it.
    let key = LineageKey::founder(&DataId::new("test:house").expect("id"), 0);
    let mut needs = washed.map(|value| NeedSample::new(value, t(bedtime - 2), 0));
    needs[2] = NeedSample::new(453, t(bedtime - 2), -30);
    let walking = |arrive: i64| InHouse {
        needs,
        place: Place::Walking {
            from: 0,
            to: 1,
            depart: t(bedtime - 2),
            arrive: t(bedtime + arrive),
        },
        action: None,
        due: None,
        decisions: sim_core::house::state::Decisions { day: 1, count: 4 },
        collapsed: false,
        reserve: Reserve::full(1000, t(bedtime - 2)),
    };
    let (houses, house) = tune_house(&data);
    let who = (t(-70 * Y), key, 7);
    let none = BTreeSet::new();
    let next = |arrive: i64, (start, after): (i64, i64)| {
        let from = (t(bedtime + start), t(bedtime + after));
        next_choice_of((house, houses), who, &walking(arrive), &none, from)
    };
    for planned in [(-2, -2), (0, -1), (1, 0)] {
        assert_eq!(next(1, planned), Some(t(2 * D + 6 * H)), "{planned:?}");
    }
    assert_eq!((next(0, (-2, -2)), next(0, (0, -1))), (None, None));
    for saved_at in [bedtime - 1, bedtime] {
        runs_on_the_same(&data, saved_at, 2 * D + 9 * H, || {
            let mut run = told();
            run.advance(saved_at);
            run
        });
    }
}

/// A bath for one in the hall (hygiene +1 000 an hour for 2 332 s), a screen for two in the den
/// that is the time-filler (fun +1 an hour for 45 min), a bed for two in the den, and the five
/// essential kinds in the hall, each raising its need by 1 an hour for a minute, the sofa
/// hygiene: nothing but the screen raises fun.
const WAIT_KINDS: &str = r#"[
  {"id": "test:bath", "slots": 1, "offers": [
    {"id": "soak", "rates": {"hygiene": 1000}, "seconds": 2332, "min_age": 0, "max_age": 130}]},
  {"id": "test:screen", "slots": 2, "offers": [
    {"id": "watch", "rates": {"fun": 1}, "seconds": 2700, "min_age": 0, "max_age": 130,
     "fallback": true}]},
  {"id": "test:bed", "slots": 2, "offers": [
    {"id": "sleep", "sleep": true, "min_age": 0, "max_age": 130}]},
  {"id": "core:dining_table", "slots": 1, "offers": [
    {"id": "eat", "rates": {"hunger": 1}, "seconds": 60, "min_age": 0, "max_age": 130}]},
  {"id": "core:fridge", "slots": 1, "offers": [
    {"id": "snack", "rates": {"hunger": 1}, "seconds": 60, "min_age": 0, "max_age": 130}]},
  {"id": "core:toilet", "slots": 1, "offers": [
    {"id": "use", "rates": {"bladder": 1}, "seconds": 60, "min_age": 0, "max_age": 130}]},
  {"id": "core:shower", "slots": 1, "offers": [
    {"id": "shower", "rates": {"hygiene": 1}, "seconds": 60, "min_age": 0, "max_age": 130}]},
  {"id": "core:sofa", "slots": 1, "offers": [
    {"id": "sit", "rates": {"hygiene": 1}, "seconds": 60, "min_age": 0, "max_age": 130}]}]"#;

/// Rooms hall 0 (the entrance) and den 1, 3 s apart; the bath 0, the screen 1 and the bed 2, the
/// essential ones 3 to 7 in the hall.
const WAIT_HOUSE: &str = r#"{
  "id": "test:wait", "capacity": 2,
  "rooms": [{"id": "hall", "entrance": true}, {"id": "den"}],
  "child_day_room": "den", "child_night_room": "den", "dwell_room": "den",
  "doors": [{"a": "hall", "b": "den", "seconds": 3}],
  "objects": [{"kind": "test:bath", "room": "hall"}, {"kind": "test:screen", "room": "den"},
    {"kind": "test:bed", "room": "den"},
    {"kind": "core:dining_table", "room": "hall"}, {"kind": "core:fridge", "room": "hall"},
    {"kind": "core:toilet", "room": "hall"}, {"kind": "core:shower", "room": "hall"},
    {"kind": "core:sofa", "room": "hall"}]
}"#;

/// A departure that waits for a time-filler ends it at its next choice from the departure on, which
/// the departure no longer closes, as a loaded world plans it (ADR-0026, addendum of 2026-11-04 on
/// comfort and the time-filler). The woman of 30 of `common::day` (full time, here from 6:50) with
/// a man of 70, who has no duty, in the house `WAIT_HOUSE`: awake only fun falls, 1 000 an hour
/// from 6:00 below 67, and fun weighs from its comfort 151 on. At 6:00 she takes the bath (5 s
/// within the hall, then 2 332 s; hygiene 700, 300 · 300 − 250), and he waits for it. It ends at
/// 6:38:57 with her fun at 151 (800 − ⌊1 000 · 2 337 / 3 600⌋), where nothing brings her 40 000:
/// she walks the 3 s to the screen as her time-filler, and arrives with fun 150, critical. The
/// screen would run past her departure, so nothing ends her filler before it began 4 h ago. At 6:50
/// her departure waits for the filler that raises her critical fun; from then the screen's other
/// place, free and open, brings her the critical points at once: her filler ends at 6:50:00 and she
/// leaves on time. Saved before, at and after the departure, the morning loads, saves to the same
/// bytes and runs on as the whole.
#[test]
fn a_departure_that_waits_for_a_time_filler_ends_it_at_once() {
    let day = include_str!("../../../data/house/day.json").replace(
        "\"full_time\": {\"depart\": 31200",
        "\"full_time\": {\"depart\": 24600",
    );
    let bands = |rate: i32| [rate; 4];
    let rates = json!({
        "awake": {"hunger": bands(0), "energy": bands(0), "bladder": bands(0),
                  "hygiene": bands(0), "fun": [-1000, -1000, -1000, 0]},
        "asleep": {"hunger": bands(0), "energy": bands(100), "bladder": bands(0),
                   "hygiene": bands(0), "fun": bands(0)},
        "comfort": [1000, 1000, 1000, 1000, 151]
    })
    .to_string();
    let houses = common::houses_from(&rates, &day, WAIT_KINDS, &[WAIT_HOUSE]);
    let data = common::death_data(1e-9, None, json!([])).with_houses(houses);
    let family = || Run::family(data.clone(), 7, "test:house", &[-30 * Y, -70 * Y], 0).0;
    let depart = MORNING + 3_000;
    let mut whole = family();
    whole.advance(depart + 60);
    let bathed = [600, 900, 500, 1000, 151];
    // Object, place, room, walk, needs and whether it is a time-filler.
    type Start = (u16, u8, u8, u32, [u16; 5], bool);
    let start_of = |payload: LogPayload| -> Start {
        match payload {
            LogPayload::ActionStarted {
                object,
                slot,
                room,
                walk_secs,
                needs,
                filler,
                ..
            } => (object, slot, room, walk_secs, needs, filler),
            _ => panic!("a start"),
        }
    };
    let hers = doings(&whole, 1, MORNING, depart + 60);
    let starts: Vec<(i64, Start)> = (hers.iter())
        .filter(|(_, payload)| matches!(payload, LogPayload::ActionStarted { .. }))
        .map(|(at, payload)| (*at, start_of(payload.clone())))
        .collect();
    assert_eq!(
        starts,
        [
            (0, (0, 0, 0, 5, MORNING_VALUES, false)),
            (2_337, (SCREEN, 0, 0, 3, bathed, true)),
        ]
    );
    let critical = LogPayload::NeedCritical {
        need: sim_core::house::data::Need::Fun,
        level: 150,
    };
    assert!(hers.contains(&(2_340, critical)));
    assert!(hers.contains(&(
        3_000,
        ended(SCREEN, EndReason::Done, [600, 900, 500, 1000, 150])
    )));
    let left = (whole.log.iter()).find(|entry| {
        entry.subjects == [Subject::Sim(sim(1))]
            && matches!(entry.payload, LogPayload::LeftHome { .. })
    });
    assert_eq!(
        left.map(|entry| (entry.t.as_secs(), entry.payload.clone())),
        Some((
            depart,
            LogPayload::LeftHome {
                place: sim_core::house::state::Duty::Work,
                late_secs: 0
            }
        ))
    );
    for saved_at in [depart - 1, depart, depart + 1] {
        runs_on_the_same(&data, saved_at, depart + 600, || {
            let mut run = family();
            run.advance(saved_at);
            run
        });
    }
}

/// An input at the clock plans the end of a time-filler anew in the very second it was due, as the
/// loader recomputes it from there (ADR-0026, addendum of 2026-11-04 on comfort and the
/// time-filler). In the tune house of `a_time_filler_lasts_until_the_next_choice` the woman's
/// time-filler ends at 6:09:00, when the loo brings enough. At the clock 6:08:59 the man is told to
/// use the loo: he walks there and takes its one place, so nothing brings her enough while he is in
/// it, and her end moves to 4 h after it began. When his 5 minutes there end at 6:14:03 the loo is
/// free again, her deficit 734 above 609 and no whole minute before 6:14:39, where her deficit 744
/// brings 40 026 at the least mood: then her time-filler ends, with fun 824 (800 + ⌊100 · 876 / 3
/// 600⌋), and she walks to the loo. Saved at the clock with the command, the morning loads, saves
/// to the same bytes and runs on as the whole.
#[test]
fn a_command_in_the_second_a_time_filler_ends_is_saved_and_runs_on_the_same() {
    let data = tune_data(-1_000);
    let clock = MORNING + 539;
    let told = || {
        let (mut run, _) = Run::moved_in(data.clone());
        run.advance(clock);
        run.submit(Input::Command(Command {
            sim: sim(2),
            object: LOO,
            offer: 0,
        }));
        run
    };
    let mut whole = told();
    whole.advance(MORNING + 900);
    let at_loo = [600, 900, 256, 700, 824];
    assert_eq!(
        doings(&whole, 1, clock + 1, MORNING + 900),
        [
            (339, ended(SCREEN, EndReason::Done, at_loo)),
            (339, started((LOO, 0), (1, 3), at_loo, false)),
            (342, LogPayload::Reached { object: LOO }),
        ]
    );
    runs_on_the_same(&data, clock, MORNING + H, told);
}

/// The loader's check of a time-filler (`consistent`): a person of the tune house at the screen as
/// its time-filler passes; at the loo, which is no `fallback` offer, as one, it is refused.
#[test]
fn a_time_filler_at_an_offer_that_is_none_is_refused() {
    let data = tune_data(-1_000);
    let (mut run, home) = Run::moved_in(data.clone());
    run.advance(MORNING + 300);
    let state = run.world.state();
    let alive = |household| household == home;
    let member = |sim: SimId| state.household_of(sim).zip(state.born(sim));
    let now = t(MORNING + 301);
    let mut house = state.house().clone();
    assert_eq!(
        consistent(&house, data.houses(), now, None, alive, member),
        Ok(())
    );
    let woman = house.sims.get_mut(&sim(1)).expect("her");
    assert_eq!(woman.action.map(|action| action.filler), Some(true));
    woman.action = Some(Action {
        object: LOO,
        offer: 0,
        slot: 0,
        filler: true,
    });
    woman.place = Place::Room(0);
    let refused = consistent(&house, data.houses(), now, None, alive, member);
    assert_eq!(refused, Err(Broken::Filler));
    // At the loo as an ordinary choice the same place passes this check.
    let woman = house.sims.get_mut(&sim(1)).expect("her");
    woman.action = woman.action.map(|action| Action {
        filler: false,
        ..action
    });
    assert_ne!(
        consistent(&house, data.houses(), now, None, alive, member),
        Err(Broken::Filler)
    );
}

//! The time-filler's length after the third playtest of 2026-11-05 (ADR-0026, addendum of
//! 2026-11-05 on the time-filler's length): a time-filler lasts its own length from its arrival
//! (`seconds` of its offer), and ends earlier at the next choice worth it, today's departure, the
//! bedtime and at the latest `fallback_max_seconds` after it began; among the free ones the
//! highest mood wins, then the shortest walk, then the smaller object and offer; one that would
//! end under `fallback_min_seconds` (600 s) after the choice, its walk included, is not taken.
//! Coming home lowers hunger no more; in the night what fills hunger or the bladder under its
//! comfort comes before a bed. The values by hand from the rates, the offers and the walks, the
//! moods from `mood` (their values from `tools/rng_reference.py`), the draws the rules name.

mod common;

use std::collections::BTreeSet;

use common::day::{self, MORNING, Run, places_hold};
use serde_json::json;
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::house::choice::{Bonus, Choice, Filler, choose, filler, fillers, mood, score};
use sim_core::house::data::{House, HouseData};
use sim_core::house::need::NeedSample;
use sim_core::house::reserve::Reserve;
use sim_core::house::state::{Decisions, InHouse, Place, filler_end_if, filler_possible};
use sim_core::ids::SimId;
use sim_core::lineage::LineageKey;
use sim_core::log::{EndReason, LogEntry, LogPayload, Subject};
use sim_core::time::{DAY, HOUR, SimTime, YEAR, calendar};
use sim_core::world::{World, YearOut};

const D: i64 = DAY.as_secs();
const H: i64 = HOUR.as_secs();
const Y: i64 = YEAR.as_secs();

/// The mood of the repository's day, `choice_mood_percent` of `data/house/day.json`.
const MOOD: u8 = 10;

const MORNING_VALUES: [u16; 5] = [600, 900, 500, 700, 800];

/// The terraced house of the repository: rooms hall 0, kitchen 1, living 2, wc 3, landing 4,
/// bath 5, bedroom 6, kids 7, small room 8; objects table 0, fridge 1, sofa 2, television 3,
/// bookshelf 4, the toilets 5 and 6, shower 7, bed 8, single beds 9 and 10, toy chest 11, single
/// bed 12.
const SOFA: u16 = 2;
const TV: u16 = 3;
const BOOKSHELF: u16 = 4;
const CHEST: u16 = 11;

fn t(secs: i64) -> SimTime {
    SimTime::from_secs(secs)
}

fn sim(id: u64) -> SimId {
    SimId::try_from(id).expect("valid")
}

fn terraced(houses: &HouseData) -> &House {
    houses
        .house(&DataId::new("core:terraced").expect("id"))
        .expect("the terraced house")
}

/// The lineage key of founder `index` of `test:filler`.
fn founder(index: u64) -> LineageKey {
    LineageKey::founder(&DataId::new("test:filler").expect("id"), index)
}

/// What a person does from `origin` on, as (seconds after it, payload): its choices, arrivals,
/// ends and idling.
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
            )
        })
        .map(|entry| (entry.t.as_secs() - origin, entry.payload.clone()))
        .collect()
}

/// A walk of `house` to offer `offer` of `object` on `slot`, from `room`, `walk_secs` long, with
/// the needs of the choice, as its entry writes it.
fn started(
    house: &str,
    (object, offer, slot): (u16, u16, u8),
    (room, walk_secs): (u8, u32),
    needs: [u16; 5],
    filler: bool,
) -> LogPayload {
    LogPayload::ActionStarted {
        house: DataId::new(house).expect("id"),
        object,
        offer,
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

/// Idle in `room` of `house` with `needs`, waiting for nothing, its time-filler's end
/// `too_short` if it took none as that would have come too soon.
fn idle(house: &str, room: u8, needs: [u16; 5], too_short: Option<i64>) -> LogPayload {
    LogPayload::Idle {
        house: DataId::new(house).expect("id"),
        room,
        needs,
        busy: Vec::new(),
        waits: None,
        held: Vec::new(),
        tier: Vec::new(),
        too_short: too_short.map(t),
    }
}

// ---------------------------------------------------------------------------------------
// Which time-filler: the highest mood.

/// The time-fillers free for one idle by day rank by the person's mood for each in the second of
/// the choice (the draw `mood`), then by the walk, then by object and offer; how much fun one
/// brings does not rank it. In the repository's terraced house at 19:01:00 of day 1, from the
/// hall: for the woman of `test:filler` (founder 0, world seed 7, an adult) the moods are the
/// sofa's `sit` −2, the television −2 and the bookshelf +9 (`tools/rng_reference.py`), so the
/// bookshelf, then the sofa and the television, tied at −2 and 3 s, by their objects; with the
/// bookshelf taken, the sofa. For the girl (founder 2, 9) the toy chest +9, 16 s up the stairs,
/// before the bookshelf +8, the television +3 and the sofa +1. At 19:00:00 the woman's moods are
/// +6, −1 and +2: the sofa.
#[test]
fn the_time_filler_has_the_highest_mood() {
    let houses = common::repository_houses();
    let house = terraced(&houses);
    let at = D + 19 * H + 60;
    let woman = |second: i64| {
        move |object: u16, offer: u16| mood(7, founder(0), t(second), (object, offer), MOOD)
    };
    let girl = |object, offer| mood(7, founder(2), t(at), (object, offer), MOOD);
    let ranked = |list: Vec<Filler>| -> Vec<(u16, u16, i8, u32)> {
        (list.iter())
            .map(|each| (each.object, each.offer, each.mood, each.walk_secs))
            .collect()
    };
    let adult = fillers((house, &houses), (35, 0), &[], woman(at));
    assert_eq!(
        ranked(adult),
        [(BOOKSHELF, 0, 9, 3), (SOFA, 1, -2, 3), (TV, 0, -2, 3)]
    );
    assert_eq!(
        filler((house, &houses), (35, 0), &[], woman(at)),
        Some((BOOKSHELF, 0))
    );
    assert_eq!(
        filler((house, &houses), (35, 0), &[BOOKSHELF], woman(at)),
        Some((SOFA, 1))
    );
    let child = fillers((house, &houses), (9, 0), &[], girl);
    assert_eq!(
        ranked(child),
        [
            (CHEST, 0, 9, 16),
            (BOOKSHELF, 0, 8, 3),
            (TV, 0, 3, 3),
            (SOFA, 1, 1, 3)
        ]
    );
    let earlier = fillers((house, &houses), (35, 0), &[], woman(at - 60));
    assert_eq!(
        ranked(earlier),
        [(SOFA, 1, 6, 3), (BOOKSHELF, 0, 2, 3), (TV, 0, -1, 3)]
    );
}

// ---------------------------------------------------------------------------------------
// How long: its length, the departure, the bedtime.

/// A screen for two in the den (fun +100 an hour for 45 min, a time-filler), a bed for two in the
/// den, a meal for two in the kitchen (hunger +1 800 an hour for 20 min, as the repository's
/// table) and a loo for two in the hall (bladder +12 000 an hour until full, at most 5 min).
const SUPPER_KINDS: &str = r#"
  {"id": "test:screen", "slots": 2, "offers": [
    {"id": "watch", "rates": {"fun": 100}, "seconds": 2700, "min_age": 0, "max_age": 130,
     "fallback": true}]},
  {"id": "test:bed", "slots": 2, "offers": [
    {"id": "sleep", "sleep": true, "min_age": 0, "max_age": 130}]},
  {"id": "test:meal", "slots": 2, "offers": [
    {"id": "eat", "rates": {"hunger": 1800}, "seconds": 1200, "min_age": 0, "max_age": 130}]},
  {"id": "test:loo", "slots": 2, "offers": [
    {"id": "use", "rates": {"bladder": 12000}, "until_full": "bladder", "max_seconds": 300,
     "min_age": 0, "max_age": 130}]}"#;

/// Rooms hall 0 (the entrance), den 1, 3 s from it, and a 2, b 3 and the kitchen 4, each 110 s
/// from the one before: the kitchen is 330 s from the hall. Objects: the screen 0 and the bed 1 in
/// the den, the meal 2 in the kitchen, the loo 3 and the five essential ones 4 to 8 in the hall.
const SUPPER: &str = r#"{
  "id": "test:supper", "capacity": 2,
  "rooms": [{"id": "hall", "entrance": true}, {"id": "den"}, {"id": "a"}, {"id": "b"},
    {"id": "kitchen"}],
  "child_day_room": "den", "child_night_room": "den", "dwell_room": "den",
  "doors": [{"a": "hall", "b": "den", "seconds": 3}, {"a": "hall", "b": "a", "seconds": 110},
    {"a": "a", "b": "b", "seconds": 110}, {"a": "b", "b": "kitchen", "seconds": 110}],
  "objects": [{"kind": "test:screen", "room": "den"}, {"kind": "test:bed", "room": "den"},
    {"kind": "test:meal", "room": "kitchen"}, {"kind": "test:loo", "room": "hall"}, ESSENTIALS]
}"#;

const SCREEN: u16 = 0;
const BED: u16 = 1;
const MEAL: u16 = 2;
const LOO: u16 = 3;

/// Awake hunger falls 1 an hour and nothing else moves; asleep hunger falls 40 and energy rises
/// 100. Hunger weighs from its comfort 700, the bladder from 500 and hygiene from 600, energy and
/// fun from full: at the morning values only energy (900) and fun (800) weigh, and nothing by day
/// raises energy.
fn supper_rates() -> String {
    let bands = |rate: i32| [rate; 4];
    json!({
        "awake": {"hunger": bands(-1), "energy": bands(0), "bladder": bands(0),
                  "hygiene": bands(0), "fun": bands(0)},
        "asleep": {"hunger": bands(-40), "energy": bands(100), "bladder": bands(0),
                   "hygiene": bands(0), "fun": bands(0)},
        "comfort": [700, 1000, 500, 600, 1000]
    })
    .to_string()
}

/// The supper house on the repository's day (`fallback_min_seconds` 600, no jump of hunger).
fn supper_data() -> Data {
    day::house_data(&supper_rates(), SUPPER_KINDS, SUPPER)
}

/// The couple of `common::day` (30 and 32, both full time: 8:40 to 17:20, the draws 30 and 3) in
/// the supper house, moved in at 23:59:59 of day 0; day 1 is a Tuesday.
fn working(data: &Data) -> Run {
    Run::moved_in(data.clone()).0
}

/// A retired couple of 70 and 72 (no duty, their night from 21:30) in the house of `data`.
fn retired(data: &Data) -> Run {
    Run::family(data.clone(), 7, "test:house", &[-70 * Y, -72 * Y], 0).0
}

/// The working couple's morning in the supper house. At 6:00 in the hall nothing brings 40 000:
/// the screen 200 · 75 less its 3 s, the meal 100 · 400 · 110 / 100 at the most less its 330 s
/// (16 500), the loo nothing (the bladder at its comfort), the essential kinds nothing. Each takes
/// the screen, its only time-filler, she its first place, he its second: there at 6:00:03, it
/// would end after its 45 minutes, 6:45:03, far beyond the 10 minutes it needs. Nothing comes
/// before: the meal from the den, 333 s away, needs hunger 577 at the most mood (123 · 423 · 110 /
/// 100 − 16 650 = 40 581; at 578 122 · 422 · 110 / 100 − 16 650 = 39 982), 23 hours away at −1 an
/// hour. So it lasts its length, and so do the next two, each taken again in the den, 5 s within
/// it: 6:45:08 to 7:30:08 and 7:30:13 to 8:15:13, fun 875, 950 and full (+75 a time), hunger 600,
/// 599 and 598 at their ends (600 − ⌊s / 3 600⌋). The fourth, there at 8:15:18, would last until
/// 9:00:18; its end is their departure at 8:40 (`world.action_until` at 8:30, not 9:00:18 nor
/// the 4 hours a time-filler may last), which ends it with reason `obligation`, and both leave on
/// time. Hunger at 8:40: 600 − ⌊9 600 / 3 600⌋ = 598.
#[test]
fn a_time_filler_lasts_its_length_and_the_departure_ends_it() {
    let data = supper_data();
    let mut run = working(&data);
    let depart = D + 8 * H + 2_400;
    run.advance(D + 8 * H + 1_800);
    for id in [1, 2] {
        assert_eq!(run.world.action_until(sim(id)), Some(t(depart)), "{id}");
    }
    run.advance(depart + 1);
    let house = "test:supper";
    for (id, slot) in [(1, 0), (2, 1)] {
        let again = |at: i64, hunger: u16, fun: u16| {
            let needs = [hunger, 900, 500, 700, fun];
            [
                (at, ended(SCREEN, EndReason::Done, needs)),
                (at, started(house, (SCREEN, 0, slot), (1, 5), needs, true)),
                (at + 5, LogPayload::Reached { object: SCREEN }),
            ]
        };
        let mut expected = vec![
            (
                0,
                started(house, (SCREEN, 0, slot), (0, 3), MORNING_VALUES, true),
            ),
            (3, LogPayload::Reached { object: SCREEN }),
        ];
        expected.extend(again(2_703, 600, 875));
        expected.extend(again(5_408, 599, 950));
        expected.extend(again(8_113, 598, 1000));
        expected.push((
            depart - MORNING,
            ended(SCREEN, EndReason::Obligation, [598, 900, 500, 700, 1000]),
        ));
        assert_eq!(doings(&run, id, MORNING, depart + 1), expected, "{id}");
        let left = (run.log.iter()).find(|entry| {
            entry.subjects == [Subject::Sim(sim(id))]
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
            )),
            "{id}"
        );
    }
}

/// The retired couple in the supper house, no duty: from 6:00 one screen after the other, each
/// its 45 minutes, the first there at 6:00:03 and over at 6:45:03, each later one 5 s within the
/// den and over 2 705 s after the one before. The 21st begins at 21:01:38 (6:45:03 + 19 · 2 705
/// s) and would end at 21:46:43; their bedtime at 21:30:00 ends it (reason `done`), still 1 702 s
/// after its choice, more than the 10 minutes. Then in the night first what fills
/// hunger under its comfort: hunger 585 (600 − ⌊55 800 / 3 600⌋) brings each to the meal, 333 s
/// from the den (ADR-0026, addendum of 2026-11-05 on the time-filler's length).
#[test]
fn a_time_filler_ends_at_bedtime() {
    let data = supper_data();
    let mut run = retired(&data);
    let bedtime = D + 21 * H + 1_800;
    run.advance(bedtime + 1);
    let house = "test:supper";
    for (id, slot) in [(1, 0), (2, 1)] {
        let lived = doings(&run, id, MORNING, bedtime + 1);
        let fillers: Vec<i64> = (lived.iter())
            .filter(|(_, payload)| {
                matches!(payload, LogPayload::ActionStarted { filler: true, .. })
            })
            .map(|(at, _)| *at)
            .collect();
        let mut expected = vec![0];
        expected.extend((0..20).map(|k| 2_703 + k * 2_705));
        assert_eq!(fillers, expected, "{id}");
        let last = 2_703 + 19 * 2_705;
        assert_eq!(MORNING + last, D + 21 * H + 98);
        let hungry = [585, 900, 500, 700, 1000];
        assert_eq!(
            lived[lived.len() - 3..],
            [
                (last + 5, LogPayload::Reached { object: SCREEN }),
                (bedtime - MORNING, ended(SCREEN, EndReason::Done, hungry)),
                (
                    bedtime - MORNING,
                    started(house, (MEAL, 0, slot), (1, 333), hungry, false)
                ),
            ][..],
            "{id}"
        );
    }
}

// ---------------------------------------------------------------------------------------
// Not under 10 minutes.

/// A stew for two in the hall (hunger +160 an hour for `seconds`), the screen and the bed of the
/// supper house in the den.
fn stew_kinds(seconds: i64) -> String {
    format!(
        r#"
  {{"id": "test:stew", "slots": 2, "offers": [
    {{"id": "simmer", "rates": {{"hunger": 160}}, "seconds": {seconds}, "min_age": 0,
      "max_age": 130}}]}},
  {{"id": "test:screen", "slots": 2, "offers": [
    {{"id": "watch", "rates": {{"fun": 100}}, "seconds": 2700, "min_age": 0, "max_age": 130,
     "fallback": true}}]}},
  {{"id": "test:bed", "slots": 2, "offers": [
    {{"id": "sleep", "sleep": true, "min_age": 0, "max_age": 130}}]}}"#
    )
}

/// Rooms hall 0 (the entrance) and den 1, 3 s apart; the stew 0 in the hall, the screen 1 and
/// the bed 2 in the den, the essential ones 3 to 7 in the hall.
const STEW: &str = r#"{
  "id": "test:stew", "capacity": 2,
  "rooms": [{"id": "hall", "entrance": true}, {"id": "den"}],
  "child_day_room": "den", "child_night_room": "den", "dwell_room": "den",
  "doors": [{"a": "hall", "b": "den", "seconds": 3}],
  "objects": [{"kind": "test:stew", "room": "hall"}, {"kind": "test:screen", "room": "den"},
    {"kind": "test:bed", "room": "den"}, ESSENTIALS]
}"#;

/// Nothing moves awake, energy rises 100 asleep; every need weighs from full but the bladder,
/// from its 500.
fn stew_data(seconds: i64) -> Data {
    let bands = |rate: i32| [rate; 4];
    let rates = json!({
        "awake": {"hunger": bands(0), "energy": bands(0), "bladder": bands(0),
                  "hygiene": bands(0), "fun": bands(0)},
        "asleep": {"hunger": bands(0), "energy": bands(100), "bladder": bands(0),
                   "hygiene": bands(0), "fun": bands(0)},
        "comfort": [1000, 1000, 500, 1000, 1000]
    });
    day::house_data(&rates.to_string(), &stew_kinds(seconds), STEW)
}

/// The girl of 9 of `test:filler` (founder 2) at 7:38:00 of day 1, a Tuesday, in the children's
/// room of the repository's terraced house, idle with nothing worth it, her school at 7:40:
/// her time-fillers there, by her moods then, are the television +5 (19 s down), the toy chest +3
/// (5 s within the room), the bookshelf 0 and the sofa −10. Each would end at the departure,
/// 7:40:00, 120 s after the choice, under its 10 minutes: she takes none
/// (`filler_end_if`). At 7:00:00 the toy chest would last its 30 minutes, to 7:30:05, and at
/// 7:20:00 end at the departure 20 minutes on: each long enough.
#[test]
fn no_time_filler_two_minutes_before_school() {
    let houses = common::repository_houses();
    let house = terraced(&houses);
    let born = t(-9 * Y);
    let seven = D + 7 * H;
    let at = seven + 38 * 60;
    let key = founder(2);
    let girl = |object, offer| mood(7, key, t(at), (object, offer), MOOD);
    let free = fillers((house, &houses), (9, 7), &[], girl);
    let ranked: Vec<(u16, i8, u32)> = (free.iter())
        .map(|each| (each.object, each.mood, each.walk_secs))
        .collect();
    assert_eq!(
        ranked,
        [
            (TV, 5, 19),
            (CHEST, 3, 5),
            (BOOKSHELF, 0, 19),
            (SOFA, -10, 19)
        ]
    );
    // Her needs at the awake rates of 6 to 12, all at their comfort or full but for energy, which
    // nothing raises by day.
    let awake = [-100, -50, -120, -30, -60];
    let values = [867, 900, 900, 900, 1000];
    let girl_at = |second: i64| InHouse {
        needs: core::array::from_fn(|n| NeedSample::new(values[n], t(second), awake[n])),
        place: Place::Room(7),
        action: None,
        due: None,
        decisions: Decisions { day: 1, count: 1 },
        collapsed: false,
        reserve: Reserve::full(1000, t(second)),

        talk_day: 0,
        talks: 0,
        entries: 0,
    };
    let end = |second: i64, (object, offer, walk): (u16, u16, u32)| {
        let person = girl_at(second);
        let taken = BTreeSet::new();
        let who = (born, key, 7);
        filler_end_if(
            (house, &houses),
            who,
            (&person, &taken),
            (object, offer, walk),
            t(second),
        )
    };
    let departure = seven + 40 * 60;
    for each in &free {
        let ends = end(at, (each.object, each.offer, each.walk_secs));
        assert_eq!(ends, Some(t(departure)), "{each:?}");
        assert!(departure - at < 600);
    }
    assert_eq!(end(seven, (CHEST, 0, 5)), Some(t(seven + 5 + 1_800)));
    assert_eq!(end(seven + 1_200, (CHEST, 0, 5)), Some(t(departure)));
}

/// The working couple in the stew house, the stew of 9 500 s: at 6:00 it brings 400 · 400 = 160
/// 000 less its 5 s, and they take it, there at 6:00:05, done at 8:38:25 with hunger full (600 +
/// 422). Nothing then brings 40 000, and the screen, their only time-filler, 3 s away, would end
/// at their departure at 8:40:00, 95 s after the choice: under 10 minutes, so each stays idle in
/// the hall, its entry naming that end (`too_short`), and leaves on time at 8:40 (ADR-0026,
/// addendum of 2026-11-05 on the time-filler's length; the 2-minute toy chest before school).
#[test]
fn no_time_filler_under_ten_minutes_before_the_departure() {
    let data = stew_data(9_500);
    let mut run = working(&data);
    let depart = D + 8 * H + 2_400;
    run.advance(depart + 1);
    let house = "test:stew";
    let full = [1000, 900, 500, 700, 800];
    for (id, slot) in [(1, 0), (2, 1)] {
        assert_eq!(
            doings(&run, id, MORNING, depart + 1),
            [
                (
                    0,
                    started(house, (0, 0, slot), (0, 5), MORNING_VALUES, false)
                ),
                (5, LogPayload::Reached { object: 0 }),
                (9_505, ended(0, EndReason::Done, full)),
                (9_505, idle(house, 0, full, Some(depart))),
            ],
            "{id}"
        );
        let left = (run.log.iter()).find(|entry| {
            entry.subjects == [Subject::Sim(sim(id))]
                && matches!(entry.payload, LogPayload::LeftHome { late_secs: 0, .. })
        });
        assert_eq!(left.map(|entry| entry.t), Some(t(depart)), "{id}");
    }
}

/// The retired couple in the stew house, the stew of 41 972 s, done at 17:39:37 (6:00:05 + 41
/// 972): then one screen after the other, the first there at 17:39:40, each its 45 minutes, ending
/// at 18:24:40, 19:09:45, 19:54:50, 20:39:55 and 21:25:00. At 21:25:00 the next would end at their
/// bedtime 21:30:00, 300 s after the choice: each stays idle in the den, its entry naming 21:30,
/// and at the bedtime takes the bed, 5 s within the den.
#[test]
fn no_time_filler_under_ten_minutes_before_bedtime() {
    let data = stew_data(41_972);
    let mut run = retired(&data);
    let bedtime = D + 21 * H + 1_800;
    run.advance(bedtime + 1);
    let house = "test:stew";
    let full = [1000, 900, 500, 700, 1000];
    for (id, slot) in [(1, 0), (2, 1)] {
        let lived = doings(&run, id, MORNING, bedtime + 1);
        let ends: Vec<i64> = (lived.iter())
            .filter(|(_, payload)| matches!(payload, LogPayload::ActionEnded { object: 1, .. }))
            .map(|(at, _)| *at)
            .collect();
        assert_eq!(ends, [44_680, 47_385, 50_090, 52_795, 55_500], "{id}");
        assert_eq!(MORNING + 55_500, D + 21 * H + 1_500);
        assert_eq!(
            lived[lived.len() - 2..],
            [
                (55_500, idle(house, 1, full, Some(bedtime))),
                (55_800, started(house, (2, 0, slot), (1, 5), full, false)),
            ][..],
            "{id}"
        );
    }
}

// ---------------------------------------------------------------------------------------
// Before the bed.

/// The design's girl at her bedtime, by hand in the repository's terraced house: 9 years old in
/// the children's room at 20:00 with hunger 599, every other need at or above its comfort and fun
/// full. The table, 20 s away (landing 4, hall 12, kitchen 4), brings 101 · 401 − 1 000 = 39 501
/// without a mood: by day she would not go; in her night, after what is critical (nothing), what
/// fills hunger under its comfort comes before a bed: the table, ranked by its points, before the
/// fridge (101 · 200 − 1 000); a bed only with both taken. With hunger at 700 the bed comes first.
#[test]
fn hunger_under_comfort_eats_before_the_bed() {
    let houses = common::repository_houses();
    let house = terraced(&houses);
    let needs = [599, 1000, 700, 900, 1000];
    let no_mood = |_: u16, _: u16| 0;
    let night = 10 * H;
    let scored = score(
        house,
        &houses,
        (9, needs, 7),
        (Some(night), None),
        no_mood,
        0,
        |_| Bonus::NONE,
    );
    let points = |object: u16| {
        (scored.iter())
            .find(|each| (each.object, each.offer) == (object, 0))
            .map(|each| each.points)
    };
    assert_eq!(points(0), Some(39_501));
    assert_eq!(points(1), Some(101 * 200 - 1_000));
    let by_day = score(
        house,
        &houses,
        (9, needs, 7),
        (None, None),
        no_mood,
        0,
        |_| Bonus::NONE,
    );
    let day_choice = choose((house, &houses), (9, needs), &by_day, &[], None);
    assert_eq!(day_choice, Choice::Idle);
    let beds = [9, 10, 12];
    let chose = |values: [u16; 5], busy: &[u16]| {
        let scored = score(
            house,
            &houses,
            (9, values, 7),
            (Some(night), None),
            no_mood,
            0,
            |_| Bonus::NONE,
        );
        match choose((house, &houses), (9, values), &scored, busy, Some(&beds)) {
            Choice::Take(chosen) => Some(chosen.object),
            Choice::Wait(_) | Choice::Idle => None,
        }
    };
    assert_eq!(chose(needs, &[]), Some(0));
    assert_eq!(chose(needs, &[0]), Some(1));
    assert_eq!(chose(needs, &[0, 1]), Some(9));
    assert_eq!(chose([700, 1000, 700, 900, 1000], &[]), Some(9));
}

/// The working couple's evening in the supper house. Home at 17:20: hunger stays at 598 (no jump
/// any more), the bladder jumps to 100, critical, so each takes the loo, done at 17:24:35 (900 at
/// +12 000 an hour from 17:20:05). Then screen after screen, there at 17:24:38 and each its 45
/// minutes: they end at 18:09:38, 18:54:43, 19:39:48, 20:24:53, 21:09:58 and 21:55:03. At 21:55:03
/// the next would end at their bedtime 22:00, 297 s on: each stays idle in the den, its entry
/// naming 22:00. At 22:00 hunger is 594 (598 − ⌊16 800 / 3 600⌋), under its comfort 700, and the
/// meal fills it (406 of 600): before the bed each walks the 333 s to the kitchen and eats until
/// 22:25:33, hunger full (594 + 600, at most 1 000); the choice after it is in the night, so the
/// bed, 333 s back, there at 22:31:06. Asleep hunger falls 40 an hour: at the end of the night,
/// 6:00, it is 1 000 − ⌊40 · 26 934 / 3 600⌋ = 701, and the day begins with a time-filler.
#[test]
fn a_meal_under_comfort_comes_before_the_bed() {
    let data = supper_data();
    let mut run = working(&data);
    let back = D + 17 * H + 1_200;
    let bedtime = D + 22 * H;
    let morning = 2 * D + 6 * H;
    run.advance(morning + 1);
    let house = "test:supper";
    let home = [598, 700, 100, 600, 850];
    let washed = [598, 700, 1000, 600, 850];
    let hungry = [594, 700, 1000, 600, 1000];
    let fed = [1000, 700, 1000, 600, 1000];
    for (id, slot) in [(1, 0), (2, 1)] {
        let lived = doings(&run, id, back, morning + 1);
        assert_eq!(
            lived[..5],
            [
                (0, started(house, (LOO, 0, slot), (0, 5), home, false)),
                (5, LogPayload::Reached { object: LOO }),
                (275, ended(LOO, EndReason::Full, washed)),
                (275, started(house, (SCREEN, 0, slot), (0, 3), washed, true)),
                (278, LogPayload::Reached { object: SCREEN }),
            ][..],
            "{id}"
        );
        let ends: Vec<i64> = (lived.iter())
            .filter(|(_, payload)| {
                matches!(payload, LogPayload::ActionEnded { object: SCREEN, .. })
            })
            .map(|(at, _)| at + back)
            .collect();
        let expected: Vec<i64> = (0..6).map(|k| D + 65_378 + k * 2_705).collect();
        assert_eq!(ends, expected, "{id}");
        assert_eq!(expected[5], D + 21 * H + 55 * 60 + 3);
        let after = |at: i64| at - back;
        let woke = [701, 1000, 1000, 600, 1000];
        assert_eq!(
            lived[lived.len() - 8..],
            [
                (after(expected[5]), idle(house, 1, hungry, Some(bedtime))),
                (
                    after(bedtime),
                    started(house, (MEAL, 0, slot), (1, 333), hungry, false)
                ),
                (after(bedtime + 333), LogPayload::Reached { object: MEAL }),
                (after(bedtime + 1_533), ended(MEAL, EndReason::Done, fed)),
                (
                    after(bedtime + 1_533),
                    started(house, (BED, 0, slot), (4, 333), fed, false)
                ),
                (after(bedtime + 1_866), LogPayload::Reached { object: BED }),
                (after(morning), ended(BED, EndReason::Done, woke)),
                (
                    after(morning),
                    started(house, (SCREEN, 0, slot), (1, 5), woke, true)
                ),
            ][..],
            "{id}"
        );
    }
}

// ---------------------------------------------------------------------------------------
// One supper, on the repository's data.

/// Founders of `test:filler`: a woman of 35 and a man of 37, both full time (their draws 51 and
/// 52, `tools/rng_reference.py`: 8:40 to 17:20), and girls of 9 and 6 at school (7:40 to 15:20),
/// whom the couple takes in; in the repository's terraced house, their first morning 6:00 of day
/// 1, a Tuesday. On the weekdays after it (days 2, 3, 4 and 7) each adult eats one supper and a
/// girl one too. By hand (the design's table): hunger falls 100 an hour awake; a meal at the table
/// (+600 in 20 min) brings w · (w + 300) at the deficit w below the comfort 700, scaled by the
/// mood, less its walk (3 s from the living room, 150): at the most mood it is worth it from w =
/// 93 (93 · 393 · 110 / 100 − 150 = 40 054), surely at the least from 110 (110 · 410 · 90 / 100 −
/// 150 = 40 440), hunger 607 to 590. Breakfast near 6:00 ends full; at 8:40, 2 h 20 later, hunger
/// is about 767 (1 000 − 233), and it stands while away and on coming home. From 767 hunger reaches
/// 607 after 1 h 36 min and 590 after 1 h 46 min: a supper chosen from 18:56 to 19:07, a few
/// minutes later if the bladder or a shower comes first (at most a toilet of 5 min and a shower of
/// 15, each with its walk): the window from 18:50 to 19:30. It ends 20 min after its walk, full,
/// so at 22:00 hunger is 1 000 less 100 an hour since then: from 727 to 773 for a supper begun
/// within the window; over 700, so no meal before the bed. A girl leaves at 7:40, 1 h 20 after a
/// breakfast near 6:00, at about 867: her supper from 17:57 to 18:08 at the most and least mood,
/// the window 17:50 to 18:30; at 20:00 her hunger is well above 700.
#[test]
fn a_weekday_has_one_supper() {
    let data = common::repository_data();
    let born = [-35 * Y, -37 * Y, -9 * Y, -6 * Y];
    let (mut run, _) = Run::family(data.clone(), 7, "test:filler", &born, 0);
    let days = [2, 3, 4, 7];
    // The adults' hunger as it stands at 22:00 of those days: the sample after the last second
    // with events at or before it.
    let mut at_ten: std::collections::BTreeMap<(i64, u64), NeedSample> = Default::default();
    run.step(8 * D, |world, second| {
        for day in days {
            if (day * D..=day * D + 22 * H).contains(&second) {
                for id in [1, 2] {
                    let hunger = world.state().house().sims[&sim(id)].needs[0];
                    at_ten.insert((day, id), hunger);
                }
            }
        }
    });
    let houses = data.houses().expect("houses");
    let house = terraced(houses);
    let feeds = |object: u16, offer: u16| {
        let kind = houses
            .kind(&house.def().objects[usize::from(object)].kind)
            .expect("a kind");
        let rates = &kind.offers[usize::from(offer)].rates;
        rates.contains_key(&sim_core::house::data::Need::Hunger)
    };
    for day in days {
        assert!(calendar(t(day * D)).weekday < 5, "{day}");
        for (id, from, (low, high)) in [
            (1, 17 * H + 1_200, (18 * H + 50 * 60, 19 * H + 30 * 60)),
            (2, 17 * H + 1_200, (18 * H + 50 * 60, 19 * H + 30 * 60)),
            (3, 15 * H + 1_200, (17 * H + 50 * 60, 18 * H + 30 * 60)),
            (4, 15 * H + 1_200, (17 * H + 50 * 60, 18 * H + 30 * 60)),
        ] {
            let suppers: Vec<i64> = (run.log.iter())
                .filter(|entry| entry.subjects == [Subject::Sim(sim(id))])
                .filter(|entry| (day * D + from..(day + 1) * D).contains(&entry.t.as_secs()))
                .filter_map(|entry| match entry.payload {
                    LogPayload::ActionStarted { object, offer, .. } if feeds(object, offer) => {
                        Some(entry.t.as_secs() - day * D)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(suppers.len(), 1, "day {day}, sim {id}: {suppers:?}");
            assert!(
                (low..=high).contains(&suppers[0]),
                "day {day}, sim {id}: supper at {}",
                suppers[0]
            );
        }
        for id in [1, 2] {
            let value = at_ten[&(day, id)].value_at(t(day * D + 22 * H));
            assert!(
                (727..=773).contains(&value),
                "day {day}, sim {id}: hunger {value} at 22:00"
            );
        }
    }
}

// ---------------------------------------------------------------------------------------
// Saves (the loader, ADR-0026, addendum of 2026-11-05 on the time-filler's length).

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

/// Saved in the last second before a time-filler's length ends (6:45:02), before the departure
/// that ends one (8:39:59), before the bedtime with hunger under its comfort and a time-filler not
/// taken (21:59:59), and during the meal before the bed (22:10:00), the working couple's day in
/// the supper house loads, saves to the same bytes and runs on to 7:00 of the next day as the
/// whole; so does the stew house's idle couple, its time-filler too short, saved at 8:39:00 and
/// at 8:39:59; the places hold after every second with events.
#[test]
fn saves_of_a_time_filler_and_a_meal_before_the_bed_run_on_the_same() {
    let data = supper_data();
    let mut whole = working(&data);
    whole.step(2 * D + 7 * H, places_hold);
    for saved_at in [
        MORNING + 2_702,
        D + 8 * H + 2_399,
        D + 22 * H - 1,
        D + 22 * H + 600,
    ] {
        runs_on_the_same(&data, saved_at, 2 * D + 7 * H, || {
            let mut run = working(&data);
            run.advance(saved_at);
            run
        });
    }
    let stew = stew_data(9_500);
    for saved_at in [D + 8 * H + 2_340, D + 8 * H + 2_399] {
        runs_on_the_same(&stew, saved_at, D + 10 * H, || {
            let mut run = working(&stew);
            run.advance(saved_at);
            run
        });
    }
}

/// A time-filler chosen by day whose walk ends in the night is one a choice can take when the
/// least time left for one is 0 (`common::repository_day_any_filler`), and the loader keeps it
/// (ADR-0026, addendum of 2026-11-05 on the time-filler's length: its choice is the walk's
/// start, by day, and its end by the bounds the save shows, its arrival in the night, comes 3 s
/// after it). The working couple in the supper house: at the clock 21:58:54 she is told to use the
/// loo in the hall, 3 s from the den; full already, she is done after its 60 s at 21:59:58, and
/// by day nothing brings 40 000: she takes the screen, 3 s back, there at 22:00:01, in her night.
/// Saved on that walk at 21:59:59 and at 22:00:00, the day loads, saves to the same bytes and runs
/// on to 7:00 as the whole. With the repository's 10 minutes she takes no time-filler there: she
/// stays idle in the hall, its end 22:00:01 named.
#[test]
fn a_time_filler_walking_into_the_night_is_kept() {
    let told = |data: &Data| {
        let mut run = working(data);
        run.advance(D + 22 * H - 66);
        run.submit(sim_core::input::Input::Command(sim_core::input::Command {
            sim: sim(1),
            object: LOO,
            offer: 0,
        }));
        run
    };
    let any = common::repository_day_any_filler();
    let data = day::house_data_on(&any, &supper_rates(), SUPPER_KINDS, SUPPER);
    let chosen = D + 22 * H - 2;
    let mut run = told(&data);
    run.advance(chosen + 1);
    let her = &run.world.state().house().sims[&sim(1)];
    let walk = Place::Walking {
        from: 0,
        to: 1,
        depart: t(chosen),
        arrive: t(chosen + 3),
    };
    assert_eq!(her.place, walk);
    assert_eq!(
        her.action.map(|action| (action.object, action.filler)),
        Some((SCREEN, true))
    );
    for saved_at in [chosen + 1, chosen + 2] {
        runs_on_the_same(&data, saved_at, 2 * D + 7 * H, || {
            let mut run = told(&data);
            run.advance(saved_at);
            run
        });
    }
    let data = supper_data();
    let mut run = told(&data);
    run.advance(chosen + 1);
    let idle = (run.log.iter()).find(|entry| {
        entry.subjects == [Subject::Sim(sim(1))]
            && entry.t == t(chosen)
            && matches!(entry.payload, LogPayload::Idle { .. })
    });
    let short = idle.map(|entry| match &entry.payload {
        LogPayload::Idle {
            too_short, room, ..
        } => (*room, *too_short),
        _ => (0, None),
    });
    assert_eq!(short, Some((0, Some(t(chosen + 3)))));
}

/// A time-filler that arrives in a night ends at once at its arrival, whatever else would end it
/// (ADR-0026, addenda of 2026-11-04 and 2026-11-05 on the time-filler), and the loader judges it
/// so too (`filler_possible`): on the repository's day changed so that all work leaves at 21:58
/// and comes back at 21:59, every one at work (`part_time_below` 100), and every night begins at
/// 22:00. An adult of 30 (founder 1 of `test:filler`, world seed 7) chooses on day 1, a Tuesday,
/// with the bookshelf (60 minutes):
/// - at 21:50:00, there at 22:00:05 in her night: it ends at the arrival, 605 s after the
///   choice, at least 600: possible, though the departure at 21:58 lies between;
/// - at 21:50:10, there at 22:00:05: 595 s, under 600: not possible;
/// - at 21:50:00, there at 21:59:59, by day: the departure at 21:58 ends it 480 s after the
///   choice: not possible;
/// - at 21:40:00, there at 21:41:00: the departure, 1 080 s after the choice: possible.
#[test]
fn a_time_filler_arriving_in_the_night_ends_at_its_arrival() {
    let houses = common::repository_houses_with_day(|day| {
        for hours in ["full_time", "part_time"] {
            day["work"][hours] = json!({"depart": 79_080, "back": 79_140});
        }
        day["work"]["part_time_below"] = json!(100);
        day["night"]["from"] = json!([79_200, 79_200, 79_200, 79_200]);
    });
    let shelf = houses
        .kind(&DataId::new("core:bookshelf").expect("id"))
        .expect("the bookshelf");
    let read = &shelf.offers[0];
    let who = (t(-30 * Y), founder(1), 7);
    let at = |h: i64, m: i64, s: i64| t(D + h * H + m * 60 + s);
    let possible = |chosen, arrive| filler_possible(&houses, who, read, (chosen, arrive));
    assert!(possible(at(21, 50, 0), at(22, 0, 5)));
    assert!(!possible(at(21, 50, 10), at(22, 0, 5)));
    assert!(!possible(at(21, 50, 0), at(21, 59, 59)));
    assert!(possible(at(21, 40, 0), at(21, 41, 0)));
}

//! The rules of the house after the playtest of 2026-11-04 (ADR-0026, addendum of 2026-11-04
//! after the playtest): one idle chooses again at the first whole minute at which an offer with
//! a free place brings 40 000 with its mood of that second, looked for between the first second
//! it could at the most mood and the first it surely does at the least, and no later than its
//! bedtime; one idle in a room it does not stay in walks to the house's `dwell_room`; in the
//! night a couple sleeps in a bed for couples, together, a child in a bed for children, anyone
//! else in any bed, then the other beds and the sofa; a child under 6 holds a place in its bed
//! from 19:00 to 7:00; the night before a Saturday or a Sunday ends at 7:00 (at 6:30 from 67).
//! The values by hand from the rates, the offers and the walks, the moods from `mood`, the draw
//! the rules name.

mod common;

use std::collections::BTreeSet;

use common::day::{self, MORNING, Run, number, places_hold};
use serde_json::json;
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::house::choice::{ENOUGH, mood, next_choice};
use sim_core::house::data::{House, HouseData};
use sim_core::house::day::night_of;
use sim_core::house::need::NeedSample;
use sim_core::house::state::{Action, Place, child_held};
use sim_core::ids::SimId;
use sim_core::lineage::LineageKey;
use sim_core::log::{Cause, LogEntry, LogPayload, Subject};
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

/// The mood of founder `index` of `test:house` (the couple of `common::day`, world seed 7) for
/// offer 0 of `object` at `second`, only read.
fn couple_mood(index: u64, second: i64, object: u16) -> i64 {
    let key = LineageKey::founder(&DataId::new("test:house").expect("id"), index);
    i64::from(mood(7, key, t(second), (object, 0), MOOD))
}

// ---------------------------------------------------------------------------------------
// The next choice of one idle (point 1).

/// A nibble for two (hunger +720 an hour for 5 min: it fills 60) in the hall and a bed for two in
/// the den; the five essential kinds of `common::day` in the hall, each filling 0.
const NIBBLE_KINDS: &str = r#"
  {"id": "test:nibble", "slots": 2, "offers": [
    {"id": "eat", "rates": {"hunger": 720}, "seconds": 300, "min_age": 0, "max_age": 130}]},
  {"id": "test:bed", "slots": 2, "offers": [
    {"id": "sleep", "sleep": true, "min_age": 0, "max_age": 130}]}"#;

/// Rooms hall 0 (the entrance) and den 1, 3 s apart. Objects: the nibble 0 in the hall, the bed
/// 1 in the den, the essential ones 2 to 6 in the hall.
const NIBBLE: &str = r#"{
  "id": "test:nibble", "capacity": 2,
  "rooms": [{"id": "hall", "entrance": true}, {"id": "den"}],
  "child_day_room": "den", "child_night_room": "den", "dwell_room": "den",
  "doors": [{"a": "hall", "b": "den", "seconds": 3}],
  "objects": [{"kind": "test:nibble", "room": "hall"}, {"kind": "test:bed", "room": "den"},
    ESSENTIALS]
}"#;

const NIBBLE_OBJECT: u16 = 0;

/// The nibble house with awake hunger falling `hunger` an hour and every other need still.
fn nibble_data(hunger: i32) -> Data {
    day::house_data(&day::rates(hunger, 0, 0), NIBBLE_KINDS, NIBBLE)
}

fn nibble_house(data: &Data) -> (&HouseData, &House) {
    let houses = data.houses().expect("houses");
    let house = houses
        .house(&DataId::new("test:nibble").expect("id"))
        .expect("the house");
    (houses, house)
}

/// Hunger 600 at 6:00 falling 1 000 an hour: its deficit `s` seconds later is 400 + ⌊1 000 · s /
/// 3 600⌋.
fn deficit(s: i64) -> i64 {
    400 + 1_000 * s / 3_600
}

/// The nibble's points from the hall at the deficit `d` and the mood `mood`: the deficit times its
/// fill of 60 (it is above 60), scaled, less its walk of 5 s in the same room.
fn nibble(d: i64, mood: i64) -> i64 {
    d * 60 * (100 + mood) / 100 - 250
}

fn idle_in(house: &str, room: u8, needs: [u16; 5], busy: &[u16]) -> LogPayload {
    LogPayload::Idle {
        house: DataId::new(house).expect("id"),
        room,
        needs,
        busy: busy.to_vec(),
        waits: None,
        held: Vec::new(),
        tier: Vec::new(),
    }
}

fn started_in(
    house: &str,
    (object, slot): (u16, u8),
    (room, walk_secs): (u8, u32),
    needs: [u16; 5],
    busy: &[u16],
) -> LogPayload {
    LogPayload::ActionStarted {
        house: DataId::new(house).expect("id"),
        object,
        offer: 0,
        slot,
        room,
        walk_secs,
        needs,
        busy: busy.to_vec(),
        commanded: false,
        held: Vec::new(),
        tier: Vec::new(),
    }
}

/// The couple of `common::day` in the nibble house, both at work from 8:40 (their wake at 7:55).
/// At 6:00 the nibble brings at most 400 · 60 · 110 / 100 − 250 = 26 150: both idle. At the most
/// mood, +10, it brings 40 000 from the deficit 610, 756 s on (6:12:36: 609 · 66 − 250 = 39 944,
/// 610 · 66 − 250 = 40 010); at the least, −10, surely from 746, 1 246 s on (6:20:46: 745 · 54 −
/// 250 = 39 980, 746 · 54 − 250 = 40 034). Hunger 400 at 6:12:00 makes both choose again (36 000
/// · 110 / 100 − 250 = 39 350 at most: idle), from the same bounds. The whole minutes between are
/// 6:13 to 6:20. Her moods for the nibble there are +10, −5, +9, …: at 6:13:00 the deficit 616
/// brings 616 · 60 · 110 / 100 − 250 = 40 406, and she walks to it, its first place, 5 s, with
/// hunger 384. His are 0, −4, −2, 0, −10, −9 from 6:13 to 6:18 (36 710, 36 210, 37 970, 39 710,
/// 36 632, 37 970) and +5 at 6:19:00: 716 · 60 · 105 / 100 − 250 = 44 858, hunger 284, the first
/// place again, hers free since her 5 minutes ended at 6:18:05. Each choice is the one planned by
/// the person's idle choice at 6:12:00; nothing else made either choose.
#[test]
fn the_next_choice_comes_at_the_first_minute_that_brings_enough() {
    let (from, until) = (756, 1_246);
    assert!(nibble(deficit(from - 1), 10) < ENOUGH && nibble(deficit(from), 10) >= ENOUGH);
    assert!(nibble(deficit(until - 1), -10) < ENOUGH && nibble(deficit(until), -10) >= ENOUGH);
    assert!(nibble(deficit(0), 10) < ENOUGH && nibble(deficit(720), 10) < ENOUGH);
    let minutes: Vec<i64> = (780..until).step_by(60).collect();
    assert_eq!(
        minutes,
        (13..=20).map(|minute| minute * 60).collect::<Vec<_>>()
    );
    let first = |index: u64| {
        let reaches =
            |s: &&i64| nibble(deficit(**s), couple_mood(index, MORNING + **s, 0)) >= ENOUGH;
        minutes.iter().find(reaches).copied().unwrap_or(until)
    };
    let hers: Vec<i64> = minutes[..3]
        .iter()
        .map(|&s| couple_mood(0, MORNING + s, 0))
        .collect();
    assert_eq!(hers, [10, -5, 9]);
    let his: Vec<i64> = minutes[..7]
        .iter()
        .map(|&s| couple_mood(1, MORNING + s, 0))
        .collect();
    assert_eq!(his, [0, -4, -2, 0, -10, -9, 5]);
    assert_eq!((first(0), first(1)), (780, 1_140));
    assert_eq!(
        (nibble(deficit(780), 10), nibble(deficit(1_140), 5)),
        (40_406, 44_858)
    );
    let (mut run, _) = Run::moved_in(nibble_data(-1_000));
    run.advance(MORNING + 1_200);
    let house = "test:nibble";
    let low = [400, 900, 500, 700, 800];
    for (id, at, hunger) in [(1, 780, 384), (2, 1_140, 284)] {
        let needs = [hunger, 900, 500, 700, 800];
        let expected = [
            (0, idle_in(house, 0, MORNING_VALUES, &[])),
            (720, idle_in(house, 0, low, &[])),
            (
                at,
                started_in(house, (NIBBLE_OBJECT, 0), (0, 5), needs, &[]),
            ),
        ];
        assert_eq!(
            doings(&run, id, MORNING, MORNING + at + 1),
            expected,
            "{id}"
        );
        let chose = entry_of(&run, id, MORNING + at, &expected[2].1);
        let idled = entry_of(&run, id, MORNING + 720, &expected[1].1);
        let [Cause::Plan(plan)] = chose.causes.as_slice() else {
            panic!("{:?}", chose.causes);
        };
        assert_eq!(number(&plan.by), number(&idled.id), "{id}");
    }
}

/// The same couple with hunger still: nothing ever brings 40 000 (the nibble 26 150 at most, the
/// rest fills nothing), so no choice is planned. They idle at 6:00 and again only at their wake
/// at 7:55, the next occasion of ADR-0026 section 3; and the next choice of an idle person with
/// these needs is none.
#[test]
fn nothing_that_can_bring_enough_plans_no_choice() {
    let data = nibble_data(0);
    let (mut run, _) = Run::moved_in(data.clone());
    run.advance(MORNING + 2 * H);
    let house = "test:nibble";
    for id in [1, 2] {
        assert_eq!(
            doings(&run, id, MORNING, MORNING + 2 * H),
            [
                (0, idle_in(house, 0, MORNING_VALUES, &[])),
                (6_900, idle_in(house, 0, MORNING_VALUES, &[])),
            ],
            "{id}"
        );
    }
    let (houses, home) = nibble_house(&data);
    let needs = MORNING_VALUES.map(|value| NeedSample::new(value, t(MORNING), 0));
    let born = t(-30 * Y);
    let next = next_choice(
        (home, houses),
        (born, &needs, 0),
        |_| true,
        t(MORNING),
        |_, _, _| 10,
    );
    assert_eq!(next, None);
}

/// The search stops before the person's bedtime, which makes it choose anyway (an adult's
/// night from 22:00), and stops at its next birthday, where the offers and their rates may
/// change. At 21:50 with hunger 900 falling 1 000 an hour, the nibble brings 40 000 at the most
/// mood only from the deficit 610, 510 · 3.6 = 1 836 s on, at 22:20:36: none. With hunger 400 at
/// 21:50 and every mood +10, from 36 s on, so at 21:51:00. With hunger still, nothing comes; a
/// birthday at 21:00 of that day, before the bedtime, brings the choice there, one at 23:00 does
/// not. In its night it looks from the night's end on: at 23:00 with hunger 400, from 6:00 of
/// the next day, its first whole minute 6:00:00.
#[test]
fn the_search_stops_at_bedtime_and_at_a_birthday() {
    let data = nibble_data(-1_000);
    let (houses, home) = nibble_house(&data);
    let day = 2 * D;
    let born = t(-30 * Y);
    let at = |hours: i64, minutes: i64, seconds: i64| t(day + hours * H + minutes * 60 + seconds);
    let needs = |hunger: u16, since: SimTime, rate: i32| {
        let mut needs = MORNING_VALUES.map(|value| NeedSample::new(value, since, 0));
        needs[0] = NeedSample::new(hunger, since, rate);
        needs
    };
    let next = |born: SimTime, needs: &[NeedSample; 5], start: SimTime, mood: i8| {
        next_choice(
            (home, houses),
            (born, needs, 0),
            |_| true,
            start,
            |_, _, _| mood,
        )
    };
    let ten_to = at(21, 50, 0);
    assert_eq!(next(born, &needs(900, ten_to, -1_000), ten_to, 10), None);
    assert_eq!(
        next(born, &needs(400, ten_to, -1_000), ten_to, 10),
        Some(at(21, 51, 0))
    );
    assert!(nibble(600 + 1_000 * 35 / 3_600, 10) < ENOUGH);
    assert!(nibble(600 + 1_000 * 36 / 3_600, 10) >= ENOUGH);
    let still = needs(400, at(11, 0, 0), 0);
    assert_eq!(next(born, &still, at(11, 0, 0), 10), None);
    let nine_pm = t(at(21, 0, 0).as_secs() - 30 * Y);
    assert_eq!(next(nine_pm, &still, at(11, 0, 0), 10), Some(at(21, 0, 0)));
    let eleven_pm = t(at(23, 0, 0).as_secs() - 30 * Y);
    assert_eq!(next(eleven_pm, &still, at(11, 0, 0), 10), None);
    let late = at(23, 0, 0);
    assert_eq!(
        next(born, &needs(400, late, -1_000), late, 10),
        Some(t(day + D + 6 * H))
    );
}

// ---------------------------------------------------------------------------------------
// A family of five in the repository's terraced house (points 2 and 3).

/// The family of the playtest: a woman of 33 and a man of 31, the couple; girls of 9 and 6 and a
/// boy of 4 (sims 3, 4 and 5), whom they take in; moved in at 23:59:59 of day 0, the first
/// morning 6:00 of day 1, a Tuesday (weekday 1). Five live in the terraced house.
fn family() -> Run {
    let born = [-33 * Y, -31 * Y, -9 * Y, -6 * Y, -4 * Y];
    let (run, home) = Run::family(common::repository_data(), SEED_RULES, TAG, &born, 0);
    let house = &run.world.state().house().households[&home].house;
    assert_eq!(house, &DataId::new("core:terraced").expect("id"));
    run
}

/// The family's seed (never reseeded) and its tag.
const SEED_RULES: u64 = 0x5eed_0002_0012_0001;
const TAG: &str = "test:rules";

/// The terraced house's rooms (`data/house/houses/terraced.json`): hall 0, kitchen 1, living 2,
/// WC 3, landing 4, bath 5, bedroom 6, kids 7, small room 8. Its beds: the double bed 8 in the
/// bedroom, the single beds 9 and 10 in the kids' room, the single bed 12 in the small room.
const LIVING: u8 = 2;
const WC: u8 = 3;
const BATH: u8 = 5;
const DOUBLE: u16 = 8;
const KIDS_BEDS: [u16; 2] = [9, 10];
const SMALL_ROOM_BED: u16 = 12;

/// One idle and waiting for nothing in the WC or the bath, rooms one does not stay in, walks to
/// the living room without an action, in the second of its choice: from the WC 2 + 3 = 5 s, from
/// the bath 3 + 12 + 3 = 18 s through the landing and the hall. There it stands idle, its rates
/// the awake ones as before, until its planned choice, which weighs the offers from the living
/// room: the first such idle choice of the Saturday (day 5), a day without duty, so nothing but
/// that choice comes next. To the Sunday nobody stands idle in the WC or the bath without
/// waiting (the state after every second with events).
#[test]
fn idle_in_the_wc_or_the_bath_walks_to_the_living_room() {
    let mut whole = family();
    let end = 6 * D;
    let mut stood = Vec::new();
    whole.step(end, |world, second| {
        places_hold(world, second);
        let house = world.state().house();
        let waiting: BTreeSet<SimId> = house.waits.values().flatten().map(|&(_, s)| s).collect();
        for (&who, person) in &house.sims {
            if let (Place::Room(room), None, false) =
                (person.place, person.action, person.collapsed)
                && [WC, BATH].contains(&room)
                && !waiting.contains(&who)
            {
                stood.push((second, who.get(), room));
            }
        }
    });
    assert_eq!(stood, []);
    let first = |room: u8| {
        (whole.log.iter())
            .filter(|entry| entry.t.as_secs() >= 5 * D)
            .find_map(|entry| {
                let &[Subject::Sim(who)] = entry.subjects.as_slice() else {
                    return None;
                };
                match entry.payload {
                    LogPayload::Idle {
                        room: there,
                        waits: None,
                        ..
                    } if there == room => Some((entry.t.as_secs(), who)),
                    _ => None,
                }
            })
            .expect("one idle there")
    };
    for (room, secs) in [(WC, 5), (BATH, 18)] {
        let (at, who) = first(room);
        let mut run = family();
        run.advance(at);
        let person = &run.world.state().house().sims[&who];
        let walking = Place::Walking {
            from: room,
            to: LIVING,
            depart: t(at),
            arrive: t(at + secs),
        };
        assert_eq!((person.place, person.action), (walking, None), "{room}");
        let rates = person.needs.map(NeedSample::rate);
        run.advance(at + secs);
        let person = &run.world.state().house().sims[&who];
        assert_eq!((person.place, person.action), (Place::Room(LIVING), None));
        assert_eq!(person.needs.map(NeedSample::rate), rates);
        let next = (whole.log.iter())
            .filter(|entry| entry.subjects == [Subject::Sim(who)] && entry.t.as_secs() > at)
            .find_map(|entry| match entry.payload {
                LogPayload::ActionStarted { room, .. } | LogPayload::Idle { room, .. } => {
                    Some(room)
                }
                _ => None,
            });
        assert_eq!(next, Some(LIVING), "{room}");
    }
}

/// The night's order in the terraced house: the girls' night begins at 20:00, the couple's at
/// 22:00. From 19:00 the boy of 4 holds the first place of a bed in the kids' room, the single
/// bed 9 (`House::child_beds`: the kids' room holds the single beds 9 and 10). At 20:00 each girl
/// takes a bed for children, 10 or 12 (her tier: the single beds 9, 10 and 12, the 9 held and
/// so without a free place); at 22:00 she, sim 1, takes the double bed (her tier the beds for
/// couples, the 8) and he joins her on it (his tier her bed); each at its bedtime or when what it
/// does then ends. So every night of the week: the couple on the two places of the double bed,
/// the girls on 10 and 12, nobody on 9.
#[test]
fn the_couple_shares_the_double_bed_and_the_girls_the_single_beds() {
    let mut run = family();
    // Per night 1 to 7, the places of the four at 3:00 of the next morning: the state after the
    // last second with events before, which stands until the next.
    let mut nights = [[None; 4]; 7];
    run.step(8 * D + 12 * H, |world, second| {
        for (night, beds) in (1..).zip(nights.iter_mut()) {
            if second <= (night + 1) * D + 3 * H {
                let house = world.state().house();
                *beds = [1, 2, 3, 4].map(|id| {
                    let person = house.sims.get(&sim(id))?;
                    let at_object = matches!(person.place, Place::Room(_));
                    (person.action.filter(|_| at_object))
                        .map(|Action { object, slot, .. }| (object, slot))
                });
            }
        }
    });
    let couple: BTreeSet<_> = [Some((DOUBLE, 0)), Some((DOUBLE, 1))].into_iter().collect();
    let singles = [Some((KIDS_BEDS[1], 0)), Some((SMALL_ROOM_BED, 0))];
    for (night, [her, him, first, second]) in (1..).zip(nights) {
        assert_eq!(
            [her, him].into_iter().collect::<BTreeSet<_>>(),
            couple,
            "night {night}"
        );
        let girls: BTreeSet<_> = [first, second].into_iter().collect();
        assert_eq!(girls, singles.into_iter().collect(), "night {night}");
    }
    let tiers = |id: u64| -> Vec<Night> {
        (run.log.iter())
            .filter(|entry| entry.subjects == [Subject::Sim(sim(id))])
            .filter(|entry| entry.t.as_secs() < 2 * D)
            .filter_map(|entry| match &entry.payload {
                LogPayload::ActionStarted {
                    held, tier, busy, ..
                } if !tier.is_empty() => Some((
                    entry.t.as_secs() - D,
                    tier.clone(),
                    held.clone(),
                    busy.clone(),
                )),
                _ => None,
            })
            .collect()
    };
    let children = vec![KIDS_BEDS[0], KIDS_BEDS[1], SMALL_ROOM_BED];
    for id in [3, 4] {
        let (at, tier, held, busy) = tiers(id).swap_remove(0);
        assert!((20 * H..22 * H).contains(&at), "{id} at {at}");
        assert_eq!((tier, held), (children.clone(), vec![KIDS_BEDS[0]]));
        assert!(busy.contains(&KIDS_BEDS[0]));
    }
    for id in [1, 2] {
        let (at, tier, held, _) = tiers(id).swap_remove(0);
        assert!((22 * H..24 * H).contains(&at), "{id} at {at}");
        assert_eq!((tier, held), (vec![DOUBLE], vec![KIDS_BEDS[0]]));
    }
}

/// A choice in the night: its second after the first morning's midnight, the beds of the
/// person's tier, the objects with a place a small child holds, those without a free place.
type Night = (i64, Vec<u16>, Vec<u16>, Vec<u16>);

/// A child under 6 holds its place by the clock alone (`young_place`): from 19:00 to before 7:00
/// of the next morning, every night; one place per child in the order of `House::child_beds`,
/// none for a child beyond them.
#[test]
fn a_young_child_holds_its_bed_from_seven_to_seven() {
    let houses = common::repository_houses();
    let house = houses
        .house(&DataId::new("core:terraced").expect("id"))
        .expect("the terraced house");
    let held = |young: usize, at: i64| child_held(house, &houses, young, t(at));
    assert_eq!(held(1, D + 19 * H - 1), []);
    assert_eq!(held(1, D + 19 * H), [(KIDS_BEDS[0], 0)]);
    assert_eq!(held(1, 2 * D + 7 * H - 1), [(KIDS_BEDS[0], 0)]);
    assert_eq!(held(1, 2 * D + 7 * H), []);
    assert_eq!(
        held(2, 5 * D + 23 * H),
        [(KIDS_BEDS[0], 0), (KIDS_BEDS[1], 0)]
    );
    assert_eq!(
        held(3, 5 * D + 23 * H),
        [(KIDS_BEDS[0], 0), (KIDS_BEDS[1], 0)]
    );
    assert_eq!(held(0, 5 * D + 23 * H), []);
}

// ---------------------------------------------------------------------------------------
// The beds' tiers when they are full (point 3).

/// A double bed for couples (two places), a single bed for children, a guest bed from 18, the
/// sofa (energy +50 asleep) and the essential kinds, each filling 0 (a television for fun).
const BED_KINDS: &str = r#"[
  {"id": "test:double", "slots": 2, "bed_for": "couples", "offers": [
    {"id": "sleep", "sleep": true, "min_age": 0, "max_age": 130}]},
  {"id": "test:single", "slots": 1, "bed_for": "children", "offers": [
    {"id": "sleep", "sleep": true, "min_age": 0, "max_age": 130}]},
  {"id": "test:guest", "slots": 1, "offers": [
    {"id": "sleep", "sleep": true, "min_age": 18, "max_age": 130}]},
  {"id": "core:sofa", "slots": 2, "offers": [
    {"id": "sleep", "rates": {"energy": 50}, "sleep": true, "min_age": 0, "max_age": 130}]},
  {"id": "test:tv", "slots": 1, "offers": [
    {"id": "watch", "rates": {"fun": 1}, "seconds": 60, "min_age": 0, "max_age": 130}]},
  {"id": "core:dining_table", "slots": 1, "offers": [
    {"id": "eat", "rates": {"hunger": 1}, "seconds": 60, "min_age": 0, "max_age": 130}]},
  {"id": "core:fridge", "slots": 1, "offers": [
    {"id": "snack", "rates": {"hunger": 1}, "seconds": 60, "min_age": 0, "max_age": 130}]},
  {"id": "core:toilet", "slots": 1, "offers": [
    {"id": "use", "rates": {"bladder": 1}, "seconds": 60, "min_age": 0, "max_age": 130}]},
  {"id": "core:shower", "slots": 1, "offers": [
    {"id": "shower", "rates": {"hygiene": 1}, "seconds": 60, "min_age": 0, "max_age": 130}]}
]"#;

/// Rooms hall 0 (the entrance), living 1, bedroom 2, kids 3, guest 4, 3, 4, 5 and 6 s from the
/// hall. Objects: the double bed 0 in the bedroom, the single bed 1 in the kids' room, the guest
/// bed 2 in the guest room, the sofa 3 and the television 4 in the living room, table, fridge,
/// toilet and shower 5 to 8 in the hall.
const BEDS: &str = r#"{
  "id": "test:beds", "capacity": 4,
  "rooms": [{"id": "hall", "entrance": true}, {"id": "living"}, {"id": "bedroom"}, {"id": "kids"},
    {"id": "guest"}],
  "child_day_room": "kids", "child_night_room": "kids", "dwell_room": "living",
  "doors": [
    {"a": "hall", "b": "living", "seconds": 3},
    {"a": "hall", "b": "bedroom", "seconds": 4},
    {"a": "hall", "b": "kids", "seconds": 5},
    {"a": "hall", "b": "guest", "seconds": 6}],
  "objects": [{"kind": "test:double", "room": "bedroom"}, {"kind": "test:single", "room": "kids"},
    {"kind": "test:guest", "room": "guest"}, {"kind": "core:sofa", "room": "living"},
    {"kind": "test:tv", "room": "living"}, {"kind": "core:dining_table", "room": "hall"},
    {"kind": "core:fridge", "room": "hall"}, {"kind": "core:toilet", "room": "hall"},
    {"kind": "core:shower", "room": "hall"}]
}"#;

/// Awake energy falls 30 an hour, asleep it rises 100; every other need stands.
fn bed_rates() -> String {
    let bands = |rate: i32| [rate; 4];
    json!({
        "awake": {"hunger": bands(0), "energy": bands(-30), "bladder": bands(0),
                  "hygiene": bands(0), "fun": bands(0)},
        "asleep": {"hunger": bands(0), "energy": bands(100), "bladder": bands(0),
                   "hygiene": bands(0), "fun": bands(0)}
    })
    .to_string()
}

/// When a tier is full, the other beds come before the sofa (ADR-0025 section 3). The couple of
/// `common::day` and two girls of 8 and 10 (sims 3 and 4) in the bed house, moved in at
/// 23:59:59 of day 4: a Saturday without duty, its morning at 6:00 as on a weekday
/// (`common::weekday_mornings`). Nothing brings 40 000 by day, so all idle in the hall. At 20:00
/// energy is 900 − 14 · 30 = 480: sim 3 takes the bed for children, 5 s; sim 4 finds it taken
/// and takes the next of any bed open to her, the double bed (the guest bed is from 18), its
/// first place, 4 s. At 22:00 energy is 420: she, sim 1, takes the second place of the double
/// bed, her tier; he, sim 2, finds his tier, her bed, full and takes the guest bed, 6 s.
#[test]
fn a_full_tier_goes_to_the_other_beds() {
    let day = common::weekday_mornings(DAY_FILE);
    let houses = common::houses_from(&bed_rates(), &day, BED_KINDS, &[BEDS]);
    let data = common::death_data(1e-9, None, json!([])).with_houses(houses);
    let born = [-30 * Y, -32 * Y, -8 * Y, -10 * Y];
    let (mut run, _) = Run::family(data, 7, "test:house", &born, 4);
    let saturday = 5 * D + 6 * H;
    run.advance(saturday + 16 * H + 10);
    let started =
        |(object, slot): (u16, u8), walk: u32, energy: u16, busy: &[u16], tier: &[u16]| {
            LogPayload::ActionStarted {
                house: DataId::new("test:beds").expect("id"),
                object,
                offer: 0,
                slot,
                room: 0,
                walk_secs: walk,
                needs: [600, energy, 500, 700, 800],
                busy: busy.to_vec(),
                commanded: false,
                held: Vec::new(),
                tier: tier.to_vec(),
            }
        };
    let chose = |id: u64| {
        let all = doings(&run, id, saturday, saturday + 17 * H);
        all.into_iter()
            .find(|(_, payload)| matches!(payload, LogPayload::ActionStarted { .. }))
            .expect("a bed")
    };
    assert_eq!(chose(3), (14 * H, started((1, 0), 5, 480, &[], &[1])));
    assert_eq!(chose(4), (14 * H, started((0, 0), 4, 480, &[1], &[1])));
    assert_eq!(chose(1), (16 * H, started((0, 1), 4, 420, &[1], &[0])));
    assert_eq!(chose(2), (16 * H, started((2, 0), 6, 420, &[0, 1], &[0])));
}

/// The repository's day.
const DAY_FILE: &str = include_str!("../../../data/house/day.json");

// ---------------------------------------------------------------------------------------
// The weekend (point 4).

/// The night of a day ends with `until` of the next, or `until_weekend` when that is a Saturday
/// or a Sunday (weekday 5 or 6; day 4 is a Friday): 7:00 for 6 to 66, 6:30 from 67, else 6:00.
#[test]
fn the_night_before_a_weekend_morning_ends_later() {
    let rules: sim_core::house::data::DayRules = serde_json::from_str(DAY_FILE).expect("the day");
    let ends = |born_years: i64, day: i64| {
        let night = night_of(&rules, day, t(-born_years * Y)).expect("a night");
        night.until.map(|until| until.as_secs() - (day + 1) * D)
    };
    for years in [8, 15, 30] {
        let ends: Vec<_> = (3..=6).map(|day| ends(years, day)).collect();
        assert_eq!(
            ends,
            [Some(6 * H), Some(7 * H), Some(7 * H), Some(6 * H)],
            "{years}"
        );
    }
    let old: Vec<_> = (3..=6).map(|day| ends(70, day)).collect();
    assert_eq!(
        old,
        [
            Some(6 * H),
            Some(6 * H + 1_800),
            Some(6 * H + 1_800),
            Some(6 * H)
        ]
    );
}

/// The family's sleeps end at 7:00 on Saturday and Sunday (days 5 and 6) and at 6:00 on the
/// Monday after (day 7), for the couple and for the girls, each a `core:action_ended` of its bed
/// with reason `done`.
#[test]
fn the_family_sleeps_in_on_the_weekend() {
    let mut run = family();
    run.advance(7 * D + 12 * H);
    for id in 1..=4 {
        let ends: Vec<i64> = (run.log.iter())
            .filter(|entry| entry.subjects == [Subject::Sim(sim(id))])
            .filter_map(|entry| match entry.payload {
                LogPayload::ActionEnded {
                    object,
                    reason: sim_core::log::EndReason::Done,
                    ..
                } if [DOUBLE, KIDS_BEDS[0], KIDS_BEDS[1], SMALL_ROOM_BED].contains(&object) => {
                    Some(entry.t.as_secs())
                }
                _ => None,
            })
            .filter(|&at| at >= 5 * D)
            .collect();
        assert_eq!(ends, [5 * D + 7 * H, 6 * D + 7 * H, 7 * D + 6 * H], "{id}");
    }
}

// ---------------------------------------------------------------------------------------
// Saves (point 6).

/// Saved with an idle person's next choice pending, on the walk to the living room, and at night
/// with the boy's bed held: each save loads, saves to the same bytes and runs on as the
/// uninterrupted family to noon of day 3, with the same state hash and the same entries. The
/// first is the second of the first idle choice after 6:30 of day 1 that waits for nothing, the
/// second that of the first walk to the living room, the third 23:00 of day 1.
#[test]
fn saves_with_a_planned_choice_a_walk_and_a_held_bed_run_on_the_same() {
    let end = 3 * D + 12 * H;
    let mut whole = family();
    whole.advance(end);
    let idle_after = |from: i64, walks: bool| {
        (whole.log.iter())
            .filter(|entry| entry.t.as_secs() >= from)
            .find_map(|entry| match entry.payload {
                LogPayload::Idle {
                    room, waits: None, ..
                } if [WC, BATH].contains(&room) == walks => Some(entry.t.as_secs()),
                _ => None,
            })
            .expect("an idle choice")
    };
    let planned = idle_after(D + 6 * H + 1_800, false);
    let walking = idle_after(D + 6 * H, true);
    for saved_at in [planned, walking, D + 23 * H] {
        let mut run = family();
        run.advance(saved_at);
        let bytes = run.world.save(&run.out.year_hashes).expect("saves");
        let data = common::repository_data();
        let (mut loaded, _) = World::load(&bytes, data).expect("loads");
        assert_eq!(loaded.save(&run.out.year_hashes).expect("saves"), bytes);
        loaded.advance_until(&mut YearOut::default(), t(end), &mut |_| false);
        let saved = run.log.len();
        run.advance(end);
        assert_eq!(loaded.state_hash(), run.world.state_hash(), "{saved_at}");
        let entries = |log: &[LogEntry]| serde_json::to_value(log).expect("json");
        assert_eq!(
            entries(&loaded.take_log()),
            entries(&run.log[saved..]),
            "{saved_at}"
        );
        assert_eq!(loaded.state_hash(), whole.world.state_hash(), "{saved_at}");
    }
}

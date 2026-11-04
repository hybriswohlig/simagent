//! Places in a crowd (plan of phase 2, section 5 item 4; ADR-0026 section 5): one bathroom for
//! four in the morning, food nobody may eat, a death in the middle of an action, and an arrival
//! in the second of leaving. Waiting for a taken object, a hold passed on, and leaving the
//! house on the way are in `house_waiting.rs` and `house_day.rs`; a critical need that ends a
//! walk in `house_critical.rs`; a departure that waits for an action in `house_rhythm.rs`.
//! The values by hand, except where a rule finds the seed.

mod common;

use common::day::{self, MORNING, Run, number, places_hold};
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::house::data::Need;
use sim_core::house::state::{Duty, Place};
use sim_core::ids::SimId;
use sim_core::input::{Input, MoveIn};
use sim_core::log::{Cause, EndReason, LogEntry, LogPayload, Subject};
use sim_core::time::{DAY, HOUR, SimTime, YEAR};

const D: i64 = DAY.as_secs();
const Y: i64 = YEAR.as_secs();
const MORNING_VALUES: [u16; 5] = [600, 900, 500, 700, 800];

fn t(secs: i64) -> SimTime {
    SimTime::from_secs(secs)
}

fn sim(id: u64) -> SimId {
    SimId::try_from(id).expect("valid")
}

fn started(
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

fn idle(house: &str, room: u8, needs: [u16; 5], busy: &[u16], waits: Option<u16>) -> LogPayload {
    LogPayload::Idle {
        house: DataId::new(house).expect("id"),
        room,
        needs,
        busy: busy.to_vec(),
        waits,
    }
}

/// A person's entries of what it does and of leaving, as (second after `morning`, payload).
fn doings(run: &Run, id: u64, morning: i64) -> Vec<(i64, LogPayload)> {
    (run.log.iter())
        .filter(|entry| entry.subjects == [Subject::Sim(sim(id))])
        .filter(|entry| {
            matches!(
                entry.payload,
                LogPayload::ActionStarted { .. }
                    | LogPayload::Reached { .. }
                    | LogPayload::ActionEnded { .. }
                    | LogPayload::Idle { .. }
                    | LogPayload::NeedCritical { .. }
                    | LogPayload::LeftHome { .. }
            )
        })
        .map(|entry| (entry.t.as_secs() - morning, entry.payload.clone()))
        .collect()
}

/// A toilet (bladder +12 000 an hour until full, at most 5 min) and a shower (hygiene +2 800
/// for 15 min), one place each, and a bed for four.
const BATH_KINDS: &str = r#"
  {"id": "test:toilet", "slots": 1, "offers": [
    {"id": "use", "rates": {"bladder": 12000}, "until_full": "bladder", "max_seconds": 300, "min_age": 0, "max_age": 130}]},
  {"id": "test:shower", "slots": 1, "offers": [
    {"id": "shower", "rates": {"hygiene": 2800}, "seconds": 900, "min_age": 0, "max_age": 130}]},
  {"id": "test:bed", "slots": 4, "offers": [
    {"id": "sleep", "sleep": true, "min_age": 0, "max_age": 130}]}"#;

/// Rooms hall 0 (the entrance), bath 1 and bedroom 2, each 3 s from the hall. Objects: toilet
/// 0 and shower 1 in the bath, bed 2 in the bedroom, the five essential ones 3 to 7 in the hall.
const BATH: &str = r#"{
  "id": "test:bath", "capacity": 4,
  "rooms": [{"id": "hall", "entrance": true}, {"id": "bath"}, {"id": "bedroom"}],
  "child_day_room": "hall", "child_night_room": "bedroom",
  "doors": [{"a": "hall", "b": "bath", "seconds": 3}, {"a": "hall", "b": "bedroom", "seconds": 3}],
  "objects": [{"kind": "test:toilet", "room": "bath"}, {"kind": "test:shower", "room": "bath"},
    {"kind": "test:bed", "room": "bedroom"}, ESSENTIALS]
}"#;

/// One bathroom, four persons (ADR-0026 test `one_bathroom_four_people`): a woman of 30, a man
/// of 32 and children of 10 and 8, moved in on day 4, so their first morning is a Saturday
/// without duties; every need still awake. At 6:00 the toilet brings each 500 · 500 − 50 · 3 =
/// 249 850, the shower 300 · 300 − 150 = 89 850, the bed 100 · 100 − 150. She takes the toilet,
/// he the shower; the children find both taken and wait for the toilet, the better one, from
/// 6:00, the girl of 10 (sim 3) first by her SimId. The toilet is full 150 s after each arrival:
/// at 153 s it is held for sim 3, and she, done, waits for the shower (89 750 from the bath);
/// at 306 s for sim 4, and sim 3 waits for the shower behind her; at 459 s sim 4 waits too. The
/// shower frees at 903 s: held for her, the first to wait; he, from it, takes the free toilet
/// (bladder 500 still: 249 750). Then the shower goes to sim 3 at 1 808 s and to sim 4 at
/// 2 713 s, in the order they began to wait; sim 4 is done at 3 618 s. Never two on a place,
/// and nobody waits beyond the end of all the others.
#[test]
fn one_bathroom_four_people_in_the_morning() {
    let data = day::house_data(&day::rates(0, 0, 0), BATH_KINDS, BATH);
    let born = [-30 * Y, -32 * Y, -10 * Y, -8 * Y];
    let (mut run, _) = Run::family(data, 7, "test:house", &born, 4);
    let morning = 5 * D + 6 * HOUR.as_secs();
    run.step(morning + 2 * HOUR.as_secs(), places_hold);
    let bath = "test:bath";
    let full = [600, 900, 1000, 700, 800];
    let washed = [600, 900, 500, 1000, 800];
    let done = [600, 900, 1000, 1000, 800];
    assert_eq!(
        doings(&run, 1, morning),
        [
            (0, started(bath, (0, 0), (0, 3), MORNING_VALUES, &[])),
            (3, LogPayload::Reached { object: 0 }),
            (153, ended(0, EndReason::Full, full)),
            (153, idle(bath, 1, full, &[0, 1], Some(1))),
            (903, started(bath, (1, 0), (1, 5), full, &[0])),
            (908, LogPayload::Reached { object: 1 }),
            (1_808, ended(1, EndReason::Done, done)),
            (1_808, idle(bath, 1, done, &[1], None)),
        ]
    );
    assert_eq!(
        doings(&run, 2, morning),
        [
            (0, started(bath, (1, 0), (0, 3), MORNING_VALUES, &[0])),
            (3, LogPayload::Reached { object: 1 }),
            (903, ended(1, EndReason::Done, washed)),
            (903, started(bath, (0, 0), (1, 5), washed, &[1])),
            (908, LogPayload::Reached { object: 0 }),
            (1_058, ended(0, EndReason::Full, done)),
            (1_058, idle(bath, 1, done, &[1], None)),
        ]
    );
    let child = |id: u64, toilet_at: i64, shower_at: i64, last: &[u16]| {
        assert_eq!(
            doings(&run, id, morning),
            [
                (0, idle(bath, 0, MORNING_VALUES, &[0, 1], Some(0))),
                (
                    toilet_at,
                    started(bath, (0, 0), (0, 3), MORNING_VALUES, &[1])
                ),
                (toilet_at + 3, LogPayload::Reached { object: 0 }),
                (toilet_at + 153, ended(0, EndReason::Full, full)),
                (
                    toilet_at + 153,
                    idle(bath, 1, full, if id == 3 { &[0, 1] } else { &[1] }, Some(1))
                ),
                (shower_at, started(bath, (1, 0), (1, 5), full, &[])),
                (shower_at + 5, LogPayload::Reached { object: 1 }),
                (shower_at + 905, ended(1, EndReason::Done, done)),
                (shower_at + 905, idle(bath, 1, done, last, None)),
            ],
            "{id}"
        );
    };
    child(3, 153, 1_808, &[1]);
    child(4, 306, 2_713, &[]);
    // Each hold came by the end that freed the place: the children's toilet by the one before
    // them, the shower by the shower before.
    let freed_by = |id: u64, object: u16| {
        let start = (run.log.iter())
            .filter(|entry| entry.subjects == [Subject::Sim(sim(id))])
            .find(|entry| matches!(entry.payload, LogPayload::ActionStarted { object: o, .. } if o == object))
            .expect("a start");
        let [Cause::Plan(plan)] = start.causes.as_slice() else {
            panic!("planned: {:?}", start.causes);
        };
        let by = (run.log.iter()).find(|entry| number(&entry.id) == number(&plan.by));
        by.map(|entry| (entry.subjects.clone(), entry.payload.clone()))
    };
    let end_of = |id: u64, object: u16, reason: EndReason, needs: [u16; 5]| {
        Some((vec![Subject::Sim(sim(id))], ended(object, reason, needs)))
    };
    assert_eq!(freed_by(3, 0), end_of(1, 0, EndReason::Full, full));
    assert_eq!(freed_by(4, 0), end_of(3, 0, EndReason::Full, full));
    assert_eq!(freed_by(1, 1), end_of(2, 1, EndReason::Done, washed));
    assert_eq!(freed_by(3, 1), end_of(1, 1, EndReason::Done, done));
    assert_eq!(freed_by(4, 1), end_of(3, 1, EndReason::Done, done));
    let house = run.world.state().house();
    assert!(house.waits.is_empty() && house.holds.is_empty());
}

/// A soup for those of 67 or more (hunger +6 000 an hour until full, at most 10 min) and a
/// gruel for all (hunger +600 an hour for 2 min), two places each, and a cot for two.
const OLD_KINDS: &str = r#"
  {"id": "test:soup", "slots": 2, "offers": [
    {"id": "eat", "rates": {"hunger": 6000}, "until_full": "hunger", "max_seconds": 600, "min_age": 67, "max_age": 130}]},
  {"id": "test:gruel", "slots": 2, "offers": [
    {"id": "eat", "rates": {"hunger": 600}, "seconds": 120, "min_age": 0, "max_age": 130}]},
  {"id": "test:cot", "slots": 2, "offers": [
    {"id": "sleep", "sleep": true, "min_age": 0, "max_age": 130}]}"#;

/// Rooms hall 0 (the entrance) and kitchen 1, 3 s apart. Objects: soup 0 and gruel 1 in the
/// kitchen, cot 2 and the five essential ones 3 to 7 in the hall.
const OLD: &str = r#"{
  "id": "test:old", "capacity": 2,
  "rooms": [{"id": "hall", "entrance": true}, {"id": "kitchen"}],
  "child_day_room": "hall", "child_night_room": "hall",
  "doors": [{"a": "hall", "b": "kitchen", "seconds": 3}],
  "objects": [{"kind": "test:soup", "room": "kitchen"}, {"kind": "test:gruel", "room": "kitchen"},
    {"kind": "test:cot", "room": "hall"}, ESSENTIALS]
}"#;

/// Food nobody may eat: the couple of `common::day` (30 and 32) next to a soup for 67 and
/// more, hunger falling 3 000 an hour awake, everything else still. The soup is no candidate
/// at their age, free or not: at 6:00 the best is the cot (100 · 100 − 250 = 9 750; the gruel
/// 400 · 20 − 150), not enough, and nothing is taken, so nobody waits; at hunger 400 (240 s)
/// the gruel brings 600 · 20 − 150, still idle. At 150 (540 s) hunger is critical and the gruel
/// takes it (10⁹ + 850 · 20 − 150 beats the table's 10⁹ − 250), there at 543 s with hunger 148
/// (600 − 452.5 truncated), 168 after its 2 min; idle in the kitchen (832 · 20 − 250), hunger
/// 150 again 22 s later (18 at −3 000 an hour), the gruel again. Until 7:00 neither eats the
/// soup or waits for it.
#[test]
fn food_nobody_may_eat_is_never_chosen_nor_waited_for() {
    let data = day::house_data(&day::rates(-3000, 0, 0), OLD_KINDS, OLD);
    let (mut run, _) = Run::moved_in(data);
    run.step(MORNING + HOUR.as_secs(), places_hold);
    let old = "test:old";
    let critical = LogPayload::NeedCritical {
        need: Need::Hunger,
        level: 150,
    };
    let starving = [150, 900, 500, 700, 800];
    for (id, slot) in [(1, 0), (2, 1)] {
        assert_eq!(
            doings(&run, id, MORNING)[..11],
            [
                (0, idle(old, 0, MORNING_VALUES, &[], None)),
                (240, idle(old, 0, [400, 900, 500, 700, 800], &[], None)),
                (540, critical.clone()),
                (540, started(old, (1, slot), (0, 3), starving, &[])),
                (543, LogPayload::Reached { object: 1 }),
                (663, ended(1, EndReason::Done, [168, 900, 500, 700, 800])),
                (663, idle(old, 1, [168, 900, 500, 700, 800], &[], None)),
                (685, critical.clone()),
                (685, started(old, (1, slot), (1, 5), starving, &[])),
                (690, LogPayload::Reached { object: 1 }),
                (810, ended(1, EndReason::Done, [166, 900, 500, 700, 800])),
            ],
            "{id}"
        );
    }
    for entry in &run.log {
        let soup = match entry.payload {
            LogPayload::ActionStarted { object, .. } => object == 0,
            LogPayload::Idle { waits, .. } => waits == Some(0),
            _ => false,
        };
        assert!(!soup, "{entry:?}");
    }
}

/// Fifty accidents a year: within a week someone of the family of four dies.
const ACCIDENT: f64 = 50.0;
/// The seeds a death in the middle of an action is looked for in, from this one up (never
/// reseeded): the first whose family is whole when it moves in, and whose first death after the
/// first morning ends an action.
const SEED_DEATH: u64 = 0x5eed_0002_0004_0001;

/// The family of four of seed `n` of `SEED_DEATH`, moved in at 23:59:59 of day 0, if it is
/// whole then.
fn mortal(data: &Data, n: u64) -> Option<Run> {
    let born = [30, 33, 6, 11].map(|years| -years * Y);
    let mut run = Run::founded(data.clone(), SEED_DEATH + n, "test:week", &born);
    run.advance(D - 2);
    let home = run.together(born.len())?;
    run.submit(Input::MoveIn(MoveIn { household: home }));
    Some(run)
}

/// A death in the middle of an action (ADR-0024 section 2, ADR-0026 section 4): a family of
/// four (30, 33, 6 and 11) in the repository's terraced house on data with fifty accidents a
/// year, the seed by the rule of `SEED_DEATH`. In the death's second the action ends with
/// reason `left`, caused by the person's `core:moved_out`, caused by its `core:death`; after it
/// the dead has no needs, whoever holds its place took it by a hold from that
/// `core:moved_out`, the places hold after every second to the end of the next day, and no
/// entry names the dead after that second (an event of someone gone would panic with
/// `house.orphan_event`).
#[test]
fn a_death_in_the_middle_of_an_action() {
    let data = common::death_data(ACCIDENT, None, serde_json::json!([]))
        .with_houses(common::repository_houses());
    let left = |entry: &LogEntry| {
        matches!(
            entry.payload,
            LogPayload::ActionEnded {
                reason: EndReason::Left,
                ..
            }
        )
    };
    let (n, death) = (0..100)
        .find_map(|n| {
            let mut run = mortal(&data, n)?;
            run.advance(8 * D);
            let death = (run.log.iter())
                .filter(|entry| entry.t.as_secs() >= MORNING)
                .find(|entry| matches!(entry.payload, LogPayload::Death { .. }))?
                .clone();
            let ends = |entry: &LogEntry| {
                entry.t == death.t && entry.subjects == death.subjects && left(entry)
            };
            run.log.iter().any(ends).then_some((n, death))
        })
        .expect("a seed");
    let &[Subject::Sim(dead)] = death.subjects.as_slice() else {
        panic!("one person");
    };
    let second = death.t.as_secs();
    let mut run = mortal(&data, n).expect("whole");
    run.advance(second - 1);
    let before = run.log.len();
    run.advance(second);
    let by_id = |id: &sim_core::ids::EventId| {
        (run.log.iter()).find(|entry| number(&entry.id) == number(id))
    };
    let cause = |entry: &LogEntry| match entry.causes.as_slice() {
        [Cause::Event(id)] => by_id(id),
        _ => None,
    };
    let ended = (run.log[before..].iter())
        .find(|entry| entry.subjects == death.subjects && left(entry))
        .expect("the end");
    let out = cause(ended).expect("its cause");
    assert!(matches!(out.payload, LogPayload::MovedOut { .. }) && out.subjects == death.subjects);
    assert_eq!(
        cause(out).map(|died| number(&died.id)),
        Some(number(&death.id))
    );
    let place = (run.log[..before].iter())
        .filter(|entry| entry.subjects == death.subjects)
        .filter_map(|entry| match entry.payload {
            LogPayload::ActionStarted { object, slot, .. } => Some((object, slot)),
            _ => None,
        })
        .next_back()
        .expect("its action");
    let house = run.world.state().house();
    assert!(!house.sims.contains_key(&dead));
    for (&other, person) in &house.sims {
        if person
            .action
            .is_some_and(|action| (action.object, action.slot) == place)
        {
            let start = (run.log[before..].iter())
                .find(|entry| {
                    entry.subjects == [Subject::Sim(other)]
                        && matches!(entry.payload, LogPayload::ActionStarted { .. })
                })
                .expect("taken in the second");
            let [Cause::Plan(plan)] = start.causes.as_slice() else {
                panic!("planned: {:?}", start.causes);
            };
            assert_eq!(number(&plan.by), number(&out.id));
        }
    }
    places_hold(&run.world, second);
    run.step((second / D + 2) * D, places_hold);
    let mentioned = |entry: &&LogEntry| entry.subjects.contains(&Subject::Sim(dead));
    let after: Vec<_> = (run.log.iter())
        .filter(|entry| entry.t.as_secs() > second)
        .filter(mentioned)
        .collect();
    assert!(after.is_empty(), "{after:?}");
}

/// A stew (hunger +160 an hour for 9 500 s) in the hall, a game (fun +600 an hour for 30 min)
/// in the den, a cot for two; two places each.
const LATE_KINDS: &str = r#"
  {"id": "test:stew", "slots": 2, "offers": [
    {"id": "simmer", "rates": {"hunger": 160}, "seconds": 9500, "min_age": 0, "max_age": 130}]},
  {"id": "test:game", "slots": 2, "offers": [
    {"id": "play", "rates": {"fun": 600}, "seconds": 1800, "min_age": 0, "max_age": 130}]},
  {"id": "test:cot", "slots": 2, "offers": [
    {"id": "sleep", "sleep": true, "min_age": 0, "max_age": 130}]}"#;

/// Rooms hall 0 (the entrance) and den 1, 95 s apart. Objects: stew 0 in the hall, game 1 in
/// the den, cot 2 and the five essential ones 3 to 7 in the hall.
const LATE: &str = r#"{
  "id": "test:late", "capacity": 2,
  "rooms": [{"id": "hall", "entrance": true}, {"id": "den"}],
  "child_day_room": "hall", "child_night_room": "hall",
  "doors": [{"a": "hall", "b": "den", "seconds": 95}],
  "objects": [{"kind": "test:stew", "room": "hall"}, {"kind": "test:game", "room": "den"},
    {"kind": "test:cot", "room": "hall"}, ESSENTIALS]
}"#;

/// An arrival in the second of leaving: the couple of `common::day`, both full time (leaving
/// at 8:40, 9 600 s after the first morning), fun falling 60 an hour awake, all else still. At
/// 6:00 the stew (400 · 400 − 250 = 159 750) beats the game (200 · 200 − 50 · 95) and the cot;
/// there at 5 s, done at 9 505 s (8:38:25) with hunger full and fun 642 (800 − 158.4
/// truncated). Then the game brings 358 · 300 − 4 750 = 102 650: walking 95 s, each would
/// arrive at 8:40:00, the second of leaving. The departure runs first (its kind comes before
/// the arrival's): fun 640 is not critical, so the walk ends there with reason `obligation`
/// (back in the hall, as if not gone), `core:left_home` on time, and the walk out of 5 s; the
/// arrival planned for 8:40:00 is cancelled and never fires. Their wakes at 7:55 and fun at 700
/// (7:40) came while they were busy and did nothing.
#[test]
fn an_arrival_in_the_second_of_leaving() {
    let data = day::house_data(&day::rates(0, 0, -60), LATE_KINDS, LATE);
    let (mut run, _) = Run::moved_in(data);
    let leaving = MORNING + 9_600;
    run.step(leaving, places_hold);
    let late = "test:late";
    let stewed = [1000, 900, 500, 700, 642];
    for (id, slot) in [(1, 0), (2, 1)] {
        assert_eq!(
            doings(&run, id, MORNING),
            [
                (0, started(late, (0, slot), (0, 5), MORNING_VALUES, &[])),
                (5, LogPayload::Reached { object: 0 }),
                (9_505, ended(0, EndReason::Done, stewed)),
                (9_505, started(late, (1, slot), (0, 95), stewed, &[])),
                (
                    9_600,
                    ended(1, EndReason::Obligation, [1000, 900, 500, 700, 640])
                ),
                (
                    9_600,
                    LogPayload::LeftHome {
                        place: Duty::Work,
                        late_secs: 0
                    }
                ),
            ],
            "{id}"
        );
        let person = &run.world.state().house().sims[&sim(id)];
        let out = Place::Walking {
            from: 0,
            to: 0,
            depart: t(leaving),
            arrive: t(leaving + 5),
        };
        assert_eq!((person.place, person.action), (out, None), "{id}");
    }
    run.step(leaving + 5, places_hold);
    for id in [1, 2] {
        assert_eq!(run.world.state().house().sims[&sim(id)].place, Place::Away);
        assert_eq!(run.world.away(sim(id)), Some((Duty::Work, t(D + 62_400))));
    }
    assert_eq!(
        (
            doings(&run, 1, MORNING).len(),
            doings(&run, 2, MORNING).len()
        ),
        (6, 6)
    );
}

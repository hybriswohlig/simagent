//! Hunger at 0 and the reserve (ADR-0026, addendum of 2026-11-06 on hunger at 0): hunger at 0
//! drains the reserve, sized once when the needs begin (1 000 for a body without a value, as the
//! map 3 fixture of the core tests gives every person); at half the person is weak, at 0 wasted:
//! a walk takes twice as long, a command only feeds, and energy falls faster from the next change
//! of rates; eating refills it. Every second here is `tools/reserve_reference.py`, the rule
//! written from the decision.
//!
//! The test house: rooms hall 0 (the entrance), living 1, bath 2, bedroom 3; the living room 3 s,
//! the bath 2 s and the bedroom 4 s from the hall, 5 s to an object in one's own room. Objects: a
//! bed for three 0 in the bedroom; the television 1 (fun +1 an hour for 12 h) and a bookshelf 2 in
//! the living room; the shower 3 and the toilet 4 in the bath; the dining table 5 (from 18) and
//! the fridge 6 (one place, hunger +1 an hour for 12 h) in the hall; the sofa 7 in the living room.
//! The household: a woman of 30 (sim 1) and a man of 70 (sim 2) whose needs stand still, and a
//! girl of 8 (sim 3) whose hunger falls 1 000 an hour and energy 61 an hour awake, rising 80
//! asleep. They move in on a Friday; their first morning is a Saturday at 6:00 (`SATURDAY`), with
//! the nights' ends of a weekday (`common::weekday_mornings`).
//!
//! The woman is told to the fridge at once and again in the second her snack ends, so the fridge
//! stays taken for a day; the girl, waiting for it from her hunger at 150, is told to the
//! television the second after her hunger reaches 0. Every choice that decides a step is carried
//! by a critical need or by the night's order, so the mood changes none.

mod common;

use common::day::{Run, number, places_hold};
use serde_json::{Value, json};
use sim_core::data::Data;
use sim_core::house::data::Need;
use sim_core::house::need::NeedSample;
use sim_core::house::reserve::{Band, Reserve};
use sim_core::ids::SimId;
use sim_core::input::{Command, Input, InputRejected, RejectCode};
use sim_core::log::{Cause, EndReason, LogEntry, LogPayload, Subject};
use sim_core::time::{DAY, HOUR, SimTime, YEAR};
use sim_core::world::{World, YearOut};

const D: i64 = DAY.as_secs();
const H: i64 = HOUR.as_secs();
const Y: i64 = YEAR.as_secs();

const REPOSITORY_DAY: &str = include_str!("../../../data/house/day.json");

/// The first morning: day 5, a Saturday, at 6:00.
const SATURDAY: i64 = 5 * D + 6 * H;

const BED: u16 = 0;
const TV: u16 = 1;
const BOOK: u16 = 2;
const FRIDGE: u16 = 6;
const LIVING: u8 = 1;
const BEDROOM: u8 = 3;

/// The seconds after `SATURDAY` of `tools/reserve_reference.py` (`lever`).
const HUNGER_0: i64 = 2_160;
const WOMAN_FIRST_END: i64 = 43_206;
const WOMAN_SECOND_END: i64 = 86_411;
const WEAKENED: i64 = 24_660;
const WASTED: i64 = 47_160;
const TV_ARRIVE: i64 = 2_164;
const TV_STOP: i64 = 44_263;
const COLLAPSE: i64 = 49_610;
const STAND_UP: i64 = 67_610;
const BED_ARRIVE: i64 = 67_624;
const DAWN: i64 = 86_400;
const FRIDGE_ARRIVE: i64 = 86_419;

fn t(secs: i64) -> SimTime {
    SimTime::from_secs(secs)
}

fn sim(id: u64) -> SimId {
    SimId::try_from(id).expect("valid")
}

/// Offer `id` raising `need` by `rate` an hour for `seconds`, from `min_age`.
fn offer(id: &str, need: &str, rate: i32, (seconds, min_age): (u32, u8)) -> Value {
    json!({"id": id, "rates": {need: rate}, "seconds": seconds, "min_age": min_age, "max_age": 130})
}

fn kinds() -> Value {
    let sleep = json!({"id": "sleep", "sleep": true, "min_age": 0, "max_age": 130});
    let sofa = json!({"id": "sleep", "rates": {"energy": 50}, "sleep": true, "min_age": 0,
        "max_age": 130});
    json!([
        {"id": "test:bed", "slots": 3, "offers": [sleep]},
        {"id": "test:tv", "slots": 1, "offers": [offer("watch", "fun", 1, (43_200, 0))]},
        {"id": "test:book", "slots": 2, "offers": [offer("read", "fun", 1, (60, 0))]},
        {"id": "core:shower", "slots": 1, "offers": [offer("shower", "hygiene", 1, (60, 0))]},
        {"id": "core:toilet", "slots": 1, "offers": [offer("use", "bladder", 1, (60, 0))]},
        {"id": "core:dining_table", "slots": 1, "offers": [offer("eat", "hunger", 1, (60, 18))]},
        {"id": "core:fridge", "slots": 1, "offers": [offer("snack", "hunger", 1, (43_200, 0))]},
        {"id": "core:sofa", "slots": 1, "offers": [sofa]}
    ])
}

fn house() -> String {
    json!({
        "id": "test:hunger", "capacity": 3,
        "rooms": [{"id": "hall", "entrance": true}, {"id": "living"}, {"id": "bath"},
            {"id": "bedroom"}],
        "child_day_room": "living", "child_night_room": "bedroom", "dwell_room": "living",
        "doors": [
            {"a": "hall", "b": "living", "seconds": 3},
            {"a": "hall", "b": "bath", "seconds": 2},
            {"a": "hall", "b": "bedroom", "seconds": 4}],
        "objects": [
            {"kind": "test:bed", "room": "bedroom"}, {"kind": "test:tv", "room": "living"},
            {"kind": "test:book", "room": "living"}, {"kind": "core:shower", "room": "bath"},
            {"kind": "core:toilet", "room": "bath"}, {"kind": "core:dining_table", "room": "hall"},
            {"kind": "core:fridge", "room": "hall"}, {"kind": "core:sofa", "room": "living"}]
    })
    .to_string()
}

/// The girl's rates in the band of 6 to 12, every other band still; asleep her hunger falls 40
/// and her energy rises 80 an hour.
fn rates() -> String {
    let band = |girl: i32| json!([girl, 0, 0, 0]);
    json!({
        "awake": {"hunger": band(-1000), "energy": band(-61), "bladder": band(0),
            "hygiene": band(0), "fun": band(0)},
        "asleep": {"hunger": band(-40), "energy": band(80), "bladder": band(0),
            "hygiene": band(0), "fun": band(0)},
        "comfort": [1000, 1000, 1000, 1000, 1000]
    })
    .to_string()
}

fn data() -> Data {
    let day = common::weekday_mornings(REPOSITORY_DAY);
    let houses = common::houses_from(&rates(), &day, &kinds().to_string(), &[&house()]);
    common::death_data(1e-9, None, json!([])).with_houses(houses)
}

fn command(id: u64, object: u16) -> Input {
    Input::Command(Command {
        sim: sim(id),
        object,
        offer: 0,
    })
}

/// The inputs of the lever, each at its clock (acting in the second after it): the woman to the
/// fridge at once and in the second her first snack ends, the girl to the television the second
/// after her hunger reaches 0.
fn inputs() -> Vec<(i64, Input)> {
    vec![
        (SATURDAY, command(1, FRIDGE)),
        (SATURDAY + HUNGER_0, command(3, TV)),
        (SATURDAY + WOMAN_FIRST_END - 1, command(1, FRIDGE)),
    ]
}

/// The household moved in at 23:59:59 of day 4, run to `until` one second with events at a time
/// with the places checked after each, the inputs before `until` given at their clocks.
fn run(until: i64) -> Run {
    let born = [-30 * Y, -70 * Y, -8 * Y];
    let (mut run, _) = Run::family(data(), 7, "test:hunger", &born, 4);
    for (clock, input) in inputs().into_iter().filter(|(clock, _)| *clock < until) {
        run.step(clock, places_hold);
        run.submit(input);
    }
    run.step(until, places_hold);
    run
}

/// What the girl's log says from the first morning on, as (seconds after it, payload): her
/// choices, ends, idling, critical and empty needs, the collapse and the reserve's entries.
fn girl(run: &Run) -> Vec<(i64, LogPayload)> {
    (run.log.iter())
        .filter(|entry| entry.subjects == [Subject::Sim(sim(3))])
        .filter(|entry| entry.t.as_secs() >= SATURDAY)
        .filter(|entry| {
            matches!(
                entry.payload,
                LogPayload::ActionStarted { .. }
                    | LogPayload::ActionEnded { .. }
                    | LogPayload::NeedEmpty { .. }
                    | LogPayload::Collapsed { .. }
                    | LogPayload::Weakened { .. }
                    | LogPayload::Wasted { .. }
            )
        })
        .map(|entry| (entry.t.as_secs() - SATURDAY, entry.payload.clone()))
        .collect()
}

/// The girl's walk that begins at `after`: to `object` from `room`, `walk_secs` long.
fn walked(run: &Run, after: i64) -> (u16, u8, u32) {
    let entry = (run.log.iter())
        .find(|entry| {
            entry.t.as_secs() == SATURDAY + after
                && entry.subjects == [Subject::Sim(sim(3))]
                && matches!(entry.payload, LogPayload::ActionStarted { .. })
        })
        .expect("a walk");
    match entry.payload {
        LogPayload::ActionStarted {
            object,
            room,
            walk_secs,
            ..
        } => (object, room, walk_secs),
        _ => unreachable!(),
    }
}

/// The girl's energy and reserve after the clock of `run`.
fn girl_now(run: &Run) -> (NeedSample, Reserve) {
    let person = &run.world.state().house().sims[&sim(3)];
    (person.needs[1], person.reserve)
}

fn reserve_entry(run: &Run, after: i64) -> &LogEntry {
    (run.log.iter())
        .find(|entry| {
            entry.t.as_secs() == SATURDAY + after
                && entry.subjects == [Subject::Sim(sim(3))]
                && matches!(
                    entry.payload,
                    LogPayload::Weakened { .. } | LogPayload::Wasted { .. }
                )
        })
        .expect("the entry")
}

/// A rejected input's code; nothing changed (ADR-0004 section 4).
fn rejected(world: &mut World, input: Input) -> Option<RejectCode> {
    let before = world.state_hash();
    let code = (world.submit(input).err()).map(|InputRejected { code }| code);
    assert_eq!(world.state_hash(), before);
    assert!(world.take_log().is_empty());
    code
}

/// The girl's hunger is at 0 at 6:36 (600 at −1 000 an hour); her reserve, full at 1 000 (no
/// body in the data), drains 80 an hour from then: at 500 after 22 500 s, `core:weakened`, at 0
/// after 45 000 s, `core:wasted`, each with the capacity and planned, as a need's crossing, by the
/// last change of her rates before it: her arrival at the television, its end.
/// The television, told the second after, ends at her energy's 150 after 44 263 s (900 at −61),
/// before its 12 h (fun 811 by then, +1 an hour); she is weak then, so that change of rates gives
/// energy −40 more: 150 at −101.
#[test]
fn hunger_at_0_drains_the_reserve_to_weak_and_wasted() {
    let run = run(SATURDAY + WASTED + 1);
    let log = girl(&run);
    let empty = (HUNGER_0, LogPayload::NeedEmpty { need: Need::Hunger });
    assert!(log.contains(&empty), "{log:?}");
    let weakened = (WEAKENED, LogPayload::Weakened { capacity: 1000 });
    let wasted = (WASTED, LogPayload::Wasted { capacity: 1000 });
    assert!(log.contains(&weakened) && log.contains(&wasted), "{log:?}");
    let reserves: Vec<i64> = (log.iter())
        .filter(|(_, payload)| {
            matches!(
                payload,
                LogPayload::Weakened { .. } | LogPayload::Wasted { .. }
            )
        })
        .map(|&(at, _)| at)
        .collect();
    assert_eq!(reserves, [WEAKENED, WASTED]);
    // Planned by the last change of rates before each: the arrival at the television, its end.
    let of = |after: i64, payload: &LogPayload| {
        (run.log.iter())
            .find(|entry| {
                entry.t.as_secs() == SATURDAY + after
                    && entry.subjects == [Subject::Sim(sim(3))]
                    && entry.payload == *payload
            })
            .expect("the entry")
            .id
    };
    let reached = of(TV_ARRIVE, &LogPayload::Reached { object: TV });
    let stopped = LogPayload::ActionEnded {
        object: TV,
        offer: 0,
        reason: EndReason::Critical,
        needs: [0, 150, 500, 700, 811],
    };
    let ended = of(TV_STOP, &stopped);
    for (after, by) in [(WEAKENED, reached), (WASTED, ended)] {
        let entry = reserve_entry(&run, after);
        let [Cause::Plan(plan)] = entry.causes.as_slice() else {
            panic!("not planned: {:?}", entry.causes);
        };
        assert_eq!(number(&plan.by), number(&by));
    }
    // The television ends at energy's 150; the change gives energy its weak extra.
    assert!(log.contains(&(TV_STOP, stopped)), "{log:?}");
    let (energy, reserve) = girl_now(&run);
    assert_eq!(energy, NeedSample::new(150, t(SATURDAY + TV_STOP), -101));
    assert_eq!(
        reserve,
        Reserve::new(1000, (1000, t(SATURDAY + HUNGER_0), -80), -40)
    );
    assert_eq!(reserve.value_at(t(SATURDAY + WEAKENED - 1)), 501);
    assert_eq!(reserve.value_at(t(SATURDAY + TV_STOP)), 65);
    assert_eq!(reserve.value_at(t(SATURDAY + WASTED)), 0);
}

/// Weak at the television, a command for anything that does not feed is refused `weak` (the
/// bookshelf, the television she watches); one for the fridge passes that and meets the woman's
/// snack, `slot_busy`. A second before her reserve reaches half the bookshelf is still allowed.
#[test]
fn weak_a_command_only_feeds() {
    let mut before = run(SATURDAY + WEAKENED - 2);
    assert!(before.world.submit(command(3, BOOK)).is_ok());
    let mut weak = run(SATURDAY + 30_000);
    assert_eq!(
        rejected(&mut weak.world, command(3, BOOK)),
        Some(RejectCode::Weak)
    );
    assert_eq!(
        rejected(&mut weak.world, command(3, TV)),
        Some(RejectCode::Weak)
    );
    assert_eq!(
        rejected(&mut weak.world, command(3, FRIDGE)),
        Some(RejectCode::SlotBusy)
    );
    // Lying on the floor wasted, `collapsed` comes first.
    let mut lying = run(SATURDAY + COLLAPSE + 10);
    assert_eq!(
        rejected(&mut lying.world, command(3, BOOK)),
        Some(RejectCode::Collapsed)
    );
}

/// Wasted, she waits for the fridge where the television left her, in the living room, until
/// energy reaches 0 after 49 610 s (150 at −101): she lies there, energy +50 an hour from 0 with no
/// extra, the reserve at the asleep drain, at 0. Up at 250 after 18 000 s, in her night: energy
/// −61 and the wasted −80, 141 an hour; the night takes the bed, twice its 7 s through the hall;
/// asleep +80 and −80 hold her energy at 250 until 6:00, then −141 again. The woman's second snack
/// ends at 86 411: the girl takes the fridge, twice the 4 s from the bedroom;
/// eating ends the band, so energy is at −61 again and the reserve rises 960 an hour, 320 in 20
/// minutes.
#[test]
fn wasted_she_falls_rises_to_minus_141_and_eats_at_last() {
    let run = run(SATURDAY + FRIDGE_ARRIVE + 1_200);
    let log = girl(&run);
    assert!(
        log.contains(&(COLLAPSE, LogPayload::Collapsed { room: LIVING })),
        "{log:?}"
    );
    assert_eq!(walked(&run, STAND_UP), (BED, LIVING, 14));
    assert_eq!(walked(&run, WOMAN_SECOND_END), (FRIDGE, BEDROOM, 8));
    let (energy, reserve) = girl_now(&run);
    assert_eq!(
        energy,
        NeedSample::new(250, t(SATURDAY + FRIDGE_ARRIVE), -61)
    );
    assert_eq!(reserve.rate(), 960);
    assert_eq!(reserve.energy(), 0);
    assert_eq!(reserve.value(), 0);
    assert_eq!(reserve.at(), t(SATURDAY + FRIDGE_ARRIVE));
    assert_eq!(reserve.value_at(t(SATURDAY + FRIDGE_ARRIVE + 1_200)), 320);
    // Each stage, from the state at its second.
    let stage = |after: i64| girl_now(&self::run(SATURDAY + after));
    let (lying, lying_reserve) = stage(COLLAPSE);
    assert_eq!(lying, NeedSample::new(0, t(SATURDAY + COLLAPSE), 50));
    assert_eq!(
        lying_reserve,
        Reserve::new(1000, (0, t(SATURDAY + COLLAPSE), -32), 0)
    );
    let (up, _) = stage(STAND_UP);
    assert_eq!(up, NeedSample::new(250, t(SATURDAY + STAND_UP), -141));
    let (asleep, asleep_reserve) = stage(BED_ARRIVE);
    assert_eq!(asleep, NeedSample::new(250, t(SATURDAY + BED_ARRIVE), 0));
    assert_eq!(asleep_reserve.energy(), -80);
    let (dawn, _) = stage(DAWN);
    assert_eq!(dawn, NeedSample::new(250, t(SATURDAY + DAWN), -141));
}

/// The band the house answer and the page show follows from the state: drawing on the reserve
/// above half, weak from half, wasted at 0, none once she eats.
#[test]
fn the_band_follows_the_reserve() {
    let band = |after: i64| {
        let run = run(SATURDAY + after);
        let person = &run.world.state().house().sims[&sim(3)];
        let at = t(SATURDAY + after);
        person.reserve.band(person.needs[0], at)
    };
    assert_eq!(band(HUNGER_0 - 1), None);
    assert_eq!(band(HUNGER_0), Some(Band::Draws));
    assert_eq!(band(WEAKENED - 1), Some(Band::Draws));
    assert_eq!(band(WEAKENED), Some(Band::Weak));
    assert_eq!(band(WASTED), Some(Band::Wasted));
    assert_eq!(band(FRIDGE_ARRIVE), None);
}

/// Saved at each stage, the world loads, saves to the same bytes and runs on as the uninterrupted
/// one with the inputs still to come, with the same entries and the same state hash: mid drain,
/// at the half's second, weak at the television and just after it stops, wasted while waiting,
/// lying, up and on the doubled walk to the bed, asleep, and on the doubled walk to the fridge.
#[test]
fn saves_at_each_stage_run_on_the_same() {
    let until = SATURDAY + FRIDGE_ARRIVE + 600;
    let whole = run(until);
    let stages = [
        HUNGER_0,
        HUNGER_0 + 1,
        WEAKENED,
        30_000,
        TV_STOP,
        WOMAN_FIRST_END - 1,
        WASTED,
        COLLAPSE,
        STAND_UP + 3,
        BED_ARRIVE + 60,
        WOMAN_SECOND_END + 4,
        FRIDGE_ARRIVE,
    ];
    for after in stages {
        let at = SATURDAY + after;
        let run = run(at);
        let bytes = run.world.save(&run.out.year_hashes).expect("saves");
        let (mut loaded, _) = World::load(&bytes, data()).expect("loads");
        assert_eq!(
            loaded.save(&run.out.year_hashes).expect("saves"),
            bytes,
            "{after}"
        );
        let mut out = YearOut::default();
        for (clock, input) in inputs().into_iter().filter(|(clock, _)| *clock >= at) {
            loaded.advance_until(&mut out, t(clock), &mut |_| false);
            loaded.submit(input).expect("accepted");
        }
        loaded.advance_until(&mut out, t(until), &mut |_| false);
        assert_eq!(loaded.state_hash(), whole.world.state_hash(), "{after}");
        let entries = |log: &[LogEntry]| serde_json::to_value(log).expect("json");
        assert_eq!(
            entries(&loaded.take_log()),
            entries(&whole.log[run.log.len()..]),
            "{after}"
        );
    }
}

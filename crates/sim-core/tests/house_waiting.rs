//! Taken places, waiting and holds (ADR-0026 section 5): who finds the best object fully taken
//! and nothing free bringing enough waits for it; when a place of it frees, it is held for the
//! first waiter, who chooses in that second and takes it, or passes it on. The couple of
//! `common::day` in a test house with a pantry of one place; the values by hand. The points are
//! those without the mood (ADR-0026, addendum of 2026-11-04 on the mood); the moods of these
//! runs turn no choice.

mod common;

use common::day::{self, MORNING, Run};
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::log::{Cause, EndReason, LogEntry, LogPayload};
use sim_core::time::{HOUR, SimTime};
use sim_core::world::{World, YearOut};

/// A television (fun +1 200 for 2 h) and a bed, two places each; two pantries of one place:
/// one fills hunger at +6 000 an hour until full (at most 10 min), the other gives +720 an
/// hour for 10 min.
const KINDS: &str = r#"
  {"id": "test:tv", "slots": 2, "offers": [
    {"id": "watch", "rates": {"fun": 1200}, "seconds": 7200, "min_age": 0, "max_age": 130}]},
  {"id": "test:bed", "slots": 2, "offers": [
    {"id": "sleep", "sleep": true, "min_age": 0, "max_age": 130}]},
  {"id": "test:pantry", "slots": 1, "offers": [
    {"id": "eat", "rates": {"hunger": 6000}, "until_full": "hunger", "max_seconds": 600, "min_age": 0, "max_age": 130}]},
  {"id": "test:larder", "slots": 1, "offers": [
    {"id": "nibble", "rates": {"hunger": 720}, "seconds": 600, "min_age": 0, "max_age": 130}]}"#;

/// Rooms hall 0, kitchen 1, den 2, bedroom 3, each 3 s from the hall. Objects: the pantry
/// (`PANTRY`) 0 in the kitchen, television 1 in the den, bed 2 in the bedroom, the five
/// essential ones 3 to 7 in the hall.
const HOUSE: &str = r#"{
  "id": "test:queue", "capacity": 2,
  "rooms": [{"id": "hall", "entrance": true}, {"id": "kitchen"}, {"id": "den"}, {"id": "bedroom"}],
  "child_day_room": "den", "child_night_room": "bedroom",
  "doors": [
    {"a": "hall", "b": "kitchen", "seconds": 3},
    {"a": "hall", "b": "den", "seconds": 3},
    {"a": "hall", "b": "bedroom", "seconds": 3}],
  "objects": [{"kind": "PANTRY", "room": "kitchen"}, {"kind": "test:tv", "room": "den"},
    {"kind": "test:bed", "room": "bedroom"}, ESSENTIALS]
}"#;

/// The pantry that fills until full; hunger falls 1 200 an hour, fun 600, energy stays.
fn filling() -> Data {
    let house = HOUSE.replace("PANTRY", "test:pantry");
    day::house_data(&day::rates(-1200, 0, -600), KINDS, &house)
}

/// The pantry of 10 minutes; hunger falls 1 000 an hour, fun 598, so that fun reaches 700 in
/// the second the pantry frees (⌈3 600 · 100 / 598⌉ = 603 = 3 s walk + 600 s).
fn larder() -> Data {
    let house = HOUSE.replace("PANTRY", "test:larder");
    day::house_data(&day::rates(-1000, 0, -598), KINDS, &house)
}

fn started(
    object: u16,
    slot: u8,
    room: u8,
    walk: u32,
    needs: [u16; 5],
    busy: &[u16],
) -> LogPayload {
    LogPayload::ActionStarted {
        house: DataId::new("test:queue").expect("id"),
        object,
        offer: 0,
        slot,
        room,
        walk_secs: walk,
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

const MORNING_NEEDS: [u16; 5] = [600, 900, 500, 700, 800];

/// He waits for the pantry while she is at it (at 6:00 the pantry has 400 · 400 − 50 · 3 =
/// 159 850 for both, the television only 200 · 200 − 150 = 39 850). She is there after 3 s with
/// hunger 599 (600 − 1 truncated), full after 241 s (+401.7 truncated): at 244 s the place is
/// held for him and she, from the kitchen, finds the pantry taken and takes the television (fun
/// 760: 240 · 240 − 50 · 6). His planned choice in that second (hunger 519, 600 − 81.3) takes
/// the held place (481 · 481 − 150 beats 57 450); there at 247 s with hunger 518, full after
/// 290 s (+483.3).
#[test]
fn a_waiter_gets_the_place_that_frees() {
    let (mut run, _) = Run::moved_in(filling());
    run.advance(MORNING + 537);
    let after = [1000, 900, 500, 700, 760];
    assert_eq!(
        run.doings(1)[..5],
        [
            (0, started(0, 0, 0, 3, MORNING_NEEDS, &[])),
            (3, LogPayload::Reached { object: 0 }),
            (244, ended(0, EndReason::Full, after)),
            (244, started(1, 0, 1, 6, after, &[0])),
            (250, LogPayload::Reached { object: 1 }),
        ]
    );
    let idle = LogPayload::Idle {
        house: DataId::new("test:queue").expect("id"),
        room: 0,
        needs: MORNING_NEEDS,
        busy: vec![0],
        waits: Some(0),
    };
    let his = run.doings(2);
    assert_eq!(
        his[..4],
        [
            (0, idle),
            (244, started(0, 0, 0, 3, [519, 900, 500, 700, 760], &[])),
            (247, LogPayload::Reached { object: 0 }),
            (537, ended(0, EndReason::Full, [1000, 900, 500, 700, 711])),
        ]
    );
    // His choice was planned by the end of hers.
    let start = (run.log.iter())
        .filter(|entry| matches!(entry.payload, LogPayload::ActionStarted { .. }))
        .find(|entry| entry.t.as_secs() == MORNING + 244 && entry.subjects == his_subject())
        .expect("his start");
    let [Cause::Plan(plan)] = start.causes.as_slice() else {
        panic!("planned: {:?}", start.causes);
    };
    assert!(matches!(
        run.entry(&plan.by).payload,
        LogPayload::ActionEnded {
            reason: EndReason::Full,
            ..
        }
    ));
    let house = run.world.state().house();
    assert!(house.waits.is_empty() && house.holds.is_empty());
}

/// The larder: at 6:00 it brings 400 · 120 − 150 = 47 850, she takes it, he waits. It frees at
/// 603 s (hunger 600 + 120); held for him, she takes the television from the kitchen (fun 700:
/// 300 · 300 − 300). In the same second his fun reaches 700: that choice comes before the planned
/// one, and the television (89 850) beats the held larder (hunger 433: 567 · 120 − 150), so he
/// passes the place on (nobody else waits, it is free) and the planned choice never runs.
#[test]
fn a_holder_who_chooses_otherwise_passes_the_place_on() {
    let (mut run, _) = Run::moved_in(larder());
    run.advance(MORNING + 606);
    let after = [720, 900, 500, 700, 700];
    assert_eq!(
        run.doings(1)[..4],
        [
            (0, started(0, 0, 0, 3, MORNING_NEEDS, &[])),
            (3, LogPayload::Reached { object: 0 }),
            (603, ended(0, EndReason::Done, after)),
            (603, started(1, 0, 1, 6, after, &[0])),
        ]
    );
    let idle = LogPayload::Idle {
        house: DataId::new("test:queue").expect("id"),
        room: 0,
        needs: MORNING_NEEDS,
        busy: vec![0],
        waits: Some(0),
    };
    assert_eq!(
        run.doings(2),
        [
            (0, idle),
            (603, started(1, 1, 0, 3, [433, 900, 500, 700, 700], &[])),
            (606, LogPayload::Reached { object: 1 }),
        ]
    );
    let house = run.world.state().house();
    assert!(house.waits.is_empty() && house.holds.is_empty());
}

fn his_subject() -> Vec<sim_core::log::Subject> {
    vec![sim_core::log::Subject::Sim(
        sim_core::ids::SimId::try_from(2).expect("id"),
    )]
}

/// Saved while he waits, in the second the place frees and right after: the save loads, saves
/// to the same bytes and runs on as the uninterrupted world until an hour after the morning,
/// with the same entries and causes.
#[test]
fn waiting_is_saved_and_loaded() {
    let filling_seconds: &[i64] = &[0, 1, 243, 244, 245];
    let larder_seconds: &[i64] = &[0, 602, 603, 604];
    for (data, seconds) in [(filling(), filling_seconds), (larder(), larder_seconds)] {
        for &after in seconds {
            let (mut run, _) = Run::moved_in(data.clone());
            run.advance(MORNING + after);
            let bytes = run.world.save(&run.out.year_hashes).expect("saves");
            let (mut loaded, _) = World::load(&bytes, data.clone()).expect("loads");
            assert_eq!(loaded.save(&run.out.year_hashes).expect("saves"), bytes);
            let until = SimTime::from_secs(MORNING + HOUR.as_secs());
            let mut out = YearOut::default();
            loaded.advance_until(&mut out, until, &mut |_| false);
            let saved = run.log.len();
            run.advance(until.as_secs());
            assert_eq!(loaded.state_hash(), run.world.state_hash(), "{after}");
            let entries = |log: &[LogEntry]| serde_json::to_value(log).expect("json");
            assert_eq!(
                entries(&loaded.take_log()),
                entries(&run.log[saved..]),
                "{after}"
            );
        }
    }
}

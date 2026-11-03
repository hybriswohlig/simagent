//! A critical need interrupts what does not raise it (ADR-0026 section 4): at 150 and at 50 the
//! person's log gets `core:need_critical`; a walk or an action whose offer does not raise the
//! need ends with reason `critical`, the rates having worked until then, on the way back in the
//! room it came from, and the person chooses again; what raises the need goes on. The couple of
//! `common::day` in two test houses whose rates make hunger fall fast; the values by hand.

mod common;

use common::day::{self, MORNING, Run};
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::house::data::Need;
use sim_core::log::{EndReason, LogEntry, LogPayload};
use sim_core::time::{HOUR, SimTime};
use sim_core::world::{World, YearOut};

/// A television (fun +1 200 for 2 h), a fridge (hunger +6 000 until full, at most 10 min), a
/// snack (hunger +600 for 2 min) and a bed; two places each.
const KINDS: &str = r#"
  {"id": "test:tv", "slots": 2, "offers": [
    {"id": "watch", "rates": {"fun": 1200}, "seconds": 7200, "min_age": 0, "max_age": 130}]},
  {"id": "test:fridge", "slots": 2, "offers": [
    {"id": "eat", "rates": {"hunger": 6000}, "until_full": "hunger", "max_seconds": 600, "min_age": 0, "max_age": 130}]},
  {"id": "test:snack", "slots": 2, "offers": [
    {"id": "nibble", "rates": {"hunger": 600}, "seconds": 120, "min_age": 0, "max_age": 130}]},
  {"id": "test:bed", "slots": 2, "offers": [
    {"id": "sleep", "sleep": true, "until_full": "energy", "max_seconds": 43200, "min_age": 0, "max_age": 130}]}"#;

/// Rooms hall 0, den 1, kitchen 2, bedroom 3; the den and the bedroom 3 s from the hall, the
/// kitchen 120 s. Objects: television 0 in the den, fridge 1 in the kitchen, bed 2 in the
/// bedroom, the five essential ones 3 to 7 in the hall.
const DEN: &str = r#"{
  "id": "test:den", "capacity": 2,
  "rooms": [{"id": "hall", "entrance": true}, {"id": "den"}, {"id": "kitchen"}, {"id": "bedroom"}],
  "child_day_room": "den", "child_night_room": "bedroom",
  "doors": [
    {"a": "hall", "b": "den", "seconds": 3},
    {"a": "hall", "b": "kitchen", "seconds": 120},
    {"a": "hall", "b": "bedroom", "seconds": 3}],
  "objects": [{"kind": "test:tv", "room": "den"}, {"kind": "test:fridge", "room": "kitchen"},
    {"kind": "test:bed", "room": "bedroom"}, ESSENTIALS]
}"#;

/// Rooms hall 0, a 1, b 2, den 3, kitchen 4; the den 330 s from the hall through a and b, the
/// kitchen 3 s. Objects: television 0 in the den, snack 1 in the kitchen, bed 2 in the den
/// (energy stays at 900 here: 100 · 100 at most, never enough).
const FAR: &str = r#"{
  "id": "test:far", "capacity": 2,
  "rooms": [{"id": "hall", "entrance": true}, {"id": "a"}, {"id": "b"}, {"id": "den"},
    {"id": "kitchen"}],
  "child_day_room": "den", "child_night_room": "den",
  "doors": [
    {"a": "hall", "b": "a", "seconds": 110},
    {"a": "a", "b": "b", "seconds": 110},
    {"a": "b", "b": "den", "seconds": 110},
    {"a": "hall", "b": "kitchen", "seconds": 3}],
  "objects": [{"kind": "test:tv", "room": "den"}, {"kind": "test:snack", "room": "kitchen"},
    {"kind": "test:bed", "room": "den"}, ESSENTIALS]
}"#;

/// Hunger falls 3 000 an hour awake and fun 600; in the den energy falls 600, in the far house
/// it stays.
fn data(house: &str, energy: i32) -> Data {
    day::house_data(&day::rates(-3000, energy, -600), KINDS, house)
}

fn den() -> Data {
    data(DEN, -600)
}

fn far() -> Data {
    data(FAR, 0)
}

fn started(house: &str, object: u16, room: u8, walk_secs: u32, needs: [u16; 5]) -> LogPayload {
    LogPayload::ActionStarted {
        house: DataId::new(house).expect("id"),
        object,
        offer: 0,
        slot: 0,
        room,
        walk_secs,
        needs,
        busy: Vec::new(),
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

fn critical(level: u16) -> LogPayload {
    LogPayload::NeedCritical {
        need: Need::Hunger,
        level,
    }
}

/// His doings are hers in the same seconds, on the second place: he chooses after her.
fn assert_mirrored(run: &Run, count: usize) {
    let hers = run.doings(1);
    let mut his = run.doings(2);
    for (_, payload) in &mut his {
        if let LogPayload::ActionStarted { slot, .. } = payload {
            assert_eq!(*slot, 1);
            *slot = 0;
        }
    }
    assert_eq!(his[..count], hers[..count]);
}

/// The den: at 6:00 the fridge (hunger 600 but 400 to full: 400 · 400, 120 s, 154 000) beats the
/// television (39 850); there at 6:02 with hunger 500, full 300 s later. Then from the kitchen
/// the television (fun 730: 270 · 270 − 50 · 123 = 66 750) beats the bed (22 750); there at
/// 543 s with fun 710 (800 − 90.5 truncated). Hunger, full at 420 s, reaches 150 at 1 440 s
/// while she watches: the television does not raise it, so it ends (fun 1 009, so 1 000; energy
/// 900 − 240) and she chooses the fridge (`10⁹ + 850 · 850 − 50 · 123`). On the way hunger
/// reaches 50 at 1 560 s and nothing ends, the fridge raises it; there at 1 563 s with hunger 48
/// (1 000 − 952.5 truncated), full after 572 s (+953.3 truncated).
#[test]
fn critical_interrupts_the_television_not_the_way_to_the_fridge() {
    let (mut run, _) = Run::moved_in(den());
    run.advance(MORNING + 2_135);
    let den = "test:den";
    let watched = [150, 660, 500, 700, 1000];
    assert_eq!(
        run.doings(1)[..11],
        [
            (0, started(den, 1, 0, 120, [600, 900, 500, 700, 800])),
            (120, LogPayload::Reached { object: 1 }),
            (420, ended(1, EndReason::Full, [1000, 830, 500, 700, 730])),
            (420, started(den, 0, 2, 123, [1000, 830, 500, 700, 730])),
            (543, LogPayload::Reached { object: 0 }),
            (1_440, critical(150)),
            (1_440, ended(0, EndReason::Critical, watched)),
            (1_440, started(den, 1, 1, 123, watched)),
            (1_560, critical(50)),
            (1_563, LogPayload::Reached { object: 1 }),
            (2_135, ended(1, EndReason::Full, [1000, 545, 500, 700, 885])),
        ]
    );
    assert_mirrored(&run, 11);
}

/// The far house: at 6:00 nothing brings enough (the television 200 · 200 − 50 · 330, the snack
/// 400 · 20), she idles in the hall. Hunger 400 at 240 s: the television (fun 760: 240 · 240 −
/// 16 500 = 41 100) beats the snack. On the way hunger reaches 150 at 540 s: the walk ends, she
/// is in the hall again and takes the snack (critical); there at 543 s with hunger 148, 120 s at
/// +600 to 168. From the kitchen the television (fun 690: 310 · 310 − 50 · 333); hunger reaches
/// 150 again 22 s later (18 at −3 000 an hour), the walk ends in the kitchen, the snack again.
#[test]
fn critical_ends_a_walk_where_it_began() {
    let (mut run, _) = Run::moved_in(far());
    run.advance(MORNING + 690);
    let far = "test:far";
    let first = [150, 900, 500, 700, 710];
    let second = [150, 900, 500, 700, 686];
    let idle = LogPayload::Idle {
        house: DataId::new(far).expect("id"),
        room: 0,
        needs: [600, 900, 500, 700, 800],
        busy: Vec::new(),
        waits: None,
    };
    assert_eq!(
        run.doings(1),
        [
            (0, idle),
            (240, started(far, 0, 0, 330, [400, 900, 500, 700, 760])),
            (540, critical(150)),
            (540, ended(0, EndReason::Critical, first)),
            (540, started(far, 1, 0, 3, first)),
            (543, LogPayload::Reached { object: 1 }),
            (663, ended(1, EndReason::Done, [168, 900, 500, 700, 690])),
            (663, started(far, 0, 4, 333, [168, 900, 500, 700, 690])),
            (685, critical(150)),
            (685, ended(0, EndReason::Critical, second)),
            (685, started(far, 1, 4, 5, second)),
            (690, LogPayload::Reached { object: 1 }),
        ]
    );
    assert_mirrored(&run, 12);
}

/// Saved in the seconds a need turns critical and right after: the save loads, saves to the
/// same bytes and runs on as the uninterrupted world until an hour after the morning, with the
/// same entries and causes.
#[test]
fn a_critical_second_is_saved_and_loaded() {
    let den_seconds: &[i64] = &[1_440, 1_441, 1_560, 1_562];
    let far_seconds: &[i64] = &[540, 541, 685, 686];
    for (data, seconds) in [(den(), den_seconds), (far(), far_seconds)] {
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

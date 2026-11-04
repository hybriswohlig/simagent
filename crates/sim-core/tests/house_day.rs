//! A day in the house (ADR-0026 sections 3 and 4): choose, walk, arrive, do it, end, choose
//! again; leaving the house ends what one does. The expected values are the ADR's, by hand,
//! for a woman of 30 (sim 1) and a man of 32 (sim 2) in the repository's flat (rooms hall 0,
//! kitchen 1, living 2, bath 3, bedroom 4, kids 5; the kitchen 4 s from the hall, the bath 3;
//! objects table 0 with four places, toilet 5 with one).

mod common;

use common::day::{MORNING, Run, number};
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::ids::SimId;
use sim_core::input::{Input, MoveOut};
use sim_core::log::{Cause, EndReason, LogEntry, LogPayload, Subject};
use sim_core::time::{DAY, HOUR, SimTime};
use sim_core::world::{World, YearOut};

fn data() -> Data {
    common::death_data(1e-9, None, serde_json::json!([])).with_houses(common::repository_houses())
}

fn flat() -> DataId {
    DataId::new("core:flat").expect("id")
}

fn started(object: u16, slot: u8, room: u8, walk_secs: u32, needs: [u16; 5]) -> LogPayload {
    LogPayload::ActionStarted {
        house: flat(),
        object,
        offer: 0,
        slot,
        room,
        walk_secs,
        needs,
        busy: Vec::new(),
        commanded: false,
    }
}

/// The first morning of the couple. At 6:00 both stand in the hall at the morning values; she
/// chooses first (the smaller SimId) and takes the toilet (249 850 points), he finds it taken
/// and takes the table (159 800). She is there after 3 s with the bladder at 500 and full
/// 150 s later at +12 000 an hour; then from the bath with hunger 596 (600 − 4.25 truncated),
/// energy 899, hygiene 699 and fun 798 the table wins (404 · 404 − 50 · 7), her place 1 next
/// to his. He eats from 6:00:04 for 20 minutes, hunger full, energy 887, bladder 460, hygiene
/// 690, fun 780.
#[test]
fn the_first_morning_of_a_couple() {
    let (mut run, _) = Run::moved_in(data());
    run.advance(MORNING + 1_204);
    let morning = [600, 900, 500, 700, 800];
    let hers = run.doings(1);
    assert_eq!(
        hers[..4],
        [
            (0, started(5, 0, 0, 3, morning)),
            (3, LogPayload::Reached { object: 5 }),
            (
                153,
                LogPayload::ActionEnded {
                    object: 5,
                    offer: 0,
                    reason: EndReason::Full,
                    needs: [596, 899, 1000, 699, 798],
                }
            ),
            (153, started(0, 1, 3, 7, [596, 899, 1000, 699, 798])),
        ]
    );
    let mut his_start = started(0, 0, 0, 4, morning);
    if let LogPayload::ActionStarted { busy, .. } = &mut his_start {
        busy.push(5);
    }
    let his = run.doings(2);
    assert_eq!(
        his[..3],
        [
            (0, his_start),
            (4, LogPayload::Reached { object: 0 }),
            (
                1_204,
                LogPayload::ActionEnded {
                    object: 0,
                    offer: 0,
                    reason: EndReason::Done,
                    needs: [1000, 887, 460, 690, 780],
                }
            ),
        ]
    );
    // The causes: the morning's choice planned by her `core:day_started`, the arrival by the
    // start, the end by the arrival, the next choice in the end's handler.
    let of = |sim: u64, kind: fn(&LogPayload) -> bool| -> Vec<&LogEntry> {
        (run.log.iter())
            .filter(|e| e.subjects == [Subject::Sim(SimId::try_from(sim).expect("id"))])
            .filter(|e| kind(&e.payload))
            .collect()
    };
    let by = |entry: &LogEntry| match entry.causes.as_slice() {
        [Cause::Plan(plan)] => run.entry(&plan.by).payload.clone(),
        [Cause::Event(id)] => run.entry(id).payload.clone(),
        other => panic!("one cause: {other:?}"),
    };
    let starts = of(1, |p| matches!(p, LogPayload::ActionStarted { .. }));
    assert!(matches!(by(starts[0]), LogPayload::DayStarted { .. }));
    assert!(matches!(starts[1].causes.as_slice(), [Cause::Event(_)]));
    assert!(matches!(by(starts[1]), LogPayload::ActionEnded { .. }));
    let reached = of(1, |p| matches!(p, LogPayload::Reached { .. }));
    assert!(matches!(by(reached[0]), LogPayload::ActionStarted { .. }));
    let ended = of(1, |p| matches!(p, LogPayload::ActionEnded { .. }));
    assert!(matches!(by(ended[0]), LogPayload::Reached { .. }));
}

/// Moving out on the way ends both walks (ADR-0026 section 4, reason `left`): after the
/// household's `core:moved_out`, caused by it, with the needs of that second; nothing of the
/// house stays planned.
#[test]
fn moving_out_ends_what_they_do() {
    let (mut run, home) = Run::moved_in(data());
    run.advance(MORNING - 1);
    let before = run.world.pending_events();
    // An input comes in the second after the clock: 6:00:02, both still on the way.
    run.advance(MORNING + 1);
    run.submit(Input::MoveOut(MoveOut { household: home }));
    let out = (run.log.iter())
        .rfind(|entry| matches!(entry.payload, LogPayload::MovedOut { .. }))
        .expect("moved out")
        .id;
    for (sim, object) in [(1, 5), (2, 0)] {
        let last = (run.log.iter())
            .rfind(|e| e.subjects == [Subject::Sim(SimId::try_from(sim).expect("id"))])
            .expect("an entry");
        assert_eq!(
            (last.t.as_secs(), &last.payload),
            (
                MORNING + 2,
                &LogPayload::ActionEnded {
                    object,
                    offer: 0,
                    reason: EndReason::Left,
                    needs: [600, 900, 500, 700, 800],
                }
            )
        );
        assert!(matches!(last.causes.as_slice(), [Cause::Event(id)] if number(id) == number(&out)));
    }
    // The morning has come and gone, the crossings, walks and days are cancelled, with the
    // two series of plans at 5:00 that came with moving in (ADR-0024, addendum of 2026-11-04).
    assert_eq!(run.world.pending_events(), before - 1 - 2);
    run.advance(MORNING + DAY.as_secs());
    assert!(run.world.state().house().is_empty());
}

/// Saved on the way, while both are busy, in the second her action ends and she chooses
/// again, and hours later: the save loads, saves to the same bytes and runs on as the
/// uninterrupted world for three days.
#[test]
fn a_day_is_saved_and_loaded() {
    for after in [2, 100, 153, 5 * HOUR.as_secs()] {
        let (mut run, _) = Run::moved_in(data());
        run.advance(MORNING + after);
        let bytes = run.world.save(&run.out.year_hashes).expect("saves");
        let (mut loaded, _) = World::load(&bytes, data()).expect("loads");
        assert_eq!(loaded.save(&run.out.year_hashes).expect("saves"), bytes);
        let until = SimTime::from_secs(MORNING + 3 * DAY.as_secs());
        let mut out = YearOut::default();
        loaded.advance_until(&mut out, until, &mut |_| false);
        run.advance(until.as_secs());
        assert_eq!(loaded.state_hash(), run.world.state_hash(), "{after}");
    }
}

//! A day in the house (ADR-0026 sections 3 and 4): choose, walk, arrive, do it, end, choose
//! again; leaving the house ends what one does. The expected values are the ADR's, by hand,
//! for a woman of 30 (sim 1) and a man of 32 (sim 2) in the repository's flat (rooms hall 0,
//! kitchen 1, living 2, bath 3, bedroom 4, kids 5; the kitchen 4 s from the hall, the bath 3;
//! objects table 0 with four places, toilet 5 with one). The points are those without the mood
//! (ADR-0026, addendum of 2026-11-04 on the mood); the moods of these runs turn no choice.

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
        held: Vec::new(),
        tier: Vec::new(),
        filler: false,
    }
}

/// The first morning of the couple. At 6:00 both stand in the hall at the morning values; she
/// chooses first (the smaller SimId) and takes the toilet (bladder 500 weighs 200 from its comfort
/// 700: 200 · 500 − 150, at her mood +2 101 850 points), he finds it taken and takes the shower
/// (hygiene 700 weighs 200 from 900: 200 · 300 − 150, at his mood +3 61 650; the table, hunger 600
/// weighing 100, brings 100 · 400 − 200, at +10 43 800). She is there after 3 s with the bladder at
/// 500 and full 150 s later at +12 000 an hour; then from the bath with hunger 596 (600 − 4.25
/// truncated), energy 899, hygiene 699 and fun 798 the best free offer, the table at her mood −4
/// (104 · 404 · 96 / 100 − 350 = 39 985), brings less than 40 000 and the shower, which he holds,
/// more (201 · 301 · 95 / 100 − 250 = 57 225): she waits for it in the bath. He showers from
/// 6:00:03 for 15 minutes, hygiene full, hunger 575, energy 890, bladder 470, fun 785; the shower
/// is held for her, and she takes it at her mood +2 (207 · 307 · 102 / 100 − 250 = 64 569) once he
/// has taken the toilet.
#[test]
fn the_first_morning_of_a_couple() {
    let (mut run, _) = Run::moved_in(data());
    run.advance(MORNING + 1_204);
    let morning = [600, 900, 500, 700, 800];
    let hers = run.doings(1);
    let waits = LogPayload::Idle {
        house: flat(),
        room: 3,
        needs: [596, 899, 1000, 699, 798],
        busy: vec![6],
        waits: Some(6),
        held: Vec::new(),
        tier: Vec::new(),
    };
    let mut shower = started(6, 0, 3, 5, [575, 890, 975, 693, 785]);
    if let LogPayload::ActionStarted { busy, .. } = &mut shower {
        busy.push(5);
    }
    assert_eq!(
        hers[..5],
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
            (153, waits),
            (903, shower),
        ]
    );
    let mut his_start = started(6, 0, 0, 3, morning);
    if let LogPayload::ActionStarted { busy, .. } = &mut his_start {
        busy.push(5);
    }
    let his = run.doings(2);
    assert_eq!(
        his[..3],
        [
            (0, his_start),
            (3, LogPayload::Reached { object: 6 }),
            (
                903,
                LogPayload::ActionEnded {
                    object: 6,
                    offer: 0,
                    reason: EndReason::Done,
                    needs: [575, 890, 470, 1000, 785],
                }
            ),
        ]
    );
    // The causes: the morning's choice planned by her `core:day_started`, the arrival by the
    // start, the end by the arrival; her shower by his end, which held its place for her.
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
    assert!(matches!(starts[1].causes.as_slice(), [Cause::Plan(_)]));
    assert!(matches!(
        by(starts[1]),
        LogPayload::ActionEnded { object: 6, .. }
    ));
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
    for (sim, object) in [(1, 5), (2, 6)] {
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

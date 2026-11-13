//! An avoided partner must not make an idle choice plan another `Decide` in the same second
//! (ADR-0030, addendum of 2026-11-13).
//!
//! The search weighs bonus 0 at `t+` and the worst drag (company 0, avoid in full) at `t−`,
//! then every whole minute between them with that minute's mood and drag. A second it names is
//! enough even with the drag, so the choice cannot idle and schedule itself again. A `Decide`
//! planned before the partner sat can still idle; the same minute with the drag is then not
//! enough, and the search moves on. No second run in that second.
//!
//! The numbers are by hand from ADR-0026 and ADR-0030, not from a run. Hunger starts at 600
//! when the day starts and falls by 100 an hour, so it is 597 after 108 s (`100 · 108 / 3600
//! = 3`). Comfort is 1000, the weight is 403, the offer's gain is 100, the filled part is
//! `403 · 100 = 40 300`. The walk inside the room costs `5 · 50 = 250`, so the points at mood 0
//! are `40 050`. One second earlier the fall is only 2 and the points are `39 950`. That is
//! `t+`. With avoid 10 000 and company 0 the weight must reach 503 (`503 · 100 − 250 − 10 000
//! = 40 050`; 502 gives 39 950), at t = 3 708 s. That is `t−`. The live drag while he sits is
//! `−9 999`. Minute 61 (t = 3 660, weight 501) scores `50 100 − 250 − 9 999 = 39 851`, so no
//! whole minute before `t−` is enough and the meal is 7:01:48. He sits at 6:01:45, before
//! `t+`, and the search moves there at once: she does not idle at 6:01:48.

mod common;

use common::day::{self, MORNING, Run};

const REPOSITORY_DAY: &str = include_str!("../../../data/house/day.json");
use serde_json::{Value, json};
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::house::choice::{ENOUGH, mood};
use sim_core::house::state::{Action, Place};
use sim_core::ids::SimId;
use sim_core::input::{Command, Input};
use sim_core::lineage::LineageKey;
use sim_core::log::LogEntry;
use sim_core::log::{LogPayload, Subject};
use sim_core::social::data::SocialData;
use sim_core::social::feeling::Feel;
use sim_core::time::{HOUR, SimTime, YEAR};
use sim_core::world::{LoadError, World, YearOut};

/// Hunger falls by 100 an hour, so the third point takes 108 s.
const FALL_SECONDS: i64 = 108;
/// `403 · 100 − 250`. Enough at mood 0, and not yet one second earlier (`402 · 100 − 250`).
const PLAIN: i64 = 40_300 - 250;
const ONE_SECOND_EARLIER: i64 = 40_200 - 250;
/// Company 1 minus avoid 10 000. Temper 0 leaves the company at 1.
const NET: i64 = 1 - 10_000;
const WITH_BONUS: i64 = PLAIN + NET;
/// `50 · 7200 / 3600 = 100`, the same gain as a rate of 100 for one hour, so he sits for two.
const GAIN: i64 = 50 * 7_200 / 3_600;
/// Weight 503 with the worst drag, `−10 000`. `503 · 100 − 250 − 10 000 = 40 050`.
const LATER: i64 = 3_708;
const LATER_POINTS: i64 = 503 * 100 - 250 - 10_000;
const ONE_SECOND_BEFORE_LATER: i64 = 502 * 100 - 250 - 10_000;
/// Minute 61. Weight 501 with the live drag `−9 999` is `39 851`, under 40 000.
const MINUTE_BEFORE: i64 = 3_660;
const MINUTE_BEFORE_POINTS: i64 = 501 * 100 - 250 - 9_999;

const _: () = assert!(GAIN == 100);
const _: () = assert!(PLAIN >= ENOUGH && WITH_BONUS < ENOUGH);
const _: () = assert!(ONE_SECOND_EARLIER < ENOUGH);
const _: () = assert!(LATER_POINTS >= ENOUGH && ONE_SECOND_BEFORE_LATER < ENOUGH);
const _: () = assert!(MINUTE_BEFORE_POINTS < ENOUGH);
const _: () = assert!(100 * LATER / 3_600 == 103);
const _: () = assert!(100 * (LATER - 1) / 3_600 == 102);
const _: () = assert!(100 * MINUTE_BEFORE / 3_600 == 101);

fn sim(id: u64) -> SimId {
    SimId::try_from(id).expect("id")
}

/// Weekday mornings, mood fixed at `percent`, no duty: a departure would close the meal.
fn day_at(percent: i64) -> String {
    let mut day: Value = serde_json::from_str(REPOSITORY_DAY).expect("day");
    day["choice_mood_percent"] = json!(percent);
    day["work"]["from_age"] = json!(120);
    day["work"]["until_age"] = json!(130);
    day["night"]["until_weekend"] = day["night"]["until"].clone();
    day.to_string()
}

fn day() -> String {
    day_at(0)
}

fn offer(id: &str, need: &str, rate: i32, seconds: u32) -> Value {
    json!({
        "id": id,
        "rates": { need: rate },
        "seconds": seconds,
        "min_age": 0,
        "max_age": 130
    })
}

fn kinds() -> Value {
    json!([
        {"id": "core:dining_table", "slots": 2, "offers": [offer("eat", "hunger", 50, 7_200)]},
        {"id": "core:fridge", "slots": 1, "offers": [offer("snack", "hunger", 1, 60)]},
        {"id": "core:toilet", "slots": 1, "offers": [offer("use", "bladder", 1, 60)]},
        {"id": "core:shower", "slots": 1, "offers": [offer("shower", "hygiene", 1, 60)]},
        {"id": "core:sofa", "slots": 1, "offers": [offer("sit", "fun", 1, 60)]},
        {"id": "test:bed", "slots": 2, "bed_for": "couples", "offers": [
            {"id": "sleep", "sleep": true, "min_age": 0, "max_age": 130}
        ]}
    ])
}

/// The table is in the living room, which is where an idle person stays. The hall is not.
fn house() -> String {
    json!({
        "id": "test:cascade", "capacity": 2,
        "rooms": [
            {"id": "hall", "entrance": true, "dwell": false},
            {"id": "living"},
            {"id": "bath"},
            {"id": "bedroom"}
        ],
        "child_day_room": "living", "child_night_room": "bedroom", "dwell_room": "living",
        "doors": [
            {"a": "hall", "b": "living", "seconds": 3},
            {"a": "hall", "b": "bath", "seconds": 3},
            {"a": "hall", "b": "bedroom", "seconds": 4}
        ],
        "objects": [
            {"kind": "core:dining_table", "room": "living"},
            {"kind": "core:fridge", "room": "hall"},
            {"kind": "core:toilet", "room": "bath"},
            {"kind": "core:shower", "room": "bath"},
            {"kind": "core:sofa", "room": "living"},
            {"kind": "test:bed", "room": "bedroom"}
        ]
    })
    .to_string()
}

fn social() -> SocialData {
    common::repository_social_with(|social| {
        // Trust under 400, so the seated partner is avoid as well as company.
        social["bonds"]["start"]["partner"][1] = json!(100);
        social["choice"]["company"] = json!(1);
        social["choice"]["avoid"] = json!(10_000);
        // Nobody of 30 talks, and the table stays sociable because a topic still names it.
        social["limits"]["min_age"] = json!(130);
    })
}

fn data() -> Data {
    let houses = common::houses_from(
        &day::rates(-100, 0, 0),
        &day(),
        &kinds().to_string(),
        &[&house()],
    );
    common::death_data(1e-9, None, json!([]))
        .with_houses(houses)
        .with_social(social())
}

fn idles(log: &[sim_core::log::LogEntry], who: u64, at: i64) -> usize {
    log.iter()
        .filter(|entry| {
            entry.t.as_secs() == at
                && entry.subjects == [Subject::Sim(sim(who))]
                && matches!(entry.payload, LogPayload::Idle { .. })
        })
        .count()
}

fn idles_after(log: &[sim_core::log::LogEntry], who: u64, from: i64, until: i64) -> Vec<i64> {
    log.iter()
        .filter(|entry| {
            let t = entry.t.as_secs();
            t > from
                && t <= until
                && entry.subjects == [Subject::Sim(sim(who))]
                && matches!(entry.payload, LogPayload::Idle { .. })
        })
        .map(|entry| entry.t.as_secs())
        .collect()
}

/// He sits at 6:01:45, before the bonus-0 second 6:01:48. The search moves at once to
/// 7:01:48, where the meal is enough even with the worst drag. She does not idle at 6:01:48,
/// so nothing schedules another `Decide` in that second. A save while she waits loads.
#[test]
fn an_idle_choice_does_not_schedule_itself_again() {
    let choose = MORNING + FALL_SECONDS;
    let later = MORNING + LATER;
    assert_eq!(choose, MORNING + 6 * HOUR.as_secs() / 100 * 3 / 6);
    // 108 s after 6:00 is 6:01:48. The command is an input at 6:01:39, so it acts at 6:01:40,
    // and the walk of 5 s inside the room seats him at 6:01:45, before `t+`. The meal lasts
    // 7 200 s from when he sits, so he is still there at 7:01:48.
    let data = data();
    let (mut run, _) = Run::moved_in(data.clone());
    run.advance(MORNING + FALL_SECONDS - 9);
    run.submit(Input::Command(Command {
        sim: sim(2),
        object: 0,
        offer: 0,
    }));
    // He is already seated, so the pending choice is 7:01:48, not the bonus-0 second.
    run.advance(choose - 1);
    let early = run
        .world
        .save(&[])
        .expect("the meal at 7:01:48 is the one pending");
    World::load(&early, data.clone()).expect("a save from the second before 6:01:48 loads");
    run.advance(choose);

    assert_eq!(idles(&run.log, 1, choose), 0, "no idle choice at 6:01:48");
    let her = run.world.state().house().sims.get(&sim(1)).expect("her");
    assert!(
        matches!(her.place, Place::Room(_)),
        "she stays in the living room"
    );
    assert!(her.action.is_none());
    let him = run.world.state().house().sims.get(&sim(2)).expect("him");
    assert!(matches!(him.place, Place::Room(_)));
    assert_eq!(
        him.action,
        Some(Action {
            object: 0,
            offer: 0,
            slot: 0,
            filler: false,
        })
    );

    let bytes = run
        .world
        .save(&[])
        .expect("the pending choice matches the recomputation");
    let (mut loaded, _) = World::load(&bytes, data).expect("loads");

    run.advance(later);
    assert_eq!(
        idles_after(&run.log, 1, choose, later),
        Vec::<i64>::new(),
        "no idle choice each second until the meal is enough with the avoid"
    );
    let company = run
        .log
        .iter()
        .rev()
        .find(|entry| {
            entry.t.as_secs() == later && matches!(entry.payload, LogPayload::Company { .. })
        })
        .expect("the bonus is logged when she takes the meal");
    match &company.payload {
        LogPayload::Company { objects } => {
            assert_eq!(objects.len(), 1);
            let row = &objects[0];
            assert_eq!(
                (row.object, row.liked, row.company, row.avoided, row.avoid),
                (0, Some(sim(2)), 1, Some(sim(2)), 10_000)
            );
        }
        other => panic!("core:company, got {other:?}"),
    }
    let started = run
        .log
        .iter()
        .find(|entry| {
            entry.t.as_secs() == later
                && entry.subjects == [Subject::Sim(sim(1))]
                && matches!(entry.payload, LogPayload::ActionStarted { .. })
        })
        .expect("she takes the meal at 7:01:48");
    match &started.payload {
        LogPayload::ActionStarted {
            object,
            offer,
            filler,
            ..
        } => assert_eq!((*object, *offer, *filler), (0, 0, false)),
        other => panic!("core:action_started, got {other:?}"),
    }

    let mut out = YearOut::default();
    loaded.advance_until(&mut out, SimTime::from_secs(later), &mut |_| false);
    let loaded_log = loaded.take_log();
    assert_eq!(
        idles_after(&loaded_log, 1, choose, later),
        Vec::<i64>::new()
    );
    assert!(loaded_log.iter().any(|entry| {
        entry.t.as_secs() == later
            && entry.subjects == [Subject::Sim(sim(1))]
            && matches!(
                entry.payload,
                LogPayload::ActionStarted {
                    object: 0,
                    offer: 0,
                    filler: false,
                    ..
                }
            )
    }));
}

/// Company 8 000 and avoid 8 000, temper −500 at the morning, returning 1 000 a day.
/// Hunger is the cascade meal. `lost` is the temper that has returned by `elapsed`.
const RETURN: i64 = 1_000;
const TEMPER: i64 = 500;
const DAY_SECS: i64 = 86_400;

fn lost_temper(elapsed: i64) -> i64 {
    RETURN * elapsed / DAY_SECS
}

/// `min(0, company · (1000 − temper_left) / 1000 − avoid)` with both points 8 000.
fn live_drag(elapsed: i64) -> i64 {
    let left = TEMPER - lost_temper(elapsed);
    let scaled = 8_000 * (1_000 - left) / 1_000;
    (scaled - 8_000).min(0)
}

fn weight(elapsed: i64) -> i64 {
    400 + 100 * elapsed / 3_600
}

/// The meal's points at mood 0 with the drag of `elapsed` read at that same second.
fn live_points(elapsed: i64) -> i64 {
    weight(elapsed) * 100 - 250 + live_drag(elapsed)
}

/// The same points with one drag frozen for the whole search.
fn frozen_points(elapsed: i64, drag: i64) -> i64 {
    weight(elapsed) * 100 - 250 + drag
}

fn decay_social() -> SocialData {
    common::repository_social_with(|social| {
        social["bonds"]["start"]["partner"][1] = json!(100);
        social["choice"]["company"] = json!(8_000);
        social["choice"]["avoid"] = json!(8_000);
        social["feelings"]["mood_return"] = json!(RETURN);
        // She is 30, so min_age stays one she can have feelings at. A talk needs both of
        // them seated, and he sits alone until she takes the meal, so the bonds stay put.
    })
}

fn decay_data() -> Data {
    let houses = common::houses_from(
        &day::rates(-100, 0, 0),
        &day(),
        &kinds().to_string(),
        &[&house()],
    );
    common::death_data(1e-9, None, json!([]))
        .with_houses(houses)
        .with_social(decay_social())
}

fn rows_after(log: &[LogEntry], from: i64) -> Vec<(i64, Vec<Subject>, LogPayload)> {
    log.iter()
        .filter(|entry| entry.t.as_secs() > from)
        .map(|entry| {
            (
                entry.t.as_secs(),
                entry.subjects.clone(),
                entry.payload.clone(),
            )
        })
        .collect()
}

/// Temper −500 at the morning, company and avoid both 8 000. He sits at 6:01:45, before
/// `t+` (6:01:48). `t−` with drag −8 000 is weight 483, t = 2 988. The first whole minute
/// that clears 40 000 is t = 1 560 (6:26:00): minute 25 (t = 1 500, weight 441, 17 of the
/// temper back, drag −3 864) scores `44 100 − 250 − 3 864 = 39 986`, and minute 26
/// (weight 443, 18 back, drag −3 856) scores `44 300 − 250 − 3 856 = 40 194`. The exact
/// second 1 512 is already enough and is not a whole minute, so it is not the choice.
/// A save at 6:16:40, while she waits, loads, and the resumed run takes the meal at 6:26:00.
#[test]
fn a_save_while_temper_returns_resumes_the_uninterrupted_run() {
    // 100 · 108 / 3 600 = 3, so the bonus-0 meal is the cascade's 6:01:48.
    const IDLE_AT: i64 = 108;
    /// Minute 25. Weight 441, drag −3 864: 39 986.
    const BEFORE: i64 = 1_500;
    /// Minute 26. Weight 443, drag −3 856: 40 194. The first whole minute that is enough.
    const LIVE: i64 = 1_560;
    /// The exact second a bisection of the live score would return. Weight 442, drag −3 864.
    const EXACT: i64 = 1_512;
    const SAVE_AT: i64 = 1_000;
    assert_eq!(lost_temper(BEFORE), 17);
    assert_eq!(live_drag(BEFORE), -3_864);
    assert_eq!(weight(BEFORE), 441);
    assert!(live_points(BEFORE) < ENOUGH);
    assert_eq!(lost_temper(LIVE), 18);
    assert_eq!(live_drag(LIVE), -3_856);
    assert_eq!(weight(LIVE), 443);
    assert!(live_points(LIVE) >= ENOUGH);
    assert_eq!(lost_temper(EXACT), 17);
    assert!(live_points(EXACT) >= ENOUGH);
    assert!(live_points(EXACT - 1) < ENOUGH);
    // Worst drag −8 000 needs weight 483: 483 · 100 − 250 − 8 000 = 40 050.
    assert_eq!(weight(2_988), 483);
    assert!(frozen_points(2_988, -8_000) >= ENOUGH);
    assert!(frozen_points(2_987, -8_000) < ENOUGH);
    // At the save the open second is one later; 1 000 and 1 001 return the same 11.
    assert_eq!(lost_temper(SAVE_AT), 11);
    assert_eq!(lost_temper(SAVE_AT + 1), 11);
    // He sat at 6:01:45 and the meal lasts 7 200 s, so he is still there at 6:26:00.
    const {
        assert!(105 + 7_200 > LIVE);
    };

    let data = decay_data();
    let (mut run, _) = Run::moved_in(data.clone());
    run.advance(MORNING);
    let morning = SimTime::from_secs(MORNING);
    let feel = Feel {
        channels: [0; 4],
        since: morning,
        temper: -500,
        temper_since: morning,
    };
    run.world = common::with_feelings(&run.world, &run.out.year_hashes, &data, &[(1, feel)]);
    run.advance(MORNING + IDLE_AT - 9);
    run.submit(Input::Command(Command {
        sim: sim(2),
        object: 0,
        offer: 0,
    }));
    let choose = MORNING + IDLE_AT;
    run.advance(choose);
    assert_eq!(
        idles(&run.log, 1, choose),
        0,
        "he sat before 6:01:48, so that choice never runs"
    );

    let save_at = MORNING + SAVE_AT;
    run.advance(save_at);
    let bytes = run
        .world
        .save(&[])
        .expect("the wait under a returning temper saves");
    let (mut loaded, _) = World::load(&bytes, data).expect("a save during the wait loads");

    let live = MORNING + LIVE;
    run.advance(live);
    assert_eq!(
        idles_after(&run.log, 1, choose, live),
        Vec::<i64>::new(),
        "she does not choose again until the live score is enough"
    );
    assert!(
        run.log.iter().any(|entry| {
            entry.t.as_secs() == live
                && entry.subjects == [Subject::Sim(sim(1))]
                && matches!(
                    entry.payload,
                    LogPayload::ActionStarted {
                        object: 0,
                        offer: 0,
                        filler: false,
                        ..
                    }
                )
        }),
        "she takes the meal at 6:26:00, the first whole minute the live score is enough"
    );
    assert!(
        !run.log.iter().any(|entry| {
            entry.t.as_secs() == MORNING + EXACT
                && matches!(entry.payload, LogPayload::ActionStarted { .. })
        }),
        "the exact second 6:25:12 is enough and is not a whole minute"
    );

    let mut out = YearOut::default();
    loaded.advance_until(&mut out, SimTime::from_secs(live), &mut |_| false);
    let loaded_log = loaded.take_log();
    assert_eq!(
        rows_after(&loaded_log, save_at),
        rows_after(&run.log, save_at)
    );
    assert_eq!(
        sim_core::log::log_hash(&loaded_log),
        sim_core::log::log_hash(
            &run.log
                .iter()
                .filter(|entry| entry.t.as_secs() > save_at)
                .cloned()
                .collect::<Vec<_>>()
        )
    );
    assert_eq!(loaded.canonical_bytes(), run.world.canonical_bytes());
}

/// The postcard of a positive second: the same bytes `SimTime` writes.
fn time_bytes(secs: i64) -> Vec<u8> {
    postcard::to_allocvec(&secs).expect("a second encodes")
}

/// `bytes` with the one pending time `from` written as `to`. Both encode in the same
/// number of bytes, the queue stays in order, and the event's handle is untouched.
fn retimed(bytes: &[u8], from: i64, to: i64) -> Vec<u8> {
    let old = time_bytes(from);
    let new = time_bytes(to);
    assert_eq!(
        old.len(),
        new.len(),
        "both seconds encode in the same width"
    );
    let mut at = Vec::new();
    let last = bytes.len().saturating_sub(old.len());
    for i in 0..=last {
        let boundary = i == 0 || bytes[i - 1] & 0x80 == 0;
        let micro = bytes.get(i + old.len()).copied() == Some(0);
        if boundary && micro && bytes[i..].starts_with(&old) {
            at.push(i);
        }
    }
    assert_eq!(at.len(), 1, "the dragged second is one pending time");
    let mut out = bytes.to_vec();
    out[at[0]..at[0] + new.len()].copy_from_slice(&new);
    out
}

/// After he sits, the pending choice is 7:01:48, already before 6:01:48. Moving it to
/// 6:01:49 keeps the handle and the queue order. The loader accepts one recomputed second
/// and refuses this one.
#[test]
fn a_decide_pulled_back_to_the_open_second_is_refused() {
    let choose = MORNING + FALL_SECONDS;
    let later = MORNING + LATER;
    let data = data();
    let (mut run, _) = Run::moved_in(data.clone());
    run.advance(MORNING + FALL_SECONDS - 9);
    run.submit(Input::Command(Command {
        sim: sim(2),
        object: 0,
        offer: 0,
    }));
    run.advance(choose);
    assert_eq!(idles(&run.log, 1, choose), 0, "no idle choice at 6:01:48");
    let bytes = run
        .world
        .save(&[])
        .expect("the meal at 7:01:48 is the one pending");
    World::load(&bytes, data.clone()).expect("the honest save loads");

    let tampered = retimed(&bytes, later, choose + 1);
    let error = match World::load(&tampered, data) {
        Err(error) => error,
        Ok(_) => panic!("the open second is not the queued choice"),
    };
    assert_eq!(error, LoadError::Inconsistent);
}

fn window_social() -> SocialData {
    common::repository_social_with(|social| {
        social["bonds"]["start"]["partner"][1] = json!(100);
        // 625 at move-in is 600 at 06:00. See the test for the elapsed time.
        social["bonds"]["start"]["partner"][4] = json!(625);
        social["bonds"]["familiarity_fade"] = json!(100);
        social["choice"]["company"] = json!(8_000);
        social["choice"]["avoid"] = json!(10_000);
        social["choice"]["company_min_sum"] = json!(1_300);
        social["limits"]["min_age"] = json!(130);
    })
}

fn window_data() -> Data {
    let houses = common::houses_from(
        &day::rates(-100, 0, 0),
        &day(),
        &kinds().to_string(),
        &[&house()],
    );
    common::death_data(1e-9, None, json!([]))
        .with_houses(houses)
        .with_social(window_social())
}

/// Company 8 000 and avoid 10 000, the sum of affection and familiarity crossing 1 300 as
/// familiarity fades. Enough at t = 828 with drag −2 000, not at t = 863 once company is
/// gone, enough again at 3 708 with drag −10 000. A bisection of that score returns 3 708
/// and skips the open window. The first whole minute inside it is t = 840.
#[test]
fn a_fading_familiarity_does_not_skip_the_open_window() {
    // Bonds form at move-in, 23:59:59 (86 399), not at 06:00. Elapsed at 06:00 is
    // 108 000 − 86 399 = 21 601. Fade 100 loses `100 · 21 601 / 86 400 = 25` (remainder
    // 100), so familiarity 625 is 600 there. Affection 700, sum 1 300, company on.
    // The next point falls when elapsed reaches 22 464 (`100 · 22 464 = 26 · 86 400`),
    // which is t = 863. Minute 14 (t = 840) is before that.
    const ELAPSED: i64 = 108_000 - 86_399;
    const DROP: i64 = 22_464 - ELAPSED;
    const MINUTE: i64 = 840;
    const EARLIER: i64 = 780;
    assert_eq!(ELAPSED, 21_601);
    assert_eq!(100 * ELAPSED / 86_400, 25);
    assert_eq!(100 * 22_464, 26 * 86_400);
    assert_eq!(DROP, 863);
    const _: () = assert!(MINUTE < DROP && EARLIER < DROP);
    // Drag −2 000 while company holds. Weight 423 at t = 840 and at the exact second 828.
    // Weight 421 at t = 780. Weight 422 at t = 827.
    assert_eq!(weight(MINUTE), 423);
    assert_eq!(weight(828), 423);
    assert_eq!(weight(827), 422);
    assert_eq!(weight(EARLIER), 421);
    assert_eq!(weight(MINUTE) * 100 - 250 - 2_000, 40_050);
    assert_eq!(weight(827) * 100 - 250 - 2_000, 39_950);
    assert_eq!(weight(EARLIER) * 100 - 250 - 2_000, 39_850);
    // With company gone the drag is −10 000, and weight 503 at t = 3 708 clears it.
    // That is the second a bisection returns, past the window.
    // Weight 423 at t = 863: `42 300 − 250 − 10 000 = 32 050`.
    assert_eq!(weight(DROP), 423);
    assert_eq!(weight(DROP) * 100 - 250 - 10_000, 32_050);
    const _: () = assert!(LATER_POINTS >= ENOUGH);
    // He sits at 6:01:45. The meal lasts 7 200 s, past 6:14:00.
    const {
        assert!(105 + 7_200 > MINUTE);
    };

    let data = window_data();
    let (mut run, _) = Run::moved_in(data.clone());
    run.advance(MORNING + FALL_SECONDS - 9);
    run.submit(Input::Command(Command {
        sim: sim(2),
        object: 0,
        offer: 0,
    }));
    let meal = MORNING + MINUTE;
    let save_at = MORNING + 400;
    run.advance(save_at);
    assert_eq!(idles(&run.log, 1, MORNING + FALL_SECONDS), 0);
    let bytes = run
        .world
        .save(&[])
        .expect("the wait inside the open window saves");
    let (mut loaded, _) = World::load(&bytes, data).expect("a save during the window loads");

    run.advance(meal);
    assert!(
        run.log.iter().any(|entry| {
            entry.t.as_secs() == meal
                && entry.subjects == [Subject::Sim(sim(1))]
                && matches!(
                    entry.payload,
                    LogPayload::ActionStarted {
                        object: 0,
                        offer: 0,
                        filler: false,
                        ..
                    }
                )
        }),
        "she takes the meal at 6:14:00, the first whole minute of the open window"
    );
    let company = run
        .log
        .iter()
        .find(|entry| {
            entry.t.as_secs() == meal && matches!(entry.payload, LogPayload::Company { .. })
        })
        .expect("company is logged with the meal");
    match &company.payload {
        LogPayload::Company { objects } => {
            assert_eq!(objects.len(), 1);
            let row = &objects[0];
            assert_eq!(
                (row.object, row.liked, row.company, row.avoided, row.avoid),
                (0, Some(sim(2)), 8_000, Some(sim(2)), 10_000)
            );
        }
        other => panic!("core:company, got {other:?}"),
    }

    let mut out = YearOut::default();
    loaded.advance_until(&mut out, SimTime::from_secs(meal), &mut |_| false);
    let loaded_log = loaded.take_log();
    assert_eq!(
        rows_after(&loaded_log, save_at),
        rows_after(&run.log, save_at)
    );
    assert_eq!(loaded.canonical_bytes(), run.world.canonical_bytes());
}

fn mood_data() -> Data {
    let houses = common::houses_from(
        &day::rates(-100, 0, 0),
        &day_at(10),
        &kinds().to_string(),
        &[&house()],
    );
    common::death_data(1e-9, None, json!([]))
        .with_houses(houses)
        .with_social(social())
}

/// Seed 4, mood ±10. At 06:00 she is in the hall and the meal is not enough (mood −8,
/// walk of 3 s: `400 · 92 − 150 = 36 650`). She reaches the living room at 06:00:03 and
/// the bonus-0 minute is 06:01:00 (mood +5, `401 · 105 − 250 = 41 855`). He sits at
/// 06:00:50, before that minute, and the search moves to minute 49. She does not idle
/// at 06:01:00. A save at 06:01:59 is not on a minute. The loader used to keep a second
/// accepted time for "the choice already ran" and, off a minute, name the bonus-0
/// minute instead of the queued one. Minute 2 is mood −9, `403 · 91 − 250 = 36 423`.
/// Minute 49 is mood +6, weight 481, `481 · 106 − 250 − 9 999 = 40 737`.
#[test]
fn a_save_off_a_minute_reloads_the_queued_choice() {
    let tag = DataId::new("test:house").expect("valid");
    let key = LineageKey::founder(&tag, 0);
    let mood_at = |t: i64| i64::from(mood(4, key, SimTime::from_secs(MORNING + t), (0, 0), 10));
    assert_eq!(
        (mood_at(0), mood_at(60), mood_at(120), mood_at(2_940)),
        (-8, 5, -9, 6)
    );
    let weighed = |t: i64, drag: i64| weight(t) * (100 + mood_at(t)) - 250 + drag;
    const HALL: i64 = 36_650;
    const MINUTE_TWO: i64 = 36_423;
    assert_eq!(weight(0) * 92 - 150, HALL);
    const _: () = assert!(HALL < ENOUGH);
    assert_eq!(weighed(60, 0), 41_855);
    assert_eq!(weighed(120, 0), MINUTE_TWO);
    const _: () = assert!(MINUTE_TWO < ENOUGH);
    assert_eq!(weight(2_940), 481);
    assert_eq!(weighed(2_940, NET), 40_737);
    assert!(weighed(2_940, NET) >= ENOUGH);
    for minute in 1..49 {
        let t = minute * 60;
        assert!(
            weighed(t, NET) < ENOUGH,
            "minute {minute} already clears 40 000"
        );
    }
    // He sits at 06:00:50. The meal lasts 7 200 s, past 06:49:00.
    const SEAT: i64 = 50;
    const MEAL_AT: i64 = 49 * 60;
    const _: () = assert!(SEAT + 7_200 > MEAL_AT);

    let data = mood_data();
    let (mut run, _) = Run::family(
        data.clone(),
        4,
        "test:house",
        &[-30 * YEAR.as_secs(), -32 * YEAR.as_secs()],
        0,
    );
    // Closed at 06:00:44. The command acts at 06:00:45 and the walk of 5 s seats him at
    // 06:00:50, before her bonus-0 minute. Her sim id is smaller than his, so a same-second
    // arrival would be ordered after her `Decide`.
    run.advance(MORNING + 44);
    let him = run.world.state().house().sims.get(&sim(2)).expect("him");
    assert!(him.action.is_none(), "he is still idle at 06:00:44");
    assert!(
        matches!(him.place, Place::Room(_)),
        "he is in the living room"
    );
    run.submit(Input::Command(Command {
        sim: sim(2),
        object: 0,
        offer: 0,
    }));
    let idle_at = MORNING + 60;
    run.advance(idle_at);
    assert_eq!(
        idles(&run.log, 1, idle_at),
        0,
        "he sat before 06:01:00, so that choice never runs"
    );

    let save_at = MORNING + 119;
    run.advance(save_at);
    let bytes = run.world.save(&[]).expect("saved at 06:01:59");
    let (mut loaded, _) = World::load(&bytes, data).expect("a save at 06:01:59 loads");

    let meal = MORNING + 49 * 60;
    run.advance(meal);
    assert_eq!(
        idles_after(&run.log, 1, MORNING, meal),
        Vec::<i64>::new(),
        "she does not choose again until minute 49"
    );
    assert!(
        run.log.iter().any(|entry| {
            entry.t.as_secs() == meal
                && entry.subjects == [Subject::Sim(sim(1))]
                && matches!(
                    entry.payload,
                    LogPayload::ActionStarted {
                        object: 0,
                        offer: 0,
                        filler: false,
                        ..
                    }
                )
        }),
        "she takes the meal at 06:49:00"
    );
    assert!(
        !run.log.iter().any(|entry| {
            entry.t.as_secs() == MORNING + 120
                && matches!(entry.payload, LogPayload::ActionStarted { .. })
        }),
        "06:02:00 is not enough"
    );

    let mut out = YearOut::default();
    loaded.advance_until(&mut out, SimTime::from_secs(meal), &mut |_| false);
    let loaded_log = loaded.take_log();
    assert_eq!(
        rows_after(&loaded_log, save_at),
        rows_after(&run.log, save_at)
    );
    assert_eq!(loaded.canonical_bytes(), run.world.canonical_bytes());
}

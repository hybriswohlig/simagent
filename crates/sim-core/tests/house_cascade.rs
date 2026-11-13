//! An idle choice must not plan another `Decide` in the same second.
//!
//! Company and avoid are applied when the choice runs and are not part of the search for a
//! later second (ADR-0030 section 4, ADR-0032 section 5). The search can therefore say "this
//! second" while the choice, with the bonus, idles. `decide` then marks the household changed,
//! and `plan_next` would schedule another `Decide` at that same second. That event idles the
//! same way. The chain is only `Decide` for that one person: each one is the next micro step
//! of the same second, and the 65th trips `scheduler.cascade_limit`. An `ActionEnd` enters the
//! same chain when it calls `decide` inline and the person stays idle.
//!
//! Pushing the next search to the following second does not end it. While the avoided person
//! sits, every later second is again "this second" for the bonus-0 search, so the person
//! chooses once a second and trips `house.decision_budget` after 200 choices. The next `Decide`
//! is the first second the offer is enough even with the negative net.
//!
//! The numbers are by hand from ADR-0026 and ADR-0030, not from a run. Hunger starts at 600
//! when the day starts and falls by 100 an hour, so it is 597 after 108 s (`100 · 108 / 3600
//! = 3`). Comfort is 1000, the weight is 403, the offer's gain is 100, the filled part is
//! `403 · 100 = 40 300`. The walk inside the room costs `5 · 50 = 250`, so the points at mood 0
//! are `40 050`. One second earlier the fall is only 2 and the points are `39 950`, under
//! 40 000, so the planned second is exactly then. The partner sits at the table: affection 700
//! and familiarity 600 are company, trust 100 is avoid. Company is 1 and avoid is 10 000, temper
//! is 0, and the net is `−9 999`. The choice scores `30 051` and idles. The same net needs
//! weight 503 (`503 · 100 − 250 − 9 999 = 40 051`; 502 gives 39 951). Weight
//! `400 + ⌊100 · t / 3600⌋` reaches 503 at t = 3 708 s (7:01:48), before bedtime and while he
//! is still seated. The mood is fixed at 0, so that second is weighed as itself.

mod common;

use common::day::{self, MORNING, Run};

const REPOSITORY_DAY: &str = include_str!("../../../data/house/day.json");
use serde_json::{Value, json};
use sim_core::data::Data;
use sim_core::house::choice::ENOUGH;
use sim_core::house::state::{Action, Place};
use sim_core::ids::SimId;
use sim_core::input::{Command, Input};
use sim_core::log::LogEntry;
use sim_core::log::{LogPayload, Subject};
use sim_core::social::data::SocialData;
use sim_core::social::feeling::Feel;
use sim_core::time::{HOUR, SimTime};
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
/// Weight 503. `503 · 100 − 250 − 9 999 = 40 051`, and 502 is 39 951.
const LATER: i64 = 3_708;
const LATER_POINTS: i64 = 503 * 100 - 250 - 9_999;
const ONE_SECOND_BEFORE_LATER: i64 = 502 * 100 - 250 - 9_999;

const _: () = assert!(GAIN == 100);
const _: () = assert!(PLAIN >= ENOUGH && WITH_BONUS < ENOUGH);
const _: () = assert!(ONE_SECOND_EARLIER < ENOUGH);
const _: () = assert!(LATER_POINTS >= ENOUGH && ONE_SECOND_BEFORE_LATER < ENOUGH);
const _: () = assert!(100 * LATER / 3_600 == 103);
const _: () = assert!(100 * (LATER - 1) / 3_600 == 102);

fn sim(id: u64) -> SimId {
    SimId::try_from(id).expect("id")
}

/// Weekday mornings, mood fixed at 0, no duty: a departure would close the meal.
fn day() -> String {
    let mut day: Value = serde_json::from_str(REPOSITORY_DAY).expect("day");
    day["choice_mood_percent"] = json!(0);
    day["work"]["from_age"] = json!(120);
    day["work"]["until_age"] = json!(130);
    day["night"]["until_weekend"] = day["night"]["until"].clone();
    day.to_string()
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

/// She is due to choose at 6:01:48, while he has been seated at the table since 6:01:45.
/// The meal is enough without him and not enough with the avoid, so she idles once. The next
/// choice is 7:01:48, when the meal is enough even with the avoid, and a save of the idle
/// second loads.
#[test]
fn an_idle_choice_does_not_schedule_itself_again() {
    let choose = MORNING + FALL_SECONDS;
    let later = MORNING + LATER;
    assert_eq!(choose, MORNING + 6 * HOUR.as_secs() / 100 * 3 / 6);
    // 108 s after 6:00 is 6:01:48. The command is an input at 6:01:39, so it acts at 6:01:40,
    // and the walk of 5 s inside the room seats him at 6:01:45, before her choice. The meal
    // lasts 7 200 s from when he sits, so he is still there at 7:01:48.
    let data = data();
    let (mut run, _) = Run::moved_in(data.clone());
    run.advance(MORNING + FALL_SECONDS - 9);
    run.submit(Input::Command(Command {
        sim: sim(2),
        object: 0,
        offer: 0,
    }));
    // The second before she chooses, the pending `Decide` is still the bonus-0 second.
    run.advance(choose - 1);
    let early = run
        .world
        .save(&[])
        .expect("the bonus-0 choice is still the one pending");
    World::load(&early, data.clone()).expect("a save from the second before loads");
    run.advance(choose);

    assert_eq!(idles(&run.log, 1, choose), 1, "one idle choice at 6:01:48");
    let company = run
        .log
        .iter()
        .rev()
        .find(|entry| {
            entry.t.as_secs() == choose && matches!(entry.payload, LogPayload::Company { .. })
        })
        .expect("the bonus is logged");
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

/// Temper −500 at the morning has returned 1 by 6:01:48, so the drag is −3 992 and a
/// search that freezes it waits until 7:01:48 (weight 443). The temper keeps returning,
/// and the meal is already enough at 6:25:12 (weight 442, drag −3 864). A save at
/// 6:16:40, while she waits, must load: the loader used to freeze the drag at the open
/// second (temper −489, drag −3 912, 6:25:12) and refuse the queued 7:01:48. The resumed
/// run is the uninterrupted one, and she takes the meal at the live second.
#[test]
fn a_save_while_temper_returns_resumes_the_uninterrupted_run() {
    // 100 · 108 / 3 600 = 3, so the bonus-0 meal is the cascade's 6:01:48.
    const IDLE_AT: i64 = 108;
    /// Weight 442 is the first that clears 40 000 with the drag read at that second.
    const LIVE: i64 = 1_512;
    /// Weight 443 is the first that clears 40 000 with the drag frozen at 6:01:48.
    const FROZEN: i64 = 1_548;
    const SAVE_AT: i64 = 1_000;
    let idle_drag = live_drag(IDLE_AT);
    assert_eq!(lost_temper(IDLE_AT), 1);
    assert_eq!(idle_drag, -3_992);
    assert!(frozen_points(FROZEN, idle_drag) >= ENOUGH);
    assert!(frozen_points(FROZEN - 1, idle_drag) < ENOUGH);
    assert_eq!(lost_temper(LIVE), 17);
    assert_eq!(live_drag(LIVE), -3_864);
    assert!(live_points(LIVE) >= ENOUGH);
    assert!(live_points(LIVE - 1) < ENOUGH);
    // At the save the open second is one later; 1 000 and 1 001 return the same 11.
    assert_eq!(lost_temper(SAVE_AT), 11);
    assert_eq!(lost_temper(SAVE_AT + 1), 11);
    assert_eq!(live_drag(SAVE_AT), -3_912);
    assert!(frozen_points(LIVE, live_drag(SAVE_AT)) >= ENOUGH);
    assert!(frozen_points(LIVE - 1, live_drag(SAVE_AT)) < ENOUGH);
    // He sat at 6:01:45 and the meal lasts 7 200 s, so he is still there at 6:25:12.
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
    assert_eq!(idles(&run.log, 1, choose), 1, "one idle choice at 6:01:48");

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
        "she takes the meal at 6:25:12, the first second the live score is enough"
    );
    assert!(
        !run.log.iter().any(|entry| {
            entry.t.as_secs() == MORNING + FROZEN
                && matches!(entry.payload, LogPayload::ActionStarted { .. })
        }),
        "the drag frozen at 6:01:48 would have waited until 7:01:48"
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

/// After 6:01:48 the pending choice is 7:01:48. Moving it to 6:01:49, the open second,
/// keeps the handle and the queue order, and used to load: `accepts` still allowed the
/// bonus-0 time. That run then chooses again, writes entries and spends a decision the
/// uninterrupted run does not. The save is refused.
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
    assert_eq!(idles(&run.log, 1, choose), 1);
    let bytes = run
        .world
        .save(&[])
        .expect("the dragged choice is the one pending");
    World::load(&bytes, data.clone()).expect("the honest save loads");

    let tampered = retimed(&bytes, later, choose + 1);
    let error = match World::load(&tampered, data) {
        Err(error) => error,
        Ok(_) => panic!("the open second is not the queued choice"),
    };
    assert_eq!(error, LoadError::Inconsistent);
}

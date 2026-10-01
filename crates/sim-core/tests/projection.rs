//! Canonical projection, state hash, year hashes and log hash (ADR-0003 section 5,
//! ADR-0002 "Speichern, Laden, Hash", ADR-0004 section 8).
//!
//! The pinned bytes and hashes come from `tools/state_reference.py` (postcard wire format
//! by hand, xxh3-64 from the C reference library), not from a run of this code.
//! Scenario "tiny": seed 0xff, one founder born one second before the start.

use sim_core::canonical::{YearHash, canonical_bytes};
use sim_core::data_id::DataId;
use sim_core::ids::{EventId, SimId};
use sim_core::input::{Founder, Input, RejectCode, Start};
use sim_core::log::{LogEntry, LogHasher, PlanRef, log_hash};
use sim_core::time::{Scheduler, SchedulerSnapshot, SimTime, SnapshotEntry};
use sim_core::world::{EventPayload, World, YearOut};

const Y: i64 = 31_449_600;

// tools/state_reference.py
const TINY_AFTER_START: &str = "0101ff01010202fe87ff1d0002010100fe87ff1d000100018088ff1d0301010100";
const TINY_AFTER_START_HASH: u64 = 0xf364_d2e5_df1e_1ba5;
const TINY_YEAR_0_HASH: u64 = 0x6710_517d_804b_1d20;
const TINY_AFTER_YEAR_0: &str =
    "0101ff01fe87ff1d0202fe8ffe3b0003010100fe8ffe3b000100018088ff1d0401010100";
const TINY_AFTER_YEAR_0_HASH: u64 = 0xa8c7_1a38_1baa_01df;
const LOG_HASH_EMPTY: u64 = 0x2d06_8005_38d3_94c2;
const LOG_HASH_ENTRY: u64 = 0xdb98_2fa2_37ed_9ecc;
const LOG_HASH_ENTRY_TWICE: u64 = 0x7e76_9d92_d1d4_9c69;

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex digits"))
        .collect()
}

fn start(seed: u64, born: &[i64]) -> Input {
    Input::Start(Start {
        world_seed: seed,
        founder_tag: DataId::new("test:projection").expect("valid"),
        founders: born
            .iter()
            .map(|&secs| Founder {
                born: SimTime::from_secs(secs),
            })
            .collect(),
    })
}

fn started(seed: u64, born: &[i64]) -> World {
    let mut world = World::new();
    world.submit(start(seed, born)).expect("accepted");
    world
}

fn tiny() -> World {
    started(0xff, &[-1])
}

fn t(secs: i64) -> SimTime {
    SimTime::from_secs(secs)
}

fn sim(n: u64) -> SimId {
    SimId::try_from(n).expect("valid sim id")
}

fn plan(by: u64, planned_at: i64) -> PlanRef {
    let by: EventId = serde_json::from_value(by.into()).expect("valid event id");
    PlanRef {
        by,
        planned_at: t(planned_at),
    }
}

#[test]
fn canonical_bytes_match_the_reference() {
    let mut world = tiny();
    assert_eq!(world.canonical_bytes(), hex(TINY_AFTER_START));
    assert_eq!(world.state_hash(), TINY_AFTER_START_HASH);
    let mut out = YearOut::default();
    world.advance_until(&mut out, t(Y - 1), &mut |_| false);
    assert_eq!(world.canonical_bytes(), hex(TINY_AFTER_YEAR_0));
    assert_eq!(world.state_hash(), TINY_AFTER_YEAR_0_HASH);
}

/// ADR-0003 5.4, ADR-0002: the year hash of year k is taken by the year end observer in
/// the last second of the year, when the clock still stands one second earlier and
/// every domain event of the second has run.
#[test]
fn the_year_hash_is_what_the_observer_sees() {
    let mut world = tiny();
    let mut out = YearOut::default();
    world.advance_until(&mut out, t(Y - 1), &mut |_| false);
    assert_eq!(
        out.year_hashes,
        [YearHash {
            year: 0,
            hash: TINY_YEAR_0_HASH,
        }]
    );
    assert_ne!(TINY_YEAR_0_HASH, world.state_hash());
    assert_eq!(
        serde_json::to_string(&out.year_hashes[0]).expect("json"),
        r#"{"year":0,"hash":"6710517d804b1d20"}"#
    );
    assert!(
        serde_json::from_str::<YearHash>(r#"{"year":0,"hash":"6710517d804b1d20","x":1}"#).is_err()
    );
}

/// ADR-0003: A and B are planned, B is cancelled: the projection contains only A.
#[test]
fn cancelled_events_not_in_projection() {
    let world = tiny();
    let a = EventPayload::Birthday { sim: sim(1) };
    let b = EventPayload::Birthday { sim: sim(2) };
    let mut both: Scheduler<EventPayload, PlanRef> = Scheduler::new();
    let mut ctx = both.input_ctx();
    ctx.schedule(t(10), a.clone(), plan(1, 0));
    let handle = ctx.schedule(t(20), b.clone(), plan(1, 0));
    let with_b = canonical_bytes(world.state(), &both);
    assert!(both.input_ctx().cancel(handle));
    let cancelled = canonical_bytes(world.state(), &both);
    assert_ne!(cancelled, with_b);
    // The same queue built without B: only the next insertion number remembers it.
    let only_a = Scheduler::restore(SchedulerSnapshot {
        closed_through: t(-1),
        next_seq: 3,
        entries: vec![SnapshotEntry {
            time: t(10),
            micro: 0,
            seq: 1,
            payload: a,
            period: None,
            meta: plan(1, 0),
        }],
    })
    .expect("valid snapshot");
    assert_eq!(cancelled, canonical_bytes(world.state(), &only_a));
    assert_eq!(cancelled.len() + 6, with_b.len(), "B took six bytes");
}

/// ADR-0003: the same queue with other companion data has the same hash.
#[test]
fn projection_ignores_plan_refs() {
    let world = tiny();
    let queue = |meta: PlanRef| {
        let mut sched: Scheduler<EventPayload, PlanRef> = Scheduler::new();
        let mut ctx = sched.input_ctx();
        ctx.schedule(t(10), EventPayload::Birthday { sim: sim(1) }, meta);
        ctx.schedule_periodic(t(20), sim_core::time::YEAR, EventPayload::YearEnd, meta);
        canonical_bytes(world.state(), &sched)
    };
    assert_eq!(queue(plan(1, 0)), queue(plan(777, -5)));
}

/// ADR-0002 `state_hash_independent_of_step_width`: 10 years in yearly, daily and
/// 7 919 s steps, with a halt after every second with events and in one piece: the same
/// year hashes and the same final hash.
#[test]
fn state_hash_independent_of_step_width() {
    let born = [-943_488_000, -794_880_000, -1];
    let end = 10 * Y - 1;
    let run = |width: i64, stop: bool| {
        let mut world = started(1, &born);
        let mut out = YearOut::default();
        while world.closed_through().as_secs() < end {
            let limit = (world.closed_through().as_secs() + width).min(end);
            world.advance_until(&mut out, t(limit), &mut |_| stop);
        }
        (out.year_hashes, world.state_hash())
    };
    let (years, last) = run(end + 1, false);
    assert_eq!(years.len(), 10);
    for (index, year) in years.iter().enumerate() {
        assert_eq!(year.year, i64::try_from(index).expect("small"));
    }
    let mut distinct: Vec<u64> = years.iter().map(|year| year.hash).collect();
    distinct.push(last);
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(
        distinct.len(),
        11,
        "every year and the end hash differently"
    );
    for (width, stop) in [(Y, false), (86_400, false), (7_919, false), (end + 1, true)] {
        assert_eq!(
            run(width, stop),
            (years.clone(), last),
            "width {width}, stop {stop}"
        );
    }
}

/// Lever test: every input that reaches the state changes the hash.
#[test]
fn the_state_hash_follows_the_state() {
    let base = started(1, &[-5, -1]).state_hash();
    assert_eq!(
        base,
        started(1, &[-5, -1]).state_hash(),
        "same input, same hash"
    );
    assert_ne!(base, started(2, &[-5, -1]).state_hash(), "seed");
    assert_ne!(base, started(1, &[-6, -1]).state_hash(), "a birth time");
    assert_ne!(
        base,
        started(1, &[-5, -1, -1]).state_hash(),
        "one more founder"
    );
    assert_ne!(base, started(1, &[-1, -5]).state_hash(), "founder order");
    assert_ne!(base, World::new().state_hash(), "not started");
    let mut advanced = started(1, &[-5, -1]);
    advanced.advance_until(&mut YearOut::default(), t(0), &mut |_| false);
    assert_ne!(base, advanced.state_hash(), "the clock");
}

/// ADR-0004 test 8 with the hash: a rejected input leaves the state hash unchanged.
#[test]
fn rejected_input_keeps_the_state_hash() {
    let mut world = tiny();
    let before = (world.state_hash(), world.canonical_bytes());
    let rejected = world.submit(start(9, &[-3])).expect_err("second start");
    assert_eq!(rejected.code, RejectCode::AlreadyStarted);
    assert_eq!((world.state_hash(), world.canonical_bytes()), before);
    let mut fresh = World::new();
    let before = fresh.state_hash();
    fresh.submit(start(9, &[0])).expect_err("founder not born");
    assert_eq!(fresh.state_hash(), before);
}

/// The log is no state: taking it does not change the hash.
#[test]
fn taking_the_log_keeps_the_state_hash() {
    let mut world = tiny();
    let before = world.state_hash();
    assert_eq!(world.take_log().len(), 2);
    assert_eq!(world.state_hash(), before);
}

/// ADR-0004 section 8: the log hash runs over the postcard bytes of all entries in
/// order, however they are fed.
#[test]
fn log_hash_known_answers() {
    let entry: LogEntry = serde_json::from_str(
        r#"{"id":7,"t":86400,"subjects":[{"sim":42}],"causes":[{"plan":{"by":3,"planned_at":0}},{"rule":"core:gompertz_hazard"},{"event":5},{"input":1}],"payload":{"core:birthday":{"age_years":1}}}"#,
    )
    .expect("parses");
    assert_eq!(log_hash(&[]), LOG_HASH_EMPTY);
    assert_eq!(log_hash(core::slice::from_ref(&entry)), LOG_HASH_ENTRY);
    assert_eq!(
        log_hash(&[entry.clone(), entry.clone()]),
        LOG_HASH_ENTRY_TWICE
    );
    let mut hasher = LogHasher::new();
    assert_eq!(hasher.digest(), LOG_HASH_EMPTY);
    hasher.update(core::slice::from_ref(&entry));
    assert_eq!(hasher.digest(), LOG_HASH_ENTRY);
    hasher.update(&[]);
    hasher.update(core::slice::from_ref(&entry));
    assert_eq!(hasher.digest(), LOG_HASH_ENTRY_TWICE);
}

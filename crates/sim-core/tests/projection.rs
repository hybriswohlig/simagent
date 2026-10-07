//! Canonical projection, state hash, year hashes and log hash (ADR-0003 section 5,
//! ADR-0002 "Speichern, Laden, Hash", ADR-0004 section 8).
//!
//! The pinned bytes and hashes come from `tools/state_reference.py` (postcard wire format
//! by hand, xxh3-64 from the C reference library), not from a run of this code.
//! Scenario "tiny": seed 0xff, one founder born one second before the start.

mod common;

use sim_core::canonical::{YearHash, canonical_bytes};
use sim_core::data_id::DataId;
use sim_core::finite::FiniteF64;
use sim_core::genome::{Allele, Genotype, Sex};
use sim_core::ids::{EventId, SimId};
use sim_core::input::{Fertility, ForkOverride, Founder, Input, RejectCode, Start};
use sim_core::log::{LogEntry, LogHasher, PlanRef, log_hash};
use sim_core::params::{ParamChange, WorldParams};
use sim_core::time::{Scheduler, SchedulerSnapshot, SimTime, SnapshotEntry};
use sim_core::world::{EventPayload, World, YearOut};

/// A fertility under which nobody ever conceives.
fn barren() -> Fertility {
    Fertility {
        per_month: FiniteF64::try_new(0.0).expect("finite"),
        from_years: 18,
        until_years: 45,
        pregnancy_months: 10,
    }
}

const Y: i64 = 31_449_600;

// tools/state_reference.py
const TINY_AFTER_START: &str = "0801ff01010000010000000000000000122d0a01020102fe87ff1d0002010100fe87ff1d000100018088ff1d030101019893ebf7f7b1d2ec5e000001000000000000";
const PAIR_AFTER_START: &str = "0801ff01010000010000000000000000122d0a01030103fc87ff1d0002010100fe87ff1d0003010200fe87ff1d000100018088ff1d040201038cf68c8181f39989b9010000010000000201aca1aef0de91d89d86010101010100000000000000";
const TINY_OVERRIDDEN: &str = "0801ff01010100010000000000000000122d0a01020102fe87ff1d0002010100fe87ff1d000100018088ff1d030101019893ebf7f7b1d2ec5e000001000000000000";
const TINY_OVERRIDE_ENTRY: &str = "030000010202030001";
const TINY_AFTER_START_HASH: u64 = 0xd969_5958_a0c9_f683;
const TINY_YEAR_0_HASH: u64 = 0x812e_417f_fb3e_56f8;
const TINY_AFTER_YEAR_0: &str = "0801ff01010000010000000000000000122d0afe87ff1d020102fe8ffe3b0003010100fe8ffe3b000100018088ff1d040101019893ebf7f7b1d2ec5e000001000000000000";
const TINY_AFTER_YEAR_0_HASH: u64 = 0xb45b_4758_1a88_e579;
const LOG_HASH_EMPTY: u64 = 0x2d06_8005_38d3_94c2;
const LOG_HASH_ENTRY: u64 = 0xdb98_2fa2_37ed_9ecc;
const LOG_HASH_ENTRY_TWICE: u64 = 0x7e76_9d92_d1d4_9c69;

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex digits"))
        .collect()
}

/// A founder born at `secs`: female, with the alleles wild and dummy.
fn founder(secs: i64) -> Founder {
    Founder {
        born: SimTime::from_secs(secs),
        sex: Sex::Female,
        genotype: Genotype {
            maternal: Allele::Wild,
            paternal: Allele::Dummy,
        },
    }
}

fn start(seed: u64, born: &[i64]) -> Input {
    Input::Start(Start {
        world_seed: seed,
        founder_tag: DataId::new("test:tiny").expect("valid"),
        world_params: WorldParams {
            hereditary_diseases: false,
        },
        death_causes: Vec::new(),
        founders: born.iter().copied().map(founder).collect(),
        couples: Vec::new(),
        fertility: barren(),
    })
}

fn started(seed: u64, born: &[i64]) -> World {
    let mut world = World::new(common::data());
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

/// Scenario "pair" of the reference tool: a woman born two seconds before the start with
/// the alleles wild and dummy, a man born one second before it with dummy twice. The
/// projection keeps for each what `Start` gave: the sex, both alleles and the lineage
/// key of the place in the list.
#[test]
fn the_projection_keeps_sex_genotype_and_key_of_each_founder() {
    let mut world = World::new(common::data());
    let dummy_twice = Genotype {
        maternal: Allele::Dummy,
        paternal: Allele::Dummy,
    };
    world
        .submit(Input::Start(Start {
            world_seed: 0xff,
            founder_tag: DataId::new("test:pair").expect("valid"),
            world_params: WorldParams {
                hereditary_diseases: false,
            },
            death_causes: Vec::new(),
            founders: vec![
                founder(-2),
                Founder {
                    born: SimTime::from_secs(-1),
                    sex: Sex::Male,
                    genotype: dummy_twice,
                },
            ],
            couples: Vec::new(),
            fertility: barren(),
        }))
        .expect("accepted");
    assert_eq!(world.canonical_bytes(), hex(PAIR_AFTER_START));
    assert_eq!(world.state_hash(), 0x392f_80ef_9e49_cb3d);
}

/// "tiny" with an override block right after `Start` (tools/state_reference.py): the
/// projection carries the world parameters as they stand after it, and nothing else
/// moves. The block writes one entry with the input as its cause.
#[test]
fn an_override_changes_the_world_parameters_in_the_projection() {
    let mut world = tiny();
    world.take_log();
    let block = Input::ForkOverride(ForkOverride {
        changes: vec![ParamChange::HereditaryDiseases(true)],
    });
    world.submit(block).expect("accepted");
    assert_eq!(world.canonical_bytes(), hex(TINY_OVERRIDDEN));
    assert_eq!(world.state_hash(), 0x8c8e_02cf_e079_9aa5);
    let log = world.take_log();
    assert_eq!(log.len(), 1);
    assert_eq!(
        postcard::to_allocvec(&log[0]).expect("encodes"),
        hex(TINY_OVERRIDE_ENTRY)
    );
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
        r#"{"year":0,"hash":"812e417ffb3e56f8"}"#
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
    assert_ne!(base, World::new(common::data()).state_hash(), "not started");
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
    let mut fresh = World::new(common::data());
    let before = fresh.state_hash();
    fresh.submit(start(9, &[0])).expect_err("founder not born");
    assert_eq!(fresh.state_hash(), before);
    // The EventId counter is not in the hash: a rejected input must not use an ID
    // either. Start wrote entries 1 and 2, so the first birthday is entry 3.
    assert_eq!(world.take_log().len(), 2);
    world.advance_until(&mut YearOut::default(), t(Y - 1), &mut |_| false);
    let log = world.take_log();
    assert_eq!(log.len(), 1);
    assert_eq!(serde_json::to_string(&log[0].id).expect("json"), "3");
    // And a rejected first input leaves the counters at their start.
    fresh.submit(start(9, &[-1])).expect("accepted");
    assert_eq!(fresh.canonical_bytes(), started(9, &[-1]).canonical_bytes());
    let ids: Vec<String> = fresh
        .take_log()
        .iter()
        .map(|entry| serde_json::to_string(&entry.id).expect("json"))
        .collect();
    assert_eq!(ids, ["1", "2"]);
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

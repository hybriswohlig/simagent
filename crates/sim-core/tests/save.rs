//! Saving and loading (ADR-0006, ADR-0003 acceptance tests, ADR-0004 test 4).
//!
//! Reference bytes come from `tools/state_reference.py`. Broken saves are built with an
//! encoder of this test (`File`), written from ADR-0006 and independent of the core's
//! types; it must produce the reference bytes for the valid case.

use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use serde::Serialize;
use sim_core::canonical::YearHash;
use sim_core::data_id::DataId;
use sim_core::input::{Founder, Input, Start};
use sim_core::log::{LogEntry, log_hash};
use sim_core::time::SimTime;
use sim_core::version::{CORE_VERSION, EMPTY_DATA_HASH};
use sim_core::world::{FORMAT_VERSION, LoadError, SaveError, World, YearOut};

const Y: i64 = 31_449_600;
const DAY: i64 = 86_400;

// tools/state_reference.py
const HEADER: &str = "53494d4147454e540101c2a9cec6d380a0832d";
const TINY_SAVE_AFTER_START: &str = "53494d4147454e540101c2a9cec6d380a0832d0101ff01010202fe87ff1d0002010100fe87ff1d000100018088ff1d030101010002020001000301010100ff0109746573743a74696e79010100";
const EMPTY_SAVE: &str = "53494d4147454e540101c2a9cec6d380a0832d010001010001000000010000";

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex digits"))
        .collect()
}

fn t(secs: i64) -> SimTime {
    SimTime::from_secs(secs)
}

fn start(seed: u64, tag: &str, born: &[i64]) -> Input {
    Input::Start(Start {
        world_seed: seed,
        founder_tag: DataId::new(tag).expect("valid"),
        founders: born.iter().map(|&secs| Founder { born: t(secs) }).collect(),
    })
}

/// Scenario "tiny" of the reference tool, with the log taken.
fn tiny() -> World {
    let mut world = World::new();
    world
        .submit(start(0xff, "test:tiny", &[-1]))
        .expect("accepted");
    world.take_log();
    world
}

fn three_founders() -> World {
    let mut world = World::new();
    let born = [-943_488_000, -794_880_000, -1];
    world
        .submit(start(1, "test:save", &born))
        .expect("accepted");
    world
}

fn lines(log: &[LogEntry]) -> Vec<String> {
    log.iter()
        .map(|entry| serde_json::to_string(entry).expect("json"))
        .collect()
}

fn load(bytes: &[u8]) -> Result<(World, Vec<YearHash>), LoadError> {
    World::load(bytes, EMPTY_DATA_HASH)
}

fn load_error(bytes: &[u8]) -> Option<LoadError> {
    load(bytes).err()
}

// ---------------------------------------------------------------------------------
// An encoder for saves, from ADR-0006 section 2 and the postcard rules: a struct is its
// fields in order, a map is a sequence of key and value, an enum is its variant index.

#[derive(Clone, Serialize)]
enum Payload {
    YearEnd,
    Birthday { sim: u64 },
}

#[derive(Clone, Serialize)]
enum TestInput {
    Start {
        world_seed: u64,
        founder_tag: String,
        founders: Vec<i64>,
    },
}

/// (time, micro, seq, payload, period)
type QueueEntry = (i64, u16, u64, Payload, Option<i64>);

#[derive(Clone, Serialize)]
struct File {
    // Header.
    magic: [u8; 8],
    format_version: u32,
    core_version: u32,
    data_hash: u64,
    // Projection (ADR-0003 5.2).
    projection_version: u32,
    world_seed: Option<u64>,
    closed_through: i64,
    next_sim_id: u64,
    queue: Vec<QueueEntry>,
    next_event_seq: u64,
    /// (id, born); a list of pairs encodes like a map and can hold duplicates.
    sims: Vec<(u64, i64)>,
    /// (id, (born, left))
    archive: Vec<(u64, (i64, i64))>,
    // Exceptions (ADR-0006 section 2).
    /// (by, planned_at), one per queue entry.
    plan_refs: Vec<(u64, i64)>,
    next_event_id: u64,
    /// (t, seq, input)
    input_log: Vec<(i64, u64, TestInput)>,
    /// (year, hash)
    year_hashes: Vec<(i64, u64)>,
}

impl File {
    /// "tiny" right after `Start`.
    fn tiny() -> File {
        File {
            magic: *b"SIMAGENT",
            format_version: 1,
            core_version: 1,
            data_hash: EMPTY_DATA_HASH,
            projection_version: 1,
            world_seed: Some(0xff),
            closed_through: -1,
            next_sim_id: 2,
            queue: vec![
                (Y - 1, 0, 2, Payload::Birthday { sim: 1 }, None),
                (Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
            ],
            next_event_seq: 3,
            sims: vec![(1, -1)],
            archive: Vec::new(),
            plan_refs: vec![(2, 0), (1, 0)],
            next_event_id: 3,
            input_log: vec![(
                -1,
                1,
                TestInput::Start {
                    world_seed: 0xff,
                    founder_tag: "test:tiny".to_owned(),
                    founders: vec![-1],
                },
            )],
            year_hashes: Vec::new(),
        }
    }

    fn bytes(&self) -> Vec<u8> {
        postcard::to_allocvec(self).expect("encodes")
    }

    fn with(mut self, change: impl FnOnce(&mut File)) -> Vec<u8> {
        change(&mut self);
        self.bytes()
    }
}

#[test]
fn the_test_encoder_matches_the_reference() {
    assert_eq!(File::tiny().bytes(), hex(TINY_SAVE_AFTER_START));
    assert_eq!(FORMAT_VERSION, 1);
    assert_eq!(CORE_VERSION, 1);
}

// ---------------------------------------------------------------------------------

/// ADR-0006 test 2: header by hand, then exactly the canonical bytes, then the
/// exceptions.
#[test]
fn saved_projection_is_the_canonical_bytes() {
    let world = tiny();
    let bytes = world.save(&[]).expect("saves");
    assert_eq!(bytes, hex(TINY_SAVE_AFTER_START));
    let header = hex(HEADER);
    assert_eq!(bytes[..header.len()], header[..]);
    let projection = world.canonical_bytes();
    assert_eq!(bytes[header.len()..][..projection.len()], projection[..]);
    assert_eq!(World::new().save(&[]).expect("saves"), hex(EMPTY_SAVE));
}

/// ADR-0003 `save_load_save_is_byte_identical`.
#[test]
fn save_load_save_is_byte_identical() {
    // Never started.
    let bytes = World::new().save(&[]).expect("saves");
    let (loaded, years) = load(&bytes).expect("loads");
    assert_eq!(loaded.save(&years).expect("saves"), bytes);
    // Right after Start.
    let bytes = tiny().save(&[]).expect("saves");
    let (loaded, years) = load(&bytes).expect("loads");
    assert!(years.is_empty());
    assert_eq!(loaded.save(&years).expect("saves"), bytes);
    // In the middle of year 3, with three year hashes.
    let mut world = three_founders();
    let mut out = YearOut::default();
    world.advance_until(&mut out, t(3 * Y + 100 * DAY), &mut |_| false);
    world.take_log();
    assert_eq!(out.year_hashes.len(), 3);
    let bytes = world.save(&out.year_hashes).expect("saves");
    let (loaded, years) = load(&bytes).expect("loads");
    assert_eq!(years, out.year_hashes);
    assert_eq!(loaded.save(&years).expect("saves"), bytes);
    assert_eq!(loaded.canonical_bytes(), world.canonical_bytes());
    assert_eq!(loaded.state_hash(), world.state_hash());
    assert_eq!(loaded.closed_through(), world.closed_through());
    assert_eq!(loaded.input_log(), world.input_log());
    assert_eq!(loaded.pending_events(), world.pending_events());
}

/// What a run produces, for comparing an interrupted run with an uninterrupted one.
#[derive(Debug, PartialEq)]
struct Run {
    year_hashes: Vec<YearHash>,
    census_rows: usize,
    end_hash: u64,
    log_hash: u64,
    log: Vec<String>,
}

fn finish(mut world: World, mut log: Vec<LogEntry>, mut out: YearOut) -> Run {
    world.advance_until(&mut out, t(10 * Y - 1), &mut |_| false);
    log.extend(world.take_log());
    Run {
        year_hashes: out.year_hashes,
        census_rows: out.census.len(),
        end_hash: world.state_hash(),
        log_hash: log_hash(&log),
        log: lines(&log),
    }
}

/// ADR-0004 test 4 (core part; the fresh process follows with the CLI): saving in the
/// middle of a year or right after an input, loading and running on gives the same
/// year hashes, the same end hash and the same log with gapless EventIds.
#[test]
fn save_load_continue_equals_uninterrupted() {
    let mut world = three_founders();
    let log = world.take_log();
    let uninterrupted = finish(world, log, YearOut::default());
    assert_eq!(uninterrupted.year_hashes.len(), 10);
    assert_eq!(uninterrupted.log.len(), 34);

    // Limits: right after the input, in the middle of a year, in the last second of a
    // year, in the first second of a year.
    for limit in [None, Some(3 * Y + 100 * DAY), Some(4 * Y - 1), Some(4 * Y)] {
        let mut world = three_founders();
        let mut out = YearOut::default();
        if let Some(limit) = limit {
            world.advance_until(&mut out, t(limit), &mut |_| false);
        }
        let log = world.take_log();
        let bytes = world.save(&out.year_hashes).expect("saves");
        drop(world);
        let (loaded, years) = load(&bytes).expect("loads");
        // The census is output of the first part; the year hashes travel with the save.
        let resumed = YearOut {
            census: out.census,
            year_hashes: years,
        };
        assert_eq!(
            finish(loaded, log, resumed),
            uninterrupted,
            "limit {limit:?}"
        );
    }
}

/// ADR-0006 test 4, ADR-0004 section 6: saving needs an empty log buffer.
#[test]
fn save_with_pending_log_is_rejected() {
    let mut world = World::new();
    world
        .submit(start(0xff, "test:tiny", &[-1]))
        .expect("accepted");
    let before = world.state_hash();
    assert_eq!(world.save(&[]), Err(SaveError::LogNotTaken));
    assert_eq!(world.state_hash(), before);
    assert_eq!(world.take_log().len(), 2, "the entries are still there");
    assert_eq!(world.save(&[]).expect("saves"), hex(TINY_SAVE_AFTER_START));
}

/// A loaded world is a world: it rejects a second `Start` and counts on.
#[test]
fn a_loaded_world_keeps_its_counters() {
    let bytes = tiny().save(&[]).expect("saves");
    let (mut loaded, _) = load(&bytes).expect("loads");
    assert!(loaded.submit(start(1, "test:tiny", &[-1])).is_err());
    assert_eq!(loaded.state().sim_ids_allocated(), 1);
    assert_eq!(loaded.stats().contexts, 0, "run stats count from the load");
    let mut out = YearOut::default();
    loaded.advance_until(&mut out, t(Y - 1), &mut |_| false);
    let log = loaded.take_log();
    assert_eq!(
        lines(&log),
        [
            r#"{"id":3,"t":31449599,"subjects":[{"sim":1}],"causes":[{"plan":{"by":2,"planned_at":0}}],"payload":{"core:birthday":{"age_years":1}}}"#
        ]
    );
}

// ---------------------------------------------------------------------------------
// Rejections (ADR-0006 section 4), one case per row of the table and per condition.

#[test]
fn load_rejects_bad_magic() {
    for bytes in [
        Vec::new(),
        b"SIMAGEN".to_vec(),
        b"simagent".to_vec(),
        File::tiny().with(|f| f.magic[0] = b'X'),
        File::tiny().with(|f| f.magic[7] = b't'),
    ] {
        assert_eq!(load_error(&bytes), Some(LoadError::BadMagic), "{bytes:?}");
    }
}

#[test]
fn load_rejects_foreign_format_version() {
    for version in [0, 2] {
        let bytes = File::tiny().with(|f| f.format_version = version);
        assert_eq!(load_error(&bytes), Some(LoadError::ForeignFormatVersion));
    }
}

/// ADR-0003 `load_rejects_foreign_core_version`.
#[test]
fn load_rejects_foreign_core_version() {
    for version in [0, 2] {
        let bytes = File::tiny().with(|f| f.core_version = version);
        assert_eq!(load_error(&bytes), Some(LoadError::ForeignCoreVersion));
    }
}

/// ADR-0003 `load_rejects_foreign_data_hash`.
#[test]
fn load_rejects_foreign_data_hash() {
    let bytes = File::tiny().bytes();
    assert!(World::load(&bytes, EMPTY_DATA_HASH).is_ok());
    assert_eq!(
        World::load(&bytes, EMPTY_DATA_HASH + 1).err(),
        Some(LoadError::ForeignDataHash)
    );
    let other = File::tiny().with(|f| f.data_hash = 7);
    assert_eq!(load_error(&other), Some(LoadError::ForeignDataHash));
    assert!(World::load(&other, 7).is_ok());
}

/// The checks run in the order of the table: the first mismatch names the error.
#[test]
fn load_checks_run_in_order() {
    let all_wrong = |f: &mut File| {
        f.format_version = 2;
        f.core_version = 2;
        f.data_hash = 7;
        f.projection_version = 2;
    };
    assert_eq!(
        load_error(&File::tiny().with(all_wrong)),
        Some(LoadError::ForeignFormatVersion)
    );
    let bytes = File::tiny().with(|f| {
        all_wrong(f);
        f.format_version = 1;
    });
    assert_eq!(load_error(&bytes), Some(LoadError::ForeignCoreVersion));
    let bytes = File::tiny().with(|f| {
        all_wrong(f);
        f.format_version = 1;
        f.core_version = 1;
    });
    assert_eq!(load_error(&bytes), Some(LoadError::ForeignDataHash));
}

#[test]
fn load_rejects_foreign_projection_version() {
    for version in [0, 2] {
        let bytes = File::tiny().with(|f| f.projection_version = version);
        assert_eq!(
            load_error(&bytes),
            Some(LoadError::ForeignProjectionVersion)
        );
    }
}

#[test]
fn load_rejects_malformed_bytes() {
    let good = File::tiny().bytes();
    let header = hex(HEADER).len();
    // Every proper prefix behind the magic is cut off somewhere.
    for cut in 8..good.len() {
        let error = load_error(&good[..cut]);
        assert_eq!(error, Some(LoadError::Malformed), "cut at {cut}");
    }
    // Bytes after the third part.
    let mut longer = good.clone();
    longer.push(0);
    assert_eq!(load_error(&longer), Some(LoadError::Malformed));
    // A map with a repeated or an unsorted key is not what `save` writes.
    let repeated = File::tiny().with(|f| f.sims = vec![(1, -1), (1, -1)]);
    assert_eq!(load_error(&repeated), Some(LoadError::Malformed));
    let unsorted = File::tiny().with(|f| {
        f.next_sim_id = 3;
        f.sims = vec![(2, -1), (1, -1)];
        f.queue = vec![
            (Y - 1, 0, 2, Payload::Birthday { sim: 1 }, None),
            (Y - 1, 0, 3, Payload::Birthday { sim: 2 }, None),
            (Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
        ];
        f.next_event_seq = 4;
        f.plan_refs = vec![(2, 0), (2, 0), (1, 0)];
    });
    assert_eq!(load_error(&unsorted), Some(LoadError::Malformed));
    // A time outside the range of the simulation.
    let far = File::tiny().with(|f| f.sims = vec![(1, i64::MIN)]);
    assert_eq!(load_error(&far), Some(LoadError::Malformed));
    // An unknown event variant: index 2 where 0 and 1 exist.
    let mut unknown = good.clone();
    let payload_at = header + 5 + 4 + 2;
    assert_eq!(
        unknown[payload_at..payload_at + 2],
        [1, 1],
        "birthday of sim 1"
    );
    unknown[payload_at] = 2;
    assert_eq!(load_error(&unknown), Some(LoadError::Malformed));
}

#[test]
fn load_rejects_an_invalid_queue() {
    let cases: Vec<(&str, Vec<u8>)> = vec![
        (
            "a plan reference is missing",
            File::tiny().with(|f| {
                f.plan_refs.pop();
            }),
        ),
        (
            "one plan reference too many",
            File::tiny().with(|f| f.plan_refs.push((1, 0))),
        ),
        (
            "an event in a closed second",
            File::tiny().with(|f| f.closed_through = Y - 1),
        ),
        (
            "a micro step other than 0",
            File::tiny().with(|f| f.queue[0].1 = 1),
        ),
        (
            "an insertion number not below the next",
            File::tiny().with(|f| f.next_event_seq = 2),
        ),
        (
            "a repeated insertion number",
            File::tiny().with(|f| f.queue[0].2 = 1),
        ),
        ("the wrong order", File::tiny().with(|f| f.queue.swap(0, 1))),
        (
            "a period below one second",
            File::tiny().with(|f| f.queue[1].4 = Some(0)),
        ),
    ];
    for (name, bytes) in cases {
        assert_eq!(load_error(&bytes), Some(LoadError::InvalidQueue), "{name}");
    }
}

#[test]
fn load_rejects_inconsistent_saves() {
    let limit = (1_u64 << 53) - 1;
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("sim counter 0", File::tiny().with(|f| f.next_sim_id = 0)),
        (
            "sim counter beyond 2^53",
            File::tiny().with(|f| f.next_sim_id = limit + 2),
        ),
        (
            "event counter 0",
            File::tiny().with(|f| f.next_event_id = 0),
        ),
        (
            "event counter beyond 2^53",
            File::tiny().with(|f| f.next_event_id = limit + 2),
        ),
        (
            "a sim id not below the counter",
            File::tiny().with(|f| f.next_sim_id = 1),
        ),
        (
            "an archived id not below the counter",
            File::tiny().with(|f| {
                f.archive = vec![(2, (-9, -2))];
            }),
        ),
        (
            "an id living and archived",
            File::tiny().with(|f| {
                f.archive = vec![(1, (-9, -2))];
            }),
        ),
        (
            "a birthday for a sim that does not live",
            File::tiny().with(|f| {
                f.next_sim_id = 3;
                f.queue[0].3 = Payload::Birthday { sim: 2 };
            }),
        ),
        (
            "a sim born after now",
            File::tiny().with(|f| f.sims = vec![(1, 1)]),
        ),
        (
            "an archived sim that left before its birth",
            File::tiny().with(|f| {
                f.next_sim_id = 3;
                f.archive = vec![(2, (-2, -9))];
            }),
        ),
        (
            "an archived sim that left after now",
            File::tiny().with(|f| {
                f.next_sim_id = 3;
                f.archive = vec![(2, (-9, 1))];
            }),
        ),
        (
            "a plan reference to an entry that does not exist yet",
            File::tiny().with(|f| {
                f.plan_refs[0].0 = 3;
            }),
        ),
        (
            "a plan made after now",
            File::tiny().with(|f| f.plan_refs[0].1 = 1),
        ),
        (
            "input numbers not from 1",
            File::tiny().with(|f| f.input_log[0].1 = 2),
        ),
        (
            "an input stamped after the clock",
            File::tiny().with(|f| f.input_log[0].0 = 0),
        ),
        (
            "a seed without a start",
            File::tiny().with(|f| f.input_log.clear()),
        ),
        (
            "a start without a seed",
            File::tiny().with(|f| {
                f.world_seed = None;
            }),
        ),
        (
            "a start with another seed",
            File::tiny().with(|f| f.world_seed = Some(0xfe)),
        ),
        (
            "two starts",
            File::tiny().with(|f| {
                let mut second = f.input_log[0].clone();
                second.1 = 2;
                f.input_log.push(second);
            }),
        ),
        (
            "year hashes out of order",
            File::tiny().with(|f| {
                f.closed_through = 5 * Y;
                f.queue = vec![(6 * Y - 1, 0, 1, Payload::YearEnd, Some(Y))];
                f.plan_refs = vec![(1, 0)];
                f.sims.clear();
                f.next_sim_id = 1;
                f.year_hashes = vec![(1, 7), (0, 7)];
            }),
        ),
        (
            "a year hash for a year that has not ended",
            File::tiny().with(|f| {
                f.year_hashes = vec![(0, 7)];
            }),
        ),
    ];
    for (name, bytes) in cases {
        assert_eq!(load_error(&bytes), Some(LoadError::Inconsistent), "{name}");
    }
    // The limits that are allowed: an exhausted counter stands at 2^53.
    let exhausted = File::tiny().with(|f| {
        f.next_sim_id = limit + 1;
        f.next_event_id = limit + 1;
    });
    assert!(load(&exhausted).is_ok());
    let hashes = File::tiny().with(|f| {
        f.closed_through = 2 * Y - 1;
        f.queue = vec![(3 * Y - 1, 0, 1, Payload::YearEnd, Some(Y))];
        f.plan_refs = vec![(1, 0)];
        f.sims.clear();
        f.next_sim_id = 1;
        f.input_log[0].2 = TestInput::Start {
            world_seed: 0xff,
            founder_tag: "test:tiny".to_owned(),
            founders: Vec::new(),
        };
        f.year_hashes = vec![(0, 7), (1, 7)];
    });
    assert_eq!(load_error(&hashes), None);
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 2_000,
        rng_seed: RngSeed::Fixed(0x5c4e_d06a),
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    /// ADR-0006 test 6: no input makes `load` panic, and whatever it accepts it writes
    /// back byte for byte.
    #[test]
    fn load_never_panics(
        garbage in prop::collection::vec(any::<u8>(), 0..200),
        changes in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 1..4),
        cut in any::<prop::sample::Index>(),
    ) {
        let mut with_magic = b"SIMAGENT".to_vec();
        with_magic.extend(&garbage);
        let mut world = three_founders();
        let mut out = YearOut::default();
        world.advance_until(&mut out, t(2 * Y + 5), &mut |_| false);
        world.take_log();
        let good = world.save(&out.year_hashes).expect("saves");
        let mut changed = good.clone();
        for (index, byte) in &changes {
            let at = index.index(changed.len());
            changed[at] = *byte;
        }
        let truncated = good[..cut.index(good.len())].to_vec();
        for bytes in [garbage, with_magic, changed, truncated] {
            if let Ok((loaded, years)) = load(&bytes) {
                prop_assert_eq!(loaded.save(&years).expect("saves"), bytes);
            }
        }
    }
}

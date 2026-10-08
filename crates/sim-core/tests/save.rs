//! Saving and loading (ADR-0006, ADR-0003 acceptance tests, ADR-0004 test 4).
//!
//! Reference bytes come from `tools/state_reference.py`. Broken saves are built with an
//! encoder of this test (`File`), written from ADR-0006 and independent of the core's
//! types; it must produce the reference bytes for the valid case.

mod common;

use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use serde::Serialize;
use sim_core::canonical::YearHash;
use sim_core::data::Data;
use sim_core::data_id::{Code, DataId};
use sim_core::finite::FiniteF64;
use sim_core::genome::Sex;
use sim_core::input::{AppliesTo, DeathCause, Fertility, ForkOverride, Founder, Input, Start};
use sim_core::log::{LogEntry, log_hash};
use sim_core::params::{ParamChange, WorldParams};
use sim_core::time::SimTime;
use sim_core::version::CORE_VERSION;
use sim_core::world::{FORMAT_VERSION, LoadError, SaveError, World, YearOut};

/// The data hash of the definitions of the repository, from `tools/data_reference.py`
/// (ADR-0007).
const DATA_HASH: u64 = 0x8098_8ce4_5d94_0fd1;
const Y: i64 = 31_449_600;
const MONTH: i64 = 2_419_200;
const DAY: i64 = 86_400;

// tools/state_reference.py
const HEADER: &str = "53494d4147454e54010ed19fd0ecc59ca3cc8001";
const TINY_SAVE_AFTER_START: &str = "53494d4147454e54010ed19fd0ecc59ca3cc80010901ff01010000010000000000000000122d0a01020102fe87ff1d0002010100fe87ff1d000100018088ff1d030101019893ebf7f7b1d2ec5e000000000100000000000000000000000002020001000301010100ff0109746573743a74696e790000010100010a636f72653a64756d6d79020001000000000000000000122d0a00";
const TINY_SAVE_OVERRIDDEN: &str = "53494d4147454e54010ed19fd0ecc59ca3cc80010901ff01010100010000000000000000122d0a01020102fe87ff1d0002010100fe87ff1d000100018088ff1d030101019893ebf7f7b1d2ec5e000000000100000000000000000000000002020001000402010100ff0109746573743a74696e790000010100010a636f72653a64756d6d79020001000000000000000000122d0a01020101000100";
const EMPTY_SAVE: &str =
    "53494d4147454e54010ed19fd0ecc59ca3cc8001090000000001010100010000000000010000";

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
        world_params: WorldParams {
            hereditary_diseases: false,
        },
        death_causes: Vec::new(),
        founders: born.iter().copied().map(founder).collect(),
        couples: Vec::new(),
        fertility: barren(),
    })
}

/// A founder born at `secs`: female, with the alleles wild and dummy.
fn founder(secs: i64) -> Founder {
    Founder {
        born: SimTime::from_secs(secs),
        sex: Sex::Female,
        alleles: common::dummy(0, 1),
    }
}

/// Scenario "tiny" of the reference tool, with the log taken.
fn tiny() -> World {
    let mut world = World::new(common::data());
    world
        .submit(start(0xff, "test:tiny", &[-1]))
        .expect("accepted");
    world.take_log();
    world
}

fn three_founders() -> World {
    let mut world = World::new(common::data());
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
    World::load(bytes, common::data())
}

/// `None` if the bytes load, and then they must also be what the loaded world saves:
/// nothing is dropped or changed on the way in (ADR-0006 section 6).
fn load_error(bytes: &[u8]) -> Option<LoadError> {
    match load(bytes) {
        Ok((world, years)) => {
            assert_eq!(
                world.save(&years).expect("saves"),
                bytes,
                "save(load(b)) == b"
            );
            None
        }
        Err(error) => Some(error),
    }
}

// ---------------------------------------------------------------------------------
// An encoder for saves, from ADR-0006 section 2 and the postcard rules: a struct is its
// fields in order, a map is a sequence of key and value, an enum is its variant index.

#[derive(Clone, Serialize)]
enum Payload {
    YearEnd,
    Birthday { sim: u64 },
    Death { sim: u64, cause: u64 },
    ConceptionAttempt { woman: u64 },
    Birth { mother: u64 },
}

#[derive(Clone, Serialize)]
enum TestInput {
    Start {
        world_seed: u64,
        founder_tag: String,
        /// The switch of the hereditary diseases, the only world parameter.
        world_params: bool,
        /// (code, hazard per year, on whom it acts)
        death_causes: Vec<(String, f64, TestAppliesTo)>,
        founders: Vec<TestFounder>,
        /// (woman, man) by their places among the founders
        couples: Vec<(u32, u32)>,
        fertility: TestFertility,
    },
    ForkOverride {
        changes: Vec<TestChange>,
    },
    ArrangePartnership {
        woman: u64,
        man: u64,
    },
}

/// On whom a cause of death acts: everyone, or the homozygous while the switch is on.
#[derive(Clone, Copy, Serialize)]
enum TestAppliesTo {
    Everyone,
    DummyHomozygous,
}
use TestAppliesTo::{DummyHomozygous, Everyone};

/// A fertility under which nobody ever conceives.
fn barren() -> Fertility {
    Fertility {
        per_month: FiniteF64::try_new(0.0).expect("finite"),
        from_years: 18,
        until_years: 45,
        pregnancy_months: 10,
    }
}

#[derive(Clone, Serialize)]
enum TestChange {
    HereditaryDiseases(bool),
}

/// An override block that sets the switch of the hereditary diseases.
fn override_block(on: bool) -> TestInput {
    TestInput::ForkOverride {
        changes: vec![TestChange::HereditaryDiseases(on)],
    }
}

/// The lineage keys of the founders 0 and 1 of `test:tiny` (tools/rng_reference.py).
const TINY_KEYS: [u64; 2] = [0x5ed9_498f_7efa_c998, 0xca85_f610_f1bb_1e89];
/// Variant indices of the sex and the alleles (tests/golden/log_codes.txt).
const FEMALE: u8 = 0;
const MALE: u8 = 1;
const WILD: u8 = 0;
const DUMMY: u8 = 1;

/// The alleles of a founder at `core:dummy`, maternal and paternal, written as the
/// input log lists them: no entry for two reference alleles, else the locus with both
/// copies (tools/postcard_reference.py).
#[derive(Clone, Copy, PartialEq, Eq)]
struct Alleles(u8, u8);

impl Serialize for Alleles {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let listed: Vec<(&str, Vec<u8>)> = if (self.0, self.1) == (0, 0) {
            Vec::new()
        } else {
            vec![("core:dummy", vec![self.0, self.1])]
        };
        listed.serialize(serializer)
    }
}

/// The sex chromosome from the father: X = variant 0 with 16 slots, Y = variant 1 with 8.
#[derive(Serialize)]
enum FromFather {
    X(u16),
    Y(u8),
}

/// A genome whose only alternative alleles are at `core:dummy`: the sex, then the alleles
/// maternal and paternal. Written as the core writes `Genome`: three words per autosome
/// copy (`core:dummy` is bit 0 of the first), the X, the sex chromosome from the father,
/// the mitochondria (tools/postcard_reference.py).
#[derive(Clone, Copy, PartialEq, Eq)]
struct Genes(u8, u8, u8);

impl Serialize for Genes {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let from_father = if self.0 == FEMALE {
            FromFather::X(0)
        } else {
            FromFather::Y(0)
        };
        let words = |bit: u8| [u64::from(bit), 0, 0];
        (words(self.1), words(self.2), 0_u16, from_father, 0_u8).serialize(serializer)
    }
}

/// (born, sex, alleles)
type TestFounder = (i64, u8, Alleles);
/// (before, since): how long the hereditary causes acted in closed periods, and since
/// when they act now.
type TestExposure = (i64, Option<i64>);
/// (mother, father, conception)
type TestParents = (u64, u64, u32);
/// (born, lineage_key, sex, genome, exposure, conceptions, parents)
type Living = (
    i64,
    u64,
    u8,
    Genes,
    Option<TestExposure>,
    u32,
    Option<TestParents>,
);
/// Why a sim left: it died of the cause with this key.
#[derive(Clone, Copy, Serialize)]
enum TestLeftBy {
    Death { cause: u64 },
}

/// (born, left, reason, lineage_key, sex, genome, exposure, conceptions, parents)
type Archived = (
    i64,
    i64,
    TestLeftBy,
    u64,
    u8,
    Genes,
    Option<TestExposure>,
    u32,
    Option<TestParents>,
);

/// A founder as `founder` of the tests builds it.
fn test_founder(born: i64) -> TestFounder {
    (born, FEMALE, Alleles(WILD, DUMMY))
}

/// The founder at `index` of the list of `test:tiny` as a living sim.
fn living(index: usize, born: i64) -> Living {
    (
        born,
        TINY_KEYS[index],
        FEMALE,
        Genes(FEMALE, WILD, DUMMY),
        None,
        0,
        None,
    )
}

/// The same founder after leaving at `left`.
fn archived(index: usize, born: i64, left: i64) -> Archived {
    let reason = TestLeftBy::Death { cause: 7 };
    (
        born,
        left,
        reason,
        TINY_KEYS[index],
        FEMALE,
        Genes(FEMALE, WILD, DUMMY),
        None,
        0,
        None,
    )
}

/// (time, micro, seq, payload, period)
type QueueEntry = (i64, u16, u64, Payload, Option<i64>);
/// (woman, man, formed, ended)
type TestPartnership = (u64, u64, i64, Option<i64>);
/// (per month, from years, until years, pregnancy months)
type TestFertility = (f64, u32, u32, u32);
/// The fertility under which nobody ever conceives.
const BARREN: TestFertility = (0.0, 18, 45, 10);
/// (father, lineage_key, conception, sex, genome, conceived)
type TestPregnancy = (u64, u64, u32, u8, Genes, i64);

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
    /// The switch of the hereditary diseases; `None` before the start.
    world_params: Option<bool>,
    /// (key of the code, hazard per year, on whom it acts)
    death_causes: Vec<(u64, f64, TestAppliesTo)>,
    fertility: Option<TestFertility>,
    closed_through: i64,
    next_sim_id: u64,
    next_partnership_id: u64,
    queue: Vec<QueueEntry>,
    next_event_seq: u64,
    /// (id, sim); a list of pairs encodes like a map and can hold duplicates.
    sims: Vec<(u64, Living)>,
    partnerships: Vec<(u64, TestPartnership)>,
    /// (mother, pregnancy)
    pregnancies: Vec<(u64, TestPregnancy)>,
    archive: Vec<(u64, Archived)>,
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
            core_version: 14,
            data_hash: DATA_HASH,
            projection_version: 9,
            world_seed: Some(0xff),
            world_params: Some(false),
            death_causes: Vec::new(),
            fertility: Some(BARREN),
            closed_through: -1,
            next_sim_id: 2,
            next_partnership_id: 1,
            queue: vec![
                (Y - 1, 0, 2, Payload::Birthday { sim: 1 }, None),
                (Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
            ],
            next_event_seq: 3,
            sims: vec![(1, living(0, -1))],
            partnerships: Vec::new(),
            pregnancies: Vec::new(),
            archive: Vec::new(),
            plan_refs: vec![(2, 0), (1, 0)],
            next_event_id: 3,
            input_log: vec![(
                -1,
                1,
                TestInput::Start {
                    world_seed: 0xff,
                    founder_tag: "test:tiny".to_owned(),
                    world_params: false,
                    death_causes: Vec::new(),
                    founders: vec![test_founder(-1)],
                    couples: Vec::new(),
                    fertility: BARREN,
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
    assert_eq!(CORE_VERSION, 14);
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
    assert_eq!(
        World::new(common::data()).save(&[]).expect("saves"),
        hex(EMPTY_SAVE)
    );
}

/// ADR-0003 `save_load_save_is_byte_identical`.
#[test]
fn save_load_save_is_byte_identical() {
    // Never started.
    let bytes = World::new(common::data()).save(&[]).expect("saves");
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
    let mut world = World::new(common::data());
    world
        .submit(start(0xff, "test:tiny", &[-1]))
        .expect("accepted");
    let before = world.state_hash();
    assert_eq!(world.save(&[]), Err(SaveError::LogNotTaken));
    assert_eq!(world.state_hash(), before);
    assert_eq!(world.take_log().len(), 2, "the entries are still there");
    assert_eq!(world.save(&[]).expect("saves"), hex(TINY_SAVE_AFTER_START));
}

/// ADR-0006 section 3: exactly the year hashes of the completed years, in order.
#[test]
fn save_rejects_wrong_year_hashes() {
    let hash = |year: i64| YearHash { year, hash: 7 };
    let mut world = three_founders();
    world.take_log();
    let fresh = world.save(&[]).expect("no year has ended");
    assert_eq!(world.save(&[hash(0)]), Err(SaveError::YearHashesMismatch));
    let mut out = YearOut::default();
    world.advance_until(&mut out, t(2 * Y - 2), &mut |_| false);
    world.take_log();
    assert_eq!(
        out.year_hashes.len(),
        1,
        "one second before the end of year 1"
    );
    let before = world.state_hash();
    for wrong in [
        vec![],
        vec![hash(1)],
        vec![hash(0), hash(1)],
        vec![hash(0), hash(0)],
        vec![hash(-1), hash(0)],
    ] {
        assert_eq!(
            world.save(&wrong),
            Err(SaveError::YearHashesMismatch),
            "{wrong:?}"
        );
    }
    assert_eq!(world.state_hash(), before);
    assert!(
        world.save(&[hash(0)]).is_ok(),
        "the values are the caller's"
    );
    world.advance_until(&mut out, t(2 * Y - 1), &mut |_| false);
    world.take_log();
    assert_eq!(world.save(&[hash(0)]), Err(SaveError::YearHashesMismatch));
    assert!(world.save(&out.year_hashes).is_ok());
    // A world that never started has no year ends, however far its clock went.
    let mut idle = World::new(common::data());
    idle.advance_until(&mut YearOut::default(), t(3 * Y), &mut |_| false);
    assert_eq!(idle.save(&[hash(0)]), Err(SaveError::YearHashesMismatch));
    assert!(idle.save(&[]).is_ok());
    assert_ne!(idle.save(&[]).expect("saves"), fresh);
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
    for version in [13, 15] {
        let bytes = File::tiny().with(|f| f.core_version = version);
        assert_eq!(load_error(&bytes), Some(LoadError::ForeignCoreVersion));
    }
}

/// ADR-0003 `load_rejects_foreign_data_hash`, with the definitions of ADR-0007.
#[test]
fn load_rejects_foreign_data_hash() {
    let bytes = File::tiny().bytes();
    assert!(World::load(&bytes, common::data()).is_ok());
    let other = File::tiny().with(|f| f.data_hash = 7);
    assert_eq!(load_error(&other), Some(LoadError::ForeignDataHash));
    // Other definitions: one calibration value changed. A world made with them saves
    // their hash, loads with them and is rejected with the repository's.
    let (mut calibration, genome_map) = common::files();
    calibration["values"]["core:mutation_rate"]["value"] = serde_json::json!(4e-5);
    let changed = Data::new(common::definitions(calibration, genome_map).expect("definitions"))
        .expect("checked");
    assert_ne!(changed.hash(), common::data().hash());
    let saved = World::new(changed.clone()).save(&[]).expect("saves");
    let (loaded, years) = World::load(&saved, changed).expect("loads with its data");
    assert_eq!(loaded.save(&years).expect("saves"), saved);
    assert_eq!(
        World::load(&saved, common::data()).err(),
        Some(LoadError::ForeignDataHash)
    );
}

/// The checks run in the order of the table: the first mismatch names the error.
#[test]
fn load_checks_run_in_order() {
    let all_wrong = |f: &mut File| {
        f.format_version = 2;
        f.core_version = 15;
        f.data_hash = 7;
        f.projection_version = 10;
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
        f.core_version = 14;
    });
    assert_eq!(load_error(&bytes), Some(LoadError::ForeignDataHash));
    // The projection version is read before the fields whose layout it decides.
    let mut bytes = File::tiny().with(|f| f.projection_version = 10);
    bytes.truncate(hex(HEADER).len() + 1);
    assert_eq!(
        load_error(&bytes),
        Some(LoadError::ForeignProjectionVersion)
    );
    // A cut field comes before every check behind it.
    let mut bytes = File::tiny().with(all_wrong);
    bytes.truncate(8);
    assert_eq!(load_error(&bytes), Some(LoadError::Malformed));
    // Decoding and the canonical form come before the queue and the invariants.
    let bytes = File::tiny().with(|f| {
        f.sims = vec![(1, living(0, -1)), (1, living(0, -1))];
        f.plan_refs.clear();
        f.next_sim_id = 0;
    });
    assert_eq!(load_error(&bytes), Some(LoadError::Malformed));
    let bytes = File::tiny().with(|f| {
        f.plan_refs.clear();
        f.next_sim_id = 0;
    });
    assert_eq!(load_error(&bytes), Some(LoadError::InvalidQueue));
}

#[test]
fn load_rejects_foreign_projection_version() {
    for version in [8, 10] {
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
    // A time outside the range of the simulation.
    let far = File::tiny().with(|f| f.sims = vec![(1, living(0, i64::MIN))]);
    assert_eq!(load_error(&far), Some(LoadError::Malformed));
    // An unknown event variant: index 2 where 0 and 1 exist.
    let mut unknown = good.clone();
    // Behind the header: version, seed (3 bytes), world parameters (2 bytes), the number of
    // causes of death, the fertility (12 bytes), clock, sim counter, partnership counter,
    // queue length, then the first entry with its time (4 bytes), micro step and
    // insertion number.
    let payload_at = header + 23 + 4 + 2;
    assert_eq!(
        unknown[payload_at..payload_at + 2],
        [1, 1],
        "birthday of sim 1"
    );
    unknown[payload_at] = 2;
    assert_eq!(load_error(&unknown), Some(LoadError::Malformed));
}

/// ADR-0006 section 4: only the bytes `save` writes are accepted. serde and postcard
/// read more than that: repeated and unsorted map keys, varints longer than needed.
#[test]
fn load_rejects_non_canonical_bytes() {
    let repeated = File::tiny().with(|f| f.sims = vec![(1, living(0, -1)), (1, living(0, -1))]);
    assert_eq!(load_error(&repeated), Some(LoadError::Malformed));
    // Two founders, valid in every other respect, with the map keys in the wrong order.
    let two = |sims: Vec<(u64, Living)>| {
        File::tiny().with(|f| {
            f.next_sim_id = 3;
            f.sims = sims;
            f.queue = vec![
                (Y - 1, 0, 2, Payload::Birthday { sim: 1 }, None),
                (Y - 1, 0, 3, Payload::Birthday { sim: 2 }, None),
                (Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
            ];
            f.next_event_seq = 4;
            f.plan_refs = vec![(2, 0), (3, 0), (1, 0)];
            f.next_event_id = 4;
            f.input_log[0].2 = TestInput::Start {
                world_seed: 0xff,
                founder_tag: "test:tiny".to_owned(),
                world_params: false,
                death_causes: Vec::new(),
                founders: vec![test_founder(-1), test_founder(-1)],
                couples: Vec::new(),
                fertility: BARREN,
            };
        })
    };
    let (first, second) = ((1, living(0, -1)), (2, living(1, -1)));
    assert_eq!(load_error(&two(vec![first, second])), None);
    assert_eq!(
        load_error(&two(vec![second, first])),
        Some(LoadError::Malformed)
    );
    // The same value as a varint of two bytes: 0x82 0x00 is 2, like 0x02.
    let good = File::tiny().bytes();
    // Behind the header: version, seed (3 bytes), world parameters (2 bytes), the number
    // of causes of death, the fertility (12 bytes), clock.
    let at = hex(HEADER).len() + 20;
    assert_eq!(good[at], 2, "next_sim_id of the tiny world");
    let mut overlong = good.clone();
    overlong.splice(at..=at, [0x82, 0x00]);
    assert_eq!(load_error(&overlong), Some(LoadError::Malformed));
    // Length prefixes far beyond the input: rejected without reading that much. The
    // partnership counter lies between.
    assert_eq!(good[at + 1], 1, "next_partnership_id of the tiny world");
    let queue_len = at + 2;
    assert_eq!(good[queue_len], 2, "two pending events");
    let mut huge = good.clone();
    huge.splice(queue_len..=queue_len, [0xff, 0xff, 0xff, 0xff, 0x0f]);
    assert_eq!(load_error(&huge), Some(LoadError::Malformed));
    let mut huge_tail = good[..good.len() - 1].to_vec();
    huge_tail.extend([0xff; 9]);
    huge_tail.push(0x01);
    assert_eq!(load_error(&huge_tail), Some(LoadError::Malformed));
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
        (
            "the insertion number 0",
            File::tiny().with(|f| f.queue[0].2 = 0),
        ),
    ];
    for (name, bytes) in cases {
        assert_eq!(load_error(&bytes), Some(LoadError::InvalidQueue), "{name}");
    }
}

/// "tiny" with an override block right after `Start` that switches the hereditary
/// diseases on (tools/state_reference.py): one more entry was written, so the next
/// EventId is 4, and the input log has a second record.
fn overridden() -> File {
    let mut file = File::tiny();
    file.world_params = Some(true);
    file.next_event_id = 4;
    file.input_log.push((-1, 2, override_block(true)));
    file
}

/// ADR-0006 section 2: the world parameters are part of the projection, the override
/// block is part of the input log.
#[test]
fn a_save_keeps_the_override_and_the_world_parameters() {
    assert_eq!(overridden().bytes(), hex(TINY_SAVE_OVERRIDDEN));
    let mut world = tiny();
    world
        .submit(Input::ForkOverride(ForkOverride {
            changes: vec![ParamChange::HereditaryDiseases(true)],
        }))
        .expect("accepted");
    world.take_log();
    let bytes = world.save(&[]).expect("saves");
    assert_eq!(bytes, hex(TINY_SAVE_OVERRIDDEN));
    let (loaded, _) = load(&bytes).expect("loads");
    assert_eq!(
        loaded.state().world_params(),
        Some(WorldParams {
            hereditary_diseases: true
        })
    );
    assert_eq!(loaded.input_log(), world.input_log());
    assert_eq!(loaded.state_hash(), world.state_hash());
}

/// ADR-0006 section 5 for the override block: the world parameters are those of `Start`
/// after every block of the input log, a block passes the check of the input, and
/// stamps never fall and never lie after the clock.
#[test]
fn load_checks_the_override_blocks_of_the_input_log() {
    assert_eq!(load_error(&overridden().bytes()), None);
    // A block that sets the value the switch already has is a block like any other.
    let same = File::tiny().with(|f| {
        f.next_event_id = 4;
        f.input_log.push((-1, 2, override_block(false)));
    });
    assert_eq!(load_error(&same), None);
    // Two blocks: the last one counts. The clock stands at 4, the second block came later.
    let two = |change: fn(&mut File)| {
        let mut file = overridden();
        file.closed_through = 4;
        file.world_params = Some(false);
        file.next_event_id = 5;
        file.input_log.push((3, 3, override_block(false)));
        change(&mut file);
        file.bytes()
    };
    assert_eq!(load_error(&two(|_| {})), None);

    let cases: [(&str, Vec<u8>); 15] = [
        (
            "a world that never started with world parameters",
            never_started().with(|f| f.world_params = Some(false)),
        ),
        (
            "a started world without world parameters",
            File::tiny().with(|f| f.world_params = None),
        ),
        (
            "a started world without world parameters and without inputs",
            File::tiny().with(|f| {
                f.world_params = None;
                f.input_log.clear();
            }),
        ),
        (
            "world parameters that are not those of the start",
            File::tiny().with(|f| f.world_params = Some(true)),
        ),
        (
            "world parameters before the override of the log",
            overridden().with(|f| f.world_params = Some(false)),
        ),
        (
            "world parameters of the first of two blocks",
            two(|f| f.world_params = Some(true)),
        ),
        (
            // The parameters are those of the start that follows: only the order is wrong.
            "an override block before the start",
            overridden().with(|f| {
                f.input_log.swap(0, 1);
                f.input_log[0].1 = 1;
                f.input_log[1].1 = 2;
                f.world_params = Some(false);
            }),
        ),
        (
            "an override block and no start",
            overridden().with(|f| {
                f.input_log.remove(0);
                f.input_log[0].1 = 1;
            }),
        ),
        (
            "an empty override block",
            overridden().with(|f| {
                f.input_log[1].2 = TestInput::ForkOverride {
                    changes: Vec::new(),
                };
                f.world_params = Some(false);
            }),
        ),
        (
            "an override block that names a parameter twice",
            overridden().with(|f| {
                f.input_log[1].2 = TestInput::ForkOverride {
                    changes: vec![
                        TestChange::HereditaryDiseases(false),
                        TestChange::HereditaryDiseases(true),
                    ],
                };
            }),
        ),
        (
            "an override block stamped before the start input",
            overridden().with(|f| f.input_log[1].0 = -2),
        ),
        (
            "an override block stamped after the clock",
            overridden().with(|f| f.input_log[1].0 = 0),
        ),
        (
            "a second block stamped before the first",
            two(|f| {
                f.input_log[1].0 = 4;
            }),
        ),
        (
            "a gap in the sequence numbers",
            overridden().with(|f| f.input_log[1].1 = 3),
        ),
        (
            "a second start after the block",
            overridden().with(|f| {
                let start = f.input_log[0].2.clone();
                f.input_log.push((-1, 3, start));
            }),
        ),
    ];
    for (name, bytes) in &cases {
        assert_eq!(load_error(bytes), Some(LoadError::Inconsistent), "{name}");
    }
}

// Scenario "mortal" of tools/state_reference.py (see tests/death.rs): she dies of
// `test:base` in year 1, he of `test:old_age` a little later.
const MORTAL_SAVE_AFTER_START: &str = "53494d4147454e54010ed19fd0ecc59ca3cc80010901ff010100028bbec9d8c5bd99888b01000000000000e03f00ded289dba5abffff549a9999999999a93f00010000000000000000122d0a01030107fc87ff1d0005010200fe87ff1d0002010100fe87ff1d000100018088ff1d88c4db2b000302018bbec9d8c5bd99888b0100baea993300070202ded289dba5abffff5400b4e9d27100040201ded289dba5abffff5400c09cbf8901000602028bbec9d8c5bd99888b010008020101dded90d1a3f7cbf6b30100000000010000000000000000000203cbdbe1f0b893cbc91e010100000100000001000001000000000000000703000200010002000300020003000401010100ff010b746573743a6d6f7274616c000209746573743a62617365000000000000e03f000c746573743a6f6c645f6167659a9999999999a93f00020100010a636f72653a64756d6d790200010301010a636f72653a64756d6d79020101000000000000000000122d0a00";
const MORTAL_SAVE_AFTER_YEAR_1: &str = "53494d4147454e54010ed19fd0ecc59ca3cc80010901ff010100028bbec9d8c5bd99888b01000000000000e03f00ded289dba5abffff549a9999999999a93f00010000000000000000122d0afe8ffe3b030101fe97fd59000100018088ff1d0a00000002010188c4db2b008bbec9d8c5bd99888b01dded90d1a3f7cbf6b30100000000010000000000000000000203baea993300ded289dba5abffff54cbdbe1f0b893cbc91e010100000100000001000001000000000101000801010100ff010b746573743a6d6f7274616c000209746573743a62617365000000000000e03f000c746573743a6f6c645f6167659a9999999999a93f00020100010a636f72653a64756d6d790200010301010a636f72653a64756d6d79020101000000000000000000122d0a0200eddcb5c9bcb2a0d3f90102c3cec993a8dec18279";
/// The lineage keys of the founders 0 and 1 of `test:mortal` (tools/rng_reference.py).
const MORTAL_KEYS: [u64; 2] = [0xb3ed_2fba_3a24_36dd, 0x1e93_2c9b_8e18_6dcb];
/// `str_key` of `test:base` and `test:old_age`.
const BASE: u64 = 0x8b10_65ec_5b12_5f0b;
const OLD_AGE: u64 = 0x54ff_fd5a_5b62_695e;
/// When each death is due, in seconds: hers of `test:base` and of `test:old_age`, his
/// of `test:base` and of `test:old_age`.
const HER_DEATH: i64 = 45_838_596;
const HER_OLD_AGE: i64 = 119_167_578;
const HIS_BASE: i64 = 144_172_832;
const HIS_DEATH: i64 = 53_688_989;
const MORTAL_YEAR_HASHES: [u64; 2] = [0xf9a6_8193_c92d_6e6d, 0x7905_06f2_8272_6743];

fn she(born: i64) -> Living {
    (
        born,
        MORTAL_KEYS[0],
        FEMALE,
        Genes(FEMALE, WILD, DUMMY),
        None,
        0,
        None,
    )
}

fn he(born: i64) -> Living {
    // The dummy allele twice: an exposure, nothing closed, the switch off.
    (
        born,
        MORTAL_KEYS[1],
        MALE,
        Genes(MALE, DUMMY, DUMMY),
        Some((0, None)),
        0,
        None,
    )
}

/// A living sim in the archive: it left at `left`, dead of the cause with the key `cause`.
fn dead(sim: Living, left: i64, cause: u64) -> Archived {
    let (born, key, sex, genes, exposure, conceptions, parents) = sim;
    (
        born,
        left,
        TestLeftBy::Death { cause },
        key,
        sex,
        genes,
        exposure,
        conceptions,
        parents,
    )
}

/// "mortal" right after `Start`: per founder the birthday and the two deaths.
fn mortal_started() -> File {
    let mut file = File::tiny();
    file.death_causes = vec![(BASE, 0.5, Everyone), (OLD_AGE, 0.05, Everyone)];
    file.next_sim_id = 3;
    file.queue = vec![
        (Y - 2, 0, 5, Payload::Birthday { sim: 2 }, None),
        (Y - 1, 0, 2, Payload::Birthday { sim: 1 }, None),
        (Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
        (HER_DEATH, 0, 3, death(1, BASE), None),
        (HIS_DEATH, 0, 7, death(2, OLD_AGE), None),
        (HER_OLD_AGE, 0, 4, death(1, OLD_AGE), None),
        (HIS_BASE, 0, 6, death(2, BASE), None),
    ];
    file.next_event_seq = 8;
    file.sims = vec![(1, she(-1)), (2, he(-2))];
    file.plan_refs = vec![(3, 0), (2, 0), (1, 0), (2, 0), (3, 0), (2, 0), (3, 0)];
    file.next_event_id = 4;
    file.input_log[0].2 = TestInput::Start {
        world_seed: 0xff,
        founder_tag: "test:mortal".to_owned(),
        world_params: false,
        death_causes: vec![
            ("test:base".to_owned(), 0.5, Everyone),
            ("test:old_age".to_owned(), 0.05, Everyone),
        ],
        founders: vec![
            (-1, FEMALE, Alleles(WILD, DUMMY)),
            (-2, MALE, Alleles(DUMMY, DUMMY)),
        ],
        couples: Vec::new(),
        fertility: BARREN,
    };
    file
}

fn death(sim: u64, cause: u64) -> Payload {
    Payload::Death { sim, cause }
}

/// "mortal" at the end of year 1: both are in the archive, only the year end series is
/// left in the queue.
fn mortal_after_year_1() -> File {
    let mut file = mortal_started();
    file.closed_through = 2 * Y - 1;
    file.queue = vec![(3 * Y - 1, 0, 1, Payload::YearEnd, Some(Y))];
    file.next_event_seq = 10;
    file.sims.clear();
    file.archive = vec![
        (1, dead(she(-1), HER_DEATH, BASE)),
        (2, dead(he(-2), HIS_DEATH, OLD_AGE)),
    ];
    file.plan_refs = vec![(1, 0)];
    file.next_event_id = 8;
    file.year_hashes = vec![(0, MORTAL_YEAR_HASHES[0]), (1, MORTAL_YEAR_HASHES[1])];
    file
}

/// "mortal" right after her death of `test:base`: she is in the archive, his events are
/// pending (his birthday was planned anew by entry 4 at his birthday in year 0).
fn mortal_after_her_death() -> File {
    let mut file = mortal_started();
    file.closed_through = HER_DEATH;
    file.queue = vec![
        (HIS_DEATH, 0, 7, death(2, OLD_AGE), None),
        (2 * Y - 2, 0, 8, Payload::Birthday { sim: 2 }, None),
        (2 * Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
        (HIS_BASE, 0, 6, death(2, BASE), None),
    ];
    file.next_event_seq = 10;
    file.sims = vec![(2, he(-2))];
    file.archive = vec![(1, dead(she(-1), HER_DEATH, BASE))];
    file.plan_refs = vec![(3, 0), (4, Y - 2), (1, 0), (3, 0)];
    file.next_event_id = 7;
    file.year_hashes = vec![(0, MORTAL_YEAR_HASHES[0])];
    file
}

// Scenario "tie" of tools/state_reference.py: both causes are due in the same second.
/// The lineage key of founder 0 of `test:tie` (tools/rng_reference.py).
const TIE_KEY: u64 = 0x9515_c6c8_f345_ed70;
const TIE_DUE: i64 = 51_240_314;

/// "tie" right after `Start`: the woman's birthday, then her two deaths in the order
/// of the causes, both due in one second.
fn tie_started() -> File {
    let mut file = File::tiny();
    let old_age_per_year = 0.593_419_882_389_541_8;
    file.death_causes = vec![(BASE, 5.0, Everyone), (OLD_AGE, old_age_per_year, Everyone)];
    file.queue = vec![
        (Y - 1, 0, 2, Payload::Birthday { sim: 1 }, None),
        (Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
        (TIE_DUE, 0, 3, death(1, BASE), None),
        (TIE_DUE, 0, 4, death(1, OLD_AGE), None),
    ];
    file.next_event_seq = 5;
    file.sims = vec![(
        1,
        (
            -1,
            TIE_KEY,
            FEMALE,
            Genes(FEMALE, WILD, DUMMY),
            None,
            0,
            None,
        ),
    )];
    file.plan_refs = vec![(2, 0), (1, 0), (2, 0), (2, 0)];
    file.input_log[0].2 = TestInput::Start {
        founder_tag: "test:tie".to_owned(),
        world_seed: 0xff,
        world_params: false,
        death_causes: vec![
            ("test:base".to_owned(), 5.0, Everyone),
            ("test:old_age".to_owned(), old_age_per_year, Everyone),
        ],
        founders: vec![test_founder(-1)],
        couples: Vec::new(),
        fertility: BARREN,
    };
    file
}

/// "mortal" at the start of year 1, one second after the year end: nobody has died yet.
/// The birthdays of year 0 planned the next ones (insertion numbers 8 and 9, by the
/// entries 4 and 5).
fn mortal_in_year_1() -> File {
    let mut file = mortal_started();
    file.closed_through = Y;
    file.queue = vec![
        (HER_DEATH, 0, 3, death(1, BASE), None),
        (HIS_DEATH, 0, 7, death(2, OLD_AGE), None),
        (2 * Y - 2, 0, 8, Payload::Birthday { sim: 2 }, None),
        (2 * Y - 1, 0, 9, Payload::Birthday { sim: 1 }, None),
        (2 * Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
        (HER_OLD_AGE, 0, 4, death(1, OLD_AGE), None),
        (HIS_BASE, 0, 6, death(2, BASE), None),
    ];
    file.next_event_seq = 10;
    file.plan_refs = vec![
        (2, 0),
        (3, 0),
        (4, Y - 2),
        (5, Y - 1),
        (1, 0),
        (2, 0),
        (3, 0),
    ];
    file.next_event_id = 6;
    file.year_hashes = vec![(0, MORTAL_YEAR_HASHES[0])];
    file
}

/// The saves of "mortal" from the reference, the test encoder and the core are the same
/// bytes, and they load.
#[test]
fn a_save_keeps_the_deaths_and_the_archive() {
    assert_eq!(mortal_started().bytes(), hex(MORTAL_SAVE_AFTER_START));
    assert_eq!(mortal_after_year_1().bytes(), hex(MORTAL_SAVE_AFTER_YEAR_1));
    for file in [
        mortal_started(),
        mortal_in_year_1(),
        mortal_after_her_death(),
        mortal_after_year_1(),
        tie_started(),
    ] {
        assert_eq!(load_error(&file.bytes()), None);
    }
    // The state in year 1 is the one the core reaches.
    let mut world = World::new(common::data());
    world
        .submit(Input::Start(Start {
            world_seed: 0xff,
            founder_tag: DataId::new("test:mortal").expect("valid"),
            world_params: WorldParams {
                hereditary_diseases: false,
            },
            death_causes: vec![
                DeathCause {
                    cause: Code::new("test:base").expect("valid"),
                    per_year: FiniteF64::try_new(0.5).expect("finite"),
                    applies_to: AppliesTo::Everyone,
                },
                DeathCause {
                    cause: Code::new("test:old_age").expect("valid"),
                    per_year: FiniteF64::try_new(0.05).expect("finite"),
                    applies_to: AppliesTo::Everyone,
                },
            ],
            founders: vec![
                founder(-1),
                Founder {
                    born: t(-2),
                    sex: Sex::Male,
                    alleles: common::dummy(1, 1),
                },
            ],
            couples: Vec::new(),
            fertility: barren(),
        }))
        .expect("accepted");
    let mut out = YearOut::default();
    world.advance_until(&mut out, t(Y), &mut |_| false);
    world.take_log();
    assert_eq!(
        world.save(&out.year_hashes).expect("saves"),
        mortal_in_year_1().bytes()
    );
    world.advance_until(&mut out, t(HER_DEATH), &mut |_| false);
    world.take_log();
    assert_eq!(
        world.save(&out.year_hashes).expect("saves"),
        mortal_after_her_death().bytes()
    );
}

/// ADR-0006 section 5 for death: an archived sim died as the rule says, a living sim
/// has exactly the deaths in the queue that its budgets give, and the causes of the
/// projection are those of `Start`.
#[test]
fn load_checks_the_deaths() {
    // The clock stands in year 4: her death of old age would have been due by now.
    let late = |change: fn(&mut File)| {
        let mut file = mortal_after_year_1();
        file.closed_through = 5 * Y - 1;
        file.queue[0].0 = 6 * Y - 1;
        file.year_hashes = (0..5).map(|year| (year, 7)).collect();
        change(&mut file);
        file.bytes()
    };
    assert_eq!(load_error(&late(|_| {})), None);

    let cases: [(&str, Vec<u8>); 24] = [
        // Review finding: a death in the first open second has not run yet.
        (
            "a death in the second that is still open",
            mortal_after_her_death().with(|f| f.closed_through = HER_DEATH - 1),
        ),
        // Review finding: of two deaths due in one second the one planned first runs
        // first; the causes were planned in the order of the list.
        (
            "two deaths of one second planned against the order of the causes",
            tie_started().with(|f| {
                f.queue[2].3 = death(1, OLD_AGE);
                f.queue[3].3 = death(1, BASE);
            }),
        ),
        // The archive.
        (
            "a death one second after the budget ran out",
            mortal_after_year_1().with(|f| f.archive[0].1.1 = HER_DEATH + 1),
        ),
        (
            "a death one second before the budget ran out",
            mortal_after_year_1().with(|f| f.archive[1].1.1 = HIS_DEATH - 1),
        ),
        (
            "a death of the other cause at that time",
            mortal_after_year_1().with(|f| f.archive[0].1.2 = TestLeftBy::Death { cause: OLD_AGE }),
        ),
        (
            "a death of a cause the world does not have",
            mortal_after_year_1().with(|f| f.archive[0].1.2 = TestLeftBy::Death { cause: 7 }),
        ),
        (
            "a death of the cause that was due later",
            late(|f| f.archive[0].1 = dead(she(-1), HER_OLD_AGE, OLD_AGE)),
        ),
        (
            "a death that lies ahead",
            mortal_in_year_1().with(|f| {
                f.sims.remove(0);
                f.archive = vec![(1, dead(she(-1), HER_DEATH, BASE))];
                f.queue.retain(|entry| {
                    !matches!(
                        entry.3,
                        Payload::Death { sim: 1, .. } | Payload::Birthday { sim: 1 }
                    )
                });
                f.plan_refs = vec![(3, 0), (4, Y - 2), (1, 0), (3, 0)];
            }),
        ),
        (
            "an archived founder with another sex",
            mortal_after_year_1().with(|f| f.archive[0].1.4 = MALE),
        ),
        (
            "an archived founder with another genotype",
            mortal_after_year_1()
                .with(|f| f.archive[0].1.5 = Genes(f.archive[0].1.4, DUMMY, DUMMY)),
        ),
        (
            "an archived founder with another lineage key",
            mortal_after_year_1().with(|f| f.archive[0].1.3 = MORTAL_KEYS[1]),
        ),
        // The living.
        (
            "a living sim whose budget has run out",
            mortal_after_year_1().with(|f| {
                f.archive.remove(0);
                f.sims = vec![(1, she(-1))];
                f.queue
                    .insert(0, (2 * Y - 1 + Y, 0, 9, Payload::Birthday { sim: 1 }, None));
                f.queue.push((HER_OLD_AGE, 0, 4, death(1, OLD_AGE), None));
                f.plan_refs = vec![(5, Y - 1), (1, 0), (2, 0)];
            }),
        ),
        // The queue.
        (
            "a death that is due and not planned",
            mortal_started().with(|f| {
                f.queue.remove(5);
                f.plan_refs.remove(5);
            }),
        ),
        (
            "a death planned a second late",
            mortal_started().with(|f| f.queue[3].0 = HER_DEATH + 1),
        ),
        (
            "a death planned a second early",
            mortal_started().with(|f| f.queue[3].0 = HER_DEATH - 1),
        ),
        (
            "a death of a cause the world does not have",
            mortal_started().with(|f| f.queue[3].3 = death(1, 7)),
        ),
        (
            "the deaths of two sims swapped",
            mortal_started().with(|f| {
                f.queue[3].3 = death(2, BASE);
                f.queue[6].3 = death(1, BASE);
            }),
        ),
        (
            "a periodic death",
            mortal_started().with(|f| f.queue[3].4 = Some(Y)),
        ),
        (
            "a death for a sim in the archive",
            mortal_after_year_1().with(|f| {
                f.queue.push((HER_OLD_AGE, 0, 4, death(1, OLD_AGE), None));
                f.plan_refs.push((2, 0));
            }),
        ),
        // The causes.
        (
            "a cause with another hazard than in the start",
            mortal_started().with(|f| f.death_causes[0].1 = 0.25),
        ),
        (
            "the causes in another order than in the start",
            mortal_started().with(|f| f.death_causes.reverse()),
        ),
        (
            "a cause less than in the start",
            mortal_started().with(|f| {
                f.death_causes.pop();
            }),
        ),
        (
            "a world that never started with a cause",
            never_started().with(|f| f.death_causes = vec![(BASE, 0.5, Everyone)]),
        ),
        (
            "a start with a hazard below the smallest one",
            File::tiny().with(|f| {
                f.death_causes = vec![(BASE, 0.0, Everyone)];
                if let TestInput::Start { death_causes, .. } = &mut f.input_log[0].2 {
                    *death_causes = vec![("test:base".to_owned(), 0.0, Everyone)];
                }
            }),
        ),
    ];
    for (name, bytes) in &cases {
        assert_eq!(load_error(bytes), Some(LoadError::Inconsistent), "{name}");
    }
    // A death twice in the queue: the scheduler sees the same insertion number, or the
    // loader the same sim and cause.
    let twice = mortal_started().with(|f| {
        f.queue.insert(4, (HER_DEATH, 0, 8, death(1, BASE), None));
        f.plan_refs.insert(4, (2, 0));
        f.next_event_seq = 9;
    });
    assert_eq!(load_error(&twice), Some(LoadError::Inconsistent));
}

// Scenario "sick" of tools/state_reference.py: `test:base` with 0.02 per year for everyone,
// `test:dummy_disease` with 0.3 per year for the homozygous only, while the switch is on.
// She has one dummy allele, he has two.
const SWITCHED_ON_SAVE: &str = "53494d4147454e54010ed19fd0ecc59ca3cc80010901ff010101028bbec9d8c5bd99888b017b14ae47e17a943f00dd90d4c2cac3a7bbf401333333333333d33f01010000000000000000122d0afe87ff1d030106fc8ffe3b0006010200fe8ffe3b0007010100fe8ffe3b000100018088ff1dc6d6ae7e00080202dd90d4c2cac3a7bbf40100dac7cacc04000502028bbec9d8c5bd99888b01008ea5b5f604000302018bbec9d8c5bd99888b010009020101abd4f19caceffaefab0100000000010000000000000000000203fba99785c4c1bc5c01010000010000000100000100018088ff1d00000000000604fc87ff1d05fe87ff1d0100068088ff1d030002000702010100ff0109746573743a7369636b000209746573743a626173657b14ae47e17a943f0012746573743a64756d6d795f64697365617365333333333333d33f01020100010a636f72653a64756d6d790200010301010a636f72653a64756d6d79020101000000000000000000122d0afe87ff1d02010100010100e8daa5bf87d5aaf3be01";
const SICK_ON_SAVE_AFTER_YEAR_3: &str = "53494d4147454e54010ed19fd0ecc59ca3cc80010901ff010101028bbec9d8c5bd99888b017b14ae47e17a943f00dd90d4c2cac3a7bbf401333333333333d33f01010000000000000000122d0afe9ffc77030103fea7fb9501000d010100fea7fb9501000100018088ff1d8ea5b5f604000302018bbec9d8c5bd99888b01000e010101abd4f19caceffaefab0100000000010000000000000000000000010203c6ceaf6000dd90d4c2cac3a7bbf401fba99785c4c1bc5c0101000001000000010000010001000000030bfe9ffc77010002000c01010100ff0109746573743a7369636b010209746573743a626173657b14ae47e17a943f0012746573743a64756d6d795f64697365617365333333333333d33f01020100010a636f72653a64756d6d790200010301010a636f72653a64756d6d79020101000000000000000000122d0a0400bfc180c7f08acfe9b10102abb398b5c6c7c3a8e3010487f3ffbbe5c6edf3810106f7bdd7b59fddb796c101";
const OFF_AT_DUE_SAVE: &str = "53494d4147454e54010ed19fd0ecc59ca3cc80010901ff010100028bbec9d8c5bd99888b017b14ae47e17a943f00dd90d4c2cac3a7bbf401333333333333d33f01010000000000000000122d0afea7fb9501030105fcaffab301000f010200feaffab3010010010100feaffab301000100018088ff1ddac7cacc04000502028bbec9d8c5bd99888b01008ea5b5f604000302018bbec9d8c5bd99888b010011020101abd4f19caceffaefab0100000000010000000000000000000203fba99785c4c1bc5c010100000100000001000001c6ceaf60000000000000050dfca7fb95010efea7fb95010100030002000f02010100ff0109746573743a7369636b010209746573743a626173657b14ae47e17a943f0012746573743a64756d6d795f64697365617365333333333333d33f01020100010a636f72653a64756d6d790200010301010a636f72653a64756d6d79020101000000000000000000122d0ac4ceaf6002010100000500bfc180c7f08acfe9b10102abb398b5c6c7c3a8e3010487f3ffbbe5c6edf3810106b0819091d684a5eae20108bca19190a0f6c0b7f001";
/// The lineage keys of the founders 0 and 1 of `test:sick` (tools/rng_reference.py).
const SICK_KEYS: [u64; 2] = [0xabdf_eb7a_c39c_6a2b, 0x00b8_f20c_40a5_d4fb];
/// The `str_key` of `test:dummy_disease`.
const DISEASE: u64 = 0xf476_9e1c_a855_085d;
/// How long each cause must act on each of them, in seconds.
const HIS_DISEASE: i64 = 101_053_347;
const HIS_SICK_BASE: i64 = 617_173_485;
const HER_SICK_BASE: i64 = 661_039_431;

fn sick_start(on: bool) -> TestInput {
    TestInput::Start {
        world_seed: 0xff,
        founder_tag: "test:sick".to_owned(),
        world_params: on,
        death_causes: vec![
            ("test:base".to_owned(), 0.02, Everyone),
            ("test:dummy_disease".to_owned(), 0.3, DummyHomozygous),
        ],
        founders: vec![
            (-1, FEMALE, Alleles(WILD, DUMMY)),
            (-2, MALE, Alleles(DUMMY, DUMMY)),
        ],
        couples: Vec::new(),
        fertility: BARREN,
    }
}

fn sick_she() -> Living {
    (
        -1,
        SICK_KEYS[0],
        FEMALE,
        Genes(FEMALE, WILD, DUMMY),
        None,
        0,
        None,
    )
}

fn sick_he(exposure: TestExposure) -> Living {
    (
        -2,
        SICK_KEYS[1],
        MALE,
        Genes(MALE, DUMMY, DUMMY),
        Some(exposure),
        0,
        None,
    )
}

/// "sick" with the switch off, switched on at the start of year 1: his exposure begins
/// there, his death of the disease is planned by the entry of the override (6) after
/// all that was planned before.
fn sick_switched_on() -> File {
    let mut file = File::tiny();
    file.world_params = Some(true);
    file.death_causes = vec![(BASE, 0.02, Everyone), (DISEASE, 0.3, DummyHomozygous)];
    file.closed_through = Y - 1;
    file.next_sim_id = 3;
    file.queue = vec![
        (2 * Y - 2, 0, 6, Payload::Birthday { sim: 2 }, None),
        (2 * Y - 1, 0, 7, Payload::Birthday { sim: 1 }, None),
        (2 * Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
        (Y + HIS_DISEASE, 0, 8, death(2, DISEASE), None),
        (HIS_SICK_BASE, 0, 5, death(2, BASE), None),
        (HER_SICK_BASE, 0, 3, death(1, BASE), None),
    ];
    file.next_event_seq = 9;
    file.sims = vec![(1, sick_she()), (2, sick_he((0, Some(Y))))];
    file.plan_refs = vec![(4, Y - 2), (5, Y - 1), (1, 0), (6, Y), (3, 0), (2, 0)];
    file.next_event_id = 7;
    file.input_log = vec![(-1, 1, sick_start(false)), (Y - 1, 2, override_block(true))];
    file.year_hashes = vec![(0, 0xbee6_aaa8_77e9_6d68)];
    file
}

/// "sick" with the switch on from the start, at the end of year 3: he died of the
/// disease, his exposure is kept as it was then.
fn sick_on_after_year_3() -> File {
    let mut file = sick_switched_on();
    file.closed_through = 4 * Y - 1;
    file.queue = vec![
        (5 * Y - 1, 0, 13, Payload::Birthday { sim: 1 }, None),
        (5 * Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
        (HER_SICK_BASE, 0, 3, death(1, BASE), None),
    ];
    file.next_event_seq = 14;
    file.sims = vec![(1, sick_she())];
    file.archive = vec![(2, dead(sick_he((0, Some(0))), HIS_DISEASE, DISEASE))];
    file.plan_refs = vec![(11, 4 * Y - 1), (1, 0), (2, 0)];
    file.next_event_id = 12;
    file.input_log = vec![(-1, 1, sick_start(true))];
    file.year_hashes = vec![
        (0, 0xb1d3_3c57_08e0_20bf),
        (1, 0xe351_0e3c_66a6_19ab),
        (2, 0x81e7_b636_577f_f987),
        (3, 0xc12c_dee9_f6b5_def7),
    ];
    file
}

/// "sick" with the switch on from the start and off in the very second his death of the
/// disease was due: he lives with the budget used up, at the end of year 4.
fn sick_off_at_due() -> File {
    let mut file = sick_switched_on();
    file.world_params = Some(false);
    file.closed_through = 5 * Y - 1;
    file.queue = vec![
        (6 * Y - 2, 0, 15, Payload::Birthday { sim: 2 }, None),
        (6 * Y - 1, 0, 16, Payload::Birthday { sim: 1 }, None),
        (6 * Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
        (HIS_SICK_BASE, 0, 5, death(2, BASE), None),
        (HER_SICK_BASE, 0, 3, death(1, BASE), None),
    ];
    file.next_event_seq = 17;
    file.sims = vec![(1, sick_she()), (2, sick_he((HIS_DISEASE, None)))];
    file.plan_refs = vec![(13, 5 * Y - 2), (14, 5 * Y - 1), (1, 0), (3, 0), (2, 0)];
    file.next_event_id = 15;
    file.input_log = vec![
        (-1, 1, sick_start(true)),
        (HIS_DISEASE - 1, 2, override_block(false)),
    ];
    file.year_hashes = vec![
        (0, 0xb1d3_3c57_08e0_20bf),
        (1, 0xe351_0e3c_66a6_19ab),
        (2, 0x81e7_b636_577f_f987),
        (3, 0xe2d4_9425_6224_00b0),
        (4, 0xf06f_03b2_0204_50bc),
    ];
    file
}

/// The saves of "sick" from the reference and the test encoder are the same bytes, and
/// they load: an exposure since the override, one kept in the archive, one used up.
#[test]
fn a_save_keeps_the_exposure() {
    assert_eq!(sick_switched_on().bytes(), hex(SWITCHED_ON_SAVE));
    assert_eq!(
        sick_on_after_year_3().bytes(),
        hex(SICK_ON_SAVE_AFTER_YEAR_3)
    );
    assert_eq!(sick_off_at_due().bytes(), hex(OFF_AT_DUE_SAVE));
    for file in [
        sick_switched_on(),
        sick_on_after_year_3(),
        sick_off_at_due(),
    ] {
        assert_eq!(load_error(&file.bytes()), None);
    }
}

/// An exposure is what the switch history of the input log gives: an exposure exactly
/// for a sim with the dummy allele twice, since the start or the last override that
/// switched on, with the closed periods before. The living have not used up a budget,
/// the archive keeps the exposure of the death. ADR-0006 section 5.
#[test]
fn load_checks_the_exposure() {
    let cases: [(&str, Vec<u8>); 15] = [
        // The living.
        (
            "a sim with the dummy allele twice without an exposure",
            sick_switched_on().with(|f| f.sims[1].1.4 = None),
        ),
        (
            "an exposure for a sim with one dummy allele",
            sick_switched_on().with(|f| f.sims[0].1.4 = Some((0, None))),
        ),
        (
            "an exposure since one second after the override",
            sick_switched_on().with(|f| {
                f.sims[1].1.4 = Some((0, Some(Y + 1)));
                f.queue[3].0 += 1;
            }),
        ),
        (
            "an exposure that is off while the switch is on",
            sick_switched_on().with(|f| {
                f.sims[1].1.4 = Some((0, None));
                f.queue.remove(3);
                f.plan_refs.remove(3);
            }),
        ),
        (
            "an exposure with a closed second that never was",
            sick_switched_on().with(|f| {
                f.sims[1].1.4 = Some((1, Some(Y)));
                f.queue[3].0 -= 1;
            }),
        ),
        (
            "an exposure with a negative time",
            sick_switched_on().with(|f| {
                f.sims[1].1.4 = Some((-1, Some(Y)));
                f.queue[3].0 += 1;
            }),
        ),
        (
            "a start with the switch on and an exposure only since the override",
            sick_switched_on().with(|f| f.input_log[0].2 = sick_start(true)),
        ),
        (
            "a cause for the homozygous that acts on everyone in the projection",
            sick_switched_on().with(|f| f.death_causes[1].2 = Everyone),
        ),
        (
            "a death of the disease for her",
            sick_switched_on().with(|f| {
                f.queue
                    .insert(4, (Y + HIS_DISEASE + 1, 0, 9, death(1, DISEASE), None));
                f.plan_refs.insert(4, (6, Y));
                f.next_event_seq = 10;
            }),
        ),
        (
            "an exposure one second short of the history",
            sick_off_at_due().with(|f| f.sims[1].1.4 = Some((HIS_DISEASE - 1, None))),
        ),
        (
            "alive although the disease used up its budget while the switch was on",
            sick_off_at_due().with(|f| {
                f.input_log[1].0 = HIS_DISEASE + 99;
                f.sims[1].1.4 = Some((HIS_DISEASE + 100, None));
            }),
        ),
        // The archive.
        (
            "an archived sim with the dummy allele twice without an exposure",
            sick_on_after_year_3().with(|f| f.archive[0].1.6 = None),
        ),
        (
            "an archived exposure that is off though the switch was on",
            sick_on_after_year_3().with(|f| f.archive[0].1.6 = Some((0, None))),
        ),
        (
            "an archived exposure since a second after the start, with its death",
            sick_on_after_year_3().with(|f| {
                f.archive[0].1.6 = Some((0, Some(1)));
                f.archive[0].1.1 = HIS_DISEASE + 1;
            }),
        ),
        (
            "an archived death of the disease one second early",
            sick_on_after_year_3().with(|f| f.archive[0].1.1 = HIS_DISEASE - 1),
        ),
    ];
    for (name, bytes) in &cases {
        assert_eq!(load_error(bytes), Some(LoadError::Inconsistent), "{name}");
    }
}

// Scenario "partners" of tools/state_reference.py: the causes of "mortal", a woman and two
// men. `Start` pairs her with the first man; he dies first, and at the start of year 1
// `ArrangePartnership` pairs her with the second.
const PARTNERS_SAVE_AFTER_ARRANGE: &str = "53494d4147454e54010ed19fd0ecc59ca3cc80010901ff010100028bbec9d8c5bd99888b01000000000000e03f00ded289dba5abffff549a9999999999a93f00010000000000000000122d0afe87ff1d040308fa8ffe3b000c010300fe8ffe3b000d010100fe8ffe3b000100018088ff1dbca1e98b01000902038bbec9d8c5bd99888b01008adcac9201000302018bbec9d8c5bd99888b0100a2f7e1d00100040201ded289dba5abffff54008090ef9b04000e03010180a8a702daa4c38f0a000a0203ded289dba5abffff54000f020101aa90d4c3eeedd09bdb0100000000010000000000000000000305a7f3f8fdb9ee88cfed010101000000000000010000000000020101020001eab582180201038088ff1d0000010203eab58218008bbec9d8c5bd99888b01f098dcaaccecf99cb001010100000100000001000001000000000807fa87ff1d08fe87ff1d0100040002000200098088ff1d04000a02010100ff010d746573743a706172746e657273000209746573743a62617365000000000000e03f000c746573743a6f6c645f6167659a9999999999a93f00030100010a636f72653a64756d6d790200010301010a636f72653a64756d6d790201010501010a636f72653a64756d6d790201000100010000000000000000122d0afe87ff1d0202010301009ae1b8c0ffaacaab0c";
const PARTNERS_SAVE_AFTER_YEAR_4: &str = "53494d4147454e54010ed19fd0ecc59ca3cc80010901ff010100028bbec9d8c5bd99888b01000000000000e03f00ded289dba5abffff549a9999999999a93f00010000000000000000122d0afea7fb9501040301feaffab301000100018088ff1d1500020101020001eab582180201038088ff1d01bca1e98b01000301018adcac9201008bbec9d8c5bd99888b01aa90d4c3eeedd09bdb0100000000010000000000000000000203eab58218008bbec9d8c5bd99888b01f098dcaaccecf99cb001010100000100000001000001000000000305bca1e98b01008bbec9d8c5bd99888b01a7f3f8fdb9ee88cfed0101010000000000000100000000000101001202010100ff010d746573743a706172746e657273000209746573743a62617365000000000000e03f000c746573743a6f6c645f6167659a9999999999a93f00030100010a636f72653a64756d6d790200010301010a636f72653a64756d6d790201010501010a636f72653a64756d6d790201000100010000000000000000122d0afe87ff1d0202010305009ae1b8c0ffaacaab0c0294ccf3dde8a9c0beb50104a6f7a3f7cfb88fec3f06f9b3c2a0e180e0ae3108b5d1deb8eac3c1c9d901";
/// The lineage keys of the founders 0 to 2 of `test:partners` (tools/rng_reference.py).
const PARTNERS_KEYS: [u64; 3] = [
    0xdb37_436e_e875_082a,
    0xb039_e764_c557_0c70,
    0xed9e_2373_9fbe_39a7,
];
/// When each of the three dies of `test:base`, and would die of `test:old_age`.
const PARTNER_DEATHS: [i64; 3] = [153_458_437, 25_185_653, 146_614_366];
const PARTNER_OLD_AGE: [i64; 3] = [218_906_065, 115_348_095, 1_358_457_133];

fn partners_start(couples: Vec<(u32, u32)>) -> TestInput {
    TestInput::Start {
        world_seed: 0xff,
        founder_tag: "test:partners".to_owned(),
        world_params: false,
        death_causes: vec![
            ("test:base".to_owned(), 0.5, Everyone),
            ("test:old_age".to_owned(), 0.05, Everyone),
        ],
        founders: vec![
            (-1, FEMALE, Alleles(WILD, DUMMY)),
            (-2, MALE, Alleles(DUMMY, DUMMY)),
            (-3, MALE, Alleles(DUMMY, WILD)),
        ],
        couples,
        fertility: BARREN,
    }
}

/// The founder at `index` of "partners" as a living sim.
fn partner(index: usize) -> Living {
    [
        (
            -1,
            PARTNERS_KEYS[0],
            FEMALE,
            Genes(FEMALE, WILD, DUMMY),
            None,
            0,
            None,
        ),
        (
            -2,
            PARTNERS_KEYS[1],
            MALE,
            Genes(MALE, DUMMY, DUMMY),
            Some((0, None)),
            0,
            None,
        ),
        (
            -3,
            PARTNERS_KEYS[2],
            MALE,
            Genes(MALE, DUMMY, WILD),
            None,
            0,
            None,
        ),
    ][index]
}

/// "partners" right after her second partnership: the first man is in the archive, the
/// first partnership ended with his death, the second formed at the start of year 1 by
/// the input with sequence number 2.
fn partners_arranged() -> File {
    let mut file = File::tiny();
    file.death_causes = vec![(BASE, 0.5, Everyone), (OLD_AGE, 0.05, Everyone)];
    file.closed_through = Y - 1;
    file.next_sim_id = 4;
    file.next_partnership_id = 3;
    file.queue = vec![
        (2 * Y - 3, 0, 12, Payload::Birthday { sim: 3 }, None),
        (2 * Y - 1, 0, 13, Payload::Birthday { sim: 1 }, None),
        (2 * Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
        (PARTNER_DEATHS[2], 0, 9, death(3, BASE), None),
        (PARTNER_DEATHS[0], 0, 3, death(1, BASE), None),
        (PARTNER_OLD_AGE[0], 0, 4, death(1, OLD_AGE), None),
        // Her attempts from her 18th birthday on (the series of the first partnership
        // ended with his death), by the entry of the second partnership.
        (
            18 * Y,
            0,
            14,
            Payload::ConceptionAttempt { woman: 1 },
            Some(MONTH),
        ),
        (PARTNER_OLD_AGE[2], 0, 10, death(3, OLD_AGE), None),
    ];
    file.next_event_seq = 15;
    file.sims = vec![(1, partner(0)), (3, partner(2))];
    file.partnerships = vec![
        (1, (1, 2, 0, Some(PARTNER_DEATHS[1]))),
        (2, (1, 3, Y, None)),
    ];
    file.archive = vec![(2, dead(partner(1), PARTNER_DEATHS[1], BASE))];
    file.plan_refs = vec![
        (7, Y - 3),
        (8, Y - 1),
        (1, 0),
        (4, 0),
        (2, 0),
        (2, 0),
        (9, Y),
        (4, 0),
    ];
    file.next_event_id = 10;
    file.input_log = vec![
        (-1, 1, partners_start(vec![(0, 1)])),
        (Y - 1, 2, TestInput::ArrangePartnership { woman: 1, man: 3 }),
    ];
    file.year_hashes = vec![(0, 0xc57_2957_f80e_309a)];
    file
}

/// "partners" at the end of year 4: all three are in the archive, the second
/// partnership ended with the death of the second man, before hers.
fn partners_after_year_4() -> File {
    let mut file = partners_arranged();
    file.closed_through = 5 * Y - 1;
    file.queue = vec![(6 * Y - 1, 0, 1, Payload::YearEnd, Some(Y))];
    file.next_event_seq = 21;
    file.sims.clear();
    file.partnerships[1].1.3 = Some(PARTNER_DEATHS[2]);
    file.archive = vec![
        (1, dead(partner(0), PARTNER_DEATHS[0], BASE)),
        (2, dead(partner(1), PARTNER_DEATHS[1], BASE)),
        (3, dead(partner(2), PARTNER_DEATHS[2], BASE)),
    ];
    file.plan_refs = vec![(1, 0)];
    file.next_event_id = 18;
    file.year_hashes = vec![
        (0, 0xc57_2957_f80e_309a),
        (1, 0xb57d_014e_8bbc_e614),
        (2, 0x3fd8_3dc4_fee8_fba6),
        (3, 0x315d_8006_1410_99f9),
        (4, 0xd993_061e_a717_a8b5),
    ];
    file
}

/// The saves of "partners" from the reference and the test encoder are the same bytes,
/// and they load.
#[test]
fn a_save_keeps_the_partnerships() {
    assert_eq!(
        partners_arranged().bytes(),
        hex(PARTNERS_SAVE_AFTER_ARRANGE)
    );
    assert_eq!(
        partners_after_year_4().bytes(),
        hex(PARTNERS_SAVE_AFTER_YEAR_4)
    );
    for file in [partners_arranged(), partners_after_year_4()] {
        assert_eq!(load_error(&file.bytes()), None);
    }
}

/// The partnerships are those the input log formed, in its order: the couples of
/// `Start` at the start, then one per `ArrangePartnership` in the second after its
/// stamp; the counter follows them. Each pairs a woman and a man; it is open while both
/// live and ended in the second of the first death of the two. ADR-0006 section 5.
#[test]
fn load_checks_the_partnerships() {
    let cases: [(&str, Vec<u8>); 19] = [
        // Review finding: a pairing while the first partnership of the woman was open.
        (
            "a second partnership formed while the first was open",
            partners_arranged().with(|f| {
                f.partnerships[1].1.2 = 0;
                f.input_log[1].0 = -1;
            }),
        ),
        (
            "a second partnership formed in the second the first ended",
            partners_arranged().with(|f| {
                f.partnerships[1].1.2 = PARTNER_DEATHS[1];
                f.input_log[1].0 = PARTNER_DEATHS[1] - 1;
            }),
        ),
        (
            "a partnership with an ID beyond its place",
            partners_arranged().with(|f| f.partnerships[1].0 = 3),
        ),
        (
            "a partnership of a man as the woman, as its input said",
            partners_arranged().with(|f| {
                f.partnerships[1].1 = (3, 1, Y, None);
                f.input_log[1].2 = TestInput::ArrangePartnership { woman: 3, man: 1 };
            }),
        ),
        (
            "a partnership formed after the death of the man",
            partners_arranged().with(|f| {
                f.partnerships[1].1 = (1, 2, Y, Some(PARTNER_DEATHS[1]));
                f.input_log[1].2 = TestInput::ArrangePartnership { woman: 1, man: 2 };
            }),
        ),
        (
            "a partnership counter one too high",
            partners_arranged().with(|f| f.next_partnership_id = 4),
        ),
        (
            "a partnership counter below a partnership",
            partners_arranged().with(|f| f.next_partnership_id = 2),
        ),
        (
            "a partnership with woman and man swapped",
            partners_arranged().with(|f| f.partnerships[1].1 = (3, 1, Y, None)),
        ),
        (
            "an open partnership of a sim in the archive",
            partners_arranged().with(|f| f.partnerships[0].1.3 = None),
        ),
        (
            "a partnership that ended a second after the death",
            partners_arranged().with(|f| f.partnerships[0].1.3 = Some(PARTNER_DEATHS[1] + 1)),
        ),
        (
            "an ended partnership of two living sims",
            partners_arranged().with(|f| f.partnerships[1].1.3 = Some(Y)),
        ),
        (
            "a partnership formed a second after its input acted",
            partners_arranged().with(|f| f.partnerships[1].1.2 = Y + 1),
        ),
        (
            "a partnership without its input",
            partners_arranged().with(|f| {
                f.input_log.pop();
            }),
        ),
        (
            "an input that paired another man",
            partners_arranged().with(|f| {
                f.input_log[1].2 = TestInput::ArrangePartnership { woman: 1, man: 2 };
            }),
        ),
        (
            "a founder couple missing from the start",
            partners_arranged().with(|f| f.input_log[0].2 = partners_start(Vec::new())),
        ),
        (
            "an archived woman with two conceptions",
            partners_after_year_4().with(|f| f.archive[0].1.7 = 2),
        ),
        (
            "an archived man with a conception",
            partners_after_year_4().with(|f| f.archive[1].1.7 = 1),
        ),
        (
            "a partnership ended at the later of the two deaths",
            partners_after_year_4().with(|f| f.partnerships[1].1.3 = Some(PARTNER_DEATHS[0])),
        ),
        (
            "a sim in two open partnerships",
            partners_arranged().with(|f| {
                f.partnerships.push((3, (1, 3, Y, None)));
                f.next_partnership_id = 4;
                f.input_log
                    .push((Y - 1, 3, TestInput::ArrangePartnership { woman: 1, man: 3 }));
            }),
        ),
    ];
    for (name, bytes) in &cases {
        assert_eq!(load_error(bytes), Some(LoadError::Inconsistent), "{name}");
    }
}

// Scenario "clan" of tools/state_reference.py: three founder couples and a fertility of
// 0.25 per month; the first woman conceives in month 5.
const CLAN_SAVE_AFTER_CONCEPTION: &str = "53494d4147454e54010ed19fd0ecc59ca3cc80010901ff010100018bbec9d8c5bd99888b017b14ae47e17a943f0001000000000000d03f122d0a80c8c40b07040ffe87ff1d000100018088ff1d8088ff1d00100101008088ff1d00110102008088ff1d00120103008088ff1d00130104008088ff1d00140105008088ff1d001501060080d8cd22001604010080b0fab301000f03030180a8a7029cc382bd02000b02058bbec9d8c5bd99888b0100d8c1a7c503000702038bbec9d8c5bd99888b0100e49ff1f30e000d02068bbec9d8c5bd99888b0100d8ba9ccb15000902048bbec9d8c5bd99888b0100da91aac119000302018bbec9d8c5bd99888b0100cabfeff02e000502028bbec9d8c5bd99888b0100170601ff9fedd704dded83919698d6d0d001000000000100000000000000010002ffafeb9305f6eb9bebea9bd7c53e010100000000000001000000000003ffdff4e70285b2a2a3e3ecdafac4010001000001000000000000010000000004ffefe38307f9a4bfd6bdf093fea701010000000000000001000000000005ffefd4e30afacbdcacb9abf18703000000000000000000000000000006ffefd4e30af1c5c4b2d9a691fded01010000000000000001000000000003010102000002030400000305060000010102f3c89ad6cdbbe183cd0100000100000100000000000080c8c40b000f01000b000c000d000e000f0010001180c8c40b09000600040007000500020003001401010100ff0109746573743a636c616e000109746573743a626173657b14ae47e17a943f0006ff9fedd70400010a636f72653a64756d6d79020001ffafeb930501010a636f72653a64756d6d79020100ffdff4e70200010a636f72653a64756d6d79020101ffefe383070100ffefd4e30a0000ffefd4e30a010003000102030405000000000000d03f122d0a00";
/// The lineage keys of the founders 0 to 5 of `test:clan` (tools/rng_reference.py).
const CLAN_KEYS: [u64; 6] = [
    0xd0a1_58c1_6220_f6dd,
    0x3e8b_5cde_ad66_f5f6,
    0xc4f5_6b66_3468_9905,
    0xa7fc_4f83_dacf_d279,
    0x030f_c55b_9597_25fa,
    0xedfa_4535_9651_22f1,
];
/// The child she conceived in month 5, and when each of the six dies of `test:base`.
const CLAN_CHILD: u64 = 0xcd07_85dc_dac6_a473;
const CLAN_DEATHS: [i64; 6] = [
    3_423_945_837,
    6_292_369_381,
    475_328_620,
    2_897_448_620,
    332_419_278,
    2_000_562_162,
];
const CLAN_FERTILITY: TestFertility = (0.25, 18, 45, 10);

fn clan_start() -> TestInput {
    let founder = |years: i64, sex: u8, genes: Alleles| (-years * Y, sex, genes);
    TestInput::Start {
        world_seed: 0xff,
        founder_tag: "test:clan".to_owned(),
        world_params: false,
        death_causes: vec![("test:base".to_owned(), 0.02, Everyone)],
        founders: vec![
            founder(20, FEMALE, Alleles(WILD, DUMMY)),
            founder(22, MALE, Alleles(DUMMY, WILD)),
            founder(12, FEMALE, Alleles(DUMMY, DUMMY)),
            founder(30, MALE, Alleles(WILD, WILD)),
            founder(46, FEMALE, Alleles(WILD, WILD)),
            founder(46, MALE, Alleles(WILD, WILD)),
        ],
        couples: vec![(0, 1), (2, 3), (4, 5)],
        fertility: CLAN_FERTILITY,
    }
}

/// The founders of "clan" as living sims: the girl has the dummy allele twice.
fn clan_sims() -> Vec<(u64, Living)> {
    let sim = |n: usize, years: i64, sex: u8, genes: Genes| {
        let exposure = (genes == Genes(sex, DUMMY, DUMMY)).then_some((0, None));
        (
            u64::try_from(n).expect("a small place") + 1,
            (-years * Y, CLAN_KEYS[n], sex, genes, exposure, 0, None),
        )
    };
    vec![
        sim(0, 20, FEMALE, Genes(FEMALE, WILD, DUMMY)),
        sim(1, 22, MALE, Genes(MALE, DUMMY, WILD)),
        sim(2, 12, FEMALE, Genes(FEMALE, DUMMY, DUMMY)),
        sim(3, 30, MALE, Genes(MALE, WILD, WILD)),
        sim(4, 46, FEMALE, Genes(FEMALE, WILD, WILD)),
        sim(5, 46, MALE, Genes(MALE, WILD, WILD)),
    ]
}

/// "clan" right after the conception in month 5: the first woman is pregnant with her
/// first conception (a girl with the dummy allele twice, born in month 15) and her
/// attempts have ended; the girl's attempts wait for her 18th birthday; the birthdays of
/// year 0 have planned those of year 1.
fn clan_conceived() -> File {
    let mut file = File::tiny();
    file.death_causes = vec![(BASE, 0.02, Everyone)];
    file.fertility = Some(CLAN_FERTILITY);
    file.closed_through = 5 * MONTH;
    file.next_sim_id = 7;
    file.next_partnership_id = 4;
    let birthday = |sim: u64, seq: u64| (Y, 0, seq, Payload::Birthday { sim }, None);
    let attempts = |woman: u64, time: i64, seq: u64| {
        let payload = Payload::ConceptionAttempt { woman };
        (time, 0, seq, payload, Some(MONTH))
    };
    file.queue = vec![
        (Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
        birthday(1, 16),
        birthday(2, 17),
        birthday(3, 18),
        birthday(4, 19),
        birthday(5, 20),
        birthday(6, 21),
        (15 * MONTH, 0, 22, Payload::Birth { mother: 1 }, None),
        attempts(3, 78 * MONTH, 15),
        (CLAN_DEATHS[4], 0, 11, death(5, BASE), None),
        (CLAN_DEATHS[2], 0, 7, death(3, BASE), None),
        (CLAN_DEATHS[5], 0, 13, death(6, BASE), None),
        (CLAN_DEATHS[3], 0, 9, death(4, BASE), None),
        (CLAN_DEATHS[0], 0, 3, death(1, BASE), None),
        (CLAN_DEATHS[1], 0, 5, death(2, BASE), None),
    ];
    file.next_event_seq = 23;
    file.sims = clan_sims();
    file.sims[0].1.5 = 1;
    file.partnerships = vec![
        (1, (1, 2, 0, None)),
        (2, (3, 4, 0, None)),
        (3, (5, 6, 0, None)),
    ];
    file.pregnancies = vec![(
        1,
        (
            2,
            CLAN_CHILD,
            0,
            FEMALE,
            Genes(FEMALE, DUMMY, DUMMY),
            5 * MONTH,
        ),
    )];
    file.plan_refs = vec![
        (1, 0),
        (11, 0),
        (12, 0),
        (13, 0),
        (14, 0),
        (15, 0),
        (16, 0),
        (17, 5 * MONTH),
        (9, 0),
        (6, 0),
        (4, 0),
        (7, 0),
        (5, 0),
        (2, 0),
        (3, 0),
    ];
    file.next_event_id = 20;
    file.input_log = vec![(-1, 1, clan_start())];
    file
}

/// The attempts of `woman` at `time`, with the plan of the entry `by`, put in their place
/// in the queue of `file` (the next insertion number).
fn with_attempts(file: &mut File, woman: u64, time: i64, by: u64) {
    let seq = file.next_event_seq;
    file.next_event_seq += 1;
    insert_event(
        file,
        (
            time,
            0,
            seq,
            Payload::ConceptionAttempt { woman },
            Some(MONTH),
        ),
        (by, 0),
    );
}

/// Puts an event with its plan into the queue of `file`, in the order of time and
/// insertion number (all events of these tests are general or births at distinct
/// seconds, or the same kind in one second).
fn insert_event(file: &mut File, entry: QueueEntry, plan: (u64, i64)) {
    let at = file
        .queue
        .iter()
        .position(|other| (other.0, other.2) > (entry.0, entry.2))
        .unwrap_or(file.queue.len());
    file.queue.insert(at, entry);
    file.plan_refs.insert(at, plan);
}

/// Takes the event of `file` at `index` out, with its plan.
fn take_event(file: &mut File, index: usize) -> (QueueEntry, (u64, i64)) {
    (file.queue.remove(index), file.plan_refs.remove(index))
}

/// The save of "clan" from the reference and the test encoder are the same bytes, and
/// it loads.
#[test]
fn a_save_keeps_the_pregnancy() {
    assert_eq!(clan_conceived().bytes(), hex(CLAN_SAVE_AFTER_CONCEPTION));
    assert_eq!(load_error(&clan_conceived().bytes()), None);
}

const CLAN_SAVE_AFTER_BIRTH: &str = "53494d4147454e54010ed19fd0ecc59ca3cc80010901ff010100018bbec9d8c5bd99888b017b14ae47e17a943f0001000000000000d03f122d0a80d8cd220804118080f524001f03010180a8a702fe8ffe3b000100018088ff1d8090fe3b00170101008090fe3b00180102008090fe3b00190103008090fe3b001a0104008090fe3b001b0105008090fe3b001c01060080e0cc40001d01070080b0fab301000f03030180a8a7029cc382bd02000b02058bbec9d8c5bd99888b0100d8c1a7c503000702038bbec9d8c5bd99888b01009889dff403001e02078bbec9d8c5bd99888b0100e49ff1f30e000d02068bbec9d8c5bd99888b0100d8ba9ccb15000902048bbec9d8c5bd99888b0100da91aac119000302018bbec9d8c5bd99888b0100cabfeff02e000502028bbec9d8c5bd99888b0100200701ff9fedd704dded83919698d6d0d001000000000100000000000000010002ffafeb9305f6eb9bebea9bd7c53e010100000000000001000000000003ffdff4e70285b2a2a3e3ecdafac4010001000001000000000000010000000004ffefe38307f9a4bfd6bdf093fea701010000000000000001000000000005ffefd4e30afacbdcacb9abf18703000000000000000000000000000006ffefd4e30af1c5c4b2d9a691fded0101000000000000000100000000000780d8cd22f3c89ad6cdbbe183cd0100010000010000000000000100000001010200030101020000020304000003050600000000111a80d8cd220100148088ff1d158088ff1d168088ff1d178088ff1d188088ff1d198088ff1d1a80d8cd220900060004001a80d8cd2207000500020003001b01010100ff0109746573743a636c616e000109746573743a626173657b14ae47e17a943f0006ff9fedd70400010a636f72653a64756d6d79020001ffafeb930501010a636f72653a64756d6d79020100ffdff4e70200010a636f72653a64756d6d79020101ffefe383070100ffefd4e30a0000ffefd4e30a010003000102030405000000000000d03f122d0a0100ddb0c9caaa95c7d4da01";
/// When the child (sim 7) dies of `test:base`.
const CHILD_DEATH: i64 = 525_066_828;

/// "clan" right after the birth in month 15: the child (sim 7) lives with its parents and
/// the dummy allele twice, its first birthday and its death are planned by the birth
/// (entry 26), and so are the mother's attempts from month 16 on.
fn clan_born() -> File {
    let mut file = clan_conceived();
    file.closed_through = 15 * MONTH;
    file.next_sim_id = 8;
    let birthday = |sim: u64, seq: u64, by: u64| {
        let entry = (2 * Y, 0, seq, Payload::Birthday { sim }, None);
        (entry, (by, Y))
    };
    let events = vec![
        (
            (
                16 * MONTH,
                0,
                31,
                Payload::ConceptionAttempt { woman: 1 },
                Some(MONTH),
            ),
            (26, 15 * MONTH),
        ),
        ((2 * Y - 1, 0, 1, Payload::YearEnd, Some(Y)), (1, 0)),
        birthday(1, 23, 20),
        birthday(2, 24, 21),
        birthday(3, 25, 22),
        birthday(4, 26, 23),
        birthday(5, 27, 24),
        birthday(6, 28, 25),
        (
            (15 * MONTH + Y, 0, 29, Payload::Birthday { sim: 7 }, None),
            (26, 15 * MONTH),
        ),
        (
            (
                78 * MONTH,
                0,
                15,
                Payload::ConceptionAttempt { woman: 3 },
                Some(MONTH),
            ),
            (9, 0),
        ),
        ((CLAN_DEATHS[4], 0, 11, death(5, BASE), None), (6, 0)),
        ((CLAN_DEATHS[2], 0, 7, death(3, BASE), None), (4, 0)),
        ((CHILD_DEATH, 0, 30, death(7, BASE), None), (26, 15 * MONTH)),
        ((CLAN_DEATHS[5], 0, 13, death(6, BASE), None), (7, 0)),
        ((CLAN_DEATHS[3], 0, 9, death(4, BASE), None), (5, 0)),
        ((CLAN_DEATHS[0], 0, 3, death(1, BASE), None), (2, 0)),
        ((CLAN_DEATHS[1], 0, 5, death(2, BASE), None), (3, 0)),
    ];
    (file.queue, file.plan_refs) = events.into_iter().unzip();
    file.next_event_seq = 32;
    file.sims.push((
        7,
        (
            15 * MONTH,
            CLAN_CHILD,
            FEMALE,
            Genes(FEMALE, DUMMY, DUMMY),
            Some((0, None)),
            0,
            Some((1, 2, 0)),
        ),
    ));
    file.pregnancies.clear();
    file.next_event_id = 27;
    file.year_hashes = vec![(0, 0xdaa9_1caa_a952_585d)];
    file
}

/// The save of "clan" after the birth from the reference and the test encoder are the
/// same bytes, and it loads.
#[test]
fn a_save_keeps_the_child() {
    assert_eq!(clan_born().bytes(), hex(CLAN_SAVE_AFTER_BIRTH));
    assert_eq!(load_error(&clan_born().bytes()), None);
}

/// A born sim is the child the rule gives: its parents in the living or the archive, born
/// as many months after a conception the rule allows as a pregnancy lasts, its key from
/// both parents and the mother's number, its alleles and sex from the draws, an exposure
/// from its birth on; the mother's count takes it in; the IDs of the born follow the
/// founders without a gap. ADR-0006 section 5.
#[test]
fn load_checks_the_born() {
    let cases: [(&str, Vec<u8>); 11] = [
        (
            "a second conception while she was pregnant with the first child",
            clan_born().with(|f| {
                // The draw of month 12 succeeds (tools/rng_reference.py).
                f.pregnancies = vec![(
                    1,
                    (
                        2,
                        0xe6f8_1c92_e297_9324,
                        1,
                        MALE,
                        Genes(MALE, WILD, WILD),
                        12 * MONTH,
                    ),
                )];
                f.sims[0].1.5 = 2;
                take_event(f, 0);
                let seq = f.next_event_seq;
                f.next_event_seq += 1;
                insert_event(
                    f,
                    (22 * MONTH, 0, seq, Payload::Birth { mother: 1 }, None),
                    (17, 12 * MONTH),
                );
            }),
        ),
        (
            "a child with another genotype",
            clan_born().with(|f| {
                f.sims[6].1.3 = Genes(f.sims[6].1.2, DUMMY, WILD);
                f.sims[6].1.4 = None;
            }),
        ),
        (
            "a child with another sex",
            clan_born().with(|f| f.sims[6].1.2 = MALE),
        ),
        (
            "a child without its exposure",
            clan_born().with(|f| f.sims[6].1.4 = None),
        ),
        (
            "a child as the mother's second conception",
            clan_born().with(|f| f.sims[6].1.6 = Some((1, 2, 1))),
        ),
        (
            "a child with the man as its mother",
            clan_born().with(|f| f.sims[6].1.6 = Some((2, 1, 0))),
        ),
        (
            "a child born in the second that is still open, her attempts from then on",
            clan_born().with(|f| {
                f.closed_through = 15 * MONTH - 1;
                f.queue[0].0 = 15 * MONTH;
            }),
        ),
        (
            "a mother whose count misses her child",
            clan_born().with(|f| f.sims[0].1.5 = 0),
        ),
        (
            "a mother whose count has one conception too many",
            clan_born().with(|f| f.sims[0].1.5 = 2),
        ),
        (
            "a sim counter with a gap after the child",
            clan_born().with(|f| f.next_sim_id = 9),
        ),
        (
            "the mother's attempts a month late",
            clan_born().with(|f| f.queue[0].0 += MONTH),
        ),
    ];
    for (name, bytes) in &cases {
        assert_eq!(load_error(bytes), Some(LoadError::Inconsistent), "{name}");
    }
}

/// A pregnancy is the one the conception rule gives: conceived at a month start in a
/// closed second by a partnership open then, with a successful draw, the mother old
/// enough; the child's key from both parents and her conception number, its alleles and
/// sex from the draws; her count of conceptions takes it in; its birth is planned. The
/// attempts are exactly those of women in an open partnership who can conceive, monthly,
/// from the first month start the rule allows. ADR-0006 section 5.
#[test]
fn load_checks_conceptions_and_pregnancies() {
    // In the model the birth is at index 7 and the girl's attempts at index 8.
    let moved_birth = |f: &mut File, to: i64| {
        let (mut birth, plan) = take_event(f, 7);
        birth.0 = to;
        insert_event(f, birth, plan);
    };
    let cases: [(&str, Vec<u8>); 23] = [
        // Each of these fails at exactly one check (keys and draws from
        // tools/rng_reference.py).
        (
            "a pregnancy conceived inside a month, in a closed second",
            clan_conceived().with(|f| {
                f.closed_through = 5 * MONTH + 10;
                f.pregnancies[0].1.5 = 5 * MONTH + 1;
                moved_birth(f, 15 * MONTH + 1);
            }),
        ),
        (
            "a pregnancy as a conception before her latest, with its own child",
            clan_conceived().with(|f| {
                f.pregnancies[0].1 = (
                    2,
                    0xe6f8_1c92_e297_9324,
                    1,
                    MALE,
                    Genes(MALE, WILD, WILD),
                    5 * MONTH,
                );
            }),
        ),
        (
            "a pregnancy by a man she is not paired with",
            clan_conceived().with(|f| {
                f.pregnancies[0].1 = (
                    4,
                    0x4645_df5a_e961_ca02,
                    0,
                    FEMALE,
                    Genes(FEMALE, WILD, WILD),
                    MONTH,
                );
                moved_birth(f, 11 * MONTH);
            }),
        ),
        (
            "a pregnancy of the girl before her 18th birthday",
            clan_conceived().with(|f| {
                f.pregnancies.push((
                    3,
                    (
                        4,
                        0x0d0d_06d7_b21d_4bc0,
                        0,
                        FEMALE,
                        Genes(FEMALE, DUMMY, WILD),
                        0,
                    ),
                ));
                f.sims[2].1.5 = 1;
                take_event(f, 8);
                let seq = f.next_event_seq;
                f.next_event_seq += 1;
                insert_event(
                    f,
                    (10 * MONTH, 0, seq, Payload::Birth { mother: 3 }, None),
                    (17, 0),
                );
            }),
        ),
        (
            "two series of attempts for the girl",
            clan_conceived().with(|f| with_attempts(f, 3, 78 * MONTH, 9)),
        ),
        (
            "a pregnancy without its birth",
            clan_conceived().with(|f| {
                take_event(f, 7);
            }),
        ),
        (
            "a birth a month after the pregnancy ends",
            clan_conceived().with(|f| moved_birth(f, 16 * MONTH)),
        ),
        (
            "a birth without a pregnancy, her attempts going on",
            clan_conceived().with(|f| {
                f.pregnancies.clear();
                f.sims[0].1.5 = 0;
                with_attempts(f, 1, 6 * MONTH, 8);
            }),
        ),
        (
            "a fertility unlike that of the start",
            clan_conceived().with(|f| f.fertility = Some((0.5, 18, 45, 10))),
        ),
        (
            "a pregnancy with another genotype",
            clan_conceived()
                .with(|f| f.pregnancies[0].1.4 = Genes(f.pregnancies[0].1.3, WILD, DUMMY)),
        ),
        (
            "a pregnancy with another sex",
            clan_conceived().with(|f| f.pregnancies[0].1.3 = MALE),
        ),
        (
            "a pregnancy with another lineage key",
            clan_conceived().with(|f| f.pregnancies[0].1.1 = CLAN_CHILD + 1),
        ),
        (
            "a pregnancy conceived in a month whose draw failed",
            clan_conceived().with(|f| {
                f.pregnancies[0].1.5 = 4 * MONTH;
                moved_birth(f, 14 * MONTH);
            }),
        ),
        (
            "a pregnancy conceived between two month starts",
            clan_conceived().with(|f| {
                f.pregnancies[0].1.5 = 5 * MONTH - 1;
                moved_birth(f, 15 * MONTH - 1);
            }),
        ),
        (
            "a pregnancy conceived in the second that is still open",
            clan_conceived().with(|f| f.closed_through = 5 * MONTH - 1),
        ),
        (
            "a pregnancy as her second conception",
            clan_conceived().with(|f| {
                f.pregnancies[0].1.2 = 1;
                f.sims[0].1.5 = 2;
            }),
        ),
        (
            "a pregnancy her count does not take in",
            clan_conceived().with(|f| f.sims[0].1.5 = 0),
        ),
        (
            "a conception counted without a pregnancy, her attempts going on",
            clan_conceived().with(|f| {
                f.pregnancies.clear();
                take_event(f, 7);
                with_attempts(f, 1, 6 * MONTH, 8);
            }),
        ),
        (
            "attempts of a pregnant woman",
            clan_conceived().with(|f| with_attempts(f, 1, 6 * MONTH, 8)),
        ),
        (
            "no attempts for the girl",
            clan_conceived().with(|f| {
                take_event(f, 8);
            }),
        ),
        (
            "the girl's attempts before her 18th birthday",
            clan_conceived().with(|f| {
                let (mut early, plan) = take_event(f, 8);
                early.0 = 6 * MONTH;
                insert_event(f, early, plan);
            }),
        ),
        (
            "the girl's attempts without their period",
            clan_conceived().with(|f| f.queue[8].4 = None),
        ),
        (
            "attempts for the woman of 46",
            clan_conceived().with(|f| with_attempts(f, 5, 6 * MONTH, 10)),
        ),
    ];
    for (name, bytes) in &cases {
        assert_eq!(load_error(bytes), Some(LoadError::Inconsistent), "{name}");
    }
}

/// A started world without sims at `closed_through`, with its year end due at `due`.
fn empty_started(closed_through: i64, due: i64) -> File {
    let mut file = File::tiny();
    file.closed_through = closed_through;
    file.queue = vec![(due, 0, 1, Payload::YearEnd, Some(Y))];
    file.plan_refs = vec![(1, 0)];
    file.next_event_seq = 2;
    file.sims.clear();
    file.next_sim_id = 1;
    file.next_event_id = 2;
    start_with_founders(&mut file, Vec::new());
    file
}

/// A world that never started.
fn never_started() -> File {
    let mut file = File::tiny();
    file.world_seed = None;
    file.world_params = None;
    file.fertility = None;
    file.queue.clear();
    file.plan_refs.clear();
    file.next_event_seq = 1;
    file.sims.clear();
    file.next_sim_id = 1;
    file.next_event_id = 1;
    file.input_log.clear();
    file
}

fn start_with_founders(file: &mut File, born: Vec<i64>) {
    file.input_log[0].2 = TestInput::Start {
        world_seed: 0xff,
        founder_tag: "test:tiny".to_owned(),
        world_params: false,
        death_causes: Vec::new(),
        founders: born.into_iter().map(test_founder).collect(),
        couples: Vec::new(),
        fertility: BARREN,
    };
}

/// ADR-0006 section 5: one case per invariant.
#[test]
fn load_rejects_inconsistent_saves() {
    let limit = (1_u64 << 53) - 1;
    let birthday = |sim: u64| Payload::Birthday { sim };
    let two_years = || empty_started(2 * Y - 1, 3 * Y - 1);
    let cases: Vec<(&str, Vec<u8>)> = vec![
        // Counters.
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
        // The clock never stands before the second before the start.
        (
            "an unstarted world with the clock before the start",
            never_started().with(|f| f.closed_through = -2),
        ),
        (
            "a started world with the clock before the start",
            File::tiny().with(|f| {
                f.closed_through = -2;
                f.input_log[0].0 = -2;
            }),
        ),
        // Never started: the world is empty.
        (
            "unstarted with a sim, the counter untouched",
            never_started().with(|f| f.sims = vec![(1, living(0, -1))]),
        ),
        (
            "unstarted with an archive, the counter untouched",
            never_started().with(|f| f.archive = vec![(1, archived(0, -9, -2))]),
        ),
        (
            "unstarted with an input",
            never_started().with(|f| f.input_log = File::tiny().input_log),
        ),
        (
            "unstarted with a sim",
            never_started().with(|f| {
                f.sims = vec![(1, living(0, -1))];
                f.next_sim_id = 2;
            }),
        ),
        (
            "unstarted with an archive",
            never_started().with(|f| {
                f.archive = vec![(1, archived(0, -9, -2))];
                f.next_sim_id = 2;
            }),
        ),
        (
            "unstarted with a used sim counter",
            never_started().with(|f| f.next_sim_id = 2),
        ),
        (
            "unstarted with a used partnership counter",
            never_started().with(|f| f.next_partnership_id = 2),
        ),
        (
            "unstarted with a partnership but an unused counter",
            never_started().with(|f| f.partnerships = vec![(1, (1, 2, 0, None))]),
        ),
        (
            "unstarted with a used event counter",
            never_started().with(|f| f.next_event_id = 2),
        ),
        (
            "unstarted with a used insertion counter",
            never_started().with(|f| f.next_event_seq = 2),
        ),
        (
            "unstarted with the insertion counter 0",
            never_started().with(|f| f.next_event_seq = 0),
        ),
        (
            "unstarted with an event",
            never_started().with(|f| {
                f.queue = vec![(Y - 1, 0, 1, Payload::YearEnd, Some(Y))];
                f.plan_refs = vec![(1, 0)];
                f.next_event_seq = 2;
                f.next_event_id = 2;
            }),
        ),
        (
            "unstarted with a year hash",
            never_started().with(|f| {
                f.closed_through = Y;
                f.year_hashes = vec![(0, 7)];
            }),
        ),
        // Started: the input log.
        (
            "a seed without a start",
            File::tiny().with(|f| f.input_log.clear()),
        ),
        (
            "a start without a seed",
            File::tiny().with(|f| f.world_seed = None),
        ),
        (
            "a start with another seed",
            File::tiny().with(|f| f.world_seed = Some(0xfe)),
        ),
        (
            "input numbers not from 1",
            File::tiny().with(|f| f.input_log[0].1 = 2),
        ),
        (
            "a start stamped after the clock",
            File::tiny().with(|f| f.input_log[0].0 = 0),
        ),
        (
            "a start not stamped -1",
            File::tiny().with(|f| f.input_log[0].0 = -2),
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
            "a start the input check rejects",
            File::tiny().with(|f| {
                // A founder born at the start; everything else fits such a founder.
                f.sims = vec![(1, living(0, 0))];
                f.queue = vec![
                    (Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
                    (Y, 0, 2, birthday(1), None),
                ];
                f.plan_refs = vec![(1, 0), (2, 0)];
                start_with_founders(f, vec![0]);
            }),
        ),
        // Sims and founders.
        (
            "a sim id not below the counter",
            File::tiny().with(|f| f.next_sim_id = 1),
        ),
        (
            "a sim counter ahead of the founders",
            File::tiny().with(|f| f.next_sim_id = 3),
        ),
        (
            "a living sim beyond the counter, with its birthday",
            File::tiny().with(|f| {
                f.sims.push((5, living(1, -1)));
                f.queue.insert(1, (Y - 1, 0, 3, birthday(5), None));
                f.plan_refs.insert(1, (2, 0));
                f.next_event_seq = 4;
            }),
        ),
        (
            "an archived id not below the counter",
            File::tiny().with(|f| f.archive = vec![(2, archived(1, -9, -2))]),
        ),
        (
            "an id living and archived",
            File::tiny().with(|f| f.archive = vec![(1, archived(0, -1, 0))]),
        ),
        (
            "a founder with another birth time",
            File::tiny().with(|f| start_with_founders(f, vec![-2])),
        ),
        (
            "a founder with another sex",
            File::tiny().with(|f| f.sims[0].1.2 = MALE),
        ),
        (
            "a founder with another allele from the mother",
            File::tiny().with(|f| f.sims[0].1.3.1 = DUMMY),
        ),
        (
            "a founder with another allele from the father",
            File::tiny().with(|f| f.sims[0].1.3.2 = WILD),
        ),
        (
            "a founder with the lineage key of another place in the list",
            File::tiny().with(|f| f.sims[0].1.1 = TINY_KEYS[1]),
        ),
        (
            "a founder with a lineage key of no founder",
            File::tiny().with(|f| f.sims[0].1.1 += 1),
        ),
        (
            "a sim without a founder",
            File::tiny().with(|f| start_with_founders(f, Vec::new())),
        ),
        (
            "a founder without a sim",
            File::tiny().with(|f| start_with_founders(f, vec![-1, -1])),
        ),
        // The queue.
        (
            "a birthday for a sim that does not live",
            File::tiny().with(|f| f.queue[0].3 = birthday(2)),
        ),
        (
            "a periodic birthday",
            File::tiny().with(|f| f.queue[0].4 = Some(Y)),
        ),
        (
            "a birthday off the anniversary",
            File::tiny().with(|f| f.queue[0].0 = Y - 2),
        ),
        (
            "a birthday a year too late",
            File::tiny().with(|f| {
                f.queue = vec![
                    (Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
                    (2 * Y - 1, 0, 2, birthday(1), None),
                ];
                f.plan_refs = vec![(1, 0), (2, 0)];
            }),
        ),
        (
            "a living sim without a birthday",
            File::tiny().with(|f| {
                f.queue.remove(0);
                f.plan_refs.remove(0);
            }),
        ),
        (
            "two birthdays for one sim",
            File::tiny().with(|f| {
                f.queue.insert(1, (Y - 1, 0, 3, birthday(1), None));
                f.plan_refs.insert(1, (2, 0));
                f.next_event_seq = 4;
            }),
        ),
        (
            "no year end",
            File::tiny().with(|f| {
                f.queue.remove(1);
                f.plan_refs.remove(1);
            }),
        ),
        (
            "a year end without a period",
            File::tiny().with(|f| f.queue[1].4 = None),
        ),
        (
            "a year end every day",
            File::tiny().with(|f| f.queue[1].4 = Some(DAY)),
        ),
        ("a year end one second late", empty_started(-1, Y).bytes()),
        (
            "a year end a year late",
            empty_started(-1, 2 * Y - 1).bytes(),
        ),
        (
            "two year ends",
            empty_started(-1, Y - 1).with(|f| {
                f.queue.push((Y - 1, 0, 2, Payload::YearEnd, Some(Y)));
                f.plan_refs.push((1, 0));
                f.next_event_seq = 3;
            }),
        ),
        // Companion data.
        (
            "a plan reference to an entry that does not exist yet",
            File::tiny().with(|f| f.plan_refs[0].0 = 3),
        ),
        (
            "a plan made after now",
            File::tiny().with(|f| f.plan_refs[0].1 = 1),
        ),
        // Year hashes: exactly the completed years.
        (
            "a year hash for a year that has not ended",
            File::tiny().with(|f| f.year_hashes = vec![(0, 7)]),
        ),
        (
            "a missing year hash",
            two_years().with(|f| f.year_hashes = vec![(0, 7)]),
        ),
        (
            "year hashes out of order",
            two_years().with(|f| f.year_hashes = vec![(1, 7), (0, 7)]),
        ),
        (
            "year hashes that start at year 1",
            two_years().with(|f| f.year_hashes = vec![(1, 7), (2, 7)]),
        ),
    ];
    for (name, bytes) in cases {
        assert_eq!(load_error(&bytes), Some(LoadError::Inconsistent), "{name}");
    }
    // The base cases of the helpers are valid, so each case above fails for its reason.
    assert_eq!(load_error(&never_started().bytes()), None);
    assert_eq!(
        load_error(&never_started().with(|f| f.closed_through = 3 * Y)),
        None
    );
    assert_eq!(load_error(&empty_started(-1, Y - 1).bytes()), None);
    let complete = two_years().with(|f| f.year_hashes = vec![(0, 7), (1, 7)]);
    assert_eq!(load_error(&complete), None);
    let one_second_earlier =
        empty_started(2 * Y - 2, 2 * Y - 1).with(|f| f.year_hashes = vec![(0, 7)]);
    assert_eq!(load_error(&one_second_earlier), None);
    // An exhausted EventId counter stands at 2^53: loadable, the next entry breaks as in
    // the uninterrupted run.
    let exhausted = File::tiny().with(|f| f.next_event_id = limit + 1);
    assert_eq!(load_error(&exhausted), None);
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
            if let Ok((mut loaded, years)) = load(&bytes) {
                prop_assert_eq!(loaded.save(&years).expect("saves"), bytes);
                // And it runs on like any world.
                let closed = loaded.closed_through().as_secs();
                if closed < 100 * Y {
                    let mut out = YearOut::default();
                    loaded.advance_until(&mut out, t(closed + Y), &mut |_| false);
                    let started = loaded.state().world_seed().is_some();
                    prop_assert_eq!(out.year_hashes.len(), usize::from(started));
                }
            }
        }
    }
}

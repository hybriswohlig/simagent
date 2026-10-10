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
use sim_core::data_id::DataId;
use sim_core::finite::FiniteF64;
use sim_core::genome::Sex;
use sim_core::input::{Fertility, ForkOverride, Founder, Input, Start};
use sim_core::log::{LogEntry, log_hash};
use sim_core::mortality::{StepHazard, disease_hazard_used};
use sim_core::params::{NutritionYear, ParamChange, WorldParams};
use sim_core::time::{SimDuration, SimTime};
use sim_core::version::CORE_VERSION;
use sim_core::world::{FORMAT_VERSION, LoadError, SaveError, World, YearOut};

/// The data hash of the definitions of the repository, from `tools/data_reference.py`
/// (ADR-0007).
const DATA_HASH: u64 = 0x5bb6_ebe1_6061_394b;
const Y: i64 = 31_449_600;
const MONTH: i64 = 2_419_200;
const DAY: i64 = 86_400;

// tools/state_reference.py
const HEADER: &str = "53494d4147454e540118cbf2848396fcbadb5b";
const TINY_SAVE_AFTER_START: &str = "53494d4147454e540118cbf2848396fcbadb5b0c01ff01010000010000000000000000122d0a01020104fe87ff1d0002010100fe87ff1d000100018088ff1dd0a5dcfc1300030201a0bb8ca1c58498a01c00b8f4e5ff8c1f0004020185d2bc82f8b2ead44d00050101019893ebf7f7b1d2ec5e00000000010000000000000000000000000402000100020002000301010100ff0109746573743a74696e790000010100010f636f72653a646973656173655f6172020001000000000000000000122d0a00";
const TINY_SAVE_OVERRIDDEN: &str = "53494d4147454e540118cbf2848396fcbadb5b0c01ff01010100010000000000000000122d0a01020104fe87ff1d0002010100fe87ff1d000100018088ff1dd0a5dcfc1300030201a0bb8ca1c58498a01c00b8f4e5ff8c1f0004020185d2bc82f8b2ead44d00050101019893ebf7f7b1d2ec5e00000000010000000000000000000000000402000100020002000402010100ff0109746573743a74696e790000010100010f636f72653a646973656173655f6172020001000000000000000000122d0a01020101000100";
const EMPTY_SAVE: &str = "53494d4147454e540118cbf2848396fcbadb5b0c00000001010100010000000000010000";

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
            nutrition: Vec::new(),
        },
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
    load_error_with(common::data(), bytes)
}

/// `load_error` on the definitions `data`.
fn load_error_with(data: Data, bytes: &[u8]) -> Option<LoadError> {
    match World::load(bytes, data) {
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

/// `str_key` of `core:senescence` and `core:accident`, the causes of death of everyone
/// (spec 7.1, `tools/state_reference.py`).
const SENESCENCE: u64 = 0x1c40_6024_5423_1da0;
const ACCIDENT: u64 = 0x4da9_a997_804f_2905;

fn death(sim: u64, cause: u64) -> Payload {
    Payload::Death { sim, cause }
}

/// `WorldParams` as postcard writes it: the switch, then the nutrition years.
type TestParams = (bool, Vec<(i64, f64)>);

/// The world parameters with the switch `on` and no nutrition year.
fn params(on: bool) -> TestParams {
    (on, Vec::new())
}

#[derive(Clone, Serialize)]
enum TestInput {
    Start {
        world_seed: u64,
        founder_tag: String,
        /// The world parameters: the switch of the hereditary diseases and the nutrition
        /// index as (year, index) pairs (ADR-0013).
        world_params: TestParams,
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
    /// (year, index), variant 1 (ADR-0013).
    NutritionYear(i64, f64),
}

/// An override block that sets the switch of the hereditary diseases.
fn override_block(on: bool) -> TestInput {
    TestInput::ForkOverride {
        changes: vec![TestChange::HereditaryDiseases(on)],
    }
}

/// The lineage keys of the founders 0 and 1 of `test:tiny` (tools/rng_reference.py).
const TINY_KEYS: [u64; 2] = [0x5ed9_498f_7efa_c998, 0xca85_f610_f1bb_1e89];
/// When the founders 0 and 1 of `test:tiny`, born a second before the start, die of
/// senescence and of accident on the repository's data (`tools/state_reference.py`).
const TINY_DEATHS: [(i64, i64); 2] = [
    (2_680_916_328, 534_320_561_436),
    (2_857_565_957, 92_384_845_286),
];
/// Variant indices of the sex and the alleles (tests/golden/log_codes.txt).
const FEMALE: u8 = 0;
const MALE: u8 = 1;
const WILD: u8 = 0;
const DUMMY: u8 = 1;

/// The alleles of a founder at `core:disease_ar`, maternal and paternal, written as the
/// input log lists them: no entry for two reference alleles, else the locus with both
/// copies (tools/postcard_reference.py).
#[derive(Clone, Copy, PartialEq, Eq)]
struct Alleles(u8, u8);

impl Serialize for Alleles {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let listed: Vec<(&str, Vec<u8>)> = if (self.0, self.1) == (0, 0) {
            Vec::new()
        } else {
            vec![("core:disease_ar", vec![self.0, self.1])]
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

/// A genome whose only alternative alleles are at `core:disease_ar`: the sex, then the alleles
/// maternal and paternal. Written as the core writes `Genome`: three words per autosome
/// copy (`core:disease_ar` is bit 0 of the first), the X, the sex chromosome from the father,
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
/// (used, since): the hazard the closed periods of the switch used, and since when the
/// disease acts now.
type TestExposure = (f64, Option<i64>);
/// (disease, onset, exposure): a hereditary disease a sim has (ADR-0017 section 3), the
/// key of its code, the onset in years, an exposure if it is lethal.
type TestAffliction = (u64, f64, Option<TestExposure>);

/// The `str_key` of `core:disease_ar`, the disease of the homozygous on the repository's
/// data (cystic fibrosis, onset 0).
fn ar_key() -> u64 {
    sim_core::data_id::str_key(&DataId::new("core:disease_ar").expect("id"))
}

/// The affliction of a homozygous sim on the repository's data with this exposure.
fn ar(exposure: TestExposure) -> Vec<TestAffliction> {
    vec![(ar_key(), 0.0, Some(exposure))]
}
/// (mother, father, conception)
type TestParents = (u64, u64, u32);
/// (born, lineage_key, sex, genome, afflictions, conceptions, parents)
type Living = (
    i64,
    u64,
    u8,
    Genes,
    Vec<TestAffliction>,
    u32,
    Option<TestParents>,
);
/// Why a sim left: it died of the cause with this key.
#[derive(Clone, Copy, Serialize)]
enum TestLeftBy {
    Death { cause: u64 },
}

/// (born, left, reason, lineage_key, sex, genome, afflictions, conceptions, parents)
type Archived = (
    i64,
    i64,
    TestLeftBy,
    u64,
    u8,
    Genes,
    Vec<TestAffliction>,
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
        Vec::new(),
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
        Vec::new(),
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
    /// The world parameters; `None` before the start.
    world_params: Option<TestParams>,
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
    /// "tiny" right after `Start`: her birthday, the year end, her deaths of senescence
    /// and of accident.
    fn tiny() -> File {
        let (senescence, accident) = TINY_DEATHS[0];
        File {
            magic: *b"SIMAGENT",
            format_version: 1,
            core_version: 24,
            data_hash: DATA_HASH,
            projection_version: 12,
            world_seed: Some(0xff),
            world_params: Some(params(false)),
            fertility: Some(BARREN),
            closed_through: -1,
            next_sim_id: 2,
            next_partnership_id: 1,
            queue: vec![
                (Y - 1, 0, 2, Payload::Birthday { sim: 1 }, None),
                (Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
                (senescence, 0, 3, death(1, SENESCENCE), None),
                (accident, 0, 4, death(1, ACCIDENT), None),
            ],
            next_event_seq: 5,
            sims: vec![(1, living(0, -1))],
            partnerships: Vec::new(),
            pregnancies: Vec::new(),
            archive: Vec::new(),
            plan_refs: vec![(2, 0), (1, 0), (2, 0), (2, 0)],
            next_event_id: 3,
            input_log: vec![(
                -1,
                1,
                TestInput::Start {
                    world_seed: 0xff,
                    founder_tag: "test:tiny".to_owned(),
                    world_params: params(false),
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
    assert_eq!(CORE_VERSION, 24);
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
    for version in [23, 25] {
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
        f.core_version = 25;
        f.data_hash = 7;
        f.projection_version = 13;
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
        f.core_version = 24;
    });
    assert_eq!(load_error(&bytes), Some(LoadError::ForeignDataHash));
    // The projection version is read before the fields whose layout it decides.
    let mut bytes = File::tiny().with(|f| f.projection_version = 13);
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
    for version in [11, 13] {
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
    // Behind the header: version, seed (3 bytes), world parameters (3 bytes: the switch
    // and no nutrition year), the fertility (12 bytes), clock, sim counter, partnership
    // counter, queue length, then the first entry with its time (4 bytes), micro step and
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
            let [(her_senescence, her_accident), (senescence, accident)] = TINY_DEATHS;
            f.next_sim_id = 3;
            f.sims = sims;
            f.queue = vec![
                (Y - 1, 0, 2, Payload::Birthday { sim: 1 }, None),
                (Y - 1, 0, 5, Payload::Birthday { sim: 2 }, None),
                (Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
                (her_senescence, 0, 3, death(1, SENESCENCE), None),
                (senescence, 0, 6, death(2, SENESCENCE), None),
                (accident, 0, 7, death(2, ACCIDENT), None),
                (her_accident, 0, 4, death(1, ACCIDENT), None),
            ];
            f.next_event_seq = 8;
            f.plan_refs = vec![(2, 0), (3, 0), (1, 0), (2, 0), (3, 0), (3, 0), (2, 0)];
            f.next_event_id = 4;
            f.input_log[0].2 = TestInput::Start {
                world_seed: 0xff,
                founder_tag: "test:tiny".to_owned(),
                world_params: params(false),
                founders: vec![test_founder(-1), test_founder(-1)],
                couples: Vec::new(),
                fertility: BARREN,
            };
        })
    };
    let (first, second) = ((1, living(0, -1)), (2, living(1, -1)));
    assert_eq!(load_error(&two(vec![first.clone(), second.clone()])), None);
    assert_eq!(
        load_error(&two(vec![second, first])),
        Some(LoadError::Malformed)
    );
    // The same value as a varint of two bytes: 0x82 0x00 is 2, like 0x02.
    let good = File::tiny().bytes();
    // Behind the header: version, seed (3 bytes), world parameters (3 bytes: the switch
    // and no nutrition year), the fertility (12 bytes), clock.
    let at = hex(HEADER).len() + 20;
    assert_eq!(good[at], 2, "next_sim_id of the tiny world");
    let mut overlong = good.clone();
    overlong.splice(at..=at, [0x82, 0x00]);
    assert_eq!(load_error(&overlong), Some(LoadError::Malformed));
    // Length prefixes far beyond the input: rejected without reading that much. The
    // partnership counter lies between.
    assert_eq!(good[at + 1], 1, "next_partnership_id of the tiny world");
    let queue_len = at + 2;
    assert_eq!(good[queue_len], 4, "four pending events");
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
            File::tiny().with(|f| f.next_event_seq = 4),
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
    file.world_params = Some(params(true));
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
            hereditary_diseases: true,
            nutrition: Vec::new(),
        })
    );
    assert_eq!(loaded.input_log(), world.input_log());
    assert_eq!(loaded.state_hash(), world.state_hash());
}

/// ADR-0013: the nutrition years an override set are kept by a save and checked by the
/// load against the input log; a projection with a year the log never set is rejected.
#[test]
fn a_save_keeps_the_nutrition_years() {
    let mut world = tiny();
    let fed = |year, index| {
        ParamChange::NutritionYear(NutritionYear {
            year,
            index: FiniteF64::try_new(index).expect("finite"),
        })
    };
    world
        .submit(Input::ForkOverride(ForkOverride {
            changes: vec![fed(3, 0.7), fed(1, 0.0)],
        }))
        .expect("accepted");
    world.take_log();
    let bytes = world.save(&[]).expect("saves");
    assert_eq!(load_error(&bytes), None);
    let (loaded, _) = load(&bytes).expect("loads");
    assert_eq!(loaded.state().world_params(), world.state().world_params());
    assert_eq!(loaded.input_log(), world.input_log());
    assert_eq!(loaded.state_hash(), world.state_hash());
    let unexplained = File::tiny().with(|f| f.world_params = Some((false, vec![(3, 0.7)])));
    assert_eq!(load_error(&unexplained), Some(LoadError::Inconsistent));
    // A block of the input log that sets a year before its fork year: stamped at -1 it acts
    // from year 0 on, so year -1 is past, even if the projection agrees with it.
    let set = |year: i64| {
        File::tiny().with(|f| {
            f.world_params = Some((false, vec![(year, 0.7)]));
            f.next_event_id = 3;
            f.input_log.push((
                -1,
                2,
                TestInput::ForkOverride {
                    changes: vec![TestChange::NutritionYear(year, 0.7)],
                },
            ));
        })
    };
    assert_eq!(load_error(&set(0)), None);
    assert_eq!(load_error(&set(-1)), Some(LoadError::Inconsistent));
    // A block stamped at the last second of time has no second to act in: the load
    // rejects it instead of panicking, with a nutrition year or without, even when the
    // projection agrees with it.
    for (changes, params) in [
        (vec![TestChange::HereditaryDiseases(true)], params(true)),
        (
            vec![TestChange::NutritionYear(999_999, 0.7)],
            (false, vec![(999_999, 0.7)]),
        ),
    ] {
        let last = File::tiny().with(|f| {
            f.closed_through = sim_core::time::MAX_ABS_SECS;
            f.queue = Vec::new();
            f.plan_refs = Vec::new();
            f.world_params = Some(params);
            f.next_event_id = 3;
            f.input_log.push((
                sim_core::time::MAX_ABS_SECS,
                2,
                TestInput::ForkOverride { changes },
            ));
        });
        assert_eq!(load_error(&last), Some(LoadError::Inconsistent));
    }
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
        file.world_params = Some(params(false));
        file.next_event_id = 5;
        file.input_log.push((3, 3, override_block(false)));
        change(&mut file);
        file.bytes()
    };
    assert_eq!(load_error(&two(|_| {})), None);

    let cases: [(&str, Vec<u8>); 15] = [
        (
            "a world that never started with world parameters",
            never_started().with(|f| f.world_params = Some(params(false))),
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
            File::tiny().with(|f| f.world_params = Some(params(true))),
        ),
        (
            "world parameters before the override of the log",
            overridden().with(|f| f.world_params = Some(params(false))),
        ),
        (
            "world parameters of the first of two blocks",
            two(|f| f.world_params = Some(params(true))),
        ),
        (
            // The parameters are those of the start that follows: only the order is wrong.
            "an override block before the start",
            overridden().with(|f| {
                f.input_log.swap(0, 1);
                f.input_log[0].1 = 1;
                f.input_log[1].1 = 2;
                f.world_params = Some(params(false));
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
                f.world_params = Some(params(false));
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

// Scenario "mortal" of tools/state_reference.py (see tests/death.rs), on test data with
// accident 0.5 and senescence `a` 0.05, `b` 0.001 per year: she dies of accident in year 4,
// he of accident in year 5.
const MORTAL_SAVE_AFTER_START: &str = "53494d4147454e540118b786a9d2c6ffe0e288010c01ff01010000010000000000000000122d0a01030107fc87ff1d0005010200fe87ff1d0002010100fe87ff1d000100018088ff1daaa1e380010004020185d2bc82f8b2ead44d00aea2e2a9010007020285d2bc82f8b2ead44d009ae1f9c50100060202a0bb8ca1c58498a01c009aadc9f70100030201a0bb8ca1c58498a01c0008020101dded90d1a3f7cbf6b30100000000010000000000000000000203cbdbe1f0b893cbc91e0101000001000000010000019aada6adcbb284bf7200000000000000000100000000000000000000000000000703000200010002000300030002000401010100ff010b746573743a6d6f7274616c0000020100010f636f72653a646973656173655f61720200010301010f636f72653a646973656173655f6172020101000000000000000000122d0a00";
const MORTAL_SAVE_AFTER_YEAR_5: &str = "53494d4147454e540118b786a9d2c6ffe0e288010c01ff01010000010000000000000000122d0afeaffab301030101feb7f9d101000100018088ff1d11000000020101aaa1e380010085d2bc82f8b2ead44ddded90d1a3f7cbf6b30100000000010000000000000000000203aea2e2a9010085d2bc82f8b2ead44dcbdbe1f0b893cbc91e0101000001000000010000019aada6adcbb284bf7200000000000000000100000000000000000000000101000f01010100ff010b746573743a6d6f7274616c0000020100010f636f72653a646973656173655f61720200010301010f636f72653a646973656173655f6172020101000000000000000000122d0a0600fee392ec89d4c588bf010291effa9384c0e6f4220489a5f5f9f6c6d7db320680f8c4f1c4adf6deb60108918bafd0aea798e8db010a8efd89f1cbace2d8a001";
/// The data hash of the test data of "mortal" (`tools/state_reference.py`).
const MORTAL_DATA_HASH: u64 = 0x88c5_83fc_6a4a_4337;
/// The lineage keys of the founders 0 and 1 of `test:mortal` (tools/rng_reference.py).
const MORTAL_KEYS: [u64; 2] = [0xb3ed_2fba_3a24_36dd, 0x1e93_2c9b_8e18_6dcb];
/// When each death is due, in seconds: hers of accident and of senescence, his of
/// accident and of senescence.
const HER_DEATH: i64 = 135_030_869;
const HER_SENESCENCE: i64 = 259_599_181;
const HIS_DEATH: i64 = 178_014_359;
const HIS_SENESCENCE: i64 = 207_566_925;
const MORTAL_YEAR_HASHES: [u64; 6] = [
    0xbf11_16a0_9d84_b1fe,
    0x22e9_9a00_427e_b791,
    0x32b7_5e37_6f3d_5289,
    0xb6bd_d96c_4e31_3c00,
    0xdbd0_613a_ea0b_c591,
    0xa0b1_8964_be22_7e8e,
];

/// The test data of "mortal".
fn mortal_data() -> Data {
    common::death_data(0.5, Some(0.05), common::repository_diseases())
}

/// The year hashes of "mortal" for the years before `years`.
fn mortal_years(years: usize) -> Vec<(i64, u64)> {
    (0..).zip(MORTAL_YEAR_HASHES).take(years).collect()
}

fn she(born: i64) -> Living {
    (
        born,
        MORTAL_KEYS[0],
        FEMALE,
        Genes(FEMALE, WILD, DUMMY),
        Vec::new(),
        0,
        None,
    )
}

fn he(born: i64) -> Living {
    // The allele of `core:disease_ar` twice: its disease, nothing closed, the switch off.
    (
        born,
        MORTAL_KEYS[1],
        MALE,
        Genes(MALE, DUMMY, DUMMY),
        ar((0.0, None)),
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

/// "mortal" right after `Start`: per founder the birthday and the deaths of senescence and
/// of accident, in the order of time.
fn mortal_started() -> File {
    let mut file = File::tiny();
    file.data_hash = MORTAL_DATA_HASH;
    file.next_sim_id = 3;
    file.queue = vec![
        (Y - 2, 0, 5, Payload::Birthday { sim: 2 }, None),
        (Y - 1, 0, 2, Payload::Birthday { sim: 1 }, None),
        (Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
        (HER_DEATH, 0, 4, death(1, ACCIDENT), None),
        (HIS_DEATH, 0, 7, death(2, ACCIDENT), None),
        (HIS_SENESCENCE, 0, 6, death(2, SENESCENCE), None),
        (HER_SENESCENCE, 0, 3, death(1, SENESCENCE), None),
    ];
    file.next_event_seq = 8;
    file.sims = vec![(1, she(-1)), (2, he(-2))];
    file.plan_refs = vec![(3, 0), (2, 0), (1, 0), (2, 0), (3, 0), (3, 0), (2, 0)];
    file.next_event_id = 4;
    file.input_log[0].2 = TestInput::Start {
        world_seed: 0xff,
        founder_tag: "test:mortal".to_owned(),
        world_params: params(false),
        founders: vec![
            (-1, FEMALE, Alleles(WILD, DUMMY)),
            (-2, MALE, Alleles(DUMMY, DUMMY)),
        ],
        couples: Vec::new(),
        fertility: BARREN,
    };
    file
}

/// "mortal" at the end of year 5: both are in the archive, only the year end series is
/// left in the queue.
fn mortal_after_year_5() -> File {
    let mut file = mortal_started();
    file.closed_through = 6 * Y - 1;
    file.queue = vec![(7 * Y - 1, 0, 1, Payload::YearEnd, Some(Y))];
    file.next_event_seq = 17;
    file.sims.clear();
    file.archive = vec![
        (1, dead(she(-1), HER_DEATH, ACCIDENT)),
        (2, dead(he(-2), HIS_DEATH, ACCIDENT)),
    ];
    file.plan_refs = vec![(1, 0)];
    file.next_event_id = 15;
    file.year_hashes = mortal_years(6);
    file
}

/// "mortal" right after her death of accident: she is in the archive, his events are
/// pending (his birthday was planned anew by entry 10 at his birthday in year 3).
fn mortal_after_her_death() -> File {
    let mut file = mortal_started();
    file.closed_through = HER_DEATH;
    file.queue = vec![
        (5 * Y - 2, 0, 14, Payload::Birthday { sim: 2 }, None),
        (5 * Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
        (HIS_DEATH, 0, 7, death(2, ACCIDENT), None),
        (HIS_SENESCENCE, 0, 6, death(2, SENESCENCE), None),
    ];
    file.next_event_seq = 16;
    file.sims = vec![(2, he(-2))];
    file.archive = vec![(1, dead(she(-1), HER_DEATH, ACCIDENT))];
    file.plan_refs = vec![(10, 4 * Y - 2), (1, 0), (3, 0), (3, 0)];
    file.next_event_id = 13;
    file.year_hashes = mortal_years(4);
    file
}

// Scenario "tie" of tools/state_reference.py: her senescence and her accident are due in
// the same second (test data: accident 0.2, senescence `a` 0.8046949992648321, `b` 0.001).
const TIE_SAVE_AFTER_START: &str = "53494d4147454e5401189d96c6ffeed5b580190c01ff01010000010000000000000000122d0a01020104fe87ff1d0002010100fe87ff1d000100018088ff1dc8ddf17000030201a0bb8ca1c58498a01c00c8ddf1700004020185d2bc82f8b2ead44d0005010101f0da979a8fd9f18a950100000000010000000000000000000000000402000100020002000301010100ff0108746573743a7469650000010100010f636f72653a646973656173655f6172020001000000000000000000122d0a00";
/// The data hash of the test data of "tie" (`tools/state_reference.py`).
const TIE_DATA_HASH: u64 = 0x1900_d6ae_eff1_8b1d;
/// The lineage key of founder 0 of `test:tie` (tools/rng_reference.py).
const TIE_KEY: u64 = 0x9515_c6c8_f345_ed70;
const TIE_DUE: i64 = 118_372_196;

/// The test data of "tie".
fn tie_data() -> Data {
    common::death_data(
        0.2,
        Some(0.804_694_999_264_832_1),
        common::repository_diseases(),
    )
}

/// "tie" right after `Start`: the woman's birthday, then her two deaths in the order
/// of the causes, both due in one second.
fn tie_started() -> File {
    let mut file = File::tiny();
    file.data_hash = TIE_DATA_HASH;
    file.queue[2].0 = TIE_DUE;
    file.queue[3].0 = TIE_DUE;
    file.sims[0].1.1 = TIE_KEY;
    file.input_log[0].2 = TestInput::Start {
        founder_tag: "test:tie".to_owned(),
        world_seed: 0xff,
        world_params: params(false),
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
        (2 * Y - 2, 0, 8, Payload::Birthday { sim: 2 }, None),
        (2 * Y - 1, 0, 9, Payload::Birthday { sim: 1 }, None),
        (2 * Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
        (HER_DEATH, 0, 4, death(1, ACCIDENT), None),
        (HIS_DEATH, 0, 7, death(2, ACCIDENT), None),
        (HIS_SENESCENCE, 0, 6, death(2, SENESCENCE), None),
        (HER_SENESCENCE, 0, 3, death(1, SENESCENCE), None),
    ];
    file.next_event_seq = 10;
    file.plan_refs = vec![
        (4, Y - 2),
        (5, Y - 1),
        (1, 0),
        (2, 0),
        (3, 0),
        (3, 0),
        (2, 0),
    ];
    file.next_event_id = 6;
    file.year_hashes = mortal_years(1);
    file
}

/// The saves of "mortal" and "tie" from the reference, the test encoder and the core are
/// the same bytes, and they load.
#[test]
fn a_save_keeps_the_deaths_and_the_archive() {
    assert_eq!(mortal_started().bytes(), hex(MORTAL_SAVE_AFTER_START));
    assert_eq!(mortal_after_year_5().bytes(), hex(MORTAL_SAVE_AFTER_YEAR_5));
    assert_eq!(tie_started().bytes(), hex(TIE_SAVE_AFTER_START));
    for file in [
        mortal_started(),
        mortal_in_year_1(),
        mortal_after_her_death(),
        mortal_after_year_5(),
    ] {
        assert_eq!(load_error_with(mortal_data(), &file.bytes()), None);
    }
    assert_eq!(load_error_with(tie_data(), &tie_started().bytes()), None);
    // The states in year 1 and at her death are the ones the core reaches.
    let mut world = World::new(mortal_data());
    world
        .submit(Input::Start(Start {
            world_seed: 0xff,
            founder_tag: DataId::new("test:mortal").expect("valid"),
            world_params: WorldParams {
                hereditary_diseases: false,
                nutrition: Vec::new(),
            },
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
/// has exactly the deaths in the queue that its budgets give, of senescence, of accident
/// and of its lethal diseases while they act.
#[test]
fn load_checks_the_deaths() {
    // The clock stands in year 8: her death of senescence would have been due by now.
    let late = |change: fn(&mut File)| {
        let mut file = mortal_after_year_5();
        file.closed_through = 9 * Y - 1;
        file.queue[0].0 = 10 * Y - 1;
        file.year_hashes = (0..9).map(|year| (year, 7)).collect();
        change(&mut file);
        file.bytes()
    };
    assert_eq!(load_error_with(mortal_data(), &late(|_| {})), None);

    let cases: [(&str, Vec<u8>); 21] = [
        // Review finding: a death in the first open second has not run yet.
        (
            "a death in the second that is still open",
            mortal_after_her_death().with(|f| f.closed_through = HER_DEATH - 1),
        ),
        // The archive.
        (
            "a death one second after the budget ran out",
            mortal_after_year_5().with(|f| f.archive[0].1.1 = HER_DEATH + 1),
        ),
        (
            "a death one second before the budget ran out",
            mortal_after_year_5().with(|f| f.archive[1].1.1 = HIS_DEATH - 1),
        ),
        (
            "a death of the other cause at that time",
            mortal_after_year_5()
                .with(|f| f.archive[0].1.2 = TestLeftBy::Death { cause: SENESCENCE }),
        ),
        (
            "a death of a cause the world does not have",
            mortal_after_year_5().with(|f| f.archive[0].1.2 = TestLeftBy::Death { cause: 7 }),
        ),
        (
            "a death of the cause that was due later",
            late(|f| f.archive[0].1 = dead(she(-1), HER_SENESCENCE, SENESCENCE)),
        ),
        (
            "a death of his disease, which does not act while the switch is off",
            mortal_after_year_5().with(|f| {
                f.archive[1].1.2 = TestLeftBy::Death { cause: ar_key() };
            }),
        ),
        (
            "a death that lies ahead",
            mortal_in_year_1().with(|f| {
                f.sims.remove(0);
                f.archive = vec![(1, dead(she(-1), HER_DEATH, ACCIDENT))];
                f.queue.retain(|entry| {
                    !matches!(
                        entry.3,
                        Payload::Death { sim: 1, .. } | Payload::Birthday { sim: 1 }
                    )
                });
                f.plan_refs = vec![(4, Y - 2), (1, 0), (3, 0), (3, 0)];
            }),
        ),
        (
            "an archived founder with another sex",
            mortal_after_year_5().with(|f| f.archive[0].1.4 = MALE),
        ),
        (
            "an archived founder with another genotype",
            mortal_after_year_5()
                .with(|f| f.archive[0].1.5 = Genes(f.archive[0].1.4, DUMMY, DUMMY)),
        ),
        (
            "an archived founder with another lineage key",
            mortal_after_year_5().with(|f| f.archive[0].1.3 = MORTAL_KEYS[1]),
        ),
        // The living.
        (
            "a living sim whose budget has run out",
            mortal_after_year_5().with(|f| {
                f.archive.remove(0);
                f.sims = vec![(1, she(-1))];
                f.queue
                    .insert(0, (7 * Y - 1, 0, 9, Payload::Birthday { sim: 1 }, None));
                f.queue
                    .push((HER_SENESCENCE, 0, 3, death(1, SENESCENCE), None));
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
            "her two causes swapped",
            mortal_started().with(|f| {
                f.queue[3].3 = death(1, SENESCENCE);
                f.queue[6].3 = death(1, ACCIDENT);
            }),
        ),
        (
            "the deaths of two sims swapped",
            mortal_started().with(|f| {
                f.queue[3].3 = death(2, ACCIDENT);
                f.queue[4].3 = death(1, ACCIDENT);
            }),
        ),
        (
            "a periodic death",
            mortal_started().with(|f| f.queue[3].4 = Some(Y)),
        ),
        (
            "a death for a sim in the archive",
            mortal_after_year_5().with(|f| {
                f.queue
                    .push((HER_SENESCENCE, 0, 3, death(1, SENESCENCE), None));
                f.plan_refs.push((2, 0));
            }),
        ),
        (
            "a death of his disease, planned while the switch is off",
            mortal_started().with(|f| {
                let entry = (HIS_SENESCENCE + 1, 0, 8, death(2, ar_key()), None);
                insert_event(f, entry, (3, 0));
                f.next_event_seq = 9;
            }),
        ),
    ];
    for (name, bytes) in &cases {
        assert_eq!(
            load_error_with(mortal_data(), bytes),
            Some(LoadError::Inconsistent),
            "{name}"
        );
    }
    // Review finding: of two deaths due in one second the one planned first runs first;
    // the causes were planned in the order of the list.
    let against_the_order = tie_started().with(|f| {
        f.queue[2].3 = death(1, ACCIDENT);
        f.queue[3].3 = death(1, SENESCENCE);
    });
    assert_eq!(
        load_error_with(tie_data(), &against_the_order),
        Some(LoadError::Inconsistent)
    );
    // A death twice in the queue: the scheduler sees the same insertion number, or the
    // loader the same sim and cause.
    let twice = mortal_started().with(|f| {
        f.queue
            .insert(4, (HER_DEATH, 0, 8, death(1, ACCIDENT), None));
        f.plan_refs.insert(4, (2, 0));
        f.next_event_seq = 9;
    });
    assert_eq!(
        load_error_with(mortal_data(), &twice),
        Some(LoadError::Inconsistent)
    );
}

// Scenario "sick" of tools/state_reference.py, on test data with accident 0.02 per year,
// senescence practically off and `test:dummy_disease` with 0.3 per year from birth on for
// the homozygous, while the switch is on. She has one dummy allele, he has two.
const SWITCHED_ON_SAVE: &str = "53494d4147454e540118c6c0f3b2b0e3c0c7e6010c01ff01010100010000000000000000122d0afe87ff1d030108fc8ffe3b0008010200fe8ffe3b0009010100fe8ffe3b000100018088ff1dc6d6ae7e000a0202dd90d4c2cac3a7bbf40100eedadbe40d0007020285d2bc82f8b2ead44d00a695b0d0280004020185d2bc82f8b2ead44d00bc87cbdac42a00060202a0bb8ca1c58498a01c00dc84cbc8823300030201a0bb8ca1c58498a01c000b020101abd4f19caceffaefab0100000000010000000000000000000203fba99785c4c1bc5c010100000100000001000001dd90d4c2cac3a7bbf4010000000000000000010000000000000000018088ff1d00000000000804fc87ff1d05fe87ff1d0100068088ff1d03000200030002000702010100ff0109746573743a7369636b0000020100010f636f72653a646973656173655f61720200010301010f636f72653a646973656173655f6172020101000000000000000000122d0afe87ff1d02010100010100dbd5f9abd6fecfc87c";
const SICK_ON_SAVE_AFTER_YEAR_3: &str = "53494d4147454e540118c6c0f3b2b0e3c0c7e6010c01ff01010100010000000000000000122d0afe9ffc77030104fea7fb9501000f010100fea7fb9501000100018088ff1da695b0d0280004020185d2bc82f8b2ead44d00dc84cbc8823300030201a0bb8ca1c58498a01c0010010101abd4f19caceffaefab0100000000010000000000000000000000010203c6ceaf6000dd90d4c2cac3a7bbf401fba99785c4c1bc5c010100000100000001000001dd90d4c2cac3a7bbf401000000000000000001000000000000000001000000040bfe9ffc770100020002000c01010100ff0109746573743a7369636b0100020100010f636f72653a646973656173655f61720200010301010f636f72653a646973656173655f6172020101000000000000000000122d0a0400f4b1c4fdd6f8f3e90c02d3b391f38afac8db0c04fec7c0b9fac2b4e9c00106c4dcb3e2f1adc79150";
const OFF_AT_DUE_SAVE: &str = "53494d4147454e540118c6c0f3b2b0e3c0c7e6010c01ff01010000010000000000000000122d0afea7fb9501030107fcaffab3010011010200feaffab3010012010100feaffab301000100018088ff1deedadbe40d0007020285d2bc82f8b2ead44d00a695b0d0280004020185d2bc82f8b2ead44d00bc87cbdac42a00060202a0bb8ca1c58498a01c00dc84cbc8823300030201a0bb8ca1c58498a01c0013020101abd4f19caceffaefab0100000000010000000000000000000203fba99785c4c1bc5c010100000100000001000001dd90d4c2cac3a7bbf401000000000000000001c3992b7cb8d8ee3f000000000000070dfca7fb95010efea7fb9501010003000200030002000f02010100ff0109746573743a7369636b0100020100010f636f72653a646973656173655f61720200010301010f636f72653a646973656173655f6172020101000000000000000000122d0ac4ceaf6002010100000500f4b1c4fdd6f8f3e90c02d3b391f38afac8db0c04fec7c0b9fac2b4e9c00106bee5a9ada1c78b882408bfa09db6d58ca38a9301";
/// The lineage keys of the founders 0 and 1 of `test:sick` (tools/rng_reference.py).
const SICK_KEYS: [u64; 2] = [0xabdf_eb7a_c39c_6a2b, 0x00b8_f20c_40a5_d4fb];
/// The `str_key` of `test:dummy_disease`.
const DISEASE: u64 = 0xf476_9e1c_a855_085d;
/// The data hash of the test data with the dummy disease from birth on
/// (`tools/state_reference.py`).
const SICK_DATA_HASH: u64 = 0xe68f_031b_065c_e046;
/// How long his disease must act on him, in seconds, and when each of them dies of
/// accident and of senescence.
const HIS_DISEASE: i64 = 101_053_347;
const HIS_SICK_ACCIDENT: i64 = 1_850_439_351;
const HER_SICK_ACCIDENT: i64 = 5_452_989_779;
const HIS_SICK_SENESCENCE: i64 = 730_776_297_950;
const HER_SICK_SENESCENCE: i64 = 876_517_876_014;
/// The hazard his disease used when the switch went off in the second of its due death
/// (`tools/state_reference.py`): a little above his budget 0.963955156377135, by the part
/// of the second that the death rounded up.
const OFF_AT_DUE_USED: f64 = 0.963_955_156_822_344_2;

/// The test data with the dummy disease (ADR-0017 section 5).
fn sick_data() -> Data {
    common::death_data(0.02, None, common::dummy_disease(0.0))
}

/// `load_error` on the test data with the dummy disease.
fn sick_load_error(bytes: &[u8]) -> Option<LoadError> {
    load_error_with(sick_data(), bytes)
}

/// The deaths of accident and of senescence of both, as each of the saves of "sick" in
/// which both live has them at its end.
fn sick_deaths() -> Vec<(QueueEntry, (u64, i64))> {
    vec![
        ((HIS_SICK_ACCIDENT, 0, 7, death(2, ACCIDENT), None), (3, 0)),
        ((HER_SICK_ACCIDENT, 0, 4, death(1, ACCIDENT), None), (2, 0)),
        (
            (HIS_SICK_SENESCENCE, 0, 6, death(2, SENESCENCE), None),
            (3, 0),
        ),
        (
            (HER_SICK_SENESCENCE, 0, 3, death(1, SENESCENCE), None),
            (2, 0),
        ),
    ]
}

fn sick_start(on: bool) -> TestInput {
    TestInput::Start {
        world_seed: 0xff,
        founder_tag: "test:sick".to_owned(),
        world_params: params(on),
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
        Vec::new(),
        0,
        None,
    )
}

/// He has the dummy disease from birth on with this exposure.
fn sick_he(exposure: TestExposure) -> Living {
    (
        -2,
        SICK_KEYS[1],
        MALE,
        Genes(MALE, DUMMY, DUMMY),
        vec![(DISEASE, 0.0, Some(exposure))],
        0,
        None,
    )
}

/// "sick" with the switch off, switched on at the start of year 1: his exposure begins
/// there, his death of the disease is planned by the entry of the override (6) after
/// all that was planned before.
fn sick_switched_on() -> File {
    let mut file = File::tiny();
    file.data_hash = SICK_DATA_HASH;
    file.world_params = Some(params(true));
    file.closed_through = Y - 1;
    file.next_sim_id = 3;
    let events = [
        (
            (2 * Y - 2, 0, 8, Payload::Birthday { sim: 2 }, None),
            (4, Y - 2),
        ),
        (
            (2 * Y - 1, 0, 9, Payload::Birthday { sim: 1 }, None),
            (5, Y - 1),
        ),
        ((2 * Y - 1, 0, 1, Payload::YearEnd, Some(Y)), (1, 0)),
        ((Y + HIS_DISEASE, 0, 10, death(2, DISEASE), None), (6, Y)),
    ];
    (file.queue, file.plan_refs) = events.into_iter().chain(sick_deaths()).unzip();
    file.next_event_seq = 11;
    file.sims = vec![(1, sick_she()), (2, sick_he((0.0, Some(Y))))];
    file.next_event_id = 7;
    file.input_log = vec![(-1, 1, sick_start(false)), (Y - 1, 2, override_block(true))];
    file.year_hashes = vec![(0, 0x7c91_3ff5_657e_6adb)];
    file
}

/// "sick" with the switch on from the start, at the end of year 3: he died of the
/// disease, his exposure is kept as it was then.
fn sick_on_after_year_3() -> File {
    let mut file = sick_switched_on();
    file.closed_through = 4 * Y - 1;
    file.queue = vec![
        (5 * Y - 1, 0, 15, Payload::Birthday { sim: 1 }, None),
        (5 * Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
        (HER_SICK_ACCIDENT, 0, 4, death(1, ACCIDENT), None),
        (HER_SICK_SENESCENCE, 0, 3, death(1, SENESCENCE), None),
    ];
    file.next_event_seq = 16;
    file.sims = vec![(1, sick_she())];
    file.archive = vec![(2, dead(sick_he((0.0, Some(0))), HIS_DISEASE, DISEASE))];
    file.plan_refs = vec![(11, 4 * Y - 1), (1, 0), (2, 0), (2, 0)];
    file.next_event_id = 12;
    file.input_log = vec![(-1, 1, sick_start(true))];
    file.year_hashes = vec![
        (0, 0x0cd3_cfc5_6fb1_18f4),
        (1, 0x0cb7_23d0_ae64_59d3),
        (2, 0xc0d2_d217_a730_23fe),
        (3, 0x5023_1d6f_1c4c_ee44),
    ];
    file
}

/// "sick" with the switch on from the start and off in the very second his death of the
/// disease was due: he lives with the budget used up, at the end of year 4.
fn sick_off_at_due() -> File {
    let mut file = sick_switched_on();
    file.world_params = Some(params(false));
    file.closed_through = 5 * Y - 1;
    let events = [
        (
            (6 * Y - 2, 0, 17, Payload::Birthday { sim: 2 }, None),
            (13, 5 * Y - 2),
        ),
        (
            (6 * Y - 1, 0, 18, Payload::Birthday { sim: 1 }, None),
            (14, 5 * Y - 1),
        ),
        ((6 * Y - 1, 0, 1, Payload::YearEnd, Some(Y)), (1, 0)),
    ];
    (file.queue, file.plan_refs) = events.into_iter().chain(sick_deaths()).unzip();
    file.next_event_seq = 19;
    file.sims = vec![(1, sick_she()), (2, sick_he((OFF_AT_DUE_USED, None)))];
    file.next_event_id = 15;
    file.input_log = vec![
        (-1, 1, sick_start(true)),
        (HIS_DISEASE - 1, 2, override_block(false)),
    ];
    file.year_hashes = vec![
        (0, 0x0cd3_cfc5_6fb1_18f4),
        (1, 0x0cb7_23d0_ae64_59d3),
        (2, 0xc0d2_d217_a730_23fe),
        (3, 0x2410_2e3a_15aa_72be),
        (4, 0x9314_8c65_56c7_503f),
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
        assert_eq!(sick_load_error(&file.bytes()), None);
    }
}

/// The afflictions are those the genotype, the sex and the draws give (ADR-0017 section 3),
/// each exposure what the switch history of the input log gives: since the start or the
/// last override that switched on, with what the closed periods used. The living have not
/// used up a budget within a closed period, the archive keeps the afflictions of the
/// death. ADR-0006 section 5.
#[test]
fn load_checks_the_afflictions() {
    let cases: [(&str, Vec<u8>); 17] = [
        // The living.
        (
            "a sim with the allele twice without its disease",
            sick_switched_on().with(|f| f.sims[1].1.4 = Vec::new()),
        ),
        (
            "the disease for a sim with one allele",
            sick_switched_on().with(|f| f.sims[0].1.4 = vec![(DISEASE, 0.0, Some((0.0, None)))]),
        ),
        (
            "his disease twice",
            sick_switched_on().with(|f| {
                let twice = f.sims[1].1.4[0];
                f.sims[1].1.4.push(twice);
            }),
        ),
        (
            "his disease under another key",
            sick_switched_on().with(|f| f.sims[1].1.4[0].0 = ACCIDENT),
        ),
        (
            "his disease with another onset",
            sick_switched_on().with(|f| f.sims[1].1.4[0].1 = 1.0),
        ),
        (
            "his disease without an exposure though it is lethal",
            sick_switched_on().with(|f| f.sims[1].1.4[0].2 = None),
        ),
        (
            "an exposure since one second after the override",
            sick_switched_on().with(|f| {
                f.sims[1].1.4[0].2 = Some((0.0, Some(Y + 1)));
                f.queue[3].0 += 1;
            }),
        ),
        (
            "an exposure that is off while the switch is on",
            sick_switched_on().with(|f| {
                f.sims[1].1.4[0].2 = Some((0.0, None));
                f.queue.remove(3);
                f.plan_refs.remove(3);
            }),
        ),
        (
            "an exposure with a closed period that never was",
            sick_switched_on().with(|f| f.sims[1].1.4[0].2 = Some((0.3, Some(Y)))),
        ),
        (
            "an exposure with a negative use",
            sick_switched_on().with(|f| f.sims[1].1.4[0].2 = Some((-0.3, Some(Y)))),
        ),
        (
            "a start with the switch on and an exposure only since the override",
            sick_switched_on().with(|f| f.input_log[0].2 = sick_start(true)),
        ),
        (
            "a death of the disease for her",
            sick_switched_on().with(|f| {
                f.queue
                    .insert(4, (Y + HIS_DISEASE + 1, 0, 11, death(1, DISEASE), None));
                f.plan_refs.insert(4, (6, Y));
                f.next_event_seq = 12;
            }),
        ),
        (
            "an exposure a little short of the history",
            sick_off_at_due().with(|f| f.sims[1].1.4[0].2 = Some((0.9, None))),
        ),
        (
            // The exposure is exactly what the switch history gives (on from the start, off
            // 100 seconds after his due death), so only the rule that the budget did not run
            // out within a closed period rejects it.
            "alive although the disease used up its budget while the switch was on",
            sick_off_at_due().with(|f| {
                f.input_log[1].0 = HIS_DISEASE + 99;
                let age = |t: i64| SimDuration::from_secs(t + 2).as_years();
                let hazard = StepHazard {
                    before: 0.3,
                    change_age: 0.0,
                    after: 0.3,
                };
                let used = disease_hazard_used(hazard, age(0), age(HIS_DISEASE + 100));
                f.sims[1].1.4[0].2 = Some((used, None));
            }),
        ),
        // The archive.
        (
            "an archived sim with the allele twice without its disease",
            sick_on_after_year_3().with(|f| f.archive[0].1.6 = Vec::new()),
        ),
        (
            "an archived exposure that is off though the switch was on",
            sick_on_after_year_3().with(|f| f.archive[0].1.6[0].2 = Some((0.0, None))),
        ),
        (
            "an archived death of the disease one second early",
            sick_on_after_year_3().with(|f| f.archive[0].1.1 = HIS_DISEASE - 1),
        ),
    ];
    for (name, bytes) in &cases {
        assert_eq!(
            sick_load_error(bytes),
            Some(LoadError::Inconsistent),
            "{name}"
        );
    }
}

// Scenario "partners" of tools/state_reference.py, on test data with accident 0.02 and
// senescence `a` 0.5, `b` 0.001 per year: a woman and two men. `Start` pairs her with the
// first man; he dies of senescence in year 0, at the start of year 1 `ArrangePartnership`
// pairs her with the second, and that partnership ends with her death of accident.
const PARTNERS_SAVE_AFTER_ARRANGE: &str = "53494d4147454e540118fec88e9b8fe6898187010c01ff01010000010000000000000000122d0afe87ff1d040308cce2e6370004020185d2bc82f8b2ead44d00fa8ffe3b000c010300fe8ffe3b000d010100fe8ffe3b000100018088ff1d9ce4ac3f00030201a0bb8ca1c58498a01c0096e1f7860300090203a0bb8ca1c58498a01c008090ef9b04000e03010180a8a702aea1c18a0a000a020385d2bc82f8b2ead44d000f020101aa90d4c3eeedd09bdb0100000000010000000000000000000305a7f3f8fdb9ee88cfed010101000000000000010000000000020101020001a2b589060201038088ff1d0000010203a2b5890600a0bb8ca1c58498a01cf098dcaaccecf99cb0010101000001000000010000019aada6adcbb284bf72000000000000000001000000000000000000000008020007fa87ff1d08fe87ff1d010002000400098088ff1d04000a02010100ff010d746573743a706172746e6572730000030100010f636f72653a646973656173655f61720200010301010f636f72653a646973656173655f61720201010501010f636f72653a646973656173655f61720201000100010000000000000000122d0afe87ff1d020201030100f086f1bae9faf7c545";
const PARTNERS_SAVE_AFTER_YEAR_4: &str = "53494d4147454e540118fec88e9b8fe6898187010c01ff01010000010000000000000000122d0afea7fb9501040304faaffab3010012010300feaffab301000100018088ff1d96e1f7860300090203a0bb8ca1c58498a01c00aea1c18a0a000a020385d2bc82f8b2ead44d0013010305a7f3f8fdb9ee88cfed010101000000000000010000000000020101020001a2b589060201038088ff1d01cce2e63700020101cce2e6370085d2bc82f8b2ead44daa90d4c3eeedd09bdb0100000000010000000000000000000203a2b5890600a0bb8ca1c58498a01cf098dcaaccecf99cb0010101000001000000010000019aada6adcbb284bf720000000000000000010000000000000000000000040efaa7fb95010100040004000f02010100ff010d746573743a706172746e6572730000030100010f636f72653a646973656173655f61720200010301010f636f72653a646973656173655f61720201010501010f636f72653a646973656173655f61720201000100010000000000000000122d0afe87ff1d020201030500f086f1bae9faf7c54502ddf9cabcffabc98e2c04fadbe2a8d2ee96e7ae0106b6b2bb859ce49cfdfd0108b39bb285f2fec39377";
/// The lineage keys of the founders 0 to 2 of `test:partners` (tools/rng_reference.py).
const PARTNERS_KEYS: [u64; 3] = [
    0xdb37_436e_e875_082a,
    0xb039_e764_c557_0c70,
    0xed9e_2373_9fbe_39a7,
];
/// The data hash of the test data of "partners" (`tools/state_reference.py`).
const PARTNERS_DATA_HASH: u64 = 0x8702_2730_f363_a47e;
/// When each of the three dies of senescence and of accident, if nothing comes first.
const PARTNER_SENESCENCE: [i64; 3] = [66_427_150, 6_368_593, 409_925_707];
const PARTNER_ACCIDENT: [i64; 3] = [58_513_574, 273_890_542, 1_353_197_655];
/// When she and the first man die.
const PARTNER_HER_DEATH: i64 = PARTNER_ACCIDENT[0];
const PARTNER_HIS_DEATH: i64 = PARTNER_SENESCENCE[1];

/// The test data of "partners".
fn partners_data() -> Data {
    common::death_data(0.02, Some(0.5), common::repository_diseases())
}

/// `load_error` on the test data of "partners".
fn partners_load_error(bytes: &[u8]) -> Option<LoadError> {
    load_error_with(partners_data(), bytes)
}

fn partners_start(couples: Vec<(u32, u32)>) -> TestInput {
    TestInput::Start {
        world_seed: 0xff,
        founder_tag: "test:partners".to_owned(),
        world_params: params(false),
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
    match index {
        0 => (
            -1,
            PARTNERS_KEYS[0],
            FEMALE,
            Genes(FEMALE, WILD, DUMMY),
            Vec::new(),
            0,
            None,
        ),
        1 => (
            -2,
            PARTNERS_KEYS[1],
            MALE,
            Genes(MALE, DUMMY, DUMMY),
            ar((0.0, None)),
            0,
            None,
        ),
        _ => (
            -3,
            PARTNERS_KEYS[2],
            MALE,
            Genes(MALE, DUMMY, WILD),
            Vec::new(),
            0,
            None,
        ),
    }
}

/// "partners" right after her second partnership: the first man is in the archive, the
/// first partnership ended with his death, the second formed at the start of year 1 by
/// the input with sequence number 2.
fn partners_arranged() -> File {
    let mut file = File::tiny();
    file.data_hash = PARTNERS_DATA_HASH;
    file.closed_through = Y - 1;
    file.next_sim_id = 4;
    file.next_partnership_id = 3;
    file.queue = vec![
        (PARTNER_HER_DEATH, 0, 4, death(1, ACCIDENT), None),
        (2 * Y - 3, 0, 12, Payload::Birthday { sim: 3 }, None),
        (2 * Y - 1, 0, 13, Payload::Birthday { sim: 1 }, None),
        (2 * Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
        (PARTNER_SENESCENCE[0], 0, 3, death(1, SENESCENCE), None),
        (PARTNER_SENESCENCE[2], 0, 9, death(3, SENESCENCE), None),
        // Her attempts from her 18th birthday on (the series of the first partnership
        // ended with his death), by the entry of the second partnership.
        (
            18 * Y,
            0,
            14,
            Payload::ConceptionAttempt { woman: 1 },
            Some(MONTH),
        ),
        (PARTNER_ACCIDENT[2], 0, 10, death(3, ACCIDENT), None),
    ];
    file.next_event_seq = 15;
    file.sims = vec![(1, partner(0)), (3, partner(2))];
    file.partnerships = vec![
        (1, (1, 2, 0, Some(PARTNER_HIS_DEATH))),
        (2, (1, 3, Y, None)),
    ];
    file.archive = vec![(2, dead(partner(1), PARTNER_HIS_DEATH, SENESCENCE))];
    file.plan_refs = vec![
        (2, 0),
        (7, Y - 3),
        (8, Y - 1),
        (1, 0),
        (2, 0),
        (4, 0),
        (9, Y),
        (4, 0),
    ];
    file.next_event_id = 10;
    file.input_log = vec![
        (-1, 1, partners_start(vec![(0, 1)])),
        (Y - 1, 2, TestInput::ArrangePartnership { woman: 1, man: 3 }),
    ];
    file.year_hashes = vec![(0, 0x458b_dfd6_975c_4370)];
    file
}

/// "partners" at the end of year 4: she and the first man are in the archive, the second
/// partnership ended with her death in year 1, the second man lives on.
fn partners_after_year_4() -> File {
    let mut file = partners_arranged();
    file.closed_through = 5 * Y - 1;
    file.queue = vec![
        (6 * Y - 3, 0, 18, Payload::Birthday { sim: 3 }, None),
        (6 * Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
        (PARTNER_SENESCENCE[2], 0, 9, death(3, SENESCENCE), None),
        (PARTNER_ACCIDENT[2], 0, 10, death(3, ACCIDENT), None),
    ];
    file.next_event_seq = 19;
    file.sims = vec![(3, partner(2))];
    file.partnerships[1].1.3 = Some(PARTNER_HER_DEATH);
    file.archive = vec![
        (1, dead(partner(0), PARTNER_HER_DEATH, ACCIDENT)),
        (2, dead(partner(1), PARTNER_HIS_DEATH, SENESCENCE)),
    ];
    file.plan_refs = vec![(14, 5 * Y - 3), (1, 0), (4, 0), (4, 0)];
    file.next_event_id = 15;
    file.year_hashes = vec![
        (0, 0x458b_dfd6_975c_4370),
        (1, 0x2c1d_255f_f792_bcdd),
        (2, 0xaece_5b75_2518_adfa),
        (3, 0xfdfa_7321_c0ae_d936),
        (4, 0x7727_0ff7_20ac_8db3),
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
        assert_eq!(partners_load_error(&file.bytes()), None);
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
                f.partnerships[1].1.2 = PARTNER_HIS_DEATH;
                f.input_log[1].0 = PARTNER_HIS_DEATH - 1;
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
                f.partnerships[1].1 = (1, 2, Y, Some(PARTNER_HIS_DEATH));
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
            partners_arranged().with(|f| f.partnerships[0].1.3 = Some(PARTNER_HIS_DEATH + 1)),
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
            "a partnership ended a second before her death",
            partners_after_year_4().with(|f| f.partnerships[1].1.3 = Some(PARTNER_HER_DEATH - 1)),
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
        assert_eq!(
            partners_load_error(bytes),
            Some(LoadError::Inconsistent),
            "{name}"
        );
    }
}

// Scenario "clan" of tools/state_reference.py, on test data with accident 0.02 per year and
// senescence practically off: three founder couples and a fertility of 0.25 per month; the
// first woman conceives in month 5.
const CLAN_SAVE_AFTER_CONCEPTION: &str = "53494d4147454e54011899eafeaec1d6dbfeb5010c01ff0101000001000000000000d03f122d0a80c8c40b070415fe87ff1d000100018088ff1d8088ff1d00160101008088ff1d00170102008088ff1d00180103008088ff1d00190104008088ff1d001a0105008088ff1d001b01060080d8cd22001c04010080b0fab301001503030180a8a702c4e9bcc101000a020385d2bc82f8b2ead44d00c6d0d8e3010013020685d2bc82f8b2ead44d00f4eba580020004020185d2bc82f8b2ead44d00f0e8f7c90a0010020585d2bc82f8b2ead44d00e0bd98dd10000d020485d2bc82f8b2ead44d00e6caef83400007020285d2bc82f8b2ead44d00baa8afa1f52d000f0205a0bb8ca1c58498a01c00e4a8fce2bb3000090203a0bb8ca1c58498a01c00d4ee8eed963200060202a0bb8ca1c58498a01c0088d1a4e7ec3300120206a0bb8ca1c58498a01c008cfefad2ec3400030201a0bb8ca1c58498a01c009cd68493ae36000c0204a0bb8ca1c58498a01c001d0601ff9fedd704dded83919698d6d0d001000000000100000000000000010002ffafeb9305f6eb9bebea9bd7c53e010100000000000001000000000003ffdff4e70285b2a2a3e3ecdafac4010001000001000000000000019aada6adcbb284bf72000000000000000001000000000000000000000004ffefe38307f9a4bfd6bdf093fea701010000000000000001000000000005ffefd4e30afacbdcacb9abf18703000000000000000000000000000006ffefd4e30af1c5c4b2d9a691fded01010000000000000001000000000003010102000002030400000305060000010102f3c89ad6cdbbe183cd0100010100000100000001000080c8c40b001501000b000c000d000e000f0010001180c8c40b09000400070002000600050003000600040003000700020005001401010100ff0109746573743a636c616e000006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100ffdff4e70200010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a010003000102030405000000000000d03f122d0a00";
/// The lineage keys of the founders 0 to 5 of `test:clan` (tools/rng_reference.py).
const CLAN_KEYS: [u64; 6] = [
    0xd0a1_58c1_6220_f6dd,
    0x3e8b_5cde_ad66_f5f6,
    0xc4f5_6b66_3468_9905,
    0xa7fc_4f83_dacf_d279,
    0x030f_c55b_9597_25fa,
    0xedfa_4535_9651_22f1,
];
/// The data hash of the test data of "clan" (`tools/state_reference.py`).
const CLAN_DATA_HASH: u64 = 0xb5fd_6eb4_15df_b519;
/// The child she conceived in month 5, and when each of the six dies of senescence and of
/// accident, if nothing comes first.
const CLAN_CHILD: u64 = 0xcd07_85dc_dac6_a473;
const CLAN_DEATHS: [(i64, i64); 6] = [
    (907_935_702_918, 268_745_466),
    (862_060_665_770, 8_593_994_419),
    (832_656_345_650, 202_873_442),
    (933_906_912_654, 2_245_201_776),
    (788_832_578_077, 1_419_704_888),
    (890_777_146_436, 238_752_803),
];
const CLAN_FERTILITY: TestFertility = (0.25, 18, 45, 10);

/// The test data of "clan".
fn clan_data() -> Data {
    common::death_data(0.02, None, common::repository_diseases())
}

/// `load_error` on the test data of "clan".
fn clan_load_error(bytes: &[u8]) -> Option<LoadError> {
    load_error_with(clan_data(), bytes)
}

/// Puts the deaths of senescence and of accident of `sim` into the queue of `file`, with
/// the insertion numbers `seq` and `seq + 1`, planned by the entry `by` at `planned_at`.
fn with_deaths(
    file: &mut File,
    sim: u64,
    (senescence, accident): (i64, i64),
    seq: u64,
    plan: (u64, i64),
) {
    insert_event(
        file,
        (senescence, 0, seq, death(sim, SENESCENCE), None),
        plan,
    );
    insert_event(
        file,
        (accident, 0, seq + 1, death(sim, ACCIDENT), None),
        plan,
    );
}

fn clan_start() -> TestInput {
    let founder = |years: i64, sex: u8, genes: Alleles| (-years * Y, sex, genes);
    TestInput::Start {
        world_seed: 0xff,
        founder_tag: "test:clan".to_owned(),
        world_params: params(false),
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
        let afflictions = if genes == Genes(sex, DUMMY, DUMMY) {
            ar((0.0, None))
        } else {
            Vec::new()
        };
        (
            u64::try_from(n).expect("a small place") + 1,
            (-years * Y, CLAN_KEYS[n], sex, genes, afflictions, 0, None),
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
/// first conception (a boy with the dummy allele twice, born in month 15) and her
/// attempts have ended; the girl's attempts wait for her 18th birthday; the birthdays of
/// year 0 have planned those of year 1.
fn clan_conceived() -> File {
    let mut file = File::tiny();
    file.data_hash = CLAN_DATA_HASH;
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
        birthday(1, 22),
        birthday(2, 23),
        birthday(3, 24),
        birthday(4, 25),
        birthday(5, 26),
        birthday(6, 27),
        (15 * MONTH, 0, 28, Payload::Birth { mother: 1 }, None),
        attempts(3, 78 * MONTH, 21),
    ];
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
    ];
    // Each founder's deaths follow its birthday in the insertion numbers, planned by the
    // entry of its creation.
    for (sim, deaths) in (1..).zip(CLAN_DEATHS) {
        with_deaths(&mut file, sim, deaths, 3 * sim, (sim + 1, 0));
    }
    file.next_event_seq = 29;
    file.sims = clan_sims();
    file.sims[0].1.5 = 1;
    file.partnerships = vec![
        (1, (1, 2, 0, None)),
        (2, (3, 4, 0, None)),
        (3, (5, 6, 0, None)),
    ];
    file.pregnancies = vec![(
        1,
        (2, CLAN_CHILD, 0, MALE, Genes(MALE, DUMMY, DUMMY), 5 * MONTH),
    )];
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
    assert_eq!(clan_load_error(&clan_conceived().bytes()), None);
}

const CLAN_SAVE_AFTER_BIRTH: &str = "53494d4147454e54011899eafeaec1d6dbfeb5010c01ff0101000001000000000000d03f122d0a80d8cd220804188080f524002603010180a8a702fe8ffe3b000100018088ff1d8090fe3b001d0101008090fe3b001e0102008090fe3b001f0103008090fe3b00200104008090fe3b00210105008090fe3b002201060080e0cc40002301070080b0fab301001503030180a8a702b0d0e5b9010025020785d2bc82f8b2ead44d00c4e9bcc101000a020385d2bc82f8b2ead44d00c6d0d8e3010013020685d2bc82f8b2ead44d00f4eba580020004020185d2bc82f8b2ead44d00f0e8f7c90a0010020585d2bc82f8b2ead44d00e0bd98dd10000d020485d2bc82f8b2ead44d00e6caef83400007020285d2bc82f8b2ead44d00baa8afa1f52d000f0205a0bb8ca1c58498a01c00e4a8fce2bb3000090203a0bb8ca1c58498a01c00d4ee8eed963200060202a0bb8ca1c58498a01c00ee8dcd9aeb3300240207a0bb8ca1c58498a01c0088d1a4e7ec3300120206a0bb8ca1c58498a01c008cfefad2ec3400030201a0bb8ca1c58498a01c009cd68493ae36000c0204a0bb8ca1c58498a01c00270701ff9fedd704dded83919698d6d0d001000000000100000000000000010002ffafeb9305f6eb9bebea9bd7c53e010100000000000001000000000003ffdff4e70285b2a2a3e3ecdafac4010001000001000000000000019aada6adcbb284bf72000000000000000001000000000000000000000004ffefe38307f9a4bfd6bdf093fea701010000000000000001000000000005ffefd4e30afacbdcacb9abf18703000000000000000000000000000006ffefd4e30af1c5c4b2d9a691fded0101000000000000000100000000000780d8cd22f3c89ad6cdbbe183cd010101000001000000010000019aada6adcbb284bf720000000000000000010000000000000000000001010200030101020000020304000003050600000000181a80d8cd220100148088ff1d158088ff1d168088ff1d178088ff1d188088ff1d198088ff1d1a80d8cd2209001a80d8cd220400070002000600050003000600040003001a80d8cd220700020005001b01010100ff0109746573743a636c616e000006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100ffdff4e70200010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a010003000102030405000000000000d03f122d0a0100e1a4acbc91aefed5f001";
/// When the child (sim 7) dies of senescence and of accident, if nothing comes first.
const CHILD_DEATHS: (i64, i64) = (890_562_519_927, 194_819_096);

/// "clan" right after the birth in month 15: the child (sim 7) lives with its parents and
/// the dummy allele twice, its first birthday and its deaths are planned by the birth
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
                38,
                Payload::ConceptionAttempt { woman: 1 },
                Some(MONTH),
            ),
            (26, 15 * MONTH),
        ),
        ((2 * Y - 1, 0, 1, Payload::YearEnd, Some(Y)), (1, 0)),
        birthday(1, 29, 20),
        birthday(2, 30, 21),
        birthday(3, 31, 22),
        birthday(4, 32, 23),
        birthday(5, 33, 24),
        birthday(6, 34, 25),
        (
            (15 * MONTH + Y, 0, 35, Payload::Birthday { sim: 7 }, None),
            (26, 15 * MONTH),
        ),
        (
            (
                78 * MONTH,
                0,
                21,
                Payload::ConceptionAttempt { woman: 3 },
                Some(MONTH),
            ),
            (9, 0),
        ),
    ];
    (file.queue, file.plan_refs) = events.into_iter().unzip();
    for (sim, deaths) in (1..).zip(CLAN_DEATHS) {
        with_deaths(&mut file, sim, deaths, 3 * sim, (sim + 1, 0));
    }
    with_deaths(&mut file, 7, CHILD_DEATHS, 36, (26, 15 * MONTH));
    file.next_event_seq = 39;
    file.sims.push((
        7,
        (
            15 * MONTH,
            CLAN_CHILD,
            MALE,
            Genes(MALE, DUMMY, DUMMY),
            ar((0.0, None)),
            0,
            Some((1, 2, 0)),
        ),
    ));
    file.pregnancies.clear();
    file.next_event_id = 27;
    file.year_hashes = vec![(0, 0xf0ab_f971_178b_1261)];
    file
}

/// The save of "clan" after the birth from the reference and the test encoder are the
/// same bytes, and it loads.
#[test]
fn a_save_keeps_the_child() {
    assert_eq!(clan_born().bytes(), hex(CLAN_SAVE_AFTER_BIRTH));
    assert_eq!(clan_load_error(&clan_born().bytes()), None);
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
                        FEMALE,
                        Genes(FEMALE, WILD, WILD),
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
                f.sims[6].1.4 = Vec::new();
            }),
        ),
        (
            "a child with another sex",
            clan_born().with(|f| f.sims[6].1.2 = FEMALE),
        ),
        (
            "a child without its disease",
            clan_born().with(|f| f.sims[6].1.4 = Vec::new()),
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
        assert_eq!(
            clan_load_error(bytes),
            Some(LoadError::Inconsistent),
            "{name}"
        );
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
                    FEMALE,
                    Genes(FEMALE, WILD, WILD),
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
                    MALE,
                    Genes(MALE, WILD, WILD),
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
                        MALE,
                        Genes(MALE, DUMMY, WILD),
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
            clan_conceived().with(|f| f.pregnancies[0].1.3 = FEMALE),
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
        assert_eq!(
            clan_load_error(bytes),
            Some(LoadError::Inconsistent),
            "{name}"
        );
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
        world_params: params(false),
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
                // A founder born at the start; everything else fits such a founder (her
                // deaths from tools/state_reference.py).
                f.sims = vec![(1, living(0, 0))];
                f.queue = vec![
                    (Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
                    (Y, 0, 2, birthday(1), None),
                    (2_680_916_329, 0, 3, death(1, SENESCENCE), None),
                    (TINY_DEATHS[0].1, 0, 4, death(1, ACCIDENT), None),
                ];
                f.plan_refs = vec![(1, 0), (2, 0), (2, 0), (2, 0)];
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
            "a living sim beyond the counter, with its birthday and its deaths",
            File::tiny().with(|f| {
                f.sims.push((5, living(1, -1)));
                f.queue.insert(1, (Y - 1, 0, 5, birthday(5), None));
                f.plan_refs.insert(1, (2, 0));
                with_deaths(f, 5, TINY_DEATHS[1], 6, (2, 0));
                f.next_event_seq = 8;
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
                f.queue[0].0 = 2 * Y - 1;
                f.queue.swap(0, 1);
                f.plan_refs.swap(0, 1);
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
                f.queue.insert(1, (Y - 1, 0, 5, birthday(1), None));
                f.plan_refs.insert(1, (2, 0));
                f.next_event_seq = 6;
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

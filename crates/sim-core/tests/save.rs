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
use sim_core::input::{ForkOverride, Founder, Input, Start};
use sim_core::log::{LogEntry, log_hash};
use sim_core::mortality::{StepHazard, disease_hazard_used};
use sim_core::params::{NutritionYear, ParamChange, WorldParams};
use sim_core::time::{SimDuration, SimTime};
use sim_core::version::CORE_VERSION;
use sim_core::world::{FORMAT_VERSION, LoadError, SaveError, World, YearOut};

/// The data hash of the definitions of the repository, from `tools/data_reference.py`
/// (ADR-0007).
const DATA_HASH: u64 = 0xadab_5027_57b4_045c;
const Y: i64 = 31_449_600;
const MONTH: i64 = 2_419_200;
const DAY: i64 = 86_400;

// tools/state_reference.py
const HEADER: &str = "53494d4147454e540227dc88d0bdf584d4d5ad01";
const TINY_SAVE_AFTER_START: &str = "53494d4147454e540227dc88d0bdf584d4d5ad011201ff01010000010201020680a8a7020002060180a8a702fe87ff1d0003010100fe87ff1d000100018088ff1dfe9fedd7040006070100d0a5dcfc1300040201a0bb8ca1c58498a01c00b8f4e5ff8c1f0005020185d2bc82f8b2ead44d00070101019893ebf7f7b1d2ec5e0000000001000000000000000000000100000000000001010000000000060100020001000200020002000401010100ff0109746573743a74696e790000010100010f636f72653a646973656173655f617202000100000000";
const TINY_SAVE_OVERRIDDEN: &str = "53494d4147454e540227dc88d0bdf584d4d5ad011201ff01010100010201020680a8a7020002060180a8a702fe87ff1d0003010100fe87ff1d000100018088ff1dfe9fedd7040006070100d0a5dcfc1300040201a0bb8ca1c58498a01c00b8f4e5ff8c1f0005020185d2bc82f8b2ead44d00070101019893ebf7f7b1d2ec5e0000000001000000000000000000000100000000000001010000000000060100020001000200020002000502010100ff0109746573743a74696e790000010100010f636f72653a646973656173655f617202000100000001020101000100";
const EMPTY_SAVE: &str =
    "53494d4147454e540227dc88d0bdf584d4d5ad0112000001010101000100000000000000010000";

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
        capacity: None,
        pool: None,
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
    Separation { partnership: u64 },
    MarketRound,
    LeaveHome { sim: u64 },
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
        /// The soft capacity (ADR-0020 section 6).
        capacity: Option<u64>,
        /// The start from the pool (ADR-0020 section 5): none in these states, one byte.
        pool: Option<()>,
    },
    ForkOverride {
        changes: Vec<TestChange>,
    },
    ArrangePartnership {
        woman: u64,
        man: u64,
    },
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
/// When founder 0 of `test:tiny` leaves home: at 20 (`tools/state_reference.py`).
const TINY_LEAVES: i64 = 20 * Y - 1;
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
/// (born, lineage_key, sex, genome, afflictions, conceptions, births, partnerships,
/// household, moved_out, carer, parents, social_parents); `File` adds the origin in the
/// pool.
type Living = (
    i64,
    u64,
    u8,
    Genes,
    Vec<TestAffliction>,
    u32,
    u32,
    u32,
    u64,
    bool,
    Option<u64>,
    Option<TestParents>,
    Vec<u64>,
);
/// Why a sim left: it died of the cause with this key.
#[derive(Clone, Copy, Serialize)]
enum TestLeftBy {
    Death { cause: u64 },
}

/// (born, left, reason, lineage_key, sex, genome, afflictions, conceptions, births,
/// partnerships, household, moved_out, parents, social_parents); `File` adds the origin
/// in the pool.
type Archived = (
    i64,
    i64,
    TestLeftBy,
    u64,
    u8,
    Genes,
    Vec<TestAffliction>,
    u32,
    u32,
    u32,
    u64,
    bool,
    Option<TestParents>,
    Vec<u64>,
);
/// (formed, ended)
type TestHousehold = (i64, Option<i64>);

/// A founder as `founder` of the tests builds it.
fn test_founder(born: i64) -> TestFounder {
    (born, FEMALE, Alleles(WILD, DUMMY))
}

/// The founder at `index` of the list of `test:tiny` as a living sim, a minor alone in
/// the household `index + 1` (ADR-0018 section 4).
fn living(index: usize, born: i64) -> Living {
    (
        born,
        TINY_KEYS[index],
        FEMALE,
        Genes(FEMALE, WILD, DUMMY),
        Vec::new(),
        0,
        0,
        0,
        u64::try_from(index).expect("small") + 1,
        false,
        None,
        None,
        Vec::new(),
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
        0,
        0,
        u64::try_from(index).expect("small") + 1,
        false,
        None,
        Vec::new(),
    )
}

/// (time, micro, seq, payload, period)
type QueueEntry = (i64, u16, u64, Payload, Option<i64>);
/// Why a partnership ended (ADR-0018 section 3).
#[derive(Clone, Copy, Serialize)]
enum TestEndedBy {
    Death,
    Separation,
}
/// (woman, man, formed, ended as (second, reason))
type TestPartnership = (u64, u64, i64, Option<(i64, TestEndedBy)>);
/// (per month, from years, until years, pregnancy months)
/// (father, the father's key from the pool, conception, children as (lineage_key, sex,
/// genome), conceived); the father is a sim of the town here and the key `None`.
type TestPregnancy = (Option<u64>, Option<u64>, u32, Vec<(u64, u8, Genes)>, i64);

/// A pregnancy with one child.
fn pregnant(
    father: u64,
    key: u64,
    conception: u32,
    sex: u8,
    genes: Genes,
    conceived: i64,
) -> TestPregnancy {
    (
        Some(father),
        None,
        conception,
        vec![(key, sex, genes)],
        conceived,
    )
}

/// The sims of a map, each followed by its origin in the pool (design note of the arrival,
/// section 6), the last field of a living and of an archived sim: none in these states,
/// one byte 0.
fn not_from_the_pool<S: serde::Serializer, T: Serialize>(
    sims: &[(u64, T)],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(sims.iter().map(|(id, sim)| (id, sim, None::<()>)))
}

/// Where the encoder puts the series of the market rounds (ADR-0018 section 1).
#[derive(Clone, Copy)]
enum Round {
    /// At the first month start after the clock, as the rule gives it.
    Due,
    /// At this second.
    At(i64),
    /// Not at all (its insertion number stays taken).
    Missing,
}

/// A save as the tests build it. The builders number the insertion numbers as a world
/// without the market would: the year end series 1, then the rest in the order they were
/// planned. `Start` plans the series of the market rounds right after the year end
/// (ADR-0018 section 1), so in the encoded save of a started world the round has the
/// number 2 and every later number is one higher; `bytes` puts it there.
#[derive(Clone, Serialize)]
struct File {
    #[serde(skip)]
    round: Round,
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
    closed_through: i64,
    next_sim_id: u64,
    next_partnership_id: u64,
    next_household_id: u64,
    queue: Vec<QueueEntry>,
    next_event_seq: u64,
    /// (id, sim); a list of pairs encodes like a map and can hold duplicates.
    #[serde(serialize_with = "not_from_the_pool")]
    sims: Vec<(u64, Living)>,
    partnerships: Vec<(u64, TestPartnership)>,
    households: Vec<(u64, TestHousehold)>,
    /// (mother, pregnancy)
    pregnancies: Vec<(u64, TestPregnancy)>,
    #[serde(serialize_with = "not_from_the_pool")]
    archive: Vec<(u64, Archived)>,
    /// The households of the pool used (ADR-0020 section 5): none here.
    pool_used: Vec<u32>,
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
            round: Round::Due,
            magic: *b"SIMAGENT",
            format_version: 2,
            core_version: 39,
            data_hash: DATA_HASH,
            projection_version: 18,
            world_seed: Some(0xff),
            world_params: Some(params(false)),
            closed_through: -1,
            next_sim_id: 2,
            next_partnership_id: 1,
            next_household_id: 2,
            queue: vec![
                (Y - 1, 0, 2, Payload::Birthday { sim: 1 }, None),
                (Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
                // She leaves home at 20 (ADR-0018 section 4), planned last by `Start`.
                (TINY_LEAVES, 0, 5, Payload::LeaveHome { sim: 1 }, None),
                (senescence, 0, 3, death(1, SENESCENCE), None),
                (accident, 0, 4, death(1, ACCIDENT), None),
            ],
            next_event_seq: 6,
            sims: vec![(1, living(0, -1))],
            partnerships: Vec::new(),
            // Her household, formed by `Start`.
            households: vec![(1, (0, None))],
            pregnancies: Vec::new(),
            archive: Vec::new(),
            pool_used: Vec::new(),
            plan_refs: vec![(2, 0), (1, 0), (2, 0), (2, 0), (2, 0)],
            next_event_id: 4,
            input_log: vec![(
                -1,
                1,
                TestInput::Start {
                    world_seed: 0xff,
                    founder_tag: "test:tiny".to_owned(),
                    world_params: params(false),
                    founders: vec![test_founder(-1)],
                    couples: Vec::new(),
                    capacity: None,
                    pool: None,
                },
            )],
            year_hashes: Vec::new(),
        }
    }

    fn bytes(&self) -> Vec<u8> {
        postcard::to_allocvec(&self.with_round()).expect("encodes")
    }

    /// The file with the round series in its place, the later insertion numbers moved up.
    fn with_round(&self) -> File {
        let mut file = self.clone();
        if file.world_seed.is_none() {
            return file;
        }
        for entry in &mut file.queue {
            if entry.2 >= 2 {
                entry.2 += 1;
            }
        }
        file.next_event_seq += 1;
        let at = match file.round {
            Round::Missing => return file,
            Round::At(at) => at,
            Round::Due => (file.closed_through.max(0).div_euclid(MONTH) + 1) * MONTH,
        };
        // In its second only births (class 200) come before the round (class 500, the
        // world before every sim and partnership, ADR-0002).
        let place = file
            .queue
            .iter()
            .position(|e| e.0 > at || (e.0 == at && !matches!(e.3, Payload::Birth { .. })))
            .unwrap_or(file.queue.len());
        file.queue
            .insert(place, (at, 0, 2, Payload::MarketRound, Some(MONTH)));
        file.plan_refs.insert(place, (1, 0));
        file
    }

    fn with(mut self, change: impl FnOnce(&mut File)) -> Vec<u8> {
        change(&mut self);
        self.bytes()
    }
}

#[test]
fn the_test_encoder_matches_the_reference() {
    assert_eq!(File::tiny().bytes(), hex(TINY_SAVE_AFTER_START));
    assert_eq!(FORMAT_VERSION, 2);
    assert_eq!(CORE_VERSION, 39);
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
    // Three more entries than before households: one `core:household_formed` per founder.
    assert_eq!(uninterrupted.log.len(), 37);

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
            trace: Vec::new(),
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
    assert_eq!(world.take_log().len(), 3, "the entries are still there");
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
            r#"{"id":4,"t":31449599,"subjects":[{"sim":1}],"causes":[{"plan":{"by":2,"planned_at":0}}],"payload":{"core:birthday":{"age_years":1}}}"#
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
    for version in [1, 3] {
        let bytes = File::tiny().with(|f| f.format_version = version);
        assert_eq!(load_error(&bytes), Some(LoadError::ForeignFormatVersion));
    }
}

/// ADR-0003 `load_rejects_foreign_core_version`.
#[test]
fn load_rejects_foreign_core_version() {
    for version in [38, 40] {
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
        f.format_version = 3;
        f.core_version = 40;
        f.data_hash = 7;
        f.projection_version = 19;
    };
    assert_eq!(
        load_error(&File::tiny().with(all_wrong)),
        Some(LoadError::ForeignFormatVersion)
    );
    let bytes = File::tiny().with(|f| {
        all_wrong(f);
        f.format_version = 2;
    });
    assert_eq!(load_error(&bytes), Some(LoadError::ForeignCoreVersion));
    let bytes = File::tiny().with(|f| {
        all_wrong(f);
        f.format_version = 2;
        f.core_version = 39;
    });
    assert_eq!(load_error(&bytes), Some(LoadError::ForeignDataHash));
    // The projection version is read before the fields whose layout it decides.
    let mut bytes = File::tiny().with(|f| f.projection_version = 19);
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
    for version in [17, 19] {
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
    // An unknown event variant: index 8 where 0 to 7 exist.
    let mut unknown = good.clone();
    // Behind the header: version, seed (3 bytes), world parameters (3 bytes: the switch
    // and no nutrition year), clock, sim counter, partnership counter, household counter,
    // queue length, then the first entry with its time (4 bytes), micro step and insertion
    // number: the first market round (variant 6, no fields), then its period (some).
    let payload_at = header + 12 + 4 + 2;
    assert_eq!(
        unknown[payload_at..payload_at + 2],
        [6, 1],
        "the first market round"
    );
    unknown[payload_at] = 8;
    assert_eq!(load_error(&unknown), Some(LoadError::Malformed));
}

/// ADR-0006 section 4: only the bytes `save` writes are accepted. serde and postcard
/// read more than that: repeated and unsorted map keys, varints longer than needed.
#[test]
fn load_rejects_non_canonical_bytes() {
    let repeated = File::tiny().with(|f| f.sims = vec![(1, living(0, -1)), (1, living(0, -1))]);
    assert_eq!(load_error(&repeated), Some(LoadError::Malformed));
    // Two founders, valid in every other respect, with the map keys in the wrong order.
    // Each is a minor in a household of her own; the second leaves home at 19
    // (`hand_leave_age` of tools/state_reference.py).
    let two = |sims: Vec<(u64, Living)>| {
        File::tiny().with(|f| {
            let [(her_senescence, her_accident), (senescence, accident)] = TINY_DEATHS;
            f.next_sim_id = 3;
            f.next_household_id = 3;
            f.sims = sims;
            f.households = vec![(1, (0, None)), (2, (0, None))];
            f.queue = vec![
                (Y - 1, 0, 2, Payload::Birthday { sim: 1 }, None),
                (Y - 1, 0, 5, Payload::Birthday { sim: 2 }, None),
                (Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
                (19 * Y - 1, 0, 9, Payload::LeaveHome { sim: 2 }, None),
                (TINY_LEAVES, 0, 8, Payload::LeaveHome { sim: 1 }, None),
                (her_senescence, 0, 3, death(1, SENESCENCE), None),
                (senescence, 0, 6, death(2, SENESCENCE), None),
                (accident, 0, 7, death(2, ACCIDENT), None),
                (her_accident, 0, 4, death(1, ACCIDENT), None),
            ];
            f.next_event_seq = 10;
            f.plan_refs = vec![
                (2, 0),
                (3, 0),
                (1, 0),
                (3, 0),
                (2, 0),
                (2, 0),
                (3, 0),
                (3, 0),
                (2, 0),
            ];
            f.next_event_id = 6;
            f.input_log[0].2 = TestInput::Start {
                world_seed: 0xff,
                founder_tag: "test:tiny".to_owned(),
                world_params: params(false),
                founders: vec![test_founder(-1), test_founder(-1)],
                couples: Vec::new(),
                capacity: None,
                pool: None,
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
    // and no nutrition year), clock.
    let at = hex(HEADER).len() + 8;
    assert_eq!(good[at], 2, "next_sim_id of the tiny world");
    let mut overlong = good.clone();
    overlong.splice(at..=at, [0x82, 0x00]);
    assert_eq!(load_error(&overlong), Some(LoadError::Malformed));
    // Length prefixes far beyond the input: rejected without reading that much. The
    // partnership and household counters lie between.
    assert_eq!(good[at + 1], 1, "next_partnership_id of the tiny world");
    assert_eq!(good[at + 2], 2, "next_household_id of the tiny world");
    let queue_len = at + 3;
    assert_eq!(good[queue_len], 6, "six pending events");
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
/// EventId is 5, and the input log has a second record.
fn overridden() -> File {
    let mut file = File::tiny();
    file.world_params = Some(params(true));
    file.next_event_id = 5;
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
            // No series fits behind the last second.
            f.round = Round::Missing;
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
const MORTAL_SAVE_AFTER_START: &str = "53494d4147454e54022797e4b4c08affa1e17e1201ff01010000010301030a80a8a7020002060180a8a702fc87ff1d0006010200fe87ff1d0003010100fe87ff1d000100018088ff1daaa1e380010005020185d2bc82f8b2ead44d00aea2e2a9010008020285d2bc82f8b2ead44d009ae1f9c50100070202a0bb8ca1c58498a01c009aadc9f70100040201a0bb8ca1c58498a01c00fc9fedd704000a070200fec7e8ed0500090701000b020101dded90d1a3f7cbf6b3010000000001000000000000000000000100000000000203cbdbe1f0b893cbc91e0101000001000000010000019aada6adcbb284bf7200000000000000000100000000000000000000000002000000000000020100000200000000000a01000300020001000200030003000200030002000601010100ff010b746573743a6d6f7274616c0000020100010f636f72653a646973656173655f61720200010301010f636f72653a646973656173655f617202010100000000";
const MORTAL_SAVE_AFTER_YEAR_5: &str = "53494d4147454e54022797e4b4c08affa1e17e1201ff01010000feaffab3010301030280b0fab3010002060180a8a702feb7f9d101000100018088ff1d14000002010001aaa1e38001020001aea2e2a90100020101aaa1e380010085d2bc82f8b2ead44ddded90d1a3f7cbf6b30100000000010000000000000000000001000000000203aea2e2a9010085d2bc82f8b2ead44dcbdbe1f0b893cbc91e0101000001000000010000019aada6adcbb284bf7200000000000000000100000000000000000000000002000000000002010001001101010100ff010b746573743a6d6f7274616c0000020100010f636f72653a646973656173655f61720200010301010f636f72653a646973656173655f61720201010000000600d9fdccb5e6a3efe84402b1a3f1e084d0baa7ab0104aa80efca968b8cbd7206aadfbda2d8c0e6efab0108a880dd8e95bcebe3720a8a9ab1db8ccef28e46";
/// The data hash of the test data of "mortal" (`tools/state_reference.py`).
const MORTAL_DATA_HASH: u64 = 0x7ec2_87f8_a80d_3217;
/// The lineage keys of the founders 0 and 1 of `test:mortal` (tools/rng_reference.py).
const MORTAL_KEYS: [u64; 2] = [0xb3ed_2fba_3a24_36dd, 0x1e93_2c9b_8e18_6dcb];
/// When each death is due, in seconds: hers of accident and of senescence, his of
/// accident and of senescence.
const HER_DEATH: i64 = 135_030_869;
const HER_SENESCENCE: i64 = 259_599_181;
const HIS_DEATH: i64 = 178_014_359;
const HIS_SENESCENCE: i64 = 207_566_925;
/// When they leave home: she at 25, he at 20 (`tools/state_reference.py`).
const HER_LEAVING: i64 = 25 * Y - 1;
const HIS_LEAVING: i64 = 20 * Y - 2;
const MORTAL_YEAR_HASHES: [u64; 6] = [
    0x44d1_bd1e_66b3_3ed9,
    0xab4e_ea80_4c1c_51b1,
    0x727a_3059_695b_c02a,
    0xabdf_9a05_844f_6faa,
    0x72c7_ade1_51d7_4028,
    0x461d_ca70_cb6c_4d0a,
];

/// The test data of "mortal".
fn mortal_data() -> Data {
    common::death_data(0.5, Some(0.05), common::repository_diseases())
}

/// The year hashes of "mortal" for the years before `years`.
fn mortal_years(years: usize) -> Vec<(i64, u64)> {
    (0..).zip(MORTAL_YEAR_HASHES).take(years).collect()
}

/// She is a minor alone in household 1, he in household 2 (ADR-0018 section 4).
fn she(born: i64) -> Living {
    (
        born,
        MORTAL_KEYS[0],
        FEMALE,
        Genes(FEMALE, WILD, DUMMY),
        Vec::new(),
        0,
        0,
        0,
        1,
        false,
        None,
        None,
        Vec::new(),
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
        0,
        0,
        2,
        false,
        None,
        None,
        Vec::new(),
    )
}

/// A living sim in the archive: it left at `left`, dead of the cause with the key `cause`.
fn dead(sim: Living, left: i64, cause: u64) -> Archived {
    let (
        born,
        key,
        sex,
        genes,
        exposure,
        conceptions,
        births,
        partnerships,
        household,
        moved_out,
        _,
        parents,
        social_parents,
    ) = sim;
    (
        born,
        left,
        TestLeftBy::Death { cause },
        key,
        sex,
        genes,
        exposure,
        conceptions,
        births,
        partnerships,
        household,
        moved_out,
        parents,
        social_parents,
    )
}

/// "mortal" right after `Start`: per founder the birthday and the deaths of senescence and
/// of accident, then the households and the days they leave home (she at 25, he at 20), in
/// the order of time.
fn mortal_started() -> File {
    let mut file = File::tiny();
    file.data_hash = MORTAL_DATA_HASH;
    file.next_sim_id = 3;
    file.next_household_id = 3;
    file.queue = vec![
        (Y - 2, 0, 5, Payload::Birthday { sim: 2 }, None),
        (Y - 1, 0, 2, Payload::Birthday { sim: 1 }, None),
        (Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
        (HER_DEATH, 0, 4, death(1, ACCIDENT), None),
        (HIS_DEATH, 0, 7, death(2, ACCIDENT), None),
        (HIS_SENESCENCE, 0, 6, death(2, SENESCENCE), None),
        (HER_SENESCENCE, 0, 3, death(1, SENESCENCE), None),
        (HIS_LEAVING, 0, 9, Payload::LeaveHome { sim: 2 }, None),
        (HER_LEAVING, 0, 8, Payload::LeaveHome { sim: 1 }, None),
    ];
    file.next_event_seq = 10;
    file.sims = vec![(1, she(-1)), (2, he(-2))];
    file.households = vec![(1, (0, None)), (2, (0, None))];
    file.plan_refs = vec![
        (3, 0),
        (2, 0),
        (1, 0),
        (2, 0),
        (3, 0),
        (3, 0),
        (2, 0),
        (3, 0),
        (2, 0),
    ];
    file.next_event_id = 6;
    file.input_log[0].2 = TestInput::Start {
        world_seed: 0xff,
        founder_tag: "test:mortal".to_owned(),
        world_params: params(false),
        founders: vec![
            (-1, FEMALE, Alleles(WILD, DUMMY)),
            (-2, MALE, Alleles(DUMMY, DUMMY)),
        ],
        couples: Vec::new(),
        capacity: None,
        pool: None,
    };
    file
}

/// "mortal" at the end of year 5: both are in the archive, only the year end series is
/// left in the queue.
fn mortal_after_year_5() -> File {
    let mut file = mortal_started();
    file.closed_through = 6 * Y - 1;
    file.queue = vec![(7 * Y - 1, 0, 1, Payload::YearEnd, Some(Y))];
    file.next_event_seq = 19;
    file.sims.clear();
    file.households = vec![(1, (0, Some(HER_DEATH))), (2, (0, Some(HIS_DEATH)))];
    file.archive = vec![
        (1, dead(she(-1), HER_DEATH, ACCIDENT)),
        (2, dead(he(-2), HIS_DEATH, ACCIDENT)),
    ];
    file.plan_refs = vec![(1, 0)];
    file.next_event_id = 17;
    file.year_hashes = mortal_years(6);
    file
}

/// "mortal" right after her death of accident: she is in the archive, his events are
/// pending (his birthday was planned anew by entry 10 at his birthday in year 3).
fn mortal_after_her_death() -> File {
    let mut file = mortal_started();
    file.closed_through = HER_DEATH;
    file.queue = vec![
        (5 * Y - 2, 0, 16, Payload::Birthday { sim: 2 }, None),
        (5 * Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
        (HIS_DEATH, 0, 7, death(2, ACCIDENT), None),
        (HIS_SENESCENCE, 0, 6, death(2, SENESCENCE), None),
        (HIS_LEAVING, 0, 9, Payload::LeaveHome { sim: 2 }, None),
    ];
    file.next_event_seq = 18;
    file.sims = vec![(2, he(-2))];
    file.households = vec![(1, (0, Some(HER_DEATH))), (2, (0, None))];
    file.archive = vec![(1, dead(she(-1), HER_DEATH, ACCIDENT))];
    file.plan_refs = vec![(12, 4 * Y - 2), (1, 0), (3, 0), (3, 0), (3, 0)];
    file.next_event_id = 15;
    file.year_hashes = mortal_years(4);
    file
}

// Scenario "tie" of tools/state_reference.py: her senescence and her accident are due in
// the same second (test data: accident 0.2, senescence `a` 0.8046949992648321, `b` 0.001).
const TIE_SAVE_AFTER_START: &str = "53494d4147454e540227ebe4c889ffd0bdbadf011201ff01010000010201020680a8a7020002060180a8a702fe87ff1d0003010100fe87ff1d000100018088ff1dc8ddf17000040201a0bb8ca1c58498a01c00c8ddf1700005020185d2bc82f8b2ead44d00fe87f0fd03000607010007010101f0da979a8fd9f18a95010000000001000000000000000000000100000000000001010000000000060100020001000200020002000401010100ff0108746573743a7469650000010100010f636f72653a646973656173655f617202000100000000";
/// The data hash of the test data of "tie" (`tools/state_reference.py`).
const TIE_DATA_HASH: u64 = 0xdf74_f687_f132_326b;
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
    file.queue = vec![
        (Y - 1, 0, 2, Payload::Birthday { sim: 1 }, None),
        (Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
        (TIE_DUE, 0, 3, death(1, SENESCENCE), None),
        (TIE_DUE, 0, 4, death(1, ACCIDENT), None),
        // She leaves home at 17.
        (17 * Y - 1, 0, 5, Payload::LeaveHome { sim: 1 }, None),
    ];
    file.sims[0].1.1 = TIE_KEY;
    file.input_log[0].2 = TestInput::Start {
        founder_tag: "test:tie".to_owned(),
        world_seed: 0xff,
        world_params: params(false),
        founders: vec![test_founder(-1)],
        couples: Vec::new(),
        capacity: None,
        pool: None,
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
        (2 * Y - 2, 0, 10, Payload::Birthday { sim: 2 }, None),
        (2 * Y - 1, 0, 11, Payload::Birthday { sim: 1 }, None),
        (2 * Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
        (HER_DEATH, 0, 4, death(1, ACCIDENT), None),
        (HIS_DEATH, 0, 7, death(2, ACCIDENT), None),
        (HIS_SENESCENCE, 0, 6, death(2, SENESCENCE), None),
        (HER_SENESCENCE, 0, 3, death(1, SENESCENCE), None),
        (HIS_LEAVING, 0, 9, Payload::LeaveHome { sim: 2 }, None),
        (HER_LEAVING, 0, 8, Payload::LeaveHome { sim: 1 }, None),
    ];
    file.next_event_seq = 12;
    file.plan_refs = vec![
        (6, Y - 2),
        (7, Y - 1),
        (1, 0),
        (2, 0),
        (3, 0),
        (3, 0),
        (2, 0),
        (3, 0),
        (2, 0),
    ];
    file.next_event_id = 8;
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
            capacity: None,
            pool: None,
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
                        Payload::Death { sim: 1, .. }
                            | Payload::Birthday { sim: 1 }
                            | Payload::LeaveHome { sim: 1 }
                    )
                });
                f.plan_refs = vec![(6, Y - 2), (1, 0), (3, 0), (3, 0), (3, 0)];
                f.households[0].1.1 = Some(HER_DEATH);
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
                let entry = (HIS_SENESCENCE + 1, 0, 10, death(2, ar_key()), None);
                insert_event(f, entry, (3, 0));
                f.next_event_seq = 11;
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
            .insert(4, (HER_DEATH, 0, 10, death(1, ACCIDENT), None));
        f.plan_refs.insert(4, (2, 0));
        f.next_event_seq = 11;
    });
    assert_eq!(
        load_error_with(mortal_data(), &twice),
        Some(LoadError::Inconsistent)
    );
}

// Scenario "sick" of tools/state_reference.py, on test data with accident 0.02 per year,
// senescence practically off and `test:dummy_disease` with 0.3 per year from birth on for
// the homozygous, while the switch is on. She has one dummy allele, he has two.
const SWITCHED_ON_SAVE: &str = "53494d4147454e540227b5c9adf9d0a9a1ea8d011201ff01010100fe87ff1d0301030b8088ff1d0002060180a8a702fc8ffe3b000b010200fe8ffe3b000c010100fe8ffe3b000100018088ff1dc6d6ae7e000d0202dd90d4c2cac3a7bbf40100fc8fef9b04000a070200fe9fedd7040009070100eedadbe40d0008020285d2bc82f8b2ead44d00a695b0d0280005020185d2bc82f8b2ead44d00bc87cbdac42a00070202a0bb8ca1c58498a01c00dc84cbc8823300040201a0bb8ca1c58498a01c000e020101abd4f19caceffaefab010000000001000000000000000000000100000000000203fba99785c4c1bc5c010100000100000001000001dd90d4c2cac3a7bbf4010000000000000000010000000000000000018088ff1d00000002000000000000020100000200000000000b010006fc87ff1d07fe87ff1d0100088088ff1d0300020003000200030002000902010100ff0109746573743a7369636b0000020100010f636f72653a646973656173655f61720200010301010f636f72653a646973656173655f6172020101000000fe87ff1d0201010001010088afc4a7f58dd4cd56";
const SICK_ON_SAVE_AFTER_YEAR_3: &str = "53494d4147454e540227b5c9adf9d0a9a1ea8d011201ff01010100fe9ffc770301030680a0fc770002060180a8a702fea7fb95010012010100fea7fb9501000100018088ff1dfe9fedd704000a070100a695b0d0280005020185d2bc82f8b2ead44d00dc84cbc8823300040201a0bb8ca1c58498a01c0013010101abd4f19caceffaefab010000000001000000000000000000000100000000000002010000020001c6ceaf6000010203c6ceaf6000dd90d4c2cac3a7bbf401fba99785c4c1bc5c010100000100000001000001dd90d4c2cac3a7bbf401000000000000000001000000000000000001000000000200000000000601000dfe9ffc7701000200020002000e01010100ff0109746573743a7369636b0100020100010f636f72653a646973656173655f61720200010301010f636f72653a646973656173655f61720201010000000400fdacb382f5f9b0b16802e7859ec4ac85ebb29c0104c1ea83e4f6c7a5a792010698f9d4d3868dc3b501";
const OFF_AT_DUE_SAVE: &str = "53494d4147454e540227b5c9adf9d0a9a1ea8d011201ff01010000fea7fb95010301030a80a8fb95010002060180a8a702fcaffab3010014010200feaffab3010015010100feaffab301000100018088ff1dfc8fef9b04000b070200fe9fedd704000a070100eedadbe40d0008020285d2bc82f8b2ead44d00a695b0d0280005020185d2bc82f8b2ead44d00bc87cbdac42a00070202a0bb8ca1c58498a01c00dc84cbc8823300040201a0bb8ca1c58498a01c0016020101abd4f19caceffaefab010000000001000000000000000000000100000000000203fba99785c4c1bc5c010100000100000001000001dd90d4c2cac3a7bbf401000000000000000001c3992b7cb8d8ee3f0000000002000000000000020100000200000000000a01000ffca7fb950110fea7fb950101000300020003000200030002001102010100ff0109746573743a7369636b0100020100010f636f72653a646973656173655f61720200010301010f636f72653a646973656173655f6172020101000000c4ceaf6002010100000500fdacb382f5f9b0b16802e7859ec4ac85ebb29c0104c1ea83e4f6c7a5a7920106cba5c296f4849f847c088cb6f0c3a4b282d28101";
/// The lineage keys of the founders 0 and 1 of `test:sick` (tools/rng_reference.py).
const SICK_KEYS: [u64; 2] = [0xabdf_eb7a_c39c_6a2b, 0x00b8_f20c_40a5_d4fb];
/// The `str_key` of `test:dummy_disease`.
const DISEASE: u64 = 0xf476_9e1c_a855_085d;
/// The data hash of the test data with the dummy disease from birth on
/// (`tools/state_reference.py`).
const SICK_DATA_HASH: u64 = 0x8dd4_854d_0f2b_64b5;
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

/// The days both leave home (she at 20, he at 18) with the insertion number of hers, his
/// is the next one before it (`Start` plans his after hers).
fn sick_leaving(hers: u64) -> Vec<(QueueEntry, (u64, i64))> {
    vec![
        (
            (18 * Y - 2, 0, hers + 1, Payload::LeaveHome { sim: 2 }, None),
            (3, 0),
        ),
        (
            (20 * Y - 1, 0, hers, Payload::LeaveHome { sim: 1 }, None),
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
        capacity: None,
        pool: None,
    }
}

/// She is a minor alone in household 1, he in household 2.
fn sick_she() -> Living {
    (
        -1,
        SICK_KEYS[0],
        FEMALE,
        Genes(FEMALE, WILD, DUMMY),
        Vec::new(),
        0,
        0,
        0,
        1,
        false,
        None,
        None,
        Vec::new(),
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
        0,
        0,
        2,
        false,
        None,
        None,
        Vec::new(),
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
    file.next_household_id = 3;
    let events = [
        (
            (2 * Y - 2, 0, 10, Payload::Birthday { sim: 2 }, None),
            (6, Y - 2),
        ),
        (
            (2 * Y - 1, 0, 11, Payload::Birthday { sim: 1 }, None),
            (7, Y - 1),
        ),
        ((2 * Y - 1, 0, 1, Payload::YearEnd, Some(Y)), (1, 0)),
        ((Y + HIS_DISEASE, 0, 12, death(2, DISEASE), None), (8, Y)),
    ];
    (file.queue, file.plan_refs) = events
        .into_iter()
        .chain(sick_leaving(8))
        .chain(sick_deaths())
        .unzip();
    file.next_event_seq = 13;
    file.sims = vec![(1, sick_she()), (2, sick_he((0.0, Some(Y))))];
    file.households = vec![(1, (0, None)), (2, (0, None))];
    file.next_event_id = 9;
    file.input_log = vec![(-1, 1, sick_start(false)), (Y - 1, 2, override_block(true))];
    file.year_hashes = vec![(0, 0x569b_506f_54f1_1788)];
    file
}

/// "sick" with the switch on from the start, at the end of year 3: he died of the
/// disease, his exposure is kept as it was then.
fn sick_on_after_year_3() -> File {
    let mut file = sick_switched_on();
    file.closed_through = 4 * Y - 1;
    file.queue = vec![
        (5 * Y - 1, 0, 17, Payload::Birthday { sim: 1 }, None),
        (5 * Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
        (20 * Y - 1, 0, 9, Payload::LeaveHome { sim: 1 }, None),
        (HER_SICK_ACCIDENT, 0, 4, death(1, ACCIDENT), None),
        (HER_SICK_SENESCENCE, 0, 3, death(1, SENESCENCE), None),
    ];
    file.next_event_seq = 18;
    file.sims = vec![(1, sick_she())];
    file.households = vec![(1, (0, None)), (2, (0, Some(HIS_DISEASE)))];
    file.archive = vec![(2, dead(sick_he((0.0, Some(0))), HIS_DISEASE, DISEASE))];
    file.plan_refs = vec![(13, 4 * Y - 1), (1, 0), (2, 0), (2, 0), (2, 0)];
    file.next_event_id = 14;
    file.input_log = vec![(-1, 1, sick_start(true))];
    file.year_hashes = vec![
        (0, 0x6862_c3cf_504c_d67d),
        (1, 0x9c65_ac2a_c887_82e7),
        (2, 0x924e_963f_6c80_f541),
        (3, 0x016b_0c68_6a75_3c98),
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
            (6 * Y - 2, 0, 19, Payload::Birthday { sim: 2 }, None),
            (15, 5 * Y - 2),
        ),
        (
            (6 * Y - 1, 0, 20, Payload::Birthday { sim: 1 }, None),
            (16, 5 * Y - 1),
        ),
        ((6 * Y - 1, 0, 1, Payload::YearEnd, Some(Y)), (1, 0)),
    ];
    (file.queue, file.plan_refs) = events
        .into_iter()
        .chain(sick_leaving(9))
        .chain(sick_deaths())
        .unzip();
    file.next_event_seq = 21;
    file.sims = vec![(1, sick_she()), (2, sick_he((OFF_AT_DUE_USED, None)))];
    file.next_event_id = 17;
    file.input_log = vec![
        (-1, 1, sick_start(true)),
        (HIS_DISEASE - 1, 2, override_block(false)),
    ];
    file.year_hashes = vec![
        (0, 0x6862_c3cf_504c_d67d),
        (1, 0x9c65_ac2a_c887_82e7),
        (2, 0x924e_963f_6c80_f541),
        (3, 0x7c08_7c27_42d0_92cb),
        (4, 0x81a4_0992_487c_1b0c),
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
                    .insert(4, (Y + HIS_DISEASE + 1, 0, 13, death(1, DISEASE), None));
                f.plan_refs.insert(4, (8, Y));
                f.next_event_seq = 14;
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
const PARTNERS_SAVE_AFTER_ARRANGE: &str = "53494d4147454e540227d7cf8aa4948791e3591201ff01010000fe87ff1d0403040a8088ff1d0002060180a8a70280b0a620001103010180a8a702cce2e6370005020185d2bc82f8b2ead44d00fa8ffe3b000f010300fe8ffe3b0010010100fe8ffe3b000100018088ff1dd89aac3e00040201a0bb8ca1c58498a01c00c8c8e28003000a0203a0bb8ca1c58498a01c00e2c8e9e2050012050200aea1c18a0a000b020385d2bc82f8b2ead44d001302018180f1df03aa90d4c3eeedd09bdb01000000000100000000000000000002030100000000038580f1df03a7f3f8fdb9ee88cfed01010100000000000001000000000000030100000000020101020001fa89fd05000201038088ff1d00030100018088ff1d0200018088ff1d038088ff1d000001028380f1df03fa89fd0500a0bb8ca1c58498a01cf098dcaaccecf99cb0010101000001000000010000019aada6adcbb284bf720000000000000000010000000000000000000000000101000000000a01000b8088ff1d020009fa87ff1d0afe87ff1d0100020004000b8088ff1d04000f02010100ff010d746573743a706172746e6572730000038180f1df0300010f636f72653a646973656173655f61720200018380f1df0301010f636f72653a646973656173655f61720201018580f1df0301010f636f72653a646973656173655f61720201000100010000fe87ff1d020201030100b0abcfccd2d2b5f458";
const PARTNERS_SAVE_AFTER_YEAR_4: &str = "53494d4147454e540227d7cf8aa4948791e3591201ff01010000fea7fb95010403040580a8fb95010002060180a8a702faaffab3010016010300feaffab301000100018088ff1dc8c8e28003000a0203a0bb8ca1c58498a01c00aea1c18a0a000b020385d2bc82f8b2ead44d001701038580f1df03a7f3f8fdb9ee88cfed01010100000000000001000000000000030100000000020101020001fa89fd05000201038088ff1d01cce2e63700030100018088ff1d0200018088ff1d038088ff1d000002018180f1df03cce2e6370085d2bc82f8b2ead44daa90d4c3eeedd09bdb010000000001000000000000000000020301000000028380f1df03fa89fd0500a0bb8ca1c58498a01cf098dcaaccecf99cb0010101000001000000010000019aada6adcbb284bf7200000000000000000100000000000000000000000001010000000005010013faa7fb95010100040004001402010100ff010d746573743a706172746e6572730000038180f1df0300010f636f72653a646973656173655f61720200018380f1df0301010f636f72653a646973656173655f61720201018580f1df0301010f636f72653a646973656173655f61720201000100010000fe87ff1d020201030500b0abcfccd2d2b5f458028994ccc3c0d3e6d50304b595d39fd18bb5833c06b4a3b4b1efc9b2dd2c08a7a9949694ebbdd7d601";
/// The lineage keys of the founders 0 to 2 of `test:partners` (tools/rng_reference.py).
const PARTNERS_KEYS: [u64; 3] = [
    0xdb37_436e_e875_082a,
    0xb039_e764_c557_0c70,
    0xed9e_2373_9fbe_39a7,
];
/// The data hash of the test data of "partners" (`tools/state_reference.py`).
const PARTNERS_DATA_HASH: u64 = 0x59c6_4439_4482_a7d7;
/// When each of the three dies of senescence and of accident, if nothing comes first.
const PARTNER_SENESCENCE: [i64; 3] = [65_373_868, 6_267_517, 403_460_644];
const PARTNER_ACCIDENT: [i64; 3] = [58_513_574, 273_890_542, 1_353_197_655];
/// When she and the first man die.
const PARTNER_HER_DEATH: i64 = PARTNER_ACCIDENT[0];
const PARTNER_HIS_DEATH: i64 = PARTNER_SENESCENCE[1];
/// When the second partnership of "partners" would separate if both lived (ADR-0018
/// section 3); her death comes first.
const PARTNERS_SECOND_SEPARATION: i64 = 774_713_905;

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
            (-16 * Y - 1, FEMALE, Alleles(WILD, DUMMY)),
            (-16 * Y - 2, MALE, Alleles(DUMMY, DUMMY)),
            (-16 * Y - 3, MALE, Alleles(DUMMY, WILD)),
        ],
        couples,
        capacity: None,
        pool: None,
    }
}

/// The founder at `index` of "partners" as a living sim, after the second partnership: the
/// first man in the household of the first couple (1), she and the second man in the
/// household of the second (3); all moved out.
fn partner(index: usize) -> Living {
    match index {
        0 => (
            -16 * Y - 1,
            PARTNERS_KEYS[0],
            FEMALE,
            Genes(FEMALE, WILD, DUMMY),
            Vec::new(),
            0,
            0,
            0,
            3,
            true,
            None,
            None,
            Vec::new(),
        ),
        1 => (
            -16 * Y - 2,
            PARTNERS_KEYS[1],
            MALE,
            Genes(MALE, DUMMY, DUMMY),
            ar((0.0, None)),
            0,
            0,
            0,
            1,
            true,
            None,
            None,
            Vec::new(),
        ),
        _ => (
            -16 * Y - 3,
            PARTNERS_KEYS[2],
            MALE,
            Genes(MALE, DUMMY, WILD),
            Vec::new(),
            0,
            0,
            0,
            3,
            true,
            None,
            None,
            Vec::new(),
        ),
    }
}

/// "partners" right after her second partnership: the first man is in the archive, the
/// first partnership ended with his death, the second formed at the start of year 1 by
/// the input with sequence number 2. The couple of `Start` formed household 1, the second
/// man (16) household 2, where his day of leaving home (insertion number 13) was planned;
/// the second partnership moved both into household 3 and cancelled it.
fn partners_arranged() -> File {
    let mut file = File::tiny();
    file.data_hash = PARTNERS_DATA_HASH;
    file.closed_through = Y - 1;
    file.next_sim_id = 4;
    file.next_partnership_id = 3;
    file.next_household_id = 4;
    file.queue = vec![
        // Her attempts from the first month start after the pairing (the series of the
        // first partnership ended with his death), by the entry of the second partnership;
        // she tries at chance 0.2 and never conceives.
        (
            14 * MONTH,
            0,
            16,
            Payload::ConceptionAttempt { woman: 1 },
            Some(MONTH),
        ),
        (PARTNER_HER_DEATH, 0, 4, death(1, ACCIDENT), None),
        (2 * Y - 3, 0, 14, Payload::Birthday { sim: 3 }, None),
        (2 * Y - 1, 0, 15, Payload::Birthday { sim: 1 }, None),
        (2 * Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
        (PARTNER_SENESCENCE[0], 0, 3, death(1, SENESCENCE), None),
        (PARTNER_SENESCENCE[2], 0, 9, death(3, SENESCENCE), None),
        // The separation of the second partnership, planned by its entry after her
        // attempts (ADR-0018 section 3); the first one's was cancelled by his death.
        (
            PARTNERS_SECOND_SEPARATION,
            0,
            17,
            Payload::Separation { partnership: 2 },
            None,
        ),
        (PARTNER_ACCIDENT[2], 0, 10, death(3, ACCIDENT), None),
    ];
    file.next_event_seq = 18;
    // She formed two partnerships.
    let mut her = partner(0);
    her.7 = 2;
    file.sims = vec![(1, her), (3, partner(2))];
    file.partnerships = vec![
        (1, (1, 2, 0, Some((PARTNER_HIS_DEATH, TestEndedBy::Death)))),
        (2, (1, 3, Y, None)),
    ];
    file.households = vec![(1, (0, Some(Y))), (2, (0, Some(Y))), (3, (Y, None))];
    file.archive = vec![(2, dead(partner(1), PARTNER_HIS_DEATH, SENESCENCE))];
    file.plan_refs = vec![
        (11, Y),
        (2, 0),
        (9, Y - 3),
        (10, Y - 1),
        (1, 0),
        (2, 0),
        (4, 0),
        (11, Y),
        (4, 0),
    ];
    file.next_event_id = 15;
    file.input_log = vec![
        (-1, 1, partners_start(vec![(0, 1)])),
        (Y - 1, 2, TestInput::ArrangePartnership { woman: 1, man: 3 }),
    ];
    file.year_hashes = vec![(0, 0x58e8_d695_2993_d5b0)];
    file
}

/// "partners" at the end of year 4: she and the first man are in the archive, the second
/// partnership ended with her death in year 1, the second man lives on.
fn partners_after_year_4() -> File {
    let mut file = partners_arranged();
    file.closed_through = 5 * Y - 1;
    file.queue = vec![
        (6 * Y - 3, 0, 21, Payload::Birthday { sim: 3 }, None),
        (6 * Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
        (PARTNER_SENESCENCE[2], 0, 9, death(3, SENESCENCE), None),
        (PARTNER_ACCIDENT[2], 0, 10, death(3, ACCIDENT), None),
    ];
    file.next_event_seq = 22;
    let her = file.sims[0].1.clone();
    file.sims = vec![(3, partner(2))];
    file.partnerships[1].1.3 = Some((PARTNER_HER_DEATH, TestEndedBy::Death));
    file.archive = vec![
        (1, dead(her, PARTNER_HER_DEATH, ACCIDENT)),
        (2, dead(partner(1), PARTNER_HIS_DEATH, SENESCENCE)),
    ];
    file.plan_refs = vec![(19, 5 * Y - 3), (1, 0), (4, 0), (4, 0)];
    file.next_event_id = 20;
    file.year_hashes = vec![
        (0, 0x58e8_d695_2993_d5b0),
        (1, 0x03ab_9a9c_0873_0a09),
        (2, 0x3c06_d45d_13f4_cab5),
        (3, 0x2cba_ca4e_f62d_11b4),
        (4, 0xd6ae_f759_42c5_14a7),
    ];
    file
}

/// The place of the separation of the second partnership in the queue of
/// `partners_arranged`.
const SECOND_SEPARATION_AT: usize = 7;

/// Each woman counts her partnerships, a man none; each open partnership has its
/// separation exactly at the second of its budget, an ended one none; an end by separation
/// only where the separation came before the first death (ADR-0018 sections 3 and 7).
#[test]
fn load_checks_the_separations() {
    assert!(matches!(
        partners_arranged().queue[SECOND_SEPARATION_AT].3,
        Payload::Separation { partnership: 2 }
    ));
    let cases: [(&str, Vec<u8>); 6] = [
        (
            "a woman whose count misses a partnership",
            partners_arranged().with(|f| f.sims[0].1.7 = 1),
        ),
        (
            "a man with a count of partnerships",
            partners_arranged().with(|f| f.sims[1].1.7 = 1),
        ),
        (
            "an open partnership without its separation",
            partners_arranged().with(|f| {
                take_event(f, SECOND_SEPARATION_AT);
            }),
        ),
        (
            "a separation a second after its budget ran out",
            partners_arranged().with(|f| f.queue[SECOND_SEPARATION_AT].0 += 1),
        ),
        (
            "a separation of an ended partnership",
            partners_after_year_4().with(|f| {
                let separation = Payload::Separation { partnership: 2 };
                let entry = (PARTNERS_SECOND_SEPARATION, 0, 16, separation, None);
                insert_event(f, entry, (9, Y));
            }),
        ),
        (
            "a partnership ended by separation where her death came first",
            partners_after_year_4().with(|f| {
                f.partnerships[1].1.3 = Some((PARTNER_HER_DEATH, TestEndedBy::Separation));
            }),
        ),
    ];
    let accepted: Vec<&str> = cases
        .iter()
        .filter(|(_, bytes)| partners_load_error(bytes) != Some(LoadError::Inconsistent))
        .map(|(name, _)| *name)
        .collect();
    assert_eq!(accepted, Vec::<&str>::new());
}

/// One series of market rounds, at the first month start after the clock; a partnership
/// not formed by an input was formed by the market in a closed round second (ADR-0018
/// sections 1 and 7).
#[test]
fn load_checks_the_market() {
    let without_input = |f: &mut File| {
        f.input_log.pop();
    };
    let cases: [(&str, Vec<u8>); 5] = [
        (
            "no series of rounds",
            partners_arranged().with(|f| f.round = Round::Missing),
        ),
        (
            "the next round a month late",
            partners_arranged().with(|f| f.round = Round::At(14 * MONTH)),
        ),
        (
            "two series of rounds",
            partners_arranged().with(|f| {
                let second = (14 * MONTH, 0, 99, Payload::MarketRound, Some(MONTH));
                insert_event(f, second, (1, 0));
                f.next_event_seq = 100;
            }),
        ),
        (
            "a partnership without its input in a round second not yet closed",
            partners_arranged().with(without_input),
        ),
        (
            "a partnership without its input off the month starts",
            partners_after_year_4().with(|f| {
                without_input(f);
                f.partnerships[1].1.2 = Y + 1;
            }),
        ),
    ];
    let accepted: Vec<&str> = cases
        .iter()
        .filter(|(_, bytes)| partners_load_error(bytes) != Some(LoadError::Inconsistent))
        .map(|(name, _)| *name)
        .collect();
    assert_eq!(accepted, Vec::<&str>::new());
    // The control: the same partnership in the closed round second of month 13, formed
    // by the market (the loader does not recompute the choice).
    let by_market = partners_after_year_4().with(without_input);
    assert_eq!(partners_load_error(&by_market), None);
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
                // All else as that pairing gives it: her series from the clock on (month
                // 13), the separation as long after the pairing as before (B / λ).
                f.queue[0].0 = 13 * MONTH;
                f.queue[SECOND_SEPARATION_AT].0 =
                    PARTNER_HIS_DEATH + (PARTNERS_SECOND_SEPARATION - Y);
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
                f.partnerships[1].1 = (1, 2, Y, Some((PARTNER_HIS_DEATH, TestEndedBy::Death)));
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
            partners_arranged().with(|f| {
                f.partnerships[0].1.3 = Some((PARTNER_HIS_DEATH + 1, TestEndedBy::Death))
            }),
        ),
        (
            "an ended partnership of two living sims",
            partners_arranged().with(|f| f.partnerships[1].1.3 = Some((Y, TestEndedBy::Death))),
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
            partners_after_year_4().with(|f| {
                f.partnerships[1].1.3 = Some((PARTNER_HER_DEATH - 1, TestEndedBy::Death))
            }),
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

// Scenario "clan" of tools/state_reference.py, on test data with accident 0.02 per year,
// senescence practically off and the fertility of the old model (ADR-0017 section 5: the
// woman's window 18 to 45, 0.25 per attempt, ten months, no pause, wish 40): three founder
// couples; the first woman conceives in month 5.
const CLAN_SAVE_AFTER_CONCEPTION: &str = "53494d4147454e540227f199a1d1dcf4da8da7011201ff0101000080c8c40b0704041680f0eb0d0002060180a8a70280f0eb0d001603030180a8a702fe87ff1d000100018088ff1d8088ff1d00170101008088ff1d00180102008088ff1d00190103008088ff1d001a0104008088ff1d001b0105008088ff1d001c01060080d8cd22001d040100c4e9bcc101000b020385d2bc82f8b2ead44d00c6d0d8e3010014020685d2bc82f8b2ead44d00f4eba580020005020185d2bc82f8b2ead44d00f0e8f7c90a0011020585d2bc82f8b2ead44d00e0bd98dd10000e020485d2bc82f8b2ead44d00e6caef83400008020285d2bc82f8b2ead44d00baa8afa1f52d00100205a0bb8ca1c58498a01c00e48880ebba30000a0203a0bb8ca1c58498a01c00d4ee8eed963200070202a0bb8ca1c58498a01c0088d1a4e7ec3300130206a0bb8ca1c58498a01c008cfefad2ec3400040201a0bb8ca1c58498a01c009cd68493ae36000d0204a0bb8ca1c58498a01c001e0601ff9fedd704dded83919698d6d0d00100000000010000000000000001000101010000000002ffafeb9305f6eb9bebea9bd7c53e01010000000000000100000000000001010000000003fffff0df0385b2a2a3e3ecdafac4010001000001000000000000019aada6adcbb284bf7200000000000000000100000000000000000000000102010000000004ffefe38307f9a4bfd6bdf093fea70101000000000000000100000000000002010000000005ffefd4e30afacbdcacb9abf1870300000000000000000000000000000103010000000006ffefd4e30af1c5c4b2d9a691fded01010000000000000001000000000000030100000000030101020000020304000003050600000301000002000003000001010102000001f3c89ad6cdbbe183cd01010100000100000001000080c8c40b0000160100090001000e000f0010001100120013001480c8c40b0400070002000600050003000600040003000700020005001701010100ff0109746573743a636c616e000006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100fffff0df0300010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a010003000102030405000000";
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
const CLAN_DATA_HASH: u64 = 0xa71b_6ba5_ca28_4cf1;
/// The child she conceived in month 5, and when each of the six dies of senescence and of
/// accident, if nothing comes first.
const CLAN_CHILD: u64 = 0xcd07_85dc_dac6_a473;
const CLAN_DEATHS: [(i64, i64); 6] = [
    (907_935_702_918, 268_745_466),
    (862_060_665_770, 8_593_994_419),
    (832_530_547_250, 202_873_442),
    (933_906_912_654, 2_245_201_776),
    (788_832_578_077, 1_419_704_888),
    (890_777_146_436, 238_752_803),
];

/// The test data of "clan".
fn clan_data() -> Data {
    let (mut calibration, genome_map) =
        common::death_files(0.02, None, common::repository_diseases());
    let mut demography = common::demography();
    common::window_fertility(&mut calibration, &mut demography, 0.25);
    Data::new(common::definitions_with(demography, calibration, genome_map).expect("definitions"))
        .expect("checked")
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
            founder(16, FEMALE, Alleles(DUMMY, DUMMY)),
            founder(30, MALE, Alleles(WILD, WILD)),
            founder(46, FEMALE, Alleles(WILD, WILD)),
            founder(46, MALE, Alleles(WILD, WILD)),
        ],
        couples: vec![(0, 1), (2, 3), (4, 5)],
        capacity: None,
        pool: None,
    }
}

/// The founders of "clan" as living sims: the young woman of 16 has the dummy allele twice;
/// each couple lives in a household of its own.
fn clan_sims() -> Vec<(u64, Living)> {
    let sim = |n: usize, years: i64, sex: u8, genes: Genes| {
        let afflictions = if genes == Genes(sex, DUMMY, DUMMY) {
            ar((0.0, None))
        } else {
            Vec::new()
        };
        (
            u64::try_from(n).expect("a small place") + 1,
            (
                -years * Y,
                CLAN_KEYS[n],
                sex,
                genes,
                afflictions,
                0,
                0,
                // Every woman of "clan" is in a couple of `Start`.
                u32::from(sex == FEMALE),
                // Each couple of `Start` founds a household, in list order.
                u64::try_from(n / 2).expect("a small place") + 1,
                true,
                None,
                None,
                Vec::new(),
            ),
        )
    };
    vec![
        sim(0, 20, FEMALE, Genes(FEMALE, WILD, DUMMY)),
        sim(1, 22, MALE, Genes(MALE, DUMMY, WILD)),
        sim(2, 16, FEMALE, Genes(FEMALE, DUMMY, DUMMY)),
        sim(3, 30, MALE, Genes(MALE, WILD, WILD)),
        sim(4, 46, FEMALE, Genes(FEMALE, WILD, WILD)),
        sim(5, 46, MALE, Genes(MALE, WILD, WILD)),
    ]
}

/// "clan" right after the conception in month 5: the first woman is pregnant with her
/// first conception (a boy with the dummy allele twice, born in month 15) and her
/// attempts have ended; the attempts of the woman of 16 run since month 1 (at chance 0
/// until her 18th birthday), the next in month 6; the birthdays of year 0 have planned
/// those of year 1.
fn clan_conceived() -> File {
    let mut file = File::tiny();
    file.data_hash = CLAN_DATA_HASH;
    file.closed_through = 5 * MONTH;
    file.next_sim_id = 7;
    file.next_partnership_id = 4;
    let birthday = |sim: u64, seq: u64| (Y, 0, seq, Payload::Birthday { sim }, None);
    let attempts = |woman: u64, time: i64, seq: u64| {
        let payload = Payload::ConceptionAttempt { woman };
        (time, 0, seq, payload, Some(MONTH))
    };
    file.queue = vec![
        attempts(3, 6 * MONTH, 21),
        (Y - 1, 0, 1, Payload::YearEnd, Some(Y)),
        birthday(1, 22),
        birthday(2, 23),
        birthday(3, 24),
        birthday(4, 25),
        birthday(5, 26),
        birthday(6, 27),
        (15 * MONTH, 0, 28, Payload::Birth { mother: 1 }, None),
    ];
    // The entries 11 to 13 are the households of the couples.
    file.plan_refs = vec![
        (9, 0),
        (1, 0),
        (14, 0),
        (15, 0),
        (16, 0),
        (17, 0),
        (18, 0),
        (19, 0),
        (20, 5 * MONTH),
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
    file.next_household_id = 4;
    file.households = vec![(1, (0, None)), (2, (0, None)), (3, (0, None))];
    file.pregnancies = vec![(
        1,
        pregnant(2, CLAN_CHILD, 0, MALE, Genes(MALE, DUMMY, DUMMY), 5 * MONTH),
    )];
    file.next_event_id = 23;
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
/// seconds, or the same kind in one second); attempts of one second come in the order of
/// the women (ADR-0002: the subject before the insertion number).
fn insert_event(file: &mut File, entry: QueueEntry, plan: (u64, i64)) {
    let order = |event: &QueueEntry| match event.3 {
        Payload::ConceptionAttempt { woman } => (event.0, woman, event.2),
        _ => (event.0, 0, event.2),
    };
    let at = file
        .queue
        .iter()
        .position(|other| order(other) > order(&entry))
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

const CLAN_SAVE_AFTER_BIRTH: &str = "53494d4147454e540227f199a1d1dcf4da8da7011201ff0101000080d8cd220804041a8080f5240002060180a8a7028080f524002803010180a8a7028080f524001603030180a8a702fe8ffe3b000100018088ff1d8090fe3b001e0101008090fe3b001f0102008090fe3b00200103008090fe3b00210104008090fe3b00220105008090fe3b002301060080e0cc400024010700b0d0e5b9010026020785d2bc82f8b2ead44d00c4e9bcc101000b020385d2bc82f8b2ead44d00c6d0d8e3010014020685d2bc82f8b2ead44d00f4eba580020005020185d2bc82f8b2ead44d008088b9b6050027070700f0e8f7c90a0011020585d2bc82f8b2ead44d00e0bd98dd10000e020485d2bc82f8b2ead44d00e6caef83400008020285d2bc82f8b2ead44d00baa8afa1f52d00100205a0bb8ca1c58498a01c00e48880ebba30000a0203a0bb8ca1c58498a01c00d4ee8eed963200070202a0bb8ca1c58498a01c00ee8dcd9aeb3300250207a0bb8ca1c58498a01c0088d1a4e7ec3300130206a0bb8ca1c58498a01c008cfefad2ec3400040201a0bb8ca1c58498a01c009cd68493ae36000d0204a0bb8ca1c58498a01c00290701ff9fedd704dded83919698d6d0d00100000000010000000000000001010101010000000002ffafeb9305f6eb9bebea9bd7c53e01010000000000000100000000000001010000000003fffff0df0385b2a2a3e3ecdafac4010001000001000000000000019aada6adcbb284bf7200000000000000000100000000000000000000000102010000000004ffefe38307f9a4bfd6bdf093fea70101000000000000000100000000000002010000000005ffefd4e30afacbdcacb9abf1870300000000000000000000000000000103010000000006ffefd4e30af1c5c4b2d9a691fded010100000000000000010000000000000301000000000780d8cd22f3c89ad6cdbbe183cd010101000001000000010000019aada6adcbb284bf720000000000000000010000000000000000000000000100010101010200000003010102000002030400000305060000030100000200000300000000001a01001d80d8cd2209000100178088ff1d188088ff1d198088ff1d1a8088ff1d1b8088ff1d1c8088ff1d1d80d8cd221d80d8cd220400070002001d80d8cd220600050003000600040003001d80d8cd220700020005001f01010100ff0109746573743a636c616e000006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100fffff0df0300010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a01000300010203040500000100aaddb4ff879ccdcfe401";
/// When the child (sim 7) dies of senescence and of accident, if nothing comes first.
const CHILD_DEATHS: (i64, i64) = (890_562_519_927, 194_819_096);

/// "clan" right after the birth in month 15: the child (sim 7) lives with its parents and
/// the dummy allele twice, in their household with its mother as carer; its first
/// birthday, its deaths and its day of leaving home (at 22) are planned by the birth
/// (entry 29), and so are the mother's attempts from month 16 on (no pause); in that second
/// the girl's attempt comes after hers (the order of the subjects).
fn clan_born() -> File {
    let mut file = clan_conceived();
    file.closed_through = 15 * MONTH;
    file.next_sim_id = 8;
    let birthday = |sim: u64, seq: u64, by: u64| {
        let entry = (2 * Y, 0, seq, Payload::Birthday { sim }, None);
        (entry, (by, Y))
    };
    let attempts = |woman: u64, seq: u64| {
        let payload = Payload::ConceptionAttempt { woman };
        (16 * MONTH, 0, seq, payload, Some(MONTH))
    };
    let events = vec![
        (attempts(1, 39), (29, 15 * MONTH)),
        (attempts(3, 21), (9, 0)),
        ((2 * Y - 1, 0, 1, Payload::YearEnd, Some(Y)), (1, 0)),
        birthday(1, 29, 23),
        birthday(2, 30, 24),
        birthday(3, 31, 25),
        birthday(4, 32, 26),
        birthday(5, 33, 27),
        birthday(6, 34, 28),
        (
            (15 * MONTH + Y, 0, 35, Payload::Birthday { sim: 7 }, None),
            (29, 15 * MONTH),
        ),
    ];
    (file.queue, file.plan_refs) = events.into_iter().unzip();
    for (sim, deaths) in (1..).zip(CLAN_DEATHS) {
        with_deaths(&mut file, sim, deaths, 3 * sim, (sim + 1, 0));
    }
    with_deaths(&mut file, 7, CHILD_DEATHS, 36, (29, 15 * MONTH));
    insert_event(
        &mut file,
        (
            15 * MONTH + 22 * Y,
            0,
            38,
            Payload::LeaveHome { sim: 7 },
            None,
        ),
        (29, 15 * MONTH),
    );
    file.next_event_seq = 40;
    file.sims.push((
        7,
        (
            15 * MONTH,
            CLAN_CHILD,
            MALE,
            Genes(MALE, DUMMY, DUMMY),
            ar((0.0, None)),
            0,
            0,
            0,
            1,
            false,
            Some(1),
            Some((1, 2, 0)),
            Vec::new(),
        ),
    ));
    // Her first live birth (ADR-0017 section 4).
    file.sims[0].1.6 = 1;
    file.pregnancies.clear();
    file.next_event_id = 31;
    file.year_hashes = vec![(0, 0xe49f_34e0_7fed_2eaa)];
    file
}

/// The save of "clan" after the birth from the reference and the test encoder are the
/// same bytes, and it loads.
#[test]
fn a_save_keeps_the_child() {
    assert_eq!(clan_born().bytes(), hex(CLAN_SAVE_AFTER_BIRTH));
    assert_eq!(clan_load_error(&clan_born().bytes()), None);
}

// Two variants of "clan" with a pause and with a wish (tools/state_reference.py): the
// pause of 12 months, and the wish always 1 (`fertility_wish_shape` [0, 1]); each saved
// right after the first birth in month 15.
const CLAN_PAUSE_SAVE_AFTER_BIRTH: &str = "53494d4147454e54022798bef6d5c6a5e096761201ff0101000080d8cd220804041a8080f5240002060180a8a7028080f524001603030180a8a702fe8ffe3b000100018088ff1d8090fe3b001e0101008090fe3b001f0102008090fe3b00200103008090fe3b00210104008090fe3b00220105008090fe3b002301060080b8a53e002803010180a8a70280e0cc400024010700b0d0e5b9010026020785d2bc82f8b2ead44d00c4e9bcc101000b020385d2bc82f8b2ead44d00c6d0d8e3010014020685d2bc82f8b2ead44d00f4eba580020005020185d2bc82f8b2ead44d008088b9b6050027070700f0e8f7c90a0011020585d2bc82f8b2ead44d00e0bd98dd10000e020485d2bc82f8b2ead44d00e6caef83400008020285d2bc82f8b2ead44d00baa8afa1f52d00100205a0bb8ca1c58498a01c00e48880ebba30000a0203a0bb8ca1c58498a01c00d4ee8eed963200070202a0bb8ca1c58498a01c00ee8dcd9aeb3300250207a0bb8ca1c58498a01c0088d1a4e7ec3300130206a0bb8ca1c58498a01c008cfefad2ec3400040201a0bb8ca1c58498a01c009cd68493ae36000d0204a0bb8ca1c58498a01c00290701ff9fedd704dded83919698d6d0d00100000000010000000000000001010101010000000002ffafeb9305f6eb9bebea9bd7c53e01010000000000000100000000000001010000000003fffff0df0385b2a2a3e3ecdafac4010001000001000000000000019aada6adcbb284bf7200000000000000000100000000000000000000000102010000000004ffefe38307f9a4bfd6bdf093fea70101000000000000000100000000000002010000000005ffefd4e30afacbdcacb9abf1870300000000000000000000000000000103010000000006ffefd4e30af1c5c4b2d9a691fded010100000000000000010000000000000301000000000780d8cd22f3c89ad6cdbbe183cd010101000001000000010000019aada6adcbb284bf720000000000000000010000000000000000000000000100010101010200000003010102000002030400000305060000030100000200000300000000001a010009000100178088ff1d188088ff1d198088ff1d1a8088ff1d1b8088ff1d1c8088ff1d1d80d8cd221d80d8cd221d80d8cd220400070002001d80d8cd220600050003000600040003001d80d8cd220700020005001f01010100ff0109746573743a636c616e000006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100fffff0df0300010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a01000300010203040500000100aaddb4ff879ccdcfe401";
const CLAN_WISH_SAVE_AFTER_BIRTH: &str = "53494d4147454e540227accb84fcb292acd29b011201ff0101000080d8cd22080404198080f5240002060180a8a7028080f524001603030180a8a702fe8ffe3b000100018088ff1d8090fe3b001e0101008090fe3b001f0102008090fe3b00200103008090fe3b00210104008090fe3b00220105008090fe3b002301060080e0cc400024010700b0d0e5b9010026020785d2bc82f8b2ead44d00c4e9bcc101000b020385d2bc82f8b2ead44d00c6d0d8e3010014020685d2bc82f8b2ead44d00f4eba580020005020185d2bc82f8b2ead44d008088b9b6050027070700f0e8f7c90a0011020585d2bc82f8b2ead44d00e0bd98dd10000e020485d2bc82f8b2ead44d00e6caef83400008020285d2bc82f8b2ead44d00baa8afa1f52d00100205a0bb8ca1c58498a01c00e48880ebba30000a0203a0bb8ca1c58498a01c00d4ee8eed963200070202a0bb8ca1c58498a01c00ee8dcd9aeb3300250207a0bb8ca1c58498a01c0088d1a4e7ec3300130206a0bb8ca1c58498a01c008cfefad2ec3400040201a0bb8ca1c58498a01c009cd68493ae36000d0204a0bb8ca1c58498a01c00280701ff9fedd704dded83919698d6d0d00100000000010000000000000001010101010000000002ffafeb9305f6eb9bebea9bd7c53e01010000000000000100000000000001010000000003fffff0df0385b2a2a3e3ecdafac4010001000001000000000000019aada6adcbb284bf7200000000000000000100000000000000000000000102010000000004ffefe38307f9a4bfd6bdf093fea70101000000000000000100000000000002010000000005ffefd4e30afacbdcacb9abf1870300000000000000000000000000000103010000000006ffefd4e30af1c5c4b2d9a691fded010100000000000000010000000000000301000000000780d8cd22f3c89ad6cdbbe183cd010101000001000000010000019aada6adcbb284bf7200000000000000000100000000000000000000000001000101010102000000030101020000020304000003050600000301000002000003000000000019010009000100178088ff1d188088ff1d198088ff1d1a8088ff1d1b8088ff1d1c8088ff1d1d80d8cd221d80d8cd220400070002001d80d8cd220600050003000600040003001d80d8cd220700020005001f01010100ff0109746573743a636c616e000006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100fffff0df0300010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a01000300010203040500000100aaddb4ff879ccdcfe401";
const CLAN_PAUSE_DATA_HASH: u64 = 0x762d_812c_6abd_9f18;
const CLAN_WISH_DATA_HASH: u64 = 0x9ba4_b093_2f81_25ac;

/// "clan" with one value of its life course data changed.
fn clan_data_with(change: impl FnOnce(&mut serde_json::Value)) -> Data {
    let (mut calibration, genome_map) =
        common::death_files(0.02, None, common::repository_diseases());
    let mut demography = common::demography();
    common::window_fertility(&mut calibration, &mut demography, 0.25);
    change(&mut demography);
    Data::new(common::definitions_with(demography, calibration, genome_map).expect("definitions"))
        .expect("checked")
}

/// The test data of "clan" with a pause of 12 months.
fn clan_pause_data() -> Data {
    clan_data_with(|d| d["values"]["core:fertility_pause"]["value"] = serde_json::json!(12.0))
}

/// The test data of "clan" with the wish always 1.
fn clan_wish_data() -> Data {
    clan_data_with(|d| {
        let weight = |x: f64| serde_json::json!({"value": x, "unit": "fraction"});
        d["fertility_wish_shape"] = serde_json::json!([weight(0.0), weight(1.0)]);
    })
}

/// "clan" with the pause right after the first birth: her series begins only in month 27,
/// twelve months after the birth (ADR-0017 section 4).
fn clan_pause_born() -> File {
    let mut file = clan_born();
    file.data_hash = CLAN_PAUSE_DATA_HASH;
    let (mut series, plan) = take_event(&mut file, 0);
    series.0 = 27 * MONTH;
    insert_event(&mut file, series, plan);
    file
}

/// "clan" with the wish 1 right after the first birth: she has her wish, so no series
/// follows; the insertion number of the series was never used.
fn clan_wish_born() -> File {
    let mut file = clan_born();
    file.data_hash = CLAN_WISH_DATA_HASH;
    take_event(&mut file, 0);
    file.next_event_seq = 39;
    file
}

/// "clan" in month 17 with the second conception of the first woman: the draw of month 17
/// succeeds at the chance 0.25 (u = 0.2064, tools/state_reference.py); the girl's attempts
/// go on in month 18; the birth is planned by the conception (entry 31). Valid on the clan
/// data (no pause, wish 40).
fn clan_second_conception(data_hash: u64) -> File {
    let mut file = clan_born();
    file.data_hash = data_hash;
    file.closed_through = 17 * MONTH;
    take_event(&mut file, 0);
    file.queue[0].0 = 18 * MONTH;
    insert_event(
        &mut file,
        (27 * MONTH, 0, 40, Payload::Birth { mother: 1 }, None),
        (31, 17 * MONTH),
    );
    file.next_event_seq = 41;
    file.sims[0].1.5 = 2;
    file.pregnancies = vec![(
        1,
        pregnant(
            2,
            0xe6f8_1c92_e297_9324,
            1,
            FEMALE,
            Genes(FEMALE, WILD, WILD),
            17 * MONTH,
        ),
    )];
    file.next_event_id = 34;
    file
}

/// The saves of the variants from the reference and the test encoder are the same bytes,
/// and they load; the second conception of the plain clan loads too.
#[test]
fn a_save_keeps_the_pause_and_the_wish() {
    assert_eq!(clan_pause_born().bytes(), hex(CLAN_PAUSE_SAVE_AFTER_BIRTH));
    assert_eq!(clan_wish_born().bytes(), hex(CLAN_WISH_SAVE_AFTER_BIRTH));
    assert_eq!(
        load_error_with(clan_pause_data(), &clan_pause_born().bytes()),
        None
    );
    assert_eq!(
        load_error_with(clan_wish_data(), &clan_wish_born().bytes()),
        None
    );
    assert_eq!(clan_data().hash(), CLAN_DATA_HASH);
    assert_eq!(clan_pause_data().hash(), CLAN_PAUSE_DATA_HASH);
    assert_eq!(clan_wish_data().hash(), CLAN_WISH_DATA_HASH);
    assert_eq!(
        clan_load_error(&clan_second_conception(CLAN_DATA_HASH).bytes()),
        None
    );
}

/// The pause and the wish are checked (ADR-0017 section 4, ADR-0006 section 5): no series
/// and no conception inside the pause, none once the wish is reached.
#[test]
fn load_checks_the_pause_and_the_wish() {
    let pause = |bytes: Vec<u8>| load_error_with(clan_pause_data(), &bytes);
    let wish = |bytes: Vec<u8>| load_error_with(clan_wish_data(), &bytes);
    let inconsistent = Some(LoadError::Inconsistent);
    assert_eq!(
        pause(clan_pause_born().with(|f| {
            let (mut early, plan) = take_event(f, 8);
            early.0 = 16 * MONTH;
            insert_event(f, early, plan);
        })),
        inconsistent,
        "a series in the month after the birth, inside the pause"
    );
    assert_eq!(
        pause(clan_second_conception(CLAN_PAUSE_DATA_HASH).bytes()),
        inconsistent,
        "a conception inside the pause"
    );
    assert_eq!(
        wish(clan_wish_born().with(|f| {
            insert_event(
                f,
                (
                    16 * MONTH,
                    0,
                    39,
                    Payload::ConceptionAttempt { woman: 1 },
                    Some(MONTH),
                ),
                (29, 15 * MONTH),
            );
            f.next_event_seq = 40;
        })),
        inconsistent,
        "a series though her wish is reached"
    );
    assert_eq!(
        wish(clan_second_conception(CLAN_WISH_DATA_HASH).bytes()),
        inconsistent,
        "a conception though her wish is reached"
    );
}

// Scenario "twins" of tools/state_reference.py: "clan" with twins at every conception
// (identical 0.3, fraternal 0.7). The first conception in month 5 gives fraternal twins, two
// boys: the first with the key of clan's first child, the second with its own key and its
// own zygote (both wild alleles).
const TWINS_SAVE_AFTER_CONCEPTION: &str = "53494d4147454e540227ffdec2aee2a7efbe561201ff0101000080c8c40b0704041680f0eb0d0002060180a8a70280f0eb0d001603030180a8a702fe87ff1d000100018088ff1d8088ff1d00170101008088ff1d00180102008088ff1d00190103008088ff1d001a0104008088ff1d001b0105008088ff1d001c01060080d8cd22001d040100c4e9bcc101000b020385d2bc82f8b2ead44d00c6d0d8e3010014020685d2bc82f8b2ead44d00f4eba580020005020185d2bc82f8b2ead44d00f0e8f7c90a0011020585d2bc82f8b2ead44d00e0bd98dd10000e020485d2bc82f8b2ead44d00e6caef83400008020285d2bc82f8b2ead44d00baa8afa1f52d00100205a0bb8ca1c58498a01c00e48880ebba30000a0203a0bb8ca1c58498a01c00d4ee8eed963200070202a0bb8ca1c58498a01c0088d1a4e7ec3300130206a0bb8ca1c58498a01c008cfefad2ec3400040201a0bb8ca1c58498a01c009cd68493ae36000d0204a0bb8ca1c58498a01c001e0601ff9fedd704dded83919698d6d0d00100000000010000000000000001000101010000000002ffafeb9305f6eb9bebea9bd7c53e01010000000000000100000000000001010000000003fffff0df0385b2a2a3e3ecdafac4010001000001000000000000019aada6adcbb284bf7200000000000000000100000000000000000000000102010000000004ffefe38307f9a4bfd6bdf093fea70101000000000000000100000000000002010000000005ffefd4e30afacbdcacb9abf1870300000000000000000000000000000103010000000006ffefd4e30af1c5c4b2d9a691fded01010000000000000001000000000000030100000000030101020000020304000003050600000301000002000003000001010102000002f3c89ad6cdbbe183cd010101000001000000010000ab8392d4eefb94f8c601010000000000000001000080c8c40b0000160100090001000e000f0010001100120013001480c8c40b0400070002000600050003000600040003000700020005001a01010100ff0109746573743a636c616e000006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100fffff0df0300010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a010003000102030405000000";
const TWINS_SAVE_AFTER_BIRTH: &str = "53494d4147454e540227ffdec2aee2a7efbe561201ff0101000080d8cd220904041e8080f5240002060180a8a7028080f524002c03010180a8a7028080f524001603030180a8a702fe8ffe3b000100018088ff1d8090fe3b001e0101008090fe3b001f0102008090fe3b00200103008090fe3b00210104008090fe3b00220105008090fe3b002301060080e0cc40002401070080e0cc400028010800b0d0e5b9010026020785d2bc82f8b2ead44d00c4e9bcc101000b020385d2bc82f8b2ead44d00c6d0d8e3010014020685d2bc82f8b2ead44d00f4eba580020005020185d2bc82f8b2ead44d00828fc7ad02002a020885d2bc82f8b2ead44d0080e8bcbe04002b0708008088b9b6050027070700f0e8f7c90a0011020585d2bc82f8b2ead44d00e0bd98dd10000e020485d2bc82f8b2ead44d00e6caef83400008020285d2bc82f8b2ead44d00baa8afa1f52d00100205a0bb8ca1c58498a01c00e48880ebba30000a0203a0bb8ca1c58498a01c00bc9987fe8e3200290208a0bb8ca1c58498a01c00d4ee8eed963200070202a0bb8ca1c58498a01c00ee8dcd9aeb3300250207a0bb8ca1c58498a01c0088d1a4e7ec3300130206a0bb8ca1c58498a01c008cfefad2ec3400040201a0bb8ca1c58498a01c009cd68493ae36000d0204a0bb8ca1c58498a01c002d0801ff9fedd704dded83919698d6d0d00100000000010000000000000001020101010000000002ffafeb9305f6eb9bebea9bd7c53e01010000000000000100000000000001010000000003fffff0df0385b2a2a3e3ecdafac4010001000001000000000000019aada6adcbb284bf7200000000000000000100000000000000000000000102010000000004ffefe38307f9a4bfd6bdf093fea70101000000000000000100000000000002010000000005ffefd4e30afacbdcacb9abf1870300000000000000000000000000000103010000000006ffefd4e30af1c5c4b2d9a691fded010100000000000000010000000000000301000000000780d8cd22f3c89ad6cdbbe183cd010101000001000000010000019aada6adcbb284bf72000000000000000001000000000000000000000000010001010101020000000880d8cd22ab8392d4eefb94f8c6010100000000000000010000000000000100010101010200000003010102000002030400000305060000030100000200000300000000001e01002080d8cd22090001001a8088ff1d1b8088ff1d1c8088ff1d1d8088ff1d1e8088ff1d1f8088ff1d2080d8cd222280d8cd222080d8cd220400070002002280d8cd222280d8cd222080d8cd22060005000300060004002280d8cd2203002080d8cd220700020005002401010100ff0109746573743a636c616e000006ff9fedd70400010f636f72653a646973656173655f6172020001ffafeb930501010f636f72653a646973656173655f6172020100fffff0df0300010f636f72653a646973656173655f6172020101ffefe383070100ffefd4e30a0000ffefd4e30a01000300010203040500000100bbb091afd5e5eafad601";
const TWINS_DATA_HASH: u64 = 0x567d_bd3e_25d0_af7f;
/// The second twin of the first conception.
const SECOND_TWIN: u64 = 0xc6f0_53de_ea84_81ab;
/// When the second twin dies of senescence and of accident, if nothing comes first.
const SECOND_TWIN_DEATHS: (i64, i64) = (861_004_686_942, 316_203_969);

/// The test data of "twins".
fn twins_data() -> Data {
    clan_data_with(|d| {
        d["values"]["core:twins_identical"]["value"] = serde_json::json!(0.3);
        d["values"]["core:twins_fraternal"]["value"] = serde_json::json!(0.7);
    })
}

/// "twins" right after the first conception: the pregnancy holds both boys; the six
/// entries of the conception (per child its entry and its two meioses) move the EventIds.
fn twins_conceived() -> File {
    let mut file = clan_conceived();
    file.data_hash = TWINS_DATA_HASH;
    file.pregnancies[0]
        .1
        .3
        .push((SECOND_TWIN, MALE, Genes(MALE, WILD, WILD)));
    file.next_event_id = 26;
    file
}

/// "twins" right after the first birth: both boys (sims 7 and 8) are born in month 15 into
/// their parents' household, each with its birthday, deaths and day of leaving home (the
/// first at 22, the second at 18) planned by its own entry (32 and 34, each followed by
/// its move); her attempts go on from month 16, planned by the entry of the first; she
/// has two live births.
fn twins_born() -> File {
    twins_born_in([0, 1])
}

/// "twins" after the first birth with the twins in the given order of SimIds: `[1, 0]` gives
/// SimId 7 to the second twin and 8 to the first, each with its key, genome and deaths.
fn twins_born_in(order: [usize; 2]) -> File {
    let mut file = twins_conceived();
    file.closed_through = 15 * MONTH;
    file.next_sim_id = 9;
    let birthday = |sim: u64, seq: u64, by: u64| {
        let entry = (2 * Y, 0, seq, Payload::Birthday { sim }, None);
        (entry, (by, Y))
    };
    let attempts = |woman: u64, seq: u64| {
        let payload = Payload::ConceptionAttempt { woman };
        (16 * MONTH, 0, seq, payload, Some(MONTH))
    };
    let events = vec![
        (attempts(1, 43), (32, 15 * MONTH)),
        (attempts(3, 21), (9, 0)),
        ((2 * Y - 1, 0, 1, Payload::YearEnd, Some(Y)), (1, 0)),
        birthday(1, 29, 26),
        birthday(2, 30, 27),
        birthday(3, 31, 28),
        birthday(4, 32, 29),
        birthday(5, 33, 30),
        birthday(6, 34, 31),
        (
            (15 * MONTH + Y, 0, 35, Payload::Birthday { sim: 7 }, None),
            (32, 15 * MONTH),
        ),
        (
            (15 * MONTH + Y, 0, 39, Payload::Birthday { sim: 8 }, None),
            (34, 15 * MONTH),
        ),
    ];
    (file.queue, file.plan_refs) = events.into_iter().unzip();
    for (sim, deaths) in (1..).zip(CLAN_DEATHS) {
        with_deaths(&mut file, sim, deaths, 3 * sim, (sim + 1, 0));
    }
    let twins = [
        (
            CLAN_CHILD,
            Genes(MALE, DUMMY, DUMMY),
            ar((0.0, None)),
            CHILD_DEATHS,
            22,
        ),
        (
            SECOND_TWIN,
            Genes(MALE, WILD, WILD),
            Vec::new(),
            SECOND_TWIN_DEATHS,
            18,
        ),
    ];
    for ((sim, seq, by), index) in [(7, 36, 32), (8, 40, 34)].into_iter().zip(order) {
        let (key, genes, afflictions, deaths, leaves) = twins[index].clone();
        with_deaths(&mut file, sim, deaths, seq, (by, 15 * MONTH));
        let leave_home = Payload::LeaveHome { sim };
        let leaving = (15 * MONTH + leaves * Y, 0, seq + 2, leave_home, None);
        insert_event(&mut file, leaving, (by, 15 * MONTH));
        let born = Some((1, 2, 0));
        let child = (
            15 * MONTH,
            key,
            MALE,
            genes,
            afflictions,
            0,
            0,
            0,
            1,
            false,
            Some(1),
            born,
            Vec::new(),
        );
        file.sims.push((sim, child));
    }
    file.next_event_seq = 44;
    file.sims[0].1.6 = 2;
    file.pregnancies.clear();
    file.next_event_id = 36;
    file.year_hashes = vec![(0, 0xd6f5_ab2d_55e4_583b)];
    file
}

/// The saves of "twins" from the reference and the test encoder are the same bytes, and
/// they load.
#[test]
fn a_save_keeps_the_twins() {
    assert_eq!(twins_data().hash(), TWINS_DATA_HASH);
    assert_eq!(twins_conceived().bytes(), hex(TWINS_SAVE_AFTER_CONCEPTION));
    assert_eq!(twins_born().bytes(), hex(TWINS_SAVE_AFTER_BIRTH));
    for file in [twins_conceived(), twins_born()] {
        assert_eq!(load_error_with(twins_data(), &file.bytes()), None);
    }
}

/// Twins are both the children the rule gives (spec 11.3, ADR-0006 section 5): a pregnancy
/// with both, both born in one second with consecutive SimIds in the order of their index,
/// both counted as live births.
#[test]
fn load_checks_the_twins() {
    let twins = |bytes: Vec<u8>| load_error_with(twins_data(), &bytes);
    let cases: [(&str, Vec<u8>); 5] = [
        (
            "a pregnancy conceived in a second that is not closed",
            twins_conceived().with(|f| {
                // The clock one second earlier; the girl's next attempt is then month 5.
                f.closed_through = 5 * MONTH - 1;
                f.queue[0].0 = 5 * MONTH;
            }),
        ),
        (
            "a pregnancy with only the first of the twins",
            twins_conceived().with(|f| {
                f.pregnancies[0].1.3.pop();
            }),
        ),
        (
            "only the first twin born",
            twins_born().with(|f| {
                f.sims.pop();
                f.next_sim_id = 8;
                f.sims[0].1.6 = 1;
                let keep: Vec<bool> = f
                    .queue
                    .iter()
                    .map(|e| {
                        !matches!(
                            e.3,
                            Payload::Birthday { sim: 8 }
                                | Payload::Death { sim: 8, .. }
                                | Payload::LeaveHome { sim: 8 }
                        )
                    })
                    .collect();
                let mut keep_iter = keep.iter();
                f.queue
                    .retain(|_| *keep_iter.next().expect("one per entry"));
                let mut keep_iter = keep.iter();
                f.plan_refs
                    .retain(|_| *keep_iter.next().expect("one per entry"));
            }),
        ),
        (
            "the twins with their SimIds swapped",
            twins_born_in([1, 0]).bytes(),
        ),
        (
            "a mother whose live births count the twins once",
            twins_born().with(|f| f.sims[0].1.6 = 1),
        ),
    ];
    let accepted: Vec<&str> = cases
        .into_iter()
        .filter(|(_, bytes)| twins(bytes.clone()) != Some(LoadError::Inconsistent))
        .map(|(name, _)| name)
        .collect();
    assert_eq!(accepted, Vec::<&str>::new());
}

/// Where people live (ADR-0018 sections 4 and 7, ADR-0006 section 5): the households
/// numbered from 1 without a gap below the counter, each formed and ended by the clock, open
/// exactly while someone lives in it; every living person in an open household, every
/// archived one in a household that existed; `moved_out` as the rule gives it; a
/// `leave_home` exactly for each living person not moved out, at the drawn second; a carer
/// for each dependent child, in its household and fit to care, or none in a household of
/// dependents alone; no carer for anyone else.
#[test]
fn load_checks_the_households() {
    let partners = |change: fn(&mut File)| partners_arranged().with(change);
    let clan = |change: fn(&mut File)| clan_born().with(change);
    let cases: [(&str, Option<LoadError>); 24] = [
        // The households.
        (
            "a household number skipped",
            partners_load_error(&partners(|f| {
                f.households[2].0 = 4;
                f.next_household_id = 5;
                f.sims[0].1.8 = 4;
                f.sims[1].1.8 = 4;
            })),
        ),
        (
            "the counter not above every household",
            partners_load_error(&partners(|f| f.next_household_id = 3)),
        ),
        (
            "the counter beyond the next household",
            partners_load_error(&partners(|f| f.next_household_id = 5)),
        ),
        (
            "a household formed after the second the clock acts in",
            partners_load_error(&partners(|f| f.households[2].1.0 = Y + 1)),
        ),
        (
            "a household ended after the second the clock acts in",
            partners_load_error(&partners(|f| f.households[0].1.1 = Some(Y + 1))),
        ),
        (
            "a household ended before it formed",
            partners_load_error(&partners(|f| f.households[1].1 = (Y, Some(Y - 1)))),
        ),
        (
            "a household not ended without a living member",
            partners_load_error(&partners(|f| f.households[0].1.1 = None)),
        ),
        (
            "a household ended with living members",
            partners_load_error(&partners(|f| f.households[2].1.1 = Some(Y))),
        ),
        // Where the people are.
        (
            "a living person in a household that has ended",
            partners_load_error(&partners(|f| f.sims[0].1.8 = 1)),
        ),
        (
            "a living person in a household that never existed",
            partners_load_error(&partners(|f| f.sims[0].1.8 = 9)),
        ),
        (
            "an archived person in a household that never existed",
            partners_load_error(&partners(|f| f.archive[0].1.10 = 9)),
        ),
        // Moved out.
        (
            "a founder of a couple not moved out",
            clan_load_error(&clan(|f| f.sims[0].1.9 = false)),
        ),
        (
            "a minor founder moved out before her day of leaving home",
            load_error(&File::tiny().with(|f| {
                f.sims[0].1.9 = true;
                take_event(f, 2);
            })),
        ),
        (
            "an archived founder of a couple not moved out",
            partners_load_error(&partners(|f| f.archive[0].1.11 = false)),
        ),
        // The days of leaving home.
        (
            "no leave_home for a person not moved out",
            load_error(&File::tiny().with(|f| {
                take_event(f, 2);
            })),
        ),
        (
            "a leave_home not at the drawn second",
            load_error(&File::tiny().with(|f| f.queue[2].0 += 1)),
        ),
        (
            "a leave_home for a person moved out",
            clan_load_error(&clan(|f| {
                let seq = f.next_event_seq;
                f.next_event_seq += 1;
                let leaving = (20 * Y, 0, seq, Payload::LeaveHome { sim: 1 }, None);
                insert_event(f, leaving, (29, 15 * MONTH));
            })),
        ),
        (
            "two leave_home for one person",
            load_error(&File::tiny().with(|f| {
                let (mut twice, plan) = (f.queue[2].clone(), f.plan_refs[2]);
                twice.2 = f.next_event_seq;
                f.next_event_seq += 1;
                insert_event(f, twice, plan);
            })),
        ),
        // The carers.
        (
            "a dependent child without a carer in a household with its parents",
            clan_load_error(&clan(|f| f.sims[6].1.10 = None)),
        ),
        (
            "a carer in another household",
            clan_load_error(&clan(|f| f.sims[6].1.10 = Some(3))),
        ),
        (
            "a carer who does not live",
            clan_load_error(&clan(|f| f.sims[6].1.10 = Some(9))),
        ),
        (
            "a carer who is dependent",
            load_error_with(
                twins_data(),
                &twins_born().with(|f| f.sims[7].1.10 = Some(7)),
            ),
        ),
        (
            "a person not dependent with a carer",
            clan_load_error(&clan(|f| f.sims[1].1.10 = Some(1))),
        ),
        (
            "a minor alone in her household with herself as carer",
            load_error(&File::tiny().with(|f| f.sims[0].1.10 = Some(1))),
        ),
    ];
    let accepted: Vec<&str> = cases
        .into_iter()
        .filter(|(_, error)| *error != Some(LoadError::Inconsistent))
        .map(|(name, _)| name)
        .collect();
    assert_eq!(accepted, Vec::<&str>::new());
}

/// The social parents (ADR-0018 section 4, ADR-0006 section 5): each a person of the
/// world, living or archived, not the person herself, not a biological parent, listed
/// once. A social parent who is neither is accepted: the loader does not replay the
/// orphan rule.
#[test]
fn load_checks_the_social_parents() {
    let clan = |change: fn(&mut File)| clan_load_error(&clan_born().with(change));
    assert_eq!(clan(|f| f.sims[6].1.12 = vec![3]), None, "another adult");
    let cases: [(&str, Option<LoadError>); 6] = [
        ("nobody", clan(|f| f.sims[6].1.12 = vec![9])),
        ("herself", clan(|f| f.sims[6].1.12 = vec![7])),
        ("the mother", clan(|f| f.sims[6].1.12 = vec![1])),
        ("the father", clan(|f| f.sims[6].1.12 = vec![2])),
        ("listed twice", clan(|f| f.sims[6].1.12 = vec![3, 3])),
        (
            "an archived person's social parent who never was",
            partners_load_error(&partners_arranged().with(|f| f.archive[0].1.13 = vec![9])),
        ),
    ];
    let accepted: Vec<&str> = cases
        .into_iter()
        .filter(|(_, error)| *error != Some(LoadError::Inconsistent))
        .map(|(name, _)| name)
        .collect();
    assert_eq!(accepted, Vec::<&str>::new());
}

/// A born sim is the child the rule gives: its parents in the living or the archive, born
/// as many months after a conception the rule allows as a pregnancy lasts, its key from
/// both parents and the mother's number, its alleles and sex from the draws, an exposure
/// from its birth on; the mother's count takes it in; the IDs of the born follow the
/// founders without a gap. ADR-0006 section 5.
#[test]
fn load_checks_the_born() {
    let cases: [(&str, Vec<u8>); 13] = [
        (
            "a second conception while she was pregnant with the first child",
            clan_born().with(|f| {
                // The draw of month 12 succeeds (tools/rng_reference.py).
                f.pregnancies = vec![(
                    1,
                    pregnant(
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
            clan_born().with(|f| f.sims[6].1.11 = Some((1, 2, 1))),
        ),
        (
            "a child with the man as its mother",
            clan_born().with(|f| f.sims[6].1.11 = Some((2, 1, 0))),
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
            clan_born().with(|f| {
                let (mut late, plan) = take_event(f, 0);
                late.0 += MONTH;
                insert_event(f, late, plan);
            }),
        ),
        (
            "a mother whose live births miss her child",
            clan_born().with(|f| f.sims[0].1.6 = 0),
        ),
        (
            "a father with a live birth",
            clan_born().with(|f| f.sims[1].1.6 = 1),
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
    // In the model the girl's attempts are at index 0 and the birth at index 8.
    let moved_birth = |f: &mut File, to: i64| {
        let (mut birth, plan) = take_event(f, 8);
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
                f.pregnancies[0].1.4 = 5 * MONTH + 1;
                moved_birth(f, 15 * MONTH + 1);
            }),
        ),
        (
            "a pregnancy as a conception before her latest, with its own child",
            clan_conceived().with(|f| {
                f.pregnancies[0].1 = pregnant(
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
                f.pregnancies[0].1 = pregnant(
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
            "a pregnancy of the girl in the month of the pairing, at chance 0",
            clan_conceived().with(|f| {
                f.pregnancies.push((
                    3,
                    pregnant(
                        4,
                        0x0d0d_06d7_b21d_4bc0,
                        0,
                        MALE,
                        Genes(MALE, DUMMY, WILD),
                        0,
                    ),
                ));
                f.sims[2].1.5 = 1;
                take_event(f, 0);
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
                take_event(f, 8);
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
            "a pregnancy conceived in the month of the pairing, before her series began",
            clan_conceived().with(|f| {
                f.pregnancies[0].1.4 = 0;
                moved_birth(f, 10 * MONTH);
            }),
        ),
        (
            "a pregnancy with another genotype",
            clan_conceived().with(|f| {
                f.pregnancies[0].1.3[0].2 = Genes(f.pregnancies[0].1.3[0].1, WILD, DUMMY)
            }),
        ),
        (
            "a pregnancy with another sex",
            clan_conceived().with(|f| f.pregnancies[0].1.3[0].1 = FEMALE),
        ),
        (
            "a pregnancy with another lineage key",
            clan_conceived().with(|f| f.pregnancies[0].1.3[0].0 = CLAN_CHILD + 1),
        ),
        (
            "a pregnancy conceived in a month whose draw failed",
            clan_conceived().with(|f| {
                f.pregnancies[0].1.4 = 4 * MONTH;
                moved_birth(f, 14 * MONTH);
            }),
        ),
        (
            "a pregnancy conceived between two month starts",
            clan_conceived().with(|f| {
                f.pregnancies[0].1.4 = 5 * MONTH - 1;
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
                take_event(f, 8);
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
                take_event(f, 0);
            }),
        ),
        (
            "the girl's attempts a month late",
            clan_conceived().with(|f| f.queue[0].0 += MONTH),
        ),
        (
            "the girl's attempts without their period",
            clan_conceived().with(|f| f.queue[0].4 = None),
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
    file.households.clear();
    file.next_household_id = 1;
    file.next_event_id = 2;
    start_with_founders(&mut file, Vec::new());
    file
}

/// A world that never started.
fn never_started() -> File {
    let mut file = File::tiny();
    file.world_seed = None;
    file.world_params = None;
    file.queue.clear();
    file.plan_refs.clear();
    file.next_event_seq = 1;
    file.sims.clear();
    file.next_sim_id = 1;
    file.households.clear();
    file.next_household_id = 1;
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
        capacity: None,
        pool: None,
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
                let mut sim = living(1, -1);
                sim.8 = 1;
                f.sims.push((5, sim));
                f.queue.insert(1, (Y - 1, 0, 6, birthday(5), None));
                f.plan_refs.insert(1, (2, 0));
                with_deaths(f, 5, TINY_DEATHS[1], 7, (2, 0));
                let leaving = (19 * Y - 1, 0, 9, Payload::LeaveHome { sim: 5 }, None);
                insert_event(f, leaving, (2, 0));
                f.next_event_seq = 10;
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
                f.queue.insert(1, (Y - 1, 0, 6, birthday(1), None));
                f.plan_refs.insert(1, (2, 0));
                f.next_event_seq = 7;
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
            File::tiny().with(|f| f.plan_refs[0].0 = 4),
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
        // Review finding: before the start there are no households.
        (
            "a household before the start",
            never_started().with(|f| {
                f.households = vec![(1, (0, None))];
                f.next_household_id = 2;
            }),
        ),
        (
            "a household counter ahead before the start",
            never_started().with(|f| f.next_household_id = 2),
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

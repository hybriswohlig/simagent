//! The persons of the pool in the town (ADR-0020 section 5, design note of the arrival):
//! the founders of a town drawn from the pool at `Start`, the yearly immigration, each
//! household with its partnership, its pregnancies and its plans shifted by `Δ = A − E_p`,
//! the kinship over the records of the pool, the data checks of a pool, the rejections of
//! `Start` and the load checks.
//!
//! The expected values come from `tools/arrival_reference.py`, which runs the model of
//! `tools/world_reference.py`, written from the design note and not from this code. Saves,
//! input records and pedigrees are pinned by their length and xxh3; a rejected save is the
//! save of "arrival" at the end of year 2 with one stretch of bytes replaced, as the
//! reference changed it.
//!
//! Each town runs on the documents its pool was extracted from, whose data hash is the
//! pool's `frozen_hash` (design note of the pool's records, section 6, K7): a town on
//! `pool_c`, or on a pool changed from it, on the documents of the pool run `father_dies`
//! (`tests/golden/father_dies`); a town on `pool_b` or `pool_d` on the template of map 4
//! (`tests/golden/map4_template`) with the repository's life course data. All with the
//! switch on, the founder tag `test:arrival`, no founders:
//! - "arrival": `pool_c` (43 households, 75 members), seed 0xff, `n_start` 50, so one
//!   person a year (`q` = 1), to the end of year 13; the immigration of year 14 finds the
//!   pool empty. Its marks: the immigration of years 0 and 1, the birth of a pool child
//!   whose father is a record only, the first children of two pool persons born in the
//!   town (twins, sims 60 and 61), a separation carried from the pool, the death of the
//!   immigrant of year 1 (sim 62).
//! - "orphans": `pool_d`, seed 5, `n_start` 30 (`q` = 0), to the end of year 4: two
//!   dependent children pass to their other parent, an orphan to a relative.
//! - "records": `pool_d`, seed 6, `n_start` 30, sims 26 and 31 paired at 18 years, to the
//!   end of year 18: an orphan finds a relative only through the records; a child of a pool
//!   person and a town person.
//! - "micro": `pool_c` with the carried separation of the woman of household 41 moved to
//!   `E_p`; the data check rejects it (a carried time at the freeze, ADR-0020 section 3).
//! - "empty_at_start": `pool_b` (29 households, 33 members): `Start` takes every household,
//!   the immigration at second 0 finds none.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use sim_core::data::{Data, DataError, Definitions};
use sim_core::data_id::DataId;
use sim_core::finite::FiniteF64;
use sim_core::genome::Sex;
use sim_core::hash::stable_hash64;
use sim_core::ids::{InputSeq, SimId};
use sim_core::input::{
    ArrangePartnership, Couple, ForkOverride, Founder, Input, InputRecord, PoolStart, Start,
};
use sim_core::log::{LogEntry, log_hash};
use sim_core::params::{ParamChange, WorldParams};
use sim_core::pool::{Pool, ranked_households, take_households};
use sim_core::presentation::card::DiseaseStatus;
use sim_core::time::SimTime;
use sim_core::world::{HeightMean, LoadError, World, YearOut};

const Y: i64 = 31_449_600;
const TAG: &str = "test:arrival";

/// An entry: its postcard bytes, and its JSON form where the reference gives one.
type Entry = (&'static str, Option<&'static str>);
/// A second of a scenario: its label, the second, the save before it and after it, as
/// (length, xxh3), and its entries.
type Mark = (
    &'static str,
    i64,
    (usize, u64),
    (usize, u64),
    &'static [Entry],
);

const POOL_A: &[u8] = include_bytes!("../../../tests/golden/pool/pool_a.bin");
const POOL_B: &[u8] = include_bytes!("../../../tests/golden/pool/pool_b.bin");
const POOL_C: &[u8] = include_bytes!("../../../tests/golden/pool/pool_c.bin");
const POOL_D: &[u8] = include_bytes!("../../../tests/golden/pool/pool_d.bin");
const POOL_E: &[u8] = include_bytes!("../../../tests/golden/pool/pool_e.bin");
const POOL_F: &[u8] = include_bytes!("../../../tests/golden/pool/pool_f.bin");

/// The reader's rejected files, which no data can hold either.
const READER_REJECTED: [(&str, &[u8]); 18] = [
    (
        "ancestors_out_of_order.bin",
        include_bytes!("../../../tests/golden/pool/reject/ancestors_out_of_order.bin"),
    ),
    (
        "household_without_adult.bin",
        include_bytes!("../../../tests/golden/pool/reject/household_without_adult.bin"),
    ),
    (
        "magic.bin",
        include_bytes!("../../../tests/golden/pool/reject/magic.bin"),
    ),
    (
        "members_out_of_order.bin",
        include_bytes!("../../../tests/golden/pool/reject/members_out_of_order.bin"),
    ),
    (
        "parent_not_a_record.bin",
        include_bytes!("../../../tests/golden/pool/reject/parent_not_a_record.bin"),
    ),
    (
        "partnership_not_members.bin",
        include_bytes!("../../../tests/golden/pool/reject/partnership_not_members.bin"),
    ),
    (
        "trailing_bytes.bin",
        include_bytes!("../../../tests/golden/pool/reject/trailing_bytes.bin"),
    ),
    (
        "version.bin",
        include_bytes!("../../../tests/golden/pool/reject/version.bin"),
    ),
    (
        "frozen_at.bin",
        include_bytes!("../../../tests/golden/pool/reject/frozen_at.bin"),
    ),
    (
        "partnership_sex.bin",
        include_bytes!("../../../tests/golden/pool/reject/partnership_sex.bin"),
    ),
    (
        "pregnancy_mother.bin",
        include_bytes!("../../../tests/golden/pool/reject/pregnancy_mother.bin"),
    ),
    (
        "birth_at.bin",
        include_bytes!("../../../tests/golden/pool/reject/birth_at.bin"),
    ),
    (
        "conception_at.bin",
        include_bytes!("../../../tests/golden/pool/reject/conception_at.bin"),
    ),
    (
        "conception_month.bin",
        include_bytes!("../../../tests/golden/pool/reject/conception_month.bin"),
    ),
    (
        "separation_at.bin",
        include_bytes!("../../../tests/golden/pool/reject/separation_at.bin"),
    ),
    (
        "leave_home_at.bin",
        include_bytes!("../../../tests/golden/pool/reject/leave_home_at.bin"),
    ),
    (
        "carried_time.bin",
        include_bytes!("../../../tests/golden/pool/reject/carried_time.bin"),
    ),
    (
        "past_time.bin",
        include_bytes!("../../../tests/golden/pool/reject/past_time.bin"),
    ),
];

// ---------------------------------------------------------------------------------------
// The data, the starts and the runs

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex digits"))
        .collect()
}

/// A rejected save as a patch of the base save: its case, the bytes kept in front and at
/// the end, the bytes between, and the save as (length, xxh3).
type Patch = (&'static str, usize, usize, &'static str, (usize, u64));

/// A change of a save: its offset, the bytes removed there, the bytes put in.
type Hunk = (usize, usize, &'static str);

/// A rejected save as hunks of another save: its case, the save it changes, that save as
/// (length, xxh3), the hunks, the changed save.
type Hunks = (
    &'static str,
    &'static str,
    (usize, u64),
    &'static [Hunk],
    (usize, u64),
);

/// Length and xxh3 of some bytes, as the reference pins a save, an input record or a file.
fn sized(bytes: &[u8]) -> (usize, u64) {
    (bytes.len(), stable_hash64(bytes))
}

/// The documents of a town: its life course data, its calibration and its genome map.
type Documents = (Value, Value, Value);

/// The template of map 4 with the repository's life course data.
fn template() -> Documents {
    let (calibration, genome_map) = common::files_from(
        include_str!("../../../tests/golden/map4_template/calibration/genetics.json"),
        include_str!("../../../tests/golden/map4_template/genetics/genome_map.json"),
    );
    (common::demography(), calibration, genome_map)
}

/// The documents `pool_c` was extracted from: the frozen documents of the pool run
/// `father_dies` without `pool_frequencies` (`tests/golden/father_dies`), whose data hash is
/// `pool_c`'s `frozen_hash`.
fn father_dies() -> Documents {
    let (calibration, genome_map) = common::files_from(
        include_str!("../../../tests/golden/father_dies/calibration/genetics.json"),
        include_str!("../../../tests/golden/father_dies/genetics/genome_map.json"),
    );
    let demography = common::demography_from(include_str!(
        "../../../tests/golden/father_dies/calibration/demography.json"
    ));
    (demography, calibration, genome_map)
}

/// The documents with a map without one locus.
fn without_locus((demography, calibration, mut genome_map): Documents, locus: &str) -> Documents {
    genome_map["loci"]
        .as_array_mut()
        .expect("loci")
        .retain(|entry| entry["id"] != locus);
    (demography, calibration, genome_map)
}

fn definitions((demography, calibration, genome_map): Documents) -> Definitions {
    common::definitions_with(demography, calibration, genome_map).expect("definitions")
}

/// The template with the golden pool `name` of these bytes (`pool_b`, `pool_d`) and the
/// frequencies it was made with.
fn town_data(name: &str, pool: &[u8]) -> Data {
    Data::with_pool(
        definitions(template()),
        pool,
        common::pool_frequencies(name),
    )
    .expect("checked")
}

/// The documents of `father_dies` with the pool of these bytes: `pool_c`, or a pool changed
/// from it; the frequencies of the file, which `pool_c` was made with.
fn pool_c_data(pool: &[u8]) -> Data {
    Data::with_pool(
        definitions(father_dies()),
        pool,
        common::pool_frequencies("pool_c"),
    )
    .expect("checked")
}

/// The frequencies of the pool loci as `Data::with_pool` takes them.
type Frequencies = BTreeMap<DataId, FiniteF64>;

/// The file's frequencies with these changes: a locus set to `Some(q)` (added if new) or
/// removed with `None`.
fn with_q(changes: &[(&str, Option<f64>)]) -> Frequencies {
    let mut frequencies = common::pool_frequencies("pool_c");
    for (locus, q) in changes {
        let locus = DataId::new(locus).expect("id");
        match q {
            Some(q) => frequencies.insert(locus, FiniteF64::try_new(*q).expect("finite")),
            None => frequencies.remove(&locus),
        };
    }
    frequencies
}

/// `pool_c`, changed by `change`, written again.
fn changed_pool_c(change: impl FnOnce(&mut Pool)) -> Vec<u8> {
    let mut pool = Pool::from_bytes(POOL_C).expect("pool_c");
    change(&mut pool);
    pool.to_bytes()
}

/// `pool_c` with the carried separation of the woman of household 41 at `E_p` ("micro"),
/// which would fall due in the second she arrives; the data check rejects it.
fn micro_pool() -> Vec<u8> {
    changed_pool_c(|pool| {
        let frozen_at = pool.frozen_at;
        let woman = pool.households[41]
            .members
            .iter_mut()
            .find(|m| m.sex == Sex::Female && m.separation_at.is_some())
            .expect("her separation");
        woman.separation_at = Some(frozen_at);
    })
}

/// The part of `Start` that founds the town from the data's pool: the households of the
/// draw for occasion 0 until they bring `n_start` persons, as `PoolStart::drawn` resolves a
/// scenario (the start hashes below are the reference's).
fn pool_start(data: &Data, seed: u64, n_start: u64, immigration: bool) -> PoolStart {
    PoolStart::drawn(data, seed, n_start, immigration).expect("a pool")
}

/// Data without a pool draw no pool start (design note of the map-4 switch, section 13).
#[test]
fn data_without_a_pool_draw_no_pool_start() {
    let plain = Data::new(definitions(template())).expect("checked");
    assert_eq!(PoolStart::drawn(&plain, 0xff, 50, true), None);
}

fn start(seed: u64, founders: Vec<Founder>, couples: Vec<Couple>, pool: PoolStart) -> Start {
    Start {
        world_seed: seed,
        founder_tag: DataId::new(TAG).expect("id"),
        world_params: WorldParams {
            hereditary_diseases: true,
            nutrition: Vec::new(),
        },
        founders,
        couples,
        capacity: None,
        pool: Some(pool),
    }
}

/// The first record of an input log, `Start` stamped −1.
fn start_record(start: &Start) -> Vec<u8> {
    postcard::to_allocvec(&InputRecord {
        t: SimTime::from_secs(-1),
        seq: InputSeq::try_from(1).expect("valid"),
        input: Input::Start(start.clone()),
    })
    .expect("encodes")
}

fn sim(n: u64) -> SimId {
    SimId::try_from(n).expect("valid")
}

/// A scenario of the reference and what it pins.
struct Scenario {
    data: fn() -> Data,
    data_hash: u64,
    pool_hash: u64,
    seed: u64,
    n_start: u64,
    start_input: (usize, u64),
    save_after_start: (usize, u64),
    first_entries: &'static [Entry],
    draws: &'static [(u64, &'static [usize])],
    used: &'static [usize],
    marks: &'static [Mark],
    end: i64,
    year_hashes: &'static [u64],
    log: (usize, u64),
    state_hash: u64,
    save_end: (usize, u64),
    pedigree: Option<(usize, u64)>,
    phi: &'static [(&'static str, u64, u64, f64)],
}

const ARRIVAL: Scenario = Scenario {
    data: || pool_c_data(POOL_C),
    data_hash: ARRIVAL_DATA_HASH,
    pool_hash: ARRIVAL_POOL_HASH,
    seed: 0xff,
    n_start: 50,
    start_input: ARRIVAL_START_INPUT,
    save_after_start: ARRIVAL_SAVE_AFTER_START,
    first_entries: &ARRIVAL_FIRST_ENTRIES,
    draws: &ARRIVAL_DRAWS,
    used: &ARRIVAL_USED,
    marks: &ARRIVAL_MARKS,
    end: ARRIVAL_END,
    year_hashes: &ARRIVAL_YEAR_HASHES,
    log: ARRIVAL_LOG,
    state_hash: ARRIVAL_STATE_HASH,
    save_end: ARRIVAL_SAVE_END,
    pedigree: Some(ARRIVAL_PEDIGREE),
    phi: &ARRIVAL_PHI,
};

const ORPHANS: Scenario = Scenario {
    data: || town_data("pool_d", POOL_D),
    data_hash: ORPHANS_DATA_HASH,
    pool_hash: ORPHANS_POOL_HASH,
    seed: 5,
    n_start: 30,
    start_input: ORPHANS_START_INPUT,
    save_after_start: ORPHANS_SAVE_AFTER_START,
    first_entries: &ORPHANS_FIRST_ENTRIES,
    draws: &ORPHANS_DRAWS,
    used: &ORPHANS_USED,
    marks: &ORPHANS_MARKS,
    end: ORPHANS_END,
    year_hashes: &ORPHANS_YEAR_HASHES,
    log: ORPHANS_LOG,
    state_hash: ORPHANS_STATE_HASH,
    save_end: ORPHANS_SAVE_END,
    pedigree: Some(ORPHANS_PEDIGREE),
    phi: &ORPHANS_PHI,
};

const RECORDS: Scenario = Scenario {
    data: || town_data("pool_d", POOL_D),
    data_hash: RECORDS_DATA_HASH,
    pool_hash: RECORDS_POOL_HASH,
    seed: 6,
    n_start: 30,
    start_input: RECORDS_START_INPUT,
    save_after_start: RECORDS_SAVE_AFTER_START,
    first_entries: &RECORDS_FIRST_ENTRIES,
    draws: &RECORDS_DRAWS,
    used: &RECORDS_USED,
    marks: &RECORDS_MARKS,
    end: RECORDS_END,
    year_hashes: &RECORDS_YEAR_HASHES,
    log: RECORDS_LOG,
    state_hash: RECORDS_STATE_HASH,
    save_end: RECORDS_SAVE_END,
    pedigree: Some(RECORDS_PEDIGREE),
    phi: &RECORDS_PHI,
};

/// "records" pairs sims 26 and 31 by an input stamped one second before 18 years.
const RECORDS_ARRANGED_AT: i64 = 18 * Y - 1;

struct Run {
    world: World,
    out: YearOut,
    log: Vec<LogEntry>,
    data: fn() -> Data,
}

impl Run {
    /// The scenario's world after `Start`; its data, its input record, its first entries
    /// and its save against the reference.
    fn start(scenario: &Scenario) -> Run {
        let data = (scenario.data)();
        assert_eq!(data.hash(), scenario.data_hash, "data hash");
        assert_eq!(data.pool().expect("a pool").hash(), scenario.pool_hash);
        let start = start(
            scenario.seed,
            Vec::new(),
            Vec::new(),
            pool_start(&data, scenario.seed, scenario.n_start, true),
        );
        assert_eq!(sized(&start_record(&start)), scenario.start_input, "input");
        let mut world = World::new(data);
        world.submit(Input::Start(start)).expect("accepted");
        let log = world.take_log();
        let run = Run {
            world,
            out: YearOut::default(),
            log,
            data: scenario.data,
        };
        assert_eq!(
            postcard::to_allocvec(&run.world.input_log()[0]).expect("encodes"),
            start_record(match &run.world.input_log()[0].input {
                Input::Start(start) => start,
                _ => unreachable!("the first input is Start"),
            })
        );
        run.assert_entries(0, scenario.first_entries);
        assert_eq!(
            sized(&run.save()),
            scenario.save_after_start,
            "save after start"
        );
        run
    }

    fn play(&mut self, until: i64) {
        self.world
            .advance_until(&mut self.out, SimTime::from_secs(until), &mut |_| false);
        self.log.extend(self.world.take_log());
    }

    fn save(&self) -> Vec<u8> {
        self.world.save(&self.out.year_hashes).expect("saves")
    }

    /// The save loads and the loaded world saves the same bytes (ADR-0006 section 6).
    fn assert_loads(&self) {
        let bytes = self.save();
        let (world, years) = World::load(&bytes, (self.data)()).expect("loads");
        assert_eq!(world.save(&years).expect("saves"), bytes);
    }

    /// The entries from `from` on start with `expected`: the bytes of each, and its JSON
    /// form where the reference gives it.
    fn assert_entries(&self, from: usize, expected: &[Entry]) {
        let entries = &self.log[from..];
        assert!(entries.len() >= expected.len(), "entries");
        for (entry, (bytes, json)) in entries.iter().zip(expected) {
            let text = serde_json::to_string(entry).expect("json");
            assert_eq!(
                postcard::to_allocvec(entry).expect("encodes"),
                hex(bytes),
                "entry {text}"
            );
            if let Some(json) = json {
                assert_eq!(text, *json);
            }
        }
    }

    /// Plays to the second before `mark`, then that second: both saves, the entries of the
    /// second (all of them), and the saves load.
    fn mark(&mut self, (label, at, before, after, entries): &Mark) {
        self.play(at - 1);
        assert_eq!(sized(&self.save()), *before, "before {label}");
        self.assert_loads();
        let from = self.log.len();
        self.play(*at);
        assert_eq!(self.log.len() - from, entries.len(), "entries of {label}");
        self.assert_entries(from, entries);
        assert_eq!(sized(&self.save()), *after, "after {label}");
        self.assert_loads();
    }

    /// The whole scenario: its marks, the inputs of "records", its end.
    fn scenario(scenario: &Scenario, arrange: bool) -> Run {
        let mut run = Run::start(scenario);
        for mark in scenario.marks {
            if arrange && mark.1 > RECORDS_ARRANGED_AT {
                run.arrange();
            }
            run.mark(mark);
        }
        run.play(scenario.end);
        let hashes: Vec<u64> = run.out.year_hashes.iter().map(|y| y.hash).collect();
        assert_eq!(hashes, scenario.year_hashes, "year hashes");
        assert_eq!((run.log.len(), log_hash(&run.log)), scenario.log, "log");
        assert_eq!(sized(&run.save()), scenario.save_end, "save at the end");
        assert_eq!(run.world.state_hash(), scenario.state_hash, "state hash");
        run.assert_loads();
        if let Some(pedigree) = scenario.pedigree {
            let text = serde_json::to_string(&run.world.pedigree()).expect("json");
            assert_eq!(sized(text.as_bytes()), pedigree, "pedigree");
        }
        for (label, a, b, phi) in scenario.phi {
            assert_eq!(
                run.world.state().kinship(sim(*a), sim(*b)),
                Some(*phi),
                "{label}"
            );
        }
        run
    }

    /// Sims 26 and 31 of "records" paired, stamped at 18 years less a second.
    fn arrange(&mut self) {
        let (before, after, entries) = RECORDS_ARRANGE;
        self.play(RECORDS_ARRANGED_AT);
        assert_eq!(sized(&self.save()), before, "before the arrangement");
        let from = self.log.len();
        self.world
            .submit(Input::ArrangePartnership(ArrangePartnership {
                woman: sim(26),
                man: sim(31),
            }))
            .expect("accepted");
        self.log.extend(self.world.take_log());
        assert_eq!(
            self.log.len() - from,
            entries.len(),
            "entries of the arrangement"
        );
        self.assert_entries(from, entries);
        assert_eq!(sized(&self.save()), after, "after the arrangement");
        self.assert_loads();
    }
}

/// The households each draw takes, in order: `Start` until `n_start` persons, then each
/// year's immigration one household (`q` = 1) or none.
fn assert_draws(scenario: &Scenario, years: u64, q: u64) {
    let data = (scenario.data)();
    let pool = data.pool().expect("a pool").pool();
    for (occasion, ranked) in scenario.draws {
        let all = ranked_households(pool, scenario.seed, *occasion);
        let order: Vec<usize> = all.into_iter().filter(|i| ranked.contains(i)).collect();
        assert_eq!(order, *ranked, "occasion {occasion}");
    }
    let mut used = BTreeSet::new();
    let (mut taken, _) = take_households(pool, scenario.seed, 0, &used, scenario.n_start);
    used.extend(taken.iter().copied());
    for year in 0..years {
        let (more, enough) = take_households(pool, scenario.seed, year, &used, q);
        assert!(enough, "year {year}");
        used.extend(more.iter().copied());
        taken.extend(more);
    }
    assert_eq!(taken, scenario.used);
}

// ---------------------------------------------------------------------------------------
// The data with a pool

/// A golden pool: its name, its bytes, the documents it was extracted from and their data
/// hash.
type GoldenPool = (&'static str, &'static [u8], fn() -> Documents, u64);

/// ADR-0020 section 5: the data hash covers the pool; each golden pool fits the documents
/// it was extracted from, whose data hash is its `frozen_hash` (design note of the pool's
/// records, section 6): `pool_c` those of `father_dies`, every other pool the template.
#[test]
fn the_data_hash_covers_the_pool() {
    assert_eq!(
        Data::new(definitions(template())).expect("checked").hash(),
        TEMPLATE_DATA_HASH
    );
    assert_eq!(
        Data::new(definitions(father_dies()))
            .expect("checked")
            .hash(),
        FATHER_DIES_DATA_HASH
    );
    let pools: [GoldenPool; 6] = [
        ("pool_a", POOL_A, template, TEMPLATE_DATA_HASH),
        ("pool_b", POOL_B, template, TEMPLATE_DATA_HASH),
        ("pool_c", POOL_C, father_dies, FATHER_DIES_DATA_HASH),
        ("pool_d", POOL_D, template, TEMPLATE_DATA_HASH),
        ("pool_e", POOL_E, template, TEMPLATE_DATA_HASH),
        ("pool_f", POOL_F, template, TEMPLATE_DATA_HASH),
    ];
    for ((name, data_hash, pool_hash, households, members), (file, bytes, documents, frozen)) in
        DATA_WITH_POOL.iter().zip(pools)
    {
        assert_eq!(*name, file);
        let data = Data::with_pool(
            definitions(documents()),
            bytes,
            common::pool_frequencies(file),
        )
        .expect("checked");
        assert_eq!(
            data.pool().expect("a pool").pool().frozen_hash,
            frozen,
            "{name}"
        );
        assert_eq!(data.hash(), *data_hash, "{name}");
        let pool = data.pool().expect("a pool");
        assert_eq!(pool.hash(), *pool_hash, "{name}");
        assert_eq!(pool.pool().households.len(), *households, "{name}");
        let in_households: usize = pool.pool().households.iter().map(|h| h.members.len()).sum();
        assert_eq!(in_households, *members, "{name}");
    }
}

/// The data check of a pool (design note, section 2; design note of the pool's records,
/// section 6; slice S2a of the map-4 switch), in its order: every carer is a member of its
/// household, every genome fits its sex, the seven loci are in the map, the pool's
/// `frozen_hash` is the hash of the documents, the frequencies name exactly the seven loci,
/// each frequency lies strictly between 0 and 1. The rows of the first three checks change
/// `pool_c` on its own documents (`father_dies`), so that each breaks only its check; a map
/// without a locus has another hash as well, so the locus rows also break
/// `pool.frozen_hash`, which comes after. The rows of K7: `pool_c` on the template, `pool_d`
/// on the documents of `father_dies`, and `pool_c` with its `frozen_hash` changed. Every row
/// carries the frequencies of the file (`pool_d` those it was made with) unless it changes
/// them: the rows of the frequencies take `pool_c` on its documents.
#[test]
fn the_data_check_rejects_a_pool_that_does_not_fit() {
    let pool = Pool::from_bytes(POOL_C).expect("pool_c");
    let child = pool.households[21]
        .members
        .iter()
        .find(|m| m.carer.is_some())
        .expect("a dependent child")
        .lineage_key;
    let stranger = pool.households[20].members[0].lineage_key;
    let flip = |sex: Sex| match sex {
        Sex::Female => Sex::Male,
        Sex::Male => Sex::Female,
    };
    let carer = move |pool: &mut Pool| {
        let member = pool.households[21]
            .members
            .iter_mut()
            .find(|m| m.lineage_key == child)
            .expect("the child");
        member.carer = Some(stranger);
    };
    let sex = move |pool: &mut Pool| {
        let member = &mut pool.households[0].members[0];
        member.sex = flip(member.sex);
    };
    let unborn_sex = move |pool: &mut Pool| {
        let unborn = &mut pool.households[28].pregnancies[0].children[0];
        unborn.sex = flip(unborn.sex);
    };
    let the_file = || common::pool_frequencies("pool_c");
    let pool_c = || POOL_C.to_vec();
    let cases: [(Vec<u8>, Documents, Frequencies); 26] = [
        (changed_pool_c(carer), father_dies(), the_file()),
        (changed_pool_c(sex), father_dies(), the_file()),
        (changed_pool_c(unborn_sex), father_dies(), the_file()),
        (
            pool_c(),
            without_locus(father_dies(), "core:mc1r"),
            the_file(),
        ),
        (
            pool_c(),
            without_locus(father_dies(), "core:herc2"),
            the_file(),
        ),
        (pool_c(), template(), the_file()),
        (
            POOL_D.to_vec(),
            father_dies(),
            common::pool_frequencies("pool_d"),
        ),
        (
            changed_pool_c(|pool| pool.frozen_hash ^= 1),
            father_dies(),
            the_file(),
        ),
        (
            changed_pool_c(|pool| {
                carer(pool);
                sex(pool);
            }),
            father_dies(),
            the_file(),
        ),
        (
            changed_pool_c(sex),
            without_locus(father_dies(), "core:mc1r"),
            the_file(),
        ),
        (
            changed_pool_c(carer),
            without_locus(father_dies(), "core:herc2"),
            the_file(),
        ),
        (pool_c(), without_locus(template(), "core:mc1r"), the_file()),
        (changed_pool_c(sex), template(), the_file()),
        (pool_c(), father_dies(), Frequencies::new()),
        (pool_c(), father_dies(), with_q(&[("core:herc2", None)])),
        (
            pool_c(),
            father_dies(),
            with_q(&[("core:disease_mt", None)]),
        ),
        (
            pool_c(),
            father_dies(),
            with_q(&[("core:growth_01", Some(0.5))]),
        ),
        (
            pool_c(),
            father_dies(),
            with_q(&[("test:unknown", Some(0.5))]),
        ),
        (pool_c(), father_dies(), with_q(&[("core:mc1r", Some(0.0))])),
        (pool_c(), father_dies(), with_q(&[("core:mc1r", Some(1.0))])),
        (
            pool_c(),
            father_dies(),
            with_q(&[("core:disease_ar", Some(-0.1))]),
        ),
        (
            pool_c(),
            father_dies(),
            with_q(&[("core:herc2", Some(1.5))]),
        ),
        (pool_c(), template(), Frequencies::new()),
        (pool_c(), template(), with_q(&[("core:mc1r", Some(0.0))])),
        (
            pool_c(),
            without_locus(father_dies(), "core:herc2"),
            with_q(&[("core:herc2", None)]),
        ),
        (
            pool_c(),
            father_dies(),
            with_q(&[("core:herc2", None), ("core:mc1r", Some(0.0))]),
        ),
    ];
    for ((name, code, file), (bytes, documents, frequencies)) in DATA_REJECTED.iter().zip(cases) {
        assert_eq!(sized(&bytes), *file, "{name}");
        let error = Data::with_pool(definitions(documents), &bytes, frequencies).expect_err(name);
        assert_eq!(error.to_string(), *code, "{name}");
    }
    for ((name, rule), (file, bytes)) in DATA_REJECTED_READER.iter().zip(READER_REJECTED) {
        assert_eq!(*name, file);
        let error = Data::with_pool(definitions(template()), bytes, the_file()).expect_err(name);
        assert_eq!(error.to_string(), format!("pool.{rule}"), "{name}");
    }
}

/// The twin of the range rows (slice S2a): the smallest `f64` above 0 at
/// `core:disease_ad_late` and the largest below 1 at `core:herc2` are accepted, and the
/// data hash covers them: it is not the hash with the file's frequencies. A frequency of 0.5
/// is accepted as well (`0 < q < 1`).
#[test]
fn the_data_check_accepts_a_frequency_just_inside_the_open_interval() {
    let twin = with_q(&[
        ("core:disease_ad_late", Some(f64::from_bits(1))),
        ("core:herc2", Some(f64::from_bits(0x3fef_ffff_ffff_ffff))),
    ]);
    let data = Data::with_pool(definitions(father_dies()), POOL_C, twin).expect("accepted");
    assert_eq!(data.hash(), FREQUENCY_TWIN_DATA_HASH);
    assert_ne!(data.hash(), ARRIVAL_DATA_HASH);
    let half = with_q(&[("core:mc1r", Some(0.5))]);
    assert!(Data::with_pool(definitions(father_dies()), POOL_C, half).is_ok());
}

/// Slice S2a: the data hash with a pool is xxh3 of the definitions' postcard bytes followed
/// by those of `(pool_seed, pool_hash, frequencies)`, the frequencies as `(locus id, q)`
/// ascending by id; the reference's appended bytes for `pool_c` with the file's frequencies
/// and for the twin with two frequencies changed. A changed frequency changes the data hash.
#[test]
fn the_data_hash_covers_the_frequencies() {
    let definitions_bytes = postcard::to_allocvec(&definitions(father_dies())).expect("encodes");
    assert_eq!(stable_hash64(&definitions_bytes), FATHER_DIES_DATA_HASH);
    let data = pool_c_data(POOL_C);
    let pool = data.pool().expect("a pool");
    assert_eq!(pool.frequencies(), &common::pool_frequencies("pool_c"));
    for (appended, data_hash) in [
        (POOL_C_APPENDED, ARRIVAL_DATA_HASH),
        (FREQUENCY_TWIN_APPENDED, FREQUENCY_TWIN_DATA_HASH),
    ] {
        let bytes = [definitions_bytes.clone(), hex(appended)].concat();
        assert_eq!(stable_hash64(&bytes), data_hash);
    }
    assert_eq!(data.hash(), ARRIVAL_DATA_HASH);
    let mut changed = common::pool_frequencies("pool_c");
    let mc1r = DataId::new("core:mc1r").expect("id");
    let next = f64::from_bits(changed[&mc1r].get().to_bits() + 1);
    changed.insert(mc1r, FiniteF64::try_new(next).expect("finite"));
    let other = Data::with_pool(definitions(father_dies()), POOL_C, changed).expect("accepted");
    assert_ne!(other.hash(), data.hash());
    assert_eq!(other.pool().expect("a pool").hash(), pool.hash());
}

/// K7 (design note of the pool's records, section 6): a pool loads only with the documents
/// it was extracted from. `pool_c` on the template is rejected with `pool.frozen_hash`; on
/// the documents of `father_dies` it is accepted; `pool_d`, whose pool run changes only
/// `pool_frequencies`, is accepted on the template.
#[test]
fn a_pool_loads_only_with_the_documents_it_was_extracted_from() {
    assert_eq!(
        Pool::from_bytes(POOL_C).expect("pool_c").frozen_hash,
        0xe2f2_44a7_e4cd_db5f
    );
    assert_eq!(
        Pool::from_bytes(POOL_D).expect("pool_d").frozen_hash,
        0x5ae3_d70c_71cf_063a
    );
    let error = Data::with_pool(
        definitions(template()),
        POOL_C,
        common::pool_frequencies("pool_c"),
    )
    .expect_err("other documents");
    assert_eq!(error, DataError::PoolFrozenHash);
    assert_eq!(error.to_string(), "pool.frozen_hash");
    let data = pool_c_data(POOL_C);
    assert_eq!(data.hash(), 0xc7fb_d166_8c8e_94f6);
    let data = town_data("pool_d", POOL_D);
    assert_eq!(data.hash(), 0xeccf_f256_9ac1_2bb4);
}

/// The codes of the data checks of a pool, `pool.frozen_hash` after `pool.locus` (design
/// note of the pool's records, section 6), the two of the frequencies after it (slice S2a);
/// their order is the rows' `order_*` above.
#[test]
fn each_data_check_of_a_pool_has_its_code() {
    let codes = [
        (DataError::PoolCarer, "pool.carer_not_member"),
        (DataError::PoolSex, "pool.sex"),
        (DataError::PoolLocus, "pool.locus"),
        (DataError::PoolFrozenHash, "pool.frozen_hash"),
        (DataError::PoolFrequenciesLoci, "pool.frequencies_loci"),
        (DataError::PoolFrequencyRange, "pool.frequency_range"),
    ];
    for (error, code) in codes {
        assert_eq!(error.to_string(), code);
    }
}

// ---------------------------------------------------------------------------------------
// Start

/// `Start` with a pool (design note, section 3): the old checks first, then the pool's in
/// their order: no founders or couples beside it, the data's pool, `n_start` at least 1,
/// the households of the draw exactly.
#[test]
fn start_rejects_a_pool_start_by_its_first_broken_rule() {
    let seed = 0xff;
    let data = || pool_c_data(POOL_C);
    let plain = || Data::new(definitions(template())).expect("checked");
    let other = || town_data("pool_b", POOL_B);
    let good = pool_start(&data(), seed, 50, true);
    let founder = |born: i64, sex: Sex| Founder {
        born: SimTime::from_secs(born),
        sex,
        alleles: Vec::new(),
    };
    let woman = || vec![founder(-30 * Y, Sex::Female)];
    let couple = || vec![founder(-30 * Y, Sex::Female), founder(-31 * Y, Sex::Male)];
    let pair = || vec![Couple { woman: 0, man: 1 }];
    let changed = |change: &dyn Fn(&mut PoolStart)| {
        let mut pool = good.clone();
        change(&mut pool);
        pool
    };
    let drawn = good.households.clone();
    let reversed: Vec<_> = drawn.iter().rev().cloned().collect();
    let pool_c = data();
    let ranked = ranked_households(pool_c.pool().expect("a pool").pool(), seed, 0);
    let mut extra = drawn.clone();
    extra.push(pool_c.pool().expect("a pool").pool().households[ranked[drawn.len()]].clone());
    let mut altered = drawn.clone();
    altered[0].members[0].conceptions += 1;
    let other_seed = pool_start(&pool_c, 0xfe, 50, true).households;
    let cases: Vec<(Data, Start)> = vec![
        (data(), start(seed, woman(), Vec::new(), good.clone())),
        (data(), start(seed, couple(), pair(), good.clone())),
        (plain(), start(seed, Vec::new(), Vec::new(), good.clone())),
        (
            data(),
            start(seed, Vec::new(), Vec::new(), changed(&|p| p.pool_hash ^= 1)),
        ),
        (other(), start(seed, Vec::new(), Vec::new(), good.clone())),
        (
            data(),
            start(
                seed,
                Vec::new(),
                Vec::new(),
                changed(&|p| {
                    p.n_start = 0;
                    p.households.clear();
                }),
            ),
        ),
        (
            data(),
            start(seed, Vec::new(), Vec::new(), changed(&|p| p.n_start = 0)),
        ),
        (
            data(),
            start(
                seed,
                Vec::new(),
                Vec::new(),
                changed(&|p| p.households.reverse()),
            ),
        ),
        (
            data(),
            start(
                seed,
                Vec::new(),
                Vec::new(),
                changed(&|p| {
                    p.households.pop();
                }),
            ),
        ),
        (
            data(),
            start(
                seed,
                Vec::new(),
                Vec::new(),
                changed(&|p| p.households.clone_from(&extra)),
            ),
        ),
        (
            data(),
            start(
                seed,
                Vec::new(),
                Vec::new(),
                changed(&|p| p.households.clone_from(&other_seed)),
            ),
        ),
        (
            data(),
            start(
                seed,
                Vec::new(),
                Vec::new(),
                changed(&|p| p.households.clone_from(&altered)),
            ),
        ),
        (
            plain(),
            start(seed, woman(), Vec::new(), changed(&|p| p.pool_hash = 1)),
        ),
        (
            plain(),
            start(seed, Vec::new(), Vec::new(), changed(&|p| p.n_start = 0)),
        ),
        (
            data(),
            start(
                seed,
                Vec::new(),
                Vec::new(),
                changed(&|p| {
                    p.n_start = 0;
                    p.households.clone_from(&reversed);
                }),
            ),
        ),
        (
            data(),
            Start {
                capacity: Some(0),
                ..start(seed, Vec::new(), Vec::new(), changed(&|p| p.pool_hash = 1))
            },
        ),
        (data(), start(seed, Vec::new(), pair(), good.clone())),
    ];
    for ((name, code, data_hash, input), (data, start)) in START_REJECTED.iter().zip(cases) {
        assert_eq!(data.hash(), *data_hash, "{name}");
        assert_eq!(sized(&start_record(&start)), *input, "{name}");
        let rejected = World::new(data)
            .submit(Input::Start(start))
            .expect_err(name);
        let code_bytes = postcard::to_allocvec(&rejected.code).expect("encodes");
        assert_eq!(
            code_bytes,
            postcard::to_allocvec(code).expect("encodes"),
            "{name}"
        );
    }
    let accepted = start(seed, Vec::new(), Vec::new(), good);
    assert_eq!(sized(&start_record(&accepted)), START_ACCEPTED);
    assert!(World::new(data()).submit(Input::Start(accepted)).is_ok());
}

/// A carried time at or before `E_p` would fall into or before the arrival second after
/// the shift, which ADR-0020 section 3 rules out: the data check rejects the pool with
/// `pool.carried_time` (design note of the pool's records, P5). In `pool_c` the dependent
/// child of household 33, which "arrival" draws at `Start`, leaves home one second before
/// `E_p` or at `E_p`; in "micro" the woman of household 41 separates at `E_p`.
#[test]
fn the_data_check_rejects_a_carried_time_at_or_before_the_freeze() {
    let leave_at = |offset: i64| {
        changed_pool_c(move |pool| {
            let frozen_at = pool.frozen_at;
            let child = pool.households[33]
                .members
                .iter_mut()
                .find(|m| m.leave_home_at.is_some())
                .expect("a dependent child");
            child.leave_home_at = Some(SimTime::from_secs(frozen_at.as_secs() + offset));
        })
    };
    let cases = [
        ("before", leave_at(-1), CARRIED_BEFORE_POOL),
        ("at the freeze", leave_at(0), LEAVE_AT_START_POOL),
        ("micro", micro_pool(), MICRO_POOL),
    ];
    for (name, bytes, pin) in cases {
        assert_eq!(sized(&bytes), pin, "{name}");
        let error = Data::with_pool(
            definitions(father_dies()),
            &bytes,
            common::pool_frequencies("pool_c"),
        )
        .expect_err(name);
        assert_eq!(error.to_string(), "pool.carried_time", "{name}");
    }
}

// ---------------------------------------------------------------------------------------
// The scenarios

#[test]
fn the_draws_take_the_households_of_the_reference() {
    assert_draws(&ARRIVAL, 14, 1);
    assert_draws(&ORPHANS, 0, 0);
    assert_draws(&RECORDS, 0, 0);
}

/// "arrival" to the end of year 13: immigration every year, the pool child whose father is
/// a record only, the child of two pool persons, the carried separation, the death of an
/// immigrant; kinship over the records.
#[test]
fn arrival_runs_as_the_reference() {
    Run::scenario(&ARRIVAL, false);
}

/// The immigration of year 14 finds the pool empty: the run stops (ADR-0020 section 5).
#[test]
#[should_panic(expected = "invariant pool.empty")]
fn an_empty_pool_stops_the_run() {
    let mut run = Run::start(&ARRIVAL);
    run.play(ARRIVAL_END);
    run.play(ARRIVAL_END + 1);
}

/// "orphans": dependent children of pool persons pass to their other parent, found by key.
#[test]
fn orphans_run_as_the_reference() {
    Run::scenario(&ORPHANS, false);
}

/// "records": an orphan finds a relative only through the records; a child of a pool
/// person and a town person; kinship across the records.
#[test]
fn records_run_as_the_reference() {
    Run::scenario(&RECORDS, true);
}

/// `Start` takes every household of a pool too small for `n_start`; the immigration at
/// second 0 then finds none.
#[test]
#[should_panic(expected = "invariant pool.empty")]
fn a_pool_too_small_for_the_start_stops_at_once() {
    let data = town_data("pool_b", POOL_B);
    assert_eq!(data.hash(), EMPTY_DATA_HASH);
    let pool = data.pool().expect("a pool").pool();
    for (occasion, ranked) in EMPTY_DRAWS {
        assert_eq!(ranked_households(pool, 0xff, occasion), ranked);
    }
    let start = start(
        0xff,
        Vec::new(),
        Vec::new(),
        pool_start(&data, 0xff, 50, true),
    );
    let taken: Vec<_> = EMPTY_USED
        .iter()
        .map(|&i| pool.households[i].clone())
        .collect();
    assert_eq!(start.pool.as_ref().expect("a pool").households, taken);
    assert_eq!(sized(&start_record(&start)), EMPTY_START_INPUT);
    let mut world = World::new(data);
    world.submit(Input::Start(start)).expect("accepted");
    world.take_log();
    assert_eq!(
        sized(&world.save(&[]).expect("saves")),
        EMPTY_SAVE_AFTER_START
    );
    world.advance_until(&mut YearOut::default(), SimTime::from_secs(0), &mut |_| {
        false
    });
}

// ---------------------------------------------------------------------------------------
// Loading

/// The load checks of the arrival (design note, section 9): "arrival" at the end of year 2
/// loads; each state changed in one respect is rejected.
#[test]
fn load_rejects_a_state_the_arrival_cannot_give() {
    let mut run = Run::start(&ARRIVAL);
    run.play(LOAD_BASE_AT);
    let base = run.save();
    assert_eq!(sized(&base), LOAD_BASE);
    run.assert_loads();
    for (name, front, back, between, expected) in LOAD_REJECTED {
        let mut bytes = base[..front].to_vec();
        bytes.extend(hex(between));
        bytes.extend(&base[base.len() - back..]);
        assert_eq!(sized(&bytes), expected, "{name}");
        assert_eq!(
            World::load(&bytes, pool_c_data(POOL_C)).err(),
            Some(LoadError::Inconsistent),
            "{name}"
        );
    }
}

/// The saves the further rejected states change, by the reference's label, with their data.
fn base_save(label: &str) -> (Vec<u8>, fn() -> Data) {
    match label {
        "arrival_save_after_start" => (Run::start(&ARRIVAL).save(), ARRIVAL.data),
        "records_save_after_arrange" => {
            let mut run = Run::start(&RECORDS);
            run.arrange();
            (run.save(), RECORDS.data)
        }
        "load_base" => {
            let mut run = Run::start(&ARRIVAL);
            run.play(LOAD_BASE_AT);
            (run.save(), ARRIVAL.data)
        }
        "unstarted" => (
            World::new(pool_c_data(POOL_C)).save(&[]).expect("saves"),
            ARRIVAL.data,
        ),
        "orphans_save_end" => {
            let mut run = Run::start(&ORPHANS);
            run.play(ORPHANS_END);
            (run.save(), ORPHANS.data)
        }
        "arrival_save_after_pool_child_without_father" => {
            let mut run = Run::start(&ARRIVAL);
            run.play(POOL_CHILD_WITHOUT_FATHER_AT);
            (run.save(), ARRIVAL.data)
        }
        "founders_world" => (founders_save(), founders_data),
        "arrival_save_after_immigration_1" => (town_save(&ARRIVAL_AT_SEED, Y), ARRIVAL.data),
        "arrival_save_end" => (town_save(&ARRIVAL_AT_SEED, ARRIVAL_END), ARRIVAL.data),
        "V3_carried_series_ended" => (town_save(&V3, V3.clock), ARRIVAL.data),
        "V4b_carried_conception_pregnant" => (town_save(&ARRIVAL_AT_SEED, V4B_AT), ARRIVAL.data),
        "V5_pool_twins_born" => (town_save(&ARRIVAL_AT_SEED, V5_TWINS_AT), ARRIVAL.data),
        "V6_pool_pregnancy_cancelled" => (town_save(&V6, V6.clock), V6.data),
        "series_end_boundary" => (town_save(&SERIES_END, MONTH_1), SERIES_END.data),
        "arrival_moved_out_by_town_partnership" => {
            let mut run = Run::start(&RECORDS);
            run.play(RECORDS_MOVED_OUT_BY_TOWN_PARTNERSHIP_AT);
            (run.save(), RECORDS.data)
        }
        "brought_partnership_member_not_moved_out" => {
            let data = brought_not_moved_out_data();
            assert_eq!(data.hash(), BROUGHT_NOT_MOVED_OUT_DATA_HASH);
            let start = start(
                0xff,
                Vec::new(),
                Vec::new(),
                pool_start(&data, 0xff, 50, true),
            );
            let mut world = World::new(data);
            world.submit(Input::Start(start)).expect("accepted");
            world.take_log();
            (world.save(&[]).expect("saves"), brought_not_moved_out_data)
        }
        _ => unreachable!("a save of the reference"),
    }
}

/// `pool_c` with the man of household 39's open partnership not moved out, his leaving
/// home carried one year after `E_p`: a brought partnership moves nobody out in the town.
fn brought_not_moved_out_pool() -> Vec<u8> {
    changed_pool_c(|pool| {
        let frozen_at = pool.frozen_at;
        let household = &mut pool.households[39];
        let man = household.partnership.expect("a partnership").man;
        let him = household
            .members
            .iter_mut()
            .find(|m| m.lineage_key == man)
            .expect("the man");
        him.moved_out = false;
        him.leave_home_at = Some(SimTime::from_secs(frozen_at.as_secs() + Y));
    })
}

fn brought_not_moved_out_data() -> Data {
    let pool = brought_not_moved_out_pool();
    assert_eq!(sized(&pool), BROUGHT_NOT_MOVED_OUT_POOL);
    pool_c_data(&pool)
}

/// A town of the reference founded from a pool, without inputs, saved at a clock.
struct Town {
    data: fn() -> Data,
    seed: u64,
    n_start: u64,
    clock: i64,
}

/// "arrival" itself, for saves at other seconds than its marks.
const ARRIVAL_AT_SEED: Town = Town {
    data: || pool_c_data(POOL_C),
    seed: 0xff,
    n_start: 50,
    clock: 0,
};

/// "arrival" with the seed 3: woman 40 (household 41) fails the attempt of her carried
/// series at the clock, and the next would fall after her fertility: none is pending.
const V3: Town = Town {
    data: || pool_c_data(POOL_C),
    seed: 3,
    n_start: 50,
    clock: 140_313_600,
};

/// `pool_d` with the seed 0xdb: the pool mother 27 dies before her pool birth.
const V6: Town = Town {
    data: || town_data("pool_d", POOL_D),
    seed: 0xdb,
    n_start: 30,
    clock: 3_263_752,
};

/// The town's save at `clock`.
fn town_save(town: &Town, clock: i64) -> Vec<u8> {
    let data = (town.data)();
    let start = start(
        town.seed,
        Vec::new(),
        Vec::new(),
        pool_start(&data, town.seed, town.n_start, true),
    );
    let mut run = Run {
        world: World::new(data),
        out: YearOut::default(),
        log: Vec::new(),
        data: town.data,
    };
    run.world.submit(Input::Start(start)).expect("accepted");
    run.world.take_log();
    run.play(clock);
    run.save()
}

/// The plans an arrival brought, checked at load (design note of the carried plans): the
/// valid states of the reference load, among them a carried series ended at the fertility
/// limit, conceptions in a carried series, pool twins born, and a pool pregnancy ended
/// with its mother; each state changed in one rule (C1 the series' second, presence and
/// end; C2 a conception before the series; C3 a pool pregnancy pending, born, or ended
/// without a child;
/// C4 the plan of a carried event; C5 their order) does not.
#[test]
fn load_checks_the_plans_an_arrival_brought() {
    let valid: [(&str, Vec<u8>, (usize, u64)); 5] = [
        ("V3", town_save(&V3, V3.clock), V3_SAVE),
        ("V4", town_save(&ARRIVAL_AT_SEED, MONTH_1), V4_SAVE),
        ("V4b", town_save(&ARRIVAL_AT_SEED, V4B_AT), V4B_SAVE),
        (
            "V5 twins",
            town_save(&ARRIVAL_AT_SEED, V5_TWINS_AT),
            V5_TWINS_SAVE,
        ),
        ("V6", town_save(&V6, V6.clock), V6_SAVE),
    ];
    for (name, bytes, pin) in valid {
        assert_eq!(sized(&bytes), pin, "{name}");
        let data = if name == "V6" { V6.data } else { ARRIVAL.data };
        assert!(World::load(&bytes, data()).is_ok(), "{name} loads");
    }
    for (name, label, base_pin, hunks, expected) in LOAD_REJECTED_CARRIED {
        let (base, data) = base_save(label);
        assert_eq!(sized(&base), base_pin, "{name}");
        assert!(World::load(&base, data()).is_ok(), "{name}: the base loads");
        let mut bytes = base.clone();
        for (offset, removed, put) in hunks.iter().rev() {
            bytes.splice(*offset..offset + removed, hex(put));
        }
        assert_eq!(sized(&bytes), expected, "{name}");
        assert_eq!(
            World::load(&bytes, data()).err(),
            Some(LoadError::Inconsistent),
            "{name}"
        );
    }
}

/// `pool_c` with the woman of household 24 born so that the second attempt of her carried
/// series falls exactly at 50 years, the end of her fertility.
fn series_end_pool() -> Vec<u8> {
    changed_pool_c(|pool| {
        let household = &mut pool.households[24];
        let woman = household.partnership.expect("a partnership").woman;
        let her = household
            .members
            .iter_mut()
            .find(|m| m.lineage_key == woman)
            .expect("the woman");
        let first = her.conception_at.expect("a carried series").as_secs();
        her.born = SimTime::from_secs(first + 2_419_200 - 50 * Y);
    })
}

fn series_end_data() -> Data {
    let pool = series_end_pool();
    assert_eq!(sized(&pool), SERIES_END_POOL);
    pool_c_data(&pool)
}

/// "arrival" on that pool: woman 28 fails her carried attempt at its first second; the next
/// would come exactly at the end of her fertility, so none is pending.
const SERIES_END: Town = Town {
    data: series_end_data,
    seed: 0xff,
    n_start: 50,
    clock: MONTH_1,
};

/// "arrival" with the seed 6: pool household 38 arrives with the immigration of year 1 and
/// its pool child is born in year 1. A pool child's deaths come from its birth with the
/// detail 0, not from its arrival year (design note of the arrival, 6.4).
const IMMIGRANT_CHILD: Town = Town {
    data: || pool_c_data(POOL_C),
    seed: 6,
    n_start: 50,
    clock: 2 * Y - 1,
};

/// The last gaps of the mutation runs (the reference's round 9): the pending immigration
/// broken in its period, its second or its persons; a pool mother's conceptions off by one;
/// a carried series with its attempt at the end of fertility. Each base loads, each changed
/// save does not; and a pool child of an immigrant born in year 1 runs as the reference.
#[test]
fn load_rejects_the_last_states_of_the_mutation_runs() {
    let data = (IMMIGRANT_CHILD.data)();
    let start = start(6, Vec::new(), Vec::new(), pool_start(&data, 6, 50, true));
    let mut run = Run {
        world: World::new(data),
        out: YearOut::default(),
        log: Vec::new(),
        data: IMMIGRANT_CHILD.data,
    };
    run.world.submit(Input::Start(start)).expect("accepted");
    run.log.extend(run.world.take_log());
    run.play(IMMIGRANT_CHILD.clock);
    let hashes: Vec<u64> = run.out.year_hashes.iter().map(|y| y.hash).collect();
    assert_eq!(hashes, IMMIGRANT_CHILD_YEAR_HASHES);
    assert_eq!(log_hash(&run.log), IMMIGRANT_CHILD_LOG_HASH);
    assert_eq!(run.world.state_hash(), IMMIGRANT_CHILD_STATE_HASH);
    assert_eq!(sized(&run.save()), IMMIGRANT_CHILD_SAVE);
    run.assert_loads();
    assert_eq!(
        sized(&town_save(&SERIES_END, SERIES_END.clock)),
        SERIES_END_SAVE
    );
    for (name, label, base_pin, hunks, expected) in LOAD_REJECTED_ROUND_9 {
        let (base, data) = base_save(label);
        assert_eq!(sized(&base), base_pin, "{name}");
        assert!(World::load(&base, data()).is_ok(), "{name}: the base loads");
        let mut bytes = base.clone();
        for (offset, removed, put) in hunks.iter().rev() {
            bytes.splice(*offset..offset + removed, hex(put));
        }
        assert_eq!(sized(&bytes), expected, "{name}");
        assert_eq!(
            World::load(&bytes, data()).err(),
            Some(LoadError::Inconsistent),
            "{name}"
        );
    }
}

/// The template without a pool.
fn founders_data() -> Data {
    Data::new(definitions(template())).expect("checked")
}

/// A town of four founders and no pool at `FOUNDERS_AT`: a couple of 25 and 27, a woman of
/// 88 and a man of 90. The woman of the couple is pregnant, the old ones have died. Each
/// founder names every locus of the template at the reference allele, so it draws nothing
/// (Grok F1) and has the genome of the reference.
fn founders_save() -> Vec<u8> {
    let data = founders_data();
    assert_eq!(data.hash(), FOUNDERS_DATA_HASH);
    let founder = |born: i64, sex: Sex| Founder {
        born: SimTime::from_secs(born),
        sex,
        alleles: common::fixed(&data, sex, &[]),
    };
    let start = Start {
        world_seed: 0xff,
        founder_tag: DataId::new(TAG).expect("id"),
        world_params: WorldParams {
            hereditary_diseases: true,
            nutrition: Vec::new(),
        },
        founders: vec![
            founder(-25 * Y, Sex::Female),
            founder(-27 * Y, Sex::Male),
            founder(-88 * Y, Sex::Female),
            founder(-90 * Y, Sex::Male),
        ],
        couples: vec![Couple { woman: 0, man: 1 }],
        capacity: None,
        pool: None,
    };
    assert_eq!(sized(&start_record(&start)), FOUNDERS_START_INPUT);
    let mut world = World::new(data);
    world.submit(Input::Start(start)).expect("accepted");
    world.take_log();
    let mut out = YearOut::default();
    world.advance_until(&mut out, SimTime::from_secs(FOUNDERS_AT), &mut |_| false);
    world.take_log();
    world.save(&out.year_hashes).expect("saves")
}

/// The load check one field at a time (design note, section 9; mutation run): each state
/// changes one field of an arrival, of a pool child or of a world without a pool, breaks
/// one rule of the arrivals' kinship, or moves an arrival out against its partnerships (one
/// formed in the town moves it out, a brought one does not). Each base loads; each changed
/// save does not.
#[test]
fn load_rejects_a_state_changed_in_one_field() {
    for (name, label, base_pin, hunks, expected) in LOAD_REJECTED_FIELDS {
        let (base, data) = base_save(label);
        assert_eq!(sized(&base), base_pin, "{name}");
        assert!(World::load(&base, data()).is_ok(), "{name}: the base loads");
        let mut bytes = base.clone();
        for (offset, removed, put) in hunks.iter().rev() {
            bytes.splice(*offset..offset + removed, hex(put));
        }
        assert_eq!(sized(&bytes), expected, "{name}");
        assert_eq!(
            World::load(&bytes, data()).err(),
            Some(LoadError::Inconsistent),
            "{name}"
        );
    }
}

/// "arrival" saved at the end of year 2, loaded and run on, ends as the run without the
/// save (ADR-0004 test 4): the same year hashes, log, save and state.
#[test]
fn a_loaded_town_runs_on_as_the_uninterrupted_one() {
    let mut run = Run::start(&ARRIVAL);
    run.play(LOAD_BASE_AT);
    let bytes = run.save();
    assert_eq!(sized(&bytes), LOAD_BASE);
    let (world, years) = World::load(&bytes, pool_c_data(POOL_C)).expect("loads");
    let mut resumed = Run {
        world,
        out: YearOut {
            census: Vec::new(),
            year_hashes: years,
            trace: Vec::new(),
        },
        log: run.log,
        data: ARRIVAL.data,
    };
    resumed.play(ARRIVAL_END);
    let hashes: Vec<u64> = resumed.out.year_hashes.iter().map(|y| y.hash).collect();
    assert_eq!(hashes, ARRIVAL_YEAR_HASHES, "year hashes");
    assert_eq!(
        (resumed.log.len(), log_hash(&resumed.log)),
        ARRIVAL_LOG,
        "log"
    );
    assert_eq!(sized(&resumed.save()), ARRIVAL_SAVE_END, "save at the end");
    assert_eq!(resumed.world.state_hash(), ARRIVAL_STATE_HASH, "state hash");
}

/// The load checks of the review (design note, sections 5 and 9; D8): the father of a
/// carried pregnancy dropped although he came with her; the town's series of an arrival
/// missing after her pairing in the town; the immigrations of years 1 and 2 swapped; a world
/// that never started with a used household. Each base loads; each changed save does not.
#[test]
fn load_rejects_the_states_of_the_review() {
    for (name, label, base_pin, hunks, expected) in LOAD_REJECTED_MORE {
        let (base, data) = base_save(label);
        assert_eq!(sized(&base), base_pin, "{name}");
        assert!(World::load(&base, data()).is_ok(), "{name}: the base loads");
        let mut bytes = base.clone();
        for (offset, removed, put) in hunks.iter().rev() {
            bytes.splice(*offset..offset + removed, hex(put));
        }
        assert_eq!(sized(&bytes), expected, "{name}");
        assert_eq!(
            World::load(&bytes, data()).err(),
            Some(LoadError::Inconsistent),
            "{name}"
        );
    }
}

/// A woman who gives birth and is paired by the market in the same second (review): the
/// birth runs before the round, so her series starts from the pairing, at the first month
/// start after it, not after the pause of the birth. "arrival" with the seed 0xe: woman 37
/// (single, her pool pregnancy's father a record only) gives birth at `TIE_AT` and the
/// round of that second pairs her with man 19 (partnership 15). Both saves load; the save
/// with her series after the pause does not.
#[test]
fn a_birth_and_a_market_pairing_in_one_second_start_the_series_at_the_pairing() {
    let data = pool_c_data(POOL_C);
    let start = start(
        TIE_SEED,
        Vec::new(),
        Vec::new(),
        pool_start(&data, TIE_SEED, 50, true),
    );
    assert_eq!(sized(&start_record(&start)), TIE_START_INPUT);
    let mut world = World::new(data);
    world.submit(Input::Start(start)).expect("accepted");
    let log = world.take_log();
    let mut run = Run {
        world,
        out: YearOut::default(),
        log,
        data: ARRIVAL.data,
    };
    run.play(TIE_AT);
    let at = run.save();
    assert_eq!(sized(&at), TIE_SAVE_AT);
    run.assert_loads();
    let (hunks, expected) = TIE_COUNTED_BIRTH;
    let mut counted = at.clone();
    for (offset, removed, put) in hunks.iter().rev() {
        counted.splice(*offset..offset + removed, hex(put));
    }
    assert_eq!(sized(&counted), expected);
    assert_eq!(
        World::load(&counted, pool_c_data(POOL_C)).err(),
        Some(LoadError::Inconsistent)
    );
    run.play(TIE_FIRST_ATTEMPT - 1);
    assert_eq!(sized(&run.save()), TIE_SAVE_BEFORE_ATTEMPT);
    run.assert_loads();
}

// ---------------------------------------------------------------------------------------
// The switch an arrival saw

/// "arrival"'s town (`pool_c`, `n_start` 50, immigration) with the seed `seed` and the
/// switch `on` at `Start`, its log so far.
fn switched_town(seed: u64, on: bool) -> Run {
    let data = pool_c_data(POOL_C);
    let mut start = start(
        seed,
        Vec::new(),
        Vec::new(),
        pool_start(&data, seed, 50, true),
    );
    start.world_params.hereditary_diseases = on;
    let mut world = World::new(data);
    world.submit(Input::Start(start)).expect("accepted");
    let log = world.take_log();
    Run {
        world,
        out: YearOut::default(),
        log,
        data: ARRIVAL.data,
    }
}

impl Run {
    /// A fork that sets the switch, stamped with the last closed second.
    fn fork(&mut self, on: bool) {
        self.world
            .submit(Input::ForkOverride(ForkOverride {
                changes: vec![ParamChange::HereditaryDiseases(on)],
            }))
            .expect("accepted");
        self.log.extend(self.world.take_log());
    }

    /// The onset entries caused by a `core:arrived` entry for which `arrived` holds.
    fn onsets_at_arrival(&self, arrived: impl Fn(&Value) -> bool) -> usize {
        let entries: Vec<Value> = (self.log.iter())
            .map(|entry| serde_json::to_value(entry).expect("json"))
            .collect();
        let ids: BTreeSet<u64> = (entries.iter())
            .filter(|entry| entry["payload"].get("core:arrived").is_some() && arrived(entry))
            .filter_map(|entry| entry["id"].as_u64())
            .collect();
        (entries.iter())
            .filter(|entry| entry["payload"].get("core:disease_onset").is_some())
            .filter(|entry| {
                (entry["causes"][0]["event"].as_u64()).is_some_and(|id| ids.contains(&id))
            })
            .count()
    }
}

/// A fork submitted right after `Start` is stamped before the start and acts from second 0
/// on, after `Start` and before the immigration of second 0 (ADR-0017 section 3, addendum):
/// what `Start` brought saw `Start`'s own switch, the immigration sees the fork's. Household
/// 40 of `pool_c` has a second member with an onset reached at its arrival and plans it
/// brought, a third with plans too (found with the reference), so their arrival entries are
/// one apart, or two with the onset between. With seed 1 it comes with `Start` (the sims 4 to
/// 6), with seed 150 by the immigration of second 0 (the sims 52 to 54). Every save loads,
/// before second 0 and after it.
#[test]
fn a_fork_before_the_start_acts_after_start_and_before_the_immigration() {
    let at_arrival = |sim: u64| move |entry: &Value| entry["subjects"][0]["sim"] == sim;
    for on in [true, false] {
        let mut run = switched_town(1, on);
        run.fork(!on);
        assert_eq!(
            run.onsets_at_arrival(at_arrival(5)),
            usize::from(on),
            "{on}"
        );
        run.assert_loads();
        run.play(0);
        run.assert_loads();
        let mut run = switched_town(150, on);
        run.fork(!on);
        run.play(0);
        assert_eq!(
            run.onsets_at_arrival(at_arrival(53)),
            usize::from(!on),
            "{on}"
        );
        run.assert_loads();
    }
}

/// An immigration sees the switch of its second, after the inputs stamped before it: in
/// "arrival" the immigration of year 9 brings household 40 of `pool_c` alone, its second
/// member with an onset reached at the arrival and plans it brought, its third with plans
/// (found with the reference). Its entry stands between their arrivals with the switch on
/// then, not with it off (off from the start, or on until a fork stamped `9 · YEAR − 1`);
/// each save after the arrival loads.
#[test]
fn an_immigration_sees_the_switch_of_its_second() {
    let mut on = switched_town(0xff, true);
    on.play(9 * Y);
    let mut off = switched_town(0xff, false);
    off.play(9 * Y);
    let mut forked = switched_town(0xff, true);
    forked.play(9 * Y - 1);
    forked.fork(false);
    forked.play(9 * Y);
    for (run, expected) in [(&on, 1), (&off, 0), (&forked, 0)] {
        let year_9 = run.onsets_at_arrival(|entry| entry["t"] == 9 * Y);
        assert_eq!(year_9, expected);
        run.assert_loads();
    }
}

/// The onsets of "arrival"'s town with the switch off at the start, on from `3 · YEAR`,
/// off from `6 · YEAR`, on from `9 · YEAR` (forks stamped one second before; inputs 2 to 4),
/// to the end of year 13 (ADR-0017 section 3, addendum): a person becomes affected in the run
/// in the first second from `max(B, E)` in which it belongs to the active population and the
/// switch is on, and exactly there stands its entry. So: at most one per person and disease;
/// none while the switch is off; one caused by a fork only in the second the fork acts; and
/// whenever the switch is on, every living person whose card shows a disease as affected
/// has its entry, with the card's year and whole years, and no other.
#[test]
fn a_town_with_forks_reports_each_onset_once_while_the_switch_is_on() {
    let mut run = switched_town(0xff, false);
    let on = |t: i64| (3 * Y..6 * Y).contains(&t) || t >= 9 * Y;
    let affected_have_entries = |run: &Run| {
        let mut entries: BTreeMap<(u64, String), (Option<i64>, Option<i64>)> = BTreeMap::new();
        for entry in &run.log {
            let entry = serde_json::to_value(entry).expect("json");
            let Some(payload) = entry["payload"].get("core:disease_onset") else {
                continue;
            };
            let sim = entry["subjects"][0]["sim"].as_u64().expect("a sim");
            let disease = payload["disease"].as_str().expect("a code").to_owned();
            let once = (
                payload["since_year"].as_i64(),
                payload["age_years"].as_i64(),
            );
            assert!(
                entries.insert((sim, disease), once).is_none(),
                "once: {entry}"
            );
        }
        let pedigree = run.world.pedigree();
        for person in
            (pedigree.persons.iter()).filter(|p| p.died.is_none() && p.emigrated.is_none())
        {
            let card = run.world.genetics_card(person.sim).expect("a card");
            for disease in &card.diseases {
                let key = (person.sim.get(), disease.disease.as_str().to_owned());
                let expected = match disease.status {
                    DiseaseStatus::Affected { year, age_years } => {
                        Some((Some(year), Some(age_years)))
                    }
                    _ => None,
                };
                assert_eq!(entries.get(&key).copied(), expected, "{key:?}");
            }
        }
        entries.len()
    };
    // Checked in the second each fork that turns the switch on acts (the card reads the
    // switch of its second; a lethal onset may soon kill), before it goes off, and at the end.
    run.play(3 * Y - 1);
    run.fork(true);
    run.play(3 * Y);
    assert!(affected_have_entries(&run) > 0, "the town has onsets");
    run.play(6 * Y - 1);
    affected_have_entries(&run);
    run.fork(false);
    run.play(9 * Y - 1);
    run.fork(true);
    run.play(9 * Y);
    affected_have_entries(&run);
    run.play(14 * Y - 1);
    affected_have_entries(&run);
    for entry in &run.log {
        let entry = serde_json::to_value(entry).expect("json");
        if entry["payload"].get("core:disease_onset").is_none() {
            continue;
        }
        let t = entry["t"].as_i64().expect("a second");
        assert!(on(t), "{entry}");
        if let Some(input) = entry["causes"][0]["input"].as_u64() {
            assert!(
                (t, input) == (3 * Y, 2) || (t, input) == (9 * Y, 4),
                "{entry}"
            );
        }
    }
}

// ---- generated by gen/arrival_pins.py from the reference's output ----
const TEMPLATE_DATA_HASH: u64 = 0x5ae3_d70c_71cf_063a;
const FATHER_DIES_DATA_HASH: u64 = 0xe2f2_44a7_e4cd_db5f;
/// (pool, data hash with it, pool hash, households, members)
const DATA_WITH_POOL: [(&str, u64, u64, usize, usize); 6] = [
    (
        "pool_a",
        0x731e_75c8_2188_deeb,
        0x9f75_0816_2a92_48c2,
        26,
        42,
    ),
    (
        "pool_b",
        0x6bc8_a5d6_8de3_6a5e,
        0x8fbc_3d92_5963_36b6,
        29,
        33,
    ),
    (
        "pool_c",
        0xc7fb_d166_8c8e_94f6,
        0xec01_a0ff_0726_0ad1,
        43,
        75,
    ),
    (
        "pool_d",
        0xeccf_f256_9ac1_2bb4,
        0xfd94_6c4f_2d00_c4cb,
        26,
        42,
    ),
    (
        "pool_e",
        0x6e82_ad19_0092_0c19,
        0x8ffd_f50f_324b_f994,
        20,
        35,
    ),
    (
        "pool_f",
        0x00eb_2d7b_d5e5_440c,
        0x44ee_c9ee_32f4_feb0,
        17,
        31,
    ),
];
/// (case, reject code, data hash, the input record as (length, xxh3))
const START_REJECTED: [(&str, u16, u64, (usize, u64)); 17] = [
    (
        "founders_with_pool",
        24,
        0xc7fb_d166_8c8e_94f6,
        (6740, 0x5a1a_6960_ce73_cd24),
    ),
    (
        "founders_and_couple_with_pool",
        24,
        0xc7fb_d166_8c8e_94f6,
        (6749, 0xc59e_2549_4405_2777),
    ),
    (
        "no_pool_in_the_data",
        25,
        0x5ae3_d70c_71cf_063a,
        (6733, 0x8226_a0ac_c792_bde4),
    ),
    (
        "pool_hash_not_the_data_pool",
        25,
        0xc7fb_d166_8c8e_94f6,
        (6733, 0x176e_9494_70d1_90b8),
    ),
    (
        "pool_of_other_data",
        25,
        0x6bc8_a5d6_8de3_6a5e,
        (6733, 0x8226_a0ac_c792_bde4),
    ),
    (
        "n_start_0_no_households",
        26,
        0xc7fb_d166_8c8e_94f6,
        (38, 0xd327_f157_9768_9820),
    ),
    (
        "n_start_0_with_households",
        26,
        0xc7fb_d166_8c8e_94f6,
        (6733, 0xcc37_5716_4cb1_3ccc),
    ),
    (
        "households_reversed",
        27,
        0xc7fb_d166_8c8e_94f6,
        (6733, 0x3c05_8809_f53b_ac78),
    ),
    (
        "households_one_missing",
        27,
        0xc7fb_d166_8c8e_94f6,
        (6394, 0x665e_a2d0_858d_b35c),
    ),
    (
        "households_one_extra",
        27,
        0xc7fb_d166_8c8e_94f6,
        (6833, 0xb9c0_e17e_dee6_a6c8),
    ),
    (
        "households_of_another_seed",
        27,
        0xc7fb_d166_8c8e_94f6,
        (6470, 0x03ec_4bd4_a91f_d77d),
    ),
    (
        "households_one_altered",
        27,
        0xc7fb_d166_8c8e_94f6,
        (6733, 0xa4dc_5acc_0a5b_ddc2),
    ),
    (
        "order_founders_before_missing_pool",
        24,
        0x5ae3_d70c_71cf_063a,
        (6731, 0x62ec_4f54_b5ea_b01e),
    ),
    (
        "order_missing_pool_before_n_start",
        25,
        0x5ae3_d70c_71cf_063a,
        (6733, 0xcc37_5716_4cb1_3ccc),
    ),
    (
        "order_n_start_before_households",
        26,
        0xc7fb_d166_8c8e_94f6,
        (6733, 0x14e0_c72a_4847_348e),
    ),
    (
        "decision_capacity_zero_before_pool",
        23,
        0xc7fb_d166_8c8e_94f6,
        (6725, 0xc536_6422_7f9e_6486),
    ),
    (
        "decision_couple_checks_before_pool",
        13,
        0xc7fb_d166_8c8e_94f6,
        (6735, 0x53b3_1124_411a_99a0),
    ),
];
const START_ACCEPTED: (usize, u64) = (6733, 0x8226_a0ac_c792_bde4);
/// (case, data check, the pool file as (length, xxh3))
/// The data hash of `pool_c` on its documents with the file's frequencies but
/// `core:disease_ad_late` at the smallest `f64` above 0 and `core:herc2` at the largest
/// below 1 (`data_accept frequency_smallest_above_zero_accepted_twin`).
const FREQUENCY_TWIN_DATA_HASH: u64 = 0x24bc_677e_fded_e48d;
/// The bytes the data hash appends to the definitions' for `pool_c` with the file's
/// frequencies: `(pool_seed, pool_hash, frequencies)` (`data_hash father_dies_with_pool_c`).
const POOL_C_APPENDED: &str = "a2b3c08d01d19598b9f09fe880ec010714636f72653a646973656173655f61645f6c617465691d554d1075ff3e17636f72653a646973656173655f61645f7061727469616cfca9f1d24d62503f0f636f72653a646973656173655f61727b14ae47e17a943f0f636f72653a646973656173655f6d74691d554d10751f3f0f636f72653a646973656173655f7872fca9f1d24d62303f0a636f72653a6865726332000000000000e83f09636f72653a6d633172333333333333c33f";
/// The same for the twin (`data_accept frequency_smallest_above_zero_accepted_twin`).
const FREQUENCY_TWIN_APPENDED: &str = "a2b3c08d01d19598b9f09fe880ec010714636f72653a646973656173655f61645f6c617465010000000000000017636f72653a646973656173655f61645f7061727469616cfca9f1d24d62503f0f636f72653a646973656173655f61727b14ae47e17a943f0f636f72653a646973656173655f6d74691d554d10751f3f0f636f72653a646973656173655f7872fca9f1d24d62303f0a636f72653a6865726332ffffffffffffef3f09636f72653a6d633172333333333333c33f";
const DATA_REJECTED: [(&str, &str, (usize, u64)); 26] = [
    (
        "carer_not_member",
        "pool.carer_not_member",
        (10966, 0x9f84_5cd5_32c9_9ec2),
    ),
    (
        "sex_of_a_member",
        "pool.sex",
        (10967, 0xa79d_439e_9c09_ecfa),
    ),
    (
        "sex_of_an_unborn_child",
        "pool.sex",
        (10967, 0xfdb1_dec9_9605_a14b),
    ),
    (
        "locus_mc1r_missing",
        "pool.locus",
        (10967, 0xec01_a0ff_0726_0ad1),
    ),
    (
        "locus_herc2_missing",
        "pool.locus",
        (10967, 0xec01_a0ff_0726_0ad1),
    ),
    (
        "frozen_hash_pool_c_on_the_template",
        "pool.frozen_hash",
        (10967, 0xec01_a0ff_0726_0ad1),
    ),
    (
        "frozen_hash_pool_d_on_father_dies",
        "pool.frozen_hash",
        (7953, 0xfd94_6c4f_2d00_c4cb),
    ),
    (
        "frozen_hash_changed",
        "pool.frozen_hash",
        (10967, 0xae72_aa6d_71eb_2207),
    ),
    (
        "order_carer_before_sex",
        "pool.carer_not_member",
        (10966, 0x6ab1_0ce8_b785_7d9d),
    ),
    (
        "order_sex_before_locus",
        "pool.sex",
        (10967, 0xa79d_439e_9c09_ecfa),
    ),
    (
        "order_carer_before_locus",
        "pool.carer_not_member",
        (10966, 0x9f84_5cd5_32c9_9ec2),
    ),
    (
        "order_locus_before_frozen_hash",
        "pool.locus",
        (10967, 0xec01_a0ff_0726_0ad1),
    ),
    (
        "order_sex_before_frozen_hash",
        "pool.sex",
        (10967, 0xa79d_439e_9c09_ecfa),
    ),
    (
        "frequencies_none",
        "pool.frequencies_loci",
        (10967, 0xec01_a0ff_0726_0ad1),
    ),
    (
        "frequencies_herc2_missing",
        "pool.frequencies_loci",
        (10967, 0xec01_a0ff_0726_0ad1),
    ),
    (
        "frequencies_disease_mt_missing",
        "pool.frequencies_loci",
        (10967, 0xec01_a0ff_0726_0ad1),
    ),
    (
        "frequencies_one_more",
        "pool.frequencies_loci",
        (10967, 0xec01_a0ff_0726_0ad1),
    ),
    (
        "frequencies_unknown_locus",
        "pool.frequencies_loci",
        (10967, 0xec01_a0ff_0726_0ad1),
    ),
    (
        "frequency_zero",
        "pool.frequency_range",
        (10967, 0xec01_a0ff_0726_0ad1),
    ),
    (
        "frequency_one",
        "pool.frequency_range",
        (10967, 0xec01_a0ff_0726_0ad1),
    ),
    (
        "frequency_negative",
        "pool.frequency_range",
        (10967, 0xec01_a0ff_0726_0ad1),
    ),
    (
        "frequency_above_one",
        "pool.frequency_range",
        (10967, 0xec01_a0ff_0726_0ad1),
    ),
    (
        "order_frozen_hash_before_frequencies_loci",
        "pool.frozen_hash",
        (10967, 0xec01_a0ff_0726_0ad1),
    ),
    (
        "order_frozen_hash_before_frequency_range",
        "pool.frozen_hash",
        (10967, 0xec01_a0ff_0726_0ad1),
    ),
    (
        "order_locus_before_frequencies_loci",
        "pool.locus",
        (10967, 0xec01_a0ff_0726_0ad1),
    ),
    (
        "order_frequencies_loci_before_frequency_range",
        "pool.frequencies_loci",
        (10967, 0xec01_a0ff_0726_0ad1),
    ),
];
const DATA_REJECTED_READER: [(&str, &str); 18] = [
    ("ancestors_out_of_order.bin", "ancestors_out_of_order"),
    ("household_without_adult.bin", "household_without_adult"),
    ("magic.bin", "magic"),
    ("members_out_of_order.bin", "members_out_of_order"),
    ("parent_not_a_record.bin", "parent_not_a_record"),
    ("partnership_not_members.bin", "partnership_not_members"),
    ("trailing_bytes.bin", "trailing_bytes"),
    ("version.bin", "version"),
    ("frozen_at.bin", "frozen_at"),
    ("partnership_sex.bin", "partnership_sex"),
    ("pregnancy_mother.bin", "pregnancy_mother"),
    ("birth_at.bin", "birth_at"),
    ("conception_at.bin", "conception_at"),
    ("conception_month.bin", "conception_month"),
    ("separation_at.bin", "separation_at"),
    ("leave_home_at.bin", "leave_home_at"),
    ("carried_time.bin", "carried_time"),
    ("past_time.bin", "past_time"),
];
const CARRIED_BEFORE_POOL: (usize, u64) = (10966, 0x7101_13a3_823e_4ae0);
const LOAD_BASE_AT: i64 = 94348799;
const LOAD_BASE: (usize, u64) = (21354, 0x79dd_7f60_6e61_16fe);
const LOAD_REJECTED: [Patch; 8] = [
    (
        "arrival_born_off_by_one",
        9936,
        11417,
        "fd",
        (21354, 0x8555_0a03_d3be_cfe8),
    ),
    (
        "used_household_twice",
        13399,
        7922,
        "210e27060903001b0b192a1f071721140d261816231c1a11100f2013251e02050c0e",
        (21355, 0xd328_30c2_6424_7162),
    ),
    (
        "pool_child_arrived_not_born",
        10138,
        11215,
        "82",
        (21354, 0x7342_b1f1_5ca7_ad9a),
    ),
    (
        "arrival_not_at_a_year_start",
        11158,
        10097,
        "fdd3d0ea0bd7eac2caadb9a5bd0700e08380fcf180a0b124e0e9c8808e809b906080c09a8c80c09e0c829580f0a78498ab03a0b28880eeeda190c1018180c481809c8b81600000000000000000200100000001d7eac2caadb9a5bd0701ffcbd1cc0b82",
        (21354, 0x3793_5606_3ae7_1404),
    ),
    (
        "arrival_year_not_its_year",
        11260,
        10093,
        "02",
        (21354, 0xecc2_6a0d_06a6_fa79),
    ),
    (
        "town_pregnancy_without_father",
        12717,
        8635,
        "00",
        (21353, 0xc676_5b4e_f47f_2f5f),
    ),
    (
        "arrival_partnership_formed_off",
        12429,
        8924,
        "fd",
        (21354, 0xd7a5_db82_370d_dd12),
    ),
    (
        "arrival_without_pool",
        10020,
        11314,
        "",
        (21334, 0xbbb6_e2bf_be8b_1398),
    ),
];
// arrival
const ARRIVAL_DATA_HASH: u64 = 0xc7fb_d166_8c8e_94f6;
const ARRIVAL_POOL_HASH: u64 = 0xec01_a0ff_0726_0ad1;
const ARRIVAL_START_INPUT: (usize, u64) = (6733, 0x8226_a0ac_c792_bde4);
const ARRIVAL_YEAR_HASHES: [u64; 14] = [
    0x283f_7e9d_1216_2483,
    0xda32_4a70_cd5a_2886,
    0x54b3_cdf3_036a_22a5,
    0x1937_583e_a8e6_3615,
    0x9268_2f0b_985c_7e50,
    0x25b2_382c_e519_4e2e,
    0xc978_c1d5_1e59_fdaa,
    0xd3ac_1add_53bc_70ce,
    0x6d67_5d4e_8d06_2706,
    0x1fc1_e6a9_326a_ea1d,
    0xec5e_62a9_b6f6_b208,
    0x04ea_b06d_a95b_f4bd,
    0x3342_07df_6a6e_fc27,
    0x0137_8cc9_18e5_b7fd,
];
const ARRIVAL_LOG: (usize, u64) = (1543, 0x0512_a03c_7e1f_9868);
/// The state at the end of year 13, the reference's `arrival_state_hash` (its stop at the
/// immigration of year 14 now runs on a copy of its world, so the row is the state at the
/// end, the projection in its end save).
const ARRIVAL_STATE_HASH: u64 = 0xeb2c_e8c8_b4ee_e711;
const ARRIVAL_END: i64 = 440294399;
const ARRIVAL_SAVE_AFTER_START: (usize, u64) = (17017, 0xfc8d_202d_d73c_180c);
const ARRIVAL_SAVE_END: (usize, u64) = (29399, 0xdfa5_4172_05a3_f8d8);
const ARRIVAL_PEDIGREE: (usize, u64) = (523225, 0xe79d_3289_f510_3846);
const ARRIVAL_USED: [usize; 43] = [
    14, 39, 6, 9, 3, 0, 27, 11, 25, 42, 31, 7, 23, 33, 20, 13, 38, 24, 22, 35, 28, 26, 17, 16, 15,
    32, 19, 37, 30, 2, 5, 12, 41, 1, 36, 8, 18, 21, 40, 10, 4, 34, 29,
];
const ARRIVAL_DRAWS: [(u64, &[usize]); 2] = [
    (
        0,
        &[
            14, 39, 6, 9, 3, 0, 27, 11, 25, 42, 31, 7, 23, 33, 20, 13, 38, 24, 22, 35, 28, 26, 17,
            16, 15, 32, 19, 37, 30, 2, 29, 21, 1, 34, 40, 5, 8, 4, 18, 12, 41, 36, 10,
        ],
    ),
    (13, &[29]),
];
/// (mark, second, the save before it, the save after it, its entries as bytes and JSON)
const ARRIVAL_MARKS: [Mark; 6] = [
    (
        "immigration_0",
        0,
        (17017, 0xfc8d_202d_d73c_180c),
        (17187, 0xfeee_e311_0d7e_71e9),
        &[
            (
                "6300010034010101000dfbf7cfcf86f2c9d96a",
                Some(
                    r#"{"id":99,"t":0,"subjects":[{"sim":52}],"causes":[{"plan":{"by":1,"planned_at":0}}],"payload":{"core:arrived":{"lineage_key":"6ab3279069f3fbfb"}}}"#,
                ),
            ),
            (
                "640001003402010100030e636f72653a686f757365686f6c640a1e",
                Some(
                    r#"{"id":100,"t":0,"subjects":[{"sim":52}],"causes":[{"plan":{"by":1,"planned_at":0}},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":30}}}"#,
                ),
            ),
        ],
    ),
    (
        "pool_child_without_father",
        14515200,
        (18317, 0xb063_362f_1fd4_ba50),
        (18537, 0x9fc3_63cf_fc4c_1e3f),
        &[
            (
                "830180f0eb0d02003b002402014400030e636f72653a676573746174696f6e08bec2c4bfb28d9fcca201",
                None,
            ),
            (
                "840180f0eb0d01003b02014400030e636f72653a686f757365686f6c640b1f",
                Some(
                    r#"{"id":132,"t":14515200,"subjects":[{"sim":59}],"causes":[{"plan":{"by":68,"planned_at":0}},{"rule":"core:household"}],"payload":{"core:moved":{"household":31}}}"#,
                ),
            ),
            (
                "850180f0eb0d010027010149000201",
                Some(
                    r#"{"id":133,"t":14515200,"subjects":[{"sim":39}],"causes":[{"plan":{"by":73,"planned_at":0}}],"payload":{"core:birthday":{"age_years":1}}}"#,
                ),
            ),
            (
                "860180f0eb0d01002801014a000201",
                Some(
                    r#"{"id":134,"t":14515200,"subjects":[{"sim":40}],"causes":[{"plan":{"by":74,"planned_at":0}}],"payload":{"core:birthday":{"age_years":1}}}"#,
                ),
            ),
        ],
    ),
    (
        "town_child_of_pool_persons",
        26611200,
        (18825, 0xf741_f7b6_4c57_3d36),
        (19030, 0x0ae7_95d1_1412_d0a9),
        &[
            (
                "b70180b8b01903003c000d000c02016780a8a702030e636f72653a676573746174696f6e08b49498f0bdf2d0a517",
                None,
            ),
            (
                "b80180b8b01901003c02016780a8a702030e636f72653a686f757365686f6c640b0a",
                Some(
                    r#"{"id":184,"t":26611200,"subjects":[{"sim":60}],"causes":[{"plan":{"by":103,"planned_at":2419200}},{"rule":"core:household"}],"payload":{"core:moved":{"household":10}}}"#,
                ),
            ),
            (
                "b90180b8b01903003d000d000c02016780a8a702030e636f72653a676573746174696f6e08c4e8be9380abdda926",
                None,
            ),
            (
                "ba0180b8b01901003d02016780a8a702030e636f72653a686f757365686f6c640b0a",
                Some(
                    r#"{"id":186,"t":26611200,"subjects":[{"sim":61}],"causes":[{"plan":{"by":103,"planned_at":2419200}},{"rule":"core:household"}],"payload":{"core:moved":{"household":10}}}"#,
                ),
            ),
        ],
    ),
    (
        "immigration_1",
        31449600,
        (18976, 0xb75d_38fc_2706_195e),
        (19163, 0x0f8c_e83b_726b_8f6e),
        &[
            (
                "bc018088ff1d01003e010101000dd7eac2caadb9a5bd07",
                Some(
                    r#"{"id":188,"t":31449600,"subjects":[{"sim":62}],"causes":[{"plan":{"by":1,"planned_at":0}}],"payload":{"core:arrived":{"lineage_key":"077a95cad950b557"}}}"#,
                ),
            ),
            (
                "bd018088ff1d01003e02010100030e636f72653a686f757365686f6c640a20",
                Some(
                    r#"{"id":189,"t":31449600,"subjects":[{"sim":62}],"causes":[{"plan":{"by":1,"planned_at":0}},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":32}}}"#,
                ),
            ),
            (
                "be018088ff1d010016010129000201",
                Some(
                    r#"{"id":190,"t":31449600,"subjects":[{"sim":22}],"causes":[{"plan":{"by":41,"planned_at":0}}],"payload":{"core:birthday":{"age_years":1}}}"#,
                ),
            ),
        ],
    ),
    (
        "carried_separation",
        84829122,
        (21287, 0x73c9_911a_1545_6182),
        (21267, 0x9a2f_2449_0c1f_de59),
        &[
            (
                "ee02848ff35002003300320201600004128ec1c0dde3ddd8da940100e48ed2e6ecad8cc8a501090f",
                Some(
                    r#"{"id":366,"t":84829122,"subjects":[{"sim":51},{"sim":50}],"causes":[{"plan":{"by":96,"planned_at":0}},{"random":{"tag":"0000000000000012","subject":"94b562ee3bb0208e","occasion":"0000000000000000","detail":"a590316eccd48764"}}],"payload":{"core:separated":{"partnership":15}}}"#,
                ),
            ),
            (
                "ef02848ff3500100320200ee02030e636f72653a686f757365686f6c640a23",
                Some(
                    r#"{"id":367,"t":84829122,"subjects":[{"sim":50}],"causes":[{"event":366},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":35}}}"#,
                ),
            ),
            (
                "f002848ff3500100320200ee02030e636f72653a686f757365686f6c640b23",
                Some(
                    r#"{"id":368,"t":84829122,"subjects":[{"sim":50}],"causes":[{"event":366},{"rule":"core:household"}],"payload":{"core:moved":{"household":35}}}"#,
                ),
            ),
        ],
    ),
    (
        "immigrant_death",
        183152461,
        (25157, 0x1ce7_2af9_a19e_7202),
        (25106, 0x7afc_b133_fc15_7007),
        &[(
            "b0059abdd5ae0101003e0201bc018088ff1d0404d7eac2caadb9a5bd07a0bb8ca1c58498a01c01040f636f72653a73656e657363656e6365",
            Some(
                r#"{"id":688,"t":183152461,"subjects":[{"sim":62}],"causes":[{"plan":{"by":188,"planned_at":31449600}},{"random":{"tag":"0000000000000004","subject":"077a95cad950b557","occasion":"1c40602454231da0","detail":"0000000000000001"}}],"payload":{"core:death":{"cause":"core:senescence"}}}"#,
            ),
        )],
    ),
];
const ARRIVAL_PHI: [(&str, u64, u64, f64); 13] = [
    ("pool_child_53_mother", 53, 51, 0.25),
    ("pool_child_53_father", 53, 50, 0.25),
    ("pool_child_54_mother", 54, 9, 0.25),
    ("pool_child_54_father", 54, 8, 0.25),
    ("pool_child_55_mother", 55, 15, 0.25),
    ("pool_child_55_father", 55, 14, 0.25),
    ("pool_child_56_mother", 56, 15, 0.25),
    ("pool_child_56_father", 56, 14, 0.25),
    ("pool_child_57_mother", 57, 27, 0.25),
    ("pool_child_57_father", 57, 26, 0.25),
    ("pool_child_58_mother", 58, 35, 0.25),
    ("pool_child_58_father", 58, 34, 0.25),
    ("pool_child_59_mother", 59, 36, 0.25),
];
const ARRIVAL_FIRST_ENTRIES: [Entry; 6] = [
    ("0100000102010000", None),
    (
        "02000100010102010dd1cae8a18bdfb0efb601",
        Some(
            r#"{"id":2,"t":0,"subjects":[{"sim":1}],"causes":[{"input":1}],"payload":{"core:arrived":{"lineage_key":"b6dec2f8b43a2551"}}}"#,
        ),
    ),
    (
        "0300010001020201030e636f72653a686f757365686f6c640a01",
        Some(
            r#"{"id":3,"t":0,"subjects":[{"sim":1}],"causes":[{"input":1},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":1}}}"#,
        ),
    ),
    (
        "04000100020102010dbfdffacfbaeef39666",
        Some(
            r#"{"id":4,"t":0,"subjects":[{"sim":2}],"causes":[{"input":1}],"payload":{"core:arrived":{"lineage_key":"662dcf73a9feafbf"}}}"#,
        ),
    ),
    (
        "05000100030102010dddc8dcff8fe384f0e301",
        Some(
            r#"{"id":5,"t":0,"subjects":[{"sim":3}],"causes":[{"input":1}],"payload":{"core:arrived":{"lineage_key":"e3e01318fff7245d"}}}"#,
        ),
    ),
    (
        "06000200020003020201030e636f72653a686f757365686f6c640a02",
        Some(
            r#"{"id":6,"t":0,"subjects":[{"sim":2},{"sim":3}],"causes":[{"input":1},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":2}}}"#,
        ),
    ),
];
// orphans
const ORPHANS_DATA_HASH: u64 = 0xeccf_f256_9ac1_2bb4;
const ORPHANS_POOL_HASH: u64 = 0xfd94_6c4f_2d00_c4cb;
const ORPHANS_START_INPUT: (usize, u64) = (5023, 0x09e8_b10e_a953_198b);
const ORPHANS_YEAR_HASHES: [u64; 5] = [
    0xf7db_39a7_5cd0_c7d2,
    0xe207_e3ad_92ea_fdb9,
    0xd8e6_8b34_07d3_947d,
    0xf665_1314_014c_2dbf,
    0x8ba9_576d_21e1_a0e9,
];
const ORPHANS_LOG: (usize, u64) = (303, 0xbf4f_b14f_3c0c_19af);
const ORPHANS_STATE_HASH: u64 = 0xaeef_73d2_31f6_ddee;
const ORPHANS_END: i64 = 157247999;
const ORPHANS_SAVE_AFTER_START: (usize, u64) = (13871, 0xa708_2dc2_573f_0bda);
const ORPHANS_SAVE_END: (usize, u64) = (14595, 0x79e0_226e_b69a_cf52);
const ORPHANS_PEDIGREE: (usize, u64) = (175594, 0x1584_c3bd_b665_7953);
const ORPHANS_USED: [usize; 19] = [
    2, 16, 5, 12, 6, 4, 14, 25, 1, 17, 15, 3, 18, 20, 7, 24, 9, 21, 23,
];
const ORPHANS_DRAWS: [(u64, &[usize]); 1] = [(
    0,
    &[
        2, 16, 5, 12, 6, 4, 14, 25, 1, 17, 15, 3, 18, 20, 7, 24, 9, 21, 23, 8, 10, 11, 13, 0, 19,
        22,
    ],
)];
/// (mark, second, the save before it, the save after it, its entries as bytes and JSON)
const ORPHANS_MARKS: [Mark; 3] = [
    (
        "succession_first",
        17104015,
        (13983, 0xa996_7295_0dc3_2a53),
        (13842, 0x2052_1e65_171a_b443),
        &[(
            "82019ef2a71001001e02015e000404d5b5dca580aceb9b26db86dcd7e8e9bbfac90100040f636f72653a646973656173655f7872",
            Some(
                r#"{"id":130,"t":17104015,"subjects":[{"sim":30}],"causes":[{"plan":{"by":94,"planned_at":0}},{"random":{"tag":"0000000000000004","subject":"2637ad6004b71ad5","occasion":"c9f4ef4e8af7035b","detail":"0000000000000000"}}],"payload":{"core:death":{"cause":"core:disease_xr"}}}"#,
            ),
        )],
    ),
    (
        "succession_second",
        33206750,
        (14250, 0x6967_239c_e986_6f6c),
        (14155, 0xa939_0640_6af8_c42f),
        &[(
            "8f01bcc7d51f01001602013f000404d7b09bb8acdba78d799aada6adcbb284bf7200040f636f72653a646973656173655f6172",
            Some(
                r#"{"id":143,"t":33206750,"subjects":[{"sim":22}],"causes":[{"plan":{"by":63,"planned_at":0}},{"random":{"tag":"0000000000000004","subject":"791a9edac706d857","occasion":"727e1194b5a9969a","detail":"0000000000000000"}}],"payload":{"core:death":{"cause":"core:disease_ar"}}}"#,
            ),
        )],
    ),
    (
        "orphan_rule",
        119487822,
        (14662, 0x6d07_e405_bb43_45f6),
        (14537, 0xfd63_7207_70e9_4e76),
        &[
            (
                "83029cf5f97101001502013c000404c98382daf5bdb0b3d001a0bb8ca1c58498a01c00040f636f72653a73656e657363656e6365",
                Some(
                    r#"{"id":259,"t":119487822,"subjects":[{"sim":21}],"causes":[{"plan":{"by":60,"planned_at":0}},{"random":{"tag":"0000000000000004","subject":"d066c1ef5b4081c9","occasion":"1c40602454231da0","detail":"0000000000000000"}}],"payload":{"core:death":{"cause":"core:senescence"}}}"#,
                ),
            ),
            (
                "84029cf5f97101001802008302030e636f72653a686f757365686f6c640b04",
                Some(
                    r#"{"id":260,"t":119487822,"subjects":[{"sim":24}],"causes":[{"event":259},{"rule":"core:household"}],"payload":{"core:moved":{"household":4}}}"#,
                ),
            ),
        ],
    ),
];
const ORPHANS_PHI: [(&str, u64, u64, f64); 1] = [("orphan_and_carer", 24, 5, 0.125)];
const ORPHANS_FIRST_ENTRIES: [Entry; 6] = [
    ("0100000102010000", None),
    (
        "02000100010102010db7c4dfa691a8bd929501",
        Some(
            r#"{"id":2,"t":0,"subjects":[{"sim":1}],"causes":[{"input":1}],"payload":{"core:arrived":{"lineage_key":"9524f54114d7e237"}}}"#,
        ),
    ),
    (
        "03000100010100020e0f636f72653a646973656173655f6172b30100",
        Some(
            r#"{"id":3,"t":0,"subjects":[{"sim":1}],"causes":[{"event":2}],"payload":{"core:disease_onset":{"disease":"core:disease_ar","since_year":-90,"age_years":0}}}"#,
        ),
    ),
    (
        "04000100010100020e17636f72653a646973656173655f61645f7061727469616c6150",
        Some(
            r#"{"id":4,"t":0,"subjects":[{"sim":1}],"causes":[{"event":2}],"payload":{"core:disease_onset":{"disease":"core:disease_ad_partial","since_year":-49,"age_years":40}}}"#,
        ),
    ),
    (
        "0500010001020201030e636f72653a686f757365686f6c640a01",
        Some(
            r#"{"id":5,"t":0,"subjects":[{"sim":1}],"causes":[{"input":1},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":1}}}"#,
        ),
    ),
    (
        "06000100020102010de7d0f8aac990d1c873",
        Some(
            r#"{"id":6,"t":0,"subjects":[{"sim":2}],"causes":[{"input":1}],"payload":{"core:arrived":{"lineage_key":"73914484955e2867"}}}"#,
        ),
    ),
];
// records
const RECORDS_DATA_HASH: u64 = 0xeccf_f256_9ac1_2bb4;
const RECORDS_POOL_HASH: u64 = 0xfd94_6c4f_2d00_c4cb;
const RECORDS_START_INPUT: (usize, u64) = (4533, 0x4748_a2ec_89af_7c85);
const RECORDS_YEAR_HASHES: [u64; 19] = [
    0xbc47_74e2_e04a_30b7,
    0x4008_57ca_11a8_06aa,
    0x94cd_0303_4ece_2a72,
    0xf939_c9e5_eb8f_4768,
    0x8f8b_96b8_5ef6_eea3,
    0x5794_5a14_d853_770a,
    0xf23a_4cd7_25db_a45b,
    0x1abc_4549_a8c0_6af9,
    0x3824_d067_1a28_dd1e,
    0xac66_9c85_afe4_29df,
    0x15e4_6121_ddf5_d9ff,
    0x631b_8531_2f64_51d4,
    0x807b_c2a0_95f0_6738,
    0x51bf_93b5_1d34_b9e9,
    0x5277_c67b_7fa6_1e36,
    0xaf66_fec4_0b9f_369f,
    0x2a41_fdf0_7860_708f,
    0x7a4a_9388_7f70_018f,
    0xd8a6_e73d_3ca3_c18d,
];
const RECORDS_LOG: (usize, u64) = (640, 0xdcc6_b7b3_420b_aa4e);
const RECORDS_STATE_HASH: u64 = 0x963f_629b_642b_ad91;
const RECORDS_END: i64 = 597542399;
const RECORDS_SAVE_AFTER_START: (usize, u64) = (12835, 0x13d0_6937_e0f8_4cdd);
const RECORDS_SAVE_END: (usize, u64) = (13978, 0x8775_f795_3888_d90c);
const RECORDS_PEDIGREE: (usize, u64) = (184813, 0x6029_5479_3ded_9589);
const RECORDS_USED: [usize; 19] = [
    16, 23, 20, 13, 18, 14, 4, 1, 12, 2, 5, 25, 6, 8, 22, 9, 3, 7, 11,
];
const RECORDS_DRAWS: [(u64, &[usize]); 1] = [(
    0,
    &[
        16, 23, 20, 13, 18, 14, 4, 1, 12, 2, 5, 25, 6, 8, 22, 9, 3, 7, 11, 17, 24, 19, 10, 0, 21,
        15,
    ],
)];
/// (mark, second, the save before it, the save after it, its entries as bytes and JSON)
const RECORDS_MARKS: [Mark; 2] = [
    (
        "orphan_rule",
        48234390,
        (13110, 0x6bc1_8977_7513_e068),
        (13049, 0x3818_0628_0585_dd3d),
        &[
            (
                "9d01acfeff2d01000a02011e000404d897c7d8d99df5d059a0bb8ca1c58498a01c00040f636f72653a73656e657363656e6365",
                Some(
                    r#"{"id":157,"t":48234390,"subjects":[{"sim":10}],"causes":[{"plan":{"by":30,"planned_at":0}},{"random":{"tag":"0000000000000004","subject":"59a1d4ed9b11cbd8","occasion":"1c40602454231da0","detail":"0000000000000000"}}],"payload":{"core:death":{"cause":"core:senescence"}}}"#,
                ),
            ),
            (
                "9e01acfeff2d01000b02009d01030e636f72653a686f757365686f6c640b0f",
                Some(
                    r#"{"id":158,"t":48234390,"subjects":[{"sim":11}],"causes":[{"event":157},{"rule":"core:household"}],"payload":{"core:moved":{"household":15}}}"#,
                ),
            ),
        ],
    ),
    (
        "mixed_child",
        595123200,
        (13884, 0x242d_cd4d_4d69_0df9),
        (13988, 0x5181_f9e6_7296_cb7f),
        &[
            (
                "fc0480f0c6b704030026001a001f0201ed0480e0bda004030e636f72653a676573746174696f6e08feffe9a2d9e7fc84d801",
                None,
            ),
            (
                "fd0480f0c6b7040100260201ed0480e0bda004030e636f72653a686f757365686f6c640b1c",
                Some(
                    r#"{"id":637,"t":595123200,"subjects":[{"sim":38}],"causes":[{"plan":{"by":621,"planned_at":570931200}},{"rule":"core:household"}],"payload":{"core:moved":{"household":28}}}"#,
                ),
            ),
            (
                "fe0480f0c6b7040100080101e00480e8c799040220",
                Some(
                    r#"{"id":638,"t":595123200,"subjects":[{"sim":8}],"causes":[{"plan":{"by":608,"planned_at":563673600}}],"payload":{"core:birthday":{"age_years":32}}}"#,
                ),
            ),
            (
                "ff0480f0c6b7040100210101e10480e8c79904020c",
                Some(
                    r#"{"id":639,"t":595123200,"subjects":[{"sim":33}],"causes":[{"plan":{"by":609,"planned_at":563673600}}],"payload":{"core:birthday":{"age_years":12}}}"#,
                ),
            ),
        ],
    ),
];
const RECORDS_PHI: [(&str, u64, u64, f64); 6] = [
    ("orphan_and_relative_only_through_records", 11, 23, 0.125),
    ("arranged_couple", 26, 31, 0.03125),
    ("town_born_grandchild_and_pool_cousin", 31, 1, 0.0625),
    ("mixed_child_and_its_mother", 38, 26, 0.265625),
    ("mixed_child_and_the_pool_cousin", 38, 1, 0.03125),
    ("arrival_self", 1, 1, 0.5),
];
const RECORDS_FIRST_ENTRIES: [Entry; 6] = [
    ("0100000102010000", None),
    (
        "02000100010102010de7d0f8aac990d1c873",
        Some(
            r#"{"id":2,"t":0,"subjects":[{"sim":1}],"causes":[{"input":1}],"payload":{"core:arrived":{"lineage_key":"73914484955e2867"}}}"#,
        ),
    ),
    (
        "0300010001020201030e636f72653a686f757365686f6c640a01",
        Some(
            r#"{"id":3,"t":0,"subjects":[{"sim":1}],"causes":[{"input":1},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":1}}}"#,
        ),
    ),
    (
        "04000100020102010dd5b5dca580aceb9b26",
        Some(
            r#"{"id":4,"t":0,"subjects":[{"sim":2}],"causes":[{"input":1}],"payload":{"core:arrived":{"lineage_key":"2637ad6004b71ad5"}}}"#,
        ),
    ),
    (
        "05000100020100040e0f636f72653a646973656173655f61723b00",
        Some(
            r#"{"id":5,"t":0,"subjects":[{"sim":2}],"causes":[{"event":4}],"payload":{"core:disease_onset":{"disease":"core:disease_ar","since_year":-30,"age_years":0}}}"#,
        ),
    ),
    (
        "06000100020100040e0f636f72653a646973656173655f78723b00",
        Some(
            r#"{"id":6,"t":0,"subjects":[{"sim":2}],"causes":[{"event":4}],"payload":{"core:disease_onset":{"disease":"core:disease_xr","since_year":-30,"age_years":0}}}"#,
        ),
    ),
];
// micro
const MICRO_POOL: (usize, u64) = (10966, 0xfff9_bfca_0d23_8c97);
const RECORDS_ARRANGE: ((usize, u64), (usize, u64), &[Entry]) = (
    (13731, 0xaaff_4450_93c4_8b5b),
    (13806, 0xff54_fd6b_b189_e021),
    &[
        (
            "e2048090ef9b0402001a001f0102020509",
            Some(
                r#"{"id":610,"t":566092800,"subjects":[{"sim":26},{"sim":31}],"causes":[{"input":2}],"payload":{"core:partnered":{"partnership":9}}}"#,
            ),
        ),
        (
            "e3048090ef9b0402001a001f0200e204030e636f72653a686f757365686f6c640a1c",
            Some(
                r#"{"id":611,"t":566092800,"subjects":[{"sim":26},{"sim":31}],"causes":[{"event":610},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":28}}}"#,
            ),
        ),
        (
            "e4048090ef9b0401001a0200e204030e636f72653a686f757365686f6c640b1c",
            Some(
                r#"{"id":612,"t":566092800,"subjects":[{"sim":26}],"causes":[{"event":610},{"rule":"core:household"}],"payload":{"core:moved":{"household":28}}}"#,
            ),
        ),
        (
            "e5048090ef9b0401001f0200e204030e636f72653a686f757365686f6c640b1c",
            Some(
                r#"{"id":613,"t":566092800,"subjects":[{"sim":31}],"causes":[{"event":610},{"rule":"core:household"}],"payload":{"core:moved":{"household":28}}}"#,
            ),
        ),
    ],
);
const EMPTY_DATA_HASH: u64 = 0x6bc8_a5d6_8de3_6a5e;
const EMPTY_START_INPUT: (usize, u64) = (3633, 0x5b72_ca7a_cb20_ef92);
const EMPTY_SAVE_AFTER_START: (usize, u64) = (9382, 0xd346_afd8_2108_ae87);
const EMPTY_USED: [usize; 29] = [
    23, 28, 19, 21, 15, 11, 9, 27, 4, 26, 14, 7, 0, 10, 25, 1, 18, 20, 3, 12, 24, 2, 6, 8, 13, 16,
    17, 5, 22,
];
const EMPTY_DRAWS: [(u64, &[usize]); 1] = [(
    0,
    &[
        23, 28, 19, 21, 15, 11, 9, 27, 4, 26, 14, 7, 0, 10, 25, 1, 18, 20, 3, 12, 24, 2, 6, 8, 13,
        16, 17, 5, 22,
    ],
)];
/// More states the rules cannot reach (review): (case, the save it changes, that save as
/// (length, xxh3), the hunks (offset in that save, bytes removed, bytes put in), the
/// changed save as (length, xxh3)).
const LOAD_REJECTED_MORE: [Hunks; 4] = [
    (
        "carried_father_dropped",
        "arrival_save_after_start",
        (17017, 0xfc8d_202d_d73c_180c),
        &[(9227, 2, "00")],
        (17016, 0x6dc9_cfc3_54aa_0a69),
    ),
    (
        "town_series_missing",
        "records_save_after_arrange",
        (13806, 0xff54_fd6b_b189_e021),
        &[(34, 1, "74"), (103, 15, ""), (8558, 1, "74"), (8595, 7, "")],
        (13784, 0x5731_2b51_5f67_b7d1),
    ),
    (
        "immigration_order_swapped",
        "load_base",
        (21354, 0x79dd_7f60_6e61_16fe),
        &[(13430, 0, "0c"), (13431, 1, "")],
        (21354, 0x5c75_f67e_48c0_f510),
    ),
    (
        "unstarted_with_used_household",
        "unstarted",
        (39, 0xf655_2425_2b8a_f300),
        &[(34, 0, "01")],
        (40, 0x9d38_a9e3_2d29_b415),
    ),
];
const LEAVE_AT_START_POOL: (usize, u64) = (10966, 0x92ea_6325_ee60_8700);
/// The tie of a birth and a pairing of the market in one second (review): "arrival" with the
/// seed 0xe; woman 37 (pool household 28) gives birth at `TIE_AT` and the round of that
/// second pairs her. The seed was 0xef on the template; on the documents of `father_dies`
/// women search more (`c` 0.2), and with 0xef the woman of household 28 is paired in round 1
/// (second 2419200), so the tie is lost there. 0xe is the smallest seed with the tie (the
/// reference's choice; 0x37 has it too).
const TIE_SEED: u64 = 0xe;
const TIE_AT: i64 = 14_515_200;
/// Her first attempt: the first month start after the pairing, not after the pause.
const TIE_FIRST_ATTEMPT: i64 = 16_934_400;
const TIE_START_INPUT: (usize, u64) = (6385, 0xa349_9461_6f64_8fa5);
const TIE_SAVE_AT: (usize, u64) = (17397, 0xe64c_0dd5_f5ee_0cac);
const TIE_SAVE_BEFORE_ATTEMPT: (usize, u64) = (17597, 0x21d8_0330_37b4_f5e1);
/// The save at `TIE_AT` with her series moved to after the pause of the birth.
const TIE_COUNTED_BIRTH: (&[Hunk], (usize, u64)) = (
    &[
        (506, 14, ""),
        (742, 0, "80d0c32900eb0103250180a8a702"),
        (10560, 5, ""),
        (10631, 0, "7d80f0eb0d"),
    ],
    (17397, 0xdaf0_5a28_7245_d2a5),
);

/// The reference's states changed in one field (out4, its notes' section 5b).
const LOAD_REJECTED_FIELDS: [Hunks; 32] = [
    (
        "arrival_arrived_one_second_late",
        "orphans_save_end",
        (14595, 0x79e0_226e_b69a_cf52),
        &[(8736, 1, "02")],
        (14595, 0xc9f2_25a0_4711_11c3),
    ),
    (
        "arrival_year_off_by_one",
        "orphans_save_end",
        (14595, 0x79e0_226e_b69a_cf52),
        &[(8737, 1, "01")],
        (14595, 0x7fc3_f602_0691_6d4f),
    ),
    (
        "arrival_born_pool_changed",
        "orphans_save_end",
        (14595, 0x79e0_226e_b69a_cf52),
        &[(4595, 1, "82")],
        (14595, 0xbdcc_1ef9_3eca_4d4f),
    ),
    (
        "arrival_genome_key_changed",
        "orphans_save_end",
        (14595, 0x79e0_226e_b69a_cf52),
        &[(4585, 1, "c8")],
        (14595, 0x90ce_d60a_ad14_a2b8),
    ),
    (
        "arrival_pool_mother_changed",
        "orphans_save_end",
        (14595, 0x79e0_226e_b69a_cf52),
        &[(4603, 9, "caeea683b29ea549")],
        (14594, 0xbce4_bf9c_4a03_b14d),
    ),
    (
        "arrival_pool_father_changed",
        "orphans_save_end",
        (14595, 0x79e0_226e_b69a_cf52),
        &[(4613, 10, "ddd0b2b8c5fbd04b")],
        (14593, 0x3549_a3c2_9370_e8b3),
    ),
    (
        "arrival_pool_social_parents_changed",
        "orphans_save_end",
        (14595, 0x79e0_226e_b69a_cf52),
        &[(4623, 1, "01caeea683b29ea549")],
        (14603, 0x7054_9fb2_e7e7_417b),
    ),
    (
        "arrival_sex_flipped",
        "orphans_save_end",
        (14595, 0x79e0_226e_b69a_cf52),
        &[(8594, 1, "00")],
        (14595, 0xfb49_f3a8_8c41_fc2e),
    ),
    (
        "arrival_genome_changed",
        "orphans_save_end",
        (14595, 0x79e0_226e_b69a_cf52),
        &[(4467, 1, "9a")],
        (14595, 0x7f1c_309b_5259_a4c8),
    ),
    (
        "arrival_town_parents_set",
        "orphans_save_end",
        (14595, 0x79e0_226e_b69a_cf52),
        &[(4581, 0, "011615")],
        (14598, 0x54de_0501_9448_d56c),
    ),
    (
        "arrival_household_moved_to_another_year",
        "orphans_save_end",
        (14595, 0x79e0_226e_b69a_cf52),
        &[
            (114, 19, ""),
            (642, 0, "92e3d6fd02001502049aada6adcbb284bf7200"),
            (725, 19, ""),
            (762, 20, ""),
            (823, 19, ""),
            (930, 19, ""),
            (1007, 0, "90c4818e0800130204a0bb8ca1c58498a01c00"),
            (1017, 0, "d49ae3bbeacfeb962f00a0a6feca08001b0205"),
            (1066, 0, "96c18d9f0a00160204d49ae3bbeacfeb962f00"),
            (1106, 0, "00c2b1a8c40a00190205a0bb8ca1c58498a01c"),
            (1226, 19, ""),
            (1333, 0, "00f4f5cefe0e001c0205db86dcd7e8e9bbfac901"),
            (1572, 10, ""),
            (1601, 0, "00d0e2fdd51500110501"),
            (1743, 0, "b68caac22100170204db86dcd7e8e9bbfac90100"),
            (1883, 40, ""),
            (2265, 0, "92e5ae99fb0d0014020485d2bc82f8b2ead44d00"),
            (2346, 0, "bad6d3ca8315001a020585d2bc82f8b2ead44d00"),
            (2397, 20, ""),
            (2741, 4, "afc2f80c"),
            (2843, 1, "8088ff1d"),
            (2871, 1, "8088ff1d"),
            (2900, 1, "8088ff1d"),
            (2924, 2, "8088ff1d01"),
            (2950, 4, "c7e0e30c"),
            (3049, 1, "8088ff1d"),
            (3078, 1, "8088ff1d"),
            (3103, 2, "8088ff1d01"),
            (6561, 3, "c785ba"),
            (6666, 1, "8088ff1d"),
            (9041, 2, ""),
            (9217, 0, "0d8088ff1d"),
            (9231, 5, "3a"),
            (9251, 2, ""),
            (
                9267,
                14,
                "33004e0038000d8088ff1d6300118088ff1d63002a000d8088ff1d",
            ),
            (9289, 0, "118088ff1d"),
            (9321, 2, ""),
            (9341, 0, "118088ff1d"),
            (9385, 8, "830180c0ba120d8088ff1d"),
            (9419, 0, "0d8088ff1d"),
            (9438, 4, ""),
            (9491, 0, "0d8088ff1d"),
            (9499, 0, "118088ff1d"),
            (9508, 2, ""),
        ],
        (14649, 0x8a4a_e9b1_685f_97a2),
    ),
    (
        "pool_child_genome_key_changed",
        "arrival_save_after_pool_child_without_father",
        (18537, 0x9fc3_63cf_fc4c_1e3f),
        &[(10409, 1, "bf")],
        (18537, 0xeb7d_2d99_c364_a5d3),
    ),
    (
        "pool_child_arrived_not_born",
        "arrival_save_after_pool_child_without_father",
        (18537, 0x9fc3_63cf_fc4c_1e3f),
        &[(10420, 2, "feef")],
        (18537, 0xb6ec_dd35_63e4_5985),
    ),
    (
        "pool_child_arrival_year_off_by_one",
        "arrival_save_after_pool_child_without_father",
        (18537, 0x9fc3_63cf_fc4c_1e3f),
        &[(10424, 1, "01")],
        (18537, 0xc4fc_84bd_9399_0dee),
    ),
    (
        "pool_child_pool_mother_changed",
        "arrival_save_after_pool_child_without_father",
        (18537, 0x9fc3_63cf_fc4c_1e3f),
        &[(10426, 9, "d7eac2caadb9a5bd07")],
        (18537, 0xc297_801b_63c4_d9a2),
    ),
    (
        "pool_child_pool_father_changed",
        "arrival_save_after_pool_child_without_father",
        (18537, 0x9fc3_63cf_fc4c_1e3f),
        &[(10436, 10, "b9b4d4cad0ccd1b808")],
        (18536, 0x8e16_e17d_9afc_1e8d),
    ),
    (
        "pool_child_social_parents_set",
        "arrival_save_after_pool_child_without_father",
        (18537, 0x9fc3_63cf_fc4c_1e3f),
        &[(10446, 1, "01cd9dd6b0cadf9ece6a")],
        (18546, 0x2a42_e9db_8635_72aa),
    ),
    (
        "pool_child_sex_flipped",
        "arrival_save_after_pool_child_without_father",
        (18537, 0x9fc3_63cf_fc4c_1e3f),
        &[(10307, 1, "01")],
        (18537, 0xb4c1_2ac7_0a15_d123),
    ),
    (
        "pool_child_genome_changed",
        "arrival_save_after_pool_child_without_father",
        (18537, 0x9fc3_63cf_fc4c_1e3f),
        &[(10319, 1, "94")],
        (18537, 0x1ae5_55d8_8c29_c37b),
    ),
    (
        "pool_child_town_parents_set",
        "arrival_save_after_pool_child_without_father",
        (18537, 0x9fc3_63cf_fc4c_1e3f),
        &[(10406, 0, "012401")],
        (18540, 0x37c9_bf03_3d98_fea0),
    ),
    (
        "pool_child_born_after_the_clock",
        "arrival_save_after_pool_child_without_father",
        (18537, 0x9fc3_63cf_fc4c_1e3f),
        &[
            (804, 1, "82"),
            (1142, 1, "a8"),
            (1192, 1, "bc"),
            (1253, 1, "82"),
            (2291, 1, "92"),
            (3268, 1, "d0"),
            (10293, 1, "82"),
            (10395, 1, "82"),
            (10420, 1, "82"),
        ],
        (18537, 0x4ee7_3c41_b230_98b3),
    ),
    (
        "pool_child_mother_not_an_arrival",
        "arrival_save_after_pool_child_without_father",
        (18537, 0x9fc3_63cf_fc4c_1e3f),
        &[(7675, 23, "011f01"), (11146, 1, "1d"), (11167, 1, "")],
        (18516, 0xa5b2_4b49_0913_d260),
    ),
    (
        "pool_child_mother_left_before_the_birth",
        "arrival_save_after_pool_child_without_father",
        (18537, 0x9fc3_63cf_fc4c_1e3f),
        &[
            (34, 1, "e7"),
            (316, 10, ""),
            (760, 14, ""),
            (1677, 20, ""),
            (3699, 24, "ff0139"),
            (7595, 107, ""),
            (10404, 2, "00"),
            (
                10995,
                1,
                "0224ffc3e1ce07feefeb0d0085d2bc82f8b2ead44dcd9dd6b0cadf9ece6a00809380ba8083a8b116a0b98280caab85a0870185c0c98a80c09188c001889b80d8cd81b0d52180f5d580ca848e806980c0fa8280e0a18aa00100000000000101021f01000001cd9dd6b0cadf9ece6a01ffb3e392070000000000",
            ),
            (11177, 1, "e7"),
            (11240, 2, ""),
            (11368, 6, ""),
            (11536, 2, ""),
            (11797, 2, ""),
        ],
        (18472, 0x4208_eb08_4b06_3d5c),
    ),
    (
        "no_pool_living_with_pool_origin",
        "founders_world",
        (10816, 0x1ec2_3efe_6f5b_27fa),
        &[(243, 0, "000000010001ffd7e6a90600")],
        (10828, 0x4ef0_f3cb_daeb_8f5b),
    ),
    (
        "no_pool_archived_with_pool_origin",
        "founders_world",
        (10816, 0x1ec2_3efe_6f5b_27fa),
        &[(348, 0, "0000010001ffbfadcf140000")],
        (10828, 0x25ab_41b3_28b6_3d36),
    ),
    (
        "no_pool_used_household",
        "founders_world",
        (10816, 0x1ec2_3efe_6f5b_27fa),
        &[(400, 0, "01")],
        (10817, 0x26b9_9bdf_6120_19d3),
    ),
    (
        "no_pool_pregnancy_without_father",
        "founders_world",
        (10816, 0x1ec2_3efe_6f5b_27fa),
        &[(272, 2, "00")],
        (10815, 0xf70d_0f06_7c93_b468),
    ),
    (
        "no_pool_pregnancy_with_pool_father",
        "founders_world",
        (10816, 0x1ec2_3efe_6f5b_27fa),
        &[(274, 1, "01d388fe84eef9bce93c")],
        (10825, 0x5337_4e34_9052_8440),
    ),
    (
        "arrival_social_parent_is_its_pool_parent",
        "orphans_save_end",
        (14595, 0x79e0_226e_b69a_cf52),
        &[(4583, 1, "15")],
        (14595, 0xb953_d0b0_8c7b_b494),
    ),
    (
        "arrival_partnership_close_kin",
        "orphans_save_end",
        (14595, 0x79e0_226e_b69a_cf52),
        &[
            (32, 3, "0a1a8b"),
            (1512, 0, "baaea9961300e902050900"),
            (2447, 1, "ea"),
            (2694, 1, "19"),
            (3236, 2, "0219"),
            (6243, 1, "19"),
            (6363, 1, "19"),
            (6556, 1, "09"),
            (6651, 7, "090a0280a8fb9501001901000002000180a8fb9501"),
            (6692, 1, "0180a8fb9501"),
            (6772, 0, "01001980a8fb95"),
            (9001, 1, "8b"),
            (9371, 0, "b00280a8fb9501"),
            (9513, 3, "b60202"),
            (14539, 0, "fea7fb950102020a02"),
        ],
        (14648, 0x5475_a6ba_bc89_b2d4),
    ),
    (
        "arrival_moved_out_by_town_partnership_flag_false",
        "arrival_moved_out_by_town_partnership",
        (13199, 0x1016_7f39_3fd1_1801),
        &[(3748, 1, "00")],
        (13199, 0x733c_4103_25b1_aaf3),
    ),
    (
        "brought_partnership_member_not_moved_out_moved_out",
        "brought_partnership_member_not_moved_out",
        (17033, 0x4734_b885_bcae_6f32),
        &[(3402, 1, "01")],
        (17033, 0x8b9f_b8ce_e2b9_820b),
    ),
];

/// The town of four founders without a pool (`founders_world`).
const FOUNDERS_AT: i64 = 16934400;
const FOUNDERS_START_INPUT: (usize, u64) = (10388, 0xfdfd_d8cc_b116_f7a6);
const FOUNDERS_DATA_HASH: u64 = 0x5ae3_d70c_71cf_063a;

/// The second of the mark "pool_child_without_father".
const POOL_CHILD_WITHOUT_FATHER_AT: i64 = 14515200;

/// "records" before its input: the market round of this second pairs arrival 7 (not moved
/// out in the pool, his leaving home still to come) with sim 11, which alone moves him out.
const RECORDS_MOVED_OUT_BY_TOWN_PARTNERSHIP_AT: i64 = 186278400;

/// The pool with a brought partnership's member not moved out, and its data.
const BROUGHT_NOT_MOVED_OUT_POOL: (usize, u64) = (10971, 0xe736_08c7_3426_0fd4);
const BROUGHT_NOT_MOVED_OUT_DATA_HASH: u64 = 0xb2d1_3c57_34cb_080a;

/// The reference's states of the carried plans (out6, its notes' section 5d). On the
/// documents of `father_dies`, `pool_birth_vanished` also moves the pending attempt of the
/// mother (sim 36, paired in the town) to the town's rule without the birth, so that only
/// the check of the carried births rejects it.
const LOAD_REJECTED_CARRIED: [Hunks; 17] = [
    (
        "carried_series_one_month_late",
        "arrival_save_after_immigration_1",
        (19163, 0x0f8c_e83b_726b_8f6e),
        &[
            (71, 13, ""),
            (169, 0, "0180a8a70280d8cd22004c0315"),
            (11561, 2, ""),
            (11595, 0, "2800"),
        ],
        (19163, 0xaaef_2c9a_0243_4e06),
    ),
    (
        "carried_series_before_its_start",
        "arrival_save_after_start",
        (17017, 0xfc8d_202d_d73c_180c),
        &[
            (43, 0, "00000a03030180a8a702"),
            (60, 13, ""),
            (9875, 5, "0500600001"),
        ],
        (17014, 0x2998_3940_1f1a_6e22),
    ),
    (
        "carried_series_missing",
        "arrival_save_after_immigration_1",
        (19163, 0x0f8c_e83b_726b_8f6e),
        &[
            (34, 1, "f1"),
            (71, 13, ""),
            (11550, 1, "f1"),
            (11560, 2, ""),
        ],
        (19148, 0x67bb_424a_65fb_09ba),
    ),
    (
        "carried_series_past_fertility",
        "V3_carried_series_ended",
        (22764, 0x4513_b2b3_8495_46e0),
        &[
            (34, 1, "b9"),
            (565, 0, "0180a8a70280b88f8801009b010328"),
            (14881, 1, "b9"),
            (15166, 0, "4900"),
        ],
        (22781, 0x13cb_1b4f_bfb7_d7ad),
    ),
    (
        "carried_conception_before_its_series",
        "V4b_carried_conception_pregnant",
        (18821, 0xf965_2da3_a491_126c),
        &[
            (330, 0, "ab02042d0080d0c32900"),
            (358, 10, ""),
            (11096, 3, "c0ba12"),
            (11376, 0, "b30180c0ba12"),
            (11393, 6, ""),
        ],
        (18821, 0x08ce_3a55_6e38_d667),
    ),
    (
        "pool_pregnancy_dropped",
        "arrival_save_after_start",
        (17017, 0xfc8d_202d_d73c_180c),
        &[
            (31, 1, "cb"),
            (199, 10, ""),
            (9225, 1, "05"),
            (9661, 89, ""),
            (9871, 1, "cb"),
            (9902, 2, ""),
        ],
        (16916, 0xd6cf_681d_08ab_1c7f),
    ),
    (
        "pool_birth_vanished",
        "arrival_save_after_pool_child_without_father",
        (18537, 0x9fc3_63cf_fc4c_1e3f),
        &[
            (31, 4, "3b1120e5"),
            (473, 0, "98931000fc0103240180a8a70280"),
            (765, 14, ""),
            (803, 10, ""),
            (1142, 20, ""),
            (1191, 20, ""),
            (1252, 11, ""),
            (2291, 20, ""),
            (3268, 21, ""),
            (3722, 1, "39"),
            (7674, 1, "00"),
            (10292, 155, ""),
            (11177, 1, "e5"),
            (11271, 0, "830180f0eb0d"),
            (11368, 6, ""),
            (11386, 6, ""),
            (11454, 16, "4a000c00"),
            (11479, 6, ""),
            (11610, 6, ""),
            (11749, 6, ""),
        ],
        (18244, 0x8fea_751a_852b_ea0a),
    ),
    (
        "pool_child_born_one_second_early",
        "arrival_save_after_pool_child_without_father",
        (18537, 0x9fc3_63cf_fc4c_1e3f),
        &[
            (784, 0, "fef7ea2b00f701013b00"),
            (803, 10, ""),
            (1142, 1, "a4"),
            (1192, 1, "b8"),
            (1253, 2, "fe8f"),
            (2291, 1, "8e"),
            (3268, 1, "cc"),
            (10293, 2, "feef"),
            (10395, 2, "feef"),
            (10420, 2, "feef"),
            (11379, 13, "830180f0eb0d850180f0eb0d86"),
        ],
        (18537, 0x719a_e3cc_679c_c489),
    ),
    (
        "pool_twin_missing",
        "V5_pool_twins_born",
        (17821, 0xacbd_6d17_c04b_d765),
        &[
            (31, 4, "381120d7"),
            (766, 10, ""),
            (1071, 11, ""),
            (2001, 20, ""),
            (3015, 21, ""),
            (3448, 1, "36"),
            (5060, 1, "01"),
            (9613, 125, ""),
            (10576, 1, "d7"),
            (10747, 5, ""),
            (10797, 5, ""),
            (10907, 5, ""),
            (11040, 5, ""),
        ],
        (17614, 0xe1c0_9498_c73e_f98a),
    ),
    (
        "carried_attempt_plan_moved",
        "arrival_save_after_start",
        (17017, 0xfc8d_202d_d73c_180c),
        &[(9880, 1, "01")],
        (17017, 0xad89_b9f4_5c5e_4a7d),
    ),
    (
        "carried_birth_plan_moved",
        "arrival_save_after_start",
        (17017, 0xfc8d_202d_d73c_180c),
        &[(9904, 1, "01")],
        (17017, 0x6dd9_e4ce_79bb_a439),
    ),
    (
        "carried_separation_plan_moved",
        "arrival_save_after_start",
        (17017, 0xfc8d_202d_d73c_180c),
        &[(10014, 1, "01")],
        (17017, 0x17d2_6375_72f0_b56b),
    ),
    (
        "carried_leave_home_plan_moved",
        "arrival_save_after_start",
        (17017, 0xfc8d_202d_d73c_180c),
        &[(10044, 1, "01")],
        (17017, 0xc942_ad7e_18e4_3578),
    ),
    (
        "carried_plan_planned_later",
        "arrival_save_end",
        (29399, 0xdfa5_4172_05a3_f8d8),
        &[(22006, 1, "82")],
        (29399, 0x5d5b_5e25_f096_c469),
    ),
    (
        "carried_plan_by_other_member",
        "arrival_save_after_start",
        (17017, 0xfc8d_202d_d73c_180c),
        &[(10091, 1, "24")],
        (17017, 0xd816_d901_399f_32e2),
    ),
    (
        "carried_plans_swapped",
        "arrival_save_after_start",
        (17017, 0xfc8d_202d_d73c_180c),
        &[(70, 1, "0b"), (2090, 1, "0a")],
        (17017, 0xb22a_c83f_7fbf_e2f6),
    ),
    (
        "pool_child_of_a_cancelled_pregnancy",
        "V6_pool_pregnancy_cancelled",
        (12998, 0xdbd8_e105_1d5f_ff7a),
        &[
            (31, 4, "200613a3"),
            (373, 0, "0090bc8d2100ad01011f"),
            (905, 0, "90c4fd9e0400b301071f00"),
            (
                1426,
                0,
                "9cac9af90a00ac010a1ff3e1e3d4baa3e9c22c00a6caa9850b00ab010a1fd49ae3bbeacfeb962f00",
            ),
            (
                1445,
                0,
                "00beb187f90b00b001021fd49ae3bbeacfeb962f00e4f3ffb90c00b101021ff3e1e3d4baa3e9c22c",
            ),
            (1694, 0, "00badec6ae1300ae01021fa0bb8ca1c58498a01c"),
            (2092, 0, "eed9a382930100b201021fdb86dcd7e8e9bbfac90100"),
            (2477, 0, "ccabc0c9e00c00af01021f85d2bc82f8b2ead44d00"),
            (2640, 3, "b4011d"),
            (
                7398,
                0,
                "1f90b48e03cebbc0d298e4dfdcb80101859680d5a184f0dd2f80c6d3808e8eb480da018780a38e80dcea0982858086de8290f929c0e69e80d88fafc02f81c0bb9780a4ca89e0010101000003d49ae3bbeacfeb962f0595c306608247400100000000000000000190b48e03f3e1e3d4baa3e9c22c27e19796a74e47400100000000000000000190b48e03db86dcd7e8e9bbfac90100000000000000000100000000000000000190b48e030000000f00011a000001cebbc0d298e4dfdcb8010090b48e0300018c8dc8be98e1f8837e01b4b68b96ebf1fbe1a60100",
            ),
            (7861, 1, "01"),
            (7925, 1, "a3"),
            (7996, 0, "6890b48e03"),
            (8058, 0, "6890b48e03"),
            (8114, 2, "6890b48e036890b48e0324006890b48e036890b48e03"),
            (8142, 0, "6890b48e03"),
            (8184, 0, "6890b48e03"),
            (8222, 0, "6890b48e03"),
            (8238, 1, "6b"),
        ],
        (13425, 0xca7c_c762_12c6_3979),
    ),
];

/// "arrival" at the first month start: woman 13 conceived in her carried series.
const MONTH_1: i64 = 2_419_200;
/// "arrival" when woman 45 conceived at the start of her carried series.
const V4B_AT: i64 = 21_772_800;
/// "arrival" when the pool twins of household 31 are born.
const V5_TWINS_AT: i64 = 7_257_600;

const V3_SAVE: (usize, u64) = (22764, 0x4513_b2b3_8495_46e0);
const V4_SAVE: (usize, u64) = (17468, 0xceb5_709b_41d3_ee85);
const V4B_SAVE: (usize, u64) = (18821, 0xf965_2da3_a491_126c);
const V5_TWINS_SAVE: (usize, u64) = (17821, 0xacbd_6d17_c04b_d765);
const V6_SAVE: (usize, u64) = (12998, 0xdbd8_e105_1d5f_ff7a);

/// The reference's round 9 (out9, its notes' section 5g).
const LOAD_REJECTED_ROUND_9: [Hunks; 5] = [
    (
        "immigration_period_not_a_year",
        "load_base",
        (21354, 0x79dd_7f60_6e61_16fe),
        &[(45, 1, "82")],
        (21354, 0x970b_87d1_2ad3_1659),
    ),
    (
        "immigration_not_due",
        "load_base",
        (21354, 0x79dd_7f60_6e61_16fe),
        &[
            (36, 13, ""),
            (878, 0, "018088ff1d80a0fc7700030901"),
            (13436, 2, ""),
            (13911, 0, "0100"),
        ],
        (21354, 0x9bd6_31d9_bc7d_a9a1),
    ),
    (
        "immigration_persons_off_by_one",
        "load_base",
        (21354, 0x79dd_7f60_6e61_16fe),
        &[(43, 1, "02")],
        (21354, 0x2413_0bf1_fb21_d2c7),
    ),
    (
        "pool_mother_conceptions_off_by_one",
        "arrival_save_after_start",
        (17017, 0xfc8d_202d_d73c_180c),
        &[(7152, 1, "02")],
        (17017, 0x947c_192f_3008_b57e),
    ),
    (
        "series_end_boundary_attempt_present",
        "series_end_boundary",
        (17452, 0x7334_6e41_0d25_d5a5),
        &[
            (34, 1, "d3"),
            (89, 0, "80d0ce040067031c0180a8a702"),
            (10274, 7, "d30146000100050036"),
        ],
        (17467, 0xe712_32ed_1319_a30d),
    ),
];

const SERIES_END_POOL: (usize, u64) = (10967, 0x686e_7564_7811_8471);
const SERIES_END_SAVE: (usize, u64) = (17452, 0x7334_6e41_0d25_d5a5);
const IMMIGRANT_CHILD_YEAR_HASHES: [u64; 2] = [0xe528_9d33_e618_7e96, 0xa520_de3c_0ce4_c5f3];
const IMMIGRANT_CHILD_LOG_HASH: u64 = 0x4752_8549_66b1_5afd;
const IMMIGRANT_CHILD_STATE_HASH: u64 = 0x37bb_cab3_14a4_72e2;
const IMMIGRANT_CHILD_SAVE: (usize, u64) = (20669, 0x28df_5379_cc0b_a775);

/// The loader's household check: the living who moved out share a household only as the
/// two partners of an open partnership. The reference (`tools/village_reference.py
/// --moved-out`) moves sim 4, alone and moved out, into household 1 of another lone sim and
/// ends its own household at the clock.
const LOAD_REJECTED_MOVED_IN: [Hunks; 1] = [(
    "load_base_moved_in",
    "load_base",
    (21354, 0x79dd_7f60_6e61_16fe),
    &[(4924, 1, "01"), (12583, 1, "01fe97fd59")],
    (21358, 0xd689_8e50_97c1_f5e3),
)];

#[test]
fn two_who_moved_out_share_a_household_only_as_a_pair() {
    for (name, label, base_pin, hunks, expected) in LOAD_REJECTED_MOVED_IN {
        let (base, data) = base_save(label);
        assert_eq!(sized(&base), base_pin, "{name}");
        assert!(World::load(&base, data()).is_ok(), "{name}: the base loads");
        let mut bytes = base.clone();
        for (offset, removed, put) in hunks.iter().rev() {
            bytes.splice(*offset..offset + removed, hex(put));
        }
        assert_eq!(sized(&bytes), expected, "{name}");
        assert_eq!(
            World::load(&bytes, data()).err(),
            Some(LoadError::Inconsistent),
            "{name}"
        );
    }
}

/// The census's population columns are the year's entries of the log, counted from the
/// definitions of the addendum to ADR-0021 (2026-10-21): births the entries `core:birth`,
/// emigrations the entries `core:emigrated` (one per person), the deaths of a cause the
/// entries `core:death` with it, arrivals the entries `core:arrived` not caused by `Start`
/// (input 1). In year 0 the arrivals of second 0 count and the persons `Start` brought
/// from the pool do not (its households until they bring 50, so at least 50). The census
/// reads the state, the entries are the log: two outputs of one run, over the 14 years
/// before the pool runs dry.
#[test]
fn the_census_counts_the_population_the_log_reports() {
    let mut run = switched_town(0xff, true);
    run.play(14 * Y - 1);
    let causes: Vec<String> = (sim_core::world::death_causes(run.world.data()).iter())
        .map(|code| code.as_str().to_owned())
        .collect();
    #[derive(Default, Debug, PartialEq)]
    struct Year {
        births: u64,
        arrivals: u64,
        emigrations: u64,
        deaths: BTreeMap<String, u64>,
    }
    let mut years: BTreeMap<i64, Year> = BTreeMap::new();
    let mut by_start = 0;
    for entry in &run.log {
        let entry = serde_json::to_value(entry).expect("json");
        let year = years
            .entry(entry["t"].as_i64().expect("a second").div_euclid(Y))
            .or_default();
        let payload = entry["payload"].as_object().expect("a payload");
        let (code, value) = payload.iter().next().expect("a code");
        match code.as_str() {
            "core:birth" => year.births += 1,
            "core:emigrated" => year.emigrations += 1,
            "core:death" => {
                let cause = value["cause"].as_str().expect("a cause").to_owned();
                assert!(causes.contains(&cause), "{cause}");
                *year.deaths.entry(cause).or_default() += 1;
            }
            "core:arrived" if entry["causes"] == serde_json::json!([{"input": 1}]) => {
                by_start += 1;
            }
            "core:arrived" => year.arrivals += 1,
            _ => {}
        }
    }
    assert!(by_start >= 50, "Start brought its households: {by_start}");
    assert_eq!(run.out.census.len(), 14);
    let (mut births, mut arrivals, mut deaths) = (0, 0, 0);
    for row in &run.out.census {
        let expected = years.remove(&row.year).unwrap_or_default();
        let got = Year {
            births: row.population.births,
            arrivals: row.population.arrivals,
            emigrations: row.population.emigrations,
            deaths: (causes.iter().zip(&row.population.deaths))
                .filter(|(_, n)| **n > 0)
                .map(|(code, n)| (code.clone(), *n))
                .collect(),
        };
        assert_eq!(got, expected, "year {}", row.year);
        births += got.births;
        arrivals += got.arrivals;
        deaths += got.deaths.values().sum::<u64>();
    }
    assert!(
        births > 0 && arrivals > 0 && deaths > 0,
        "{births} {arrivals} {deaths}"
    );
}

/// A save in year 0 continues with the census row of year 0 the uninterrupted run writes:
/// the loaded world knows again which persons `Start` made.
#[test]
fn a_save_in_year_0_keeps_what_start_made_out_of_the_arrivals() {
    let mut whole = switched_town(0xff, true);
    whole.play(Y - 1);
    let mut part = switched_town(0xff, true);
    part.play(Y / 2);
    let (mut loaded, _) = World::load(&part.save(), pool_c_data(POOL_C)).expect("loads");
    let mut out = YearOut::default();
    loaded.advance_until(&mut out, SimTime::from_secs(Y - 1), &mut |_| false);
    assert_eq!(out.census, whole.out.census);
}

/// The mean of the addendum to ADR-0021 (2026-10-21): one division, then half away from
/// zero.
fn mean_mm(sum: f64, count: u64) -> u64 {
    if count == 0 {
        return 0;
    }
    let n = sim_math::i64_to_f64_exact(i64::try_from(count).expect("small")).expect("exact");
    let m = sum / n;
    let up = sim_math::ceil(m);
    let down = if up > m { up - 1.0 } else { up };
    let rounded = if m - down >= 0.5 { down + 1.0 } else { down };
    u64::try_from(sim_math::to_i64_exact(rounded).expect("an integer")).expect("positive")
}

/// The census's heights are those of the cards (ADR-0021, addendum 2026-10-21): in each
/// year's last second, the living by sex, of at least 25 and of exactly 25 full years, the
/// mean of their height now (the body's `height_cm`, as on the card) in tenths of a cm,
/// added in ascending SimId. Every living person here has a body.
#[test]
fn the_census_heights_are_the_cards() {
    let mut run = switched_town(0xff, true);
    for year in 0..14 {
        let end = (year + 1) * Y - 1;
        run.play(end);
        let t = SimTime::from_secs(end);
        let (mut sums, mut counts) = ([[0.0_f64; 2]; 2], [[0_u64; 2]; 2]);
        for person in &run.world.pedigree().persons {
            if person.died.is_some() || person.emigrated.is_some() {
                continue;
            }
            let body = run.world.state().body_at(person.sim, t).expect("a body");
            let tenths = body
                .params
                .get(sim_core::body_contract::BodyParamKey::HeightCm)
                * 10.0;
            let sex = usize::from(person.sex == Sex::Male);
            let age = (end - person.born.as_secs()).div_euclid(Y);
            for (group, member) in [(0, age >= 25), (1, age == 25)] {
                if member {
                    sums[group][sex] += tenths;
                    counts[group][sex] += 1;
                }
            }
        }
        let means = |group: usize| {
            [0, 1].map(|sex| HeightMean {
                count: counts[group][sex],
                mm: mean_mm(sums[group][sex], counts[group][sex]),
            })
        };
        let row = run.out.census.last().expect("a row");
        assert_eq!(row.year, year);
        assert_eq!(row.inheritance.adults, means(0), "year {year}: at least 25");
        assert_eq!(row.inheritance.age25, means(1), "year {year}: 25");
    }
}

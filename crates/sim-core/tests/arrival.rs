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
use sim_core::input::{ArrangePartnership, Couple, Founder, Input, InputRecord, PoolStart, Start};
use sim_core::log::{LogEntry, log_hash};
use sim_core::params::WorldParams;
use sim_core::pool::{Pool, ranked_households, take_households};
use sim_core::time::SimTime;
use sim_core::world::{LoadError, World, YearOut};

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
        (6739, 0xbf84_e81e_8912_6795),
    ),
    (
        "founders_and_couple_with_pool",
        24,
        0xc7fb_d166_8c8e_94f6,
        (6748, 0x5adf_3843_03af_94e0),
    ),
    (
        "no_pool_in_the_data",
        25,
        0x5ae3_d70c_71cf_063a,
        (6732, 0xceb9_015a_a0bb_6b5f),
    ),
    (
        "pool_hash_not_the_data_pool",
        25,
        0xc7fb_d166_8c8e_94f6,
        (6732, 0x7176_4060_0438_277d),
    ),
    (
        "pool_of_other_data",
        25,
        0x6bc8_a5d6_8de3_6a5e,
        (6732, 0xceb9_015a_a0bb_6b5f),
    ),
    (
        "n_start_0_no_households",
        26,
        0xc7fb_d166_8c8e_94f6,
        (37, 0xf675_7735_a6eb_1955),
    ),
    (
        "n_start_0_with_households",
        26,
        0xc7fb_d166_8c8e_94f6,
        (6732, 0x4eaa_08ea_b798_ccb4),
    ),
    (
        "households_reversed",
        27,
        0xc7fb_d166_8c8e_94f6,
        (6732, 0x2fd5_a23a_9568_9037),
    ),
    (
        "households_one_missing",
        27,
        0xc7fb_d166_8c8e_94f6,
        (6393, 0x0737_e0b0_3880_e6b0),
    ),
    (
        "households_one_extra",
        27,
        0xc7fb_d166_8c8e_94f6,
        (6832, 0xa943_6cfd_1be5_85cb),
    ),
    (
        "households_of_another_seed",
        27,
        0xc7fb_d166_8c8e_94f6,
        (6469, 0x8d01_93d9_b13b_f476),
    ),
    (
        "households_one_altered",
        27,
        0xc7fb_d166_8c8e_94f6,
        (6732, 0x9435_eb8e_a071_4bf6),
    ),
    (
        "order_founders_before_missing_pool",
        24,
        0x5ae3_d70c_71cf_063a,
        (6730, 0xa751_592f_fca7_0895),
    ),
    (
        "order_missing_pool_before_n_start",
        25,
        0x5ae3_d70c_71cf_063a,
        (6732, 0x4eaa_08ea_b798_ccb4),
    ),
    (
        "order_n_start_before_households",
        26,
        0xc7fb_d166_8c8e_94f6,
        (6732, 0xa489_7270_e896_e036),
    ),
    (
        "decision_capacity_zero_before_pool",
        23,
        0xc7fb_d166_8c8e_94f6,
        (6724, 0x0028_066a_825f_c2c6),
    ),
    (
        "decision_couple_checks_before_pool",
        13,
        0xc7fb_d166_8c8e_94f6,
        (6734, 0x94c5_44c0_2e8a_4857),
    ),
];
const START_ACCEPTED: (usize, u64) = (6732, 0xceb9_015a_a0bb_6b5f);
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
const LOAD_BASE: (usize, u64) = (21141, 0x0084_8193_99de_8dc7);
const LOAD_REJECTED: [Patch; 8] = [
    (
        "arrival_born_off_by_one",
        9756,
        11384,
        "fd",
        (21141, 0x452c_e690_4400_6486),
    ),
    (
        "used_household_twice",
        13219,
        7889,
        "210e27060903001b0b192a1f071721140d261816231c1a11100f2013251e02050c0e",
        (21142, 0x049a_12a7_c55b_8b15),
    ),
    (
        "pool_child_arrived_not_born",
        9958,
        11182,
        "82",
        (21141, 0x9f3a_6e96_bb16_89fc),
    ),
    (
        "arrival_not_at_a_year_start",
        10978,
        10064,
        "fdd3d0ea0bd7eac2caadb9a5bd0700e08380fcf180a0b124e0e9c8808e809b906080c09a8c80c09e0c829580f0a78498ab03a0b28880eeeda190c1018180c481809c8b81600000000000000000200100000001d7eac2caadb9a5bd0701ffcbd1cc0b82",
        (21141, 0xfd51_8780_9a83_91ef),
    ),
    (
        "arrival_year_not_its_year",
        11080,
        10060,
        "02",
        (21141, 0xd5ec_66f8_564e_a3e7),
    ),
    (
        "town_pregnancy_without_father",
        12537,
        8602,
        "00",
        (21140, 0x080b_7aec_f05f_9af4),
    ),
    (
        "arrival_partnership_formed_off",
        12249,
        8891,
        "fd",
        (21141, 0x38f2_e717_24c8_6be6),
    ),
    (
        "arrival_without_pool",
        9840,
        11281,
        "",
        (21121, 0xd828_92ac_7400_1bf2),
    ),
];
// arrival
const ARRIVAL_DATA_HASH: u64 = 0xc7fb_d166_8c8e_94f6;
const ARRIVAL_POOL_HASH: u64 = 0xec01_a0ff_0726_0ad1;
const ARRIVAL_START_INPUT: (usize, u64) = (6732, 0xceb9_015a_a0bb_6b5f);
const ARRIVAL_YEAR_HASHES: [u64; 14] = [
    0xffcc_209a_649b_715e,
    0x0dcb_d352_d750_ed2f,
    0x81ca_f9ad_9b06_65d6,
    0xccf0_4232_598d_6899,
    0x585f_3fae_8e40_5210,
    0xcb75_75d9_ed66_207c,
    0xd6ec_704f_9d8c_2370,
    0xc816_237d_e65b_e114,
    0xd954_e3cb_7c75_fdcc,
    0xdf77_22b3_ac2f_aa5e,
    0xe63c_5b47_8a3c_729f,
    0x78be_420a_098e_db68,
    0xaba5_3f9c_3811_d365,
    0xaa4e_c881_af86_a6c1,
];
const ARRIVAL_LOG: (usize, u64) = (1537, 0xa7f5_0c2c_d8fc_b27c);
/// The state at the end of year 13, the reference's `arrival_state_hash` (its stop at the
/// immigration of year 14 now runs on a copy of its world, so the row is the state at the
/// end, the projection in its end save).
const ARRIVAL_STATE_HASH: u64 = 0xa44f_41e1_e22b_90e8;
const ARRIVAL_END: i64 = 440294399;
const ARRIVAL_SAVE_AFTER_START: (usize, u64) = (16885, 0xde81_027e_38b9_00e8);
const ARRIVAL_SAVE_END: (usize, u64) = (29105, 0xb5d5_8c3d_0152_865c);
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
        (16885, 0xde81_027e_38b9_00e8),
        (17055, 0x41d4_c857_9879_f243),
        &[
            (
                "6100010034010101000dfbf7cfcf86f2c9d96a",
                Some(
                    r#"{"id":97,"t":0,"subjects":[{"sim":52}],"causes":[{"plan":{"by":1,"planned_at":0}}],"payload":{"core:arrived":{"lineage_key":"6ab3279069f3fbfb"}}}"#,
                ),
            ),
            (
                "620001003402010100030e636f72653a686f757365686f6c640a1e",
                Some(
                    r#"{"id":98,"t":0,"subjects":[{"sim":52}],"causes":[{"plan":{"by":1,"planned_at":0}},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":30}}}"#,
                ),
            ),
        ],
    ),
    (
        "pool_child_without_father",
        14515200,
        (18160, 0x4094_0e19_3c8c_08b5),
        (18354, 0xc863_aa04_662d_bf04),
        &[
            (
                "810180f0eb0d02003b002402014400030e636f72653a676573746174696f6e08bec2c4bfb28d9fcca201",
                None,
            ),
            (
                "820180f0eb0d01003b02014400030e636f72653a686f757365686f6c640b1f",
                Some(
                    r#"{"id":130,"t":14515200,"subjects":[{"sim":59}],"causes":[{"plan":{"by":68,"planned_at":0}},{"rule":"core:household"}],"payload":{"core:moved":{"household":31}}}"#,
                ),
            ),
            (
                "830180f0eb0d010027010148000201",
                Some(
                    r#"{"id":131,"t":14515200,"subjects":[{"sim":39}],"causes":[{"plan":{"by":72,"planned_at":0}}],"payload":{"core:birthday":{"age_years":1}}}"#,
                ),
            ),
            (
                "840180f0eb0d010028010149000201",
                Some(
                    r#"{"id":132,"t":14515200,"subjects":[{"sim":40}],"causes":[{"plan":{"by":73,"planned_at":0}}],"payload":{"core:birthday":{"age_years":1}}}"#,
                ),
            ),
        ],
    ),
    (
        "town_child_of_pool_persons",
        26611200,
        (18643, 0x90f3_4378_2ddf_b13b),
        (18848, 0x73dc_a66d_e6e0_4fad),
        &[
            (
                "b50180b8b01903003c000d000c02016580a8a702030e636f72653a676573746174696f6e08b49498f0bdf2d0a517",
                None,
            ),
            (
                "b60180b8b01901003c02016580a8a702030e636f72653a686f757365686f6c640b0a",
                Some(
                    r#"{"id":182,"t":26611200,"subjects":[{"sim":60}],"causes":[{"plan":{"by":101,"planned_at":2419200}},{"rule":"core:household"}],"payload":{"core:moved":{"household":10}}}"#,
                ),
            ),
            (
                "b70180b8b01903003d000d000c02016580a8a702030e636f72653a676573746174696f6e08c4e8be9380abdda926",
                None,
            ),
            (
                "b80180b8b01901003d02016580a8a702030e636f72653a686f757365686f6c640b0a",
                Some(
                    r#"{"id":184,"t":26611200,"subjects":[{"sim":61}],"causes":[{"plan":{"by":101,"planned_at":2419200}},{"rule":"core:household"}],"payload":{"core:moved":{"household":10}}}"#,
                ),
            ),
        ],
    ),
    (
        "immigration_1",
        31449600,
        (18795, 0xc46f_f6e7_900b_0791),
        (18982, 0x1640_83ac_ee63_1fb4),
        &[
            (
                "ba018088ff1d01003e010101000dd7eac2caadb9a5bd07",
                Some(
                    r#"{"id":186,"t":31449600,"subjects":[{"sim":62}],"causes":[{"plan":{"by":1,"planned_at":0}}],"payload":{"core:arrived":{"lineage_key":"077a95cad950b557"}}}"#,
                ),
            ),
            (
                "bb018088ff1d01003e02010100030e636f72653a686f757365686f6c640a20",
                Some(
                    r#"{"id":187,"t":31449600,"subjects":[{"sim":62}],"causes":[{"plan":{"by":1,"planned_at":0}},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":32}}}"#,
                ),
            ),
            (
                "bc018088ff1d010016010129000201",
                Some(
                    r#"{"id":188,"t":31449600,"subjects":[{"sim":22}],"causes":[{"plan":{"by":41,"planned_at":0}}],"payload":{"core:birthday":{"age_years":1}}}"#,
                ),
            ),
        ],
    ),
    (
        "carried_separation",
        84829122,
        (21073, 0x9840_211b_3815_979b),
        (21053, 0x38eb_47b0_3fa5_3be1),
        &[
            (
                "eb02848ff350020033003202015e0004128ec1c0dde3ddd8da940100e48ed2e6ecad8cc8a501090f",
                Some(
                    r#"{"id":363,"t":84829122,"subjects":[{"sim":51},{"sim":50}],"causes":[{"plan":{"by":94,"planned_at":0}},{"random":{"tag":"0000000000000012","subject":"94b562ee3bb0208e","occasion":"0000000000000000","detail":"a590316eccd48764"}}],"payload":{"core:separated":{"partnership":15}}}"#,
                ),
            ),
            (
                "ec02848ff3500100320200eb02030e636f72653a686f757365686f6c640a23",
                Some(
                    r#"{"id":364,"t":84829122,"subjects":[{"sim":50}],"causes":[{"event":363},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":35}}}"#,
                ),
            ),
            (
                "ed02848ff3500100320200eb02030e636f72653a686f757365686f6c640b23",
                Some(
                    r#"{"id":365,"t":84829122,"subjects":[{"sim":50}],"causes":[{"event":363},{"rule":"core:household"}],"payload":{"core:moved":{"household":35}}}"#,
                ),
            ),
        ],
    ),
    (
        "immigrant_death",
        183152461,
        (24939, 0xd7d0_db9f_2b5a_ff7f),
        (24888, 0x8251_12f1_73bf_f385),
        &[(
            "ac059abdd5ae0101003e0201ba018088ff1d0404d7eac2caadb9a5bd07a0bb8ca1c58498a01c01040f636f72653a73656e657363656e6365",
            Some(
                r#"{"id":684,"t":183152461,"subjects":[{"sim":62}],"causes":[{"plan":{"by":186,"planned_at":31449600}},{"random":{"tag":"0000000000000004","subject":"077a95cad950b557","occasion":"1c40602454231da0","detail":"0000000000000001"}}],"payload":{"core:death":{"cause":"core:senescence"}}}"#,
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
const ORPHANS_START_INPUT: (usize, u64) = (5022, 0x711b_51a7_935d_ec72);
const ORPHANS_YEAR_HASHES: [u64; 5] = [
    0xe941_9467_3223_a208,
    0x6e8c_367b_d785_e64d,
    0x6c48_2616_7328_a50f,
    0xb128_8281_24f9_2aa4,
    0x388c_773a_1400_f342,
];
const ORPHANS_LOG: (usize, u64) = (255, 0x6870_f48c_6d0a_07e8);
const ORPHANS_STATE_HASH: u64 = 0xf80a_19af_7a0c_36ab;
const ORPHANS_END: i64 = 157247999;
const ORPHANS_SAVE_AFTER_START: (usize, u64) = (13674, 0x9d5c_c867_91dc_4cc7);
const ORPHANS_SAVE_END: (usize, u64) = (14315, 0xedd3_339f_dc1a_27aa);
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
        (13784, 0xe635_d543_8c99_3777),
        (13664, 0x5aa7_1e80_0417_e3fd),
        &[(
            "579ef2a71001001e020137000404d5b5dca580aceb9b26db86dcd7e8e9bbfac90100040f636f72653a646973656173655f7872",
            Some(
                r#"{"id":87,"t":17104015,"subjects":[{"sim":30}],"causes":[{"plan":{"by":55,"planned_at":0}},{"random":{"tag":"0000000000000004","subject":"2637ad6004b71ad5","occasion":"c9f4ef4e8af7035b","detail":"0000000000000000"}}],"payload":{"core:death":{"cause":"core:disease_xr"}}}"#,
            ),
        )],
    ),
    (
        "succession_second",
        33206750,
        (14003, 0x829f_d521_e88c_f582),
        (13909, 0xd234_2678_0bc2_adf3),
        &[(
            "63bcc7d51f010016020128000404d7b09bb8acdba78d799aada6adcbb284bf7200040f636f72653a646973656173655f6172",
            Some(
                r#"{"id":99,"t":33206750,"subjects":[{"sim":22}],"causes":[{"plan":{"by":40,"planned_at":0}},{"random":{"tag":"0000000000000004","subject":"791a9edac706d857","occasion":"727e1194b5a9969a","detail":"0000000000000000"}}],"payload":{"core:death":{"cause":"core:disease_ar"}}}"#,
            ),
        )],
    ),
    (
        "orphan_rule",
        119487822,
        (14364, 0xbb1c_a1b4_dbad_90bf),
        (14260, 0xa672_5f4e_3ea7_d13f),
        &[
            (
                "d3019cf5f971010015020127000404c98382daf5bdb0b3d001a0bb8ca1c58498a01c00040f636f72653a73656e657363656e6365",
                Some(
                    r#"{"id":211,"t":119487822,"subjects":[{"sim":21}],"causes":[{"plan":{"by":39,"planned_at":0}},{"random":{"tag":"0000000000000004","subject":"d066c1ef5b4081c9","occasion":"1c40602454231da0","detail":"0000000000000000"}}],"payload":{"core:death":{"cause":"core:senescence"}}}"#,
                ),
            ),
            (
                "d4019cf5f9710100180200d301030e636f72653a686f757365686f6c640b04",
                Some(
                    r#"{"id":212,"t":119487822,"subjects":[{"sim":24}],"causes":[{"event":211},{"rule":"core:household"}],"payload":{"core:moved":{"household":4}}}"#,
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
        "0300010001020201030e636f72653a686f757365686f6c640a01",
        Some(
            r#"{"id":3,"t":0,"subjects":[{"sim":1}],"causes":[{"input":1},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":1}}}"#,
        ),
    ),
    (
        "04000100020102010de7d0f8aac990d1c873",
        Some(
            r#"{"id":4,"t":0,"subjects":[{"sim":2}],"causes":[{"input":1}],"payload":{"core:arrived":{"lineage_key":"73914484955e2867"}}}"#,
        ),
    ),
    (
        "0500010002020201030e636f72653a686f757365686f6c640a02",
        Some(
            r#"{"id":5,"t":0,"subjects":[{"sim":2}],"causes":[{"input":1},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":2}}}"#,
        ),
    ),
    (
        "06000100030102010dfde2dfbfc0a8c99a6f",
        Some(
            r#"{"id":6,"t":0,"subjects":[{"sim":3}],"causes":[{"input":1}],"payload":{"core:arrived":{"lineage_key":"6f35254407f7f17d"}}}"#,
        ),
    ),
];
// records
const RECORDS_DATA_HASH: u64 = 0xeccf_f256_9ac1_2bb4;
const RECORDS_POOL_HASH: u64 = 0xfd94_6c4f_2d00_c4cb;
const RECORDS_START_INPUT: (usize, u64) = (4532, 0xb22b_1583_db62_c176);
const RECORDS_YEAR_HASHES: [u64; 19] = [
    0x989d_00e7_e2ab_d25e,
    0xecb7_e8b7_0a89_643f,
    0x0d67_eb47_7781_00b9,
    0x8be7_0f43_7ab0_09bd,
    0xdafb_a987_6ddc_a63f,
    0xfd31_df8f_1d15_aaf9,
    0x082e_d907_1477_3774,
    0x4a6d_e7f9_1f4a_a02c,
    0x8296_025b_b7d6_8af8,
    0xa905_1cbe_7bb8_02ae,
    0x7c5d_4cb9_8dfe_0295,
    0x1fd1_f2c5_debf_0192,
    0xc5c1_17b3_ccc4_91ab,
    0x59ea_4caf_d41f_f7b5,
    0xb427_6cfb_9448_74c6,
    0x5ec5_0caf_7d7f_6368,
    0x411e_b8ae_7e12_d467,
    0x0161_bd22_dc55_e825,
    0xfd3a_4987_58ea_42d2,
];
const RECORDS_LOG: (usize, u64) = (586, 0x6403_ccdb_e0e3_4537);
const RECORDS_STATE_HASH: u64 = 0x0aa1_79b0_51cb_7fab;
const RECORDS_END: i64 = 597542399;
const RECORDS_SAVE_AFTER_START: (usize, u64) = (12638, 0xf943_4ba1_99c6_328c);
const RECORDS_SAVE_END: (usize, u64) = (13809, 0xd110_72e8_6457_4f2c);
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
        (12896, 0xb42a_2ab9_8771_2332),
        (12836, 0xed29_67c2_5eef_20f8),
        &[
            (
                "6facfeff2d01000a020111000404d897c7d8d99df5d059a0bb8ca1c58498a01c00040f636f72653a73656e657363656e6365",
                Some(
                    r#"{"id":111,"t":48234390,"subjects":[{"sim":10}],"causes":[{"plan":{"by":17,"planned_at":0}},{"random":{"tag":"0000000000000004","subject":"59a1d4ed9b11cbd8","occasion":"1c40602454231da0","detail":"0000000000000000"}}],"payload":{"core:death":{"cause":"core:senescence"}}}"#,
                ),
            ),
            (
                "70acfeff2d01000b02006f030e636f72653a686f757365686f6c640b0f",
                Some(
                    r#"{"id":112,"t":48234390,"subjects":[{"sim":11}],"causes":[{"event":111},{"rule":"core:household"}],"payload":{"core:moved":{"household":15}}}"#,
                ),
            ),
        ],
    ),
    (
        "mixed_child",
        595123200,
        (13694, 0x68d2_59b4_b513_0f1f),
        (13798, 0xe943_4835_2a80_f7b9),
        &[
            (
                "c70480f0c6b704030026001a001f0201b80480e0bda004030e636f72653a676573746174696f6e08feffe9a2d9e7fc84d801",
                None,
            ),
            (
                "c80480f0c6b7040100260201b80480e0bda004030e636f72653a686f757365686f6c640b1c",
                Some(
                    r#"{"id":584,"t":595123200,"subjects":[{"sim":38}],"causes":[{"plan":{"by":568,"planned_at":570931200}},{"rule":"core:household"}],"payload":{"core:moved":{"household":28}}}"#,
                ),
            ),
            (
                "c90480f0c6b7040100080101ab0480e8c799040220",
                Some(
                    r#"{"id":585,"t":595123200,"subjects":[{"sim":8}],"causes":[{"plan":{"by":555,"planned_at":563673600}}],"payload":{"core:birthday":{"age_years":32}}}"#,
                ),
            ),
            (
                "ca0480f0c6b7040100210101ac0480e8c79904020c",
                Some(
                    r#"{"id":586,"t":595123200,"subjects":[{"sim":33}],"causes":[{"plan":{"by":556,"planned_at":563673600}}],"payload":{"core:birthday":{"age_years":12}}}"#,
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
        "05000100030102010dc3bebf8ffef3b5c2f701",
        Some(
            r#"{"id":5,"t":0,"subjects":[{"sim":3}],"causes":[{"input":1}],"payload":{"core:arrived":{"lineage_key":"f784d79fe1efdf43"}}}"#,
        ),
    ),
    (
        "06000100040102010dece4ceded8abe3f64d",
        Some(
            r#"{"id":6,"t":0,"subjects":[{"sim":4}],"causes":[{"input":1}],"payload":{"core:arrived":{"lineage_key":"4ded8d5d8bd3b26c"}}}"#,
        ),
    ),
];
// micro
const MICRO_POOL: (usize, u64) = (10966, 0xfff9_bfca_0d23_8c97);
const RECORDS_ARRANGE: ((usize, u64), (usize, u64), &[Entry]) = (
    (13541, 0x2a38_cf77_e1eb_9e4b),
    (13616, 0x3648_9dfc_4977_ca6c),
    &[
        (
            "ad048090ef9b0402001a001f0102020509",
            Some(
                r#"{"id":557,"t":566092800,"subjects":[{"sim":26},{"sim":31}],"causes":[{"input":2}],"payload":{"core:partnered":{"partnership":9}}}"#,
            ),
        ),
        (
            "ae048090ef9b0402001a001f0200ad04030e636f72653a686f757365686f6c640a1c",
            Some(
                r#"{"id":558,"t":566092800,"subjects":[{"sim":26},{"sim":31}],"causes":[{"event":557},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":28}}}"#,
            ),
        ),
        (
            "af048090ef9b0401001a0200ad04030e636f72653a686f757365686f6c640b1c",
            Some(
                r#"{"id":559,"t":566092800,"subjects":[{"sim":26}],"causes":[{"event":557},{"rule":"core:household"}],"payload":{"core:moved":{"household":28}}}"#,
            ),
        ),
        (
            "b0048090ef9b0401001f0200ad04030e636f72653a686f757365686f6c640b1c",
            Some(
                r#"{"id":560,"t":566092800,"subjects":[{"sim":31}],"causes":[{"event":557},{"rule":"core:household"}],"payload":{"core:moved":{"household":28}}}"#,
            ),
        ),
    ],
);
const EMPTY_DATA_HASH: u64 = 0x6bc8_a5d6_8de3_6a5e;
const EMPTY_START_INPUT: (usize, u64) = (3632, 0x7ae8_c304_20e2_6b00);
const EMPTY_SAVE_AFTER_START: (usize, u64) = (9381, 0x975a_40b0_ae95_5b18);
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
        (16885, 0xde81_027e_38b9_00e8),
        &[(9108, 2, "00")],
        (16884, 0x4763_bcf2_e71d_fa08),
    ),
    (
        "town_series_missing",
        "records_save_after_arrange",
        (13616, 0x3648_9dfc_4977_ca6c),
        &[(34, 1, "6c"), (103, 15, ""), (8401, 1, "6c"), (8438, 7, "")],
        (13594, 0xa78b_23fa_ed32_edc5),
    ),
    (
        "immigration_order_swapped",
        "load_base",
        (21141, 0x0084_8193_99de_8dc7),
        &[(13250, 0, "0c"), (13251, 1, "")],
        (21141, 0xa1cb_c6f9_9ef6_f894),
    ),
    (
        "unstarted_with_used_household",
        "unstarted",
        (39, 0xb477_0446_92af_285d),
        &[(34, 0, "01")],
        (40, 0x8a26_fc21_28e0_0725),
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
const TIE_START_INPUT: (usize, u64) = (6384, 0xfa83_f5e3_49aa_50f3);
const TIE_SAVE_AT: (usize, u64) = (17235, 0xce18_8365_b53c_3ace);
const TIE_SAVE_BEFORE_ATTEMPT: (usize, u64) = (17436, 0x8d9b_961c_ce80_b58f);
/// The save at `TIE_AT` with her series moved to after the pause of the birth.
const TIE_COUNTED_BIRTH: (&[Hunk], (usize, u64)) = (
    &[
        (505, 14, ""),
        (723, 0, "80d0c32900e30103250180a8a702"),
        (10420, 5, ""),
        (10489, 0, "7980f0eb0d"),
    ],
    (17235, 0x3058_a5b6_75c9_d04c),
);

/// The reference's states changed in one field (out4, its notes' section 5b).
const LOAD_REJECTED_FIELDS: [Hunks; 32] = [
    (
        "arrival_arrived_one_second_late",
        "orphans_save_end",
        (14315, 0xedd3_339f_dc1a_27aa),
        &[(8514, 1, "02")],
        (14315, 0xa898_edff_00f3_b2c0),
    ),
    (
        "arrival_year_off_by_one",
        "orphans_save_end",
        (14315, 0xedd3_339f_dc1a_27aa),
        &[(8515, 1, "01")],
        (14315, 0xe83c_ac5e_6a88_f0d1),
    ),
    (
        "arrival_born_pool_changed",
        "orphans_save_end",
        (14315, 0xedd3_339f_dc1a_27aa),
        &[(4373, 1, "82")],
        (14315, 0x1a3c_4cdd_59ce_63ad),
    ),
    (
        "arrival_genome_key_changed",
        "orphans_save_end",
        (14315, 0xedd3_339f_dc1a_27aa),
        &[(4363, 1, "c8")],
        (14315, 0x1d39_7076_da79_b250),
    ),
    (
        "arrival_pool_mother_changed",
        "orphans_save_end",
        (14315, 0xedd3_339f_dc1a_27aa),
        &[(4381, 9, "caeea683b29ea549")],
        (14314, 0x8149_4b34_71ab_0af7),
    ),
    (
        "arrival_pool_father_changed",
        "orphans_save_end",
        (14315, 0xedd3_339f_dc1a_27aa),
        &[(4391, 10, "ddd0b2b8c5fbd04b")],
        (14313, 0x1e89_cc1e_a814_189a),
    ),
    (
        "arrival_pool_social_parents_changed",
        "orphans_save_end",
        (14315, 0xedd3_339f_dc1a_27aa),
        &[(4401, 1, "01caeea683b29ea549")],
        (14323, 0x1194_9612_6b3a_9046),
    ),
    (
        "arrival_sex_flipped",
        "orphans_save_end",
        (14315, 0xedd3_339f_dc1a_27aa),
        &[(8372, 1, "00")],
        (14315, 0xbd30_c6fc_0cfa_f134),
    ),
    (
        "arrival_genome_changed",
        "orphans_save_end",
        (14315, 0xedd3_339f_dc1a_27aa),
        &[(4245, 1, "9a")],
        (14315, 0x3097_9acf_515b_cdcc),
    ),
    (
        "arrival_town_parents_set",
        "orphans_save_end",
        (14315, 0xedd3_339f_dc1a_27aa),
        &[(4359, 0, "011615")],
        (14318, 0xf767_47a7_af6e_0b31),
    ),
    (
        "arrival_household_moved_to_another_year",
        "orphans_save_end",
        (14315, 0xedd3_339f_dc1a_27aa),
        &[
            (113, 19, ""),
            (601, 0, "92e3d6fd02001502049aada6adcbb284bf7200"),
            (683, 39, ""),
            (762, 19, ""),
            (850, 19, ""),
            (
                926,
                0,
                "90c4818e0800130204a0bb8ca1c58498a01c00a0a6feca08001b0205d49ae3bbeacfeb962f00",
            ),
            (944, 0, "0096c18d9f0a00160204d49ae3bbeacfeb962f"),
            (965, 0, "00c2b1a8c40a00190205a0bb8ca1c58498a01c"),
            (1004, 19, ""),
            (1112, 0, "00f4f5cefe0e001c0205db86dcd7e8e9bbfac901"),
            (1351, 10, ""),
            (1380, 0, "00d0e2fdd51500110501"),
            (1522, 0, "b68caac22100170204db86dcd7e8e9bbfac90100"),
            (1662, 40, ""),
            (2043, 0, "92e5ae99fb0d0014020485d2bc82f8b2ead44d00"),
            (2124, 0, "bad6d3ca8315001a020585d2bc82f8b2ead44d00"),
            (2175, 20, ""),
            (2519, 4, "afc2f80c"),
            (2621, 1, "8088ff1d"),
            (2649, 1, "8088ff1d"),
            (2678, 1, "8088ff1d"),
            (2702, 2, "8088ff1d01"),
            (2728, 4, "c7e0e30c"),
            (2827, 1, "8088ff1d"),
            (2856, 1, "8088ff1d"),
            (2881, 2, "8088ff1d01"),
            (6339, 3, "c785ba"),
            (6444, 1, "8088ff1d"),
            (8818, 2, ""),
            (8990, 0, "088088ff1d"),
            (9003, 4, ""),
            (9020, 2, ""),
            (
                9034,
                21,
                "1f002f002400088088ff1d098088ff1d1900088088ff1d3900098088ff1db6018090f3602a",
            ),
            (9074, 0, "098088ff1d"),
            (9115, 7, "5880c0ba12088088ff1d"),
            (9147, 0, "088088ff1d"),
            (9165, 4, ""),
            (9215, 0, "088088ff1d"),
            (9223, 0, "098088ff1d"),
            (9232, 2, ""),
        ],
        (14369, 0x1889_f42d_b04e_f38b),
    ),
    (
        "pool_child_genome_key_changed",
        "arrival_save_after_pool_child_without_father",
        (18354, 0xc863_aa04_662d_bf04),
        &[(10250, 1, "bf")],
        (18354, 0x556b_ac44_3231_5306),
    ),
    (
        "pool_child_arrived_not_born",
        "arrival_save_after_pool_child_without_father",
        (18354, 0xc863_aa04_662d_bf04),
        &[(10261, 2, "feef")],
        (18354, 0xa1d2_770d_8339_26c9),
    ),
    (
        "pool_child_arrival_year_off_by_one",
        "arrival_save_after_pool_child_without_father",
        (18354, 0xc863_aa04_662d_bf04),
        &[(10265, 1, "01")],
        (18354, 0x217c_7e45_3eeb_26a0),
    ),
    (
        "pool_child_pool_mother_changed",
        "arrival_save_after_pool_child_without_father",
        (18354, 0xc863_aa04_662d_bf04),
        &[(10267, 9, "d7eac2caadb9a5bd07")],
        (18354, 0xdeef_f589_8488_1502),
    ),
    (
        "pool_child_pool_father_changed",
        "arrival_save_after_pool_child_without_father",
        (18354, 0xc863_aa04_662d_bf04),
        &[(10277, 10, "b9b4d4cad0ccd1b808")],
        (18353, 0x91d3_a182_55bf_d4e3),
    ),
    (
        "pool_child_social_parents_set",
        "arrival_save_after_pool_child_without_father",
        (18354, 0xc863_aa04_662d_bf04),
        &[(10287, 1, "01cd9dd6b0cadf9ece6a")],
        (18363, 0x6b53_eaa6_73a6_e38a),
    ),
    (
        "pool_child_sex_flipped",
        "arrival_save_after_pool_child_without_father",
        (18354, 0xc863_aa04_662d_bf04),
        &[(10148, 1, "01")],
        (18354, 0x8051_91d4_7e98_ddb2),
    ),
    (
        "pool_child_genome_changed",
        "arrival_save_after_pool_child_without_father",
        (18354, 0xc863_aa04_662d_bf04),
        &[(10160, 1, "94")],
        (18354, 0x68ca_99e4_4ffc_39b8),
    ),
    (
        "pool_child_town_parents_set",
        "arrival_save_after_pool_child_without_father",
        (18354, 0xc863_aa04_662d_bf04),
        &[(10247, 0, "012401")],
        (18357, 0x6a6f_61e4_c64b_a427),
    ),
    (
        "pool_child_born_after_the_clock",
        "arrival_save_after_pool_child_without_father",
        (18354, 0xc863_aa04_662d_bf04),
        &[
            (785, 1, "82"),
            (1094, 1, "bc"),
            (1155, 1, "82"),
            (2133, 1, "92"),
            (3110, 1, "d0"),
            (10134, 1, "82"),
            (10236, 1, "82"),
            (10261, 1, "82"),
        ],
        (18354, 0x7ff2_e3bd_0b68_ee0f),
    ),
    (
        "pool_child_mother_not_an_arrival",
        "arrival_save_after_pool_child_without_father",
        (18354, 0xc863_aa04_662d_bf04),
        &[(7516, 23, "011f01"), (10987, 1, "1d"), (11008, 1, "")],
        (18333, 0xc4bf_6161_f920_aaf0),
    ),
    (
        "pool_child_mother_left_before_the_birth",
        "arrival_save_after_pool_child_without_father",
        (18354, 0xc863_aa04_662d_bf04),
        &[
            (34, 1, "df"),
            (315, 10, ""),
            (741, 14, ""),
            (1519, 20, ""),
            (3540, 24, "f70139"),
            (7436, 107, ""),
            (10245, 2, "00"),
            (
                10836,
                1,
                "0224ffc3e1ce07feefeb0d0085d2bc82f8b2ead44dcd9dd6b0cadf9ece6a00809380ba8083a8b116a0b98280caab85a0870185c0c98a80c09188c001889b80d8cd81b0d52180f5d580ca848e806980c0fa8280e0a18aa00100000000000101021f01000001cd9dd6b0cadf9ece6a01ffb3e392070000000000",
            ),
            (11018, 1, "df"),
            (11081, 2, ""),
            (11207, 6, ""),
            (11354, 2, ""),
            (11615, 2, ""),
        ],
        (18289, 0xa550_6230_86ad_21e0),
    ),
    (
        "no_pool_living_with_pool_origin",
        "founders_world",
        (10816, 0xb49c_8383_4fff_5a68),
        &[(243, 0, "000000010001ffd7e6a90600")],
        (10828, 0x7349_768c_9a06_503d),
    ),
    (
        "no_pool_archived_with_pool_origin",
        "founders_world",
        (10816, 0xb49c_8383_4fff_5a68),
        &[(348, 0, "0000010001ffbfadcf140000")],
        (10828, 0x5752_0e1f_11eb_1eb8),
    ),
    (
        "no_pool_used_household",
        "founders_world",
        (10816, 0xb49c_8383_4fff_5a68),
        &[(400, 0, "01")],
        (10817, 0x6cab_6c31_0f80_22a6),
    ),
    (
        "no_pool_pregnancy_without_father",
        "founders_world",
        (10816, 0xb49c_8383_4fff_5a68),
        &[(272, 2, "00")],
        (10815, 0x67cb_16f4_1305_111a),
    ),
    (
        "no_pool_pregnancy_with_pool_father",
        "founders_world",
        (10816, 0xb49c_8383_4fff_5a68),
        &[(274, 1, "01d388fe84eef9bce93c")],
        (10825, 0x284b_c8e1_4516_6665),
    ),
    (
        "arrival_social_parent_is_its_pool_parent",
        "orphans_save_end",
        (14315, 0xedd3_339f_dc1a_27aa),
        &[(4361, 1, "15")],
        (14315, 0xcd41_8b8c_b4ff_969d),
    ),
    (
        "arrival_partnership_close_kin",
        "orphans_save_end",
        (14315, 0xedd3_339f_dc1a_27aa),
        &[
            (32, 3, "0a1a8001"),
            (1291, 0, "baaea9961300db02050900"),
            (2225, 1, "dc"),
            (2472, 1, "19"),
            (3014, 2, "0219"),
            (6021, 1, "19"),
            (6141, 1, "19"),
            (6334, 1, "09"),
            (6429, 7, "090a0280a8fb9501001901000002000180a8fb9501"),
            (6470, 1, "0180a8fb9501"),
            (6550, 0, "01001980a8fb95"),
            (8779, 1, "8001"),
            (9102, 0, "800280a8fb9501"),
            (9237, 3, "860202"),
            (14262, 0, "fea7fb950102020a02"),
        ],
        (14370, 0xa063_e237_7adf_b656),
    ),
    (
        "arrival_moved_out_by_town_partnership_flag_false",
        "arrival_moved_out_by_town_partnership",
        (12986, 0x63f1_6586_e5bf_c439),
        &[(3567, 1, "00")],
        (12986, 0x5d79_b012_47b9_68a3),
    ),
    (
        "brought_partnership_member_not_moved_out_moved_out",
        "brought_partnership_member_not_moved_out",
        (16901, 0x09d7_42f6_b95a_1873),
        &[(3283, 1, "01")],
        (16901, 0xbfab_dd3a_914e_f6a8),
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
        (18982, 0x1640_83ac_ee63_1fb4),
        &[
            (53, 13, ""),
            (151, 0, "0180a8a70280d8cd22004b0315"),
            (11401, 2, ""),
            (11435, 0, "2800"),
        ],
        (18982, 0x15c9_9edb_fef8_89de),
    ),
    (
        "carried_series_before_its_start",
        "arrival_save_after_start",
        (16885, 0xde81_027e_38b9_00e8),
        &[
            (43, 0, "00000a03030180a8a702"),
            (60, 13, ""),
            (9756, 5, "05005e0001"),
        ],
        (16882, 0x49f4_5900_f625_f182),
    ),
    (
        "carried_series_missing",
        "arrival_save_after_immigration_1",
        (18982, 0x1640_83ac_ee63_1fb4),
        &[(34, 1, "e9"), (53, 13, ""), (11392, 10, "e9017880a09d0901")],
        (18967, 0xc496_46cc_850e_d6b8),
    ),
    (
        "carried_series_past_fertility",
        "V3_carried_series_ended",
        (22466, 0x6dc9_69c0_4742_1a73),
        &[
            (34, 1, "ad"),
            (565, 0, "0180a8a70280b88f88010097010328"),
            (14642, 1, "ad"),
            (14927, 0, "4800"),
        ],
        (22483, 0x73ad_b9c7_60d9_54ce),
    ),
    (
        "carried_conception_before_its_series",
        "V4b_carried_conception_pregnant",
        (18639, 0x4a27_0aa4_283a_64a2),
        &[
            (312, 0, "a302042d0080d0c32900"),
            (340, 10, ""),
            (10938, 3, "c0ba12"),
            (11216, 0, "b10180c0ba12"),
            (11233, 6, ""),
        ],
        (18639, 0x1ec5_7cc9_2d83_cda5),
    ),
    (
        "pool_pregnancy_dropped",
        "arrival_save_after_start",
        (16885, 0xde81_027e_38b9_00e8),
        &[
            (31, 1, "c5"),
            (199, 10, ""),
            (9106, 1, "05"),
            (9542, 89, ""),
            (9752, 1, "c5"),
            (9783, 2, ""),
        ],
        (16784, 0xe921_f166_c067_fb0c),
    ),
    (
        "pool_birth_vanished",
        "arrival_save_after_pool_child_without_father",
        (18354, 0xc863_aa04_662d_bf04),
        &[
            (31, 4, "3b1120de"),
            (472, 0, "98931000f40103240180a8a70280"),
            (746, 14, ""),
            (784, 10, ""),
            (1094, 20, ""),
            (1154, 11, ""),
            (2133, 20, ""),
            (3110, 21, ""),
            (3563, 1, "39"),
            (7515, 1, "00"),
            (10133, 155, ""),
            (11018, 1, "de"),
            (11112, 0, "810180f0eb0d"),
            (11207, 6, ""),
            (11225, 6, ""),
            (11291, 6, ""),
            (11306, 6, ""),
            (11428, 6, ""),
            (11567, 6, ""),
        ],
        (18087, 0x6676_503f_7d17_9a85),
    ),
    (
        "pool_child_born_one_second_early",
        "arrival_save_after_pool_child_without_father",
        (18354, 0xc863_aa04_662d_bf04),
        &[
            (765, 0, "fef7ea2b00ef01013b00"),
            (784, 10, ""),
            (1094, 1, "b8"),
            (1155, 2, "fe8f"),
            (2133, 1, "8e"),
            (3110, 1, "cc"),
            (10134, 2, "feef"),
            (10236, 2, "feef"),
            (10261, 2, "feef"),
            (11218, 13, "810180f0eb0d830180f0eb0d84"),
        ],
        (18354, 0xb430_62a4_ef71_8cff),
    ),
    (
        "pool_twin_missing",
        "V5_pool_twins_born",
        (17689, 0x281d_6851_e4ce_15d4),
        &[
            (31, 4, "381120d1"),
            (747, 10, ""),
            (993, 11, ""),
            (1883, 20, ""),
            (2897, 21, ""),
            (3329, 1, "36"),
            (4941, 1, "01"),
            (9494, 125, ""),
            (10457, 1, "d1"),
            (10626, 5, ""),
            (10670, 5, ""),
            (10776, 5, ""),
            (10909, 5, ""),
        ],
        (17482, 0xfbe8_ca63_4d9a_b988),
    ),
    (
        "carried_attempt_plan_moved",
        "arrival_save_after_start",
        (16885, 0xde81_027e_38b9_00e8),
        &[(9761, 1, "01")],
        (16885, 0xe6de_1bef_f85e_ee75),
    ),
    (
        "carried_birth_plan_moved",
        "arrival_save_after_start",
        (16885, 0xde81_027e_38b9_00e8),
        &[(9785, 1, "01")],
        (16885, 0x30ef_7b82_d8fe_92c5),
    ),
    (
        "carried_separation_plan_moved",
        "arrival_save_after_start",
        (16885, 0xde81_027e_38b9_00e8),
        &[(9893, 1, "01")],
        (16885, 0xfecf_a896_03ce_0ab5),
    ),
    (
        "carried_leave_home_plan_moved",
        "arrival_save_after_start",
        (16885, 0xde81_027e_38b9_00e8),
        &[(9917, 1, "01")],
        (16885, 0xd7a2_d7e9_2fb8_24fa),
    ),
    (
        "carried_plan_planned_later",
        "arrival_save_end",
        (29105, 0xb5d5_8c3d_0152_865c),
        &[(21707, 1, "82")],
        (29105, 0xceb4_5ba5_97a7_6f24),
    ),
    (
        "carried_plan_by_other_member",
        "arrival_save_after_start",
        (16885, 0xde81_027e_38b9_00e8),
        &[(9960, 1, "24")],
        (16885, 0xab46_9d28_e402_f996),
    ),
    (
        "carried_plans_swapped",
        "arrival_save_after_start",
        (16885, 0xde81_027e_38b9_00e8),
        &[(70, 1, "0b"), (1972, 1, "0a")],
        (16885, 0x634d_d195_cd1b_f76b),
    ),
    (
        "pool_child_of_a_cancelled_pregnancy",
        "V6_pool_pregnancy_cancelled",
        (12822, 0xfbd4_f970_3432_3d5b),
        &[
            (31, 4, "20061399"),
            (354, 0, "0090bc8d2100a201011f"),
            (807, 0, "0090c4fd9e0400a801071f"),
            (
                1291,
                0,
                "beb187f90b00a501021fd49ae3bbeacfeb962f00e4f3ffb90c00a601021ff3e1e3d4baa3e9c22c00",
            ),
            (1538, 0, "00badec6ae1300a301021fa0bb8ca1c58498a01c"),
            (1935, 0, "eed9a382930100a701021fdb86dcd7e8e9bbfac90100"),
            (2319, 0, "ccabc0c9e00c00a401021f85d2bc82f8b2ead44d00"),
            (2481, 3, "a9011d"),
            (
                7239,
                0,
                "1f90b48e03cebbc0d298e4dfdcb80101859680d5a184f0dd2f80c6d3808e8eb480da018780a38e80dcea0982858086de8290f929c0e69e80d88fafc02f81c0bb9780a4ca89e0010101000003d49ae3bbeacfeb962f0595c306608247400100000000000000000190b48e03f3e1e3d4baa3e9c22c27e19796a74e47400100000000000000000190b48e03db86dcd7e8e9bbfac90100000000000000000100000000000000000190b48e030000000f00011a000001cebbc0d298e4dfdcb8010090b48e0300018c8dc8be98e1f8837e01b4b68b96ebf1fbe1a60100",
            ),
            (7702, 1, "01"),
            (7766, 1, "99"),
            (7835, 0, "3b90b48e03"),
            (7889, 0, "3b90b48e03"),
            (7941, 0, "3b90b48e033b90b48e03"),
            (7967, 0, "3b90b48e03"),
            (8009, 0, "3b90b48e03"),
            (8047, 0, "3b90b48e03"),
            (8063, 1, "3d"),
        ],
        (13199, 0x3aa7_5f14_0ebd_3ed5),
    ),
];

/// "arrival" at the first month start: woman 13 conceived in her carried series.
const MONTH_1: i64 = 2_419_200;
/// "arrival" when woman 45 conceived at the start of her carried series.
const V4B_AT: i64 = 21_772_800;
/// "arrival" when the pool twins of household 31 are born.
const V5_TWINS_AT: i64 = 7_257_600;

const V3_SAVE: (usize, u64) = (22466, 0x6dc9_69c0_4742_1a73);
const V4_SAVE: (usize, u64) = (17336, 0x1115_0bb1_cc14_8bf9);
const V4B_SAVE: (usize, u64) = (18639, 0x4a27_0aa4_283a_64a2);
const V5_TWINS_SAVE: (usize, u64) = (17689, 0x281d_6851_e4ce_15d4);
const V6_SAVE: (usize, u64) = (12822, 0xfbd4_f970_3432_3d5b);

/// The reference's round 9 (out9, its notes' section 5g).
const LOAD_REJECTED_ROUND_9: [Hunks; 5] = [
    (
        "immigration_period_not_a_year",
        "load_base",
        (21141, 0x0084_8193_99de_8dc7),
        &[(45, 1, "82")],
        (21141, 0x819e_f00e_1c7b_d740),
    ),
    (
        "immigration_not_due",
        "load_base",
        (21141, 0x0084_8193_99de_8dc7),
        &[
            (36, 13, ""),
            (878, 0, "018088ff1d80a0fc7700030901"),
            (13256, 2, ""),
            (13731, 0, "0100"),
        ],
        (21141, 0x752c_3799_cd5d_d12c),
    ),
    (
        "immigration_persons_off_by_one",
        "load_base",
        (21141, 0x0084_8193_99de_8dc7),
        &[(43, 1, "02")],
        (21141, 0xff45_c538_bf21_2690),
    ),
    (
        "pool_mother_conceptions_off_by_one",
        "arrival_save_after_start",
        (16885, 0xde81_027e_38b9_00e8),
        &[(7033, 1, "02")],
        (16885, 0x6945_a87b_b424_ae4a),
    ),
    (
        "series_end_boundary_attempt_present",
        "series_end_boundary",
        (17320, 0x16d2_41d0_e0c9_ce6d),
        &[
            (34, 1, "cd"),
            (89, 0, "80d0ce040066031c0180a8a702"),
            (10155, 7, "cd0146000100050036"),
        ],
        (17335, 0xb101_9c2c_24b2_e7b9),
    ),
];

const SERIES_END_POOL: (usize, u64) = (10967, 0x686e_7564_7811_8471);
const SERIES_END_SAVE: (usize, u64) = (17320, 0x16d2_41d0_e0c9_ce6d);
const IMMIGRANT_CHILD_YEAR_HASHES: [u64; 2] = [0xb2d2_7171_a721_2f3c, 0x2c4f_7c6b_28a4_711a];
const IMMIGRANT_CHILD_LOG_HASH: u64 = 0x989a_b950_12f1_4d5f;
const IMMIGRANT_CHILD_STATE_HASH: u64 = 0xa96c_b756_d1cb_dcdb;
const IMMIGRANT_CHILD_SAVE: (usize, u64) = (20476, 0x4f89_03ce_deb4_ba40);

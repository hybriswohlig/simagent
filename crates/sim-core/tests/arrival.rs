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
//! All on the template of map 4 (`tests/golden/map4_template`) with the repository's life
//! course data, the switch on, the founder tag `test:arrival`, no founders:
//! - "arrival": `pool_c` (43 households, 75 members), seed 0xff, `n_start` 50, so one
//!   person a year (`q` = 1), to the end of year 13; the immigration of year 14 finds the
//!   pool empty. Its marks: the immigration of years 0 and 1, the birth of a pool child
//!   whose father is a record only, the first child of two pool persons born in the town,
//!   a separation carried from the pool, the death of an immigrant of year 1.
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

use std::collections::BTreeSet;

use serde_json::Value;
use sim_core::data::{Data, Definitions};
use sim_core::data_id::DataId;
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

/// The calibration and the genome map of the template of map 4.
fn template() -> (Value, Value) {
    common::files_from(
        include_str!("../../../tests/golden/map4_template/calibration/genetics.json"),
        include_str!("../../../tests/golden/map4_template/genetics/genome_map.json"),
    )
}

/// The template's map without one locus.
fn without_locus(locus: &str) -> (Value, Value) {
    let (calibration, mut genome_map) = template();
    genome_map["loci"]
        .as_array_mut()
        .expect("loci")
        .retain(|entry| entry["id"] != locus);
    (calibration, genome_map)
}

fn definitions((calibration, genome_map): (Value, Value)) -> Definitions {
    common::definitions(calibration, genome_map).expect("definitions")
}

/// The template with the pool of these bytes.
fn town_data(pool: &[u8]) -> Data {
    Data::with_pool(definitions(template()), pool).expect("checked")
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
/// draw for occasion 0 until they bring `n_start` persons.
fn pool_start(data: &Data, seed: u64, n_start: u64, immigration: bool) -> PoolStart {
    let pool = data.pool().expect("a pool");
    let (taken, _) = take_households(pool.pool(), seed, 0, &BTreeSet::new(), n_start);
    PoolStart {
        pool_hash: pool.hash(),
        n_start,
        immigration,
        households: taken
            .iter()
            .map(|&index| pool.pool().households[index].clone())
            .collect(),
    }
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
    data: || town_data(POOL_C),
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
    data: || town_data(POOL_D),
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
    data: || town_data(POOL_D),
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

/// ADR-0020 section 5: the data hash covers the pool; each golden pool fits the template.
#[test]
fn the_data_hash_covers_the_pool() {
    assert_eq!(
        Data::new(definitions(template())).expect("checked").hash(),
        TEMPLATE_DATA_HASH
    );
    let pools = [
        ("pool_a", POOL_A),
        ("pool_b", POOL_B),
        ("pool_c", POOL_C),
        ("pool_d", POOL_D),
        ("pool_e", POOL_E),
    ];
    for ((name, data_hash, pool_hash, households, members), (file, bytes)) in
        DATA_WITH_POOL.iter().zip(pools)
    {
        assert_eq!(*name, file);
        let data = town_data(bytes);
        assert_eq!(data.hash(), *data_hash, "{name}");
        let pool = data.pool().expect("a pool");
        assert_eq!(pool.hash(), *pool_hash, "{name}");
        assert_eq!(pool.pool().households.len(), *households, "{name}");
        let in_households: usize = pool.pool().households.iter().map(|h| h.members.len()).sum();
        assert_eq!(in_households, *members, "{name}");
    }
}

/// The data check of a pool (design note, section 2), in its order: every carer is a
/// member of its household, every genome fits its sex, the seven loci are in the map.
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
    let cases: [(Vec<u8>, (Value, Value)); 8] = [
        (changed_pool_c(carer), template()),
        (changed_pool_c(sex), template()),
        (changed_pool_c(unborn_sex), template()),
        (POOL_C.to_vec(), without_locus("core:mc1r")),
        (POOL_C.to_vec(), without_locus("core:herc2")),
        (
            changed_pool_c(|pool| {
                carer(pool);
                sex(pool);
            }),
            template(),
        ),
        (changed_pool_c(sex), without_locus("core:mc1r")),
        (changed_pool_c(carer), without_locus("core:herc2")),
    ];
    for ((name, code, file), (bytes, documents)) in DATA_REJECTED.iter().zip(cases) {
        assert_eq!(sized(&bytes), *file, "{name}");
        let error = Data::with_pool(definitions(documents), &bytes).expect_err(name);
        assert_eq!(error.to_string(), *code, "{name}");
    }
    for ((name, rule), (file, bytes)) in DATA_REJECTED_READER.iter().zip(READER_REJECTED) {
        assert_eq!(*name, file);
        let error = Data::with_pool(definitions(template()), bytes).expect_err(name);
        assert_eq!(error.to_string(), format!("pool.{rule}"), "{name}");
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
    let data = || town_data(POOL_C);
    let plain = || Data::new(definitions(template())).expect("checked");
    let other = || town_data(POOL_B);
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
        let error = Data::with_pool(definitions(template()), &bytes).expect_err(name);
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
    let data = town_data(POOL_B);
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
            World::load(&bytes, town_data(POOL_C)).err(),
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
            World::new(town_data(POOL_C)).save(&[]).expect("saves"),
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
    town_data(&pool)
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
    data: || town_data(POOL_C),
    seed: 0xff,
    n_start: 50,
    clock: 0,
};

/// "arrival" with the seed 3: woman 40 (household 41) fails the attempt of her carried
/// series at the clock, and the next would fall after her fertility: none is pending.
const V3: Town = Town {
    data: || town_data(POOL_C),
    seed: 3,
    n_start: 50,
    clock: 140_313_600,
};

/// `pool_d` with the seed 0xdb: the pool mother 27 dies before her pool birth.
const V6: Town = Town {
    data: || town_data(POOL_D),
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
    town_data(&pool)
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
    data: || town_data(POOL_C),
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
/// 88 and a man of 90. The woman of the couple is pregnant, the old ones have died.
fn founders_save() -> Vec<u8> {
    let data = founders_data();
    assert_eq!(data.hash(), FOUNDERS_DATA_HASH);
    let founder = |born: i64, sex: Sex| Founder {
        born: SimTime::from_secs(born),
        sex,
        alleles: Vec::new(),
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
    let (world, years) = World::load(&bytes, town_data(POOL_C)).expect("loads");
    let mut resumed = Run {
        world,
        out: YearOut {
            census: Vec::new(),
            year_hashes: years,
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
/// start after it, not after the pause of the birth. "arrival" with the seed 0xef: woman 27
/// (single, her pool pregnancy's father a record only) gives birth at `TIE_AT` and the
/// round of that second pairs her with man 26. Both saves load; the save with her series
/// after the pause does not.
#[test]
fn a_birth_and_a_market_pairing_in_one_second_start_the_series_at_the_pairing() {
    let data = town_data(POOL_C);
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
        World::load(&counted, town_data(POOL_C)).err(),
        Some(LoadError::Inconsistent)
    );
    run.play(TIE_FIRST_ATTEMPT - 1);
    assert_eq!(sized(&run.save()), TIE_SAVE_BEFORE_ATTEMPT);
    run.assert_loads();
}

// ---- generated by gen/arrival_pins.py from the reference's output ----
const TEMPLATE_DATA_HASH: u64 = 0x5ae3_d70c_71cf_063a;
/// (pool, data hash with it, pool hash, households, members)
const DATA_WITH_POOL: [(&str, u64, u64, usize, usize); 5] = [
    (
        "pool_a",
        0x797a_2b4f_964e_e9f2,
        0x6488_c5c7_c9a7_40ef,
        26,
        42,
    ),
    (
        "pool_b",
        0xe3e1_23a2_16aa_dc64,
        0x28af_470e_4c30_38f4,
        29,
        33,
    ),
    (
        "pool_c",
        0x6cac_772d_1146_140e,
        0xe4a7_3ea3_34e7_4b6c,
        43,
        75,
    ),
    (
        "pool_d",
        0x0d74_d26f_06a7_0d50,
        0x979f_8665_a74a_4b1e,
        26,
        42,
    ),
    (
        "pool_e",
        0x40c9_e268_83b7_1372,
        0x4556_05ea_f7c2_5849,
        20,
        35,
    ),
];
/// (case, reject code, data hash, the input record as (length, xxh3))
const START_REJECTED: [(&str, u16, u64, (usize, u64)); 17] = [
    (
        "founders_with_pool",
        24,
        0x6cac_772d_1146_140e,
        (6739, 0x8558_6108_7c9a_dc69),
    ),
    (
        "founders_and_couple_with_pool",
        24,
        0x6cac_772d_1146_140e,
        (6748, 0x53e1_b878_6968_eaaa),
    ),
    (
        "no_pool_in_the_data",
        25,
        0x5ae3_d70c_71cf_063a,
        (6732, 0xe05c_ddfa_b1af_5a56),
    ),
    (
        "pool_hash_not_the_data_pool",
        25,
        0x6cac_772d_1146_140e,
        (6732, 0xecc4_96b7_b2ce_3cf5),
    ),
    (
        "pool_of_other_data",
        25,
        0xe3e1_23a2_16aa_dc64,
        (6732, 0xe05c_ddfa_b1af_5a56),
    ),
    (
        "n_start_0_no_households",
        26,
        0x6cac_772d_1146_140e,
        (37, 0xdbfe_dcce_76eb_496e),
    ),
    (
        "n_start_0_with_households",
        26,
        0x6cac_772d_1146_140e,
        (6732, 0x57e7_853c_ec3c_f7aa),
    ),
    (
        "households_reversed",
        27,
        0x6cac_772d_1146_140e,
        (6732, 0x00ca_43ea_8bc6_8fb0),
    ),
    (
        "households_one_missing",
        27,
        0x6cac_772d_1146_140e,
        (6393, 0xbc53_bb3f_bb78_346d),
    ),
    (
        "households_one_extra",
        27,
        0x6cac_772d_1146_140e,
        (6832, 0xac39_2bbe_34c5_9891),
    ),
    (
        "households_of_another_seed",
        27,
        0x6cac_772d_1146_140e,
        (6469, 0xde34_c877_d2b1_39b4),
    ),
    (
        "households_one_altered",
        27,
        0x6cac_772d_1146_140e,
        (6732, 0xe527_afaf_4587_6376),
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
        (6732, 0x57e7_853c_ec3c_f7aa),
    ),
    (
        "order_n_start_before_households",
        26,
        0x6cac_772d_1146_140e,
        (6732, 0x390f_7530_d11a_bb41),
    ),
    (
        "decision_capacity_zero_before_pool",
        23,
        0x6cac_772d_1146_140e,
        (6724, 0x0028_066a_825f_c2c6),
    ),
    (
        "decision_couple_checks_before_pool",
        13,
        0x6cac_772d_1146_140e,
        (6734, 0x0f33_cb57_4896_65f5),
    ),
];
const START_ACCEPTED: (usize, u64) = (6732, 0xe05c_ddfa_b1af_5a56);
/// (case, data check, the pool file as (length, xxh3))
const DATA_REJECTED: [(&str, &str, (usize, u64)); 8] = [
    (
        "carer_not_member",
        "pool.carer_not_member",
        (10956, 0x314d_3640_9da3_03fa),
    ),
    (
        "sex_of_a_member",
        "pool.sex",
        (10957, 0x32d2_1e28_fad9_d7b8),
    ),
    (
        "sex_of_an_unborn_child",
        "pool.sex",
        (10957, 0x51c5_8bdb_b209_17d4),
    ),
    (
        "locus_mc1r_missing",
        "pool.locus",
        (10957, 0xe4a7_3ea3_34e7_4b6c),
    ),
    (
        "locus_herc2_missing",
        "pool.locus",
        (10957, 0xe4a7_3ea3_34e7_4b6c),
    ),
    (
        "order_carer_before_sex",
        "pool.carer_not_member",
        (10956, 0xd6b6_8eb2_be77_9cf5),
    ),
    (
        "order_sex_before_locus",
        "pool.sex",
        (10957, 0x32d2_1e28_fad9_d7b8),
    ),
    (
        "order_carer_before_locus",
        "pool.carer_not_member",
        (10956, 0x314d_3640_9da3_03fa),
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
const CARRIED_BEFORE_POOL: (usize, u64) = (10956, 0x7a8b_0ba8_9d1a_55eb);
const LOAD_BASE_AT: i64 = 94348799;
const LOAD_BASE: (usize, u64) = (20695, 0xf509_3f7b_da81_7ed0);
const LOAD_REJECTED: [Patch; 8] = [
    (
        "arrival_born_off_by_one",
        10111,
        10583,
        "fd",
        (20695, 0xfeea_83a5_984e_6231),
    ),
    (
        "used_household_twice",
        12834,
        7828,
        "210e27060903001b0b192a1f071721140d261816231c1a11100f2013251e02050c0e",
        (20696, 0x9c8e_bd88_1fde_778f),
    ),
    (
        "pool_child_arrived_not_born",
        10313,
        10381,
        "82",
        (20695, 0xd95f_4ffb_ec34_e391),
    ),
    (
        "arrival_not_at_a_year_start",
        11244,
        9352,
        "fdd3d0ea0bd7eac2caadb9a5bd0700e08380fcf180a0b124e0e9c8808e809b906080c09a8c80c09e0c829580f0a78498ab03a0b28880eeeda190c1018180c481809c8b816000000000000000001f0100000001d7eac2caadb9a5bd0701ffcbd1cc0b82",
        (20695, 0x19f7_3e82_1a97_1814),
    ),
    (
        "arrival_year_not_its_year",
        11346,
        9348,
        "02",
        (20695, 0x2608_2f34_a4b9_c5b6),
    ),
    (
        "town_pregnancy_without_father",
        12434,
        8259,
        "00",
        (20694, 0x4566_5aa6_7c62_0e3e),
    ),
    (
        "arrival_partnership_formed_off",
        12182,
        8512,
        "fd",
        (20695, 0x48a8_4081_be0b_e504),
    ),
    (
        "arrival_without_pool",
        10195,
        10480,
        "",
        (20675, 0xc406_b9d1_1c10_d536),
    ),
];
// arrival
const ARRIVAL_DATA_HASH: u64 = 0x6cac_772d_1146_140e;
const ARRIVAL_POOL_HASH: u64 = 0xe4a7_3ea3_34e7_4b6c;
const ARRIVAL_START_INPUT: (usize, u64) = (6732, 0xe05c_ddfa_b1af_5a56);
const ARRIVAL_YEAR_HASHES: [u64; 14] = [
    0xe6fc_d8a2_5e63_7170,
    0x2688_7bfd_43e3_fa4c,
    0xf8e2_ee9e_e6ed_218f,
    0x212b_119a_5011_34a5,
    0x93eb_da07_339e_f1cd,
    0x42a6_3c20_ed9c_ca9c,
    0xbbf2_1e73_068a_cc31,
    0x6ba9_5cd7_268b_258e,
    0xfa06_2f10_34e5_5208,
    0xac12_fbcf_a86e_9832,
    0xad9a_af8b_c14d_e4a6,
    0xf5d8_d634_a181_bd9b,
    0x04dd_9689_77fd_0746,
    0xdf24_b7b8_3bae_38ad,
];
const ARRIVAL_LOG: (usize, u64) = (1503, 0x3811_f843_0deb_22ef);
/// The reference prints `arrival_state_hash` after its attempt at the immigration of year
/// 14, which leaves its world changed; this value is its model's state hash at the end of
/// year 13 (`arrival_world()` advanced to `14 · YEAR − 1`), the projection in its end save.
const ARRIVAL_STATE_HASH: u64 = 0xb6e6_e68c_5de8_18d7;
const ARRIVAL_END: i64 = 440294399;
const ARRIVAL_SAVE_AFTER_START: (usize, u64) = (16887, 0xc681_ab82_572f_2171);
const ARRIVAL_SAVE_END: (usize, u64) = (28063, 0x3cba_e9e9_5c2e_a6f0);
const ARRIVAL_PEDIGREE: (usize, u64) = (495997, 0x6dc7_2e77_d282_2e90);
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
        (16887, 0xc681_ab82_572f_2171),
        (17057, 0xd8a7_8856_6bfb_c57f),
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
        (18125, 0x9e6a_8397_6779_8083),
        (18292, 0xe162_57b4_e6db_e086),
        &[
            (
                "7b80f0eb0d02003b002402014400030e636f72653a676573746174696f6e08bec2c4bfb28d9fcca201",
                None,
            ),
            (
                "7c80f0eb0d01003b02014400030e636f72653a686f757365686f6c640b15",
                Some(
                    r#"{"id":124,"t":14515200,"subjects":[{"sim":59}],"causes":[{"plan":{"by":68,"planned_at":0}},{"rule":"core:household"}],"payload":{"core:moved":{"household":21}}}"#,
                ),
            ),
            (
                "7d80f0eb0d010027010148000201",
                Some(
                    r#"{"id":125,"t":14515200,"subjects":[{"sim":39}],"causes":[{"plan":{"by":72,"planned_at":0}}],"payload":{"core:birthday":{"age_years":1}}}"#,
                ),
            ),
            (
                "7e80f0eb0d010028010149000201",
                Some(
                    r#"{"id":126,"t":14515200,"subjects":[{"sim":40}],"causes":[{"plan":{"by":73,"planned_at":0}}],"payload":{"core:birthday":{"age_years":1}}}"#,
                ),
            ),
        ],
    ),
    (
        "town_child_of_pool_persons",
        26611200,
        (18585, 0x75c5_c8fd_3175_8108),
        (18685, 0x672e_eb7b_fde7_e4d1),
        &[
            (
                "b00180b8b01903003c000d000c02016580a8a702030e636f72653a676573746174696f6e08b49498f0bdf2d0a517",
                None,
            ),
            (
                "b10180b8b01901003c02016580a8a702030e636f72653a686f757365686f6c640b0a",
                Some(
                    r#"{"id":177,"t":26611200,"subjects":[{"sim":60}],"causes":[{"plan":{"by":101,"planned_at":2419200}},{"rule":"core:household"}],"payload":{"core:moved":{"household":10}}}"#,
                ),
            ),
        ],
    ),
    (
        "immigration_1",
        31449600,
        (18696, 0x97c4_0105_d1fa_0903),
        (18883, 0x46dc_09da_4ae3_a196),
        &[
            (
                "b2018088ff1d01003d010101000dd7eac2caadb9a5bd07",
                Some(
                    r#"{"id":178,"t":31449600,"subjects":[{"sim":61}],"causes":[{"plan":{"by":1,"planned_at":0}}],"payload":{"core:arrived":{"lineage_key":"077a95cad950b557"}}}"#,
                ),
            ),
            (
                "b3018088ff1d01003d02010100030e636f72653a686f757365686f6c640a1f",
                Some(
                    r#"{"id":179,"t":31449600,"subjects":[{"sim":61}],"causes":[{"plan":{"by":1,"planned_at":0}},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":31}}}"#,
                ),
            ),
            (
                "b4018088ff1d010016010129000201",
                Some(
                    r#"{"id":180,"t":31449600,"subjects":[{"sim":22}],"causes":[{"plan":{"by":41,"planned_at":0}}],"payload":{"core:birthday":{"age_years":1}}}"#,
                ),
            ),
        ],
    ),
    (
        "carried_separation",
        84829122,
        (20474, 0x14a2_0d95_38fa_09f1),
        (20454, 0xfc36_aee0_c79f_ded2),
        &[
            (
                "dc02848ff350020033003202015e0004128ec1c0dde3ddd8da940100e48ed2e6ecad8cc8a501090f",
                Some(
                    r#"{"id":348,"t":84829122,"subjects":[{"sim":51},{"sim":50}],"causes":[{"plan":{"by":94,"planned_at":0}},{"random":{"tag":"0000000000000012","subject":"94b562ee3bb0208e","occasion":"0000000000000000","detail":"a590316eccd48764"}}],"payload":{"core:separated":{"partnership":15}}}"#,
                ),
            ),
            (
                "dd02848ff3500100320200dc02030e636f72653a686f757365686f6c640a22",
                Some(
                    r#"{"id":349,"t":84829122,"subjects":[{"sim":50}],"causes":[{"event":348},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":34}}}"#,
                ),
            ),
            (
                "de02848ff3500100320200dc02030e636f72653a686f757365686f6c640b22",
                Some(
                    r#"{"id":350,"t":84829122,"subjects":[{"sim":50}],"causes":[{"event":348},{"rule":"core:household"}],"payload":{"core:moved":{"household":34}}}"#,
                ),
            ),
        ],
    ),
    (
        "immigrant_death",
        183152461,
        (24092, 0x53e4_8320_de7f_c98c),
        (24041, 0x4fe0_1ee6_0b28_923c),
        &[(
            "97059abdd5ae0101003d0201b2018088ff1d0404d7eac2caadb9a5bd07a0bb8ca1c58498a01c01040f636f72653a73656e657363656e6365",
            Some(
                r#"{"id":663,"t":183152461,"subjects":[{"sim":61}],"causes":[{"plan":{"by":178,"planned_at":31449600}},{"random":{"tag":"0000000000000004","subject":"077a95cad950b557","occasion":"1c40602454231da0","detail":"0000000000000001"}}],"payload":{"core:death":{"cause":"core:senescence"}}}"#,
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
const ORPHANS_DATA_HASH: u64 = 0x0d74_d26f_06a7_0d50;
const ORPHANS_POOL_HASH: u64 = 0x979f_8665_a74a_4b1e;
const ORPHANS_START_INPUT: (usize, u64) = (5022, 0x926a_6d76_6745_ac26);
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
const ORPHANS_SAVE_AFTER_START: (usize, u64) = (13673, 0xdca7_508f_452d_ce8d);
const ORPHANS_SAVE_END: (usize, u64) = (14314, 0x6153_a5c1_c385_e271);
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
        (13783, 0x5dd6_098a_410f_d7fa),
        (13663, 0xce8e_446d_c7a8_5859),
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
        (14002, 0x3411_b787_9313_dae3),
        (13908, 0x3ec6_eae7_c175_ffec),
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
        (14363, 0xf8fc_6a5f_e2c0_7ea9),
        (14259, 0xf97b_ccfb_aba6_48c4),
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
const RECORDS_DATA_HASH: u64 = 0x0d74_d26f_06a7_0d50;
const RECORDS_POOL_HASH: u64 = 0x979f_8665_a74a_4b1e;
const RECORDS_START_INPUT: (usize, u64) = (4532, 0xdf9b_4309_6f51_ea70);
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
const RECORDS_SAVE_AFTER_START: (usize, u64) = (12637, 0x3d72_d44b_6784_e08d);
const RECORDS_SAVE_END: (usize, u64) = (13808, 0xe7da_73ff_f19f_4765);
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
        (12895, 0xdb02_b60d_031c_76cc),
        (12835, 0xb9fe_3e92_e8db_7f73),
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
        (13693, 0x30a7_f86b_c479_b105),
        (13797, 0xeef6_1c75_ccdb_5e8e),
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
const MICRO_POOL: (usize, u64) = (10956, 0x6dc0_8974_07ee_7772);
const RECORDS_ARRANGE: ((usize, u64), (usize, u64), &[Entry]) = (
    (13540, 0x27cf_8d0d_1cd1_24f9),
    (13615, 0x5843_6275_9083_920f),
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
const EMPTY_DATA_HASH: u64 = 0xe3e1_23a2_16aa_dc64;
const EMPTY_START_INPUT: (usize, u64) = (3631, 0x38d8_9f9c_1b64_328b);
const EMPTY_SAVE_AFTER_START: (usize, u64) = (9381, 0xad19_0663_d479_5596);
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
        (16887, 0xc681_ab82_572f_2171),
        &[(9110, 2, "00")],
        (16886, 0x880e_09f7_d713_c0cc),
    ),
    (
        "town_series_missing",
        "records_save_after_arrange",
        (13615, 0x5843_6275_9083_920f),
        &[(33, 1, "6c"), (102, 15, ""), (8400, 1, "6c"), (8437, 7, "")],
        (13593, 0xd419_8d99_e36e_94d7),
    ),
    (
        "immigration_order_swapped",
        "load_base",
        (20695, 0xf509_3f7b_da81_7ed0),
        &[(12865, 0, "0c"), (12866, 1, "")],
        (20695, 0x64da_c046_63a5_8eb8),
    ),
    (
        "unstarted_with_used_household",
        "unstarted",
        (38, 0xcdd2_3751_61ca_9734),
        &[(33, 0, "01")],
        (39, 0x8e1b_5a81_15f5_02e7),
    ),
];
const LEAVE_AT_START_POOL: (usize, u64) = (10956, 0x781d_b8c6_6891_9f33);
/// The tie of a birth and a pairing of the market in one second (review): "arrival" with the
/// seed 0xef; woman 27 gives birth at `TIE_AT` and the round of that second pairs her.
const TIE_SEED: u64 = 0xef;
const TIE_AT: i64 = 14_515_200;
/// Her first attempt: the first month start after the pairing, not after the pause.
const TIE_FIRST_ATTEMPT: i64 = 16_934_400;
const TIE_START_INPUT: (usize, u64) = (6484, 0xd1e3_00db_46dd_4dbd);
const TIE_SAVE_AT: (usize, u64) = (17388, 0x3df1_9f9f_fb6c_75d2);
const TIE_SAVE_BEFORE_ATTEMPT: (usize, u64) = (17579, 0xe8b3_eb1f_9f4d_e1df);
/// The save at `TIE_AT` with her series moved to after the pause of the birth.
const TIE_COUNTED_BIRTH: (&[Hunk], (usize, u64)) = (
    &[
        (489, 14, ""),
        (693, 0, "0180a8a70280d0c32900e401031b"),
        (10478, 5, ""),
        (10538, 0, "6f80f0eb0d"),
    ],
    (17388, 0xa9c6_8a19_3bd2_3aa4),
);

/// The reference's states changed in one field (out4, its notes' section 5b).
const LOAD_REJECTED_FIELDS: [Hunks; 32] = [
    (
        "arrival_arrived_one_second_late",
        "orphans_save_end",
        (14314, 0x6153_a5c1_c385_e271),
        &[(8513, 1, "02")],
        (14314, 0xabcf_4306_e2f8_2631),
    ),
    (
        "arrival_year_off_by_one",
        "orphans_save_end",
        (14314, 0x6153_a5c1_c385_e271),
        &[(8514, 1, "01")],
        (14314, 0x8fce_e314_4261_283c),
    ),
    (
        "arrival_born_pool_changed",
        "orphans_save_end",
        (14314, 0x6153_a5c1_c385_e271),
        &[(4372, 1, "82")],
        (14314, 0x100f_9c3a_8b9f_e253),
    ),
    (
        "arrival_genome_key_changed",
        "orphans_save_end",
        (14314, 0x6153_a5c1_c385_e271),
        &[(4362, 1, "c8")],
        (14314, 0xa08e_8bba_a00e_36ea),
    ),
    (
        "arrival_pool_mother_changed",
        "orphans_save_end",
        (14314, 0x6153_a5c1_c385_e271),
        &[(4380, 9, "caeea683b29ea549")],
        (14313, 0x6719_5495_3d69_3a6e),
    ),
    (
        "arrival_pool_father_changed",
        "orphans_save_end",
        (14314, 0x6153_a5c1_c385_e271),
        &[(4390, 10, "ddd0b2b8c5fbd04b")],
        (14312, 0x0bc3_d6ac_02e3_2427),
    ),
    (
        "arrival_pool_social_parents_changed",
        "orphans_save_end",
        (14314, 0x6153_a5c1_c385_e271),
        &[(4400, 1, "01caeea683b29ea549")],
        (14322, 0xd901_f117_3eef_33fa),
    ),
    (
        "arrival_sex_flipped",
        "orphans_save_end",
        (14314, 0x6153_a5c1_c385_e271),
        &[(8371, 1, "00")],
        (14314, 0xe864_6815_98ed_d2a8),
    ),
    (
        "arrival_genome_changed",
        "orphans_save_end",
        (14314, 0x6153_a5c1_c385_e271),
        &[(4244, 1, "9a")],
        (14314, 0x7551_ad19_fa41_3c72),
    ),
    (
        "arrival_town_parents_set",
        "orphans_save_end",
        (14314, 0x6153_a5c1_c385_e271),
        &[(4358, 0, "011615")],
        (14317, 0x76aa_64d6_1a04_b9e1),
    ),
    (
        "arrival_household_moved_to_another_year",
        "orphans_save_end",
        (14314, 0x6153_a5c1_c385_e271),
        &[
            (112, 19, ""),
            (600, 0, "92e3d6fd02001502049aada6adcbb284bf7200"),
            (682, 39, ""),
            (761, 19, ""),
            (849, 19, ""),
            (
                925,
                0,
                "90c4818e0800130204a0bb8ca1c58498a01c00a0a6feca08001b0205d49ae3bbeacfeb962f00",
            ),
            (943, 0, "0096c18d9f0a00160204d49ae3bbeacfeb962f"),
            (964, 0, "00c2b1a8c40a00190205a0bb8ca1c58498a01c"),
            (1003, 19, ""),
            (1111, 0, "00f4f5cefe0e001c0205db86dcd7e8e9bbfac901"),
            (1350, 10, ""),
            (1379, 0, "00d0e2fdd51500110501"),
            (1521, 0, "b68caac22100170204db86dcd7e8e9bbfac90100"),
            (1661, 40, ""),
            (2042, 0, "92e5ae99fb0d0014020485d2bc82f8b2ead44d00"),
            (2123, 0, "bad6d3ca8315001a020585d2bc82f8b2ead44d00"),
            (2174, 20, ""),
            (2518, 4, "afc2f80c"),
            (2620, 1, "8088ff1d"),
            (2648, 1, "8088ff1d"),
            (2677, 1, "8088ff1d"),
            (2701, 2, "8088ff1d01"),
            (2727, 4, "c7e0e30c"),
            (2826, 1, "8088ff1d"),
            (2855, 1, "8088ff1d"),
            (2880, 2, "8088ff1d01"),
            (6338, 3, "c785ba"),
            (6443, 1, "8088ff1d"),
            (8817, 2, ""),
            (8989, 0, "088088ff1d"),
            (9002, 4, ""),
            (9019, 2, ""),
            (
                9033,
                21,
                "1f002f002400088088ff1d098088ff1d1900088088ff1d3900098088ff1db6018090f3602a",
            ),
            (9073, 0, "098088ff1d"),
            (9114, 7, "5880c0ba12088088ff1d"),
            (9146, 0, "088088ff1d"),
            (9164, 4, ""),
            (9214, 0, "088088ff1d"),
            (9222, 0, "098088ff1d"),
            (9231, 2, ""),
        ],
        (14368, 0xba89_96f8_6376_d382),
    ),
    (
        "pool_child_genome_key_changed",
        "arrival_save_after_pool_child_without_father",
        (18292, 0xe162_57b4_e6db_e086),
        &[(10445, 1, "bf")],
        (18292, 0x1bce_17d7_0eaa_5a99),
    ),
    (
        "pool_child_arrived_not_born",
        "arrival_save_after_pool_child_without_father",
        (18292, 0xe162_57b4_e6db_e086),
        &[(10456, 2, "feef")],
        (18292, 0xccc6_ff17_3351_56b5),
    ),
    (
        "pool_child_arrival_year_off_by_one",
        "arrival_save_after_pool_child_without_father",
        (18292, 0xe162_57b4_e6db_e086),
        &[(10460, 1, "01")],
        (18292, 0xcbe4_6ef5_8e3e_b8f2),
    ),
    (
        "pool_child_pool_mother_changed",
        "arrival_save_after_pool_child_without_father",
        (18292, 0xe162_57b4_e6db_e086),
        &[(10462, 9, "d7eac2caadb9a5bd07")],
        (18292, 0x98c5_0086_9150_51eb),
    ),
    (
        "pool_child_pool_father_changed",
        "arrival_save_after_pool_child_without_father",
        (18292, 0xe162_57b4_e6db_e086),
        &[(10472, 10, "b9b4d4cad0ccd1b808")],
        (18291, 0x9632_3464_81fe_7527),
    ),
    (
        "pool_child_social_parents_set",
        "arrival_save_after_pool_child_without_father",
        (18292, 0xe162_57b4_e6db_e086),
        &[(10482, 1, "01cd9dd6b0cadf9ece6a")],
        (18301, 0x235f_0036_f662_e6fa),
    ),
    (
        "pool_child_sex_flipped",
        "arrival_save_after_pool_child_without_father",
        (18292, 0xe162_57b4_e6db_e086),
        &[(10343, 1, "01")],
        (18292, 0x70a2_6311_ac96_63f5),
    ),
    (
        "pool_child_genome_changed",
        "arrival_save_after_pool_child_without_father",
        (18292, 0xe162_57b4_e6db_e086),
        &[(10355, 1, "94")],
        (18292, 0xdfdc_45a5_1b57_97ff),
    ),
    (
        "pool_child_town_parents_set",
        "arrival_save_after_pool_child_without_father",
        (18292, 0xe162_57b4_e6db_e086),
        &[(10442, 0, "012401")],
        (18295, 0x0ca1_ebfb_3af8_b477),
    ),
    (
        "pool_child_born_after_the_clock",
        "arrival_save_after_pool_child_without_father",
        (18292, 0xe162_57b4_e6db_e086),
        &[
            (761, 1, "82"),
            (1065, 1, "82"),
            (1836, 1, "b2"),
            (2082, 1, "92"),
            (3168, 1, "d0"),
            (10329, 1, "82"),
            (10431, 1, "82"),
            (10456, 1, "82"),
        ],
        (18292, 0xd595_1aa9_889f_5e44),
    ),
    (
        "pool_child_mother_not_an_arrival",
        "arrival_save_after_pool_child_without_father",
        (18292, 0xe162_57b4_e6db_e086),
        &[(7574, 23, "001501"), (10935, 1, "1d"), (10956, 1, "")],
        (18271, 0x6389_8d5e_780d_0cc5),
    ),
    (
        "pool_child_mother_left_before_the_birth",
        "arrival_save_after_pool_child_without_father",
        (18292, 0xe162_57b4_e6db_e086),
        &[
            (33, 1, "e3"),
            (314, 10, ""),
            (1389, 20, ""),
            (3598, 24, "f5013a"),
            (7494, 107, ""),
            (10440, 2, "00"),
            (
                10934,
                0,
                "0124ffc3e1ce07feefeb0d0085d2bc82f8b2ead44dcd9dd6b0cadf9ece6a00809380ba8083a8b116a0b98280caab85a0870185c0c98a80c09188c001889b80d8cd81b0d52180f5d580ca848e806980c0fa8280e0a18aa00100000000000101011501000001cd9dd6b0cadf9ece6a01ffb3e3920700000000",
            ),
            (10966, 1, "e3"),
            (11029, 2, ""),
            (11274, 2, ""),
            (11554, 2, ""),
        ],
        (18247, 0x8f3e_a498_2728_a637),
    ),
    (
        "no_pool_living_with_pool_origin",
        "founders_world",
        (482, 0x7c06_f372_f88b_9308),
        &[(246, 0, "010001ffd7e6a90600000000")],
        (494, 0xa86f_cb5c_2b5d_7707),
    ),
    (
        "no_pool_archived_with_pool_origin",
        "founders_world",
        (482, 0x7c06_f372_f88b_9308),
        &[(350, 0, "010001ffbfadcf1400000000")],
        (494, 0xfb72_99fb_8ffe_4ccb),
    ),
    (
        "no_pool_used_household",
        "founders_world",
        (482, 0x7c06_f372_f88b_9308),
        &[(400, 0, "01")],
        (483, 0x4d56_0dd9_b372_1fa3),
    ),
    (
        "no_pool_pregnancy_without_father",
        "founders_world",
        (482, 0x7c06_f372_f88b_9308),
        &[(272, 2, "00")],
        (481, 0x7969_3b32_2aed_3d11),
    ),
    (
        "no_pool_pregnancy_with_pool_father",
        "founders_world",
        (482, 0x7c06_f372_f88b_9308),
        &[(274, 1, "01d388fe84eef9bce93c")],
        (491, 0xc553_2e6f_65d5_022b),
    ),
    (
        "arrival_social_parent_is_its_pool_parent",
        "orphans_save_end",
        (14314, 0x6153_a5c1_c385_e271),
        &[(4360, 1, "15")],
        (14314, 0x45ff_7a16_e3cf_33a6),
    ),
    (
        "arrival_partnership_close_kin",
        "orphans_save_end",
        (14314, 0x6153_a5c1_c385_e271),
        &[
            (31, 3, "0a1a8001"),
            (1290, 0, "baaea9961300db02050900"),
            (2224, 1, "dc"),
            (2471, 1, "19"),
            (3013, 2, "0219"),
            (6020, 1, "19"),
            (6140, 1, "19"),
            (6333, 1, "09"),
            (6428, 7, "090a0280a8fb9501001901000002000180a8fb9501"),
            (6469, 1, "0180a8fb9501"),
            (6549, 0, "01001980a8fb95"),
            (8778, 1, "8001"),
            (9101, 0, "800280a8fb9501"),
            (9236, 3, "860202"),
            (14261, 0, "fea7fb950102020a02"),
        ],
        (14369, 0xbc02_d85c_6ab7_25ec),
    ),
    (
        "arrival_moved_out_by_town_partnership_flag_false",
        "arrival_moved_out_by_town_partnership",
        (12985, 0x5f68_255f_c4db_f400),
        &[(3566, 1, "00")],
        (12985, 0xb773_8cce_1b84_e078),
    ),
    (
        "brought_partnership_member_not_moved_out_moved_out",
        "brought_partnership_member_not_moved_out",
        (16904, 0x4b80_af68_46e0_1c3d),
        &[(3286, 1, "01")],
        (16904, 0x457e_67a1_1b94_bf0e),
    ),
];

/// The town of four founders without a pool (`founders_world`).
const FOUNDERS_AT: i64 = 16934400;
const FOUNDERS_START_INPUT: (usize, u64) = (54, 0xa380_9ee9_8bdb_3d5a);
const FOUNDERS_DATA_HASH: u64 = 0x5ae3_d70c_71cf_063a;

/// The second of the mark "pool_child_without_father".
const POOL_CHILD_WITHOUT_FATHER_AT: i64 = 14515200;

/// "records" before its input: the market round of this second pairs arrival 7 (not moved
/// out in the pool, his leaving home still to come) with sim 11, which alone moves him out.
const RECORDS_MOVED_OUT_BY_TOWN_PARTNERSHIP_AT: i64 = 186278400;

/// The pool with a brought partnership's member not moved out, and its data.
const BROUGHT_NOT_MOVED_OUT_POOL: (usize, u64) = (10961, 0x8ee0_f947_b74c_76cf);
const BROUGHT_NOT_MOVED_OUT_DATA_HASH: u64 = 0xc8d7_d7ec_95e3_f2a2;

/// The reference's states of the carried plans (out6, its notes' section 5d).
const LOAD_REJECTED_CARRIED: [Hunks; 17] = [
    (
        "carried_series_one_month_late",
        "arrival_save_after_immigration_1",
        (18883, 0x46dc_09da_4ae3_a196),
        &[
            (52, 13, ""),
            (150, 0, "0180a8a70280d8cd22004b0315"),
            (11320, 2, ""),
            (11354, 0, "2800"),
        ],
        (18883, 0xc45e_8910_d969_7d9a),
    ),
    (
        "carried_series_before_its_start",
        "arrival_save_after_start",
        (16887, 0xc681_ab82_572f_2171),
        &[
            (42, 0, "00000a03030180a8a702"),
            (59, 13, ""),
            (9758, 5, "05005e0001"),
        ],
        (16884, 0xb7c9_a850_7b28_f55e),
    ),
    (
        "carried_series_missing",
        "arrival_save_after_immigration_1",
        (18883, 0x46dc_09da_4ae3_a196),
        &[(33, 1, "ec"), (52, 13, ""), (11311, 10, "ec017280a09d0901")],
        (18868, 0x103b_09d1_1433_24a1),
    ),
    (
        "carried_series_past_fertility",
        "V3_carried_series_ended",
        (21734, 0x33b4_3258_3764_9558),
        &[
            (33, 1, "aa"),
            (623, 0, "0180a8a70280b88f88010097010328"),
            (13983, 1, "aa"),
            (14298, 0, "4800"),
        ],
        (21751, 0xed51_8372_ef9d_8c56),
    ),
    (
        "carried_conception_before_its_series",
        "V4b_carried_conception_pregnant",
        (18581, 0x83a2_f784_3f4e_6ba6),
        &[
            (292, 0, "a202042d0080d0c32900"),
            (306, 10, ""),
            (11036, 3, "c0ba12"),
            (11162, 0, "ac0180c0ba12"),
            (11172, 6, ""),
        ],
        (18581, 0x840d_2a0a_47c6_d468),
    ),
    (
        "pool_pregnancy_dropped",
        "arrival_save_after_start",
        (16887, 0xc681_ab82_572f_2171),
        &[
            (30, 1, "c5"),
            (179, 10, ""),
            (9108, 1, "05"),
            (9544, 89, ""),
            (9754, 1, "c5"),
            (9783, 2, ""),
        ],
        (16786, 0x85ca_3f93_1fa4_36cc),
    ),
    (
        "pool_birth_vanished",
        "arrival_save_after_pool_child_without_father",
        (18292, 0xe162_57b4_e6db_e086),
        &[
            (30, 4, "3b101fe1"),
            (761, 10, ""),
            (1064, 11, ""),
            (1836, 20, ""),
            (2072, 20, ""),
            (3168, 21, ""),
            (3621, 1, "3a"),
            (7573, 1, "00"),
            (10328, 155, ""),
            (10966, 1, "e1"),
            (11169, 5, ""),
            (11234, 5, ""),
            (11324, 5, ""),
            (11362, 5, ""),
            (11507, 5, ""),
        ],
        (18030, 0x3ce0_9995_03d5_e3c3),
    ),
    (
        "pool_child_born_one_second_early",
        "arrival_save_after_pool_child_without_father",
        (18292, 0xe162_57b4_e6db_e086),
        &[
            (741, 0, "fef7ea2b00ee01013b00"),
            (760, 10, ""),
            (1065, 2, "fe8f"),
            (1836, 1, "ae"),
            (2082, 1, "8e"),
            (3168, 1, "cc"),
            (10329, 2, "feef"),
            (10431, 2, "feef"),
            (10456, 2, "feef"),
            (11159, 11, "7b80f0eb0d7d80f0eb0d7e"),
        ],
        (18292, 0x1222_4083_95bd_bf2f),
    ),
    (
        "pool_twin_missing",
        "V5_pool_twins_born",
        (17656, 0xf40a_6465_d36e_a861),
        &[
            (30, 4, "38101fd5"),
            (737, 10, ""),
            (957, 11, ""),
            (1846, 20, ""),
            (2969, 21, ""),
            (3401, 1, "37"),
            (5013, 1, "01"),
            (9703, 125, ""),
            (10419, 1, "d5"),
            (10588, 5, ""),
            (10630, 5, ""),
            (10736, 5, ""),
            (10876, 5, ""),
        ],
        (17449, 0xa83a_85b8_4190_22a7),
    ),
    (
        "carried_attempt_plan_moved",
        "arrival_save_after_start",
        (16887, 0xc681_ab82_572f_2171),
        &[(9763, 1, "01")],
        (16887, 0x363d_a074_2361_ed94),
    ),
    (
        "carried_birth_plan_moved",
        "arrival_save_after_start",
        (16887, 0xc681_ab82_572f_2171),
        &[(9785, 1, "01")],
        (16887, 0xdb13_b1db_de6f_1038),
    ),
    (
        "carried_separation_plan_moved",
        "arrival_save_after_start",
        (16887, 0xc681_ab82_572f_2171),
        &[(9889, 1, "01")],
        (16887, 0x0f9f_d80c_ef16_4154),
    ),
    (
        "carried_leave_home_plan_moved",
        "arrival_save_after_start",
        (16887, 0xc681_ab82_572f_2171),
        &[(9911, 1, "01")],
        (16887, 0xb9f9_b14d_aece_61cc),
    ),
    (
        "carried_plan_planned_later",
        "arrival_save_end",
        (28063, 0x3cba_e9e9_5c2e_a6f0),
        &[(20681, 1, "82")],
        (28063, 0x57ae_e84a_5eb9_cbf6),
    ),
    (
        "carried_plan_by_other_member",
        "arrival_save_after_start",
        (16887, 0xc681_ab82_572f_2171),
        &[(9950, 1, "24")],
        (16887, 0xee89_7345_d7c0_8bfa),
    ),
    (
        "carried_plans_swapped",
        "arrival_save_after_start",
        (16887, 0xc681_ab82_572f_2171),
        &[(69, 1, "0b"), (1895, 1, "0a")],
        (16887, 0x7379_b057_0f07_d572),
    ),
    (
        "pool_child_of_a_cancelled_pregnancy",
        "V6_pool_pregnancy_cancelled",
        (12821, 0xa97a_34d1_4abe_10db),
        &[
            (30, 4, "20061399"),
            (353, 0, "0090bc8d2100a201011f"),
            (806, 0, "0090c4fd9e0400a801071f"),
            (
                1290,
                0,
                "beb187f90b00a501021fd49ae3bbeacfeb962f00e4f3ffb90c00a601021ff3e1e3d4baa3e9c22c00",
            ),
            (1537, 0, "00badec6ae1300a301021fa0bb8ca1c58498a01c"),
            (1934, 0, "eed9a382930100a701021fdb86dcd7e8e9bbfac90100"),
            (2318, 0, "ccabc0c9e00c00a401021f85d2bc82f8b2ead44d00"),
            (2480, 3, "a9011d"),
            (
                7238,
                0,
                "1f90b48e03cebbc0d298e4dfdcb80101859680d5a184f0dd2f80c6d3808e8eb480da018780a38e80dcea0982858086de8290f929c0e69e80d88fafc02f81c0bb9780a4ca89e0010101000003d49ae3bbeacfeb962f0595c306608247400100000000000000000190b48e03f3e1e3d4baa3e9c22c27e19796a74e47400100000000000000000190b48e03db86dcd7e8e9bbfac90100000000000000000100000000000000000190b48e030000000f00011a000001cebbc0d298e4dfdcb8010090b48e0300018c8dc8be98e1f8837e01b4b68b96ebf1fbe1a60100",
            ),
            (7701, 1, "01"),
            (7765, 1, "99"),
            (7834, 0, "3b90b48e03"),
            (7888, 0, "3b90b48e03"),
            (7940, 0, "3b90b48e033b90b48e03"),
            (7966, 0, "3b90b48e03"),
            (8008, 0, "3b90b48e03"),
            (8046, 0, "3b90b48e03"),
            (8062, 1, "3d"),
        ],
        (13198, 0x5a64_5787_9beb_542c),
    ),
];

/// "arrival" at the first month start: woman 13 conceived in her carried series.
const MONTH_1: i64 = 2_419_200;
/// "arrival" when woman 45 conceived at the start of her carried series.
const V4B_AT: i64 = 21_772_800;
/// "arrival" when the pool twins of household 31 are born.
const V5_TWINS_AT: i64 = 7_257_600;

const V3_SAVE: (usize, u64) = (21734, 0x33b4_3258_3764_9558);
const V4_SAVE: (usize, u64) = (17268, 0x0b30_54a5_c8ec_e064);
const V4B_SAVE: (usize, u64) = (18581, 0x83a2_f784_3f4e_6ba6);
const V5_TWINS_SAVE: (usize, u64) = (17656, 0xf40a_6465_d36e_a861);
const V6_SAVE: (usize, u64) = (12821, 0xa97a_34d1_4abe_10db);

/// The reference's round 9 (out9, its notes' section 5g).
const LOAD_REJECTED_ROUND_9: [Hunks; 5] = [
    (
        "immigration_period_not_a_year",
        "load_base",
        (20695, 0xf509_3f7b_da81_7ed0),
        &[(44, 1, "82")],
        (20695, 0xcdcc_1532_d332_e77b),
    ),
    (
        "immigration_not_due",
        "load_base",
        (20695, 0xf509_3f7b_da81_7ed0),
        &[
            (35, 13, ""),
            (887, 0, "018088ff1d80a0fc7700030901"),
            (12871, 2, ""),
            (13352, 0, "0100"),
        ],
        (20695, 0x4fed_7972_6178_9d62),
    ),
    (
        "immigration_persons_off_by_one",
        "load_base",
        (20695, 0xf509_3f7b_da81_7ed0),
        &[(42, 1, "02")],
        (20695, 0xe025_be39_0912_941e),
    ),
    (
        "pool_mother_conceptions_off_by_one",
        "arrival_save_after_start",
        (16887, 0xc681_ab82_572f_2171),
        &[(7035, 1, "02")],
        (16887, 0xf31c_9c3b_63c3_1596),
    ),
    (
        "series_end_boundary_attempt_present",
        "series_end_boundary",
        (17254, 0x8b41_ca26_4bbe_4afd),
        &[
            (34, 1, "cd"),
            (70, 0, "80d0ce040066031c0180a8a702"),
            (10088, 5, "cd010100050036"),
        ],
        (17269, 0x3cc7_89c3_99b5_c4ca),
    ),
];

const SERIES_END_POOL: (usize, u64) = (10957, 0xee94_2e56_5b74_4552);
const SERIES_END_SAVE: (usize, u64) = (17254, 0x8b41_ca26_4bbe_4afd);
const IMMIGRANT_CHILD_YEAR_HASHES: [u64; 2] = [0x5336_b22a_0f86_ef0e, 0xdd77_0aa9_236a_a350];
const IMMIGRANT_CHILD_LOG_HASH: u64 = 0xdec1_31e5_a000_d64e;
const IMMIGRANT_CHILD_STATE_HASH: u64 = 0x9ab7_bb3d_e95f_8ab0;
const IMMIGRANT_CHILD_SAVE: (usize, u64) = (20535, 0x7203_dd9e_b604_e2cd);

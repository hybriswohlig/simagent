//! The village start of P4 (ADR-0020 section 8, G4): `n_start / 2` persons of each sex of
//! the repository's pool, aged from their 18th birthday to before their 41st at the freeze
//! (`18 * YEAR <= E_p - born < 41 * YEAR`), drawn per sex by rank on occasion 1 (`POOL_RANK`,
//! subject `WORLD`, the lineage key); each alone in a household of their own, without
//! partnership, pregnancy, carer or carried plans, moved out; no immigration. The women
//! arrive first (SimIds 1 to 40), then the men (41 to 80); no household of the pool is used.
//!
//! The expected values come from the independent reference (`tools/village_reference.py`
//! on `tools/world_reference.py`, written from the design note of the village and not from
//! this code): the persons of seeds 0x5eed_0004_0000_0002 and 0x5eed_0004_0000_0003, the
//! village `Start` of each (tag `core:criteria_village`, the switch on, no nutrition, no
//! founders, no couples, capacity 80, `n_start` 80), the rejected `Start`s of seed 2, and the
//! village of seed 2 over 30 years. Input records and saves are pinned by length and xxh3.

mod common;

use std::collections::BTreeMap;

use serde_json::Value;
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::finite::FiniteF64;
use sim_core::genome::Sex;
use sim_core::hash::stable_hash64;
use sim_core::ids::{InputSeq, SimId};
use sim_core::input::{Input, InputRecord, PoolStart, Start};
use sim_core::log::{LogEntry, log_hash};
use sim_core::params::WorldParams;
use sim_core::pool::PoolPerson;
use sim_core::time::SimTime;
use sim_core::world::{LoadError, World, YearOut};

const Y: i64 = 31_449_600;
const TAG: &str = "core:criteria_village";
const SEED_2_WORLD: u64 = 0x5eed_0004_0000_0002;
const SEED_3_WORLD: u64 = 0x5eed_0004_0000_0003;
const N_START: u64 = 80;
const CAPACITY: u64 = 80;

const CALIBRATION: &str = include_str!("../../../data/calibration/genetics.json");
const GENOME_MAP: &str = include_str!("../../../data/genetics/genome_map.json");
const POOL: &[u8] = include_bytes!("../../../data/pool/pool.bin");
const FREQUENCIES: &str = include_str!("../../../data/pool/pool_frequencies.json");

// The data and the pool (the reference's `village_<seed>_config`).
const DATA_HASH: u64 = 0xb1f5_c005_4302_363d;
const POOL_HASH: u64 = 0xac11_f329_cd5d_a77a;
const POOL_SEED: u64 = 0x11b0_19a2;
const FROZEN_AT: i64 = 23_587_200_000;
const CANDIDATE_WOMEN: usize = 5664;
const CANDIDATE_MEN: usize = 5925;

/// The frequencies of `data/pool/pool_frequencies.json`, each entry's `value`, as
/// `Data::with_pool` takes them (sim-cli reads the file the same way, its text removed).
fn frequencies() -> BTreeMap<DataId, FiniteF64> {
    let file: Value = serde_json::from_str(FREQUENCIES).expect("json");
    file["pool_frequencies"]
        .as_object()
        .expect("the entries")
        .iter()
        .map(|(id, entry)| {
            assert_eq!(entry["unit"], "fraction", "{id}");
            let q = entry["value"].as_f64().expect("a value");
            (
                DataId::new(id).expect("id"),
                FiniteF64::try_new(q).expect("finite"),
            )
        })
        .collect()
}

/// The repository's data as the CLI builds them: the life course data, the calibration and
/// the map of `data/` without their text, with the pool and its frequencies. (Here and not
/// in `common`, like `clamp_pool.rs`: the pool file is 8 MB, and every test crate compiles
/// `common`.)
fn repository_data() -> Data {
    let (calibration, genome_map) = common::files_from(CALIBRATION, GENOME_MAP);
    Data::with_pool(
        common::definitions(calibration, genome_map).expect("definitions"),
        POOL,
        frequencies(),
    )
    .and_then(|data| data.with_shocks(common::repository_shocks()))
    .expect("data")
}

/// Length and xxh3 of some bytes, as the reference pins an input record or a save.
fn sized(bytes: &[u8]) -> (usize, u64) {
    (bytes.len(), stable_hash64(bytes))
}

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex"))
        .collect()
}

fn sim(n: u64) -> SimId {
    SimId::try_from(n).expect("valid")
}

/// The village `Start` of `seed` with this pool part.
fn village_start(seed: u64, pool: PoolStart) -> Start {
    Start {
        world_seed: seed,
        founder_tag: DataId::new(TAG).expect("id"),
        world_params: WorldParams {
            hereditary_diseases: true,
            nutrition: Vec::new(),
        },
        founders: Vec::new(),
        couples: Vec::new(),
        capacity: Some(CAPACITY),
        pool: Some(pool),
    }
}

/// The first record of an input log, `Start` stamped -1.
fn start_record(start: &Start) -> Vec<u8> {
    postcard::to_allocvec(&InputRecord {
        t: SimTime::from_secs(-1),
        seq: InputSeq::try_from(1).expect("valid"),
        input: Input::Start(start.clone()),
    })
    .expect("encodes")
}

/// The fields of a record of the file that the village clears, as the reference names
/// them: each carried time that is set, and `moved_out` if it is false; `-` for none.
fn struck(record: &PoolPerson) -> String {
    let names: Vec<&str> = [
        ("conception_at", record.conception_at.is_some()),
        ("birth_at", record.birth_at.is_some()),
        ("separation_at", record.separation_at.is_some()),
        ("leave_home_at", record.leave_home_at.is_some()),
        ("moved_out", !record.moved_out),
    ]
    .into_iter()
    .filter_map(|(name, set)| set.then_some(name))
    .collect();
    if names.is_empty() {
        "-".to_owned()
    } else {
        names.join(",")
    }
}

/// A chosen person as the reference lists it: the lineage key, the age at the freeze in
/// seconds (`E_p - born`), the household of the pool file it comes from, the fields of its
/// record the village clears (`struck`).
type Chosen = (u64, i64, usize, &'static str);

/// What a village pins after `Start`: the input record, the entries, the state hash, the
/// save, and the pairs of the chosen (by SimId) whose kinship is above 0.
struct Village {
    seed: u64,
    chosen: &'static [Chosen; 80],
    start_input: (usize, u64),
    entries: usize,
    state_hash: u64,
    save: (usize, u64),
    kin: &'static [(u64, u64, f64)],
}

const VILLAGES: [Village; 2] = [
    Village {
        seed: SEED_2_WORLD,
        chosen: &SEED_2,
        start_input: (9743, 0x43ef_a525_3114_14aa),
        entries: 161,
        state_hash: 0x8e4e_057f_3454_42aa,
        save: (24703, 0x5e41_fd73_4b22_6ca2),
        kin: &[
            (9, 59, 1.0 / 256.0),
            (18, 61, 1.0 / 128.0),
            (18, 72, 1.0 / 64.0),
            (21, 54, 1.0 / 64.0),
            (21, 71, 1.0 / 64.0),
            (24, 63, 1.0 / 16.0),
            (39, 60, 1.0 / 64.0),
            (42, 60, 1.0 / 64.0),
            (43, 70, 1.0 / 16.0),
            (61, 72, 1.0 / 128.0),
            (68, 73, 1.0 / 16.0),
        ],
    },
    Village {
        seed: SEED_3_WORLD,
        chosen: &SEED_3,
        start_input: (9733, 0xf407_3a57_e854_cf28),
        entries: 161,
        state_hash: 0x3155_91e3_3864_aba7,
        save: (24700, 0x6351_646d_597c_7853),
        kin: &[
            (6, 32, 1.0 / 128.0),
            (6, 50, 1.0 / 128.0),
            (11, 74, 1.0 / 16.0),
            (12, 49, 1.0 / 128.0),
            (13, 44, 1.0 / 128.0),
            (18, 74, 1.0 / 256.0),
            (20, 46, 1.0 / 16.0),
            (23, 26, 1.0 / 32.0),
            (24, 30, 1.0 / 128.0),
            (34, 39, 1.0 / 64.0),
            (39, 75, 1.0 / 128.0),
            (50, 67, 1.0 / 128.0),
            (54, 80, 1.0 / 64.0),
            (57, 61, 1.0 / 16.0),
        ],
    },
];

/// The pool's members of the village's ages are 5664 women and 5925 men; `PoolStart::village`
/// takes 40 of each for each seed, in the reference's rank order, each alone and with the
/// file's record but for the carried plans, the carer and `moved_out`; the village `Start`
/// is the reference's input, and the town after it is the reference's (state, save, the
/// SimIds in arrival order, the kinship among the chosen).
#[test]
fn the_village_draws_the_references_persons() {
    let data = repository_data();
    assert_eq!(data.hash(), DATA_HASH);
    let held = data.pool().expect("a pool");
    assert_eq!(held.hash(), POOL_HASH);
    let pool = held.pool();
    assert_eq!(pool.pool_seed, POOL_SEED);
    assert_eq!(pool.frozen_at, SimTime::from_secs(FROZEN_AT));
    let candidates = |sex: Sex| {
        pool.households
            .iter()
            .flat_map(|household| household.members.iter())
            .filter(|member| {
                member.sex == sex && (18 * Y..41 * Y).contains(&(FROZEN_AT - member.born.as_secs()))
            })
            .count()
    };
    assert_eq!(candidates(Sex::Female), CANDIDATE_WOMEN);
    assert_eq!(candidates(Sex::Male), CANDIDATE_MEN);

    for village in &VILLAGES {
        let seed = village.seed;
        let drawn = PoolStart::village(&data, seed, N_START).expect("a village");
        assert_eq!(drawn.pool_hash, POOL_HASH, "{seed:x}");
        assert_eq!(drawn.n_start, N_START, "{seed:x}");
        assert!(!drawn.immigration, "{seed:x}");
        assert!(drawn.village, "{seed:x}");
        assert_eq!(drawn.households.len(), 80, "{seed:x}");
        for (place, (household, &(key, age, file, cleared))) in
            drawn.households.iter().zip(village.chosen).enumerate()
        {
            let at = format!("{seed:x} place {place}");
            let [person] = household.members.as_slice() else {
                panic!("{at}: one member");
            };
            assert_eq!(person.lineage_key.get(), key, "{at}");
            let sex = if place < 40 { Sex::Female } else { Sex::Male };
            assert_eq!(person.sex, sex, "{at}");
            assert_eq!(FROZEN_AT - person.born.as_secs(), age, "{at}");
            assert_eq!(household.partnership, None, "{at}");
            assert!(household.pregnancies.is_empty(), "{at}");
            let record = pool.households[file]
                .members
                .iter()
                .find(|member| member.lineage_key == person.lineage_key)
                .expect("a member of its household in the file");
            assert_eq!(struck(record), cleared, "{at}");
            assert_eq!(record.carer, None, "{at}: no candidate has a carer");
            let alone = PoolPerson {
                carer: None,
                conception_at: None,
                birth_at: None,
                separation_at: None,
                leave_home_at: None,
                moved_out: true,
                ..record.clone()
            };
            assert_eq!(person, &alone, "{at}");
        }

        let start = village_start(seed, drawn);
        assert_eq!(
            sized(&start_record(&start)),
            village.start_input,
            "{seed:x}"
        );
        let mut world = World::new(data.clone());
        world.submit(Input::Start(start)).expect("accepted");
        assert_eq!(
            postcard::to_allocvec(&world.input_log()[0]).expect("encodes"),
            start_record(match &world.input_log()[0].input {
                Input::Start(start) => start,
                _ => unreachable!("the first input is Start"),
            }),
            "{seed:x}"
        );
        assert_eq!(world.take_log().len(), village.entries, "{seed:x}");
        assert_eq!(world.state().living(), 80, "{seed:x}");
        assert_eq!(world.state_hash(), village.state_hash, "{seed:x}");
        assert_eq!(
            sized(&world.save(&[]).expect("saves")),
            village.save,
            "{seed:x}"
        );
        let pedigree = world.pedigree();
        let keys: Vec<(u64, u64)> = pedigree
            .persons
            .iter()
            .map(|person| (person.sim.get(), person.lineage_key.get()))
            .collect();
        let expected: Vec<(u64, u64)> = (1..=80).zip(village.chosen.iter().map(|c| c.0)).collect();
        assert_eq!(keys, expected, "{seed:x}: the SimIds in arrival order");
        for a in 1..=80 {
            for b in a + 1..=80 {
                let phi = village
                    .kin
                    .iter()
                    .find(|(x, y, _)| (*x, *y) == (a, b))
                    .map_or(0.0, |kin| kin.2);
                assert_eq!(
                    world.state().kinship(sim(a), sim(b)),
                    Some(phi),
                    "{seed:x}: sims {a} and {b}"
                );
            }
        }
    }
}

/// A rejected village `Start` of seed 2 as the reference built it: the case, the reject
/// code (postcard index and name), and its input record (length, xxh3; the hex of a short
/// one).
type Rejected = (
    &'static str,
    u8,
    &'static str,
    (usize, u64),
    Option<&'static str>,
);

const REJECTED: [Rejected; 16] = [
    (
        "odd_n_start",
        28,
        "start_pool_village_n_start",
        (9743, 0xc43d_8f9b_f746_b207),
        None,
    ),
    (
        "odd_n_start_no_households",
        28,
        "start_pool_village_n_start",
        (55, 0xed9a_3f0f_2d0a_e833),
        Some(
            "01010082808080c080c0f65e15636f72653a63726974657269615f76696c6c61676501000000015001facef6ea9ce5fc88ac014f000100",
        ),
    ),
    (
        "immigration_on",
        29,
        "start_pool_village_immigration",
        (9743, 0x7f8e_5078_a071_5e86),
        None,
    ),
    (
        "immigration_on_no_households",
        29,
        "start_pool_village_immigration",
        (55, 0x9398_c13c_fc86_8817),
        Some(
            "01010082808080c080c0f65e15636f72653a63726974657269615f76696c6c61676501000000015001facef6ea9ce5fc88ac0150010100",
        ),
    ),
    (
        "odd_n_start_before_immigration",
        28,
        "start_pool_village_n_start",
        (9743, 0x2ee4_b877_b28b_fabc),
        None,
    ),
    (
        "n_start_0_before_village",
        26,
        "start_pool_n_start",
        (55, 0xbe87_b7d9_dd01_fd3f),
        Some(
            "01010082808080c080c0f65e15636f72653a63726974657269615f76696c6c61676501000000015001facef6ea9ce5fc88ac0100000100",
        ),
    ),
    (
        "too_few_candidates_n_start_11330",
        30,
        "start_pool_village_candidates",
        (56, 0xe6e6_8d46_7b55_32d6),
        Some(
            "01010082808080c080c0f65e15636f72653a63726974657269615f76696c6c61676501000000015001facef6ea9ce5fc88ac01c258000100",
        ),
    ),
    (
        "immigration_before_candidates",
        29,
        "start_pool_village_immigration",
        (56, 0x1ef9_62e3_afd7_0b0d),
        Some(
            "01010082808080c080c0f65e15636f72653a63726974657269615f76696c6c61676501000000015001facef6ea9ce5fc88ac01c258010100",
        ),
    ),
    (
        "men_first",
        27,
        "start_pool_households",
        (9743, 0xe52b_cc25_4c9c_ef05),
        None,
    ),
    (
        "one_missing",
        27,
        "start_pool_households",
        (9622, 0x2bb3_c008_776c_e6a4),
        None,
    ),
    (
        "one_altered_leave_home",
        27,
        "start_pool_households",
        (9744, 0xf028_92d7_7904_1af6),
        None,
    ),
    (
        "record_unstruck_place_1_separation_at",
        27,
        "start_pool_households",
        (9749, 0xf14b_941b_d979_4bcc),
        None,
    ),
    (
        "the_town_draw",
        27,
        "start_pool_households",
        (10539, 0x35e6_58be_b78d_b890),
        None,
    ),
    (
        "another_seed",
        27,
        "start_pool_households",
        (9733, 0xd12c_e124_6324_1b9b),
        None,
    ),
    (
        "village_false_with_the_village_households",
        27,
        "start_pool_households",
        (9743, 0x1116_2f5c_0c8c_ed9a),
        None,
    ),
    (
        "decision_capacity_zero_before_pool",
        23,
        "capacity_zero",
        (9743, 0xbc79_f794_f8a7_f295),
        None,
    ),
];

/// The three checks of a village (an odd `n_start`, immigration on, more persons than a sex
/// has candidates) in this order after `start_pool_n_start` and before the households, then
/// the households, which must be the village's draw exactly; each rejected `Start` as the
/// reference built it. The draw takes 11328 (5664 women) and not 11330.
#[test]
fn a_village_start_is_rejected_as_the_reference_rejects_it() {
    let data = repository_data();
    let seed = SEED_2_WORLD;
    let good = PoolStart::village(&data, seed, N_START).expect("a village");
    let changed = |change: &dyn Fn(&mut PoolStart)| {
        let mut pool = good.clone();
        change(&mut pool);
        village_start(seed, pool)
    };
    let (women, men) = good.households.split_at(40);
    let mut altered = good.households.clone();
    altered[0].members[0].leave_home_at = Some(SimTime::from_secs(1));
    // Place 1, the first whose record the village changes (`separation_at`), as the file
    // holds it.
    let held = data.pool().expect("a pool").pool();
    let unstruck = {
        let mut households = good.households.clone();
        let key = households[1].members[0].lineage_key;
        let record = held.households[SEED_2[1].2]
            .members
            .iter()
            .find(|member| member.lineage_key == key)
            .expect("in the file")
            .clone();
        assert_eq!(struck(&record), "separation_at");
        households[1].members[0] = record;
        households
    };
    let town = PoolStart::drawn(&data, seed, N_START, false).expect("a pool");
    let other = PoolStart::village(&data, SEED_3_WORLD, N_START).expect("a village");
    let cases: [Start; 16] = [
        changed(&|p| p.n_start = 79),
        changed(&|p| {
            p.n_start = 79;
            p.households.clear();
        }),
        changed(&|p| p.immigration = true),
        changed(&|p| {
            p.immigration = true;
            p.households.clear();
        }),
        changed(&|p| {
            p.n_start = 81;
            p.immigration = true;
        }),
        changed(&|p| {
            p.n_start = 0;
            p.households.clear();
        }),
        changed(&|p| {
            p.n_start = 11_330;
            p.households.clear();
        }),
        changed(&|p| {
            p.n_start = 11_330;
            p.immigration = true;
            p.households.clear();
        }),
        changed(&|p| p.households = [men, women].concat()),
        changed(&|p| {
            p.households.pop();
        }),
        changed(&|p| p.households.clone_from(&altered)),
        changed(&|p| p.households.clone_from(&unstruck)),
        changed(&|p| p.households.clone_from(&town.households)),
        changed(&|p| p.households.clone_from(&other.households)),
        changed(&|p| p.village = false),
        Start {
            capacity: Some(0),
            ..changed(&|p| p.n_start = 79)
        },
    ];
    for ((name, code, code_name, input, short), start) in REJECTED.iter().zip(cases) {
        let record = start_record(&start);
        assert_eq!(sized(&record), *input, "{name}");
        if let Some(short) = short {
            assert_eq!(record, hex(short), "{name}");
        }
        let rejected = World::new(data.clone())
            .submit(Input::Start(start))
            .expect_err(name);
        assert_eq!(
            postcard::to_allocvec(&rejected.code).expect("encodes"),
            [*code],
            "{name}"
        );
        assert_eq!(
            serde_json::to_value(rejected.code).expect("json"),
            Value::from(*code_name),
            "{name}"
        );
    }
    assert!(
        World::new(data.clone())
            .submit(Input::Start(village_start(seed, good)))
            .is_ok()
    );
    let largest = 2 * u64::try_from(CANDIDATE_WOMEN).expect("fits");
    assert!(PoolStart::village(&data, seed, largest).is_some());
    assert_eq!(PoolStart::village(&data, seed, largest + 2), None);
}

// The village of seed 2 to the end of year 29 (the reference's `village_<seed>_run`).
const RUN_END: i64 = 30 * Y - 1;
const RUN_LOG: (usize, u64) = (2856, 0xdddb_91f3_e62e_c3a8);
const RUN_SAVE_END: (usize, u64) = (30396, 0xa5ff_5b1f_3174_4971);
const RUN_STATE_HASH: u64 = 0x0ffc_76bd_1c91_9793;
const RUN_ARCHIVED: u64 = 36;
const RUN_YEAR_HASHES: [u64; 30] = [
    0x8809_877b_3aa4_e1b4,
    0xbd85_3bc2_6e17_bbaa,
    0x5338_aee6_9a0c_0189,
    0x214b_a2e5_c30b_24ba,
    0xac71_19a6_2d1a_c3e7,
    0x99b1_3f46_ba16_6608,
    0xf0b9_6ab2_8d7e_efe5,
    0xec35_2d7d_c388_1f12,
    0x81e0_f7fd_5055_3b39,
    0xe710_a2ec_89ad_2e49,
    0xe1e1_aa8f_172a_3916,
    0x0624_888f_26c7_cd5f,
    0x15c9_3929_d4ca_6099,
    0xae08_fb70_9dcb_2d64,
    0xa6d2_1775_23a2_ce28,
    0x8e89_3be5_529f_716a,
    0x1895_d713_d496_8f9c,
    0xb21b_beea_35f3_4066,
    0x2bd8_e8ff_e0bf_8b43,
    0x2919_87a5_a434_31e3,
    0x792a_9edb_909e_52fa,
    0x6b07_cbec_0020_6970,
    0x9939_a72f_c5e0_f68f,
    0xac0f_25af_f31c_af27,
    0xe061_ffad_606e_ead9,
    0x9122_6907_2328_f26b,
    0x285c_b3f1_52b1_d5f4,
    0xfe53_5cdb_7b64_e7fb,
    0x5bae_e468_f9f3_4940,
    0x7d7f_842a_c91a_3617,
];
/// The living at the end of each year.
const RUN_LIVING: [u64; 30] = [
    80, 79, 80, 78, 71, 74, 76, 79, 81, 81, 82, 79, 80, 80, 80, 80, 80, 80, 80, 80, 80, 80, 80, 79,
    79, 80, 81, 81, 78, 78,
];
/// The births, the deaths and the emigrants of each year.
const RUN_BIRTHS: [u64; 30] = [
    0, 2, 1, 2, 3, 3, 2, 3, 2, 2, 1, 2, 1, 0, 3, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 1, 2, 0,
];
const RUN_DEATHS: [u64; 30] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0,
];
const RUN_EMIGRATED: [u64; 30] = [
    0, 3, 0, 4, 10, 0, 0, 0, 0, 2, 0, 5, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 5, 0,
];
/// The year whose save the resumed run starts from.
const RUN_RESUME_YEAR: i64 = 9;
/// A change of a save: its offset, the bytes removed there, the bytes put in.
type Hunk = (usize, usize, &'static str);

/// The save of year 9 as (length, xxh3), the hunks of the reference's forged save and that
/// save.
const VILLAGE_MOVED_IN: ((usize, u64), &[Hunk], (usize, u64)) = (
    (28648, 0x6854_1c16_1ecd_43eb),
    &[(8902, 1, "06"), (14657, 1, "01fecff6ab02")],
    (28653, 0x013e_c467_f2e5_5a92),
);

/// The village of seed 2 over 30 years (`K` = 80): the year hashes, the living at each year
/// end, the births, deaths (both `core:senescence`) and emigrants of each year, the log, the
/// save and the state at the end. At every year end the save loads (the loader's village
/// branch: its persons arrived alone, no household of the pool used), to the same state and
/// the same bytes; the world loaded at the end of year 9 runs on as the uninterrupted one.
#[test]
fn the_village_of_seed_2_over_30_years() {
    let data = repository_data();
    let start = village_start(
        SEED_2_WORLD,
        PoolStart::village(&data, SEED_2_WORLD, N_START).expect("a village"),
    );
    assert_eq!(sized(&start_record(&start)), VILLAGES[0].start_input);
    let mut world = World::new(data.clone());
    world.submit(Input::Start(start)).expect("accepted");
    let mut out = YearOut::default();
    let mut log: Vec<LogEntry> = world.take_log();
    let mut living = Vec::new();
    let mut resume: Option<(Vec<u8>, usize)> = None;
    for year in 0..30_i64 {
        let end = (year + 1) * Y - 1;
        world.advance_until(&mut out, SimTime::from_secs(end), &mut |_| false);
        log.extend(world.take_log());
        living.push(world.state().living());
        let bytes = world.save(&out.year_hashes).expect("saves");
        let (loaded, years) = World::load(&bytes, data.clone()).expect("loads");
        assert_eq!(loaded.state_hash(), world.state_hash(), "year {year}");
        assert_eq!(years, out.year_hashes, "year {year}");
        assert_eq!(loaded.save(&years).expect("saves"), bytes, "year {year}");
        if year == RUN_RESUME_YEAR {
            resume = Some((bytes, log.len()));
        }
    }
    let hashes: Vec<u64> = out.year_hashes.iter().map(|y| y.hash).collect();
    assert_eq!(hashes, RUN_YEAR_HASHES, "year hashes");
    assert_eq!(living, RUN_LIVING, "the living at each year end");
    assert_eq!((log.len(), log_hash(&log)), RUN_LOG, "log");
    let save = world.save(&out.year_hashes).expect("saves");
    assert_eq!(sized(&save), RUN_SAVE_END, "save at the end");
    assert_eq!(world.state_hash(), RUN_STATE_HASH, "state hash");
    assert_eq!(world.state().archive_len(), RUN_ARCHIVED, "archived");

    let (mut births, mut deaths, mut emigrated) = ([0_u64; 30], [0_u64; 30], [0_u64; 30]);
    let year_of = |at: SimTime| usize::try_from(at.as_secs().div_euclid(Y)).expect("a year");
    for person in world.pedigree().persons {
        if person.sim.get() > N_START {
            births[year_of(person.born)] += 1;
        }
        if let Some(died) = person.died {
            deaths[year_of(died)] += 1;
            let cause = person.cause_of_death.expect("a cause");
            assert_eq!(cause.as_str(), "core:senescence");
        }
        if let Some(left) = person.emigrated {
            emigrated[year_of(left)] += 1;
        }
    }
    assert_eq!(births, RUN_BIRTHS, "births");
    assert_eq!(deaths, RUN_DEATHS, "deaths");
    assert_eq!(emigrated, RUN_EMIGRATED, "emigrants");

    let (bytes, entries) = resume.expect("the save of year 9");
    // The loader's household check: sim 42, alone and moved out, moved into household 6 of
    // sim 6, its own household ended at the clock (the reference's `--moved-out`).
    let (base_pin, hunks, forged_pin) = VILLAGE_MOVED_IN;
    assert_eq!(sized(&bytes), base_pin, "the save of year 9");
    let mut forged = bytes.clone();
    for (offset, removed, put) in hunks.iter().rev() {
        forged.splice(*offset..offset + removed, hex(put));
    }
    assert_eq!(sized(&forged), forged_pin, "the forged save");
    assert_eq!(
        World::load(&forged, data.clone()).err(),
        Some(LoadError::Inconsistent),
        "two villagers who moved out in one household"
    );
    let (mut resumed, years) = World::load(&bytes, data).expect("loads");
    let mut out = YearOut {
        census: Vec::new(),
        year_hashes: years,
        trace: Vec::new(),
    };
    let mut log = log[..entries].to_vec();
    resumed.advance_until(&mut out, SimTime::from_secs(RUN_END), &mut |_| false);
    log.extend(resumed.take_log());
    let hashes: Vec<u64> = out.year_hashes.iter().map(|y| y.hash).collect();
    assert_eq!(hashes, RUN_YEAR_HASHES, "resumed: year hashes");
    assert_eq!((log.len(), log_hash(&log)), RUN_LOG, "resumed: log");
    assert_eq!(
        sized(&resumed.save(&out.year_hashes).expect("saves")),
        RUN_SAVE_END,
        "resumed: save at the end"
    );
    assert_eq!(resumed.state_hash(), RUN_STATE_HASH, "resumed: state hash");
}

/// Seed `0x5eed_0004_0000_0002`: the 40 women, then the 40 men, in rank order.
const SEED_2: [Chosen; 80] = [
    (0xfdc2_da1d_bf87_caf6, 679_795_200, 18_988, "-"),
    (0x6c09_5661_a903_1c84, 955_584_000, 14_411, "separation_at"),
    (
        0xe753_06ce_40a7_db0c,
        687_052_800,
        17_660,
        "birth_at,separation_at",
    ),
    (0xcc03_ef48_7fc1_81ce, 658_022_400, 18_806, "-"),
    (
        0x4dba_15f4_f6ba_6f25,
        800_755_200,
        20_057,
        "conception_at,separation_at",
    ),
    (
        0x7e30_af65_04df_a11f,
        1_195_084_800,
        15_113,
        "separation_at",
    ),
    (0xf753_5712_aa24_412a, 616_896_000, 19_699, "-"),
    (0x1269_7a78_e057_f17f, 696_729_600, 20_007, "-"),
    (
        0x2f82_832d_68e9_b1b7,
        1_129_766_400,
        14_195,
        "separation_at",
    ),
    (
        0xbff8_e0a7_9813_6332,
        1_054_771_200,
        14_001,
        "separation_at",
    ),
    (
        0xd097_0240_2160_056d,
        737_856_000,
        19_399,
        "conception_at,separation_at",
    ),
    (0xd9cc_fa92_424a_ec17, 706_406_400, 20_499, "-"),
    (
        0x3807_93ff_8fc3_7de2,
        720_921_600,
        20_415,
        "birth_at,separation_at",
    ),
    (0xc0e7_6c44_0c50_2b57, 1_270_080_000, 11_469, "-"),
    (
        0x3383_128d_de42_3073,
        1_265_241_600,
        16_547,
        "separation_at",
    ),
    (
        0x80e5_3774_9508_36fc,
        1_195_084_800,
        12_059,
        "separation_at",
    ),
    (0xf23a_5f73_a6cf_1271, 851_558_400, 15_536, "separation_at"),
    (
        0x1c3d_50cc_dea9_39ca,
        1_054_771_200,
        15_392,
        "separation_at",
    ),
    (
        0x642e_0c0e_9dba_5b15,
        1_270_080_000,
        13_947,
        "separation_at",
    ),
    (0x46f5_0bce_addc_669e, 909_619_200, 16_538, "separation_at"),
    (0xde0c_7960_4bb2_bcd2, 1_016_064_000, 13_829, "-"),
    (
        0xa786_b990_7a50_1878,
        628_992_000,
        11_293,
        "leave_home_at,moved_out",
    ),
    (0x0aa4_347e_2c07_54c6, 846_720_000, 18_172, "-"),
    (
        0x43d2_7110_947f_d383,
        907_200_000,
        18_107,
        "conception_at,separation_at",
    ),
    (0xfb79_326a_c443_9117, 863_654_400, 20_303, "birth_at"),
    (
        0xba41_257d_b358_c738,
        1_204_761_600,
        12_143,
        "separation_at",
    ),
    (
        0xdf97_5db0_4797_2009,
        728_179_200,
        19_387,
        "conception_at,separation_at",
    ),
    (
        0xb21b_f674_3411_27f4,
        1_091_059_200,
        13_317,
        "separation_at",
    ),
    (
        0xce67_5ac5_27ad_05a3,
        1_049_932_800,
        13_411,
        "separation_at",
    ),
    (0x158b_0cfe_8627_d7e0, 740_275_200, 18_539, "separation_at"),
    (0xcd63_e6a6_ba1a_0f57, 754_790_400, 17_605, "-"),
    (0x3b11_54ce_c0c3_1a15, 866_073_600, 15_507, "separation_at"),
    (0x664c_8da2_15c0_5f0e, 568_512_000, 20_793, "-"),
    (0xba87_835c_24d3_b2f9, 1_107_993_600, 12_812, "-"),
    (
        0x4c2c_9af4_24a5_7f2b,
        1_052_352_000,
        13_990,
        "separation_at",
    ),
    (0xe582_e115_af63_0d93, 633_830_400, 20_723, "-"),
    (
        0xa173_c9ca_d0d6_c632,
        1_008_806_400,
        17_104,
        "separation_at",
    ),
    (0x8818_1fee_ddd5_ebc8, 1_074_124_800, 14_196, "-"),
    (
        0x4702_972c_20e6_37b3,
        912_038_400,
        16_849,
        "conception_at,separation_at",
    ),
    (0x950f_09ed_af2f_c004, 933_811_200, 13_844, "separation_at"),
    (0xa530_4f60_dede_988b, 866_073_600, 15_325, "-"),
    (0x3f22_52a1_4452_65ba, 694_310_400, 20_541, "-"),
    (0x85ee_2c65_f4d0_5656, 708_825_600, 17_827, "-"),
    (0xf098_db83_6a1c_2e37, 1_098_316_800, 15_811, "-"),
    (0x08a2_0acf_7378_929a, 984_614_400, 17_425, "-"),
    (
        0xb125_8ede_4f50_f91c,
        592_704_000,
        10_543,
        "leave_home_at,moved_out",
    ),
    (0xb228_ed0e_2dd0_d3cc, 1_250_726_400, 13_031, "-"),
    (0x8f48_8283_c83f_5922, 832_204_800, 16_781, "-"),
    (0xca38_00ce_49de_15f3, 677_376_000, 20_442, "-"),
    (0x5033_d9bd_94bf_2a6f, 897_523_200, 15_322, "-"),
    (0xbbcc_b581_a3a6_4390, 948_326_400, 14_256, "-"),
    (0x242b_8231_8ec4_a99f, 887_846_400, 14_853, "-"),
    (0x3149_fe31_e47d_6281, 916_876_800, 17_009, "-"),
    (0x288c_941b_5eb8_fd67, 1_091_059_200, 13_704, "-"),
    (0xabc7_2d89_a81a_63bb, 1_175_731_200, 15_757, "-"),
    (0xdcc3_54d7_c217_3b92, 899_942_400, 14_868, "-"),
    (0x8562_8d60_8e11_3ed3, 1_069_286_400, 12_895, "-"),
    (0x8c6f_846c_4ef2_4f27, 856_396_800, 19_218, "-"),
    (0x96ad_6702_516b_4fe8, 979_776_000, 13_903, "-"),
    (0xfa3c_1fbe_3c31_2e18, 948_326_400, 17_996, "-"),
    (0x5d4d_244f_8b1f_7588, 718_502_400, 18_283, "-"),
    (0x6c5f_6a64_c247_0d78, 1_137_024_000, 20_636, "-"),
    (0xdbe7_8682_38ee_b9fe, 824_947_200, 17_427, "-"),
    (0xbfb6_583b_7ae4_9d34, 628_992_000, 19_719, "-"),
    (0x950c_47ee_02ff_361f, 1_178_150_400, 13_096, "-"),
    (0x7fee_aeb6_ce4a_d886, 919_296_000, 14_303, "-"),
    (0xc43c_4c6c_fabd_bcf4, 1_270_080_000, 14_446, "-"),
    (0x1681_8fee_ac96_b21a, 633_830_400, 19_322, "-"),
    (0x437e_7b58_934d_5807, 578_188_800, 20_587, "-"),
    (0xb3b7_d0a2_ee3a_4e64, 863_654_400, 14_923, "-"),
    (0x9f7c_6856_12d9_89c6, 1_272_499_200, 13_457, "-"),
    (0x4320_da54_09a4_74d3, 769_305_600, 16_278, "-"),
    (
        0xeb12_9b9d_b02b_00b6,
        585_446_400,
        11_681,
        "leave_home_at,moved_out",
    ),
    (0x808e_bf19_e991_6c53, 994_291_200, 15_404, "-"),
    (0xde52_cb0e_4385_6ad0, 602_380_800, 20_019, "-"),
    (0xabb1_209c_e962_77fb, 1_195_084_800, 11_249, "-"),
    (0x1aa9_3967_4d60_9725, 1_187_827_200, 17_563, "-"),
    (0x1402_afe5_9dd3_9543, 1_074_124_800, 17_621, "-"),
    (0x5d79_3372_77c3_779e, 1_238_630_400, 10_686, "-"),
    (0xc3c8_79f8_65b4_1cd1, 1_214_438_400, 11_787, "-"),
];

/// Seed `0x5eed_0004_0000_0003`: the 40 women, then the 40 men, in rank order.
const SEED_3: [Chosen; 80] = [
    (
        0xef47_333d_4543_4f38,
        1_255_564_800,
        12_876,
        "separation_at",
    ),
    (0x761e_764c_e95b_85b1, 585_446_400, 20_402, "-"),
    (0xfd52_7a59_9700_821b, 1_250_726_400, 11_462, "-"),
    (
        0xea9e_82e4_dd2b_8794,
        713_664_000,
        10_286,
        "leave_home_at,moved_out",
    ),
    (0xaa71_5c19_22e4_0d0d, 1_074_124_800, 12_842, "-"),
    (
        0x419d_43d8_dcdb_ad55,
        1_284_595_200,
        13_308,
        "separation_at",
    ),
    (
        0xbc4b_a72c_875b_2582,
        1_257_984_000,
        11_713,
        "separation_at",
    ),
    (
        0x9b8d_aae1_47ec_0e73,
        1_221_696_000,
        11_988,
        "separation_at",
    ),
    (0x62ea_a0f4_ca11_1a69, 1_199_923_200, 13_522, "-"),
    (
        0xd133_055e_5d81_e1de,
        803_174_400,
        20_137,
        "conception_at,separation_at",
    ),
    (
        0x1120_0556_dac2_db95,
        1_049_932_800,
        15_571,
        "separation_at",
    ),
    (0x5adc_fb24_8cc2_6cee, 650_764_800, 19_641, "-"),
    (
        0x770a_347e_6c34_6db1,
        1_137_024_000,
        16_056,
        "separation_at",
    ),
    (0x8520_5cc9_c3a7_8f2f, 967_680_000, 16_441, "separation_at"),
    (0x3a21_7763_ce9a_ca04, 832_204_800, 16_967, "separation_at"),
    (
        0x4f6b_f3d4_8412_1c3b,
        999_129_600,
        20_146,
        "conception_at,separation_at",
    ),
    (
        0xacbf_29b6_55ea_0146,
        687_052_800,
        10_071,
        "leave_home_at,moved_out",
    ),
    (
        0xec77_9580_cd34_130e,
        1_069_286_400,
        13_455,
        "separation_at",
    ),
    (
        0xd48e_6059_ce8e_cf1b,
        1_248_307_200,
        12_066,
        "separation_at",
    ),
    (
        0x697e_8b26_07c8_36bf,
        1_086_220_800,
        13_352,
        "separation_at",
    ),
    (0x449c_f891_a408_7095, 999_129_600, 15_797, "separation_at"),
    (
        0xc43c_361e_2214_759a,
        824_947_200,
        20_138,
        "conception_at,separation_at",
    ),
    (0xd8ab_1038_f8a2_8b7d, 1_020_902_400, 13_402, "-"),
    (
        0xd4a8_84d5_73fe_91d3,
        573_350_400,
        9_953,
        "leave_home_at,moved_out",
    ),
    (0x6a04_956a_246a_b3ab, 795_916_800, 16_865, "-"),
    (0xbfb2_a149_56ba_48e5, 754_790_400, 17_550, "-"),
    (0xf4b3_4d22_0659_65f1, 749_952_000, 17_637, "-"),
    (
        0x8643_9c9c_8d2c_362d,
        1_231_372_800,
        11_724,
        "separation_at",
    ),
    (0x3cca_8251_2c98_6629, 815_270_400, 16_040, "-"),
    (
        0x0027_75ab_66ba_f8c3,
        1_270_080_000,
        13_386,
        "separation_at",
    ),
    (
        0x80c4_2a49_bdef_8932,
        1_170_892_800,
        13_137,
        "separation_at",
    ),
    (0x8818_1fee_ddd5_ebc8, 1_074_124_800, 14_196, "-"),
    (
        0x9d1b_27f0_4d70_d9cd,
        1_127_347_200,
        15_037,
        "separation_at",
    ),
    (
        0x67ad_c2ae_1e1a_bf79,
        1_180_569_600,
        13_172,
        "separation_at",
    ),
    (
        0x1357_528a_6c72_a010,
        1_185_408_000,
        17_851,
        "birth_at,separation_at",
    ),
    (
        0x3eb3_458d_a68d_d390,
        878_169_600,
        17_995,
        "birth_at,separation_at",
    ),
    (0xbdb8_57e2_e2e6_0efc, 696_729_600, 18_051, "-"),
    (
        0xa731_ef6a_6449_afa1,
        706_406_400,
        19_463,
        "conception_at,separation_at",
    ),
    (
        0x6ccc_d87e_ce31_89ae,
        1_047_513_600,
        13_744,
        "separation_at",
    ),
    (
        0xf43a_35b3_ebb4_3e8d,
        597_542_400,
        10_209,
        "leave_home_at,moved_out",
    ),
    (
        0xffd9_0159_8bd4_ac25,
        728_179_200,
        10_258,
        "leave_home_at,moved_out",
    ),
    (
        0x0416_5580_8421_31db,
        607_219_200,
        10_653,
        "leave_home_at,moved_out",
    ),
    (0x8635_382d_1463_db4a, 783_820_800, 16_525, "-"),
    (0x4e1b_95f8_2c85_e99c, 812_851_200, 16_080, "-"),
    (0xe958_2e3a_724f_2a84, 1_025_740_800, 14_460, "-"),
    (0x3499_595d_355c_5474, 1_081_382_400, 15_700, "-"),
    (0xdf69_1b46_1841_5910, 827_366_400, 19_165, "-"),
    (0x0d7b_eb29_d0f1_8fc9, 1_141_862_400, 12_132, "-"),
    (0xe361_ceab_10a9_2abe, 941_068_800, 13_809, "-"),
    (0x764d_a1a7_3d1a_d615, 861_235_200, 16_304, "-"),
    (0xa9bc_a6ee_54f6_0eea, 1_248_307_200, 15_567, "-"),
    (0xed7f_a615_bcfa_380f, 1_139_443_200, 14_771, "-"),
    (0x1c77_4625_4e49_0deb, 1_045_094_400, 12_503, "-"),
    (0xbb37_29bc_1b20_359a, 926_553_600, 14_115, "-"),
    (0xfcf5_db5e_b960_76e1, 822_528_000, 16_944, "-"),
    (
        0xfe9f_eb4f_3a18_9455,
        619_315_200,
        10_084,
        "leave_home_at,moved_out",
    ),
    (0xff41_3e8e_e389_d922, 1_224_115_200, 11_278, "-"),
    (0x3a24_83e5_b9e7_a725, 1_074_124_800, 13_894, "-"),
    (0x9c82_693c_3ac6_d254, 1_025_740_800, 13_247, "-"),
    (0x3abc_6933_04a7_2245, 1_078_963_200, 16_537, "-"),
    (0xf06e_bcbf_1ec8_5564, 907_200_000, 17_622, "-"),
    (0xf4dd_2786_3eff_4308, 1_248_307_200, 13_643, "-"),
    (0x6ac3_706f_8ab5_2220, 720_921_600, 18_795, "-"),
    (0x0f85_8f07_e2b8_ad41, 960_422_400, 16_733, "-"),
    (0x0a71_87bf_1307_889a, 977_356_800, 20_410, "-"),
    (0x33cd_60ea_8ba7_3322, 1_081_382_400, 15_509, "-"),
    (0xca51_ff2d_cc64_e779, 706_406_400, 20_817, "-"),
    (0x4e8b_8c5d_5d44_b07c, 735_436_800, 16_314, "-"),
    (
        0x55ac_ee55_dbd5_074e,
        609_638_400,
        11_174,
        "leave_home_at,moved_out",
    ),
    (0x1630_af69_ddf4_8530, 788_659_200, 16_455, "-"),
    (0xcead_22ac_0fd8_9234, 655_603_200, 20_231, "-"),
    (0x6bb2_a146_68cb_e545, 1_209_600_000, 15_421, "-"),
    (0xf3bc_e7da_a320_9657, 645_926_400, 20_448, "-"),
    (0x3402_58bf_5eda_e402, 815_270_400, 16_043, "-"),
    (0x2a45_bf3b_4938_d847, 1_202_342_400, 13_173, "-"),
    (0x5286_4476_fe40_d2ac, 740_275_200, 19_672, "-"),
    (0xb17d_806c_6416_c377, 653_184_000, 18_907, "-"),
    (0x73b7_70cc_606d_73b2, 1_052_352_000, 14_424, "-"),
    (0xc4a5_5a41_5514_51cd, 662_860_800, 19_212, "-"),
    (0x987d_7f6f_0957_34a7, 1_028_160_000, 13_018, "-"),
];

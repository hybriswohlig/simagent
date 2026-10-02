//! Death through hazard budgets (ADR-0001, C4 simplified for the Durchstich): a sim dies
//! of a cause when its budget for that cause has run out, leaves for the archive, and
//! what was planned for it is cancelled.
//!
//! The expected bytes, times and hashes come from `tools/state_reference.py`, which runs
//! the model of `tools/world_reference.py`, written from the ADRs and not from this code.
//!
//! Scenario "mortal": seed 0xff, founder tag `test:mortal`, `test:base` with 0.5 per year
//! and `test:old_age` with 0.05 per year, a woman born one second and a man born two
//! seconds before the start. She dies of `test:base` after about 1.46 years, he of
//! `test:old_age` after about 1.71 years.

use sim_core::canonical::YearHash;
use sim_core::data_id::{Code, DataId};
use sim_core::finite::FiniteF64;
use sim_core::genome::{Allele, Genotype, Sex};
use sim_core::input::{DeathCause, Founder, Input, InputRejected, RejectCode, Start};
use sim_core::log::{LogEntry, log_hash};
use sim_core::params::WorldParams;
use sim_core::time::SimTime;
use sim_core::version::EMPTY_DATA_HASH;
use sim_core::world::{CensusRow, World, YearOut};

const Y: i64 = 31_449_600;

// tools/state_reference.py
const MORTAL_AFTER_START: &str = "0401ff010100028bbec9d8c5bd99888b01000000000000e03fded289dba5abffff549a9999999999a93f010307fc87ff1d0005010200fe87ff1d0002010100fe87ff1d000100018088ff1d88c4db2b000302018bbec9d8c5bd99888b0100baea993300070202ded289dba5abffff5400b4e9d27100040201ded289dba5abffff5400c09cbf8901000602028bbec9d8c5bd99888b010008020101dded90d1a3f7cbf6b3010000010203cbdbe1f0b893cbc91e01010100";
const MORTAL_SAVE_AFTER_START: &str = "53494d4147454e540104c2a9cec6d380a0832d0401ff010100028bbec9d8c5bd99888b01000000000000e03fded289dba5abffff549a9999999999a93f010307fc87ff1d0005010200fe87ff1d0002010100fe87ff1d000100018088ff1d88c4db2b000302018bbec9d8c5bd99888b0100baea993300070202ded289dba5abffff5400b4e9d27100040201ded289dba5abffff5400c09cbf8901000602028bbec9d8c5bd99888b010008020101dded90d1a3f7cbf6b3010000010203cbdbe1f0b893cbc91e010101000703000200010002000300020003000401010100ff010b746573743a6d6f7274616c000209746573743a62617365000000000000e03f0c746573743a6f6c645f6167659a9999999999a93f02010000010301010100";
const MORTAL_AFTER_YEAR_1: &str = "0401ff010100028bbec9d8c5bd99888b01000000000000e03fded289dba5abffff549a9999999999a93ffe8ffe3b0301fe97fd59000100018088ff1d0a0002010188c4db2b008bbec9d8c5bd99888b01dded90d1a3f7cbf6b3010000010203baea993300ded289dba5abffff54cbdbe1f0b893cbc91e010101";
const MORTAL_SAVE_AFTER_YEAR_1: &str = "53494d4147454e540104c2a9cec6d380a0832d0401ff010100028bbec9d8c5bd99888b01000000000000e03fded289dba5abffff549a9999999999a93ffe8ffe3b0301fe97fd59000100018088ff1d0a0002010188c4db2b008bbec9d8c5bd99888b01dded90d1a3f7cbf6b3010000010203baea993300ded289dba5abffff54cbdbe1f0b893cbc91e0101010101000801010100ff010b746573743a6d6f7274616c000209746573743a62617365000000000000e03f0c746573743a6f6c645f6167659a9999999999a93f0201000001030101010200a38de3ed87fedad3f701028cb782a1e6a3a6efb701";
const MORTAL_LOG: [&str; 7] = [
    "0100000102010002",
    "02000100010102010101dded90d1a3f7cbf6b301",
    "03000100020102010103cbdbe1f0b893cbc91e",
    "04fc87ff1d010002010103000201",
    "05fe87ff1d010001010102000201",
    "0688c4db2b010001020102000404dded90d1a3f7cbf6b3018bbec9d8c5bd99888b01000409746573743a62617365",
    "07baea9933010002020103000404cbdbe1f0b893cbc91eded289dba5abffff5400040c746573743a6f6c645f616765",
];
const MORTAL_LOG_HASH: u64 = 0xee37f173c0a6bcda;
const MORTAL_YEAR_HASHES: [u64; 2] = [0xf7a7_6bf0_7db8_c6a3, 0xb7de_991e_6420_9b8c];
/// When she dies of `test:base` and he of `test:old_age`, in seconds.
const HER_DEATH: i64 = 45_838_596;
const HIS_DEATH: i64 = 53_688_989;
const TIE_BASE_FIRST_SAVE: &str = "53494d4147454e540104c2a9cec6d380a0832d0401ff010100028bbec9d8c5bd99888b010000000000001440ded289dba5abffff54901875b14bfde23ffe8ffe3b0201fe97fd59000100018088ff1d0600010101f4f5ee30008bbec9d8c5bd99888b01f0da979a8fd9f18a95010000010101000501010100ff0108746573743a746965000209746573743a6261736500000000000014400c746573743a6f6c645f616765901875b14bfde23f01010000010200d5e890adfbaddeaefc0102bff8ada3d1e2c2c415";
const TIE_OLD_AGE_FIRST_SAVE: &str = "53494d4147454e540104c2a9cec6d380a0832d0401ff01010002ded289dba5abffff54901875b14bfde23f8bbec9d8c5bd99888b010000000000001440fe8ffe3b0201fe97fd59000100018088ff1d0600010101f4f5ee3000ded289dba5abffff54f0da979a8fd9f18a95010000010101000501010100ff0108746573743a74696500020c746573743a6f6c645f616765901875b14bfde23f09746573743a62617365000000000000144001010000010200c2ecebd0a7dafcf0d00102fce583918eaa8e98ad01";
/// The hazard of `test:old_age` for which both causes of "tie" are due in one second.
const TIE_OLD_AGE_PER_YEAR: f64 = 0.593_419_882_389_541_8;

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex digits"))
        .collect()
}

fn cause(code: &str, per_year: f64) -> DeathCause {
    DeathCause {
        cause: Code::new(code).expect("valid"),
        per_year: FiniteF64::try_new(per_year).expect("finite"),
    }
}

fn woman(born: i64) -> Founder {
    Founder {
        born: SimTime::from_secs(born),
        sex: Sex::Female,
        genotype: Genotype {
            maternal: Allele::Wild,
            paternal: Allele::Dummy,
        },
    }
}

fn man(born: i64) -> Founder {
    Founder {
        born: SimTime::from_secs(born),
        sex: Sex::Male,
        genotype: Genotype {
            maternal: Allele::Dummy,
            paternal: Allele::Dummy,
        },
    }
}

fn start(tag: &str, death_causes: Vec<DeathCause>, founders: Vec<Founder>) -> Input {
    Input::Start(Start {
        world_seed: 0xff,
        founder_tag: DataId::new(tag).expect("valid"),
        world_params: WorldParams {
            hereditary_diseases: false,
        },
        death_causes,
        founders,
    })
}

fn mortal_causes() -> Vec<DeathCause> {
    vec![cause("test:base", 0.5), cause("test:old_age", 0.05)]
}

fn mortal() -> World {
    let mut world = World::new();
    world
        .submit(start(
            "test:mortal",
            mortal_causes(),
            vec![woman(-1), man(-2)],
        ))
        .expect("accepted");
    world
}

struct Run {
    world: World,
    out: YearOut,
    log: Vec<LogEntry>,
}

fn run(mut world: World, until: i64) -> Run {
    let mut out = YearOut::default();
    let mut log = world.take_log();
    world.advance_until(&mut out, SimTime::from_secs(until), &mut |_| false);
    log.extend(world.take_log());
    Run { world, out, log }
}

/// `Start` plans per founder the first birthday and then one death per cause, in the
/// order of the causes; the causes are part of the projection.
#[test]
fn start_plans_a_death_per_cause() {
    let mut world = mortal();
    assert_eq!(world.canonical_bytes(), hex(MORTAL_AFTER_START));
    assert_eq!(
        world.pending_events(),
        7,
        "the year end series, two birthdays and four deaths"
    );
    world.take_log();
    assert_eq!(
        world.save(&[]).expect("saves"),
        hex(MORTAL_SAVE_AFTER_START)
    );
}

/// The two deaths of "mortal": each sim dies in the second its budget for the cause
/// runs out, the entry names the plan and the draw of the budget, the sim is in the
/// archive, and its birthday and its other death are cancelled.
#[test]
fn a_sim_dies_when_its_budget_runs_out() {
    let run = run(mortal(), 2 * Y - 1);
    let entries: Vec<Vec<u8>> = run
        .log
        .iter()
        .map(|entry| postcard::to_allocvec(entry).expect("encodes"))
        .collect();
    assert_eq!(entries, MORTAL_LOG.map(hex));
    assert_eq!(log_hash(&run.log), MORTAL_LOG_HASH);
    assert_eq!(
        serde_json::to_string(&run.log[5]).expect("json"),
        format!(
            r#"{{"id":6,"t":{HER_DEATH},"subjects":[{{"sim":1}}],"causes":[{{"plan":{{"by":2,"planned_at":0}}}},{{"random":{{"tag":"0000000000000004","subject":"b3ed2fba3a2436dd","occasion":"8b1065ec5b125f0b","detail":"0000000000000000"}}}}],"payload":{{"core:death":{{"cause":"test:base"}}}}}}"#
        )
    );
    assert_eq!(
        serde_json::to_string(&run.log[6]).expect("json"),
        format!(
            r#"{{"id":7,"t":{HIS_DEATH},"subjects":[{{"sim":2}}],"causes":[{{"plan":{{"by":3,"planned_at":0}}}},{{"random":{{"tag":"0000000000000004","subject":"1e932c9b8e186dcb","occasion":"54fffd5a5b62695e","detail":"0000000000000000"}}}}],"payload":{{"core:death":{{"cause":"test:old_age"}}}}}}"#
        )
    );

    let state = run.world.state();
    assert_eq!((state.living(), state.archive_len()), (0, 2));
    assert_eq!(state.sim_ids_allocated(), 2, "nobody was created");
    // An archived sim stays resolvable by its ID.
    let sim = |n: u64| sim_core::ids::SimId::try_from(n).expect("valid");
    assert_eq!(state.born(sim(1)), Some(SimTime::from_secs(-1)));
    assert_eq!(state.born(sim(2)), Some(SimTime::from_secs(-2)));
    assert_eq!(run.world.pending_events(), 1, "only the year end series");
    assert_eq!(run.world.canonical_bytes(), hex(MORTAL_AFTER_YEAR_1));
    let hashes: Vec<u64> = run.out.year_hashes.iter().map(|year| year.hash).collect();
    assert_eq!(hashes, MORTAL_YEAR_HASHES);
    assert_eq!(
        run.world.save(&run.out.year_hashes).expect("saves"),
        hex(MORTAL_SAVE_AFTER_YEAR_1)
    );

    // The census counts the seconds they lived: the second of death is not lived.
    let lived = |secs: i64| u64::try_from(secs).expect("positive");
    assert_eq!(
        run.out.census,
        [
            CensusRow {
                year: 0,
                living: 2,
                lived_secs: lived(2 * Y),
            },
            CensusRow {
                year: 1,
                living: 0,
                lived_secs: lived((HER_DEATH - Y) + (HIS_DEATH - Y)),
            },
        ]
    );
}

/// A death does not touch the others: he has his birthday at the end of year 1 if only
/// she dies, with the entry that her death would otherwise have had no part in.
#[test]
fn the_death_of_one_sim_leaves_the_others_alone() {
    let run = run(mortal(), HER_DEATH);
    let state = run.world.state();
    assert_eq!((state.living(), state.archive_len()), (1, 1));
    // The year end series, his birthday and his two deaths.
    assert_eq!(run.world.pending_events(), 4);
    assert_eq!(run.log.len(), 6);
    // One second earlier she lives, and her three events are pending.
    let before = self::run(mortal(), HER_DEATH - 1);
    assert_eq!(before.world.state().living(), 2);
    assert_eq!(before.world.pending_events(), 7);
}

/// The world goes on without living sims: the year end series runs, the years are
/// counted, nothing else happens.
#[test]
fn a_world_without_the_living_goes_on() {
    let run = run(mortal(), 5 * Y - 1);
    assert_eq!(run.log.len(), 7);
    assert_eq!(run.out.year_hashes.len(), 5);
    assert_eq!(run.out.census[4].living, 0);
    assert_eq!(run.out.census[4].lived_secs, 0);
    assert_eq!(run.world.pending_events(), 1);
}

/// Loading and going on gives the uninterrupted run, whether the save was taken before
/// the deaths, between them or after them: the loaded world cancels the events of a
/// dying sim like the world that planned them.
#[test]
fn save_load_continue_across_the_deaths() {
    let whole = run(mortal(), 3 * Y - 1);
    for stop in [
        0,
        Y - 1,
        HER_DEATH - 1,
        HER_DEATH,
        HIS_DEATH - 1,
        HIS_DEATH,
        2 * Y - 1,
    ] {
        let part = run(mortal(), stop);
        let bytes = part.world.save(&part.out.year_hashes).expect("saves");
        let (mut loaded, years) = World::load(&bytes, EMPTY_DATA_HASH).expect("loads");
        assert_eq!(loaded.save(&years).expect("saves"), bytes, "stop {stop}");
        let mut out = YearOut {
            census: part.out.census.clone(),
            year_hashes: years,
        };
        loaded.advance_until(&mut out, SimTime::from_secs(3 * Y - 1), &mut |_| false);
        let mut log = part.log;
        log.extend(loaded.take_log());
        assert_eq!(out.year_hashes, whole.out.year_hashes, "stop {stop}");
        assert_eq!(out.census, whole.out.census, "stop {stop}");
        assert_eq!(log_hash(&log), log_hash(&whole.log), "stop {stop}");
        assert_eq!(loaded.state_hash(), whole.world.state_hash(), "stop {stop}");
        assert_eq!(loaded.pending_events(), 1, "stop {stop}");
    }
}

/// Scenario "tie" of the reference: two causes due in the same second. The cause that
/// comes first in the list wins, because its event was planned first.
#[test]
fn of_two_causes_due_together_the_first_in_the_list_wins() {
    let base = || cause("test:base", 5.0);
    let old_age = || cause("test:old_age", TIE_OLD_AGE_PER_YEAR);
    for (causes, code, save) in [
        (vec![base(), old_age()], "test:base", TIE_BASE_FIRST_SAVE),
        (
            vec![old_age(), base()],
            "test:old_age",
            TIE_OLD_AGE_FIRST_SAVE,
        ),
    ] {
        let mut world = World::new();
        world
            .submit(start("test:tie", causes, vec![woman(-1)]))
            .expect("accepted");
        let run = run(world, 2 * Y - 1);
        let died: Vec<String> = run
            .log
            .iter()
            .map(|entry| serde_json::to_string(&entry.payload).expect("json"))
            .filter(|payload| payload.contains("core:death"))
            .collect();
        assert_eq!(died, [format!(r#"{{"core:death":{{"cause":"{code}"}}}}"#)]);
        assert_eq!(run.world.pending_events(), 1);
        assert_eq!(
            run.world.save(&run.out.year_hashes).expect("saves"),
            hex(save),
            "{code}"
        );
    }
}

/// A hazard so small that the budget lasts 100 000 years or more means never (ADR-0002):
/// no event is planned, and the sim has its birthdays.
#[test]
fn a_death_that_is_never_due_is_not_planned() {
    let mut world = World::new();
    world
        .submit(start(
            "test:mortal",
            vec![cause("test:base", 1e-9), cause("test:old_age", 1e-6)],
            vec![woman(-1)],
        ))
        .expect("accepted");
    assert_eq!(
        world.pending_events(),
        2,
        "the year end series and a birthday"
    );
    let run = run(world, 200 * Y - 1);
    assert_eq!(run.world.state().living(), 1);
    assert_eq!(run.log.len(), 2 + 200);
}

/// The causes of death are checked with the start: none twice, no hazard below the
/// smallest one. A rejected start changes nothing.
#[test]
fn a_start_with_bad_causes_of_death_is_rejected() {
    let rejected = |causes: Vec<DeathCause>| {
        let mut world = World::new();
        let before = world.state_hash();
        let result = world.submit(start("test:mortal", causes, vec![woman(-1)]));
        if result.is_err() {
            assert_eq!(world.state_hash(), before);
            assert!(world.input_log().is_empty() && world.take_log().is_empty());
            assert_eq!(world.pending_events(), 0);
        }
        result.err()
    };
    let code = |code| Some(InputRejected { code });
    assert_eq!(rejected(mortal_causes()), None);
    assert_eq!(rejected(Vec::new()), None);
    assert_eq!(rejected(vec![cause("test:base", 1e-9)]), None);
    assert_eq!(rejected(vec![cause("test:base", 1e300)]), None);
    for per_year in [9.9e-10, 0.0, -0.5, f64::MIN_POSITIVE] {
        assert_eq!(
            rejected(vec![
                cause("test:old_age", 0.05),
                cause("test:base", per_year)
            ]),
            code(RejectCode::DeathRateOutOfRange),
            "{per_year}"
        );
    }
    assert_eq!(
        rejected(vec![
            cause("test:base", 0.5),
            cause("test:old_age", 0.05),
            cause("test:base", 0.1),
        ]),
        code(RejectCode::DeathCauseTwice)
    );
    // Both faults: the cause twice is named.
    assert_eq!(
        rejected(vec![cause("test:base", 0.0), cause("test:base", 0.1)]),
        code(RejectCode::DeathCauseTwice)
    );
    // The other checks of the start come first.
    let mut world = mortal();
    let again = world.submit(start(
        "test:mortal",
        vec![cause("test:base", 0.0)],
        Vec::new(),
    ));
    assert_eq!(again.err(), code(RejectCode::AlreadyStarted));
}

/// The year hashes of a run with deaths are those of the reference.
#[test]
fn the_year_hashes_see_the_deaths() {
    let run = run(mortal(), 2 * Y - 1);
    assert_eq!(
        run.out.year_hashes,
        [
            YearHash {
                year: 0,
                hash: MORTAL_YEAR_HASHES[0]
            },
            YearHash {
                year: 1,
                hash: MORTAL_YEAR_HASHES[1]
            },
        ]
    );
}

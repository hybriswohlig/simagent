//! Death through hazard budgets (ADR-0001, C4, ADR-0017 section 2): a sim dies of a cause
//! when its budget for that cause has run out, leaves for the archive, and what was planned
//! for it is cancelled. The causes come from the data: senescence and accident for
//! everyone, then the hereditary diseases (tests/disease.rs, tests/afflictions.rs).
//!
//! The expected bytes, times and hashes come from `tools/state_reference.py`, which runs
//! the model of `tools/world_reference.py`, written from the specification and not from
//! this code.
//!
//! Scenario "mortal": seed 0xff, founder tag `test:mortal`, test data with accident 0.5
//! and senescence `a` 0.05, `b` 0.001 per year for both sexes, a woman born one second and
//! a man born two seconds before the start. She dies of accident after about 4.29 years,
//! he of accident after about 5.66 years.

mod common;

use std::collections::BTreeMap;

use sim_core::ageing::bio_age;
use sim_core::body_contract::BodyParamKey;
use sim_core::canonical::YearHash;
use sim_core::data::Data;
use sim_core::data_id::{DataId, str_key};
use sim_core::finite::FiniteF64;
use sim_core::genome::{Genome, LocusAlleles, Sex};
use sim_core::ids::SimId;
use sim_core::input::{Couple, Fertility, Founder, Input, Start};
use sim_core::lineage::LineageKey;
use sim_core::log::{LogEntry, log_hash};
use sim_core::mortality;
use sim_core::params::WorldParams;
use sim_core::rng::{DrawKey, Draws, SubjectKey, WorldSeed, tags};
use sim_core::time::{SimDuration, SimTime};
use sim_core::world::{CensusRow, World, YearOut};

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
const MORTAL_AFTER_START: &str = "0c01ff01010000010000000000000000122d0a01030107fc87ff1d0005010200fe87ff1d0002010100fe87ff1d000100018088ff1daaa1e380010004020185d2bc82f8b2ead44d00aea2e2a9010007020285d2bc82f8b2ead44d009ae1f9c50100060202a0bb8ca1c58498a01c009aadc9f70100030201a0bb8ca1c58498a01c0008020101dded90d1a3f7cbf6b30100000000010000000000000000000203cbdbe1f0b893cbc91e0101000001000000010000019aada6adcbb284bf720000000000000000010000000000000000000000000000";
const MORTAL_SAVE_AFTER_START: &str = "53494d4147454e540118b786a9d2c6ffe0e288010c01ff01010000010000000000000000122d0a01030107fc87ff1d0005010200fe87ff1d0002010100fe87ff1d000100018088ff1daaa1e380010004020185d2bc82f8b2ead44d00aea2e2a9010007020285d2bc82f8b2ead44d009ae1f9c50100060202a0bb8ca1c58498a01c009aadc9f70100030201a0bb8ca1c58498a01c0008020101dded90d1a3f7cbf6b30100000000010000000000000000000203cbdbe1f0b893cbc91e0101000001000000010000019aada6adcbb284bf7200000000000000000100000000000000000000000000000703000200010002000300030002000401010100ff010b746573743a6d6f7274616c0000020100010f636f72653a646973656173655f61720200010301010f636f72653a646973656173655f6172020101000000000000000000122d0a00";
const MORTAL_AFTER_YEAR_5: &str = "0c01ff01010000010000000000000000122d0afeaffab301030101feb7f9d101000100018088ff1d11000000020101aaa1e380010085d2bc82f8b2ead44ddded90d1a3f7cbf6b30100000000010000000000000000000203aea2e2a9010085d2bc82f8b2ead44dcbdbe1f0b893cbc91e0101000001000000010000019aada6adcbb284bf720000000000000000010000000000000000000000";
const MORTAL_SAVE_AFTER_YEAR_5: &str = "53494d4147454e540118b786a9d2c6ffe0e288010c01ff01010000010000000000000000122d0afeaffab301030101feb7f9d101000100018088ff1d11000000020101aaa1e380010085d2bc82f8b2ead44ddded90d1a3f7cbf6b30100000000010000000000000000000203aea2e2a9010085d2bc82f8b2ead44dcbdbe1f0b893cbc91e0101000001000000010000019aada6adcbb284bf7200000000000000000100000000000000000000000101000f01010100ff010b746573743a6d6f7274616c0000020100010f636f72653a646973656173655f61720200010301010f636f72653a646973656173655f6172020101000000000000000000122d0a0600fee392ec89d4c588bf010291effa9384c0e6f4220489a5f5f9f6c6d7db320680f8c4f1c4adf6deb60108918bafd0aea798e8db010a8efd89f1cbace2d8a001";
const MORTAL_LOG: [&str; 14] = [
    "0100000102010002",
    "02000100010102010101dded90d1a3f7cbf6b301",
    "03000100020102010103cbdbe1f0b893cbc91e",
    "04fc87ff1d010002010103000201",
    "05fe87ff1d010001010102000201",
    "06fc8ffe3b010002010104fc87ff1d0202",
    "07fe8ffe3b010001010105fe87ff1d0202",
    "08fc97fd59010002010106fc8ffe3b0203",
    "09fe97fd59010001010107fe8ffe3b0203",
    "0afc9ffc77010002010108fc97fd590204",
    "0bfe9ffc77010001010109fe97fd590204",
    "0caaa1e38001010001020102000404dded90d1a3f7cbf6b30185d2bc82f8b2ead44d00040d636f72653a6163636964656e74",
    "0dfca7fb950101000201010afc9ffc770205",
    "0eaea2e2a901010002020103000404cbdbe1f0b893cbc91e85d2bc82f8b2ead44d00040d636f72653a6163636964656e74",
];
const MORTAL_LOG_HASH: u64 = 0xcb0a_2e01_d573_81d7;
const MORTAL_YEAR_HASHES: [u64; 6] = [
    0xbf11_16a0_9d84_b1fe,
    0x22e9_9a00_427e_b791,
    0x32b7_5e37_6f3d_5289,
    0xb6bd_d96c_4e31_3c00,
    0xdbd0_613a_ea0b_c591,
    0xa0b1_8964_be22_7e8e,
];
/// When she and he die of accident, in seconds.
const HER_DEATH: i64 = 135_030_869;
const HIS_DEATH: i64 = 178_014_359;
/// The keys of the codes `core:senescence` and `core:accident`.
const SENESCENCE: u64 = 0x1c40_6024_5423_1da0;
const ACCIDENT: u64 = 0x4da9_a997_8040_2905;
/// "tie": her senescence and her accident are due in this second.
const TIE_DUE: i64 = 118_372_196;
const TIE_SAVE: &str = "53494d4147454e5401189d96c6ffeed5b580190c01ff01010000010000000000000000122d0afe9ffc77020101fea7fb9501000100018088ff1d08000000010101c8ddf17000a0bb8ca1c58498a01cf0da979a8fd9f18a950100000000010000000000000000000101000701010100ff0108746573743a7469650000010100010f636f72653a646973656173655f6172020001000000000000000000122d0a0400dbf6e4d788cbb0d97702a2ebbf9b84b9be87ed010490c0e4eae1fcfc93b60106c4c98fc58bc7cff8d201";
const TIE_DEATH_ENTRY: &str = "06c8ddf170010001020102000404f0da979a8fd9f18a9501a0bb8ca1c58498a01c00040f636f72653a73656e657363656e6365";

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex digits"))
        .collect()
}

fn woman(born: i64) -> Founder {
    Founder {
        born: SimTime::from_secs(born),
        sex: Sex::Female,
        alleles: common::dummy(0, 1),
    }
}

fn man(born: i64) -> Founder {
    Founder {
        born: SimTime::from_secs(born),
        sex: Sex::Male,
        alleles: common::dummy(1, 1),
    }
}

fn start(tag: &str, founders: Vec<Founder>) -> Input {
    Input::Start(Start {
        world_seed: 0xff,
        founder_tag: DataId::new(tag).expect("valid"),
        world_params: WorldParams {
            hereditary_diseases: false,
            nutrition: Vec::new(),
        },
        founders,
        couples: Vec::new(),
        fertility: barren(),
    })
}

/// The test data of "mortal".
fn mortal_data() -> Data {
    common::death_data(0.5, Some(0.05), common::repository_diseases())
}

fn mortal() -> World {
    let mut world = World::new(mortal_data());
    world
        .submit(start("test:mortal", vec![woman(-1), man(-2)]))
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

/// `Start` plans per founder the first birthday and then one death per cause, senescence
/// before accident (ADR-0017 section 2).
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
    let run = run(mortal(), 6 * Y - 1);
    let entries: Vec<Vec<u8>> = run
        .log
        .iter()
        .map(|entry| postcard::to_allocvec(entry).expect("encodes"))
        .collect();
    assert_eq!(entries, MORTAL_LOG.map(hex));
    assert_eq!(log_hash(&run.log), MORTAL_LOG_HASH);
    assert_eq!(
        serde_json::to_string(&run.log[11]).expect("json"),
        format!(
            r#"{{"id":12,"t":{HER_DEATH},"subjects":[{{"sim":1}}],"causes":[{{"plan":{{"by":2,"planned_at":0}}}},{{"random":{{"tag":"0000000000000004","subject":"b3ed2fba3a2436dd","occasion":"4da9a997804f2905","detail":"0000000000000000"}}}}],"payload":{{"core:death":{{"cause":"core:accident"}}}}}}"#
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
    assert_eq!(run.world.canonical_bytes(), hex(MORTAL_AFTER_YEAR_5));
    let hashes: Vec<u64> = run.out.year_hashes.iter().map(|year| year.hash).collect();
    assert_eq!(hashes, MORTAL_YEAR_HASHES);
    assert_eq!(
        run.world.save(&run.out.year_hashes).expect("saves"),
        hex(MORTAL_SAVE_AFTER_YEAR_5)
    );

    // The census counts the seconds they lived: the second of death is not lived.
    let lived = |secs: i64| u64::try_from(secs).expect("positive");
    let row = |year: i64, living: u64, secs: i64| CensusRow {
        year,
        living,
        lived_secs: lived(secs),
    };
    assert_eq!(
        run.out.census,
        [
            row(0, 2, 2 * Y),
            row(1, 2, 2 * Y),
            row(2, 2, 2 * Y),
            row(3, 2, 2 * Y),
            row(4, 1, (HER_DEATH - 4 * Y) + Y),
            row(5, 0, HIS_DEATH - 5 * Y),
        ]
    );
}

/// A death does not touch the others: when she dies, his birthday and his two deaths stay.
#[test]
fn the_death_of_one_sim_leaves_the_others_alone() {
    let run = run(mortal(), HER_DEATH);
    let state = run.world.state();
    assert_eq!((state.living(), state.archive_len()), (1, 1));
    // The year end series, his birthday and his two deaths.
    assert_eq!(run.world.pending_events(), 4);
    assert_eq!(run.log.len(), 12);
    // One second earlier she lives, and her three events are pending.
    let before = self::run(mortal(), HER_DEATH - 1);
    assert_eq!(before.world.state().living(), 2);
    assert_eq!(before.world.pending_events(), 7);
}

/// A death before the first birthday cancels that birthday as well: with accident at 5
/// per year both die within the first year (their budgets of accident are about 2.15 and
/// 2.83), so no birthday is ever written.
#[test]
fn a_death_before_the_first_birthday_cancels_it() {
    let mut world = World::new(common::death_data(
        5.0,
        Some(0.05),
        common::repository_diseases(),
    ));
    world
        .submit(start("test:mortal", vec![woman(-1), man(-2)]))
        .expect("accepted");
    let run = run(world, 3 * Y - 1);
    let kinds: Vec<String> = run
        .log
        .iter()
        .map(|entry| {
            let payload = serde_json::to_value(&entry.payload).expect("json");
            payload
                .as_object()
                .and_then(|kind| kind.keys().next().cloned())
                .expect("a kind")
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "core:started",
            "core:founder_added",
            "core:founder_added",
            "core:death",
            "core:death"
        ]
    );
    assert!(run.log[3].t.as_secs() < Y - 2 && run.log[4].t.as_secs() < Y - 2);
    assert_eq!(run.world.pending_events(), 1, "only the year end series");
}

/// The world goes on without living sims: the year end series runs, the years are
/// counted, nothing else happens.
#[test]
fn a_world_without_the_living_goes_on() {
    let run = run(mortal(), 8 * Y - 1);
    assert_eq!(run.log.len(), 14);
    assert_eq!(run.out.year_hashes.len(), 8);
    assert_eq!(run.out.census[7].living, 0);
    assert_eq!(run.out.census[7].lived_secs, 0);
    assert_eq!(run.world.pending_events(), 1);
}

/// Loading and going on gives the uninterrupted run, whether the save was taken before
/// the deaths, between them or after them: the loaded world cancels the events of a
/// dying sim like the world that planned them.
#[test]
fn save_load_continue_across_the_deaths() {
    let end = 7 * Y - 1;
    let whole = run(mortal(), end);
    for stop in [
        0,
        Y - 1,
        HER_DEATH - 1,
        HER_DEATH,
        HIS_DEATH - 1,
        HIS_DEATH,
        6 * Y - 1,
    ] {
        let part = run(mortal(), stop);
        let bytes = part.world.save(&part.out.year_hashes).expect("saves");
        let (mut loaded, years) = World::load(&bytes, mortal_data()).expect("loads");
        assert_eq!(loaded.save(&years).expect("saves"), bytes, "stop {stop}");
        let mut out = YearOut {
            census: part.out.census.clone(),
            year_hashes: years,
        };
        loaded.advance_until(&mut out, SimTime::from_secs(end), &mut |_| false);
        let mut log = part.log;
        log.extend(loaded.take_log());
        assert_eq!(out.year_hashes, whole.out.year_hashes, "stop {stop}");
        assert_eq!(out.census, whole.out.census, "stop {stop}");
        assert_eq!(log_hash(&log), log_hash(&whole.log), "stop {stop}");
        assert_eq!(loaded.state_hash(), whole.world.state_hash(), "stop {stop}");
        assert_eq!(loaded.pending_events(), 1, "stop {stop}");
    }
}

/// Scenario "tie" of the reference: her senescence and her accident are due in the same
/// second (test data: accident 0.2, senescence `a` 0.8046949992648321, `b` 0.001). The
/// cause that comes first in the list wins: senescence, its event was planned first.
#[test]
fn of_two_causes_due_together_the_first_in_the_list_wins() {
    let data = || {
        common::death_data(
            0.2,
            Some(0.804_694_999_264_832_1),
            common::repository_diseases(),
        )
    };
    let mut world = World::new(data());
    world
        .submit(start("test:tie", vec![woman(-1)]))
        .expect("accepted");
    let run = run(world, 4 * Y - 1);
    let died: Vec<&LogEntry> = run
        .log
        .iter()
        .filter(|entry| {
            serde_json::to_string(&entry.payload)
                .expect("json")
                .contains("core:death")
        })
        .collect();
    assert_eq!(died.len(), 1);
    assert_eq!(died[0].t.as_secs(), TIE_DUE);
    assert_eq!(
        postcard::to_allocvec(died[0]).expect("encodes"),
        hex(TIE_DEATH_ENTRY)
    );
    assert_eq!(run.world.pending_events(), 1);
    let bytes = run.world.save(&run.out.year_hashes).expect("saves");
    assert_eq!(bytes, hex(TIE_SAVE));
    // Loading knows the rule: the save with the reason changed to accident, due in the
    // same second but later in the list, is no state the core reaches.
    assert!(World::load(&bytes, data()).is_ok());
    let senescence = postcard::to_allocvec(&SENESCENCE).expect("encodes");
    let accident = postcard::to_allocvec(&ACCIDENT).expect("encodes");
    let at: Vec<usize> = (0..bytes.len())
        .filter(|&i| bytes[i..].starts_with(&senescence))
        .collect();
    assert_eq!(at.len(), 1, "in the archive");
    let mut changed = bytes[..at[0]].to_vec();
    changed.extend(&accident);
    changed.extend(&bytes[at[0] + senescence.len()..]);
    assert_eq!(
        World::load(&changed, data()).err(),
        Some(sim_core::world::LoadError::Inconsistent)
    );
}

/// A hazard so small that the budget lasts 100 000 years or more means never (ADR-0002):
/// accident at 1e-9 per year plans no event; senescence practically off (`a` 1e-15) is
/// due only after tens of thousands of years, so she lives through two hundred years with
/// her birthdays.
#[test]
fn a_death_that_is_never_due_is_not_planned() {
    let mut world = World::new(common::death_data(
        1e-9,
        None,
        common::repository_diseases(),
    ));
    world
        .submit(start("test:mortal", vec![woman(-1)]))
        .expect("accepted");
    assert_eq!(
        world.pending_events(),
        3,
        "the year end series, a birthday and her far senescence"
    );
    let run = run(world, 200 * Y - 1);
    assert_eq!(run.world.state().living(), 1);
    assert_eq!(run.log.len(), 2 + 200);
}

/// The year hashes of a run with deaths are those of the reference.
#[test]
fn the_year_hashes_see_the_deaths() {
    let run = run(mortal(), 6 * Y - 1);
    let expected: Vec<YearHash> = (0_i64..)
        .zip(MORTAL_YEAR_HASHES)
        .map(|(year, hash)| YearHash { year, hash })
        .collect();
    assert_eq!(run.out.year_hashes, expected);
}

/// The fixture with traits (`tests/golden/data_traits`) with `test:growth` as the trait of
/// longevity (genetics specification 7.2, ADR-0017 section 2).
fn longevity_data() -> Data {
    let (mut calibration, genome_map) = common::files_from(
        include_str!("../../../tests/golden/data_traits/calibration/genetics.json"),
        include_str!("../../../tests/golden/data_traits/genetics/genome_map.json"),
    );
    calibration["longevity_trait"] = serde_json::json!("test:growth");
    Data::new(common::definitions(calibration, genome_map).expect("definitions")).expect("checked")
}

/// The alleles of a founder at the two loci of `test:growth`, maternal and paternal.
fn growth_alleles(g1: [u8; 2], g2: [u8; 2]) -> Vec<LocusAlleles> {
    let locus = |id: &str, copies: [u8; 2]| LocusAlleles {
        locus: DataId::new(id).expect("id"),
        copies: copies.to_vec(),
    };
    vec![locus("test:g1", g1), locus("test:g2", g2)]
}

/// A trait of longevity scales senescence (genetics specification 7.2): a founder dies of
/// it where the closed form of `mortality` puts it with `m = e^(−σ_g · g)` and `g` the
/// genetic value of the trait; accident stays as it is. On the fixture with `test:growth`
/// as the trait; by hand from 4.1 (`test:g1` additive with 0.5 and `d_ref` 1.25, `test:g2`
/// with 0.125 and weight 2 around 0.5, `σ_ref` 2): two copies at `test:g1` give
/// `(0.5 · 0.75 + 0.25 · (−0.5)) / 2 = 0.125`, none at either `(0.5 · (−1.25) + 0.25 ·
/// (−0.5)) / 2 = −0.375`. Without the trait (`m` = 1) both die at other times.
#[test]
fn a_trait_of_longevity_scales_senescence() {
    let data = longevity_data();
    let tag = DataId::new("test:longevity").expect("valid");
    let born = SimTime::from_secs(-70 * Y);
    let founders = [
        (Sex::Female, growth_alleles([1, 1], [0, 0]), 0.125),
        (Sex::Male, growth_alleles([0, 0], [0, 0]), -0.375),
    ];
    let genetics = data.genetics();
    let growth = DataId::new("test:growth").expect("id");
    let draws = Draws::new(WorldSeed::new(0xff));
    let x0 = SimDuration::from_secs(70 * Y).as_years();
    let due = |age: f64| {
        SimTime::START
            .checked_add(SimDuration::from_years_ceil(age - x0).expect("in range"))
            .expect("in range")
    };
    let mut expected = Vec::new();
    for (index, (sex, alleles, value)) in (0..).zip(&founders) {
        let genome = Genome::with_alleles(*sex, alleles, &data).expect("fits");
        assert_eq!(genome.genetic_value(&growth, &data), Some(*value));
        let (gompertz, per_year) = match sex {
            Sex::Female => (genetics.senescence_female, genetics.accident_female),
            Sex::Male => (genetics.senescence_male, genetics.accident_male),
        };
        let budget = |code: &str| {
            let key = DrawKey {
                tag: tags::DEATH_BUDGET,
                subject: SubjectKey::from(LineageKey::founder(&tag, index)),
                occasion: str_key(&DataId::new(code).expect("id")),
                detail: 0,
            };
            draws.exponential(key, 1.0)
        };
        let senescence = |m: f64| {
            due(mortality::senescence_age(
                x0,
                budget("core:senescence"),
                gompertz,
                m,
            ))
        };
        let m = mortality::multiplier(Some(*value), genetics.longevity_sigma_g);
        let with_trait = senescence(m);
        let without = senescence(1.0);
        let accident = due(mortality::accident_age(
            x0,
            budget("core:accident"),
            per_year,
        ));
        assert!(with_trait < accident, "senescence comes first");
        // A positive value makes the sim live longer, a negative one shorter.
        assert_eq!(with_trait > without, *value > 0.0);
        expected.push((with_trait, without));
    }

    let world_with = |data: Data| {
        let mut world = World::new(data);
        let founders = founders
            .iter()
            .map(|(sex, alleles, _)| Founder {
                born,
                sex: *sex,
                alleles: alleles.clone(),
            })
            .collect();
        world
            .submit(start("test:longevity", founders))
            .expect("accepted");
        let last = expected
            .iter()
            .map(|(with_trait, without)| (*with_trait).max(*without))
            .max()
            .expect("two");
        run(world, last.as_secs()).world
    };
    let deaths = |world: &World| -> Vec<(Option<SimTime>, Option<String>)> {
        world
            .state()
            .pedigree()
            .persons
            .iter()
            .map(|person| {
                let cause = person.cause_of_death.as_ref();
                (person.died, cause.map(|code| code.as_str().to_owned()))
            })
            .collect()
    };
    let senescence = || Some("core:senescence".to_owned());
    assert_eq!(
        deaths(&world_with(longevity_data())),
        expected
            .iter()
            .map(|(with_trait, _)| (Some(*with_trait), senescence()))
            .collect::<Vec<_>>()
    );
    // The same founders on the fixture without the trait.
    let (calibration, genome_map) = common::files_from(
        include_str!("../../../tests/golden/data_traits/calibration/genetics.json"),
        include_str!("../../../tests/golden/data_traits/genetics/genome_map.json"),
    );
    let plain = Data::new(common::definitions(calibration, genome_map).expect("definitions"))
        .expect("checked");
    assert_eq!(
        deaths(&world_with(plain)),
        expected
            .iter()
            .map(|(_, without)| (Some(*without), senescence()))
            .collect::<Vec<_>>()
    );
}

/// ADR-0005, ADR-0017: the form age of every sim at every year end is never smaller than at
/// the one before, with the real biological age (7.4) from the offset of its trait of
/// longevity (7.2): `form_age(b) = min(b, 130)` (10.2) of `bio_age(age, Δ, w)` with
/// `Δ = −σ_g · ĝ / b`. A world on the fixture with `test:growth` as the trait, three couples
/// of different genotypes and a fertility of 0.25 per month, sixty years: founders, children
/// and the dead.
#[test]
fn form_age_never_decreases_over_life() {
    let data = longevity_data();
    let genetics = data.genetics();
    let growth = DataId::new("test:growth").expect("id");
    let founder = |years: i64, sex: Sex, g1: [u8; 2], g2: [u8; 2]| Founder {
        born: SimTime::from_secs(-years * Y),
        sex,
        alleles: growth_alleles(g1, g2),
    };
    let mut world = World::new(data.clone());
    world
        .submit(Input::Start(Start {
            world_seed: 0xff,
            founder_tag: DataId::new("test:form").expect("valid"),
            world_params: WorldParams {
                hereditary_diseases: false,
                nutrition: Vec::new(),
            },
            founders: vec![
                founder(20, Sex::Female, [1, 1], [1, 1]),
                founder(22, Sex::Male, [0, 0], [0, 0]),
                founder(19, Sex::Female, [1, 0], [0, 0]),
                founder(25, Sex::Male, [1, 1], [0, 1]),
                founder(60, Sex::Female, [0, 0], [1, 0]),
                founder(62, Sex::Male, [0, 1], [1, 1]),
            ],
            couples: vec![
                Couple { woman: 0, man: 1 },
                Couple { woman: 2, man: 3 },
                Couple { woman: 4, man: 5 },
            ],
            fertility: Fertility {
                per_month: FiniteF64::try_new(0.25).expect("finite"),
                from_years: 18,
                until_years: 45,
                pregnancy_months: 10,
            },
        }))
        .expect("accepted");
    let limit = BodyParamKey::BodyFormAge.spec().max;
    let mut last: BTreeMap<SimId, f64> = BTreeMap::new();
    let (mut checked, mut offset, mut born) = (0, false, 0);
    let mut out = YearOut::default();
    for year in 1..=60 {
        let now = SimTime::from_secs(year * Y - 1);
        world.advance_until(&mut out, now, &mut |_| false);
        world.take_log();
        for person in world.state().pedigree().persons {
            if person.died.is_some() {
                continue;
            }
            let genome = Genome::with_alleles(person.sex, &person.alleles, &data).expect("fits");
            let value = genome.genetic_value(&growth, &data).expect("the trait");
            let b = match person.sex {
                Sex::Female => genetics.senescence_female.b,
                Sex::Male => genetics.senescence_male.b,
            };
            let delta = mortality::age_offset(Some(value), genetics.longevity_sigma_g, b);
            let age = SimDuration::from_secs(now.as_secs() - person.born.as_secs()).as_years();
            let bio = bio_age(age, delta, genetics.bio_age_weight);
            let form = if bio < limit { bio } else { limit };
            if let Some(before) = last.insert(person.sim, form) {
                assert!(form >= before, "sim {:?} in year {year}", person.sim);
                checked += 1;
            }
            offset |= age > 25.0 && bio != age;
            born += usize::from(person.mother.is_some() && year == 60);
        }
    }
    assert!(checked > 300, "{checked} comparisons");
    assert!(offset, "the trait moves the biological age");
    assert!(born > 0, "children are among them");
    assert!(world.state().archive_len() > 0, "and some died");
}

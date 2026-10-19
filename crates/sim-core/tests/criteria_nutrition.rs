//! P6, the shortage cohort (genetics specification 8.2 P6 and 5.4; ADR-0020 section 8; Grok
//! K4 (a)): a fork with the nutrition index of year `Y` = 1 at 0.7 makes every person of birth
//! cohort `Y − 1` who becomes an adult in both runs exactly 1.50 cm smaller, and every person of
//! cohort `Y` conceived before `T_Y` by `k · 0.3 · (Y + 1 − birth)`, `k` = 5 cm, within 1e-9 cm,
//! on the pure height at 18 (no new draw). A town of 1000 from the repository's pool, immigration
//! off, no capacity, the switch on, gate seed `0x5eed_0006_0000_0001`, tag `core:criteria_p6`,
//! to the end of year 20. The fork as ADR-0001 section 6 defines it: the same inputs from t0, the
//! override stamped `T_Y − 1 s`.
//!
//! Every expected value comes from the independent reference (`tools/p6_reference.py`): the
//! units below with their heights, the start, state and log hashes. Cohort 0 holds 12 adults: 9
//! children of the pool's pregnancies born in the town, the units of K4 (town-born), and 3
//! persons born at the arrival second, whose birth is the pool's history; the rule holds for all
//! 12. Cohort 1 holds the 9 conceived before `T_Y` (a birth 10 months after its conception).
//! Negative twin: the override on year 3 (forked at `T_3 − 1 s`) costs cohort 0 only 0.75 cm, so
//! the 1.50 assertion is red there.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::finite::FiniteF64;
use sim_core::hash::stable_hash64;
use sim_core::ids::InputSeq;
use sim_core::input::{ForkOverride, Input, InputRecord, PoolStart, Start};
use sim_core::log::log_hash;
use sim_core::params::{NutritionYear, ParamChange, WorldParams};
use sim_core::time::SimTime;
use sim_core::world::{World, YearOut};

const SEED: u64 = 0x5eed_0006_0000_0001;
const YEAR: i64 = 31_449_600;
const MONTH: i64 = 2_419_200;
/// The end of year 20.
const END: i64 = 21 * YEAR - 1;
/// The tolerance of the specification, in cm.
const TOLERANCE: f64 = 1e-9;

/// The units of the reference: cohort, lineage key, birth second, town-born, the pure height
/// at 18 in the base run and in the fork (bits), the expected difference (bits).
const UNITS: [(u8, u64, i64, bool, u64, u64, u64); 21] = [
    (
        1,
        0x0d3c71c3d06eb8ea,
        48384000,
        true,
        0x406495aacc2a4d9e,
        0x40647f8369b4263c,
        0x3fe6276276276276,
    ),
    (
        1,
        0x1678f6b6a0674118,
        53222400,
        true,
        0x40656e5b567122fa,
        0x40655f966a225e0e,
        0x3fdd89d89d89d89e,
    ),
    (
        0,
        0x2ce06cf9beca5c12,
        7257600,
        true,
        0x4064ee96244347c2,
        0x4064be96244347c2,
        0x3ff8000000000000,
    ),
    (
        1,
        0x2d25637bd36d1a10,
        38707200,
        true,
        0x4066be13ac57e497,
        0x406699275d92f848,
        0x3ff2762762762762,
    ),
    (
        0,
        0x306c42c6b1d42657,
        0,
        false,
        0x4065a6bbd2f2517a,
        0x406576bbd2f2517a,
        0x3ff8000000000000,
    ),
    (
        0,
        0x36970f5db1682406,
        4838400,
        true,
        0x4064d0603f1e0f2b,
        0x4064a0603f1e0f2b,
        0x3ff8000000000000,
    ),
    (
        0,
        0x38305dbb1bc85d04,
        2419200,
        true,
        0x4063ddc1b5a2657e,
        0x4063adc1b5a2657e,
        0x3ff8000000000000,
    ),
    (
        1,
        0x463f8cf172a53e1c,
        48384000,
        true,
        0x4062befa5a880a7a,
        0x4062a8d2f811e318,
        0x3fe6276276276276,
    ),
    (
        1,
        0x510da8dc47a6ea70,
        41126400,
        true,
        0x4065fbf7336b2e81,
        0x4065dabc1fb9f36d,
        0x3ff09d89d89d89d9,
    ),
    (
        0,
        0x6f8ac687cd55eda0,
        4838400,
        true,
        0x4063e32be1327d59,
        0x4063b32be1327d59,
        0x3ff8000000000000,
    ),
    (
        0,
        0x77bed138cb1845b9,
        9676800,
        true,
        0x4063894262ce4139,
        0x4063594262ce4139,
        0x3ff8000000000000,
    ),
    (
        1,
        0x796b283e692bd4ee,
        45964800,
        true,
        0x4066ed5e280fed7c,
        0x4066d3858a8614de,
        0x3fe9d89d89d89d8a,
    ),
    (
        1,
        0x8487496cf7377073,
        31449600,
        true,
        0x4066db75df98334a,
        0x4066ab75df98334a,
        0x3ff8000000000000,
    ),
    (
        0,
        0xa42a4271e7392149,
        0,
        false,
        0x4065dd4ce774a735,
        0x4065ad4ce774a735,
        0x3ff8000000000000,
    ),
    (
        0,
        0xad55a952df2b1d4d,
        7257600,
        true,
        0x406482f2d972e04d,
        0x406452f2d972e04d,
        0x3ff8000000000000,
    ),
    (
        0,
        0xb6aed0146a1e345e,
        19353600,
        true,
        0x406539cc6e53f918,
        0x406509cc6e53f918,
        0x3ff8000000000000,
    ),
    (
        0,
        0xc6d3945887d4316e,
        14515200,
        true,
        0x4067a1295430cb2a,
        0x406771295430cb2a,
        0x3ff8000000000000,
    ),
    (
        1,
        0xcdd882d9945571e7,
        48384000,
        true,
        0x406713d782cbf577,
        0x4066fdb02055ce15,
        0x3fe6276276276276,
    ),
    (
        1,
        0xe847ffc7b775504a,
        38707200,
        true,
        0x4065b55376784d7c,
        0x4065906727b3612d,
        0x3ff2762762762762,
    ),
    (
        0,
        0xeec8c7a3752ccb4f,
        7257600,
        true,
        0x4065714c27f2b0ea,
        0x4065414c27f2b0ea,
        0x3ff8000000000000,
    ),
    (
        0,
        0xfcf15502e035b615,
        0,
        false,
        0x40660531b84e23e4,
        0x4065d531b84e23e4,
        0x3ff8000000000000,
    ),
];
const START_INPUT_HASH: u64 = 0x485cb1f43a84dca1;
const BASE_STATE: u64 = 0xd83bf1042b95f9be;
const BASE_LOG: u64 = 0x00bbf2c45c2a8cbd;
const FORK_STATE: u64 = 0x0b00a95f925fb98b;
const FORK_LOG: u64 = 0xb59b0a39b29fafca;
const TWIN_LATE_STATE: u64 = 0x5f10bc421d95b110;
const TWIN_LATE_LOG: u64 = 0xa779ef653bd7acca;
const IDENTICAL: usize = 1020;
const PERSONS: usize = 1295;

fn start(data: &Data) -> Start {
    Start {
        world_seed: SEED,
        founder_tag: DataId::new("core:criteria_p6").expect("id"),
        world_params: WorldParams {
            hereditary_diseases: true,
            nutrition: Vec::new(),
        },
        founders: Vec::new(),
        couples: Vec::new(),
        capacity: None,
        pool: PoolStart::drawn(data, SEED, 1000, false),
    }
}

/// A run to the end of year 20; with an override of `year` stamped in the last second
/// before that year, as a fork of ADR-0001 section 6 replays it.
fn run(year: Option<i64>) -> World {
    let data = common::repository_data();
    let start = start(&data);
    let mut world = World::new(data);
    world.submit(Input::Start(start)).expect("accepted");
    let mut out = YearOut::default();
    if let Some(year) = year {
        world.advance_until(&mut out, SimTime::from_secs(year * YEAR - 1), &mut |_| {
            false
        });
        let change = ParamChange::NutritionYear(NutritionYear {
            year,
            index: FiniteF64::try_new(0.7).expect("finite"),
        });
        world
            .submit(Input::ForkOverride(ForkOverride {
                changes: vec![change],
            }))
            .expect("accepted");
    }
    world.advance_until(&mut out, SimTime::from_secs(END), &mut |_| false);
    world
}

/// Each person's sim by lineage key, with birth and death.
fn persons(world: &World) -> BTreeMap<u64, (sim_core::ids::SimId, SimTime, Option<SimTime>)> {
    world
        .pedigree()
        .persons
        .iter()
        .map(|person| {
            (
                person.lineage_key.get(),
                (person.sim, person.born, person.died),
            )
        })
        .collect()
}

/// The pure height at 18 of the person with lineage key `key`.
fn height_at_18(world: &World, key: u64) -> f64 {
    let (sim, born, _) = persons(world)[&key];
    world
        .state()
        .pure_height(sim, born + sim_core::time::YEAR * 18)
        .expect("a pure height")
}

/// The loss the rule gives a unit of cohort `cohort` born at `born` (5.4): 1.50 cm for cohort
/// 0, `5 · 0.3 · (2 − birth in years)` for cohort 1.
fn rule(cohort: u8, born: i64) -> f64 {
    if cohort == 0 {
        1.5
    } else {
        let seconds = |secs: i64| f64::from(i32::try_from(secs).expect("within a few years"));
        5.0 * 0.3 * (2.0 - seconds(born) / seconds(YEAR))
    }
}

#[test]
fn the_shortage_cohort_loses_exactly_what_the_rule_says() {
    let data = common::repository_data();
    let record = postcard::to_allocvec(&InputRecord {
        t: SimTime::from_secs(-1),
        seq: InputSeq::try_from(1).expect("valid"),
        input: Input::Start(start(&data)),
    })
    .expect("encodes");
    assert_eq!(stable_hash64(&record), START_INPUT_HASH);
    let mut base = run(None);
    let mut fork = run(Some(1));
    assert_eq!(base.state_hash(), BASE_STATE);
    assert_eq!(fork.state_hash(), FORK_STATE);
    assert_eq!(log_hash(&base.take_log()), BASE_LOG);
    assert_eq!(log_hash(&fork.take_log()), FORK_LOG);
    // Whoever was conceived before T_Y is the same person, born and dying at the same second.
    let (in_base, in_fork) = (persons(&base), persons(&fork));
    assert_eq!(in_base.len(), PERSONS);
    // A birth falls `pregnancy_months` months after its conception (the core's rule); the
    // reference's value of the data is 10.
    let months = data.demography().fertility.pregnancy_months;
    assert_eq!(months, 10);
    let pregnancy = i64::from(months) * MONTH;
    let conceived_before = |born: SimTime| born.as_secs() - pregnancy < YEAR;
    let identical: Vec<_> = in_base
        .iter()
        .filter(|(_, (_, born, _))| conceived_before(*born))
        .collect();
    assert_eq!(identical.len(), IDENTICAL);
    for (key, (_, born, died)) in identical {
        let (_, other_born, other_died) = in_fork[key];
        assert_eq!((born, died), (&other_born, &other_died), "{key:016x}");
    }
    // The cohorts are the reference's units: cohort 0 everyone born in year 0 who is an adult in
    // both runs, cohort 1 everyone born in year 1 conceived before T_Y.
    let adult = |born: SimTime, died: Option<SimTime>| {
        died.is_none_or(|died| died.as_secs() >= born.as_secs() + 18 * YEAR)
    };
    let cohort = |which: i64| -> BTreeSet<u64> {
        in_base
            .iter()
            .filter(|(key, (_, born, died))| {
                let (_, _, other_died) = in_fork[key];
                born.as_secs().div_euclid(YEAR) == which
                    && conceived_before(*born)
                    && adult(*born, *died)
                    && adult(*born, other_died)
            })
            .map(|(key, _)| *key)
            .collect()
    };
    for which in [0, 1] {
        let units: BTreeSet<u64> = UNITS
            .iter()
            .filter(|unit| i64::from(unit.0) == which)
            .map(|unit| unit.1)
            .collect();
        assert_eq!(cohort(which), units, "cohort {which}");
    }
    // K4 counts the town-born: whoever was not born in the pool (an arrival's birth at the
    // arrival second is the pool's history). Cohort 0: 9 children of the pool's pregnancies
    // born in the town, 3 arrivals; each as the reference says.
    let pedigree = base.pedigree();
    let born_in_town = |key: u64| {
        pedigree
            .persons
            .iter()
            .find(|person| person.lineage_key.get() == key)
            .is_some_and(|person| {
                person
                    .pool
                    .as_ref()
                    .is_none_or(|pool| pool.born_pool.is_none())
            })
    };
    for (which, key, _, town_born, ..) in UNITS {
        assert_eq!(born_in_town(key), town_born, "{which} {key:016x}");
    }
    let cohort_0: Vec<_> = UNITS.iter().filter(|unit| unit.0 == 0).collect();
    assert_eq!(
        cohort_0.iter().filter(|unit| born_in_town(unit.1)).count(),
        9
    );
    assert_eq!(
        cohort_0.iter().filter(|unit| !born_in_town(unit.1)).count(),
        3
    );
    for (which, key, born, _, base_bits, fork_bits, expected_bits) in UNITS {
        let (ours, theirs) = (height_at_18(&base, key), height_at_18(&fork, key));
        // Against the reference's heights, relative 1e-12: `sim-math` keeps `exp` and its
        // relatives within 1 ulp of the correctly rounded value the reference uses (ADR-0003),
        // so bits may differ by a few ulp; the criterion itself is the 1e-9 cm below.
        let close = |ours: f64, reference: f64| {
            let scale = if reference.abs() > 1.0 {
                reference.abs()
            } else {
                1.0
            };
            (ours - reference).abs() <= 1e-12 * scale
        };
        assert!(close(ours, f64::from_bits(base_bits)), "{key:016x} base");
        assert!(close(theirs, f64::from_bits(fork_bits)), "{key:016x} fork");
        let expected = f64::from_bits(expected_bits);
        assert!(
            (expected - rule(which, born)).abs() <= 1e-12,
            "{key:016x} rule"
        );
        assert!(
            (ours - theirs - expected).abs() <= TOLERANCE,
            "{key:016x} loss"
        );
    }
}

/// The negative twin: the shortage two years later (year 3, forked at `T_3 − 1 s`) costs each
/// unit what 5.4 gives for that year: cohort 0 0.75 cm (the window 0.5 of ages 2 to 5), cohort 1
/// `0.75 · b` cm (`b` the birth in years: the part of year 3 before age 2 at weight 1, the rest at
/// 0.5); so the rule of P6 is missed for every unit.
#[test]
fn a_shortage_two_years_later_misses_the_rule() {
    let base = run(None);
    let mut twin = run(Some(3));
    assert_eq!(twin.state_hash(), TWIN_LATE_STATE);
    assert_eq!(log_hash(&twin.take_log()), TWIN_LATE_LOG);
    let years = |secs: i64| f64::from(i32::try_from(secs).expect("within a few years"));
    for (which, key, born, ..) in UNITS {
        let loss = height_at_18(&base, key) - height_at_18(&twin, key);
        let expected = if which == 0 {
            0.75
        } else {
            0.75 * (years(born) / years(YEAR))
        };
        assert!(
            (loss - expected).abs() <= TOLERANCE,
            "{key:016x}: {loss} against {expected}"
        );
        assert!((loss - rule(which, born)).abs() > TOLERANCE, "{key:016x}");
    }
}

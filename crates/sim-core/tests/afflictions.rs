//! The hereditary diseases of the repository's data in the world (genetics specification
//! 3.1 to 3.4, ADR-0017 section 3): who has which disease by genotype, sex and draws, from
//! what age, and when it kills while the switch is on.
//!
//! The expected values come from `tools/state_reference.py` (scenario "afflicted"), which
//! runs the model of `tools/world_reference.py`, written from the specification and not
//! from this code.
//!
//! Scenario "afflicted": seed 0xff, founder tag `test:afflicted`, the switch on from the
//! start, on test data where only the diseases kill (`common::only_diseases_data`), nobody
//! conceives, ten founders born a second before the
//! start (the ninth 30 years before): a man with the allele of `core:disease_xr` on his X,
//! a woman with it once (a carrier) and one with it twice; a man and a woman with
//! `core:disease_mt`; a woman with `core:disease_ad_late` once; a woman and a man with
//! `core:disease_ad_partial` once; a man with `core:disease_ar` twice and a woman with it
//! once (a carrier).

mod common;

use sim_core::data_id::DataId;
use sim_core::genome::{LocusAlleles, Sex};
use sim_core::input::{Founder, Input, Start};
use sim_core::log::LogEntry;
use sim_core::params::WorldParams;
use sim_core::time::SimTime;
use sim_core::world::{World, YearOut};

const Y: i64 = 31_449_600;

fn founder(born: i64, sex: Sex, locus: &str, copies: &[u8]) -> Founder {
    Founder {
        born: SimTime::from_secs(born),
        sex,
        alleles: vec![LocusAlleles {
            locus: DataId::new(locus).expect("id"),
            copies: copies.to_vec(),
        }],
    }
}

/// "afflicted" on the repository's data with the switch `on`.
fn afflicted(tag: &str, on: bool, founders: Vec<Founder>) -> World {
    let mut world = World::new(common::only_diseases_data());
    world
        .submit(Input::Start(Start {
            world_seed: 0xff,
            founder_tag: DataId::new(tag).expect("valid"),
            world_params: WorldParams {
                hereditary_diseases: on,
                nutrition: Vec::new(),
            },
            founders,
            couples: Vec::new(),
        }))
        .expect("accepted");
    world
}

fn the_ten() -> Vec<Founder> {
    vec![
        founder(-1, Sex::Male, "core:disease_xr", &[1]),
        founder(-1, Sex::Female, "core:disease_xr", &[0, 1]),
        founder(-1, Sex::Female, "core:disease_xr", &[1, 1]),
        founder(-1, Sex::Male, "core:disease_mt", &[1]),
        founder(-1, Sex::Female, "core:disease_mt", &[1]),
        founder(-1, Sex::Female, "core:disease_ad_late", &[0, 1]),
        founder(-1, Sex::Female, "core:disease_ad_partial", &[1, 0]),
        founder(-1, Sex::Male, "core:disease_ad_partial", &[1, 0]),
        founder(-30 * Y, Sex::Male, "core:disease_ar", &[1, 1]),
        founder(-1, Sex::Female, "core:disease_ar", &[1, 0]),
    ]
}

/// (second, sim, cause) of every death in the log.
fn deaths(log: &[LogEntry]) -> Vec<(i64, u64, String)> {
    log.iter()
        .filter_map(|entry| {
            let payload = serde_json::to_value(&entry.payload).expect("json");
            let cause = payload
                .get("core:death")?
                .get("cause")?
                .as_str()?
                .to_owned();
            let subjects = serde_json::to_value(&entry.subjects).expect("json");
            let sim = subjects[0]["sim"].as_u64()?;
            Some((entry.t.as_secs(), sim, cause))
        })
        .collect()
}

/// Each lethal disease kills at the time of the reference: the man with the X allele
/// and the woman with it twice (haemophilia, from birth, two steps), the women with the
/// dominant late diseases after their onsets (drawn and held, 3.1), the man with cystic
/// fibrosis. The carriers, the mitochondrial disease (visible, and here not penetrant)
/// and the man with `core:disease_ad_partial` (penetrance 0.1, not penetrant) never die
/// of a disease; with no cause of `Start` they live on. Over these 170 years the market
/// pairs the founders once they are 16 and children are born; only the founders count
/// here (their deaths are planned at the start, `tools/state_reference.py`).
#[test]
fn the_lethal_diseases_kill_by_genotype_sex_and_onset() {
    let mut world = afflicted("test:afflicted", true, the_ten());
    let mut log = world.take_log();
    let mut out = YearOut::default();
    world.advance_until(&mut out, SimTime::from_secs(5_373_003_592), &mut |_| false);
    log.extend(world.take_log());
    let died = |t: i64, sim: u64, code: &str| (t, sim, code.to_owned());
    let founders: Vec<(i64, u64, String)> = deaths(&log)
        .into_iter()
        .filter(|(_, sim, _)| *sim <= 10)
        .collect();
    assert_eq!(
        founders,
        [
            died(1_193_979_596, 9, "core:disease_ar"),
            died(1_519_743_745, 6, "core:disease_ad_late"),
            died(2_182_145_604, 1, "core:disease_xr"),
            died(3_763_342_717, 3, "core:disease_xr"),
            died(5_373_003_592, 7, "core:disease_ad_partial"),
        ]
    );
    let alive = |sim: u64| {
        world
            .state()
            .pedigree()
            .persons
            .iter()
            .any(|person| person.sim.get() == sim && person.died.is_none())
    };
    let living: Vec<u64> = (1..=10).filter(|sim| alive(*sim)).collect();
    assert_eq!(living, [2, 4, 5, 8, 10]);
}

/// The same founders with the switch off: the diseases are fixed at the start all the
/// same (the draws do not depend on the switch, 3.1), but nobody dies of one, neither a
/// founder nor a child of theirs.
#[test]
fn with_the_switch_off_nobody_dies_of_a_disease() {
    let mut world = afflicted("test:afflicted", false, the_ten());
    world.take_log();
    let mut out = YearOut::default();
    world.advance_until(&mut out, SimTime::from_secs(5_400_000_000), &mut |_| false);
    assert_eq!(deaths(&world.take_log()), []);
    let founders_alive = world
        .state()
        .pedigree()
        .persons
        .iter()
        .filter(|person| person.sim.get() <= 10 && person.died.is_none())
        .count();
    assert_eq!(founders_alive, 10);
}

/// A visible disease never kills (3.1): forty men with the mitochondrial allele, half of
/// them affected by the penetrance of men, all live through two hundred years with the
/// switch on, nothing else acting on them.
#[test]
fn a_visible_disease_never_kills() {
    let men = (0..40)
        .map(|_| founder(-1, Sex::Male, "core:disease_mt", &[1]))
        .collect();
    let mut world = afflicted("test:visible", true, men);
    world.take_log();
    let mut out = YearOut::default();
    world.advance_until(&mut out, SimTime::from_secs(200 * Y), &mut |_| false);
    assert_eq!(deaths(&world.take_log()), []);
    assert_eq!(world.state().living(), 40);
}

/// The projection right after the start has the hash of the reference: the afflictions
/// with their onsets, two of them drawn from the normal of 3.1, and their exposures.
#[test]
fn the_afflictions_are_those_of_the_reference() {
    let world = afflicted("test:afflicted", true, the_ten());
    assert_eq!(world.state_hash(), 0xcdd4_eb8e_f356_6cb6);
}

/// A save of "afflicted" loads and gives the same bytes, at the start and after the first
/// death, so the loader accepts the afflictions the world makes.
#[test]
fn a_save_of_the_afflicted_loads() {
    let mut world = afflicted("test:afflicted", true, the_ten());
    world.take_log();
    let mut out = YearOut::default();
    for until in [0, 1_193_979_596, 1_519_743_745] {
        world.advance_until(&mut out, SimTime::from_secs(until), &mut |_| false);
        world.take_log();
        let bytes = world.save(&out.year_hashes).expect("saves");
        let (loaded, years) = World::load(&bytes, common::only_diseases_data()).expect("loads");
        assert_eq!(loaded.save(&years).expect("saves"), bytes, "{until}");
    }
}

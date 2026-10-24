//! The neutral environment of the sheet of portraits (ADR-0021 G10 and its addendum of
//! 2026-10-24; Grok's round 37, B7 b): for the sheet `z = √h² · G`, `C = 0` and `E = 0`, so
//! the same genes in two persons give the same body. It is a way of the presentation only:
//! the bodies of every run keep both environments.

mod common;

use sim_core::data_id::DataId;
use sim_core::genome::{LocusAlleles, Sex};
use sim_core::ids::SimId;
use sim_core::input::{Founder, Input, Start};
use sim_core::params::WorldParams;
use sim_core::time::SimTime;
use sim_core::world::{World, YearOut};

const Y: i64 = 31_449_600;

/// Every locus of the repository's map with one copy of each allele: two copies on the
/// autosomes, two on a woman's X and one on a man's, one on the mt.
fn same_genes(sex: Sex) -> Vec<LocusAlleles> {
    let map: serde_json::Value =
        serde_json::from_str(include_str!("../../../data/genetics/genome_map.json"))
            .expect("the map");
    (map["loci"].as_array().expect("loci").iter())
        .map(|locus| {
            let id = locus["id"].as_str().expect("an id");
            let copies = match (locus["chromosome"].as_str().expect("a chromosome"), sex) {
                ("mt", _) | ("x", Sex::Male) => vec![0],
                _ => vec![0, 1],
            };
            LocusAlleles {
                locus: DataId::new(id).expect("a locus"),
                copies,
            }
        })
        .collect()
}

/// Two persons of the sex and the age with the same genes, founders 0 and 1 of one tag: two
/// lineage keys, so two draws of each environment.
fn twins_by_genes() -> World {
    pair(Sex::Female, 30)
}

fn pair(sex: Sex, years: i64) -> World {
    let woman = || Founder {
        born: SimTime::from_secs(-years * Y),
        sex,
        alleles: same_genes(sex),
    };
    let mut world = World::new(common::repository_data());
    world
        .submit(Input::Start(Start {
            world_seed: 0x10,
            founder_tag: DataId::new("test:sheet").expect("valid"),
            world_params: WorldParams {
                hereditary_diseases: false,
                nutrition: Vec::new(),
            },
            founders: vec![woman(), woman()],
            couples: Vec::new(),
            capacity: None,
            pool: None,
        }))
        .expect("accepted");
    world.advance_until(&mut YearOut::default(), SimTime::from_secs(0), &mut |_| {
        false
    });
    world
}

#[test]
fn the_same_genes_give_the_same_neutral_body_and_two_with_the_environment() {
    let world = twins_by_genes();
    let (one, two) = (
        SimId::try_from(1).expect("an id"),
        SimId::try_from(2).expect("an id"),
    );
    let neutral = |sim| world.neutral_person_view(sim).expect("a view");
    let (a, b) = (neutral(one), neutral(two));
    assert_eq!(a.shown.body, b.shown.body);
    assert_eq!(a.at_ages.len(), 5, "2, 8, 12, 16 and 25 years");
    for (at_a, at_b) in a.at_ages.iter().zip(&b.at_ages) {
        assert_eq!(at_a.at, at_b.at);
        assert_eq!(at_a.body, at_b.body, "at {} years", at_a.at.age_years);
    }
    // With the environment the two differ, and the neutral body is neither of them.
    let drawn = |sim| world.person_view(sim).expect("a view").shown.body;
    assert_ne!(drawn(one), drawn(two));
    assert_ne!(drawn(one), a.shown.body);
    // The moments are those of the person view.
    let plain = world.person_view(one).expect("a view");
    assert_eq!(a.born, plain.born);
    assert_eq!(a.ended, plain.ended);
    assert_eq!(a.shown.at, plain.shown.at);
}

/// Astra's review: the same for two men of 61, at every age of the sheet, 60 included.
#[test]
fn two_men_of_the_same_genes_have_the_same_neutral_bodies_at_every_age() {
    let world = pair(Sex::Male, 61);
    let view =
        |n: u64| (world.neutral_person_view(SimId::try_from(n).expect("an id"))).expect("a view");
    let (a, b) = (view(1), view(2));
    let ages: Vec<i64> = a.at_ages.iter().map(|shown| shown.at.age_years).collect();
    assert_eq!(ages, [2, 8, 12, 16, 25, 40, 60]);
    assert_eq!(a.at_ages, b.at_ages);
    assert_eq!(a.shown.body, b.shown.body);
}

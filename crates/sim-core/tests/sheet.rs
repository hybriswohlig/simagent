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

/// Every locus of the repository's map with one copy of each allele for a woman: two copies
/// on the autosomes and the X, one on the mt.
fn same_genes() -> Vec<LocusAlleles> {
    let map: serde_json::Value =
        serde_json::from_str(include_str!("../../../data/genetics/genome_map.json"))
            .expect("the map");
    (map["loci"].as_array().expect("loci").iter())
        .map(|locus| {
            let id = locus["id"].as_str().expect("an id");
            let copies = match locus["chromosome"].as_str().expect("a chromosome") {
                "mt" => vec![0],
                _ => vec![0, 1],
            };
            LocusAlleles {
                locus: DataId::new(id).expect("a locus"),
                copies,
            }
        })
        .collect()
}

/// Two women of 30 with the same genes, founders 0 and 1 of one tag: two lineage keys, so
/// two draws of each environment.
fn twins_by_genes() -> World {
    let woman = || Founder {
        born: SimTime::from_secs(-30 * Y),
        sex: Sex::Female,
        alleles: same_genes(),
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

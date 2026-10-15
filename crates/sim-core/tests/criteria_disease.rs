//! P2 (genetics specification 8.2, ADR-0020 section 8): the recessive disease on fixed
//! parents. Two carriers of `core:disease_ar` have children with `d = 2` with 1/4 (2.6);
//! the unit is the child of a carrier pair, at least 2000, drawn directly from the meiosis
//! (8.1: independent per child, ADR-0001). Tolerance 4 standard errors at the actual size,
//! 0.0387 at 2000 [W8.2a]. Exactly: no carrier (`d = 1`) is ever affected, and with the
//! switch off nobody is. Negative twin: a carrier mother and a father without the allele,
//! on whom the check must be red, and every child with `d = 2` there needs a new mutation
//! at the locus. The seed is a gate seed (protected list, ADR-0000): never reseed to get
//! green. The counts and the fingerprint of `d` by key come from the independent reference
//! (`tools/p2_reference.py`).

mod common;

use serde_json::{Value, json};
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::genome::meiosis::Zygote;
use sim_core::genome::{Genome, PaternalSex, Sex};
use sim_core::hash::stable_hash64;
use sim_core::input::{Founder, Input, Start};
use sim_core::lineage::GenomeKey;
use sim_core::params::WorldParams;
use sim_core::rng::{Draws, WorldSeed};
use sim_core::time::SimTime;
use sim_core::world::World;

const SEED_P2: u64 = 0x5eed_0000_0000_0002;
const CHILDREN: u64 = 2000;

/// The reference's counts of `d = 0, 1, 2` and the xxh3 of the 2000 digits of `d` by key.
const P2_COUNTS: [usize; 3] = [501, 1033, 466];
const P2_DIGITS: u64 = 0x34dd_33f8_9eb7_9de7;
const TWIN_COUNTS: [usize; 3] = [1011, 989, 0];
const TWIN_DIGITS: u64 = 0xd886_cba7_4d17_1ebf;

fn key(n: u64) -> GenomeKey {
    serde_json::from_value(json!(format!("{n:016x}"))).expect("key")
}

/// A genome with the alternative allele of `core:disease_ar` (autosome segment 0, slot 0)
/// on the maternal copy if `carrier`, the reference allele everywhere else.
fn parent(carrier: bool, from_father: Value) -> Genome {
    serde_json::from_value(json!({
        "maternal": [u64::from(carrier), 0, 0], "paternal": [0, 0, 0], "maternal_x": 0,
        "paternal_sex": from_father, "mt": 0
    }))
    .expect("a genome")
}

fn locus() -> DataId {
    DataId::new("core:disease_ar").expect("id")
}

/// The children of the keys 0 to 1999 and their `d` at the locus.
fn children(data: &Data, mother: &Genome, father: &Genome) -> Vec<(Zygote, u8)> {
    let draws = Draws::new(WorldSeed::new(SEED_P2));
    let ar = data
        .genome_map()
        .loci
        .iter()
        .find(|l| l.id == locus())
        .expect("the locus");
    (0..CHILDREN)
        .map(|k| {
            let zygote = Zygote::of(&draws, data, key(k), mother, father);
            let d = zygote.genome.dose(ar);
            (zygote, d)
        })
        .collect()
}

fn counts(children: &[(Zygote, u8)]) -> [usize; 3] {
    let mut counts = [0; 3];
    for (_, d) in children {
        counts[usize::from(*d)] += 1;
    }
    counts
}

fn digits(children: &[(Zygote, u8)]) -> u64 {
    let text: String = children.iter().map(|(_, d)| char::from(b'0' + d)).collect();
    stable_hash64(text.as_bytes())
}

/// Whether the share of `d = 2` lies within 4 standard errors of 1/4 at the actual size
/// (8.1); the formula gives W8.2a at 2000.
fn within(children: &[(Zygote, u8)]) -> bool {
    let n = f64::from(u32::try_from(children.len()).expect("small"));
    let tolerance = 4.0 * sim_math::sqrt(0.25 * 0.75 / n);
    let affected = f64::from(u32::try_from(counts(children)[2]).expect("small"));
    (affected / n - 0.25).abs() <= tolerance
}

#[test]
fn p2_tolerance_is_w8_2a_at_the_minimum_size() {
    let tolerance = 4.0 * sim_math::sqrt(0.25 * 0.75 / 2000.0);
    assert!((tolerance - 0.0387).abs() < 5e-5, "{tolerance}");
}

/// P2: of the children of two carriers a quarter has `d = 2`; the twin, a father without
/// the allele, is red, and its children with `d = 2` (none here) each carry a new mutation.
#[test]
fn p2_a_quarter_of_the_children_of_two_carriers_have_both_alleles() {
    let data = common::data();
    let mother = parent(true, json!({"x": 0}));
    let ours = children(&data, &mother, &parent(true, json!({"y": 0})));
    assert_eq!(counts(&ours), P2_COUNTS);
    assert_eq!(digits(&ours), P2_DIGITS);
    assert!(within(&ours));
    let twin = children(&data, &mother, &parent(false, json!({"y": 0})));
    assert_eq!(counts(&twin), TWIN_COUNTS);
    assert_eq!(digits(&twin), TWIN_DIGITS);
    assert!(!within(&twin));
    for (zygote, d) in &twin {
        if *d == 2 {
            assert!(
                zygote
                    .father
                    .mutations
                    .iter()
                    .chain(&zygote.mother.mutations)
                    .any(|m| m.locus == locus()),
                "d = 2 without a new mutation"
            );
        }
    }
}

/// P2 exactly: the 2000 children as founders right before the start, each with its sex and
/// its two copies, which the pedigree shows back. Whatever the switch, exactly those with
/// `d = 2` have the disease of the locus, with onset 0, and no carrier has it; it shows as
/// affected only with the switch on.
#[test]
fn p2_only_the_homozygous_are_affected_and_only_with_the_switch_on() {
    let data = common::data();
    let ours = children(
        &data,
        &parent(true, json!({"x": 0})),
        &parent(true, json!({"y": 0})),
    );
    let founders: Vec<Founder> = ours
        .iter()
        .map(|(zygote, _)| Founder {
            born: SimTime::from_secs(-1),
            sex: match zygote.genome.paternal_sex {
                PaternalSex::X(_) => Sex::Female,
                PaternalSex::Y(_) => Sex::Male,
            },
            alleles: common::dummy(
                zygote.genome.maternal.allele(0, 0),
                zygote.genome.paternal.allele(0, 0),
            ),
        })
        .collect();
    // Each dose with both sexes, so neither a lost copy nor a lost sex hides.
    for d in 0..=2 {
        for sex in [Sex::Female, Sex::Male] {
            assert!(
                founders
                    .iter()
                    .zip(&ours)
                    .any(|(founder, (_, dose))| *dose == d && founder.sex == sex),
                "d = {d}, {sex:?}"
            );
        }
    }
    for on in [true, false] {
        let mut world = World::new(common::data());
        world
            .submit(Input::Start(Start {
                world_seed: SEED_P2,
                founder_tag: DataId::new("test:p2").expect("id"),
                world_params: WorldParams {
                    hereditary_diseases: on,
                    nutrition: Vec::new(),
                },
                founders: founders.clone(),
                couples: Vec::new(),
                capacity: None,
                pool: None,
            }))
            .expect("accepted");
        let persons = world.pedigree().persons;
        assert_eq!(persons.len(), founders.len());
        for ((person, founder), (_, d)) in persons.iter().zip(&founders).zip(&ours) {
            assert_eq!(person.sex, founder.sex);
            assert_eq!(person.alleles, founder.alleles);
            let disease: Vec<_> = person
                .diseases
                .iter()
                .filter(|shown| shown.disease.as_str() == "core:disease_ar")
                .collect();
            if *d == 2 {
                assert_eq!(disease.len(), 1, "d = 2");
                assert!(disease[0].onset == 0.0, "onset {}", disease[0].onset);
                assert_eq!(disease[0].affected, on, "switch {on}");
            } else {
                assert!(disease.is_empty(), "d = {d} has the disease");
            }
        }
    }
}

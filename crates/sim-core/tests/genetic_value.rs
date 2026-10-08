//! The genetic value of a trait (genetics specification 4.1, ADR-0008 section 3):
//! `A_t` folded from 0 over the trait's own loci in map order, then over each included
//! trait in the order of its data ID; `G_t = A_t / sigma_ref`. The expected values are
//! the arithmetic of section 3 written out by hand, with binary fractions where the
//! result is exact.

mod common;

use serde_json::{Value, json};
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::genome::{Genome, LocusAlleles, Sex};

fn id(text: &str) -> DataId {
    DataId::new(text).expect("id")
}

/// A woman with these doses (maternal copy first: dose 1 is `[1, 0]`).
fn woman(data: &Data, doses: &[(&str, u8)]) -> Genome {
    let alleles: Vec<LocusAlleles> = doses
        .iter()
        .map(|(locus, dose)| LocusAlleles {
            locus: id(locus),
            copies: match dose {
                0 => vec![0, 0],
                1 => vec![1, 0],
                _ => vec![1, 1],
            },
        })
        .collect();
    Genome::with_alleles(Sex::Female, &alleles, data).expect("fits")
}

/// The fixture map with traits (`tests/golden/data_traits`): growth from `test:g1`
/// (weight 1, effect 0.5, d_ref 1.25) and the threshold locus `test:g2` (weight 2, effect
/// 0.125, d_ref 0.5), sigma 2; mood from `test:g1` (weight -0.5), sigma 3; face from
/// `test:f1` (weight 1, effect 0.75, d_ref 1) and growth included with weight 0.25,
/// sigma 1.5; the exclusive pigment from `test:p1` (weight 1, effect 1, d_ref 0), sigma
/// 0.75.
fn fixture() -> Data {
    let (calibration, genome_map) = common::files_from(
        include_str!("../../../tests/golden/data_traits/calibration/genetics.json"),
        include_str!("../../../tests/golden/data_traits/genetics/genome_map.json"),
    );
    Data::new(common::definitions(calibration, genome_map).expect("definitions")).expect("checked")
}

#[test]
fn genetic_value_follows_the_specification() {
    let data = fixture();
    let genome = woman(
        &data,
        &[("test:g1", 2), ("test:g2", 0), ("test:f1", 2), ("test:p1", 1)],
    );
    let value = |t: &str| genome.genetic_value(&id(t), &data).expect("a trait");
    // Growth: (1 · 0.5) · (2 − 1.25) + (2 · 0.125) · (0 − 0.5) = 0.375 − 0.125, over 2.
    assert_eq!(value("test:growth"), 0.125);
    // Mood: a negative weight on the same locus, (−0.5 · 0.5) · 0.75, over 3.
    assert_eq!(value("test:mood"), -0.0625);
    // Face: its own locus (1 · 0.75) · (2 − 1) = 0.75, then growth with weight 0.25, each
    // locus with the weight inside: ((0.25 · 1) · 0.5) · 0.75 and ((0.25 · 2) · 0.125) ·
    // (0 − 0.5), folded in that order; one division by 1.5 at the end.
    assert_eq!(value("test:face"), ((0.75 + 0.093_75) - 0.031_25) / 1.5);
    // Pigment: dose 1 against d_ref 0.
    assert_eq!(value("test:pigment"), 1.0 / 0.75);
    // A trait the map does not name has no value; a locus is no trait.
    assert_eq!(genome.genetic_value(&id("test:other"), &data), None);
    assert_eq!(genome.genetic_value(&id("test:g1"), &data), None);
    // The reference genome: every dose 0.
    let reference = Genome::reference(Sex::Female);
    assert_eq!(
        reference.genetic_value(&id("test:growth"), &data),
        Some(((0.5 * -1.25) + (0.25 * -0.5)) / 2.0)
    );
}

/// A map in which the order of the fold decides the bits: own loci of `test:t` with
/// terms `1e17`, `-1e17` and `1` in map order; `test:t2` with its own locus (term `1e17`)
/// and the includes `test:s1` (term `1`) and `test:s2` (term `-1e17`), whose loci stand
/// in the map in the other order.
fn ordered() -> Data {
    let additive = |id: &str, slot: u64, loads: Value| {
        json!({"id": id, "chromosome": "autosome", "segment": 4, "slot": slot,
            "action": {"additive": {"effect": 1.0, "d_ref": 0.0, "loads": loads}},
            "mark": "small", "mutation": "both"})
    };
    let on = |t: &str, weight: f64| json!({"trait": t, "weight": weight});
    let trait_ = |id: &str, includes: Value| {
        json!({"id": id, "sigma_ref": 1.0, "exclusive": false, "includes": includes})
    };
    let (calibration, mut genome_map) = common::files();
    genome_map["traits"] = json!([
        trait_("test:s1", json!([])),
        trait_("test:s2", json!([])),
        trait_("test:t", json!([])),
        trait_("test:t2", json!([on("test:s1", 1.0), on("test:s2", 1.0)])),
    ]);
    let dummy = genome_map["loci"][0].clone();
    genome_map["loci"] = json!([
        dummy,
        additive("test:a", 0, json!([on("test:t", 1e17)])),
        additive("test:b", 1, json!([on("test:t", -1e17)])),
        additive("test:c", 2, json!([on("test:t", 1.0)])),
        additive("test:d", 3, json!([on("test:t2", 1e17)])),
        additive("test:e", 4, json!([on("test:s2", -1e17)])),
        additive("test:f", 5, json!([on("test:s1", 1.0)])),
    ]);
    Data::new(common::definitions(calibration, genome_map).expect("definitions")).expect("checked")
}

/// ADR-0008 section 3: the fold runs left to right, own loci in map order, then each
/// included trait in the order of its ID, its loci in map order. Any other order gives
/// other bits here: `1e17 + 1` rounds to `1e17`.
#[test]
fn the_fold_runs_in_the_order_of_the_specification() {
    let data = ordered();
    let loci = ["test:a", "test:b", "test:c", "test:d", "test:e", "test:f"];
    let doses: Vec<(&str, u8)> = loci.iter().map(|locus| (*locus, 1)).collect();
    let genome = woman(&data, &doses);
    // Map order: (1e17 − 1e17) + 1 = 1; in reverse, (1 − 1e17) + 1e17 would be 0.
    assert_eq!(genome.genetic_value(&id("test:t"), &data), Some(1.0));
    assert_ne!((1.0 - 1e17) + 1e17, 1.0);
    // Own locus first, then test:s1 (1), then test:s2 (−1e17): (1e17 + 1) − 1e17 = 0.
    // Merged in map order (d, e, f), or with the includes the other way round, it would
    // be (1e17 − 1e17) + 1 = 1.
    assert_eq!(genome.genetic_value(&id("test:t2"), &data), Some(0.0));
    assert_eq!((1e17 + 1.0) - 1e17, 0.0);
    assert_eq!((1e17 - 1e17) + 1.0, 1.0);
}

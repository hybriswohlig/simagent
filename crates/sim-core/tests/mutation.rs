//! Genetics specification 2.4: new mutations in the meiosis.

mod common;

use serde_json::{Value, json};
use sim_core::data::Data;
use sim_core::genome::meiosis::Mutation;
use sim_core::genome::meiosis::Zygote;
use sim_core::genome::{Genome, LocusAlleles, Sex};
use sim_core::lineage::GenomeKey;
use sim_core::rng::{Draws, WorldSeed};

/// A founder with every listed locus at `allele` in both copies.
fn parent(data: &Data, sex: Sex, allele: u8) -> Genome {
    let alleles: Vec<LocusAlleles> = data
        .genome_map()
        .loci
        .iter()
        .map(|locus| LocusAlleles {
            locus: locus.id.clone(),
            copies: vec![allele, allele],
        })
        .collect();
    Genome::with_alleles(sex, &alleles, data).expect("fits")
}

/// The mean number of new mutations per side over `n` conceptions.
fn mean_mutations(data: &Data, mother: &Genome, father: &Genome, n: u64) -> f64 {
    let draws = Draws::new(WorldSeed::new(7));
    let mut total = 0;
    for key in 0..n {
        let genome: GenomeKey = serde_json::from_value(json!(format!("{key:016x}"))).expect("key");
        let zygote = Zygote::of(&draws, data, genome, mother, father);
        total += zygote.mother.mutations.len() + zygote.father.mutations.len();
    }
    let total = u32::try_from(total).expect("small");
    let sides = u32::try_from(2 * n).expect("small");
    f64::from(total) / f64::from(sides)
}

/// Specification 2.4: above 30, the count is a sum of Poisson parts of at most 30, so its
/// mean is the whole total rate. 150 loci at 0.3 give 45 per side; over 4000 sides the
/// standard error is about 0.106, the tolerance 4 standard errors. Wrong model: only one
/// part of 30.
#[test]
fn a_total_rate_above_30_is_drawn_in_parts() {
    let data = common::autosomal_map(150, "both", 0.3);
    let (mother, father) = (parent(&data, Sex::Female, 0), parent(&data, Sex::Male, 0));
    let mean = mean_mutations(&data, &mother, &father, 2000);
    assert!((mean - 45.0).abs() < 0.43, "{mean}");
}

/// A locus mutates only in its direction: with every allele already where `to_alt` and
/// `to_ref` lead, nothing is eligible and nothing changes; turned around, every side
/// mutates.
#[test]
fn a_locus_mutates_only_in_its_direction() {
    for (direction, allele) in [("to_alt", 1), ("to_ref", 0)] {
        let data = common::autosomal_map(40, direction, 0.5);
        let (mother, father) = (
            parent(&data, Sex::Female, allele),
            parent(&data, Sex::Male, allele),
        );
        assert_eq!(
            mean_mutations(&data, &mother, &father, 200),
            0.0,
            "{direction}"
        );
        let (mother, father) = (
            parent(&data, Sex::Female, 1 - allele),
            parent(&data, Sex::Male, 1 - allele),
        );
        assert!(
            mean_mutations(&data, &mother, &father, 200) > 15.0,
            "{direction}"
        );
    }
}

const VECTORS: &str = include_str!("../../../tests/golden/mutation_vectors.json");

/// Bits of a list of 0 and 1, slot `i` at bit `i`.
fn bits(slots: &Value) -> u64 {
    let slots = slots.as_array().expect("slots");
    slots.iter().enumerate().fold(0, |word, (i, bit)| {
        word | (bit.as_u64().expect("bit") << u32::try_from(i).expect("small"))
    })
}

/// The three words of an autosome copy: segment `s`, slot `i` at bit `24 s + i`.
fn autosomes(segments: &Value) -> Value {
    let mut words = [0_u64; 3];
    for (s, segment) in segments.as_array().expect("segments").iter().enumerate() {
        for (i, bit) in segment.as_array().expect("slots").iter().enumerate() {
            let at = 24 * s + i;
            words[at / 64] |= bit.as_u64().expect("bit") << (at % 64);
        }
    }
    json!(words)
}

/// A genome of the reference's form as the core reads it.
fn genome(person: &Value) -> Genome {
    let from_father = match (person.get("x_paternal"), person.get("paternal_sex")) {
        (Some(x), None) => json!({"x": bits(x)}),
        (None, Some(sex)) => match (sex.get("x"), sex.get("y")) {
            (Some(x), None) => json!({"x": bits(x)}),
            (None, Some(y)) => json!({"y": bits(y)}),
            _ => panic!("one sex chromosome"),
        },
        _ => panic!("one second sex chromosome"),
    };
    serde_json::from_value(json!({
        "maternal": autosomes(&person["autosomes_maternal"]),
        "paternal": autosomes(&person["autosomes_paternal"]),
        "maternal_x": bits(&person["x_maternal"]),
        "paternal_sex": from_father,
        "mt": bits(&person["mt"]),
    }))
    .expect("a genome")
}

/// The test map of the reference in the form of the genome map: X, Y and mt loci sit in
/// segment 0 of their type (the reference numbers the mother's X 8 as in the draw
/// details), every locus acts additively, the direction is its mutation.
fn reference_data(vectors: &Value) -> Data {
    let loci: Vec<Value> = vectors["genome_map"]
        .as_array()
        .expect("loci")
        .iter()
        .map(|locus| {
            let autosome = locus["chromosome"] == "autosome";
            json!({"id": locus["id"], "chromosome": locus["chromosome"],
                "segment": if autosome { locus["segment"].clone() } else { json!(0) },
                "slot": locus["slot"], "action": "additive", "mark": locus["mark"],
                "mutation": locus["direction"]})
        })
        .collect();
    let (mut calibration, _) = common::files();
    calibration["values"]["core:mutation_rate"]["value"] = vectors["rates"]["standard"].clone();
    calibration["values"]["core:mutation_rate_disease"]["value"] =
        vectors["rates"]["disease"].clone();
    let genome_map = json!({"format_version": 1, "map_version": 1, "loci": loci});
    Data::new(common::definitions(calibration, genome_map).expect("definitions")).expect("checked")
}

/// Compares the core with every case of a vector set of the reference: child genome and
/// the mutations of both sides, in their order. Returns the number of mutations.
fn compare(set: &Value) -> usize {
    let data = reference_data(set);
    let mut seen = 0;
    for case in set["cases"].as_array().expect("cases") {
        let seed = case["seed"].as_u64().expect("seed");
        let key: GenomeKey = serde_json::from_value(case["genome_key"].clone()).expect("key");
        let zygote = Zygote::of(
            &Draws::new(WorldSeed::new(seed)),
            &data,
            key,
            &genome(&case["mother"]),
            &genome(&case["father"]),
        );
        assert_eq!(zygote.genome, genome(&case["child"]), "child of {key:?}");
        for (ours, side) in [
            (&zygote.mother.mutations, "mother"),
            (&zygote.father.mutations, "father"),
        ] {
            let theirs: Vec<Mutation> =
                serde_json::from_value(case["records"][side]["mutations"].clone())
                    .expect("mutations");
            assert_eq!(ours, &theirs, "{side} of {key:?}");
            seen += ours.len();
        }
    }
    seen
}

/// Specification 2.4 against the independent reference (`tools/meiosis_reference.py
/// --mutation-vectors`, written from the specification by another model): child genome
/// and the mutations of both sides, in their order.
#[test]
fn mutation_matches_the_independent_reference() {
    let vectors: Value = serde_json::from_str(VECTORS).expect("json");
    assert_eq!(vectors["cases"].as_array().expect("cases").len(), 6);
    assert_eq!(compare(&vectors), 11, "the vectors hold eleven mutations");
}

/// The count in parts against the same reference: 40 loci at rate 0.9 give a total of 36
/// on each side, drawn as 30 at detail 0 and 6 at detail 1 (2.4).
#[test]
fn the_count_in_parts_matches_the_independent_reference() {
    let vectors: Value = serde_json::from_str(VECTORS).expect("json");
    let parts = &vectors["parts"];
    assert_eq!(parts["cases"].as_array().expect("cases").len(), 3);
    assert_eq!(compare(parts), 231, "the parts hold 231 mutations");
}

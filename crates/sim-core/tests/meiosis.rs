//! Genetics specification 2.3 against the independent reference: `tests/golden/meiosis_vectors.json`
//! comes from `tools/meiosis_reference.py`, which another model wrote from the
//! specification alone (B13). The child and both meioses must match bit for bit.

mod common;

use serde_json::{Value, json};
use sim_core::genome::meiosis::Zygote;
use sim_core::genome::{Genome, Sex, SexChromosome, Strand};
use sim_core::lineage::GenomeKey;
use sim_core::rng::{Draws, WorldSeed};

const VECTORS: &str = include_str!("../../../tests/golden/meiosis_vectors.json");

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

/// A genome of the reference's form as the core reads it (serde form of `Genome`).
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

/// A child with its sex given (Grok's round 44, K2) is the drawn child but for the father's
/// sex chromosome: with the drawn sex it is the same zygote, with the other sex the same
/// autosomes, the same X from the mother and the same mitochondria.
#[test]
fn a_child_of_a_given_sex_is_the_drawn_child_but_for_the_fathers_sex_chromosome() {
    let vectors: Value = serde_json::from_str(VECTORS).expect("json");
    let data = common::data();
    for case in vectors["cases"].as_array().expect("cases") {
        let seed = case["seed"].as_u64().expect("seed");
        let key: GenomeKey = serde_json::from_value(case["genome_key"].clone()).expect("key");
        let draws = Draws::new(WorldSeed::new(seed));
        let (mother, father) = (genome(&case["mother"]), genome(&case["father"]));
        let drawn = Zygote::of(&draws, &data, key, &mother, &father);
        let sex = drawn.genome.sex();
        assert_eq!(
            Zygote::with_sex(&draws, &data, key, &mother, &father, sex),
            drawn
        );
        let other = if sex == Sex::Female {
            Sex::Male
        } else {
            Sex::Female
        };
        let given = Zygote::with_sex(&draws, &data, key, &mother, &father, other);
        assert_eq!(given.genome.sex(), other);
        assert_eq!(given.mother, drawn.mother);
        assert_eq!(given.father.segments, drawn.father.segments);
    }
}

#[test]
fn meiosis_matches_the_independent_reference() {
    let vectors: Value = serde_json::from_str(VECTORS).expect("json");
    let data = common::data();
    let cases = vectors["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 6);
    for case in cases {
        let seed = case["seed"].as_u64().expect("seed");
        let key: GenomeKey = serde_json::from_value(case["genome_key"].clone()).expect("key");
        let draws = Draws::new(WorldSeed::new(seed));
        let zygote = Zygote::of(
            &draws,
            &data,
            key,
            &genome(&case["mother"]),
            &genome(&case["father"]),
        );
        assert_eq!(zygote.genome, genome(&case["child"]), "child of {key:?}");
        for (meiosis, record) in [
            (&zygote.mother, &case["records"]["mother"]),
            (&zygote.father, &case["records"]["father"]),
        ] {
            let segments = record["segments"].as_array().expect("segments");
            assert_eq!(meiosis.segments.len(), segments.len());
            for (ours, theirs) in meiosis.segments.iter().zip(segments) {
                assert_eq!(
                    u64::from(ours.segment),
                    theirs["segment"].as_u64().expect("index")
                );
                let strand = match theirs["start_strand"].as_u64() {
                    Some(0) => Strand::Maternal,
                    Some(1) => Strand::Paternal,
                    other => panic!("strand {other:?}"),
                };
                assert_eq!(ours.start_strand, strand);
                let positions: Vec<f64> = theirs["crossover_positions"]
                    .as_array()
                    .expect("positions")
                    .iter()
                    .map(|p| p.as_f64().expect("a float"))
                    .collect();
                let ours: Vec<f64> = ours.crossovers.iter().map(|c| c.get()).collect();
                assert_eq!(ours, positions);
            }
            let chromosome = match record.get("sex_chromosome").and_then(Value::as_str) {
                None | Some("x") => SexChromosome::X,
                Some("y") => SexChromosome::Y,
                other => panic!("chromosome {other:?}"),
            };
            assert_eq!(meiosis.sex_chromosome, chromosome);
        }
    }
}

//! The reference mean of the genetic value (Grok GF-1, genetics specification 4.1):
//! `G_t = (A_t − mean_ref_t) / sigma_ref_t`, a map without `mean_ref` exactly as before. On
//! the template of map 4 (`tests/golden/map4_template`).

mod common;

use serde_json::{Value, json};
use sim_core::data::{Data, DataError};
use sim_core::data_id::DataId;
use sim_core::genome::{Genome, LocusAlleles, Sex};

/// The template's data hash today (`tests/map4_template.rs`): a map without `mean_ref`
/// keeps it.
const TEMPLATE_DATA_HASH: u64 = 0x5934_8c2c_9b8d_5230;

fn template_files() -> (Value, Value) {
    common::files_from(
        include_str!("../../../tests/golden/map4_template/calibration/genetics.json"),
        include_str!("../../../tests/golden/map4_template/genetics/genome_map.json"),
    )
}

fn data_of(calibration: Value, map: Value) -> Result<Data, DataError> {
    Data::new(common::definitions(calibration, map).expect("definitions"))
}

/// The template's map with `mean_ref` set on the named traits.
fn with_means(means: &[(&str, f64)]) -> (Value, Value) {
    let (calibration, mut map) = template_files();
    for t in map["traits"].as_array_mut().expect("traits") {
        if let Some((_, mean)) = means.iter().find(|(id, _)| t["id"] == json!(id)) {
            t["mean_ref"] = json!(mean);
        }
    }
    (calibration, map)
}

fn sigma_of(map: &Value, id: &str) -> f64 {
    map["traits"]
        .as_array()
        .expect("traits")
        .iter()
        .find(|t| t["id"] == json!(id))
        .and_then(|t| t["sigma_ref"].as_f64())
        .expect("sigma_ref")
}

/// A few genomes: the reference genome of each sex, and genomes with an alternative allele
/// on every second additive locus of the map, both copies or one.
fn genomes(data: &Data, map: &Value) -> Vec<Genome> {
    let additive: Vec<String> = map["loci"]
        .as_array()
        .expect("loci")
        .iter()
        .filter(|locus| locus["action"].get("additive").is_some())
        .filter_map(|locus| locus["id"].as_str().map(str::to_owned))
        .collect();
    let some = |copies: Vec<u8>, step: usize| -> Vec<LocusAlleles> {
        additive
            .iter()
            .step_by(step)
            .map(|id| LocusAlleles {
                locus: DataId::new(id).expect("id"),
                copies: copies.clone(),
            })
            .collect()
    };
    [
        (Sex::Female, Vec::new()),
        (Sex::Male, Vec::new()),
        (Sex::Female, some(vec![1, 1], 2)),
        (Sex::Male, some(vec![1, 0], 3)),
        (Sex::Female, some(vec![0, 1], 5)),
    ]
    .into_iter()
    .map(|(sex, alleles)| Genome::with_alleles(sex, &alleles, data).expect("valid"))
    .collect()
}

#[test]
fn a_map_without_a_mean_is_unchanged() {
    let (calibration, map) = template_files();
    assert_eq!(
        data_of(calibration, map).expect("checked").hash(),
        TEMPLATE_DATA_HASH
    );
}

#[test]
fn a_mean_moves_every_genetic_value_of_its_trait_and_only_there() {
    let means = [("core:longevity", 0.25), ("core:growth", -0.5)];
    let (calibration, plain_map) = template_files();
    let plain = data_of(calibration, plain_map.clone()).expect("checked");
    let (calibration, map) = with_means(&means);
    let moved = data_of(calibration, map).expect("checked");
    assert_ne!(moved.hash(), plain.hash());
    let untouched = DataId::new("core:cognition").expect("id");
    for genome in genomes(&plain, &plain_map) {
        for (id, mean) in means {
            let t = DataId::new(id).expect("id");
            let a = genome.additive_value(&t, &plain).expect("A");
            assert_eq!(genome.additive_value(&t, &moved), Some(a), "A stays");
            let sigma = sigma_of(&plain_map, id);
            let want = (a - mean) / sigma;
            assert_eq!(
                genome.genetic_value(&t, &moved).map(f64::to_bits),
                Some(want.to_bits()),
                "{id}"
            );
            assert_eq!(
                genome.genetic_value(&t, &plain).map(f64::to_bits),
                Some((a / sigma).to_bits()),
                "{id} without a mean"
            );
        }
        assert_eq!(
            genome.genetic_value(&untouched, &moved).map(f64::to_bits),
            genome.genetic_value(&untouched, &plain).map(f64::to_bits),
            "another trait"
        );
    }
}

#[test]
fn a_mean_of_zero_is_written_and_hashed() {
    let (calibration, map) = with_means(&[("core:longevity", 0.0)]);
    let zero = data_of(calibration, map).expect("checked");
    assert_ne!(
        zero.hash(),
        TEMPLATE_DATA_HASH,
        "present differs from absent"
    );
}

/// The map with `sigma_ref` 1e-10 on longevity and, if given, a `mean_ref` there.
fn tiny_sigma(mean: Option<f64>) -> (Value, Value) {
    let (calibration, mut map) = match mean {
        Some(mean) => with_means(&[("core:longevity", mean)]),
        None => template_files(),
    };
    for t in map["traits"].as_array_mut().expect("traits") {
        if t["id"] == json!("core:longevity") {
            t["sigma_ref"] = json!(1e-10);
        }
    }
    (calibration, map)
}

#[test]
fn a_mean_counts_into_the_bound_of_the_trait() {
    // The small sigma alone keeps every value finite; with a huge mean it would not.
    let (calibration, map) = tiny_sigma(None);
    assert!(data_of(calibration, map).is_ok());
    let (calibration, map) = tiny_sigma(Some(1e308));
    assert!(matches!(
        data_of(calibration, map),
        Err(DataError::TraitBounds)
    ));
}

const HASH: &str = include_str!("../../../tests/golden/gl_mean/hash.json");
const GENETIC: &str = include_str!("../../../tests/golden/gl_mean/genetic.json");

fn bits(value: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(value.as_str().expect("hex"), 16).expect("hex"))
}

fn hex(value: &Value) -> u64 {
    u64::from_str_radix(value.as_str().expect("hex"), 16).expect("hex")
}

/// The template with the case's `mean_ref` values (from their bits) and, if given, another
/// `sigma_ref` on longevity.
fn case_data(case: &Value) -> Result<Data, String> {
    let (calibration, mut map) = template_files();
    for t in map["traits"].as_array_mut().expect("traits") {
        let id = t["id"].as_str().expect("id").to_owned();
        if let Some(mean) = case["mean_ref"].get(&id) {
            t["mean_ref"] = Value::from(bits(&mean["bits"]));
        }
        if id == "core:longevity"
            && let Some(sigma) = case.get("sigma_ref_longevity").and_then(Value::as_f64)
        {
            t["sigma_ref"] = json!(sigma);
        }
    }
    let definitions = common::definitions(calibration, map)?;
    Data::new(definitions).map_err(|e| format!("{e:?}"))
}

/// The reference's data hashes (`tools/data_reference.py`, Grok GF-1): unchanged without a
/// mean, another with one, another again with a mean of zero; and the bounds.
#[test]
fn the_reference_pins_the_hashes_and_the_bounds() {
    let fixture: Value = serde_json::from_str(HASH).expect("json");
    for case in fixture["cases"].as_array().expect("cases") {
        let data = case_data(case).expect("checked");
        assert_eq!(data.hash(), hex(&case["data_hash"]), "{}", case["name"]);
    }
    for case in fixture["bounds"]["cases"].as_array().expect("cases") {
        let data = case_data(case);
        if case["accepted"] == json!(true) {
            let data = data.expect("accepted");
            assert_eq!(data.hash(), hex(&case["data_hash"]), "{}", case["name"]);
        } else {
            assert!(data.is_err(), "{} was accepted", case["name"]);
        }
    }
}

/// The reference's genetic values of six founders on the template and on the map with two
/// means (`tools/world_reference.py`): `A` unchanged, `G = (A − mean) / sigma` bit for bit.
#[test]
fn the_reference_pins_the_genetic_values() {
    let fixture: Value = serde_json::from_str(GENETIC).expect("json");
    let hashes: Value = serde_json::from_str(HASH).expect("json");
    let cases = hashes["cases"].as_array().expect("cases");
    let [plain, moved] = ["a_template", "b_two_means"].map(|name| {
        let case = cases
            .iter()
            .find(|c| c["name"] == json!(name))
            .expect("a case");
        case_data(case).expect("checked")
    });
    for genome in fixture["genomes"].as_array().expect("genomes") {
        let sex = if genome["sex"] == json!("female") {
            Sex::Female
        } else {
            Sex::Male
        };
        let alleles: Vec<LocusAlleles> = genome["alleles"]
            .as_array()
            .expect("alleles")
            .iter()
            .map(|entry| LocusAlleles {
                locus: DataId::new(entry[0].as_str().expect("id")).expect("id"),
                copies: entry[1]
                    .as_array()
                    .expect("copies")
                    .iter()
                    .map(|c| u8::try_from(c.as_u64().expect("an allele")).expect("small"))
                    .collect(),
            })
            .collect();
        for (data, map) in [(&plain, "map_a"), (&moved, "map_b")] {
            let built = Genome::with_alleles(sex, &alleles, data).expect("valid");
            for (id, want) in genome[map]["traits"].as_object().expect("traits") {
                let t = DataId::new(id).expect("id");
                let name = format!("{} {map} {id}", genome["name"]);
                let a = built.additive_value(&t, data).expect("A");
                assert_eq!(a.to_bits(), bits(&want["A_bits"]).to_bits(), "{name} A");
                let g = built.genetic_value(&t, data).expect("G");
                assert_eq!(g.to_bits(), bits(&want["G_bits"]).to_bits(), "{name} G");
            }
        }
    }
}

/// The mean adds its magnitude to the bound, right at the edge (mutants): longevity's terms
/// reach 8 (the reference's bound); with `sigma_ref` = 8.5 / f64::MAX the value 8 / sigma
/// is finite and 9 / sigma is not, so a mean of 1 (or −1) turns an accepted map into one
/// whose genetic value could overflow; 8 − 1 or 8 · 1 would not.
#[test]
fn the_bound_adds_the_mean_s_magnitude() {
    let edge = |mean: Option<f64>| {
        let (calibration, mut map) = template_files();
        for t in map["traits"].as_array_mut().expect("traits") {
            if t["id"] == json!("core:longevity") {
                t["sigma_ref"] = json!(8.5 / f64::MAX);
                if let Some(mean) = mean {
                    t["mean_ref"] = json!(mean);
                }
            }
        }
        data_of(calibration, map)
    };
    assert!(edge(None).is_ok());
    for mean in [1.0, -1.0] {
        assert!(
            matches!(edge(Some(mean)), Err(DataError::TraitBounds)),
            "{mean}"
        );
    }
}

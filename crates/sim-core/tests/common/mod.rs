//! The definitions of the repository for the tests, read the way sim-cli reads them
//! (ADR-0007 section 2, ADR-0008 section 1): the text fields of each calibration value,
//! each trait and each locus removed, the rest into the core types.

use serde_json::{Value, json};
use sim_core::data::{Data, Definitions};

const DEMOGRAPHY: &str = include_str!("../../../../data/calibration/demography.json");
const CALIBRATION: &str = include_str!("../../../../data/calibration/genetics.json");
const GENOME_MAP: &str = include_str!("../../../../data/genetics/genome_map.json");

/// Removes the text fields `names` of the entries under `entries`; each must be there
/// and not blank, as sim-cli demands.
fn without(mut file: Value, entries: &str, names: &[&str]) -> Value {
    let list: Vec<&mut Value> = match &mut file[entries] {
        Value::Object(values) => values.values_mut().collect(),
        Value::Array(loci) => loci.iter_mut().collect(),
        _ => panic!("entries"),
    };
    for entry in list {
        for name in names {
            let text = entry.as_object_mut().expect("entry").remove(*name);
            let filled = text
                .as_ref()
                .and_then(Value::as_str)
                .is_some_and(|text| !text.trim().is_empty());
            assert!(filled, "{name} missing or blank");
        }
    }
    file
}

/// Removes the text of each of the four weights of each phenotype row (ADR-0009) and of
/// the two numbers of each sex difference (ADR-0014).
fn without_weights(mut calibration: Value) -> Value {
    for (list, numbers) in [
        (
            "traits",
            &["h2_child", "c2_child", "h2_adult", "c2_adult"][..],
        ),
        ("sex_differences", &["shift", "variance_ratio"][..]),
    ] {
        let rows = calibration[list].as_array_mut().expect("list");
        for row in rows.iter_mut() {
            for number in numbers {
                for name in ["source", "as_of", "note"] {
                    let text = row[*number].as_object_mut().expect("number").remove(name);
                    let filled = text
                        .as_ref()
                        .and_then(Value::as_str)
                        .is_some_and(|text| !text.trim().is_empty());
                    assert!(filled, "{name} missing or blank");
                }
            }
        }
    }
    for disease in calibration["diseases"]
        .as_array_mut()
        .expect("diseases")
        .iter_mut()
    {
        without_numbers(disease);
    }
    without_numbers(&mut calibration["fertility"]);
    calibration
}

/// Removes the text of every number (an object with a unit) inside a disease (ADR-0017
/// section 1), however deep it sits.
fn without_numbers(value: &mut Value) {
    match value {
        Value::Object(fields) if fields.contains_key("unit") => {
            for name in ["source", "as_of", "note"] {
                let text = fields.remove(name);
                let filled = text
                    .as_ref()
                    .and_then(Value::as_str)
                    .is_some_and(|text| !text.trim().is_empty());
                assert!(filled, "{name} missing or blank");
            }
        }
        Value::Object(fields) => fields.values_mut().for_each(without_numbers),
        Value::Array(items) => items.iter_mut().for_each(without_numbers),
        _ => {}
    }
}

/// The life course data of the repository (ADR-0017 section 1) as a JSON value without its
/// text: each value and each weight of the wish.
#[allow(
    dead_code,
    reason = "not every test crate changes the life course data"
)]
pub fn demography() -> Value {
    let read: Value = serde_json::from_str(DEMOGRAPHY).expect("json");
    without(
        without(read, "values", &["source", "as_of", "note"]),
        "fertility_wish_shape",
        &["source", "as_of", "note"],
    )
}

/// The fertility of the old model as test data (ADR-0017 section 5): the woman's window
/// from 18 until 45, `rate` per attempt, ten months of pregnancy, no pause, no twins, and
/// the wish always 40, so that the wish never ends a series.
#[allow(dead_code, reason = "not every test crate needs the window")]
pub fn window_fertility(calibration: &mut Value, demography: &mut Value, rate: f64) {
    let year = |value: f64| json!({"value": value, "unit": "year"});
    calibration["fertility"] = json!({"window": {"from": year(18.0), "until": year(45.0)}});
    let values = &mut demography["values"];
    values["core:fertility_adult_rate"]["value"] = json!(rate);
    values["core:pregnancy_length"]["value"] = json!(10.0);
    values["core:fertility_pause"]["value"] = json!(0.0);
    values["core:twins_identical"]["value"] = json!(0.0);
    values["core:twins_fraternal"]["value"] = json!(0.0);
    values["core:separation_rate"]["value"] = json!(0.0);
    let mut shape = vec![json!({"value": 0.0, "unit": "fraction"}); 40];
    shape.push(json!({"value": 1.0, "unit": "fraction"}));
    demography["fertility_wish_shape"] = Value::Array(shape);
}

/// The two files of the repository as JSON values without their text.
pub fn files() -> (Value, Value) {
    files_from(CALIBRATION, GENOME_MAP)
}

/// Two definition files as JSON values without their text.
pub fn files_from(calibration: &str, genome_map: &str) -> (Value, Value) {
    let read = |text: &str| serde_json::from_str::<Value>(text).expect("json");
    (
        without_weights(without(
            read(calibration),
            "values",
            &["source", "as_of", "note"],
        )),
        without(
            without(read(genome_map), "traits", &["description"]),
            "loci",
            &["description"],
        ),
    )
}

/// Definitions from two JSON values; `Err` if serde rejects one.
pub fn definitions(calibration: Value, genome_map: Value) -> Result<Definitions, String> {
    definitions_with(demography(), calibration, genome_map)
}

/// The definitions from the three files.
pub fn definitions_with(
    demography: Value,
    calibration: Value,
    genome_map: Value,
) -> Result<Definitions, String> {
    Ok(Definitions {
        demography: serde_json::from_value(demography).map_err(|e| e.to_string())?,
        calibration: serde_json::from_value(calibration).map_err(|e| e.to_string())?,
        genome_map: serde_json::from_value(genome_map).map_err(|e| e.to_string())?,
    })
}

/// The checked definitions of the repository.
#[allow(dead_code, reason = "some test crates build their own definitions")]
pub fn data() -> Data {
    let (calibration, genome_map) = files();
    Data::new(definitions(calibration, genome_map).expect("definitions")).expect("checked")
}

/// The data of the repository with `diseases` as its list of hereditary diseases (ADR-0017
/// section 1).
#[allow(dead_code, reason = "not every test crate needs other diseases")]
pub fn data_with_diseases(diseases: Value) -> Data {
    let (mut calibration, genome_map) = files();
    calibration["diseases"] = diseases;
    Data::new(definitions(calibration, genome_map).expect("definitions")).expect("checked")
}

/// Test data with the causes of death of a test (ADR-0017 section 2): accident `accident`
/// per year and senescence `a` = `old_age` with `b` = 0.001 (the smallest `b` of
/// `mortality`), the same for both sexes; without `old_age` senescence is practically off
/// (`a` = 1e-15); the hereditary diseases `diseases`.
#[allow(
    dead_code,
    reason = "not every test crate needs its own causes of death"
)]
pub fn death_data(accident: f64, old_age: Option<f64>, diseases: Value) -> Data {
    let (calibration, genome_map) = death_files(accident, old_age, diseases);
    Data::new(definitions(calibration, genome_map).expect("definitions")).expect("checked")
}

/// The calibration and the map of `death_data`, to change further.
#[allow(dead_code, reason = "not every test crate changes them further")]
pub fn death_files(accident: f64, old_age: Option<f64>, diseases: Value) -> (Value, Value) {
    let (mut calibration, genome_map) = files();
    let values = &mut calibration["values"];
    for sex in ["female", "male"] {
        values[format!("core:accident_{sex}")]["value"] = json!(accident);
        values[format!("core:senescence_a_{sex}")]["value"] = json!(old_age.unwrap_or(1e-15));
        values[format!("core:senescence_b_{sex}")]["value"] = json!(0.001);
    }
    calibration["diseases"] = diseases;
    (calibration, genome_map)
}

/// The repository's six hereditary diseases (ADR-0017 section 1).
#[allow(dead_code, reason = "not every test crate needs the diseases")]
pub fn repository_diseases() -> Value {
    files().0["diseases"].clone()
}

/// Test data in which only the hereditary diseases kill: accident 1e-9 per year (beyond
/// 100 000 years, so never) and senescence practically off, the repository's diseases.
#[allow(dead_code, reason = "not every test crate needs it")]
pub fn only_diseases_data() -> Data {
    death_data(1e-9, None, repository_diseases())
}

/// The disease of the walking skeleton as data (ADR-0017 section 5): `test:dummy_disease` on
/// `core:disease_ar`, penetrance 1, onset `onset`, 0.3 per year. With onset 0 it acts like
/// the old cause of `Start` for the homozygous; the acceptance scenario takes onset 18.
#[allow(dead_code, reason = "not every test crate needs the dummy disease")]
pub fn dummy_disease(onset: f64) -> Value {
    let number = |value: f64, unit: &str| json!({"value": value, "unit": unit});
    json!([{
        "code": "test:dummy_disease",
        "carrier": {"locus": {
            "locus": "core:disease_ar",
            "penetrance_female": number(1.0, "fraction"),
            "penetrance_male": number(1.0, "fraction")
        }},
        "onset": {"fixed": {"age": number(onset, "year")}},
        "effect": {"death": {"per_year": number(0.3, "per_year")}}
    }])
}

/// The alleles of a founder at the locus `core:disease_ar` of the walking skeleton: 1 for the
/// dummy allele, 0 for the reference allele, maternal then paternal; no entry for two
/// reference alleles (genetics specification 2.1).
#[allow(dead_code, reason = "not every test crate builds founders")]
pub fn dummy(maternal: u8, paternal: u8) -> Vec<sim_core::genome::LocusAlleles> {
    if (maternal, paternal) == (0, 0) {
        return Vec::new();
    }
    vec![sim_core::genome::LocusAlleles {
        locus: sim_core::data_id::DataId::new("core:disease_ar").expect("id"),
        copies: vec![maternal, paternal],
    }]
}

/// A genome map of `n` autosomal loci (segments in turn, slots from 0), with the
/// mutation direction `direction`, and the standard rate `rate`. The loci are recessive
/// and large: only the mark decides the rate (disease or not), and an additive locus
/// would need a trait (ADR-0008).
#[allow(dead_code, reason = "not every test crate needs a map of many loci")]
pub fn autosomal_map(n: u64, direction: &str, rate: f64) -> Data {
    let (mut calibration, _) = files();
    // The diseases of the repository name loci this map does not have.
    calibration["diseases"] = json!([]);
    calibration["values"]["core:mutation_rate"]["value"] = json!(rate);
    let mut loci: Vec<(u64, u64)> = (0..n).map(|i| (i % 8, i / 8)).collect();
    loci.sort_unstable();
    let loci: Vec<Value> = loci
        .iter()
        .map(|(segment, slot)| {
            json!({"id": format!("test:l{segment}_{slot}"), "chromosome": "autosome",
                "segment": segment, "slot": slot, "action": "recessive", "mark": "large",
                "mutation": direction})
        })
        .collect();
    let genome_map = json!({"format_version": 2, "map_version": 1, "traits": [], "loci": loci});
    Data::new(definitions(calibration, genome_map).expect("definitions")).expect("checked")
}

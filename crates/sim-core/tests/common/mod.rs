//! The definitions of the repository for the tests, read the way sim-cli reads them
//! (ADR-0007 section 2): the text fields of each calibration value and each locus
//! removed, the rest into the core types.

use serde_json::{Value, json};
use sim_core::data::{Data, Definitions};

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

/// The two files as JSON values without their text.
pub fn files() -> (Value, Value) {
    let read = |text: &str| serde_json::from_str::<Value>(text).expect("json");
    (
        without(read(CALIBRATION), "values", &["source", "as_of", "note"]),
        without(read(GENOME_MAP), "loci", &["description"]),
    )
}

/// Definitions from two JSON values; `Err` if serde rejects one.
pub fn definitions(calibration: Value, genome_map: Value) -> Result<Definitions, String> {
    Ok(Definitions {
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

/// The alleles of a founder at the locus `core:dummy` of the walking skeleton: 1 for the
/// dummy allele, 0 for the reference allele, maternal then paternal; no entry for two
/// reference alleles (genetics specification 2.1).
#[allow(dead_code, reason = "not every test crate builds founders")]
pub fn dummy(maternal: u8, paternal: u8) -> Vec<sim_core::genome::LocusAlleles> {
    if (maternal, paternal) == (0, 0) {
        return Vec::new();
    }
    vec![sim_core::genome::LocusAlleles {
        locus: sim_core::data_id::DataId::new("core:dummy").expect("id"),
        copies: vec![maternal, paternal],
    }]
}

/// A genome map of `n` autosomal loci (segments in turn, slots from 0), all small, with
/// the mutation direction `direction`, and the standard rate `rate`.
#[allow(dead_code, reason = "not every test crate needs a map of many loci")]
pub fn autosomal_map(n: u64, direction: &str, rate: f64) -> Data {
    let (mut calibration, _) = files();
    calibration["values"]["core:mutation_rate"]["value"] = json!(rate);
    let mut loci: Vec<(u64, u64)> = (0..n).map(|i| (i % 8, i / 8)).collect();
    loci.sort_unstable();
    let loci: Vec<Value> = loci
        .iter()
        .map(|(segment, slot)| {
            json!({"id": format!("test:l{segment}_{slot}"), "chromosome": "autosome",
                "segment": segment, "slot": slot, "action": "additive", "mark": "small",
                "mutation": direction})
        })
        .collect();
    let genome_map = json!({"format_version": 1, "map_version": 1, "loci": loci});
    Data::new(definitions(calibration, genome_map).expect("definitions")).expect("checked")
}

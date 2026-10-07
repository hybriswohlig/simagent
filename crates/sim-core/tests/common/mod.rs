//! The definitions of the repository for the tests, read the way sim-cli reads them
//! (ADR-0007 section 2): the text fields of each calibration value and each locus
//! removed, the rest into the core types.

use serde_json::Value;
use sim_core::data::{Data, Definitions};

const CALIBRATION: &str = include_str!("../../../../data/calibration/genetics.json");
const GENOME_MAP: &str = include_str!("../../../../data/genetics/genome_map.json");

/// Removes the text fields `names` of the entries under `entries`.
fn without(mut file: Value, entries: &str, names: &[&str]) -> Value {
    let list: Vec<&mut Value> = match &mut file[entries] {
        Value::Object(values) => values.values_mut().collect(),
        Value::Array(loci) => loci.iter_mut().collect(),
        _ => panic!("entries"),
    };
    for entry in list {
        for name in names {
            entry.as_object_mut().expect("entry").remove(*name);
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
pub fn data() -> Data {
    let (calibration, genome_map) = files();
    Data::new(definitions(calibration, genome_map).expect("definitions")).expect("checked")
}

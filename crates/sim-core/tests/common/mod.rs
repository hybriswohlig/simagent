//! The definitions of the repository for the tests, read the way sim-cli reads them: the
//! text fields removed, the rest into the core types (ADR-0007 section 2).

use serde_json::Value;
use sim_core::data::{Data, Definitions};

const CALIBRATION: &str = include_str!("../../../../data/calibration/genetics.json");
const GENOME_MAP: &str = include_str!("../../../../data/genetics/genome_map.json");

fn strip(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            for name in ["source", "as_of", "note", "description"] {
                fields.remove(name);
            }
            fields.values_mut().for_each(strip);
        }
        Value::Array(items) => items.iter_mut().for_each(strip),
        _ => {}
    }
}

/// The two files as JSON values without their text.
pub fn files() -> (Value, Value) {
    let read = |text: &str| {
        let mut value: Value = serde_json::from_str(text).expect("json");
        strip(&mut value);
        value
    };
    (read(CALIBRATION), read(GENOME_MAP))
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

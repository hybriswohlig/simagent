//! The definitions of the base game (ADR-0007): the files under `data/`, built into the
//! binary so that a run does not depend on the working directory. Their text is checked
//! here and removed; the rest goes into the core types, which reject unknown fields.

use serde_json::Value;
use sim_core::data::{Data, Definitions};

use crate::error::CliError;

const CALIBRATION: &str = include_str!("../../../data/calibration/genetics.json");
const GENOME_MAP: &str = include_str!("../../../data/genetics/genome_map.json");

/// The text fields of a calibration value, and of a trait or a locus (ADR-0007 section 2,
/// ADR-0008 section 1).
const CALIBRATION_TEXT: [&str; 3] = ["source", "as_of", "note"];
const LOCUS_TEXT: [&str; 1] = ["description"];

/// The checked definitions of the base game.
pub fn core_data() -> Result<Data, CliError> {
    data_from(CALIBRATION, GENOME_MAP)
}

/// The checked definitions from the text of the two files.
pub fn data_from(calibration: &str, genome_map: &str) -> Result<Data, CliError> {
    let definitions = Definitions {
        calibration: read(
            calibration,
            "calibration/genetics.json",
            calibration_entries,
            &CALIBRATION_TEXT,
        )?,
        genome_map: read(
            genome_map,
            "genetics/genome_map.json",
            map_entries,
            &LOCUS_TEXT,
        )?,
    };
    Data::new(definitions).map_err(|error| rejected(&error.to_string()))
}

fn rejected(detail: &str) -> CliError {
    CliError::new("data_rejected", detail)
}

/// The calibration values, and each of the four weights of each phenotype row, with their
/// text fields (ADR-0009 section 2); `None` unless both lists are there.
fn calibration_entries(file: &mut Value) -> Option<Vec<&mut Value>> {
    let mut all = Vec::new();
    let mut lists = 0;
    for (name, list) in file.as_object_mut()? {
        if name == "values" {
            all.extend(list.as_object_mut()?.values_mut());
            lists += 1;
        } else if name == "traits" {
            for row in list.as_array_mut()? {
                let row = row.as_object_mut()?;
                for (field, weight) in row.iter_mut() {
                    if WEIGHTS.contains(&field.as_str()) {
                        all.push(weight);
                    }
                }
            }
            lists += 1;
        }
    }
    (lists == 2).then_some(all)
}

/// The four weights of a phenotype row.
const WEIGHTS: [&str; 4] = ["h2_child", "c2_child", "h2_adult", "c2_adult"];

/// The traits and the loci of the genome map, each with its text field `description`
/// (ADR-0008 section 1); `None` unless both lists are there.
fn map_entries(file: &mut Value) -> Option<Vec<&mut Value>> {
    let mut all = Vec::new();
    let mut lists = 0;
    for (name, list) in file.as_object_mut()? {
        if name == "traits" || name == "loci" {
            all.extend(list.as_array_mut()?.iter_mut());
            lists += 1;
        }
    }
    (lists == 2).then_some(all)
}

/// Reads one file: JSON, every entry with each text field present and not blank, the
/// text removed, the rest into the core type.
fn read<T: serde::de::DeserializeOwned>(
    text: &str,
    file: &str,
    entries_of: impl FnOnce(&mut Value) -> Option<Vec<&mut Value>>,
    names: &[&str],
) -> Result<T, CliError> {
    let mut value: Value =
        serde_json::from_str(text).map_err(|error| rejected(&format!("{file}: {error}")))?;
    let entries = entries_of(&mut value).ok_or_else(|| rejected(&format!("{file}: no entries")))?;
    for entry in entries {
        let fields = entry
            .as_object_mut()
            .ok_or_else(|| rejected(&format!("{file}: an entry is not an object")))?;
        for name in names {
            match fields.remove(*name) {
                Some(Value::String(text)) if !text.trim().is_empty() => {}
                _ => return Err(rejected(&format!("{file}: {name} missing or blank"))),
            }
        }
    }
    serde_json::from_value(value).map_err(|error| rejected(&format!("{file}: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(result: Result<Data, CliError>) -> String {
        match result {
            Ok(_) => "ok".to_owned(),
            Err(error) => error.detail,
        }
    }

    #[test]
    fn the_base_game_loads() {
        // The pinned data hash of the core tests, from tools/data_reference.py: the
        // core sees the same bytes from both readers.
        assert_eq!(core_data().expect("data").hash(), 0xd5cc_b8c5_cd8b_30f4);
    }

    #[test]
    fn text_does_not_change_the_hash() {
        let noted = CALIBRATION.replacen("rounded to 0.01 M", "rounded to a hundredth", 1);
        assert_ne!(noted, CALIBRATION);
        let described = GENOME_MAP.replacen("walking skeleton", "Durchstich", 1);
        assert_ne!(described, GENOME_MAP);
        let hash = |calibration: &str, genome_map: &str| {
            data_from(calibration, genome_map).expect("data").hash()
        };
        assert_eq!(hash(&noted, &described), hash(CALIBRATION, GENOME_MAP));
    }

    #[test]
    fn text_must_be_there_and_not_blank() {
        let blank = CALIBRATION.replacen("\"as_of\": \"2026-10-07\"", "\"as_of\": \"  \"", 1);
        assert_ne!(blank, CALIBRATION);
        assert_eq!(
            code(data_from(&blank, GENOME_MAP)),
            "calibration/genetics.json: as_of missing or blank"
        );
        let missing = GENOME_MAP.replacen("\"description\":", "\"comment\":", 1);
        assert_ne!(missing, GENOME_MAP);
        assert_eq!(
            code(data_from(CALIBRATION, &missing)),
            "genetics/genome_map.json: description missing or blank"
        );
    }

    #[test]
    fn unknown_fields_and_text_in_the_wrong_place_are_rejected() {
        // A text field where the file has none stays and meets deny_unknown_fields.
        let misplaced = GENOME_MAP.replacen(
            "\"map_version\": 2,",
            "\"map_version\": 2, \"note\": \"x\",",
            1,
        );
        assert_ne!(misplaced, GENOME_MAP);
        assert!(
            code(data_from(CALIBRATION, &misplaced))
                .starts_with("genetics/genome_map.json: unknown field `note`")
        );
        let typo = GENOME_MAP.replacen("\"slot\":", "\"slots\":", 1);
        assert_ne!(typo, GENOME_MAP);
        assert!(
            code(data_from(CALIBRATION, &typo))
                .starts_with("genetics/genome_map.json: unknown field `slots`")
        );
    }

    const TRAITS_MAP: &str =
        include_str!("../../../tests/golden/data_traits/genetics/genome_map.json");
    const TRAITS_CALIBRATION: &str =
        include_str!("../../../tests/golden/data_traits/calibration/genetics.json");

    /// Traits have their text too (ADR-0008 section 1): removed before the core reads
    /// them, so the hash is the one of the independent reference; a trait without it, and
    /// a map without the list of traits, are rejected.
    #[test]
    fn traits_have_their_text() {
        let data = data_from(TRAITS_CALIBRATION, TRAITS_MAP).expect("data");
        assert_eq!(data.hash(), 0x1644_7a40_212e_298c);
        assert_eq!(data.genome_map().traits.len(), 4);
        let blank = TRAITS_MAP.replacen(
            "\"description\": \"An exclusive trait.\"",
            "\"description\": \" \"",
            1,
        );
        assert_ne!(blank, TRAITS_MAP);
        assert_eq!(
            code(data_from(TRAITS_CALIBRATION, &blank)),
            "genetics/genome_map.json: description missing or blank"
        );
        // The weights of a phenotype row have their text too (ADR-0009 section 2).
        let unsourced =
            TRAITS_CALIBRATION.replacen("\"source\": \"fixture: a face axis\"", "\"x\": 1", 1);
        assert_ne!(unsourced, TRAITS_CALIBRATION);
        assert_eq!(
            code(data_from(&unsourced, TRAITS_MAP)),
            "calibration/genetics.json: source missing or blank"
        );
        let without = GENOME_MAP.replacen("\"traits\": [],", "", 1);
        assert_ne!(without, GENOME_MAP);
        assert_eq!(
            code(data_from(CALIBRATION, &without)),
            "genetics/genome_map.json: no entries"
        );
    }
}

//! The definitions of the base game (ADR-0007): the files under `data/`, built into the
//! binary so that a run does not depend on the working directory. Their text is checked
//! here and removed; the rest goes into the core types, which reject unknown fields.

use serde_json::Value;
use sim_core::data::{Data, Definitions};

use crate::error::CliError;

const DEMOGRAPHY: &str = include_str!("../../../data/calibration/demography.json");
const CALIBRATION: &str = include_str!("../../../data/calibration/genetics.json");
const GENOME_MAP: &str = include_str!("../../../data/genetics/genome_map.json");

/// The text fields of a calibration value, and of a trait or a locus (ADR-0007 section 2,
/// ADR-0008 section 1).
const CALIBRATION_TEXT: [&str; 3] = ["source", "as_of", "note"];
const LOCUS_TEXT: [&str; 1] = ["description"];

/// The checked definitions of the base game.
pub fn core_data() -> Result<Data, CliError> {
    data_from(DEMOGRAPHY, CALIBRATION, GENOME_MAP)
}

/// The checked definitions from the text of the three files.
pub fn data_from(demography: &str, calibration: &str, genome_map: &str) -> Result<Data, CliError> {
    let definitions = Definitions {
        demography: read(
            demography,
            "calibration/demography.json",
            demography_entries,
            &CALIBRATION_TEXT,
        )?,
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

/// The calibration values, each of the four weights of each phenotype row, the two
/// numbers of each sex difference, every number of each disease and of the curves of
/// fertility, with their text fields (ADR-0009 section 2, ADR-0014, ADR-0017 section 1);
/// `None` unless all five are there.
fn calibration_entries(file: &mut Value) -> Option<Vec<&mut Value>> {
    let mut all = Vec::new();
    let mut lists = 0;
    for (name, list) in file.as_object_mut()? {
        if name == "values" {
            all.extend(list.as_object_mut()?.values_mut());
            lists += 1;
        } else if name == "traits" || name == "sex_differences" {
            let numbers: &[&str] = if name == "traits" {
                &WEIGHTS
            } else {
                &SEX_DIFFERENCE
            };
            for row in list.as_array_mut()? {
                let row = row.as_object_mut()?;
                for (field, number) in row.iter_mut() {
                    if numbers.contains(&field.as_str()) {
                        all.push(number);
                    }
                }
            }
            lists += 1;
        } else if name == "diseases" {
            for disease in list.as_array_mut()? {
                numbers(disease, &mut all);
            }
            lists += 1;
        } else if name == "fertility" {
            numbers(list, &mut all);
            lists += 1;
        }
    }
    (lists == 5).then_some(all)
}

/// The values of the life course data and the weights of the wish, with their text fields
/// (ADR-0017 section 1); `None` unless both lists are there.
fn demography_entries(file: &mut Value) -> Option<Vec<&mut Value>> {
    let mut all = Vec::new();
    let mut lists = 0;
    for (name, list) in file.as_object_mut()? {
        if name == "values" {
            all.extend(list.as_object_mut()?.values_mut());
            lists += 1;
        } else if name == "fertility_wish_shape" {
            all.extend(list.as_array_mut()?.iter_mut());
            lists += 1;
        }
    }
    (lists == 2).then_some(all)
}

/// Every number inside a disease: an object with a unit, however deep it sits.
fn numbers<'a>(value: &'a mut Value, all: &mut Vec<&'a mut Value>) {
    if value.get("unit").is_some() {
        all.push(value);
        return;
    }
    match value {
        Value::Object(fields) => fields.values_mut().for_each(|v| numbers(v, all)),
        Value::Array(items) => items.iter_mut().for_each(|v| numbers(v, all)),
        _ => {}
    }
}

/// The four weights of a phenotype row.
const WEIGHTS: [&str; 4] = ["h2_child", "c2_child", "h2_adult", "c2_adult"];
/// The two numbers of a sex difference.
const SEX_DIFFERENCE: [&str; 2] = ["shift", "variance_ratio"];

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
        assert_eq!(core_data().expect("data").hash(), 0xc466_265f_81ba_9819);
    }

    #[test]
    fn text_does_not_change_the_hash() {
        let noted = CALIBRATION.replacen("rounded to 0.01 M", "rounded to a hundredth", 1);
        assert_ne!(noted, CALIBRATION);
        let described = GENOME_MAP.replacen("walking skeleton", "Durchstich", 1);
        assert_ne!(described, GENOME_MAP);
        let hash = |calibration: &str, genome_map: &str| {
            data_from(DEMOGRAPHY, calibration, genome_map)
                .expect("data")
                .hash()
        };
        assert_eq!(hash(&noted, &described), hash(CALIBRATION, GENOME_MAP));
    }

    /// The life course data have their text like the calibration (ADR-0017 section 1): each
    /// value and each weight of the wish; it does not change the hash.
    #[test]
    fn the_life_course_data_have_their_text() {
        let noted = DEMOGRAPHY.replacen("play value", "a value for the game", 1);
        assert_ne!(noted, DEMOGRAPHY);
        let hash = |demography: &str| {
            data_from(demography, CALIBRATION, GENOME_MAP)
                .expect("data")
                .hash()
        };
        assert_eq!(hash(&noted), hash(DEMOGRAPHY));
        let blank = DEMOGRAPHY.replacen("\"as_of\": \"2026-10-10\"", "\"as_of\": \" \"", 1);
        assert_ne!(blank, DEMOGRAPHY);
        assert_eq!(
            code(data_from(&blank, CALIBRATION, GENOME_MAP)),
            "calibration/demography.json: as_of missing or blank"
        );
        let unsourced = DEMOGRAPHY.replacen(
            "\"source\": \"docs/genetik-spezifikation.md 11.3 [K11.3d]\"",
            "\"x\": 1",
            1,
        );
        assert_ne!(unsourced, DEMOGRAPHY);
        assert_eq!(
            code(data_from(&unsourced, CALIBRATION, GENOME_MAP)),
            "calibration/demography.json: source missing or blank"
        );
        let without = DEMOGRAPHY.replacen("\"fertility_wish_shape\"", "\"wish\"", 1);
        assert_ne!(without, DEMOGRAPHY);
        assert_eq!(
            code(data_from(&without, CALIBRATION, GENOME_MAP)),
            "calibration/demography.json: no entries"
        );
    }

    #[test]
    fn text_must_be_there_and_not_blank() {
        let blank = CALIBRATION.replacen("\"as_of\": \"2026-10-07\"", "\"as_of\": \"  \"", 1);
        assert_ne!(blank, CALIBRATION);
        assert_eq!(
            code(data_from(DEMOGRAPHY, &blank, GENOME_MAP)),
            "calibration/genetics.json: as_of missing or blank"
        );
        let missing = GENOME_MAP.replacen("\"description\":", "\"comment\":", 1);
        assert_ne!(missing, GENOME_MAP);
        assert_eq!(
            code(data_from(DEMOGRAPHY, CALIBRATION, &missing)),
            "genetics/genome_map.json: description missing or blank"
        );
    }

    #[test]
    fn unknown_fields_and_text_in_the_wrong_place_are_rejected() {
        // A text field where the file has none stays and meets deny_unknown_fields.
        let misplaced = GENOME_MAP.replacen(
            "\"map_version\": 3,",
            "\"map_version\": 3, \"note\": \"x\",",
            1,
        );
        assert_ne!(misplaced, GENOME_MAP);
        assert!(
            code(data_from(DEMOGRAPHY, CALIBRATION, &misplaced))
                .starts_with("genetics/genome_map.json: unknown field `note`")
        );
        let typo = GENOME_MAP.replacen("\"slot\":", "\"slots\":", 1);
        assert_ne!(typo, GENOME_MAP);
        assert!(
            code(data_from(DEMOGRAPHY, CALIBRATION, &typo))
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
        let data = data_from(DEMOGRAPHY, TRAITS_CALIBRATION, TRAITS_MAP).expect("data");
        assert_eq!(data.hash(), 0x7bd2_444d_9785_4563);
        assert_eq!(data.genome_map().traits.len(), 4);
        let blank = TRAITS_MAP.replacen(
            "\"description\": \"An exclusive trait.\"",
            "\"description\": \" \"",
            1,
        );
        assert_ne!(blank, TRAITS_MAP);
        assert_eq!(
            code(data_from(DEMOGRAPHY, TRAITS_CALIBRATION, &blank)),
            "genetics/genome_map.json: description missing or blank"
        );
        // The weights of a phenotype row have their text too (ADR-0009 section 2).
        let unsourced =
            TRAITS_CALIBRATION.replacen("\"source\": \"fixture: a face axis\"", "\"x\": 1", 1);
        assert_ne!(unsourced, TRAITS_CALIBRATION);
        assert_eq!(
            code(data_from(DEMOGRAPHY, &unsourced, TRAITS_MAP)),
            "calibration/genetics.json: source missing or blank"
        );
        // Text on the row instead of on a weight, and an unknown field on a weight.
        let on_row = TRAITS_CALIBRATION.replacen(
            "\"trait\": \"test:face\",",
            "\"trait\": \"test:face\", \"note\": \"x\",",
            1,
        );
        assert_ne!(on_row, TRAITS_CALIBRATION);
        assert!(
            code(data_from(DEMOGRAPHY, &on_row, TRAITS_MAP))
                .starts_with("calibration/genetics.json: unknown field `note`")
        );
        let unknown = TRAITS_CALIBRATION.replacen(
            "\"unit\": \"fraction\",",
            "\"unit\": \"fraction\", \"x\": 1,",
            1,
        );
        assert_ne!(unknown, TRAITS_CALIBRATION);
        assert!(
            code(data_from(DEMOGRAPHY, &unknown, TRAITS_MAP))
                .starts_with("calibration/genetics.json: unknown field `x`")
        );
        let without = GENOME_MAP.replacen("\"traits\": [],", "", 1);
        assert_ne!(without, GENOME_MAP);
        assert_eq!(
            code(data_from(DEMOGRAPHY, CALIBRATION, &without)),
            "genetics/genome_map.json: no entries"
        );
    }
}

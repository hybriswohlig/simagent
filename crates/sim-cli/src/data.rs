//! The definitions of the base game (ADR-0007): the files under `data/`, built into the
//! binary so that a run does not depend on the working directory. Their text is checked
//! here and removed; the rest goes into the core types, which reject unknown fields.

use std::collections::BTreeMap;

use serde_json::Value;
use sim_core::data::{Calibrated, Data, Definitions, ShockKind, ShockTemplate, Unit};
use sim_core::data_id::DataId;
use sim_core::finite::FiniteF64;

use crate::error::CliError;
use crate::house::{Layout, house_data_from, layout_from};

const DEMOGRAPHY: &str = include_str!("../../../data/calibration/demography.json");
const CALIBRATION: &str = include_str!("../../../data/calibration/genetics.json");
const GENOME_MAP: &str = include_str!("../../../data/genetics/genome_map.json");
const POOL: &[u8] = include_bytes!("../../../data/pool/pool.bin");
const FREQUENCIES: &str = include_str!("../../../data/pool/pool_frequencies.json");
const SHOCKS: &str = include_str!("../../../data/shocks.json");
const NEEDS: &str = include_str!("../../../data/house/needs.json");
const DAY: &str = include_str!("../../../data/house/day.json");
const KINDS: &str = include_str!("../../../data/house/kinds.json");
/// The houses with their names in the messages, in the order of the file names.
const HOUSES: [(&str, &str); 3] = [
    (
        "house/houses/flat.json",
        include_str!("../../../data/house/houses/flat.json"),
    ),
    (
        "house/houses/house.json",
        include_str!("../../../data/house/houses/house.json"),
    ),
    (
        "house/houses/terraced.json",
        include_str!("../../../data/house/houses/terraced.json"),
    ),
];

/// The drawings of the houses, in the order of `HOUSES` (ADR-0025 section 1): not part of the
/// data, the core never reads them.
const LAYOUTS: [(&str, &str); 3] = [
    (
        "house/houses/flat.layout.json",
        include_str!("../../../data/house/houses/flat.layout.json"),
    ),
    (
        "house/houses/house.layout.json",
        include_str!("../../../data/house/houses/house.layout.json"),
    ),
    (
        "house/houses/terraced.layout.json",
        include_str!("../../../data/house/houses/terraced.layout.json"),
    ),
];

/// The drawings of the repository's houses by house, each checked against its house file.
pub fn layouts() -> Result<BTreeMap<DataId, Layout>, CliError> {
    (HOUSES.iter().zip(LAYOUTS))
        .map(|(&(_, house), (name, text))| {
            let layout = layout_from(name, text, house)?;
            Ok((layout.house.clone(), layout))
        })
        .collect()
}

/// The names of each kind's offers in `kinds.json`, in their order: the core numbers them and
/// drops the names, the house page shows them.
pub fn offer_names() -> Result<BTreeMap<DataId, Vec<String>>, CliError> {
    let refused = || CliError::new("data_rejected", "kinds.json: the offers");
    let kinds: Value = serde_json::from_str(KINDS).map_err(|_| refused())?;
    (kinds.as_array().ok_or_else(refused)?.iter())
        .map(|kind| {
            let id = (kind["id"].as_str()).and_then(|id| DataId::new(id).ok());
            let offers = (kind["offers"].as_array().into_iter().flatten())
                .map(|offer| offer["id"].as_str().map(str::to_owned))
                .collect::<Option<Vec<String>>>();
            Ok((id.ok_or_else(refused)?, offers.ok_or_else(refused)?))
        })
        .collect()
}

/// The text fields of a calibration value, and of a trait or a locus (ADR-0007 section 2,
/// ADR-0008 section 1).
const CALIBRATION_TEXT: [&str; 3] = ["source", "as_of", "note"];
const LOCUS_TEXT: [&str; 1] = ["description"];

/// The file a runs directory may hold (`<runs>/settings.json`). When it is there,
/// `simagent view` builds its worlds from these numbers instead of the built-in
/// calibration. Each key is one calibration value (`core:market_age`, `female_ramp`,
/// the rates, the curve anchors). A new town keeps the data hash of the result. An old
/// town opens only while the file still produces the hash it was founded with.
///
/// Adulthood stays 18 and leaving home still starts at 16: both are the household rule
/// (ADR-0018 section 4), not calibration values.
pub const SETTINGS_FILE: &str = "settings.json";

/// The checked definitions of the base game, with the pool of the pre-run and its
/// frequencies (ADR-0020 sections 4 and 5), the shock templates (ADR-0023 section 3) and
/// the houses (ADR-0025).
pub fn core_data() -> Result<Data, CliError> {
    assemble(DEMOGRAPHY, CALIBRATION)
}

/// The base game with the calibration values named in a `simagent.settings` document
/// replaced. A key must name exactly one value. The core then checks the ranges, so a
/// market age of 11 is refused here as it would be in the file.
pub fn data_with_settings(text: &str) -> Result<Data, CliError> {
    let (demography, calibration) = apply_settings(DEMOGRAPHY, CALIBRATION, text)?;
    let base = assemble(DEMOGRAPHY, CALIBRATION)?;
    if demography == DEMOGRAPHY && calibration == CALIBRATION {
        return Ok(base);
    }
    // The pool is sealed to the built-in definitions. The replacement is checked on top
    // of that pool, and the data hash changes with it.
    let definitions = definitions_from(&demography, &calibration, GENOME_MAP)?;
    base.with_calibration(definitions.demography, definitions.calibration)
        .map_err(|error| rejected(&error.to_string()))
}

fn assemble(demography: &str, calibration: &str) -> Result<Data, CliError> {
    let definitions = definitions_from(demography, calibration, GENOME_MAP)?;
    let shocks = shocks_from(SHOCKS)?;
    let houses = house_data_from(NEEDS, DAY, KINDS, &HOUSES)?;
    Data::with_pool(definitions, POOL, frequencies_from(FREQUENCIES)?)
        .and_then(|data| data.with_shocks(shocks))
        .map(|data| data.with_houses(houses))
        .map_err(|error| rejected(&error.to_string()))
}

/// Replaces calibration values in the two built-in documents. The key is the value's
/// name (`core:market_age` in the demography file, `female_ramp` under fertility).
fn apply_settings(
    demography: &str,
    calibration: &str,
    text: &str,
) -> Result<(String, String), CliError> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct File {
        format: String,
        version: u32,
        values: BTreeMap<String, f64>,
    }
    let file: File =
        serde_json::from_str(text).map_err(|error| rejected(&format!("settings.json: {error}")))?;
    if (file.format.as_str(), file.version) != ("simagent.settings", 1) {
        return Err(rejected("settings.json: not simagent.settings version 1"));
    }
    if file.values.is_empty() {
        return Ok((demography.to_owned(), calibration.to_owned()));
    }
    let mut demography: Value = serde_json::from_str(demography)
        .map_err(|error| rejected(&format!("calibration/demography.json: {error}")))?;
    let mut calibration: Value = serde_json::from_str(calibration)
        .map_err(|error| rejected(&format!("calibration/genetics.json: {error}")))?;
    for (key, number) in &file.values {
        if !number.is_finite() {
            return Err(rejected(&format!(
                "settings.json: {key} is not a finite number"
            )));
        }
        let hits = replace_calibrated(&mut demography, key, *number)
            + replace_calibrated(&mut calibration, key, *number);
        if hits != 1 {
            let why = if hits == 0 {
                "is not a calibration value"
            } else {
                "names more than one value"
            };
            return Err(rejected(&format!("settings.json: {key} {why}")));
        }
    }
    let demography = serde_json::to_string(&demography)
        .map_err(|error| rejected(&format!("settings.json: {error}")))?;
    let calibration = serde_json::to_string(&calibration)
        .map_err(|error| rejected(&format!("settings.json: {error}")))?;
    Ok((demography, calibration))
}

/// Writes `number` into the one calibrated object named `key`. An object counts when it
/// has `value` and `unit`. Returns how many it wrote.
fn replace_calibrated(value: &mut Value, key: &str, number: f64) -> usize {
    let mut found = 0;
    fn walk(value: &mut Value, key: &str, number: f64, found: &mut usize) {
        match value {
            Value::Array(items) => {
                for item in items {
                    walk(item, key, number, found);
                }
            }
            Value::Object(fields) => {
                let hit = fields.get(key).is_some_and(|entry| {
                    entry.get("unit").is_some() && entry.get("value").is_some()
                });
                if hit {
                    fields
                        .get_mut(key)
                        .and_then(Value::as_object_mut)
                        .expect("the entry is an object")
                        .insert("value".to_owned(), serde_json::json!(number));
                    *found += 1;
                }
                for (name, child) in fields {
                    if name != key {
                        walk(child, key, number, found);
                    }
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
        }
    }
    walk(value, key, number, &mut found);
    found
}

/// The shock templates of `shocks.json` (ADR-0023 section 3 and its addendum of
/// 2026-11-01): the file's format and version, each template with its text (`source`)
/// removed and with exactly the fields of its kind; the core checks the values.
pub fn shocks_from(text: &str) -> Result<Vec<ShockTemplate>, CliError> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "snake_case")]
    enum Kind {
        Hunger,
        FertilityBoost,
    }
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Entry {
        id: DataId,
        kind: Kind,
        years: Option<u32>,
        index: Option<FiniteF64>,
        months: Option<u32>,
        rate: Option<FiniteF64>,
        #[serde(rename = "source")]
        _source: String,
    }
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct File {
        format: String,
        version: u32,
        shocks: Vec<serde_json::Map<String, serde_json::Value>>,
    }
    let file: File =
        serde_json::from_str(text).map_err(|error| rejected(&format!("shocks.json: {error}")))?;
    if (file.format.as_str(), file.version) != ("simagent.shocks", 1) {
        return Err(rejected("shocks.json: not simagent.shocks version 1"));
    }
    (file.shocks.into_iter())
        .map(|fields| {
            // Exactly the fields of the kind: a field of another kind is refused even as
            // `null`, which the options below could not tell from a missing one (review).
            let own: &[&str] = match fields.get("kind").and_then(serde_json::Value::as_str) {
                Some("hunger") => &["id", "kind", "years", "index", "source"],
                Some("fertility_boost") => &["id", "kind", "months", "rate", "source"],
                _ => &[],
            };
            if fields.len() != own.len() || !own.iter().all(|key| fields.contains_key(*key)) {
                return Err(rejected(
                    "shocks.json: a template without exactly its kind's fields",
                ));
            }
            let entry: Entry = serde_json::from_value(serde_json::Value::Object(fields))
                .map_err(|error| rejected(&format!("shocks.json: {error}")))?;
            let kind = match (
                entry.kind,
                entry.years,
                entry.index,
                entry.months,
                entry.rate,
            ) {
                (Kind::Hunger, Some(years), Some(index), None, None) => {
                    ShockKind::Hunger { years, index }
                }
                (Kind::FertilityBoost, None, None, Some(months), Some(rate)) => {
                    ShockKind::FertilityBoost { months, rate }
                }
                _ => {
                    return Err(rejected(&format!(
                        "shocks.json: {} lacks a field of its kind or has another kind's",
                        entry.id.as_str()
                    )));
                }
            };
            Ok(ShockTemplate { id: entry.id, kind })
        })
        .collect()
}

/// The frequencies of the pool's loci (`pool/pool_frequencies.json`): per locus a value of
/// the unit `fraction` with its text fields, checked and removed like a calibration value;
/// the core checks which loci and the range.
pub fn frequencies_from(text: &str) -> Result<BTreeMap<DataId, FiniteF64>, CliError> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct File {
        pool_frequencies: BTreeMap<DataId, Calibrated>,
    }
    let file: File = read(
        text,
        "pool/pool_frequencies.json",
        |file| {
            Some(
                file.get_mut("pool_frequencies")?
                    .as_object_mut()?
                    .values_mut()
                    .collect(),
            )
        },
        &CALIBRATION_TEXT,
    )?;
    file.pool_frequencies
        .into_iter()
        .map(|(locus, entry)| match entry.unit {
            Unit::Fraction => Ok((locus, entry.value)),
            _ => Err(rejected(&format!(
                "pool/pool_frequencies.json: {} is not a fraction",
                locus.as_str()
            ))),
        })
        .collect()
}

/// The checked definitions from the text of the three files.
pub fn data_from(demography: &str, calibration: &str, genome_map: &str) -> Result<Data, CliError> {
    Data::new(definitions_from(demography, calibration, genome_map)?)
        .map_err(|error| rejected(&error.to_string()))
}

/// The definitions from the text of the three files, their text fields checked and
/// removed; the data check comes after, with or without a pool (ADR-0020 section 5).
pub fn definitions_from(
    demography: &str,
    calibration: &str,
    genome_map: &str,
) -> Result<Definitions, CliError> {
    Ok(Definitions {
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
    })
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

/// The values of the life course data, the weights of the wish and of leaving home, with
/// their text fields (ADR-0017 section 1, ADR-0018 section 4); `None` unless all three
/// lists are there.
fn demography_entries(file: &mut Value) -> Option<Vec<&mut Value>> {
    let mut all = Vec::new();
    let mut lists = 0;
    for (name, list) in file.as_object_mut()? {
        if name == "values" {
            all.extend(list.as_object_mut()?.values_mut());
            lists += 1;
        } else if name == "fertility_wish_shape" || name == "leave_home_ages" {
            all.extend(list.as_array_mut()?.iter_mut());
            lists += 1;
        }
    }
    (lists == 3).then_some(all)
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

    /// Map 3 as a fixture: these tests edit its text (design note of the map 4 switch,
    /// section 4); the built-in data are the subject of `the_base_game_loads` only.
    const MAP3_CALIBRATION: &str =
        include_str!("../../../tests/golden/map3/calibration/genetics.json");
    const MAP3_GENOME_MAP: &str =
        include_str!("../../../tests/golden/map3/genetics/genome_map.json");

    /// ADR-0023, addendum of 2026-11-01: each template with exactly the fields of its kind.
    #[test]
    fn a_template_has_the_fields_of_its_kind() {
        let file = |entries: &str| {
            format!(r#"{{"format":"simagent.shocks","version":1,"shocks":[{entries}]}}"#)
        };
        let boost = r#"{"id":"core:time_together","kind":"fertility_boost","months":3,"rate":0.3,"source":"x"}"#;
        let hunger = r#"{"id":"core:h","kind":"hunger","years":1,"index":0.7,"source":"x"}"#;
        let read = shocks_from(&file(&format!("{boost},{hunger}"))).expect("reads");
        assert_eq!(read[0].boost().map(|(m, r)| (m, r.get())), Some((3, 0.3)));
        assert_eq!(read[1].hunger().map(|(y, i)| (y, i.get())), Some((1, 0.7)));
        for wrong in [
            r#"{"id":"core:h","kind":"hunger","years":1,"index":0.7,"months":null,"source":"x"}"#,
            r#"{"id":"core:b","kind":"fertility_boost","months":3,"rate":null,"source":"x"}"#,
            r#"{"id":"core:b","kind":"other","source":"x"}"#,
            r#"{"id":"core:b","kind":"fertility_boost","months":3,"source":"x"}"#,
            r#"{"id":"core:b","kind":"fertility_boost","months":3,"rate":0.3,"years":1,"source":"x"}"#,
            r#"{"id":"core:h","kind":"hunger","years":1,"index":0.7,"rate":0.3,"source":"x"}"#,
        ] {
            assert!(shocks_from(&file(wrong)).is_err(), "{wrong}");
        }
    }

    #[test]
    fn the_base_game_loads() {
        // The data hash of the built-in data, map 4 with the pool and its frequencies, the
        // shock templates and the houses, from tools/data_reference.py (line `data_hash`).
        assert_eq!(core_data().expect("data").hash(), 0xc1d4_68d5_44a4_665f);
    }

    fn puberty_ramp(data: &Data) -> f64 {
        match data.genetics().fertility {
            sim_core::fertility::Curves::Puberty(curve) => curve.female_ramp,
            sim_core::fertility::Curves::Window { .. } => {
                panic!("the base game has a puberty curve")
            }
        }
    }

    /// A settings document replaces calibration values. The market age is one of them.
    /// An unknown name, a value outside its range, and a document of another format are
    /// refused. The built-in data stay as they are.
    #[test]
    fn a_settings_document_replaces_calibration_values() {
        let base = core_data().expect("data");
        let settings = |values: &str| {
            format!(r#"{{"format":"simagent.settings","version":1,"values":{{{values}}}}}"#)
        };
        let aged = data_with_settings(&settings(r#""core:market_age":12"#)).expect("reads");
        assert_eq!(aged.demography().market.age_years, 12);
        assert_eq!(
            aged.demography().market.candidates,
            base.demography().market.candidates
        );
        assert_eq!(
            aged.demography().fertility.adult_rate,
            base.demography().fertility.adult_rate
        );
        assert_eq!(
            aged.demography().market.search_center,
            base.demography().market.search_center
        );
        assert_ne!(aged.hash(), base.hash());
        let curve = data_with_settings(&settings(r#""female_ramp":4"#)).expect("reads");
        let (curve_ramp, base_ramp) = (puberty_ramp(&curve), puberty_ramp(&base));
        assert_eq!(curve_ramp, 4.0);
        assert_ne!(base_ramp, 4.0);
        assert_ne!(curve.hash(), base.hash());
        let same = data_with_settings(&settings("")).expect("empty");
        assert_eq!(same.hash(), base.hash());
        assert_eq!(core_data().expect("data").hash(), base.hash());
        for refused in [
            settings(r#""core:no_such_value":1"#),
            settings(r#""core:market_age":11"#),
            r#"{"format":"simagent.settings","version":2,"values":{}}"#.to_owned(),
            r#"{"format":"simagent.settings","version":1,"values":{},"extra":1}"#.to_owned(),
        ] {
            assert!(data_with_settings(&refused).is_err(), "{refused}");
        }
    }

    /// The houses of `data/house/` (ADR-0025 section 3): one to three members take the flat,
    /// four and five the terraced house, six to eight the house, nine none.
    #[test]
    fn the_base_game_has_its_houses() {
        let data = core_data().expect("data");
        let houses = data.houses().expect("the houses");
        let chosen: Vec<Option<&str>> = (1..=9)
            .map(|members| houses.house_for(members).map(|house| house.id().as_str()))
            .collect();
        let (flat, terraced, house) =
            (Some("core:flat"), Some("core:terraced"), Some("core:house"));
        let expected = [
            flat, flat, flat, terraced, terraced, house, house, house, None,
        ];
        assert_eq!(chosen, expected);
    }

    /// The pool's frequencies are read like calibration values: the unit `fraction`, the three
    /// text fields filled, no other field; what is wrong names the file (review).
    #[test]
    fn the_frequencies_file_is_checked() {
        let detail = |text: &str| match frequencies_from(text) {
            Ok(_) => "ok".to_owned(),
            Err(error) => error.detail,
        };
        assert_eq!(detail(FREQUENCIES), "ok");
        let unit = FREQUENCIES.replacen(r#""unit": "fraction""#, r#""unit": "ratio""#, 1);
        assert_ne!(unit, FREQUENCIES);
        assert!(
            detail(&unit).ends_with("is not a fraction"),
            "{}",
            detail(&unit)
        );
        let edited = |edit: &dyn Fn(&mut serde_json::Map<String, Value>)| {
            let mut file: Value = serde_json::from_str(FREQUENCIES).expect("json");
            edit(
                file["pool_frequencies"]["core:mc1r"]
                    .as_object_mut()
                    .expect("an entry"),
            );
            detail(&file.to_string())
        };
        for field in ["source", "as_of", "note"] {
            assert_eq!(
                edited(&|entry| {
                    entry.remove(field);
                }),
                format!("pool/pool_frequencies.json: {field} missing or blank"),
                "{field}"
            );
            assert_eq!(
                edited(&|entry| {
                    entry.insert(field.to_owned(), Value::from(" "));
                }),
                format!("pool/pool_frequencies.json: {field} missing or blank"),
                "{field} blank"
            );
        }
        let unknown = edited(&|entry| {
            entry.insert("extra".to_owned(), Value::from(1));
        });
        assert!(
            unknown.starts_with("pool/pool_frequencies.json: unknown field `extra`"),
            "{unknown}"
        );
        let unwrapped = FREQUENCIES.replacen("pool_frequencies", "frequencies", 1);
        assert_eq!(detail(&unwrapped), "pool/pool_frequencies.json: no entries");
    }

    #[test]
    fn text_does_not_change_the_hash() {
        let noted = MAP3_CALIBRATION.replacen("rounded to 0.01 M", "rounded to a hundredth", 1);
        assert_ne!(noted, MAP3_CALIBRATION);
        let described = MAP3_GENOME_MAP.replacen("walking skeleton", "Durchstich", 1);
        assert_ne!(described, MAP3_GENOME_MAP);
        let hash = |calibration: &str, genome_map: &str| {
            data_from(DEMOGRAPHY, calibration, genome_map)
                .expect("data")
                .hash()
        };
        assert_eq!(
            hash(&noted, &described),
            hash(MAP3_CALIBRATION, MAP3_GENOME_MAP)
        );
    }

    /// The life course data have their text like the calibration (ADR-0017 section 1): each
    /// value and each weight of the wish; it does not change the hash.
    #[test]
    fn the_life_course_data_have_their_text() {
        let noted = DEMOGRAPHY.replacen("play value", "a value for the game", 1);
        assert_ne!(noted, DEMOGRAPHY);
        let hash = |demography: &str| {
            data_from(demography, MAP3_CALIBRATION, MAP3_GENOME_MAP)
                .expect("data")
                .hash()
        };
        assert_eq!(hash(&noted), hash(DEMOGRAPHY));
        let blank = DEMOGRAPHY.replacen("\"as_of\": \"2026-10-10\"", "\"as_of\": \" \"", 1);
        assert_ne!(blank, DEMOGRAPHY);
        assert_eq!(
            code(data_from(&blank, MAP3_CALIBRATION, MAP3_GENOME_MAP)),
            "calibration/demography.json: as_of missing or blank"
        );
        let unsourced = DEMOGRAPHY.replacen(
            "\"source\": \"docs/genetik-spezifikation.md 11.3 [K11.3d]\"",
            "\"x\": 1",
            1,
        );
        assert_ne!(unsourced, DEMOGRAPHY);
        assert_eq!(
            code(data_from(&unsourced, MAP3_CALIBRATION, MAP3_GENOME_MAP)),
            "calibration/demography.json: source missing or blank"
        );
        let without = DEMOGRAPHY.replacen("\"fertility_wish_shape\"", "\"wish\"", 1);
        assert_ne!(without, DEMOGRAPHY);
        assert_eq!(
            code(data_from(&without, MAP3_CALIBRATION, MAP3_GENOME_MAP)),
            "calibration/demography.json: no entries"
        );
    }

    #[test]
    fn text_must_be_there_and_not_blank() {
        let blank = MAP3_CALIBRATION.replacen("\"as_of\": \"2026-10-07\"", "\"as_of\": \"  \"", 1);
        assert_ne!(blank, MAP3_CALIBRATION);
        assert_eq!(
            code(data_from(DEMOGRAPHY, &blank, MAP3_GENOME_MAP)),
            "calibration/genetics.json: as_of missing or blank"
        );
        let missing = MAP3_GENOME_MAP.replacen("\"description\":", "\"comment\":", 1);
        assert_ne!(missing, MAP3_GENOME_MAP);
        assert_eq!(
            code(data_from(DEMOGRAPHY, MAP3_CALIBRATION, &missing)),
            "genetics/genome_map.json: description missing or blank"
        );
    }

    #[test]
    fn unknown_fields_and_text_in_the_wrong_place_are_rejected() {
        // A text field where the file has none stays and meets deny_unknown_fields.
        let misplaced = MAP3_GENOME_MAP.replacen(
            "\"map_version\": 3,",
            "\"map_version\": 3, \"note\": \"x\",",
            1,
        );
        assert_ne!(misplaced, MAP3_GENOME_MAP);
        assert!(
            code(data_from(DEMOGRAPHY, MAP3_CALIBRATION, &misplaced))
                .starts_with("genetics/genome_map.json: unknown field `note`")
        );
        let typo = MAP3_GENOME_MAP.replacen("\"slot\":", "\"slots\":", 1);
        assert_ne!(typo, MAP3_GENOME_MAP);
        assert!(
            code(data_from(DEMOGRAPHY, MAP3_CALIBRATION, &typo))
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
        assert_eq!(data.hash(), 0x96ec_ba88_f9e4_f738);
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
        let without = MAP3_GENOME_MAP.replacen("\"traits\": [],", "", 1);
        assert_ne!(without, MAP3_GENOME_MAP);
        assert_eq!(
            code(data_from(DEMOGRAPHY, MAP3_CALIBRATION, &without)),
            "genetics/genome_map.json: no entries"
        );
    }
}

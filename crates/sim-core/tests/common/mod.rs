//! The definitions of the repository for the tests, read the way sim-cli reads them
//! (ADR-0007 section 2, ADR-0008 section 1): the text fields of each calibration value,
//! each trait and each locus removed, the rest into the core types.

use serde_json::{Value, json};
use sim_core::data::{Data, Definitions};

const DEMOGRAPHY: &str = include_str!("../../../../data/calibration/demography.json");
/// Map 3 as a fixture (`tests/golden/map3/`): the subject of these tests is a rule, not
/// the repository's map (design note of the map 4 switch, section 4).
const CALIBRATION: &str = include_str!("../../../../tests/golden/map3/calibration/genetics.json");
const GENOME_MAP: &str = include_str!("../../../../tests/golden/map3/genetics/genome_map.json");

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
    demography_from(DEMOGRAPHY)
}

/// Life course data from its text (a fixture's), as a JSON value without its text.
#[allow(
    dead_code,
    reason = "not every test crate reads other life course data"
)]
pub fn demography_from(text: &str) -> Value {
    let read: Value = serde_json::from_str(text).expect("json");
    let texts = &["source", "as_of", "note"];
    without(
        without(
            without(read, "values", texts),
            "fertility_wish_shape",
            texts,
        ),
        "leave_home_ages",
        texts,
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
    values["core:market_search_height_female"]["value"] = json!(0.0);
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

/// The frequencies of the alternative allele at the seven pool loci the golden pool `pool`
/// was made with, as `Data::with_pool` takes them (slice S2a of the map-4 switch; the
/// reference's `arrival_reference.pool_frequencies`): the values of
/// `tests/golden/pool/pool_frequencies.json` for `pool_a`, `pool_b`, `pool_c` (and every
/// pool changed from it), `pool_e` and `pool_f`; for `pool_d` those of its pool run
/// `frequent_alleles` (`tools/pool_reference.py`).
#[allow(dead_code, reason = "not every test crate loads a pool")]
pub fn pool_frequencies(
    pool: &str,
) -> std::collections::BTreeMap<sim_core::data_id::DataId, sim_core::finite::FiniteF64> {
    let values: Vec<(String, f64)> = match pool {
        "pool_a" | "pool_b" | "pool_c" | "pool_e" | "pool_f" => {
            let file: Value = serde_json::from_str(include_str!(
                "../../../../tests/golden/pool/pool_frequencies.json"
            ))
            .expect("json");
            file["pool_frequencies"]
                .as_object()
                .expect("the entries")
                .iter()
                .map(|(id, entry)| (id.clone(), entry["value"].as_f64().expect("a value")))
                .collect()
        }
        "pool_d" => [
            ("core:disease_ar", 0.4),
            ("core:disease_ad_late", 0.3),
            ("core:disease_ad_partial", 0.35),
            ("core:disease_xr", 0.45),
            ("core:disease_mt", 0.25),
            ("core:mc1r", 0.6),
            ("core:herc2", 0.55),
        ]
        .map(|(id, q)| (id.to_owned(), q))
        .to_vec(),
        _ => panic!("no golden pool {pool}"),
    };
    values
        .into_iter()
        .map(|(id, q)| {
            (
                sim_core::data_id::DataId::new(&id).expect("id"),
                sim_core::finite::FiniteF64::try_new(q).expect("finite"),
            )
        })
        .collect()
}

/// Alleles that name every locus of the data's map, in map order (Grok F1: a founder of
/// `Start` draws every locus it does not name; the convention of the reference's
/// `world_reference.fixed_alleles`): the copies of `named` where it names the locus, the
/// reference allele on every copy elsewhere (`[0, 0]`, one `[0]` on a man's X and Y and on
/// the mt; a woman names no Y locus). Such a founder draws nothing, so it has the genome its
/// named alleles alone gave before F1.
#[allow(
    dead_code,
    reason = "not every test crate builds founders on data that draw"
)]
pub fn fixed(
    data: &Data,
    sex: sim_core::genome::Sex,
    named: &[sim_core::genome::LocusAlleles],
) -> Vec<sim_core::genome::LocusAlleles> {
    use sim_core::data::Chromosome;
    use sim_core::genome::Sex;
    let loci = &data.genome_map().loci;
    for alleles in named {
        assert!(
            loci.iter().any(|locus| locus.id == alleles.locus),
            "{} is no locus of the map",
            alleles.locus.as_str()
        );
    }
    loci.iter()
        .filter_map(|locus| {
            let copies = match (locus.chromosome, sex) {
                (Chromosome::Autosome, _) | (Chromosome::X, Sex::Female) => 2,
                (Chromosome::Y, Sex::Female) => 0,
                (Chromosome::X | Chromosome::Y | Chromosome::Mt, _) => 1,
            };
            let given = named.iter().find(|alleles| alleles.locus == locus.id);
            if copies == 0 {
                assert!(given.is_none(), "a woman has no Y locus");
                return None;
            }
            Some(sim_core::genome::LocusAlleles {
                locus: locus.id.clone(),
                copies: given.map_or_else(|| vec![0; copies], |alleles| alleles.copies.clone()),
            })
        })
        .collect()
}

/// The repository's data (map 4 with the pool, ADR-0020): `data/`.
const REPOSITORY_CALIBRATION: &str = include_str!("../../../../data/calibration/genetics.json");
const REPOSITORY_GENOME_MAP: &str = include_str!("../../../../data/genetics/genome_map.json");
#[allow(dead_code, reason = "only the tests on the repository's data need it")]
pub const REPOSITORY_POOL: &[u8] = include_bytes!("../../../../data/pool/pool.bin");
const REPOSITORY_FREQUENCIES: &str = include_str!("../../../../data/pool/pool_frequencies.json");

/// The frequencies of `data/pool/pool_frequencies.json`, each entry's `value`, as
/// `Data::with_pool` takes them (sim-cli reads the file the same way, its text removed).
#[allow(
    dead_code,
    reason = "only the tests on the repository's data need them"
)]
pub fn repository_frequencies()
-> std::collections::BTreeMap<sim_core::data_id::DataId, sim_core::finite::FiniteF64> {
    let file: Value = serde_json::from_str(REPOSITORY_FREQUENCIES).expect("json");
    file["pool_frequencies"]
        .as_object()
        .expect("the entries")
        .iter()
        .map(|(id, entry)| {
            assert_eq!(entry["unit"], "fraction", "{id}");
            let q = entry["value"].as_f64().expect("a value");
            (
                sim_core::data_id::DataId::new(id).expect("id"),
                sim_core::finite::FiniteF64::try_new(q).expect("finite"),
            )
        })
        .collect()
}

/// The repository's data as the CLI builds them: the life course data, the calibration and
/// the map of `data/` without their text, with the pool and its frequencies.
#[allow(
    dead_code,
    reason = "only the tests on the repository's data need them"
)]
pub fn repository_data() -> Data {
    let (calibration, genome_map) = files_from(REPOSITORY_CALIBRATION, REPOSITORY_GENOME_MAP);
    Data::with_pool(
        definitions(calibration, genome_map).expect("definitions"),
        REPOSITORY_POOL,
        repository_frequencies(),
    )
    .expect("data")
}

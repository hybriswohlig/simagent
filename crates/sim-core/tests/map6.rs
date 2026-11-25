//! Map 6 (ADR-0034): the hair extension on top of the temperament extension, loaded into the
//! frozen pool like the temperament loci (ADR-0033 section 2) through the ordered table of
//! extensions. `common::map6_files` builds map 6 from map 5 (the repository's map 6 without
//! the hair entries) with the patch `tools/data_reference.py` prints, and checks it equals the
//! repository's map 6.
//!
//! The expected values come from `tools/data_reference.py` (rows `map6 ...` and
//! `map6_rejection ...`) and `tools/pool_extension_reference.py real` (rows `map6_...`).

mod common;

use serde_json::{Value, json};
use sim_core::data::{Data, DataError, HairDryness};
use sim_core::data_id::DataId;
use sim_core::genome::{Genome, LocusAlleles, Sex};
use sim_core::pool::Pool;

fn id(text: &str) -> DataId {
    DataId::new(text).expect("id")
}

/// Map 6 and its calibration alone, with the extract's life course data.
fn map6_definitions_data() -> Data {
    let (calibration, genome_map) = common::map6_files();
    Data::new(common::definitions(calibration, genome_map).expect("definitions")).expect("map 6")
}

/// `Data::new` on map 6 changed by `change`.
fn changed(change: impl FnOnce(&mut Value, &mut Value)) -> Result<Data, DataError> {
    let (mut calibration, mut genome_map) = common::map6_files();
    change(&mut calibration, &mut genome_map);
    Data::new(common::definitions(calibration, genome_map).expect("definitions"))
}

fn retain(list: &mut Value, keep: impl Fn(&Value) -> bool) {
    list.as_array_mut().expect("list").retain(keep);
}

fn locus_mut<'a>(genome_map: &'a mut Value, locus: &str) -> &'a mut Value {
    genome_map["loci"]
        .as_array_mut()
        .expect("loci")
        .iter_mut()
        .find(|entry| entry["id"] == locus)
        .expect("the locus")
}

const TEMPERAMENT: [&str; 5] = [
    "core:agreeableness",
    "core:conscientiousness",
    "core:extraversion",
    "core:neuroticism",
    "core:openness",
];

/// The temperament trait an id belongs to: the trait itself, a locus `{trait}_NN` or a
/// value `{trait}_development_...`.
fn of_temperament(text: &str) -> bool {
    TEMPERAMENT.iter().any(|axis| {
        text == *axis
            || text
                .strip_prefix(axis)
                .is_some_and(|rest| rest.starts_with('_'))
    })
}

/// Row `map6 definitions_hash` (with the extract's life course data) and row `map6
/// data_hash ... pool_hash` (as the CLI builds the data: the game's life course data, the
/// pool extended with 42 loci, shocks, houses, social data, upbringing). The map-5 data hash
/// and the map-5 extended pool stay the pins of `tests/village.rs`.
#[test]
fn map6_loads_with_the_reference_hashes() {
    let definitions = map6_definitions_data();
    assert_eq!(definitions.genome_map().map_version, 6);
    assert_eq!(definitions.genome_map().traits.len(), 45);
    assert_eq!(definitions.genome_map().loci.len(), 170);
    assert_eq!(definitions.hash(), 0xb037_c9de_ff20_1479);

    let pool = Pool::from_bytes(common::REPOSITORY_POOL).expect("the pool");
    // The definitions without both extensions (map 4): the pool's frozen hash, which is
    // also that of map 5 without the temperament extension.
    assert_eq!(pool.frozen_hash, 0x4864_bcf4_0e82_4de2);
    let data = common::map6_data();
    assert_eq!(data.hash(), 0x6b43_f872_efc1_daa1);
    assert_eq!(data.pool().expect("a pool").hash(), 0x20e1_34c2_1553_132b);

    let map5 = common::map5_data();
    assert_eq!(map5.hash(), 0x5a9b_dc91_8b46_8f1c);
    assert_eq!(map5.pool().expect("a pool").hash(), 0x12ec_62c5_7af2_a290);
}

/// The stripped hash drops everything the extensions own, phenotype rows and calibration
/// values included: a changed hair value still meets the frozen hash, a changed old value
/// does not.
#[test]
fn the_frozen_hash_ignores_what_the_extensions_own_only() {
    let with_pool = |change: fn(&mut Value, &mut Value)| {
        let (mut calibration, mut genome_map) = common::map6_files();
        change(&mut calibration, &mut genome_map);
        Data::with_pool(
            common::definitions(calibration, genome_map).expect("definitions"),
            common::REPOSITORY_POOL,
            common::repository_frequencies(),
        )
        .map(|data| data.hash())
    };
    assert!(
        with_pool(|c, _| c["values"]["core:body_dryness_tempo"]["value"] = json!(11.0)).is_ok()
    );
    assert!(
        with_pool(|c, _| {
            let rows = c["traits"].as_array_mut().expect("rows");
            let row = (rows.iter_mut())
                .find(|r| r["trait"] == "core:hair_thickness")
                .expect("the row");
            row["h2_adult"]["value"] = json!(0.25);
        })
        .is_ok()
    );
    assert_eq!(
        with_pool(|c, _| c["values"]["core:map_length_x_female"]["value"] = json!(1.8)),
        Err(DataError::PoolFrozenHash)
    );
}

/// Each row `map6_rejection` of the reference, rejected with the code of the rule it breaks.
#[test]
fn every_rejection_case_of_the_reference_is_rejected() {
    // half extension (one hair locus)
    assert_eq!(
        changed(|_, m| retain(&mut m["loci"], |l| l["id"] != "core:hair_thickness_02")).err(),
        Some(DataError::CalibrationMissing)
    );
    // hair without temperament: map 5 stripped of the temperament extension, version 5
    assert_eq!(
        changed(|c, m| {
            m["map_version"] = json!(5);
            retain(&mut m["traits"], |t| {
                !of_temperament(t["id"].as_str().expect("id"))
            });
            retain(&mut m["loci"], |l| {
                !of_temperament(l["id"].as_str().expect("id"))
            });
            retain(&mut c["traits"], |r| {
                !of_temperament(r["trait"].as_str().expect("id"))
            });
            retain(&mut c["sex_differences"], |r| {
                !of_temperament(r["trait"].as_str().expect("id"))
            });
            c["values"]
                .as_object_mut()
                .expect("values")
                .retain(|key, _| !of_temperament(key));
        })
        .err(),
        Some(DataError::CalibrationMissing)
    );
    // map_version 5 with both extensions
    assert_eq!(
        changed(|_, m| m["map_version"] = json!(5)).err(),
        Some(DataError::MapVersion)
    );
    // hair locus off its place
    assert_eq!(
        changed(|_, m| locus_mut(m, "core:hair_thickness_02")["slot"] = json!(20)).err(),
        Some(DataError::LocusPosition)
    );
    // a second load locus (a third locus on core:hair_thickness at segment 2 slot 19)
    assert_eq!(
        changed(|_, m| {
            let mut extra = locus_mut(m, "core:hair_thickness_01").clone();
            extra["id"] = json!("core:hair_thickness_03");
            extra["segment"] = json!(2);
            extra["slot"] = json!(19);
            let loci = m["loci"].as_array_mut().expect("loci");
            let at = loci
                .iter()
                .position(|l| l["chromosome"] == "autosome" && l["segment"] == 2 && l["slot"] == 21)
                .expect("core:pigment_13");
            loci.insert(at, extra);
        })
        .err(),
        Some(DataError::LocusLoad)
    );
    // an include of an extension trait
    assert_eq!(
        changed(|_, m| {
            let traits = m["traits"].as_array_mut().expect("traits");
            let bald = traits
                .iter_mut()
                .find(|t| t["id"] == "core:bald")
                .expect("bald");
            bald["includes"]
                .as_array_mut()
                .expect("includes")
                .push(json!({"trait": "core:hair_thickness", "weight": 0.5}));
        })
        .err(),
        Some(DataError::TraitInclude)
    );
    // a dryness value missing
    assert_eq!(
        changed(|c, _| {
            c["values"]
                .as_object_mut()
                .expect("values")
                .remove("core:body_dryness_tempo");
        })
        .err(),
        Some(DataError::CalibrationMissing)
    );
    // all seven dryness values missing, the hair loci there
    assert_eq!(
        changed(|c, _| {
            let values = c["values"].as_object_mut().expect("values");
            values.retain(|key, _| !key.starts_with("core:body_dryness_"));
        })
        .err(),
        Some(DataError::CalibrationMissing)
    );
    // a phenotype row missing
    assert_eq!(
        changed(|c, _| retain(&mut c["traits"], |r| r["trait"] != "core:hair_dryness")).err(),
        Some(DataError::CalibrationMissing)
    );
    // one calibration value too many
    assert_eq!(
        changed(|c, _| {
            let tempo = c["values"]["core:body_dryness_tempo"].clone();
            c["values"]["core:body_dryness_extra"] = tempo;
        })
        .err(),
        Some(DataError::CalibrationUnknown)
    );
}

/// A hair locus that loads on another trait as well, or by a threshold, is rejected: the
/// stripped hash drops the locus whole.
#[test]
fn a_hair_locus_loads_only_its_own_trait() {
    assert_eq!(
        changed(|_, m| {
            locus_mut(m, "core:hair_thickness_01")["action"]["additive"]["loads"]
                .as_array_mut()
                .expect("loads")
                .push(json!({"trait": "core:curl", "weight": 0.1}));
        })
        .err(),
        Some(DataError::CalibrationUnknown)
    );
    assert_eq!(
        changed(|_, m| {
            let locus = locus_mut(m, "core:hair_thickness_01");
            let weights = locus["action"]["additive"].clone();
            locus["action"] = json!({"threshold": weights});
        })
        .err(),
        Some(DataError::CalibrationUnknown)
    );
}

/// The seven values of the dryness (ADR-0034 section 3) as the core reads them; none on map 5.
#[test]
fn the_hair_model_reads_the_seven_dryness_values() {
    assert_eq!(
        map6_definitions_data().genetics().hair,
        Some(HairDryness {
            offset: -0.942,
            curl: 0.75,
            slope: 1.30,
            age: 2.0,
            male: -0.35,
            center: 55.0,
            tempo: 10.0,
        })
    );
    let (calibration, genome_map) = common::map5_files();
    let map5 = Data::new(common::definitions(calibration, genome_map).expect("definitions"))
        .expect("map 5");
    assert_eq!(map5.genetics().hair, None);

    let value =
        |key: &str, field: &str, set: Value| changed(|c, _| c["values"][key][field] = set).err();
    assert_eq!(
        value("core:body_dryness_offset", "value", json!(20.5)),
        Some(DataError::CalibrationRange)
    );
    assert_eq!(
        value("core:body_dryness_male", "value", json!(-20.5)),
        Some(DataError::CalibrationRange)
    );
    assert_eq!(
        value("core:body_dryness_curl", "unit", json!("year")),
        Some(DataError::CalibrationUnit)
    );
    assert_eq!(
        value("core:body_dryness_center", "unit", json!("logit")),
        Some(DataError::CalibrationUnit)
    );
    assert_eq!(
        value("core:body_dryness_tempo", "value", json!(0.0)),
        Some(DataError::CalibrationRange)
    );
    assert_eq!(
        value("core:body_dryness_center", "value", json!(131.0)),
        Some(DataError::CalibrationRange)
    );
    assert_eq!(
        value("core:body_dryness_offset", "value", json!(-20.0)),
        None
    );
    assert_eq!(value("core:body_dryness_slope", "value", json!(0.0)), None);
}

/// `G_dry = -G_androgen` bit for bit (ADR-0034 section 1): the include with weight -1 negates
/// every term, `mean_ref` is androgen's negated and `sigma_ref` androgen's. On pool genomes,
/// on genomes with every androgen locus at dose 0, at 1 and at 2, and on three mixed ones.
#[test]
fn hair_dryness_is_minus_androgen_bit_for_bit() {
    let data = map6_definitions_data();
    let (androgen, dryness) = (id("core:androgen"), id("core:hair_dryness"));
    let check = |genome: &Genome| {
        let a = genome.genetic_value(&androgen, &data).expect("androgen");
        let d = genome.genetic_value(&dryness, &data).expect("dryness");
        assert_eq!(d.to_bits(), (-a).to_bits(), "{a} {d}");
    };
    let pool = Pool::from_bytes(common::REPOSITORY_POOL).expect("the pool");
    let mut checked = 0;
    for member in pool
        .households
        .iter()
        .flat_map(|h| h.members.iter())
        .take(200)
    {
        check(&member.genome);
        checked += 1;
    }
    assert_eq!(checked, 200);
    let androgen_loci: Vec<DataId> = (data.genome_map().loci.iter())
        .filter(|locus| {
            locus
                .action
                .weights()
                .is_some_and(|w| w.loads.iter().any(|load| load.target == androgen))
        })
        .map(|locus| locus.id.clone())
        .collect();
    assert!(!androgen_loci.is_empty());
    for dose in 0..3u8 {
        let alleles: Vec<LocusAlleles> = (androgen_loci.iter())
            .map(|locus| LocusAlleles {
                locus: locus.clone(),
                copies: vec![u8::from(dose >= 1), u8::from(dose == 2)],
            })
            .collect();
        for sex in [Sex::Female, Sex::Male] {
            check(&Genome::with_alleles(sex, &alleles, &data).expect("fits"));
        }
    }
    for pattern in 0..3u32 {
        let alleles: Vec<LocusAlleles> = (androgen_loci.iter().enumerate())
            .map(|(n, locus)| {
                let dose = (pattern + u32::try_from(n).expect("few")) % 3;
                LocusAlleles {
                    locus: locus.clone(),
                    copies: vec![u8::from(dose >= 1), u8::from(dose == 2)],
                }
            })
            .collect();
        for sex in [Sex::Female, Sex::Male] {
            check(&Genome::with_alleles(sex, &alleles, &data).expect("fits"));
        }
    }
}

/// The trait of `genome_map` with this id.
fn trait_mut<'a>(genome_map: &'a mut Value, trait_id: &str) -> &'a mut Value {
    genome_map["traits"]
        .as_array_mut()
        .expect("traits")
        .iter_mut()
        .find(|entry| entry["id"] == trait_id)
        .expect("the trait")
}

/// The loader enforces `G_dry = -G_androgen` (ADR-0034 section 1): the dryness's only include
/// is `core:androgen` with weight -1 (`data.trait_include`), its `sigma_ref` is androgen's and
/// its `mean_ref` androgen's negated, bit for bit (`data.trait_sigma`).
#[test]
fn the_dryness_must_be_the_negated_androgen() {
    let dryness =
        |change: fn(&mut Value)| changed(|_, m| change(trait_mut(m, "core:hair_dryness"))).err();
    assert_eq!(
        dryness(|t| t["includes"][0]["weight"] = json!(-0.5)),
        Some(DataError::TraitInclude)
    );
    assert_eq!(
        dryness(|t| {
            let includes = t["includes"].as_array_mut().expect("includes");
            includes.push(json!({"trait": "core:curl", "weight": 0.5}));
        }),
        Some(DataError::TraitInclude)
    );
    assert_eq!(
        dryness(|t| t["includes"][0]["trait"] = json!("core:estrogen")),
        Some(DataError::TraitInclude)
    );
    assert_eq!(
        dryness(|t| t["sigma_ref"] = json!(2.0)),
        Some(DataError::TraitSigma)
    );
    assert_eq!(
        dryness(|t| t["mean_ref"] = json!(0.011_102_035_190_670_132)),
        Some(DataError::TraitSigma)
    );
}

/// Map 5 changed by `change`.
fn changed5(change: impl FnOnce(&mut Value, &mut Value)) -> Result<Data, DataError> {
    let (mut calibration, mut genome_map) = common::map5_files();
    change(&mut calibration, &mut genome_map);
    Data::new(common::definitions(calibration, genome_map).expect("definitions"))
}

/// A sex row for `trait_id` (shift 0, variance ratio 1), inserted in the order of the rows.
fn add_sex_row(calibration: &mut Value, trait_id: &str) {
    let rows = calibration["sex_differences"].as_array_mut().expect("rows");
    let mut row = rows[0].clone();
    row["trait"] = json!(trait_id);
    row["shift"]["value"] = json!(0.0);
    row["variance_ratio"]["value"] = json!(1.0);
    rows.push(row);
    rows.sort_by(|a, b| a["trait"].as_str().cmp(&b["trait"].as_str()));
}

/// Parts of the hair extension without its loci, and an extra sex row on map 6.
#[test]
fn a_part_of_the_hair_extension_alone_is_rejected() {
    let (patched, _) = common::map6_files();
    let (_, map6) = common::map6_files();
    let traits = map6["traits"].as_array().expect("traits").clone();
    let hair_trait = |trait_id: &str| {
        let found = traits.iter().find(|t| t["id"] == trait_id);
        found.expect("a hair trait").clone()
    };
    let with_traits = |m: &mut Value, ids: &[&str]| {
        let list = m["traits"].as_array_mut().expect("traits");
        list.extend(ids.iter().map(|id| hair_trait(id)));
        list.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    };
    // Only the dryness trait (it includes androgen, so it is used): incomplete.
    assert_eq!(
        changed5(|_, m| with_traits(m, &["core:hair_dryness"])).err(),
        Some(DataError::CalibrationMissing)
    );
    // Both hair traits: the thickness without loci is unused before the extension rules.
    assert_eq!(
        changed5(|_, m| with_traits(m, &["core:hair_dryness", "core:hair_thickness"])).err(),
        Some(DataError::TraitUnused)
    );
    // Only the seven values: incomplete.
    assert_eq!(
        changed5(|c, _| {
            let values = c["values"].as_object_mut().expect("values");
            for (key, entry) in patched["values"].as_object().expect("values") {
                if key.starts_with("core:body_dryness_") {
                    values.insert(key.clone(), entry.clone());
                }
            }
        })
        .err(),
        Some(DataError::CalibrationMissing)
    );
    // Only a sex row: a sex row without a phenotype row is unknown (ADR-0014 section 1).
    assert_eq!(
        changed5(|c, _| add_sex_row(c, "core:hair_dryness")).err(),
        Some(DataError::CalibrationUnknown)
    );
    // Map 6 with a sex row for a hair trait, which has none: unknown.
    assert_eq!(
        changed(|c, _| add_sex_row(c, "core:hair_thickness")).err(),
        Some(DataError::CalibrationUnknown)
    );
}

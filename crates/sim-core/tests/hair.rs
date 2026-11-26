//! Hair strand thickness and dryness on the body contract (ADR-0034 section 3), scalp hair
//! only: `hair_strand_thickness = L(z_thickness)` and
//! `hair_dryness = L(offset + curl · z_curl + slope · z_dry + age · s_age + male · m)` with
//! `s_age = L((b − center) / tempo)`, `b` the biological age; without the hair group the
//! defaults 0.5 and 0.3 directly.
//!
//! Every expected value comes from `tools/body_reference.py` (rows `body_hair_anchor`,
//! `body_hair_row`, `body_hair_defaults` and `body_hair_group`), whose `exp` is the reference
//! of `sim-math`; the anchors also against the three decimals of the decision.

mod common;

use serde_json::{Value, json};
use sim_core::body_contract::{
    BodyData, BodyInputs, BodyParamKey, BodyTraits, body_params, body_traits,
};
use sim_core::data::{Data, DataError};
use sim_core::data_id::DataId;
use sim_core::development::Lived;
use sim_core::genome::Sex;
use sim_core::phenotype::{Environment, phenotype};
use sim_core::rng::{Draws, WorldSeed};
use sim_core::time::{SimDuration, YEAR, year_start};

/// A person of the calendar age `years` and the biological age `b`, with the phenotypes `z`.
fn person(sex: Sex, years: i64, b: f64, z: BodyTraits) -> BodyInputs<'static> {
    BodyInputs {
        sex,
        age: YEAR * years,
        bio_age: b,
        born: year_start(0),
        nutrition: &[],
        z,
        mc1r: 0,
        herc2: 0,
    }
}

/// The phenotypes with the given thickness, curl and dryness, every other 0.
fn traits(thickness: f64, curl: f64, dryness: f64) -> BodyTraits {
    BodyTraits {
        hair_thickness: thickness,
        curl,
        hair_dryness: dryness,
        ..BodyTraits::default()
    }
}

/// `(hair_strand_thickness, hair_dryness)` of `inputs` on `data`.
fn hair(data: &Data, inputs: &BodyInputs<'_>) -> (f64, f64) {
    let body = BodyData::new(data).expect("body data");
    let params = body_params(&body, inputs).params;
    assert_eq!(
        params.get(BodyParamKey::HairStrandThickness).to_bits(),
        params.hair_strand_thickness().to_bits()
    );
    (params.hair_strand_thickness(), params.hair_dryness())
}

fn sex_of(man: bool) -> Sex {
    if man { Sex::Male } else { Sex::Female }
}

/// Map 5 (the repository's map 6 without the hair extension) with its pool.
fn map5() -> Data {
    common::map5_data()
}

/// The anchors of the decision (`body_hair_anchor`): the thickness at `z = 0, 1, 2, −1, −2`
/// and the dryness of a woman (and a man) at 25 and 70 years, each as the reference prints it
/// and within half a unit of the decision's third decimal.
#[test]
fn the_anchors_are_the_references() {
    let data = common::map6_data();
    let thickness = [
        (0.0, 0.5, 0.500),
        (1.0, 0.7310585786300049, 0.731),
        (2.0, 0.8807970779778823, 0.881),
        (-1.0, 0.2689414213699951, 0.269),
        (-2.0, 0.11920292202211755, 0.119),
    ];
    for (z, reference, decision) in thickness {
        let (value, _) = hair(&data, &person(Sex::Female, 30, 30.0, traits(z, 0.0, 0.0)));
        assert_eq!(
            value.to_bits(),
            f64::to_bits(reference),
            "thickness z {z}: {value}"
        );
        assert!(
            (value - decision).abs() < 0.0005,
            "thickness z {z}: {value}"
        );
    }
    // (z_curl, z_dry, b, man, the reference, the decision)
    let dryness = [
        (0.0, 0.0, 25, false, 0.3000314183559118, 0.300),
        (0.0, 0.0, 25, true, 0.23198291475696398, 0.232),
        (1.0, 0.0, 25, false, 0.475732019915115, 0.476),
        (0.0, 1.0, 25, false, 0.6113170478767082, 0.611),
        (1.5, -1.0, 25, false, 0.26460915804726803, 0.265),
        (-1.0, 1.0, 25, false, 0.4262547638416667, 0.426),
        (0.0, 0.0, 70, false, 0.6666670604059597, 0.667),
        (0.0, 0.0, 70, true, 0.5849552377517374, 0.585),
    ];
    for (z_curl, z_dry, years, man, reference, decision) in dryness {
        let b = f64::from(i32::try_from(years).expect("small"));
        let inputs = person(sex_of(man), years, b, traits(0.0, z_curl, z_dry));
        let (_, value) = hair(&data, &inputs);
        let case = format!("dryness z_curl {z_curl} z_dry {z_dry} b {b} man {man}: {value}");
        assert_eq!(value.to_bits(), f64::to_bits(reference), "{case}");
        assert!((value - decision).abs() < 0.0005, "{case}");
    }
}

/// The eight rows of `body_hair_row` bit for bit: `(z_thickness, z_curl, z_dry, b, man)`,
/// then the bits of the thickness and the dryness. The calendar age is 30 years throughout;
/// the dryness reads `b`.
#[test]
fn the_rows_are_the_references_bit_for_bit() {
    let data = common::map6_data();
    let rows: [(f64, f64, f64, f64, bool, u64, u64); 8] = [
        (
            0.0,
            0.0,
            0.0,
            25.0,
            false,
            0x3fe0000000000000,
            0x3fd333b6fa671cd7,
        ),
        (
            0.0,
            0.0,
            0.0,
            25.0,
            true,
            0x3fe0000000000000,
            0x3fcdb19dbc0e5344,
        ),
        (
            1.0,
            1.5,
            -1.0,
            25.0,
            false,
            0x3fe764d4f5d5a2bd,
            0x3fd0ef5b40023f44,
        ),
        (
            -1.0,
            -1.0,
            1.0,
            70.0,
            true,
            0x3fd136561454ba86,
            0x3fe6b48c5863da72,
        ),
        (
            0.37,
            0.8125,
            -0.4375,
            55.0,
            false,
            0x3fe2ed3b61296b65,
            0x3fe0c9d2081931b5,
        ),
        (
            -2.5,
            -2.25,
            3.0,
            0.0,
            true,
            0x3fb36b7112534847,
            0x3fe6f0052e504a96,
        ),
        (
            2.75,
            3.5,
            1.125,
            83.5,
            false,
            0x3fee13c52952723c,
            0x3fefcb1b06101eb7,
        ),
        (
            -0.125,
            0.0625,
            -2.0,
            12.25,
            true,
            0x3fde00aa6681fcf3,
            0x3f9605ab3d694bc5,
        ),
    ];
    for (z_thickness, z_curl, z_dry, b, man, thickness, dryness) in rows {
        let inputs = person(sex_of(man), 30, b, traits(z_thickness, z_curl, z_dry));
        let (ours_thickness, ours_dryness) = hair(&data, &inputs);
        let case = format!("z_thickness {z_thickness} z_curl {z_curl} z_dry {z_dry} b {b}");
        assert_eq!(
            ours_thickness.to_bits(),
            thickness,
            "{case}: {ours_thickness}"
        );
        assert_eq!(ours_dryness.to_bits(), dryness, "{case}: {ours_dryness}");
    }
}

/// The dryness reads the biological age `b`, not the calendar age: 25 calendar years with
/// `b = 70` is the anchor of 70, and 70 calendar years with `b = 25` the anchor of 25.
#[test]
fn the_dryness_reads_the_biological_age() {
    let data = common::map6_data();
    let (_, old) = hair(&data, &person(Sex::Female, 25, 70.0, BodyTraits::default()));
    assert_eq!(old.to_bits(), f64::to_bits(0.6666670604059597), "{old}");
    let (_, young) = hair(&data, &person(Sex::Female, 70, 25.0, BodyTraits::default()));
    assert_eq!(young.to_bits(), f64::to_bits(0.3000314183559118), "{young}");
    let (_, man) = hair(&data, &person(Sex::Male, 25, 70.0, BodyTraits::default()));
    assert_eq!(man.to_bits(), f64::to_bits(0.5849552377517374), "{man}");
}

/// Without the hair group (map 5, the body fixture of map 4) the two keys are the defaults
/// 0.5 and 0.3 directly (`body_hair_defaults`), whatever the phenotypes, sex and age: not
/// `z = 0`, which would give a dryness by age and sex.
#[test]
fn without_the_hair_group_the_defaults() {
    let (calibration, genome_map) = common::files_from(
        include_str!("../../../tests/golden/data_body/calibration/genetics.json"),
        include_str!("../../../tests/golden/data_body/genetics/genome_map.json"),
    );
    let map4 = Data::new(common::definitions(calibration, genome_map).expect("definitions"))
        .expect("map 4");
    for data in [map5(), map4] {
        assert_eq!(data.genetics().hair, None);
        let cases = [
            person(Sex::Female, 25, 25.0, BodyTraits::default()),
            person(Sex::Male, 70, 70.0, BodyTraits::default()),
            person(Sex::Female, 30, 83.5, traits(2.75, 3.5, 1.125)),
            person(Sex::Male, 12, 12.25, traits(-0.125, 0.0625, -2.0)),
        ];
        for inputs in cases {
            let (thickness, dryness) = hair(&data, &inputs);
            assert_eq!(thickness.to_bits(), 0x3fe0000000000000, "{thickness}");
            assert_eq!(dryness.to_bits(), 0x3fd3333333333333, "{dryness}");
            assert_eq!(
                BodyParamKey::HairDryness.spec().default.to_bits(),
                0x3fd3333333333333
            );
        }
    }
}

/// The hair group of the body data (`body_hair_group`): present on map 6, absent on map 5,
/// for `BodyData::new` and `BodyData::census`. A hair trait whose weights depend on the age
/// is rejected by both; half a group (a row or a value missing) never loads, so no body data
/// see it. The mapping still reads the 35 traits of `body_traits`.
#[test]
fn the_hair_group_is_whole_or_absent() {
    let map6 = common::map6_data();
    assert!(BodyData::new(&map6).is_ok());
    assert!(BodyData::census(&map6).is_ok());
    let map5 = map5();
    assert!(BodyData::new(&map5).is_ok());
    assert!(BodyData::census(&map5).is_ok());
    assert_eq!(body_traits().len(), 35);

    let changed = |change: &dyn Fn(&mut Value)| {
        let (mut calibration, genome_map) = common::map6_files();
        change(&mut calibration);
        Data::new(common::definitions(calibration, genome_map).expect("definitions"))
    };
    for name in ["core:hair_thickness", "core:hair_dryness"] {
        for weight in ["h2_child", "c2_child", "h2_adult", "c2_adult"] {
            let aged = changed(&|calibration| {
                let rows = calibration["traits"].as_array_mut().expect("rows");
                let row = rows.iter_mut().find(|row| row["trait"] == json!(name));
                let row = row.expect("the row");
                let other = row[weight]["value"].as_f64().expect("weight");
                row[weight]["value"] = json!(other + 0.05);
            })
            .expect("loads");
            assert_eq!(
                BodyData::new(&aged).err(),
                Some(DataError::BodyTraits),
                "{name} {weight}"
            );
            assert_eq!(
                BodyData::census(&aged).err(),
                Some(DataError::BodyTraits),
                "{name} {weight}"
            );
        }
        let without_row = changed(&|calibration| {
            let rows = calibration["traits"].as_array_mut().expect("rows");
            rows.retain(|row| row["trait"] != json!(name));
        });
        assert_eq!(
            without_row.err(),
            Some(DataError::CalibrationMissing),
            "{name}"
        );
    }
    let without_tempo = changed(&|calibration| {
        let values = calibration["values"].as_object_mut().expect("values");
        values.remove("core:body_dryness_tempo");
    });
    assert_eq!(without_tempo.err(), Some(DataError::CalibrationMissing));
}

/// A person's hair phenotypes on map 6 are those of the two hair traits at the age 0, as for
/// every trait the body reads (pool persons of the extended pool); on map 5 they stay 0 and
/// are not read.
#[test]
fn a_persons_hair_traits_are_its_phenotypes() {
    let data = common::map6_data();
    let body = BodyData::new(&data).expect("body data");
    let individual = Draws::new(WorldSeed::new(0x5eed));
    let shared = Draws::new(WorldSeed::new(0x5eee));
    let environment = Environment {
        individual: &individual,
        shared: &shared,
        neutral: false,
        lived: Lived::NONE,
    };
    let id = |text: &str| DataId::new(text).expect("id");
    let pool = data.pool().expect("a pool").pool();
    let members: Vec<_> = (pool.households.iter())
        .flat_map(|h| h.members.iter())
        .take(20)
        .collect();
    assert_eq!(members.len(), 20);
    for member in &members {
        let (genome, own) = (&member.genome, member.lineage_key);
        let z = BodyTraits::of_person(&body, environment, genome, own, None);
        for (ours, name) in [
            (z.hair_thickness, "core:hair_thickness"),
            (z.hair_dryness, "core:hair_dryness"),
            (z.curl, "core:curl"),
        ] {
            let zero = SimDuration::ZERO;
            let expected = phenotype(environment, &data, genome, own, None, &id(name), zero)
                .expect("a phenotype");
            assert_eq!(ours.to_bits(), expected.to_bits(), "{name}");
        }
    }
    let map5 = map5();
    let body5 = BodyData::new(&map5).expect("body data");
    let pool5 = map5.pool().expect("a pool").pool();
    let member = &pool5.households[0].members[0];
    let z = BodyTraits::of_person(
        &body5,
        environment,
        &member.genome,
        member.lineage_key,
        None,
    );
    assert_eq!((z.hair_thickness, z.hair_dryness), (0.0, 0.0));
}

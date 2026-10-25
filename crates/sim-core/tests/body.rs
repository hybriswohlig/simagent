//! The mapping onto the body contract (genetics specification 10, ADR-0015). The expected
//! values write the formulas of section 10 out with `exp` from `sim-math` and the numbers
//! of the calibration; a few are checked against the round numbers the specification
//! prints.

mod common;

use serde_json::{Value, json};
use sim_core::body_contract::{
    BodyData, BodyInputs, BodyParamKey, BodyTraits, CLAMPED_KEYS, body_params,
};
use sim_core::data::{Data, DataError};
use sim_core::finite::FiniteF64;
use sim_core::genome::Sex;
use sim_core::growth::{IndividualCurve, height_loss, mean_height, nutrition_loss};
use sim_core::params::NutritionYear;
use sim_core::time::{DAY, SimDuration, SimTime, YEAR, year_start};

fn files() -> (Value, Value) {
    common::files_from(
        include_str!("../../../tests/golden/data_body/calibration/genetics.json"),
        include_str!("../../../tests/golden/data_body/genetics/genome_map.json"),
    )
}

fn fixture() -> Data {
    let (calibration, genome_map) = files();
    Data::new(common::definitions(calibration, genome_map).expect("definitions")).expect("checked")
}

fn l(u: f64) -> f64 {
    1.0 / (1.0 + sim_math::exp(-u))
}

/// The smoothstep `S(0, end, t)` of 10.1.
fn s(end: f64, t: f64) -> f64 {
    let u = t / end;
    if u <= 0.0 {
        0.0
    } else if u >= 1.0 {
        1.0
    } else {
        3.0 * (u * u) - 2.0 * ((u * u) * u)
    }
}

/// A person aged `years` (calendar and biological) with all phenotypes `z`.
fn person(sex: Sex, years: i64, z: BodyTraits) -> BodyInputs<'static> {
    BodyInputs {
        sex,
        age: YEAR * years,
        bio_age: f64::from(i32::try_from(years).expect("small")),
        born: year_start(0),
        nutrition: &[],
        z,
        mc1r: 0,
        herc2: 0,
    }
}

fn value(data: &Data, inputs: &BodyInputs<'_>, key: BodyParamKey) -> f64 {
    let body = BodyData::new(data).expect("body data");
    let evaluation = body_params(&body, inputs);
    let params = evaluation.params;
    let all = serde_json::to_value(params).expect("json");
    all[key.as_str()].as_f64().expect("number")
}

/// ADR-0015 section 2: the base map lacks the traits until step 11; the fixture has them;
/// a missing trait, weights that depend on age, a missing or non-recessive pigment locus
/// are rejected.
#[test]
fn body_data_rejects() {
    assert_eq!(
        BodyData::new(&common::data()).err(),
        Some(DataError::BodyTraits)
    );
    let data = fixture();
    assert!(BodyData::new(&data).is_ok());
    let rejected = |change: fn(&mut Value, &mut Value)| {
        let (mut c, mut g) = files();
        change(&mut c, &mut g);
        let data = Data::new(common::definitions(c, g).expect("definitions")).expect("checked");
        BodyData::new(&data).err()
    };
    let cases: [fn(&mut Value, &mut Value); 4] = [
        // core:androgen loses its phenotype row (the rows are sorted, it is the first).
        |c, _| {
            let rows = c["traits"].as_array_mut().expect("rows");
            rows.retain(|row| row["trait"] != json!("core:androgen"));
        },
        // Weights that depend on age.
        |c, _| {
            for row in c["traits"].as_array_mut().expect("rows") {
                if row["trait"] == json!("core:grey") {
                    row["h2_child"]["value"] = json!(0.5);
                }
            }
        },
        // MC1R as a dominant locus.
        |_, g| {
            for locus in g["loci"].as_array_mut().expect("loci") {
                if locus["id"] == json!("core:mc1r") {
                    locus["action"] = json!("dominant");
                }
            }
        },
        // No HERC2.
        |_, g| {
            let loci = g["loci"].as_array_mut().expect("loci");
            loci.retain(|locus| locus["id"] != json!("core:herc2"));
        },
    ];
    for change in cases {
        assert_eq!(rejected(change), Some(DataError::BodyTraits));
    }
    assert_eq!(sim_core::body_contract::body_traits().len(), 35);
}

/// ADR-0005 section 1: only `z` and `cm` are clamped, and those are exactly the parameters
/// of `contracts/body.json` with these units.
#[test]
fn clamped_units_are_z_and_cm() {
    let contract: Value =
        serde_json::from_str(include_str!("../../../contracts/body.json")).expect("json");
    let mut expected: Vec<&str> = contract["parameters"]
        .as_array()
        .expect("parameters")
        .iter()
        .filter(|p| p["unit"] == json!("z") || p["unit"] == json!("cm"))
        .map(|p| p["key"].as_str().expect("key"))
        .collect();
    let mut actual: Vec<&str> = CLAMPED_KEYS.iter().map(|k| k.as_str()).collect();
    expected.sort_unstable();
    actual.sort_unstable();
    assert_eq!(actual, expected);
}

/// 10.2: form ages are `min(b, 130)`; height is the individual curve at `min(age, 25)`
/// minus the nutrition loss minus the loss in old age.
#[test]
fn form_ages_and_height() {
    let data = fixture();
    let g = data.genetics();
    for (b, form) in [(0, 0.0), (40, 40.0), (130, 130.0)] {
        let inputs = person(Sex::Female, b, BodyTraits::default());
        assert_eq!(value(&data, &inputs, BodyParamKey::BodyFormAge), form);
        assert_eq!(value(&data, &inputs, BodyParamKey::FaceFormAge), form);
    }
    let mut old = person(Sex::Female, 30, BodyTraits::default());
    old.bio_age = 140.0;
    assert_eq!(value(&data, &old, BodyParamKey::BodyFormAge), 130.0);
    // Height of a woman at 70 (bio_age 70) with z_h = 1, z_p = −0.5 and a shock in her
    // second year of life.
    let z = BodyTraits {
        height: 1.0,
        puberty: -0.5,
        ..BodyTraits::default()
    };
    let shock = [NutritionYear {
        year: 1,
        index: FiniteF64::try_new(0.7).expect("finite"),
    }];
    let inputs = BodyInputs {
        nutrition: &shock,
        ..person(Sex::Female, 70, z)
    };
    let curve = IndividualCurve::new(g.growth(Sex::Female), 1.0, -0.5);
    let x = YEAR * 25;
    let expected = (curve.height(x)
        - nutrition_loss(&shock, &g.nutrition, year_start(0), x, curve.theta))
        - height_loss(&g.height_loss, Sex::Female, 70.0);
    let at = value(&data, &inputs, BodyParamKey::HeightCm);
    assert_eq!(at.to_bits(), expected.to_bits());
    // 1.5 cm from the shock (5.4), 5 cm from age (5.3).
    let plain = curve.height(x);
    assert!((plain - at - 6.5).abs() < 1e-9, "{}", plain - at);
    // A child of 10 reads the curve at 10, not 25.
    let child = person(Sex::Male, 10, BodyTraits::default());
    let mean = mean_height(&g.growth(Sex::Male), YEAR * 10, g.growth(Sex::Male).theta);
    assert_eq!(value(&data, &child, BodyParamKey::HeightCm), mean);
}

/// 10.3: each z parameter is the phenotype of its own trait, for both sexes.
#[test]
fn z_parameters_are_their_phenotypes() {
    let data = fixture();
    let z = BodyTraits {
        leg_ratio: 0.1,
        head_size: -0.2,
        body_fat: 0.3,
        fat_distribution: -0.4,
        muscle: 0.5,
        shoulder_width: -0.6,
        chest_depth: 0.7,
        hip_width: -0.8,
        face: [
            0.9, -1.0, 1.1, -1.2, 1.3, -1.4, 1.5, -1.6, 1.7, -1.8, 1.9, -2.0,
        ],
        ..BodyTraits::default()
    };
    for sex in [Sex::Female, Sex::Male] {
        let inputs = person(sex, 33, z);
        let at = |key| value(&data, &inputs, key);
        assert_eq!(at(BodyParamKey::LegRatioZ), 0.1);
        assert_eq!(at(BodyParamKey::HeadSizeZ), -0.2);
        assert_eq!(at(BodyParamKey::BodyFatZ), 0.3);
        assert_eq!(at(BodyParamKey::FatDistributionZ), -0.4);
        assert_eq!(at(BodyParamKey::MuscleZ), 0.5);
        assert_eq!(at(BodyParamKey::ShoulderWidthZ), -0.6);
        assert_eq!(at(BodyParamKey::ChestDepthZ), 0.7);
        assert_eq!(at(BodyParamKey::HipWidthZ), -0.8);
        assert_eq!(at(BodyParamKey::FaceAxis01), 0.9);
        assert_eq!(at(BodyParamKey::FaceAxis06), -1.4);
        assert_eq!(at(BodyParamKey::FaceAxis12), -2.0);
    }
    // Beyond ±4 a z parameter is clamped and noted.
    let far = BodyTraits {
        muscle: 4.5,
        face: [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -4.25],
        ..BodyTraits::default()
    };
    let body = BodyData::new(&data).expect("body");
    let evaluation = body_params(&body, &person(Sex::Male, 33, far));
    assert!(evaluation.is_clamped(BodyParamKey::MuscleZ));
    assert!(evaluation.is_clamped(BodyParamKey::FaceAxis12));
    assert!(!evaluation.is_clamped(BodyParamKey::FaceAxis11));
    assert_eq!(evaluation.params.muscle_z(), 4.0);
    assert_eq!(evaluation.params.face_axis_12(), -4.0);
    // Exactly at the ends is inside: clamped means truly outside (ADR-0005).
    let ends = BodyTraits {
        muscle: 4.0,
        leg_ratio: -4.0,
        ..BodyTraits::default()
    };
    let evaluation = body_params(&body, &person(Sex::Female, 33, ends));
    assert!(evaluation.clamped.is_empty(), "{:?}", evaluation.clamped);
}

/// 10.4: pigment from the logits, darkening in childhood, MC1R and HERC2 at two copies.
#[test]
fn pigment_follows_the_specification() {
    let data = fixture();
    let m = data.genetics().body;
    let z = BodyTraits {
        pigment: 0.4,
        hair_darkness: -0.3,
        ..BodyTraits::default()
    };
    for (years, b) in [(1, 1.0), (30, 30.0)] {
        let inputs = person(Sex::Female, years, z);
        let at = |key| value(&data, &inputs, key);
        let eta_s = m.pigment_skin_offset + m.pigment_skin_slope * 0.4;
        let eta_h = (m.pigment_hair_slope * 0.4 + m.pigment_hair_residual * -0.3)
            - m.pigment_hair_darkening * (1.0 - s(m.pigment_hair_darkening_age, b));
        let eta_i = (m.pigment_iris_offset + m.pigment_iris_slope * 0.4)
            - m.pigment_iris_darkening * (1.0 - s(m.pigment_iris_darkening_age, b));
        assert_eq!(at(BodyParamKey::SkinEumelanin), l(eta_s) * 1.0);
        assert_eq!(at(BodyParamKey::HairEumelanin), l(eta_h) * 1.0);
        assert_eq!(at(BodyParamKey::IrisMelanin), l(eta_i) * 1.0);
        assert_eq!(
            at(BodyParamKey::SkinPheomelanin),
            l(m.pigment_skin_pheo_offset + m.pigment_skin_pheo_slope * 0.4)
        );
        assert_eq!(
            at(BodyParamKey::HairPheomelanin),
            l(m.pigment_hair_pheo_offset + m.pigment_hair_pheo_slope * 0.4)
        );
        // Two copies of MC1R and HERC2 (MC1R in hair since Grok's round 39, R1 c: × 0.08 and
        // pheomelanin 1).
        let red = BodyInputs {
            mc1r: 2,
            herc2: 2,
            ..inputs
        };
        let at = |key| value(&data, &red, key);
        assert_eq!(at(BodyParamKey::SkinEumelanin), l(eta_s) * 0.55);
        assert_eq!(at(BodyParamKey::HairEumelanin), l(eta_h) * 0.08);
        assert_eq!(at(BodyParamKey::IrisMelanin), l(eta_i) * 0.15);
        assert_eq!(at(BodyParamKey::SkinPheomelanin), 0.45);
        assert_eq!(at(BodyParamKey::HairPheomelanin), 1.0);
        // One copy changes the colours not at all.
        let carrier = BodyInputs {
            mc1r: 1,
            herc2: 1,
            ..inputs
        };
        assert_eq!(
            value(&data, &carrier, BodyParamKey::HairEumelanin),
            l(eta_h) * 1.0
        );
        assert_eq!(
            value(&data, &carrier, BodyParamKey::IrisMelanin),
            l(eta_i) * 1.0
        );
    }
    // The values the specification prints at z_pig = 0 for an adult: skin about 0.23,
    // hair 0.50, iris about 0.62.
    let adult = person(Sex::Male, 30, BodyTraits::default());
    let at = |key| value(&data, &adult, key);
    assert!((at(BodyParamKey::SkinEumelanin) - 0.23).abs() < 0.005);
    assert_eq!(at(BodyParamKey::HairEumelanin), 0.5);
    assert!((at(BodyParamKey::IrisMelanin) - 0.62).abs() < 0.005);
}

/// 10.5: the development weights by sex around θ_i.
#[test]
fn development_follows_the_specification() {
    let data = fixture();
    let g = data.genetics();
    let m = g.body;
    let z = BodyTraits {
        androgen: 0.5,
        estrogen: -0.25,
        breast: 0.75,
        ..BodyTraits::default()
    };
    for (sex, years) in [
        (Sex::Male, 12),
        (Sex::Male, 30),
        (Sex::Female, 11),
        (Sex::Female, 30),
    ] {
        let inputs = person(sex, years, z);
        let at = |key| value(&data, &inputs, key);
        let b = f64::from(i32::try_from(years).expect("small"));
        let theta = g.growth(sex).theta;
        let tempo = m.development_tempo;
        let s_a = l(((b - theta) - m.development_androgen_lag) / tempo);
        let s_e = l(((b - theta) + m.development_estrogen_lead) / tempo);
        let s_b = l(((b - theta) + m.development_breast_lead) / tempo);
        let androgen = s_a * l(m.development_androgen_offset + 0.5);
        let estrogen = s_e * l(m.development_estrogen_offset + -0.25);
        let (a, e, br) = match sex {
            Sex::Male => (androgen, m.development_estrogen_men * estrogen, 0.0),
            Sex::Female => (
                m.development_androgen_women * androgen,
                estrogen,
                s_b * l(0.75),
            ),
        };
        assert_eq!(
            at(BodyParamKey::AndrogenicDevelopment),
            a,
            "{sex:?} {years}"
        );
        assert_eq!(
            at(BodyParamKey::EstrogenicDevelopment),
            e,
            "{sex:?} {years}"
        );
        assert_eq!(at(BodyParamKey::BreastDevelopment), br, "{sex:?} {years}");
    }
    // Adult means the specification prints at z = 0: about 0.82 (men) and 0.12 (women).
    let man = person(Sex::Male, 40, BodyTraits::default());
    let woman = person(Sex::Female, 40, BodyTraits::default());
    let andro = |p: &BodyInputs<'_>| value(&data, p, BodyParamKey::AndrogenicDevelopment);
    assert!((andro(&man) - 0.82).abs() < 0.005, "{}", andro(&man));
    assert!((andro(&woman) - 0.12).abs() < 0.005, "{}", andro(&woman));
}

/// 10.6: hair, skin and the signs of age, for both sexes at a young and an old age; with
/// map 4 the eyebrows read `hair_density` and the beard's coverage reads `beard`.
#[test]
fn hair_skin_and_age_follow_the_specification() {
    let data = fixture();
    let g = data.genetics();
    let m = g.body;
    let z = BodyTraits {
        pigment: -0.5,
        androgen: 0.2,
        curl: 0.3,
        freckle: -0.4,
        beard: -0.7,
        body_hair: -0.9,
        bald: 1.0,
        hair_density: -1.1,
        grey: 1.2,
        skin_age: -1.3,
        ..BodyTraits::default()
    };
    for sex in [Sex::Male, Sex::Female] {
        for years in [16, 58] {
            let inputs = BodyInputs {
                mc1r: 1,
                ..person(sex, years, z)
            };
            let at = |key| value(&data, &inputs, key);
            let b = f64::from(i32::try_from(years).expect("small"));
            let theta = g.growth(sex).theta;
            let male = matches!(sex, Sex::Male);
            assert_eq!(
                at(BodyParamKey::HairCurl),
                l(m.curl_offset + m.curl_slope * 0.3)
            );
            let freckles = l(((m.freckle_offset + m.freckle_slope * -0.4)
                + m.freckle_pigment * -0.5)
                + m.mc1r_freckle * 1.0);
            assert_eq!(at(BodyParamKey::FreckleDensity), freckles);
            // Every copy of MC1R adds its term: none and two copies as well.
            for copies in [0_u8, 2] {
                let other = BodyInputs {
                    mc1r: copies,
                    ..inputs
                };
                let expected = l(((m.freckle_offset + m.freckle_slope * -0.4)
                    + m.freckle_pigment * -0.5)
                    + m.mc1r_freckle * f64::from(copies));
                assert_eq!(
                    value(&data, &other, BodyParamKey::FreckleDensity),
                    expected,
                    "{copies}"
                );
            }
            // Map 4 (ADR-0020 section 1, 10.7): the eyebrows read the hair density.
            assert_eq!(at(BodyParamKey::EyebrowDensity), l(-1.1));
            let beard = l(((b - theta) - m.beard_lag) / m.beard_tempo);
            let (density, coverage) = if male {
                (
                    beard * l((m.beard_offset + -0.7) + m.beard_androgen * 0.2),
                    // Map 4: the coverage reads the beard itself.
                    beard * l(-0.7),
                )
            } else {
                (0.0, 0.0)
            };
            assert_eq!(at(BodyParamKey::BeardDensity), density, "{sex:?} {years}");
            assert_eq!(at(BodyParamKey::BeardCoverage), coverage, "{sex:?} {years}");
            let hair = l(((b - theta) - m.body_hair_lag) / m.body_hair_tempo)
                * l(-0.9 + m.body_hair_androgen * 0.2);
            let body_hair = if male { hair } else { m.body_hair_women * hair };
            assert_eq!(at(BodyParamKey::BodyHair), body_hair, "{sex:?} {years}");
            let balding = if male {
                let center =
                    (m.bald_men_center - m.bald_men_slope * 1.0) - m.bald_men_androgen * 0.2;
                m.bald_men_stage * l((b - center) / m.bald_men_tempo)
            } else {
                let center = m.bald_women_center - m.bald_women_slope * 1.0;
                m.bald_women_stage * l((b - center) / m.bald_women_tempo)
            };
            assert_eq!(at(BodyParamKey::BaldingStage), balding, "{sex:?} {years}");
            let density = l(m.hair_density_offset + -1.1)
                * (1.0
                    - m.hair_density_loss * l((b - m.hair_density_center) / m.hair_density_tempo));
            assert_eq!(at(BodyParamKey::HairDensity), density);
            let grey = l((b - (m.grey_center - m.grey_slope * 1.2)) / m.grey_tempo);
            assert_eq!(at(BodyParamKey::HairGreyFraction), grey);
            let skin = -1.3;
            assert_eq!(
                at(BodyParamKey::FaceWrinkles),
                l(((b - m.wrinkles_center) + m.wrinkles_slope * skin) / m.wrinkles_tempo)
            );
            assert_eq!(
                at(BodyParamKey::SkinLaxity),
                l(((b - m.laxity_center) + m.laxity_slope * skin) / m.laxity_tempo)
            );
            assert_eq!(
                at(BodyParamKey::AgeSpots),
                l(
                    (((b - m.spots_center) + m.spots_skin * skin) - m.spots_pigment * -0.5)
                        / m.spots_tempo
                )
            );
            assert_eq!(
                at(BodyParamKey::PostureStoop),
                l(((b - m.stoop_center) + m.stoop_skin * skin) / m.stoop_tempo)
            );
        }
    }
    // The median man is bald to stage 7 · L(−1.4), about 1.38, at 50 (10.6).
    let man = person(Sex::Male, 50, BodyTraits::default());
    let stage = value(&data, &man, BodyParamKey::BaldingStage);
    assert!((stage - 1.385).abs() < 0.001, "{stage}");
}

/// ADR-0015 section 3: from the genome, `of_person` reads each phenotype and the doses of
/// the two pigment loci; the same person gives the same bits.
#[test]
fn inputs_of_a_person() {
    let data = fixture();
    let body = BodyData::new(&data).expect("body");
    let draws = sim_core::rng::Draws::new(sim_core::rng::WorldSeed::new(7));
    let own = sim_core::lineage::LineageKey::founder(
        &sim_core::data_id::DataId::new("test:body").expect("id"),
        0,
    );
    let alleles = [
        ("core:mc1r", [1, 1]),
        ("core:herc2", [0, 1]),
        ("core:muscle_locus", [1, 1]),
    ]
    .map(|(locus, copies)| sim_core::genome::LocusAlleles {
        locus: sim_core::data_id::DataId::new(locus).expect("id"),
        copies: copies.to_vec(),
    });
    let genome = sim_core::genome::Genome::with_alleles(Sex::Male, &alleles, &data).expect("fits");
    let born = SimTime::START - DAY;
    let inputs = BodyInputs::of_person(
        &body,
        sim_core::phenotype::Environment::one(&draws),
        &genome,
        own,
        None,
        born,
        YEAR * 20,
        &[],
        20.0,
    );
    assert_eq!(inputs.mc1r, 2);
    assert_eq!(inputs.herc2, 1);
    assert_eq!(inputs.sex, Sex::Male);
    let muscle = sim_core::phenotype::phenotype(
        sim_core::phenotype::Environment::one(&draws),
        &data,
        &genome,
        own,
        None,
        &sim_core::data_id::DataId::new("core:muscle").expect("id"),
        SimDuration::ZERO,
    )
    .expect("phenotype");
    assert_eq!(inputs.z.muscle.to_bits(), muscle.to_bits());
    let again = body_params(&body, &inputs);
    assert_eq!(again, body_params(&body, &inputs));
}

/// ADR-0005 section 1: a negative or not finite biological age breaks the invariant
/// `body_input_age`.
#[test]
#[should_panic(expected = "invariant body_input_age")]
fn a_negative_bio_age_is_an_invariant_break() {
    let data = fixture();
    let body = BodyData::new(&data).expect("body");
    let mut inputs = person(Sex::Female, 10, BodyTraits::default());
    inputs.bio_age = -0.5;
    let _ = body_params(&body, &inputs);
}

#[test]
#[should_panic(expected = "invariant body_input_age")]
fn a_bio_age_of_nan_is_an_invariant_break() {
    let data = fixture();
    let body = BodyData::new(&data).expect("body");
    let mut inputs = person(Sex::Female, 10, BodyTraits::default());
    inputs.bio_age = f64::NAN;
    let _ = body_params(&body, &inputs);
}

/// ADR-0005 section 1: a raw value that is not finite breaks `body_param_finite`.
#[test]
#[should_panic(expected = "invariant body_param_finite: muscle_z")]
fn a_value_that_is_not_finite_is_an_invariant_break() {
    let data = fixture();
    let body = BodyData::new(&data).expect("body");
    let z = BodyTraits {
        muscle: f64::INFINITY,
        ..BodyTraits::default()
    };
    let _ = body_params(&body, &person(Sex::Male, 30, z));
}

#[test]
#[should_panic(expected = "invariant body_input_age")]
fn an_infinite_bio_age_is_an_invariant_break() {
    let data = fixture();
    let body = BodyData::new(&data).expect("body");
    let mut inputs = person(Sex::Female, 10, BodyTraits::default());
    inputs.bio_age = f64::INFINITY;
    let _ = body_params(&body, &inputs);
}

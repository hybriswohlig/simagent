//! The properties and the clamp rate of the mapping onto the body contract (ADR-0005
//! section 8, ADR-0015 section 5), on the test map with every trait of the mapping
//! (`tests/golden/data_body`, one own locus per trait at frequency 0.5, standardized).

mod common;

use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use serde_json::Value;
use sim_core::body_contract::{
    BodyData, BodyEvaluation, BodyInputs, BodyParamKey, BodyTraits, CLAMPED_KEYS, body_params,
    body_traits,
};
use sim_core::data::Data;
use sim_core::finite::FiniteF64;
use sim_core::genome::{Genome, LocusAlleles, Sex};
use sim_core::growth::{IndividualCurve, height_loss, nutrition_loss};
use sim_core::params::NutritionYear;
use sim_core::rng::{DrawKey, Draws, SubjectKey, WorldSeed, tags};
use sim_core::time::{SimDuration, SimTime, YEAR, year_start};

fn fixture() -> Data {
    let (calibration, genome_map) = common::files_from(
        include_str!("../../../tests/golden/data_body/calibration/genetics.json"),
        include_str!("../../../tests/golden/data_body/genetics/genome_map.json"),
    );
    Data::new(common::definitions(calibration, genome_map).expect("definitions")).expect("checked")
}

/// A genome with the alternative allele where `bits` says so, two copies per locus of the
/// map in map order.
fn genome(data: &Data, sex: Sex, bits: &[bool]) -> Genome {
    let alleles: Vec<LocusAlleles> = data
        .genome_map()
        .loci
        .iter()
        .zip(bits.chunks(2))
        .map(|(locus, pair)| LocusAlleles {
            locus: locus.id.clone(),
            copies: pair.iter().map(|&bit| u8::from(bit)).collect(),
        })
        .collect();
    Genome::with_alleles(sex, &alleles, data).expect("fits")
}

/// The phenotypes of 4.2 from the genetic values and the given environments `c` and `e`
/// (one each per trait, in the order of `body_traits`).
fn traits(data: &Data, genome: &Genome, c: &[f64], e: &[f64]) -> BodyTraits {
    let values: Vec<f64> = body_traits()
        .iter()
        .zip(c.iter().zip(e))
        .map(|(t, (c, e))| {
            let [h2, c2, _, _] = data.phenotype_weights(t).expect("weights");
            let g = genome.genetic_value(t, data).expect("value");
            let e2 = (1.0 - h2) - c2;
            ((sim_math::sqrt(h2) * g) + (sim_math::sqrt(c2) * c)) + (sim_math::sqrt(e2) * e)
        })
        .collect();
    BodyTraits::from_values(values.try_into().expect("37"))
}

/// The raw value of `key` among the clamped units: the phenotype for a z parameter (10.3),
/// the height of 10.2 written out.
fn raw(data: &Data, inputs: &BodyInputs<'_>, key: BodyParamKey) -> f64 {
    let z = &inputs.z;
    let g = data.genetics();
    match key {
        BodyParamKey::HeightCm => {
            let curve = IndividualCurve::new(g.growth(inputs.sex), z.height, z.puberty);
            let x = if inputs.age < YEAR * 25 {
                inputs.age
            } else {
                YEAR * 25
            };
            (curve.height(x)
                - nutrition_loss(inputs.nutrition, &g.nutrition, inputs.born, x, curve.theta))
                - height_loss(&g.height_loss, inputs.sex, inputs.bio_age)
        }
        BodyParamKey::LegRatioZ => z.leg_ratio,
        BodyParamKey::HeadSizeZ => z.head_size,
        BodyParamKey::BodyFatZ => z.body_fat,
        BodyParamKey::FatDistributionZ => z.fat_distribution,
        BodyParamKey::MuscleZ => z.muscle,
        BodyParamKey::ShoulderWidthZ => z.shoulder_width,
        BodyParamKey::ChestDepthZ => z.chest_depth,
        BodyParamKey::HipWidthZ => z.hip_width,
        BodyParamKey::FaceAxis01 => z.face[0],
        BodyParamKey::FaceAxis02 => z.face[1],
        BodyParamKey::FaceAxis03 => z.face[2],
        BodyParamKey::FaceAxis04 => z.face[3],
        BodyParamKey::FaceAxis05 => z.face[4],
        BodyParamKey::FaceAxis06 => z.face[5],
        BodyParamKey::FaceAxis07 => z.face[6],
        BodyParamKey::FaceAxis08 => z.face[7],
        BodyParamKey::FaceAxis09 => z.face[8],
        BodyParamKey::FaceAxis10 => z.face[9],
        BodyParamKey::FaceAxis11 => z.face[10],
        BodyParamKey::FaceAxis12 => z.face[11],
        _ => panic!("not a clamped unit"),
    }
}

fn values(evaluation: &BodyEvaluation) -> Value {
    serde_json::to_value(evaluation.params).expect("json")
}

/// The statements of ADR-0005 section 8 for (a) and (b) about one input.
fn check(data: &Data, inputs: &BodyInputs<'_>) {
    let body = BodyData::new(data).expect("body");
    // Finite (else the evaluation panics), the same bits for the same input.
    let evaluation = body_params(&body, inputs);
    assert_eq!(body_params(&body, inputs), evaluation);
    // A flag exactly when the raw value lies outside; bounded units never clamped.
    for key in BodyParamKey::ALL {
        let flagged = evaluation.is_clamped(key);
        if CLAMPED_KEYS.contains(&key) {
            let spec = key.spec();
            let raw = raw(data, inputs, key);
            assert_eq!(
                flagged,
                raw < spec.min || raw > spec.max,
                "{}",
                key.as_str()
            );
        } else {
            assert!(!flagged, "{}", key.as_str());
        }
    }
    // Form ages: `min(b, 130)`, never falling, `b` from 25 on.
    let all = values(&evaluation);
    let b = inputs.bio_age;
    let form = if b < 130.0 { b } else { 130.0 };
    assert_eq!(all["body_form_age"].as_f64(), Some(form));
    assert_eq!(all["face_form_age"].as_f64(), Some(form));
    // Face axes do not read the age: the same person at another biological age.
    let younger = BodyInputs {
        bio_age: b / 2.0,
        ..*inputs
    };
    let other = values(&body_params(&body, &younger));
    for i in 1..=12 {
        let axis = format!("face_axis_{i:02}");
        assert_eq!(all[&axis], other[&axis], "{axis}");
    }
    // Nutrition after the year of the age reached changes nothing.
    let now = inputs.born + inputs.age;
    let later = sim_core::time::calendar(now).year + 1;
    let mut fed: Vec<NutritionYear> = inputs
        .nutrition
        .iter()
        .copied()
        .filter(|y| y.year < later)
        .collect();
    fed.push(NutritionYear {
        year: later,
        index: FiniteF64::try_new(0.0).expect("finite"),
    });
    let with_future = BodyInputs {
        nutrition: &fed,
        ..*inputs
    };
    let without: Vec<NutritionYear> = fed[..fed.len() - 1].to_vec();
    let plain = BodyInputs {
        nutrition: &without,
        ..*inputs
    };
    assert_eq!(body_params(&body, &with_future), body_params(&body, &plain));
}

/// The checked nutrition years from arbitrary pairs: ascending, one per year.
fn years(pairs: &[(i64, f64)]) -> Vec<NutritionYear> {
    let mut sorted: Vec<(i64, f64)> = pairs.to_vec();
    sorted.sort_by_key(|&(year, _)| year);
    sorted.dedup_by_key(|(year, _)| *year);
    sorted
        .into_iter()
        .map(|(year, index)| NutritionYear {
            year,
            index: FiniteF64::try_new(index).expect("finite"),
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 10_000,
        rng_seed: RngSeed::Fixed(0x0b0d_0015),
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    /// (a): uniform genome bits, C and E uniform from −6 to 6, nutrition uniform over its
    /// range, the age from 0 to 125 years, both sexes.
    #[test]
    fn body_params_properties_random(
        bits in prop::collection::vec(any::<bool>(), 80),
        male in any::<bool>(),
        c in prop::collection::vec(-6.0..=6.0_f64, 37),
        e in prop::collection::vec(-6.0..=6.0_f64, 37),
        age in 0..=(125 * YEAR.as_secs()),
        fed in prop::collection::vec((-2_i64..130, 0.0..=1.0_f64), 0..4),
    ) {
        let data = fixture();
        let sex = if male { Sex::Male } else { Sex::Female };
        let genome = genome(&data, sex, &bits);
        let nutrition = years(&fed);
        let age = SimDuration::from_secs(age);
        let inputs = BodyInputs {
            sex,
            age,
            bio_age: age.as_years(),
            born: year_start(0),
            nutrition: &nutrition,
            z: traits(&data, &genome, &c, &e),
            mc1r: genome.dose(data.genome_map().loci.iter().find(|l| l.id == sim_core::body_contract::mc1r()).expect("mc1r")),
            herc2: genome.dose(data.genome_map().loci.iter().find(|l| l.id == sim_core::body_contract::herc2()).expect("herc2")),
        };
        check(&data, &inputs);
    }
}

/// (b): extreme genomes (all alleles 0, all 1, each pigment locus in both homozygotes),
/// C and E at −6 and 6, nutrition at both ends of its range, the ages of ADR-0005.
#[test]
fn body_params_properties_extreme() {
    let data = fixture();
    let loci = data.genome_map().loci.len();
    for sex in [Sex::Female, Sex::Male] {
        for all in [false, true] {
            for (mc1r, herc2) in [(0, 0), (2, 0), (0, 2), (2, 2)] {
                let bits = vec![all; 2 * loci];
                let genome = genome(&data, sex, &bits);
                for env in [-6.0, 6.0] {
                    let z = traits(&data, &genome, &[env; 37], &[env; 37]);
                    for index in [0.0, 1.0] {
                        let nutrition = years(&[(0, index), (1, index), (12, index)]);
                        for years_old in [0, 2, 6, 10, 13, 16, 20, 25, 60, 100, 125] {
                            let age = YEAR * years_old;
                            let inputs = BodyInputs {
                                sex,
                                age,
                                bio_age: age.as_years(),
                                born: year_start(0),
                                nutrition: &nutrition,
                                z,
                                mc1r,
                                herc2,
                            };
                            check(&data, &inputs);
                        }
                    }
                }
            }
        }
    }
}

/// The sample of (c), person `i`: alleles at frequency 0.5, C and E standard normal, the
/// age uniform from 0 to 100 years, nutrition 1, from the draws of `BODY_CLAMP_SAMPLE`.
fn sampled(data: &Data, draws: &Draws, i: u64) -> (Genome, BodyTraits, SimDuration, u8, u8) {
    let key = |detail: u64| DrawKey {
        tag: tags::BODY_CLAMP_SAMPLE,
        subject: SubjectKey::WORLD,
        occasion: i,
        detail,
    };
    let [w0, w1] = draws.words::<2>(key(0));
    let bit = |n: u32| {
        let word = if n < 64 { w0 } else { w1 };
        (word >> (n % 64)) & 1 == 1
    };
    let loci = u32::try_from(data.genome_map().loci.len()).expect("small");
    let bits: Vec<bool> = (0..2 * loci).map(bit).collect();
    let sex = if bit(127) { Sex::Male } else { Sex::Female };
    let genome = genome(data, sex, &bits);
    let mut normals = Vec::with_capacity(76);
    for detail in 1..=19 {
        let w = draws.words::<8>(key(detail));
        for pair in w.chunks(2) {
            normals.push(sim_math::dist::normal(pair[0], pair[1], 0.0, 1.0));
        }
    }
    let z = traits(data, &genome, &normals[..37], &normals[37..74]);
    let age = SimDuration::from_secs(
        i64::try_from(draws.uniform_below(key(20), 100 * 364 * 86_400 + 1)).expect("small"),
    );
    let dose = |id: sim_core::data_id::DataId| {
        genome.dose(
            data.genome_map()
                .loci
                .iter()
                .find(|l| l.id == id)
                .expect("locus"),
        )
    };
    let (mc1r, herc2) = (
        dose(sim_core::body_contract::mc1r()),
        dose(sim_core::body_contract::herc2()),
    );
    (genome, z, age, mc1r, herc2)
}

/// The clamped parameters of a sample of `n` people, by key; `stretch` multiplies every
/// z phenotype (the negative twin).
fn clamp_counts(data: &Data, n: u64, stretch: f64) -> Vec<(BodyParamKey, u64)> {
    let body = BodyData::new(data).expect("body");
    let draws = Draws::new(WorldSeed::new(0x0b0d_c1a4));
    let mut counts: Vec<(BodyParamKey, u64)> = CLAMPED_KEYS.iter().map(|&k| (k, 0)).collect();
    for i in 0..n {
        let (genome, z, age, mc1r, herc2) = sampled(data, &draws, i);
        let mut z = z;
        if stretch != 1.0 {
            for v in [
                &mut z.leg_ratio,
                &mut z.head_size,
                &mut z.body_fat,
                &mut z.fat_distribution,
                &mut z.muscle,
                &mut z.shoulder_width,
                &mut z.chest_depth,
                &mut z.hip_width,
            ] {
                *v *= stretch;
            }
            for v in &mut z.face {
                *v *= stretch;
            }
        }
        let inputs = BodyInputs {
            sex: genome.sex(),
            age,
            bio_age: age.as_years(),
            born: SimTime::START,
            nutrition: &[],
            z,
            mc1r,
            herc2,
        };
        let evaluation = body_params(&body, &inputs);
        for (key, count) in &mut counts {
            if evaluation.is_clamped(*key) {
                *count += 1;
            }
        }
    }
    counts
}

/// (c): in a population sample of 100 000, every clamped parameter is clamped at most
/// 0.1 percent of the time (100); a standard normal z lies beyond ±4 about 6 times.
#[test]
fn body_clamp_rate_population_sample() {
    let data = fixture();
    for (key, count) in clamp_counts(&data, 100_000, 1.0) {
        assert!(count <= 100, "{}: {count}", key.as_str());
    }
}

/// The negative twin of (c): with every z raw value times 1.5 the check fires for every
/// z parameter (beyond ±4 then means beyond ±2.67, about 0.8 percent).
#[test]
fn body_clamp_rate_negative_twin() {
    let data = fixture();
    for (key, count) in clamp_counts(&data, 100_000, 1.5) {
        if key != BodyParamKey::HeightCm {
            assert!(count > 100, "{}: {count}", key.as_str());
        }
    }
}

/// A few people of the extreme set (b) for the named tests of ADR-0005 section 8.
fn some_people(data: &Data) -> Vec<(Sex, BodyTraits, u8, u8)> {
    let loci = data.genome_map().loci.len();
    let mut people = Vec::new();
    for sex in [Sex::Female, Sex::Male] {
        for (all, env, mc1r, herc2) in [(false, -6.0, 0, 2), (true, 6.0, 2, 0), (true, 0.5, 1, 1)] {
            let genome = genome(data, sex, &vec![all; 2 * loci]);
            people.push((
                sex,
                traits(data, &genome, &[env; 37], &[-env; 37]),
                mc1r,
                herc2,
            ));
        }
    }
    people
}

fn adult(sex: Sex, z: BodyTraits, mc1r: u8, herc2: u8, b: f64) -> BodyInputs<'static> {
    BodyInputs {
        sex,
        age: YEAR * 30,
        bio_age: b,
        born: year_start(0),
        nutrition: &[],
        z,
        mc1r,
        herc2,
    }
}

#[test]
fn body_params_pure() {
    let data = fixture();
    let body = BodyData::new(&data).expect("body");
    for (sex, z, mc1r, herc2) in some_people(&data) {
        let inputs = adult(sex, z, mc1r, herc2, 47.5);
        let first = values(&body_params(&body, &inputs));
        let again = values(&body_params(&body, &inputs));
        assert_eq!(first.to_string(), again.to_string());
    }
}

#[test]
fn bounded_units_never_clamped() {
    let data = fixture();
    let body = BodyData::new(&data).expect("body");
    for (sex, z, mc1r, herc2) in some_people(&data) {
        for b in [0.0, 13.0, 60.0, 125.0] {
            let evaluation = body_params(&body, &adult(sex, z, mc1r, herc2, b));
            assert!(
                evaluation
                    .clamped
                    .iter()
                    .all(|key| CLAMPED_KEYS.contains(key))
            );
        }
    }
}

/// Form ages never fall with the biological age and equal it from 25 on (up to 130).
#[test]
fn form_age_monotone_and_mature() {
    let data = fixture();
    let body = BodyData::new(&data).expect("body");
    for (sex, z, mc1r, herc2) in some_people(&data) {
        let mut previous = -1.0;
        for step in 0..=270_u32 {
            let b = f64::from(step) / 2.0;
            let all = values(&body_params(&body, &adult(sex, z, mc1r, herc2, b)));
            let form = all["body_form_age"].as_f64().expect("number");
            assert_eq!(all["face_form_age"].as_f64(), Some(form));
            assert!(form >= previous, "{b}");
            if (25.0..=130.0).contains(&b) {
                assert_eq!(form, b);
            }
            previous = form;
        }
    }
}

#[test]
fn face_axes_age_invariant() {
    let data = fixture();
    let body = BodyData::new(&data).expect("body");
    for (sex, z, mc1r, herc2) in some_people(&data) {
        let young = values(&body_params(&body, &adult(sex, z, mc1r, herc2, 1.0)));
        let old = values(&body_params(&body, &adult(sex, z, mc1r, herc2, 99.0)));
        for i in 1..=12 {
            let axis = format!("face_axis_{i:02}");
            assert_eq!(young[&axis], old[&axis], "{axis}");
        }
    }
}

/// Nutrition of a year after the current one changes nothing (ADR-0005 section 1).
#[test]
fn body_params_ignore_future_nutrition() {
    let data = fixture();
    let body = BodyData::new(&data).expect("body");
    let past = years(&[(3, 0.6)]);
    let future = years(&[(3, 0.6), (31, 0.0), (40, 0.2)]);
    for (sex, z, mc1r, herc2) in some_people(&data) {
        let with_past = BodyInputs {
            nutrition: &past,
            ..adult(sex, z, mc1r, herc2, 30.0)
        };
        let with_future = BodyInputs {
            nutrition: &future,
            ..with_past
        };
        assert_eq!(
            body_params(&body, &with_past),
            body_params(&body, &with_future)
        );
    }
}

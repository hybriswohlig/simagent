//! The mapping of a person onto the body contract (genetics specification 10, ADR-0015):
//! a pure function of the phenotypes, the environment, the sex and the age. Nothing of it
//! is state.

use alloc::vec::Vec;

use super::{BodyParamKey, BodyParams};
use crate::data::{Action, BodyModel, Chromosome, Data, DataError};
use crate::data_id::DataId;
use crate::genome::{Genome, Sex};
use crate::growth::{IndividualCurve, height_loss, nutrition_loss};
use crate::lineage::LineageKey;
use crate::params::NutritionYear;
use crate::phenotype::{Environment, Parents, phenotype};
use crate::time::{SimDuration, SimTime, YEAR};

/// The traits the mapping reads (ADR-0015 section 2, map 4 per ADR-0020 section 1): the
/// growth factor (`z_h`, the target height) and the timing, the z parameters, then the
/// traits of pigment, development, hair and skin.
pub fn body_traits() -> [DataId; 35] {
    [
        "core:growth",
        "core:puberty",
        "core:leg_ratio",
        "core:head_size",
        "core:body_fat",
        "core:fat_distribution",
        "core:muscle",
        "core:shoulder_width",
        "core:chest_depth",
        "core:hip_width",
        "core:face_01",
        "core:face_02",
        "core:face_03",
        "core:face_04",
        "core:face_05",
        "core:face_06",
        "core:face_07",
        "core:face_08",
        "core:face_09",
        "core:face_10",
        "core:face_11",
        "core:face_12",
        "core:pigment",
        "core:hair_darkness",
        "core:androgen",
        "core:estrogen",
        "core:breast",
        "core:curl",
        "core:freckle",
        "core:beard",
        "core:body_hair",
        "core:bald",
        "core:hair_density",
        "core:grey",
        "core:skin_age",
    ]
    .map(|t| DataId::new(t).unwrap_or_else(|_| panic!("invariant body.trait_id")))
}

/// The two large recessive pigment loci (genetics specification 10.4).
pub fn mc1r() -> DataId {
    DataId::new("core:mc1r").unwrap_or_else(|_| panic!("invariant body.locus_id"))
}

pub fn herc2() -> DataId {
    DataId::new("core:herc2").unwrap_or_else(|_| panic!("invariant body.locus_id"))
}

/// The parameters with the units `z` and `cm`, the only ones that are clamped (ADR-0005
/// section 1).
pub const CLAMPED_KEYS: [BodyParamKey; 21] = [
    BodyParamKey::HeightCm,
    BodyParamKey::LegRatioZ,
    BodyParamKey::HeadSizeZ,
    BodyParamKey::BodyFatZ,
    BodyParamKey::FatDistributionZ,
    BodyParamKey::MuscleZ,
    BodyParamKey::ShoulderWidthZ,
    BodyParamKey::ChestDepthZ,
    BodyParamKey::HipWidthZ,
    BodyParamKey::FaceAxis01,
    BodyParamKey::FaceAxis02,
    BodyParamKey::FaceAxis03,
    BodyParamKey::FaceAxis04,
    BodyParamKey::FaceAxis05,
    BodyParamKey::FaceAxis06,
    BodyParamKey::FaceAxis07,
    BodyParamKey::FaceAxis08,
    BodyParamKey::FaceAxis09,
    BodyParamKey::FaceAxis10,
    BodyParamKey::FaceAxis11,
    BodyParamKey::FaceAxis12,
];

/// The calendar age up to which `x = min(b, 25)` follows the age (10.1, 7.4).
const MATURE_YEARS: i64 = 25;

/// Definitions checked for the mapping (ADR-0015 section 2). Only `BodyData::new` makes one.
#[derive(Clone, Copy, Debug)]
pub struct BodyData<'a> {
    data: &'a Data,
}

impl<'a> BodyData<'a> {
    /// Every trait of `body_traits` stands in the map with a phenotype row whose child and
    /// adult weights are bitwise equal, and both pigment loci stand in the map as recessive
    /// loci on an autosome; else `data.body_traits`.
    pub fn new(data: &'a Data) -> Result<BodyData<'a>, DataError> {
        let traits_ok = body_traits().iter().all(|t| {
            data.phenotype_weights(t)
                .is_some_and(|[h2_child, c2_child, h2_adult, c2_adult]| {
                    h2_child.to_bits() == h2_adult.to_bits()
                        && c2_child.to_bits() == c2_adult.to_bits()
                })
        });
        let loci_ok = [mc1r(), herc2()].iter().all(|name| {
            data.genome_map().loci.iter().any(|locus| {
                locus.id == *name
                    && matches!(locus.action, Action::Recessive)
                    && locus.chromosome == Chromosome::Autosome
            })
        });
        if traits_ok && loci_ok {
            Ok(BodyData { data })
        } else {
            Err(DataError::BodyTraits)
        }
    }

    pub const fn data(&self) -> &'a Data {
        self.data
    }
}

/// The phenotypes the mapping reads, by trait (10.1, 10.3). `height` is `z_h`, the
/// phenotype of `core:growth` (ADR-0020 section 1); the eyebrows read `hair_density`, the
/// beard's coverage reads `beard` (the two merges of 10.7).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BodyTraits {
    pub height: f64,
    pub puberty: f64,
    pub leg_ratio: f64,
    pub head_size: f64,
    pub body_fat: f64,
    pub fat_distribution: f64,
    pub muscle: f64,
    pub shoulder_width: f64,
    pub chest_depth: f64,
    pub hip_width: f64,
    pub face: [f64; 12],
    pub pigment: f64,
    pub hair_darkness: f64,
    pub androgen: f64,
    pub estrogen: f64,
    pub breast: f64,
    pub curl: f64,
    pub freckle: f64,
    pub beard: f64,
    pub body_hair: f64,
    pub bald: f64,
    pub hair_density: f64,
    pub grey: f64,
    pub skin_age: f64,
}

impl BodyTraits {
    /// The phenotypes in the order of `body_traits`.
    pub fn from_values(values: [f64; 35]) -> BodyTraits {
        let [
            height,
            puberty,
            leg_ratio,
            head_size,
            body_fat,
            fat_distribution,
            muscle,
            shoulder_width,
            chest_depth,
            hip_width,
            f1,
            f2,
            f3,
            f4,
            f5,
            f6,
            f7,
            f8,
            f9,
            f10,
            f11,
            f12,
            pigment,
            hair_darkness,
            androgen,
            estrogen,
            breast,
            curl,
            freckle,
            beard,
            body_hair,
            bald,
            hair_density,
            grey,
            skin_age,
        ] = values;
        BodyTraits {
            height,
            puberty,
            leg_ratio,
            head_size,
            body_fat,
            fat_distribution,
            muscle,
            shoulder_width,
            chest_depth,
            hip_width,
            face: [f1, f2, f3, f4, f5, f6, f7, f8, f9, f10, f11, f12],
            pigment,
            hair_darkness,
            androgen,
            estrogen,
            breast,
            curl,
            freckle,
            beard,
            body_hair,
            bald,
            hair_density,
            grey,
            skin_age,
        }
    }
}

/// What the mapping reads of a person (ADR-0015 section 3).
#[derive(Clone, Copy, Debug)]
pub struct BodyInputs<'a> {
    pub sex: Sex,
    /// The calendar age.
    pub age: SimDuration,
    /// The biological age `b` in years (7.4).
    pub bio_age: f64,
    pub born: SimTime,
    /// The years of the nutrition index of the world (ADR-0013).
    pub nutrition: &'a [NutritionYear],
    pub z: BodyTraits,
    /// The copies of the alternative allele at MC1R and HERC2.
    pub mc1r: u8,
    pub herc2: u8,
}

impl<'a> BodyInputs<'a> {
    /// The inputs of a person: the sex from the genome, the phenotypes with the age 0 (their
    /// weights do not depend on age, `BodyData::new` checked that), the doses of the two
    /// pigment loci.
    #[allow(clippy::too_many_arguments)]
    pub fn of_person(
        body: &BodyData<'_>,
        environment: Environment<'_>,
        genome: &Genome,
        own: LineageKey,
        parents: Option<Parents>,
        born: SimTime,
        age: SimDuration,
        nutrition: &'a [NutritionYear],
        bio_age: f64,
    ) -> BodyInputs<'a> {
        let data = body.data();
        let values = body_traits().map(|t| {
            phenotype(
                environment,
                data,
                genome,
                own,
                parents,
                &t,
                SimDuration::ZERO,
            )
            .unwrap_or_else(|| panic!("invariant body.phenotype"))
        });
        let dose = |name: DataId| {
            let locus = data
                .genome_map()
                .loci
                .iter()
                .find(|locus| locus.id == name)
                .unwrap_or_else(|| panic!("invariant body.locus"));
            genome.dose(locus)
        };
        BodyInputs {
            sex: genome.sex(),
            age,
            bio_age,
            born,
            nutrition,
            z: BodyTraits::from_values(values),
            mc1r: dose(mc1r()),
            herc2: dose(herc2()),
        }
    }
}

/// The values of the contract and which of them were clamped (ADR-0005 section 1).
#[derive(Clone, Debug, PartialEq)]
pub struct BodyEvaluation {
    pub params: BodyParams,
    /// The parameters whose raw value lay truly outside their range.
    pub clamped: Vec<BodyParamKey>,
    /// Whether an input of the individual curve was held at the bound (ADR-0012).
    pub curve_clamped: bool,
}

impl BodyEvaluation {
    pub fn is_clamped(&self, key: BodyParamKey) -> bool {
        self.clamped.contains(&key)
    }
}

/// `L(u) = 1 / (1 + e^(−u))` (10.1).
fn logistic(u: f64) -> f64 {
    1.0 / (1.0 + sim_math::exp(-u))
}

/// `S(0, end, t)`: 0 up to 0, 1 from `end` on, the smoothstep of 5.1 in between (10.1).
fn rise(end: f64, t: f64) -> f64 {
    let u = t / end;
    if u <= 0.0 {
        0.0
    } else if u >= 1.0 {
        1.0
    } else {
        3.0 * (u * u) - 2.0 * ((u * u) * u)
    }
}

/// The raw values of the contract in the order of `BodyParamKey::ALL` (genetics
/// specification 10), and whether the curve held an input.
pub(crate) fn raw_body_values(
    body: &BodyData<'_>,
    inputs: &BodyInputs<'_>,
) -> (Vec<(BodyParamKey, f64)>, bool) {
    let genetics = body.data().genetics();
    let m: &BodyModel = &genetics.body;
    let z = &inputs.z;
    let b = inputs.bio_age;
    let male = matches!(inputs.sex, Sex::Male);
    let mature = YEAR * MATURE_YEARS;
    let x = if inputs.age < mature {
        inputs.age
    } else {
        mature
    };

    // 10.2: form ages and height.
    let form_age = if b < BodyParamKey::BodyFormAge.spec().max {
        b
    } else {
        BodyParamKey::BodyFormAge.spec().max
    };
    let curve = IndividualCurve::new(genetics.growth(inputs.sex), z.height, z.puberty);
    let theta = curve.theta;
    let height = (curve.height(x)
        - nutrition_loss(inputs.nutrition, &genetics.nutrition, inputs.born, x, theta))
        - height_loss(&genetics.height_loss, inputs.sex, b);

    // 10.4: pigment.
    let red = inputs.mc1r == 2;
    let eta_s = m.pigment_skin_offset + m.pigment_skin_slope * z.pigment;
    let eta_h = (m.pigment_hair_slope * z.pigment + m.pigment_hair_residual * z.hair_darkness)
        - m.pigment_hair_darkening * (1.0 - rise(m.pigment_hair_darkening_age, b));
    let eta_i = (m.pigment_iris_offset + m.pigment_iris_slope * z.pigment)
        - m.pigment_iris_darkening * (1.0 - rise(m.pigment_iris_darkening_age, b));
    let skin_eu = logistic(eta_s) * if red { m.mc1r_skin_eu } else { 1.0 };
    let hair_eu = logistic(eta_h) * if red { m.mc1r_hair_eu } else { 1.0 };
    let iris = logistic(eta_i) * if inputs.herc2 == 2 { m.herc2_iris } else { 1.0 };
    let skin_pheo = if red {
        m.mc1r_skin_pheo
    } else {
        logistic(m.pigment_skin_pheo_offset + m.pigment_skin_pheo_slope * z.pigment)
    };
    let hair_pheo = if red {
        m.mc1r_hair_pheo
    } else {
        logistic(m.pigment_hair_pheo_offset + m.pigment_hair_pheo_slope * z.pigment)
    };

    // 10.5: development.
    let tempo = m.development_tempo;
    let s_a = logistic(((b - theta) - m.development_androgen_lag) / tempo);
    let s_e = logistic(((b - theta) + m.development_estrogen_lead) / tempo);
    let s_b = logistic(((b - theta) + m.development_breast_lead) / tempo);
    let androgen = s_a * logistic(m.development_androgen_offset + z.androgen);
    let estrogen = s_e * logistic(m.development_estrogen_offset + z.estrogen);
    let (androgenic, estrogenic, breast) = if male {
        (androgen, m.development_estrogen_men * estrogen, 0.0)
    } else {
        (
            m.development_androgen_women * androgen,
            estrogen,
            s_b * logistic(z.breast),
        )
    };

    // 10.6: hair, skin and the signs of age.
    let curl = logistic(m.curl_offset + m.curl_slope * z.curl);
    let freckles = logistic(
        ((m.freckle_offset + m.freckle_slope * z.freckle) + m.freckle_pigment * z.pigment)
            + m.mc1r_freckle * f64::from(inputs.mc1r),
    );
    let brow = logistic(z.hair_density);
    let beard_rise = logistic(((b - theta) - m.beard_lag) / m.beard_tempo);
    let (beard_density, beard_coverage) = if male {
        (
            beard_rise * logistic((m.beard_offset + z.beard) + m.beard_androgen * z.androgen),
            beard_rise * logistic(z.beard),
        )
    } else {
        (0.0, 0.0)
    };
    let hair_rise = logistic(((b - theta) - m.body_hair_lag) / m.body_hair_tempo);
    let body_hair_men = hair_rise * logistic(z.body_hair + m.body_hair_androgen * z.androgen);
    let body_hair = if male {
        body_hair_men
    } else {
        m.body_hair_women * body_hair_men
    };
    let balding = if male {
        let center =
            (m.bald_men_center - m.bald_men_slope * z.bald) - m.bald_men_androgen * z.androgen;
        m.bald_men_stage * logistic((b - center) / m.bald_men_tempo)
    } else {
        let center = m.bald_women_center - m.bald_women_slope * z.bald;
        m.bald_women_stage * logistic((b - center) / m.bald_women_tempo)
    };
    let density = logistic(m.hair_density_offset + z.hair_density)
        * (1.0
            - m.hair_density_loss * logistic((b - m.hair_density_center) / m.hair_density_tempo));
    let grey = logistic((b - (m.grey_center - m.grey_slope * z.grey)) / m.grey_tempo);
    let wrinkles =
        logistic(((b - m.wrinkles_center) + m.wrinkles_slope * z.skin_age) / m.wrinkles_tempo);
    let laxity = logistic(((b - m.laxity_center) + m.laxity_slope * z.skin_age) / m.laxity_tempo);
    let spots = logistic(
        (((b - m.spots_center) + m.spots_skin * z.skin_age) - m.spots_pigment * z.pigment)
            / m.spots_tempo,
    );
    let stoop = logistic(((b - m.stoop_center) + m.stoop_skin * z.skin_age) / m.stoop_tempo);

    let f = &z.face;
    let values = [
        (BodyParamKey::BodyFormAge, form_age),
        (BodyParamKey::FaceFormAge, form_age),
        (BodyParamKey::HeightCm, height),
        (BodyParamKey::LegRatioZ, z.leg_ratio),
        (BodyParamKey::HeadSizeZ, z.head_size),
        (BodyParamKey::BodyFatZ, z.body_fat),
        (BodyParamKey::FatDistributionZ, z.fat_distribution),
        (BodyParamKey::MuscleZ, z.muscle),
        (BodyParamKey::ShoulderWidthZ, z.shoulder_width),
        (BodyParamKey::ChestDepthZ, z.chest_depth),
        (BodyParamKey::HipWidthZ, z.hip_width),
        (BodyParamKey::FaceAxis01, f[0]),
        (BodyParamKey::FaceAxis02, f[1]),
        (BodyParamKey::FaceAxis03, f[2]),
        (BodyParamKey::FaceAxis04, f[3]),
        (BodyParamKey::FaceAxis05, f[4]),
        (BodyParamKey::FaceAxis06, f[5]),
        (BodyParamKey::FaceAxis07, f[6]),
        (BodyParamKey::FaceAxis08, f[7]),
        (BodyParamKey::FaceAxis09, f[8]),
        (BodyParamKey::FaceAxis10, f[9]),
        (BodyParamKey::FaceAxis11, f[10]),
        (BodyParamKey::FaceAxis12, f[11]),
        (BodyParamKey::SkinEumelanin, skin_eu),
        (BodyParamKey::SkinPheomelanin, skin_pheo),
        (BodyParamKey::HairEumelanin, hair_eu),
        (BodyParamKey::HairPheomelanin, hair_pheo),
        (BodyParamKey::IrisMelanin, iris),
        (BodyParamKey::HairCurl, curl),
        (BodyParamKey::HairDensity, density),
        (BodyParamKey::BaldingStage, balding),
        (BodyParamKey::HairGreyFraction, grey),
        (BodyParamKey::EyebrowDensity, brow),
        (BodyParamKey::BeardDensity, beard_density),
        (BodyParamKey::BeardCoverage, beard_coverage),
        (BodyParamKey::BodyHair, body_hair),
        (BodyParamKey::FreckleDensity, freckles),
        (BodyParamKey::FaceWrinkles, wrinkles),
        (BodyParamKey::SkinLaxity, laxity),
        (BodyParamKey::AgeSpots, spots),
        (BodyParamKey::PostureStoop, stoop),
        (BodyParamKey::AndrogenicDevelopment, androgenic),
        (BodyParamKey::EstrogenicDevelopment, estrogenic),
        (BodyParamKey::BreastDevelopment, breast),
    ];
    (values.to_vec(), curve.clamped)
}

/// The body of a person (ADR-0005 section 1, ADR-0015 section 3): `b` finite and at least 0,
/// every raw value finite, `z` and `cm` clamped to their range and noted, every other value
/// inside its range by its formula.
pub fn body_params(body: &BodyData<'_>, inputs: &BodyInputs<'_>) -> BodyEvaluation {
    let b = inputs.bio_age;
    if !(b.is_finite() && b >= 0.0) {
        panic!("invariant body_input_age: {b}");
    }
    let (raw, curve_clamped) = raw_body_values(body, inputs);
    let mut params = BodyParams::DEFAULT;
    let mut clamped = Vec::new();
    for (key, value) in raw {
        if !value.is_finite() {
            panic!("invariant body_param_finite: {}", key.as_str());
        }
        let spec = key.spec();
        let value = if !CLAMPED_KEYS.contains(&key) {
            value
        } else if value < spec.min {
            clamped.push(key);
            spec.min
        } else if value > spec.max {
            clamped.push(key);
            spec.max
        } else {
            value
        };
        params = params
            .with(key, value)
            .unwrap_or_else(|_| panic!("invariant body_param_bounded: {}", key.as_str()));
    }
    BodyEvaluation {
        params,
        clamped,
        curve_clamped,
    }
}

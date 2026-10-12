//! Definitions (ADR-0007): the genome map (B9) and the genetics calibration (B13) as the
//! core sees them, without the text of their files. `Data::new` checks them once; only
//! checked data reaches a world. The data hash covers them (ADR-0004 section 2). Traits,
//! loads and reference values of the genome map follow ADR-0008.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::fmt;

use serde::{Deserialize, Serialize};

use crate::data_id::DataId;
use crate::data_id::str_key;
use crate::demography::{Demography, DemographyCalibration, read_demography};
use crate::fertility::{Curves, Puberty};
use crate::finite::FiniteF64;
use crate::hash::stable_hash64;
use crate::mortality::{ACCIDENT_RANGE, GOMPERTZ_A_RANGE, GOMPERTZ_B_RANGE, Gompertz, MAX_SIGMA_G};

/// Format version of the calibration file (ADR-0009 section 2, ADR-0017 section 1).
pub const CALIBRATION_FORMAT_VERSION: u32 = 7;

/// Format version of the genome map (ADR-0008 section 5).
pub const MAP_FORMAT_VERSION: u32 = 2;

/// The calibration values of the genetics (`core:calibration_genetics`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneticsCalibration {
    pub format_version: u32,
    pub values: BTreeMap<DataId, Calibrated>,
    pub traits: Vec<PhenotypeWeights>,
    pub sex_differences: Vec<SexDifference>,
    pub diseases: Vec<Disease>,
    /// The trait of longevity `g_L` (genetics specification 7.2), or none: then the genetic
    /// factor of death is 1 (11.1). The repository's data names none before the pre-run.
    /// The field is required: the data name the trait or say `null` (ADR-0017 section 1).
    #[serde(deserialize_with = "Option::deserialize")]
    pub longevity_trait: Option<DataId>,
    /// The trait of the puberty timing `z_p` (5.2), or none: then `θ_i = θ` (11.1). Required
    /// like the trait of longevity (ADR-0017 section 1).
    #[serde(deserialize_with = "Option::deserialize")]
    pub puberty_trait: Option<DataId>,
    /// The trait of cognitive ability `z_K` the partner market compares (11.2), or none:
    /// then the term drops out (11.1). Required like the others; it needs phenotype
    /// weights.
    #[serde(deserialize_with = "Option::deserialize")]
    pub cognition_trait: Option<DataId>,
    /// The trait of the target height `z_h` the partner market compares (11.2), or none,
    /// as `cognition_trait`.
    #[serde(deserialize_with = "Option::deserialize")]
    pub height_trait: Option<DataId>,
    /// The curves of fertility (5.6, ADR-0017 section 1).
    pub fertility: FertilityCurves,
}

/// The curves of fertility in the file (ADR-0017 section 1): after 5.6, or the window of
/// the test data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FertilityCurves {
    Puberty(PubertyCurves),
    Window(WindowCurve),
}

/// The twelve numbers of 5.6 (`crate::fertility::Puberty`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PubertyCurves {
    pub menarche_mean: Calibrated,
    pub menarche_sd: Calibrated,
    pub menarche_low: Calibrated,
    pub menarche_high: Calibrated,
    pub female_ramp: Calibrated,
    pub female_decline_from: Calibrated,
    pub female_decline_to: Calibrated,
    pub male_lead: Calibrated,
    pub male_ramp: Calibrated,
    pub male_decline_from: Calibrated,
    pub male_floor: Calibrated,
    pub male_decline_to: Calibrated,
}

/// The woman's window of the test data, in years.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowCurve {
    pub from: Calibrated,
    pub until: Calibrated,
}

/// A hereditary disease (genetics specification 3, ADR-0017 section 1): what carries it,
/// when it begins and what it does. The order of the list is the order of the causes of
/// death (ADR-0017 section 2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Disease {
    pub code: DataId,
    pub carrier: Carrier,
    pub onset: Onset,
    pub effect: Effect,
}

/// Who can have a disease (genetics specification 3.1, 3.4, 3.6).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Carrier {
    /// A mendelian locus of the map, marked disease: affected by the genotype of 3.4 and
    /// the penetrance of the sex.
    Locus {
        locus: DataId,
        penetrance_female: Calibrated,
        penetrance_male: Calibrated,
    },
    /// The threshold model of 3.6 on a trait of liability; without one nobody is affected
    /// (11.1).
    Threshold {
        liability: Option<DataId>,
        heritability: Calibrated,
        prevalence: Calibrated,
    },
}

/// The onset of a disease in years (genetics specification 3.1): fixed, or drawn and held
/// at its bounds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Onset {
    Fixed {
        age: Calibrated,
    },
    Normal {
        mean: Calibrated,
        sd: Calibrated,
        low: Calibrated,
        high: Calibrated,
    },
}

/// What a disease does from its onset while the switch is on (genetics specification 3.1).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Effect {
    /// A cause of death with a constant hazard per year.
    Death { per_year: Calibrated },
    /// A cause of death with its hazard in two steps (`mortality::StepHazard`).
    DeathInTwoSteps {
        before: Calibrated,
        change_age: Calibrated,
        after: Calibrated,
    },
    /// Visible in the pedigree, without a risk of death.
    Visible,
}

/// The sex difference of one trait with a phenotype (ADR-0014 section 1, genetics
/// specification 6.2): the shift `d_t` of men in standard deviations of women and the
/// variance ratio `VR_t` of men to women.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SexDifference {
    #[serde(rename = "trait")]
    pub target: DataId,
    pub shift: Calibrated,
    pub variance_ratio: Calibrated,
}

/// The weights of the phenotype of one trait (ADR-0009 section 1, genetics specification
/// 4.2): heritability and shared environment for the child and the adult.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhenotypeWeights {
    #[serde(rename = "trait")]
    pub target: DataId,
    pub h2_child: Calibrated,
    pub c2_child: Calibrated,
    pub h2_adult: Calibrated,
    pub c2_adult: Calibrated,
}

/// One calibration value with its unit; source, date and note stay in the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Calibrated {
    pub value: FiniteF64,
    pub unit: Unit,
}

/// The unit of a calibration value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Morgan,
    PerTransmission,
    Fraction,
    Centimeter,
    Year,
    PerYear,
    CentimeterPerYear,
    CentimeterPerYearSquared,
    StandardDeviation,
    Ratio,
    Logit,
    Stage,
    /// The life course data (ADR-0017 section 1).
    Month,
    Count,
    Hour,
    PerStandardDeviation,
}

/// The genome map (`core:genome_map`, B9): the traits in the order of their data IDs,
/// then the loci in map order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenomeMap {
    pub format_version: u32,
    pub map_version: u32,
    pub traits: Vec<Trait>,
    pub loci: Vec<Locus>,
}

/// A trait the core computes a genetic value for (ADR-0008 section 1): its reference
/// standard deviation, whether its loci may load on nothing else, and the traits whose
/// loci go into its raw value with a weight.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trait {
    pub id: DataId,
    pub sigma_ref: FiniteF64,
    pub exclusive: bool,
    pub includes: Vec<Load>,
}

/// A load of a locus on a trait, or a trait included in another one, with its weight.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Load {
    #[serde(rename = "trait")]
    pub target: DataId,
    pub weight: FiniteF64,
}

/// The values of an additive or threshold locus (ADR-0008 section 2): effect size, mean
/// dose of the reference population, one or two loads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Weights {
    pub effect: FiniteF64,
    pub d_ref: FiniteF64,
    pub loads: Vec<Load>,
}

/// A locus of the genome map (genetics specification 2.1, 2.5, 2.4).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Locus {
    pub id: DataId,
    pub chromosome: Chromosome,
    /// The segment within its chromosome type: 0 to 7 on the autosomes, 0 elsewhere.
    pub segment: u8,
    pub slot: u8,
    pub action: Action,
    pub mark: Mark,
    pub mutation: MutationDirection,
}

/// The chromosome type of a locus, in map order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Chromosome {
    Autosome,
    X,
    Y,
    Mt,
}

/// How the alleles of a locus act (genetics specification 2.5); additive and threshold
/// loci carry their values (ADR-0008 section 2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Additive(Weights),
    Recessive,
    Dominant,
    Threshold(Weights),
}

impl Action {
    /// The values of an additive or threshold locus.
    pub const fn weights(&self) -> Option<&Weights> {
        match self {
            Action::Additive(weights) | Action::Threshold(weights) => Some(weights),
            Action::Recessive | Action::Dominant => None,
        }
    }
}

/// The mark of a locus (B9): small polygenic, large, disease.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mark {
    Small,
    Large,
    Disease,
}

/// Which way an allele of a locus may mutate (genetics specification 2.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationDirection {
    Both,
    ToAlt,
    ToRef,
}

/// The definitions of one world, before the check. Its postcard bytes are what the data
/// hash covers (ADR-0007 section 3): the sets in the order of their data IDs,
/// `core:calibration_demography`, `core:calibration_genetics`, `core:genome_map`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Definitions {
    pub demography: DemographyCalibration,
    pub calibration: GeneticsCalibration,
    pub genome_map: GenomeMap,
}

/// A map length in Morgan: the expected number of crossovers of a segment per meiosis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Morgans(pub f64);

/// A rate per locus and transmission.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PerTransmission(pub f64);

/// The genetics calibration as the core reads it (genetics specification 2.2, 2.4, 11.3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Genetics {
    pub map_length_autosome_female: Morgans,
    pub map_length_autosome_male: Morgans,
    pub map_length_x_female: Morgans,
    pub mutation_rate: PerTransmission,
    pub mutation_rate_disease: PerTransmission,
    pub growth_male: GrowthCurve,
    pub growth_female: GrowthCurve,
    pub nutrition: NutritionLoss,
    /// The half width `w` of the puberty progress around `θ_i` in years (ADR-0014).
    pub puberty_half_width: f64,
    pub height_loss: HeightLoss,
    pub body: BodyModel,
    /// The weight `w` of the biological age (genetics specification 7.4, ADR-0015).
    pub bio_age_weight: f64,
    /// The sex ratio at birth `r`, sons per daughter (genetics specification 2.3, 7.5,
    /// 11.3); the father passes his Y with `r / (1 + r)` (ADR-0017 section 4).
    pub sex_ratio_at_birth: f64,
    /// Death (genetics specification 7.1, 7.2, ADR-0017 section 2): the Gompertz term of
    /// senescence and the Makeham term of accident per sex, and `σ_g` of the genetic factor.
    pub senescence_female: Gompertz,
    pub senescence_male: Gompertz,
    pub accident_female: f64,
    pub accident_male: f64,
    pub longevity_sigma_g: f64,
    /// The curves of fertility (5.6).
    pub fertility: Curves,
}

/// The numbers of the mapping onto the body contract (genetics specification 10, ADR-0015
/// section 1), one field per calibration value `core:body_<field>`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyModel {
    pub pigment_skin_offset: f64,
    pub pigment_skin_slope: f64,
    pub pigment_hair_slope: f64,
    pub pigment_hair_residual: f64,
    pub pigment_hair_darkening: f64,
    pub pigment_hair_darkening_age: f64,
    pub pigment_iris_offset: f64,
    pub pigment_iris_slope: f64,
    pub pigment_iris_darkening: f64,
    pub pigment_iris_darkening_age: f64,
    pub pigment_skin_pheo_offset: f64,
    pub pigment_skin_pheo_slope: f64,
    pub pigment_hair_pheo_offset: f64,
    pub pigment_hair_pheo_slope: f64,
    pub mc1r_hair_eu: f64,
    pub mc1r_skin_eu: f64,
    pub mc1r_hair_pheo: f64,
    pub mc1r_skin_pheo: f64,
    pub mc1r_freckle: f64,
    pub herc2_iris: f64,
    pub development_androgen_lag: f64,
    pub development_estrogen_lead: f64,
    pub development_breast_lead: f64,
    pub development_tempo: f64,
    pub development_androgen_offset: f64,
    pub development_estrogen_offset: f64,
    pub development_estrogen_men: f64,
    pub development_androgen_women: f64,
    pub curl_offset: f64,
    pub curl_slope: f64,
    pub freckle_offset: f64,
    pub freckle_slope: f64,
    pub freckle_pigment: f64,
    pub beard_lag: f64,
    pub beard_tempo: f64,
    pub beard_offset: f64,
    pub beard_androgen: f64,
    pub body_hair_lag: f64,
    pub body_hair_tempo: f64,
    pub body_hair_androgen: f64,
    pub body_hair_women: f64,
    pub bald_men_stage: f64,
    pub bald_men_center: f64,
    pub bald_men_slope: f64,
    pub bald_men_androgen: f64,
    pub bald_men_tempo: f64,
    pub bald_women_stage: f64,
    pub bald_women_center: f64,
    pub bald_women_slope: f64,
    pub bald_women_tempo: f64,
    pub hair_density_offset: f64,
    pub hair_density_loss: f64,
    pub hair_density_center: f64,
    pub hair_density_tempo: f64,
    pub grey_center: f64,
    pub grey_slope: f64,
    pub grey_tempo: f64,
    pub wrinkles_center: f64,
    pub wrinkles_slope: f64,
    pub wrinkles_tempo: f64,
    pub laxity_center: f64,
    pub laxity_slope: f64,
    pub laxity_tempo: f64,
    pub spots_center: f64,
    pub spots_skin: f64,
    pub spots_pigment: f64,
    pub spots_tempo: f64,
    pub stoop_center: f64,
    pub stoop_skin: f64,
    pub stoop_tempo: f64,
}

/// The loss of height in old age (genetics specification 5.3, ADR-0014): from the
/// biological age `onset` on, `a · (b − onset)²` cm with the factor of the sex in cm per
/// year².
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeightLoss {
    pub onset: f64,
    pub male: f64,
    pub female: f64,
}

/// The loss of height from nutrition (genetics specification 5.4, ADR-0013 section 3): `k`
/// in cm per deficit year, the weights of the three windows, the ends of the first two
/// windows (years of age) and the reach of the puberty window around `θ_i` (years).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NutritionLoss {
    pub per_deficit_year: f64,
    pub weight_infant: f64,
    pub weight_child: f64,
    pub weight_puberty: f64,
    pub age_infant: f64,
    pub age_child: f64,
    pub puberty_before: f64,
    pub puberty_after: f64,
}

impl Genetics {
    /// The mean growth curve of one sex (genetics specification 5.1).
    pub const fn growth(&self, sex: crate::genome::Sex) -> GrowthCurve {
        match sex {
            crate::genome::Sex::Male => self.growth_male,
            crate::genome::Sex::Female => self.growth_female,
        }
    }
}

/// The values of the growth curve of one sex (ADR-0010): adult height `h1` (cm),
/// Preece-Baines `theta` (years), `h_theta` (cm), `s0` and `s1` (per year), the WHO
/// medians at birth, 12 and 24 months (cm), and the infancy term's `b` and `c` derived
/// from them; for the individual curve (ADR-0012) the spread of adult height `sd` (cm),
/// of the timing `theta_sd` (years) and the trade-off `gamma` (cm per year).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GrowthCurve {
    pub h1: f64,
    pub theta: f64,
    pub h_theta: f64,
    pub s0: f64,
    pub s1: f64,
    pub l0: f64,
    pub l12: f64,
    pub l24: f64,
    pub b: f64,
    pub c: f64,
    pub sd: f64,
    pub theta_sd: f64,
    pub gamma: f64,
}

/// Upper limits of the growth values (ADR-0010 section 2, ADR-0012 section 2).
const MAX_HEIGHT: f64 = 300.0;
const MAX_THETA: f64 = 30.0;
const MAX_RATE_PER_YEAR: f64 = 10.0;
const MAX_THETA_SD: f64 = 5.0;
/// Upper limit of the reach of the puberty window (ADR-0013 section 3) and of the half
/// width of the puberty progress (ADR-0014).
const MAX_PUBERTY_REACH: f64 = 10.0;
/// Upper limit of an age in the calibration (ADR-0014), the oldest age of the contract.
const MAX_AGE: f64 = 130.0;
/// Limits of a logit and of a stage of the body mapping (ADR-0015 section 1).
const MAX_LOGIT: f64 = 20.0;
const MAX_STAGE: f64 = 7.0;
/// Limits of a sex difference (ADR-0014 section 1).
const MAX_SHIFT: f64 = 10.0;
const MAX_VARIANCE_RATIO: f64 = 10.0;
/// Upper limit of the sex ratio at birth (ADR-0017 section 4); every ratio above 0 gives
/// a probability of a son below 1, real ratios lie near 1.05.
const MAX_SEX_RATIO: f64 = 10.0;

/// Checked definitions with their data hash. Only `Data::new` makes one.
#[derive(Clone, Debug, PartialEq)]
pub struct Data {
    definitions: Definitions,
    demography: Demography,
    genetics: Genetics,
    hash: u64,
    /// The position of each locus in the map, by its data ID.
    loci: BTreeMap<DataId, usize>,
    /// For each trait its terms in the order of the fold and its `sigma_ref` (ADR-0008
    /// section 3).
    traits: BTreeMap<DataId, (Vec<Term>, f64)>,
    /// For each trait with a phenotype its weights: h2 and c2 of the child, then of the
    /// adult (ADR-0009 section 1).
    phenotypes: BTreeMap<DataId, [f64; 4]>,
    /// For each trait with a sex difference its shift and variance ratio (ADR-0014).
    sex_differences: BTreeMap<DataId, (f64, f64)>,
    /// For each threshold disease with a trait of liability its threshold
    /// `T = probit(1 − K)` (genetics specification 3.6, ADR-0020 section 2).
    thresholds: BTreeMap<DataId, f64>,
}

/// Why definitions are not loaded; the code is stable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataError {
    FormatVersion,
    MapVersion,
    LocusPosition,
    LocusOrder,
    LocusId,
    LocusKey,
    LocusMark,
    TraitOrder,
    TraitKey,
    TraitSigma,
    TraitInclude,
    TraitUnused,
    LocusWeights,
    LocusLoad,
    LoadExclusive,
    LocusAdditive,
    TraitBounds,
    CalibrationMissing,
    CalibrationUnknown,
    CalibrationUnit,
    CalibrationRange,
    /// The definitions lack a trait or a locus of the body mapping (ADR-0015 section 2).
    BodyTraits,
    /// A disease with a carrier or a value that does not fit (ADR-0017 section 1).
    Disease,
    /// A trait named by the calibration is not in the map (ADR-0017 section 1).
    TraitNamed,
    /// A value of the life course data outside its range, or a contradiction (ADR-0017
    /// section 1).
    Demography,
}

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            DataError::FormatVersion => "data.format_version",
            DataError::MapVersion => "data.map_version",
            DataError::LocusPosition => "data.locus_position",
            DataError::LocusOrder => "data.locus_order",
            DataError::LocusId => "data.locus_id",
            DataError::LocusKey => "data.locus_key",
            DataError::LocusMark => "data.locus_mark",
            DataError::TraitOrder => "data.trait_order",
            DataError::TraitKey => "data.trait_key",
            DataError::TraitSigma => "data.trait_sigma",
            DataError::TraitInclude => "data.trait_include",
            DataError::TraitUnused => "data.trait_unused",
            DataError::LocusWeights => "data.locus_weights",
            DataError::LocusLoad => "data.locus_load",
            DataError::LoadExclusive => "data.load_exclusive",
            DataError::LocusAdditive => "data.locus_additive",
            DataError::TraitBounds => "data.trait_bounds",
            DataError::CalibrationMissing => "data.calibration_missing",
            DataError::CalibrationUnknown => "data.calibration_unknown",
            DataError::CalibrationUnit => "data.calibration_unit",
            DataError::CalibrationRange => "data.calibration_range",
            DataError::BodyTraits => "data.body_traits",
            DataError::Disease => "data.disease",
            DataError::TraitNamed => "data.trait_named",
            DataError::Demography => "data.demography",
        })
    }
}

/// Autosomal segments and slots per segment, slots of X, Y and mt (genetics
/// specification 2.1, 2.2).
pub const AUTOSOME_SEGMENTS: u8 = 8;
pub const AUTOSOME_SLOTS: u8 = 24;
pub const X_SLOTS: u8 = 16;
pub const Y_SLOTS: u8 = 8;
pub const MT_SLOTS: u8 = 8;

/// Largest map length of a segment: the largest mean of `poisson` (ADR-0001 section 3).
const MAX_MAP_LENGTH: f64 = 30.0;

impl Locus {
    /// Inside its chromosome type (specification 2.1, 2.2).
    const fn in_range(&self) -> bool {
        let (segments, slots) = match self.chromosome {
            Chromosome::Autosome => (AUTOSOME_SEGMENTS, AUTOSOME_SLOTS),
            Chromosome::X => (1, X_SLOTS),
            Chromosome::Y => (1, Y_SLOTS),
            Chromosome::Mt => (1, MT_SLOTS),
        };
        self.segment < segments && self.slot < slots
    }

    /// The marks of B9: a disease locus mutates only to the alternative allele (3.3), a
    /// mendelian locus is marked large or disease.
    const fn marked(&self) -> bool {
        let disease_ok = !matches!(self.mark, Mark::Disease)
            || matches!(self.mutation, MutationDirection::ToAlt);
        let mendelian = matches!(self.action, Action::Recessive | Action::Dominant);
        disease_ok && (!mendelian || matches!(self.mark, Mark::Large | Mark::Disease))
    }

    const fn position(&self) -> (Chromosome, u8, u8) {
        (self.chromosome, self.segment, self.slot)
    }
}

/// The checks of the genome map; `key` is `str_key` outside the tests.
fn check_map(map: &GenomeMap, key: impl Fn(&DataId) -> u64) -> Result<(), DataError> {
    if map.format_version != MAP_FORMAT_VERSION {
        return Err(DataError::FormatVersion);
    }
    if map.map_version == 0 {
        return Err(DataError::MapVersion);
    }
    check_traits(&map.traits)?;
    let mut ids = BTreeMap::new();
    let mut previous = None;
    for locus in &map.loci {
        if !locus.in_range() {
            return Err(DataError::LocusPosition);
        }
        if previous.is_some_and(|before| before >= locus.position()) {
            return Err(DataError::LocusOrder);
        }
        previous = Some(locus.position());
        if ids.insert(locus.id.clone(), ()).is_some() {
            return Err(DataError::LocusId);
        }
        if !locus.marked() {
            return Err(DataError::LocusMark);
        }
        if let Some(weights) = locus.action.weights() {
            if locus.chromosome != Chromosome::Autosome {
                return Err(DataError::LocusAdditive);
            }
            check_weights(weights, &map.traits)?;
        }
    }
    if !keys_unique(map.loci.iter().map(|locus| &locus.id), &key) {
        return Err(DataError::LocusKey);
    }
    let all = map.loci.iter().map(|locus| &locus.id);
    if !keys_unique(all.chain(map.traits.iter().map(|t| &t.id)), &key) {
        return Err(DataError::TraitKey);
    }
    for t in &map.traits {
        // A locus counts once in a trait: not directly and through an include as well.
        let twice = map
            .loci
            .iter()
            .filter_map(|locus| locus.action.weights())
            .any(|weights| {
                let on = |id: &DataId| weights.loads.iter().any(|load| load.target == *id);
                on(&t.id) && t.includes.iter().any(|include| on(&include.target))
            });
        if twice {
            return Err(DataError::TraitInclude);
        }
        let loaded = map
            .loci
            .iter()
            .filter_map(|locus| locus.action.weights())
            .any(|weights| weights.loads.iter().any(|load| load.target == t.id));
        if !loaded && t.includes.is_empty() {
            return Err(DataError::TraitUnused);
        }
        check_bounds(map, t)?;
    }
    Ok(())
}

/// The trait with this data ID; the traits are in the order of their IDs.
fn find<'a>(traits: &'a [Trait], id: &DataId) -> Option<&'a Trait> {
    traits
        .binary_search_by(|t| t.id.cmp(id))
        .ok()
        .and_then(|index| traits.get(index))
}

/// Order, reference standard deviation and includes of the traits (ADR-0008 section 1).
fn check_traits(traits: &[Trait]) -> Result<(), DataError> {
    for pair in traits.windows(2) {
        if let [before, after] = pair
            && before.id >= after.id
        {
            return Err(DataError::TraitOrder);
        }
    }
    for t in traits {
        if t.sigma_ref.get() <= 0.0 {
            return Err(DataError::TraitSigma);
        }
        // Includes in ascending order of their IDs (ADR-0003 2.3), so none twice.
        for pair in t.includes.windows(2) {
            if let [before, after] = pair
                && before.target >= after.target
            {
                return Err(DataError::TraitInclude);
            }
        }
        for include in &t.includes {
            let fits = find(traits, &include.target)
                .is_some_and(|target| target.id != t.id && target.includes.is_empty());
            if !fits || include.weight.get() == 0.0 {
                return Err(DataError::TraitInclude);
            }
            if find(traits, &include.target).is_some_and(|target| target.exclusive) {
                return Err(DataError::LoadExclusive);
            }
        }
    }
    Ok(())
}

/// The values of an additive or threshold locus (ADR-0008 section 2).
fn check_weights(weights: &Weights, traits: &[Trait]) -> Result<(), DataError> {
    let d_ref = weights.d_ref.get();
    let distinct = match weights.loads.as_slice() {
        [_] => true,
        [first, second] => first.target != second.target,
        _ => false,
    };
    if weights.effect.get() <= 0.0
        || !(0.0..=2.0).contains(&d_ref)
        || !distinct
        || weights.loads.iter().any(|load| load.weight.get() == 0.0)
    {
        return Err(DataError::LocusWeights);
    }
    let mut exclusive = false;
    for load in &weights.loads {
        let target = find(traits, &load.target).ok_or(DataError::LocusLoad)?;
        exclusive |= target.exclusive;
    }
    if exclusive && weights.loads.len() > 1 {
        return Err(DataError::LoadExclusive);
    }
    Ok(())
}

/// One term of the raw value of a trait (ADR-0008 section 3): the locus, its coefficient
/// `λ · β` (or `(w · λ) · β` through an include) and its reference dose.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Term {
    pub(crate) locus: usize,
    pub(crate) coefficient: f64,
    pub(crate) d_ref: f64,
}

/// The terms of the loci that load on `target`, in map order; through an include, the
/// weight `include` multiplies each load first.
fn terms_on<'a>(
    map: &'a GenomeMap,
    target: &'a DataId,
    include: Option<f64>,
) -> impl Iterator<Item = Term> + 'a {
    (0..).zip(&map.loci).filter_map(move |(index, locus)| {
        let weights = locus.action.weights()?;
        let load = weights.loads.iter().find(|load| load.target == *target)?;
        let lambda = include.map_or(load.weight.get(), |w| w * load.weight.get());
        Some(Term {
            locus: index,
            coefficient: lambda * weights.effect.get(),
            d_ref: weights.d_ref.get(),
        })
    })
}

/// The terms of trait `t` in the order of its fold: the loci loading on it in map order,
/// then for each include in its order the loci loading on the included trait.
pub(crate) fn terms(map: &GenomeMap, t: &Trait) -> Vec<Term> {
    let mut all: Vec<Term> = terms_on(map, &t.id, None).collect();
    for include in &t.includes {
        all.extend(terms_on(map, &include.target, Some(include.weight.get())));
    }
    all
}

/// The largest magnitude the genetic value of `t` can reach, and that it is finite for
/// every genome (ADR-0008 section 3): every term
/// is finite at the doses 0, 1 and 2, every term of a load is not 0 at one of them, and
/// the fold of their largest magnitudes is finite and stays finite divided by
/// `sigma_ref`.
fn bound(map: &GenomeMap, t: &Trait) -> Result<f64, DataError> {
    // The terms of the trait's own loads come first (see `terms`); only they must not
    // vanish. An include term of 0 adds nothing and is finite.
    let direct = terms_on(map, &t.id, None).count();
    let mut largest = 0.0;
    for (position, term) in terms(map, t).into_iter().enumerate() {
        let at = [0.0, 1.0, 2.0].map(|dose| term.coefficient * (dose - term.d_ref));
        let vanishes = position < direct && at.iter().all(|c| *c == 0.0);
        if !at.iter().all(|c| c.is_finite()) || vanishes {
            return Err(DataError::TraitBounds);
        }
        largest += at.iter().fold(
            0.0,
            |most: f64, c| if c.abs() > most { c.abs() } else { most },
        );
    }
    let scaled = largest / t.sigma_ref.get();
    if !largest.is_finite() || !scaled.is_finite() {
        return Err(DataError::TraitBounds);
    }
    Ok(scaled)
}

fn check_bounds(map: &GenomeMap, t: &Trait) -> Result<(), DataError> {
    bound(map, t).map(|_| ())
}

/// No two IDs share a key (ADR-0004 section 2: a collision is a load error); `key` is
/// `str_key` outside the tests.
fn keys_unique<'a>(ids: impl Iterator<Item = &'a DataId>, key: impl Fn(&DataId) -> u64) -> bool {
    let mut seen = BTreeMap::new();
    ids.into_iter().all(|id| seen.insert(key(id), ()).is_none())
}

/// The calibration values the core reads, with their units; nothing else is allowed.
fn read_genetics(calibration: &GeneticsCalibration) -> Result<Genetics, DataError> {
    if calibration.format_version != CALIBRATION_FORMAT_VERSION {
        return Err(DataError::FormatVersion);
    }
    let value = |key, unit, limit: f64| -> Result<f64, DataError> {
        let id = DataId::new(key).unwrap_or_else(|_| panic!("invariant data.key"));
        let entry = calibration
            .values
            .get(&id)
            .ok_or(DataError::CalibrationMissing)?;
        if entry.unit != unit {
            return Err(DataError::CalibrationUnit);
        }
        let x = entry.value.get();
        if !(x > 0.0 && x <= limit) {
            return Err(DataError::CalibrationRange);
        }
        Ok(x)
    };
    // The ranges of the death math (`mortality`): inside them every age at death is finite.
    let in_range = |x: f64, (low, high): (f64, f64)| {
        if x >= low && x <= high {
            Ok(x)
        } else {
            Err(DataError::CalibrationRange)
        }
    };
    let gompertz = |a, b| -> Result<Gompertz, DataError> {
        Ok(Gompertz {
            a: in_range(
                value(a, Unit::PerYear, GOMPERTZ_A_RANGE.1)?,
                GOMPERTZ_A_RANGE,
            )?,
            b: in_range(
                value(b, Unit::PerYear, GOMPERTZ_B_RANGE.1)?,
                GOMPERTZ_B_RANGE,
            )?,
        })
    };
    let gamma = value("core:growth_gamma", Unit::CentimeterPerYear, MAX_HEIGHT)?;
    let nutrition = NutritionLoss {
        per_deficit_year: value("core:nutrition_loss", Unit::CentimeterPerYear, MAX_HEIGHT)?,
        weight_infant: value("core:nutrition_weight_infant", Unit::Fraction, 1.0)?,
        weight_child: value("core:nutrition_weight_child", Unit::Fraction, 1.0)?,
        weight_puberty: value("core:nutrition_weight_puberty", Unit::Fraction, 1.0)?,
        age_infant: value("core:nutrition_age_infant", Unit::Year, MAX_THETA)?,
        age_child: value("core:nutrition_age_child", Unit::Year, MAX_THETA)?,
        puberty_before: value(
            "core:nutrition_puberty_before",
            Unit::Year,
            MAX_PUBERTY_REACH,
        )?,
        puberty_after: value(
            "core:nutrition_puberty_after",
            Unit::Year,
            MAX_PUBERTY_REACH,
        )?,
    };
    if nutrition.age_infant >= nutrition.age_child {
        return Err(DataError::CalibrationRange);
    }
    let puberty_half_width = value("core:puberty_half_width", Unit::Year, MAX_PUBERTY_REACH)?;
    // A logit may be 0 or negative (ADR-0015 section 1).
    let signed = |key, unit, limit: f64| -> Result<f64, DataError> {
        let id = DataId::new(key).unwrap_or_else(|_| panic!("invariant data.key"));
        let entry = calibration
            .values
            .get(&id)
            .ok_or(DataError::CalibrationMissing)?;
        if entry.unit != unit {
            return Err(DataError::CalibrationUnit);
        }
        let x = entry.value.get();
        if !(-limit..=limit).contains(&x) {
            return Err(DataError::CalibrationRange);
        }
        Ok(x)
    };
    let body = BodyModel {
        pigment_skin_offset: signed("core:body_pigment_skin_offset", Unit::Logit, MAX_LOGIT)?,
        pigment_skin_slope: signed("core:body_pigment_skin_slope", Unit::Logit, MAX_LOGIT)?,
        pigment_hair_slope: signed("core:body_pigment_hair_slope", Unit::Logit, MAX_LOGIT)?,
        pigment_hair_residual: signed("core:body_pigment_hair_residual", Unit::Logit, MAX_LOGIT)?,
        pigment_hair_darkening: signed("core:body_pigment_hair_darkening", Unit::Logit, MAX_LOGIT)?,
        pigment_hair_darkening_age: value(
            "core:body_pigment_hair_darkening_age",
            Unit::Year,
            MAX_AGE,
        )?,
        pigment_iris_offset: signed("core:body_pigment_iris_offset", Unit::Logit, MAX_LOGIT)?,
        pigment_iris_slope: signed("core:body_pigment_iris_slope", Unit::Logit, MAX_LOGIT)?,
        pigment_iris_darkening: signed("core:body_pigment_iris_darkening", Unit::Logit, MAX_LOGIT)?,
        pigment_iris_darkening_age: value(
            "core:body_pigment_iris_darkening_age",
            Unit::Year,
            MAX_AGE,
        )?,
        pigment_skin_pheo_offset: signed(
            "core:body_pigment_skin_pheo_offset",
            Unit::Logit,
            MAX_LOGIT,
        )?,
        pigment_skin_pheo_slope: signed(
            "core:body_pigment_skin_pheo_slope",
            Unit::Logit,
            MAX_LOGIT,
        )?,
        pigment_hair_pheo_offset: signed(
            "core:body_pigment_hair_pheo_offset",
            Unit::Logit,
            MAX_LOGIT,
        )?,
        pigment_hair_pheo_slope: signed(
            "core:body_pigment_hair_pheo_slope",
            Unit::Logit,
            MAX_LOGIT,
        )?,
        mc1r_hair_eu: value("core:body_mc1r_hair_eu", Unit::Fraction, 1.0)?,
        mc1r_skin_eu: value("core:body_mc1r_skin_eu", Unit::Fraction, 1.0)?,
        mc1r_hair_pheo: value("core:body_mc1r_hair_pheo", Unit::Fraction, 1.0)?,
        mc1r_skin_pheo: value("core:body_mc1r_skin_pheo", Unit::Fraction, 1.0)?,
        mc1r_freckle: signed("core:body_mc1r_freckle", Unit::Logit, MAX_LOGIT)?,
        herc2_iris: value("core:body_herc2_iris", Unit::Fraction, 1.0)?,
        development_androgen_lag: value("core:body_development_androgen_lag", Unit::Year, MAX_AGE)?,
        development_estrogen_lead: value(
            "core:body_development_estrogen_lead",
            Unit::Year,
            MAX_AGE,
        )?,
        development_breast_lead: value("core:body_development_breast_lead", Unit::Year, MAX_AGE)?,
        development_tempo: value("core:body_development_tempo", Unit::Year, MAX_AGE)?,
        development_androgen_offset: signed(
            "core:body_development_androgen_offset",
            Unit::Logit,
            MAX_LOGIT,
        )?,
        development_estrogen_offset: signed(
            "core:body_development_estrogen_offset",
            Unit::Logit,
            MAX_LOGIT,
        )?,
        development_estrogen_men: value("core:body_development_estrogen_men", Unit::Fraction, 1.0)?,
        development_androgen_women: value(
            "core:body_development_androgen_women",
            Unit::Fraction,
            1.0,
        )?,
        curl_offset: signed("core:body_curl_offset", Unit::Logit, MAX_LOGIT)?,
        curl_slope: signed("core:body_curl_slope", Unit::Logit, MAX_LOGIT)?,
        freckle_offset: signed("core:body_freckle_offset", Unit::Logit, MAX_LOGIT)?,
        freckle_slope: signed("core:body_freckle_slope", Unit::Logit, MAX_LOGIT)?,
        freckle_pigment: signed("core:body_freckle_pigment", Unit::Logit, MAX_LOGIT)?,
        beard_lag: value("core:body_beard_lag", Unit::Year, MAX_AGE)?,
        beard_tempo: value("core:body_beard_tempo", Unit::Year, MAX_AGE)?,
        beard_offset: signed("core:body_beard_offset", Unit::Logit, MAX_LOGIT)?,
        beard_androgen: signed("core:body_beard_androgen", Unit::Logit, MAX_LOGIT)?,
        body_hair_lag: value("core:body_body_hair_lag", Unit::Year, MAX_AGE)?,
        body_hair_tempo: value("core:body_body_hair_tempo", Unit::Year, MAX_AGE)?,
        body_hair_androgen: signed("core:body_body_hair_androgen", Unit::Logit, MAX_LOGIT)?,
        body_hair_women: value("core:body_body_hair_women", Unit::Fraction, 1.0)?,
        bald_men_stage: value("core:body_bald_men_stage", Unit::Stage, MAX_STAGE)?,
        bald_men_center: value("core:body_bald_men_center", Unit::Year, MAX_AGE)?,
        bald_men_slope: value("core:body_bald_men_slope", Unit::Year, MAX_AGE)?,
        bald_men_androgen: value("core:body_bald_men_androgen", Unit::Year, MAX_AGE)?,
        bald_men_tempo: value("core:body_bald_men_tempo", Unit::Year, MAX_AGE)?,
        bald_women_stage: value("core:body_bald_women_stage", Unit::Stage, MAX_STAGE)?,
        bald_women_center: value("core:body_bald_women_center", Unit::Year, MAX_AGE)?,
        bald_women_slope: value("core:body_bald_women_slope", Unit::Year, MAX_AGE)?,
        bald_women_tempo: value("core:body_bald_women_tempo", Unit::Year, MAX_AGE)?,
        hair_density_offset: signed("core:body_hair_density_offset", Unit::Logit, MAX_LOGIT)?,
        hair_density_loss: value("core:body_hair_density_loss", Unit::Fraction, 1.0)?,
        hair_density_center: value("core:body_hair_density_center", Unit::Year, MAX_AGE)?,
        hair_density_tempo: value("core:body_hair_density_tempo", Unit::Year, MAX_AGE)?,
        grey_center: value("core:body_grey_center", Unit::Year, MAX_AGE)?,
        grey_slope: value("core:body_grey_slope", Unit::Year, MAX_AGE)?,
        grey_tempo: value("core:body_grey_tempo", Unit::Year, MAX_AGE)?,
        wrinkles_center: value("core:body_wrinkles_center", Unit::Year, MAX_AGE)?,
        wrinkles_slope: value("core:body_wrinkles_slope", Unit::Year, MAX_AGE)?,
        wrinkles_tempo: value("core:body_wrinkles_tempo", Unit::Year, MAX_AGE)?,
        laxity_center: value("core:body_laxity_center", Unit::Year, MAX_AGE)?,
        laxity_slope: value("core:body_laxity_slope", Unit::Year, MAX_AGE)?,
        laxity_tempo: value("core:body_laxity_tempo", Unit::Year, MAX_AGE)?,
        spots_center: value("core:body_spots_center", Unit::Year, MAX_AGE)?,
        spots_skin: value("core:body_spots_skin", Unit::Year, MAX_AGE)?,
        spots_pigment: value("core:body_spots_pigment", Unit::Year, MAX_AGE)?,
        spots_tempo: value("core:body_spots_tempo", Unit::Year, MAX_AGE)?,
        stoop_center: value("core:body_stoop_center", Unit::Year, MAX_AGE)?,
        stoop_skin: value("core:body_stoop_skin", Unit::Year, MAX_AGE)?,
        stoop_tempo: value("core:body_stoop_tempo", Unit::Year, MAX_AGE)?,
    };
    let bio_age_weight = value("core:bio_age_weight", Unit::Fraction, 1.0)?;
    let height_loss = HeightLoss {
        onset: value("core:height_loss_onset", Unit::Year, MAX_AGE)?,
        male: value("core:height_loss_male", Unit::CentimeterPerYearSquared, 1.0)?,
        female: value(
            "core:height_loss_female",
            Unit::CentimeterPerYearSquared,
            1.0,
        )?,
    };
    let genetics = Genetics {
        map_length_autosome_female: Morgans(value(
            "core:map_length_autosome_female",
            Unit::Morgan,
            MAX_MAP_LENGTH,
        )?),
        map_length_autosome_male: Morgans(value(
            "core:map_length_autosome_male",
            Unit::Morgan,
            MAX_MAP_LENGTH,
        )?),
        map_length_x_female: Morgans(value(
            "core:map_length_x_female",
            Unit::Morgan,
            MAX_MAP_LENGTH,
        )?),
        mutation_rate: PerTransmission(value("core:mutation_rate", Unit::PerTransmission, 1.0)?),
        mutation_rate_disease: PerTransmission(value(
            "core:mutation_rate_disease",
            Unit::PerTransmission,
            1.0,
        )?),
        growth_male: growth([
            value("core:height_adult_male", Unit::Centimeter, MAX_HEIGHT)?,
            value("core:growth_theta_male", Unit::Year, MAX_THETA)?,
            value(
                "core:growth_height_theta_male",
                Unit::Centimeter,
                MAX_HEIGHT,
            )?,
            value("core:growth_s0_male", Unit::PerYear, MAX_RATE_PER_YEAR)?,
            value("core:growth_s1_male", Unit::PerYear, MAX_RATE_PER_YEAR)?,
            value("core:length_birth_male", Unit::Centimeter, MAX_HEIGHT)?,
            value("core:length_12_months_male", Unit::Centimeter, MAX_HEIGHT)?,
            value("core:length_24_months_male", Unit::Centimeter, MAX_HEIGHT)?,
            value("core:height_sd_male", Unit::Centimeter, MAX_HEIGHT)?,
            value("core:growth_theta_sd_male", Unit::Year, MAX_THETA_SD)?,
            gamma,
        ])?,
        growth_female: growth([
            value("core:height_adult_female", Unit::Centimeter, MAX_HEIGHT)?,
            value("core:growth_theta_female", Unit::Year, MAX_THETA)?,
            value(
                "core:growth_height_theta_female",
                Unit::Centimeter,
                MAX_HEIGHT,
            )?,
            value("core:growth_s0_female", Unit::PerYear, MAX_RATE_PER_YEAR)?,
            value("core:growth_s1_female", Unit::PerYear, MAX_RATE_PER_YEAR)?,
            value("core:length_birth_female", Unit::Centimeter, MAX_HEIGHT)?,
            value("core:length_12_months_female", Unit::Centimeter, MAX_HEIGHT)?,
            value("core:length_24_months_female", Unit::Centimeter, MAX_HEIGHT)?,
            value("core:height_sd_female", Unit::Centimeter, MAX_HEIGHT)?,
            value("core:growth_theta_sd_female", Unit::Year, MAX_THETA_SD)?,
            gamma,
        ])?,
        nutrition,
        puberty_half_width,
        height_loss,
        body,
        bio_age_weight,
        sex_ratio_at_birth: value("core:sex_ratio_at_birth", Unit::Ratio, MAX_SEX_RATIO)?,
        senescence_female: gompertz("core:senescence_a_female", "core:senescence_b_female")?,
        senescence_male: gompertz("core:senescence_a_male", "core:senescence_b_male")?,
        accident_female: in_range(
            value("core:accident_female", Unit::PerYear, ACCIDENT_RANGE.1)?,
            ACCIDENT_RANGE,
        )?,
        accident_male: in_range(
            value("core:accident_male", Unit::PerYear, ACCIDENT_RANGE.1)?,
            ACCIDENT_RANGE,
        )?,
        longevity_sigma_g: value("core:longevity_sigma_g", Unit::Logit, MAX_SIGMA_G)?,
        fertility: read_fertility(&calibration.fertility)?,
    };
    if calibration.values.len() != 117 {
        return Err(DataError::CalibrationUnknown);
    }
    Ok(genetics)
}

/// The curves of fertility, checked (ADR-0017 section 1): each number in its unit and
/// range, the bounds of the menarche and the ages of each decline in order.
fn read_fertility(curves: &FertilityCurves) -> Result<Curves, DataError> {
    let number = |entry: &Calibrated, unit, (low, high): (f64, f64)| {
        if entry.unit != unit {
            return Err(DataError::CalibrationUnit);
        }
        let x = entry.value.get();
        if (low..=high).contains(&x) {
            Ok(x)
        } else {
            Err(DataError::CalibrationRange)
        }
    };
    let positive = |entry: &Calibrated, high: f64| {
        let x = number(entry, Unit::Year, (0.0, high))?;
        if x > 0.0 {
            Ok(x)
        } else {
            Err(DataError::CalibrationRange)
        }
    };
    let ordered = |from: f64, to: f64| {
        if from > 0.0 && from < to && to <= MAX_AGE {
            Ok((from, to))
        } else {
            Err(DataError::CalibrationRange)
        }
    };
    match curves {
        FertilityCurves::Puberty(p) => {
            let year = |entry| number(entry, Unit::Year, (-MAX_AGE, MAX_AGE));
            let offset = |entry| number(entry, Unit::Year, (-10.0, 10.0));
            let (menarche_low, menarche_high) =
                (offset(&p.menarche_low)?, offset(&p.menarche_high)?);
            if menarche_low >= menarche_high {
                return Err(DataError::CalibrationRange);
            }
            let (female_decline_from, female_decline_to) =
                ordered(year(&p.female_decline_from)?, year(&p.female_decline_to)?)?;
            let (male_decline_from, male_decline_to) =
                ordered(year(&p.male_decline_from)?, year(&p.male_decline_to)?)?;
            Ok(Curves::Puberty(Puberty {
                menarche_mean: offset(&p.menarche_mean)?,
                menarche_sd: positive(&p.menarche_sd, 10.0)?,
                menarche_low,
                menarche_high,
                female_ramp: positive(&p.female_ramp, 20.0)?,
                female_decline_from,
                female_decline_to,
                male_lead: number(&p.male_lead, Unit::Year, (0.0, 10.0))?,
                male_ramp: positive(&p.male_ramp, 20.0)?,
                male_decline_from,
                male_floor: number(&p.male_floor, Unit::Fraction, (0.0, 1.0))?,
                male_decline_to,
            }))
        }
        FertilityCurves::Window(w) => {
            let from = number(&w.from, Unit::Year, (0.0, MAX_AGE))?;
            let until = number(&w.until, Unit::Year, (0.0, MAX_AGE))?;
            if from < until {
                Ok(Curves::Window { from, until })
            } else {
                Err(DataError::CalibrationRange)
            }
        }
    }
}

/// The mean growth curve of one sex from its eight values in the order of
/// `GrowthCurve` (ADR-0010): their order, and `b` and `c` of the infancy term from the
/// three medians; the target height at the lower corner of the bounds is above 0
/// (ADR-0012 section 2).
fn growth(values: [f64; 11]) -> Result<GrowthCurve, DataError> {
    let [
        h1,
        theta,
        h_theta,
        s0,
        s1,
        l0,
        l12,
        l24,
        sd,
        theta_sd,
        gamma,
    ] = values;
    let ordered = l0 < l12 && l12 < l24 && l24 - l12 < l12 - l0;
    if !ordered || h_theta >= h1 || s0 >= s1 {
        return Err(DataError::CalibrationRange);
    }
    let q = (l24 - l12) / (l12 - l0);
    let curve = GrowthCurve {
        h1,
        theta,
        h_theta,
        s0,
        s1,
        l0,
        l12,
        l24,
        b: (l12 - l0) / (1.0 - q),
        c: -sim_math::ln(q),
        sd,
        theta_sd,
        gamma,
    };
    let bound = crate::growth::CURVE_INPUT_BOUND;
    let corner = crate::growth::IndividualCurve::new(curve, -bound, -bound);
    if corner.target > 0.0 {
        Ok(curve)
    } else {
        Err(DataError::CalibrationRange)
    }
}

/// The phenotype weights (ADR-0009 section 2): each row names a trait of the map, the rows
/// stand in ascending order, every weight is a fraction from 0 to 1, and heritability
/// plus shared environment is at most 1 for the child and for the adult.
/// The bound of the genetic value of a trait with a phenotype: 2^1022, so that weights up
/// to 1 and two bounded draws keep the phenotype finite (ADR-0009 section 2).
const PHENOTYPE_BOUND: f64 = f64::from_bits(2045_u64 << 52);

fn check_weights_of_traits(rows: &[PhenotypeWeights], map: &GenomeMap) -> Result<(), DataError> {
    for pair in rows.windows(2) {
        if let [before, after] = pair
            && before.target >= after.target
        {
            return Err(DataError::CalibrationUnknown);
        }
    }
    for row in rows {
        let t = find(&map.traits, &row.target).ok_or(DataError::CalibrationUnknown)?;
        let all = [&row.h2_child, &row.c2_child, &row.h2_adult, &row.c2_adult];
        if all.iter().any(|weight| weight.unit != Unit::Fraction) {
            return Err(DataError::CalibrationUnit);
        }
        let in_range = all
            .iter()
            .all(|weight| (0.0..=1.0).contains(&weight.value.get()));
        let child = row.h2_child.value.get() + row.c2_child.value.get();
        let adult = row.h2_adult.value.get() + row.c2_adult.value.get();
        if !in_range || child > 1.0 || adult > 1.0 {
            return Err(DataError::CalibrationRange);
        }
        if bound(map, t)? > PHENOTYPE_BOUND {
            return Err(DataError::TraitBounds);
        }
    }
    Ok(())
}

/// The sex differences (ADR-0014 section 1): each row names a trait with a phenotype row,
/// the rows stand in ascending order, the shift is in standard deviations from −10 to 10
/// and the variance ratio a ratio above 0 and at most 10.
fn check_sex_differences(
    rows: &[SexDifference],
    phenotypes: &[PhenotypeWeights],
) -> Result<(), DataError> {
    for pair in rows.windows(2) {
        if let [before, after] = pair
            && before.target >= after.target
        {
            return Err(DataError::CalibrationUnknown);
        }
    }
    for row in rows {
        if !phenotypes.iter().any(|p| p.target == row.target) {
            return Err(DataError::CalibrationUnknown);
        }
        if row.shift.unit != Unit::StandardDeviation || row.variance_ratio.unit != Unit::Ratio {
            return Err(DataError::CalibrationUnit);
        }
        let (d, vr) = (row.shift.value.get(), row.variance_ratio.value.get());
        if !((-MAX_SHIFT..=MAX_SHIFT).contains(&d) && vr > 0.0 && vr <= MAX_VARIANCE_RATIO) {
            return Err(DataError::CalibrationRange);
        }
    }
    Ok(())
}

/// The value of a disease number with its unit; `data.calibration_unit` for another unit.
fn disease_number(c: &Calibrated, unit: Unit) -> Result<f64, DataError> {
    if c.unit != unit {
        return Err(DataError::CalibrationUnit);
    }
    Ok(c.value.get())
}

/// The diseases (ADR-0017 section 1): codes with unique keys (the occasion of their
/// draws), none of them a cause of death for everyone; a carrier locus of the map, marked disease, recessive or dominant, at most one
/// disease per locus; a named trait of liability is in the map, carries the code of its
/// disease and has a phenotype row with `h²` the disease's heritability and `c²` 0, equal
/// in both ages (ADR-0020 section 2); penetrances in [0, 1], a
/// heritability in (0, 1), a prevalence in (0, 0.5]; ages from 0 to 130 with a positive
/// spread and ordered bounds; hazards in the ranges of `mortality`, a death with some.
fn check_diseases(
    diseases: &[Disease],
    map: &GenomeMap,
    weights: &[PhenotypeWeights],
    key: impl Fn(&DataId) -> u64,
) -> Result<(), DataError> {
    use crate::mortality::{CHANGE_AGE_RANGE, DISEASE_HAZARD_RANGE};
    let fraction = |x: f64| (0.0..=1.0).contains(&x);
    let age = |x: f64| (0.0..=CHANGE_AGE_RANGE.1).contains(&x);
    let hazard = |x: f64| (DISEASE_HAZARD_RANGE.0..=DISEASE_HAZARD_RANGE.1).contains(&x);
    let fits = |ok: bool| if ok { Ok(()) } else { Err(DataError::Disease) };
    // The causes of death for everyone come first in the list of causes (ADR-0017
    // section 2) and share the budget address by key: no disease may take their names.
    let reserved = ["core:senescence", "core:accident"]
        .map(|code| DataId::new(code).unwrap_or_else(|_| panic!("invariant data.key")));
    fits(keys_unique(
        reserved.iter().chain(diseases.iter().map(|d| &d.code)),
        key,
    ))?;
    let mut carriers = BTreeMap::new();
    for disease in diseases {
        match &disease.carrier {
            Carrier::Locus {
                locus,
                penetrance_female,
                penetrance_male,
            } => {
                let found = map
                    .loci
                    .iter()
                    .find(|l| &l.id == locus)
                    .ok_or(DataError::Disease)?;
                let mendelian = matches!(found.action, Action::Recessive | Action::Dominant);
                fits(found.mark == Mark::Disease && mendelian)?;
                fits(carriers.insert(locus.clone(), ()).is_none())?;
                let female = disease_number(penetrance_female, Unit::Fraction)?;
                let male = disease_number(penetrance_male, Unit::Fraction)?;
                fits(fraction(female) && fraction(male))?;
            }
            Carrier::Threshold {
                liability,
                heritability,
                prevalence,
            } => {
                let h2 = disease_number(heritability, Unit::Fraction)?;
                let k = disease_number(prevalence, Unit::Fraction)?;
                fits(h2 > 0.0 && h2 < 1.0 && k > 0.0 && k <= 0.5)?;
                if let Some(t) = liability {
                    fits(map.traits.iter().any(|known| &known.id == t))?;
                    fits(t == &disease.code)?;
                    let row = weights
                        .iter()
                        .find(|row| &row.target == t)
                        .ok_or(DataError::Disease)?;
                    let bits = |w: &Calibrated| w.value.get().to_bits();
                    fits(bits(&row.h2_child) == bits(&row.h2_adult))?;
                    fits(bits(&row.h2_child) == heritability.value.get().to_bits())?;
                    fits(bits(&row.c2_child) == 0 && bits(&row.c2_adult) == 0)?;
                }
            }
        }
        match &disease.onset {
            Onset::Fixed { age: at } => fits(age(disease_number(at, Unit::Year)?))?,
            Onset::Normal {
                mean,
                sd,
                low,
                high,
            } => {
                let mean = disease_number(mean, Unit::Year)?;
                let sd = disease_number(sd, Unit::Year)?;
                let low = disease_number(low, Unit::Year)?;
                let high = disease_number(high, Unit::Year)?;
                fits(age(mean) && age(low) && age(high) && sd > 0.0 && age(sd) && low <= high)?;
            }
        }
        match &disease.effect {
            Effect::Death { per_year } => fits(hazard(disease_number(per_year, Unit::PerYear)?))?,
            Effect::DeathInTwoSteps {
                before,
                change_age,
                after,
            } => {
                let h0 = disease_number(before, Unit::PerYear)?;
                let s = disease_number(change_age, Unit::Year)?;
                let h1 = disease_number(after, Unit::PerYear)?;
                let step = |h: f64| h == 0.0 || hazard(h);
                fits(step(h0) && step(h1) && (h0 > 0.0 || h1 > 0.0) && s > 0.0 && age(s))?;
            }
            Effect::Visible => {}
        }
    }
    Ok(())
}

fn encode<T: Serialize>(value: &T) -> Vec<u8> {
    postcard::to_allocvec(value).unwrap_or_else(|_| panic!("invariant data.encode"))
}

impl Data {
    /// Checks the definitions and computes their data hash (ADR-0007).
    pub fn new(definitions: Definitions) -> Result<Data, DataError> {
        let demography = read_demography(&definitions.demography)?;
        let genetics = read_genetics(&definitions.calibration)?;
        check_map(&definitions.genome_map, str_key)?;
        check_weights_of_traits(&definitions.calibration.traits, &definitions.genome_map)?;
        check_sex_differences(
            &definitions.calibration.sex_differences,
            &definitions.calibration.traits,
        )?;
        check_diseases(
            &definitions.calibration.diseases,
            &definitions.genome_map,
            &definitions.calibration.traits,
            str_key,
        )?;
        let named = [
            &definitions.calibration.longevity_trait,
            &definitions.calibration.puberty_trait,
            &definitions.calibration.cognition_trait,
            &definitions.calibration.height_trait,
        ];
        for t in named.into_iter().flatten() {
            if !definitions
                .genome_map
                .traits
                .iter()
                .any(|known| &known.id == t)
            {
                return Err(DataError::TraitNamed);
            }
        }
        // The market reads the phenotypes of the traits it compares (11.2).
        let compared = [
            &definitions.calibration.cognition_trait,
            &definitions.calibration.height_trait,
        ];
        for t in compared.into_iter().flatten() {
            if !definitions
                .calibration
                .traits
                .iter()
                .any(|row| &row.target == t)
            {
                return Err(DataError::TraitNamed);
            }
        }
        let hash = stable_hash64(&encode(&definitions));
        let loci = (0..)
            .zip(&definitions.genome_map.loci)
            .map(|(index, locus)| (locus.id.clone(), index))
            .collect();
        let map = &definitions.genome_map;
        let traits = map
            .traits
            .iter()
            .map(|t| (t.id.clone(), (terms(map, t), t.sigma_ref.get())))
            .collect();
        let phenotypes = definitions
            .calibration
            .traits
            .iter()
            .map(|row| {
                let weights = [&row.h2_child, &row.c2_child, &row.h2_adult, &row.c2_adult];
                (row.target.clone(), weights.map(|w| w.value.get()))
            })
            .collect();
        let sex_differences = definitions
            .calibration
            .sex_differences
            .iter()
            .map(|row| {
                let pair = (row.shift.value.get(), row.variance_ratio.value.get());
                (row.target.clone(), pair)
            })
            .collect();
        let thresholds = definitions
            .calibration
            .diseases
            .iter()
            .filter_map(|disease| match &disease.carrier {
                Carrier::Threshold {
                    liability: Some(_),
                    prevalence,
                    ..
                } => Some((
                    disease.code.clone(),
                    sim_math::probit(1.0 - prevalence.value.get()),
                )),
                _ => None,
            })
            .collect();
        Ok(Data {
            definitions,
            demography,
            genetics,
            hash,
            loci,
            traits,
            phenotypes,
            sex_differences,
            thresholds,
        })
    }

    /// The threshold `T` of a threshold disease with a trait of liability (ADR-0020 section
    /// 2); `None` for any other disease.
    pub fn threshold(&self, disease: &DataId) -> Option<f64> {
        self.thresholds.get(disease).copied()
    }

    /// The phenotype weights of a trait, if it has a phenotype (ADR-0009 section 1): h2
    /// and c2 of the child, then of the adult.
    pub fn phenotype_weights(&self, id: &DataId) -> Option<[f64; 4]> {
        self.phenotypes.get(id).copied()
    }

    /// The shift and the variance ratio of a trait with a sex difference (ADR-0014);
    /// `None` for a trait without one, which has none.
    pub fn sex_difference(&self, id: &DataId) -> Option<(f64, f64)> {
        self.sex_differences.get(id).copied()
    }

    /// The terms of a trait in the order of its fold and its `sigma_ref`, if the map has
    /// the trait (ADR-0008 section 3).
    pub(crate) fn trait_terms(&self, id: &DataId) -> Option<(&[Term], f64)> {
        let (terms, sigma) = self.traits.get(id)?;
        Some((terms, *sigma))
    }

    /// The trait of the puberty timing, if the data name one (genetics specification 5.2,
    /// 11.1).
    pub fn puberty_trait(&self) -> Option<&DataId> {
        self.definitions.calibration.puberty_trait.as_ref()
    }

    /// The trait of longevity, if the data name one (genetics specification 7.2, 11.1).
    pub fn longevity_trait(&self) -> Option<&DataId> {
        self.definitions.calibration.longevity_trait.as_ref()
    }

    /// The traits the partner market compares, cognition and the target height, each if
    /// the data name it (genetics specification 11.2, 11.1).
    pub fn pairing_traits(&self) -> [Option<&DataId>; 2] {
        let calibration = &self.definitions.calibration;
        [
            calibration.cognition_trait.as_ref(),
            calibration.height_trait.as_ref(),
        ]
    }

    /// The hereditary diseases in the order of the data (ADR-0017 sections 1 and 2).
    pub fn diseases(&self) -> &[Disease] {
        &self.definitions.calibration.diseases
    }

    /// The data hash (ADR-0007 section 3).
    pub const fn hash(&self) -> u64 {
        self.hash
    }

    pub const fn genome_map(&self) -> &GenomeMap {
        &self.definitions.genome_map
    }

    pub const fn genetics(&self) -> Genetics {
        self.genetics
    }

    /// The checked values of the life course (ADR-0017 section 1).
    pub const fn demography(&self) -> &Demography {
        &self.demography
    }

    /// The locus with this data ID, if the map has it.
    pub fn locus(&self, id: &DataId) -> Option<&Locus> {
        let index = *self.loci.get(id)?;
        self.definitions.genome_map.loci.get(index)
    }
}

/// The definitions of the repository, read the way sim-cli reads them (ADR-0007 section
/// 2, ADR-0008 section 1): the text fields of each calibration value, trait and locus
/// removed, the rest into the core types.
#[cfg(test)]
pub(crate) mod testing {
    use serde_json::Value;

    use super::{Data, Definitions};

    /// Removes the text of each calibration value, trait and locus; each text must be
    /// there and not blank, as sim-cli demands.
    fn without_text(mut calibration: Value, mut genome_map: Value) -> (Value, Value) {
        let take = |entry: &mut Value, name| {
            let text = entry.as_object_mut().expect("entry").remove(name);
            let filled = text
                .as_ref()
                .and_then(Value::as_str)
                .is_some_and(|t| !t.trim().is_empty());
            assert!(filled, "text field missing or blank");
        };
        let values = calibration["values"].as_object_mut().expect("values");
        for value in values.values_mut() {
            for name in ["source", "as_of", "note"] {
                take(value, name);
            }
        }
        for row in calibration["traits"].as_array_mut().expect("traits") {
            for weight in ["h2_child", "c2_child", "h2_adult", "c2_adult"] {
                for name in ["source", "as_of", "note"] {
                    take(&mut row[weight], name);
                }
            }
        }
        for row in calibration["sex_differences"]
            .as_array_mut()
            .expect("sex_differences")
        {
            for number in ["shift", "variance_ratio"] {
                for name in ["source", "as_of", "note"] {
                    take(&mut row[number], name);
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
        for list in ["traits", "loci"] {
            for entry in genome_map[list].as_array_mut().expect("list") {
                take(entry, "description");
            }
        }
        (calibration, genome_map)
    }

    /// Removes the text of every number (an object with a unit) inside a disease; each
    /// text must be there and not blank.
    fn without_numbers(value: &mut Value) {
        match value {
            Value::Object(fields) if fields.contains_key("unit") => {
                for name in ["source", "as_of", "note"] {
                    let text = fields.remove(name);
                    let filled = text
                        .as_ref()
                        .and_then(Value::as_str)
                        .is_some_and(|t| !t.trim().is_empty());
                    assert!(filled, "text field missing or blank");
                }
            }
            Value::Object(fields) => fields.values_mut().for_each(without_numbers),
            Value::Array(items) => items.iter_mut().for_each(without_numbers),
            _ => {}
        }
    }

    pub(crate) fn data() -> Data {
        let read = |text| serde_json::from_str::<Value>(text).expect("json");
        let (calibration, genome_map) = without_text(
            read(include_str!("../../../data/calibration/genetics.json")),
            read(include_str!("../../../data/genetics/genome_map.json")),
        );
        let mut demography = read(include_str!("../../../data/calibration/demography.json"));
        for list in ["values", "fertility_wish_shape", "leave_home_ages"] {
            let entries: alloc::vec::Vec<&mut Value> = match &mut demography[list] {
                Value::Object(values) => values.values_mut().collect(),
                Value::Array(weights) => weights.iter_mut().collect(),
                _ => panic!("entries"),
            };
            for entry in entries {
                for name in ["source", "as_of", "note"] {
                    entry.as_object_mut().expect("entry").remove(name);
                }
            }
        }
        let definitions = Definitions {
            demography: serde_json::from_value(demography).expect("demography"),
            calibration: serde_json::from_value(calibration).expect("calibration"),
            genome_map: serde_json::from_value(genome_map).expect("genome map"),
        };
        Data::new(definitions).expect("checked")
    }

    /// The alleles of a founder at `core:disease_ar`: maternal, paternal; none for two
    /// reference alleles.
    pub(crate) fn dummy(
        maternal: u8,
        paternal: u8,
    ) -> alloc::vec::Vec<crate::genome::LocusAlleles> {
        if (maternal, paternal) == (0, 0) {
            return alloc::vec::Vec::new();
        }
        alloc::vec![crate::genome::LocusAlleles {
            locus: super::DataId::new("core:disease_ar").expect("id"),
            copies: alloc::vec![maternal, paternal],
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_loci_with_one_key_are_rejected() {
        let map = testing::data().genome_map().clone();
        assert_eq!(check_map(&map, str_key), Ok(()));
        let mut two = map.clone();
        let mut other = two.loci[0].clone();
        other.id = DataId::new("test:other").expect("id");
        other.slot = 1;
        // Next to the first locus, so the map stays in order.
        two.loci.insert(1, other);
        assert_eq!(check_map(&two, str_key), Ok(()));
        assert_eq!(check_map(&two, |_| 7), Err(DataError::LocusKey));
    }

    /// A trait may not share its key with a locus: its key is a draw occasion (ADR-0008).
    #[test]
    fn a_trait_with_the_key_of_a_locus_is_rejected() {
        let id = |text| DataId::new(text).expect("id");
        let weights = Weights {
            effect: FiniteF64::try_new(1.0).expect("finite"),
            d_ref: FiniteF64::try_new(1.0).expect("finite"),
            loads: alloc::vec![Load {
                target: id("test:t"),
                weight: FiniteF64::try_new(1.0).expect("finite"),
            }],
        };
        let mut map = testing::data().genome_map().clone();
        map.traits = alloc::vec![Trait {
            id: id("test:t"),
            sigma_ref: FiniteF64::try_new(1.0).expect("finite"),
            exclusive: false,
            includes: alloc::vec::Vec::new(),
        }];
        map.loci = alloc::vec![Locus {
            id: id("test:a"),
            chromosome: Chromosome::Autosome,
            segment: 0,
            slot: 0,
            action: Action::Additive(weights),
            mark: Mark::Small,
            mutation: MutationDirection::Both,
        }];
        assert_eq!(check_map(&map, str_key), Ok(()));
        assert_eq!(check_map(&map, |_| 7), Err(DataError::TraitKey));
        // Two traits with one key, apart from every locus.
        let mut second = map.traits[0].clone();
        second.id = id("test:u");
        map.traits.push(second);
        let mut other = map.loci[0].clone();
        other.id = id("test:b");
        other.slot = 1;
        if let Action::Additive(weights) = &mut other.action {
            weights.loads[0].target = id("test:u");
        }
        map.loci.push(other);
        assert_eq!(check_map(&map, str_key), Ok(()));
        let traits_collide = |x: &DataId| match x.as_str() {
            "test:t" | "test:u" => 7,
            _ => str_key(x),
        };
        assert_eq!(check_map(&map, traits_collide), Err(DataError::TraitKey));
    }

    #[test]
    fn colliding_keys_are_found() {
        let ids = [
            DataId::new("test:a").expect("id"),
            DataId::new("test:b").expect("id"),
        ];
        assert!(keys_unique(ids.iter(), str_key));
        assert!(!keys_unique(ids.iter(), |_| 7));
        assert!(keys_unique(ids[..1].iter(), |_| 7));
    }
}

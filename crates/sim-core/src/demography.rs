//! The data of the life course (ADR-0017 section 1): `data/calibration/demography.json`,
//! the definition set `core:calibration_demography`, with the values of fertility and
//! twins (genetics specification 11.3), the partner market (11.2), separation (ADR-0018
//! section 3) and learning (11.5). `Data::new` reads and checks it; the rules of the life
//! course read the checked values.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::data::{Calibrated, DataError, Unit};
use crate::data_id::DataId;
use crate::learning::LearningParams;

/// Format version of the life course data (ADR-0017 section 1).
pub const DEMOGRAPHY_FORMAT_VERSION: u32 = 2;

/// The file as the core reads it, without its text fields (ADR-0007 section 2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DemographyCalibration {
    pub format_version: u32,
    pub values: BTreeMap<DataId, Calibrated>,
    /// The weights `p_0 … p_n` of the wish for children (11.3).
    pub fertility_wish_shape: Vec<Calibrated>,
    /// The weights of the ages 16 to 25 at which a person leaves home (ADR-0018 section 4;
    /// Dario's table).
    pub leave_home_ages: Vec<Calibrated>,
}

/// The checked values of the life course.
#[derive(Clone, Debug, PartialEq)]
pub struct Demography {
    pub fertility: Fertility,
    pub twins: Twins,
    pub market: Market,
    /// `λ_sep` per year; 0 is a world without separation (ADR-0018 section 3).
    pub separation_rate: f64,
    pub learning: LearningParams,
    /// The weights of leaving home at 16 to 25 (ADR-0018 section 4).
    pub leave_home: Vec<f64>,
}

/// Fertility (11.3): the adult rate of an attempt, the pause after a birth, the length of
/// a pregnancy, the wish `P_n ∝ p_n · e^(α·n)`, and the extra child.
#[derive(Clone, Debug, PartialEq)]
pub struct Fertility {
    pub adult_rate: f64,
    pub pause_months: u32,
    pub pregnancy_months: u32,
    pub wish_tilt: f64,
    pub wish_shape: Vec<f64>,
    /// The extra child of both partners' wishes (ADR-0017, addendum of 2026-11-05); `None`
    /// is the rule without it: she tries while her live births are below her wish.
    pub extra: Option<ExtraChild>,
}

/// The extra child (ADR-0017, addendum of 2026-11-05): four values, all or none.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExtraChild {
    /// A child stops counting as young from the first month start of its this many whole
    /// years.
    pub count_until_years: u32,
    /// Her live births may reach at most her wish plus this before an attempt.
    pub extra: u32,
    /// A grown child reopens her only with at least this many live births.
    pub grown_from: u32,
    /// And only if her draw is below this chance.
    pub grown_chance: f64,
}

/// Twins per conception (11.3): identical, fraternal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Twins {
    pub identical: f64,
    pub fraternal: f64,
}

/// The partner market (11.2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Market {
    /// The market age in whole years: from this birthday on.
    pub age_years: u32,
    /// `c`, `m` and the width of the women's search rate, `ρ` from the market age until
    /// before the 18th birthday.
    pub search_height: f64,
    pub search_center: f64,
    pub search_width: f64,
    pub minor_factor: f64,
    /// `k`, the most candidates per searching woman.
    pub candidates: u32,
    /// `w_K`, `w_H`, `w_a` and `μ_a` of the utility.
    pub weight_cognition: f64,
    pub weight_height: f64,
    pub weight_age: f64,
    pub age_gap: f64,
    /// Candidates with `φ` at or above this are skipped.
    pub kinship_limit: f64,
    /// How much more often a stranger is chosen than an otherwise equal first cousin; the
    /// kinship term is `16 · ln(odds) · φ`.
    pub cousin_odds: f64,
    /// The man's answer and the target ages (11.2a); `None` is the market where the chosen
    /// man does not answer and nobody has a taste.
    pub male: Option<MaleAnswer>,
}

/// The man's answer and the target ages (11.2a, ADR-0018 addendum of 2026-11-06): six values,
/// all or none.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaleAnswer {
    /// `σ_ν`, the spread of the taste `ν ~ normal(0, σ_ν)` every person draws once, in years.
    pub taste_sd: f64,
    /// His target age is his age minus `min(slope · max(a − from, 0), cap)`, not below the
    /// minimum: the slope per year of age.
    pub target_slope: f64,
    /// The age from which the target falls behind his own, in years.
    pub target_from: f64,
    /// The most the target falls behind his age, in years.
    pub target_cap: f64,
    /// The lowest target age before the taste, in years.
    pub target_min: f64,
    /// `w_am`, the weight of his age term, per squared year.
    pub weight_age: f64,
}

/// The keys of the man's answer, in the order of `MaleAnswer`.
const MALE_ANSWER: [&str; 6] = [
    "core:pairing_taste_sd",
    "core:pairing_male_target_slope",
    "core:pairing_male_target_from",
    "core:pairing_male_target_cap",
    "core:pairing_male_target_min",
    "core:pairing_weight_age_male",
];

/// The number of values without the extra child.
const VALUES: usize = 22;
/// The ages of leaving home: 16 to 25 (ADR-0018 section 4).
const LEAVE_AGES: usize = 10;
/// The most entries of the wish shape: with `α ≤ 10` every `e^(α·n)` stays finite.
const MAX_WISH: usize = 64;

/// Reads and checks the life course data (ADR-0017 section 1): the format, every value
/// with its unit, then the ranges (`data.demography`); zero only where it describes a
/// model without that part.
pub(crate) fn read_demography(file: &DemographyCalibration) -> Result<Demography, DataError> {
    if file.format_version != DEMOGRAPHY_FORMAT_VERSION {
        return Err(DataError::FormatVersion);
    }
    let value = |key, unit| -> Result<f64, DataError> {
        let id = DataId::new(key).unwrap_or_else(|_| panic!("invariant data.key"));
        let entry = file.values.get(&id).ok_or(DataError::CalibrationMissing)?;
        if entry.unit != unit {
            return Err(DataError::CalibrationUnit);
        }
        Ok(entry.value.get())
    };
    let within = |x: f64, (low, high): (f64, f64)| {
        if x >= low && x <= high {
            Ok(x)
        } else {
            Err(DataError::Demography)
        }
    };
    let positive = |x: f64, high: f64| {
        if x > 0.0 && x <= high {
            Ok(x)
        } else {
            Err(DataError::Demography)
        }
    };
    let whole = |x: f64, low: u32, high: u32| {
        (low..=high)
            .find(|n| f64::from(*n) == x)
            .ok_or(DataError::Demography)
    };
    let fertility = Fertility {
        adult_rate: positive(value("core:fertility_adult_rate", Unit::Fraction)?, 1.0)?,
        pause_months: whole(value("core:fertility_pause", Unit::Month)?, 0, 130)?,
        pregnancy_months: whole(value("core:pregnancy_length", Unit::Month)?, 1, 20)?,
        wish_tilt: positive(value("core:fertility_wish_tilt", Unit::Logit)?, 10.0)?,
        wish_shape: wish_shape(&file.fertility_wish_shape)?,
        extra: None,
    };
    let twins = Twins {
        identical: within(value("core:twins_identical", Unit::Fraction)?, (0.0, 1.0))?,
        fraternal: within(value("core:twins_fraternal", Unit::Fraction)?, (0.0, 1.0))?,
    };
    if twins.identical + twins.fraternal > 1.0 {
        return Err(DataError::Demography);
    }
    let market = Market {
        // The base game publishes 16.
        age_years: whole(value("core:market_age", Unit::Year)?, 16, 120)?,
        search_height: within(
            value("core:market_search_height_female", Unit::Fraction)?,
            (0.0, 1.0),
        )?,
        search_center: positive(
            value("core:market_search_center_female", Unit::Year)?,
            150.0,
        )?,
        search_width: positive(value("core:market_search_width", Unit::Year)?, 150.0)?,
        minor_factor: positive(
            value("core:market_search_factor_minor", Unit::Fraction)?,
            1.0,
        )?,
        candidates: whole(value("core:market_candidates", Unit::Count)?, 1, 1000)?,
        weight_cognition: positive(value("core:pairing_weight_cognition", Unit::Logit)?, 1e6)?,
        weight_height: positive(value("core:pairing_weight_height", Unit::Logit)?, 1e6)?,
        weight_age: positive(value("core:pairing_weight_age", Unit::Logit)?, 1e6)?,
        age_gap: positive(value("core:pairing_age_gap_preferred", Unit::Year)?, 150.0)?,
        kinship_limit: positive(value("core:pairing_kinship_limit", Unit::Fraction)?, 1.0)?,
        cousin_odds: within(value("core:pairing_cousin_odds", Unit::Ratio)?, (1.0, 1e6))?,
        male: None,
    };
    // Zero, or inside the range of every hazard, so that `B / λ` is finite.
    let separation_rate = value("core:separation_rate", Unit::PerYear)?;
    if separation_rate != 0.0 {
        within(separation_rate, (1e-9, 1000.0))?;
    }
    let learning = LearningParams {
        kappa: positive(
            value("core:learning_plateau", Unit::PerStandardDeviation)?,
            10.0,
        )?,
        nu: positive(
            value("core:learning_speed", Unit::PerStandardDeviation)?,
            10.0,
        )?,
        tau0: positive(value("core:learning_time", Unit::Hour)?, 1e9)?,
    };
    // The extra child (ADR-0017, addendum of 2026-11-05), one group: none of its keys (the
    // rule without it, 22 values), or all of them.
    let extra_child = [
        "core:fertility_count_until",
        "core:fertility_extra",
        "core:fertility_grown_from",
        "core:fertility_grown_chance",
    ];
    let named = extra_child
        .iter()
        .filter(|key| {
            let id = DataId::new(key).unwrap_or_else(|_| panic!("invariant data.key"));
            file.values.contains_key(&id)
        })
        .count();
    let extra = match named {
        0 => None,
        n if n == extra_child.len() => Some(ExtraChild {
            count_until_years: whole(value(extra_child[0], Unit::Year)?, 1, 120)?,
            extra: whole(value(extra_child[1], Unit::Count)?, 1, 64)?,
            grown_from: whole(value(extra_child[2], Unit::Count)?, 1, 64)?,
            grown_chance: within(value(extra_child[3], Unit::Fraction)?, (0.0, 1.0))?,
        }),
        _ => return Err(DataError::CalibrationMissing),
    };
    // The man's answer (ADR-0018, addendum of 2026-11-06), one group as well: none of its six
    // keys (the market where the chosen man does not answer), or all of them.
    let answering = MALE_ANSWER
        .iter()
        .filter(|key| {
            let id = DataId::new(key).unwrap_or_else(|_| panic!("invariant data.key"));
            file.values.contains_key(&id)
        })
        .count();
    let male = match answering {
        0 => None,
        n if n == MALE_ANSWER.len() => Some(MaleAnswer {
            taste_sd: within(value(MALE_ANSWER[0], Unit::Year)?, (0.0, 50.0))?,
            target_slope: within(value(MALE_ANSWER[1], Unit::Ratio)?, (0.0, 1.0))?,
            target_from: positive(value(MALE_ANSWER[2], Unit::Year)?, 150.0)?,
            target_cap: positive(value(MALE_ANSWER[3], Unit::Year)?, 150.0)?,
            target_min: positive(value(MALE_ANSWER[4], Unit::Year)?, 150.0)?,
            weight_age: positive(value(MALE_ANSWER[5], Unit::Logit)?, 1e6)?,
        }),
        _ => return Err(DataError::CalibrationMissing),
    };
    if file.values.len() != VALUES + named + answering {
        return Err(DataError::CalibrationUnknown);
    }
    let fertility = Fertility { extra, ..fertility };
    let market = Market { male, ..market };
    Ok(Demography {
        fertility,
        twins,
        market,
        separation_rate,
        learning,
        leave_home: leave_home_ages(&file.leave_home_ages)?,
    })
}

/// The ages of leaving home: ten weights for 16 to 25, each a fraction in [0, 1], at least
/// one above 0.
fn leave_home_ages(entries: &[Calibrated]) -> Result<Vec<f64>, DataError> {
    if entries.len() != LEAVE_AGES {
        return Err(DataError::Demography);
    }
    let weights = fractions(entries)?;
    if weights.iter().any(|w| *w > 0.0) {
        Ok(weights)
    } else {
        Err(DataError::Demography)
    }
}

/// Weights, each a fraction in [0, 1].
fn fractions(entries: &[Calibrated]) -> Result<Vec<f64>, DataError> {
    let mut weights = Vec::with_capacity(entries.len());
    for entry in entries {
        if entry.unit != Unit::Fraction {
            return Err(DataError::CalibrationUnit);
        }
        let x = entry.value.get();
        if !(0.0..=1.0).contains(&x) {
            return Err(DataError::Demography);
        }
        weights.push(x);
    }
    Ok(weights)
}

/// The wish shape: 1 to 64 weights, each a fraction in [0, 1], at least one above 0.
fn wish_shape(entries: &[Calibrated]) -> Result<Vec<f64>, DataError> {
    if entries.is_empty() || entries.len() > MAX_WISH {
        return Err(DataError::Demography);
    }
    let mut weights = Vec::with_capacity(entries.len());
    for entry in entries {
        if entry.unit != Unit::Fraction {
            return Err(DataError::CalibrationUnit);
        }
        let x = entry.value.get();
        if !(0.0..=1.0).contains(&x) {
            return Err(DataError::Demography);
        }
        weights.push(x);
    }
    if weights.iter().all(|w| *w <= 0.0) {
        return Err(DataError::Demography);
    }
    Ok(weights)
}

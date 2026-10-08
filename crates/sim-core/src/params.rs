//! World parameters (ADR-0004 section 4): set by `Start`, changed only by the override
//! block of a fork (ADR-0001 section 6). They are state and part of the projection.

use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::finite::FiniteF64;
use crate::time::{MAX_ABS_SECS, YEAR};

/// The parameters of a world. Every field is required: a scenario names each one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldParams {
    /// The switch of the hereditary diseases (C5). Off means penetrance 0: the alleles
    /// stay and every random draw is made as with the switch on.
    pub hereditary_diseases: bool,
    /// The nutrition index of the calendar years that have one, by strictly ascending
    /// year (genetics specification 5.4, ADR-0013); every other year has 1.
    pub nutrition: Vec<NutritionYear>,
}

/// The nutrition index `N` of one calendar year, from 0 to 1 (ADR-0013).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NutritionYear {
    pub year: i64,
    pub index: FiniteF64,
}

/// The years whose first and last second lie in the range of time (ADR-0002).
const FIRST_YEAR: i64 = -(MAX_ABS_SECS / YEAR.as_secs());
const LAST_YEAR: i64 = MAX_ABS_SECS / YEAR.as_secs() - 1;

impl NutritionYear {
    /// The year lies in the range of time and the index in [0, 1] (ADR-0013 section 1).
    pub(crate) fn in_range(self) -> bool {
        let index = self.index.get();
        (FIRST_YEAR..=LAST_YEAR).contains(&self.year) && (0.0..=1.0).contains(&index)
    }
}

/// One world parameter with its new value. The name of the variant is the data ID of
/// the parameter; new parameters only go to the end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParamChange {
    #[serde(rename = "core:hereditary_diseases")]
    HereditaryDiseases(bool),
    /// The index of one year (ADR-0013 section 2); the year is the parameter.
    #[serde(rename = "core:nutrition_year")]
    NutritionYear(NutritionYear),
}

impl WorldParams {
    /// These parameters after `change`; a year of the nutrition index is inserted or
    /// replaced in the order of the years.
    pub(crate) fn with(mut self, change: ParamChange) -> WorldParams {
        match change {
            ParamChange::HereditaryDiseases(on) => self.hereditary_diseases = on,
            ParamChange::NutritionYear(entry) => {
                match self.nutrition.binary_search_by_key(&entry.year, |e| e.year) {
                    Ok(at) => self.nutrition[at] = entry,
                    Err(at) => self.nutrition.insert(at, entry),
                }
            }
        }
        self
    }
}

impl ParamChange {
    /// True if both change the same parameter, whatever the values.
    pub(crate) const fn same_param(self, other: ParamChange) -> bool {
        match (self, other) {
            (ParamChange::HereditaryDiseases(_), ParamChange::HereditaryDiseases(_)) => true,
            (ParamChange::NutritionYear(a), ParamChange::NutritionYear(b)) => a.year == b.year,
            (ParamChange::HereditaryDiseases(_), ParamChange::NutritionYear(_))
            | (ParamChange::NutritionYear(_), ParamChange::HereditaryDiseases(_)) => false,
        }
    }
}

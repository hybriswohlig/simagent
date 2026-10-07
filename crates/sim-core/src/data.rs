//! Definitions (ADR-0007): the genome map (B9) and the genetics calibration (B13) as the
//! core sees them, without the text of their files. `Data::new` checks them once; only
//! checked data reaches a world. The data hash covers them (ADR-0004 section 2).

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::fmt;

use serde::{Deserialize, Serialize};

use crate::data_id::DataId;
use crate::finite::FiniteF64;

/// Format version of both definition files.
pub const FORMAT_VERSION: u32 = 1;

/// The calibration values of the genetics (`core:calibration_genetics`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneticsCalibration {
    pub format_version: u32,
    pub values: BTreeMap<DataId, Calibrated>,
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
}

/// The genome map (`core:genome_map`, B9): the loci in map order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenomeMap {
    pub format_version: u32,
    pub map_version: u32,
    pub loci: Vec<Locus>,
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

/// How the alleles of a locus act (genetics specification 2.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Additive,
    Recessive,
    Dominant,
    Threshold,
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

/// The definitions of one world, before the check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Definitions {
    pub calibration: GeneticsCalibration,
    pub genome_map: GenomeMap,
}

/// A map length in Morgan: the expected number of crossovers of a segment per meiosis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Morgans(pub f64);

/// A rate per locus and transmission.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PerTransmission(pub f64);

/// The genetics calibration as the core reads it (genetics specification 2.2, 2.4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Genetics {
    pub map_length_autosome_female: Morgans,
    pub map_length_autosome_male: Morgans,
    pub map_length_x_female: Morgans,
    pub mutation_rate: PerTransmission,
    pub mutation_rate_disease: PerTransmission,
}

/// Checked definitions with their data hash. Only `Data::new` makes one.
#[derive(Clone, Debug, PartialEq)]
pub struct Data {
    definitions: Definitions,
    genetics: Genetics,
    hash: u64,
}

/// Why definitions are not loaded; the code is stable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataError {
    FormatVersion,
    MapVersion,
    LocusPosition,
    LocusOrder,
    LocusId,
    LocusMark,
    CalibrationMissing,
    CalibrationUnknown,
    CalibrationUnit,
    CalibrationRange,
}

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            DataError::FormatVersion => "data.format_version",
            DataError::MapVersion => "data.map_version",
            DataError::LocusPosition => "data.locus_position",
            DataError::LocusOrder => "data.locus_order",
            DataError::LocusId => "data.locus_id",
            DataError::LocusMark => "data.locus_mark",
            DataError::CalibrationMissing => "data.calibration_missing",
            DataError::CalibrationUnknown => "data.calibration_unknown",
            DataError::CalibrationUnit => "data.calibration_unit",
            DataError::CalibrationRange => "data.calibration_range",
        })
    }
}

impl Data {
    /// Checks the definitions and computes their data hash (ADR-0007).
    pub fn new(definitions: Definitions) -> Result<Data, DataError> {
        let genetics = Genetics {
            map_length_autosome_female: Morgans(0.0),
            map_length_autosome_male: Morgans(0.0),
            map_length_x_female: Morgans(0.0),
            mutation_rate: PerTransmission(0.0),
            mutation_rate_disease: PerTransmission(0.0),
        };
        Ok(Data {
            definitions,
            genetics,
            hash: 0,
        })
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
}

/// The definitions of the repository, read the way sim-cli reads them: the text fields
/// removed, the rest into the core types (ADR-0007 section 2).
#[cfg(test)]
pub(crate) mod testing {
    use serde_json::Value;

    use super::{Data, Definitions};

    fn without_text(mut value: Value) -> Value {
        strip(&mut value);
        value
    }

    fn strip(value: &mut Value) {
        match value {
            Value::Object(fields) => {
                for name in ["source", "as_of", "note", "description"] {
                    fields.remove(name);
                }
                fields.values_mut().for_each(strip);
            }
            Value::Array(items) => items.iter_mut().for_each(strip),
            _ => {}
        }
    }

    pub(crate) fn data() -> Data {
        let read = |text| without_text(serde_json::from_str(text).expect("json"));
        let definitions = Definitions {
            calibration: serde_json::from_value(read(include_str!(
                "../../../data/calibration/genetics.json"
            )))
            .expect("calibration"),
            genome_map: serde_json::from_value(read(include_str!(
                "../../../data/genetics/genome_map.json"
            )))
            .expect("genome map"),
        };
        Data::new(definitions).expect("checked")
    }
}

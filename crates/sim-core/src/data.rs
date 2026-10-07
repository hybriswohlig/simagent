//! Definitions (ADR-0007): the genome map (B9) and the genetics calibration (B13) as the
//! core sees them, without the text of their files. `Data::new` checks them once; only
//! checked data reaches a world. The data hash covers them (ADR-0004 section 2).

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::fmt;

use serde::{Deserialize, Serialize};

use crate::data_id::DataId;
use crate::data_id::str_key;
use crate::finite::FiniteF64;
use crate::hash::stable_hash64;

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

/// The definitions of one world, before the check. Its postcard bytes are what the data
/// hash covers (ADR-0007 section 3): the sets in the order of their data IDs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
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
    LocusKey,
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
            DataError::LocusKey => "data.locus_key",
            DataError::LocusMark => "data.locus_mark",
            DataError::CalibrationMissing => "data.calibration_missing",
            DataError::CalibrationUnknown => "data.calibration_unknown",
            DataError::CalibrationUnit => "data.calibration_unit",
            DataError::CalibrationRange => "data.calibration_range",
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

fn check_map(map: &GenomeMap) -> Result<(), DataError> {
    if map.format_version != FORMAT_VERSION {
        return Err(DataError::FormatVersion);
    }
    if map.map_version == 0 {
        return Err(DataError::MapVersion);
    }
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
    }
    if !keys_unique(map.loci.iter().map(|locus| &locus.id), str_key) {
        return Err(DataError::LocusKey);
    }
    Ok(())
}

/// No two IDs share a key (ADR-0004 section 2: a collision is a load error); `key` is
/// `str_key` outside the tests.
fn keys_unique<'a>(ids: impl Iterator<Item = &'a DataId>, key: impl Fn(&DataId) -> u64) -> bool {
    let mut seen = BTreeMap::new();
    ids.into_iter().all(|id| seen.insert(key(id), ()).is_none())
}

/// The calibration values the core reads, with their units; nothing else is allowed.
fn read_genetics(calibration: &GeneticsCalibration) -> Result<Genetics, DataError> {
    if calibration.format_version != FORMAT_VERSION {
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
    };
    if calibration.values.len() != 5 {
        return Err(DataError::CalibrationUnknown);
    }
    Ok(genetics)
}

fn encode<T: Serialize>(value: &T) -> Vec<u8> {
    postcard::to_allocvec(value).unwrap_or_else(|_| panic!("invariant data.encode"))
}

impl Data {
    /// Checks the definitions and computes their data hash (ADR-0007).
    pub fn new(definitions: Definitions) -> Result<Data, DataError> {
        let genetics = read_genetics(&definitions.calibration)?;
        check_map(&definitions.genome_map)?;
        let hash = stable_hash64(&encode(&definitions));
        Ok(Data {
            definitions,
            genetics,
            hash,
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

/// The definitions of the repository, read the way sim-cli reads them (ADR-0007 section
/// 2): the text fields of each calibration value and each locus removed, the rest into
/// the core types.
#[cfg(test)]
pub(crate) mod testing {
    use serde_json::Value;

    use super::{Data, Definitions};

    fn without(mut file: Value, entries: &str, names: &[&str]) -> Value {
        let list: alloc::vec::Vec<&mut Value> = match &mut file[entries] {
            Value::Object(values) => values.values_mut().collect(),
            Value::Array(loci) => loci.iter_mut().collect(),
            _ => panic!("entries"),
        };
        for entry in list {
            for name in names {
                entry.as_object_mut().expect("entry").remove(*name);
            }
        }
        file
    }

    pub(crate) fn data() -> Data {
        let read = |text| serde_json::from_str::<Value>(text).expect("json");
        let calibration = without(
            read(include_str!("../../../data/calibration/genetics.json")),
            "values",
            &["source", "as_of", "note"],
        );
        let genome_map = without(
            read(include_str!("../../../data/genetics/genome_map.json")),
            "loci",
            &["description"],
        );
        let definitions = Definitions {
            calibration: serde_json::from_value(calibration).expect("calibration"),
            genome_map: serde_json::from_value(genome_map).expect("genome map"),
        };
        Data::new(definitions).expect("checked")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

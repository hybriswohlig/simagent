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
use crate::finite::FiniteF64;
use crate::hash::stable_hash64;

/// Format version of the calibration file.
pub const CALIBRATION_FORMAT_VERSION: u32 = 1;

/// Format version of the genome map (ADR-0008 section 5).
pub const MAP_FORMAT_VERSION: u32 = 2;

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
    /// The position of each locus in the map, by its data ID.
    loci: BTreeMap<DataId, usize>,
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

/// The genetic value of `t` is finite for every genome (ADR-0008 section 3): every term
/// is finite at the doses 0, 1 and 2, every term of a load is not 0 at one of them, and
/// the fold of their largest magnitudes is finite and stays finite divided by
/// `sigma_ref`.
fn check_bounds(map: &GenomeMap, t: &Trait) -> Result<(), DataError> {
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
        check_map(&definitions.genome_map, str_key)?;
        let hash = stable_hash64(&encode(&definitions));
        let loci = (0..)
            .zip(&definitions.genome_map.loci)
            .map(|(index, locus)| (locus.id.clone(), index))
            .collect();
        Ok(Data {
            definitions,
            genetics,
            hash,
            loci,
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
        for list in ["traits", "loci"] {
            for entry in genome_map[list].as_array_mut().expect("list") {
                take(entry, "description");
            }
        }
        (calibration, genome_map)
    }

    pub(crate) fn data() -> Data {
        let read = |text| serde_json::from_str::<Value>(text).expect("json");
        let (calibration, genome_map) = without_text(
            read(include_str!("../../../data/calibration/genetics.json")),
            read(include_str!("../../../data/genetics/genome_map.json")),
        );
        let definitions = Definitions {
            calibration: serde_json::from_value(calibration).expect("calibration"),
            genome_map: serde_json::from_value(genome_map).expect("genome map"),
        };
        Data::new(definitions).expect("checked")
    }

    /// The alleles of a founder at `core:dummy`: maternal, paternal; none for two
    /// reference alleles.
    pub(crate) fn dummy(
        maternal: u8,
        paternal: u8,
    ) -> alloc::vec::Vec<crate::genome::LocusAlleles> {
        if (maternal, paternal) == (0, 0) {
            return alloc::vec::Vec::new();
        }
        alloc::vec![crate::genome::LocusAlleles {
            locus: super::DataId::new("core:dummy").expect("id"),
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
        two.loci.push(other);
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

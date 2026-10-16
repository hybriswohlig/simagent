//! The genome (genetics specification 2.1, B9): the autosomes in two copies, the X from
//! the mother, the X or Y from the father, the mitochondria; one bit per slot. Which slot
//! is which locus says the genome map (ADR-0007).

use alloc::vec;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::data::{Chromosome, Data, Locus};
use crate::data_id::DataId;

/// The genetic sex.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sex {
    Female,
    Male,
}

/// Which of the two chromosomes of a parent a meiosis starts on: the one the parent got
/// from its mother or the one from its father.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Strand {
    Maternal,
    Paternal,
}

/// The sex chromosome a parent passes on: a mother always X, a father X or Y.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SexChromosome {
    X,
    Y,
}

pub mod meiosis;

/// One copy of the autosomes: 8 segments of 24 slots, slot `i` of segment `s` at bit
/// `24 s + i` of 192 bits (genetics specification 2.1, 2.2). A bit is the allele: 0 the
/// reference allele, 1 the alternative.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Autosomes(pub(crate) [u64; 3]);

/// The X: 16 slots, slot `i` at bit `i`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct XChromosome(pub(crate) u16);

/// The Y: 8 slots, slot `i` at bit `i`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct YChromosome(pub(crate) u8);

/// The mitochondria: 8 slots, slot `i` at bit `i`; one copy, from the mother.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mitochondria(pub(crate) u8);

/// The sex chromosome a person has from the father; it decides the sex.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaternalSex {
    X(XChromosome),
    Y(YChromosome),
}

/// The genome of a person (genetics specification 2.1): the autosomes from mother and
/// father, the X from the mother, the X or Y from the father, the mitochondria.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Genome {
    pub maternal: Autosomes,
    pub paternal: Autosomes,
    pub maternal_x: XChromosome,
    pub paternal_sex: PaternalSex,
    pub mt: Mitochondria,
}

/// Bit `bit` of a word as an allele.
const fn bit_of(word: u64, bit: u32) -> u8 {
    if word >> bit & 1 == 1 { 1 } else { 0 }
}

impl Autosomes {
    /// The allele at slot `slot` of segment `segment`; both must be in range.
    pub fn allele(self, segment: u8, slot: u8) -> u8 {
        let bit = u32::from(segment) * 24 + u32::from(slot);
        let word = usize::try_from(bit / 64).unwrap_or_else(|_| panic!("invariant genome.bit"));
        bit_of(self.0[word], bit % 64)
    }

    /// Sets the allele at a slot.
    pub(crate) fn set(&mut self, segment: u8, slot: u8, allele: u8) {
        let bit = u32::from(segment) * 24 + u32::from(slot);
        let word = usize::try_from(bit / 64).unwrap_or_else(|_| panic!("invariant genome.bit"));
        let mask = 1u64 << (bit % 64);
        if allele == 1 {
            self.0[word] |= mask;
        } else {
            self.0[word] &= !mask;
        }
    }
}

impl XChromosome {
    pub fn allele(self, slot: u8) -> u8 {
        bit_of(u64::from(self.0), u32::from(slot))
    }

    pub(crate) fn set(&mut self, slot: u8, allele: u8) {
        let mask = 1u16 << slot;
        if allele == 1 {
            self.0 |= mask;
        } else {
            self.0 &= !mask;
        }
    }
}

impl YChromosome {
    pub fn allele(self, slot: u8) -> u8 {
        bit_of(u64::from(self.0), u32::from(slot))
    }

    pub(crate) fn set(&mut self, slot: u8, allele: u8) {
        let mask = 1u8 << slot;
        if allele == 1 {
            self.0 |= mask;
        } else {
            self.0 &= !mask;
        }
    }
}

impl Mitochondria {
    pub fn allele(self, slot: u8) -> u8 {
        bit_of(u64::from(self.0), u32::from(slot))
    }

    pub(crate) fn set(&mut self, slot: u8, allele: u8) {
        let mask = 1u8 << slot;
        if allele == 1 {
            self.0 |= mask;
        } else {
            self.0 &= !mask;
        }
    }
}

impl Genome {
    /// All reference alleles, with the sex chromosome from the father for `sex`.
    pub const fn reference(sex: Sex) -> Genome {
        Genome {
            maternal: Autosomes([0; 3]),
            paternal: Autosomes([0; 3]),
            maternal_x: XChromosome(0),
            paternal_sex: match sex {
                Sex::Female => PaternalSex::X(XChromosome(0)),
                Sex::Male => PaternalSex::Y(YChromosome(0)),
            },
            mt: Mitochondria(0),
        }
    }

    pub const fn sex(&self) -> Sex {
        match self.paternal_sex {
            PaternalSex::X(_) => Sex::Female,
            PaternalSex::Y(_) => Sex::Male,
        }
    }
}

/// The alleles of a person at one locus, in the order of its copies: maternal, then
/// paternal where the chromosome type has both (genetics specification 2.1). Founders and
/// the export list only loci with an alternative allele; every other locus carries the
/// reference allele in every copy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocusAlleles {
    pub locus: DataId,
    pub copies: Vec<u8>,
}

/// Why the alleles of a founder do not fit the genome map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlleleError {
    UnknownLocus,
    LocusTwice,
    Copies,
}

impl Genome {
    /// The alleles at `locus`, one per copy this person has there.
    pub fn copies(&self, locus: &Locus) -> Vec<u8> {
        match (locus.chromosome, self.paternal_sex) {
            (Chromosome::Autosome, _) => vec![
                self.maternal.allele(locus.segment, locus.slot),
                self.paternal.allele(locus.segment, locus.slot),
            ],
            (Chromosome::X, PaternalSex::X(paternal)) => vec![
                self.maternal_x.allele(locus.slot),
                paternal.allele(locus.slot),
            ],
            (Chromosome::X, PaternalSex::Y(_)) => vec![self.maternal_x.allele(locus.slot)],
            (Chromosome::Y, PaternalSex::Y(y)) => vec![y.allele(locus.slot)],
            (Chromosome::Y, PaternalSex::X(_)) => Vec::new(),
            (Chromosome::Mt, _) => vec![self.mt.allele(locus.slot)],
        }
    }

    /// The dose at `locus`: the number of copies with the alternative allele.
    pub fn dose(&self, locus: &Locus) -> u8 {
        self.copies(locus).iter().sum()
    }

    /// The genetic value `G_t` of trait `t` (genetics specification 4.1, ADR-0008
    /// section 3, Grok GF-1): the additive value less the map's `mean_ref`, divided by
    /// `sigma_ref`; without a `mean_ref` exactly `A / sigma_ref`. `None` if the map has no
    /// such trait.
    pub fn genetic_value(&self, t: &DataId, data: &Data) -> Option<f64> {
        let (_, sigma, mean) = data.trait_terms(t)?;
        let a = self.additive_value(t, data)?;
        Some(mean.map_or(a, |mean| a - mean) / sigma)
    }

    /// The additive value `A_t` of trait `t` (ADR-0008 section 3, ADR-0020 section 3): the
    /// terms folded from 0 in their order, before the division by `sigma_ref`. `None` if
    /// the map has no such trait.
    pub fn additive_value(&self, t: &DataId, data: &Data) -> Option<f64> {
        let (terms, _, _) = data.trait_terms(t)?;
        Some(terms.iter().fold(0.0, |sum, term| {
            let locus = data
                .genome_map()
                .loci
                .get(term.locus)
                .unwrap_or_else(|| panic!("invariant genome.term_locus"));
            sum + term.coefficient * (f64::from(self.dose(locus)) - term.d_ref)
        }))
    }

    pub(crate) fn set_copies(&mut self, locus: &Locus, copies: &[u8]) {
        let (segment, slot) = (locus.segment, locus.slot);
        match (locus.chromosome, &mut self.paternal_sex, copies) {
            (Chromosome::Autosome, _, [maternal, paternal]) => {
                self.maternal.set(segment, slot, *maternal);
                self.paternal.set(segment, slot, *paternal);
            }
            (Chromosome::X, PaternalSex::X(x), [maternal, paternal]) => {
                self.maternal_x.set(slot, *maternal);
                x.set(slot, *paternal);
            }
            (Chromosome::X, PaternalSex::Y(_), [maternal]) => self.maternal_x.set(slot, *maternal),
            (Chromosome::Y, PaternalSex::Y(y), [allele]) => y.set(slot, *allele),
            (Chromosome::Mt, _, [allele]) => self.mt.set(slot, *allele),
            _ => panic!("invariant genome.copies"),
        }
    }

    /// The genome of a founder of sex `sex` with these alleles; every locus the list
    /// does not name carries the reference allele.
    pub fn with_alleles(
        sex: Sex,
        alleles: &[LocusAlleles],
        data: &Data,
    ) -> Result<Genome, AlleleError> {
        let mut genome = Genome::reference(sex);
        let mut seen = alloc::collections::BTreeSet::new();
        for entry in alleles {
            let locus = data.locus(&entry.locus).ok_or(AlleleError::UnknownLocus)?;
            if !seen.insert(&entry.locus) {
                return Err(AlleleError::LocusTwice);
            }
            // A locus this person has no copy of (a Y locus of a woman) cannot be listed.
            let has = genome.copies(locus).len();
            let fits = has > 0
                && entry.copies.len() == has
                && entry.copies.iter().all(|allele| *allele <= 1);
            if !fits {
                return Err(AlleleError::Copies);
            }
            genome.set_copies(locus, &entry.copies);
        }
        Ok(genome)
    }

    /// The loci with an alternative allele, in map order, with all their copies.
    pub fn alleles(&self, data: &Data) -> Vec<LocusAlleles> {
        data.genome_map()
            .loci
            .iter()
            .filter(|locus| self.dose(locus) > 0)
            .map(|locus| LocusAlleles {
                locus: locus.id.clone(),
                copies: self.copies(locus),
            })
            .collect()
    }
}

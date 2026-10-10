//! Meiosis (genetics specification 2.3): per parent and segment a start strand and a
//! Poisson number of crossovers at uniform positions; the mother passes an X the same
//! way, the father his X or his Y unchanged (his Y with the probability of a son from the
//! sex ratio at birth, 11.3), the mitochondria come from the mother. Every
//! draw is addressed by the genome key of the child and the side (ADR-0001), so a
//! meiosis depends only on the parents' genomes, the seed and the key.

use alloc::vec::Vec;

use serde::{Deserialize, Serialize};
use sim_math::dist;

use super::{Autosomes, Genome, Mitochondria, PaternalSex, SexChromosome, Strand, XChromosome};
use crate::data::{
    AUTOSOME_SEGMENTS, AUTOSOME_SLOTS, Chromosome, Data, Genetics, Locus, Mark, Morgans,
    MutationDirection, X_SLOTS,
};
use crate::data_id::DataId;
use crate::finite::FiniteF64;
use crate::lineage::GenomeKey;
use crate::rng::{DrawKey, Draws, SubjectKey, tags};

/// The segment index of the mother's X in the draw details (specification 2.3).
pub const X_SEGMENT: u8 = 8;

/// The side of a meiosis: 0 the mother, 1 the father (the occasion of its draws).
pub const MOTHER: u64 = 0;
pub const FATHER: u64 = 1;

/// What one segment did in a meiosis: its start strand and the crossover positions in
/// the order they were drawn.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SegmentMeiosis {
    pub segment: u8,
    pub start_strand: Strand,
    pub crossovers: Vec<FiniteF64>,
}

/// A new mutation of a meiosis (ADR-0004 section 7): the locus, the old and the new
/// allele.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mutation {
    pub locus: DataId,
    pub old: u8,
    pub new: u8,
}

/// What a meiosis of one parent did: the segments in order (autosomes 0 to 7, then the
/// mother's X), the sex chromosome passed on and the new mutations in their order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Meiosis {
    pub segments: Vec<SegmentMeiosis>,
    pub sex_chromosome: SexChromosome,
    pub mutations: Vec<Mutation>,
}

/// A zygote: the genome of the child and the meioses of mother and father.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Zygote {
    pub genome: Genome,
    pub mother: Meiosis,
    pub father: Meiosis,
}

fn key(tag: crate::rng::PurposeTag, genome: GenomeKey, side: u64, detail: u64) -> DrawKey {
    DrawKey {
        tag,
        subject: SubjectKey::from(genome),
        occasion: side,
        detail,
    }
}

/// The address of the start strand of a segment.
pub fn start_strand_key(genome: GenomeKey, side: u64, segment: u8) -> DrawKey {
    key(tags::MEIOSIS_START_STRAND, genome, side, u64::from(segment))
}

/// The address of the number of crossovers of a segment.
pub fn crossover_count_key(genome: GenomeKey, side: u64, segment: u8) -> DrawKey {
    key(tags::CROSSOVER_COUNT, genome, side, u64::from(segment))
}

/// The address of crossover `j` of a segment: detail `(segment << 32) | j`.
pub fn crossover_position_key(genome: GenomeKey, side: u64, segment: u8, j: u32) -> DrawKey {
    let detail = (u64::from(segment) << 32) | u64::from(j);
    key(tags::CROSSOVER_POSITION, genome, side, detail)
}

/// The address of the sex chromosome the father passes on: `bernoulli(p_Y)`, true for his
/// Y (specification 2.3, the table of 11.7).
pub fn sex_key(genome: GenomeKey) -> DrawKey {
    key(tags::SEX_CHROMOSOME, genome, FATHER, 0)
}

/// The address of part `part` of the number of new mutations of a side (specification
/// 2.4: one part while the total rate is at most 30, the largest mean of `poisson`).
pub fn mutation_count_key(genome: GenomeKey, side: u64, part: u64) -> DrawKey {
    key(tags::MUTATION_COUNT, genome, side, part)
}

/// The address of the site of new mutation `j` of a side.
pub fn mutation_site_key(genome: GenomeKey, side: u64, j: u32) -> DrawKey {
    key(tags::MUTATION_SITE, genome, side, u64::from(j))
}

/// The largest mean of `poisson` (ADR-0001 section 3).
const MAX_POISSON_MEAN: f64 = 30.0;

/// What one side passes on: the autosomes, the sex chromosome and, from the mother, the
/// mitochondria.
struct Gamete {
    autosomes: Autosomes,
    sex: PaternalSex,
    mt: Option<Mitochondria>,
}

impl Gamete {
    /// The allele the gamete carries at `locus`, `None` if it has no copy there.
    fn allele(&self, locus: &Locus) -> Option<u8> {
        match (locus.chromosome, self.sex, self.mt) {
            (Chromosome::Autosome, _, _) => Some(self.autosomes.allele(locus.segment, locus.slot)),
            (Chromosome::X, PaternalSex::X(x), _) => Some(x.allele(locus.slot)),
            (Chromosome::Y, PaternalSex::Y(y), _) => Some(y.allele(locus.slot)),
            (Chromosome::Mt, _, Some(mt)) => Some(mt.allele(locus.slot)),
            _ => None,
        }
    }

    fn set(&mut self, locus: &Locus, allele: u8) {
        match (locus.chromosome, &mut self.sex, &mut self.mt) {
            (Chromosome::Autosome, _, _) => self.autosomes.set(locus.segment, locus.slot, allele),
            (Chromosome::X, PaternalSex::X(x), _) => x.set(locus.slot, allele),
            (Chromosome::Y, PaternalSex::Y(y), _) => y.set(locus.slot, allele),
            (Chromosome::Mt, _, Some(mt)) => mt.set(locus.slot, allele),
            _ => panic!("invariant meiosis.mutation_locus"),
        }
    }
}

/// The rate of a locus per transmission: the disease rate for a disease locus, else the
/// standard rate (specification 2.4).
fn rate(genetics: &Genetics, locus: &Locus) -> f64 {
    if locus.mark == Mark::Disease {
        genetics.mutation_rate_disease.0
    } else {
        genetics.mutation_rate.0
    }
}

/// The rates of `loci` added left to right.
fn total(genetics: &Genetics, loci: &[&Locus]) -> f64 {
    loci.iter()
        .fold(0.0, |sum, locus| sum + rate(genetics, locus))
}

/// The position in a list of rates that a site draw `u` chooses (specification 2.4): the
/// first at which the running sum is greater than `u` times the total, both added left to
/// right; the last one if rounding leaves the threshold at the total.
fn site(u: f64, rates: &[f64]) -> usize {
    let threshold = u * rates.iter().fold(0.0, |sum, rate| sum + rate);
    let mut running = 0.0;
    for (position, rate) in rates.iter().enumerate() {
        running += rate;
        if running > threshold {
            return position;
        }
    }
    rates.len() - 1
}

/// The new mutations of one side (specification 2.4): the eligible loci of the gamete in
/// map order, a Poisson number of them, each chosen by its rate among those not chosen
/// yet. Changes the gamete and returns the mutations in their order.
fn mutate(
    draws: &Draws,
    data: &Data,
    genome: GenomeKey,
    side: u64,
    gamete: &mut Gamete,
) -> Vec<Mutation> {
    let genetics = data.genetics();
    let mut eligible: Vec<&Locus> = data
        .genome_map()
        .loci
        .iter()
        .filter(|locus| {
            gamete
                .allele(locus)
                .is_some_and(|allele| match locus.mutation {
                    MutationDirection::Both => true,
                    MutationDirection::ToAlt => allele == 0,
                    MutationDirection::ToRef => allele == 1,
                })
        })
        .collect();
    // The count, in parts of at most the largest mean of `poisson`.
    let mut rest = total(&genetics, &eligible);
    let mut count = 0_u32;
    let mut part = 0;
    loop {
        let mean = if rest < MAX_POISSON_MEAN {
            rest
        } else {
            MAX_POISSON_MEAN
        };
        count = count
            .checked_add(draws.poisson(mutation_count_key(genome, side, part), mean))
            .unwrap_or_else(|| panic!("invariant meiosis.mutation_count"));
        rest -= mean;
        if rest <= 0.0 {
            break;
        }
        part += 1;
    }
    let mut mutations = Vec::new();
    for j in 0..count {
        if eligible.is_empty() {
            break;
        }
        let [word] = draws.words::<1>(mutation_site_key(genome, side, j));
        let rates: Vec<f64> = eligible
            .iter()
            .map(|locus| rate(&genetics, locus))
            .collect();
        let locus = eligible.remove(site(dist::unit(word), &rates));
        let old = gamete
            .allele(locus)
            .unwrap_or_else(|| panic!("invariant meiosis.mutation_locus"));
        let new = 1 - old;
        gamete.set(locus, new);
        mutations.push(Mutation {
            locus: locus.id.clone(),
            old,
            new,
        });
    }
    mutations
}

/// One segment of `slots` slots and map length `length`: its record and, per slot,
/// whether the slot comes from the copy the parent has from its father.
fn segment(
    draws: &Draws,
    genome: GenomeKey,
    side: u64,
    index: u8,
    length: Morgans,
    slots: u8,
) -> (SegmentMeiosis, Vec<bool>) {
    let start_strand = match draws.uniform_below(start_strand_key(genome, side, index), 2) {
        0 => Strand::Maternal,
        _ => Strand::Paternal,
    };
    let count = draws.poisson(crossover_count_key(genome, side, index), length.0);
    let crossovers: Vec<f64> = (0..count)
        .map(|j| {
            let [word] = draws.words::<1>(crossover_position_key(genome, side, index, j));
            dist::unit(word)
        })
        .collect();
    let from_father = (0..slots)
        .map(|slot| {
            let position = (f64::from(slot) + 0.5) / f64::from(slots);
            let crossed = crossovers.iter().filter(|c| **c < position).count();
            (start_strand == Strand::Paternal) != (crossed % 2 == 1)
        })
        .collect();
    let record = SegmentMeiosis {
        segment: index,
        start_strand,
        crossovers: crossovers
            .into_iter()
            .map(|c| FiniteF64::try_new(c).unwrap_or_else(|_| panic!("invariant meiosis.position")))
            .collect(),
    };
    (record, from_father)
}

/// The autosomes a parent passes on, with the records of the eight segments.
fn autosomes(
    draws: &Draws,
    genome: GenomeKey,
    side: u64,
    parent: &Genome,
    length: Morgans,
) -> (Autosomes, Vec<SegmentMeiosis>) {
    let mut gamete = Autosomes::default();
    let mut records = Vec::new();
    for index in 0..AUTOSOME_SEGMENTS {
        let (record, from_father) = segment(draws, genome, side, index, length, AUTOSOME_SLOTS);
        for (slot, father) in (0..AUTOSOME_SLOTS).zip(from_father) {
            let copy = if father {
                parent.paternal
            } else {
                parent.maternal
            };
            gamete.set(index, slot, copy.allele(index, slot));
        }
        records.push(record);
    }
    (gamete, records)
}

impl Zygote {
    /// The child of `mother` and `father` with genome key `genome`. The mother must be
    /// female and the father male; anything else is an invariant break.
    pub fn of(
        seed: &Draws,
        data: &Data,
        genome: GenomeKey,
        mother: &Genome,
        father: &Genome,
    ) -> Zygote {
        let genetics = data.genetics();
        let PaternalSex::X(her_paternal_x) = mother.paternal_sex else {
            panic!("invariant meiosis.mother_sex");
        };
        let PaternalSex::Y(his_y) = father.paternal_sex else {
            panic!("invariant meiosis.father_sex");
        };
        let (maternal, mut her_segments) = autosomes(
            seed,
            genome,
            MOTHER,
            mother,
            genetics.map_length_autosome_female,
        );
        let (x_record, from_father) = segment(
            seed,
            genome,
            MOTHER,
            X_SEGMENT,
            genetics.map_length_x_female,
            X_SLOTS,
        );
        let mut maternal_x = XChromosome::default();
        for (slot, father_copy) in (0..X_SLOTS).zip(from_father) {
            let copy = if father_copy {
                her_paternal_x
            } else {
                mother.maternal_x
            };
            maternal_x.set(slot, copy.allele(slot));
        }
        her_segments.push(x_record);
        let (paternal, his_segments) = autosomes(
            seed,
            genome,
            FATHER,
            father,
            genetics.map_length_autosome_male,
        );
        // The father passes his Y with `p_Y = r / (1 + r)` of the sex ratio at birth `r`
        // (specification 2.3 and 11.3, W11.3d; 21/41 for 1.05), his X otherwise.
        let r = genetics.sex_ratio_at_birth;
        let (paternal_sex, sex_chromosome) = if seed.bernoulli(sex_key(genome), r / (1.0 + r)) {
            (PaternalSex::Y(his_y), SexChromosome::Y)
        } else {
            (PaternalSex::X(father.maternal_x), SexChromosome::X)
        };
        let mut hers = Gamete {
            autosomes: maternal,
            sex: PaternalSex::X(maternal_x),
            mt: Some(mother.mt),
        };
        let her_mutations = mutate(seed, data, genome, MOTHER, &mut hers);
        let mut his = Gamete {
            autosomes: paternal,
            sex: paternal_sex,
            mt: None,
        };
        let his_mutations = mutate(seed, data, genome, FATHER, &mut his);
        let (PaternalSex::X(maternal_x), Some(mt)) = (hers.sex, hers.mt) else {
            panic!("invariant meiosis.mother_gamete");
        };
        Zygote {
            genome: Genome {
                maternal: hers.autosomes,
                paternal: his.autosomes,
                maternal_x,
                paternal_sex: his.sex,
                mt,
            },
            mother: Meiosis {
                segments: her_segments,
                sex_chromosome: SexChromosome::X,
                mutations: her_mutations,
            },
            father: Meiosis {
                segments: his_segments,
                sex_chromosome,
                mutations: his_mutations,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::site;

    /// A threshold exactly on a running sum does not choose that locus: the sum must be
    /// greater (2.4). With rates 1 and 1, `u` = 1/2 puts the threshold at 1.
    #[test]
    fn a_site_on_a_running_sum_goes_to_the_next_locus() {
        assert_eq!(site(0.5, &[1.0, 1.0]), 1);
        assert_eq!(site(0.0, &[1.0, 1.0]), 0);
        assert_eq!(site(0.25, &[0.5, 0.25, 0.25]), 0);
        assert_eq!(site(0.5, &[0.5, 0.25, 0.25]), 1);
    }

    /// With normal rates `u · total` stays below the total (`u` is at most 1 - 2^-53), so
    /// the last running sum is always greater. Subnormal rates, which the checks of the
    /// definitions accept, can round the threshold up to the total: then no running sum
    /// is greater, and the last locus is chosen.
    #[test]
    fn rounding_to_the_total_chooses_the_last_locus() {
        let largest = 1.0 - f64::EPSILON / 2.0;
        assert!(largest * 3.0 < 3.0);
        assert_eq!(site(largest, &[1.0, 1.0, 1.0]), 2);
        let tiny = f64::from_bits(1);
        assert_eq!(largest * (tiny + tiny), tiny + tiny);
        assert_eq!(site(largest, &[tiny, tiny]), 1);
    }
}

//! Meiosis (genetics specification 2.3): per parent and segment a start strand and a
//! Poisson number of crossovers at uniform positions; the mother passes an X the same
//! way, the father his X or his Y unchanged, the mitochondria come from the mother. Every
//! draw is addressed by the genome key of the child and the side (ADR-0001), so a
//! meiosis depends only on the parents' genomes, the seed and the key.

use alloc::vec::Vec;

use serde::{Deserialize, Serialize};
use sim_math::dist;

use super::{Autosomes, Genome, PaternalSex, SexChromosome, Strand, XChromosome};
use crate::data::{AUTOSOME_SEGMENTS, AUTOSOME_SLOTS, Genetics, Morgans, X_SLOTS};
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

/// What a meiosis of one parent did: the segments in order (autosomes 0 to 7, then the
/// mother's X) and the sex chromosome passed on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Meiosis {
    pub segments: Vec<SegmentMeiosis>,
    pub sex_chromosome: SexChromosome,
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

/// The address of the sex chromosome the father passes on.
pub fn sex_key(genome: GenomeKey) -> DrawKey {
    key(tags::SEX_CHROMOSOME, genome, FATHER, 0)
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
        genetics: &Genetics,
        genome: GenomeKey,
        mother: &Genome,
        father: &Genome,
    ) -> Zygote {
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
        let (paternal_sex, sex_chromosome) = match seed.uniform_below(sex_key(genome), 2) {
            0 => (PaternalSex::X(father.maternal_x), SexChromosome::X),
            _ => (PaternalSex::Y(his_y), SexChromosome::Y),
        };
        Zygote {
            genome: Genome {
                maternal,
                paternal,
                maternal_x,
                paternal_sex,
                mt: mother.mt,
            },
            mother: Meiosis {
                segments: her_segments,
                sex_chromosome: SexChromosome::X,
            },
            father: Meiosis {
                segments: his_segments,
                sex_chromosome,
            },
        }
    }
}

//! The genetics card (ADR-0021 section 5, Grok's G2 to G5 and L1 to L5): per trait the
//! phenotype `z` as a percentile against the frozen model reference, in which `z` is N(0, 1)
//! at every age and for both sexes, and the four parts it adds up from. The core gives the
//! numbers, the viewer the words. Read only, like the rest of the presentation state.
//!
//! # Percentile
//!
//! `p = 100 Φ(z)`; from the middle up (`z ≥ 0`) "größer als `n`" with `n = min(⌊p⌋, 99)`,
//! below "kleiner als `n`" with `n = min(⌊100 − p⌋, 99)` (L1). `n` is counted against the
//! thresholds `probit(j / 100)` for `j` from 1 to 99 (the pinned `probit`, ADR-0019, exactly
//! 0 at one half): `Φ(z) ≥ j / 100` exactly when `z` is at least the threshold, so no other
//! function of the normal distribution is needed. Only a `z` within the tolerance of
//! `probit` (2^-48 relative) of a threshold could fall to the other side.

use alloc::vec::Vec;

use serde::Serialize;

use crate::body_contract::{herc2, mc1r};
use crate::data::{Action, Carrier, Chromosome};
use crate::data_id::{DataId, str_key};
use crate::genome::Genome;
use crate::ids::SimId;
use crate::lineage::LineageKey;
use crate::phenotype::{Environment, PhenotypeParts, phenotype_parts};
use crate::rng::{Draws, WorldSeed};
use crate::state::DomainState;
use crate::state::disease::qualifies;
use crate::time::{SimDuration, SimTime, YEAR};
use crate::world::World;

/// The traits the card reads as a percentile (G3): height as the target height `z_h` of
/// `core:growth`, cognition, curls.
pub fn card_traits() -> [DataId; 3] {
    ["core:growth", "core:cognition", "core:curl"]
        .map(|t| DataId::new(t).unwrap_or_else(|_| panic!("invariant card.trait_id")))
}

/// From this age in whole years the card compares with men and women, below it with boys
/// and girls of the same age (L3).
pub const ADULT_YEARS: i64 = 18;

/// A percentile sentence: "größer als `n` von 100" (`up`) or "kleiner als `n` von 100", `n`
/// from 0 to 99.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Percentile {
    pub up: bool,
    pub n: u8,
}

/// The percentile of `z` against the model reference (see Percentile).
pub fn percentile(z: f64) -> Percentile {
    let up = z >= 0.0;
    let n = (1..=99_u32)
        .filter(|&j| {
            let threshold = sim_math::probit(f64::from(j) / 100.0);
            if up { threshold <= z } else { threshold >= z }
        })
        .count();
    Percentile {
        up,
        n: u8::try_from(n).unwrap_or_else(|_| panic!("invariant card.percentile")),
    }
}

/// The parts `z` adds up from (G5, L4): with both parents' genomes in the run, the parents'
/// share `√h² · (G_mother + G_father) / 2`, the passing on `√h² · G` less that share, the
/// shared childhood `√c² · C` and the own life `√e² · E`; otherwise the inherited part
/// `√h² · G` whole, and how many parents are in the run (0 or 1).
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Parts {
    Split {
        parents: f64,
        passing: f64,
        childhood: f64,
        own: f64,
    },
    Inherited {
        inherited: f64,
        childhood: f64,
        own: f64,
        parents_in_town: u8,
    },
}

/// One trait of the card.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TraitCard {
    #[serde(rename = "trait")]
    pub trait_id: DataId,
    pub z: f64,
    pub percentile: Percentile,
    /// Whether the person was at least [`ADULT_YEARS`] old at the second shown.
    pub adult: bool,
    pub parts: Parts,
    /// The percentile of the mean of both parents' `z`, each at its own second shown; only
    /// with both parents in the run (L5).
    pub parents_mean: Option<Percentile>,
}

/// The height lines of the card (L5, L6), in cm: what the curve loses at the second shown
/// to the hunger years (5.4) and to age (5.3), and with both parents in the run the mean of
/// their target heights `h1_i` (5.2). The height itself is the body's.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct HeightCard {
    pub hunger_cm: f64,
    pub age_cm: f64,
    pub parents_target_cm: Option<f64>,
}

/// The copies of a disease's allele a person carries (G4, L7), the mother's first: two
/// copies of an autosome or a woman's X; a man's one X (his father gave a Y); the one
/// mitochondrial copy, from the mother; for the threshold disease no copies but the
/// percentile of its liability `L`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Copies {
    Two { mother: bool, father: bool },
    MotherAndY { mother: bool },
    Mother { mother: bool },
    Liability { percentile: Percentile },
}

/// What a person's genotype found for a disease: nothing, a carrier of a recessive allele,
/// a qualifying genotype whose penetrance fell, or the onset age in years.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Found {
    Not,
    Carrier,
    PenetranceFallen,
    Onset(f64),
}

/// The status of a disease at the second shown (G4, L7, M1, M4): not affected; a carrier who
/// stays healthy; a qualifying genotype whose penetrance fell; with the switch off a resting
/// one; affected since the calendar year of the onset at its age in whole years; or not yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DiseaseStatus {
    NotAffected,
    Carrier,
    PenetranceFallen,
    NotYet,
    Resting,
    Affected { year: i64, age_years: i64 },
}

/// The status of what the genotype `found` at `at` for a person born at `born`, with the
/// switch of the hereditary diseases `switch_on` then. Affected from the age `onset` on (as
/// the pedigree says), since the calendar year of the second `born + ⌈onset · YEAR⌉`, at
/// `⌊onset⌋` whole years; an onset whose second lies beyond the clock never comes.
pub fn disease_status(found: Found, switch_on: bool, born: SimTime, at: SimTime) -> DiseaseStatus {
    match found {
        Found::Not => DiseaseStatus::NotAffected,
        Found::Carrier => DiseaseStatus::Carrier,
        Found::PenetranceFallen => DiseaseStatus::PenetranceFallen,
        Found::Onset(_) if !switch_on => DiseaseStatus::Resting,
        Found::Onset(onset) if crate::state::onset::reached(born, onset, at) => {
            // The year and whole years the onset entry of the log carries.
            let (year, age_years) = crate::state::onset::since(born, onset);
            DiseaseStatus::Affected { year, age_years }
        }
        Found::Onset(_) => DiseaseStatus::NotYet,
    }
}

/// Where a person's copy of a pigment allele from a parent came from (G7, L8): the parent
/// carries one copy on its mother's strand, so from the person's grandmother, or on its
/// father's, so from the grandfather; from the pool if that grandparent is not in the run;
/// a parent without a copy made it new.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CopySource {
    Grandmother { sim: SimId },
    Grandfather { sim: SimId },
    Pool,
    New,
}

/// The source of the copy a parent with `parent_copies` (its mother's strand first, one or
/// no copy of the allele) passed on, its parents in the run being `grandparents` (mother,
/// father). Read from the genotypes (Grok's N1): a carrying parent whose other strand
/// mutated to the allele in that very meiosis is not told apart (about one such line in ten
/// thousand at the mutation rate of 5e-5); the page of the model's assumptions says so.
pub fn copy_source(parent_copies: &[u8], grandparents: [Option<SimId>; 2]) -> CopySource {
    let [grandmother, grandfather] = grandparents;
    match parent_copies {
        [1, _] => grandmother.map_or(CopySource::Pool, |sim| CopySource::Grandmother { sim }),
        [_, 1] => grandfather.map_or(CopySource::Pool, |sim| CopySource::Grandfather { sim }),
        _ => CopySource::New,
    }
}

/// A line "übersprungen" (G7, L8): the person carries two copies at a large pigment locus,
/// both parents are in the run and neither carries two; where each copy came from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SkippedLine {
    pub locus: DataId,
    pub mother_copy: CopySource,
    pub father_copy: CopySource,
}

/// The line "übersprungen" at `locus` of a person carrying `own` there, whose parents carry
/// `mother` and `father` (each its mother's strand first) and have `grandparents` in the run
/// (the mother's parents, then the father's): a line only if the person carries two copies
/// and neither parent does.
pub fn skipped_line(
    locus: DataId,
    own: &[u8],
    (mother, father): (&[u8], &[u8]),
    grandparents: [[Option<SimId>; 2]; 2],
) -> Option<SkippedLine> {
    let two = |copies: &[u8]| copies == [1, 1];
    if !two(own) || two(mother) || two(father) {
        return None;
    }
    let [mothers, fathers] = grandparents;
    Some(SkippedLine {
        locus,
        mother_copy: copy_source(mother, mothers),
        father_copy: copy_source(father, fathers),
    })
}

/// One disease of the card.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DiseaseCard {
    pub disease: DataId,
    pub copies: Copies,
    pub status: DiseaseStatus,
}

/// The genetics card of a person: the traits of [`card_traits`] the map has, the height
/// lines where the data have the growth traits, the diseases of the data, with whether the
/// switch of the hereditary diseases was off at the second shown (M1; for the living as it
/// stands from the next second, with an input at the stop, Grok's round 36), and the lines
/// "übersprungen".
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GeneticsCard {
    pub traits: Vec<TraitCard>,
    pub height: Option<HeightCard>,
    pub diseases_off: bool,
    pub diseases: Vec<DiseaseCard>,
    /// The lines "übersprungen", MC1R before HERC2.
    pub skipped: Vec<SkippedLine>,
}

/// What the card reads of a sim, living or archived: its birth, the second shown (now for
/// the living, the second it left for the others), its genome and lineage key.
fn facts(
    domain: &DomainState,
    now: SimTime,
    sim: SimId,
) -> Option<(SimTime, SimTime, &Genome, LineageKey)> {
    match domain.sims.get(&sim) {
        Some(person) => Some((person.born, now, &person.genome, person.lineage_key)),
        None => {
            let gone = domain.archive.get(&sim)?;
            Some((gone.born, gone.left, &gone.genome, gone.lineage_key))
        }
    }
}

impl World {
    /// The terms of the phenotype of trait `t` of `sim` at its second shown.
    fn parts_shown(&self, sim: SimId, t: &DataId) -> Option<(PhenotypeParts, i64)> {
        let state = self.state();
        let domain = state.domain();
        let data = state.data();
        let (born, at, genome, key) = facts(domain, self.closed_through(), sim)?;
        let age = at.checked_since(born).filter(|age| age.as_secs() >= 0)?;
        let seeds = domain.seeds(data, sim);
        let individual = Draws::new(WorldSeed::new(seeds.individual));
        let shared = Draws::new(WorldSeed::new(seeds.shared));
        let environment = Environment {
            individual: &individual,
            shared: &shared,
            neutral: false,
        };
        let parts = phenotype_parts(
            environment,
            data,
            genome,
            key,
            domain.parent_keys(sim),
            t,
            age,
        )?;
        Some((parts, age.as_secs().div_euclid(YEAR.as_secs())))
    }

    /// The genetics card of `sim`, living or archived; `None` for an unknown sim.
    pub fn genetics_card(&self, sim: SimId) -> Option<GeneticsCard> {
        let state = self.state();
        let domain = state.domain();
        let data = state.data();
        facts(domain, self.closed_through(), sim)?;
        let in_town: Vec<SimId> = (domain.parent_keys(sim).into_iter())
            .flat_map(|parents| [parents.mother, parents.father])
            .filter_map(|key| domain.keys.get(&key).copied())
            .collect();
        let genome_of = |parent: SimId| facts(domain, self.closed_through(), parent).map(|f| f.2);
        let traits = card_traits()
            .iter()
            .filter_map(|t| {
                let (parts, years) = self.parts_shown(sim, t)?;
                let childhood = parts.shared;
                let own = parts.individual;
                let (parts_of_z, parents_mean) = match in_town.as_slice() {
                    &[mother, father] => {
                        let g = |parent: SimId| genome_of(parent)?.genetic_value(t, data);
                        let (g_mother, g_father) = (g(mother)?, g(father)?);
                        let share = parts.root_h2 * ((g_mother + g_father) / 2.0);
                        let z_of = |parent: SimId| self.parts_shown(parent, t).map(|(p, _)| p.z());
                        let mean = (z_of(mother)? + z_of(father)?) / 2.0;
                        let split = Parts::Split {
                            parents: share,
                            passing: parts.genetic - share,
                            childhood,
                            own,
                        };
                        (split, Some(percentile(mean)))
                    }
                    others => {
                        let count = u8::try_from(others.len())
                            .unwrap_or_else(|_| panic!("invariant card.parents"));
                        let whole = Parts::Inherited {
                            inherited: parts.genetic,
                            childhood,
                            own,
                            parents_in_town: count,
                        };
                        (whole, None)
                    }
                };
                let z = parts.z();
                Some(TraitCard {
                    trait_id: t.clone(),
                    z,
                    percentile: percentile(z),
                    adult: years >= ADULT_YEARS,
                    parts: parts_of_z,
                    parents_mean,
                })
            })
            .collect();
        let shown = |sim: SimId| facts(domain, self.closed_through(), sim).map(|f| f.1);
        let terms = |sim: SimId| state.height_terms(sim, shown(sim)?);
        let height = terms(sim).map(|own| HeightCard {
            hunger_cm: own.nutrition_loss,
            age_cm: own.age_loss,
            parents_target_cm: match in_town.as_slice() {
                &[mother, father] => (terms(mother).zip(terms(father)))
                    .map(|(m, f)| (m.curve.target + f.curve.target) / 2.0),
                _ => None,
            },
        });
        let (diseases_off, diseases) = self.disease_lines(sim)?;
        Some(GeneticsCard {
            traits,
            height,
            diseases_off,
            diseases,
            skipped: self.skipped_lines(sim),
        })
    }

    /// The lines "übersprungen" of `sim` at MC1R and HERC2 (see [`SkippedLine`]).
    fn skipped_lines(&self, sim: SimId) -> Vec<SkippedLine> {
        let state = self.state();
        let domain = state.domain();
        let data = state.data();
        let now = self.closed_through();
        let in_run = |sim: SimId| -> [Option<SimId>; 2] {
            domain.parent_keys(sim).map_or([None, None], |parents| {
                [parents.mother, parents.father].map(|key| domain.keys.get(&key).copied())
            })
        };
        let genome = |sim: SimId| facts(domain, now, sim).map(|f| f.2);
        let (Some(own), [Some(mother), Some(father)]) = (genome(sim), in_run(sim)) else {
            return Vec::new();
        };
        let (Some(of_mother), Some(of_father)) = (genome(mother), genome(father)) else {
            return Vec::new();
        };
        let grandparents = [in_run(mother), in_run(father)];
        [mc1r(), herc2()]
            .into_iter()
            .filter_map(|id| {
                let locus = data.locus(&id)?;
                let (from_mother, from_father) = (of_mother.copies(locus), of_father.copies(locus));
                skipped_line(
                    id,
                    &own.copies(locus),
                    (&from_mother, &from_father),
                    grandparents,
                )
            })
            .collect()
    }

    /// The switch off at the second shown, and the disease lines of `sim` in data order: a
    /// disease of a locus on an autosome, X or mt, or a threshold disease with its trait of
    /// liability (one without afflicts nobody, 11.1, and a disease on Y has no words yet;
    /// neither has a line).
    fn disease_lines(&self, sim: SimId) -> Option<(bool, Vec<DiseaseCard>)> {
        let state = self.state();
        let domain = state.domain();
        let data = state.data();
        let (born, at, genome, key) = facts(domain, self.closed_through(), sim)?;
        let afflictions = (domain
            .sims
            .get(&sim)
            .map(|person| person.afflictions.as_slice()))
        .or_else(|| {
            domain
                .archive
                .get(&sim)
                .map(|gone| gone.afflictions.as_slice())
        })?;
        // A living person's card reads the switch as it stands from the next second, with an
        // input at the stop, as the run page's button does (Grok's round 36, S1); the rest
        // of the card stays at `at`. An input is never stamped at the clock's last second.
        let living = domain.sims.contains_key(&sim);
        let next = (at.checked_add(SimDuration::from_secs(1)))
            .filter(|_| living)
            .unwrap_or(at);
        let switch_on = crate::pedigree::switch_at(self.input_log(), next);
        let seeds = domain.seeds(data, sim);
        let individual = Draws::new(WorldSeed::new(seeds.individual));
        let shared = Draws::new(WorldSeed::new(seeds.shared));
        let environment = Environment {
            individual: &individual,
            shared: &shared,
            neutral: false,
        };
        let lines = (data.diseases().iter())
            .filter_map(|disease| {
                let (copies, qualifies, carrier) = match &disease.carrier {
                    Carrier::Locus { locus, .. } => {
                        let locus = data
                            .locus(locus)
                            .unwrap_or_else(|| panic!("invariant card.disease_locus"));
                        let held = genome.copies(locus);
                        let copies = match (held.as_slice(), locus.chromosome) {
                            (&[mother, father], _) => Copies::Two {
                                mother: mother == 1,
                                father: father == 1,
                            },
                            (&[mother], Chromosome::X) => Copies::MotherAndY {
                                mother: mother == 1,
                            },
                            (&[mother], Chromosome::Mt) => Copies::Mother {
                                mother: mother == 1,
                            },
                            // A disease on Y: none in the data, and no words for it.
                            _ => return None,
                        };
                        let qualifies = qualifies(genome, locus);
                        let carrier =
                            locus.action == Action::Recessive && !qualifies && held.contains(&1);
                        (copies, qualifies, carrier)
                    }
                    Carrier::Threshold {
                        liability: Some(trait_id),
                        ..
                    } => {
                        let liability = phenotype_parts(
                            environment,
                            data,
                            genome,
                            key,
                            domain.parent_keys(sim),
                            trait_id,
                            SimDuration::ZERO,
                        )
                        .unwrap_or_else(|| panic!("invariant card.liability"))
                        .z();
                        let threshold = data
                            .threshold(&disease.code)
                            .unwrap_or_else(|| panic!("invariant card.threshold"));
                        let copies = Copies::Liability {
                            percentile: percentile(liability),
                        };
                        (copies, liability > threshold, false)
                    }
                    Carrier::Threshold {
                        liability: None, ..
                    } => return None,
                };
                let held = (afflictions.iter())
                    .find(|affliction| affliction.disease == str_key(&disease.code));
                let found = match (qualifies, held) {
                    (false, _) if carrier => Found::Carrier,
                    (false, _) => Found::Not,
                    (true, None) => Found::PenetranceFallen,
                    (true, Some(affliction)) => Found::Onset(affliction.onset.get()),
                };
                Some(DiseaseCard {
                    disease: disease.code.clone(),
                    copies,
                    status: disease_status(found, switch_on, born, at),
                })
            })
            .collect();
        Some((!switch_on, lines))
    }
}

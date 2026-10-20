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

use crate::data_id::DataId;
use crate::genome::Genome;
use crate::ids::SimId;
use crate::lineage::LineageKey;
use crate::phenotype::{Environment, PhenotypeParts, phenotype_parts};
use crate::rng::{Draws, WorldSeed};
use crate::state::DomainState;
use crate::time::{SimTime, YEAR};
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

/// The genetics card of a person: the traits of [`card_traits`] the map has.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GeneticsCard {
    pub traits: Vec<TraitCard>,
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
        Some(GeneticsCard { traits })
    }
}

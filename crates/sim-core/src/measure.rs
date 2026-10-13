//! The measurement of the pre-run (ADR-0020 section 3, design note of the measurement): over
//! the living, per trait the mean and the population spread of the additive value `A` (the
//! sum `genetic_value` divides by `σ_ref`), per additive locus the mean dose. A read-only
//! view of the state, like the pedigree.

use alloc::vec::Vec;

use serde::Serialize;

use crate::data::Action;
use crate::data_id::DataId;
use crate::world::WorldState;

/// The measurement of the living at one moment.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Measurement {
    /// How many sims live (`n`, at least 1).
    pub living: u64,
    /// Every trait of the map, in its order.
    pub traits: Vec<TraitSpread>,
    /// Every additive locus of the map, in its order (ADR-0020 section 3: "je additivem
    /// Ort"; a threshold locus is not measured).
    pub loci: Vec<LocusDose>,
}

/// The mean and the population spread (divisor `n`) of the additive value of a trait.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TraitSpread {
    #[serde(rename = "trait")]
    pub trait_id: DataId,
    pub mean: f64,
    pub sigma: f64,
}

/// The mean dose of an additive locus, from 0 to 2.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LocusDose {
    pub locus: DataId,
    pub mean_dose: f64,
}

/// An integer as f64, exactly (counts and dose sums lie far below 2^53).
fn exact(n: u64) -> f64 {
    i64::try_from(n)
        .ok()
        .and_then(sim_math::i64_to_f64_exact)
        .unwrap_or_else(|| panic!("invariant measure.count_range"))
}

impl WorldState {
    /// The measurement of the living now; `None` without living sims. The sums run over
    /// the sims in ascending lineage key, each from +0.0.
    pub fn measure(&self) -> Option<Measurement> {
        let data = self.data();
        let mut living: Vec<_> = self.domain().sims.values().collect();
        if living.is_empty() {
            return None;
        }
        living.sort_unstable_by_key(|sim| sim.lineage_key);
        let n =
            u64::try_from(living.len()).unwrap_or_else(|_| panic!("invariant measure.count_range"));
        let divisor = exact(n);
        let map = data.genome_map();
        let traits = map
            .traits
            .iter()
            .map(|t| {
                let values: Vec<f64> = living
                    .iter()
                    .map(|sim| {
                        sim.genome
                            .additive_value(&t.id, data)
                            .unwrap_or_else(|| panic!("invariant measure.trait"))
                    })
                    .collect();
                let mean = values.iter().fold(0.0, |sum, a| sum + a) / divisor;
                let variance = values
                    .iter()
                    .fold(0.0, |sum, a| sum + (a - mean) * (a - mean))
                    / divisor;
                TraitSpread {
                    trait_id: t.id.clone(),
                    mean,
                    sigma: sim_math::sqrt(variance),
                }
            })
            .collect();
        let loci = map
            .loci
            .iter()
            .filter(|locus| matches!(locus.action, Action::Additive(_)))
            .map(|locus| {
                let doses: u64 = living
                    .iter()
                    .map(|sim| u64::from(sim.genome.dose(locus)))
                    .sum();
                LocusDose {
                    locus: locus.id.clone(),
                    mean_dose: exact(doses) / divisor,
                }
            })
            .collect();
        Some(Measurement {
            living: n,
            traits,
            loci,
        })
    }
}

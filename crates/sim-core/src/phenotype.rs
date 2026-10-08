//! The phenotype of a trait (genetics specification 4.2, ADR-0009): the genetic value,
//! the shared and the individual environment, weighted by the age. A pure function of the
//! genome, the keys, the data, the seed and the age; nothing of it is state.

use crate::data::Data;
use crate::data_id::{DataId, str_key};
use crate::genome::Genome;
use crate::lineage::LineageKey;
use crate::rng::{DrawKey, Draws, SubjectKey, tags};
use crate::time::SimDuration;

/// The biological parents of a person in the run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Parents {
    pub mother: LineageKey,
    pub father: LineageKey,
}

/// The shared environment `C_t` (ADR-0009 section 3): one draw per parent pair and trait;
/// a person without parents in the run counts as its own mother and father.
pub fn shared_environment(
    draws: &Draws,
    t: &DataId,
    own: LineageKey,
    parents: Option<Parents>,
) -> f64 {
    let Parents { mother, father } = parents.unwrap_or(Parents {
        mother: own,
        father: own,
    });
    draws.normal(
        DrawKey {
            tag: tags::ENV_SHARED,
            subject: SubjectKey::from(mother),
            occasion: str_key(t),
            detail: father.get(),
        },
        0.0,
        1.0,
    )
}

/// The individual environment `E_t` (ADR-0009 section 3): one draw per person and trait.
pub fn individual_environment(draws: &Draws, t: &DataId, own: LineageKey) -> f64 {
    draws.normal(
        DrawKey {
            tag: tags::ENV_INDIVIDUAL,
            subject: SubjectKey::from(own),
            occasion: str_key(t),
            detail: 0,
        },
        0.0,
        1.0,
    )
}

/// A weight held in 0 to 1 before its root (ADR-0009 section 4): 0 unless greater than
/// 0, so that `-0.0` becomes `+0.0`, and 1 above 1.
fn held(weight: f64) -> f64 {
    if weight > 1.0 {
        1.0
    } else if weight > 0.0 {
        weight
    } else {
        0.0
    }
}

/// The phenotype `z_t` at the calendar age `age` (ADR-0009 section 4); `None` for a trait
/// without a phenotype or not in the map.
pub fn phenotype(
    draws: &Draws,
    data: &Data,
    genome: &Genome,
    own: LineageKey,
    parents: Option<Parents>,
    t: &DataId,
    age: SimDuration,
) -> Option<f64> {
    let [h2_child, c2_child, h2_adult, c2_adult] = data.phenotype_weights(t)?;
    let g = genome.genetic_value(t, data)?;
    let x = age.as_years();
    let s = 1.0 / (1.0 + sim_math::exp((12.0 - x) / 2.0));
    let h2 = h2_child + (h2_adult - h2_child) * s;
    let c2 = c2_child + (c2_adult - c2_child) * s;
    let e2 = (1.0 - h2) - c2;
    let c = shared_environment(draws, t, own, parents);
    let e = individual_environment(draws, t, own);
    Some(
        ((sim_math::sqrt(held(h2)) * g) + (sim_math::sqrt(held(c2)) * c))
            + (sim_math::sqrt(held(e2)) * e),
    )
}

#[cfg(test)]
mod tests {
    use super::held;

    /// ADR-0009 section 4: a weight rounded below 0 or to `-0.0` becomes `+0.0`, one
    /// rounded above 1 becomes 1, so `sqrt` never sees a value outside 0 to 1.
    #[test]
    fn a_weight_is_held_in_zero_to_one() {
        assert_eq!(held(-0.0).to_bits(), 0.0_f64.to_bits());
        assert_eq!(held(-1e-17).to_bits(), 0.0_f64.to_bits());
        assert_eq!(held(1.0 + f64::EPSILON), 1.0);
        assert_eq!(held(0.25), 0.25);
        assert_eq!(held(1.0), 1.0);
    }
}

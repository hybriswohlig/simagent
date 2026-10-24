//! The phenotype of a trait (genetics specification 4.2, ADR-0009): the genetic value,
//! the shared and the individual environment, weighted by the age. A pure function of the
//! genome, the keys, the data, the seed and the age; nothing of it is state.

use crate::data::Data;
use crate::data_id::{DataId, str_key};
use crate::genome::{Genome, Sex};
use crate::lineage::LineageKey;
use crate::rng::{DrawKey, Draws, SubjectKey, tags};
use crate::time::SimDuration;

/// The draws of the two environments of a person (ADR-0009 section 3, ADR-0020 section
/// 5): `E` and `C` may come from different seeds, as for a town-born child of two pool
/// persons (`C` from the pool seed, `E` from the world seed).
#[derive(Clone, Copy)]
pub struct Environment<'a> {
    pub individual: &'a Draws,
    pub shared: &'a Draws,
    /// The sheet of portraits (ADR-0021 G10, Grok's round 37, B7 b): `C = 0` and `E = 0`,
    /// so the same genes in two persons give the same phenotype. A way of the presentation
    /// only; every run draws both environments.
    pub neutral: bool,
}

impl<'a> Environment<'a> {
    /// Both environments from the same draws.
    pub const fn one(draws: &'a Draws) -> Environment<'a> {
        Environment {
            individual: draws,
            shared: draws,
            neutral: false,
        }
    }

    /// No environment: `C = 0` and `E = 0` (the sheet of portraits); the draws are never
    /// read.
    pub const fn neutral(draws: &'a Draws) -> Environment<'a> {
        Environment {
            individual: draws,
            shared: draws,
            neutral: true,
        }
    }
}

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

/// The three terms of a phenotype `z_t` (ADR-0009 section 4), each with its held weight:
/// `√h² · G`, `√c² · C` and `√e² · E`, and `√h²` alone.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhenotypeParts {
    /// `√h²`, the weight of the genetic value.
    pub root_h2: f64,
    /// `√h² · G`.
    pub genetic: f64,
    /// `√c² · C`, the shared environment.
    pub shared: f64,
    /// `√e² · E`, the individual environment.
    pub individual: f64,
}

impl PhenotypeParts {
    /// `z_t = ((√h² · G) + (√c² · C)) + (√e² · E)`, summed in this order.
    pub fn z(&self) -> f64 {
        (self.genetic + self.shared) + self.individual
    }
}

/// The terms of the phenotype `z_t` at the calendar age `age` (ADR-0009 section 4); `None`
/// for a trait without a phenotype or not in the map.
pub fn phenotype_parts(
    environment: Environment<'_>,
    data: &Data,
    genome: &Genome,
    own: LineageKey,
    parents: Option<Parents>,
    t: &DataId,
    age: SimDuration,
) -> Option<PhenotypeParts> {
    let [h2_child, c2_child, h2_adult, c2_adult] = data.phenotype_weights(t)?;
    let g = genome.genetic_value(t, data)?;
    let x = age.as_years();
    let s = 1.0 / (1.0 + sim_math::exp((12.0 - x) / 2.0));
    let h2 = h2_child + (h2_adult - h2_child) * s;
    let c2 = c2_child + (c2_adult - c2_child) * s;
    let e2 = (1.0 - h2) - c2;
    let (c, e) = if environment.neutral {
        (0.0, 0.0)
    } else {
        (
            shared_environment(environment.shared, t, own, parents),
            individual_environment(environment.individual, t, own),
        )
    };
    let root_h2 = sim_math::sqrt(held(h2));
    Some(PhenotypeParts {
        root_h2,
        genetic: root_h2 * g,
        shared: sim_math::sqrt(held(c2)) * c,
        individual: sim_math::sqrt(held(e2)) * e,
    })
}

/// The phenotype `z_t` at the calendar age `age` (ADR-0009 section 4): the sum of its
/// terms (`phenotype_parts`); `None` for a trait without a phenotype or not in the map.
pub fn phenotype(
    environment: Environment<'_>,
    data: &Data,
    genome: &Genome,
    own: LineageKey,
    parents: Option<Parents>,
    t: &DataId,
    age: SimDuration,
) -> Option<f64> {
    phenotype_parts(environment, data, genome, own, parents, t, age).map(|parts| parts.z())
}

/// The expression of a trait in standard deviations of women (genetics specification 6.2,
/// ADR-0014 section 2): women `z`, men `z · (1 + (√VR − 1) · P) + d · P` with the
/// puberty progress `P`; a trait without a sex difference is `z` for both.
pub fn sex_expression(data: &Data, t: &DataId, sex: Sex, z: f64, progress: f64) -> f64 {
    let Some((shift, variance_ratio)) = data.sex_difference(t) else {
        return z;
    };
    match sex {
        Sex::Female => z,
        Sex::Male => {
            z * (1.0 + (sim_math::sqrt(variance_ratio) - 1.0) * progress) + shift * progress
        }
    }
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

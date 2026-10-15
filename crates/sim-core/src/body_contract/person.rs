//! A person's body in the world (ADR-0015 section 3, ADR-0020 section 7; design note of the
//! census): the inputs of the mapping from the state. The biological age comes from the
//! person's value of the longevity trait (7.2, 7.4); `E` and `C` from its seeds and the keys
//! of its parents; the nutrition years are the world's, for an arrival only those after its
//! arrival (G6, ADR-0020 section 5).

use alloc::vec::Vec;

use super::{BodyData, BodyEvaluation, BodyInputs, body_params};
use crate::ageing;
use crate::data::Data;
use crate::genome::Sex;
use crate::ids::SimId;
use crate::mortality;
use crate::params::NutritionYear;
use crate::phenotype::Environment;
use crate::rng::{Draws, WorldSeed};
use crate::state::DomainState;
use crate::time::{SimTime, YEAR};
use crate::world::WorldState;

/// The nutrition years that act on `sim` (G6): an arrival does without every year that
/// ended at or before its arrival, which comes at a year start; a pool child and a town
/// person take every year (the loss counts only years after the birth).
pub(crate) fn nutrition_of(
    domain: &DomainState,
    sim: SimId,
    world: &[NutritionYear],
) -> Vec<NutritionYear> {
    let arrived = domain
        .pool_origin(sim)
        .filter(|origin| origin.born_pool.is_some())
        .map(|origin| origin.arrived);
    world
        .iter()
        .filter(|year| {
            arrived.is_none_or(|arrived| {
                year.year
                    .checked_add(1)
                    .and_then(|next| next.checked_mul(YEAR.as_secs()))
                    .is_some_and(|end| end > arrived.as_secs())
            })
        })
        .copied()
        .collect()
}

/// The mapping's inputs of a living sim at `t`, with the nutrition years `nutrition` (see
/// `nutrition_of`); `None` for an unknown sim or a `t` before its birth.
pub(crate) fn inputs_of<'a>(
    domain: &DomainState,
    data: &Data,
    body: &BodyData<'_>,
    sim: SimId,
    t: SimTime,
    nutrition: &'a [NutritionYear],
) -> Option<BodyInputs<'a>> {
    let person = domain.sims.get(&sim)?;
    let age = t.checked_since(person.born)?;
    let genetics = data.genetics();
    let gompertz = match person.sex {
        Sex::Female => genetics.senescence_female,
        Sex::Male => genetics.senescence_male,
    };
    let longevity = data.longevity_trait().map(|t| {
        person
            .genome
            .genetic_value(t, data)
            .unwrap_or_else(|| panic!("invariant body.longevity"))
    });
    let delta = mortality::age_offset(longevity, genetics.longevity_sigma_g, gompertz.b);
    let bio_age = ageing::bio_age(age.as_years(), delta, genetics.bio_age_weight);
    let seeds = domain.seeds(data, sim);
    let individual = Draws::new(WorldSeed::new(seeds.individual));
    let shared = Draws::new(WorldSeed::new(seeds.shared));
    Some(BodyInputs::of_person(
        body,
        Environment {
            individual: &individual,
            shared: &shared,
        },
        &person.genome,
        person.lineage_key,
        domain.parent_keys(sim),
        person.born,
        age,
        nutrition,
        bio_age,
    ))
}

impl WorldState {
    /// The body of a living sim at `t` (ADR-0005 section 1): `None` without body data in
    /// the data, for an unknown sim or before its birth. One entry for the census, the tests
    /// and the export of step 12.
    pub fn body_at(&self, sim: SimId, t: SimTime) -> Option<BodyEvaluation> {
        let data = self.data();
        let body = BodyData::new(data).ok()?;
        let domain = self.domain();
        let world = domain
            .world_params
            .as_ref()
            .map_or(&[][..], |params| params.nutrition.as_slice());
        let nutrition = nutrition_of(domain, sim, world);
        let inputs = inputs_of(domain, data, &body, sim, t, &nutrition)?;
        Some(body_params(&body, &inputs))
    }
}

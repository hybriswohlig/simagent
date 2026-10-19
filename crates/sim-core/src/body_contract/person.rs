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
use crate::growth::{IndividualCurve, height_loss, nutrition_loss};
use crate::ids::SimId;
use crate::mortality;
use crate::params::NutritionYear;
use crate::phenotype::Environment;
use crate::rng::{Draws, WorldSeed};
use crate::state::DomainState;
use crate::time::{SimDuration, SimTime, YEAR};
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

/// The biological age `b` of a person of this sex and genome at the calendar age `age`
/// (7.4): `Δ` from its value of the longevity trait (7.2).
fn bio_age(data: &Data, sex: Sex, genome: &crate::genome::Genome, age: SimDuration) -> f64 {
    let genetics = data.genetics();
    let gompertz = match sex {
        Sex::Female => genetics.senescence_female,
        Sex::Male => genetics.senescence_male,
    };
    let longevity = data.longevity_trait().map(|t| {
        genome
            .genetic_value(t, data)
            .unwrap_or_else(|| panic!("invariant body.longevity"))
    });
    let delta = mortality::age_offset(longevity, genetics.longevity_sigma_g, gompertz.b);
    ageing::bio_age(age.as_years(), delta, genetics.bio_age_weight)
}

/// The mapping's inputs of a sim at `t`, living or archived (the archive keeps its birth,
/// sex, genome and key), with the nutrition years `nutrition` (see `nutrition_of`); `None`
/// for an unknown sim or a `t` before its birth.
pub(crate) fn inputs_of<'a>(
    domain: &DomainState,
    data: &Data,
    body: &BodyData<'_>,
    sim: SimId,
    t: SimTime,
    nutrition: &'a [NutritionYear],
) -> Option<BodyInputs<'a>> {
    let (born, sex, genome, key) = match domain.sims.get(&sim) {
        Some(person) => (person.born, person.sex, &person.genome, person.lineage_key),
        None => {
            let gone = domain.archive.get(&sim)?;
            (gone.born, gone.sex, &gone.genome, gone.lineage_key)
        }
    };
    let age = t.checked_since(born).filter(|age| age.as_secs() >= 0)?;
    let bio_age = bio_age(data, sex, genome, age);
    let seeds = domain.seeds(data, sim);
    let individual = Draws::new(WorldSeed::new(seeds.individual));
    let shared = Draws::new(WorldSeed::new(seeds.shared));
    Some(BodyInputs::of_person(
        body,
        Environment {
            individual: &individual,
            shared: &shared,
        },
        genome,
        key,
        domain.parent_keys(sim),
        born,
        age,
        nutrition,
        bio_age,
    ))
}

impl WorldState {
    /// The body of a sim at `t`, living or archived (ADR-0005 section 1): `None` without
    /// body data in the data, for an unknown sim or before its birth. One entry for the
    /// census, the tests and the presentation state of step 12.
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

impl WorldState {
    /// The pure height of a sim at `t` in cm (ADR-0015 section 4; criterion P6): the
    /// height of the mapping, `κ_i · H(x; θ_i) − v_i(x) − Verlust(b)` (specification 10.2,
    /// with 5.2, 5.4 and 5.3), never clamped: its individual growth curve at
    /// `x = min(age, 25 years)`, less the loss of its nutrition years (G6 for an arrival) and
    /// the loss of age at its biological age. Living or archived; `None` for an unknown sim,
    /// a `t` before its birth, or data without the two traits.
    pub fn pure_height(&self, sim: SimId, t: SimTime) -> Option<f64> {
        let data = self.data();
        let domain = self.domain();
        let (born, sex, genome) = domain
            .sims
            .get(&sim)
            .map(|person| (person.born, person.sex, &person.genome))
            .or_else(|| {
                domain
                    .archive
                    .get(&sim)
                    .map(|person| (person.born, person.sex, &person.genome))
            })?;
        let age = t.checked_since(born)?;
        let [growth, puberty, ..] = super::body_traits();
        let z_h = self.phenotype(sim, &growth, SimDuration::ZERO)?;
        let z_p = self.phenotype(sim, &puberty, SimDuration::ZERO)?;
        let genetics = data.genetics();
        let curve = IndividualCurve::new(genetics.growth(sex), z_h, z_p);
        let mature = YEAR * 25;
        let x = if age < mature { age } else { mature };
        let world = domain
            .world_params
            .as_ref()
            .map_or(&[][..], |params| params.nutrition.as_slice());
        let nutrition = nutrition_of(domain, sim, world);
        let b = bio_age(data, sex, genome, age);
        let grown =
            curve.height(x) - nutrition_loss(&nutrition, &genetics.nutrition, born, x, curve.theta);
        Some(grown - height_loss(&genetics.height_loss, sex, b))
    }
}

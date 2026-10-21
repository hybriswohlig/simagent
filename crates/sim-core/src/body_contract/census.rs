//! The body in the yearly census (ADR-0020 section 7, ADR-0005 section 8): every living sim
//! evaluated once at the year end, counted per clamped parameter. Integers only, so the
//! counts do not depend on an order of sums.

use super::person::{inputs_of, nutrition_of};
use super::{BodyData, BodyParamKey, CLAMPED_KEYS, body_params};
use crate::data::Data;
use crate::ids::SimId;
use crate::state::DomainState;
use crate::time::SimTime;

/// The evaluations of one census: how many, per parameter of `CLAMPED_KEYS` (in that
/// order) how many lay truly outside the contract's range, and how many held an input of
/// the growth curve (ADR-0012).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BodyCensus {
    pub evaluated: u64,
    pub clamped: [u64; 21],
    pub curve_clamped: u64,
}

/// The body census of the living at `t`; all zero for data without body traits. Each sim
/// whose growth curve held an input goes to `held` with `z_h` and `z_p` before the hold;
/// each evaluated sim goes to `measured` with its height now (`height_cm`), in ascending
/// SimId.
pub(crate) fn count(
    domain: &DomainState,
    data: &Data,
    t: SimTime,
    held: &mut dyn FnMut(SimId, f64, f64),
    measured: &mut dyn FnMut(SimId, f64),
) -> BodyCensus {
    let mut census = BodyCensus::default();
    let Ok(body) = BodyData::census(data) else {
        return census;
    };
    let world = domain
        .world_params
        .as_ref()
        .map_or(&[][..], |params| params.nutrition.as_slice());
    let one = |n: u64| {
        n.checked_add(1)
            .unwrap_or_else(|| panic!("invariant census.body_count"))
    };
    for sim in domain.sims.keys() {
        let nutrition = nutrition_of(domain, *sim, world);
        let inputs = inputs_of(domain, data, &body, *sim, t, &nutrition)
            .unwrap_or_else(|| panic!("invariant census.body_inputs"));
        let evaluation = body_params(&body, &inputs);
        measured(*sim, evaluation.params.get(BodyParamKey::HeightCm));
        census.evaluated = one(census.evaluated);
        for (place, key) in CLAMPED_KEYS.iter().enumerate() {
            if evaluation.is_clamped(*key) {
                census.clamped[place] = one(census.clamped[place]);
            }
        }
        if evaluation.curve_clamped {
            census.curve_clamped = one(census.curve_clamped);
            held(*sim, inputs.z.height, inputs.z.puberty);
        }
    }
    census
}

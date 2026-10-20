//! The partners a pairing input would be accepted with now (ADR-0018 section 1, ADR-0021
//! section 6, "ein Paar stiften"): the viewer offers only them, so it judges neither age
//! nor kinship itself. Read only, like the rest of the presentation state.

use alloc::vec::Vec;

use crate::genome::Sex;
use crate::ids::SimId;
use crate::input::ArrangePartnership;
use crate::state::partnership::check_arrange;
use crate::world::World;

impl World {
    /// The living sims an `ArrangePartnership` of `sim` with them would pair in the next
    /// second, in id order: the other sex, both single and of the market age, not too
    /// closely related. Empty for a sim that is not alive or cannot be paired itself.
    pub fn partner_candidates(&self, sim: SimId) -> Vec<SimId> {
        let state = self.state();
        let domain = state.domain();
        let Some(person) = domain.sims.get(&sim) else {
            return Vec::new();
        };
        (domain.sims.iter())
            .filter(|(_, other)| other.sex != person.sex)
            .map(|(&other, _)| other)
            .filter(|&other| {
                let (woman, man) = match person.sex {
                    Sex::Female => (sim, other),
                    Sex::Male => (other, sim),
                };
                let pair = ArrangePartnership { woman, man };
                check_arrange(domain, state.data(), self.closed_through(), &pair).is_ok()
            })
            .collect()
    }
}

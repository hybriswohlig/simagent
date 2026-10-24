//! The partners a pairing input would be accepted with now (ADR-0018 sections 1 and 8,
//! ADR-0021 section 6, "ein Paar stiften"): the viewer offers only them, nearest in age
//! first, so it judges neither age nor kinship itself. Read only, like the rest of the
//! presentation state.

use alloc::vec::Vec;

use crate::genome::Sex;
use crate::ids::SimId;
use crate::input::ArrangePartnership;
use crate::state::partnership::check_arrange;
use crate::world::World;

use super::whole_years;

impl World {
    /// The living sims an `ArrangePartnership` of `sim` with them would pair in the next
    /// second: the other sex, both single and of the market age, not in direct line, not
    /// too closely related. Nearest in age first: by the difference of their whole years at
    /// the last closed second, then the smaller id (Grok's round 33, P3). Empty for a sim
    /// that is not alive or cannot be paired itself.
    pub fn partner_candidates(&self, sim: SimId) -> Vec<SimId> {
        let state = self.state();
        let domain = state.domain();
        let Some(person) = domain.sims.get(&sim) else {
            return Vec::new();
        };
        let now = self.closed_through();
        let own = whole_years(person.born, now);
        let mut found: Vec<(i64, SimId)> = (domain.sims.iter())
            .filter(|(_, other)| other.sex != person.sex)
            .filter(|&(&other, _)| {
                let (woman, man) = match person.sex {
                    Sex::Female => (sim, other),
                    Sex::Male => (other, sim),
                };
                let pair = ArrangePartnership { woman, man };
                check_arrange(domain, state.data(), now, &pair).is_ok()
            })
            .map(|(&other, them)| ((whole_years(them.born, now) - own).abs(), other))
            .collect();
        found.sort_unstable();
        found.into_iter().map(|(_, other)| other).collect()
    }
}

//! Who is raised in a house (ADR-0033 section 3): a person gets `raised_since` once, at the
//! first second in which it belongs, under 18, to a household that lives in a house: born into
//! one, its household moving in, or joining one. Later moves change nothing; a run without
//! `MoveIn` raises no one.

use crate::canonical::LoadedState;
use crate::development::{Lived, RAISED_UNTIL, Raised};
use crate::ids::SimId;
use crate::time::{DAY, SimTime};

use super::DomainState;

/// `sim` belongs from `now` on to a household that lives in a house: it is raised from now on
/// if it is under 18 and was not raised before.
pub(super) fn raise(domain: &mut DomainState, sim: SimId, now: SimTime) {
    let Some(person) = domain.sims.get(&sim) else {
        panic!("invariant raised.not_living");
    };
    let young = now
        .checked_since(person.born)
        .is_some_and(|age| age < RAISED_UNTIL);
    if young {
        domain
            .house
            .raised
            .entry(sim)
            .or_insert_with(|| Raised::new(now));
    }
}

impl DomainState {
    /// What `sim`, living or archived, lived through as the phenotype reads it;
    /// `Lived::NONE` for one never raised or unknown.
    pub(crate) fn lived(&self, sim: SimId) -> Lived {
        let born = (self.sims.get(&sim).map(|person| person.born))
            .or_else(|| self.archive.get(&sim).map(|person| person.born));
        match born {
            Some(born) => Lived::of(self.house.raised.get(&sim), born),
            None => Lived::NONE,
        }
    }
}

/// The loader's check: each person raised is living or archived, was raised at an age of 0
/// to under 18 and not after the second `now` the clock opens next, each `D` lies within its
/// bound, and no need was counted on a day after that of `now`. `now` stays a number of seconds: at the end of time it lies past the range.
///
/// And the converse: each living person under 18 at `now` whose household lives in a house has
/// been raised, for it was under 18 when it first belonged there.
pub(crate) fn consistent(loaded: &LoadedState, now: i64) -> bool {
    let fitting = loaded.house.raised.iter().all(|(sim, raised)| {
        let born = (loaded.sims.get(sim).map(|person| person.born))
            .or_else(|| loaded.archive.get(sim).map(|person| person.born));
        let today = now.div_euclid(DAY.as_secs());
        let counted = (raised.counted.iter().flatten()).all(|day| *day <= today);
        born.is_some_and(|born| raised.fits(born)) && raised.since.as_secs() <= now && counted
    });
    let complete = loaded.sims.iter().all(|(sim, person)| {
        let young = now.saturating_sub(person.born.as_secs()) < RAISED_UNTIL.as_secs();
        let housed = loaded.house.households.contains_key(&person.household);
        !(young && housed) || loaded.house.raised.contains_key(sim)
    });
    fitting && complete
}

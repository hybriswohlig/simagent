//! The domain state (ADR-0003 section 4). Domain code changes it only inside a handler
//! or an applied input and sees the rest of the world through `SimCtx`.

pub(crate) mod birthday;
pub(crate) mod census;
pub(crate) mod start;

use alloc::collections::BTreeMap;

use crate::ids::SimId;
use crate::time::{SimDuration, SimTime, YEAR};

pub(crate) struct Sim {
    pub(crate) born: SimTime,
}

/// Who left the active population stays resolvable by ID (ADR-0004 section 1). The
/// reason and the genome arrive with the rules that write them.
pub(crate) struct ArchivedSim {
    pub(crate) born: SimTime,
    pub(crate) left: SimTime,
}

pub(crate) struct DomainState {
    /// `None` until `Start` was applied.
    pub(crate) world_seed: Option<u64>,
    /// The living, by ID.
    pub(crate) sims: BTreeMap<SimId, Sim>,
    pub(crate) archive: BTreeMap<SimId, ArchivedSim>,
}

impl DomainState {
    pub(crate) const fn new() -> DomainState {
        DomainState {
            world_seed: None,
            sims: BTreeMap::new(),
            archive: BTreeMap::new(),
        }
    }
}

/// Whole years in a span that is not negative.
#[track_caller]
pub(crate) fn whole_years(elapsed: SimDuration) -> u32 {
    let years = elapsed.as_secs().div_euclid(YEAR.as_secs());
    u32::try_from(years).unwrap_or_else(|_| panic!("invariant age.out_of_range"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_years_rounds_down() {
        let year = YEAR.as_secs();
        for (secs, years) in [
            (0, 0),
            (year - 1, 0),
            (year, 1),
            (year + 1, 1),
            (30 * year, 30),
            (31 * year - 1, 30),
        ] {
            assert_eq!(whole_years(SimDuration::from_secs(secs)), years, "{secs}");
        }
    }

    #[test]
    #[should_panic(expected = "invariant age.out_of_range")]
    fn a_negative_span_has_no_age() {
        whole_years(SimDuration::from_secs(-1));
    }
}

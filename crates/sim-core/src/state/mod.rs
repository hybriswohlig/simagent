//! The domain state (ADR-0003 section 4). Domain code changes it only inside a handler
//! or an applied input and sees the rest of the world through `SimCtx`.

pub(crate) mod birthday;
pub(crate) mod census;
pub(crate) mod start;

use alloc::collections::BTreeMap;

use crate::ids::SimId;
use crate::time::{SimTime, YEAR};

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

/// Whole years from `born` to `now` (`born <= now`). Works on plain seconds: the span
/// between two valid times can be twice as long as the longest `SimDuration`.
#[track_caller]
pub(crate) fn whole_years(born: SimTime, now: SimTime) -> u32 {
    let years = (now.as_secs() - born.as_secs()).div_euclid(YEAR.as_secs());
    u32::try_from(years).unwrap_or_else(|_| panic!("invariant age.out_of_range"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_years_rounds_down() {
        let year = YEAR.as_secs();
        let born = SimTime::from_secs(-7);
        for (secs, years) in [
            (0, 0),
            (year - 1, 0),
            (year, 1),
            (year + 1, 1),
            (30 * year, 30),
            (31 * year - 1, 30),
        ] {
            let now = SimTime::from_secs(-7 + secs);
            assert_eq!(whole_years(born, now), years, "{secs}");
        }
    }

    /// The longest span the time range allows: 2 000 000 years.
    #[test]
    fn whole_years_spans_the_whole_time_range() {
        let limit = crate::time::MAX_ABS_SECS;
        let (first, last) = (SimTime::from_secs(-limit), SimTime::from_secs(limit));
        assert_eq!(whole_years(first, last), 2_000_000);
    }

    #[test]
    #[should_panic(expected = "invariant age.out_of_range")]
    fn a_negative_span_has_no_age() {
        whole_years(SimTime::from_secs(0), SimTime::from_secs(-1));
    }
}

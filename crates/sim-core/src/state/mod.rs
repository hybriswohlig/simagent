//! The domain state (ADR-0003 section 4). Domain code changes it only inside a handler
//! or an applied input and sees the rest of the world through `SimCtx`.

pub(crate) mod birthday;
pub(crate) mod census;
pub(crate) mod death;
pub(crate) mod params;
pub(crate) mod start;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::genome::{Genotype, Sex};
use crate::ids::SimId;
use crate::input::DeathCause;
use crate::lineage::{LineageIndex, LineageKey};
use crate::params::WorldParams;
use crate::time::{EventHandle, SimTime, YEAR};

#[derive(Serialize, Deserialize)]
pub(crate) struct Sim {
    pub(crate) born: SimTime,
    /// Who this is in every fork (ADR-0001 section 4); the ID is only a counter.
    pub(crate) lineage_key: LineageKey,
    pub(crate) sex: Sex,
    pub(crate) genotype: Genotype,
}

/// Why a sim left the active population.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum LeftBy {
    /// It died of the cause with this `str_key`.
    Death { cause: u64 },
}

/// Who left the active population stays resolvable by ID, with reason, time, lineage
/// key and genome (ADR-0004 section 1).
#[derive(Serialize, Deserialize)]
pub(crate) struct ArchivedSim {
    pub(crate) born: SimTime,
    pub(crate) left: SimTime,
    pub(crate) left_by: LeftBy,
    pub(crate) lineage_key: LineageKey,
    pub(crate) sex: Sex,
    pub(crate) genotype: Genotype,
}

pub(crate) struct DomainState {
    /// `None` until `Start` was applied.
    pub(crate) world_seed: Option<u64>,
    /// The parameters of `Start` after every override; `None` until `Start`.
    pub(crate) world_params: Option<WorldParams>,
    /// The causes of death of `Start`, in its order.
    pub(crate) death_causes: Vec<DeathCause>,
    /// The living, by ID.
    pub(crate) sims: BTreeMap<SimId, Sim>,
    pub(crate) archive: BTreeMap<SimId, ArchivedSim>,
    /// The lineage keys of the living and the archive. Not part of the projection:
    /// loading builds it again (ADR-0001 section 4).
    pub(crate) lineage: LineageIndex,
    /// The handle of every pending event of a sim, so that the events of a sim that
    /// dies can be cancelled. Not part of the projection: the queue holds the events,
    /// loading builds the index again.
    pub(crate) pending: BTreeMap<(SimId, Slot), EventHandle>,
}

/// What a pending event of a sim is. A sim has at most one of each.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Slot {
    Birthday,
    /// The death of the cause with this `str_key`.
    Death(u64),
}

impl DomainState {
    pub(crate) const fn new() -> DomainState {
        DomainState {
            world_seed: None,
            world_params: None,
            death_causes: Vec::new(),
            sims: BTreeMap::new(),
            archive: BTreeMap::new(),
            lineage: LineageIndex::new(),
            pending: BTreeMap::new(),
        }
    }
}

/// Whole years from `born` to `now` (`born <= now`). Works on plain seconds: the span
/// between two valid times can be twice as long as the longest `SimDuration`.
#[track_caller]
pub(crate) fn whole_years(born: SimTime, now: SimTime) -> u32 {
    years_between(born.as_secs(), now.as_secs())
        .unwrap_or_else(|| panic!("invariant age.out_of_range"))
}

/// Whole years between two seconds, `None` if `now_secs` is before `born_secs`. For
/// checking a save: neither value needs to be a valid time, and nothing panics.
pub(crate) fn years_between(born_secs: i64, now_secs: i64) -> Option<u32> {
    u32::try_from((now_secs - born_secs).div_euclid(YEAR.as_secs())).ok()
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
    fn years_between_has_no_answer_before_the_birth() {
        let year = YEAR.as_secs();
        assert_eq!(years_between(5, 4), None);
        assert_eq!(years_between(5, 5), Some(0));
        assert_eq!(years_between(-year, 0), Some(1));
        assert_eq!(years_between(0, -year), None);
    }

    #[test]
    #[should_panic(expected = "invariant age.out_of_range")]
    fn a_negative_span_has_no_age() {
        whole_years(SimTime::from_secs(0), SimTime::from_secs(-1));
    }
}

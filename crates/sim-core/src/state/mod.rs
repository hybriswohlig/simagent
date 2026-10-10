//! The domain state (ADR-0003 section 4). Domain code changes it only inside a handler
//! or an applied input and sees the rest of the world through `SimCtx`.

pub(crate) mod birth;
pub(crate) mod birthday;
pub(crate) mod census;
pub(crate) mod conception;
pub(crate) mod death;
pub(crate) mod disease;
pub(crate) mod params;
pub(crate) mod partnership;
pub(crate) mod start;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::finite::FiniteF64;
use crate::genome::{Genome, Sex};
use crate::ids::{PartnershipId, SimId};
use crate::input::Fertility;
use crate::lineage::{LineageIndex, LineageKey};
use crate::params::WorldParams;
use crate::time::{EventHandle, SimTime, YEAR};

#[derive(Serialize, Deserialize)]
pub(crate) struct Sim {
    pub(crate) born: SimTime,
    /// Who this is in every fork (ADR-0001 section 4); the ID is only a counter.
    pub(crate) lineage_key: LineageKey,
    pub(crate) sex: Sex,
    pub(crate) genome: Genome,
    /// The hereditary diseases it has, in the order of the data (ADR-0017 section 3).
    pub(crate) afflictions: Vec<Affliction>,
    /// How often she conceived (ADR-0001 section 4: the conception number of the next
    /// child); always 0 for a man.
    pub(crate) conceptions: u32,
    /// The biological parents of a born sim; `None` for a founder.
    pub(crate) parents: Option<Parents>,
}

/// The biological parents of a sim and the mother's conception number of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Parents {
    pub(crate) mother: SimId,
    pub(crate) father: SimId,
    pub(crate) conception: u32,
}

/// A running pregnancy, fixed at conception (B18): the father, the child's lineage key,
/// the mother's conception number, sex and genome.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Pregnancy {
    pub(crate) father: SimId,
    pub(crate) lineage_key: LineageKey,
    pub(crate) conception: u32,
    pub(crate) sex: Sex,
    pub(crate) genome: Genome,
    pub(crate) conceived: SimTime,
}

/// A hereditary disease a sim has (genetics specification 3.1, ADR-0017 section 3): the
/// key of its code, the age of its onset in years, and for a lethal one its exposure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Affliction {
    pub(crate) disease: u64,
    pub(crate) onset: FiniteF64,
    pub(crate) exposure: Option<Exposure>,
}

/// How much of the hazard of a lethal disease the switch has let act (C5): `used` in its
/// closed periods, `since` the start of the current one while the switch is on. The
/// hazard acts from the onset on, so a period before it uses nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Exposure {
    pub(crate) used: FiniteF64,
    pub(crate) since: Option<SimTime>,
}

/// A woman and a man paired by `Start` or `ArrangePartnership`. It ends with the first
/// death of the two and stays as history.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Partnership {
    pub(crate) woman: SimId,
    pub(crate) man: SimId,
    pub(crate) formed: SimTime,
    /// The second of the first death of the two; `None` while both live.
    pub(crate) ended: Option<SimTime>,
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
    pub(crate) genome: Genome,
    /// The afflictions as they were at the death: the exposures show whether the switch
    /// was on.
    pub(crate) afflictions: Vec<Affliction>,
    pub(crate) conceptions: u32,
    pub(crate) parents: Option<Parents>,
}

pub(crate) struct DomainState {
    /// `None` until `Start` was applied.
    pub(crate) world_seed: Option<u64>,
    /// The parameters of `Start` after every override; `None` until `Start`.
    pub(crate) world_params: Option<WorldParams>,
    /// The fertility of `Start`; `None` until `Start`.
    pub(crate) fertility: Option<Fertility>,
    /// The living, by ID.
    pub(crate) sims: BTreeMap<SimId, Sim>,
    pub(crate) partnerships: BTreeMap<PartnershipId, Partnership>,
    /// The running pregnancies, by mother.
    pub(crate) pregnancies: BTreeMap<SimId, Pregnancy>,
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
    /// The monthly series of conception attempts of a woman.
    Conception,
    /// The birth that ends her pregnancy.
    Birth,
    /// The death of the cause with this `str_key`.
    Death(u64),
}

impl DomainState {
    pub(crate) const fn new() -> DomainState {
        DomainState {
            world_seed: None,
            world_params: None,
            sims: BTreeMap::new(),
            fertility: None,
            pregnancies: BTreeMap::new(),
            partnerships: BTreeMap::new(),
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

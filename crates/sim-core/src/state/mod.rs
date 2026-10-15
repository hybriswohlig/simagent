//! The domain state (ADR-0003 section 4). Domain code changes it only inside a handler
//! or an applied input and sees the rest of the world through `SimCtx`.

pub(crate) mod arrival;
pub(crate) mod birth;
pub(crate) mod birthday;
pub(crate) mod census;
pub(crate) mod conception;
pub(crate) mod death;
pub(crate) mod disease;
pub(crate) mod emigration;
pub(crate) mod household;
pub(crate) mod market;
pub(crate) mod params;
pub(crate) mod partnership;
pub(crate) mod start;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::finite::FiniteF64;
use crate::genome::{Genome, Sex};
use crate::ids::{HouseholdId, PartnershipId, SimId};
use crate::kinship::KinshipCache;
use crate::lineage::{GenomeKey, LineageIndex, LineageKey};
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
    /// How many children she bore alive (ADR-0017 section 4: the wish reads it); always
    /// 0 for a man.
    pub(crate) births: u32,
    /// How many partnerships she formed (ADR-0018 section 3: the address of the budget of
    /// a separation); always 0 for a man.
    pub(crate) partnerships: u32,
    /// The household it lives in (ADR-0018 section 4).
    pub(crate) household: HouseholdId,
    /// Whether it has moved out: by leaving home, by a partnership, or as a founder of 18
    /// or in a couple; never falls back (ADR-0018 section 4).
    pub(crate) moved_out: bool,
    /// The person caring for it while it is dependent (under 18, not moved out); `None`
    /// otherwise, or alone in a household of its own (the orphan rule's last stage).
    pub(crate) carer: Option<SimId>,
    /// The biological parents of a born sim; `None` for a founder.
    pub(crate) parents: Option<Parents>,
    /// Who took it in by the orphan rule (a relative, or a foster mother and her partner),
    /// in the order they became its social parents, each once (ADR-0018 section 4).
    pub(crate) social_parents: Vec<SimId>,
    /// Where a pool person came from (ADR-0020 section 5); `None` for a town person.
    pub(crate) pool: Option<PoolOrigin>,
}

/// What a pool person brings from the pool (design note of the arrival, section 6.1): its
/// relations as lineage keys, since its parents may never come into the town.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PoolOrigin {
    pub(crate) genome_key: GenomeKey,
    /// Its birth second in the pool run; `None` for a child of a pool pregnancy born in
    /// the town.
    pub(crate) born_pool: Option<SimTime>,
    /// The second it arrived, or the second of its birth for a pool child.
    pub(crate) arrived: SimTime,
    /// The year it arrived (0 for founders); for a pool child the year of its birth.
    pub(crate) arrival_year: u64,
    pub(crate) mother: Option<LineageKey>,
    pub(crate) father: Option<LineageKey>,
    pub(crate) social_parents: Vec<LineageKey>,
}

/// The seeds of a person's draws (design note of the arrival, section 6.2): `E`, `C`,
/// and its own life draws (the wish and the menarche offset, penetrance and onset).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Seeds {
    pub(crate) individual: u64,
    pub(crate) shared: u64,
    pub(crate) life: u64,
}

impl Seeds {
    pub(crate) const fn one(seed: u64) -> Seeds {
        Seeds {
            individual: seed,
            shared: seed,
            life: seed,
        }
    }
}

/// The biological parents of a sim and the mother's conception number of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Parents {
    pub(crate) mother: SimId,
    pub(crate) father: SimId,
    pub(crate) conception: u32,
}

/// A running pregnancy, fixed at conception (B18): the father, the mother's conception
/// number, the one or two children (spec 11.3) and when she conceived.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Pregnancy {
    /// The father, if he is a person of the town; a pregnancy brought from the pool may
    /// have a father who never came (design note of the arrival, section 5).
    pub(crate) father: Option<SimId>,
    /// The father's lineage key for a pregnancy from the pool; `None` otherwise.
    pub(crate) pool_father: Option<LineageKey>,
    pub(crate) conception: u32,
    /// In the order of their multiple index (ADR-0001 section 4).
    pub(crate) children: Vec<Unborn>,
    pub(crate) conceived: SimTime,
}

/// A child of a pregnancy: its lineage key, sex and genome, fixed at conception.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Unborn {
    pub(crate) lineage_key: LineageKey,
    pub(crate) sex: Sex,
    pub(crate) genome: Genome,
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
/// death of the two or with their separation (ADR-0018 section 3) and stays as history.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Partnership {
    pub(crate) woman: SimId,
    pub(crate) man: SimId,
    pub(crate) formed: SimTime,
    /// How it ended; `None` while it is open.
    pub(crate) ended: Option<Ending>,
}

/// The end of a partnership: the second and the reason.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Ending {
    pub(crate) at: SimTime,
    pub(crate) by: EndedBy,
}

/// Why a partnership ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum EndedBy {
    /// The first death of the two.
    Death,
    /// Their separation came due (ADR-0018 section 3).
    Separation,
    /// Both left the town with their household (ADR-0020 section 6).
    Emigration,
}

/// Why a sim left the active population.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum LeftBy {
    /// It died of the cause with this `str_key`.
    Death { cause: u64 },
    /// It left the town with its household above the soft capacity (ADR-0020 section 6).
    Emigration,
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
    pub(crate) births: u32,
    pub(crate) partnerships: u32,
    /// The household it lived in last.
    pub(crate) household: HouseholdId,
    pub(crate) moved_out: bool,
    pub(crate) parents: Option<Parents>,
    pub(crate) social_parents: Vec<SimId>,
    pub(crate) pool: Option<PoolOrigin>,
}

/// A household (ADR-0018 section 4): from its forming until its last member leaves; it
/// stays as history.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Household {
    pub(crate) formed: SimTime,
    /// The second its last member left; `None` while it has one.
    pub(crate) ended: Option<SimTime>,
}

pub(crate) struct DomainState {
    /// `None` until `Start` was applied.
    pub(crate) world_seed: Option<u64>,
    /// The parameters of `Start` after every override; `None` until `Start`.
    pub(crate) world_params: Option<WorldParams>,
    /// The living, by ID.
    pub(crate) sims: BTreeMap<SimId, Sim>,
    pub(crate) partnerships: BTreeMap<PartnershipId, Partnership>,
    pub(crate) households: BTreeMap<HouseholdId, Household>,
    /// The running pregnancies, by mother.
    pub(crate) pregnancies: BTreeMap<SimId, Pregnancy>,
    pub(crate) archive: BTreeMap<SimId, ArchivedSim>,
    /// The households of the data's pool that came into the town, as their indices in the
    /// pool file, in arrival order (founders first; ADR-0020 section 5).
    pub(crate) pool_used: Vec<u32>,
    /// The lineage keys of the living and the archive. Not part of the projection:
    /// loading builds it again (ADR-0001 section 4).
    pub(crate) lineage: LineageIndex,
    /// The `SimId` of every person of the town, living or archived, by its lineage key
    /// (`φ` over lineage keys, ADR-0020 section 4). Not part of the projection: loading
    /// builds it again.
    pub(crate) keys: BTreeMap<LineageKey, SimId>,
    /// The handle of every pending event of a sim, so that the events of a sim that
    /// dies can be cancelled. Not part of the projection: the queue holds the events,
    /// loading builds the index again.
    pub(crate) pending: BTreeMap<(SimId, Slot), EventHandle>,
    /// The open partnership of each sim in one (ADR-0018 section 2). Not part of the
    /// projection: loading builds it again from the partnerships.
    pub(crate) open: BTreeMap<SimId, PartnershipId>,
    /// The working memory of the φ queries (ADR-0018 section 2), empty at each query
    /// (ADR-0020 section 4). Not part of the projection and not saved.
    pub(crate) kinship: KinshipCache<LineageKey>,
    /// The living members of each household that has not ended (ADR-0018 section 4). Not
    /// part of the projection: loading builds it again from the sims.
    pub(crate) members: BTreeMap<HouseholdId, BTreeSet<SimId>>,
}

/// What a pending event of a sim is. A sim has at most one of each.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Slot {
    Birthday,
    /// The monthly series of conception attempts of a woman.
    Conception,
    /// The birth that ends her pregnancy.
    Birth,
    /// The separation of her open partnership (ADR-0018 section 3).
    Separation,
    /// Leaving home at the drawn age (ADR-0018 section 4).
    LeaveHome,
    /// The death of the cause with this `str_key`.
    Death(u64),
}

impl DomainState {
    /// Where the person with this ID came from, living or archived; `None` for a town
    /// person (and for an unknown ID).
    pub(crate) fn pool_origin(&self, sim: SimId) -> Option<&PoolOrigin> {
        self.sims
            .get(&sim)
            .and_then(|person| person.pool.as_ref())
            .or_else(|| {
                self.archive
                    .get(&sim)
                    .and_then(|person| person.pool.as_ref())
            })
    }

    /// The lineage key of a person of the town, living or archived.
    pub(crate) fn key_of(&self, sim: SimId) -> LineageKey {
        self.sims
            .get(&sim)
            .map(|person| person.lineage_key)
            .or_else(|| self.archive.get(&sim).map(|person| person.lineage_key))
            .unwrap_or_else(|| panic!("invariant unknown_entity_id: sim {}", sim.get()))
    }

    /// The lineage keys of the biological parents of a person, living or archived: a pool
    /// person's from the pool, a town person's from its parents (design note of the
    /// arrival, section 6.2); `None` for a founder and for a pool person without both.
    pub(crate) fn parent_keys(&self, sim: SimId) -> Option<crate::phenotype::Parents> {
        if let Some(from) = self.pool_origin(sim) {
            return from
                .mother
                .zip(from.father)
                .map(|(mother, father)| crate::phenotype::Parents { mother, father });
        }
        let parents = self
            .sims
            .get(&sim)
            .and_then(|person| person.parents)
            .or_else(|| self.archive.get(&sim).and_then(|person| person.parents))?;
        Some(crate::phenotype::Parents {
            mother: self.key_of(parents.mother),
            father: self.key_of(parents.father),
        })
    }

    /// The seeds of a person's draws (design note of the arrival, section 6.2): a pool
    /// person draws everything with the pool seed; a town person whose two biological
    /// parents are pool persons draws `C` with it; everyone else with the world seed.
    pub(crate) fn seeds(&self, data: &crate::data::Data, sim: SimId) -> Seeds {
        let world = self
            .world_seed
            .unwrap_or_else(|| panic!("invariant seeds.before_start"));
        let pool = data.pool().map(|held| held.pool().pool_seed);
        if self.pool_origin(sim).is_some() {
            return Seeds::one(pool.unwrap_or_else(|| panic!("invariant pool.missing")));
        }
        let parents = self
            .sims
            .get(&sim)
            .and_then(|person| person.parents)
            .or_else(|| self.archive.get(&sim).and_then(|person| person.parents));
        match (parents, pool) {
            (Some(p), Some(pool))
                if self.pool_origin(p.mother).is_some() && self.pool_origin(p.father).is_some() =>
            {
                Seeds {
                    individual: world,
                    shared: pool,
                    life: world,
                }
            }
            _ => Seeds::one(world),
        }
    }
}

impl DomainState {
    pub(crate) const fn new() -> DomainState {
        DomainState {
            world_seed: None,
            world_params: None,
            sims: BTreeMap::new(),
            pregnancies: BTreeMap::new(),
            partnerships: BTreeMap::new(),
            households: BTreeMap::new(),
            archive: BTreeMap::new(),
            pool_used: Vec::new(),
            lineage: LineageIndex::new(),
            keys: BTreeMap::new(),
            pending: BTreeMap::new(),
            open: BTreeMap::new(),
            kinship: KinshipCache::new(),
            members: BTreeMap::new(),
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

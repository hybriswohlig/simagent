//! Saving and loading (ADR-0006): header, the canonical projection, the saved
//! exceptions. The bytes are uncompressed and fully deterministic.

use crate::data::Data;
use alloc::borrow::Cow;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::ctx::{IdCounters, LogBuffer};
use super::{EventPayload, InputLog, World, WorldState};
use crate::canonical::{self, LoadedState, PROJECTION_VERSION, QueueEntry, YearHash};
use crate::finite::FiniteF64;
use crate::genome::meiosis::Zygote;
use crate::genome::{Genome, Sex};
use crate::ids::{Counter, HouseholdId, PartnershipId, SimId};
use crate::input::{Input, InputRecord, PoolStart, Start};
use crate::kinship::{Kin, KinshipCache};
use crate::lineage::GenomeKey;
use crate::lineage::{LineageIndex, LineageKey};
use crate::log::PlanRef;
use crate::params::WorldParams;
use crate::pool::PoolPerson;
use crate::rng::{Draws, WorldSeed};
use crate::state::conception;
use crate::state::{
    self, Affliction, ArchivedSim, DomainState, EndedBy, Ending, Exposure, LeftBy, Parents,
    Partnership, Pregnancy, Sim, Slot,
};
use crate::time::{
    EventHandle, MONTH, Scheduler, SchedulerSnapshot, SimDuration, SimTime, SnapshotEntry, YEAR,
};
use crate::trace::TraceBuffer;
use crate::version::CORE_VERSION;

/// Rises with every change of the layout of a save (ADR-0006 section 2).
pub const FORMAT_VERSION: u32 = 2;

/// The first eight bytes of every save.
const MAGIC: [u8; 8] = *b"SIMAGENT";

/// Why a world cannot be saved right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SaveError {
    /// The log buffer still holds entries; take them first (ADR-0004 section 6).
    LogNotTaken,
    /// Not exactly the year hashes of the completed years (ADR-0006 section 3).
    YearHashesMismatch,
}

/// Why bytes are not a save of this core (ADR-0006 section 4), in checking order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadError {
    BadMagic,
    ForeignFormatVersion,
    ForeignCoreVersion,
    ForeignDataHash,
    ForeignProjectionVersion,
    Malformed,
    InvalidQueue,
    Inconsistent,
}

/// The third part of a save: what the projection leaves out and a continued run needs
/// (ADR-0003 5.3, ADR-0006 section 2).
#[derive(Serialize, Deserialize)]
struct Exceptions {
    /// One per pending event, in queue order.
    plan_refs: Vec<PlanRef>,
    next_event_id: u64,
    input_log: Vec<InputRecord>,
    year_hashes: Vec<YearHash>,
}

fn encode<T: Serialize>(value: &T) -> Vec<u8> {
    postcard::to_allocvec(value).unwrap_or_else(|_| panic!("invariant save.encode"))
}

/// The header: the magic bytes, then three numbers.
fn header_bytes(data_hash: u64) -> Vec<u8> {
    let mut bytes = MAGIC.to_vec();
    bytes.extend(encode(&FORMAT_VERSION));
    bytes.extend(encode(&CORE_VERSION));
    bytes.extend(encode(&data_hash));
    bytes
}

/// Reads one value from the front; a cut or unreadable field is `Malformed`.
fn take<T: DeserializeOwned>(bytes: &[u8]) -> Result<(T, &[u8]), LoadError> {
    postcard::take_from_bytes(bytes).map_err(|_| LoadError::Malformed)
}

/// The number of year ends up to and including `closed_through`; a world that never
/// started has none.
fn completed_years(started: bool, closed_through: SimTime) -> i64 {
    if started {
        (closed_through.as_secs() + 1).div_euclid(YEAR.as_secs())
    } else {
        0
    }
}

/// Exactly the hashes of the years 0 to n - 1, in order (ADR-0006 section 3).
fn year_hashes_complete(started: bool, closed_through: SimTime, hashes: &[YearHash]) -> bool {
    let years = completed_years(started, closed_through);
    i64::try_from(hashes.len()).is_ok_and(|count| count == years)
        && hashes.iter().zip(0..).all(|(hash, year)| hash.year == year)
}

impl World {
    /// The uncompressed save of this world. `year_hashes` come from the caller's
    /// `YearOut`, which the core does not keep.
    pub fn save(&self, year_hashes: &[YearHash]) -> Result<Vec<u8>, SaveError> {
        if !self.state.log.is_empty() {
            return Err(SaveError::LogNotTaken);
        }
        let started = self.state.domain.world_seed.is_some();
        if !year_hashes_complete(started, self.sched.closed_through(), year_hashes) {
            return Err(SaveError::YearHashesMismatch);
        }
        let exceptions = Exceptions {
            plan_refs: self
                .sched
                .snapshot()
                .entries
                .into_iter()
                .map(|entry| entry.meta)
                .collect(),
            next_event_id: self.state.log.next_event(),
            input_log: self.inputs.records.clone(),
            year_hashes: year_hashes.to_vec(),
        };
        let mut bytes = header_bytes(self.state.data.hash());
        bytes.extend(canonical::canonical_bytes(&self.state, &self.sched));
        bytes.extend(encode(&exceptions));
        Ok(bytes)
    }

    /// Rebuilds a world from a save with the definitions the caller has loaded; the
    /// save must carry their data hash (ADR-0007). Reads field by field; the first
    /// violation in the order of ADR-0006 section 4 names the error.
    pub fn load(bytes: &[u8], data: Data) -> Result<(World, Vec<YearHash>), LoadError> {
        let data_hash = data.hash();
        let rest = bytes.strip_prefix(&MAGIC).ok_or(LoadError::BadMagic)?;
        let (format_version, rest) = take::<u32>(rest)?;
        if format_version != FORMAT_VERSION {
            return Err(LoadError::ForeignFormatVersion);
        }
        let (core_version, rest) = take::<u32>(rest)?;
        if core_version != CORE_VERSION {
            return Err(LoadError::ForeignCoreVersion);
        }
        let (saved_data_hash, rest) = take::<u64>(rest)?;
        if saved_data_hash != data_hash {
            return Err(LoadError::ForeignDataHash);
        }
        // The projection version decides the layout of everything behind it.
        let (projection_version, _) = take::<u32>(rest)?;
        if projection_version != PROJECTION_VERSION {
            return Err(LoadError::ForeignProjectionVersion);
        }
        let (loaded, rest) = take::<LoadedState>(rest)?;
        let (exceptions, rest) = take::<Exceptions>(rest)?;
        // Canonical: the parts as read must encode to exactly the bytes that were read.
        // serde and postcard accept repeated or unsorted map keys and overlong varints;
        // bytes behind the third part show here as well.
        let _ = rest;
        let mut canonical = header_bytes(data_hash);
        canonical.extend(encode(&loaded));
        canonical.extend(encode(&exceptions));
        if canonical != bytes {
            return Err(LoadError::Malformed);
        }
        if exceptions.plan_refs.len() != loaded.queue.len()
            || loaded.queue.iter().any(|entry| entry.seq == 0)
        {
            return Err(LoadError::InvalidQueue);
        }
        let entries = loaded
            .queue
            .iter()
            .zip(&exceptions.plan_refs)
            .map(|(entry, meta)| SnapshotEntry {
                time: entry.time,
                micro: entry.micro,
                seq: entry.seq,
                payload: entry.payload.clone(),
                period: entry.period,
                meta: *meta,
            })
            .collect();
        let sched = Scheduler::restore(SchedulerSnapshot {
            closed_through: loaded.closed_through,
            next_seq: loaded.next_event_seq,
            entries,
        })
        .map_err(|_| LoadError::InvalidQueue)?;
        if !consistent(&loaded, &exceptions, &data) {
            return Err(LoadError::Inconsistent);
        }
        // `consistent` has checked the ranges of the three counters.
        let counter = |next: u64| Counter::from_next(next).ok_or(LoadError::Inconsistent);
        let next_input = u64::try_from(exceptions.input_log.len())
            .ok()
            .and_then(|len| len.checked_add(1))
            .ok_or(LoadError::Inconsistent)?;
        // Every sim is a founder with the key of its index (`consistent`), so the keys
        // differ exactly where the uninterrupted run had them differ.
        let lineage = lineage_index(&loaded.sims, &loaded.archive, &loaded.pregnancies);
        let pending = pending_index(&loaded.queue, &loaded.partnerships);
        let open = open_index(&loaded.partnerships);
        let members = member_index(&loaded.sims);
        let keys = key_index(&loaded.sims, &loaded.archive);
        let world = World {
            sched,
            state: WorldState {
                domain: DomainState {
                    world_seed: loaded.world_seed,
                    world_params: loaded.world_params,
                    sims: loaded.sims,
                    partnerships: loaded.partnerships,
                    households: loaded.households,
                    pregnancies: loaded.pregnancies,
                    archive: loaded.archive,
                    pool_used: loaded.pool_used,
                    lineage,
                    keys,
                    pending,
                    open,
                    kinship: KinshipCache::new(),
                    members,
                },
                ids: IdCounters::restore(
                    counter(loaded.next_sim_id)?,
                    counter(loaded.next_partnership_id)?,
                    counter(loaded.next_household_id)?,
                ),
                log: LogBuffer::restore(counter(exceptions.next_event_id)?),
                trace: TraceBuffer::default(),
                data,
            },
            inputs: InputLog {
                next_seq: counter(next_input)?,
                records: exceptions.input_log,
            },
        };
        Ok((world, exceptions.year_hashes))
    }
}

/// The lineage keys of a saved state: those of the living and of the archive (ADR-0001
/// section 4). The index is not saved, loading builds it again.
pub(super) fn lineage_index(
    sims: &BTreeMap<SimId, Sim>,
    archive: &BTreeMap<SimId, ArchivedSim>,
    pregnancies: &BTreeMap<SimId, Pregnancy>,
) -> LineageIndex {
    let mut index = LineageIndex::new();
    for key in lineage_keys(sims, archive, pregnancies) {
        index.insert(key);
    }
    index
}

/// The lineage keys of the living, the archive and the running pregnancies.
fn lineage_keys<'a>(
    sims: &'a BTreeMap<SimId, Sim>,
    archive: &'a BTreeMap<SimId, ArchivedSim>,
    pregnancies: &'a BTreeMap<SimId, Pregnancy>,
) -> impl Iterator<Item = LineageKey> + 'a {
    let living = sims.values().map(|sim| sim.lineage_key);
    let archived = archive.values().map(|sim| sim.lineage_key);
    let unborn = pregnancies
        .values()
        .flat_map(|pregnancy| pregnancy.children.iter().map(|child| child.lineage_key));
    living.chain(archived).chain(unborn)
}

/// The handle of every pending event of a sim, from the queue: the handle is the
/// insertion number (ADR-0002). The index is not saved, loading builds it again. The
/// separation of a partnership is a pending event of its woman.
pub(super) fn pending_index(
    queue: &[QueueEntry],
    partnerships: &BTreeMap<PartnershipId, Partnership>,
) -> BTreeMap<(SimId, Slot), EventHandle> {
    let mut index = BTreeMap::new();
    for entry in queue {
        let slot = match &entry.payload {
            EventPayload::YearEnd
            | EventPayload::MarketRound
            | EventPayload::Emigration { .. }
            | EventPayload::Immigration { .. } => {
                continue;
            }
            EventPayload::Birthday { sim } => (*sim, Slot::Birthday),
            EventPayload::Death { sim, cause } => (*sim, Slot::Death(*cause)),
            EventPayload::ConceptionAttempt { woman } => (*woman, Slot::Conception),
            EventPayload::Birth { mother } => (*mother, Slot::Birth),
            EventPayload::LeaveHome { sim } => (*sim, Slot::LeaveHome),
            EventPayload::Separation { partnership } => {
                let Some(partnership) = partnerships.get(partnership) else {
                    panic!("invariant save.separation_of_unknown_partnership");
                };
                (partnership.woman, Slot::Separation)
            }
        };
        index.insert(slot, EventHandle::from_seq(entry.seq));
    }
    index
}

/// Whether the saved state is one the core can reach, as far as that shows without a
/// replay (ADR-0006 section 5). The queue has passed the scheduler's own checks. The
/// ranges of the ID counters are checked when the world is built from them.
fn consistent(loaded: &LoadedState, exceptions: &Exceptions, data: &Data) -> bool {
    let closed = loaded.closed_through.as_secs();
    if closed < SimTime::START.as_secs() - 1 {
        return false;
    }
    let now = closed + 1;
    let Some(seed) = loaded.world_seed else {
        // Never started: the world is empty. The queue is empty too: an event needs an
        // insertion number below the counter, which stands at 1.
        let empty = loaded.world_params.is_none()
            && loaded.pool_used.is_empty()
            && loaded.sims.is_empty()
            && loaded.partnerships.is_empty()
            && loaded.pregnancies.is_empty()
            && loaded.archive.is_empty()
            && loaded.households.is_empty()
            && loaded.next_sim_id == 1
            && loaded.next_partnership_id == 1
            && loaded.next_household_id == 1
            && loaded.next_event_seq == 1
            && exceptions.next_event_id == 1
            && exceptions.input_log.is_empty()
            && exceptions.year_hashes.is_empty();
        return empty;
    };
    let Some(history) = input_log_consistent(loaded, exceptions, seed, data) else {
        return false;
    };
    let Some(view) = pool_view(loaded, data, seed, history.pool.as_ref(), now) else {
        return false;
    };
    let rules = Rules {
        seed,
        data,
        switch: &history.switch,
        capacity: history.capacity,
        pool: &view,
        pairs: &history.pairs,
    };
    // The other checks leave no two equal keys short of a 64-bit collision; this one
    // keeps building the index from a panic.
    let keys = lineage_keys(&loaded.sims, &loaded.archive, &loaded.pregnancies);
    let unborn: usize = loaded
        .pregnancies
        .values()
        .map(|pregnancy| pregnancy.children.len())
        .sum();
    let distinct =
        keys.collect::<BTreeSet<_>>().len() == loaded.sims.len() + loaded.archive.len() + unborn;
    distinct
        && born_consistent(loaded, now, &rules, history.founders)
        && twins_complete(loaded, &rules)
        && sims_consistent(loaded, now, &rules)
        && partnerships_consistent(loaded, now, &rules, &history.pairs)
        && partnership_counts_consistent(loaded, &rules)
        && pregnancies_consistent(loaded, now, &rules)
        && conceptions_consistent(loaded, &rules)
        && births_consistent(loaded, &rules)
        && households_consistent(loaded, &rules)
        && queue_consistent(loaded, now, &rules)
        && carried_plans_consistent(loaded, &exceptions.plan_refs, &rules, now)
        && exceptions.plan_refs.iter().all(|plan| {
            plan.by.is_below(exceptions.next_event_id) && plan.planned_at.as_secs() <= now
        })
        && year_hashes_complete(true, loaded.closed_through, &exceptions.year_hashes)
}

/// What the loader knows of the pool persons of a save (design note of the arrival,
/// section 9), after `pool_view` has checked them against the data's pool.
#[derive(Default)]
struct PoolView<'a> {
    /// Each arrival: its record (the pool's, or for a village the person as it arrived,
    /// alone and without carried plans) and the shift `Δ` of its household.
    arrivals: BTreeMap<SimId, (Cow<'a, PoolPerson>, i64)>,
    /// The children of pool pregnancies born in the town.
    children: BTreeSet<SimId>,
    /// The partnerships the arrivals brought: the woman's record and the shift.
    carried: BTreeMap<PartnershipId, (&'a PoolPerson, i64)>,
    /// The pregnancies brought from the pool, by mother: her record and the shift.
    pregnancies: BTreeMap<SimId, (&'a PoolPerson, i64)>,
    /// The persons of each year's immigration, if it runs.
    immigration: Option<u64>,
    /// Each arrival's place: its household's among the used ones, its own in the household.
    places: BTreeMap<SimId, (usize, usize)>,
}

impl PoolView<'_> {
    /// Whether the sim came from the pool (an arrival or a pool child).
    fn came_from_pool(&self, sim: SimId) -> bool {
        self.arrivals.contains_key(&sim) || self.children.contains(&sim)
    }
}

/// A sim's origin in the pool, living or archived.
fn pool_of(loaded: &LoadedState, id: SimId) -> Option<&state::PoolOrigin> {
    loaded
        .sims
        .get(&id)
        .and_then(|sim| sim.pool.as_ref())
        .or_else(|| loaded.archive.get(&id).and_then(|sim| sim.pool.as_ref()))
}

/// The pool persons of the save against the data's pool (design note of the arrival,
/// section 9): `None` if anything is off. Without a pool start nobody came from the pool
/// and every pregnancy has its father. With one, the used households are distinct, the
/// founders' are the draw of `Start`; their members are exactly the arrivals, each equal to
/// its record (sex, genome, birth in the pool, keys) and shifted by its household's
/// `Δ = arrived − E_p`, arrived in a closed year start with that year; each pool child is an
/// unborn child of the pool born in the town of an arrival; a pregnancy from the pool is
/// the arrival's, shifted, its father a person of the town only under his key; a
/// partnership an arrival brought is the household's, formed at its pool second plus `Δ`.
fn pool_view<'a>(
    loaded: &LoadedState,
    data: &'a Data,
    seed: u64,
    start: Option<&PoolStart>,
    now: i64,
) -> Option<PoolView<'a>> {
    let mut view = PoolView::default();
    let Some(start) = start else {
        let none = loaded.sims.values().all(|sim| sim.pool.is_none())
            && loaded.archive.values().all(|sim| sim.pool.is_none())
            && loaded.pool_used.is_empty()
            && loaded.pregnancies.values().all(|p| p.pool_father.is_none());
        return none.then_some(view);
    };
    let held = data.pool()?;
    let pool = held.pool();
    // A village (ADR-0020 section 8, G4): its persons, each alone, arrived in second 0; no
    // household of the pool is used and nobody immigrates.
    let village = if start.village {
        if start.immigration || start.n_start % 2 != 0 {
            return None;
        }
        crate::pool::village_households(pool, seed, start.n_start / 2)?
    } else {
        Vec::new()
    };
    let persons = start.n_start.saturating_add(50) / 100;
    view.immigration = (start.immigration && persons > 0).then_some(persons);
    // The used households are exactly the draws, in their order (review): those of
    // `Start`, arriving in second 0 while the clock may still stand before it, then those
    // of each immigration whose second the clock has passed. Each arrives in the second of
    // its draw; a draw the pool could not fill stopped the run, so no save holds it.
    let mut drawn: Vec<(usize, i64)> = if start.village {
        Vec::new()
    } else {
        crate::pool::take_households(pool, seed, 0, &BTreeSet::new(), start.n_start)
            .0
            .into_iter()
            .map(|index| (index, 0))
            .collect()
    };
    if let Some(persons) = view.immigration {
        let mut taken: BTreeSet<usize> = drawn.iter().map(|(index, _)| *index).collect();
        let mut year = 0_u64;
        loop {
            let at = i64::try_from(year).ok()?.checked_mul(YEAR.as_secs())?;
            if at >= now {
                break;
            }
            let (more, enough) = crate::pool::take_households(pool, seed, year, &taken, persons);
            if !enough {
                return None;
            }
            taken.extend(more.iter().copied());
            drawn.extend(more.into_iter().map(|index| (index, at)));
            year = year.checked_add(1)?;
        }
    }
    let used_fit = loaded.pool_used.len() == drawn.len()
        && loaded
            .pool_used
            .iter()
            .zip(&drawn)
            .all(|(used, (index, _))| usize::try_from(*used).ok() == Some(*index));
    if !used_fit {
        return None;
    }
    let keys = key_index(&loaded.sims, &loaded.archive);
    // Each pool key's place in the order of arrival: its household's place among the used.
    let mut place_of: BTreeMap<LineageKey, usize> = BTreeMap::new();
    let mut arrivals_seen = 0_usize;
    let arrived_households = drawn
        .iter()
        .map(|(index, second)| (Cow::Borrowed(&pool.households[*index]), *second))
        .chain(
            village
                .into_iter()
                .map(|household| (Cow::Owned(household), 0)),
        );
    for (place, (household, second)) in arrived_households.enumerate() {
        let on_time = |at: SimTime| at.as_secs() == second;
        let mut arrived: Option<SimTime> = None;
        for (member_place, member) in household.members.iter().enumerate() {
            let id = *keys.get(&member.lineage_key)?;
            let facts = facts(loaded, id)?;
            let origin = pool_of(loaded, id)?;
            let at = *arrived.get_or_insert(origin.arrived);
            let delta = at.as_secs().checked_sub(pool.frozen_at.as_secs())?;
            let born = member.born.as_secs().checked_add(delta)?;
            let fits = origin.arrived == at
                && on_time(at)
                && u64::try_from(at.as_secs() / YEAR.as_secs()).ok() == Some(origin.arrival_year)
                && origin.born_pool == Some(member.born)
                && origin.genome_key == member.genome_key
                && origin.mother == member.mother
                && origin.father == member.father
                && origin.social_parents == member.social_parents
                && facts.born.as_secs() == born
                && facts.sex == member.sex
                && facts.genome == member.genome
                && facts.parents.is_none();
            if !fits {
                return None;
            }
            let record = match &household {
                Cow::Borrowed(household) => Cow::Borrowed(&household.members[member_place]),
                Cow::Owned(_) => Cow::Owned(member.clone()),
            };
            view.arrivals.insert(id, (record, delta));
            view.places.insert(id, (place, member_place));
            place_of.insert(member.lineage_key, place);
            arrivals_seen += 1;
        }
        let delta = arrived?.as_secs().checked_sub(pool.frozen_at.as_secs())?;
        // A village's persons bring no partnership and no pregnancy.
        let Cow::Borrowed(household) = household else {
            continue;
        };
        if let Some(pair) = household.partnership {
            let (woman, man) = (*keys.get(&pair.woman)?, *keys.get(&pair.man)?);
            let formed = pair.formed.as_secs().checked_add(delta)?;
            let carried = loaded
                .partnerships
                .iter()
                .find(|(_, p)| p.woman == woman && p.man == man && p.formed.as_secs() == formed);
            let (id, _) = carried?;
            let record = household
                .members
                .iter()
                .find(|m| m.lineage_key == pair.woman)?;
            view.carried.insert(*id, (record, delta));
        }
        for pregnancy in &household.pregnancies {
            let mother = *keys.get(&pregnancy.mother)?;
            let record = household
                .members
                .iter()
                .find(|m| m.lineage_key == pregnancy.mother)?;
            view.pregnancies.insert(mother, (record, delta));
        }
    }
    // Everyone with a pool birth is an arrival of a used household.
    let with_pool_birth = loaded
        .sims
        .values()
        .map(|sim| sim.pool.as_ref())
        .chain(loaded.archive.values().map(|sim| sim.pool.as_ref()))
        .flatten()
        .filter(|origin| origin.born_pool.is_some())
        .count();
    if with_pool_birth != arrivals_seen {
        return None;
    }
    // The pool children.
    let others = loaded
        .sims
        .iter()
        .map(|(id, sim)| (*id, sim.pool.as_ref(), sim.born))
        .chain(
            loaded
                .archive
                .iter()
                .map(|(id, sim)| (*id, sim.pool.as_ref(), sim.born)),
        );
    for (id, origin, born) in others {
        let Some(origin) = origin.filter(|origin| origin.born_pool.is_none()) else {
            continue;
        };
        let facts = facts(loaded, id)?;
        let unborn = held.unborn(facts.lineage_key)?;
        let (pregnancy_mother, pregnancy_father) = pool
            .households
            .iter()
            .flat_map(|h| h.pregnancies.iter())
            .find(|p| {
                p.children
                    .iter()
                    .any(|c| c.lineage_key == facts.lineage_key)
            })
            .map(|p| (p.mother, p.father))?;
        let mother = *keys.get(&pregnancy_mother)?;
        let left = loaded.archive.get(&mother).map(|her| her.left);
        let fits = origin.genome_key == unborn.genome_key
            && origin.arrived == born
            && u64::try_from(born.as_secs().div_euclid(YEAR.as_secs())).ok()
                == Some(origin.arrival_year)
            && origin.mother == Some(pregnancy_mother)
            && origin.father == Some(pregnancy_father)
            && origin.social_parents.is_empty()
            && facts.sex == unborn.sex
            && facts.genome == unborn.genome
            && facts.parents.is_none()
            && born.as_secs() < now
            && view.arrivals.contains_key(&mother)
            && lived_at(left, born);
        if !fits {
            return None;
        }
        view.children.insert(id);
    }
    // Each pregnancy of the used households is pending while its mother lives, born in
    // its second with all its children one after the other, or ended with her before it
    // (design note of the carried plans, C3).
    for (mother, (record, delta)) in &view.pregnancies {
        let birth = SimTime::from_secs(record.birth_at?.as_secs().checked_add(*delta)?);
        let carried = pool
            .households
            .iter()
            .flat_map(|h| h.pregnancies.iter())
            .find(|p| p.mother == record.lineage_key)?;
        let left = loaded.archive.get(mother).map(|her| her.left);
        let fits = if !lived_at(left, birth) {
            carried
                .children
                .iter()
                .all(|child| !keys.contains_key(&child.lineage_key))
        } else if birth.as_secs() >= now {
            loaded
                .pregnancies
                .get(mother)
                .is_some_and(|pregnancy| pregnancy.pool_father.is_some())
        } else {
            let born: Option<Vec<(SimId, SimTime)>> = carried
                .children
                .iter()
                .map(|child| {
                    let id = *keys.get(&child.lineage_key)?;
                    Some((id, facts(loaded, id)?.born))
                })
                .collect();
            born.is_some_and(|born| {
                born.iter().all(|(_, at)| *at == birth)
                    && born
                        .windows(2)
                        .all(|pair| pair[0].0.get().checked_add(1) == Some(pair[1].0.get()))
            })
        };
        if !fits {
            return None;
        }
    }
    // The pregnancies: one from the pool is the arrival's, shifted; any other has its
    // father in the town. The father of a carried pregnancy is the sim with his key exactly
    // when his household came no later than hers (D8: set at her arrival, review).
    let pregnancies_fit = loaded.pregnancies.iter().all(|(mother, pregnancy)| {
        match (pregnancy.pool_father, view.pregnancies.get(mother)) {
            (Some(father_key), Some((record, delta))) => {
                let Some(her_place) = place_of.get(&record.lineage_key) else {
                    return false;
                };
                let father = place_of
                    .get(&father_key)
                    .filter(|his_place| *his_place <= her_place)
                    .and_then(|_| keys.get(&father_key).copied());
                let Some(from) = pool
                    .households
                    .iter()
                    .flat_map(|h| h.pregnancies.iter())
                    .find(|p| p.mother == record.lineage_key)
                else {
                    return false;
                };
                let conceived = from.conceived.as_secs().checked_add(*delta);
                from.father == father_key
                    && from.conception == pregnancy.conception
                    && conceived == Some(pregnancy.conceived.as_secs())
                    && from.children.len() == pregnancy.children.len()
                    && from.children.iter().zip(&pregnancy.children).all(|(c, u)| {
                        (c.lineage_key, c.sex, c.genome) == (u.lineage_key, u.sex, u.genome)
                    })
                    && pregnancy.father == father
            }
            (None, _) => pregnancy.father.is_some(),
            (Some(_), None) => false,
        }
    });
    pregnancies_fit.then_some(view)
}

/// A pending plan an arrival brought: its order (household, member, slot), its insertion
/// number and its plan.
type CarriedPlan = ((usize, usize, u8), u64, PlanRef);

/// The plans an arrival brought and the queue still holds (design note of the carried
/// plans, C4 and C5): each was planned at her arrival by her `core:arrived`, so planned at
/// `A`, all of one arrival by one entry, those of one household by consecutive entries in
/// the order of its members; and they were inserted in the order of the arrival: by
/// household, member, then conception, birth, separation, leaving home.
fn carried_plans_consistent(
    loaded: &LoadedState,
    plan_refs: &[PlanRef],
    rules: &Rules<'_>,
    now: i64,
) -> bool {
    let mut carried: Vec<CarriedPlan> = Vec::new();
    for (entry, plan) in loaded.queue.iter().zip(plan_refs) {
        let owner = match &entry.payload {
            EventPayload::ConceptionAttempt { woman }
                if carried_series(loaded, rules, *woman, now) == Some(true) =>
            {
                Some((*woman, 0))
            }
            EventPayload::Birth { mother }
                if loaded
                    .pregnancies
                    .get(mother)
                    .is_some_and(|pregnancy| pregnancy.pool_father.is_some()) =>
            {
                Some((*mother, 1))
            }
            EventPayload::Separation { partnership }
                if rules.pool.carried.contains_key(partnership) =>
            {
                loaded.partnerships.get(partnership).map(|p| (p.woman, 2))
            }
            EventPayload::LeaveHome { sim } if rules.pool.arrivals.contains_key(sim) => {
                Some((*sim, 3))
            }
            _ => None,
        };
        let Some((owner, slot)) = owner else {
            continue;
        };
        let (Some(&(household, member)), Some(origin)) =
            (rules.pool.places.get(&owner), pool_of(loaded, owner))
        else {
            return false;
        };
        if plan.planned_at != origin.arrived {
            return false;
        }
        carried.push(((household, member, slot), entry.seq, *plan));
    }
    carried.sort_unstable_by_key(|(order, ..)| *order);
    let in_order = carried.windows(2).all(|pair| pair[0].1 < pair[1].1);
    // One entry per arrival; within a household one after the other.
    let by_arrival = carried.windows(2).all(|pair| {
        let ((household, member, _), _, plan) = pair[0];
        let ((next_household, next_member, _), _, next_plan) = pair[1];
        household != next_household
            || u64::try_from(next_member - member)
                .ok()
                .and_then(|step| plan.by.get().checked_add(step))
                == Some(next_plan.by.get())
    });
    in_order && by_arrival
}

/// What the input log says beyond the projection.
struct InputHistory {
    /// The switch of the hereditary diseases over time (see `Rules`).
    switch: SwitchHistory,
    /// (woman, man, formed) of each partnership the inputs formed, in their order.
    pairs: Vec<(SimId, SimId, SimTime)>,
    /// How many founders `Start` names: the sims 1 to N.
    founders: u64,
    /// The soft capacity of `Start`.
    capacity: Option<u64>,
    /// The pool of `Start`, if the town was founded from the pool.
    pool: Option<PoolStart>,
}

/// Sequence numbers gapless from 1, stamps that never fall and never lie after the
/// clock. `Start` first and only first, stamped before the start, with the seed of the
/// world and the founders as the sims 1 to N; then override blocks
/// that pass their own check, and pairings. The parameters of `Start` after every
/// override are those of the world. Gives what the log says beyond the projection, or
/// `None` if anything is off.
fn input_log_consistent(
    loaded: &LoadedState,
    exceptions: &Exceptions,
    seed: u64,
    data: &Data,
) -> Option<InputHistory> {
    let mut started: Option<(WorldParams, &Start)> = None;
    let mut switch = Vec::new();
    let mut pairs = Vec::new();
    let mut last_stamp = None;
    for (record, position) in exceptions.input_log.iter().zip(1_u64..) {
        let in_order = last_stamp.is_none_or(|last| last <= record.t);
        if record.seq.get() != position || !in_order || record.t > loaded.closed_through {
            return None;
        }
        last_stamp = Some(record.t);
        match (&record.input, started.take()) {
            (Input::Start(start), None) => {
                let unstarted = DomainState::new();
                let fits = state::start::check_start(&unstarted, data, record.t, start).is_ok()
                    && start.world_seed == seed
                    && founders_are_the_first_sims(loaded, start, data);
                if !fits {
                    return None;
                }
                started = Some((start.world_params.clone(), start));
                switch.push((
                    SimTime::START.as_secs(),
                    start.world_params.hereditary_diseases,
                ));
                // The founders are the sims 1 to N, in the order of the list.
                let founder = |place: u32| SimId::try_from(u64::from(place) + 1).ok();
                for couple in &start.couples {
                    pairs.push((founder(couple.woman)?, founder(couple.man)?, SimTime::START));
                }
            }
            (Input::ForkOverride(block), Some((before, start))) => {
                if state::params::block_rejection(block, record.t).is_some() {
                    return None;
                }
                let after = block
                    .changes
                    .iter()
                    .fold(before, |params, change| params.with(*change));
                // Stamped with the last closed second, the block acts in the next one.
                switch.push((record.t.as_secs() + 1, after.hereditary_diseases));
                started = Some((after, start));
            }
            (Input::ArrangePartnership(pair), Some(kept)) => {
                let formed = record.t.checked_add(SimDuration::from_secs(1))?;
                pairs.push((pair.woman, pair.man, formed));
                started = Some(kept);
            }
            // A second `Start`, or another input before the first.
            (Input::Start(_), Some(_))
            | (Input::ForkOverride(_) | Input::ArrangePartnership(_), None) => return None,
        }
    }
    let (params, start) = started?;
    let founders = u64::try_from(start.founders.len()).ok()?;
    (Some(params) == loaded.world_params).then_some(InputHistory {
        switch,
        pairs,
        founders,
        capacity: start.capacity,
        pool: start.pool.clone(),
    })
}

/// The partnerships are those the input log formed and those the market formed, with the
/// IDs from 1 in the order they were formed, and the counter follows them (ADR-0018
/// section 7): the inputs' in the order of the log; the market's in a closed round second
/// (a positive multiple of `MONTH`), after the inputs' of that second. Each pairs a woman
/// and a man of at least the market age with `φ` below the limit; it ended by the first of
/// its due separation (if that second is closed) and the first death of the two, the
/// separation first in a tie (ADR-0018 section 3), and is open while neither has come;
/// never ended before it was formed. Each of the two was free when it was formed: an
/// earlier partnership of theirs had ended in an earlier second (a pairing acts before
/// the events of its second).
fn partnerships_consistent(
    loaded: &LoadedState,
    now: i64,
    rules: &Rules<'_>,
    pairs: &[(SimId, SimId, SimTime)],
) -> bool {
    let counted = u64::try_from(loaded.partnerships.len())
        .ok()
        .and_then(|count| count.checked_add(1))
        == Some(loaded.next_partnership_id);
    let sex_of = |id: &SimId| facts(loaded, *id).map(|person| person.sex);
    let born_of = |id: &SimId| facts(loaded, *id).map(|person| person.born);
    // When a partner left and whether by emigration; in one second a death comes first
    // (`DEATH` runs before `EMIGRATION`, ADR-0020 section 6).
    let left_of = |id: &SimId| {
        loaded
            .archive
            .get(id)
            .map(|sim| (sim.left, sim.left_by == LeftBy::Emigration))
    };
    let age = state::market::market_age(rules.data);
    let limit = rules.data.demography().market.kinship_limit;
    let mut kinship = KinshipCache::new();
    let keys = key_index(&loaded.sims, &loaded.archive);
    let mut inputs = pairs.iter().peekable();
    // The second of the previous partnership and whether the market formed it.
    let mut previous: Option<(SimTime, bool)> = None;
    // Per sim the end of its latest partnership so far (`None` while it is open); the
    // partnerships come in the order they were formed.
    let mut latest: BTreeMap<SimId, Option<SimTime>> = BTreeMap::new();
    let all = loaded
        .partnerships
        .iter()
        .zip(1_u64..)
        .all(|((id, partnership), number)| {
            let (woman, man, formed) = (partnership.woman, partnership.man, partnership.formed);
            // A partnership an arrival brought was formed in the pool; it came with the
            // household, in the second of the arrival (design note of the arrival, 9).
            let carried = rules.pool.carried.contains_key(id);
            let by_input = !carried
                && inputs
                    .next_if(|pair| **pair == (woman, man, formed))
                    .is_some();
            let round = formed.as_secs() > 0 && formed.as_secs().rem_euclid(MONTH.as_secs()) == 0;
            let by_market = !carried && !by_input && round && formed.as_secs() < now;
            let created = if carried {
                pool_of(loaded, woman).map_or(formed, |origin| origin.arrived)
            } else {
                formed
            };
            let in_order = formed_in_order(previous, created, by_market);
            previous = Some((created, by_market));
            let of_age = [woman, man].iter().all(|sim| {
                born_of(sim).is_some_and(|born| state::market::of_age(born, formed, age))
            });
            let related = match (keys_of(loaded, woman), keys_of(loaded, man)) {
                (Some(her), Some(his)) => {
                    kinship.phi(her, his, |key| kin(loaded, rules.data, &keys, key)) >= limit
                }
                _ => false,
            };
            let first_leave = [left_of(&woman), left_of(&man)].into_iter().flatten().min();
            let separation =
                separation_due(loaded, rules, *id, partnership).filter(|due| due.as_secs() < now);
            let ending = match (separation, first_leave) {
                (Some(at), left) if left.is_none_or(|(left, _)| at <= left) => Some(Ending {
                    at,
                    by: EndedBy::Separation,
                }),
                (_, Some((at, emigrated))) => Some(Ending {
                    at,
                    by: if emigrated {
                        EndedBy::Emigration
                    } else {
                        EndedBy::Death
                    },
                }),
                (_, None) => None,
            };
            let ended = partnership.ended.map(|ending| ending.at);
            // Partners leave together: an emigration ending needs both archived by
            // emigration in that second, from one household (ADR-0020 section 6).
            let together = ending.is_none_or(|ending| {
                ending.by != EndedBy::Emigration
                    || match (loaded.archive.get(&woman), loaded.archive.get(&man)) {
                        (Some(her), Some(him)) => {
                            [her, him].iter().all(|sim| {
                                sim.left == ending.at && sim.left_by == LeftBy::Emigration
                            }) && her.household == him.household
                        }
                        _ => false,
                    }
            });
            id.get() == number
                && together
                && (by_input || by_market || carried)
                && in_order
                && sex_of(&woman) == Some(Sex::Female)
                && sex_of(&man) == Some(Sex::Male)
                && of_age
                && !related
                && partnership.ended == ending
                && first_leave.is_none_or(|(left, _)| left >= formed)
                && [woman, man]
                    .into_iter()
                    .all(|sim| match latest.insert(sim, ended) {
                        None => true,
                        Some(earlier) => earlier.is_some_and(|end| end < formed),
                    })
        });
    counted && all && inputs.next().is_none()
}

/// Whether a partnership formed at `formed` (by the market or not) may follow the previous
/// one in ID order: in a later second, or in the same second unless the previous one came
/// from the market and this one from an input (an input acts before the round of its
/// second, ADR-0002).
fn formed_in_order(previous: Option<(SimTime, bool)>, formed: SimTime, by_market: bool) -> bool {
    previous.is_none_or(|(at, market)| at < formed || (at == formed && (!market || by_market)))
}

/// Order and parents of a person of the save for φ, by its lineage key (design note of the
/// arrival, section 7): a person of the save, or a record of the data's pool; anything else
/// has no parents (the checks of the IDs reject such a save).
fn kin(
    loaded: &LoadedState,
    data: &Data,
    keys: &BTreeMap<LineageKey, SimId>,
    key: LineageKey,
) -> Kin<LineageKey> {
    use state::market::{POOL, TOWN};
    if let Some(id) = keys.get(&key) {
        let origin = pool_of(loaded, *id);
        let Some(person) = facts(loaded, *id) else {
            return Kin {
                order: (TOWN, SimTime::START),
                mother: None,
                father: None,
            };
        };
        return match origin {
            Some(from) => Kin {
                order: from.born_pool.map_or((TOWN, person.born), |at| (POOL, at)),
                mother: from.mother,
                father: from.father,
            },
            None => Kin {
                order: (TOWN, person.born),
                mother: person.parents.and_then(|p| keys_of(loaded, p.mother)),
                father: person.parents.and_then(|p| keys_of(loaded, p.father)),
            },
        };
    }
    match data.pool().and_then(|held| held.record(key)) {
        Some(record) => Kin {
            order: (POOL, record.born),
            mother: record.mother,
            father: record.father,
        },
        None => Kin {
            order: (TOWN, SimTime::START),
            mother: None,
            father: None,
        },
    }
}

/// The lineage key of a sim of the save.
fn keys_of(loaded: &LoadedState, id: SimId) -> Option<LineageKey> {
    facts(loaded, id).map(|facts| facts.lineage_key)
}

/// The `SimId` of every person of the save, living or archived, by its lineage key.
pub(super) fn key_index(
    sims: &BTreeMap<SimId, Sim>,
    archive: &BTreeMap<SimId, ArchivedSim>,
) -> BTreeMap<LineageKey, SimId> {
    sims.iter()
        .map(|(id, sim)| (sim.lineage_key, *id))
        .chain(archive.iter().map(|(id, sim)| (sim.lineage_key, *id)))
        .collect()
}

pub(super) fn member_index(sims: &BTreeMap<SimId, Sim>) -> BTreeMap<HouseholdId, BTreeSet<SimId>> {
    let mut members: BTreeMap<HouseholdId, BTreeSet<SimId>> = BTreeMap::new();
    for (id, sim) in sims {
        members.entry(sim.household).or_default().insert(*id);
    }
    members
}

/// The households as the rule leaves them (ADR-0018 sections 4 and 7), at `t`, the clock
/// or the start right after `Start`: the IDs 1 to the counter are households (kept as
/// history), formed in a second that is closed or acted in by an input; each living person
/// lives in one that has not ended, each that has not ended has a living member and each
/// that ended has none. A person moved out exactly if it is a founder of 18 or more, ever
/// formed a partnership, or its second of leaving home has come (for the dead: before its
/// death). A dependent person (under 18, not moved out) has a carer in its household who
/// is not dependent, or no carer in a household where nobody is moved out or has a carer
/// (the last stage of the orphan rule); nobody else has a carer. The living who moved out
/// share a household only as the two partners of an open partnership (a village's persons
/// arrive alone, ADR-0020 section 8; nothing else joins two who left home).
fn households_consistent(loaded: &LoadedState, rules: &Rules<'_>) -> bool {
    let clock = loaded.closed_through;
    let t = clock.max(SimTime::START);
    let acted = clock.as_secs().saturating_add(1);
    let ids_complete = u64::try_from(loaded.households.len())
        .ok()
        .and_then(|count| count.checked_add(1))
        == Some(loaded.next_household_id)
        && loaded
            .households
            .keys()
            .zip(1_u64..)
            .all(|(id, number)| id.get() == number);
    let members = member_index(&loaded.sims);
    let open = open_index(&loaded.partnerships);
    let households_fit = loaded.households.iter().all(|(id, household)| {
        let formed = household.formed.as_secs() <= acted;
        let living = members.get(id).is_some_and(|set| !set.is_empty());
        let ended = match household.ended {
            None => living,
            Some(at) => !living && at >= household.formed && at.as_secs() <= acted,
        };
        formed && ended
    });
    let archived_known = loaded
        .archive
        .values()
        .all(|sim| loaded.households.contains_key(&sim.household));
    let partnered: BTreeSet<SimId> = loaded
        .partnerships
        .values()
        .flat_map(|p| [p.woman, p.man])
        .collect();
    let moved_out = |id: &SimId, born: SimTime, key: LineageKey, founder: bool, until: SimTime| {
        let adult_founder =
            founder && state::market::of_age(born, SimTime::START, ADULT_FOUNDER_YEARS);
        let leaves = state::household::leave_second(rules.data, rules.seed, born, key);
        adult_founder || partnered.contains(id) || leaves <= until
    };
    // An arrival moved out in the pool, or by a partnership formed in the town, or when its
    // carried leaving home came (design note of the arrival, section 9).
    let arrival_moved_out = |id: &SimId, until: SimTime| {
        rules.pool.arrivals.get(id).map(|(record, delta)| {
            let leaves = record
                .leave_home_at
                .and_then(|at| at.as_secs().checked_add(*delta))
                .is_some_and(|at| at <= until.as_secs());
            let town_partnership = loaded.partnerships.iter().any(|(pid, p)| {
                !rules.pool.carried.contains_key(pid) && (p.woman == *id || p.man == *id)
            });
            record.moved_out || town_partnership || leaves
        })
    };
    let living_fit = loaded.sims.iter().all(|(id, sim)| {
        let founder = sim.parents.is_none() && sim.pool.is_none();
        // A carried leaving home is an event of its own: it has run once the clock passed
        // it (a valid pool carries none at its freeze, so none falls into second 0).
        let expected = arrival_moved_out(id, clock)
            .unwrap_or_else(|| moved_out(id, sim.born, sim.lineage_key, founder, t));
        let home = loaded.households.get(&sim.household);
        let at_home = home.is_some_and(|household| household.ended.is_none());
        let carer_fits = if state::household::dependent(sim, t) {
            // An input in the second the clock acts in may pair the carer of a child who
            // comes of age in that second: the child stays, and its carer clears only when
            // its birthday runs later in that second (ADR-0018 section 4).
            let of_age_next = clock
                .checked_add(SimDuration::from_secs(1))
                .is_some_and(|next| !state::household::dependent(sim, next));
            match sim.carer {
                Some(carer) => loaded.sims.get(&carer).is_some_and(|c| {
                    let pairing = open
                        .get(&carer)
                        .and_then(|id| loaded.partnerships.get(id))
                        .map(|p| {
                            let together = [p.woman, p.man].iter().all(|partner| {
                                loaded
                                    .sims
                                    .get(partner)
                                    .is_some_and(|other| other.household == c.household)
                            });
                            (p.formed, together)
                        });
                    let home = loaded.households.get(&c.household).map(|h| h.formed);
                    let away = of_age_next && paired_away(acted, home, pairing);
                    (c.household == sim.household || away) && !state::household::dependent(c, t)
                }),
                // The orphan rule's own households only ever take in people who are not
                // moved out: the orphans, and later relatives who never left home.
                None => members.get(&sim.household).is_some_and(|set| {
                    set.iter()
                        .all(|other| loaded.sims.get(other).is_some_and(|o| !o.moved_out))
                }),
            }
        } else {
            sim.carer.is_none()
        };
        sim.moved_out == expected && at_home && carer_fits
    });
    let archived_fit = loaded.archive.iter().all(|(id, sim)| {
        let founder = sim.parents.is_none() && sim.pool.is_none();
        let expected = arrival_moved_out(id, sim.left)
            .unwrap_or_else(|| moved_out(id, sim.born, sim.lineage_key, founder, sim.left));
        sim.moved_out == expected
    });
    let social_fit = loaded
        .sims
        .iter()
        .map(|(id, sim)| (id, sim.parents, sim.pool.as_ref(), &sim.social_parents))
        .chain(
            loaded
                .archive
                .iter()
                .map(|(id, sim)| (id, sim.parents, sim.pool.as_ref(), &sim.social_parents)),
        )
        .all(|(id, parents, pool, social)| {
            social_parents_fit(loaded, *id, (parents, pool), social)
        });
    let pairs_fit = members.values().all(|set| {
        let moved: Vec<SimId> = (set.iter().copied())
            .filter(|id| loaded.sims.get(id).is_some_and(|sim| sim.moved_out))
            .collect();
        match moved.as_slice() {
            [] | [_] => true,
            [one, other] => (open.get(one))
                .and_then(|id| loaded.partnerships.get(id))
                .is_some_and(|p| {
                    [p.woman, p.man] == [*one, *other] || [p.man, p.woman] == [*one, *other]
                }),
            _ => false,
        }
    });
    ids_complete
        && households_fit
        && archived_known
        && living_fit
        && archived_fit
        && social_fit
        && pairs_fit
}

/// The social parents of `id` (ADR-0018 section 4): each a person of the world, living or
/// archived, not `id` itself, not a biological parent, listed once.
fn social_parents_fit(
    loaded: &LoadedState,
    id: SimId,
    (parents, pool): (Option<Parents>, Option<&state::PoolOrigin>),
    social: &[SimId],
) -> bool {
    let known = |sim: &SimId| loaded.sims.contains_key(sim) || loaded.archive.contains_key(sim);
    // A pool person's biological parents by their keys (design note of the arrival).
    let biological = |sim: &SimId| {
        parents.is_some_and(|p| p.mother == *sim || p.father == *sim)
            || pool.is_some_and(|from| {
                let key = keys_of(loaded, *sim);
                key.is_some() && (from.mother == key || from.father == key)
            })
    };
    social.iter().enumerate().all(|(place, sim)| {
        known(sim) && *sim != id && !biological(sim) && !social[..place].contains(sim)
    })
}

/// A founder of this age at the start is moved out (ADR-0018 section 4).
const ADULT_FOUNDER_YEARS: i64 = 18;

/// Whether a carer lives apart from a child who comes of age in the second the clock acts
/// in (`acted`) because an input paired the carer in that second: the carer's household
/// formed then (`home`), and so did the carer's open partnership (`pairing`: its forming
/// and whether the partner lives in the carer's household). The child stayed, and its
/// birthday clears the carer later in that second (ADR-0018 section 4).
fn paired_away(acted: i64, home: Option<SimTime>, pairing: Option<(SimTime, bool)>) -> bool {
    let then = |at: SimTime| at.as_secs() == acted;
    home.is_some_and(then) && pairing.is_some_and(|(formed, together)| then(formed) && together)
}

/// The open partnership of each sim in one, from the partnerships (ADR-0018 section 2).
pub(super) fn open_index(
    partnerships: &BTreeMap<PartnershipId, Partnership>,
) -> BTreeMap<SimId, PartnershipId> {
    partnerships
        .iter()
        .filter(|(_, partnership)| partnership.ended.is_none())
        .flat_map(|(id, partnership)| [(partnership.woman, *id), (partnership.man, *id)])
        .collect()
}

/// When the partnership `id` separates if both live (ADR-0018 section 3): one an arrival
/// brought at its carried second; any other drawn with her partnership number, her count
/// of partnerships less those with this ID or a larger one (design note of the arrival).
fn separation_due(
    loaded: &LoadedState,
    rules: &Rules<'_>,
    id: PartnershipId,
    partnership: &Partnership,
) -> Option<SimTime> {
    if let Some((record, delta)) = rules.pool.carried.get(&id) {
        let at = record.separation_at?.as_secs().checked_add(*delta)?;
        return Some(SimTime::from_secs(at));
    }
    let count = loaded
        .sims
        .get(&partnership.woman)
        .map(|her| her.partnerships)
        .or_else(|| {
            loaded
                .archive
                .get(&partnership.woman)
                .map(|her| her.partnerships)
        })?;
    let later = loaded
        .partnerships
        .range(id..)
        .filter(|(_, other)| other.woman == partnership.woman)
        .count();
    let k = count.checked_sub(u32::try_from(later).ok()?)?;
    let her = facts(loaded, partnership.woman)?.lineage_key;
    let his = facts(loaded, partnership.man)?.lineage_key;
    let address = (her, k, his);
    state::partnership::separation_due(rules.data, rules.seed, partnership.formed, address)
}

/// Each woman has formed as many partnerships as there are partnerships whose woman she
/// is, an arrival as many as the pool counted plus those she formed in the town (the one
/// she brought is in both); a man has none (ADR-0018 section 3).
fn partnership_counts_consistent(loaded: &LoadedState, rules: &Rules<'_>) -> bool {
    let mut formed: BTreeMap<SimId, u32> = BTreeMap::new();
    for (id, partnership) in &loaded.partnerships {
        if rules.pool.carried.contains_key(id) {
            continue;
        }
        let count = formed.entry(partnership.woman).or_default();
        *count = count.saturating_add(1);
    }
    let own = |id: &SimId| {
        let base = rules
            .pool
            .arrivals
            .get(id)
            .map_or(0, |(record, _)| record.partnerships);
        base.saturating_add(formed.get(id).copied().unwrap_or(0))
    };
    loaded
        .sims
        .iter()
        .all(|(id, sim)| sim.partnerships == own(id))
        && loaded
            .archive
            .iter()
            .all(|(id, sim)| sim.partnerships == own(id))
}

/// The switch of the hereditary diseases: each value with the second from which it holds.
type SwitchHistory = Vec<(i64, bool)>;
/// A conception: the mother, the father and her conception number.
type Conception = (SimId, SimId, u32);
/// A born child: its lineage key, its birth and its SimId.
type BornChild = (LineageKey, SimTime, u64);

/// What the sims are checked against: the seed, the causes of death of `Start`, and the
/// history of the switch of the hereditary diseases from the input log, as the second
/// from which each value holds (`Start` at the start, an override block one second
/// after its stamp). All sims are founders so far: the causes act from the start on.
struct Rules<'a> {
    seed: u64,
    data: &'a Data,
    switch: &'a [(i64, bool)],
    /// The soft capacity of `Start` (ADR-0020 section 6).
    capacity: Option<u64>,
    /// The pool persons of the save (design note of the arrival, section 9).
    pool: &'a PoolView<'a>,
    /// (woman, man, formed) of each partnership an input formed.
    pairs: &'a [(SimId, SimId, SimTime)],
}

impl Rules<'_> {
    /// Where the hazards of a person act from and the detail of its budgets (design note of
    /// the arrival, section 6.4), as seconds.
    fn lot(&self, born: SimTime, pool: Option<&state::PoolOrigin>) -> (SimTime, u64) {
        state::death::mortal(born, pool)
    }

    /// The seeds of a town person's draws: `C` with the pool seed if both its biological
    /// parents came from the pool (design note of the arrival, section 6.2).
    fn town_seeds(&self, loaded: &LoadedState, parents: Option<Parents>) -> state::Seeds {
        let pool = self.data.pool().map(|held| held.pool().pool_seed);
        match (parents, pool) {
            (Some(p), Some(pool))
                if pool_of(loaded, p.mother).is_some() && pool_of(loaded, p.father).is_some() =>
            {
                state::Seeds {
                    individual: self.seed,
                    shared: pool,
                    life: self.seed,
                }
            }
            _ => state::Seeds::one(self.seed),
        }
    }
    /// When a sim dies of a lethal affliction if nothing else comes first.
    fn disease_due(
        &self,
        born: SimTime,
        person: (LineageKey, u64),
        affliction: &Affliction,
    ) -> Option<SimTime> {
        let disease = state::disease::of_key(self.data, affliction.disease)?;
        state::disease::due(self.seed, born, person, affliction, disease)
    }

    /// The causes of a sim in their order: senescence, accident, then its lethal
    /// afflictions (ADR-0017 section 2). Each with its key and when it is due.
    fn dues(
        &self,
        facts: (SimTime, LineageKey, Sex, &Genome),
        afflictions: &[Affliction],
        lot: (SimTime, u64),
    ) -> Vec<(u64, Option<SimTime>)> {
        let (born, lineage_key, _, _) = facts;
        let everyone = state::death::everyone_dues(self.data, self.seed, facts, lot);
        let diseases = afflictions
            .iter()
            .filter(|affliction| affliction.exposure.is_some())
            .map(|affliction| {
                let due = self.disease_due(born, (lineage_key, lot.1), affliction);
                (affliction.disease, due)
            });
        everyone.into_iter().chain(diseases).collect()
    }

    /// The exposure the switch history gives a lethal affliction of a sim born `born`
    /// whose diseases act from `origin` on, at the second `at`, built in the order the
    /// world builds it: each closed period adds what it used. `None` if the sim would
    /// have died of it within a closed period: its budget ran out before the switch went
    /// off (exactly at that second is possible, the switch acts first).
    fn exposure_at(
        &self,
        born: SimTime,
        person: (LineageKey, u64),
        affliction: &Affliction,
        origin: i64,
        at: i64,
    ) -> Option<Exposure> {
        let disease = state::disease::of_key(self.data, affliction.disease)?;
        let hazard = state::disease::hazard(disease)?;
        let mut exposure = Exposure {
            used: FiniteF64::try_new(0.0).ok()?,
            since: None,
        };
        for &(from, on) in self.switch.iter().filter(|(from, _)| *from <= at) {
            let from = SimTime::from_secs(from.max(origin));
            match (exposure.since, on) {
                (None, true) => exposure.since = Some(from),
                (Some(begin), false) => {
                    let open = Affliction {
                        exposure: Some(exposure),
                        ..*affliction
                    };
                    let due = state::disease::due(self.seed, born, person, &open, disease);
                    if due.is_some_and(|due| due < from) {
                        return None;
                    }
                    let used = state::disease::period_used(
                        hazard,
                        affliction.onset.get(),
                        born,
                        begin,
                        from,
                    );
                    exposure.used = FiniteF64::try_new(exposure.used.get() + used).ok()?;
                    exposure.since = None;
                }
                (None, false) | (Some(_), true) => {}
            }
        }
        Some(exposure)
    }

    /// A sim has exactly the afflictions its genotype, sex and draws give it (the same
    /// diseases with the same onsets, an exposure for each lethal one), and each exposure
    /// is the one the switch history gives it at `at` (now for the living, the death for
    /// the archive).
    fn afflictions_consistent(
        &self,
        (born, lineage_key): (SimTime, LineageKey),
        drawn: &[Affliction],
        afflictions: &[Affliction],
        (origin, detail): (i64, u64),
        at: i64,
    ) -> bool {
        drawn.len() == afflictions.len()
            && drawn.iter().zip(afflictions).all(|(drawn, held)| {
                let same = drawn.disease == held.disease
                    && drawn.onset == held.onset
                    && drawn.exposure.is_some() == held.exposure.is_some();
                same && held.exposure.is_none_or(|_| {
                    self.exposure_at(born, (lineage_key, detail), held, origin, at) == held.exposure
                })
            })
    }

    /// The afflictions a sim is created with (without their exposures): a pool person's
    /// from the pool, a town person's drawn from its genotype with its seeds.
    fn drawn(&self, loaded: &LoadedState, id: SimId, facts: &Facts) -> Option<Vec<Affliction>> {
        let held = self.data.pool();
        let from_pool = |pooled: &[crate::pool::PoolAffliction]| {
            state::arrival::afflictions(self.data, pooled, SimTime::START, false)
        };
        if let Some((record, _)) = self.pool.arrivals.get(&id) {
            return Some(from_pool(&record.afflictions));
        }
        if self.pool.children.contains(&id) {
            return Some(from_pool(&held?.unborn(facts.lineage_key)?.afflictions));
        }
        let keys = match facts.parents {
            None => None,
            Some(p) => Some(crate::phenotype::Parents {
                mother: facts_of_key(loaded, p.mother)?,
                father: facts_of_key(loaded, p.father)?,
            }),
        };
        Some(state::disease::afflictions(
            self.data,
            self.town_seeds(loaded, facts.parents),
            (facts.lineage_key, keys),
            facts.sex,
            &facts.genome,
            false,
            SimTime::START,
        ))
    }
}

/// The founders are the sims 1 to N, living or archived, with their birth times, sex
/// and genome from `Start` (drawn as `Start` draws them), the lineage key of their place in
/// its list and no parents.
fn founders_are_the_first_sims(loaded: &LoadedState, start: &Start, data: &Data) -> bool {
    (1_u64..)
        .zip((0_u64..).zip(&start.founders))
        .all(|(number, (index, founder))| {
            let Some(found) = SimId::try_from(number)
                .ok()
                .and_then(|id| facts(loaded, id))
            else {
                return false;
            };
            let lineage_key = LineageKey::founder(&start.founder_tag, index);
            // As `Start` builds it: named loci as named, every other one drawn (Grok F1).
            let Ok(genome) =
                crate::state::start::founder_genome(data, start.world_seed, lineage_key, founder)
            else {
                return false;
            };
            (
                found.born,
                found.lineage_key,
                found.sex,
                found.genome,
                found.parents,
            ) == (founder.born, lineage_key, founder.sex, genome, None)
        })
}

/// Every sim has the afflictions its genotype and draws give it, each exposure as the
/// switch history gives it from the sim's origin on, for the archive at the death; a
/// departure in a closed second (an event of the first open second has not run yet) and
/// as the death rule gives it. The IDs are those of `born_consistent`.
fn sims_consistent(loaded: &LoadedState, now: i64, rules: &Rules<'_>) -> bool {
    let fits =
        |id: SimId, pool: Option<&state::PoolOrigin>, afflictions: &[Affliction], at: i64| {
            let Some(facts) = facts(loaded, id) else {
                return false;
            };
            let (origin, detail) = rules.lot(facts.born, pool);
            rules.drawn(loaded, id, &facts).is_some_and(|drawn| {
                rules.afflictions_consistent(
                    (facts.born, facts.lineage_key),
                    &drawn,
                    afflictions,
                    (origin.as_secs(), detail),
                    at,
                )
            })
        };
    loaded
        .sims
        .iter()
        .all(|(id, sim)| fits(*id, sim.pool.as_ref(), &sim.afflictions, now))
        && loaded.archive.iter().all(|(id, sim)| {
            let left = sim.left.as_secs();
            left < now
                && fits(*id, sim.pool.as_ref(), &sim.afflictions, left)
                && match sim.left_by {
                    LeftBy::Death { cause } => died_by_the_rule(sim, cause, rules),
                    LeftBy::Emigration => emigrated_by_the_rule(loaded, sim, rules),
                }
        })
}

/// The lineage key of a sim in exactly one place.
fn facts_of_key(loaded: &LoadedState, id: SimId) -> Option<LineageKey> {
    facts(loaded, id).map(|facts| facts.lineage_key)
}

/// What a sim, living or archived, is born as.
struct Facts {
    born: SimTime,
    lineage_key: LineageKey,
    sex: Sex,
    genome: Genome,
    parents: Option<Parents>,
    /// Where a pool person came from; `None` for a town person.
    pool: Option<state::PoolOrigin>,
}

/// The facts of a sim in exactly one place, the living or the archive; `None` for an ID
/// in neither or in both.
fn facts(loaded: &LoadedState, id: SimId) -> Option<Facts> {
    match (loaded.sims.get(&id), loaded.archive.get(&id)) {
        (Some(sim), None) => Some(Facts {
            born: sim.born,
            lineage_key: sim.lineage_key,
            sex: sim.sex,
            genome: sim.genome,
            parents: sim.parents,
            pool: sim.pool.clone(),
        }),
        (None, Some(sim)) => Some(Facts {
            born: sim.born,
            lineage_key: sim.lineage_key,
            sex: sim.sex,
            genome: sim.genome,
            parents: sim.parents,
            pool: sim.pool.clone(),
        }),
        (None, None) | (Some(_), Some(_)) => None,
    }
}

/// The SimIds are 1 to the counter without a gap, each in one place (`facts`): the
/// founders first (`founders_are_the_first_sims`), then the born. Each born sim is the
/// child of a conception the rule allows, born as many months later as a pregnancy lasts,
/// in a closed second, while its mother lived (her death ends the pregnancy, B18; in the
/// second of the birth she dies after it).
fn born_consistent(loaded: &LoadedState, now: i64, rules: &Rules<'_>, founders: u64) -> bool {
    let below_counter = |id: &SimId| id.get() < loaded.next_sim_id;
    loaded
        .sims
        .keys()
        .chain(loaded.archive.keys())
        .all(below_counter)
        && (founders.saturating_add(1)..loaded.next_sim_id).all(|number| {
            let Some(id) = SimId::try_from(number).ok() else {
                return false;
            };
            // A pool person is checked by `pool_view`.
            if rules.pool.came_from_pool(id) {
                return facts(loaded, id).is_some();
            }
            let Some(child) = facts(loaded, id) else {
                return false;
            };
            let Some(parents) = child.parents else {
                return false;
            };
            let months = MONTH.checked_mul(i64::from(pregnancy_months(rules)));
            let Some(conceived) = months.and_then(|months| child.born.checked_sub(months)) else {
                return false;
            };
            let left = loaded
                .archive
                .get(&parents.mother)
                .map(|mother| mother.left);
            child.born.as_secs() < now
                && lived_at(left, child.born)
                && conception_fits(loaded, rules, parents, conceived, &child)
        })
}

/// Whether a sim that left at `left` (`None`: it lives) was alive in the second `at`; in
/// the second of its death it still was (a birth comes before a death, ADR-0002).
fn lived_at(left: Option<SimTime>, at: SimTime) -> bool {
    left.is_none_or(|left| left >= at)
}

/// A conception the rule gives (ADR-0017 section 4, B18): a partnership of mother and
/// father open at `conceived`, which lies at a month start in her series of attempts
/// (after the pairing and after the pause of her last birth in it), with fewer live births
/// than she wishes and her fertility not ended, and a successful draw at the chance of the
/// curves; the child's key comes from both parents and her number, its alleles and sex
/// from the draws.
fn conception_fits(
    loaded: &LoadedState,
    rules: &Rules<'_>,
    parents: Parents,
    conceived: SimTime,
    child: &Facts,
) -> bool {
    let (Some(her), Some(him)) = (facts(loaded, parents.mother), facts(loaded, parents.father))
    else {
        return false;
    };
    // A partnership pairs a woman and a man (`partnerships_consistent`).
    let formed = loaded.partnerships.iter().find_map(|(id, partnership)| {
        ((partnership.woman, partnership.man) == (parents.mother, parents.father)
            && partnership.formed <= conceived
            && partnership.ended.is_none_or(|ended| ended.at >= conceived))
        .then_some((*id, partnership.formed))
    });
    let at_a_month_start =
        conceived.as_secs() >= 0 && conceived.as_secs().rem_euclid(MONTH.as_secs()) == 0;
    let in_her_series = formed.is_some_and(|(id, formed)| {
        let born_before = children_born(loaded, parents.mother, conceived);
        series_start(
            rules,
            id,
            (parents.mother, parents.father, formed),
            &born_before,
        )
        .is_some_and(|start| start <= conceived)
            && conception::may_begin(
                rules.data,
                life_seed(rules, &her),
                her.born,
                her.lineage_key,
                births_before(rules, parents.mother, born_before.len()),
                conceived,
            )
    });
    let drawn = conception::month_index(conceived).is_some_and(|month| {
        let key = conception::attempt_key(her.lineage_key, him.lineage_key, month);
        let (Some(woman), Some(man)) = (person(loaded, rules, &her), person(loaded, rules, &him))
        else {
            return false;
        };
        let p = conception::chance(rules.data, &woman, &man, conceived);
        Draws::new(WorldSeed::new(rules.seed)).bernoulli(key, p)
    });
    // The child is one of the children of that conception, with its genome and sex.
    let fits = expected_children(rules, &her, &him, parents.conception).is_some_and(|children| {
        children.iter().any(|(key, _, zygote)| {
            child.lineage_key == *key
                && child.genome == zygote.genome
                && child.sex == zygote.genome.sex()
        })
    });
    at_a_month_start && in_her_series && drawn && fits
}

/// The seed of a woman's wish: the pool seed for a pool person (design note of the
/// arrival, section 6.2).
fn life_seed(rules: &Rules<'_>, her: &Facts) -> u64 {
    match (&her.pool, rules.data.pool()) {
        (Some(_), Some(held)) => held.pool().pool_seed,
        _ => rules.seed,
    }
}

/// Her live births before a conception: those in the town, and for an arrival those the
/// pool counted.
fn births_before(rules: &Rules<'_>, mother: SimId, in_town: usize) -> u32 {
    let base = rules
        .pool
        .arrivals
        .get(&mother)
        .map_or(0, |(record, _)| record.births);
    base.saturating_add(u32::try_from(in_town).unwrap_or(u32::MAX))
}

/// The children of her conception number `number` with him by the rule: one, or twins
/// (ADR-0001 section 4, spec 11.3); `None` unless she is a woman and he a man (the meiosis
/// needs both).
fn expected_children(
    rules: &Rules<'_>,
    her: &Facts,
    him: &Facts,
    number: u32,
) -> Option<Vec<(LineageKey, GenomeKey, Zygote)>> {
    let sexes = (her.genome.sex(), him.genome.sex()) == (Sex::Female, Sex::Male);
    sexes.then(|| {
        conception::children(
            rules.data,
            rules.seed,
            (her.lineage_key, &her.genome),
            (him.lineage_key, &him.genome),
            number,
        )
    })
}

/// The months of a pregnancy (11.3).
fn pregnancy_months(rules: &Rules<'_>) -> u32 {
    rules.data.demography().fertility.pregnancy_months
}

/// The birth times of her children born in the town before `before`, in order: her town
/// children and the pool children of her pregnancy from the pool.
fn children_born(loaded: &LoadedState, mother: SimId, before: SimTime) -> Vec<SimTime> {
    let her_key = facts(loaded, mother).map(|her| her.lineage_key);
    let hers = |parents: Option<Parents>, pool: Option<&state::PoolOrigin>| {
        parents.is_some_and(|p| p.mother == mother)
            || pool.is_some_and(|from| from.born_pool.is_none() && from.mother == her_key)
    };
    let mut born: Vec<SimTime> = loaded
        .sims
        .values()
        .map(|sim| (hers(sim.parents, sim.pool.as_ref()), sim.born))
        .chain(
            loaded
                .archive
                .values()
                .map(|sim| (hers(sim.parents, sim.pool.as_ref()), sim.born)),
        )
        .filter(|(mine, born)| *mine && *born < before)
        .map(|(_, born)| born)
        .collect();
    born.sort_unstable();
    born
}

/// Where her series of attempts in a partnership formed at `formed` begins, given the
/// times of her births so far: after the pairing, or after the pause of her last birth in
/// that partnership (ADR-0017 section 4). In a partnership an arrival brought and without
/// a birth in it, at the first attempt she brought, none without one (design note of the
/// carried plans, C2).
fn series_start(
    rules: &Rules<'_>,
    partnership: PartnershipId,
    (woman, man, formed): (SimId, SimId, SimTime),
    births: &[SimTime],
) -> Option<SimTime> {
    let pause = rules.data.demography().fertility.pause_months;
    // A birth in the second of the pairing came before a pairing of the market (births run
    // before the round) and after one of an input (it acts before the events of its second;
    // review).
    let by_input = rules.pairs.contains(&(woman, man, formed));
    let in_it = |born: &SimTime| *born > formed || (*born == formed && by_input);
    match births.iter().rev().find(|born| in_it(born)) {
        Some(born) => conception::series_after_birth(*born, pause),
        None => match rules.pool.carried.get(&partnership) {
            Some((record, delta)) => record
                .conception_at
                .and_then(|at| at.as_secs().checked_add(*delta))
                .map(SimTime::from_secs),
            None => conception::series_after_pairing(formed),
        },
    }
}

/// What the conception rule reads of a sim: its facts and the lineage keys of its
/// parents.
fn person<'a>(
    loaded: &LoadedState,
    rules: &Rules<'_>,
    sim: &'a Facts,
) -> Option<conception::Person<'a>> {
    let key_of = |id: SimId| facts(loaded, id).map(|parent| parent.lineage_key);
    // A pool person: its parents by its pool keys, every draw with the pool seed (design
    // note of the arrival, section 6.2).
    if let Some(from) = &sim.pool {
        let pool = rules.data.pool()?.pool().pool_seed;
        return Some(conception::Person {
            born: sim.born,
            lineage_key: sim.lineage_key,
            genome: &sim.genome,
            parents: from
                .mother
                .zip(from.father)
                .map(|(mother, father)| crate::phenotype::Parents { mother, father }),
            seeds: state::Seeds::one(pool),
        });
    }
    let parents = match sim.parents {
        None => None,
        Some(p) => Some(crate::phenotype::Parents {
            mother: key_of(p.mother)?,
            father: key_of(p.father)?,
        }),
    };
    Some(conception::Person {
        born: sim.born,
        lineage_key: sim.lineage_key,
        genome: &sim.genome,
        parents,
        seeds: rules.town_seeds(loaded, sim.parents),
    })
}

/// Each sim has as many live births as there are sims whose mother it is, an arrival as
/// many as the pool counted plus her children born in the town (pool children by her key);
/// a man has none (ADR-0017 section 4).
fn births_consistent(loaded: &LoadedState, rules: &Rules<'_>) -> bool {
    let mut children: BTreeMap<SimId, u32> = BTreeMap::new();
    let keys = key_index(&loaded.sims, &loaded.archive);
    let mothers = loaded
        .sims
        .iter()
        .map(|(id, sim)| (*id, sim.parents))
        .chain(loaded.archive.iter().map(|(id, sim)| (*id, sim.parents)))
        .filter_map(|(id, parents)| match parents {
            Some(p) => Some(p.mother),
            None if rules.pool.children.contains(&id) => pool_of(loaded, id)?
                .mother
                .and_then(|key| keys.get(&key).copied()),
            None => None,
        });
    for mother in mothers {
        let count = children.entry(mother).or_default();
        *count = count.saturating_add(1);
    }
    let own = |id: &SimId| {
        let base = rules
            .pool
            .arrivals
            .get(id)
            .map_or(0, |(record, _)| record.births);
        base.saturating_add(children.get(id).copied().unwrap_or(0))
    };
    loaded.sims.iter().all(|(id, sim)| sim.births == own(id))
        && loaded.archive.iter().all(|(id, sim)| sim.births == own(id))
}

/// Each pregnancy is one the conception rule gives, of a living mother, conceived in a
/// closed second, and her latest conception.
fn pregnancies_consistent(loaded: &LoadedState, now: i64, rules: &Rules<'_>) -> bool {
    loaded.pregnancies.iter().all(|(mother, pregnancy)| {
        let Some(her) = loaded.sims.get(mother) else {
            return false;
        };
        // A pregnancy from the pool: `pool_view` checked it against the pool.
        if pregnancy.pool_father.is_some() {
            return pregnancy.conception.checked_add(1) == Some(her.conceptions)
                && pregnancy.conceived.as_secs() < now;
        }
        let Some(father) = pregnancy.father else {
            return false;
        };
        let parents = Parents {
            mother: *mother,
            father,
            conception: pregnancy.conception,
        };
        let child = |unborn: &state::Unborn| Facts {
            born: pregnancy.conceived,
            lineage_key: unborn.lineage_key,
            sex: unborn.sex,
            genome: unborn.genome,
            parents: Some(parents),
            pool: None,
        };
        // All the children of the conception, in the order of their index.
        let complete = facts(loaded, *mother)
            .zip(facts(loaded, father))
            .and_then(|(woman, man)| expected_children(rules, &woman, &man, pregnancy.conception))
            .is_some_and(|children| {
                children.len() == pregnancy.children.len()
                    && children
                        .iter()
                        .zip(&pregnancy.children)
                        .all(|((key, _, _), unborn)| *key == unborn.lineage_key)
            });
        pregnancy.conception.checked_add(1) == Some(her.conceptions)
            && pregnancy.conceived.as_secs() < now
            && complete
            && pregnancy.children.iter().all(|unborn| {
                conception_fits(loaded, rules, parents, pregnancy.conceived, &child(unborn))
            })
    })
}

/// The born children of a conception are all its children by the rule (twins both, spec
/// 11.3), born in one second with consecutive SimIds in the order of their index; a
/// conception whose pregnancy ended with the mother's death has none born.
fn twins_complete(loaded: &LoadedState, rules: &Rules<'_>) -> bool {
    // Per (mother, father, conception): the born children with their birth, by SimId.
    let mut born: BTreeMap<Conception, Vec<BornChild>> = BTreeMap::new();
    let all = loaded
        .sims
        .iter()
        .map(|(id, sim)| (id.get(), sim.parents, sim.lineage_key, sim.born))
        .chain(
            loaded
                .archive
                .iter()
                .map(|(id, sim)| (id.get(), sim.parents, sim.lineage_key, sim.born)),
        );
    for (id, parents, key, at) in all {
        if let Some(p) = parents {
            born.entry((p.mother, p.father, p.conception))
                .or_default()
                .push((key, at, id));
        }
    }
    born.into_iter()
        .all(|((mother, father, number), children)| {
            let expected = facts(loaded, mother)
                .zip(facts(loaded, father))
                .and_then(|(her, him)| expected_children(rules, &her, &him, number));
            let Some(expected) = expected else {
                return false;
            };
            let mut children = children;
            children.sort_unstable_by_key(|(_, _, id)| *id);
            let in_order = children
                .iter()
                .map(|(key, _, _)| *key)
                .eq(expected.iter().map(|(key, _, _)| *key));
            in_order && born_together(&children)
        })
}

/// The children of one birth, by SimId: all born in the same second, with consecutive
/// SimIds (a birth creates them one after the other).
fn born_together(children: &[BornChild]) -> bool {
    children
        .windows(2)
        .all(|pair| pair[0].1 == pair[1].1 && pair[0].2.checked_add(1) == Some(pair[1].2))
}

/// The conceptions of a woman are numbered from 0 without a gap: her born children and
/// her running pregnancy, for one in the archive perhaps also a last one that ended with
/// her death. Each conception comes after the birth of the one before. A man has none.
fn conceptions_consistent(loaded: &LoadedState, rules: &Rules<'_>) -> bool {
    let months = MONTH.checked_mul(i64::from(pregnancy_months(rules)));
    let Some(months) = months else {
        return false;
    };
    // Per mother: (conception number, when it was conceived).
    let mut conceived: BTreeMap<SimId, Vec<(u32, SimTime)>> = BTreeMap::new();
    let born = loaded
        .sims
        .values()
        .map(|sim| (sim.parents, sim.born))
        .chain(loaded.archive.values().map(|sim| (sim.parents, sim.born)));
    for (parents, born) in born {
        if let Some(parents) = parents {
            let Some(at) = born.checked_sub(months) else {
                return false;
            };
            conceived
                .entry(parents.mother)
                .or_default()
                .push((parents.conception, at));
        }
    }
    for (mother, pregnancy) in &loaded.pregnancies {
        // A pregnancy from the pool is counted in the pool's number.
        if pregnancy.pool_father.is_some() {
            continue;
        }
        conceived
            .entry(*mother)
            .or_default()
            .push((pregnancy.conception, pregnancy.conceived));
    }
    let mut counted = |id: &SimId, sex: Sex, count: u32, archived: bool| {
        let mut own = conceived.remove(id).unwrap_or_default();
        own.sort_unstable();
        // Twins share their conception (spec 11.3).
        own.dedup();
        let numbers: Vec<u32> = own.iter().map(|(number, _)| *number).collect();
        // An arrival's conceptions in the town follow those of the pool.
        let first = rules
            .pool
            .arrivals
            .get(id)
            .map_or(0, |(record, _)| record.conceptions);
        let all = (first..count).eq(numbers.iter().copied());
        let but_the_last =
            archived && count > first && (first..count - 1).eq(numbers.iter().copied());
        let in_order = own.windows(2).all(|pair| {
            pair[0]
                .1
                .checked_add(months)
                .is_some_and(|birth| pair[1].1 > birth)
        });
        match sex {
            Sex::Female => (all || but_the_last) && in_order,
            // A man is no mother: the partnership at a conception has shown it.
            Sex::Male => count == 0,
        }
    };
    let living = loaded
        .sims
        .iter()
        .all(|(id, sim)| counted(id, sim.sex, sim.conceptions, false));
    let archived = loaded
        .archive
        .iter()
        .all(|(id, sim)| counted(id, sim.sex, sim.conceptions, true));
    // A child of a mother who is nowhere fails at its conception.
    living && archived
}

/// When the next attempt of a living woman is due (ADR-0017 section 4): the first month
/// start after now that lies in her series (after the pairing of her open partnership, or
/// after the pause of her last birth in it), if she is not pregnant, has fewer live births
/// than she wishes and her fertility has not ended by then.
fn next_attempt(
    loaded: &LoadedState,
    now: i64,
    rules: &Rules<'_>,
    woman: SimId,
) -> Option<SimTime> {
    let her = loaded.sims.get(&woman)?;
    if let Some(brought) = carried_series(loaded, rules, woman, now) {
        return carried_attempt(rules, woman, her.born, now)
            .filter(|_| brought && !loaded.pregnancies.contains_key(&woman));
    }
    let (id, open) = loaded
        .partnerships
        .iter()
        .find(|(_, partnership)| partnership.woman == woman && partnership.ended.is_none())?;
    if loaded.pregnancies.contains_key(&woman) {
        return None;
    }
    let clock = SimTime::try_from(now.checked_sub(1)?).ok()?;
    let births = children_born(loaded, woman, SimTime::try_from(now).ok()?);
    let start = series_start(rules, *id, (woman, open.man, open.formed), &births)?;
    let after_now = conception::month_start_after(clock)?;
    let next = if start > after_now { start } else { after_now };
    let life = if her.pool.is_some() {
        rules.data.pool()?.pool().pool_seed
    } else {
        rules.seed
    };
    conception::may_begin(
        rules.data,
        life,
        her.born,
        her.lineage_key,
        her.births,
        next,
    )
    .then_some(next)
}

/// The pending attempt of the series an arrival brought (design note of the carried plans,
/// C1): on the grid of its first attempt `s`, the first second after the clock. The first
/// is pending until it runs; after a failed one the next follows while her fertility has
/// not ended by then, as `conception::on_attempt` goes on.
fn carried_attempt(rules: &Rules<'_>, woman: SimId, born: SimTime, now: i64) -> Option<SimTime> {
    let (record, delta) = rules.pool.arrivals.get(&woman)?;
    let first = record.conception_at?.as_secs().checked_add(*delta)?;
    let month = MONTH.as_secs();
    let months = if first >= now {
        0
    } else {
        now.checked_sub(first)?
            .checked_sub(1)?
            .div_euclid(month)
            .checked_add(1)?
    };
    let next = SimTime::from_secs(first.checked_add(months.checked_mul(month)?)?);
    let end = conception::fertility_end(&rules.data.genetics().fertility);
    (months == 0 || state::disease::age(born, next) < end).then_some(next)
}

/// Whether an arrival's attempts are still the series she brought from the pool (review):
/// her open partnership is the one she brought and no child of hers was born in the town.
/// `Some(true)` if she brought a series, `Some(false)` if not; `None` once the town's rule
/// plans her attempts.
fn carried_series(loaded: &LoadedState, rules: &Rules<'_>, woman: SimId, now: i64) -> Option<bool> {
    let (record, _) = rules.pool.arrivals.get(&woman)?;
    let brought = loaded
        .partnerships
        .iter()
        .find(|(_, p)| p.woman == woman && p.ended.is_none())
        .is_some_and(|(id, _)| rules.pool.carried.contains_key(id));
    let no_town_child = children_born(loaded, woman, SimTime::try_from(now).ok()?).is_empty();
    (brought && no_town_child).then_some(record.conception_at.is_some())
}

/// An archived sim emigrated with its household above the soft capacity (ADR-0020
/// section 6): the world has a capacity; it left in the last second of a year from year 0
/// on; its
/// household ended in that second; at least one who emigrated from it then was 18 or more;
/// and no death of the sim was due by then (a death of that second runs before the
/// emigration). Who else lived in the household then either died in that second or
/// emigrated with it: the household ended.
fn emigrated_by_the_rule(loaded: &LoadedState, sim: &ArchivedSim, rules: &Rules<'_>) -> bool {
    let left = sim.left;
    // In the last second of a year from year 0 on: the capacity acts only after `Start`.
    let year_end = left.as_secs() >= YEAR.as_secs() - 1
        && left
            .as_secs()
            .checked_add(1)
            .is_some_and(|next| next.rem_euclid(YEAR.as_secs()) == 0);
    let ended_there = loaded
        .households
        .get(&sim.household)
        .is_some_and(|household| household.ended == Some(left));
    let adult = loaded.archive.values().any(|other| {
        other.household == sim.household
            && other.left == left
            && other.left_by == LeftBy::Emigration
            && state::market::of_age(other.born, left, state::emigration::ADULT_YEARS)
    });
    let no_death_due = rules
        .dues(
            (sim.born, sim.lineage_key, sim.sex, &sim.genome),
            &sim.afflictions,
            rules.lot(sim.born, sim.pool.as_ref()),
        )
        .iter()
        .all(|(_, due)| due.is_none_or(|at| at > left));
    rules.capacity.is_some() && year_end && ended_there && adult && no_death_due
}

/// An archived sim died of a cause of the world, in the second its budget for that
/// cause ran out, and no other cause was due before. Of two causes due in the same
/// second the one earlier in the list wins: its event was planned first. A disease was
/// due only if its exposure kept with the sim ran at the death.
fn died_by_the_rule(sim: &ArchivedSim, cause: u64, rules: &Rules<'_>) -> bool {
    let dues = rules.dues(
        (sim.born, sim.lineage_key, sim.sex, &sim.genome),
        &sim.afflictions,
        rules.lot(sim.born, sim.pool.as_ref()),
    );
    let Some(position) = dues.iter().position(|(key, _)| *key == cause) else {
        return false;
    };
    dues[position].1 == Some(sim.left)
        && dues.iter().enumerate().all(|(index, (_, due))| match due {
            None => true,
            Some(at) => *at > sim.left || (*at == sim.left && index >= position),
        })
}

/// One year end series, due at the next year end; one birthday per living sim, due at
/// its next anniversary; per living sim and cause of death one death, due when the
/// budget for that cause runs out, and none where it never does; nothing for a sim that
/// does not live.
fn queue_consistent(loaded: &LoadedState, now: i64, rules: &Rules<'_>) -> bool {
    let next_year_end = (now.div_euclid(YEAR.as_secs()) + 1) * YEAR.as_secs() - 1;
    let mut year_end_seen = false;
    let mut round_seen = false;
    let mut emigration_seen = false;
    let mut immigration_seen = false;
    let next_year_start = (now.div_euclid(YEAR.as_secs()) + 1) * YEAR.as_secs();
    let now_is_year_start = now.rem_euclid(YEAR.as_secs()) == 0;
    let mut birthdays = BTreeSet::new();
    let mut births = BTreeSet::new();
    let mut attempts = BTreeSet::new();
    let mut separations = BTreeSet::new();
    let mut leaving = BTreeSet::new();
    let mut deaths = BTreeSet::new();
    // Per death: the sim, the insertion number and the place of the cause in the list.
    let mut planned = Vec::new();
    let dues = |person: &Sim| {
        rules.dues(
            (person.born, person.lineage_key, person.sex, &person.genome),
            &person.afflictions,
            rules.lot(person.born, person.pool.as_ref()),
        )
    };
    for entry in &loaded.queue {
        let due = entry.time.as_secs();
        let expected = match &entry.payload {
            EventPayload::YearEnd => {
                // A second series is rejected at once; a count could overflow.
                let first = !year_end_seen;
                year_end_seen = true;
                first && entry.period == Some(YEAR) && due == next_year_end
            }
            EventPayload::MarketRound => {
                // One series of rounds, due at the first month start after the clock, and
                // never before the first month start after the start (ADR-0018 section 1).
                let first = !round_seen;
                round_seen = true;
                let from = loaded.closed_through.max(SimTime::START);
                first
                    && entry.period == Some(MONTH)
                    && conception::month_start_after(from) == Some(entry.time)
            }
            EventPayload::Emigration { capacity } => {
                // One yearly series with the capacity of `Start`, due at the next year end.
                let first = !emigration_seen;
                emigration_seen = true;
                first
                    && entry.period == Some(YEAR)
                    && due == next_year_end
                    && rules.capacity == Some(*capacity)
            }
            EventPayload::Immigration { persons } => {
                // One yearly series with the persons of `Start`, due at the next year start
                // (or now, if the clock stands right before one; ADR-0020 section 5).
                let first = !immigration_seen;
                immigration_seen = true;
                let due_at = if now_is_year_start {
                    now
                } else {
                    next_year_start
                };
                first
                    && entry.period == Some(YEAR)
                    && due == due_at
                    && rules.pool.immigration == Some(*persons)
            }
            EventPayload::Birthday { sim } => {
                entry.period.is_none()
                    && birthdays.insert(*sim)
                    && loaded.sims.get(sim).is_some_and(|person| {
                        state::start::next_anniversary_secs(person.born, now) == Some(due)
                    })
            }
            EventPayload::Death { sim, cause } => {
                let person = loaded.sims.get(sim);
                let all = person.map(dues).unwrap_or_default();
                let position = all.iter().position(|(key, _)| key == cause);
                planned.push((*sim, entry.seq, position));
                entry.period.is_none()
                    && deaths.insert((*sim, *cause))
                    && position.is_some_and(|place| all[place].1 == Some(entry.time))
            }
            EventPayload::Birth { mother } => {
                entry.period.is_none()
                    && births.insert(*mother)
                    && loaded.pregnancies.get(mother).is_some_and(|pregnancy| {
                        // A pregnancy from the pool keeps its carried birth.
                        let carried =
                            rules
                                .pool
                                .pregnancies
                                .get(mother)
                                .and_then(|(record, delta)| {
                                    record.birth_at?.as_secs().checked_add(*delta)
                                });
                        match (pregnancy.pool_father, carried) {
                            (Some(_), Some(at)) => at == entry.time.as_secs(),
                            (Some(_), None) => false,
                            (None, _) => {
                                conception::birth_of(rules.data, pregnancy.conceived)
                                    == Some(entry.time)
                            }
                        }
                    })
            }
            EventPayload::ConceptionAttempt { woman } => {
                entry.period == Some(MONTH)
                    && attempts.insert(*woman)
                    && next_attempt(loaded, now, rules, *woman) == Some(entry.time)
            }
            EventPayload::LeaveHome { sim } => {
                entry.period.is_none()
                    && leaving.insert(*sim)
                    && loaded.sims.get(sim).is_some_and(|person| {
                        // An arrival's leaving home came from the pool.
                        let carried = rules.pool.arrivals.get(sim).map(|(record, delta)| {
                            record
                                .leave_home_at
                                .and_then(|at| at.as_secs().checked_add(*delta))
                        });
                        let due = match carried {
                            Some(at) => at.map(SimTime::from_secs),
                            None => Some(state::household::leave_second(
                                rules.data,
                                rules.seed,
                                person.born,
                                person.lineage_key,
                            )),
                        };
                        !person.moved_out && due == Some(entry.time)
                    })
            }
            EventPayload::Separation { partnership: id } => {
                entry.period.is_none()
                    && separations.insert(*id)
                    && loaded.partnerships.get(id).is_some_and(|partnership| {
                        partnership.ended.is_none()
                            && separation_due(loaded, rules, *id, partnership) == Some(entry.time)
                    })
            }
        };
        if !expected {
            return false;
        }
    }
    // Every death that is due is in the queue: a living sim whose budget has run out
    // has no event there, the scheduler holds none in the past.
    let all_deaths_planned = loaded.sims.iter().all(|(id, person)| {
        dues(person)
            .iter()
            .all(|(key, due)| due.is_none() || deaths.contains(&(*id, *key)))
    });
    // The deaths of a sim were planned in the order of the causes: of two due in the same
    // second the earlier cause runs first, as `died_by_the_rule` expects.
    planned.sort_unstable();
    let in_order = planned
        .windows(2)
        .all(|pair| pair[0].0 != pair[1].0 || pair[0].2 < pair[1].2);
    // Every woman who tries has her series.
    let all_attempts_planned = loaded.sims.keys().all(|woman| {
        next_attempt(loaded, now, rules, *woman).is_none() || attempts.contains(woman)
    });
    // Every open partnership that separates has its separation.
    let all_separations_planned = loaded.partnerships.iter().all(|(id, partnership)| {
        partnership.ended.is_some()
            || separation_due(loaded, rules, *id, partnership).is_none()
            || separations.contains(id)
    });
    // Everyone who has not moved out leaves home at the drawn age.
    let all_leaving_planned = loaded
        .sims
        .iter()
        .all(|(id, person)| person.moved_out || leaving.contains(id));
    year_end_seen
        && round_seen
        && emigration_seen == rules.capacity.is_some()
        && immigration_seen == rules.pool.immigration.is_some()
        && all_leaving_planned
        && birthdays.len() == loaded.sims.len()
        && all_separations_planned
        && all_deaths_planned
        && in_order
        && all_attempts_planned
        && births.len() == loaded.pregnancies.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mother who died before the second of a birth cannot have borne the child (her
    /// death ends the pregnancy, B18); in that second she still could, the birth runs
    /// before the death (ADR-0002).
    /// Twins are born in one second with consecutive SimIds; one child alone is a birth.
    #[test]
    fn twins_are_born_together() {
        let tag = crate::data_id::DataId::new("test:twins").expect("id");
        let key = LineageKey::founder(&tag, 0);
        let at = |secs: i64| SimTime::from_secs(secs);
        assert!(born_together(&[(key, at(5), 7)]));
        assert!(born_together(&[(key, at(5), 7), (key, at(5), 8)]));
        assert!(!born_together(&[(key, at(5), 7), (key, at(6), 8)]));
        assert!(!born_together(&[(key, at(5), 7), (key, at(5), 9)]));
    }

    /// In one second the partnerships of inputs come before those of the market round
    /// (ADR-0018 section 7); a later second may follow anything, an earlier none.
    #[test]
    fn the_inputs_of_a_second_pair_before_its_round() {
        let t = |secs: i64| SimTime::from_secs(secs);
        assert!(formed_in_order(None, t(5), true));
        assert!(formed_in_order(Some((t(5), false)), t(5), false));
        assert!(formed_in_order(Some((t(5), false)), t(5), true));
        assert!(formed_in_order(Some((t(5), true)), t(5), true));
        assert!(!formed_in_order(Some((t(5), true)), t(5), false));
        assert!(formed_in_order(Some((t(5), true)), t(6), false));
        assert!(!formed_in_order(Some((t(6), false)), t(5), false));
    }

    /// Review finding: a carer may live apart from a child coming of age only in the
    /// household an input paired her into in that very second, with her partner.
    #[test]
    fn a_carer_lives_apart_only_after_a_pairing_in_that_second() {
        let t = SimTime::from_secs;
        assert!(paired_away(9, Some(t(9)), Some((t(9), true))));
        // A household formed then, but no pairing of the carer: a forged time.
        assert!(!paired_away(9, Some(t(9)), None));
        assert!(!paired_away(9, Some(t(9)), Some((t(5), true))));
        assert!(!paired_away(9, Some(t(9)), Some((t(9), false))));
        assert!(!paired_away(9, Some(t(5)), Some((t(9), true))));
        assert!(!paired_away(9, None, Some((t(9), true))));
    }

    #[test]
    fn a_child_is_born_while_its_mother_lives() {
        let at = SimTime::from_secs(1_000);
        assert!(lived_at(None, at));
        assert!(lived_at(Some(SimTime::from_secs(1_001)), at));
        assert!(lived_at(Some(at), at));
        assert!(!lived_at(Some(SimTime::from_secs(999)), at));
    }
}

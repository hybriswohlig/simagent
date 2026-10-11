//! Saving and loading (ADR-0006): header, the canonical projection, the saved
//! exceptions. The bytes are uncompressed and fully deterministic.

use crate::data::Data;
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
use crate::ids::{Counter, SimId};
use crate::input::{Input, InputRecord, Start};
use crate::lineage::GenomeKey;
use crate::lineage::{LineageIndex, LineageKey};
use crate::log::PlanRef;
use crate::params::WorldParams;
use crate::rng::{Draws, WorldSeed};
use crate::state::conception;
use crate::state::{
    self, Affliction, ArchivedSim, DomainState, Exposure, LeftBy, Parents, Pregnancy, Sim, Slot,
};
use crate::time::{
    EventHandle, MONTH, Scheduler, SchedulerSnapshot, SimDuration, SimTime, SnapshotEntry, YEAR,
};
use crate::trace::TraceBuffer;
use crate::version::CORE_VERSION;

/// Rises with every change of the layout of a save (ADR-0006 section 2).
pub const FORMAT_VERSION: u32 = 1;

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
        let pending = pending_index(&loaded.queue);
        let world = World {
            sched,
            state: WorldState {
                domain: DomainState {
                    world_seed: loaded.world_seed,
                    world_params: loaded.world_params,
                    sims: loaded.sims,
                    partnerships: loaded.partnerships,
                    pregnancies: loaded.pregnancies,
                    archive: loaded.archive,
                    lineage,
                    pending,
                },
                ids: IdCounters::restore(
                    counter(loaded.next_sim_id)?,
                    counter(loaded.next_partnership_id)?,
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
/// insertion number (ADR-0002). The index is not saved, loading builds it again.
pub(super) fn pending_index(queue: &[QueueEntry]) -> BTreeMap<(SimId, Slot), EventHandle> {
    let mut index = BTreeMap::new();
    for entry in queue {
        let slot = match &entry.payload {
            EventPayload::YearEnd => continue,
            EventPayload::Birthday { sim } => (*sim, Slot::Birthday),
            EventPayload::Death { sim, cause } => (*sim, Slot::Death(*cause)),
            EventPayload::ConceptionAttempt { woman } => (*woman, Slot::Conception),
            EventPayload::Birth { mother } => (*mother, Slot::Birth),
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
            && loaded.sims.is_empty()
            && loaded.partnerships.is_empty()
            && loaded.pregnancies.is_empty()
            && loaded.archive.is_empty()
            && loaded.next_sim_id == 1
            && loaded.next_partnership_id == 1
            && loaded.next_event_seq == 1
            && exceptions.next_event_id == 1
            && exceptions.input_log.is_empty()
            && exceptions.year_hashes.is_empty();
        return empty;
    };
    let Some(history) = input_log_consistent(loaded, exceptions, seed, data) else {
        return false;
    };
    let rules = Rules {
        seed,
        data,
        switch: &history.switch,
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
        && partnerships_consistent(loaded, &history.pairs)
        && pregnancies_consistent(loaded, now, &rules)
        && conceptions_consistent(loaded, &rules)
        && births_consistent(loaded)
        && queue_consistent(loaded, now, &rules)
        && exceptions.plan_refs.iter().all(|plan| {
            plan.by.is_below(exceptions.next_event_id) && plan.planned_at.as_secs() <= now
        })
        && year_hashes_complete(true, loaded.closed_through, &exceptions.year_hashes)
}

/// What the input log says beyond the projection.
struct InputHistory {
    /// The switch of the hereditary diseases over time (see `Rules`).
    switch: SwitchHistory,
    /// (woman, man, formed) of each partnership the inputs formed, in their order.
    pairs: Vec<(SimId, SimId, SimTime)>,
    /// How many founders `Start` names: the sims 1 to N.
    founders: u64,
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
    })
}

/// The partnerships are those the input log formed, with the IDs from 1 in that order,
/// and the counter follows them. Each pairs a woman and a man; it is open while both
/// live and ended in the second of the first death of the two, never before it was
/// formed. Each of the two was free when it was formed: an earlier partnership of theirs
/// had ended in an earlier second (a pairing acts before the events of its second).
fn partnerships_consistent(loaded: &LoadedState, pairs: &[(SimId, SimId, SimTime)]) -> bool {
    let counted = u64::try_from(pairs.len())
        .ok()
        .and_then(|count| count.checked_add(1))
        == Some(loaded.next_partnership_id);
    let sex_of = |id: &SimId| {
        loaded
            .sims
            .get(id)
            .map(|sim| sim.sex)
            .or_else(|| loaded.archive.get(id).map(|sim| sim.sex))
    };
    let left_of = |id: &SimId| loaded.archive.get(id).map(|sim| sim.left);
    // Per sim the end of its latest partnership so far (`None` while it is open); the
    // partnerships come in the order they were formed.
    let mut latest: BTreeMap<SimId, Option<SimTime>> = BTreeMap::new();
    counted
        && loaded.partnerships.len() == pairs.len()
        && loaded.partnerships.iter().zip(pairs).zip(1_u64..).all(
            |(((id, partnership), &(woman, man, formed)), number)| {
                let first_death = [left_of(&woman), left_of(&man)].into_iter().flatten().min();
                id.get() == number
                    && (partnership.woman, partnership.man, partnership.formed)
                        == (woman, man, formed)
                    && sex_of(&woman) == Some(Sex::Female)
                    && sex_of(&man) == Some(Sex::Male)
                    && partnership.ended == first_death
                    && first_death.is_none_or(|death| death >= formed)
                    && [woman, man].into_iter().all(|sim| {
                        match latest.insert(sim, partnership.ended) {
                            None => true,
                            Some(earlier) => earlier.is_some_and(|end| end < formed),
                        }
                    })
            },
        )
}

/// The switch of the hereditary diseases: each value with the second from which it holds.
type SwitchHistory = Vec<(i64, bool)>;

/// What the sims are checked against: the seed, the causes of death of `Start`, and the
/// history of the switch of the hereditary diseases from the input log, as the second
/// from which each value holds (`Start` at the start, an override block one second
/// after its stamp). All sims are founders so far: the causes act from the start on.
struct Rules<'a> {
    seed: u64,
    data: &'a Data,
    switch: &'a [(i64, bool)],
}

impl Rules<'_> {
    /// When a sim dies of a lethal affliction if nothing else comes first.
    fn disease_due(
        &self,
        born: SimTime,
        lineage_key: LineageKey,
        affliction: &Affliction,
    ) -> Option<SimTime> {
        let disease = state::disease::of_key(self.data, affliction.disease)?;
        state::disease::due(self.seed, born, lineage_key, affliction, disease)
    }

    /// The causes of a sim in their order: senescence, accident, then its lethal
    /// afflictions (ADR-0017 section 2). Each with its key and when it is due.
    fn dues(
        &self,
        facts: (SimTime, LineageKey, Sex, &Genome),
        afflictions: &[Affliction],
    ) -> Vec<(u64, Option<SimTime>)> {
        let (born, lineage_key, _, _) = facts;
        let everyone = state::death::everyone_dues(self.data, self.seed, facts);
        let diseases = afflictions
            .iter()
            .filter(|affliction| affliction.exposure.is_some())
            .map(|affliction| {
                let due = self.disease_due(born, lineage_key, affliction);
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
        lineage_key: LineageKey,
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
                    let due = state::disease::due(self.seed, born, lineage_key, &open, disease);
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
        facts: (SimTime, LineageKey, Sex, &Genome),
        afflictions: &[Affliction],
        origin: i64,
        at: i64,
    ) -> bool {
        let (born, lineage_key, sex, genome) = facts;
        let drawn = state::disease::afflictions(
            self.data,
            self.seed,
            lineage_key,
            sex,
            genome,
            false,
            SimTime::from_secs(origin),
        );
        drawn.len() == afflictions.len()
            && drawn.iter().zip(afflictions).all(|(drawn, held)| {
                let same = drawn.disease == held.disease
                    && drawn.onset == held.onset
                    && drawn.exposure.is_some() == held.exposure.is_some();
                same && held.exposure.is_none_or(|_| {
                    self.exposure_at(born, lineage_key, held, origin, at) == held.exposure
                })
            })
    }
}

/// The founders are the sims 1 to N, living or archived, with their birth times, sex
/// and genome from `Start`, the lineage key of their place in its list and no parents.
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
            let Ok(genome) = Genome::with_alleles(founder.sex, &founder.alleles, data) else {
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
    let origin = |born: SimTime| state::death::origin(born).as_secs();
    loaded.sims.values().all(|sim| {
        rules.afflictions_consistent(
            (sim.born, sim.lineage_key, sim.sex, &sim.genome),
            &sim.afflictions,
            origin(sim.born),
            now,
        )
    }) && loaded.archive.values().all(|sim| {
        let left = sim.left.as_secs();
        left < now
            && rules.afflictions_consistent(
                (sim.born, sim.lineage_key, sim.sex, &sim.genome),
                &sim.afflictions,
                origin(sim.born),
                left,
            )
            && died_by_the_rule(sim, rules)
    })
}

/// What a sim, living or archived, is born as.
struct Facts {
    born: SimTime,
    lineage_key: LineageKey,
    sex: Sex,
    genome: Genome,
    parents: Option<Parents>,
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
        }),
        (None, Some(sim)) => Some(Facts {
            born: sim.born,
            lineage_key: sim.lineage_key,
            sex: sim.sex,
            genome: sim.genome,
            parents: sim.parents,
        }),
        (None, None) | (Some(_), Some(_)) => None,
    }
}

/// The SimIds are 1 to the counter without a gap, each in one place (`facts`): the
/// founders first (`founders_are_the_first_sims`), then the born. Each born sim is the child of a
/// conception the rule allows, born as many months later as a pregnancy lasts and in a
/// closed second.
fn born_consistent(loaded: &LoadedState, now: i64, rules: &Rules<'_>, founders: u64) -> bool {
    let below_counter = |id: &SimId| id.get() < loaded.next_sim_id;
    loaded
        .sims
        .keys()
        .chain(loaded.archive.keys())
        .all(below_counter)
        && (founders.saturating_add(1)..loaded.next_sim_id).all(|number| {
            let Some(child) = SimId::try_from(number)
                .ok()
                .and_then(|id| facts(loaded, id))
            else {
                return false;
            };
            let Some(parents) = child.parents else {
                return false;
            };
            let months = MONTH.checked_mul(i64::from(pregnancy_months(rules)));
            let Some(conceived) = months.and_then(|months| child.born.checked_sub(months)) else {
                return false;
            };
            child.born.as_secs() < now && conception_fits(loaded, rules, parents, conceived, &child)
        })
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
    let formed = loaded.partnerships.values().find_map(|partnership| {
        ((partnership.woman, partnership.man) == (parents.mother, parents.father)
            && partnership.formed <= conceived
            && partnership.ended.is_none_or(|ended| ended >= conceived))
        .then_some(partnership.formed)
    });
    let at_a_month_start =
        conceived.as_secs() >= 0 && conceived.as_secs().rem_euclid(MONTH.as_secs()) == 0;
    let in_her_series = formed.is_some_and(|formed| {
        let born_before = children_born(loaded, parents.mother, conceived);
        series_start(rules, formed, &born_before).is_some_and(|start| start <= conceived)
            && conception::may_begin(
                rules.data,
                rules.seed,
                her.born,
                her.lineage_key,
                u32::try_from(born_before.len()).unwrap_or(u32::MAX),
                conceived,
            )
    });
    let drawn = conception::month_index(conceived).is_some_and(|month| {
        let key = conception::attempt_key(her.lineage_key, him.lineage_key, month);
        let (Some(woman), Some(man)) = (person(loaded, &her), person(loaded, &him)) else {
            return false;
        };
        let p = conception::chance(rules.data, rules.seed, &woman, &man, conceived);
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

/// The birth times of her children born before `before`, in order.
fn children_born(loaded: &LoadedState, mother: SimId, before: SimTime) -> Vec<SimTime> {
    let mut born: Vec<SimTime> = loaded
        .sims
        .values()
        .map(|sim| (sim.parents, sim.born))
        .chain(loaded.archive.values().map(|sim| (sim.parents, sim.born)))
        .filter(|(parents, born)| parents.is_some_and(|p| p.mother == mother) && *born < before)
        .map(|(_, born)| born)
        .collect();
    born.sort_unstable();
    born
}

/// Where her series of attempts in a partnership formed at `formed` begins, given the
/// times of her births so far: after the pairing, or after the pause of her last birth in
/// that partnership (ADR-0017 section 4).
fn series_start(rules: &Rules<'_>, formed: SimTime, births: &[SimTime]) -> Option<SimTime> {
    let pause = rules.data.demography().fertility.pause_months;
    match births.iter().rev().find(|born| **born >= formed) {
        Some(born) => conception::series_after_birth(*born, pause),
        None => conception::series_after_pairing(formed),
    }
}

/// What the conception rule reads of a sim: its facts and the lineage keys of its
/// parents.
fn person<'a>(loaded: &LoadedState, sim: &'a Facts) -> Option<conception::Person<'a>> {
    let key_of = |id: SimId| facts(loaded, id).map(|parent| parent.lineage_key);
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
    })
}

/// Each sim has as many live births as there are sims whose mother it is; a man has none
/// (ADR-0017 section 4).
fn births_consistent(loaded: &LoadedState) -> bool {
    let mut children: BTreeMap<SimId, u32> = BTreeMap::new();
    for parents in loaded
        .sims
        .values()
        .map(|sim| sim.parents)
        .chain(loaded.archive.values().map(|sim| sim.parents))
        .flatten()
    {
        let count = children.entry(parents.mother).or_default();
        *count = count.saturating_add(1);
    }
    let own = |id: &SimId| children.get(id).copied().unwrap_or(0);
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
        let parents = Parents {
            mother: *mother,
            father: pregnancy.father,
            conception: pregnancy.conception,
        };
        let child = |unborn: &state::Unborn| Facts {
            born: pregnancy.conceived,
            lineage_key: unborn.lineage_key,
            sex: unborn.sex,
            genome: unborn.genome,
            parents: Some(parents),
        };
        // All the children of the conception, in the order of their index.
        let complete = facts(loaded, *mother)
            .zip(facts(loaded, pregnancy.father))
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
    let mut born: BTreeMap<(SimId, SimId, u32), Vec<(LineageKey, SimTime, u64)>> = BTreeMap::new();
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
            in_order
                && children.windows(2).all(|pair| {
                    pair[0].1 == pair[1].1 && pair[0].2.checked_add(1) == Some(pair[1].2)
                })
        })
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
        let all = (0..count).eq(numbers.iter().copied());
        let but_the_last = archived && count >= 1 && (0..count - 1).eq(numbers.iter().copied());
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
    let formed = loaded
        .partnerships
        .values()
        .find(|partnership| partnership.woman == woman && partnership.ended.is_none())?
        .formed;
    if loaded.pregnancies.contains_key(&woman) {
        return None;
    }
    let clock = SimTime::try_from(now.checked_sub(1)?).ok()?;
    let births = children_born(loaded, woman, SimTime::try_from(now).ok()?);
    let start = series_start(rules, formed, &births)?;
    let after_now = conception::month_start_after(clock)?;
    let next = if start > after_now { start } else { after_now };
    conception::may_begin(
        rules.data,
        rules.seed,
        her.born,
        her.lineage_key,
        her.births,
        next,
    )
    .then_some(next)
}

/// An archived sim died of a cause of the world, in the second its budget for that
/// cause ran out, and no other cause was due before. Of two causes due in the same
/// second the one earlier in the list wins: its event was planned first. A disease was
/// due only if its exposure kept with the sim ran at the death.
fn died_by_the_rule(sim: &ArchivedSim, rules: &Rules<'_>) -> bool {
    let LeftBy::Death { cause } = sim.left_by;
    let dues = rules.dues(
        (sim.born, sim.lineage_key, sim.sex, &sim.genome),
        &sim.afflictions,
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
    let mut birthdays = BTreeSet::new();
    let mut births = BTreeSet::new();
    let mut attempts = BTreeSet::new();
    let mut deaths = BTreeSet::new();
    // Per death: the sim, the insertion number and the place of the cause in the list.
    let mut planned = Vec::new();
    let dues = |person: &Sim| {
        rules.dues(
            (person.born, person.lineage_key, person.sex, &person.genome),
            &person.afflictions,
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
                        conception::birth_of(rules.data, pregnancy.conceived) == Some(entry.time)
                    })
            }
            EventPayload::ConceptionAttempt { woman } => {
                entry.period == Some(MONTH)
                    && attempts.insert(*woman)
                    && next_attempt(loaded, now, rules, *woman) == Some(entry.time)
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
    year_end_seen
        && birthdays.len() == loaded.sims.len()
        && all_deaths_planned
        && in_order
        && all_attempts_planned
        && births.len() == loaded.pregnancies.len()
}

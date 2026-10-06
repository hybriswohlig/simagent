//! Saving and loading (ADR-0006): header, the canonical projection, the saved
//! exceptions. The bytes are uncompressed and fully deterministic.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::ctx::{IdCounters, LogBuffer};
use super::{EventPayload, InputLog, World, WorldState};
use crate::canonical::{
    self, LoadedState, PROJECTION_VERSION, ProjectedCause, QueueEntry, YearHash,
};
use crate::genome::{Genotype, Sex};
use crate::ids::{Counter, SimId};
use crate::input::{AppliesTo, DeathCause, Input, InputRecord, Start};
use crate::lineage::{LineageIndex, LineageKey};
use crate::log::PlanRef;
use crate::params::WorldParams;
use crate::state::{self, ArchivedSim, DomainState, Exposure, LeftBy, Sim, Slot};
use crate::time::{
    EventHandle, Scheduler, SchedulerSnapshot, SimDuration, SimTime, SnapshotEntry, YEAR,
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
        let mut bytes = header_bytes(self.data_hash);
        bytes.extend(canonical::canonical_bytes(&self.state, &self.sched));
        bytes.extend(encode(&exceptions));
        Ok(bytes)
    }

    /// Rebuilds a world from a save. `data_hash` is the data hash of the definitions
    /// the caller has loaded. Reads field by field; the first violation in the order of
    /// ADR-0006 section 4 names the error.
    pub fn load(bytes: &[u8], data_hash: u64) -> Result<(World, Vec<YearHash>), LoadError> {
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
        let death_causes = consistent(&loaded, &exceptions).ok_or(LoadError::Inconsistent)?;
        // `consistent` has checked the ranges of the three counters.
        let counter = |next: u64| Counter::from_next(next).ok_or(LoadError::Inconsistent);
        let next_input = u64::try_from(exceptions.input_log.len())
            .ok()
            .and_then(|len| len.checked_add(1))
            .ok_or(LoadError::Inconsistent)?;
        // Every sim is a founder with the key of its index (`consistent`), so the keys
        // differ exactly where the uninterrupted run had them differ.
        let lineage = lineage_index(&loaded.sims, &loaded.archive);
        let pending = pending_index(&loaded.queue);
        let world = World {
            sched,
            state: WorldState {
                domain: DomainState {
                    world_seed: loaded.world_seed,
                    world_params: loaded.world_params,
                    death_causes,
                    sims: loaded.sims,
                    partnerships: loaded.partnerships,
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
            },
            inputs: InputLog {
                next_seq: counter(next_input)?,
                records: exceptions.input_log,
            },
            data_hash,
        };
        Ok((world, exceptions.year_hashes))
    }
}

/// The lineage keys of a saved state: those of the living and of the archive (ADR-0001
/// section 4). The index is not saved, loading builds it again.
pub(super) fn lineage_index(
    sims: &BTreeMap<SimId, Sim>,
    archive: &BTreeMap<SimId, ArchivedSim>,
) -> LineageIndex {
    let mut index = LineageIndex::new();
    let living = sims.values().map(|sim| sim.lineage_key);
    for key in living.chain(archive.values().map(|sim| sim.lineage_key)) {
        index.insert(key);
    }
    index
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
        };
        index.insert(slot, EventHandle::from_seq(entry.seq));
    }
    index
}

/// The causes of death of the world if the saved state is one the core can reach, as
/// far as that shows without a replay (ADR-0006 section 5); `None` if it is not. The
/// projection holds the causes by key, their codes come from `Start`. The queue has
/// passed the scheduler's own checks. The ranges of the ID counters are checked when
/// the world is built from them.
fn consistent(loaded: &LoadedState, exceptions: &Exceptions) -> Option<Vec<DeathCause>> {
    let closed = loaded.closed_through.as_secs();
    if closed < SimTime::START.as_secs() - 1 {
        return None;
    }
    let now = closed + 1;
    let Some(seed) = loaded.world_seed else {
        // Never started: the world is empty. The queue is empty too: an event needs an
        // insertion number below the counter, which stands at 1.
        let empty = loaded.world_params.is_none()
            && loaded.death_causes.is_empty()
            && loaded.sims.is_empty()
            && loaded.partnerships.is_empty()
            && loaded.archive.is_empty()
            && loaded.next_sim_id == 1
            && loaded.next_partnership_id == 1
            && loaded.next_event_seq == 1
            && exceptions.next_event_id == 1
            && exceptions.input_log.is_empty()
            && exceptions.year_hashes.is_empty();
        return empty.then(Vec::new);
    };
    let history = input_log_consistent(loaded, exceptions, seed)?;
    let rules = Rules {
        seed,
        causes: &history.causes,
        switch: &history.switch,
    };
    let rest = sims_consistent(loaded, now, &rules)
        && partnerships_consistent(loaded, &history.pairs)
        && queue_consistent(loaded, now, &rules)
        && exceptions.plan_refs.iter().all(|plan| {
            plan.by.is_below(exceptions.next_event_id) && plan.planned_at.as_secs() <= now
        })
        && year_hashes_complete(true, loaded.closed_through, &exceptions.year_hashes);
    rest.then_some(history.causes)
}

/// What the input log says beyond the projection.
struct InputHistory {
    /// The causes of death of `Start`.
    causes: Vec<DeathCause>,
    /// The switch of the hereditary diseases over time (see `Rules`).
    switch: SwitchHistory,
    /// (woman, man, formed) of each partnership the inputs formed, in their order.
    pairs: Vec<(SimId, SimId, SimTime)>,
}

/// Sequence numbers gapless from 1, stamps that never fall and never lie after the
/// clock. `Start` first and only first, stamped before the start, with the seed of the
/// world, its causes of death and the founders as the sims 1 to N; then override blocks
/// that pass their own check, and pairings. The parameters of `Start` after every
/// override are those of the world. Gives what the log says beyond the projection, or
/// `None` if anything is off.
fn input_log_consistent(
    loaded: &LoadedState,
    exceptions: &Exceptions,
    seed: u64,
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
        match (&record.input, started) {
            (Input::Start(start), None) => {
                let unstarted = DomainState::new();
                let fits = state::start::check_start(&unstarted, record.t, start).is_ok()
                    && start.world_seed == seed
                    && founders_are_the_first_sims(loaded, start)
                    && causes_are_those_of(loaded, start);
                if !fits {
                    return None;
                }
                started = Some((start.world_params, start));
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
                if block.rejection().is_some() {
                    return None;
                }
                let after = block
                    .changes
                    .iter()
                    .fold(before, |params, change| params.with(*change));
                started = Some((after, start));
                // Stamped with the last closed second, the block acts in the next one.
                switch.push((record.t.as_secs() + 1, after.hereditary_diseases));
            }
            (Input::ArrangePartnership(pair), Some(_)) => {
                let formed = record.t.checked_add(SimDuration::from_secs(1))?;
                pairs.push((pair.woman, pair.man, formed));
            }
            // A second `Start`, or another input before the first.
            (Input::Start(_), Some(_))
            | (Input::ForkOverride(_) | Input::ArrangePartnership(_), None) => return None,
        }
    }
    let (params, start) = started?;
    (Some(params) == loaded.world_params).then(|| InputHistory {
        causes: start.death_causes.clone(),
        switch,
        pairs,
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
    causes: &'a [DeathCause],
    switch: &'a [(i64, bool)],
}

impl Rules<'_> {
    /// When a sim dies of `cause` if nothing else comes first.
    fn due(
        &self,
        lineage_key: LineageKey,
        exposure: Option<Exposure>,
        cause: &DeathCause,
    ) -> Option<SimTime> {
        state::death::due(self.seed, lineage_key, exposure, cause, SimTime::START)
    }

    /// The exposure the switch history gives a founder at the second `at`: the closed
    /// periods of the switch before, and since when it is on.
    fn exposure_at(&self, at: i64) -> (i64, Option<i64>) {
        let mut before = 0_i64;
        let mut since = None;
        for &(from, on) in self.switch.iter().filter(|(from, _)| *from <= at) {
            match (since, on) {
                (None, true) => since = Some(from),
                (Some(begin), false) => {
                    before += from - begin;
                    since = None;
                }
                (None, false) | (Some(_), true) => {}
            }
        }
        (before, since)
    }

    /// A sim has an exposure exactly if it has the dummy allele twice, and then the one
    /// the switch history gives it at `at` (now for the living, the death for the
    /// archive). No hereditary cause has used up more than its budget: the sim would
    /// have died of it. Exactly used up is possible: the switch went off in the very
    /// second of the death.
    fn exposure_consistent(
        &self,
        genotype: Genotype,
        lineage_key: LineageKey,
        exposure: Option<Exposure>,
        at: i64,
    ) -> bool {
        match (genotype.dummy_homozygous(), exposure) {
            (false, None) => true,
            (true, Some(exposure)) => {
                let held = (
                    exposure.before.as_secs(),
                    exposure.since.map(SimTime::as_secs),
                );
                held == self.exposure_at(at)
                    && self
                        .causes
                        .iter()
                        .filter(|cause| cause.applies_to == AppliesTo::DummyHomozygous)
                        .all(|cause| {
                            state::death::needed(self.seed, lineage_key, cause)
                                .is_none_or(|needed| exposure.before <= needed)
                        })
            }
            (false, Some(_)) | (true, None) => false,
        }
    }
}

/// The causes of death of the projection are those of `Start`, in its order: each by
/// the key of its code, with its hazard.
fn causes_are_those_of(loaded: &LoadedState, start: &Start) -> bool {
    let projected = start.death_causes.iter().map(|cause| ProjectedCause {
        cause: cause.cause.key(),
        per_year: cause.per_year,
        applies_to: cause.applies_to,
    });
    loaded.death_causes.iter().copied().eq(projected)
}

/// The founders are the sims 1 to N, living or archived, with their birth times, sex
/// and genotype from `Start` and the lineage key of their place in its list. As long as
/// nothing else creates sims, the counter stands at N + 1.
fn founders_are_the_first_sims(loaded: &LoadedState, start: &Start) -> bool {
    let mut next_id = 1_u64;
    for (index, founder) in (0_u64..).zip(&start.founders) {
        let Ok(id) = SimId::try_from(next_id) else {
            return false;
        };
        let found = match (loaded.sims.get(&id), loaded.archive.get(&id)) {
            (Some(sim), None) => (sim.born, sim.lineage_key, sim.sex, sim.genotype),
            (None, Some(sim)) => (sim.born, sim.lineage_key, sim.sex, sim.genotype),
            (None, None) | (Some(_), Some(_)) => return false,
        };
        let lineage_key = LineageKey::founder(&start.founder_tag, index);
        if found != (founder.born, lineage_key, founder.sex, founder.genotype) {
            return false;
        }
        next_id += 1;
    }
    loaded.next_sim_id == next_id
}

/// Every ID below the counter; every exposure as the switch history gives it, for the
/// archive at the death; a departure in a closed second (an event of the first open
/// second has not run yet) and as the death rule gives it (so never before the birth:
/// the causes act from the start on). All sims are founders so far:
/// `founders_are_the_first_sims` has shown that each is in one place only and born
/// before the start. The first rule that creates sims needs both checks here (ADR-0006
/// section 5).
fn sims_consistent(loaded: &LoadedState, now: i64, rules: &Rules<'_>) -> bool {
    let below_counter = |id: &SimId| id.get() < loaded.next_sim_id;
    loaded.sims.iter().all(|(id, sim)| {
        below_counter(id)
            && rules.exposure_consistent(sim.genotype, sim.lineage_key, sim.exposure, now)
    }) && loaded.archive.iter().all(|(id, sim)| {
        let left = sim.left.as_secs();
        below_counter(id)
            && left < now
            && rules.exposure_consistent(sim.genotype, sim.lineage_key, sim.exposure, left)
            && died_by_the_rule(sim, rules)
    })
}

/// An archived sim died of a cause of the world, in the second its budget for that
/// cause ran out, and no other cause was due before. Of two causes due in the same
/// second the one earlier in the list wins: its event was planned first. A hereditary
/// cause was due only if the exposure kept with the sim ran at the death.
fn died_by_the_rule(sim: &ArchivedSim, rules: &Rules<'_>) -> bool {
    let LeftBy::Death { cause } = sim.left_by;
    let causes = rules.causes;
    let Some(position) = causes.iter().position(|other| other.cause.key() == cause) else {
        return false;
    };
    let due = |cause: &DeathCause| rules.due(sim.lineage_key, sim.exposure, cause);
    due(&causes[position]) == Some(sim.left)
        && causes
            .iter()
            .enumerate()
            .all(|(index, other)| match due(other) {
                None => true,
                Some(at) => at > sim.left || (at == sim.left && index >= position),
            })
}

/// One year end series, due at the next year end; one birthday per living sim, due at
/// its next anniversary; per living sim and cause of death one death, due when the
/// budget for that cause runs out, and none where it never does; nothing for a sim that
/// does not live.
fn queue_consistent(loaded: &LoadedState, now: i64, rules: &Rules<'_>) -> bool {
    let causes = rules.causes;
    let next_year_end = (now.div_euclid(YEAR.as_secs()) + 1) * YEAR.as_secs() - 1;
    let mut year_end_seen = false;
    let mut birthdays = BTreeSet::new();
    let mut deaths = BTreeSet::new();
    // Per death: the sim, the insertion number and the place of the cause in the list.
    let mut planned = Vec::new();
    let death_due = |person: &Sim, key: u64| {
        let cause = causes.iter().find(|cause| cause.cause.key() == key)?;
        rules.due(person.lineage_key, person.exposure, cause)
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
                let position = causes.iter().position(|other| other.cause.key() == *cause);
                planned.push((*sim, entry.seq, position));
                entry.period.is_none()
                    && deaths.insert((*sim, *cause))
                    && loaded
                        .sims
                        .get(sim)
                        .is_some_and(|person| death_due(person, *cause) == Some(entry.time))
            }
        };
        if !expected {
            return false;
        }
    }
    // Every death that is due is in the queue: a living sim whose budget has run out
    // has no event there, the scheduler holds none in the past.
    let all_deaths_planned = loaded.sims.iter().all(|(id, person)| {
        causes.iter().all(|cause| {
            let key = cause.cause.key();
            death_due(person, key).is_none() || deaths.contains(&(*id, key))
        })
    });
    // The deaths of a sim were planned in the order of the causes: of two due in the same
    // second the earlier cause runs first, as `died_by_the_rule` expects.
    planned.sort_unstable();
    let in_order = planned
        .windows(2)
        .all(|pair| pair[0].0 != pair[1].0 || pair[0].2 < pair[1].2);
    year_end_seen && birthdays.len() == loaded.sims.len() && all_deaths_planned && in_order
}

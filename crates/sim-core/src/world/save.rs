//! Saving and loading (ADR-0006): header, the canonical projection, the saved
//! exceptions. The bytes are uncompressed and fully deterministic.

use alloc::collections::BTreeSet;
use alloc::vec::Vec;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::ctx::{IdCounters, LogBuffer};
use super::{EventPayload, InputLog, World, WorldState};
use crate::canonical::{self, LoadedState, PROJECTION_VERSION, YearHash};
use crate::ids::{Counter, SimId};
use crate::input::{Input, InputRecord};
use crate::log::PlanRef;
use crate::state::{self, DomainState};
use crate::time::{
    MAX_ABS_SECS, Scheduler, SchedulerSnapshot, SimDuration, SimTime, SnapshotEntry, YEAR,
};
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
        if !rest.is_empty() {
            return Err(LoadError::Malformed);
        }
        // Canonical: the parts as read must encode to exactly the bytes that were read.
        // serde and postcard accept repeated or unsorted map keys and overlong varints.
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
        if !consistent(&loaded, &exceptions) {
            return Err(LoadError::Inconsistent);
        }
        // `consistent` has checked the ranges of the three counters.
        let counter = |next: u64| Counter::from_next(next).ok_or(LoadError::Inconsistent);
        let next_input = u64::try_from(exceptions.input_log.len())
            .ok()
            .and_then(|len| len.checked_add(1))
            .ok_or(LoadError::Inconsistent)?;
        let world = World {
            sched,
            state: WorldState {
                domain: DomainState {
                    world_seed: loaded.world_seed,
                    sims: loaded.sims,
                    archive: loaded.archive,
                },
                ids: IdCounters::restore(counter(loaded.next_sim_id)?),
                log: LogBuffer::restore(counter(exceptions.next_event_id)?),
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

/// True if the saved state is one the core can reach, as far as that shows without a
/// replay (ADR-0006 section 5). The queue has passed the scheduler's own checks.
fn consistent(loaded: &LoadedState, exceptions: &Exceptions) -> bool {
    let closed = loaded.closed_through.as_secs();
    if closed < SimTime::START.as_secs() - 1 {
        return false;
    }
    let now = closed + 1;
    let counters_in_range = Counter::from_next(loaded.next_sim_id).is_some()
        && Counter::from_next(exceptions.next_event_id).is_some()
        && loaded.next_event_seq >= 1;
    if !counters_in_range {
        return false;
    }
    let Some(seed) = loaded.world_seed else {
        // Never started: the world is empty.
        return loaded.sims.is_empty()
            && loaded.archive.is_empty()
            && loaded.queue.is_empty()
            && loaded.next_sim_id == 1
            && loaded.next_event_seq == 1
            && exceptions.next_event_id == 1
            && exceptions.input_log.is_empty()
            && exceptions.year_hashes.is_empty();
    };
    // A started world always has its next year end ahead, inside the time range.
    if now > MAX_ABS_SECS {
        return false;
    }
    input_log_consistent(loaded, exceptions, seed)
        && sims_consistent(loaded, now)
        && queue_consistent(loaded, closed)
        && exceptions.plan_refs.iter().all(|plan| {
            plan.by.is_below(exceptions.next_event_id) && plan.planned_at.as_secs() <= now
        })
        && year_hashes_complete(true, loaded.closed_through, &exceptions.year_hashes)
}

/// Sequence numbers gapless from 1, stamps in order and not after the clock, `Start`
/// first and only first, and its founders are the sims 1 to N.
fn input_log_consistent(loaded: &LoadedState, exceptions: &Exceptions, seed: u64) -> bool {
    if exceptions.input_log.is_empty() {
        return false;
    }
    let mut previous = SimTime::START - SimDuration::from_secs(1);
    for (record, position) in exceptions.input_log.iter().zip(1_u64..) {
        if record.seq.get() != position || record.t < previous || record.t > loaded.closed_through {
            return false;
        }
        previous = record.t;
        let accepted = match (&record.input, position) {
            (Input::Start(start), 1) => {
                let unstarted = DomainState::new();
                state::start::check_start(&unstarted, record.t, start).is_ok()
                    && start.world_seed == seed
                    && founders_are_the_first_sims(loaded, start.founders.iter().map(|f| f.born))
            }
            (Input::Start(_), _) => false,
        };
        if !accepted {
            return false;
        }
    }
    true
}

/// The founders are the sims 1 to N, living or archived, with their birth times. As
/// long as nothing else creates sims, the counter stands at N + 1.
fn founders_are_the_first_sims(
    loaded: &LoadedState,
    founders: impl Iterator<Item = SimTime>,
) -> bool {
    let mut next_id = 1_u64;
    for born in founders {
        let Ok(id) = SimId::try_from(next_id) else {
            return false;
        };
        let found = match (loaded.sims.get(&id), loaded.archive.get(&id)) {
            (Some(sim), None) => sim.born,
            (None, Some(sim)) => sim.born,
            (None, None) | (Some(_), Some(_)) => return false,
        };
        if found != born {
            return false;
        }
        next_id += 1;
    }
    loaded.next_sim_id == next_id
}

/// Every ID below the counter and in one place only; births and departures not after
/// the first open second, a departure not before the birth.
fn sims_consistent(loaded: &LoadedState, now: i64) -> bool {
    let below_counter = |id: u64| id < loaded.next_sim_id;
    loaded.sims.iter().all(|(id, sim)| {
        below_counter(id.get()) && !loaded.archive.contains_key(id) && sim.born.as_secs() <= now
    }) && loaded.archive.iter().all(|(id, sim)| {
        below_counter(id.get()) && sim.born <= sim.left && sim.left.as_secs() <= now
    })
}

/// One year end series, due at the next year end; one birthday per living sim, due at
/// its next anniversary; nothing for a sim that does not live.
fn queue_consistent(loaded: &LoadedState, closed: i64) -> bool {
    let now = closed + 1;
    let next_year_end = (now.div_euclid(YEAR.as_secs()) + 1) * YEAR.as_secs() - 1;
    let mut year_ends = 0_u32;
    let mut birthdays = BTreeSet::new();
    for entry in &loaded.queue {
        let due = entry.time.as_secs();
        let expected = match &entry.payload {
            EventPayload::YearEnd => {
                year_ends += 1;
                entry.period == Some(YEAR) && due == next_year_end
            }
            EventPayload::Birthday { sim } => {
                entry.period.is_none()
                    && birthdays.insert(*sim)
                    && loaded.sims.get(sim).is_some_and(|person| {
                        person.born.as_secs() <= now
                            && due
                                == state::start::next_anniversary_secs(
                                    person.born,
                                    SimTime::from_secs(now),
                                )
                    })
            }
        };
        if !expected {
            return false;
        }
    }
    year_ends == 1 && birthdays.len() == loaded.sims.len()
}

//! Canonical projection and state hash (ADR-0003 section 5): the one view of the state
//! that hashes, goldens, forks and saves agree on.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::hash::stable_hash64;
use crate::ids::SimId;
use crate::log::PlanRef;
use crate::state::{ArchivedSim, Sim};
use crate::time::{Scheduler, SimDuration, SimTime};
use crate::world::{EventPayload, WorldState};

/// The hash of the state the year end observer saw in the last second of `year`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct YearHash {
    pub year: i64,
    #[serde(with = "crate::hex_u64")]
    pub hash: u64,
}

/// Rises with every change of the projection (ADR-0003 5.2).
pub const PROJECTION_VERSION: u32 = 1;

/// A pending event as the projection sees it (ADR-0002): phase, class, subject and kind
/// follow from the payload; the companion data is left out.
#[derive(Serialize)]
struct QueueEntry {
    time: SimTime,
    micro: u16,
    seq: u64,
    payload: EventPayload,
    period: Option<SimDuration>,
}

/// The canonical view of the state, in the field order of ADR-0003 5.2. Maps serialize
/// in key order, so entities come by ascending ID.
#[derive(Serialize)]
pub struct CanonicalState<'a> {
    projection_version: u32,
    world_seed: Option<u64>,
    closed_through: SimTime,
    next_sim_id: u64,
    queue: Vec<QueueEntry>,
    next_event_seq: u64,
    sims: &'a BTreeMap<SimId, Sim>,
    archive: &'a BTreeMap<SimId, ArchivedSim>,
}

/// The projection of the handler state and the scheduler.
///
/// Not included, and this list is complete (ADR-0003 5.3): the index of the queue, the
/// companion data of pending events (plan references), the EventId counter, the log
/// buffer, `RunStats`, the input log with its sequence counter, the core version, the
/// data hash and the commit. A new state field goes in unless it is on this list or
/// can be rebuilt from the projection.
pub fn canonical_projection<'a>(
    state: &'a WorldState,
    sched: &Scheduler<EventPayload, PlanRef>,
) -> CanonicalState<'a> {
    let snapshot = sched.snapshot();
    let domain = state.domain();
    CanonicalState {
        projection_version: PROJECTION_VERSION,
        world_seed: domain.world_seed,
        closed_through: snapshot.closed_through,
        next_sim_id: state.next_sim_id(),
        queue: snapshot
            .entries
            .into_iter()
            .map(|entry| QueueEntry {
                time: entry.time,
                micro: entry.micro,
                seq: entry.seq,
                payload: entry.payload,
                period: entry.period,
            })
            .collect(),
        next_event_seq: snapshot.next_seq,
        sims: &domain.sims,
        archive: &domain.archive,
    }
}

/// The postcard bytes of the canonical projection.
pub fn canonical_bytes(state: &WorldState, sched: &Scheduler<EventPayload, PlanRef>) -> Vec<u8> {
    postcard::to_allocvec(&canonical_projection(state, sched))
        .unwrap_or_else(|_| panic!("invariant canonical.encode"))
}

/// `stable_hash64` over the canonical bytes.
pub fn state_hash(state: &WorldState, sched: &Scheduler<EventPayload, PlanRef>) -> u64 {
    stable_hash64(&canonical_bytes(state, sched))
}

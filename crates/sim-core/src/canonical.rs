//! Canonical projection and state hash (ADR-0003 section 5): the one view of the state
//! that hashes, goldens, forks and saves agree on.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::hash::stable_hash64;
use crate::ids::{HouseholdId, PartnershipId, SimId};
use crate::log::PlanRef;
use crate::params::WorldParams;
use crate::state::{ArchivedSim, Household, Partnership, Pregnancy, Sim};
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
pub const PROJECTION_VERSION: u32 = 18;

/// A pending event as the projection sees it (ADR-0002): phase, class, subject and kind
/// follow from the payload; the companion data is left out.
#[derive(Serialize, Deserialize)]
pub(crate) struct QueueEntry {
    pub(crate) time: SimTime,
    pub(crate) micro: u16,
    pub(crate) seq: u64,
    pub(crate) payload: EventPayload,
    pub(crate) period: Option<SimDuration>,
}

/// The projection as a save holds it: the same fields in the same order as
/// `CanonicalState`, owned, so a load can read them (ADR-0006). The two encode to the
/// same bytes; `tests/save.rs` holds them together.
#[derive(Serialize, Deserialize)]
pub(crate) struct LoadedState {
    pub(crate) projection_version: u32,
    pub(crate) world_seed: Option<u64>,
    pub(crate) world_params: Option<WorldParams>,
    pub(crate) closed_through: SimTime,
    pub(crate) next_sim_id: u64,
    pub(crate) next_partnership_id: u64,
    pub(crate) next_household_id: u64,
    pub(crate) queue: Vec<QueueEntry>,
    pub(crate) next_event_seq: u64,
    pub(crate) sims: BTreeMap<SimId, Sim>,
    pub(crate) partnerships: BTreeMap<PartnershipId, Partnership>,
    pub(crate) households: BTreeMap<HouseholdId, Household>,
    pub(crate) pregnancies: BTreeMap<SimId, Pregnancy>,
    pub(crate) archive: BTreeMap<SimId, ArchivedSim>,
    pub(crate) pool_used: Vec<u32>,
}

/// The canonical view of the state, in the field order of ADR-0003 5.2. Maps serialize
/// in key order, so entities come by ascending ID.
#[derive(Serialize)]
pub struct CanonicalState<'a> {
    projection_version: u32,
    world_seed: Option<u64>,
    world_params: Option<WorldParams>,
    closed_through: SimTime,
    next_sim_id: u64,
    next_partnership_id: u64,
    next_household_id: u64,
    queue: Vec<QueueEntry>,
    next_event_seq: u64,
    sims: &'a BTreeMap<SimId, Sim>,
    partnerships: &'a BTreeMap<PartnershipId, Partnership>,
    households: &'a BTreeMap<HouseholdId, Household>,
    pregnancies: &'a BTreeMap<SimId, Pregnancy>,
    archive: &'a BTreeMap<SimId, ArchivedSim>,
    pool_used: &'a Vec<u32>,
}

/// The projection of the handler state and the scheduler.
///
/// Not included, and this list is complete (ADR-0003 5.3): the index of the queue, the
/// companion data of pending events (plan references), the EventId counter, the log
/// buffer and the trace buffer, caches, indices (among them the number of persons `Start`
/// made, an index of the input log rebuilt on load, ADR-0021), maps from ID to slot,
/// capacities, `RunStats`, the input log with its sequence counter and inputs not applied
/// yet, the trace switch, the core version, the data hash and the commit. A new state
/// field goes in unless it is on this list or can be rebuilt from the projection.
pub fn canonical_projection<'a>(
    state: &'a WorldState,
    sched: &Scheduler<EventPayload, PlanRef>,
) -> CanonicalState<'a> {
    let snapshot = sched.snapshot();
    let domain = state.domain();
    CanonicalState {
        projection_version: PROJECTION_VERSION,
        world_seed: domain.world_seed,
        world_params: domain.world_params.clone(),
        closed_through: snapshot.closed_through,
        next_sim_id: state.next_sim_id(),
        next_partnership_id: state.next_partnership_id(),
        next_household_id: state.next_household_id(),
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
        partnerships: &domain.partnerships,
        households: &domain.households,
        pregnancies: &domain.pregnancies,
        archive: &domain.archive,
        pool_used: &domain.pool_used,
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

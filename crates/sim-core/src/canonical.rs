//! Canonical projection and state hash (ADR-0003 section 5): the one view of the state
//! that hashes, goldens, forks and saves agree on.

use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::hash::stable_hash64;
use crate::log::PlanRef;
use crate::time::Scheduler;
use crate::world::{EventPayload, WorldState};

/// The hash of the state the year end observer saw in the last second of `year`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct YearHash {
    pub year: i64,
    #[serde(with = "crate::hex_u64")]
    pub hash: u64,
}

/// The postcard bytes of the canonical projection.
pub fn canonical_bytes(state: &WorldState, sched: &Scheduler<EventPayload, PlanRef>) -> Vec<u8> {
    let _ = (state, sched);
    Vec::new()
}

/// `stable_hash64` over the canonical bytes.
pub fn state_hash(state: &WorldState, sched: &Scheduler<EventPayload, PlanRef>) -> u64 {
    stable_hash64(&canonical_bytes(state, sched))
}

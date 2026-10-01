//! The domain state (ADR-0003 section 4). Domain code changes it only inside a handler
//! or an applied input and sees the rest of the world through `SimCtx`.

pub(crate) mod start;

use alloc::collections::BTreeMap;

use crate::ids::SimId;
use crate::time::SimTime;

pub(crate) struct Sim {
    pub(crate) born: SimTime,
}

pub(crate) struct DomainState {
    /// `None` until `Start` was applied.
    pub(crate) world_seed: Option<u64>,
    /// The living, by ID.
    pub(crate) sims: BTreeMap<SimId, Sim>,
}

impl DomainState {
    pub(crate) const fn new() -> DomainState {
        DomainState {
            world_seed: None,
            sims: BTreeMap::new(),
        }
    }
}

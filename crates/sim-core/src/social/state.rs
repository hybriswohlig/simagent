//! The social part of the state (ADR-0030 section 5, ADR-0031 section 7): the memories, the
//! bonds, the beliefs and the deeds of the persons, by person. It lives in the house part of
//! the projection (`house::state::HouseState`) and is empty in a run without `MoveIn`. Beliefs
//! and deeds are skipped until the save slice, so a save of this slice still encodes the two
//! maps it encoded before.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::ids::SimId;
use crate::social::belief::{Belief, Deeds};
use crate::social::bond::Bond;
use crate::social::memory::Memory;

/// What the persons remember, the bonds they hold, and the beliefs and deeds they carry. A
/// list is never empty (the entry of a person with none is gone). Memories are sorted by
/// `Memory::key` without a repeated key. Bonds are sorted by the other person, at most
/// `limits.bonds`, never to oneself. Beliefs are sorted by statement key, at most
/// `limits.beliefs`. Deeds stay in the order they came about, at most `limits.deeds`. The
/// memory list of a person who lives in a moved-in household is live (at most `limits.memories`
/// entries, one open `Forget`); that of one who moved out is frozen (at most
/// `limits.keep_on_move_out`, no event). Bonds have no event: outside a house they are
/// settled and do not fall, until a death or an emigration drops the owner's list. Beliefs
/// and deeds freeze on moving out and are dropped at death, the same way.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Social {
    pub memories: BTreeMap<SimId, Vec<Memory>>,
    pub bonds: BTreeMap<SimId, Vec<Bond>>,
    /// Skipped until the save slice stores them (ADR-0031 section 7). Empty on load.
    #[serde(skip)]
    pub beliefs: BTreeMap<SimId, Vec<Belief>>,
    /// Skipped until the save slice stores them. Empty on load.
    #[serde(skip)]
    pub deeds: BTreeMap<SimId, Deeds>,
}

impl Social {
    pub const fn new() -> Social {
        Social {
            memories: BTreeMap::new(),
            bonds: BTreeMap::new(),
            beliefs: BTreeMap::new(),
            deeds: BTreeMap::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.memories.is_empty()
            && self.bonds.is_empty()
            && self.beliefs.is_empty()
            && self.deeds.is_empty()
    }
}

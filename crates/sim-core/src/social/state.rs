//! The social part of the state (ADR-0030 section 5): the memories and the bonds of the
//! persons, by person. It lives in the house part of the projection
//! (`house::state::HouseState`) and is empty in a run without `MoveIn`, where it adds no byte
//! beyond its two empty maps (`HouseState`'s encoding).

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::ids::SimId;
use crate::social::bond::Bond;
use crate::social::memory::Memory;

/// What the persons remember and the bonds they hold. A list is never empty (the entry of a
/// person with none is gone). Memories are sorted by `Memory::key` without a repeated key.
/// Bonds are sorted by the other person, at most `limits.bonds`, never to oneself. The memory
/// list of a person who lives in a moved-in household is live (at most `limits.memories`
/// entries, one open `Forget`); that of one who moved out is frozen (at most
/// `limits.keep_on_move_out`, no event). Bonds have no event: outside a house they are
/// settled and do not fall.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Social {
    pub memories: BTreeMap<SimId, Vec<Memory>>,
    pub bonds: BTreeMap<SimId, Vec<Bond>>,
}

impl Social {
    pub const fn new() -> Social {
        Social {
            memories: BTreeMap::new(),
            bonds: BTreeMap::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.memories.is_empty() && self.bonds.is_empty()
    }
}

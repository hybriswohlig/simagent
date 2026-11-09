//! The social part of the state (ADR-0030 section 5): the memories of the persons, by person. It
//! lives in the house part of the projection (`house::state::HouseState`) and is empty in a run
//! without `MoveIn`, where it adds no byte to the projection (`HouseState`'s encoding).

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::ids::SimId;
use crate::social::memory::Memory;

/// What the persons remember. A list is never empty (the entry of a person with no memory is
/// gone) and is sorted by `Memory::key` without a repeated key. The list of a person who lives
/// in a moved-in household is live (at most `limits.memories` entries, one open `Forget`); that of
/// one who moved out is frozen (at most `limits.keep_on_move_out`, no event).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Social {
    pub memories: BTreeMap<SimId, Vec<Memory>>,
}

impl Social {
    pub const fn new() -> Social {
        Social {
            memories: BTreeMap::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.memories.is_empty()
    }
}

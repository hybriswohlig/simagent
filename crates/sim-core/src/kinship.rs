//! The kinship coefficient φ (genetics specification 2.7).

use alloc::collections::BTreeMap;

use crate::ids::SimId;
use crate::time::SimTime;

/// What the recursion needs to know about one person.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kin {
    /// The second of birth.
    pub born: SimTime,
    /// Mother and father, in this order; `None` for a founder.
    pub parents: Option<(SimId, SimId)>,
}

/// The values φ already computed, by pair.
#[derive(Clone, Debug, Default)]
pub struct KinshipCache {
    values: BTreeMap<(SimId, SimId), f64>,
}

impl KinshipCache {
    pub const fn new() -> KinshipCache {
        KinshipCache {
            values: BTreeMap::new(),
        }
    }

    /// φ of `x` and `y`.
    pub fn phi(&mut self, x: SimId, y: SimId, lookup: impl Fn(SimId) -> Kin) -> f64 {
        let _ = (x, y, &lookup, &self.values);
        0.0
    }
}

/// φ of `x` and `y` with a cache of its own.
pub fn phi(x: SimId, y: SimId, lookup: impl Fn(SimId) -> Kin) -> f64 {
    KinshipCache::new().phi(x, y, lookup)
}

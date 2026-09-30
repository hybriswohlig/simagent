//! The only access to xxhash-rust (ADR-0003 5.1, scan S5): xxh3-64 with seed 0. It is
//! used for the state hash, the log hash, the data hash and `str_key`; sim-cli and xtask
//! call these functions, never xxhash-rust itself.

use xxhash_rust::xxh3::{Xxh3, xxh3_64};

/// xxh3-64 with seed 0 over `bytes`.
pub fn stable_hash64(bytes: &[u8]) -> u64 {
    xxh3_64(bytes)
}

/// Streaming form of `stable_hash64`: the same value for the same bytes, however they
/// are split into `update` calls.
pub struct StableHasher(Xxh3);

impl StableHasher {
    pub fn new() -> StableHasher {
        StableHasher(Xxh3::new())
    }

    pub fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    pub fn digest(&self) -> u64 {
        self.0.digest()
    }
}

impl Default for StableHasher {
    fn default() -> Self {
        Self::new()
    }
}

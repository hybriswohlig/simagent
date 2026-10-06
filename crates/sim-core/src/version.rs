//! Versions that saves and replays are checked against (ADR-0003 1.3, ADR-0006).

/// Rises by 1 with every change of results: a year hash, the log hash or a byte of a
/// save for the same data hash and input log (ADR-0003 1.3).
pub const CORE_VERSION: u32 = 8;

/// The data hash of the empty set of definitions: `stable_hash64` over no bytes. It is
/// the data hash of every run until the first definitions are loaded (ADR-0006).
pub const EMPTY_DATA_HASH: u64 = 0x2d06_8005_38d3_94c2;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_empty_data_hash_is_the_hash_of_nothing() {
        assert_eq!(crate::hash::stable_hash64(&[]), EMPTY_DATA_HASH);
    }
}

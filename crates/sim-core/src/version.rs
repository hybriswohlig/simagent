//! Versions that saves and replays are checked against (ADR-0003 1.3, ADR-0006).

/// Rises by 1 with every change of results: a year hash, the log hash or a byte of a
/// save for the same data hash and input log (ADR-0003 1.3).
pub const CORE_VERSION: u32 = 20;

//! The criteria town (criteria plan step 5, ADR-0020 section 8): P3, P4 city, P5, P9 and F11
//! on a town founded from the pool, `n_start` 10 000, capacity 10 000, immigration on, 200
//! years, the hereditary diseases on, one gate seed. Too large for the gate, so it runs as a
//! tool in release, alone on the machine:
//!
//! `cargo run --release --locked -p xtask -- criteria town --map <dir> --pool <pool.bin>
//! --out <dir>`
//!
//! It writes `export.ndjson` and `report.json` into `<dir>` and is red when a check is red
//! or a negative twin is green.

/// Gate seed of the criteria town (protected list; never reseeded).
pub const SEED_TOWN: u64 = 0x5eed_0004_0000_0001;

#[cfg(test)]
#[path = "town_tests.rs"]
mod tests;

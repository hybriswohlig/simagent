//! The estimators of the criteria (spec 8.1 to 8.3, ADR-0020 section 8, Grok K1 to K5):
//! means, Pearson, the slope, Fisher's z, `Φ` and the power, the median of the first
//! partnership, the period life table, the expectation of P9 and of the village, the
//! tolerances at the actual `n`, and the fixed permutations of the negative twins.
//!
//! Every sum is a left fold from +0.0 over its list in the given (canonical) order and
//! every power repeated multiplication, so a reference that follows the same order agrees
//! bit for bit; only `ln` and `Φ` may differ, within `1e-12 · max(1, |x|)`.

/// Gate seeds of the permutations of the negative twins (protected list; never reseeded).
pub const SEED_P3: u64 = 0x5eed_0003_0000_0001;
pub const SEED_P5: u64 = 0x5eed_0005_0000_0001;
pub const SEED_P9_POLYGENIC: u64 = 0x5eed_0009_0000_0001;
pub const SEED_P9_MC1R: u64 = 0x5eed_0009_0000_0002;

#[cfg(test)]
#[path = "stats_tests.rs"]
mod tests;

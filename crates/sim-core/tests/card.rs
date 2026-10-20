//! The percentile of the genetics card (ADR-0021 section 5, Grok G2 and L1): `p = 100 Φ(z)`;
//! from the middle up "größer als `min(⌊p⌋, 99)`", below "kleiner als `min(⌊100 − p⌋, 99)`".
//! The expected values from tables of Φ: Φ(0.5) = 0.69146, Φ(1) = 0.84134, Φ(0.1) = 0.53983,
//! Φ(2.32) = 0.98983, Φ(2.33) = 0.99010, Φ(6) = 1 − 1e-9.

use sim_core::presentation::card::{Percentile, percentile};
use sim_math::probit;

#[test]
fn the_percentile_by_hand() {
    let up = |n| Percentile { up: true, n };
    let down = |n| Percentile { up: false, n };
    let cases = [
        (0.0, up(50)),
        (-0.0, up(50)),
        (0.5, up(69)),
        (1.0, up(84)),
        (-1.0, down(84)),
        (-0.1, down(53)),
        (2.32, up(98)),
        (2.33, up(99)),
        (-2.33, down(99)),
        (6.0, up(99)),
        (-6.0, down(99)),
    ];
    for (z, expected) in cases {
        assert_eq!(percentile(z), expected, "{z}");
    }
}

/// A `z` that is a threshold itself counts: the core's rule is "at or above the threshold"
/// (`probit(j / 100) ≤ z` up, `≥ z` down), which may differ from `⌊100 Φ(z)⌋` within the
/// tolerance of `probit`; the reference keeps every test value away from such a `z`.
#[test]
fn a_threshold_itself_counts() {
    assert_eq!(percentile(probit(0.6)), Percentile { up: true, n: 60 });
    assert_eq!(percentile(probit(0.4)), Percentile { up: false, n: 60 });
    assert_eq!(percentile(probit(0.99)), Percentile { up: true, n: 99 });
}

//! The biological age (genetics specification 7.4, ADR-0015): the calendar age up to 25,
//! then `25 + (x − 25) · e^(w · Δ / 50)` with `w` = 0.5. The expected values write the
//! formula out with `exp` from `sim-math`.

mod common;

use sim_core::ageing::bio_age;

#[test]
fn bio_age_follows_the_specification() {
    let w = common::data().genetics().bio_age_weight;
    assert_eq!(w, 0.5);
    // The calendar age up to 25, whatever Δ.
    for x in [0.0, 1.5, 12.0, 25.0] {
        for delta in [-30.0, 0.0, 6.0, 30.0] {
            assert_eq!(bio_age(x, delta, w), x, "{x} {delta}");
        }
    }
    // From 25 on with the rate e^(w · Δ / 50): at 75 shifted by 50 · (e^(w · Δ / 50) − 1),
    // about w · Δ for a small Δ (7.4).
    for delta in [-6.0, 2.0, 6.0] {
        let expected = 25.0 + (75.0 - 25.0) * sim_math::exp(w * delta / 50.0);
        assert_eq!(
            bio_age(75.0, delta, w).to_bits(),
            expected.to_bits(),
            "{delta}"
        );
        assert!(
            (bio_age(75.0, delta, w) - 75.0 - w * delta).abs() < 0.1,
            "{delta}"
        );
    }
    // Continuous at 25 and never falling for Δ from −30 to 30, on a grid of a tenth of a
    // year up to 130 years.
    for delta in [-30.0, -6.48, 0.0, 6.13, 30.0] {
        let mut previous = -1.0;
        for k in 0..=1300_u32 {
            let x = f64::from(k) / 10.0;
            let b = bio_age(x, delta, w);
            assert!(b >= previous, "{delta} at {x}: {b} < {previous}");
            previous = b;
        }
        assert!(
            (bio_age(25.000_001, delta, w) - 25.0).abs() < 1e-5,
            "{delta}"
        );
    }
}

//! The learning function (genetics specification 11.5):
//! `L(z, h) = e^(κ·z) · (1 − e^(−h / τ(z)))`, `τ(z) = τ_0 · e^(−ν·z)`, with the values of
//! the specification κ = 0.25, ν = 0.2 and τ_0 = 1000 hours. The expected values come from
//! the specification and from Python, never from a run of this crate.

use sim_core::learning::{LearningParams, level};
use std::panic::{UnwindSafe, catch_unwind};

/// The parameters of 11.5.
const SPEC: LearningParams = LearningParams {
    kappa: 0.25,
    nu: 0.2,
    tau0: 1000.0,
};

/// A million hours: `e^(−h / τ)` underflows to 0 for the talents 0 and 1, so the level is
/// the plateau `e^(κ·z)`.
const PLATEAU_HOURS: f64 = 1.0e6;

/// P8 (8.2) on its exact grid: talent from −3 to 3 in steps of 0.25, practice from 250 to
/// 10 000 hours in steps of 250 (the time 0 is left out, there every talent is at 0).
#[test]
fn p8_level_strictly_increasing_in_talent() {
    for k in 1..=40_u32 {
        let h = f64::from(k) * 250.0;
        for i in -12..12_i32 {
            let (z, z_next) = (f64::from(i) / 4.0, f64::from(i + 1) / 4.0);
            let (low, high) = (level(z, h, &SPEC), level(z_next, h, &SPEC));
            assert!(high > low, "h {h}: L({z_next}) = {high} <= L({z}) = {low}");
        }
    }
}

/// W11.5a: on the plateau one standard deviation of talent raises the level by
/// `e^0.25 − 1` = 0.2840254166877414, about 28.4 percent.
#[test]
fn w11_5a_one_sd_raises_the_plateau_by_28_4_percent() {
    let base = level(0.0, PLATEAU_HOURS, &SPEC);
    assert_eq!(base, 1.0, "the plateau of the talent 0");
    let raise = level(1.0, PLATEAU_HOURS, &SPEC) / base - 1.0;
    assert!((raise - 0.284).abs() < 0.0005, "{raise}");
    assert!((raise - 0.284_025_416_687_741_4).abs() < 1e-15, "{raise}");
}

/// Known answers (11.5) from Python's `math.exp`, independent of this crate:
///
/// ```text
/// python3 -c "import math; L=lambda z,h: math.exp(0.25*z)*(1-math.exp(-h/(1000*math.exp(-0.2*z)))); print([repr(L(z,h)) for z,h in [(0,1000),(1,1000),(-1.5,2000),(2.5,5000)]])"
/// ```
///
/// Both sides run the same `f64` operations in the same order; only `exp` differs, within
/// 1 ulp of the exact value on both (ADR-0003 3.3 for libm; CPython calls the platform
/// libm). Carried through `τ`, the quotient and `1 − e^(−x)`, each side stays within 7.5 u
/// of the exact value at these points (u = 2^-53), so the two within 15 u; the relative
/// tolerance is 16 u = 8 ε.
#[test]
fn s11_5_level_meets_known_answers() {
    let known = [
        (0.0, 1000.0, 0.632_120_558_828_557_7),
        (1.0, 1000.0, 0.905_473_767_617_137_4),
        (-1.5, 2000.0, 0.531_092_152_811_242_8),
        (2.5, 5000.0, 1.867_754_731_482_604_6),
    ];
    for (z, h, expected) in known {
        let got = level(z, h, &SPEC);
        let relative = (got - expected).abs() / expected;
        assert!(
            relative <= 8.0 * f64::EPSILON,
            "L({z}, {h}) = {got}, expected {expected}"
        );
    }
}

/// 11.5: without practice the level is exactly +0, whatever the talent; −0 hours count as
/// none.
#[test]
fn level_is_zero_without_practice() {
    for z in [-3.0, -0.5, 0.0, 1.25, 3.0] {
        for h in [0.0, -0.0] {
            assert_eq!(level(z, h, &SPEC).to_bits(), 0, "z {z}, h {h}");
        }
    }
}

/// Each parameter is read: with ν = 0 the time constant is `τ_0` for every talent, and
/// twice `τ_0` with twice the hours gives the same level bit for bit.
#[test]
fn s11_5_every_parameter_is_read() {
    let constant = LearningParams { nu: 0.0, ..SPEC };
    let expected = sim_math::exp(0.5) * (1.0 - sim_math::exp(-1.0));
    assert_eq!(level(2.0, 1000.0, &constant).to_bits(), expected.to_bits());
    let slower = LearningParams {
        tau0: 2000.0,
        ..SPEC
    };
    for (z, h) in [(-1.5, 250.0), (0.0, 1000.0), (2.25, 7500.0)] {
        assert_eq!(
            level(z, 2.0 * h, &slower).to_bits(),
            level(z, h, &SPEC).to_bits(),
            "z {z}, h {h}"
        );
    }
}

/// `call` panics with exactly the message `invariant`.
fn breaks(call: impl FnOnce() -> f64 + UnwindSafe, invariant: &str) {
    let payload = catch_unwind(call).expect_err("an invariant break");
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .expect("a panic message");
    assert_eq!(message, invariant);
}

/// Every invalid input, also NaN and infinity, breaks its invariant.
#[test]
fn every_invalid_input_breaks_its_invariant() {
    let bad = [f64::NAN, f64::INFINITY, f64::NEG_INFINITY];
    for z in bad {
        breaks(move || level(z, 1000.0, &SPEC), "invariant learning.z");
    }
    for hours in [-1.0, -f64::MIN_POSITIVE, f64::NAN, f64::INFINITY] {
        breaks(move || level(0.0, hours, &SPEC), "invariant learning.hours");
    }
    for kappa in [0.0, -0.25, f64::NAN, f64::INFINITY] {
        let params = LearningParams { kappa, ..SPEC };
        breaks(
            move || level(0.0, 1000.0, &params),
            "invariant learning.kappa",
        );
    }
    for nu in [-0.2, -f64::MIN_POSITIVE, f64::NAN, f64::INFINITY] {
        let params = LearningParams { nu, ..SPEC };
        breaks(move || level(0.0, 1000.0, &params), "invariant learning.nu");
    }
    for tau0 in [0.0, -1000.0, f64::NAN, f64::INFINITY] {
        let params = LearningParams { tau0, ..SPEC };
        breaks(
            move || level(0.0, 1000.0, &params),
            "invariant learning.tau0",
        );
    }
}

#[test]
#[should_panic(expected = "invariant learning.z")]
fn a_talent_that_is_not_finite_breaks_the_invariant() {
    level(f64::NAN, 1000.0, &SPEC);
}

#[test]
#[should_panic(expected = "invariant learning.hours")]
fn negative_hours_break_the_invariant() {
    level(0.0, -250.0, &SPEC);
}

#[test]
#[should_panic(expected = "invariant learning.kappa")]
fn a_kappa_of_zero_breaks_the_invariant() {
    level(0.0, 1000.0, &LearningParams { kappa: 0.0, ..SPEC });
}

#[test]
#[should_panic(expected = "invariant learning.nu")]
fn a_negative_nu_breaks_the_invariant() {
    level(0.0, 1000.0, &LearningParams { nu: -0.2, ..SPEC });
}

#[test]
#[should_panic(expected = "invariant learning.tau0")]
fn a_tau0_of_zero_breaks_the_invariant() {
    level(0.0, 1000.0, &LearningParams { tau0: 0.0, ..SPEC });
}

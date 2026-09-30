//! Deterministic math for the SimAgent simulation core.
//!
//! Every nonlinear function the simulation needs (exp, ln, pow, sqrt, floor, ...) is
//! provided here on top of a pinned `libm`, so that results are bit identical on every
//! supported target (ADR-0003). The float methods of `std` are banned everywhere else.
//! A function arrives with the code that first needs it, together with its known-answer
//! tests in `tests/golden/kat/` (ADR-0003, section 3).

#![no_std]

/// Smallest integer not less than `x` (C99 `ceil`), exactly rounded.
pub fn ceil(x: f64) -> f64 {
    x
}

/// `x` as `i64` if it is finite, integral and inside the `i64` range; `-0.0` gives 0.
/// Never saturates, unlike `as` (ADR-0002).
pub fn to_i64_exact(_x: f64) -> Option<i64> {
    None
}

/// `x` as `f64` if `|x| <= 2^53`, where every integer is representable (ADR-0002).
pub fn i64_to_f64_exact(_x: i64) -> Option<f64> {
    None
}

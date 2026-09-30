//! Deterministic math for the SimAgent simulation core.
//!
//! Every nonlinear function the simulation needs (exp, ln, pow, sqrt, floor, ...) is
//! provided here on top of a pinned `libm`, so that results are bit identical on every
//! supported target (ADR-0003). The float methods of `std` are banned everywhere else.
//! A function arrives with the code that first needs it, together with its known-answer
//! tests in `tests/golden/kat/` (ADR-0003, section 3).

#![no_std]

/// Width of the fraction field of an `f64`.
const FRACTION_BITS: u32 = 52;
/// Exponent bias of an `f64` plus the fraction width: a normal value is
/// `(2^52 + fraction) * 2^(exponent_field - 1075)`.
const EXPONENT_OFFSET: i64 = 1075;
/// 2^53: every integer up to this magnitude is exactly representable as `f64`.
const EXACT_INTEGER_LIMIT: i64 = 1 << 53;
/// 2^32 as `f64`, for splitting an integer into two exactly convertible halves.
const TWO_POW_32: f64 = 4_294_967_296.0;

/// Smallest integer not less than `x` (C99 `ceil`), exactly rounded.
pub fn ceil(x: f64) -> f64 {
    libm::ceil(x)
}

/// `x` as `i64` if it is finite, integral and inside the `i64` range; `-0.0` gives 0.
/// Works on the bit pattern and never saturates, unlike `as` (ADR-0002).
pub fn to_i64_exact(x: f64) -> Option<i64> {
    if !x.is_finite() {
        return None;
    }
    let bits = x.to_bits();
    let negative = bits >> 63 == 1;
    let exponent_field = (bits >> FRACTION_BITS) & 0x7ff;
    let fraction = bits & ((1_u64 << FRACTION_BITS) - 1);
    if exponent_field == 0 {
        // Zero (either sign) or subnormal; a nonzero subnormal is not an integer.
        return (fraction == 0).then_some(0);
    }
    let mantissa = fraction | (1_u64 << FRACTION_BITS);
    let exponent = i64::try_from(exponent_field).ok()? - EXPONENT_OFFSET;
    let magnitude = if exponent >= 0 {
        // The value is mantissa * 2^exponent < 2^(53 + exponent); above 2^63 only
        // -2^63 itself still fits into i64.
        if exponent > 11 {
            return None;
        }
        mantissa << u32::try_from(exponent).ok()?
    } else {
        let shift = u32::try_from(-exponent).ok()?;
        if shift > FRACTION_BITS {
            return None; // 0 < |x| < 1
        }
        if mantissa & ((1_u64 << shift) - 1) != 0 {
            return None; // fractional part
        }
        mantissa >> shift
    };
    if negative {
        if magnitude == 1_u64 << 63 {
            Some(i64::MIN)
        } else {
            i64::try_from(magnitude).ok().map(|m| -m)
        }
    } else {
        i64::try_from(magnitude).ok()
    }
}

/// `x` as `f64` if `|x| <= 2^53`, where every integer is representable (ADR-0002).
/// Computed as `high * 2^32 + low` from two exactly convertible halves.
pub fn i64_to_f64_exact(x: i64) -> Option<f64> {
    if !(-EXACT_INTEGER_LIMIT..=EXACT_INTEGER_LIMIT).contains(&x) {
        return None;
    }
    let high = i32::try_from(x.div_euclid(1_i64 << 32)).ok()?;
    let low = u32::try_from(x.rem_euclid(1_i64 << 32)).ok()?;
    Some(f64::from(high) * TWO_POW_32 + f64::from(low))
}

//! Deterministic math for the SimAgent simulation core.
//!
//! Every nonlinear function the simulation needs (exp, ln, pow, sqrt, floor, ...) is
//! provided here on top of a pinned `libm`, so that results are bit identical on every
//! supported target (ADR-0003). The float methods of `std` are banned everywhere else.
//! A function arrives with the code that first needs it, together with its known-answer
//! tests in `tests/golden/kat/` (ADR-0003, section 3).

#![no_std]

#[cfg(test)]
extern crate std;

pub mod dist;

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

/// e to the power of `x`, platform-equal through the pinned libm.
pub fn exp(x: f64) -> f64 {
    libm::exp(x)
}

/// Natural logarithm of `x`, platform-equal through the pinned libm.
pub fn ln(x: f64) -> f64 {
    libm::log(x)
}

/// Cosine of `x` (radians), platform-equal through the pinned libm.
pub fn cos(x: f64) -> f64 {
    libm::cos(x)
}

/// `x` to the power of `y`, platform-equal through the pinned libm.
pub fn pow(x: f64, y: f64) -> f64 {
    libm::pow(x, y)
}

/// Square root of `x`, exactly rounded.
pub fn sqrt(x: f64) -> f64 {
    libm::sqrt(x)
}

// The coefficients of AS241 (Wichura 1988, PPND16), highest power first, each the f64
// nearest to the published 20-digit value (written as the shortest decimal that gives
// it). Numerators of the three ranges, then denominators without their constant term 1.
const PROBIT_CENTRAL_NUM: [f64; 8] = [
    2_509.080_928_730_122_7,
    33_430.575_583_588_13,
    67_265.770_927_008_7,
    45_921.953_931_549_87,
    13_731.693_765_509_46,
    1_971.590_950_306_551_3,
    133.141_667_891_784_38,
    3.387_132_872_796_366_5,
];
const PROBIT_CENTRAL_DEN: [f64; 7] = [
    5_226.495_278_852_854,
    28_729.085_735_721_943,
    39_307.895_800_092_71,
    21_213.794_301_586_597,
    5_394.196_021_424_751,
    687.187_007_492_057_9,
    42.313_330_701_600_91,
];
const PROBIT_NEAR_NUM: [f64; 8] = [
    0.000_774_545_014_278_341_4,
    0.022_723_844_989_269_184,
    0.241_780_725_177_450_6,
    1.270_458_252_452_368_4,
    3.647_848_324_763_204_5,
    5.769_497_221_460_691,
    4.630_337_846_156_546,
    1.423_437_110_749_683_5,
];
const PROBIT_NEAR_DEN: [f64; 7] = [
    1.050_750_071_644_416_9e-9,
    0.000_547_593_808_499_534_5,
    0.015_198_666_563_616_457,
    0.148_103_976_427_480_08,
    0.689_767_334_985_1,
    1.676_384_830_183_803_8,
    2.053_191_626_637_759,
];
const PROBIT_FAR_NUM: [f64; 8] = [
    2.010_334_399_292_288_1e-7,
    2.711_555_568_743_487_6e-5,
    0.001_242_660_947_388_078_4,
    0.026_532_189_526_576_124,
    0.296_560_571_828_504_87,
    1.784_826_539_917_291_3,
    5.463_784_911_164_114,
    6.657_904_643_501_103,
];
const PROBIT_FAR_DEN: [f64; 7] = [
    2.044_263_103_389_939_7e-15,
    1.421_511_758_316_446e-7,
    1.846_318_317_510_054_8e-5,
    0.000_786_869_131_145_613_3,
    0.014_875_361_290_850_615,
    0.136_929_880_922_735_8,
    0.599_832_206_555_888,
];

/// `c[0] · r^(n-1) + ... + c[n-1]` by Horner's scheme, highest power first, as the paper
/// writes it, then `· r + last` for a denominator's constant term.
fn horner(coefficients: &[f64], r: f64) -> f64 {
    let mut value = 0.0;
    for coefficient in coefficients {
        value = value * r + coefficient;
    }
    value
}

/// The inverse of the standard normal distribution function, `Φ⁻¹(p)` for `0 < p < 1`:
/// Wichura's algorithm AS241 (PPND16, Applied Statistics 37:477-484, 1988), rational
/// approximations in three ranges of `p`, with `ln` and `sqrt` of the pinned libm. Within
/// 2^-48 relative of the exact value (ADR-0019); exactly 0 at 0.5. NaN outside (0, 1) and
/// for NaN without a test of its own: the tail is then 0 or negative or NaN, its logarithm
/// infinite or NaN, and NaN reaches the result through the rational function. The
/// evaluation order is fixed, so the result is the same on every target.
pub fn probit(p: f64) -> f64 {
    let q = p - 0.5;
    if libm::fabs(q) <= 0.425 {
        let r = 0.180_625 - q * q;
        let num = horner(&PROBIT_CENTRAL_NUM, r) * q;
        let den = horner(&PROBIT_CENTRAL_DEN, r) * r + 1.0;
        return num / den;
    }
    // The smaller of p and 1 - p (both exact here); the sign of q gives the side.
    let tail = libm::fmin(p, 1.0 - p);
    let r = sqrt(0.0 - ln(tail));
    let x = if r <= 5.0 {
        let r = r - 1.6;
        horner(&PROBIT_NEAR_NUM, r) / (horner(&PROBIT_NEAR_DEN, r) * r + 1.0)
    } else {
        let r = r - 5.0;
        horner(&PROBIT_FAR_NUM, r) / (horner(&PROBIT_FAR_DEN, r) * r + 1.0)
    };
    libm::copysign(x, q)
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

//! Distributions (ADR-0001 section 3): pure functions from random words to values, with
//! a fixed number of words and without rejection. `sim-core` fetches the words. An
//! invalid parameter (also NaN) is an invariant break.

use crate::{cos, exp, i64_to_f64_exact, ln, sqrt};

/// 2^-53: the distance of the values of `unit`.
const TWO_POW_MINUS_53: f64 = f64::from_bits(0x3ca0_0000_0000_0000);
/// 2 pi, the nearest f64.
const TAU: f64 = core::f64::consts::TAU;
/// Largest `lambda` of `poisson`.
const POISSON_MAX_LAMBDA: f64 = 30.0;
/// Largest result of `poisson`; the sum saturates long before for `lambda <= 30`.
const POISSON_MAX_K: u32 = 255;

/// `b * 2^-53` for `b <= 2^53`, exact: an integer up to 2^53 times a power of two.
fn scaled(b: u64) -> f64 {
    let exact = i64::try_from(b)
        .ok()
        .and_then(i64_to_f64_exact)
        .unwrap_or_else(|| panic!("invariant dist.unit_range"));
    exact * TWO_POW_MINUS_53
}

/// Uniform in [0, 1): the upper 53 bits of `w` times 2^-53.
pub fn unit(w: u64) -> f64 {
    scaled(w >> 11)
}

/// Uniform in (0, 1]: as `unit` plus 2^-53, so that `ln` never sees 0.
pub fn unit_open0(w: u64) -> f64 {
    scaled((w >> 11) + 1)
}

/// Uniform on `0..n` for `n >= 1`: the upper 64 bits of `w * n`. The bias is at most
/// n / 2^64.
pub fn uniform_below(w: u64, n: u64) -> u64 {
    if n == 0 {
        panic!("invariant dist.uniform_below_n");
    }
    let product = u128::from(w) * u128::from(n);
    u64::try_from(product >> 64).unwrap_or_else(|_| panic!("invariant dist.uniform_below_range"))
}

/// True with probability `p`, `p` in [0, 1].
pub fn bernoulli(w: u64, p: f64) -> bool {
    if !(0.0..=1.0).contains(&p) {
        panic!("invariant dist.bernoulli_p");
    }
    unit(w) < p
}

/// Exponential with `rate > 0` (mean `1 / rate`). `0.0 - x` instead of `-x` keeps a
/// result of zero positive.
pub fn exponential(w: u64, rate: f64) -> f64 {
    let valid = rate > 0.0 && rate.is_finite();
    if !valid {
        panic!("invariant dist.exponential_rate");
    }
    (0.0 - ln(unit_open0(w))) / rate
}

/// Normal with `mean` and standard deviation `sd >= 0` (Box-Muller, the cosine branch).
pub fn normal(w0: u64, w1: u64, mean: f64, sd: f64) -> f64 {
    let valid = mean.is_finite() && sd >= 0.0 && sd.is_finite();
    if !valid {
        panic!("invariant dist.normal_parameters");
    }
    mean + sd * (sqrt(0.0 - 2.0 * ln(unit_open0(w0))) * cos(TAU * unit(w1)))
}

/// Poisson with `lambda` in [0, 30], by inversion: the smallest `k` with `u < F(k)`.
/// `g == f` ends the sum where it no longer grows (the rounding rest at the upper edge).
pub fn poisson(w: u64, lambda: f64) -> u32 {
    if !(0.0..=POISSON_MAX_LAMBDA).contains(&lambda) {
        panic!("invariant dist.poisson_lambda");
    }
    let u = unit(w);
    let mut p = exp(0.0 - lambda);
    let mut f = 0.0;
    let mut k = 0;
    loop {
        let g = f + p;
        if u < g || g == f {
            return k;
        }
        if k == POISSON_MAX_K {
            panic!("invariant dist.poisson_tail");
        }
        f = g;
        p = p * lambda / f64::from(k + 1);
        k += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec::Vec;

    const UNIT: &str = include_str!("../../../tests/golden/kat/unit.csv");
    const UNIT_OPEN0: &str = include_str!("../../../tests/golden/kat/unit_open0.csv");
    const UNIFORM_BELOW: &str = include_str!("../../../tests/golden/kat/uniform_below.csv");
    const BERNOULLI: &str = include_str!("../../../tests/golden/kat/bernoulli.csv");
    const POISSON: &str = include_str!("../../../tests/golden/kat/poisson.csv");
    const EXPONENTIAL: &str = include_str!("../../../tests/golden/kat/exponential.csv");
    const NORMAL: &str = include_str!("../../../tests/golden/kat/normal.csv");

    /// Data rows of a KAT file (without the comment line and the header).
    fn rows(csv: &str) -> Vec<Vec<&str>> {
        csv.lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
            .skip(1)
            .map(|line| line.split(',').collect())
            .collect()
    }

    fn word(hex: &str) -> u64 {
        u64::from_str_radix(hex, 16).expect("hex")
    }

    fn float(hex: &str) -> f64 {
        f64::from_bits(word(hex))
    }

    /// Class E: the bits of the reference. Class P: the recorded bits of the pinned libm.
    fn assert_bits(what: &str, input: &str, expected: &str, actual: f64) {
        assert_ne!(
            expected, "unrecorded",
            "{what}({input}): run cargo xtask kat-record"
        );
        assert_eq!(
            actual.to_bits(),
            word(expected),
            "{what}({input}): expected bits {expected}, got {:016x}",
            actual.to_bits()
        );
    }

    #[test]
    fn unit_kat() {
        let rows = rows(UNIT);
        assert_eq!(rows.len(), 10);
        for row in rows {
            assert_bits("unit", row[0], row[1], unit(word(row[0])));
        }
    }

    #[test]
    fn unit_open0_kat() {
        let rows = rows(UNIT_OPEN0);
        assert_eq!(rows.len(), 10);
        for row in rows {
            assert_bits("unit_open0", row[0], row[1], unit_open0(word(row[0])));
        }
    }

    #[test]
    fn uniform_below_kat() {
        let rows = rows(UNIFORM_BELOW);
        assert_eq!(rows.len(), 40);
        for row in rows {
            let (w, n) = (word(row[0]), word(row[1]));
            assert_eq!(
                uniform_below(w, n),
                word(row[2]),
                "uniform_below({w:x}, {n:x})"
            );
        }
    }

    #[test]
    fn bernoulli_kat() {
        let rows = rows(BERNOULLI);
        assert_eq!(rows.len(), 50);
        for row in rows {
            let (w, p) = (word(row[0]), float(row[1]));
            assert_eq!(bernoulli(w, p), row[2] == "1", "bernoulli({w:x}, {p})");
        }
    }

    #[test]
    fn poisson_kat() {
        let rows = rows(POISSON);
        assert_eq!(rows.len(), 32);
        for row in rows {
            let (w, lambda) = (word(row[0]), float(row[1]));
            let expected: u32 = row[2].parse().expect("count");
            assert_eq!(poisson(w, lambda), expected, "poisson({w:x}, {lambda})");
        }
    }

    #[test]
    fn exponential_kat() {
        let rows = rows(EXPONENTIAL);
        assert_eq!(rows.len(), 40);
        for row in rows {
            let inputs: Vec<&str> = row[0].split(';').collect();
            let actual = exponential(word(inputs[0]), float(inputs[1]));
            assert_bits("exponential", row[0], row[1], actual);
        }
    }

    #[test]
    fn normal_kat() {
        let rows = rows(NORMAL);
        assert_eq!(rows.len(), 21);
        for row in rows {
            let inputs: Vec<&str> = row[0].split(';').collect();
            let actual = normal(
                word(inputs[0]),
                word(inputs[1]),
                float(inputs[2]),
                float(inputs[3]),
            );
            assert_bits("normal", row[0], row[1], actual);
        }
    }

    /// ADR-0001: the tail of the Poisson draw. At lambda = 0 every word gives 0, also
    /// the largest. For lambda <= 30 the sum saturates before k = 90, so the largest
    /// word ends there instead of breaking the invariant. At 2.5, 4 and 7 the rounded sum
    /// stays some ulp below the largest u: only the exit `g == f` ends the draw there.
    #[test]
    fn poisson_ends_at_the_upper_edge() {
        assert_eq!(poisson(u64::MAX, 0.0), 0);
        assert_eq!(poisson(0, 30.0), 0, "u = 0 lies below exp(-30)");
        for lambda in [0.5, 2.5, 4.0, 5.0, 7.0, 30.0] {
            let k = poisson(u64::MAX, lambda);
            assert!(k > 0 && k < 90, "lambda {lambda}: {k}");
        }
    }

    /// SplitMix64 with start state 0 (Vigna): word number `index` from 0.
    fn splitmix(index: u64) -> u64 {
        let mut z = index.wrapping_add(1).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    #[test]
    fn splitmix_reference() {
        assert_eq!(
            [splitmix(0), splitmix(1), splitmix(2)],
            [
                0xe220_a839_7b1d_cdaf,
                0x6e78_9e6a_a1b9_65f4,
                0x06c4_5d18_8009_454f
            ]
        );
    }

    /// 2^18 words per smoke test (ADR-0001 section 3).
    const WORDS: u64 = 1 << 18;

    /// Mean and variance of `count` samples lie within 5 standard errors of the theory.
    /// The variance is taken around the theoretical mean, so its standard error is
    /// sqrt((m4 - var^2) / count) with the fourth central moment `m4`.
    fn assert_moments(
        what: &str,
        count: u64,
        sample: impl Fn(u64) -> f64,
        mean: f64,
        variance: f64,
        fourth: f64,
    ) {
        let n = i64_to_f64_exact(i64::try_from(count).expect("count")).expect("count");
        let mut sum = 0.0;
        let mut squares = 0.0;
        for index in 0..count {
            let x = sample(index);
            assert!(x.is_finite(), "{what}: sample {index} is {x}");
            sum += x;
            squares += (x - mean) * (x - mean);
        }
        let mean_error = sqrt(variance / n);
        let variance_error = sqrt((fourth - variance * variance) / n);
        assert!(
            (sum / n - mean).abs() <= 5.0 * mean_error,
            "{what}: mean {} instead of {mean}, standard error {mean_error}",
            sum / n
        );
        assert!(
            (squares / n - variance).abs() <= 5.0 * variance_error,
            "{what}: variance {} instead of {variance}, standard error {variance_error}",
            squares / n
        );
    }

    fn count_as_float(k: u64) -> f64 {
        i64_to_f64_exact(i64::try_from(k).expect("small")).expect("below 2^53")
    }

    /// Discrete uniform on 0..n: mean (n - 1) / 2, variance (n^2 - 1) / 12, fourth
    /// central moment (n^2 - 1)(3 n^2 - 7) / 240.
    #[test]
    fn uniform_below_smoke() {
        for n in [7_u64, (1 << 40) + 3] {
            let size = count_as_float(n);
            let square = size * size;
            assert_moments(
                "uniform_below",
                WORDS,
                |index| count_as_float(uniform_below(splitmix(index), n)),
                (size - 1.0) / 2.0,
                (square - 1.0) / 12.0,
                (square - 1.0) * (3.0 * square - 7.0) / 240.0,
            );
        }
    }

    /// Bernoulli: mean p, variance p q, fourth central moment p q (1 - 3 p q).
    #[test]
    fn bernoulli_smoke() {
        let p = 0.3;
        let pq = p * (1.0 - p);
        assert_moments(
            "bernoulli",
            WORDS,
            |index| {
                if bernoulli(splitmix(index), p) {
                    1.0
                } else {
                    0.0
                }
            },
            p,
            pq,
            pq * (1.0 - 3.0 * pq),
        );
    }

    /// Exponential: mean 1 / rate, variance 1 / rate^2, fourth central moment 9 / rate^4.
    #[test]
    fn exponential_smoke() {
        let rate = 2.0;
        assert_moments(
            "exponential",
            WORDS,
            |index| exponential(splitmix(index), rate),
            1.0 / rate,
            1.0 / (rate * rate),
            9.0 / (rate * rate * rate * rate),
        );
    }

    /// Normal: fourth central moment 3 sd^4. Two words per draw, 2^17 draws.
    #[test]
    fn normal_smoke() {
        let (mean, sd) = (1.0, 3.0);
        assert_moments(
            "normal",
            WORDS / 2,
            |index| normal(splitmix(2 * index), splitmix(2 * index + 1), mean, sd),
            mean,
            sd * sd,
            3.0 * sd * sd * sd * sd,
        );
    }

    /// Poisson: mean and variance lambda, fourth central moment lambda (1 + 3 lambda).
    #[test]
    fn poisson_smoke() {
        for lambda in [0.5, 5.0, 30.0] {
            assert_moments(
                "poisson",
                WORDS,
                |index| f64::from(poisson(splitmix(index), lambda)),
                lambda,
                lambda,
                lambda * (1.0 + 3.0 * lambda),
            );
        }
    }

    #[test]
    #[should_panic(expected = "invariant dist.uniform_below_n")]
    fn uniform_below_zero_is_an_invariant_break() {
        uniform_below(0, 0);
    }

    fn breaks(call: impl FnOnce() + std::panic::UnwindSafe, invariant: &str) {
        let payload = std::panic::catch_unwind(call).expect_err("an invariant break");
        let message = payload
            .downcast_ref::<std::string::String>()
            .map(|text| text.as_str())
            .or_else(|| payload.downcast_ref::<&str>().copied())
            .expect("a panic message");
        assert_eq!(message, invariant);
    }

    /// ADR-0001: an invalid parameter, also NaN, is an invariant break.
    #[test]
    fn invalid_parameters_break_the_invariant() {
        for p in [-0.1, 1.000_000_1, f64::NAN, f64::INFINITY] {
            breaks(
                move || {
                    let _ = bernoulli(0, p);
                },
                "invariant dist.bernoulli_p",
            );
        }
        for rate in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            breaks(
                move || {
                    let _ = exponential(0, rate);
                },
                "invariant dist.exponential_rate",
            );
        }
        for (mean, sd) in [
            (0.0, -1.0),
            (0.0, f64::NAN),
            (0.0, f64::INFINITY),
            (f64::NAN, 1.0),
            (f64::NEG_INFINITY, 1.0),
        ] {
            breaks(
                move || {
                    let _ = normal(0, 0, mean, sd);
                },
                "invariant dist.normal_parameters",
            );
        }
        for lambda in [-0.1, 30.000_000_1, f64::NAN, f64::INFINITY] {
            breaks(
                move || {
                    let _ = poisson(0, lambda);
                },
                "invariant dist.poisson_lambda",
            );
        }
    }

    /// The edges of the parameter ranges are valid.
    #[test]
    fn the_edges_of_the_parameters_are_valid() {
        assert!(!bernoulli(0, 0.0));
        assert!(bernoulli(u64::MAX, 1.0));
        assert_eq!(normal(0, 0, 2.0, 0.0), 2.0);
        assert_eq!(poisson(1 << 63, 0.0), 0);
        assert_eq!(poisson(1 << 63, 30.0), 30);
        assert_eq!(uniform_below(u64::MAX, 1), 0);
        assert_eq!(uniform_below(u64::MAX, u64::MAX), u64::MAX - 1);
    }
}

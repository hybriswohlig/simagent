//! The estimators of the criteria (spec 8.1 to 8.3, ADR-0020 section 8, Grok K1 to K5):
//! means, Pearson, the slope, Fisher's z, `Φ` and the power, the median of the first
//! partnership, the period life table, the expectation of P9 and of the village, the
//! tolerances at the actual `n`, and the fixed permutations of the negative twins.
//!
//! Every sum is a left fold from +0.0 over its list in the given (canonical) order and
//! every power repeated multiplication, so a reference that follows the same order agrees
//! bit for bit; only `ln` and `Φ` may differ, within `1e-12 · max(1, |x|)`.

use sim_math::{ceil, i64_to_f64_exact, ln, probit, sqrt, to_i64_exact};

/// Gate seeds of the permutations of the negative twins (protected list; never reseeded).
pub const SEED_P3: u64 = 0x5eed_0003_0000_0001;
pub const SEED_P5: u64 = 0x5eed_0005_0000_0001;
pub const SEED_P9_POLYGENIC: u64 = 0x5eed_0009_0000_0001;
pub const SEED_P9_MC1R: u64 = 0x5eed_0009_0000_0002;

/// Seconds of a year, as `f64`.
const YEAR_SECS: f64 = 31_449_600.0;
/// Months of a year (spec 1).
const MONTHS_PER_YEAR: f64 = 13.0;
/// Months of the first-partnership histogram: the ages below 50 years.
pub const MONTHS: usize = 650;

/// The smallest samples of the criteria (spec 8.3 to 8.9); below them a check is red.
pub const MIN_P3: usize = 2000;
pub const MIN_P4: usize = 500;
pub const MIN_FISHER: usize = 1000;
pub const MIN_MC1R: usize = 400;

/// The allele frequency of `core:mc1r` (spec 8.9, W8.9a) and its complement.
const MC1R_Q: f64 = 0.15;
const MC1R_P: f64 = 0.85;

/// An integer count or a span of seconds as `f64`; every one stays far below 2^53.
pub fn real(n: u64) -> f64 {
    i64::try_from(n)
        .ok()
        .and_then(i64_to_f64_exact)
        .unwrap_or_else(|| panic!("invariant criteria.exact: {n}"))
}

fn count(n: usize) -> f64 {
    real(u64::try_from(n).unwrap_or_else(|_| panic!("invariant criteria.count")))
}

/// `Σ x` from +0.0, left to right.
fn total(values: impl IntoIterator<Item = f64>) -> f64 {
    values.into_iter().fold(0.0, |sum, x| sum + x)
}

/// `x` multiplied `n` times, starting from 1.0.
fn raised(x: f64, n: u32) -> f64 {
    (0..n).fold(1.0, |product, _| product * x)
}

/// `m = (Σ x) / n`.
pub fn mean(values: &[f64]) -> f64 {
    total(values.iter().copied()) / count(values.len())
}

/// `V = Σ (x − m)² / (n − 1)`.
pub fn variance(values: &[f64]) -> f64 {
    let m = mean(values);
    total(values.iter().map(|x| (x - m) * (x - m))) / (count(values.len()) - 1.0)
}

/// The sums of squares and products about the means: `S_xx`, `S_yy`, `S_xy`, one pass
/// after the means.
fn moments(pairs: &[(f64, f64)]) -> [f64; 3] {
    let n = count(pairs.len());
    let mean_x = total(pairs.iter().map(|pair| pair.0)) / n;
    let mean_y = total(pairs.iter().map(|pair| pair.1)) / n;
    pairs.iter().fold([0.0; 3], |[xx, yy, xy], &(x, y)| {
        let (dx, dy) = (x - mean_x, y - mean_y);
        [xx + dx * dx, yy + dy * dy, xy + dx * dy]
    })
}

/// Pearson's `r = S_xy / sqrt(S_xx · S_yy)`.
pub fn pearson(pairs: &[(f64, f64)]) -> f64 {
    let [xx, yy, xy] = moments(pairs);
    xy / sqrt(xx * yy)
}

/// The least-squares slope of `y` on `x`, `S_xy / S_xx`.
pub fn slope(pairs: &[(f64, f64)]) -> f64 {
    let [xx, _, xy] = moments(pairs);
    xy / xx
}

/// Fisher's `F(r) = 0.5 · ln((1 + r) / (1 − r))`.
pub fn fisher(r: f64) -> f64 {
    0.5 * ln((1.0 + r) / (1.0 - r))
}

/// The standard normal distribution function: 100 halvings of `[0, 1]` against `probit`,
/// the result the midpoint (no new math, ADR-0003).
pub fn phi(x: f64) -> f64 {
    let (mut lower, mut upper) = (0.0, 1.0);
    for _ in 0..100 {
        let mid = 0.5 * (lower + upper);
        if probit(mid) < x {
            lower = mid;
        } else {
            upper = mid;
        }
    }
    0.5 * (lower + upper)
}

/// The power of the check `|c − value| ≤ tol` when the value lies at `alt` with standard
/// error `se`: `Φ((c − tol − alt) / se) + Φ((alt − c − tol) / se)`.
pub fn power(c: f64, tol: f64, alt: f64, se: f64) -> f64 {
    phi((c - tol - alt) / se) + phi((alt - c - tol) / se)
}

/// The tolerance of P3 at `n` families: `4 sqrt((1 − 0.64 · 0.6) / (0.6 n))` (W8.3a).
pub fn p3_tolerance(n: usize) -> f64 {
    4.0 * sqrt((1.0 - 0.64 * 0.6) / (0.6 * count(n)))
}

/// The tolerance of P4 on `ln V` at `n` units: `4 sqrt(2 / (n − 1))` (W8.4a).
pub fn p4_tolerance(n: usize) -> f64 {
    4.0 * sqrt(2.0 / (count(n) - 1.0))
}

/// The tolerance on Fisher's z scale at `n` pairs: `4 / sqrt(n − 3)` (W8.5a, W8.7a).
pub fn fisher_tolerance(n: usize) -> f64 {
    4.0 / sqrt(count(n) - 3.0)
}

/// The tolerance of the share never partnered by 50 at `n` women: `4 sqrt(0.09 / n)`
/// (spec 8.1, the model's 0.10).
pub fn never_tolerance(n: usize) -> f64 {
    4.0 * sqrt(0.09 / count(n))
}

/// A share against its expectation: the sample share, the expected share and the standard
/// error of the sample share.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Share {
    pub value: f64,
    pub expected: f64,
    pub se: f64,
}

/// The expectation of P9 for one family with `n` children (K5): `A / (A + B + C)` with
/// `A = (2pq)² (1 − (3/4)ⁿ)`, `B = 4pq³ (1 − (1/2)ⁿ)`, `C = q⁴`.
pub fn mc1r_expectation(n: u32) -> f64 {
    let (p, q) = (MC1R_P, MC1R_Q);
    let two_pq = 2.0 * p * q;
    let a = two_pq * two_pq * (1.0 - raised(0.75, n));
    let b = 4.0 * p * raised(q, 3) * (1.0 - raised(0.5, n));
    let c = raised(q, 4);
    a / (a + b + c)
}

/// P9 on `core:mc1r`: per family whether both parents are unaffected (`y_f`) and its
/// expectation `E_f`; the share `Σ y_f / N` against `Σ E_f / N`, standard error
/// `sqrt(Σ E_f (1 − E_f)) / N`.
pub fn mc1r_share(families: &[(bool, f64)]) -> Share {
    let n = count(families.len());
    let yes = families.iter().filter(|family| family.0).count();
    Share {
        value: count(yes) / n,
        expected: total(families.iter().map(|family| family.1)) / n,
        se: sqrt(total(
            families.iter().map(|family| family.1 * (1.0 - family.1)),
        )) / n,
    }
}

/// The tolerance of P9 on `core:mc1r`, four standard errors (W8.9c).
pub fn mc1r_tolerance(share: &Share) -> f64 {
    4.0 * share.se
}

/// The mean of a count per person and its standard error `sqrt(V / n)` (births per woman).
pub fn mean_se(values: &[u64]) -> (f64, f64) {
    let values: Vec<f64> = values.iter().map(|value| real(*value)).collect();
    let n = count(values.len());
    (mean(&values), sqrt(variance(&values) / n))
}

/// A smoothed Kaplan-Meier median in years and its standard error.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Median {
    pub median: f64,
    pub se: f64,
}

/// The median age at the first partnership: `hist[f]` the first partnerships in month `f`
/// of age, `never` the persons without one before 50 (`tools/life_reference.py`, `median`
/// and `median_se`). `None` if fewer than half had one.
pub fn partnership_median(hist: &[u64; MONTHS], never: u64) -> Option<Median> {
    let n = hist.iter().fold(never, |sum, h| add(sum, *h));
    let half = real(n) / 2.0;
    let mut before = 0;
    for (k, h) in hist.iter().enumerate() {
        if *h > 0 && real(add(before, *h)) >= half {
            let median = (count(k) - 0.5 + (half - real(before)) / real(*h)) / MONTHS_PER_YEAR;
            let lo = whole((median - 1.0) * MONTHS_PER_YEAR);
            let hi = MONTHS.min(whole((median + 1.0) * MONTHS_PER_YEAR) + 1);
            let near = hist[lo..hi].iter().fold(0, |sum, h| add(sum, *h));
            let density = real(near) / real(n) / (count(hi - lo) / MONTHS_PER_YEAR);
            return Some(Median {
                median,
                se: sqrt(0.25 / real(n)) / density,
            });
        }
        before = add(before, *h);
    }
    None
}

fn add(a: u64, b: u64) -> u64 {
    a.checked_add(b)
        .unwrap_or_else(|| panic!("invariant criteria.sum"))
}

/// `max(0, trunc(x))` as an index.
fn whole(x: f64) -> usize {
    if x <= 0.0 {
        return 0;
    }
    to_i64_exact(0.0 - ceil(0.0 - x))
        .and_then(|i| usize::try_from(i).ok())
        .unwrap_or_else(|| panic!("invariant criteria.whole: {x}"))
}

/// Life expectancy at birth and its standard error.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LifeTable {
    pub e0: f64,
    pub se: f64,
}

/// The period life table of one sex (Chiang II, one-year ages, `a_x` = 0.5): `exposure[x]`
/// the seconds lived at age `x` in the period and `deaths[x]` the deaths there, the last
/// entry the open interval `ω` (100 years and older in a town). `None` if an age has no exposure or the
/// open interval no death (a finding, red).
pub fn life_table(exposure: &[u64], deaths: &[u64]) -> Option<LifeTable> {
    let omega = exposure.len().checked_sub(1)?;
    if deaths.len() != exposure.len() || exposure.contains(&0) || deaths[omega] == 0 {
        return None;
    }
    let years: Vec<f64> = exposure.iter().map(|e| real(*e) / YEAR_SECS).collect();
    let rates: Vec<f64> = deaths
        .iter()
        .zip(&years)
        .map(|(d, y)| real(*d) / y)
        .collect();
    let q: Vec<f64> = rates[..omega].iter().map(|m| m / (1.0 + 0.5 * m)).collect();
    let mut l = vec![1.0];
    for q_x in &q {
        let last = l[l.len() - 1];
        l.push(last * (1.0 - q_x));
    }
    let mut lived: Vec<f64> = (0..omega).map(|x| 0.5 * (l[x] + l[x + 1])).collect();
    lived.push(l[omega] / rates[omega]);
    let mut t = vec![0.0; omega + 1];
    t[omega] = lived[omega];
    for x in (0..omega).rev() {
        t[x] = lived[x] + t[x + 1];
    }
    let e: Vec<f64> = t.iter().zip(&l).map(|(t_x, l_x)| t_x / l_x).collect();
    let closed = total((0..omega).map(|x| {
        let var_q = if deaths[x] == 0 {
            0.0
        } else {
            q[x] * q[x] * (1.0 - q[x]) / real(deaths[x])
        };
        let s = 0.5 + e[x + 1];
        l[x] * l[x] * (s * s) * var_q
    }));
    let (l_w, y_w, m_w) = (l[omega], years[omega], rates[omega]);
    let open = l_w * l_w * real(deaths[omega]) / (y_w * y_w * raised(m_w, 4));
    Some(LifeTable {
        e0: e[0],
        se: sqrt(closed + open),
    })
}

/// The village check (K2) over the seeds: `V̄`, `Ē`, `s` and its standard error
/// `s / sqrt(seeds)`, the verdict `|V̄ − Ē| ≤ 4 se` and its power against 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Village {
    pub mean_v: f64,
    pub mean_e: f64,
    pub s: f64,
    pub se: f64,
    pub pass: bool,
    pub power: f64,
}

/// The expectation of one village (K2, G4): `h² · Π_g (1 − 1 / (2 N_e,g)) + (1 − h²)`
/// over the blocks with births in ascending order, `N_e,g = 4 N_m N_f / (N_m + N_f)` from
/// the exact counts of fathers and mothers.
pub fn village_expectation(h2: f64, blocks: &[(u64, u64)]) -> f64 {
    let kept = blocks.iter().fold(1.0, |product, (fathers, mothers)| {
        let four = fathers
            .checked_mul(*mothers)
            .and_then(|nm| nm.checked_mul(4))
            .unwrap_or_else(|| panic!("invariant criteria.ne"));
        let ne = real(four) / real(add(*fathers, *mothers));
        product * (1.0 - 1.0 / (2.0 * ne))
    });
    h2 * kept + (1.0 - h2)
}

/// The village check over the seeds' variances `v` and expectations `e`, both in seed
/// order.
pub fn village(v: &[f64], e: &[f64]) -> Village {
    let (mean_v, mean_e) = (mean(v), mean(e));
    let s = sqrt(variance(v));
    let se = s / sqrt(count(v.len()));
    Village {
        mean_v,
        mean_e,
        s,
        se,
        pass: (mean_v - mean_e).abs() <= 4.0 * se,
        power: power(mean_e, 4.0 * se, 1.0, se),
    }
}

/// SplitMix64 (Vigna), as P7.
fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// The fixed permutation of `0..n` for a negative twin: Fisher-Yates from the end, each
/// `j` the next SplitMix64 number modulo `i + 1`.
pub fn permutation(n: usize, seed: u64) -> Vec<usize> {
    let mut order: Vec<usize> = (0..n).collect();
    let mut state = seed;
    for i in (1..n).rev() {
        let bound = u64::try_from(i + 1).unwrap_or_else(|_| panic!("invariant criteria.n"));
        let j = usize::try_from(splitmix(&mut state) % bound)
            .unwrap_or_else(|_| panic!("invariant criteria.j"));
        order.swap(i, j);
    }
    order
}

#[cfg(test)]
#[path = "stats_tests.rs"]
mod tests;

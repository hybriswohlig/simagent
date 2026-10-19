#!/usr/bin/env python3
"""The criteria kit (step 11, plan steps 3 and 4; design note of the criteria kit, draft 1),
the known answers, written from the note, Grok's decisions (K1 to K5 of 2026-10-15, GK-1 to
GK-4 of 2026-10-16, all (a)), the genetics specification (4.2 to 4.5, 7.3, 8.1, 8.2, 11.2,
11.3) and ADR-0020 (sections 4, 5, 8), independent of the Rust code. The world model is
tools/world_reference.py with tools/arrival_reference.py (pools, arrivals) and the seeds of
tools/census_reference.py; tools/life_reference.py has the life course estimators.

What it covers (the note's sections; NOTES.md of the output says every choice):

- The export (note section 1): `export_world` builds the rows of a `world_reference` world
  from what a reader of the public views sees (`World::pedigree()`, the phenotype view, the
  genome, `Data::pool()`, the log), `write_export` writes the NDJSON, `read_export` reads it
  back strictly (the order of the keys, the order of the lines, the counts).
- The numeric rules (section 2): every sum is an explicit left fold from +0.0 in the unit
  order (`fold`, never `sum()` over floats or `math.fsum`), powers by repeated
  multiplication from 1.0 (`power_of`), unit lists hashed as 8-byte little-endian words
  (`unit_hash`).
- The estimators (section 3): mean, variance, Pearson, OLS slope, Fisher's `F`, shares,
  `Φ` by 100 halvings of the probit (`phi`, AS241 as `sim_math::probit`, ADR-0019; the
  `erfc` form is only a cross-check), the power, the Kaplan-Meier median of the first
  partnership with its SE (as life_reference `median` smoothed and `median_se`), the period
  life table (Chiang II, one-year ages, `a_x` = 0.5), births per woman, the P9 expectation per
  family (K5), the village expectation and aggregation (K2).
- The units (section 4): families and first-borns (K1), P3, P5 siblings and strangers, P9
  polygenic and MC1R with `q` (GK-3) and `n` (GK-4), P4 city (G5) and the village block (G5,
  G4), the F11 samples (K3, GK-1, GK-2).
- The fixed permutations (section 5) and the tolerances at the actual `n` (section 6), with
  the asserts of the W values at the minimum sizes (`assert_w_values`).
- The fixtures (section 7): the estimator vectors, the hand pedigree with its hand-derived
  units, and the two small pool towns end to end (T1 `pool_d`, T2 `pool_c`).

Usage: $HOME/.cache/simagent-tools-venv/bin/python tools/criteria_reference.py fixtures [--out DIR]
       (default DIR ~/.cache/simagent-notes/ref-criteria; writes every fixture)
       ... criteria_reference.py export t1|t2 OUT.ndjson   (one small town's export)
       ... criteria_reference.py report EXPORT [OUT.json]  (the report of an export)
       ... criteria_reference.py compare A B               (two exports or two reports)
"""

import json
import math
import struct
import sys
from pathlib import Path

import arrival_reference as ar
import pool_reference as pr
import world_reference as wr
from kat_reference import ln_reference
from world_reference import MONTH, YEAR, hash64

DEFAULT_OUT = Path.home() / ".cache" / "simagent-notes" / "ref-criteria"
TOOLS = Path(__file__).resolve().parent
ROOT = TOOLS.parent

YEAR_F = 31449600.0
assert float(YEAR) == YEAR_F and YEAR == 13 * MONTH

# ---------------------------------------------------------------------------------------
# Small helpers


def bits(value: float) -> str:
    """The f64 bits as 16 lowercase hex digits."""
    return "%016x" % struct.unpack("<Q", struct.pack("<d", value))[0]


def from_bits(text: str) -> float:
    assert len(text) == 16 and text == text.lower()
    return struct.unpack("<d", struct.pack("<Q", int(text, 16)))[0]


def hx(value: int) -> str:
    assert 0 <= value < 1 << 64
    return f"{value:016x}"


def hx_opt(value):
    return None if value is None else hx(value)


def fbits(value):
    return None if value is None else bits(value)


def unit_hash(keys: list) -> str:
    """`stable_hash64` (xxh3-64, seed 0) over the anchor keys as 8-byte little-endian words in
    list order (note section 2); a pair contributes both keys, first then second."""
    data = b"".join(struct.pack("<Q", key) for key in keys)
    return hx(hash64(data))


def flat(pairs: list) -> list:
    return [key for pair in pairs for key in pair]


# ---------------------------------------------------------------------------------------
# Section 2 and 3: the numeric rules and the estimators


EXACT_LIMIT = 2**53


def exact(n: int) -> float:
    """An integer count or span of seconds as f64 (review round 4, note section 10): every
    such value an estimator reads is at most 2^53 in magnitude, so the conversion is exact;
    above it both sides stop (Rust's invariant `criteria.exact`, here an assertion), never a
    finding. The magnitude, because a span (the age gap) may be negative."""
    assert isinstance(n, int) and not isinstance(n, bool), n
    assert -EXACT_LIMIT <= n <= EXACT_LIMIT, f"criteria.exact: {n}"
    return float(n)


def fold(values) -> float:
    """An explicit left fold from +0.0 in the given order (note section 2; never `sum()`)."""
    acc = 0.0
    for value in values:
        acc = acc + value
    return acc


def power_of(x: float, n: int) -> float:
    """`x^n` by repeated multiplication from 1.0 (note section 2)."""
    acc = 1.0
    for _ in range(n):
        acc = acc * x
    return acc


def mean(xs: list):
    """`m = (Σ x) / n`; None for no value."""
    if not xs:
        return None
    return fold(xs) / exact(len(xs))


def variance(xs: list):
    """`V = Σ (x − m)² / (exact(n) − 1.0)`, the square one product `d · d` (the count converted
    first, then 1.0 subtracted, review round 5); None below 2 values."""
    if len(xs) < 2:
        return None
    m = mean(xs)
    acc = 0.0
    for x in xs:
        d = x - m
        acc = acc + d * d
    return acc / (exact(len(xs)) - 1.0)


def mean_se(xs: list):
    """The SE of a mean, `sqrt(V / n)` (births per woman); None below 2 values (review round
    1). The mean itself is defined from one value on."""
    v = variance(xs)
    if v is None:
        return None
    return math.sqrt(v / exact(len(xs)))


def pearson_parts(xs: list, ys: list):
    """(`m_x`, `m_y`, `S_xx`, `S_yy`, `S_xy`): the means first, then one pass with
    `dx = x − m_x`, `dy = y − m_y`, `S_xx += dx · dx`, `S_yy += dy · dy`, `S_xy += dx · dy`
    (each its own fold from +0.0). None for no pair."""
    assert len(xs) == len(ys)
    if not xs:
        return None
    mx, my = mean(xs), mean(ys)
    sxx = syy = sxy = 0.0
    for x, y in zip(xs, ys):
        dx, dy = x - mx, y - my
        sxx = sxx + dx * dx
        syy = syy + dy * dy
        sxy = sxy + dx * dy
    return mx, my, sxx, syy, sxy


def pearson(xs: list, ys: list):
    """`r = S_xy / sqrt(S_xx · S_yy)`; None below 2 pairs or if `S_xx · S_yy` is not above 0."""
    if len(xs) < 2:
        return None
    _, _, sxx, syy, sxy = pearson_parts(xs, ys)
    product = sxx * syy
    if not product > 0.0:
        return None
    return sxy / math.sqrt(product)


def slope(xs: list, ys: list):
    """OLS slope of `y` on `x`: `b = S_xy / S_xx`; None below 2 pairs or if `S_xx` is 0."""
    if len(xs) < 2:
        return None
    _, _, sxx, _, sxy = pearson_parts(xs, ys)
    if not sxx > 0.0:
        return None
    return sxy / sxx


def fisher(r):
    """`F(r) = 0.5 · ln((1 + r) / (1 − r))` (`math.log`, not `atanh`); None outside (−1, 1)."""
    if r is None or not -1.0 < r < 1.0:
        return None
    return 0.5 * math.log((1.0 + r) / (1.0 - r))


def share(k: int, n: int):
    """`p̂ = k / n` (both exact integers converted once); None for `n` = 0."""
    if n == 0:
        return None
    return exact(k) / exact(n)


# The probit (ADR-0019): AS241 (Wichura 1988) as `sim_math::probit`, the coefficients and
# the Horner order of the paper (CPython `statistics._normal_dist_inv_cdf`, which ADR-0019
# names as the check of the coefficients), `ln` correctly rounded (the pinned libm is within
# 1 ulp of it; tests/golden/kat/probit.csv holds the core's bits, `check_probit`), `sqrt`
# correctly rounded.


def ln_cr(x: float) -> float:
    return ln_reference(x)[1]


def probit(p: float) -> float:
    q = p - 0.5
    if math.fabs(q) <= 0.425:
        r = 0.180625 - q * q
        num = (((((((2.5090809287301226727e+3 * r +
                     3.3430575583588128105e+4) * r +
                     6.7265770927008700853e+4) * r +
                     4.5921953931549871457e+4) * r +
                     1.3731693765509461125e+4) * r +
                     1.9715909503065514427e+3) * r +
                     1.3314166789178437745e+2) * r +
                     3.3871328727963666080e+0) * q
        den = (((((((5.2264952788528545610e+3 * r +
                     2.8729085735721942674e+4) * r +
                     3.9307895800092710610e+4) * r +
                     2.1213794301586595867e+4) * r +
                     5.3941960214247511077e+3) * r +
                     6.8718700749205790830e+2) * r +
                     4.2313330701600911252e+1) * r +
                     1.0)
        return num / den
    r = p if q <= 0.0 else 1.0 - p
    r = math.sqrt(-ln_cr(r))
    if r <= 5.0:
        r = r - 1.6
        num = (((((((7.7454501427834140764e-4 * r +
                     2.2723844989269184583e-2) * r +
                     2.4178072517745061177e-1) * r +
                     1.2704582524523683826e+0) * r +
                     3.6478483247632045960e+0) * r +
                     5.7694972214606914055e+0) * r +
                     4.6303378461565452959e+0) * r +
                     1.4234371107496835773e+0)
        den = (((((((1.0507500716444168432e-9 * r +
                     5.4759380849953449460e-4) * r +
                     1.5198666563616457197e-2) * r +
                     1.4810397642748007459e-1) * r +
                     6.8976733498510000455e-1) * r +
                     1.6763848301838038494e+0) * r +
                     2.0531916266377588219e+0) * r +
                     1.0)
    else:
        r = r - 5.0
        num = (((((((2.0103343992922881327e-7 * r +
                     2.7115555687434875782e-5) * r +
                     1.2426609473880784386e-3) * r +
                     2.6532189526576123093e-2) * r +
                     2.9656057182850489123e-1) * r +
                     1.7848265399172913358e+0) * r +
                     5.4637849111641143699e+0) * r +
                     6.6579046435011037772e+0)
        den = (((((((2.0442631033899397856e-15 * r +
                     1.4215117583164458887e-7) * r +
                     1.8463183175100546818e-5) * r +
                     7.8686913114561329059e-4) * r +
                     1.4875361290850614852e-2) * r +
                     1.3692988092273580531e-1) * r +
                     5.9983220655588793769e-1) * r +
                     1.0)
    x = num / den
    if q < 0.0:
        x = -x
    return x


def check_probit() -> int:
    """The port against the core's recorded bits (tests/golden/kat/probit.csv, the class P
    rows and the exact 0.5): returns the rows that differ as (input bits, ulps). The pinned
    `ln` of the core may differ from the correctly rounded one by an ulp, which AS241 carries
    on (ADR-0019 measured up to 3.3 ulp against the exact value); asserted at most 3 ulps.
    Known: `p` = 0.99 differs by 2 ulps (CPython's AS241 with the platform `ln` agrees with
    this port there)."""
    differ = []
    for line in (ROOT / "tests" / "golden" / "kat" / "probit.csv").read_text().splitlines()[2:]:
        given, expected, kind, _ = line.split(",")
        if expected == "nan":
            continue
        got = probit(from_bits(given))
        if bits(got) != expected:
            ulps = int(bits(got), 16) - int(expected, 16)
            assert abs(ulps) <= 3, (given, bits(got), expected)
            differ.append((given, ulps))
    return differ


PHI_HALVINGS = 100


def phi(x: float) -> float:
    """`Φ(x)` (note section 3, I-2): 100 halvings of `[0, 1]`: `mid = (lo + hi) / 2.0`, if
    `probit(mid) < x` the lower end moves to `mid`, else the upper end; the result is the
    midpoint `(lo + hi) / 2.0` after the last halving."""
    lo, hi = 0.0, 1.0
    for _ in range(PHI_HALVINGS):
        mid = (lo + hi) / 2.0
        if probit(mid) < x:
            lo = mid
        else:
            hi = mid
    return (lo + hi) / 2.0


def phi_erfc(x: float) -> float:
    """The cross-check `0.5 · erfc(−x / sqrt(2))` (the note's Python form)."""
    return 0.5 * math.erfc(-x / math.sqrt(2.0))


def close(a: float, b: float, rel: float = 1e-12) -> bool:
    """The census rule for values through `ln` or `Φ`: `|a − b| ≤ 1e-12 · max(1, |a|)`."""
    return abs(a - b) <= rel * max(1.0, abs(a))


def power(c, tol, alt, se):
    """`Φ(((c − tol) − alt) / se) + Φ(((alt − c) − tol) / se)` (note section 3). Undefined
    (None) unless `se > 0` (review round 1, note section 10: NaN, 0 and negative `se` give
    None; `Φ` of NaN would read as a probability near 0), or if an input is undefined."""
    if c is None or tol is None or alt is None or se is None or not se > 0.0:
        return None
    return phi(((c - tol) - alt) / se) + phi(((alt - c) - tol) / se)


MONTHS_PER_YEAR = 13
HIST_MONTHS = 650  # 50 years of 13 months


def km_median(hist: list, never: int):
    """The smoothed Kaplan-Meier median of the first partnership in years (note section 3,
    life_reference `median`): `n = Σ hist + never` (integers), `half = n / 2.0`; scanning `k`
    ascending with `c` the count before `k`, the first `k` with `hist[k] > 0` and `c +
    hist[k] ≥ half` gives `((k − 0.5) + (half − c) / hist[k]) / 13.0`. None if no such `k`."""
    assert len(hist) == HIST_MONTHS
    n = 0
    for h in hist:
        n += h
    n += never
    half = exact(n) / 2.0
    c = 0
    for k, h in enumerate(hist):
        if h > 0 and exact(c + h) >= half:
            return ((exact(k) - 0.5) + (half - exact(c)) / exact(h)) / 13.0
        c += h
    return None


def km_median_window(med: float) -> tuple:
    """The window of `median_se`: `lo = max(0, trunc((med − 1.0) · 13.0))`, `hi = min(650,
    trunc((med + 1.0) · 13.0) + 1)`, `hi` excluded (truncation toward 0; below 0 the `max`
    makes floor and truncation agree)."""
    return max(0, int((med - 1.0) * 13.0)), min(HIST_MONTHS, int((med + 1.0) * 13.0) + 1)


def km_median_se(hist: list, never: int, med: float) -> float:
    """life_reference `median_se`: the window `km_median_window`, `dens = ((Σ hist[lo..hi]) /
    n) / ((hi − lo) / 13.0)` (the sum an integer), `se = sqrt(0.25 / n) / dens`."""
    n = 0
    for h in hist:
        n += h
    n += never
    lo, hi = km_median_window(med)
    s = 0
    for h in hist[lo:hi]:
        s += h
    dens = (exact(s) / exact(n)) / (exact(hi - lo) / 13.0)
    return math.sqrt(0.25 / exact(n)) / dens


def life_table_parts(exposure: list, deaths: list) -> tuple:
    """(finding, table) of the period life table (note section 3, I-3; review rounds 2 and 3,
    note section 10): Chiang II with one-year ages and `a_x` = 0.5; `exposure` and `deaths`
    per age `x` = 0 to ω − 1 and the open interval ω last (integers: seconds and counts).

    Undefined (finding, None), checked in this order: `bad_lengths` (the lists empty or of
    different lengths), `zero_exposure` (any `E_x` = 0), `no_open_deaths` (`D_ω` = 0),
    `q_at_least_one` (a closed age with `q_x ≥ 1`, that is `m_x ≥ 2`), `l_underflow` (any `l_x`,
    x = 0 to ω, not above 0.0: a run of `q` just below 1 underflows `l` to 0), `not_finite`
    (`e_0` or its SE not finite; a last guard, which no input of the domain reaches: counts and
    seconds at most 2^53, at most 101 ages, NOTES.md section 9; above 2^53 `exact` stops). A table of the open interval alone (one entry) is valid: `e_0 = (l_0 / m_ω) /
    l_0`, its variance the open term alone.

    The order of every operation (NOTES.md): `y_x = E_x / 31449600.0`, `m_x = D_x / y_x`, `q_x =
    m_x / (1.0 + 0.5 · m_x)`, `l_0 = 1.0`, `l_{x+1} = l_x · (1.0 − q_x)`, `L_x = 0.5 · (l_x +
    l_{x+1})`, `L_ω = l_ω / m_ω`, `T_ω = L_ω`, `T_x = L_x + T_{x+1}` downwards, `e_x = T_x /
    l_x`; `Var(q_x) = ((q_x · q_x) · (1.0 − q_x)) / D_x` (0 if `D_x` = 0); `Var(e_0)` the fold
    over `x < ω` ascending of `((l_x · l_x) · (t · t)) · Var(q_x)` with `t = 0.5 + e_{x+1}`,
    then plus `((l_ω · l_ω) · D_ω) / ((y_ω · y_ω) · (((m_ω · m_ω) · m_ω) · m_ω))`; `se =
    sqrt(Var(e_0))`."""
    if not exposure or len(exposure) != len(deaths):
        return "bad_lengths", None
    if any(e == 0 for e in exposure):
        return "zero_exposure", None
    if deaths[-1] == 0:
        return "no_open_deaths", None
    w = len(exposure) - 1
    y = [exact(e) / YEAR_F for e in exposure]
    m = [exact(d) / yx for d, yx in zip(deaths, y)]
    q = [m[x] / (1.0 + 0.5 * m[x]) for x in range(w)]
    if any(qx >= 1.0 for qx in q):
        return "q_at_least_one", None
    l = [1.0]
    for x in range(w):
        l.append(l[x] * (1.0 - q[x]))
    if any(not lx > 0.0 for lx in l):
        return "l_underflow", None
    big_l = [0.5 * (l[x] + l[x + 1]) for x in range(w)]
    t = [0.0] * (w + 1)
    t[w] = l[w] / m[w]
    for x in range(w - 1, -1, -1):
        t[x] = big_l[x] + t[x + 1]
    e = [t[x] / l[x] for x in range(w + 1)]
    var_q = [((q[x] * q[x]) * (1.0 - q[x])) / exact(deaths[x]) if deaths[x] > 0 else 0.0 for x in range(w)]
    acc = 0.0
    for x in range(w):
        lead = 0.5 + e[x + 1]
        acc = acc + ((l[x] * l[x]) * (lead * lead)) * var_q[x]
    m4 = ((m[w] * m[w]) * m[w]) * m[w]
    last = ((l[w] * l[w]) * exact(deaths[w])) / ((y[w] * y[w]) * m4)
    var_e0 = acc + last
    se = math.sqrt(var_e0) if var_e0 >= 0.0 else math.nan
    if not (math.isfinite(e[0]) and math.isfinite(se)):
        return "not_finite", None
    return None, {"e0": e[0], "var_e0": var_e0, "se": se, "q": q, "l": l, "e": e}


def life_table_finding(exposure: list, deaths: list):
    """Why the life table is undefined (`life_table_parts`), or None."""
    return life_table_parts(exposure, deaths)[0]


def life_table(exposure: list, deaths: list):
    """The life table (`life_table_parts`), or None (red, a finding)."""
    return life_table_parts(exposure, deaths)[1]


P9_Q = 0.15
P9_P = 0.85


def p9_expectation(n: int) -> float:
    """`E_n` of K5 with `q` = 0.15, `p` = 0.85 (literals): `s = (2.0 · p) · q`, `A = (s · s) ·
    (1.0 − 0.75^n)`, `B = ((4.0 · p) · q³) · (1.0 − 0.5^n)`, `C = q⁴`, `E_n = A / ((A + B) +
    C)`; the powers by repeated multiplication from 1.0."""
    assert n >= 1
    s = (2.0 * P9_P) * P9_Q
    a = (s * s) * (1.0 - power_of(0.75, n))
    b = ((4.0 * P9_P) * power_of(P9_Q, 3)) * (1.0 - power_of(0.5, n))
    c = power_of(P9_Q, 4)
    return a / ((a + b) + c)


VILLAGE_H2 = 0.8


def n_e(n_m: int, n_f: int) -> float:
    """`N_e,g = (4 N_m N_f) / (N_m + N_f)`: numerator and denominator exact integers, each
    converted once, one division."""
    assert n_m > 0 and n_f > 0
    return exact(4 * n_m * n_f) / exact(n_m + n_f)


def village_expectation(blocks: list, h2: float = VILLAGE_H2) -> float:
    """`E_i = (h² · Π_g (1.0 − 1.0 / (2.0 · N_e,g))) + (1.0 − h²)`, the product from 1.0 over
    the blocks with births ascending (`blocks` as (g, N_m, N_f), a block without births has
    no entry)."""
    prod = 1.0
    for _, n_m, n_f in blocks:
        prod = prod * (1.0 - 1.0 / (2.0 * n_e(n_m, n_f)))
    return h2 * prod + (1.0 - h2)


VILLAGE_SEEDS = 20
VILLAGE_LAST_END = 2100
VILLAGE_STEP = 300
VILLAGE_POWER = 0.9


def village_twin(vs: list, v_bar: float, e_bar: float) -> dict:
    """The twin of K2 (Grok D3): `t_i = (V_i − V̄) + 1.0` (shifted to mean 1, the spread kept),
    then the same estimators on the `t_i`: `t̄` the fold divided by `n`, `s_t = sqrt(Σ (t_i −
    t̄)² / (n − 1))` recomputed from the `t_i` (equal to `s` up to the rounding of the shift),
    `se_t = s_t / sqrt(n)`; green iff `|t̄ − Ē| ≤ 4.0 · se_t` (the comparison only, no power,
    no decision)."""
    n = exact(len(vs))
    ts = [(v - v_bar) + 1.0 for v in vs]
    t_bar = fold(ts) / n
    acc = 0.0
    for t in ts:
        d = t - t_bar
        acc = acc + d * d
    s_t = math.sqrt(acc / (exact(len(vs)) - 1.0))
    se_t = s_t / math.sqrt(n)
    return {"v_bar": t_bar, "s": s_t, "se": se_t, "tol": 4.0 * se_t, "pass": abs(t_bar - e_bar) <= 4.0 * se_t}


def village_aggregate(vs: list, es: list, end_years: int) -> dict:
    """K2 (note section 3): `V̄`, `Ē` folds over the seed list divided by 20; `s = sqrt(Σ (V_i
    − V̄)² / 19)`, `se = s / sqrt(20)`, `tol = 4.0 · se`, pass iff `|V̄ − Ē| ≤ tol`; the power
    against 1 `power(Ē, tol, 1.0, se)`; below 0.9 the runs extend by 300 years (`extend`),
    at 2100 red. The decision reads `s` and `Ē` only. Twin (Grok D3, design note of the village
    section 7.2): the measured variances shifted to mean 1, `(V_i − V̄) + 1.0`, against the same
    `Ē` (`village_twin`); it must be red, and it is green iff its comparison passes.

    Review round 1 (note section 10): red with a finding before the numbers count, in this
    order: fewer than 20 seeds (`below_minimum`), any `V_i` exactly 0.0 (`no_variance`, K2),
    `se` not above 0 (`no_spread`, no power). A finding gives {"n", "pass": False,
    "decision": "red", "finding"} and no numbers."""
    assert len(vs) == len(es)

    def finding(name: str) -> dict:
        return {"n": len(vs), "pass": False, "decision": "red", "finding": name}

    if len(vs) < VILLAGE_SEEDS:
        return finding("below_minimum")
    if any(v == 0.0 for v in vs):
        return finding("no_variance")
    n = exact(len(vs))
    v_bar = fold(vs) / n
    e_bar = fold(es) / n
    acc = 0.0
    for v in vs:
        d = v - v_bar
        acc = acc + d * d
    s = math.sqrt(acc / (exact(len(vs)) - 1.0))
    se = s / math.sqrt(n)
    if not se > 0.0:
        return finding("no_spread")
    tol = 4.0 * se
    pw = power(e_bar, tol, 1.0, se)
    passed = abs(v_bar - e_bar) <= tol
    if pw < VILLAGE_POWER:
        decision = "extend" if end_years < VILLAGE_LAST_END else "red"
    else:
        decision = "pass" if passed else "red"
    twin = village_twin(vs, v_bar, e_bar)
    twin_bar, twin_se, twin_pass = twin["v_bar"], twin["se"], twin["pass"]
    return {
        "n": len(vs), "v_bar": v_bar, "e_bar": e_bar, "s": s, "se": se, "tol": tol, "power": pw,
        "pass": passed, "decision": decision, "twin_v_bar": twin_bar, "twin_se": twin_se, "twin_pass": twin_pass,
    }


VILLAGE_RUN = 30


def village_measurable(obj: dict) -> bool:
    """A village counts toward K2 iff its block has at least two units (`village_obj`: a
    `g*` and `n ≥ 2`, so `V` is defined); Grok D1."""
    return obj["g_star"] is not None and obj["n"] >= 2


def village_counted(objs: list) -> dict:
    """Grok D1 (design note of the village, section 7.2): of the villages of the seed list,
    in list order, the first 20 that are measurable count; the others are reported, not
    counted. Fewer than 20: red with the finding `fewer_than_20_villages` (before the
    aggregate, whose `below_minimum` it replaces)."""
    measurable = [i for i, obj in enumerate(objs) if village_measurable(obj)]
    counted = measurable[:VILLAGE_SEEDS]
    finding = None if len(counted) == VILLAGE_SEEDS else "fewer_than_20_villages"
    return {"counted": counted, "measurable": len(measurable), "finding": finding}


# ---------------------------------------------------------------------------------------
# Section 5: the fixed permutations

SEED_P3 = 0x5EED_0003_0000_0001
SEED_P5 = 0x5EED_0005_0000_0001
SEED_P9_POLYGENIC = 0x5EED_0009_0000_0001
SEED_P9_MC1R = 0x5EED_0009_0000_0002
MASK = (1 << 64) - 1


class SplitMix64:
    """SplitMix64 as P7 (note section 5): `state += 0x9e3779b97f4a7c15`, `z = (z ^ (z >> 30))
    · 0xbf58476d1ce4e5b9`, `z = (z ^ (z >> 27)) · 0x94d049bb133111eb`, return `z ^ (z >> 31)`,
    all modulo 2^64."""

    def __init__(self, seed: int):
        self.state = seed & MASK

    def next(self) -> int:
        self.state = (self.state + 0x9E3779B97F4A7C15) & MASK
        z = self.state
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK
        return z ^ (z >> 31)


def shuffled(values: list, seed: int) -> list:
    """Fisher-Yates over a copy in canonical order: for `i` from `n − 1` down to 1, `j =
    splitmix(state) mod (i + 1)`, swap `v[i]`, `v[j]`; one generator from `seed`."""
    v = list(values)
    rng = SplitMix64(seed)
    for i in range(len(v) - 1, 0, -1):
        j = rng.next() % (i + 1)
        v[i], v[j] = v[j], v[i]
    return v


# ---------------------------------------------------------------------------------------
# Section 6: the tolerances at the actual n, the powers, the W asserts

MIN_P3 = 2000
MIN_P4 = 500
MIN_FISHER = 1000
MIN_MC1R = 400
W_SLACK = 5e-5

TARGET_P3 = 0.8
TARGET_SIB = 0.514
TARGET_P9 = 0.4
TARGET_R_K = 0.4
TARGET_R_H = 0.2
P4_WRONG = 0.664
TARGET_MEDIAN = 23.5
TARGET_NEVER = 0.10
TARGET_BIRTHS = 2.20
TARGET_E0 = {"f": 83.43, "m": 78.80}
TARGET_H2T_R2 = 0.2
GAP_BAND = (2.5, 4.0)
EARLY_BAND = (0.001, 0.005)
MC1R_Q_BAND = (0.13, 0.17)


def tol_p3(n: int):
    """`4.0 · sqrt((1.0 − 0.64 · 0.6) / (0.6 · n))`; None for `n` = 0."""
    if n < 1:
        return None
    return 4.0 * math.sqrt((1.0 - 0.64 * 0.6) / (0.6 * exact(n)))


def se_slope(b: float, n: int) -> float:
    """The SE of the slope under the slope `b` (spec_reference `slope`): `sqrt((1.0 − (b · b)
    · 0.6) / (0.6 · n))`."""
    return math.sqrt((1.0 - (b * b) * 0.6) / (0.6 * exact(n)))


def power_p3(n: int):
    """Against slope 1 with the SE of the alternative (W8.3b): `power(0.8, tol_p3(n), 1.0,
    se_slope(1.0, n))`; None for `n` = 0."""
    if n < 1:
        return None
    return power(TARGET_P3, tol_p3(n), 1.0, se_slope(1.0, n))


def tol_log_var(n: int):
    """`4.0 · sqrt(2.0 / (exact(n) − 1.0))` (the count converted first, review round 5); None
    below 2."""
    if n < 2:
        return None
    return 4.0 * math.sqrt(2.0 / (exact(n) - 1.0))


def power_p4(n: int):
    """Against `ln 0.664` (W8.4d): `power(0.0, tol, ln(0.664), sqrt(2.0 / (n − 1)))`; None
    below 2."""
    if n < 2:
        return None
    return power(0.0, tol_log_var(n), math.log(P4_WRONG), math.sqrt(2.0 / (exact(n) - 1.0)))


def tol_fisher(n: int):
    """`4.0 / sqrt(exact(n) − 3.0)` (the count converted first, review round 5); None below 4."""
    if n < 4:
        return None
    return 4.0 / math.sqrt(exact(n) - 3.0)


def power_fisher(target: float, alt: float, n: int):
    """`power(F(target), tol_fisher(n), F(alt), 1.0 / sqrt(n − 3))`; None below 4."""
    if n < 4:
        return None
    return power(fisher(target), tol_fisher(n), fisher(alt), 1.0 / math.sqrt(exact(n) - 3.0))


def mc1r_sums(es: list) -> tuple:
    """(`Σ E_f`, `Σ E_f · (1.0 − E_f)`), two folds in the given order."""
    return fold(es), fold([e * (1.0 - e) for e in es])


def tol_mc1r(es: list):
    """`(4.0 · sqrt(Σ E_f (1 − E_f))) / N`; None for no family."""
    if not es:
        return None
    return (4.0 * math.sqrt(mc1r_sums(es)[1])) / exact(len(es))


def power_mc1r(es: list):
    """Against the share 0.5 with its own SE (W8.9d): `power(Σ E_f / N, tol_mc1r, 0.5,
    sqrt((0.5 · 0.5) / N))`; None for no family."""
    if not es:
        return None
    n = exact(len(es))
    return power(mc1r_sums(es)[0] / n, tol_mc1r(es), 0.5, math.sqrt((0.5 * 0.5) / n))


def tol_never(n: int):
    """`4.0 · sqrt(0.09 / n)` (the model `p` 0.10 of 8.1)."""
    if n < 1:
        return None
    return 4.0 * math.sqrt(0.09 / exact(n))


_SPEC = {}


def spec_value(key: str) -> float:
    """A W value of tools/spec_reference.py, rounded to the decimals the specification
    prints (the value a test asserts)."""
    if not _SPEC:
        import spec_reference

        spec_reference.section_8()
        for name, (value, fmt) in spec_reference.VALUES.items():
            _SPEC[name] = float(spec_reference.render(value, fmt).replace(",", "."))
    return _SPEC[key]


def w_table() -> list:
    """(label, value at the minimum size, W key) for every tolerance and power (note section
    6)."""
    e1 = p9_expectation(1)
    mc1r = [e1] * MIN_MC1R
    return [
        ("p3_tol", tol_p3(MIN_P3), "W8.3a"),
        ("p3_power", power_p3(MIN_P3), "W8.3b"),
        ("p4_tol", tol_log_var(MIN_P4), "W8.4a"),
        ("p4_power", power_p4(MIN_P4), "W8.4d"),
        ("p5_siblings_tol", tol_fisher(MIN_FISHER), "W8.5a"),
        ("p5_siblings_power", power_fisher(TARGET_SIB, 0.257, MIN_FISHER), "W8.5b"),
        ("p5_strangers_tol", tol_fisher(MIN_FISHER), "W8.5a"),
        ("p5_strangers_power", power_fisher(0.0, 0.2, MIN_FISHER), "W8.5c"),
        ("p9_polygenic_tol", tol_fisher(MIN_FISHER), "W8.9f"),
        ("p9_polygenic_power", power_fisher(TARGET_P9, 0.2, MIN_FISHER), "W8.9e"),
        ("p9_mc1r_tol", tol_mc1r(mc1r), "W8.9c"),
        ("p9_mc1r_power", power_mc1r(mc1r), "W8.9d"),
        ("f11_r_k_tol", tol_fisher(MIN_FISHER), "W8.7a"),
        ("f11_r_k_power", power_fisher(TARGET_R_K, 0.2, MIN_FISHER), "W8.7b"),
        ("f11_r_h_tol", tol_fisher(MIN_FISHER), "W8.7c"),
        ("f11_r_h_power", power_fisher(TARGET_R_H, 0.0, MIN_FISHER), "W8.7d"),
    ]


def assert_w_values() -> list:
    """Every tolerance and power at its minimum size reproduces its W value within 5e-5 (note
    section 6), and the four tolerances the note's six decimals (0.090627, 0.253236, 0.126681,
    0.089553); every `Φ` of a power agrees with the `erfc` form."""
    rows = w_table()
    for label, value, key in rows:
        assert abs(value - spec_value(key)) <= W_SLACK, (label, value, key, spec_value(key))
    six = {"p3_tol": 0.090627, "p4_tol": 0.253236, "p5_siblings_tol": 0.126681, "p9_mc1r_tol": 0.089553}
    for label, value, _ in rows:
        if label in six:
            assert round(value, 6) == six[label], (label, value)
    assert p9_expectation(1) == p9_expectation(1) and round(p9_expectation(1), 4) == 0.7225
    assert round(p9_expectation(2), 4) == 0.7574 and round(p9_expectation(3), 4) == 0.7809
    return rows


# ---------------------------------------------------------------------------------------
# Section 1: the export


FORMAT = "simagent.criteria"
VERSION = 1
TRAITS = (("core:growth", 0), ("core:pigment", 0), ("core:cognition", 60 * YEAR))
LONGEVITY = "core:longevity"
MC1R = "core:mc1r"
HEADER_KEYS = ["format", "version", "core_version", "data_hash", "world_seed", "pool_seed", "end", "year", "month", "traits", "longevity", "mc1r", "persons", "couples"]
PERSON_KEYS = ["k", "key", "gkey", "sex", "born", "died", "emig", "mother", "father", "pool", "arrived", "mult", "brought", "genome", "z", "gl", "mc1r"]
GENOME_KEYS = ["m", "p", "x", "px", "y", "mt"]
COUPLE_KEYS = ["k", "woman", "man", "formed", "carried"]


def line(obj: dict) -> str:
    """One compact JSON line: no spaces, keys in insertion order."""
    return json.dumps(obj, separators=(",", ":"), ensure_ascii=True)


def conception_multiples(world: wr.World) -> dict:
    """The multiple index of every child conceived in the town (note section 1, `mult`): the
    number of earlier `core:conception` entries (payload 6) of the log with the same second
    and mother; checked against the conceptions the model recorded."""
    seen, index = {}, {}
    for entry in world.log:
        _, t, subjects, kind, key = pr.parse_entry(entry)
        if kind != 6:
            continue
        mother = subjects[0]
        index[key] = seen.get((t, mother), 0)
        seen[(t, mother)] = index[key] + 1
    for _, _, _, _, _, _, children in world.multiples:
        for i, (key, _) in enumerate(children):
            assert index[key] == i
    return index


def genome_json(person: dict) -> dict:
    """The raw genome (`pool_reference.genome_raw`) as the export writes it."""
    maternal, paternal, x_maternal, (kind, second), mt = pr.genome_raw(person)
    return {
        "m": [hx(word) for word in maternal],
        "p": [hx(word) for word in paternal],
        "x": x_maternal,
        "px": second if kind == 0 else None,
        "y": second if kind == 1 else None,
        "mt": mt,
    }


def genome_from_json(genome: dict) -> dict:
    raw = (
        tuple(int(word, 16) for word in genome["m"]),
        tuple(int(word, 16) for word in genome["p"]),
        genome["x"],
        (0, genome["px"]) if genome["y"] is None else (1, genome["y"]),
        genome["mt"],
    )
    return wr.genome_from_raw(raw)


def header_traits(documents: tuple) -> list:
    """The three header traits as `PhenotypeTrait` of the documents the world runs on (the
    calibration's rows, the map's terms)."""
    import data_reference

    calibration, genome_map = data_reference.strip(documents[1]), data_reference.strip(documents[2])
    return [wr.Data.phenotype_trait(trait, calibration, genome_map) for trait, _ in TRAITS]


def export_world(world: wr.World, end: int, documents: tuple) -> dict:
    """The export of a world run to `end` (note section 1), from what the public views give:
    the pedigree (persons living and archived, partnerships), the phenotype view (`env_of`,
    `phenotype` at the header ages), the genome, the pool of the data and the log (genome
    keys, multiple indices). Returns {"header", "persons", "couples"} with Python values."""
    assert world.closed == end
    data = world.data
    traits = header_traits(documents)
    assert data.longevity is not None and data.longevity_trait == LONGEVITY
    mc1r = data.loci[MC1R]
    _, genome_keys = pr.genome_keys_from_log(world.log)
    multiples = conception_multiples(world)
    pool = data.pool
    persons = []
    for sim in sorted(set(world.sims) | set(world.archive)):
        if sim in world.sims:
            born, key, sex, genome = world.sims[sim][:4]
            died = emig = None
        else:
            born, left, cause, key, sex, genome = world.archive[sim][:6]
            died, emig = (left, None) if cause is not None else (None, left)
        if world.is_pool_person(sim):
            origin = world.origins[sim]
            gkey = origin["genome_key"]
            mother, father = origin["mother"], origin["father"]
            if origin["born_pool"] is not None:
                kind, arrived, mult = 1, origin["arrived"], None
                brought = pool.members[key][1]["births"]
            else:
                _, pregnancy, child = pool.unborn[key]
                kind, arrived, brought = 2, None, 0
                mult = [c["lineage_key"] for c in pregnancy["children"]].index(key)
        else:
            parents = world.parent_keys(sim)
            mother, father = (None, None) if parents is None else parents
            kind, arrived, brought = 0, None, 0
            gkey = key if parents is None else genome_keys[key]
            mult = None if parents is None else multiples[key]
        e_seed, c_seed, env_parents = world.env_of(sim)
        z = [wr.phenotype(e_seed, trait, genome, key, env_parents, age, c_seed) for trait, (_, age) in zip(traits, TRAITS)]
        persons.append({
            "key": key, "gkey": gkey, "sex": "f" if sex == wr.FEMALE else "m", "born": born, "died": died, "emig": emig,
            "mother": mother, "father": father, "pool": kind, "arrived": arrived, "mult": mult, "brought": brought,
            "genome": genome_json(genome), "z": z, "gl": wr.genetic_value(genome, data.longevity),
            "mc1r": sum(wr.copies_at(genome, mc1r)),
        })
    persons.sort(key=lambda p: p["key"])
    by_key = {p["key"]: p for p in persons}
    household_pairs = set()
    if pool is not None:
        for household in pool.households:
            if household["partnership"] is not None:
                household_pairs.add(household["partnership"][:2])
    couples = []
    for woman, man, formed, _ in world.partnerships.values():
        wk, mk = world.key_of(woman), world.key_of(man)
        her = by_key[wk]
        carried = (wk, mk) in household_pairs and her["pool"] == 1 and formed <= her["arrived"]
        couples.append({"woman": wk, "man": mk, "formed": formed, "carried": carried})
    couples.sort(key=lambda c: (c["formed"], c["woman"], c["man"]))
    header = {
        "format": FORMAT, "version": VERSION, "core_version": wr.CORE_VERSION, "data_hash": data.hash,
        "world_seed": world.seed, "pool_seed": None if pool is None else pool.seed, "end": end,
        "year": YEAR, "month": MONTH, "traits": [list(t) for t in TRAITS], "longevity": LONGEVITY, "mc1r": MC1R,
        "persons": len(persons), "couples": len(couples),
    }
    return {"header": header, "persons": persons, "couples": couples}


def export_text(export: dict) -> str:
    """The NDJSON text (note section 1): the header, the persons ascending by key, the couples
    ascending by (formed, woman, man); `\\n` after every line."""
    h = export["header"]
    out = [line({
        "format": h["format"], "version": h["version"], "core_version": h["core_version"], "data_hash": hx(h["data_hash"]),
        "world_seed": hx(h["world_seed"]), "pool_seed": hx_opt(h["pool_seed"]), "end": h["end"], "year": h["year"],
        "month": h["month"], "traits": h["traits"], "longevity": h["longevity"], "mc1r": h["mc1r"],
        "persons": h["persons"], "couples": h["couples"],
    })]
    for p in export["persons"]:
        out.append(line({
            "k": "p", "key": hx(p["key"]), "gkey": hx(p["gkey"]), "sex": p["sex"], "born": p["born"], "died": p["died"],
            "emig": p["emig"], "mother": hx_opt(p["mother"]), "father": hx_opt(p["father"]), "pool": p["pool"],
            "arrived": p["arrived"], "mult": p["mult"], "brought": p["brought"], "genome": p["genome"],
            "z": [bits(z) for z in p["z"]], "gl": bits(p["gl"]), "mc1r": p["mc1r"],
        }))
    for c in export["couples"]:
        out.append(line({"k": "c", "woman": hx(c["woman"]), "man": hx(c["man"]), "formed": c["formed"], "carried": c["carried"]}))
    return "".join(text + "\n" for text in out)


class ExportError(Exception):
    pass


def _require(condition: bool, what: str) -> None:
    if not condition:
        raise ExportError(what)


U64_MAX = (1 << 64) - 1
I64_MIN, I64_MAX = -(1 << 63), (1 << 63) - 1
HEX_DIGITS = frozenset("0123456789abcdef")


def _is_int(value) -> bool:
    """A JSON integer: never a bool, never a float (also not 1.0)."""
    return isinstance(value, int) and not isinstance(value, bool)


def _unsigned(value, name: str, nullable: bool = False, top: int = U64_MAX):
    """An unsigned integer field, 0 to 2^64 − 1 (or `top`); message `unsigned <name>`."""
    if value is None and nullable:
        return None
    _require(_is_int(value) and 0 <= value <= top, f"unsigned {name}")
    return value


def _signed(value, name: str, nullable: bool = False):
    """A signed integer field, −2^63 to 2^63 − 1; message `signed <name>`."""
    if value is None and nullable:
        return None
    _require(_is_int(value) and I64_MIN <= value <= I64_MAX, f"signed {name}")
    return value


def _hex(value, name: str, nullable: bool = False):
    """A hex field: a string of exactly 16 characters 0-9a-f; message `hex <name>`."""
    if value is None and nullable:
        return None
    _require(isinstance(value, str) and len(value) == 16 and all(ch in HEX_DIGITS for ch in value), f"hex {name}")
    return int(value, 16)


def _list(value, length: int, name: str) -> list:
    """A JSON list of exactly `length` entries; message `<name>`."""
    _require(isinstance(value, list) and len(value) == length, name)
    return value


def read_export(text: str) -> dict:
    """The strict reader (the rules of the Rust reader, reviews of step 3b and export round 4).

    Lines: a newline after every line; each line one JSON object in the compact form with
    exactly the keys of the note in their order; the header first, then `persons` person
    lines strictly ascending by key, then `couples` couple lines strictly ascending by
    (formed, woman, man) (a duplicate tuple is rejected).

    Types: unsigned integers (a JSON integer, never a bool or a float, 0 to 2^64 − 1):
    `version` (must be 1), `core_version`, `persons`, `couples`, `brought`, `mult`, the
    genome's `x`, `px`, `y`, `mt`; `pool` and `mc1r` at most 255 before their rules. Signed
    integers (−2^63 to 2^63 − 1): `end`, `year`, `month` (must be 31449600 and 2419200), the
    trait ages, `born`, `died`, `emig`, `arrived`, `formed`. Hex (exactly 16 characters
    0-9a-f): `key`, `gkey`, `mother`, `father`, `data_hash`, `world_seed`, `pool_seed`, the
    genome words, each `z`, `gl`, `woman`, `man`. `z` a list of exactly three, the genome's `m`
    and `p` lists of exactly three; `carried` a JSON bool; `k` "p" or "c"; `sex` "f" or "m".

    Rules: the header's traits, longevity and MC1R exactly those of the note; per person
    `pool` in {0, 1, 2}, `mc1r` 0 to 2, not both `died` and `emig`, `arrived` non-null
    exactly for `pool` = 1, `brought` 0 unless `pool` = 1, `mult` null for `pool` = 1 and
    non-null for `pool` = 2, for `pool` = 0 `mult` non-null exactly when `mother` is, a
    woman's genome with `px` and no `y`, a man's with `y` and no `px`. `check_reader` holds
    one rejected export per rule."""
    _require(text.endswith("\n"), "the last line has no newline")
    lines = text[:-1].split("\n")
    objs = []
    for raw in lines:
        try:
            obj = json.loads(raw)
        except ValueError:
            raise ExportError("json")
        _require(isinstance(obj, dict), "a line is not an object")
        _require(line(obj) == raw, "a line is not in the compact form")
        objs.append(obj)
    h = objs[0]
    _require(list(h) == HEADER_KEYS, "header keys")
    _require(h["format"] == FORMAT, "format")
    _require(_unsigned(h["version"], "version") == VERSION, "version")
    _unsigned(h["core_version"], "core_version")
    header = dict(h)
    header["data_hash"] = _hex(h["data_hash"], "data_hash")
    header["world_seed"] = _hex(h["world_seed"], "world_seed")
    header["pool_seed"] = _hex(h["pool_seed"], "pool_seed", True)
    _signed(h["end"], "end")
    _require(_signed(h["year"], "year") == YEAR, "year")
    _require(_signed(h["month"], "month") == MONTH, "month")
    traits = _list(h["traits"], len(TRAITS), "traits")
    for trait in traits:
        _require(isinstance(_list(trait, 2, "traits")[0], str), "traits")
        _signed(trait[1], "trait age")
    _require(traits == [list(t) for t in TRAITS], "traits")
    _require(h["longevity"] == LONGEVITY, "longevity")
    _require(h["mc1r"] == MC1R, "mc1r")
    _unsigned(h["persons"], "persons")
    _unsigned(h["couples"], "couples")
    _require(len(objs) == 1 + h["persons"] + h["couples"], "line count")
    persons = []
    for obj in objs[1 : 1 + h["persons"]]:
        _require(list(obj) == PERSON_KEYS, "person keys")
        _require(obj["k"] == "p", "k")
        person = {}
        person["key"] = _hex(obj["key"], "key")
        person["gkey"] = _hex(obj["gkey"], "gkey")
        _require(obj["sex"] in ("f", "m"), "sex")
        _signed(obj["born"], "born")
        _signed(obj["died"], "died", True)
        _signed(obj["emig"], "emig", True)
        person["mother"] = _hex(obj["mother"], "mother", True)
        person["father"] = _hex(obj["father"], "father", True)
        _unsigned(obj["pool"], "pool", top=255)
        _signed(obj["arrived"], "arrived", True)
        _unsigned(obj["mult"], "mult", True)
        _unsigned(obj["brought"], "brought")
        g = obj["genome"]
        _require(isinstance(g, dict) and list(g) == GENOME_KEYS, "genome keys")
        for word in _list(g["m"], 3, "genome m") + _list(g["p"], 3, "genome p"):
            _hex(word, "genome word")
        _unsigned(g["x"], "x")
        _unsigned(g["px"], "px", True)
        _unsigned(g["y"], "y", True)
        _unsigned(g["mt"], "mt")
        zs = _list(obj["z"], 3, "z")
        for z in zs:
            _hex(z, "z")
        person["z"] = [from_bits(z) for z in zs]
        _hex(obj["gl"], "gl")
        person["gl"] = from_bits(obj["gl"])
        _unsigned(obj["mc1r"], "mc1r", top=255)
        _require(obj["pool"] in (0, 1, 2), "pool")
        _require(obj["mc1r"] <= 2, "mc1r")
        _require(not (obj["died"] is not None and obj["emig"] is not None), "died and emigrated")
        _require((obj["arrived"] is not None) == (obj["pool"] == 1), "arrived iff pool 1")
        _require(obj["brought"] == 0 or obj["pool"] == 1, "brought of a town-born person")
        _require(obj["mult"] is None or obj["pool"] != 1, "mult of an arrival")
        _require(obj["mult"] is not None or obj["pool"] != 2, "mult of a pool child")
        _require(obj["pool"] != 0 or (obj["mult"] is None) == (obj["mother"] is None), "mult iff mother of a town person")
        if obj["sex"] == "f":
            _require(g["px"] is not None and g["y"] is None, "paternal X of a woman")
        else:
            _require(g["y"] is not None and g["px"] is None, "Y of a man")
        for k in ("sex", "born", "died", "emig", "pool", "arrived", "mult", "brought", "genome", "mc1r"):
            person[k] = obj[k]
        persons.append({k: person[k] for k in PERSON_KEYS[1:]})
    for a, b in zip(persons, persons[1:]):
        _require(a["key"] < b["key"], "persons strictly ascending by key")
    couples = []
    for obj in objs[1 + h["persons"] :]:
        _require(list(obj) == COUPLE_KEYS, "couple keys")
        _require(obj["k"] == "c", "k")
        woman, man = _hex(obj["woman"], "woman"), _hex(obj["man"], "man")
        _signed(obj["formed"], "formed")
        _require(isinstance(obj["carried"], bool), "bool carried")
        couples.append({"woman": woman, "man": man, "formed": obj["formed"], "carried": obj["carried"]})
    order = [(c["formed"], c["woman"], c["man"]) for c in couples]
    for a, b in zip(order, order[1:]):
        _require(a < b, "couples strictly ascending by (formed, woman, man)")
    return {"header": header, "persons": persons, "couples": couples}


def recompute_check(export: dict, world: wr.World, documents: tuple) -> dict:
    """The separate check of section 2: `z` and `gl` recomputed from the exported genome, keys
    and seeds (`E` with the pool seed for `pool` ≠ 0, `C` with the pool seed for `pool` ≠ 0 and
    for a town child of two pool persons, else the world seed; `C` at the parents' keys, its
    own key for both without) against the exported bits within 1e-12; `gl` and `mc1r`
    exactly. Returns the counts."""
    data = world.data
    traits = header_traits(documents)
    h = export["header"]
    by_key = {p["key"]: p for p in export["persons"]}
    worst = 0.0
    for p in export["persons"]:
        genome = genome_from_json(p["genome"])
        pool_seed = h["pool_seed"]
        if p["pool"] != 0:
            e_seed = c_seed = pool_seed
        else:
            e_seed = h["world_seed"]
            two_pool = p["mother"] is not None and by_key[p["mother"]]["pool"] != 0 and by_key[p["father"]]["pool"] != 0
            c_seed = pool_seed if two_pool else h["world_seed"]
        parents = None if p["mother"] is None and p["father"] is None else (p["mother"], p["father"])
        for (trait, (_, age)), z in zip(zip(traits, TRAITS), p["z"]):
            again = wr.phenotype(e_seed, trait, genome, p["key"], parents, age, c_seed)
            assert close(z, again), (hx(p["key"]), z, again)
            worst = max(worst, abs(z - again))
        assert bits(wr.genetic_value(genome, data.longevity)) == bits(p["gl"])
        assert sum(wr.copies_at(genome, data.loci[MC1R])) == p["mc1r"]
    return {"persons": len(export["persons"]), "worst_z_difference": worst}


# ---------------------------------------------------------------------------------------
# Section 4: the units


class Rows:
    """An export read back, with the derived notions of section 1: *town-born* iff `pool` ≠ 1;
    *left before t* iff `died` or `emig` is set and below `t`; *reaches age a in the town*
    iff `born + a · YEAR ≤ end`, the entry (`arrived` for `pool` = 1, else `born`) is at most
    that second and the person has not left before it."""

    def __init__(self, export: dict):
        self.header = export["header"]
        self.end = self.header["end"]
        self.persons = export["persons"]
        self.couples = export["couples"]
        self.by_key = {p["key"]: p for p in self.persons}

    @staticmethod
    def town_born(p: dict) -> bool:
        return p["pool"] != 1

    @staticmethod
    def entry(p: dict) -> int:
        return p["arrived"] if p["pool"] == 1 else p["born"]

    @staticmethod
    def left_before(p: dict, t: int) -> bool:
        return (p["died"] is not None and p["died"] < t) or (p["emig"] is not None and p["emig"] < t)

    def reaches(self, p: dict, age_years: int) -> bool:
        t = p["born"] + age_years * YEAR
        return t <= self.end and self.entry(p) <= t and not self.left_before(p, t)

    def exit(self, p: dict) -> int:
        if p["died"] is not None:
            return p["died"]
        if p["emig"] is not None:
            return p["emig"]
        return self.end + 1

    @staticmethod
    def order(p: dict) -> tuple:
        assert p["mult"] is not None, hx(p["key"])
        return p["born"], p["mult"]


def families(rows: Rows) -> list:
    """K1: a family is a pair (mother key, father key), both rows, with at least one town-born
    child of these parents; its children are its town-born children by (`born`, `mult`), the
    first-born the first; the list in ascending first-born key."""
    groups = {}
    for p in rows.persons:
        if not rows.town_born(p) or p["mother"] is None or p["father"] is None:
            continue
        if p["mother"] in rows.by_key and p["father"] in rows.by_key:
            groups.setdefault((p["mother"], p["father"]), []).append(p)
    out = []
    for (mother, father), children in groups.items():
        children.sort(key=rows.order)
        out.append({"mother": rows.by_key[mother], "father": rows.by_key[father], "children": children, "first": children[0]})
    out.sort(key=lambda f: f["first"]["key"])
    return out


Z_H, Z_PIG, Z_K = 0, 1, 2


def p3_units(fams: list) -> dict:
    """P3: per family `x = (z_h(mother) + z_h(father)) / 2.0`, `y = z_h(first-born)`; anchor
    the first-born."""
    xs = [(f["mother"]["z"][Z_H] + f["father"]["z"][Z_H]) / 2.0 for f in fams]
    ys = [f["first"]["z"][Z_H] for f in fams]
    return {"anchors": [f["first"]["key"] for f in fams], "x": xs, "y": ys}


def p5_sibling_pairs(fams: list) -> list:
    """P5 siblings: `a` the first-born, `b` the first later child with another genome key
    (an MZ co-twin is skipped); families without such a child give nothing."""
    pairs = []
    for f in fams:
        a = f["first"]
        later = [c for c in f["children"][1:] if c["gkey"] != a["gkey"]]
        if later:
            pairs.append((a, later[0]))
    return pairs


def ancestors_a(rows: Rows, f: dict) -> set:
    """`A(c)` of a family's first-born: the non-null keys among its parents and the parents
    on its parents' rows."""
    keys = {f["mother"]["key"], f["father"]["key"]}
    for parent in (f["mother"], f["father"]):
        keys |= {k for k in (parent["mother"], parent["father"]) if k is not None}
    return keys


def p5_stranger_pairs(rows: Rows, fams: list) -> list:
    """P5 strangers: walk the families in canonical order with one pending first-born: none
    pending, it becomes pending; else if `A(pending) ∩ A(c) = ∅` emit (pending, c) and clear;
    else drop `c`."""
    pairs, pending = [], None
    for f in fams:
        a = ancestors_a(rows, f)
        if pending is None:
            pending = (f["first"], a)
        elif not (pending[1] & a):
            pairs.append((pending[0], f["first"]))
            pending = None
    return pairs


def p9_polygenic_pairs(fams: list) -> list:
    """P9 polygenic: family `i` (0-based, canonical order) pairs the mother if `i` is even,
    else the father, with the first-born."""
    return [(f["mother"] if i % 2 == 0 else f["father"], f["first"]) for i, f in enumerate(fams)]


def p9_mc1r_units(fams: list) -> dict:
    """P9 MC1R: the families with an affected child (`mc1r` = 2); `y` = 1 iff both parents
    have `mc1r` < 2; `n_f` the distinct genome keys among the family's children (GK-4); the
    anchor the earliest affected child; `q` the copies over the distinct parents (by key) of
    all families divided by twice their number (GK-3)."""
    sample = []
    for index, f in enumerate(fams):
        affected = [c for c in f["children"] if c["mc1r"] == 2]
        if not affected:
            continue
        y = 1 if f["mother"]["mc1r"] < 2 and f["father"]["mc1r"] < 2 else 0
        n_f = len({c["gkey"] for c in f["children"]})
        sample.append({"index": index, "anchor": affected[0]["key"], "y": y, "n": n_f})
    parents = {}
    for f in fams:
        for parent in (f["mother"], f["father"]):
            parents[parent["key"]] = parent["mc1r"]
    copies = 0
    for value in parents.values():
        copies += value
    return {"sample": sample, "parents": len(parents), "copies": copies}


P4_DECADES = 18


def first_per_mother(rows: Rows, lo: int, hi: int, village: bool = False) -> list:
    """G5: the town-born persons born in `[lo, hi)` (the village also `born ≥ 0`), grouped by
    mother key; per mother the first by (`born`, `mult`); a unit iff it reaches 18 in the
    town, else the mother gives nothing. Units ascending by key."""
    by_mother = {}
    for p in rows.persons:
        if not rows.town_born(p) or p["mother"] is None or not lo <= p["born"] < hi:
            continue
        if village and p["born"] < 0:
            continue
        by_mother.setdefault(p["mother"], []).append(p)
    units = []
    for children in by_mother.values():
        first = min(children, key=rows.order)
        if rows.reaches(first, 18):
            units.append(first)
    units.sort(key=lambda p: p["key"])
    return units


def p4_city_units(rows: Rows) -> list:
    """P4 city (G5): decade `k` = 0 to 17, `[10k · YEAR, (10k + 10) · YEAR)`."""
    return [first_per_mother(rows, 10 * k * YEAR, (10 * k + 10) * YEAR) for k in range(P4_DECADES)]


def village_block(rows: Rows) -> dict:
    """The village of G4 and G5 on one export: end `E = (end + 1) / YEAR` years, the measured
    block `g* = (E − 18) div 30 − 1`, its units as the city (village-born, `born ≥ 0`); per
    block `g` = 0 to `g*` the distinct father and mother keys of the town-born persons born in
    `[30g · YEAR, 30(g + 1) · YEAR)` (also later dead or emigrated; a null key is no key), a
    block without births no factor. A finding: fewer than 2 units or no living person."""
    assert (rows.end + 1) % YEAR == 0, "the village ends at a year end"
    end_years = (rows.end + 1) // YEAR
    if end_years - 18 < 30:
        # No block whose youngest reach 18 by the end (`g*` would be negative; Rust's
        # truncating division and Python's floor division differ there): no measured block.
        return {"end_years": end_years, "g_star": None, "units": [], "blocks": [], "living": None}
    g_star = (end_years - 18) // 30 - 1
    units = first_per_mother(rows, 30 * g_star * YEAR, 30 * (g_star + 1) * YEAR, village=True)
    blocks = []
    for g in range(g_star + 1):
        fathers, mothers = set(), set()
        for p in rows.persons:
            if rows.town_born(p) and 30 * g * YEAR <= p["born"] < 30 * (g + 1) * YEAR:
                if p["father"] is not None:
                    fathers.add(p["father"])
                if p["mother"] is not None:
                    mothers.add(p["mother"])
        if fathers or mothers:
            assert fathers and mothers, "a block with births has fathers and mothers"
            blocks.append((g, len(fathers), len(mothers)))
    living = [p for p in rows.persons if p["died"] is None and p["emig"] is None]
    return {"end_years": end_years, "g_star": g_star, "units": units, "blocks": blocks, "living": len(living)}


def earliest_couples(rows: Rows) -> dict:
    """Per key the earliest couple (the first in couple order) it is a partner of."""
    first = {}
    for c in rows.couples:
        for k in (c["woman"], c["man"]):
            first.setdefault(k, c)
    return first


def f11_pairs(rows: Rows) -> list:
    """F11 pair correlations (GK-1): couples not carried, both partners town-born, and for
    each partner its earliest couple; couple order."""
    first = earliest_couples(rows)
    out = []
    for c in rows.couples:
        w, m = rows.by_key[c["woman"]], rows.by_key[c["man"]]
        if c["carried"] or not (rows.town_born(w) and rows.town_born(m)):
            continue
        if first[c["woman"]] is c and first[c["man"]] is c:
            out.append((w, m))
    return out


def f11_first_partnership(rows: Rows, sex: str) -> dict:
    """F11 median and never share: town-born persons of the sex reaching 50 in the town, their
    earliest couple; `f = (formed − born) div MONTH`, `hist[f]` if `f < 650`, else (or without
    a couple) `never`."""
    first = earliest_couples(rows)
    units, months = [], []
    hist, never = [0] * HIST_MONTHS, 0
    for p in rows.persons:
        if p["sex"] != sex or not rows.town_born(p) or not rows.reaches(p, 50):
            continue
        units.append(p["key"])
        c = first.get(p["key"])
        f = None if c is None else (c["formed"] - p["born"]) // MONTH
        if f is not None:
            assert f >= 0
        if f is not None and f < HIST_MONTHS:
            hist[f] += 1
            months.append(f)
        else:
            never += 1
            months.append(None)
    return {"units": units, "months": months, "hist": hist, "never": never}


def f11_births(rows: Rows) -> list:
    """F11 births (K3, I-7): every woman reaching 50 in the town; `brought` plus her town-born
    children with `born < born_her + 50 · YEAR`; ascending key. Returns (key, births)."""
    children = {}
    for p in rows.persons:
        if rows.town_born(p) and p["mother"] is not None:
            children.setdefault(p["mother"], []).append(p)
    out = []
    for p in rows.persons:
        if p["sex"] != "f" or not rows.reaches(p, 50):
            continue
        limit = p["born"] + 50 * YEAR
        count = len([c for c in children.get(p["key"], []) if c["born"] < limit])
        out.append((p["key"], p["brought"] + count))
    return out


def f11_early_births(rows: Rows) -> dict:
    """F11 early births: every town-born person (each child of a multiple) with `born ≥ 0` and
    a mother; early iff `born − born(mother) < 18 · YEAR`."""
    units, early = [], []
    for p in rows.persons:
        if not rows.town_born(p) or p["born"] < 0 or p["mother"] is None:
            continue
        units.append(p["key"])
        if p["born"] - rows.by_key[p["mother"]]["born"] < 18 * YEAR:
            early.append(p["key"])
    return {"units": units, "early": early}


def f11_gap_couples(rows: Rows) -> list:
    """F11 age gap: couples not carried, couple order."""
    return [c for c in rows.couples if not c["carried"]]


def gap_years(rows: Rows, c: dict) -> float:
    """`(born(woman) − born(man)) / YEAR`: the exact i64 difference to f64, then divided."""
    return exact(rows.by_key[c["woman"]]["born"] - rows.by_key[c["man"]]["born"]) / YEAR_F


PERIOD = (100 * YEAR, 200 * YEAR)
OMEGA = 100


def life_table_input(rows: Rows, sex: str, period=PERIOD) -> dict:
    """E_x and D_x per age 0 to 99 and ω = 100 (note section 3): exposure the seconds in the
    town (entry as section 1, exit `died`, `emig` or `end + 1`) within the period, split at
    the birthdays; deaths in the period at the age `floor((died − born) / YEAR)`; ages of 100
    and more into ω. Every row of the sex is a unit."""
    p0, p1 = period
    exposure, deaths = [0] * (OMEGA + 1), [0] * (OMEGA + 1)
    units = []
    for p in rows.persons:
        if p["sex"] != sex:
            continue
        units.append(p["key"])
        a, b = max(rows.entry(p), p0), min(rows.exit(p), p1)
        born = p["born"]
        if a < b:
            x = (a - born) // YEAR
            while born + x * YEAR < b:
                lo, hi = max(a, born + x * YEAR), min(b, born + (x + 1) * YEAR)
                if lo < hi:
                    exposure[min(x, OMEGA)] += hi - lo
                x += 1
        if p["died"] is not None and p0 <= p["died"] < p1:
            deaths[min((p["died"] - born) // YEAR, OMEGA)] += 1
    return {"units": units, "E": exposure, "D": deaths}


def h2t_sample(rows: Rows, sex: str) -> dict:
    """F11 `h²_T` (GK-2): town-born persons of the sex born in `[0, 90 · YEAR)` who died in
    the town; `T = (died − born) / YEAR` (exact i64 to f64, then divided); the emigrated and
    the still living of the same births counted."""
    units, emigrated, living = [], 0, 0
    for p in rows.persons:
        if p["sex"] != sex or not rows.town_born(p) or not 0 <= p["born"] < 90 * YEAR:
            continue
        if p["died"] is not None:
            units.append(p)
        elif p["emig"] is not None:
            emigrated += 1
        else:
            living += 1
    return {"units": units, "emigrated": emigrated, "living": living}


# ---------------------------------------------------------------------------------------
# The report (note section 8, slice 4b): per check `n`, the unit hash, the value and the
# tolerance as bits, pass, the twin. Fields through `ln` or `Φ` are listed in `inexact`
# (compared within 1e-12 · max(1, |x|)); every other float agrees bit for bit.

FRAGILE = 1e-9


def verdict(n: int, min_n: int, value, target: float, tol) -> tuple:
    """(pass, finding): red below the minimum, red if the value or the tolerance is not
    defined, else pass iff `|value − target| ≤ tol` (inclusive)."""
    if n < min_n:
        return False, "below_minimum"
    if value is None or tol is None:
        return False, "undefined"
    if abs(value - target) <= tol:
        return True, None
    return False, "outside"


def fragile(value, target, tol) -> bool:
    """Whether the decision lies within 1e-9 of its boundary (where a value compared within
    1e-12 could flip it); asserted false for every fixture."""
    if value is None or tol is None:
        return False
    return abs(abs(value - target) - tol) <= FRAGILE


def check_obj(name: str, n: int, min_n: int, anchors: list, value, target, tol, inexact: list, **extra) -> dict:
    passed, finding = verdict(n, min_n, value, target, tol)
    obj = {
        "check": name, "n": n, "min_n": min_n, "units": unit_hash(anchors), "value": fbits(value),
        "target": fbits(target), "tol": fbits(tol), "pass": passed, "finding": finding, "inexact": inexact,
    }
    obj.update(extra)
    obj["fragile"] = fragile(value, target, tol)
    return obj


def twin_obj(n: int, min_n: int, value, target, tol, **extra) -> dict:
    passed, finding = verdict(n, min_n, value, target, tol)
    obj = {"value": fbits(value), "tol": fbits(tol), "pass": passed, "finding": finding}
    obj.update(extra)
    obj["fragile"] = fragile(value, target, tol)
    return obj


def fisher_check(name: str, xs: list, ys: list, anchors: list, target_r: float, alt_r: float, min_n: int, twin_ys=None) -> dict:
    """A correlation on Fisher's scale: `F(r)` against `F(target)` with `4 / sqrt(n − 3)`; the
    power against `F(alt)` at the actual `n`; with `twin_ys` the permuted twin."""
    n = len(xs)
    r = pearson(xs, ys)
    value, target, tol = fisher(r), fisher(target_r), tol_fisher(n)
    pw = power_fisher(target_r, alt_r, n)
    extra = {"r": fbits(r), "power": fbits(pw)}
    if twin_ys is not None:
        tr = pearson(xs, twin_ys)
        extra["twin"] = twin_obj(n, min_n, fisher(tr), target, tol, r=fbits(tr))
    return check_obj(name, n, min_n, anchors, value, target, tol, ["value", "target", "power", "twin.value"], **extra)


def p3_check(fams: list) -> dict:
    u = p3_units(fams)
    n = len(fams)
    b = slope(u["x"], u["y"])
    tb = slope(u["x"], shuffled(u["y"], SEED_P3))
    pw = power_p3(n)
    tol = tol_p3(n)
    return check_obj("p3", n, MIN_P3, u["anchors"], b, TARGET_P3, tol, ["power"], power=fbits(pw), twin=twin_obj(n, MIN_P3, tb, TARGET_P3, tol))


def p4_city_check(rows: Rows) -> dict:
    """P4 city: per decade `ln V_k` of `z_h` against 0 with `4 sqrt(2 / (n − 1))`, at least
    500; the check is red if any decade is red. Twin: every `z_h` times `sqrt(0.664)`; the twin
    test (any decade red) must be red."""
    scale = math.sqrt(P4_WRONG)
    decades, all_pass, twin_all = [], True, True
    for k, units in enumerate(p4_city_units(rows)):
        n = len(units)
        zs = [p["z"][Z_H] for p in units]
        v = variance(zs)
        value = math.log(v) if v is not None and v > 0.0 else None
        tv = variance([z * scale for z in zs])
        twin_value = math.log(tv) if tv is not None and tv > 0.0 else None
        tol = tol_log_var(n)
        decade = check_obj(f"p4_city_{k}", n, MIN_P4, [p["key"] for p in units], value, 0.0, tol, ["value", "power", "twin.value"],
                           k=k, variance=fbits(v), power=fbits(power_p4(n)),
                           twin=twin_obj(n, MIN_P4, twin_value, 0.0, tol, variance=fbits(tv)))
        all_pass = all_pass and decade["pass"]
        twin_all = twin_all and decade["twin"]["pass"]
        decades.append(decade)
    return {"check": "p4_city", "pass": all_pass, "twin_pass": twin_all, "decades": decades}


def p5_checks(rows: Rows, fams: list) -> list:
    sib = p5_sibling_pairs(fams)
    xa = [a["z"][Z_H] for a, _ in sib]
    xb = [b["z"][Z_H] for _, b in sib]
    sib_anchors = flat([(a["key"], b["key"]) for a, b in sib])
    siblings = fisher_check("p5_siblings", xa, xb, sib_anchors, TARGET_SIB, 0.257, MIN_FISHER, shuffled(xb, SEED_P5))
    strangers_pairs = p5_stranger_pairs(rows, fams)
    sa = [a["z"][Z_H] for a, _ in strangers_pairs]
    sb = [b["z"][Z_H] for _, b in strangers_pairs]
    strangers = fisher_check("p5_strangers", sa, sb, flat([(a["key"], b["key"]) for a, b in strangers_pairs]), 0.0, 0.2, MIN_FISHER)
    # Twin (I-c): the sibling pairs taken as strangers, against 0.
    tr = pearson(xa, xb)
    strangers["twin"] = twin_obj(len(sib), MIN_FISHER, fisher(tr), 0.0, tol_fisher(len(sib)), r=fbits(tr), units=unit_hash(sib_anchors))
    return [siblings, strangers]


def p9_polygenic_check(fams: list) -> dict:
    pairs = p9_polygenic_pairs(fams)
    xs = [parent["z"][Z_PIG] for parent, _ in pairs]
    ys = [child["z"][Z_PIG] for _, child in pairs]
    anchors = flat([(parent["key"], child["key"]) for parent, child in pairs])
    return fisher_check("p9_polygenic", xs, ys, anchors, TARGET_P9, 0.2, MIN_FISHER, shuffled(ys, SEED_P9_POLYGENIC))


def p9_mc1r_check(fams: list) -> dict:
    """P9 MC1R (K5, GK-3, GK-4): `q` first (outside [0.13, 0.17] red, no expectation); else the
    share `Σ y_f / N` against `Σ E_f / N` with `4 sqrt(Σ E_f (1 − E_f)) / N`, at least 400.
    Twin: the index vector of all families permuted; the sample family of canonical index `i`
    takes the parents of family `π[i]`, `E_f` unchanged."""
    m = p9_mc1r_units(fams)
    sample = m["sample"]
    n = len(sample)
    anchors = [s["anchor"] for s in sample]
    q = None if m["parents"] == 0 else exact(m["copies"]) / exact(2 * m["parents"])
    extra = {"q": fbits(q), "q_parents": m["parents"], "q_copies": m["copies"], "affected": n}
    if q is None or not MC1R_Q_BAND[0] <= q <= MC1R_Q_BAND[1]:
        obj = check_obj("p9_mc1r", n, MIN_MC1R, anchors, None, None, None, [], **extra)
        obj["pass"], obj["finding"] = False, "q_outside"
        obj["twin"] = {"pass": False, "finding": "q_outside"}
        return obj
    es = [p9_expectation(s["n"]) for s in sample]
    k = 0
    for s in sample:
        k += s["y"]
    value = share(k, n)
    expectation = None if n == 0 else mc1r_sums(es)[0] / exact(n)
    tol = tol_mc1r(es)
    pw = power_mc1r(es)
    pi = shuffled(list(range(len(fams))), SEED_P9_MC1R)
    tk = 0
    for s in sample:
        f = fams[pi[s["index"]]]
        tk += 1 if f["mother"]["mc1r"] < 2 and f["father"]["mc1r"] < 2 else 0
    extra.update({"k": k, "sum_e": fbits(mc1r_sums(es)[0]), "sum_var": fbits(mc1r_sums(es)[1]), "power": fbits(pw)})
    obj = check_obj("p9_mc1r", n, MIN_MC1R, anchors, value, expectation, tol, ["power"], **extra)
    obj["twin"] = twin_obj(n, MIN_MC1R, share(tk, n), expectation, tol, k=tk)
    return obj


def f11_checks(rows: Rows) -> list:
    out = []
    pairs = f11_pairs(rows)
    anchors = flat([(w["key"], m["key"]) for w, m in pairs])
    for name, index, target, alt in (("f11_r_k", Z_K, TARGET_R_K, 0.2), ("f11_r_h", Z_H, TARGET_R_H, 0.0)):
        out.append(fisher_check(name, [w["z"][index] for w, _ in pairs], [m["z"][index] for _, m in pairs], anchors, target, alt, MIN_FISHER))
    for sex in ("f", "m"):
        fp = f11_first_partnership(rows, sex)
        n = len(fp["units"])
        med = km_median(fp["hist"], fp["never"]) if n else None
        se = km_median_se(fp["hist"], fp["never"], med) if med is not None else None
        tol = None if se is None else 4.0 * se
        median = check_obj(f"f11_median_{sex}", n, 0, fp["units"], med, TARGET_MEDIAN, tol, [], se=fbits(se), never=fp["never"])
        p_never = share(fp["never"], n)
        never = check_obj(f"f11_never_{sex}", n, 0, fp["units"], p_never, TARGET_NEVER, tol_never(n), [], never=fp["never"])
        if sex == "m":
            for obj in (median, never):
                obj["pass"], obj["finding"], obj["report_only"] = None, None, True
        out += [median, never]
    births = f11_births(rows)
    ys = [exact(y) for _, y in births]
    n = len(ys)
    m, v, se = mean(ys), variance(ys), mean_se(ys)
    out.append(check_obj("f11_births", n, 0, [k for k, _ in births], m, TARGET_BIRTHS, None if se is None else 4.0 * se, [],
                         variance=fbits(v), se=fbits(se), total=int(fold(ys))))
    early = f11_early_births(rows)
    value = share(len(early["early"]), len(early["units"]))
    obj = check_obj("f11_early_births", len(early["units"]), 0, early["units"], value, None, None, [], early=len(early["early"]),
                    band=[bits(EARLY_BAND[0]), bits(EARLY_BAND[1])])
    obj["pass"] = value is not None and EARLY_BAND[0] <= value <= EARLY_BAND[1]
    obj["finding"] = None if obj["pass"] else ("undefined" if value is None else "outside")
    out.append(obj)
    gaps = f11_gap_couples(rows)
    value = mean([gap_years(rows, c) for c in gaps])
    obj = check_obj("f11_age_gap", len(gaps), 0, flat([(c["woman"], c["man"]) for c in gaps]), value, None, None, [],
                    band=[bits(GAP_BAND[0]), bits(GAP_BAND[1])])
    obj["pass"] = value is not None and GAP_BAND[0] <= value <= GAP_BAND[1]
    obj["finding"] = None if obj["pass"] else ("undefined" if value is None else "outside")
    out.append(obj)
    for sex in ("f", "m"):
        lt = life_table_input(rows, sex)
        table = life_table(lt["E"], lt["D"])
        e0 = None if table is None else table["e0"]
        se = None if table is None else table["se"]
        obj = check_obj(f"f11_e0_{sex}", len(lt["units"]), 0, lt["units"], e0, TARGET_E0[sex], None if se is None else 4.0 * se, [],
                        se=fbits(se), exposure=lt["E"], deaths=lt["D"], zero_exposure=[x for x, e in enumerate(lt["E"]) if e == 0])
        if table is None:
            obj["finding"] = life_table_finding(lt["E"], lt["D"])
        out.append(obj)
    for sex in ("f", "m"):
        s = h2t_sample(rows, sex)
        ts = [exact(p["died"] - p["born"]) / YEAR_F for p in s["units"]]
        gl = [p["gl"] for p in s["units"]]
        r = pearson(ts, gl)
        obj = fisher_check(f"f11_h2t_{sex}", ts, gl, [p["key"] for p in s["units"]], math.sqrt(TARGET_H2T_R2), math.sqrt(TARGET_H2T_R2), 0)
        obj["h2"] = fbits(None if r is None else r * r)
        obj["emigrated"], obj["living"] = s["emigrated"], s["living"]
        obj["power"] = None
        obj["inexact"] = ["value", "target"]
        out.append(obj)
    return out


def village_obj(rows: Rows) -> dict:
    vb = village_block(rows)
    zs = [p["z"][Z_H] for p in vb["units"]]
    v = variance(zs)
    e = village_expectation(vb["blocks"]) if vb["blocks"] else None
    finding = None
    if vb["g_star"] is None:
        finding = "no_block"
    elif len(vb["units"]) < 2:
        finding = "fewer_than_two_units"
    elif vb["living"] == 0:
        finding = "no_living_person"
    return {
        "check": "village_block", "end_years": vb["end_years"], "g_star": vb["g_star"], "n": len(vb["units"]),
        "units": unit_hash([p["key"] for p in vb["units"]]), "variance": fbits(v), "expectation": fbits(e),
        "blocks": [list(b) for b in vb["blocks"]], "living": vb["living"], "finding": finding,
    }


def report(export: dict, export_bytes: bytes) -> dict:
    rows = Rows(export)
    fams = families(rows)
    checks = [p3_check(fams), p4_city_check(rows)] + p5_checks(rows, fams) + [p9_polygenic_check(fams), p9_mc1r_check(fams)] + f11_checks(rows)
    return {
        "format": "simagent.criteria.report", "version": 1, "export": hx(hash64(export_bytes)), "end": rows.end,
        "persons": len(rows.persons), "couples": len(rows.couples), "families": len(fams),
        "family_units": unit_hash([f["first"]["key"] for f in fams]), "checks": checks, "village_block": village_obj(rows),
    }


def all_check_objects(rep: dict) -> list:
    out = []
    for c in rep["checks"]:
        if "decades" in c:
            out += c["decades"]
        else:
            out.append(c)
    return out


def assert_not_fragile(rep: dict) -> None:
    for c in all_check_objects(rep):
        assert not c.get("fragile"), c["check"]
        assert not c.get("twin", {}).get("fragile"), c["check"]


def summary_lines(name: str, rep: dict) -> list:
    """A readable form of a report (not pinned)."""
    def show(value):
        return "null" if value is None else repr(from_bits(value))

    out = [f"{name}: export {rep['export']} end {rep['end']} persons {rep['persons']} couples {rep['couples']} families {rep['families']}"]
    for c in all_check_objects(rep):
        line_ = f"  {c['check']:<20} n {c['n']:>5} value {show(c.get('value'))} target {show(c.get('target'))} tol {show(c.get('tol'))} pass {c['pass']} {c.get('finding') or ''}"
        if "twin" in c and "value" in c["twin"]:
            line_ += f" | twin {show(c['twin']['value'])} pass {c['twin']['pass']}"
        out.append(line_)
    v = rep["village_block"]
    out.append(f"  village_block g* {v['g_star']} n {v['n']} V {show(v['variance'])} E {show(v['expectation'])} blocks {v['blocks']} {v['finding'] or ''}")
    return out


# ---------------------------------------------------------------------------------------
# compare


def compare_values(path: str, a, b, inexact: set, problems: list) -> None:
    if isinstance(a, dict) and isinstance(b, dict):
        if list(a) != list(b):
            problems.append(f"{path}: keys {list(a)} != {list(b)}")
            return
        for k in a:
            compare_values(f"{path}.{k}" if path else k, a[k], b[k], inexact, problems)
    elif isinstance(a, list) and isinstance(b, list):
        if len(a) != len(b):
            problems.append(f"{path}: length {len(a)} != {len(b)}")
            return
        for i, (x, y) in enumerate(zip(a, b)):
            compare_values(f"{path}[{i}]", x, y, inexact, problems)
    elif a != b:
        if path.split(".")[-1].split("[")[0] in inexact or path in inexact:
            if isinstance(a, str) and isinstance(b, str) and close(from_bits(a), from_bits(b)):
                return
        problems.append(f"{path}: {a!r} != {b!r}")


def compare_reports(a: dict, b: dict) -> list:
    """Counts, hashes and verdicts exact; floats bit for bit unless the check lists them in
    `inexact` (then within 1e-12 · max(1, |x|))."""
    problems = []
    flat_a, flat_b = all_check_objects(a), all_check_objects(b)
    head = {k: v for k, v in a.items() if k not in ("checks", "export")}
    compare_values("", head, {k: v for k, v in b.items() if k not in ("checks", "export")}, set(), problems)
    if len(flat_a) != len(flat_b):
        return problems + ["check count differs"]
    for x, y in zip(flat_a, flat_b):
        inexact = set(x.get("inexact", []))
        inexact |= {name.split(".")[-1] for name in inexact if name.startswith("twin.")}
        compare_values(x["check"], x, y, inexact, problems)
    return problems


def compare_exports(a: dict, b: dict) -> list:
    """Integers exact, `z` and `gl` within 1e-12 · max(1, |x|) (note section 7, fixture 3)."""
    problems = []
    if a["header"] != b["header"]:
        problems.append("header differs")
    if len(a["persons"]) != len(b["persons"]):
        return problems + ["person count differs"]
    for p, q in zip(a["persons"], b["persons"]):
        for k in PERSON_KEYS[1:]:
            if k in ("z", "gl"):
                continue
            if p[k] != q[k]:
                problems.append(f"person {hx(p['key'])} {k}")
        for x, y in zip(p["z"] + [p["gl"]], q["z"] + [q["gl"]]):
            if not close(x, y):
                problems.append(f"person {hx(p['key'])} z or gl")
    if a["couples"] != b["couples"]:
        problems.append("couples differ")
    return problems


# ---------------------------------------------------------------------------------------
# Fixture 1: the estimator vectors (note section 7.1)

EST_X = [0.73, -1.12, 0.05, 1.98, -0.41, 0.66, -2.03, 0.31, 1.27, -0.88]
EST_Y = [0.52, -0.67, 0.44, 1.31, -0.92, 0.18, -1.45, 0.73, 0.61, -0.29]
EST_FISHER = [0.0, 0.2, 0.257, 0.4, 0.514, -0.3, 0.9]
EST_HIST = {150: 3, 230: 7, 247: 11, 260: 9, 273: 12, 286: 6, 299: 4, 351: 2, 499: 1, 649: 1}
EST_NEVER = 5
# A three-age table: ages 0 and 1, the open interval 2 (seconds and deaths).
EST_LIFE_E = [1000 * YEAR + 123_456, 950 * YEAR + 654_321, 5000 * YEAR + 777]
EST_LIFE_D = [10, 19, 2500]
EST_BIRTHS = [3, 2, 0, 1, 4, 2, 2, 3, 1, 0, 2, 5]
EST_PHI = [-3.0, 0.0, 1.5]
EST_POWER = [(0.8, 0.0906, 1.0, 0.0183), (0.0, 0.2532, -0.4095, 0.0633), (0.5, 0.1, 0.62, 0.03)]
# The village: per seed the blocks (g, N_m, N_f) and a variance V_i (synthetic).
EST_VILLAGE_BLOCKS = [(0, 40, 40), (1, 31, 33), (2, 29, 35), (3, 30, 30)]
EST_VILLAGE_V = [0.81, 0.93, 0.77, 0.88, 1.02, 0.85, 0.79, 0.95, 0.83, 0.9, 0.86, 0.74, 0.99, 0.82, 0.91, 0.87, 0.8, 0.94, 0.84, 0.89]
EST_VILLAGE_E = [0.9612, 0.9587, 0.9634, 0.9601, 0.9579, 0.9623, 0.9599, 0.9608, 0.9615, 0.959, 0.9604, 0.9611, 0.9597, 0.9609, 0.962, 0.9583, 0.9606, 0.9614, 0.9593, 0.9602]
# A drifted village (Ē near 0.75): the power is high, the check passes, the twin is red.
EST_VILLAGE_V2 = [0.71, 0.83, 0.69, 0.77, 0.74, 0.81, 0.66, 0.79, 0.72, 0.85, 0.7, 0.76, 0.68, 0.8, 0.73, 0.78, 0.75, 0.64, 0.82, 0.76]
EST_VILLAGE_E2 = [0.7512, 0.7488, 0.7531, 0.7476, 0.7505, 0.7521, 0.7493, 0.7517, 0.7484, 0.7509, 0.7526, 0.7499, 0.7513, 0.7481, 0.7502, 0.7518, 0.7495, 0.7507, 0.749, 0.7511]


# Review round 1: the undefined cases, more tolerances, median windows, village findings.
UNDEF_VARYING = [0.1, 0.4, -0.2, 0.9]
UNDEF_CONSTANT = [0.5, 0.5, 0.5, 0.5]  # a constant whose fold mean is exact (2.0 / 4)
NEAR_CONSTANT = [0.1, 0.1, 0.1]  # constant, but its fold mean is not 0.1
NEAR_CONSTANT_Y = [0.2, 0.5, 0.9]
MEDIAN_WINDOWS = {
    "a": ({286: 1, 287: 1, 300: 5, 313: 2}, 1),
    "b": ({649: 4}, 1),
    "c": ({2: 4}, 1),
}


def review_round_1_vectors() -> dict:
    """The keys `undefined`, `defined_near_constant`, `tolerances_more`, `median_windows`,
    `village_findings` of estimators.json (review round 1 of the estimators)."""
    nan = float("nan")
    out = {}
    pearson_cases = [([], []), ([0.3], [0.9]), (UNDEF_CONSTANT, UNDEF_VARYING), (UNDEF_VARYING, UNDEF_CONSTANT)]
    slope_cases = [([0.3], [0.9]), (UNDEF_CONSTANT, UNDEF_VARYING)]
    power_cases = [(TARGET_P3, 0.0906, 1.0, 0.0), (TARGET_P3, 0.0906, 1.0, -1.0), (TARGET_P3, 0.0906, 1.0, nan)]
    for xs, ys in pearson_cases:
        assert pearson(xs, ys) is None
    for xs, ys in slope_cases:
        assert slope(xs, ys) is None
    for r in (1.0, -1.0, 1.5):
        assert fisher(r) is None
    for case in power_cases:
        assert power(*case) is None
    assert mean([]) is None and variance([0.7]) is None and tol_mc1r([]) is None
    assert tol_p3(0) is None and tol_never(0) is None and all(tol_log_var(n) is None for n in (0, 1))
    assert all(tol_fisher(n) is None for n in range(4))
    zero_slope = slope(UNDEF_VARYING, UNDEF_CONSTANT)
    assert bits(zero_slope) == bits(0.0)
    one = [2.0]
    assert mean(one) == 2.0 and mean_se(one) is None
    seq_bits = lambda xs: [bits(x) for x in xs]
    out["undefined"] = {
        "mean": [{"x": []}],
        "variance": [{"x": seq_bits([0.7])}],
        "pearson": [{"x": seq_bits(xs), "y": seq_bits(ys)} for xs, ys in pearson_cases],
        "slope": [{"x": seq_bits(xs), "y": seq_bits(ys)} for xs, ys in slope_cases],
        "slope_constant_y_is_zero": {"x": seq_bits(UNDEF_VARYING), "y": seq_bits(UNDEF_CONSTANT), "slope": bits(zero_slope)},
        "fisher": seq_bits([1.0, -1.0, 1.5]),
        "power": [{"c": bits(c), "tol": bits(t), "alt": bits(a), "se": bits(se)} for c, t, a, se in power_cases],
        "tol_p3": [0], "tol_log_var": [0, 1], "tol_fisher": [0, 1, 2, 3], "tol_never": [0], "tol_mc1r_families": [0],
        "births_one_value": {"values": [2], "mean": bits(2.0), "variance": None, "se": None},
    }
    near = pearson(NEAR_CONSTANT, NEAR_CONSTANT_Y)
    mx, _, sxx, _, _ = pearson_parts(NEAR_CONSTANT, NEAR_CONSTANT_Y)
    assert near is not None and sxx > 0.0 and mx != 0.1
    out["defined_near_constant"] = {"x": seq_bits(NEAR_CONSTANT), "y": seq_bits(NEAR_CONSTANT_Y), "mean_x": bits(mx),
                                    "s_xx": bits(sxx), "r": bits(near), "slope": bits(slope(NEAR_CONSTANT, NEAR_CONSTANT_Y))}
    out["tolerances_more"] = {
        "p3": {str(n): bits(tol_p3(n)) for n in (500, 8000)},
        "p4": {str(n): bits(tol_log_var(n)) for n in (3, 9)},
        "fisher": {str(n): bits(tol_fisher(n)) for n in (4, 403)},
        "never": {"9": bits(tol_never(9))},
        "mc1r_16_half": bits(tol_mc1r([0.5] * 16)),
    }
    assert tol_log_var(3) == 4.0 and tol_log_var(9) == 2.0 and tol_fisher(4) == 4.0 and tol_fisher(403) == 0.2
    assert tol_mc1r([0.5] * 16) == 0.5
    windows = {}
    for name, (counts, never) in MEDIAN_WINDOWS.items():
        hist = [0] * HIST_MONTHS
        for k, h in counts.items():
            hist[k] = h
        med = km_median(hist, never)
        lo, hi = km_median_window(med)
        inside = 0
        for h in hist[lo:hi]:
            inside += h
        windows[name] = {"hist": {str(k): h for k, h in counts.items()}, "never": never, "median": bits(med),
                         "se": bits(km_median_se(hist, never, med)), "lo": lo, "hi": hi, "inside": inside}
    # By hand: (a) n 10, half 5, k 300 with c 2: (299.5 + 3/5) / 13; window [287, 314) holds
    # 287, 300, 313 = 8 (one month down or up: 7). (b) n 5: (648.5 + 2.5/4) / 13; [636, 650).
    # (c) n 5: (1.5 + 2.5/4) / 13; trunc(-10.875) = -10, so [0, 16).
    assert (windows["a"]["lo"], windows["a"]["hi"], windows["a"]["inside"]) == (287, 314, 8)
    assert (windows["b"]["lo"], windows["b"]["hi"], windows["b"]["inside"]) == (636, 650, 4)
    assert (windows["c"]["lo"], windows["c"]["hi"], windows["c"]["inside"]) == (0, 16, 4)
    assert from_bits(windows["a"]["median"]) == 300.1 / 13.0
    out["median_windows"] = windows
    no_variance = list(EST_VILLAGE_V)
    no_variance[5] = 0.0
    cases = {
        "below_minimum": (EST_VILLAGE_V[:19], EST_VILLAGE_E[:19]),
        "no_variance": (no_variance, EST_VILLAGE_E),
        "no_spread": ([0.5] * 20, [0.5] * 20),
    }
    findings = {}
    for name, (vs, es) in cases.items():
        result = village_aggregate(vs, es, 1200)
        assert result == {"n": len(vs), "pass": False, "decision": "red", "finding": name}, result
        findings[name] = {"v": seq_bits(vs), "e": seq_bits(es), "result": result}
    out["village_findings"] = findings
    return out


# Review round 2: the edges of the life table (exposure in seconds, deaths).
LIFE_TABLE_EDGES = [
    ("empty", [], []),
    ("lengths_2_3", [YEAR, YEAR], [1, 1, 1]),
    ("q0_one", [YEAR, YEAR], [2, 1]),
    ("q0_above_one", [YEAR, YEAR], [3, 1]),
    ("open_only", [4 * YEAR], [2]),
    ("two_ages_q0_two_thirds", [YEAR, YEAR], [1, 1]),
    ("two_ages_q0_just_below_one", [YEAR + 1, YEAR], [2, 1]),
    # Review round 3: every q about 0.9999999841 < 1, but l[42] underflows to 0.
    ("l_underflow", [YEAR + 1] * 100 + [YEAR], [2] * 100 + [1]),
]
# Outside the domain (deaths are counts, never negative; an unsigned count cannot hold
# these): negative deaths with m just above −2 make 1 − q about 6.3e7, l overflows to inf
# (above 0, so not `l_underflow`) and e_0 = inf / inf is NaN: the only way to `not_finite`.
LIFE_TABLE_OUT_OF_DOMAIN = [
    ("negative_deaths_l_overflow", [YEAR + 1] * 100 + [YEAR], [-2] * 100 + [1]),
]


def life_table_edges() -> dict:
    """`life_table_edges` of estimators.json (review round 2): per case the lists, the finding
    and `e_0`, `Var(e_0)`, SE as bits (null where undefined). By hand: `open_only` `y` 4, `m`
    0.5, `e_0` = 1 / 0.5 = 2, `Var` = (1 · 2) / (16 · 0.0625) = 2, SE `sqrt 2`;
    `two_ages_q0_two_thirds` `q_0` 2/3, `l_1` 1/3, `T_1` = 1/3, `T_0` = 2/3 + 1/3, `e_0` 1,
    `Var` = 2.25 · 4/27 + 1/9 = 4/9, SE 2/3 (each up to rounding)."""
    out = {}
    for name, exposure, deaths in LIFE_TABLE_EDGES:
        table = life_table(exposure, deaths)
        out[name] = {
            "E": exposure, "D": deaths, "finding": life_table_finding(exposure, deaths),
            "e0": None if table is None else bits(table["e0"]), "var_e0": None if table is None else bits(table["var_e0"]),
            "se": None if table is None else bits(table["se"]), "q": None if table is None else [bits(q) for q in table["q"]],
        }
    expected = {"empty": "bad_lengths", "lengths_2_3": "bad_lengths", "q0_one": "q_at_least_one", "q0_above_one": "q_at_least_one",
                "l_underflow": "l_underflow"}
    assert all(q < 1.0 for q in [m / (1.0 + 0.5 * m) for m in [2.0 / ((YEAR + 1) / YEAR_F)]])
    for name, finding in expected.items():
        assert out[name]["finding"] == finding and out[name]["e0"] is None, name
    open_only = life_table([4 * YEAR], [2])
    assert open_only["e0"] == 2.0 and open_only["var_e0"] == 2.0 and open_only["se"] == math.sqrt(2.0)
    two = life_table([YEAR, YEAR], [1, 1])
    assert abs(two["e0"] - 1.0) <= 1e-15 and abs(two["var_e0"] - 4.0 / 9.0) <= 1e-15 and abs(two["se"] - 2.0 / 3.0) <= 1e-15
    near = life_table([YEAR + 1, YEAR], [2, 1])
    assert 0.9999999 < near["q"][0] < 1.0
    return out


def life_table_out_of_domain() -> dict:
    """Inputs outside the domain (negative deaths) that reach `not_finite`; kept apart from
    `life_table_edges` so that a reader of unsigned counts can skip them."""
    out = {}
    for name, exposure, deaths in LIFE_TABLE_OUT_OF_DOMAIN:
        finding = life_table_finding(exposure, deaths)
        assert finding == "not_finite", (name, finding)
        out[name] = {"E": exposure, "D": deaths, "finding": finding, "e0": None, "var_e0": None, "se": None, "q": None}
    return out


def estimator_vectors() -> dict:
    mx, my, sxx, syy, sxy = pearson_parts(EST_X, EST_Y)
    hist = [0] * HIST_MONTHS
    for k, h in EST_HIST.items():
        hist[k] = h
    med = km_median(hist, EST_NEVER)
    table = life_table(EST_LIFE_E, EST_LIFE_D)
    by = [exact(y) for y in EST_BIRTHS]
    vb, mb = variance(by), mean(by)
    village = village_aggregate(EST_VILLAGE_V, EST_VILLAGE_E, 1200)
    village_at_end = village_aggregate(EST_VILLAGE_V, EST_VILLAGE_E, 2100)
    drifted = village_aggregate(EST_VILLAGE_V2, EST_VILLAGE_E2, 1200)
    assert village["decision"] == "extend" and village_at_end["decision"] == "red" and village["twin_pass"]
    assert drifted["power"] >= VILLAGE_POWER and drifted["pass"] and drifted["decision"] == "pass" and not drifted["twin_pass"]
    perms = {name: shuffled(list(range(10)), seed) for name, seed in
             (("p3", SEED_P3), ("p5", SEED_P5), ("p9_polygenic", SEED_P9_POLYGENIC), ("p9_mc1r", SEED_P9_MC1R))}
    first_words = {name: [hx(SplitMix64(seed).next())] for name, seed in
                   (("p3", SEED_P3), ("p5", SEED_P5), ("p9_polygenic", SEED_P9_POLYGENIC), ("p9_mc1r", SEED_P9_MC1R))}
    for name, seed in (("p3", SEED_P3),):
        rng = SplitMix64(seed)
        first_words[name] = [hx(rng.next()) for _ in range(3)]
    for x in EST_PHI:
        assert close(phi(x), phi_erfc(x))
    return {
        "format": "simagent.criteria.estimators", "version": 1,
        "inexact": ["fisher[].F", "phi.*", "power[].power", "w[].value of the rows *_power", "village.aggregate.power",
                    "village_drifted.aggregate.power"],
        "pearson": {
            "x": [bits(x) for x in EST_X], "y": [bits(y) for y in EST_Y], "mean_x": bits(mx), "mean_y": bits(my),
            "s_xx": bits(sxx), "s_yy": bits(syy), "s_xy": bits(sxy), "r": bits(pearson(EST_X, EST_Y)),
            "slope": bits(slope(EST_X, EST_Y)), "variance_x": bits(variance(EST_X)), "variance_y": bits(variance(EST_Y)),
        },
        "fisher": [{"r": bits(r), "F": bits(fisher(r))} for r in EST_FISHER],
        "median": {"hist": {str(k): v for k, v in EST_HIST.items()}, "never": EST_NEVER, "median": bits(med),
                   "se": bits(km_median_se(hist, EST_NEVER, med))},
        "life_table": {"E": EST_LIFE_E, "D": EST_LIFE_D, "e0": bits(table["e0"]), "var_e0": bits(table["var_e0"]),
                       "se": bits(table["se"]), "q": [bits(q) for q in table["q"]], "l": [bits(l) for l in table["l"]],
                       "e": [bits(e) for e in table["e"]]},
        "births": {"values": EST_BIRTHS, "mean": bits(mb), "variance": bits(vb), "se": bits(mean_se(by))},
        "share": {"k": 7, "n": 1234, "value": bits(share(7, 1234))},
        "p9_expectation": {str(n): bits(p9_expectation(n)) for n in range(1, 6)},
        "phi": {repr(x): bits(phi(x)) for x in EST_PHI},
        "power": [{"c": bits(c), "tol": bits(t), "alt": bits(a), "se": bits(s), "power": bits(power(c, t, a, s))} for c, t, a, s in EST_POWER],
        "w": [{"label": label, "value": bits(value), "w_key": key, "w": spec_value(key)} for label, value, key in assert_w_values()],
        "tolerances_at_min": {"p3": bits(tol_p3(MIN_P3)), "p4": bits(tol_log_var(MIN_P4)), "fisher": bits(tol_fisher(MIN_FISHER)),
                              "mc1r": bits(tol_mc1r([p9_expectation(1)] * MIN_MC1R)), "never_n_100": bits(tol_never(100))},
        "permutations": perms,
        "splitmix_first": first_words,
        "village": {
            "blocks": [list(b) for b in EST_VILLAGE_BLOCKS], "n_e": [bits(n_e(m, f)) for _, m, f in EST_VILLAGE_BLOCKS],
            "expectation": bits(village_expectation(EST_VILLAGE_BLOCKS)),
            "v": [bits(v) for v in EST_VILLAGE_V], "e": [bits(e) for e in EST_VILLAGE_E],
            "aggregate": {k: (bits(v) if isinstance(v, float) else v) for k, v in village.items()},
            "decision_at_2100": village_at_end["decision"],
        },
        "village_drifted": {
            "v": [bits(v) for v in EST_VILLAGE_V2], "e": [bits(e) for e in EST_VILLAGE_E2],
            "aggregate": {k: (bits(v) if isinstance(v, float) else v) for k, v in drifted.items()},
        },
        "probit_rows_differing_from_core": check_probit(),
        **review_round_1_vectors(),
        "life_table_edges": life_table_edges(),
        "life_table_out_of_domain": life_table_out_of_domain(),
    }


# ---------------------------------------------------------------------------------------
# Fixture 2: the hand pedigree (note section 7.2). Rows written by hand; the expected units
# (`HAND_UNITS`) derived by hand from section 4 and checked against the selection. NOTES.md
# walks through each derivation. Genomes are placeholders (zero words; the criteria read
# `mc1r`, `z` and `gl` only, the recomputation does not apply).

Y, M = YEAR, MONTH
END_HAND = 200 * Y - 1


def r_key(n: int) -> int:
    """A pool ancestor record's key (never a row)."""
    return 0xF000_0000_0000_0000 + n


# (name, key, sex, born, died, emig, mother, father, pool, arrived, mult, brought, gkey,
#  z_h, z_pig, z_K, gl, mc1r); mother and father as keys (records via r_key), gkey None = key.
HAND_ROWS = [
    # Arrivals at second 0 (pool 1), parents records.
    ("M1", 0x0101, "f", -25 * Y, 80 * Y, None, r_key(0x01), r_key(0x02), 1, 0, None, 0, None, 0.42, -0.31, 0.15, 0.27, 1),
    ("P1", 0x0102, "m", -27 * Y, 75 * Y, None, r_key(0x03), r_key(0x04), 1, 0, None, 0, None, 1.05, 0.22, -0.48, -0.66, 1),
    ("M2", 0x0103, "f", -24 * Y, 82 * Y, None, r_key(0x05), r_key(0x06), 1, 0, None, 0, None, -0.73, 1.12, 0.61, 0.05, 0),
    ("P2", 0x0104, "m", -26 * Y, 77 * Y, None, r_key(0x07), r_key(0x08), 1, 0, None, 0, None, -0.12, -0.95, 0.33, 1.21, 0),
    ("M3", 0x0105, "f", -22 * Y, 85 * Y, None, r_key(0x09), r_key(0x0A), 1, 0, None, 0, None, 0.88, 0.41, -1.02, -0.34, 1),
    ("P3", 0x0106, "m", -25 * Y, 70 * Y, None, r_key(0x0B), r_key(0x0C), 1, 0, None, 0, None, 0.19, -0.27, 0.74, 0.58, 0),
    ("P4", 0x0107, "m", -24 * Y, 70 * Y, None, r_key(0x0D), r_key(0x0E), 1, 0, None, 0, None, -1.36, 0.63, 0.09, -1.10, 0),
    ("M5", 0x0108, "f", -23 * Y, 81 * Y, None, r_key(0x0F), r_key(0x10), 1, 0, None, 0, None, 0.07, -1.48, 1.33, 0.92, 1),
    ("P5", 0x0109, "m", -26 * Y, 72 * Y, None, r_key(0x11), r_key(0x12), 1, 0, None, 0, None, 0.55, 0.81, -0.26, -0.17, 1),
    ("M6", 0x010A, "f", -21 * Y, 79 * Y, None, r_key(0x13), r_key(0x14), 1, 0, None, 0, None, -0.44, 0.36, -0.71, 0.44, 2),
    ("P6", 0x010B, "m", -23 * Y, 74 * Y, None, r_key(0x15), r_key(0x16), 1, 0, None, 0, None, 1.27, -0.58, 0.47, -0.83, 1),
    ("M7", 0x010C, "f", -17 * Y, 90 * Y, None, r_key(0x17), r_key(0x18), 1, 0, None, 0, None, -0.91, 0.12, 0.28, 0.71, 0),
    ("M8", 0x010D, "f", -24 * Y, 80 * Y, None, r_key(0x1A), r_key(0x1B), 1, 0, None, 0, None, 0.33, -0.66, -0.39, -0.29, 0),
    ("M9", 0x010E, "f", -22 * Y, 80 * Y, None, r_key(0x1A), r_key(0x1B), 1, 0, None, 0, None, 0.61, -0.21, 0.86, 0.13, 0),
    ("P8", 0x010F, "m", -26 * Y, 75 * Y, None, r_key(0x1C), r_key(0x1D), 1, 0, None, 0, None, -0.25, 0.97, -1.17, 0.36, 0),
    ("P9", 0x0110, "m", -25 * Y, 75 * Y, None, r_key(0x1E), r_key(0x1F), 1, 0, None, 0, None, 0.94, 0.04, 0.52, -0.58, 0),
    ("W1", 0x0111, "f", -30 * Y, 60 * Y, None, r_key(0x20), r_key(0x21), 1, 0, None, 2, None, -0.18, 0.73, -0.64, 1.02, 0),
    ("P10", 0x0112, "m", -32 * Y, 55 * Y, None, r_key(0x22), r_key(0x23), 1, 0, None, 0, None, 0.29, -1.13, 0.18, -0.47, 0),
    ("W2", 0x0113, "f", -41 * Y, 70 * Y, None, r_key(0x24), r_key(0x25), 1, 10 * Y, None, 3, None, 1.41, 0.26, -0.09, 0.19, 0),
    # The pool child of M7 (pool 2), its father a record only.
    ("K1", 0x0631, "f", 5 * M, 85 * Y, None, 0x010C, r_key(0x19), 2, None, 0, 0, None, -0.62, 0.49, -0.13, 0.31, 0),
    # Town-born, second generation.
    ("C1a", 0x0301, "m", 2 * Y, 85 * Y, None, 0x0101, 0x0102, 0, None, 0, 0, None, 0.81, -0.12, 0.37, 0.48, 2),
    ("C1b", 0x0302, "m", 2 * Y, 84 * Y, None, 0x0101, 0x0102, 0, None, 1, 0, 0x0301, 0.77, -0.15, 0.29, 0.48, 2),
    ("C1c", 0x0303, "f", 5 * Y, 83 * Y, None, 0x0101, 0x0102, 0, None, 0, 0, None, 0.36, 0.08, -0.54, -0.37, 0),
    ("D1", 0x0401, "m", 3 * Y, 88 * Y, None, 0x0103, 0x0104, 0, None, 1, 0, None, -0.49, 0.64, 0.71, 0.86, 0),
    ("D0", 0x0402, "f", 3 * Y, 84 * Y, None, 0x0103, 0x0104, 0, None, 0, 0, None, -0.21, 0.93, 0.24, -0.15, 0),
    ("D2", 0x0403, "m", 26 * Y, 80 * Y, None, 0x0103, 0x0104, 0, None, 0, 0, None, -0.68, 0.17, -0.42, 0.59, 0),
    ("E1", 0x0501, "m", 2 * Y, 81 * Y, None, 0x0105, 0x0106, 0, None, 0, 0, None, 0.63, 0.29, -0.81, -0.62, 0),
    ("E2", 0x0502, "f", 8 * Y, 87 * Y, None, 0x0105, 0x0107, 0, None, 0, 0, None, -0.35, 0.54, 0.46, 0.21, 0),
    ("G1", 0x0601, "f", 6 * Y, None, 24 * Y - 1, 0x0108, 0x0109, 0, None, 0, 0, None, 0.24, -0.97, 0.98, 0.67, 0),
    ("G2", 0x0602, "m", 9 * Y, 79 * Y, None, 0x0108, 0x0109, 0, None, 0, 0, None, 0.47, -0.36, 0.12, -0.44, 2),
    ("G3", 0x0603, "f", 12 * Y, 86 * Y, None, 0x0108, 0x0109, 0, None, 0, 0, None, 0.18, -0.71, 0.57, 0.39, 1),
    ("L1", 0x0701, "m", 5 * Y, 83 * Y, None, 0x010D, 0x010F, 0, None, 0, 0, None, 0.11, 0.46, -0.77, 0.12, 0),
    ("L2", 0x0702, "f", 6 * Y, 84 * Y, None, 0x010E, 0x0110, 0, None, 0, 0, None, 0.72, -0.38, 0.65, -0.09, 0),
    ("N1", 0x0711, "m", 3 * Y, 82 * Y, None, 0x0111, 0x0112, 0, None, 0, 0, None, 0.05, 0.12, -0.23, 0.74, 0),
    ("H1", 0x0751, "f", 9 * Y, 27 * Y, None, 0x010A, 0x010B, 0, None, 0, 0, None, -0.15, -0.09, -0.48, -0.53, 2),
    ("H2", 0x0752, "m", 11 * Y, 86 * Y, None, 0x010A, 0x010B, 0, None, 0, 0, None, 0.98, -0.43, 0.81, 0.28, 1),
    # Town-born, third generation.
    ("S1", 0x0801, "m", 28 * Y, 92 * Y, None, 0x0303, 0x0701, 0, None, 0, 0, None, 0.26, 0.69, -0.61, -0.18, 0),
    ("S2", 0x0802, "f", 25 * Y, 85 * Y, None, 0x0402, 0x0501, 0, None, 0, 0, None, 0.14, 0.55, 0.38, 0.45, 0),
    ("S4", 0x0803, "f", 33 * Y, 70 * Y, None, 0x0502, 0x0401, 0, None, 0, 0, None, -0.82, 0.21, 0.69, -0.71, 0),
    ("S3", 0x0804, "m", 27 * Y, 96 * Y, None, 0x0702, 0x0711, 0, None, 0, 0, None, 0.39, -0.24, 0.07, 0.36, 0),
    # Arrivals of the life table (period [100, 200) years).
    ("T1p", 0x0901, "f", 70 * Y + 6 * M, 100 * Y + 10 * M, None, r_key(0x26), r_key(0x27), 1, 95 * Y, None, 0, None, 0.12, -0.44, 0.63, -0.21, 0),
    ("T2p", 0x0902, "m", 4 * Y, 107 * Y + 2 * M, None, r_key(0x28), r_key(0x29), 1, 99 * Y, None, 0, None, -0.57, 0.31, -0.88, 0.84, 0),
    ("T3p", 0x0903, "m", 150 * Y, None, None, r_key(0x2A), r_key(0x2B), 1, 190 * Y, None, 0, None, 0.76, -0.17, 1.09, -0.95, 0),
    ("T4p", 0x0904, "f", 60 * Y, None, 130 * Y - 1, r_key(0x2C), r_key(0x2D), 1, 120 * Y, None, 0, None, -1.04, 0.58, -0.35, 0.62, 0),
]

# (woman, man, formed, carried), in couple order.
HAND_COUPLES = [
    (0x0111, 0x0112, -10 * Y, True),
    (0x0108, 0x0109, -3 * Y, True),
    (0x0101, 0x0102, -2 * Y, True),
    (0x0105, 0x0106, -1 * Y, True),
    (0x0103, 0x0104, 1 * Y, False),
    (0x010A, 0x010B, 1 * Y, False),
    (0x010D, 0x010F, 2 * Y, False),
    (0x010E, 0x0110, 3 * Y, False),
    (0x0105, 0x0107, 5 * Y, False),
    (0x0402, 0x0501, 22 * Y, False),
    (0x0702, 0x0711, 24 * Y, False),
    (0x0303, 0x0701, 25 * Y, False),
    (0x0502, 0x0401, 30 * Y, False),
    (0x0631, 0x0401, 45 * Y, False),
    (0x0603, 0x0752, 62 * Y, False),
]

# Derived by hand from section 4 (NOTES.md, "The hand pedigree").
HAND_UNITS = {
    "families": [
        (0x0301, 0x0101, 0x0102), (0x0402, 0x0103, 0x0104), (0x0501, 0x0105, 0x0106), (0x0502, 0x0105, 0x0107),
        (0x0601, 0x0108, 0x0109), (0x0701, 0x010D, 0x010F), (0x0702, 0x010E, 0x0110), (0x0711, 0x0111, 0x0112),
        (0x0751, 0x010A, 0x010B), (0x0801, 0x0303, 0x0701), (0x0802, 0x0402, 0x0501), (0x0803, 0x0502, 0x0401),
        (0x0804, 0x0702, 0x0711),
    ],
    "p5_siblings": [(0x0301, 0x0303), (0x0402, 0x0401), (0x0601, 0x0602), (0x0751, 0x0752)],
    "p5_strangers": [(0x0301, 0x0402), (0x0501, 0x0601), (0x0701, 0x0711), (0x0751, 0x0801), (0x0802, 0x0804)],
    "p9_polygenic": [
        (0x0101, 0x0301), (0x0104, 0x0402), (0x0105, 0x0501), (0x0107, 0x0502), (0x0108, 0x0601), (0x010F, 0x0701),
        (0x010E, 0x0702), (0x0112, 0x0711), (0x010A, 0x0751), (0x0701, 0x0801), (0x0402, 0x0802), (0x0401, 0x0803),
        (0x0702, 0x0804),
    ],
    "p9_mc1r": {"sample": [(0x0301, 1, 2), (0x0602, 1, 3), (0x0751, 0, 2)], "parents": 25, "copies": 8},
    "p4_city": {
        0: [0x0301, 0x0402, 0x0501, 0x0631, 0x0701, 0x0702, 0x0711, 0x0751],
        1: [0x0603, 0x0752],
        2: [0x0403, 0x0801, 0x0802, 0x0804],
        3: [0x0803],
    },
    "f11_pairs": [(0x0402, 0x0501), (0x0702, 0x0711), (0x0303, 0x0701), (0x0502, 0x0401), (0x0603, 0x0752)],
    "f11_first_partnership_f": [(0x0303, 260), (0x0402, 247), (0x0502, 286), (0x0603, None), (0x0631, 580), (0x0702, 234), (0x0802, None)],
    "f11_median_f": 22.0,
    "f11_first_partnership_m": [
        (0x0301, None), (0x0302, None), (0x0401, 351), (0x0403, None), (0x0501, 260), (0x0602, None), (0x0701, 260),
        (0x0711, 273), (0x0752, None), (0x0801, None), (0x0804, None),
    ],
    "f11_median_m": None,
    "f11_births": [
        (0x0101, 3), (0x0103, 2), (0x0105, 2), (0x0108, 3), (0x010A, 2), (0x010C, 1), (0x010D, 1), (0x010E, 1), (0x0111, 3),
        (0x0303, 1), (0x0402, 1), (0x0502, 1), (0x0603, 0), (0x0631, 0), (0x0702, 1), (0x0802, 0),
    ],
    "f11_early_births": {
        "units": [0x0301, 0x0302, 0x0303, 0x0401, 0x0402, 0x0403, 0x0501, 0x0502, 0x0601, 0x0602, 0x0603, 0x0631, 0x0701,
                  0x0702, 0x0711, 0x0751, 0x0752, 0x0801, 0x0802, 0x0803, 0x0804],
        "early": [0x0631],
    },
    "f11_gap": [(0x0103, 0x0104), (0x010A, 0x010B), (0x010D, 0x010F), (0x010E, 0x0110), (0x0105, 0x0107), (0x0402, 0x0501),
                (0x0702, 0x0711), (0x0303, 0x0701), (0x0502, 0x0401), (0x0631, 0x0401), (0x0603, 0x0752)],
    "f11_gap_years_times_13": [26, 26, 26, 39, 26, 13, 39, 0, 65, -34, 13],
    "f11_life_table": {
        "f": {"E": {29: 6 * M, 30: 4 * M, **{x: Y for x in range(60, 69)}, 69: Y - 1}, "D": {30: 1}, "rows": 22},
        "m": {"E": {**{x: Y for x in range(40, 50)}, **{x: Y for x in range(96, 100)}, 100: 3 * Y + 2 * M}, "D": {100: 1}, "rows": 22},
    },
    "f11_h2t": {
        "f": {"units": [0x0303, 0x0402, 0x0502, 0x0603, 0x0631, 0x0702, 0x0751, 0x0802, 0x0803], "emigrated": 1, "living": 0},
        "m": {"units": [0x0301, 0x0302, 0x0401, 0x0403, 0x0501, 0x0602, 0x0701, 0x0711, 0x0752, 0x0801, 0x0804], "emigrated": 0, "living": 0},
    },
    "village": {"end_years": 200, "g_star": 5, "units": [], "blocks": [(0, 13, 12), (1, 1, 1)], "living": 1},
}


def hand_export() -> dict:
    persons = []
    for name, key, sex, born, died, emig, mother, father, pool, arrived, mult, brought, gkey, zh, zp, zk, gl, mc1r in HAND_ROWS:
        genome = {"m": [hx(0)] * 3, "p": [hx(0)] * 3, "x": 0, "px": 0 if sex == "f" else None, "y": None if sex == "f" else 0, "mt": 0}
        persons.append({
            "key": key, "gkey": key if gkey is None else gkey, "sex": sex, "born": born, "died": died, "emig": emig,
            "mother": mother, "father": father, "pool": pool, "arrived": arrived, "mult": mult, "brought": brought,
            "genome": genome, "z": [zh, zp, zk], "gl": gl, "mc1r": mc1r,
        })
    persons.sort(key=lambda p: p["key"])
    couples = [{"woman": w, "man": m, "formed": f, "carried": c} for w, m, f, c in HAND_COUPLES]
    order = [(c["formed"], c["woman"], c["man"]) for c in couples]
    assert order == sorted(order), "the hand couples are written in couple order"
    header = {
        "format": FORMAT, "version": VERSION, "core_version": wr.CORE_VERSION, "data_hash": 0x0000_0000_0000_0000,
        "world_seed": 0x0000_0000_0000_0007, "pool_seed": 0x0000_0000_11B0_19A2, "end": END_HAND, "year": YEAR, "month": MONTH,
        "traits": [list(t) for t in TRAITS], "longevity": LONGEVITY, "mc1r": MC1R, "persons": len(persons), "couples": len(couples),
    }
    return {"header": header, "persons": persons, "couples": couples}


def hand_selected(rows: Rows) -> dict:
    """The selection of section 4 on the hand rows, in the form of `HAND_UNITS`."""
    fams = families(rows)
    mc = p9_mc1r_units(fams)
    fp = {sex: f11_first_partnership(rows, sex) for sex in ("f", "m")}
    lt = {sex: life_table_input(rows, sex) for sex in ("f", "m")}
    h2 = {sex: h2t_sample(rows, sex) for sex in ("f", "m")}
    vb = village_block(rows)
    medians = {sex: km_median(fp[sex]["hist"], fp[sex]["never"]) for sex in ("f", "m")}
    return {
        "families": [(f["first"]["key"], f["mother"]["key"], f["father"]["key"]) for f in fams],
        "p5_siblings": [(a["key"], b["key"]) for a, b in p5_sibling_pairs(fams)],
        "p5_strangers": [(a["key"], b["key"]) for a, b in p5_stranger_pairs(rows, fams)],
        "p9_polygenic": [(a["key"], b["key"]) for a, b in p9_polygenic_pairs(fams)],
        "p9_mc1r": {"sample": [(s["anchor"], s["y"], s["n"]) for s in mc["sample"]], "parents": mc["parents"], "copies": mc["copies"]},
        "p4_city": {k: [p["key"] for p in units] for k, units in enumerate(p4_city_units(rows)) if units},
        "f11_pairs": [(w["key"], m["key"]) for w, m in f11_pairs(rows)],
        "f11_first_partnership_f": list(zip(fp["f"]["units"], fp["f"]["months"])),
        "f11_median_f": medians["f"],
        "f11_first_partnership_m": list(zip(fp["m"]["units"], fp["m"]["months"])),
        "f11_median_m": medians["m"],
        "f11_births": f11_births(rows),
        "f11_early_births": f11_early_births(rows),
        "f11_gap": [(c["woman"], c["man"]) for c in f11_gap_couples(rows)],
        "f11_gap_years_times_13": [(rows.by_key[c["woman"]]["born"] - rows.by_key[c["man"]]["born"]) // MONTH for c in f11_gap_couples(rows)],
        "f11_life_table": {
            sex: {"E": {x: e for x, e in enumerate(lt[sex]["E"]) if e}, "D": {x: d for x, d in enumerate(lt[sex]["D"]) if d}, "rows": len(lt[sex]["units"])}
            for sex in ("f", "m")
        },
        "f11_h2t": {sex: {"units": [p["key"] for p in h2[sex]["units"]], "emigrated": h2[sex]["emigrated"], "living": h2[sex]["living"]} for sex in ("f", "m")},
        "village": {"end_years": vb["end_years"], "g_star": vb["g_star"], "units": [p["key"] for p in vb["units"]], "blocks": vb["blocks"], "living": vb["living"]},
    }


def json_keys(value):
    """`HAND_UNITS` as JSON: keys as 16 hex digits, tuples as lists, dict keys as strings."""
    if isinstance(value, dict):
        return {str(k): json_keys(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [json_keys(v) for v in value]
    return value


def hand_units_json() -> dict:
    def keyed(name, value):
        if name in ("f11_median_f", "f11_median_m"):
            return None if value is None else bits(value)
        if name == "f11_gap_years_times_13":
            return value
        if name in ("p9_mc1r",):
            return {"sample": [[hx(a), y, n] for a, y, n in value["sample"]], "parents": value["parents"], "copies": value["copies"]}
        if name.startswith("f11_first_partnership") or name == "f11_births":
            return [[hx(k), m] for k, m in value]
        if name == "f11_life_table":
            return {sex: {"E": {str(x): e for x, e in v["E"].items()}, "D": {str(x): d for x, d in v["D"].items()}, "rows": v["rows"]} for sex, v in value.items()}
        if name == "f11_h2t":
            return {sex: {"units": [hx(k) for k in v["units"]], "emigrated": v["emigrated"], "living": v["living"]} for sex, v in value.items()}
        if name == "village":
            return {"end_years": value["end_years"], "g_star": value["g_star"], "units": [hx(k) for k in value["units"]],
                    "blocks": [list(b) for b in value["blocks"]], "living": value["living"]}
        if name == "p4_city":
            return {str(k): [hx(x) for x in v] for k, v in value.items()}
        if name == "f11_early_births":
            return {"units": [hx(k) for k in value["units"]], "early": [hx(k) for k in value["early"]]}
        return [[hx(k) for k in item] for item in value]

    out = {"format": "simagent.criteria.hand_units", "version": 1}
    for name, value in HAND_UNITS.items():
        out[name] = keyed(name, value)
    return out


def check_hand(rows: Rows) -> None:
    """The selection equals the units derived by hand, entry by entry."""
    got = hand_selected(rows)
    for name, expected in HAND_UNITS.items():
        value = got[name]
        if name in ("f11_median_f", "f11_median_m"):
            assert (value is None and expected is None) or bits(value) == bits(expected), (name, value, expected)
            continue
        assert json_keys(value) == json_keys(expected), (name, value, expected)


# ---------------------------------------------------------------------------------------
# Fixture 3: the small pool towns end to end (note section 7.3)

TOWNS = {
    # name: (pool, world seed, n_start, immigration, switch, end)
    "t1_pool_d": ("pool_d", 6, 30, False, True, 60 * YEAR - 1),
    "t2_pool_c": ("pool_c", 0xFF, 50, True, True, 14 * YEAR - 1),
}


def town_world(name: str):
    pool_name, seed, n_start, immigration, switch, end = TOWNS[name]
    # The documents its pool was extracted from (design note of the pool's records, section 6):
    # the documents of father_dies for pool_c, the template for pool_d.
    documents = ar.pool_documents(pool_name)
    # With the frequencies the pool was made with (slice S2a): those of `frequent_alleles`
    # for pool_d, the file's for pool_c.
    world = ar.make_world(ar.town_data(ar.golden_pool(pool_name), documents, ar.pool_frequencies(pool_name)), seed, n_start, immigration, switch)
    for year in range((end + 1) // YEAR):
        world.advance(min((year + 1) * YEAR - 1, end))
    assert world.closed == end
    return world, documents


def assert_village_h2(documents: tuple) -> None:
    """K4.2a: `h²` = 0.8 equals the data's child and adult `h²` of `core:growth`."""
    import data_reference

    (row,) = [r for r in data_reference.strip(documents[1])["traits"] if r["trait"] == "core:growth"]
    assert wr.number(row["h2_child"]) == VILLAGE_H2 == wr.number(row["h2_adult"])


# ---------------------------------------------------------------------------------------
# A self-test at full sample sizes (not a pin): synthetic families from the model of 4.3,
# so that each check is green and its twin red for the right reason, not only below the
# minimum. Python's `random` with a fixed seed; nothing here is written to a fixture.


def synthetic_export(families_n: int = 12_000, seed: int = 20261016) -> dict:
    import random

    rng = random.Random(seed)
    persons, couples = [], []

    def mc1r_copies() -> int:
        return (1 if rng.random() < P9_Q else 0) + (1 if rng.random() < P9_Q else 0)

    def gamete(copies: int) -> int:
        return 1 if (copies == 2 or (copies == 1 and rng.random() < 0.5)) else 0

    genomes = {"f": {"m": [hx(0)] * 3, "p": [hx(0)] * 3, "x": 0, "px": 0, "y": None, "mt": 0},
               "m": {"m": [hx(0)] * 3, "p": [hx(0)] * 3, "x": 0, "px": None, "y": 0, "mt": 0}}
    genome = genomes["f"]
    for i in range(families_n):
        mother_key, father_key = 0x1000_0000 + 2 * i, 0x1000_0001 + 2 * i
        zm = rng.gauss(0.0, 1.0)
        zf = 0.2 * zm + math.sqrt(0.96) * rng.gauss(0.0, 1.0)
        pm, pf = rng.gauss(0.0, 1.0), rng.gauss(0.0, 1.0)
        cm, cf = mc1r_copies(), mc1r_copies()
        for key, sex, z, zp, c in ((mother_key, "f", zm, pm, cm), (father_key, "m", zf, pf, cf)):
            persons.append({"key": key, "gkey": key, "sex": sex, "born": -25 * YEAR, "died": None, "emig": None,
                            "mother": None, "father": None, "pool": 1, "arrived": 0, "mult": None, "brought": 0,
                            "genome": genomes[sex], "z": [z, zp, 0.0], "gl": 0.0, "mc1r": c})
        couples.append({"woman": mother_key, "man": father_key, "formed": -2 * YEAR, "carried": True})
        mp = (zm + zf) / 2.0
        shared = rng.gauss(0.0, 1.0)
        born = (i % 180) * YEAR + (i // 180) % 5 * MONTH
        for j in range(2):
            e = math.sqrt(0.13) * shared + math.sqrt(0.616 - 0.13) * rng.gauss(0.0, 1.0)
            zp = 0.4 * (pm + pf) + math.sqrt(1.0 - 0.32) * rng.gauss(0.0, 1.0)
            persons.append({"key": 0x5000_0000 + 2 * i + j, "gkey": 0x5000_0000 + 2 * i + j, "sex": "f", "born": born + j * 2 * YEAR,
                            "died": None, "emig": None, "mother": mother_key, "father": father_key, "pool": 0, "arrived": None,
                            "mult": 0, "brought": 0, "genome": genome, "z": [0.8 * mp + e, zp, 0.0], "gl": 0.0,
                            "mc1r": gamete(cm) + gamete(cf)})
    persons.sort(key=lambda p: p["key"])
    header = {"format": FORMAT, "version": VERSION, "core_version": wr.CORE_VERSION, "data_hash": 0, "world_seed": seed,
              "pool_seed": None, "end": 200 * YEAR - 1, "year": YEAR, "month": MONTH, "traits": [list(t) for t in TRAITS],
              "longevity": LONGEVITY, "mc1r": MC1R, "persons": len(persons), "couples": len(couples)}
    return {"header": header, "persons": persons, "couples": couples}


def self_test() -> list:
    """Every family check and P4 city green on the synthetic town, every twin red."""
    rep = report(synthetic_export(), b"")
    by = {c["check"]: c for c in rep["checks"]}
    out = []
    for name in ("p3", "p5_siblings", "p5_strangers", "p9_polygenic", "p9_mc1r"):
        c = by[name]
        assert c["pass"] and not c["twin"]["pass"], (name, c)
        out.append(f"self-test {name}: n {c['n']} value {from_bits(c['value']):.4f} green, twin {from_bits(c['twin']['value']):.4f} red")
    p4 = by["p4_city"]
    assert p4["pass"] and not p4["twin_pass"], p4["decades"][0]
    reds = sum(1 for d in p4["decades"] if not d["twin"]["pass"])
    out.append(f"self-test p4_city: 18 decades green (n {min(d['n'] for d in p4['decades'])} to {max(d['n'] for d in p4['decades'])}), twin red in {reds} of 18")
    return out


# ---------------------------------------------------------------------------------------
# Writing the fixtures and the command line


def dump(path: Path, obj) -> None:
    path.write_text(json.dumps(obj, indent=1, ensure_ascii=True) + "\n")


def export_and_report(name: str, export: dict, out: Path) -> tuple:
    text = export_text(export)
    data = text.encode("ascii")
    back = read_export(text)
    assert export_text(back) == text, "the reader and the writer round-trip"
    rep = report(back, data)
    assert_not_fragile(rep)
    (out / f"{name}.ndjson").write_bytes(data)
    dump(out / f"{name}.report.json", rep)
    return back, rep


def check_reader() -> None:
    """One rejected export per rule of the reader (review of step 3b, export review round 4):
    each case changes one field or line of the hand export (the header, the arrivals 0x0101
    and 0x0102, the living arrival 0x0903, the pool child 0x0631, the town child 0x0301, the
    town man 0x0401, a couple) and must be rejected with that rule's message; the unchanged
    text is read."""
    text = export_text(hand_export())
    read_export(text)

    def header(change) -> str:
        lines = text.split("\n")
        h = json.loads(lines[0])
        change(h)
        lines[0] = line(h)
        return "\n".join(lines)

    def person(key: int, change) -> str:
        lines = text.split("\n")
        for i, raw in enumerate(lines):
            if raw.startswith('{"k":"p","key":"%s"' % hx(key)):
                obj = json.loads(raw)
                change(obj)
                lines[i] = line(obj)
                return "\n".join(lines)
        raise AssertionError(hx(key))

    def couple(index: int, change) -> str:
        lines = text.split("\n")
        at = 1 + len(HAND_ROWS) + index
        obj = json.loads(lines[at])
        assert obj["k"] == "c"
        change(obj)
        lines[at] = line(obj)
        return "\n".join(lines)

    def duplicated(kind: str) -> str:
        lines = text.split("\n")
        h = json.loads(lines[0])
        h[kind] += 1
        lines[0] = line(h)
        at = 1 if kind == "persons" else 1 + len(HAND_ROWS) + len(HAND_COUPLES) - 1
        lines.insert(at + 1, lines[at])
        return "\n".join(lines)

    def swapped_couples() -> str:
        lines = text.split("\n")
        a = 1 + len(HAND_ROWS)
        lines[a], lines[a + 1] = lines[a + 1], lines[a]
        return "\n".join(lines)

    def put(**fields):
        return lambda obj: obj.update(fields)

    def z_at(i: int, value):
        return lambda obj: obj["z"].__setitem__(i, value)

    def genome(**fields):
        return lambda obj: obj["genome"].update(fields)

    cognition = list(TRAITS[2])
    cases = [
        ("traits", header(lambda h: h.update(traits=[list(TRAITS[1]), list(TRAITS[0]), cognition]))),
        ("traits", header(lambda h: h.update(traits=[list(TRAITS[0]), list(TRAITS[1]), [TRAITS[2][0], TRAITS[2][1] - 1]]))),
        ("longevity", header(lambda h: h.update(longevity="core:lifespan"))),
        ("mc1r", header(lambda h: h.update(mc1r="core:herc2"))),
        ("pool", person(0x0101, put(pool=3))),
        ("mc1r", person(0x0101, put(mc1r=3))),
        ("died and emigrated", person(0x0101, put(emig=100 * YEAR - 1))),
        ("arrived iff pool 1", person(0x0101, put(arrived=None))),
        ("arrived iff pool 1", person(0x0301, put(arrived=0))),
        ("brought of a town-born person", person(0x0301, put(brought=1))),
        ("brought of a town-born person", person(0x0631, put(brought=1))),
        ("mult of an arrival", person(0x0101, put(mult=0))),
        ("mult of a pool child", person(0x0631, put(mult=None))),
        ("mult iff mother of a town person", person(0x0301, put(mult=None))),
        ("mult iff mother of a town person", person(0x0301, put(mother=None, father=None))),
        ("paternal X of a woman", person(0x0101, genome(px=None, y=0))),
        ("Y of a man", person(0x0401, genome(px=0, y=None))),
        # Export review round 4: unsigned integers (0 to 2^64 − 1, never a bool or a float).
        ("unsigned version", header(put(version=True))),
        ("version", header(put(version=2))),
        ("unsigned core_version", header(put(core_version=-1))),
        ("unsigned persons", header(put(persons=float(len(HAND_ROWS))))),
        ("unsigned couples", header(put(couples=1 << 64))),
        ("unsigned brought", person(0x0101, put(brought=-1))),
        ("unsigned mult", person(0x0301, put(mult=0.0))),
        ("unsigned pool", person(0x0101, put(pool=256))),
        ("unsigned mc1r", person(0x0101, put(mc1r=True))),
        ("unsigned x", person(0x0101, genome(x=-1))),
        ("unsigned px", person(0x0101, genome(px=1.0))),
        ("unsigned y", person(0x0102, genome(y=1 << 64))),
        ("unsigned mt", person(0x0101, genome(mt=False))),
        # Signed integers (−2^63 to 2^63 − 1, never a bool or a float).
        ("signed end", header(put(end=1.5))),
        ("signed year", header(put(year=float(YEAR)))),
        ("year", header(put(year=YEAR + 1))),
        ("signed month", header(put(month=True))),
        ("month", header(put(month=MONTH + 1))),
        ("signed trait age", header(put(traits=[["core:growth", False], list(TRAITS[1]), cognition]))),
        ("signed born", person(0x0101, put(born=1 << 63))),
        ("signed died", person(0x0101, put(died=True))),
        ("signed emig", person(0x0903, put(emig=1.0))),
        ("signed arrived", person(0x0101, put(arrived="0"))),
        ("signed formed", couple(0, put(formed=-(1 << 63) - 1))),
        # Hex: a string of exactly 16 characters 0-9a-f.
        ("hex key", person(0x0101, put(key="000000000000101"))),
        ("hex gkey", person(0x0101, put(gkey="00000000000001010"))),
        ("hex mother", person(0x0301, put(mother=0x0101))),
        ("hex father", person(0x0301, put(father="000000000000010g"))),
        ("hex data_hash", header(put(data_hash="000000000000000A"))),
        ("hex world_seed", header(put(world_seed="00000000000000007"))),
        ("hex pool_seed", header(put(pool_seed=0))),
        ("hex genome word", person(0x0101, lambda obj: obj["genome"]["m"].__setitem__(0, "0x00000000000000"))),
        ("hex z", person(0x0101, z_at(1, "BFD3D70A3D70A3D7"))),
        ("hex gl", person(0x0101, put(gl=0.27))),
        ("hex woman", couple(0, put(woman="111"))),
        ("hex man", couple(0, put(man=None))),
        # Lists, the bool, k and sex.
        ("z", person(0x0101, lambda obj: obj["z"].pop())),
        ("z", person(0x0101, put(z="3fdae147ae147ae1"))),
        ("genome keys", person(0x0101, put(genome=[]))),
        ("genome m", person(0x0101, lambda obj: obj["genome"]["m"].pop())),
        ("genome p", person(0x0101, lambda obj: obj["genome"]["p"].append("0000000000000000"))),
        ("bool carried", couple(0, put(carried=1))),
        ("bool carried", couple(0, put(carried="true"))),
        ("k", person(0x0101, put(k="c"))),
        ("k", couple(0, put(k="p"))),
        ("sex", person(0x0101, put(sex="x"))),
        # Order: persons strictly ascending by key, couples strictly by (formed, woman, man).
        ("persons strictly ascending by key", duplicated("persons")),
        ("couples strictly ascending by (formed, woman, man)", duplicated("couples")),
        ("couples strictly ascending by (formed, woman, man)", swapped_couples()),
    ]
    rules = {rule for rule, _ in cases}
    for rule, bad in cases:
        try:
            read_export(bad)
        except ExportError as error:
            assert str(error) == rule, (rule, str(error))
        else:
            raise AssertionError(f"accepted: {rule}")
    assert len(cases) == 66 and len(rules) == 57, (len(cases), len(rules))


def check_domain() -> None:
    """Review round 4: `E = [2^53]`, `D = [1]` is a table; `E = [2^53 + 1]` stops with
    `criteria.exact` (an assertion, never a finding); so do a share, a histogram count, a
    village count and a span above 2^53. Review round 5: the count is converted before a
    constant is subtracted, so `tol_log_var(2^53 + 1)`, `tol_fisher(2^53 + 3)` and their
    powers stop too."""
    assert life_table([EXACT_LIMIT], [1]) is not None
    stops = [
        lambda: life_table([EXACT_LIMIT + 1], [1]),
        lambda: life_table([YEAR, YEAR], [1, EXACT_LIMIT + 1]),
        lambda: share(1, EXACT_LIMIT + 1),
        lambda: km_median([0] * HIST_MONTHS, EXACT_LIMIT + 1),
        lambda: n_e(2**26, 2**26),
        lambda: exact(-(EXACT_LIMIT + 1)),
        lambda: tol_log_var(EXACT_LIMIT + 1),
        lambda: tol_fisher(EXACT_LIMIT + 3),
        lambda: power_p4(EXACT_LIMIT + 1),
        lambda: power_fisher(TARGET_P9, 0.2, EXACT_LIMIT + 3),
    ]
    for stop in stops:
        try:
            stop()
        except AssertionError as error:
            assert str(error).startswith("criteria.exact"), error
        else:
            raise AssertionError("a value above 2^53 did not stop")


def write_fixtures(out: Path) -> list:
    out.mkdir(parents=True, exist_ok=True)
    check_reader()
    check_domain()
    lines = []
    est = estimator_vectors()
    dump(out / "estimators.json", est)
    lines.append(f"estimators: probit rows differing from the core {est['probit_rows_differing_from_core']}")
    for row in est["w"]:
        lines.append(f"  W {row['label']:<20} {from_bits(row['value']):.6f} {row['w_key']} {row['w']}")
    # The hand pedigree.
    hand = hand_export()
    back, rep = export_and_report("hand", hand, out)
    check_hand(Rows(back))
    dump(out / "hand_units.json", hand_units_json())
    lines += summary_lines("hand", rep)
    index = {"hand": {"export": rep["export"], "end": END_HAND}}
    # The small pool towns.
    for name in TOWNS:
        world, documents = town_world(name)
        assert_village_h2(documents)
        export = export_world(world, TOWNS[name][5], documents)
        back, rep = export_and_report(name, export, out)
        rc = recompute_check(back, world, documents)
        assert compare_exports(export, back) == []
        pool_name, seed, n_start, immigration, switch, end = TOWNS[name]
        index[name] = {
            "export": rep["export"], "pool": pool_name, "pool_hash": hx(world.data.pool.hash), "data_hash": hx(world.data.hash),
            "world_seed": seed, "n_start": n_start, "immigration": immigration, "switch": switch, "end": end,
            "persons": rep["persons"], "couples": rep["couples"], "recompute": rc,
            "pools": {k: sum(1 for p in back["persons"] if p["pool"] == k) for k in (0, 1, 2)},
            "carried": sum(1 for c in back["couples"] if c["carried"]),
        }
        lines += summary_lines(name, rep)
        lines.append(f"  recompute: {rc}; pool kinds {index[name]['pools']}; carried couples {index[name]['carried']}")
    lines += self_test()
    dump(out / "index.json", index)
    (out / "summary.txt").write_text("\n".join(lines) + "\n")
    return lines


def main(argv: list) -> int:
    args = argv[1:]
    if not args:
        print(__doc__)
        return 2
    command = args[0]
    if command == "fixtures":
        out = Path(args[2]) if len(args) >= 3 and args[1] == "--out" else DEFAULT_OUT
        for text in write_fixtures(out):
            print(text)
        return 0
    if command == "export" and len(args) == 3:
        name = {"t1": "t1_pool_d", "t2": "t2_pool_c"}[args[1]]
        world, documents = town_world(name)
        Path(args[2]).write_text(export_text(export_world(world, TOWNS[name][5], documents)))
        return 0
    if command == "report" and len(args) in (2, 3):
        data = Path(args[1]).read_bytes()
        rep = report(read_export(data.decode("ascii")), data)
        if len(args) == 3:
            dump(Path(args[2]), rep)
        for text in summary_lines(args[1], rep):
            print(text)
        return 0
    if command == "compare" and len(args) == 3:
        a, b = (Path(x).read_text() for x in args[1:])
        if a.startswith('{"format":"simagent.criteria"'):
            problems = compare_exports(read_export(a), read_export(b))
        else:
            problems = compare_reports(json.loads(a), json.loads(b))
        for text in problems:
            print(text)
        print("equal" if not problems else f"{len(problems)} differences")
        return 0 if not problems else 1
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))

#!/usr/bin/env python3
"""Recomputes the derived values of docs/genetik-spezifikation.md (B13, section 1.4).

Every value the specification marks with an ID in square brackets, such as `0,25 [W2.6a]`,
is computed here from the formulas of the specification, independently of the Rust code.
Exact results are fractions; the rest uses mpmath with 40 digits. The script never runs
the simulation.

Usage:
  python3 tools/spec_reference.py                 print every ID with its value
  python3 tools/spec_reference.py --check FILE    compare FILE with the computed values

`--check` reads every `<number> [<ID>]` in FILE. It fails on a number that differs from the
computed one after rounding, on an ID that FILE uses but this script does not know, and on
an ID this script computes but FILE never uses.
"""

import math
import re
import sys
from fractions import Fraction

import mpmath as mp

mp.mp.dps = 40

VALUES = {}


def put(key, value, fmt):
    """Registers `value` under `key`; `fmt` is ("frac",), ("fix", n), ("sig", n) or ("int",)."""
    if key in VALUES:
        raise SystemExit(f"duplicate ID {key}")
    VALUES[key] = (value, fmt)


def german(text):
    return text.replace(".", ",")


def render(value, fmt):
    kind = fmt[0]
    if kind == "frac":
        frac = Fraction(value)
        return str(frac.numerator) if frac.denominator == 1 else f"{frac.numerator}/{frac.denominator}"
    if kind == "frac4":
        # A fraction with at most four decimals, written exactly.
        frac = Fraction(value)
        scaled = frac * 10000
        assert scaled.denominator == 1, frac
        return german(f"{float(frac):.4f}".rstrip("0").rstrip("."))
    if kind == "int":
        return str(int(value))
    if kind == "fix":
        return german(f"{float(value):.{fmt[1]}f}")
    if kind == "sig":
        digits = fmt[1]
        number = float(value)
        text = f"{number:.{digits - 1}e}"
        mantissa, exponent = text.split("e")
        exponent = int(exponent)
        if -3 <= exponent < 0 or 0 <= exponent < 4:
            decimals = max(digits - 1 - exponent, 0)
            return german(f"{number:.{decimals}f}")
        mantissa = mantissa.rstrip("0").rstrip(".")
        return german(f"{mantissa}e{exponent}")
    raise SystemExit(f"unknown format {fmt}")


# ---------------------------------------------------------------------------------------
# Section 2: genome and inheritance

def child_distribution(mother, father):
    """Autosomal locus: each parent passes either copy with 1/2. Returns {dose: prob}."""
    out = {}
    for m in mother:
        for f in father:
            dose = m + f
            out[dose] = out.get(dose, Fraction(0)) + Fraction(1, 4)
    return out


def x_children(mother, father_x):
    """X-linked locus. `mother` is her two X alleles, `father_x` his one X allele. Returns
    (daughters, sons): {dose or allele: prob} conditional on the sex of the child."""
    daughters, sons = {}, {}
    for m in mother:
        dose = m + father_x
        daughters[dose] = daughters.get(dose, Fraction(0)) + Fraction(1, 2)
        sons[m] = sons.get(m, Fraction(0)) + Fraction(1, 2)
    return daughters, sons


def haldane(d):
    return (1 - mp.e ** (-2 * d)) / 2


def odd_poisson(d):
    """P(odd count) of a Poisson(d) count, by the series: the crossover model of 2.3."""
    return mp.nsum(lambda k: mp.e ** (-d) * d ** (2 * k + 1) / mp.factorial(2 * k + 1), [0, mp.inf])


# The depth of `φ` (2.7; Grok, H2, ADR-0020 section 4): every query counts the ancestors of
# its two persons up to the great-grandparents (depth 1 the parents, 2 the grandparents, 3
# the great-grandparents).
KINSHIP_DEPTH = 3


class Pedigree:
    """Kinship by the standard recursion over founders (Falconer & Mackay ch. 5): phi(x, x)
    = (1 + F_x) / 2 with F_x = phi(mother, father); phi(x, y) = (phi(m_x, y) + phi(f_x, y))
    / 2 when x is not an ancestor of y (x is the younger one by insertion order).

    The depth of 2.7 (Grok, H2; ADR-0020 section 4; design note of the kinship depth,
    section 1): a query `phi(x, y)` (also `x = y`) has the set `S` of `x`, `y` and every
    ancestor of either up to `depth` generations (3, the great-grandparents), each once.
    Within the query a person's mother is known iff she is in `S`, its father iff he is in
    `S`, each on its own; an unknown parent's term is 0, `F` is 0 unless both parents are
    known, and a person without known parents is unrelated to everyone else of the query.
    The depth belongs to the pair of the query: the recursion keeps that `S` all the way
    down (F of a person in `S` is not a query of its own), and no intermediate value is
    reused by another query. `depth=None` counts every known ancestor (the rule before H2,
    kept only to show what the depth changes)."""

    def __init__(self):
        self.parents = {}
        self.order = []
        self.position = {}

    def add(self, name, mother=None, father=None):
        assert (mother is None) == (father is None), "both parents or none"
        assert name not in self.parents
        self.parents[name] = (mother, father)
        self.position[name] = len(self.order)
        self.order.append(name)

    def ancestors(self, x, depth):
        """The ancestors of `x` up to `depth` generations (every known one for None)."""
        found, frontier, generation = set(), {x}, 0
        while frontier and (depth is None or generation < depth):
            frontier = {parent for person in frontier for parent in self.parents[person] if parent is not None}
            found |= frontier
            generation += 1
        return found

    def query(self, x, y, depth=KINSHIP_DEPTH):
        """`(phi, S, visited)`: `φ(x, y)` exact under the depth rule, the set `S` of the
        query, and the pairs the recursion evaluated (unordered, `a = b` allowed; the query
        pair included; each once, memoized within this query only). A pair is visited when
        its value is needed: from `(a, a)` the pair of its parents if both are known, from
        `(a, b)` with `a` the younger the pairs of its known parents with `b`."""
        s = {x, y} | self.ancestors(x, depth) | self.ancestors(y, depth)
        memo = {}

        def known(person):
            return tuple(parent if parent is not None and parent in s else None for parent in self.parents[person])

        def rec(a, b):
            if self.position[a] < self.position[b]:
                a, b = b, a
            if (a, b) in memo:
                return memo[(a, b)]
            mother, father = known(a)
            if a == b:
                inbreeding = rec(mother, father) if mother is not None and father is not None else Fraction(0)
                value = (1 + inbreeding) / 2
            else:
                value = sum((rec(parent, b) for parent in (mother, father) if parent is not None), Fraction(0)) / 2
            memo[(a, b)] = value
            return value

        value = rec(x, y)
        return value, s, set(memo)

    def phi(self, x, y, depth=KINSHIP_DEPTH):
        return self.query(x, y, depth)[0]


def kinships():
    ped = Pedigree()
    for founder in ["gm", "gf", "gm2", "gf2", "x", "y"]:
        ped.add(founder)
    ped.add("mother", "gm", "gf")
    ped.add("aunt", "gm", "gf")
    ped.add("father", "gm2", "gf2")
    ped.add("uncle_in_law", "x", "y")
    ped.add("child", "mother", "father")
    ped.add("sib", "mother", "father")
    ped.add("half_sib", "mother", "y")
    ped.add("cousin", "aunt", "uncle_in_law")
    return {
        "parent_offspring": ped.phi("mother", "child"),
        "full_sibs": ped.phi("child", "sib"),
        "half_sibs": ped.phi("child", "half_sib"),
        "grandparent": ped.phi("gm", "child"),
        "avuncular": ped.phi("aunt", "child"),
        "first_cousins": ped.phi("child", "cousin"),
        "cousin_offspring_F": ped.phi("child", "cousin"),
    }


def section_2():
    carrier = child_distribution((0, 1), (0, 1))
    put("W2.6a", carrier[2], ("frac",))
    put("W2.6b", carrier[1], ("frac",))
    put("W2.6c", carrier[1] / (carrier[0] + carrier[1]), ("frac",))
    affected_by_carrier = child_distribution((1, 1), (0, 1))
    put("W2.6d", affected_by_carrier[2], ("frac",))

    daughters, sons = x_children((0, 1), 0)
    put("W2.6e", sons[1], ("frac",))
    put("W2.6f", daughters[1], ("frac",))
    daughters, sons = x_children((0, 0), 1)
    put("W2.6g", daughters[1], ("frac",))

    for key, d in [("W2.3a", mp.mpf("0.1")), ("W2.3b", mp.mpf("0.5")), ("W2.3c", mp.mpf(1))]:
        exact = haldane(d)
        assert abs(exact - odd_poisson(d)) < mp.mpf("1e-30"), key
        put(key, exact, ("fix", 4))

    phi = kinships()
    put("W2.7a", phi["parent_offspring"], ("frac",))
    put("W2.7b", phi["full_sibs"], ("frac",))
    put("W2.7c", phi["half_sibs"], ("frac",))
    put("W2.7d", phi["grandparent"], ("frac",))
    put("W2.7e", phi["avuncular"], ("frac",))
    put("W2.7f", phi["first_cousins"], ("frac",))
    q = Fraction(2, 100)
    big_f = phi["cousin_offspring_F"]
    affected = q * q + big_f * q * (1 - q)
    put("W2.7g", affected, ("sig", 4))
    put("W2.7h", affected / (q * q), ("fix", 2))

    # Map lengths (Kong et al. 2002, table 1): female autosomes 42.81 M, male 25.90 M,
    # X in women 1.79 M; 8 autosomal segments of 24 slots.
    # The calibration rounds the per-segment lengths to 0.01 M (K2.2a, K2.2b, K2.2c).
    segment_f, segment_m, x_female = mp.mpf("5.35"), mp.mpf("3.24"), mp.mpf("1.79")
    assert abs(segment_f - mp.mpf("42.81") / 8) < mp.mpf("0.005")
    assert abs(segment_m - mp.mpf("25.90") / 8) < mp.mpf("0.005")
    put("W2.3d", 8 * segment_f + x_female, ("fix", 2))
    put("W2.3h", 8 * segment_f, ("fix", 2))
    put("W2.3e", 8 * segment_m, ("fix", 2))
    put("W2.3f", haldane(segment_f / 24), ("fix", 4))
    put("W2.3g", haldane(segment_m / 24), ("fix", 4))

    mu = mp.mpf("5e-5")
    n = 150
    put("W2.4a", mu * n, ("sig", 2))
    put("W2.4b", mu * n * 2000, ("sig", 2))
    put("W2.4c", 1 - mp.e ** (-2 * mu * n), ("sig", 4))


# ---------------------------------------------------------------------------------------
# Section 3: hereditary diseases (C5). Founder frequencies and penetrances are calibration
# inputs (K3.x); everything here follows from Hardy-Weinberg and the liability model.

def bivariate_upper(t, rho):
    """P(X > t, Y > t) for a standard bivariate normal with correlation rho."""
    phi = lambda x: mp.e ** (-x * x / 2) / mp.sqrt(2 * mp.pi)
    upper = lambda x: mp.ncdf(-x)
    return mp.quad(lambda x: phi(x) * upper((t - rho * x) / mp.sqrt(1 - rho * rho)), [t, t + 4, mp.inf])


def section_3():
    q_ar = mp.mpf("0.02")
    put("W3.2a", q_ar ** 2, ("sig", 2))
    put("W3.2b", 2 * q_ar * (1 - q_ar), ("fix", 4))
    q_ad = mp.mpf("3e-5")
    put("W3.2c", 2 * q_ad * (1 - q_ad) + q_ad ** 2, ("sig", 2))
    q_inc = mp.mpf("1e-3")
    carriers = 2 * q_inc * (1 - q_inc) + q_inc ** 2
    put("W3.2d", carriers * mp.mpf("0.7"), ("sig", 2))
    put("W3.2e", carriers * mp.mpf("0.1"), ("sig", 2))
    q_x = mp.mpf("2.5e-4")
    put("W3.2f", q_x, ("sig", 2))
    put("W3.2g", q_x ** 2, ("sig", 3))
    f_mt = mp.mpf("1.2e-4")
    put("W3.2h", f_mt * mp.mpf("0.5"), ("sig", 2))
    put("W3.2i", f_mt * mp.mpf("0.1"), ("sig", 2))
    # Liability threshold, 3.6.
    k = mp.mpf("0.01")
    h2 = mp.mpf("0.6")
    t = -mp.sqrt(2) * mp.erfinv(2 * k - 1)
    assert abs(mp.ncdf(-t) - k) < mp.mpf("1e-30")
    put("W3.6a", t, ("fix", 4))
    sib = bivariate_upper(t, h2 / 2) / k
    put("W3.6b", sib, ("fix", 4))
    put("W3.6c", sib / k, ("fix", 2))
    mz = bivariate_upper(t, h2) / k
    put("W3.6d", mz, ("fix", 4))
    # Check against Falconer's case of schizophrenia-like values (K 1 percent, h2 0.81:
    # sibling risk ratio near 9, as observed).
    assert abs(bivariate_upper(t, mp.mpf("0.405")) / k / k - mp.mpf("8.8")) < mp.mpf("0.3")
    # Mutation toward a disease allele at rate 1e-6 per transmission: the frequency grows
    # by at most this much per generation.
    put("W3.3a", mp.mpf("1e-6") / q_ad, ("fix", 3))
    put("W3.3b", mp.mpf("5e-5") / q_ad, ("fix", 2))
    # Piecewise hazards (3.1, 3.2). One Exp(1) budget; the childhood cumulative hazard
    # is a play value, the adult rate is whatever makes the cited median.
    ln2 = mp.log(2)
    h18_ar = mp.mpf("0.02")
    median_ar = mp.mpf("65")
    put("W3.2j", h18_ar / 18, ("sig", 4))
    put("W3.2k", (ln2 - h18_ar) / (median_ar - 18), ("sig", 4))
    put("W3.2l", 1 - mp.e ** (-h18_ar), ("sig", 4))
    h18_x = mp.mpf("0.01")
    put("W3.2m", h18_x / 18, ("sig", 3))
    put("W3.2n", 1 - mp.e ** (-h18_x), ("sig", 3))
    # Liability onset N(25, 5), mass at or below the lower clamp of 15.
    put("W3.2o", mp.ncdf(-2), ("sig", 3))


# ---------------------------------------------------------------------------------------
# Section 4: quantitative genetics. All in z units of the reference population (variance
# 1), additive genetic values, equilibrium under assortative mating with spousal
# phenotypic correlation r. Derivations (Falconer & Mackay ch. 10; Crow & Kimura 1970):
#   Cov(A_father, P_mother) = r V_A (mating on phenotype), Cov(A_mother, A_father) = r h2 V_A
#   offspring A = (A_m + A_f) / 2 + segregation with variance V_A0 / 2 (V_A0 genic variance)
#   Cov(O, MP) = V_A (1 + r) / 2, Var(MP) = (1 + r) / 2      -> b(O on MP) = h2
#   Cov(O, P_m) = V_A (1 + r) / 2                             -> b(O on P) = h2 (1 + r) / 2
#   full sibs share both parents and C: (h2 / 2)(1 + r h2) + c2
#   half sibs (one mother, two fathers chosen independently given her phenotype):
#   Cov(A_f1, A_f2) = (r h2)^2 V_P = r^2 h2 V_A -> (h2 / 4)(1 + 2 r h2 + r^2 h2), no shared C
#   equilibrium: V_A = V_A0 / 2 + (V_A / 2)(1 + r h2) -> V_A0 = V_A (1 - r h2)

TRAITS = {
    # h2, c2 and the spousal correlation of adult phenotypes (sections 4.2 and 4.3).
    "height": dict(h2=Fraction(8, 10), c2=Fraction(5, 100), r=Fraction(2, 10)),
    "cognition": dict(h2=Fraction(75, 100), c2=Fraction(5, 100), r=Fraction(4, 10)),
}


def bulmer_generations(h2, r, tolerance=Fraction(1, 100)):
    """Generations from linkage equilibrium (random mating) until V_A is within
    `tolerance` of the equilibrium, by the recursion above; the non-genetic variance
    1 - h2 stays fixed."""
    equilibrium = h2
    genic = h2 * (1 - r * h2)
    other = 1 - h2
    v = genic
    generations = 0
    while (equilibrium - v) / equilibrium > tolerance:
        v = genic / 2 + v / 2 * (1 + r * v / (v + other))
        generations += 1
    return generations


def section_4():
    put("W4.2a", 1 / (1 + mp.e ** (-(20 - 12) / mp.mpf(2))), ("fix", 3))
    letters = iter("abcdefghijkl")
    for name in ["height", "cognition"]:
        t = TRAITS[name]
        h2, c2, r = t["h2"], t["c2"], t["r"]
        put("W4.3" + next(letters), h2, ("fix", 2))
        put("W4.3" + next(letters), h2 * (1 + r) / 2, ("frac4",))
        put("W4.3" + next(letters), h2 / 2 * (1 + r * h2) + c2, ("frac4",))
        put("W4.3" + next(letters), h2 / 4 * (1 + 2 * r * h2 + r * r * h2), ("frac4",))
    letters = iter("abcdefgh")
    for name in ["height", "cognition"]:
        t = TRAITS[name]
        h2, r = t["h2"], t["r"]
        put("W4.4" + next(letters), h2 * (1 - r * h2), ("fix", 3))
        put("W4.4" + next(letters), 1 / (1 - r * h2), ("fix", 3))
        put("W4.4" + next(letters), bulmer_generations(h2, r), ("int",))
    for key, ne in [("W4.5a", 50), ("W4.5b", 500)]:
        put(key, (1 - mp.mpf(1) / (2 * ne)) ** 8, ("fix", 4))


# ---------------------------------------------------------------------------------------
# Section 5: growth (C10). Preece-Baines model 1 with an infancy curve blended in between
# ages 2 and 4; individual size by scaling, individual timing by theta with h_theta moved
# so that childhood stays the same.

GROWTH = {
    # Adult mean and SD: Jelenkovic 2016 (European twins born 1970-79). Shape from 4 to 19
    # years: CDC 2000 stature-for-age medians (Kuczmarski 2002, file statage.csv, ages
    # Agemos / 12 at Agemos = 48.5 + 12 k), scaled by h1 / CDC median at 240 months. WHO
    # length medians at 0, 12 and 24 months (WHO 2006, CDC data files). Target age at peak
    # height velocity: ALSPAC (Cole 2020), only for comparison.
    "m": dict(h1="180.7", sd="7.01", who=("49.8842", "75.7488", "87.8161"), cdc_adult="176.8492",
              cdc=[102.51, 109.18, 115.66, 122.03, 128.12, 133.73, 138.82, 143.73, 149.31,
                   156.41, 164.14, 170.14, 173.61, 175.34, 176.19, 176.62],
              start=(13.9, 168.0, 0.11, 1.2)),
    "f": dict(h1="167.3", sd="6.67", who=("49.1477", "74.0150", "86.4153"), cdc_adult="163.3383",
              cdc=[101.03, 107.96, 115.01, 121.76, 127.83, 133.13, 138.21, 144.26, 151.49,
                   157.34, 160.48, 161.90, 162.57, 162.92, 163.13, 163.26],
              start=(12.1, 155.0, 0.13, 1.2)),
}
CDC_AGES = [(48.5 + 12 * k) / 12 for k in range(16)]


def pb_float(t, h1, theta, h_theta, s0, s1):
    return h1 - 2 * (h1 - h_theta) / (math.exp(s0 * (t - theta)) + math.exp(s1 * (t - theta)))


def fit_pb(sex):
    """Least squares of PB1 with h1 fixed against the scaled CDC medians, by
    Levenberg-Marquardt in floats; returns theta, h_theta, s0, s1."""
    g = GROWTH[sex]
    h1 = float(g["h1"])
    scale = h1 / float(g["cdc_adult"])
    data = [(t, y * scale) for t, y in zip(CDC_AGES, g["cdc"])]
    params = list(g["start"])

    def residuals(p):
        return [pb_float(t, h1, *p) - y for t, y in data]

    damping = 1e-3
    for _ in range(500):
        res = residuals(params)
        jac = []
        for i in range(4):
            step = 1e-6 * max(1.0, abs(params[i]))
            moved = params[:]
            moved[i] += step
            jac.append([(a - b) / step for a, b in zip(residuals(moved), res)])
        normal = [[sum(jac[i][k] * jac[j][k] for k in range(len(data))) for j in range(4)] for i in range(4)]
        grad = [sum(jac[i][k] * res[k] for k in range(len(data))) for i in range(4)]
        for i in range(4):
            normal[i][i] *= 1 + damping
        delta = solve4(normal, [-x for x in grad])
        trial = [a + b for a, b in zip(params, delta)]
        if sum(x * x for x in residuals(trial)) < sum(x * x for x in res):
            params, damping = trial, damping / 3
            if max(abs(d) for d in delta) < 1e-10:
                break
        else:
            damping *= 5
    return params, math.sqrt(sum(x * x for x in residuals(params)) / len(data))


def solve4(a, b):
    n = len(b)
    m = [row[:] + [b[i]] for i, row in enumerate(a)]
    for col in range(n):
        pivot = max(range(col, n), key=lambda r: abs(m[r][col]))
        m[col], m[pivot] = m[pivot], m[col]
        for r in range(n):
            if r != col:
                f = m[r][col] / m[col][col]
                m[r] = [x - f * y for x, y in zip(m[r], m[col])]
    return [m[i][n] / m[i][i] for i in range(n)]


class Growth:
    def __init__(self, sex):
        g = GROWTH[sex]
        self.h1 = mp.mpf(g["h1"])
        (theta, h_theta, s0, s1), self.rms = fit_pb(sex)
        self.theta = rounded(theta, 4)
        self.gap = self.h1 - rounded(h_theta, 4)
        self.s0, self.s1 = rounded(s0, 4), rounded(s1, 4)
        l0, l1, l2 = (mp.mpf(v) for v in g["who"])
        decay = (l2 - l1) / (l1 - l0)
        self.c = -mp.log(decay)
        self.b = (l1 - l0) / (1 - decay)
        self.l0 = l0

    def pb(self, t, theta=None):
        theta = self.theta if theta is None else theta
        gap = self.gap * mp.e ** (-self.s0 * (theta - self.theta))
        return self.h1 - 2 * gap / (mp.e ** (self.s0 * (t - theta)) + mp.e ** (self.s1 * (t - theta)))

    def infancy(self, t):
        return self.l0 + self.b * (1 - mp.e ** (-self.c * t))

    def height(self, t, theta=None):
        """The mean curve: infancy up to 2 years, PB1 from 3, smoothstep in between."""
        if t <= 2:
            return self.infancy(t)
        if t >= 3:
            return self.pb(t, theta)
        u = t - 2
        w = 3 * u * u - 2 * u ** 3
        return (1 - w) * self.infancy(t) + w * self.pb(t, theta)

    def phv_age(self, theta=None):
        """Sayers, Baines & Tilling 2013: the '+' root of s* gives the peak."""
        theta = self.theta if theta is None else theta
        s0, s1 = self.s0, self.s1
        star = ((s0 + s1) + mp.sqrt((s0 + s1) ** 2 - 8 * s0 * s1)) / 4
        return theta + mp.log((star - s0) / (s1 - star)) / (s1 - s0)

    def velocity(self, t, theta=None):
        return mp.diff(lambda x: self.height(x, theta), t)


WHO_2_5 = {
    # WHO 2006 length/height-for-age medians, expanded tables (days 730, 1095, 1461, 1826).
    "m": {2: "87.8018", 3: "96.0674", 4: "103.3273", 5: "109.9593"},
    "f": {2: "86.4008", 3: "95.0344", 4: "102.7312", 5: "109.4189"},
}


def section_5():
    letters = iter("abcdefghijklmnopqrstuvwxyz")
    for sex in "mf":
        g = Growth(sex)
        put("W5.2" + next(letters), g.theta, ("fix", 2))
        put("W5.2" + next(letters), g.h1 - g.gap, ("fix", 1))
        put("W5.2" + next(letters), g.s0, ("fix", 4))
        put("W5.2" + next(letters), g.s1, ("fix", 4))
        put("W5.2" + next(letters), g.rms, ("fix", 2))
        put("W5.2" + next(letters), g.c, ("fix", 3))
        put("W5.2" + next(letters), g.l0 + g.b, ("fix", 1))
        put("W5.2" + next(letters), g.phv_age(), ("fix", 2))
        put("W5.2" + next(letters), g.velocity(g.phv_age()), ("fix", 1))
        for age in [3, 5]:
            put("W5.2" + next(letters), g.height(age) - mp.mpf(WHO_2_5[sex][age]), ("fix", 1))
        # Monotone on a fine grid from 0 to 25 years, also with timing 3 years off.
        grid = [mp.mpf(k) / 20 for k in range(0, 501)]
        for theta_shift in [0, -3, 3]:
            heights = [g.height(t, g.theta + theta_shift) for t in grid]
            assert all(b > a for a, b in zip(heights, heights[1:])), (sex, theta_shift)
    letters = iter("abcdefgh")
    for sex in "mf":
        g = Growth(sex)
        for shift, age in [(-1, 12), (1, 12), (-1, 18)]:
            put("W5.3" + next(letters), g.height(age, g.theta + shift) - g.height(age), ("fix", 1))
    # Nutrition (5.4): k cm per year of deficit 1, weights 1 in ages 0 to 2, 0.5 in 2 to 5.
    k = mp.mpf(5)
    put("W5.4a", k * mp.mpf("0.3"), ("fix", 2))
    put("W5.4b", k * mp.mpf("0.5"), ("fix", 2))
    put("W5.4c", nutrition_cohort_loss(k, mp.mpf("0.3"), -1), ("fix", 2))
    put("W5.4d", nutrition_cohort_loss(k, mp.mpf("0.3"), 0), ("fix", 3))
    put("W5.4e", nutrition_cohort_loss(k, mp.mpf("0.3"), -2), ("fix", 3))
    put("W5.4f", nutrition_cohort_loss(k, mp.mpf("0.3"), -4), ("fix", 3))
    # Fertility multipliers (5.6). Smoothstep midpoint is exact; the declines are linear.
    put("W5.6a", mp.mpf("0.5"), ("fix", 1))
    put("W5.6b", (mp.mpf("50") - 40) / 20, ("fix", 1))
    put("W5.6c", 1 + (mp.mpf("0.4") - 1) * mp.mpf("0.5"), ("fix", 1))


def nutrition_weight(age):
    """Window weights of 5.4 without the puberty window (theta is far from these ages)."""
    if age < 0:
        return 0
    if age < 2:
        return 1
    if age < 5:
        return mp.mpf("0.5")
    return 0


def nutrition_cohort_loss(k, deficit, born):
    """Mean loss in cm of the cohort born in year Y + born (birthdays uniform in that year)
    from one shock year Y = [0, 1) with deficit `deficit`: k * deficit * the integral over
    the shock year of the window weight at the child's age."""
    def loss(birth):
        cuts = sorted({0, 1} | {x for x in (birth, birth + 2, birth + 5) if 0 < x < 1})
        return k * deficit * mp.quad(lambda t: nutrition_weight(t - birth) if t >= birth else 0, cuts)
    return mp.quad(lambda b: loss(born + b), [0, 1])


# ---------------------------------------------------------------------------------------
# Section 6: sex differences (C11). Adult height from Jelenkovic 2016 (K6.1a to K6.1d).

def section_6():
    mm, sm = mp.mpf(GROWTH["m"]["h1"]), mp.mpf(GROWTH["m"]["sd"])
    mf, sf = mp.mpf(GROWTH["f"]["h1"]), mp.mpf(GROWTH["f"]["sd"])
    put("W6.1a", (mm - mf) / mp.sqrt((sm ** 2 + sf ** 2) / 2), ("fix", 2))
    put("W6.1b", (sm / sf) ** 2, ("fix", 3))
    put("W6.1c", 100 * sm / mm, ("fix", 2))
    put("W6.1d", 100 * sf / mf, ("fix", 2))
    put("W6.1e", mp.ncdf((mf - mm) / sf), ("fix", 4))
    put("W6.1f", mp.ncdf((mf - mm) / sm), ("fix", 4))
    boys, girls = Growth("m"), Growth("f")
    for key, age in [("W6.1g", 8), ("W6.1h", 12)]:
        put(key, boys.height(age) - girls.height(age), ("fix", 1))


# ---------------------------------------------------------------------------------------
# Section 11: life course (step 9). Partner choice, fertility and twins, the bound of the
# genetic value of longevity, the learning function.

G_L_BOUND = 6
WISH_SHAPE = [Fraction(10, 100), Fraction(15, 100), Fraction(40, 100), Fraction(25, 100), Fraction(10, 100)]
TWINS_MZ = Fraction(4, 1000)
TWINS_DZ = Fraction(10, 1000)


def section_11():
    # The penalty 16 ln(10) phi is ln(10) for first cousins (phi = 1/16), so a cousin is
    # chosen a tenth as often as an equally fitting stranger; the odds ratio is e^penalty.
    put("W11.2a", 16 * mp.log(10) / 16, ("fix", 3))
    put("W11.2b", mp.exp(16 * mp.log(10) / 32), ("fix", 3))
    put("W11.2c", mp.exp(16 * mp.log(10) / 64), ("fix", 3))
    assert sum(WISH_SHAPE) == 1
    put("W11.3a", sum(n * p for n, p in enumerate(WISH_SHAPE)), ("fix", 2))
    twin_births = TWINS_MZ + TWINS_DZ
    put("W11.3b", 100 * twin_births, ("fix", 1))
    put("W11.3c", 100 * 2 * twin_births / (1 + twin_births), ("fix", 2))
    put("W11.3d", Fraction(105, 205), ("frac",))
    # Share of the reference population beyond the bound, two-sided.
    put("W11.4a", mp.erfc(G_L_BOUND / mp.sqrt(2)), ("sig", 3))
    put("W11.5a", 100 * (mp.exp(mp.mpf("0.25")) - 1), ("fix", 1))


# ---------------------------------------------------------------------------------------
# Section 7: death (C4). Gompertz-Makeham with a lognormal genetic multiplier.

def hermite_rule(n):
    """Gauss-Hermite rule for the standard normal (Golub-Welsch): nodes and weights with
    sum E[f(g)] = sum w_i f(g_i)."""
    jacobi = mp.zeros(n, n)
    for k in range(1, n):
        jacobi[k, k - 1] = jacobi[k - 1, k] = mp.sqrt(k)
    nodes, vectors = mp.eigsy(jacobi)
    return [(nodes[i], vectors[0, i] ** 2) for i in range(n)]


GH = None


class Mortality:
    """Hazard of one sex: lam + a * exp(-sigma * g) * exp(b * x), g standard normal: the
    genetic value of longevity, a larger g lowers the hazard (section 7.2)."""

    def __init__(self, lam, a, b, sigma):
        self.lam, self.a, self.b, self.sigma = lam, a, b, sigma

    def survival_given(self, g, x):
        c = self.a * mp.e ** (-self.sigma * g) / self.b
        return mp.e ** (-self.lam * x - c * (mp.e ** (self.b * x) - 1))

    def tail_given(self, g, x0):
        """Integral of the survival from x0 to infinity, closed form by the upper
        incomplete gamma function: substitute y = exp(b x)."""
        c = self.a * mp.e ** (-self.sigma * g) / self.b
        s = self.lam / self.b
        return mp.e ** c * c ** s * mp.gammainc(-s, c * mp.e ** (self.b * x0)) / self.b

    def survival(self, x):
        return mp.fsum(w * self.survival_given(g, x) for g, w in GH)

    def tail(self, x0):
        return mp.fsum(w * self.tail_given(g, x0) for g, w in GH)

    def e0(self):
        return self.tail(0)

    def e_at(self, x0):
        return self.tail(x0) / self.survival(x0)

    def heritability(self):
        """Squared correlation of lifespan T with g: the share of Var(T) that is linear in
        the additive genetic value (section 7.2)."""
        return self.correlation() ** 2

    def correlation(self):
        """Correlation of lifespan T with g, with its sign."""
        means, squares = [], []
        for g, w in GH:
            mean = self.tail_given(g, 0)
            second = 2 * mp.quad(lambda x: x * self.survival_given(g, x), [0, 60, 90, 120, 250])
            means.append((g, w, mean))
            squares.append(w * second)
        mean_t = mp.fsum(w * m for _, w, m in means)
        var_t = mp.fsum(squares) - mean_t ** 2
        cov = mp.fsum(w * g * m for g, w, m in means)
        return cov / mp.sqrt(var_t)

    def median(self):
        return mp.findroot(lambda x: self.survival(x) - mp.mpf("0.5"), 80)

    def mode(self):
        density = lambda x: -mp.diff(self.survival, x)
        return mp.findroot(lambda x: mp.diff(density, x), 85)


def fit(lam, e0, e65, sigma, guess):
    def equations(a_log, b):
        model = Mortality(lam, mp.e ** a_log, b, sigma)
        return [model.e0() - e0, model.e_at(65) - e65]
    a_log, b = mp.findroot(equations, (mp.log(guess[0]), guess[1]))
    return mp.e ** a_log, b


# Calibration inputs of section 7 (K7.1a to K7.1h): Destatis Sterbetafel 2023/2025 and
# the heritability of lifespan from HANDOFF section 4.
DEATH_TARGETS = {
    "f": dict(lam="1.5e-4", e0="83.43", e65="21.10", guess=(3.3e-6, 0.118)),
    "m": dict(lam="2.0e-4", e0="78.82", e65="17.97", guess=(8.7e-6, 0.112)),
}
LIFESPAN_H2 = "0.2"


def rounded(x, digits):
    return mp.mpf(render(x, ("sig", digits)).replace(",", "."))


def section_7():
    """Fits a and b per sex to e0 and e65 and sigma to the heritability, rounds them as
    the calibration file will hold them, and derives every other value from the rounded
    parameters."""
    global GH
    saved = mp.mp.dps
    mp.mp.dps = 20
    GH = hermite_rule(24)
    target = mp.mpf(LIFESPAN_H2)
    lam = {sex: mp.mpf(t["lam"]) for sex, t in DEATH_TARGETS.items()}
    guess = {sex: t["guess"] for sex, t in DEATH_TARGETS.items()}
    sigma = mp.mpf("0.72")
    for _ in range(8):
        fitted = {}
        for sex, t in DEATH_TARGETS.items():
            fitted[sex] = fit(lam[sex], mp.mpf(t["e0"]), mp.mpf(t["e65"]), sigma, guess[sex])
            guess[sex] = fitted[sex]

        def gap(sig):
            values = [Mortality(lam[x], fitted[x][0], fitted[x][1], sig).heritability() for x in "fm"]
            return (values[0] + values[1]) / 2 - target
        new_sigma = mp.findroot(gap, sigma)
        done = abs(new_sigma - sigma) < mp.mpf("1e-7")
        sigma = new_sigma
        if done:
            break
    sigma = rounded(sigma, 3)
    put("W7.2a", sigma, ("sig", 3))
    params = {}
    for sex, (key_a, key_b) in {"f": ("W7.2b", "W7.2c"), "m": ("W7.2d", "W7.2e")}.items():
        a, b = rounded(fitted[sex][0], 3), rounded(fitted[sex][1], 4)
        put(key_a, a, ("sig", 3))
        put(key_b, b, ("sig", 4))
        params[sex] = (a, b)
    # The proposal of C4, without the genetic factor, for comparison.
    put("W7.2f", Mortality(mp.mpf("3e-4"), mp.mpf("2.4e-5"), mp.mpf("0.095"), 0).e0(), ("fix", 1))
    put("W7.2g", Mortality(mp.mpf("3e-4"), mp.mpf("4.5e-5"), mp.mpf("0.095"), 0).e0(), ("fix", 1))
    letters = iter("abcdefghijklmnop")
    for sex in "fm":
        a, b = params[sex]
        model = Mortality(lam[sex], a, b, sigma)
        for value, fmt in [
            (model.e0(), ("fix", 2)),
            (model.e_at(65), ("fix", 2)),
            (model.median(), ("fix", 1)),
            (model.mode(), ("fix", 1)),
            (model.survival(65), ("fix", 3)),
            (model.survival(80), ("fix", 3)),
            (model.survival(90), ("fix", 3)),
            (model.heritability(), ("fix", 3)),
        ]:
            put("W7.3" + next(letters), value, fmt)
        put("W7.4" + ("a" if sex == "f" else "b"), sigma / b, ("fix", 2))
        # 11.4: the bound of g_L at 6 as a shift of age.
        put("W11.4" + ("b" if sex == "f" else "c"), G_L_BOUND * sigma / b, ("fix", 1))
        # A larger genetic value of longevity means a longer life (criterion P16).
        assert model.correlation() > 0, sex
    mp.mp.dps = saved


# ---------------------------------------------------------------------------------------
# Section 8: criteria catalog (C6). Every statistical criterion uses the tolerance
# Z_TOL standard errors of its estimator under the specified model; the false alarm rate
# is two-sided normal, the power is the chance to leave the tolerance under a named wrong
# model. Normal approximations throughout (n is large).

Z_TOL = mp.mpf(4)


def two_sided_false_alarm():
    return 2 * mp.ncdf(-Z_TOL)


def power(center, tol, alt, se_alt):
    """P(|estimate - center| > tol) when the estimate is N(alt, se_alt^2)."""
    return mp.ncdf((center - tol - alt) / se_alt) + 1 - mp.ncdf((center + tol - alt) / se_alt)


def proportion(p0, n, p_alt):
    se0 = mp.sqrt(p0 * (1 - p0) / n)
    tol = Z_TOL * se0
    return tol, power(p0, tol, p_alt, mp.sqrt(p_alt * (1 - p_alt) / n))


def slope(b0, var_x, n, b_alt):
    """Least-squares slope of y on x with Var(y) = 1 in the model."""
    se = lambda b: mp.sqrt((1 - b * b * var_x) / (n * var_x))
    tol = Z_TOL * se(b0)
    return tol, power(b0, tol, b_alt, se(b_alt))


def correlation(rho0, n, rho_alt):
    """On Fisher's z scale: SE 1 / sqrt(n - 3); tolerance reported on that scale."""
    se = 1 / mp.sqrt(n - 3)
    tol = Z_TOL * se
    return tol, power(mp.atanh(rho0), tol, mp.atanh(rho_alt), se)


def log_variance(n, ratio_alt):
    """ln of the sample variance: SE sqrt(2 / (n - 1)) for normal data."""
    se = mp.sqrt(mp.mpf(2) / (n - 1))
    tol = Z_TOL * se
    return tol, power(0, tol, mp.log(ratio_alt), se)


def section_8():
    put("W8.1a", two_sided_false_alarm(), ("sig", 2))
    # P2: two carriers, 2000 children, wrong model: one quarter becomes one third.
    tol, pw = proportion(mp.mpf(1) / 4, 2000, mp.mpf(1) / 3)
    put("W8.2a", tol, ("fix", 4))
    put("W8.2b", pw, ("fix", 4))
    # P3: offspring on midparent, 2000 families, Var(MP) = (1 + r) / 2 with r = 0.2;
    # wrong model: no regression to the mean (slope 1).
    tol, pw = slope(mp.mpf("0.8"), mp.mpf("0.6"), 2000, mp.mpf(1))
    put("W8.3a", tol, ("fix", 4))
    put("W8.3b", pw, ("fix", 4))
    # P4: variance of adult z per birth decade, 500 persons (one per family); wrong model:
    # the segregation variance missing for one generation, 4.4 without V_A0 / 2:
    # (V_A / 2)(1 + r h2) + 1 - h2 with V_A = h2 = 0.8, r = 0.2.
    h2, r = mp.mpf("0.8"), mp.mpf("0.2")
    without = h2 / 2 * (1 + r * h2) + 1 - h2
    put("W8.4e", without, ("fix", 3))
    tol, pw = log_variance(500, without)
    put("W8.4a", tol, ("fix", 4))
    put("W8.4b", mp.e ** tol, ("fix", 3))
    put("W8.4c", mp.e ** -tol, ("fix", 3))
    put("W8.4d", pw, ("fix", 4))
    # P5: full sibs, 1000 pairs (one pair per family), rho 0.514; wrong model: half the
    # sibling resemblance (0.257). Strangers: 1000 pairs, rho 0.
    tol, pw = correlation(mp.mpf("0.514"), 1000, mp.mpf("0.257"))
    put("W8.5a", tol, ("fix", 4))
    put("W8.5b", pw, ("fix", 4))
    tol, pw = correlation(mp.mpf("0"), 1000, mp.mpf("0.2"))
    put("W8.5c", pw, ("fix", 4))
    # P7: spouses, 1000 couples, cognition 0.4; wrong model: 0.2 (rule 10 of the tests).
    tol, pw = correlation(mp.mpf("0.4"), 1000, mp.mpf("0.2"))
    put("W8.7a", tol, ("fix", 4))
    put("W8.7b", pw, ("fix", 4))
    tol, pw = correlation(mp.mpf("0.2"), 1000, mp.mpf("0"))
    put("W8.7c", tol, ("fix", 4))
    put("W8.7d", pw, ("fix", 4))
    # P9: red hair, recessive locus with q = 0.15: share of red children with two non-red
    # parents p^2 under Hardy-Weinberg and random mating on this locus.
    q = mp.mpf("0.15")
    put("W8.9a", q * q, ("fix", 4))
    put("W8.9b", (1 - q) ** 2, ("fix", 4))
    tol, pw = proportion((1 - q) ** 2, 400, mp.mpf("0.5"))
    put("W8.9c", tol, ("fix", 4))
    put("W8.9d", pw, ("fix", 4))
    # P9: parent and child, polygenic pigment with h2 0.8 and no assortment: rho 0.4;
    # 1000 pairs (one child per family and one parent at random); wrong model: 0.2.
    tol, pw = correlation(mp.mpf("0.4"), 1000, mp.mpf("0.2"))
    put("W8.9e", pw, ("fix", 4))
    put("W8.9f", tol, ("fix", 4))
    # P15: sons of carrier mothers (X recessive), 1000 sons; wrong model: one quarter.
    tol, pw = proportion(mp.mpf(1) / 2, 1000, mp.mpf(1) / 4)
    put("W8.15a", tol, ("fix", 4))
    put("W8.15b", pw, ("fix", 4))
    # P16: lifespan on g_L, 20000 births per sex, correlation sqrt(0.2); wrong model: the
    # genetic factor read with half its sigma (h2 about a quarter): correlation 0.233.
    tol, pw = correlation(mp.sqrt(mp.mpf("0.2")), 20000, mp.mpf("0.233"))
    put("W8.16a", mp.sqrt(mp.mpf("0.2")), ("fix", 4))
    put("W8.16b", tol, ("fix", 4))
    put("W8.16c", pw, ("fix", 4))
    # P12: recombination between two loci 0.5 M apart in mothers, 5000 meioses; wrong
    # model: free recombination (0.5).
    tol, pw = proportion(haldane(mp.mpf("0.5")), 5000, mp.mpf("0.5"))
    put("W8.12a", tol, ("fix", 4))
    put("W8.12b", pw, ("fix", 4))
    # P13: new mutations in 20000 meioses with Lambda 0.0075: Poisson count; wrong model:
    # half the rate.
    lam = mp.mpf("0.0075") * 20000
    tol = Z_TOL * mp.sqrt(lam)
    put("W8.13a", lam, ("fix", 0))
    put("W8.13b", tol, ("fix", 1))
    put("W8.13c", power(lam, tol, lam / 2, mp.sqrt(lam / 2)), ("fix", 4))
    # P14: survival to 80 among 20000 births per sex, women model 0.660; wrong model:
    # the genetic factor missing raises it (no frailty) by several points; take 0.70.
    tol, pw = proportion(mp.mpf("0.660"), 20000, mp.mpf("0.70"))
    put("W8.14a", tol, ("fix", 4))
    put("W8.14b", pw, ("fix", 4))


def check(path):
    text = open(path, encoding="utf-8").read()
    found = re.findall(r"(-?\d[\d,/e-]*) \[([WK]\d+\.\d+[a-z])\]", text)
    for match in re.finditer(r"\[([WK]\d+\.\d+[a-z])\]", text):
        if not re.search(r"-?\d[\d,/e-]* $", text[max(0, match.start() - 30):match.start()]):
            print(f"{match.group(1)}: no number in front of it on the same line")
            return 1
    problems = []
    used = set()
    for number, key in found:
        used.add(key)
        if key.startswith("K"):
            continue
        if key not in VALUES:
            problems.append(f"{key}: unknown ID")
            continue
        want = render(*VALUES[key])
        if number.rstrip(",") != want:
            problems.append(f"{key}: the text says {number}, computed {want}")
    for key in VALUES:
        if key not in used:
            problems.append(f"{key}: computed but not in the text")
    for line in problems:
        print(line)
    print(f"{len(found)} numbers with ID, {len(VALUES)} computed, {len(problems)} problems")
    return 1 if problems else 0


def main(argv):
    section_2()
    section_3()
    section_4()
    section_5()
    section_6()
    section_8()
    section_11()
    if "--fast" not in argv:
        section_7()
    args = [a for a in argv[1:] if a != "--fast"]
    if len(args) == 2 and args[0] == "--check":
        return check(args[1])
    if not args:
        for key, (value, fmt) in VALUES.items():
            print(f"{key}\t{render(value, fmt)}")
        return 0
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))

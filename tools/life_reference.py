#!/usr/bin/env python3
"""Fits the life-course inputs of docs/genetik-spezifikation.md section 11 (and the
separation rate of ADR-0018 section 3) by simulating closed towns, written from the
specification and the ADRs only, independently of the Rust code (ADR-0017 section 1). The
fitted values become calibration inputs of the core; the core's tests then check the
emergent values against the targets of the specification.

Usage:
  python3 tools/life_reference.py              the full fit, then an independent check run
  python3 tools/life_reference.py --quick      a smoke test (under a minute, imprecise)
  python3 tools/life_reference.py --jobs N     worker processes (default: all cores, at
                                               most 12); the result does not depend on N
  python3 tools/life_reference.py --r7         the earlier market of Grok's R7, kept for
                                               comparison (not the rule)
  python3 tools/life_reference.py --wa-scan    adds a scan of w_a at the fitted point

Pure Python (standard library only); every number is fixed by the seeds below, so two runs
print the same digits. The output is a table of the fitted values with approximate standard
errors, the targets they reach in an independent check run with town-jackknife standard
errors (the authoritative check), the checks, a check run on towns of 100 founders, and a
JSON snippet for data/calibration/demography.json.

What the tool fits (targets from the specification and Grok's decisions of 2026-10-08 and
2026-10-09 on the earlier fits, ADR-0011, R6 to R8):

- `c` and `m` of the women's search rate `s_f(a) = min(1, c * exp(-1/2 * ((a - m) /
  7)^2))` (11.2 with R8): among women who reach 50, the median age at the first
  partnership is 23.5 and the share without any partnership by 50 is 0.10. The men's
  median and never share are emergent and only reported (R8).
- `rho`, a factor on `s_f(a)` from the 16th until before the 18th birthday (R6b): the
  share of live births to mothers under 18 among all live births after the burn-in is
  0.0024 (band 0.001 to 0.005); `rho` stays in (0, 1].
- `mu_a`, the preferred age gap (man older) of the age term, with `w_a` = 0.02 fixed
  (R8): the mean age gap man minus woman over all partnerships is 3.0 (band 2.5 to 4).
- `alpha` of the wish `P_n ~ p_n * exp(alpha * n)` (11.3): women who reach 50 have 2.20
  live births on average, twins counting two. A wish mean above 3.0 stops the tool. The
  search solves it on an estimator (below); the check run checks the realized births.
- `lambda_sep` per year (ADR-0018 section 3): one third of all partnerships end by
  separation, the rest by the first death.
- `w_K` and `w_H` (11.2): in first partnerships (neither partner had one before) the
  correlation of the partners' adult phenotypes is 0.4 (cognition `z_K`) and 0.2 (height
  `z_h`) (P7). It is measured on the adult phenotypes (adult weights, s = 1 below), while
  the market's utility reads `z_K` at each person's age at the round.

The model, exactly as written in spec 7 and 11 unless stated:

- Time in sim months of 28 days, 13 per year. Every birth falls on a month start, so ages
  at month starts are whole months; "from 16" means from the 16th birthday.
- Death (7.1, 7.2, 11.4): Gompertz-Makeham per sex with `m = exp(-sigma_g * g)`, `g` the
  longevity value N(0, 1) held at +-6; one Exp(1) budget per cause, death age in closed
  form. Founders start at their age with fresh budgets (7.1). Hereditary diseases are
  left out (they add a small hazard to few carriers and would need the genome).
- Market (11.2 with Grok's R8, the female-dominant market of demographic
  microsimulation) at every month start, after that month's births and conception
  attempts: the single persons of at least 16 are held; each single woman searches with
  `s_f(a)` (times `rho` from 16 until before 18), separated and widowed women by the same
  rule; men do not search. Searchers choose in the order of a random rank; the
  candidates are the single men of the snapshot in the order of a random rank per
  searcher, skipping those with `phi >= 1/8`, at most 12; utility `-w_K (dz_K)^2 - w_H
  (dz_h)^2 - 0.02 (da - mu_a)^2 - 16 ln(10) phi + Gumbel`, `z_K` of both at their ages at
  the round (4.2), the Gumbel noise `-ln(-ln(u))` with u by the open mapping of
  PAIRING_NOISE (11.7); the largest wins; a pair forms if the chosen man is still single,
  else nothing; no counter check. A new partnership tries to conceive from the next month
  start on (ADR-0018 section 1).
- Same-second order (ADR-0018 section 1: births, round, conception attempts, separations,
  deaths): death and separation times are continuous, never on the month grid, so taking
  those up to a month start before its births and round is that order; the attempts
  before the round are equivalent to after it (comment in `simulate`).
- The earlier market of R7 (`--r7`, cfg["market"] = "r7"): a single person enters the
  market with `e(a)` per sex and stays until a partnership forms; everyone on the market
  searches with q = 0.25 among the other sex on the market; `mu_a` = 2.5. It is kept to
  reproduce the findings that led to R8, not as a rule.
- Kinship `phi` by the recursion of 2.7 over the simulated pedigree, founders unrelated;
  kept as an exact fixed-point matrix over the living (values are dyadic, scale 2^62).
- Fertility (11.3 with the curve of 5.6, decided by Dario, commit a96418f): a woman in an
  open partnership tries at every month start while her live births are below her wish,
  she is not pregnant and at least 12 months have passed since her last birth; success
  `0.20 x woman factor x man factor` (`PubertyFertility`): the woman's factor is 0 before
  her menarche `theta_i + delta` (`delta ~ N(0.2, 0.6)` held on [theta_i - 1,
  theta_i + 3]), a smoothstep over 3 years, then 1, times a decline from 1 at 30 to 0 at
  50; the man's is 0 until `theta_i - 0.7`, a smoothstep over 2 years, from 45 linear
  down to 0.4 at 70. `theta_i = theta + sigma_theta z_p` (5.2) per sex, `z_p` held at
  +-6. Pregnancy 10 months; her death ends it; per conception identical twins with
  0.004, fraternal with 0.010; sons with `p_Y = 21/41`. Children stay with the mother;
  that changes no rule here.
- Two rules stay replaceable: the conception chance (`cfg["fertility"]`, an object with
  `chance` and `last_age`) and the market's minimum age (`cfg["market_age"]`, default 16
  since Grok's decision; it was 18).
- Checks: the mean age gap within 2.5 to 4 and the share of births to mothers under 18
  within 0.001 to 0.005; the men's median and never share, the women's never share under
  R7, the mean age of mothers at first birth and the total fertility rate are reported.
- Separation (ADR-0018 section 3): an Exp(1) budget per partnership drawn at its start;
  while both live it ends after `budget / lambda_sep` years.

Modeling choices of this tool (not in the specification):

- Phenotypes: no inheritance. Each person's `G`, `C` and `E` of cognition, `z_h` and `z_p`
  are independent N(0, 1) draws at birth (no shared `C` among siblings, no shared `G`
  among identical twins). Cognition follows 4.2 with age: `z_K(x) = sqrt(h2(x)) G +
  sqrt(c2(x)) C + sqrt(e2(x)) E`, each weight `w(x) = w_child + (w_adult - w_child) s(x)`
  with `s(x) = 1 / (1 + exp(-(x - 12) / 2))`, h2 from 0.5 to 0.75, c2 from 0.25 to 0.05, e2
  the rest; the market reads it at the person's age at the round, P7 at the adult weights
  (s = 1). Height's weights (0.8, 0.05) do not change with age, so `z_h` is one draw for
  life. Without inheritance there is no Bulmer increase of the variance (4.4): the fit
  holds for a population at the equilibrium of 4.1, where `z` is N(0, 1) by construction
  of `sigma_ref` (at every age, since the squared weights sum to 1). Relatives are not
  more alike than strangers here, so the kinship penalty does not interact with
  similarity.
- Random numbers are addressed by entity and occasion (a splitmix64 mixer over person
  keys built like lineage keys, not the core's draws). That makes a run independent of
  the order of evaluation, but it does not make nearby parameters comparable: one
  changed search decision changes every later market round of the town, and a 1 percent
  change of `c` already decorrelates a town's medians. The fit therefore treats every
  run as an independent sample.
- Founders: ages from the stationary age distribution of the death model, sex by `p_Y`;
  women and men of the market age are paired at the start by a simple rule (probability by
  age, a man of the woman's age -2 to +8 years) and founder women are given a share of
  their wish as earlier births without persons. Founders are only the burn-in; nothing is
  measured on them.
- Measurement: persons born after the burn-in who reach 50 before the end (search curve,
  wish); partnerships formed after the burn-in (correlations, age gap, separation). The
  separation share counts the eventual ending of each partnership, which the tool knows
  even after the run's end because it draws death and separation times in advance.
- Search statistics with less noise: the separation share is averaged over the separation
  budget given the time to the first death (`1 - exp(-lambda * D)`); `D` is known when the
  budget is drawn, so this has the expectation of the realized share. The mean live
  births of a woman is averaged over her wish given her history (her partnerships and
  the conceptions the tool keeps simulating up to four births). That is not an exact
  conditional expectation in a closed town: her wish changes her descendants and so,
  through the market, later partnership chances, her own included. Its bias is not
  known in advance; the check run measures the realized live births per woman reaching
  50, and that is the check of `alpha`: if it misses 2.20 by more than 2 of its standard
  errors (town jackknife), the tool stops with a finding and `alpha` has to be fitted on
  realized births instead. The realized values are printed next to the estimators.

- Town size: 1000 founders, growing to about 2000 to 2500 persons in 300 years. The
  market outcome depends on the number of singles, so the values hold for towns of this
  size; a much smaller town (ten founder families) has a thinner market.
- Run: 300 years, of which 100 are burn-in (about three and a half generations; the
  founders' age structure is already stationary, and the report shows that the cohorts of
  the first and second half of the measured years agree).

Fitting: `alpha` and `lambda_sep` are solved exactly on a run's sample (both statistics
are smooth and monotone in their parameter), at the end on new towns at the fitted point.
The other six (`ln c` and `m` per sex, `ln w_K`, `ln w_H`) are found by a trust-region
search with a local regression model: a design of 12 runs one step away along each axis,
then iterations that fit the six outer residuals linearly in the parameters by weighted
least squares over all runs near the current point (weight: towns times a Gaussian
kernel), take a Levenberg-Marquardt step towards the model's root and run new towns
there. Plain Newton steps do not work here: the women's search center `m_f` has a weak
effect where women search little (most women pair by being chosen), the finite-difference
Jacobian is nearly singular and noisy, and the steps cycle. The fitted values are the
least-squares point of the final local model (its root when it has one; residuals
weighted by the per-town standard deviations TSCALE). Their standard errors are
approximate, not full Monte Carlo errors: the six outer ones by the delta method from
the covariance of the model's intercepts across the targets (kernel weights and the
Jacobian's noise not modeled), `alpha`'s and `lambda_sep`'s by the delete-one-town
jackknife of their solution on the settle run (without the uncertainty of the other
six). The authoritative check is a check run at the fitted values on new towns: it gives
the achieved targets (births as realized births), with standard errors by the
delete-one-town jackknife (they include the dependence within a town).

Exit status 1 when a target misses by more than 4 standard errors, a check of 11.2 fails
(per 11.2 the finding then goes back to Grok before the values are used) or the realized
births miss 2.20 by more than 2 standard errors (then `alpha` needs a fit on realized
births; the smoke test only notes this one, its four towns cannot tell bias from noise).
"""

import heapq
import json
import math
import os
import sys
import time
from bisect import bisect_right
from concurrent.futures import ProcessPoolExecutor
from statistics import NormalDist

# ---------------------------------------------------------------------------------------
# Constants of the specification

MONTHS = 13  # sim months per sim year (11.1, ADR-0002)
MARKET_AGE = 16  # years, the default of cfg["market_age"] (Grok, 2026-10-08; was 18)
WIDTH = 7.0  # K11.2b
K_CAND = 12  # K11.2f
MU_AGE = 2.5  # K11.2g
W_AGE = 0.02  # K11.2h, fixed again (Grok, R6c)
Q_SEARCH = 0.25  # search probability per round of a person on the market (Grok, R7)
MINOR_UNTIL = 18  # years: rho multiplies e(a) from the market age until before 18 (R6b)
KIN_PENALTY = 16.0 * math.log(10.0)  # 11.2
PAUSE = 12  # K11.3a, months
PREGNANCY = 10  # K11.3c, months
TWIN_MZ = 0.004  # K11.3f
TWIN_ALL = 0.014  # MZ + DZ (K11.3g)
P_Y = 21.0 / 41.0  # W11.3d
WISH_SHAPE = (0.10, 0.15, 0.40, 0.25, 0.10)  # K11.3d
WISH_MAX_MEAN = 3.0  # 11.3
SIGMA_G = 0.724  # W7.2a
GOMPERTZ = {0: (3.25e-6, 0.1182, 1.5e-4), 1: (8.73e-6, 0.1118, 2.0e-4)}  # (a, b, lambda)
G_BOUND = 6.0  # 11.4
# Puberty timing theta_i = theta + sigma_theta * z_p (5.2), z_p held at +-6 (ADR-0012):
# theta W5.2l, W5.2a and sigma_theta K5.2b, K5.2a, in data/calibration/genetics.json as
# core:growth_theta_female/male and core:growth_theta_sd_female/male.
THETA = {0: (12.30, 0.89), 1: (14.02, 0.81)}
Z_BOUND = 6.0
MENARCHE = (0.2, 0.6)  # mean and SD of the menarche offset after theta_i, K5.6a, K5.6b
MENARCHE_RAMP = 3.0  # K5.6c
WOMAN_DECLINE = (30.0, 50.0)  # K5.6d, K5.6e
MAN_START, MAN_RAMP = 0.7, 2.0  # K5.6f, K5.6g
MAN_DECLINE = (45.0, 70.0, 0.4)  # K5.6h, K5.6j, K5.6i
# Cognition phenotype at age x (4.2): z_K(x) = sqrt(h2(x)) G + sqrt(c2(x)) C + sqrt(e2(x)) E,
# each weight from its child to its adult value along s(x) = 1 / (1 + exp(-(x - 12) / 2)),
# e2 the rest to 1. Height keeps 0.8 and 0.05 [K4.2a, K4.2b] at every age, so z_h does not
# depend on age.
COG_H2 = (0.5, 0.75)  # child, adult [K4.2d]
COG_C2 = (0.25, 0.05)  # child, adult [K4.2e]
COG_MID, COG_SPREAD = 12.0, 2.0  # years, the transition s(x) of 4.2
U18_BAND = (0.001, 0.005)  # share of live births to mothers under 18 (Grok, 2026-10-08)
GAP_BAND = (2.5, 4.0)  # 11.2

TARGET = {
    "med_f": 23.5, "med_m": 26.0, "never_m": 0.10,  # K11.2c, K11.2d, K11.2e (men only)
    "u18": 0.0024,  # share of live births to mothers under 18 (Grok, R6b; Germany 2025)
    "gap": 3.0,  # mean age gap man minus woman, all partnerships, via mu_a (Grok, R8)
    "r_K": 0.4, "r_H": 0.2,  # K4.2f, K4.2c
    "births": 2.20,  # K11.3e
    "sep": 1.0 / 3.0,  # ADR-0018 section 3
    # Not a target since Grok's decision of 2026-10-08, only reported; it picks the
    # point among the solutions of the others with a small weight (TSCALE).
    "never_f": 0.10,
}

FEMALE, MALE = 0, 1

# ---------------------------------------------------------------------------------------
# Addressed random numbers: a splitmix64 finalizer over (tag, entity key, occasion).

M64 = (1 << 64) - 1
GA = 0x9E3779B97F4A7C15
GB = 0xC2B2AE3D27D4EB4F
GC = 0x165667B19E3779F9
F1 = 0xBF58476D1CE4E5B9
F2 = 0x94D049BB133111EB
U53 = 2.0 ** -53

(TAG_FOUNDER, TAG_CHILD, TAG_SEX, TAG_G, TAG_ZK, TAG_ZH, TAG_SEN, TAG_ACC, TAG_WISH,
 TAG_SEARCH, TAG_ORDER, TAG_RANK, TAG_NOISE, TAG_CONCEIVE, TAG_TWIN, TAG_SEP, TAG_AGE,
 TAG_PAIR0, TAG_PRIOR, TAG_ZP, TAG_MENARCHE, TAG_QSEARCH) = range(1, 23)


def mix(x):
    x &= M64
    x = ((x ^ (x >> 30)) * F1) & M64
    x = ((x ^ (x >> 27)) * F2) & M64
    return x ^ (x >> 31)


def salt(tag, a, b=0):
    """A 64-bit word for (tag, a, b); `a` may be a key, `b` a small occasion."""
    return mix(mix(tag * GC + a) + b * GA)


def unit(w):
    """In (0, 1]: the top 2^11 words give 1.0, because 2^53 - 1/2 is not a double and
    rounds up. That is harmless for the comparisons `unit(w) < p` with p <= 1, the wish
    and `exp1` (a budget of 0); `normal` would raise there (probability 2^-53 per draw).
    Draws that need 0 < u < 1 use `unit_open`."""
    return ((w >> 11) + 0.5) * U53


def unit_open(w):
    """Strictly between 0 and 1, the mapping of PAIRING_NOISE (11.7):
    `u = (2 * (w >> 12) + 1) * 2^-53`, from 2^-53 to 1 - 2^-53, every value exact."""
    return (2 * (w >> 12) + 1) * U53


NORMAL = NormalDist()


def normal(w):
    return NORMAL.inv_cdf(unit(w))


def exp1(w):
    return -math.log(unit(w))


# ---------------------------------------------------------------------------------------
# Death (7.1, 7.2, 11.4)

def death_age(sex, x0, g, b_sen, b_acc):
    """Age at death in years from origin age `x0` with fresh Exp(1) budgets."""
    a, b, lam = GOMPERTZ[sex]
    held = max(-G_BOUND, min(G_BOUND, g))
    m = math.exp(-SIGMA_G * held)
    senescence = math.log(math.exp(b * x0) + b * b_sen / (a * m)) / b
    accident = x0 + b_acc / lam
    return min(senescence, accident)


def survival_table(sex, months):
    """Population survival l(a) averaged over g, on whole months."""
    a, b, lam = GOMPERTZ[sex]
    n = 64
    ms = [math.exp(-SIGMA_G * NORMAL.inv_cdf((i + 0.5) / n)) for i in range(n)]
    out = []
    for k in range(months):
        x = k / MONTHS
        h = a / b * (math.exp(b * x) - 1.0)
        out.append(sum(math.exp(-lam * x - m * h) for m in ms) / n)
    return out


# ---------------------------------------------------------------------------------------
# Kinship (2.7): a fixed-point matrix over slots of the living, scale 2^62, exact for the
# dyadic values of the recursion. A child's row is the mean of its parents' rows, written
# as its row and its column; its own entry is (1 + phi(mother, father)) / 2.

ONE = 1 << 62
EIGHTH = 1 << 59


class Kinship:
    def __init__(self, cap):
        self.cap = 0
        self.buf = bytearray()
        self.free = []
        self._grow(cap)

    def _grow(self, cap):
        old_cap, old = self.cap, self.buf
        buf = bytearray(cap * cap * 8)
        rb_old, rb = old_cap * 8, cap * 8
        for r in range(old_cap):
            buf[r * rb:r * rb + rb_old] = old[r * rb_old:(r + 1) * rb_old]
        self.buf = buf
        self.q = memoryview(buf).cast("Q")
        self.mask = int.from_bytes((b"\xff" * 7 + b"\x7f") * cap, "little")
        self.free.extend(range(cap - 1, old_cap - 1, -1))
        self.cap = cap

    def take(self):
        if not self.free:
            self._grow(int(self.cap * 1.25) + 64)
        return self.free.pop()

    def release(self, slot):
        self.free.append(slot)

    def founder(self, slot):
        cap = self.cap
        self.q[slot * cap:(slot + 1) * cap] = memoryview(bytes(cap * 8)).cast("Q")
        self.q[slot::cap] = memoryview(bytes(cap * 8)).cast("Q")
        self.q[slot * cap + slot] = ONE >> 1

    def child(self, slot, sm, sf):
        cap = self.cap
        rb = cap * 8
        mv = memoryview(self.buf)
        rm = int.from_bytes(mv[sm * rb:(sm + 1) * rb], "little")
        rf = int.from_bytes(mv[sf * rb:(sf + 1) * rb], "little")
        row = (((rm + rf) >> 1) & self.mask).to_bytes(rb, "little")
        self.buf[slot * rb:(slot + 1) * rb] = row
        self.q[slot::cap] = memoryview(row).cast("Q")
        self.q[slot * cap + slot] = (ONE + self.q[sm * cap + sf]) >> 1


# ---------------------------------------------------------------------------------------
# Parameters

def search_table(c, m, market_age, rho):
    """e(a) on whole months of age; 0 below the market age (11.2), times rho from the
    market age until before the 18th birthday (Grok, R6b)."""
    out = [0.0] * (131 * MONTHS)
    for k in range(market_age, len(out)):
        x = (k / MONTHS - m) / WIDTH
        out[k] = min(1.0, c * math.exp(-0.5 * x * x))
        if k < MINOR_UNTIL * MONTHS:
            out[k] *= rho
    return out


def cognition_weights(s):
    """(sqrt h2, sqrt c2, sqrt e2) of cognition at the transition value s (4.2):
    w = w_child + (w_adult - w_child) * s for h2 and c2, e2 the rest to 1."""
    h2 = COG_H2[0] + (COG_H2[1] - COG_H2[0]) * s
    c2 = COG_C2[0] + (COG_C2[1] - COG_C2[0]) * s
    return math.sqrt(h2), math.sqrt(c2), math.sqrt(1.0 - h2 - c2)


# The weights on whole months of age (every age at a month start is whole months); from
# 131 years on the last entry, where s(x) is 1 in double precision. The adult phenotype of
# the correlation target P7 takes s = 1.
COG_BY_AGE = [cognition_weights(1.0 / (1.0 + math.exp(-(k / MONTHS - COG_MID) / COG_SPREAD)))
              for k in range(131 * MONTHS)]
COG_LAST = len(COG_BY_AGE) - 1
COG_ADULT = cognition_weights(1.0)


def smoothstep(t):
    """3t^2 - 2t^3 on [0, 1], 0 before and 1 after (6.2)."""
    if t <= 0.0:
        return 0.0
    if t >= 1.0:
        return 1.0
    return t * t * (3.0 - 2.0 * t)


def puberty_timing(sex, key):
    """theta_i = theta + sigma_theta * z_p (5.2) with z_p held at +-6. In this tool z_p is
    an independent N(0, 1) draw per person (no inheritance, like z_K and z_h)."""
    theta, sd = THETA[sex]
    z = max(-Z_BOUND, min(Z_BOUND, normal(salt(TAG_ZP, key))))
    return theta + sd * z


class PubertyFertility:
    """The monthly chance of a conception that is born, as one replaceable rule:
    `adult_rate * woman_factor(age_w, key_w) * man_factor(age_m, key_m)`, ages in years at
    the month start, `key` the person's addressed key. This is spec 5.6 (decided by Dario,
    commit a96418f): `adult_rate` 0.20 (K11.3b). Woman: menarche `m_i = theta_i + delta`,
    `delta ~ N(0.2, 0.6)` held on [theta_i - 1, theta_i + 3]; 0 before `m_i`, a smoothstep
    over 3 years, then 1; times a linear decline from 1 at 30 to 0 at 50. Man: 0 until
    `theta_i - 0.7`, a smoothstep over 2 years to 1; from 45 linear down to 0.4 at 70, then
    0.4. `last_age` is the age from which the woman factor is 0 for good; it only takes
    women off the list of attempts."""

    adult_rate = 0.20
    last_age = WOMAN_DECLINE[1]

    def __init__(self):
        self.menarche = {}
        self.start = {}

    def woman_factor(self, age, key):
        if age >= WOMAN_DECLINE[1]:
            return 0.0
        m = self.menarche.get(key)
        if m is None:
            theta = puberty_timing(FEMALE, key)
            m = theta + MENARCHE[0] + MENARCHE[1] * normal(salt(TAG_MENARCHE, key))
            m = max(theta - 1.0, min(theta + 3.0, m))
            self.menarche[key] = m
        f = smoothstep((age - m) / MENARCHE_RAMP)
        if age > WOMAN_DECLINE[0]:
            f *= (WOMAN_DECLINE[1] - age) / (WOMAN_DECLINE[1] - WOMAN_DECLINE[0])
        return f

    def man_factor(self, age, key):
        s = self.start.get(key)
        if s is None:
            s = puberty_timing(MALE, key) - MAN_START
            self.start[key] = s
        f = smoothstep((age - s) / MAN_RAMP)
        lo, hi, floor = MAN_DECLINE
        if age >= hi:
            f *= floor
        elif age > lo:
            f *= 1.0 - (1.0 - floor) * (age - lo) / (hi - lo)
        return f

    def chance(self, age_w, key_w, age_m, key_m):
        f = self.woman_factor(age_w, key_w)
        if f <= 0.0:
            return 0.0
        return self.adult_rate * f * self.man_factor(age_m, key_m)


def wish_probs(alpha):
    w = [p * math.exp(alpha * n) for n, p in enumerate(WISH_SHAPE)]
    s = sum(w)
    return [x / s for x in w]


def wish_mean(alpha):
    return sum(n * p for n, p in enumerate(wish_probs(alpha)))


def wish_of(u, cum):
    for n, c in enumerate(cum):
        if u < c:
            return n
    return len(cum) - 1


# ---------------------------------------------------------------------------------------
# One town

def simulate(par, seed, cfg):
    """Runs one closed town; returns the sufficient statistics of the measurement."""
    years, burn_years = cfg["years"], cfg["burn_in"]
    t_end = years * MONTHS
    burn = burn_years * MONTHS
    cohort_end = t_end - 50 * MONTHS
    split = (burn + cohort_end) // 2
    n0 = cfg["founders"]
    lam_sep = par["lam"]
    w_k, w_h, w_a = par["w_K"], par["w_H"], par.get("w_a", W_AGE)
    female_market = cfg.get("market", "female") == "female"
    mu_a = par.get("mu_a", MU_AGE)
    market_age = round(cfg.get("market_age", MARKET_AGE) * MONTHS)
    rule = cfg.get("fertility") or PubertyFertility()
    chance = rule.chance
    fert_until = math.ceil(rule.last_age * MONTHS)
    tabs = (search_table(par["c_f"], par["m_f"], market_age, par["rho"]),
            search_table(par.get("c_m", 0.0), par.get("m_m", 30.0), market_age, par["rho"]))
    tab_len = len(tabs[0])
    cum = []
    acc = 0.0
    for p in wish_probs(par["alpha"]):
        acc += p
        cum.append(acc)
    cum[-1] = 2.0
    town = mix(seed * GC + 0x5157)

    # Person columns.
    sex, birth, death, key, mother, father = [], [], [], [], [], []
    kg, kc, ke = [], [], []  # cognition G, C, E (4.2), independent N(0, 1) per person here
    zk, zh, slot, first, npart, partner_of = [], [], [], [], [], []  # zk: adult z_K (P7)
    wish, vcount, vpreg, vlast, nconc, cumv, actual, first_birth = [], [], [], [], [], [], [], []
    pend = []
    kin = Kinship(int(n0 * 1.6) + 64)
    heap = []
    births_at = {}
    adult_at = {}
    singles = (set(), set())
    market = (set(), set())  # on the market (R7): entered and not yet partnered
    fert = set()

    def person(sx, born, k, gk, mo, fa, x0):
        p = len(sex)
        g = normal(salt(TAG_G, gk))
        d = born + MONTHS * death_age(sx, x0, g, exp1(salt(TAG_SEN, k)), exp1(salt(TAG_ACC, k)))
        sex.append(sx)
        birth.append(born)
        death.append(d)
        key.append(k)
        mother.append(mo)
        father.append(fa)
        g_k, c_k, e_k = (normal(salt(TAG_ZK, k, part)) for part in range(3))
        kg.append(g_k)
        kc.append(c_k)
        ke.append(e_k)
        zk.append(COG_ADULT[0] * g_k + COG_ADULT[1] * c_k + COG_ADULT[2] * e_k)
        zh.append(normal(salt(TAG_ZH, k)))
        first.append(-1)
        npart.append(0)
        partner_of.append(-1)
        wish.append(wish_of(unit(salt(TAG_WISH, k)), cum) if sx == FEMALE else 0)
        vcount.append(0)
        vpreg.append(False)
        vlast.append(-10 ** 9)
        nconc.append(0)
        cumv.append(None)
        actual.append(0)
        first_birth.append(-1)
        pend.append(0)
        s = kin.take()
        slot.append(s)
        if d < t_end:
            heapq.heappush(heap, (d, 0, p))
        adult = born + market_age
        if adult > 0 and d > adult:
            adult_at.setdefault(adult, []).append(p)
        return p

    # Partnerships.
    q_w, q_m, q_open = [], [], []
    stats_gap = [0, 0.0, 0.0]
    stats_gap_first = [0, 0.0]
    gap_hist = [0] * 201  # whole years from -100 to 100, man minus woman
    stats_corr = [0] + [0.0] * 10  # n, then K: sx, sy, sxx, syy, sxy, then H likewise
    stats_cands = [0, 0, 0]  # searchers after the burn-in, their candidates, without any
    stats_market = [0, 0, 0]  # rounds after the burn-in, persons on the market (F, M)
    sep_d = []
    sep_real = [0, 0, 0]  # separations, deaths, still open at the end

    def form(w, m, t):
        q = len(q_w)
        q_w.append(w)
        q_m.append(m)
        q_open.append(True)
        is_first = npart[w] == 0 and npart[m] == 0
        b = exp1(salt(TAG_SEP, key[w], npart[w]))
        for p in (w, m):
            if npart[p] == 0 and t >= 0:
                first[p] = t - birth[p]
            npart[p] += 1
            partner_of[p] = q
            singles[sex[p]].discard(p)
            market[sex[p]].discard(p)
        d = min(death[w], death[m])
        t_sep = t + MONTHS * b / lam_sep if lam_sep > 0.0 else math.inf
        if t_sep < d and t_sep < t_end:
            heapq.heappush(heap, (t_sep, 1, q))
        if t >= burn:
            gap = (birth[w] - birth[m]) / MONTHS
            stats_gap[0] += 1
            stats_gap[1] += gap
            stats_gap[2] += gap * gap
            gap_hist[100 + max(-100, min(100, math.floor(gap)))] += 1
            if is_first:
                stats_gap_first[0] += 1
                stats_gap_first[1] += gap
            sep_d.append((d - t) / MONTHS)
            sep_real[0 if t_sep < d else 1] += 1
            if min(d, t_sep) >= t_end:
                sep_real[2] += 1
            if is_first:
                # P7 is on the adult phenotypes (s = 1), whatever the ages at the round.
                c = stats_corr
                c[0] += 1
                for off, (x, y) in ((1, (zk[w], zk[m])), (6, (zh[w], zh[m]))):
                    c[off] += x
                    c[off + 1] += y
                    c[off + 2] += x * x
                    c[off + 3] += y * y
                    c[off + 4] += x * y
        if t - birth[w] < fert_until and vcount[w] < 4:
            fert.add(w)

    def close(q):
        q_open[q] = False
        w, m = q_w[q], q_m[q]
        fert.discard(w)
        partner_of[w] = -1
        partner_of[m] = -1

    def free_slot(p):
        if pend[p] == 0:
            kin.release(slot[p])
            slot[p] = -1

    # Founders.
    surv = (survival_table(FEMALE, 110 * MONTHS), survival_table(MALE, 110 * MONTHS))
    cdf = []
    for tab in surv:
        acc = 0.0
        c = []
        for v in tab:
            acc += v
            c.append(acc)
        cdf.append([x / acc for x in c])
    for i in range(n0):
        k = mix(mix(town + TAG_FOUNDER * GC) + i * GA)
        sx = MALE if unit(salt(TAG_SEX, k)) < P_Y else FEMALE
        age = bisect_right(cdf[sx], unit(salt(TAG_AGE, k)))
        p = person(sx, -age, k, k, -1, -1, age / MONTHS)
        kin.founder(slot[p])
    # A rough start for the founders (only the burn-in sees it): partnered with a
    # probability by age to a single founder man 2 years younger to 8 years older, and a
    # share of the wish already born (without persons).
    adult_years = market_age / MONTHS
    founders_by_age = {}
    for p in range(n0):
        if sex[p] == MALE and -birth[p] >= market_age:
            founders_by_age.setdefault(-birth[p] // MONTHS, []).append(p)
    for w in sorted(range(n0), key=lambda p: key[p]):
        if sex[w] != FEMALE:
            continue
        age = -birth[w] / MONTHS
        if age < adult_years:
            continue
        share = min(1.0, (age - adult_years) / 20.0)
        u = unit(salt(TAG_PRIOR, key[w]))
        vcount[w] = min(wish[w], int(wish[w] * share + u))
        if age < adult_years + 10.0:
            prob = 0.8 * (age - adult_years) / 10.0
        elif age < 60:
            prob = 0.8
        else:
            prob = max(0.3, 0.8 - 0.5 * (age - 60.0) / 25.0)
        if unit(salt(TAG_PAIR0, key[w])) >= prob:
            continue
        best, best_rank = -1, M64 + 1
        for y in range(int(age) - 2, int(age) + 9):
            for m in founders_by_age.get(y, ()):
                if partner_of[m] < 0:
                    r = mix(key[w] ^ key[m])
                    if r < best_rank:
                        best, best_rank = m, r
        if best >= 0:
            form(w, best, 0)
            first[w] = first[best] = -2
    for sx in (FEMALE, MALE):
        for p in range(n0):
            if sex[p] == sx and -birth[p] >= market_age and partner_of[p] < 0:
                singles[sx].add(p)

    snapshots = []

    # The months.
    for t in range(t_end):
        # Same-second order (ADR-0018 section 1): births, then the market round, then the
        # conception attempts, then separations, then deaths. Death and separation times
        # here are continuous (closed-form ages and Exp(1) budgets scaled to months, never
        # rounded to the month grid), so one at exactly a month start t has probability
        # zero (double rounding aside, about 1e-13 per event); taking those in (t - 1, t]
        # before the births and the round at t is the ADR order. The one grid-aligned case,
        # a budget of exactly 0 (`unit` giving 1.0, 2^-53), puts a newborn's death at its
        # birth month start or a separation at its round, and both are taken at t + 1,
        # after that round, as in the ADR. The conception attempts come before the round; that
        # is the same as after it, because a partnership formed in the round tries only
        # from the next month start and an attempt changes no one's single status.
        while heap and heap[0][0] <= t:
            _, kind, x = heapq.heappop(heap)
            if kind == 0:
                singles[sex[x]].discard(x)
                market[sex[x]].discard(x)
                if sex[x] == FEMALE:
                    fert.discard(x)
                q = partner_of[x]
                if q >= 0:
                    close(q)
                    other = q_m[q] if q_w[q] == x else q_w[q]
                    singles[sex[other]].add(other)
                free_slot(x)
            elif q_open[x]:
                close(x)
                singles[FEMALE].add(q_w[x])
                singles[MALE].add(q_m[x])
        # Births at this month start.
        for w, fa, nc, is_actual in births_at.pop(t, ()):
            if is_actual:
                pend[fa] -= 1
            if death[w] <= t:
                if is_actual and death[fa] <= t:
                    free_slot(fa)
                continue
            k0 = mix(mix(key[w] + TAG_CHILD * GC) + nc * GA)
            u = unit(salt(TAG_TWIN, k0))
            size = 2 if u < TWIN_ALL else 1
            vcount[w] += size
            vpreg[w] = False
            vlast[w] = t
            if cumv[w] is None:
                cumv[w] = []
            cumv[w].append(vcount[w])
            if is_actual:
                if first_birth[w] < 0:
                    first_birth[w] = t
                actual[w] += size
                for idx in range(size):
                    k = k0 if idx == 0 else mix(mix(key[w] + TAG_CHILD * GC) + nc * GA + idx * GB)
                    gk = k0 if (idx == 1 and u < TWIN_MZ) else k
                    sx = MALE if unit(salt(TAG_SEX, gk)) < P_Y else FEMALE
                    c = person(sx, t, k, gk, w, fa, 0.0)
                    kin.child(slot[c], slot[w], slot[fa])
                if death[fa] <= t:
                    free_slot(fa)
            if vcount[w] >= 4:
                fert.discard(w)
        # Conception attempts (only partnerships formed before this month start).
        if fert:
            cs = salt(TAG_CONCEIVE, t)
            drop = []
            for w in fert:
                if vpreg[w] or t < vlast[w] + PAUSE:
                    continue
                am = t - birth[w]
                if am >= fert_until:
                    drop.append(w)
                    continue
                fa = q_m[partner_of[w]]
                f = chance(am / MONTHS, key[w], (t - birth[fa]) / MONTHS, key[fa])
                if f <= 0.0:
                    continue
                x = cs ^ key[w]
                x = ((x ^ (x >> 30)) * F1) & M64
                x = ((x ^ (x >> 27)) * F2) & M64
                x ^= x >> 31
                if ((x >> 11) + 0.5) * U53 < f:
                    vpreg[w] = True
                    is_actual = vcount[w] < wish[w]
                    if is_actual:
                        pend[fa] += 1
                    births_at.setdefault(t + PREGNANCY, []).append((w, fa, nconc[w], is_actual))
                    nconc[w] += 1
            for w in drop:
                fert.discard(w)
        # New adults join the market.
        for p in adult_at.pop(t, ()):
            if death[p] > t and partner_of[p] < 0:
                singles[sex[p]].add(p)
        if female_market:
            # The market of Grok's R8 (the female-dominant market of demographic
            # microsimulation): only single women search, with s_f(a) times rho from 16
            # until before 18; the candidates are the single men of the round's snapshot.
            held = (sorted(singles[FEMALE]), sorted(singles[MALE]))
            ss = salt(TAG_SEARCH, t)
            tab = tabs[FEMALE]
            seekers = []
            for p in held[FEMALE]:
                am = t - birth[p]
                if am < tab_len and tab[am] > 0.0 and unit(mix(ss ^ key[p])) < tab[am]:
                    seekers.append(p)
        else:
            # Market round (Grok, R7): single persons not on the market enter it with e(a);
            # then everyone on the market searches with q; the candidates are the persons of
            # the other sex on the market at the start of the round.
            ss = salt(TAG_SEARCH, t)
            for sx in (FEMALE, MALE):
                tab = tabs[sx]
                mk = market[sx]
                for p in sorted(singles[sx]):
                    if p in mk:
                        continue
                    am = t - birth[p]
                    if am >= tab_len:
                        continue
                    e = tab[am]
                    if e <= 0.0:
                        continue
                    x = ss ^ key[p]
                    x = ((x ^ (x >> 30)) * F1) & M64
                    x = ((x ^ (x >> 27)) * F2) & M64
                    x ^= x >> 31
                    if ((x >> 11) + 0.5) * U53 < e:
                        mk.add(p)
            held = (sorted(market[FEMALE]), sorted(market[MALE]))
            if t >= burn:
                stats_market[0] += 1
                stats_market[1] += len(held[FEMALE])
                stats_market[2] += len(held[MALE])
            qs = salt(TAG_QSEARCH, t)
            seekers = [p for sx in (FEMALE, MALE) for p in held[sx]
                       if unit(mix(qs ^ key[p])) < Q_SEARCH]
        if seekers:
            os_ = salt(TAG_ORDER, town, t)
            seekers.sort(key=lambda p: mix(os_ ^ key[p]))
            paired = set()
            q_kin = kin.q
            cap = kin.cap
            for i in seekers:
                if i in paired:
                    continue
                pool = held[1 - sex[i]]
                if not pool:
                    continue
                rs = salt(TAG_RANK, key[i], t)
                ranked = heapq.nsmallest(3 * K_CAND, pool, key=lambda j: mix(rs ^ key[j]))
                row = slot[i] * cap
                cands = [j for j in ranked if q_kin[row + slot[j]] < EIGHTH][:K_CAND]
                if len(cands) < K_CAND and len(ranked) < len(pool):
                    ranked = sorted(pool, key=lambda j: mix(rs ^ key[j]))
                    cands = [j for j in ranked if q_kin[row + slot[j]] < EIGHTH][:K_CAND]
                if t >= burn:
                    stats_cands[0] += 1
                    stats_cands[1] += len(cands)
                    stats_cands[2] += not cands
                if not cands:
                    continue
                ns = salt(TAG_NOISE, key[i], t)
                bi = birth[i]
                # z_K of both at their ages at the round (4.2, 11.2); z_h is age-free.
                cw = COG_BY_AGE[min(t - bi, COG_LAST)]
                zi_k = cw[0] * kg[i] + cw[1] * kc[i] + cw[2] * ke[i]
                zi_h = zh[i]
                male_i = sex[i] == MALE
                best, best_u = -1, -math.inf
                for j in cands:
                    da = (birth[j] - bi) / MONTHS if male_i else (bi - birth[j]) / MONTHS
                    cw = COG_BY_AGE[min(t - birth[j], COG_LAST)]
                    dk = zi_k - (cw[0] * kg[j] + cw[1] * kc[j] + cw[2] * ke[j])
                    dh = zi_h - zh[j]
                    gumbel = -math.log(-math.log(unit_open(mix(ns ^ key[j]))))
                    u = (-w_k * dk * dk - w_h * dh * dh - w_a * (da - mu_a) ** 2
                         - KIN_PENALTY * q_kin[row + slot[j]] / ONE + gumbel)
                    if u > best_u:
                        best, best_u = j, u
                if best in paired:
                    continue
                paired.add(i)
                paired.add(best)
                if male_i:
                    form(best, i, t)
                else:
                    form(i, best, t)
        if t % (25 * MONTHS) == 0:
            snapshots.append((t, census(t, birth, death, partner_of)))
    snapshots.append((t_end, census(t_end, birth, death, partner_of)))

    # Measurement.
    out = {
        "hist": [[0] * (50 * MONTHS) for _ in range(2)],
        "never": [0, 0],
        "hist_half": [[[0] * (50 * MONTHS) for _ in range(2)] for _ in range(2)],
        "never_half": [[0, 0], [0, 0]],
        "bsum": [0.0] * 5,
        "bsq": [[0.0] * 5 for _ in range(5)],
        "nwomen": 0,
        "births_real": [0, 0],
        "bsum_half": [[0.0] * 5, [0.0] * 5],
        "nwomen_half": [0, 0],
        "first_birth": [0, 0.0],
        "gap": stats_gap,
        "gap_first": stats_gap_first,
        "gap_hist": gap_hist,
        "corr": stats_corr,
        "cands": stats_cands,
        "market": stats_market,
        "sep_d": sep_d,
        "sep_real": sep_real,
        "asfr_births": [0] * 60,
        "asfr_years": [0.0] * 60,
        "persons": len(sex),
        "snapshots": snapshots,
    }
    hist, never = out["hist"], out["never"]
    for p in range(len(sex)):
        b = birth[p]
        if sex[p] == FEMALE:
            # Exposure and births for the period fertility rate after the burn-in.
            lo, hi = max(b, burn), min(death[p], t_end)
            if hi > lo:
                for y in range(15, 50):
                    a0, a1 = max(lo, b + y * MONTHS), min(hi, b + (y + 1) * MONTHS)
                    if a1 > a0:
                        out["asfr_years"][y] += (a1 - a0) / MONTHS
        if b < burn or b >= cohort_end or death[p] <= b + 50 * MONTHS:
            continue
        half = 0 if b < split else 1
        f = first[p]
        if 0 <= f < 50 * MONTHS:
            hist[sex[p]][f] += 1
            out["hist_half"][half][sex[p]][f] += 1
        else:
            never[sex[p]] += 1
            out["never_half"][half][sex[p]] += 1
        if sex[p] == FEMALE:
            seq = cumv[p] or []
            bn = [0.0] * 5
            for n in range(1, 5):
                bn[n] = float(next((c for c in seq if c >= n), seq[-1] if seq else 0))
            out["nwomen"] += 1
            out["nwomen_half"][half] += 1
            for n in range(5):
                out["bsum"][n] += bn[n]
                out["bsum_half"][half][n] += bn[n]
                for n2 in range(5):
                    out["bsq"][n][n2] += bn[n] * bn[n2]
            out["births_real"][0] += 1
            out["births_real"][1] += actual[p]
            if first_birth[p] >= 0:
                out["first_birth"][0] += 1
                out["first_birth"][1] += (first_birth[p] - b) / MONTHS
    for c in range(len(sex)):
        mo = mother[c]
        if mo >= 0 and burn <= birth[c] < t_end:
            out["asfr_births"][(birth[c] - birth[mo]) // MONTHS] += 1
    return out


def census(t, birth, death, partner_of):
    alive = 0
    young = old = 0
    age_sum = 0.0
    for p in range(len(birth)):
        if birth[p] <= t < death[p]:
            alive += 1
            a = (t - birth[p]) / MONTHS
            age_sum += a
            if a < 18:
                young += 1
            elif a >= 65:
                old += 1
    return (alive, age_sum / max(alive, 1), young / max(alive, 1), old / max(alive, 1))


# ---------------------------------------------------------------------------------------
# Aggregation and the measured values

def add_into(acc, x):
    if isinstance(x, list) and x and isinstance(x[0], list):
        for a, b in zip(acc, x):
            add_into(a, b)
    else:
        for i, v in enumerate(x):
            acc[i] += v


def aggregate(outs):
    agg = {}
    for key in outs[0]:
        if key == "sep_d":
            agg[key] = [d for o in outs for d in o[key]]
        elif key == "snapshots":
            agg[key] = [o[key] for o in outs]
        elif key in ("persons", "nwomen"):
            agg[key] = sum(o[key] for o in outs)
        else:
            acc = json.loads(json.dumps(outs[0][key]))
            for o in outs[1:]:
                add_into(acc, o[key])
            agg[key] = acc
    return agg


def median(hist, never):
    """(smoothed, plain): the plain Kaplan-Meier median is the first age (on the month
    grid) at which half of all persons had a first partnership; the smoothed one spreads
    each month's count over half a month on either side, so it moves continuously."""
    n = sum(hist) + never
    half = n / 2.0
    c = 0
    for k, h in enumerate(hist):
        if h and c + h >= half:
            return (k - 0.5 + (half - c) / h) / MONTHS, k / MONTHS
        c += h
    return math.inf, math.inf


def median_se(hist, never, med):
    n = sum(hist) + never
    lo = max(0, int((med - 1.0) * MONTHS))
    hi = min(len(hist), int((med + 1.0) * MONTHS) + 1)
    dens = sum(hist[lo:hi]) / n / ((hi - lo) / MONTHS)
    return math.sqrt(0.25 / n) / dens


def births_rb(agg, alpha):
    p = wish_probs(alpha)
    n = agg["nwomen"]
    mean = sum(pi * s for pi, s in zip(p, agg["bsum"])) / n
    sq = sum(p[i] * p[j] * agg["bsq"][i][j] for i in range(5) for j in range(5))
    var = (sq - n * mean * mean) / (n - 1)
    return mean, math.sqrt(max(var, 0.0) / n)


def sep_rb(agg, lam):
    ds = agg["sep_d"]
    vals = [1.0 - math.exp(-lam * d) for d in ds]
    n = len(vals)
    mean = sum(vals) / n
    var = sum((v - mean) ** 2 for v in vals) / (n - 1)
    return mean, math.sqrt(var / n)


def correlation(c, off):
    n = c[0]
    sx, sy, sxx, syy, sxy = c[off:off + 5]
    cov = sxy / n - sx * sy / n / n
    vx = sxx / n - (sx / n) ** 2
    vy = syy / n - (sy / n) ** 2
    r = cov / math.sqrt(vx * vy)
    return r, (1.0 - r * r) / math.sqrt(n - 3)


def measure(agg, par):
    m = {}
    for sx, tag in ((FEMALE, "f"), (MALE, "m")):
        hist, never = agg["hist"][sx], agg["never"][sx]
        n = sum(hist) + never
        smooth, plain = median(hist, never)
        m["med_" + tag] = (smooth, median_se(hist, never, smooth))
        m["km_" + tag] = plain
        p = never / n
        m["never_" + tag] = (p, math.sqrt(p * (1 - p) / n))
        m["n_" + tag] = n
        for half in (0, 1):
            h, nv = agg["hist_half"][half][sx], agg["never_half"][half][sx]
            m["med_%s_%d" % (tag, half)] = median(h, nv)[0]
            m["never_%s_%d" % (tag, half)] = nv / (sum(h) + nv)
    m["r_K"] = correlation(agg["corr"], 1)
    m["r_H"] = correlation(agg["corr"], 6)
    m["n_first"] = agg["corr"][0]
    ns, nc, n0 = agg["cands"]
    m["cands_mean"] = nc / ns
    m["cands_none"] = n0 / ns
    nr, mf, mm = agg["market"]
    m["market_f"] = mf / nr if nr else math.nan
    m["market_m"] = mm / nr if nr else math.nan
    m["births"] = births_rb(agg, par["alpha"])
    nr, br = agg["births_real"]
    m["births_real"] = br / nr
    p = wish_probs(par["alpha"])
    for half in (0, 1):
        nw = agg["nwomen_half"][half]
        m["births_%d" % half] = sum(pi * s for pi, s in zip(p, agg["bsum_half"][half])) / nw
    m["n_women"] = agg["nwomen"]
    m["sep"] = sep_rb(agg, par["lam"])
    s, d, open_ = agg["sep_real"]
    m["sep_real"] = s / (s + d)
    m["n_part"] = s + d
    m["n_open"] = open_
    n, g, g2 = agg["gap"]
    m["gap"] = (g / n, math.sqrt((g2 / n - (g / n) ** 2) / n))
    m["gap_sd"] = math.sqrt(g2 / n - (g / n) ** 2)
    nf, gf = agg["gap_first"]
    m["gap_first"] = gf / nf
    hist = agg["gap_hist"]
    m["gap_woman_older"] = sum(hist[:100]) / n
    m["gap_over_10"] = (sum(hist[:90]) + sum(hist[110:])) / n
    c = 0
    for i, h in enumerate(hist):
        c += h
        if c >= n / 2:
            m["gap_median_bin"] = i - 100
            break
    nb, sb = agg["first_birth"]
    m["first_birth"] = sb / nb
    m["tfr"] = sum(b / y for b, y in zip(agg["asfr_births"], agg["asfr_years"]) if y > 0)
    births = agg["asfr_births"]
    m["u18"] = sum(births[:18]) / sum(births)  # live births to mothers under 18, after burn-in
    m["wish_mean"] = wish_mean(par["alpha"])
    return m


# The residuals of the outer fit; never_f last, only to choose among the solutions of the
# others (small weight in TSCALE).
OUTER = ("med_f", "med_m", "never_m", "u18", "r_K", "r_H", "never_f")
REPORTED = ("med_f", "med_m", "never_m", "u18", "births", "sep", "r_K", "r_H")


def value(m, k):
    v = m[k]
    return v[0] if isinstance(v, tuple) else v


def residual(m):
    return [value(m, k) - TARGET[k] for k in OUTER]


def derived(m):
    """Values for the report that are differences of measured ones."""
    out = {}
    for k in ("med_f", "med_m", "never_f", "never_m", "births"):
        out["drift_" + k] = m[k + "_1"] - m[k + "_0"]
    return out


def jackknife(outs, par, keys):
    """Standard errors by the delete-one-town jackknife: they include the dependence of
    persons within a town (market, kin, cohort sizes), unlike the analytic ones."""
    n = len(outs)
    if n < 3:
        return {}
    rows = []
    for i in range(n):
        m = measure(aggregate(outs[:i] + outs[i + 1:]), par)
        m.update(derived(m))
        rows.append(m)
    se = {}
    for k in keys:
        vals = [value(m, k) for m in rows]
        mean = sum(vals) / n
        se[k] = math.sqrt((n - 1) / n * sum((v - mean) ** 2 for v in vals))
    return se


# ---------------------------------------------------------------------------------------
# Fitting

def root(f, lo, hi, tol=1e-12):
    """Bisection of an increasing function on [lo, hi]; None without a sign change."""
    if f(lo) > 0 or f(hi) < 0:
        return None
    for _ in range(200):
        mid = 0.5 * (lo + hi)
        if f(mid) < 0:
            lo = mid
        else:
            hi = mid
        if hi - lo < tol:
            break
    return 0.5 * (lo + hi)


class WishTooHigh(Exception):
    pass


ALPHA_MAX = root(lambda a: wish_mean(a) - WISH_MAX_MEAN, -5.0, 5.0)  # wish mean 3.0


def inner(agg, strict=False):
    """alpha and lambda_sep solved exactly on one sample. If the births target needs a
    wish mean above 3.0 (11.3), the search holds alpha at that limit; at the fitted point
    (`strict`) the tool stops."""
    alpha = root(lambda a: births_rb(agg, a)[0] - TARGET["births"], -5.0, 5.0)
    lam = root(lambda x: sep_rb(agg, x)[0] - TARGET["sep"], 1e-7, 1.0)
    if alpha is None or alpha > ALPHA_MAX:
        if strict:
            raise WishTooHigh(alpha)
        alpha = ALPHA_MAX if alpha is None or alpha > ALPHA_MAX else alpha
    return alpha, lam


XNAMES = ("c_f", "m_f", "c_m", "m_m", "rho", "w_K", "w_H")  # the outer parameters
LOGX = (True, False, True, False, True, True, True)  # fitted as logarithms
RHO_INDEX = 4  # ln rho stays at most 0 (rho in (0, 1], R6b)


def params(x, alpha, lam):
    par = {k: (math.exp(v) if lg else v) for k, v, lg in zip(XNAMES, x, LOGX)}
    par["rho"] = min(1.0, par["rho"])
    par["alpha"] = alpha
    par["lam"] = lam
    return par


def run_town(args):
    par, seed, cfg = args
    return simulate(par, seed, cfg)


class Runner:
    """Runs towns in worker processes; results are combined in task order, so they do not
    depend on the number of workers."""

    def __init__(self, cfg, jobs):
        self.cfg = cfg
        self.pool = ProcessPoolExecutor(max_workers=jobs) if jobs > 1 else None
        self.towns = 0

    def evaluate(self, pars, seeds, cfg=None):
        tasks = [(p, s, cfg or self.cfg) for p in pars for s in seeds]
        if self.pool:
            outs = list(self.pool.map(run_town, tasks))
        else:
            outs = [run_town(t) for t in tasks]
        self.towns += len(tasks)
        n = len(seeds)
        result = []
        for i, p in enumerate(pars):
            part = outs[i * n:(i + 1) * n]
            agg = aggregate(part)
            result.append((agg, measure(agg, p), part))
        return result


def solve_linear(a, b):
    n = len(b)
    m = [row[:] + [b[i]] for i, row in enumerate(a)]
    for col in range(n):
        piv = max(range(col, n), key=lambda r: abs(m[r][col]))
        m[col], m[piv] = m[piv], m[col]
        for r in range(n):
            if r != col:
                f = m[r][col] / m[col][col]
                for c in range(col, n + 1):
                    m[r][c] -= f * m[col][c]
    return [m[i][n] / m[i][i] for i in range(n)]


SCALE = (0.15, 1.5, 0.15, 1.5, 0.5, 0.25, 0.25)  # trust-region unit of x (ln or years)
# Per-town standard deviations of the outer residuals (development runs). They weight the
# residuals in the least-squares step, which matters only where not all can be met;
# never_f gets ten times its own, so it only picks among the solutions of the others.
TSCALE = (0.48, 0.165, 0.0069, 0.0008, 0.021, 0.021, 0.20)
RHO0, RHO_MIN, RHO_DECAY = 2.0, 1.0, 0.85  # trust radius in SCALE units and its schedule
MU, MU_FINAL = 0.05, 0.01  # Levenberg-Marquardt regularization in SCALE units


def log(msg):
    print(msg, flush=True)


def describe(label, x, alpha, lam, m):
    p = params(x, alpha, lam)
    log("  %-6s %s alpha %.4f lam %.5f" % (label, " ".join(
        "%s %.5g" % (k, p[k]) for k in XNAMES), alpha, lam))
    log("         %s | never_f %.4f gap %.3f med_m %.3f never_m %.4f" % (" ".join(
        "%s %.5g" % (k, value(m, k)) for k in REPORTED), value(m, "never_f"),
        value(m, "gap"), value(m, "med_m"), value(m, "never_m")))


def seeds_of(k, towns):
    return [FIT_SEED0 + 1000 * k + i for i in range(towns)]


def regress(points, xc, rho):
    """Weighted least squares of the outer residuals on (x - xc) / SCALE over all runs;
    weight = towns x Gaussian kernel of radius rho around xc. Returns the intercepts (the
    model's residuals at xc), the Jacobian rows (per SCALE unit), the covariance matrix of
    the intercepts across the targets (one set of towns gives all of them) and the
    effective number of runs. The residual (co)variance includes any lack of fit, and the
    kernel weights are treated as precision weights, so the covariances are approximate
    and the variances err on the large side. The radius grows until at least
    2 (nx + 1) runs count, so the model never rests on a handful of runs."""
    nx = len(SCALE)
    while True:
        neff = sum(math.exp(-0.5 * sum(((xi - ci) / s) ** 2 for xi, ci, s in zip(
            x, xc, SCALE)) / (rho * rho)) for x, _, _ in points)
        if neff >= 2 * (nx + 1) or rho > 20.0:
            break
        rho *= 1.25
    return regress_at(points, xc, rho)


def regress_at(points, xc, rho):
    nx, nout = len(SCALE), len(OUTER)
    p = nx + 1
    a = [[0.0] * p for _ in range(p)]
    rhs = [[0.0] * p for _ in range(nout)]
    rows = []
    for x, g, n in points:
        u = [(xi - ci) / s for xi, ci, s in zip(x, xc, SCALE)]
        k = math.exp(-0.5 * sum(v * v for v in u) / (rho * rho))
        row = [1.0] + u
        rows.append((row, g, n * k, k))
        for i in range(p):
            for j in range(p):
                a[i][j] += n * k * row[i] * row[j]
            for t in range(nout):
                rhs[t][i] += n * k * row[i] * g[t]
    beta = [solve_linear(a, rhs[t]) for t in range(nout)]
    neff = sum(r[3] for r in rows)
    inv00 = solve_linear(a, [1.0] + [0.0] * (p - 1))[0]
    res = [[g[t] - sum(b * r for b, r in zip(beta[t], row)) for t in range(nout)]
           for row, g, _, _ in rows]
    f = inv00 / max(neff - p, 1.0)
    cov_a = [[f * sum(r[2] * e[t] * e[s] for r, e in zip(rows, res)) for s in range(nout)]
             for t in range(nout)]
    return [b[0] for b in beta], [b[1:] for b in beta], cov_a, neff


def lm_gain(jac, mu):
    """G = (Jt'Jt + mu I)^-1 Jt' with Jt = J / TSCALE (rows); the step is d = -G a/TSCALE."""
    nx, nout = len(SCALE), len(OUTER)
    jt = [[jac[t][i] / TSCALE[t] for i in range(nx)] for t in range(nout)]
    m = [[sum(jt[t][i] * jt[t][j] for t in range(nout)) + (mu if i == j else 0.0)
          for j in range(nx)] for i in range(nx)]
    cols = [solve_linear(m, [jt[t][i] for i in range(nx)]) for t in range(nout)]
    return [[cols[t][i] for t in range(nout)] for i in range(nx)]


def lm_step(a, jac, mu, rho):
    """The step d (SCALE units) minimizing sum(((a + J d) / TSCALE)^2) + mu |d|^2,
    shortened to length rho in the largest coordinate."""
    g = lm_gain(jac, mu)
    d = [-sum(row[t] * a[t] / TSCALE[t] for t in range(len(OUTER))) for row in g]
    big = max(abs(v) for v in d)
    if big > rho:
        d = [v * rho / big for v in d]
    return d, big > rho


def fit(runner, x, alpha, lam, cfg):
    """A trust-region search with a local regression model. A design of runs one SCALE
    unit away along each axis gives the first model; each iteration fits the outer
    residuals linearly in the parameters by weighted least squares over all runs near the
    current point, takes a Levenberg-Marquardt step towards the model's least-squares
    point and runs fresh towns there. The fitted values are that point of the final local
    model. A single fixed set of towns would not help: one changed search decision changes
    every later market round, so nearby parameters on the same towns differ as much as on
    new ones. alpha and lambda_sep are solved exactly on the previous run, at the end on a
    run of new towns at the fitted point."""
    towns = cfg["towns"]
    nx, nout = len(SCALE), len(OUTER)
    points, runs, clipped = [], [], []

    def record(x, alpha, lam, res, label):
        agg, m, outs = res
        points.append((x[:], residual(m), len(outs)))
        runs.append((x[:], alpha, lam, agg, m, outs))
        describe(label, x, alpha, lam, m)

    log("Start, and a design one SCALE unit away along each axis (one set of towns):")
    record(x, alpha, lam, runner.evaluate([params(x, alpha, lam)], seeds_of(0, towns))[0],
           "start")
    alpha, lam = inner(runs[-1][3])
    xs = []
    for i in range(nx):
        for sgn in (1.0, -1.0):
            xi = x[:]
            xi[i] += sgn * SCALE[i]
            xi[RHO_INDEX] = min(0.0, xi[RHO_INDEX])
            xs.append(xi)
    res = runner.evaluate([params(xi, alpha, lam) for xi in xs], seeds_of(1, towns))
    for xi, r in zip(xs, res):
        record(xi, alpha, lam, r, "design")
    xc = x[:]
    log("Iterations (fresh towns each):")
    for k in range(1, cfg["iters"] + 1):
        rho = max(RHO_MIN, RHO0 * RHO_DECAY ** k)
        a, jac, _, _ = regress(points, xc, rho)
        d, short = lm_step(a, jac, MU, rho)
        clipped.append(short)
        xc = [c + s * v for c, s, v in zip(xc, SCALE, d)]
        xc[RHO_INDEX] = min(0.0, xc[RHO_INDEX])
        alpha, lam = inner(runs[-1][3])
        res = runner.evaluate([params(xc, alpha, lam)], seeds_of(k + 1, towns))[0]
        record(xc, alpha, lam, res, "it %d" % k)
    a, jac, cov_a, neff = regress(points, xc, RHO_MIN)
    var_a = [cov_a[t][t] for t in range(nout)]
    d, short = lm_step(a, jac, MU_FINAL, RHO_MIN)
    clipped.append(short)
    x_fit = [c + s * v for c, s, v in zip(xc, SCALE, d)]
    x_fit[RHO_INDEX] = min(0.0, x_fit[RHO_INDEX])
    model = [a[t] + sum(jac[t][i] * d[i] for i in range(nx)) for t in range(nout)]
    # Approximate standard errors of the fitted point: the delta method on the weighted
    # least-squares step with the intercepts' covariance across the targets; the
    # Jacobian's own noise and the kernel weights are not modeled.
    gain = lm_gain(jac, MU_FINAL)
    se_x = []
    for i in range(nx):
        gi = [gain[i][t] / TSCALE[t] for t in range(nout)]
        v = sum(gi[t] * gi[s] * cov_a[t][s] for t in range(nout) for s in range(nout))
        se_x.append(SCALE[i] * math.sqrt(max(v, 0.0)))
    # alpha and lambda_sep settled at the fitted point on new towns.
    log("Settling alpha and lambda_sep at the fitted point:")
    alpha, lam = inner(aggregate([o for r in runs[-4:] for o in r[5]]))
    res = runner.evaluate([params(x_fit, alpha, lam)],
                          seeds_of(cfg["iters"] + 2, cfg["settle_towns"]))[0]
    describe("settle", x_fit, alpha, lam, res[1])
    alpha, lam = inner(res[0], strict=True)
    fitted = {"x": x_fit, "alpha": alpha, "lam": lam, "model": model, "se_x": se_x,
              "model_se": [math.sqrt(v) for v in var_a], "jac": jac, "neff": neff,
              "pooled": res[0], "settle": res[2], "runs": runs,
              "clipped": sum(clipped[-5:])}
    return fitted


def fit_errors(fitted, par):
    """Approximate standard errors of the fitted values, not full Monte Carlo errors. The
    six outer ones come from the final local model (see `fit`). alpha's and lambda_sep's
    are the delete-one-town jackknife of their solution on the settle run, so they carry
    the dependence within a town but not the uncertainty of the outer six. The check run's
    town jackknife on the targets is the authoritative check."""
    out = {}
    for name, lg, se in zip(XNAMES, LOGX, fitted["se_x"]):
        out[name] = par[name] * se if lg else se
    outs = fitted["settle"]
    n = len(outs)
    sols = {"alpha": [], "lam": []}
    for i in range(n):
        agg = aggregate(outs[:i] + outs[i + 1:])
        sols["alpha"].append(root(lambda a: births_rb(agg, a)[0] - TARGET["births"], -5.0, 5.0))
        ds = agg["sep_d"]
        sols["lam"].append(root(lambda x: 1.0 - sum(math.exp(-x * d) for d in ds) / len(ds)
                                - TARGET["sep"], 1e-7, 1.0))
    for name, vals in sols.items():
        if n < 3 or None in vals:
            out[name] = math.nan
            continue
        mean = sum(vals) / n
        out[name] = math.sqrt((n - 1) / n * sum((v - mean) ** 2 for v in vals))
    return out


# ---------------------------------------------------------------------------------------
# Report

FULL = {"founders": 1000, "years": 300, "burn_in": 100, "towns": 12, "iters": 24,
        "settle_towns": 24, "check_towns": 48}
QUICK = {"founders": 1000, "years": 200, "burn_in": 80, "towns": 4, "iters": 3,
         "settle_towns": 4, "check_towns": 4}
FIT_SEED0, CHECK_SEED0 = 1, 900001
WK_SCAN = (2.0, 10.0, 50.0, 250.0)  # w_K values of the correlation scan in the report
# Start of the fit, from development runs; the result does not depend on it beyond its
# standard error.
X0 = [math.log(0.014), 27.0, math.log(0.012), 27.5, math.log(0.12), math.log(1.5),
      math.log(0.6)]
ALPHA0, LAM0 = 0.25, 0.0086
SMALL_FOUNDERS, SMALL_TOWNS = 100, 96  # the extra check run on small towns (R7)
WA_SCAN = (0.02, 0.05, 0.1, 0.2)  # w_a values of the scan of the female market

# The rule of Grok's R8 (the default, cfg["market"] = "female"): only women search; the
# outer fit has c and m of s_f, rho, mu_a, w_K and w_H for the women's median and never
# share, the births under 18, the mean age gap and the two correlations. The men's
# median and never share are emergent. R7 stays behind `--r7` for comparison.
FEMALE_MODE = {
    "XNAMES": ("c_f", "m_f", "rho", "mu_a", "w_K", "w_H"),
    "LOGX": (True, False, True, False, True, True),
    "RHO_INDEX": 2,
    "SCALE": (0.15, 1.5, 0.5, 1.0, 0.25, 0.25),
    "TSCALE": (0.48, 0.020, 0.0008, 0.21, 0.021, 0.021),
    "OUTER": ("med_f", "never_f", "u18", "gap", "r_K", "r_H"),
    "REPORTED": ("med_f", "never_f", "u18", "gap", "births", "sep", "r_K", "r_H"),
    "X0": [math.log(0.0118), 25.25, math.log(0.0935), 5.0, math.log(0.388),
           math.log(0.167)],
}


def set_mode(mode):
    """Rebinds the fit's globals to a mode table (main process only; the towns read
    only their parameters and cfg)."""
    globals().update(mode)
FIT_DATE = "2026-10-10"
NAMES = ("c_f", "m_f", "c_m", "m_m", "rho", "mu_a", "alpha", "lam", "w_K", "w_H")
MEANING = {
    "c_f": "height c of e(a), women, per month", "m_f": "center m of e(a), women, years",
    "c_m": "height c of e(a), men, per month", "m_m": "center m of e(a), men, years",
    "rho": "factor on e(a) from 16 until before 18, both sexes",
    "alpha": "tilt of the wish distribution", "lam": "separation rate lambda_sep per year",
    "w_K": "similarity weight w_K (cognition)", "w_H": "similarity weight w_H (height z_h)",
}
FEMALE_MODE["MEANING"] = dict(MEANING, c_f="height c of s_f(a), women, per month",
                              m_f="center m of s_f(a), women, years",
                              rho="factor on s_f(a) from 16 until before 18",
                              mu_a="preferred age gap mu_a (man older), years")
LABELS = {
    "med_f": "median age at first partnership, women",
    "never_f": "share without partnership by 50, women",
    "med_m": "median age at first partnership, men",
    "never_m": "share without partnership by 50, men",
    "u18": "share of live births to mothers under 18",
    "gap": "mean age gap man minus woman, all partnerships",
    "first_birth": "mean age of mothers at first birth",
    "tfr": "total fertility rate",
    "births": "mean live births, women reaching 50",
    "births_real": "live births per woman reaching 50, realized",
    "sep": "share of partnerships ending by separation",
    "r_K": "partner correlation z_K, first partnerships",
    "r_H": "partner correlation z_h, first partnerships",
}
EXTRA = ("never_f", "med_m", "never_m", "gap", "first_birth", "tfr", "births_real", "sep_real",
         "drift_med_f", "drift_med_m", "drift_never_f", "drift_never_m", "drift_births")
# The check run checks the births target on the realized live births, not on the search
# estimator of alpha (see the module docstring).
CHECKED_AS = {"births": "births_real"}
BIRTHS_BIAS_SE = 2.0  # realized births within 2 SE of 2.20, else fit alpha on them (review)


def sig(value, se):
    """Four significant digits, three when the fourth is noise by a factor of ten."""
    digits = 4
    if value != 0.0 and se == se:
        step = 10.0 ** (math.floor(math.log10(abs(value))) - 3)
        if se > 10.0 * step:
            digits = 3
    return float("%.*g" % (digits, value))


def german(x):
    return repr(x).replace(".", ",")


def report(cfg, par, se_par, fitted, chk, scan, small, runtime, towns_run, wa_scan=None):
    agg, m, outs = chk
    m.update(derived(m))
    se = jackknife(outs, par, REPORTED + EXTRA)
    if not se:
        se = {k: m[k][1] if isinstance(m[k], tuple) else math.nan for k in REPORTED + EXTRA}
    runs = fitted["runs"]
    log("")
    log("Fitted values: least-squares point of the local regression over %d runs (%.1f "
        "effective at the end); alpha and lambda_sep solved on %d new towns there; "
        "approximate standard errors:" % (len(runs), fitted["neff"], cfg["settle_towns"]))
    log("  %-6s %12s %10s  %s" % ("value", "fitted", "approx SE", "meaning"))
    for k in NAMES:
        if k in se_par:
            log("  %-6s %12.6g %10.2g  %s" % (k, par[k], se_par[k], MEANING[k]))
    log("  wish P_0..P_4 = %s, mean %.4f (limit 3.0)" % (
        ", ".join("%.4f" % p for p in wish_probs(par["alpha"])), wish_mean(par["alpha"])))
    log("  The standard errors are approximate, not full Monte Carlo errors: the outer values'")
    log("  from the final local model (delta method with the intercepts' covariance across the")
    log("  targets; kernel weights and the Jacobian's noise not modeled), alpha's and")
    log("  lambda_sep's from the delete-one-town jackknife of the settle run (without the")
    log("  uncertainty of the outer values). The authoritative check is the check run below.")
    log("")
    n_chk = cfg["check_towns"]
    log("Targets: the regression model at the fitted values (model; births and separation "
        "on the settle run), and a check run there on %d new towns (seeds %d-%d), SE by the "
        "delete-one-town jackknife:" % (n_chk, CHECK_SEED0, CHECK_SEED0 + n_chk - 1))
    log("  %-44s %8s %9s %9s %8s %7s" % ("target", "goal", "model", "check", "SE", "dev/SE"))
    model = dict(zip(OUTER, (TARGET[k] + v for k, v in zip(OUTER, fitted["model"]))))
    model["births"] = births_rb(fitted["pooled"], par["alpha"])[0]
    model["sep"] = sep_rb(fitted["pooled"], par["lam"])[0]
    reached = True
    for k in REPORTED:
        fit_mean = model[k]
        ck = CHECKED_AS.get(k, k)
        v = value(m, ck)
        dev = (v - TARGET[k]) / se[ck]
        reached = reached and abs(dev) <= 4.0
        log("  %-44s %8.4f %9.4f %9.4f %8.4f %7.2f" % (LABELS[k], TARGET[k], fit_mean, v,
                                                       se[ck], dev))
    missed = [k for k, v, s in zip(OUTER, fitted["model"], fitted["model_se"])
              if k != "never_f" and abs(v) > 3 * s]
    if "never_f" not in REPORTED:
        log("  share without partnership by 50, women (report only, not a target): model "
            "%.4f, check %.4f +- %.4f" % (model["never_f"], value(m, "never_f"),
                                          se["never_f"]))
    for k in ("med_m", "never_m"):
        if k not in REPORTED:
            log("  %s (emergent, not fitted): %.4f +- %.4f" % (LABELS[k], value(m, k), se[k]))
    if fitted["clipped"] >= 3:
        log("  NOT CONVERGED: %d of the last 5 steps were shortened to the trust radius; the "
            "search was still moving." % fitted["clipped"])
    if missed:
        log("  NO ROOT: the local model itself misses %s by more than 3 of its standard errors;"
            " the fitted point is its least-squares compromise (residuals weighted by the "
            "per-town standard deviations TSCALE)." % ", ".join(missed))
    log("  Medians are Kaplan-Meier medians among persons reaching 50 (no other censoring),")
    log("  with each month's count spread over half a month on either side; plain (first")
    log("  month with half reached): women %.3f, men %.3f." % (m["km_f"], m["km_m"]))
    log("  Births: the model column is the search estimator (the mean over the wish given each")
    log("  woman's history, solved to 2.20 on the settle run); the check column is the realized")
    log("  mean live births, the check of alpha. The search estimator on the check run: %.4f "
        "+- %.4f." % (value(m, "births"), se["births"]))
    log("  Separation: partnerships formed in years %d-%d, each counted by its eventual end; "
        "realized share %.4f +- %.4f; %d of %d still open at the end of the run." % (
            cfg["burn_in"], cfg["years"], m["sep_real"], se["sep_real"], m["n_open"],
            m["n_part"]))
    log("  Sample: %d women and %d men reaching 50 (born in years %d-%d), %d first "
        "partnerships, %d partnerships, %d persons simulated." % (
            m["n_f"], m["n_m"], cfg["burn_in"], cfg["years"] - 50, m["n_first"], m["n_part"],
            agg["persons"]))
    checks = []
    log("")
    log("Checks (check run):")
    g = m["gap"][0]
    ok = GAP_BAND[0] <= g <= GAP_BAND[1]
    checks.append(ok)
    log("  mean age gap man minus woman, all partnerships, w_a = %g: %.3f +- %.3f (2.5 to 4) "
        "%s" % (W_AGE, g, se["gap"], "ok" if ok else "FINDING"))
    log("    in first partnerships %.3f; SD %.2f; median in [%d, %d) years; woman older in "
        "%.3f; |gap| of 10 years or more in %.3f" % (
            m["gap_first"], m["gap_sd"], m["gap_median_bin"], m["gap_median_bin"] + 1,
            m["gap_woman_older"], m["gap_over_10"]))
    u18 = m["u18"]
    ok = U18_BAND[0] <= u18 <= U18_BAND[1]
    checks.append(ok)
    log("  share of live births to mothers under 18, years %d-%d: %.5f +- %.5f (fitted via "
        "rho to 0.0024; band 0.001 to 0.005) %s" % (
            cfg["burn_in"], cfg["years"], u18, se["u18"], "ok" if ok else "FINDING"))
    if par["rho"] >= 1.0:
        log("    rho is at its bound 1: the share cannot rise further by rho")
    b = m["births_real"]
    births_dev = (b - TARGET["births"]) / se["births_real"]
    births_ok = abs(births_dev) <= BIRTHS_BIAS_SE
    verdict = "ok" if births_ok else "FINDING: fit alpha on realized births"
    log("  realized live births per woman reaching 50, the check of alpha's search estimator "
        "(not an exact conditional expectation in a closed town): %.4f +- %.4f, %+.2f SE from "
        "2.20 (within %g SE) %s" % (b, se["births_real"], births_dev, BIRTHS_BIAS_SE, verdict))
    log("  candidates per searcher (after the burn-in): mean %.2f, none in "
        "%.3f of searches" % (m["cands_mean"], m["cands_none"]))
    log("  w_K scan at the fitted point (%d towns each): %s" % (
        cfg["settle_towns"], "; ".join("w_K %g r_K %.3f" % (w, value(ms, "r_K"))
                                       for w, ms in zip(WK_SCAN, scan))))
    if m["market_f"] == m["market_f"]:
        log("  persons on the market per round, mean: women %.1f, men %.1f" % (
            m["market_f"], m["market_m"]))
    if wa_scan:
        log("  w_a scan at the fitted point (%d towns each):" % cfg["settle_towns"])
        for w, ms in zip(WA_SCAN, wa_scan):
            log("    w_a %-5g gap mean %.3f SD %.2f, r_K %.3f, r_H %.3f, men's median %.3f, "
                "men's never share %.4f" % (w, value(ms, "gap"), ms["gap_sd"], value(ms, "r_K"),
                                            value(ms, "r_H"), value(ms, "med_m"),
                                            value(ms, "never_m")))
    log("  mean age of mothers at first birth (women reaching 50): %.2f +- %.2f (report)" % (
        m["first_birth"], se["first_birth"]))
    tfr = m["tfr"]
    log("  total fertility rate, years %d-%d: %.3f +- %.3f (2.1 to 2.3: %s)" % (
        cfg["burn_in"], cfg["years"], tfr, se["tfr"],
        "inside" if 2.1 <= tfr <= 2.3 else "REPORT ITEM"))
    log("")
    sagg, sm, souts = small
    sm.update(derived(sm))
    sse = jackknife(souts, par, REPORTED + EXTRA)
    log("Town size: the fitted values on %d towns of %d founders (the check run has %d):" % (
        len(souts), SMALL_FOUNDERS, cfg["founders"]))
    for k in REPORTED + tuple(k for k in ("births_real", "never_f", "med_m", "never_m", "gap",
                                           "first_birth", "tfr") if k not in REPORTED):
        log("  %-44s %9.4f +- %.4f (check run %.4f)" % (
            LABELS.get(k, k), value(sm, k), sse.get(k, math.nan), value(m, k)))
    log("  candidates per searcher %.2f (check run %.2f)" % (sm["cands_mean"], m["cands_mean"]))
    if sm["market_f"] == sm["market_f"]:
        log("  on the market per round: women %.2f, men %.2f" % (sm["market_f"], sm["market_m"]))
    log("")
    log("Founders and burn-in (check run): change from the cohorts born in the first half of "
        "years %d-%d to the second half (0 expected):" % (cfg["burn_in"], cfg["years"] - 50))
    for k, name in (("med_f", "median women"), ("med_m", "median men"),
                    ("never_f", "never women"), ("never_m", "never men"),
                    ("births", "births")):
        d = m["drift_" + k]
        log("  %-13s %+.4f +- %.4f (%+.1f SE)" % (name, d, se["drift_" + k],
                                                 d / se["drift_" + k]))
    snaps = agg["snapshots"]
    log("  population of all towns by year: alive, mean age, share under 18, 65 and over")
    for i in range(len(snaps[0])):
        t = snaps[0][i][0]
        alive = sum(s[i][1][0] for s in snaps)
        mean_age = sum(s[i][1][0] * s[i][1][1] for s in snaps) / alive
        young = sum(s[i][1][0] * s[i][1][2] for s in snaps) / alive
        old = sum(s[i][1][0] * s[i][1][3] for s in snaps) / alive
        log("    %3d %7d %6.2f %6.3f %6.3f" % (t // MONTHS, alive, mean_age, young, old))
    log("")
    log("Jacobian of the final local model: rows (%s) per unit of (%s):" % (
        ", ".join(OUTER), ", ".join(("ln " if lg else "") + k for k, lg in zip(XNAMES, LOGX))))
    for row in fitted["jac"]:
        log("  " + " ".join("%9.4f" % (v / s) for v, s in zip(row, SCALE)))
    rounded = {k: sig(par[k], se_par[k]) for k in NAMES if k in se_par}
    log("")
    log("Values for the specification (German decimals):")
    log("  " + ", ".join("%s %s" % (k, german(v)) for k, v in rounded.items()))
    source = ("tools/life_reference.py: closed towns of %d founders over %d years (burn-in %d), "
              "local regression over %d runs of %d towns, checked on %d new towns" % (
                  cfg["founders"], cfg["years"], cfg["burn_in"], len(runs), cfg["towns"], n_chk))
    # Units from the closed list of tools/data_reference.py (ADR-0010 to ADR-0015).
    female = "c_m" not in rounded
    if female:
        market_entries = (
            ("core:market_search_height_female", "c_f", "fraction",
             "11.2 with Grok R8, c of the search rate s_f(a) of women, per market round"),
            ("core:market_search_center_female", "m_f", "year",
             "11.2 with Grok R8, m of the search rate s_f(a) of women"),
            ("core:market_search_factor_minor", "rho", "fraction",
             "Grok R6b and R8, factor on s_f(a) from the 16th until before the 18th birthday"),
            ("core:pairing_age_gap_preferred", "mu_a", "year",
             "11.2 with Grok R8, mu_a of the age term (man older), with w_a 0.02"),
        )
    else:
        market_entries = (
            ("core:market_entry_height_female", "c_f", "fraction",
             "Grok R7, c of the entry rate e(a) for women, per market round"),
            ("core:market_entry_center_female", "m_f", "year",
             "Grok R7, m of the entry rate e(a) for women"),
            ("core:market_entry_height_male", "c_m", "fraction",
             "Grok R7, c of the entry rate e(a) for men, per market round"),
            ("core:market_entry_center_male", "m_m", "year",
             "Grok R7, m of the entry rate e(a) for men"),
            ("core:market_entry_factor_minor", "rho", "fraction",
             "Grok R6b and R7, factor on e(a) from the 16th until before the 18th birthday"),
        )
    entries = market_entries + (
        ("core:pairing_weight_cognition", "w_K", "logit",
         "docs/genetik-spezifikation.md 11.2, w_K, utility per squared phenotype difference"),
        ("core:pairing_weight_height", "w_H", "logit",
         "docs/genetik-spezifikation.md 11.2, w_H, utility per squared phenotype difference"),
        ("core:fertility_wish_tilt", "alpha", "logit",
         "docs/genetik-spezifikation.md 11.3, alpha, log weight per wished child"),
        ("core:separation_rate", "lam", "per_year", "ADR-0018 section 3, lambda_sep"),
    )
    snippet = {}
    for name, k, unit_, note in entries:
        if k not in rounded:
            continue
        snippet[name] = {"value": rounded[k], "unit": unit_, "source": source,
                         "as_of": FIT_DATE,
                         "note": "%s; approximate standard error %.2g" % (note, se_par[k])}
    log("")
    log("JSON for data/calibration/demography.json (the key names are proposals):")
    log(json.dumps(snippet, indent=2, ensure_ascii=False))
    log("")
    log("%d town runs, %.0f s." % (towns_run, runtime))
    return reached, all(checks), births_ok


def main(argv):
    quick = "--quick" in argv
    female = "--r7" not in argv
    if female:
        set_mode(FEMALE_MODE)
    jobs = min(os.cpu_count() or 1, 12)
    if "--jobs" in argv:
        jobs = int(argv[argv.index("--jobs") + 1])
    cfg = dict(QUICK if quick else FULL)
    t0 = time.time()
    log("life_reference: %s; towns of %d founders over %d years (burn-in %d); %d towns per "
        "run; %d jobs" % ("QUICK smoke test, imprecise" if quick else "full fit",
                          cfg["founders"], cfg["years"], cfg["burn_in"], cfg["towns"], jobs))
    sim_cfg = {k: cfg[k] for k in ("founders", "years", "burn_in")}
    sim_cfg["market"] = "female" if female else "r7"
    log("market: %s" % ("only women search (Grok R8)" if female else
                        "R7, the market as a state (kept for comparison)"))
    runner = Runner(sim_cfg, jobs)
    try:
        fitted = fit(runner, X0[:], ALPHA0, LAM0, cfg)
    except WishTooHigh as err:
        log("STOP: the births target needs alpha %s, a wish mean above %.1f (11.3)." % (
            err.args[0], WISH_MAX_MEAN))
        return 1
    par = params(fitted["x"], fitted["alpha"], fitted["lam"])
    se_par = fit_errors(fitted, par)
    check_seeds = list(range(CHECK_SEED0, CHECK_SEED0 + cfg["check_towns"]))
    chk, = runner.evaluate([par], check_seeds)
    scan = runner.evaluate([dict(par, w_K=w) for w in WK_SCAN],
                           seeds_of(cfg["iters"] + 3, cfg["settle_towns"]))
    small_cfg = dict(sim_cfg, founders=SMALL_FOUNDERS)
    small_towns = SMALL_TOWNS if not quick else 8
    small, = runner.evaluate([par], list(range(CHECK_SEED0 + 5000,
                                               CHECK_SEED0 + 5000 + small_towns)), small_cfg)
    wa_scan = None
    if female and "--wa-scan" in argv:
        wa_scan = [r[1] for r in runner.evaluate(
            [dict(par, w_a=w) for w in WA_SCAN], seeds_of(cfg["iters"] + 4, cfg["settle_towns"]))]
    reached, ok, births_ok = report(cfg, par, se_par, fitted, chk, [r[1] for r in scan], small,
                                    time.time() - t0, runner.towns, wa_scan)
    status = 0
    if not reached:
        log("NOT REACHED: a target misses by more than 4 standard errors in the check run.")
        status = 1
    if not ok:
        log("STOP: a check failed (age gap or births under 18); the finding goes back to Grok "
            "before these values enter the calibration.")
        status = 1
    # Four towns cannot tell a bias of the estimator from noise, so the smoke test only
    # reports this finding; the full fit stops on it.
    if not births_ok and quick:
        log("NOTE: the realized births miss 2.20 by more than %g standard errors on the few "
            "towns of the smoke test; only the full fit decides." % BIRTHS_BIAS_SE)
    elif not births_ok:
        log("STOP: the realized births miss 2.20 by more than %g standard errors; alpha's search "
            "estimator is biased here and alpha has to be fitted on realized births." %
            BIRTHS_BIAS_SE)
        status = 1
    return status


if __name__ == "__main__":
    sys.exit(main(sys.argv))

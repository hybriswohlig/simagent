#!/usr/bin/env python3
"""Reference for growing up in phase 4 (lived experience D, temperament phenotype, skills,
habits, niche, neuroticism factor), written from ADR-0033 (sections 1 to 8) and, for the
temperament weights, ADR-0009 section 4 and genetic specification 11.5 alone, without a look
at the Rust code. Plain integers where the ADR says integers, f64 (Python float, IEEE double,
operations left to right) where it says f64.

What it implements (section numbers refer to ADR-0033 unless noted):

- Saturating step of D with division toward zero (4), the age factor of a step (4).
- The age weight w from seconds and `raised_since` (3), and w at the 18th birthday.
- Lived-experience term U and the temperament phenotype z (1, 2; ADR-0009 4 for the weights,
  the clamping to 0..1 before the root and the left-to-right order).
- Effective practice seconds with window factor and diligence factor (6), the learning level
  of spec 11.5 (kappa 0.25, nu 0.2, tau0 1000 h), `level_milli`, and the milestones of 10
  effective hours crossed by one addition, each with the level exactly at its boundary (6).
- Habits (7): strength, +100 per completion capped at 1000, analytic decay, at most 8 per
  person with the weakest dropped, quarter of the day, points 15 * strength.
- Niche (7): lead against the comparison sibling and points clamp(20 * lead, 0, 10000).
- Neuroticism feeling factor in thousandths and its application to a step (8).

Times are whole seconds. Year = 364 days, day = 86 400 s. Where the ADR text is open, the
literal reading is taken and marked `READING` with the section in a comment.

This script is independent of the Rust code (ADR-0033 acceptance test): the core's tests
take their expected values from here.

Usage: ~/.cache/simagent-tools-venv/bin/python development_reference.py   (from tools/)
         (assert every worked value of the acceptance test and print the pinned values)
"""

import math
from typing import Dict, List, Optional, Sequence, Tuple

DAY = 86_400
YEAR = 364 * DAY  # one year of the simulation clock: 364 days

# Learning function of spec 11.5 (play values).
KAPPA = 0.25
NU = 0.2
TAU0 = 1000.0  # hours

# Habits (section 7).
HABIT_CAP = 8
HABIT_STEP = 100
HABIT_MAX = 1000
HABIT_DECAY_PER_DAY = 20
HABIT_POINTS_PER_STRENGTH = 15

# Niche (section 7).
NICHE_POINTS_PER_MILLI = 20
NICHE_POINTS_MAX = 10_000
LEVEL_CAP = 1e12  # level_milli = floor(min(level, 1e12) * 1000)

# Practice milestones: every 10 effective hours (section 6).
MILESTONE_SECONDS = 10 * 3600


# ---------------------------------------------------------------------------------------
# Small helpers


def div0(a: int, b: int) -> int:
    """Integer division truncating toward zero, exact for any size (Rust's `/` on integers)."""
    q = abs(a) // abs(b)
    return q if (a >= 0) == (b > 0) else -q


def clamp(x: float, lo: float, hi: float) -> float:
    """f64 clamp; the bounds are never NaN here."""
    return lo if x < lo else hi if x > hi else x


def clamp_int(x: int, lo: int, hi: int) -> int:
    return lo if x < lo else hi if x > hi else x


# ---------------------------------------------------------------------------------------
# Section 4: lived experience D


def saturate(d: int, step: int) -> int:
    """One saturating step of D (thousandths, -3000..3000), `step` already age-scaled.
    positive: D + step * (3000 - D) / 3000, negative: D + step * (3000 + D) / 3000,
    division toward zero (i64 in the core)."""
    if step >= 0:
        out = d + div0(step * (3000 - d), 3000)
    else:
        out = d + div0(step * (3000 + d), 3000)
    return clamp_int(out, -3000, 3000)


def age_factor_step(step: int, age_seconds: int) -> int:
    """Age factor of the child at the second of the occasion: 1 under 11, 1/2 from 11 to under
    15, 1/4 from 15 to under 18, applied as step * 4 / 4, step * 2 / 4, step * 1 / 4 (division
    toward zero). From 18 on D does not change: 0."""
    if age_seconds < 11 * YEAR:
        return div0(step * 4, 4)
    if age_seconds < 15 * YEAR:
        return div0(step * 2, 4)
    if age_seconds < 18 * YEAR:
        return div0(step * 1, 4)
    return 0


# ---------------------------------------------------------------------------------------
# Section 3: age weight w


def w_age(t: int, birth: int, raised_since: Optional[int]) -> float:
    """w = clamp((min(t, birth + 18 years) - raised_since) / 18 years, 0, 1), f64 from the
    seconds; 0 when raised_since is None."""
    if raised_since is None:
        return 0.0
    upper = min(t, birth + 18 * YEAR)
    return clamp(float(upper - raised_since) / float(18 * YEAR), 0.0, 1.0)


def w18(birth: int, raised_since: Optional[int]) -> float:
    """w at the 18th birthday (the value that talent for play uses, section 6)."""
    return w_age(birth + 18 * YEAR, birth, raised_since)


# ---------------------------------------------------------------------------------------
# Sections 1 and 2: U and the temperament phenotype z


def d_hat(d: int, m_d: float, s_d: float) -> float:
    """D-hat = (D / 1000 - m_D) / s_D."""
    return (float(d) / 1000.0 - m_d) / s_d


def u_term(e: float, d: int, w: float, m_d: float, s_d: float) -> float:
    """U = (sqrt(1 - w * w) * E) + (w * D-hat); 1 - w * w held in 0..1 before the root."""
    one_minus = clamp(1.0 - w * w, 0.0, 1.0)
    return (math.sqrt(one_minus) * e) + (w * d_hat(d, m_d, s_d))


def clamp01(x: float) -> float:
    """ADR-0009 4: 0 if not greater than 0 (also -0.0), 1 if greater than 1."""
    return 0.0 if not (x > 0.0) else 1.0 if x > 1.0 else x


def phenotype_weights(
    age_years: float, h2_child: float, c2_child: float, h2_adult: float, c2_adult: float
) -> Tuple[float, float, float]:
    """(h2, c2, e2) at the age, ADR-0009 4: s(x) = 1 / (1 + exp((12 - x) / 2)),
    w(x) = w_child + (w_adult - w_child) * s(x), e2 = (1 - h2) - c2, each held in 0..1.
    READING (ADR-0009 4): e2 is computed from the unclamped h2 and c2; each of the three is
    clamped only before its root."""
    s = 1.0 / (1.0 + math.exp((12.0 - age_years) / 2.0))
    h2 = h2_child + (h2_adult - h2_child) * s
    c2 = c2_child + (c2_adult - c2_child) * s
    e2 = (1.0 - h2) - c2
    return clamp01(h2), clamp01(c2), clamp01(e2)


def z_phenotype(
    g: float,
    c: float,
    e: float,
    d: int,
    w: float,
    age_years: float,
    h2_child: float,
    c2_child: float,
    h2_adult: float,
    c2_adult: float,
    m_d: float,
    s_d: float,
) -> float:
    """Temperament phenotype with lived experience (sections 1, 2):
    z = ((sqrt(h2) * G) + (sqrt(c2) * C)) + (sqrt(e2) * U), left to right."""
    h2, c2, e2 = phenotype_weights(age_years, h2_child, c2_child, h2_adult, c2_adult)
    u = u_term(e, d, w, m_d, s_d)
    return ((math.sqrt(h2) * g) + (math.sqrt(c2) * c)) + (math.sqrt(e2) * u)


# ---------------------------------------------------------------------------------------
# Section 6: skills


SKILLS = ("language", "reading", "play")


def window_factor(skill: str, age_seconds: int) -> float:
    """Window factor of a practice second by skill and age at that second:
    language 0 to under 6 full else 1/4; reading 6 to under 12 full else 1/4;
    play 6 to under 12 full else 1/2."""
    if skill == "language":
        return 1.0 if age_seconds < 6 * YEAR else 0.25
    if skill == "reading":
        return 1.0 if 6 * YEAR <= age_seconds < 12 * YEAR else 0.25
    if skill == "play":
        return 1.0 if 6 * YEAR <= age_seconds < 12 * YEAR else 0.5
    raise ValueError(skill)


def diligence_factor(z_c: float) -> float:
    """f = clamp(1 + 0.2 * z_C, 0.5, 1.5), from conscientiousness at the second of practice."""
    return clamp(1.0 + 0.2 * z_c, 0.5, 1.5)


def effective_seconds(raw_seconds: int, window: float, f: float) -> int:
    """(raw seconds * window factor) * f as f64 from left to right, rounded down to whole
    seconds (u64)."""
    return int(math.floor((float(raw_seconds) * window) * f))


def level(z: float, h: float) -> float:
    """Spec 11.5: L(z, h) = e^(kappa z) * (1 - e^(-h / tau(z))), tau(z) = tau0 * e^(-nu z);
    h in effective hours; L = 0 at h = 0."""
    if h <= 0.0:
        return 0.0
    tau = TAU0 * math.exp(-NU * z)
    return math.exp(KAPPA * z) * (1.0 - math.exp(-h / tau))


def level_milli(lv: float) -> int:
    """floor(min(level, 10^12) * 1000) as i64 (level is finite and not negative)."""
    return int(math.floor(min(lv, LEVEL_CAP) * 1000.0))


def milestones(z: float, eff_before: int, eff_after: int) -> List[Tuple[int, int]]:
    """Milestones of 10 effective hours crossed when the effective seconds go from
    `eff_before` to `eff_after` (one addition): one (hours, level_milli) per crossed boundary,
    ascending, hours = 10, 20, ..., level_milli = the level at exactly that boundary
    (h = hours). A boundary is crossed when eff_before < boundary <= eff_after.
    READING (6): reaching a boundary exactly counts as crossing it."""
    out: List[Tuple[int, int]] = []
    k = eff_before // MILESTONE_SECONDS + 1
    while k * MILESTONE_SECONDS <= eff_after:
        hours = 10 * k
        out.append((hours, level_milli(level(z, float(hours)))))
        k += 1
    return out


# ---------------------------------------------------------------------------------------
# Section 7: habits and niche

Key = Tuple[int, int, int]  # (furniture kind data id, offer index, quarter of the day)
Habits = Dict[Key, Tuple[int, int]]  # key -> (value, since second)


def habit_quarter(second: int) -> int:
    """Quarter of the day from the seconds since midnight: 0 to 3 for 0, 6, 12, 18 h."""
    return (second % DAY) // (6 * 3600)


def habit_key(kind: int, offer: int, second: int) -> Key:
    """Ordered by the furniture kind's data id, then the offer index, then the quarter."""
    return (kind, offer, habit_quarter(second))


def habit_strength(value: int, since: int, t: int) -> int:
    """Stored value minus 20 * (t - since) / 86400 (integer division), toward 0, never
    below 0."""
    return max(0, value - div0(HABIT_DECAY_PER_DAY * (t - since), DAY))


def habits_at(habits: Habits, t: int) -> Dict[Key, int]:
    """Current strength per key at second t; a strength of 0 is not kept."""
    out = {}
    for key, (value, since) in habits.items():
        s = habit_strength(value, since, t)
        if s > 0:
            out[key] = s
    return out


def habit_complete(habits: Habits, key: Key, t: int) -> Habits:
    """A completion (end with Done or Full, without a command) at second t: strength +100,
    capped at 1000, the decayed value plus 100 stored with the second t. Only that entry is
    stored again; every other entry keeps its own value and second, because integer decay
    from two seconds is not the decay from one. Entries at strength 0 are removed first. At
    most 8 habits: with a full list and a new key, the weakest existing habit (smallest
    strength at t, tie smallest key) falls and the newcomer comes in with 100."""
    current = habits_at(habits, t)
    new: Habits = {k: habits[k] for k in current}
    value = min(current.get(key, 0) + HABIT_STEP, HABIT_MAX)
    if key not in new and len(new) >= HABIT_CAP:
        drop = min(current, key=lambda k: (current[k], k))
        del new[drop]
    new[key] = (value, t)
    return new


def habit_points(strength: int) -> int:
    """15 points per unit of strength on the offer at the time of day of the start."""
    return HABIT_POINTS_PER_STRENGTH * strength


def habit_completes(reason: str, commanded: bool) -> bool:
    """Whether an action's end strengthens its habit: `done` or `full`, and not chosen by a
    command (an end by a command has neither reason)."""
    return reason in ("done", "full") and not commanded


# Per point a critical need lies below 151 (ADR-0026 section 3): the share no other sum reaches.
PER_CRITICAL_POINT = 1_000_000_000


def leaning_rows(
    habits: Habits, offers: Sequence[Tuple[int, int, int]], t: int, in_night: bool
) -> List[Tuple[int, int, int, int, Optional[str], Optional[int], Optional[int]]]:
    """The rows of `core:leaning` for a choice at second t (section 7): `offers` are the open
    offers the choice weighs as (object, offer index, furniture kind data id), in the order of
    the objects and their offers. Each offer whose habit (kind, offer, quarter of t) has a
    strength above 0 gives a row (object, offer, habit points, niche points, skill, sibling,
    lead_milli); without the niche (slice H1) the last four are 0, None, None, None. In the
    person's night no habit counts, so there are no rows.

    READING (section 7, "ohne Wirkung in den Stufen der Nacht"): the bonus is 0 in the night,
    not merely outranked there, and so the night's choices write no `core:leaning`.
    READING (section 7, "je Angebot mit Zuschlag"): one row per offer, so two objects of one
    kind with the habit's offer give two rows with the same points."""
    if in_night:
        return []
    current = habits_at(habits, t)
    rows = []
    for obj, offer, kind in offers:
        strength = current.get(habit_key(kind, offer, t), 0)
        if strength > 0:
            rows.append((obj, offer, habit_points(strength), 0, None, None, None))
    return rows


def niche_lead(
    own_milli: int, siblings: Sequence[Tuple[int, int, int]]
) -> Optional[Tuple[int, int]]:
    """Lead of the own level_milli over the comparison sibling, or None without siblings.
    `siblings` are (sim_id, birth_second, level_milli). The comparison sibling has the
    highest level, ties: earlier birth, then smaller id. Returns (lead, sibling_id)."""
    if not siblings:
        return None
    sid, _birth, milli = min(siblings, key=lambda s: (-s[2], s[1], s[0]))
    return own_milli - milli, sid


def niche_points(lead: int) -> int:
    """clamp(20 * lead, 0, 10000) points on `read` (reading) or `play` (play)."""
    return clamp_int(NICHE_POINTS_PER_MILLI * lead, 0, NICHE_POINTS_MAX)


# ---------------------------------------------------------------------------------------
# Section 8: neuroticism feeling factor


def feeling_factor_milli(z_n: float) -> int:
    """floor(1000 * clamp(1 + 0.15 * z_N, 0.7, 1.3))."""
    return int(math.floor(1000.0 * clamp(1.0 + 0.15 * z_n, 0.7, 1.3)))


def apply_feeling_factor(step: int, milli: int) -> int:
    """step * thousandths / 1000, division toward zero."""
    return div0(step * milli, 1000)


# ---------------------------------------------------------------------------------------
# Acceptance test with hand-derived values


def main() -> None:
    # --- Sustained values of spec 11.5 (given) -------------------------------------------
    assert level(0.0, 1000.0) == 0.6321205588285577
    assert level(1.0, 1000.0) == 0.9054737676171374
    assert level(-1.5, 2000.0) == 0.5310921528112428
    assert level(2.5, 5000.0) == 1.8677547314826046
    assert level(1.0, 0.0) == 0.0
    print("L(0,1000) =", repr(level(0.0, 1000.0)), " L(1,1000) =", repr(level(1.0, 1000.0)))
    print("L(-1.5,2000) =", repr(level(-1.5, 2000.0)), " L(2.5,5000) =", repr(level(2.5, 5000.0)))

    # --- Saturation (4) -------------------------------------------------------------------
    d1 = saturate(0, 40)
    d2 = saturate(d1, 40)
    # By hand: 0 + 40 * 3000 / 3000 = 40; then 40 + 40 * 2960 / 3000 = 40 + 118400/3000
    # = 40 + 39.47 -> 39 (toward 0) = 79.
    assert (d1, d2) == (40, 79)
    # Negative mirror: 0 - 40 = -40; -40 + (-40 * 2960) / 3000 = -40 - 39 = -79.
    assert (saturate(0, -40), saturate(-40, -40)) == (-40, -79)
    # D = 1000, step 30: 1000 + 30 * 2000 / 3000 = 1000 + 20 = 1020.
    assert saturate(1000, 30) == 1020
    # D = 1000, step -30: 1000 + (-30 * 4000) / 3000 = 1000 - 40 = 960.
    assert saturate(1000, -30) == 960
    # D = -2999, step +1: 5999 / 3000 = 1.99 -> 1, so -2998 (positive branch uses 3000 - D).
    assert saturate(-2999, 1) == -2998
    # D = 2999, step -1: -5999 / 3000 -> -1 (toward 0), so 2998.
    assert saturate(2999, -1) == 2998
    # At the bound: 3000 + 40 * 0 / 3000 = 3000; -3000 - 40 * 0 / 3000 = -3000.
    assert saturate(3000, 40) == 3000 and saturate(-3000, -40) == -3000
    # Step 0 changes nothing.
    assert saturate(1234, 0) == 1234
    # An absurd step (10**12 from 0 gives 0 + 10**12) is held to the range by the final clamp.
    assert saturate(0, 10**12) == 3000 and saturate(0, -(10**12)) == -3000
    print("saturate: 0 +40 ->", d1, "+40 ->", d2)

    # --- Age factor (4) -------------------------------------------------------------------
    # step 40: age 10 -> 40 * 4 / 4 = 40; 11 -> 40 * 2 / 4 = 20; 14 -> 20; 15 -> 40 / 4 = 10;
    # 17 -> 10; 18 -> 0.
    expected = {10: 40, 11: 20, 14: 20, 15: 10, 17: 10, 18: 0}
    for years, want in expected.items():
        got = age_factor_step(40, years * YEAR)
        assert got == want, (years, got, want)
    # Boundaries to the second: one second before 11 years is full, at 11 years half, etc.
    assert age_factor_step(40, 11 * YEAR - 1) == 40 and age_factor_step(40, 11 * YEAR) == 20
    assert age_factor_step(40, 15 * YEAR - 1) == 20 and age_factor_step(40, 15 * YEAR) == 10
    assert age_factor_step(40, 18 * YEAR - 1) == 10 and age_factor_step(40, 18 * YEAR) == 0
    # Toward zero: 5 * 2 / 4 = 2.5 -> 2; -5 * 2 / 4 = -2.5 -> -2 (floor would give -3);
    # -5 * 1 / 4 = -1.25 -> -1; 3 * 1 / 4 = 0.75 -> 0.
    assert age_factor_step(5, 12 * YEAR) == 2 and age_factor_step(-5, 12 * YEAR) == -2
    assert age_factor_step(-5, 16 * YEAR) == -1 and age_factor_step(3, 16 * YEAR) == 0
    print("age factor of a +40 step at 10, 11, 14, 15, 17, 18:", [age_factor_step(40, y * YEAR) for y in (10, 11, 14, 15, 17, 18)])

    # --- w (3) ----------------------------------------------------------------------------
    # Born at 0, moved in at 12 years. Age 8: not yet in (raised_since None) -> 0, and even
    # with raised_since = 12 y a time before it clamps to 0. Age 12: (12 - 12) / 18 = 0.
    # Age 18: (18 - 12) / 18 = 1/3.
    moved = 12 * YEAR
    assert w_age(8 * YEAR, 0, None) == 0.0
    assert w_age(8 * YEAR, 0, moved) == 0.0
    assert w_age(12 * YEAR, 0, moved) == 0.0
    assert w_age(18 * YEAR, 0, moved) == 6.0 / 18.0 == 1.0 / 3.0
    # After 18 the clock stops: w stays 1/3 at 30 years.
    assert w_age(30 * YEAR, 0, moved) == 1.0 / 3.0 and w18(0, moved) == 1.0 / 3.0
    # Raised since birth: w = 9 / 18 = 0.5 at 9 years, 1 at 18 and after.
    assert w_age(9 * YEAR, 0, 0) == 0.5 and w_age(18 * YEAR, 0, 0) == 1.0 and w18(0, 0) == 1.0
    # A birth at a later second shifts everything: born at 1000 s, raised since birth.
    assert w_age(1000 + 18 * YEAR + 5, 1000, 1000) == 1.0
    print("w at 8, 12, 18 for move-in at 12:", w_age(8 * YEAR, 0, moved), w_age(12 * YEAR, 0, moved), w_age(18 * YEAR, 0, moved))

    # --- z with U (1, 2) ------------------------------------------------------------------
    # G = C = 0, D = 1000, w = 1, m_D = 0, s_D = 1: D-hat = (1000 / 1000 - 0) / 1 = 1;
    # U = sqrt(1 - 1) * E + 1 * 1 = 1 for any E; at an adult age the weights are h2 = 0.5,
    # c2 = 0.05, e2 = (1 - 0.5) - 0.05 = 0.45, so z = 0 + 0 + sqrt(0.45) * 1.
    params = dict(h2_child=0.5, c2_child=0.15, h2_adult=0.5, c2_adult=0.05, m_d=0.0, s_d=1.0)
    want_adult = math.sqrt(0.45)
    for e in (-2.7, 0.0, 1.3):
        z = z_phenotype(0.0, 0.0, e, 1000, 1.0, 1000.0, **params)
        assert math.isclose(z, want_adult, rel_tol=1e-12, abs_tol=0.0), (e, z)
    # At age 12: s = 1 / (1 + exp(0)) = 0.5; h2 = 0.5, c2 = 0.15 + (0.05 - 0.15) * 0.5 = 0.10,
    # e2 = 0.5 - 0.10 = 0.40: z = sqrt(0.40) for the same inputs.
    z12 = z_phenotype(0.0, 0.0, 0.7, 1000, 1.0, 12.0, **params)
    assert math.isclose(z12, math.sqrt(0.40), rel_tol=1e-12)
    # w = 0 gives U = E: z = ((sqrt(.5) * G) + (sqrt(.05) * C)) + (sqrt(.45) * E) at adult age;
    # with G = 1, C = 0, E = 0: sqrt(0.5).
    z0 = z_phenotype(1.0, 0.0, 0.0, 3000, 0.0, 1000.0, **params)
    assert math.isclose(z0, math.sqrt(0.5), rel_tol=1e-12)
    # Centering and scaling: D = 1500, m_D = 0.5, s_D = 0.5: D-hat = (1.5 - 0.5) / 0.5 = 2.
    assert d_hat(1500, 0.5, 0.5) == 2.0
    # U at w = 0.6: sqrt(1 - 0.36) = 0.8, so U = 0.8 * E + 0.6 * D-hat = 0.8 * 1 + 0.6 * 2 = 2.
    assert math.isclose(u_term(1.0, 1500, 0.6, 0.5, 0.5), 2.0, rel_tol=1e-12)
    print("z (G=C=0, D=1000, w=1, adult) =", repr(z_phenotype(0.0, 0.0, 0.0, 1000, 1.0, 1000.0, **params)), " sqrt(0.45) =", repr(want_adult))

    # --- Effective practice seconds (6) --------------------------------------------------
    # One hour of reading at 13 with f = 1: reading window 6 to under 12 is over, so 1/4:
    # 3600 * 0.25 * 1 = 900.
    assert effective_seconds(3600, window_factor("reading", 13 * YEAR), 1.0) == 900
    # Inside the window with f = 1: 3600 s count in full; reading at 8: 3600.
    assert effective_seconds(3600, window_factor("reading", 8 * YEAR), 1.0) == 3600
    # Play outside the window counts 1/2: one hour at 13 -> 1800; at 12 y minus one second
    # -> full, at 12 y exactly -> half.
    assert effective_seconds(3600, window_factor("play", 13 * YEAR), 1.0) == 1800
    assert window_factor("play", 12 * YEAR - 1) == 1.0 and window_factor("play", 12 * YEAR) == 0.5
    # Language: 600 s at 3 with f = 1 -> 600; at 7 -> 150; at 5 y 364 d minus 1 s -> full.
    assert effective_seconds(600, window_factor("language", 3 * YEAR), 1.0) == 600
    assert effective_seconds(600, window_factor("language", 7 * YEAR), 1.0) == 150
    assert window_factor("language", 6 * YEAR - 1) == 1.0 and window_factor("language", 6 * YEAR) == 0.25
    # Reading below 6 is outside the window: 1/4.
    assert window_factor("reading", 6 * YEAR - 1) == 0.25 and window_factor("reading", 6 * YEAR) == 1.0
    # Diligence: z_C = 0 -> f = 1; z_C = 2 -> 1.4; z_C = 5 -> 2.0 clamped to 1.5; z_C = -5 ->
    # 0 clamped to 0.5. 600 s at 3 with f = 1.5 -> 900; with f = 0.5 -> 300.
    assert diligence_factor(0.0) == 1.0 and diligence_factor(5.0) == 1.5 and diligence_factor(-5.0) == 0.5
    assert math.isclose(diligence_factor(2.0), 1.4, rel_tol=1e-15)
    assert effective_seconds(600, 1.0, diligence_factor(5.0)) == 900
    assert effective_seconds(600, 1.0, diligence_factor(-5.0)) == 300
    # Rounding down: 7 s * 0.5 * 1 = 3.5 -> 3.
    assert effective_seconds(7, 0.5, 1.0) == 3
    print("effective seconds of 1 h reading at 13, f=1:", effective_seconds(3600, window_factor("reading", 13 * YEAR), 1.0))

    # --- level_milli and milestones (6) --------------------------------------------------
    # L(0, 1000) = 1 - e^-1 = 0.63212... -> 632 thousandths.
    assert level_milli(level(0.0, 1000.0)) == 632
    # L(1, 1000) = 0.90547... -> 905; L(-1.5, 2000) = 0.53109... -> 531; L(2.5, 5000) -> 1867.
    assert level_milli(level(1.0, 1000.0)) == 905
    assert level_milli(level(-1.5, 2000.0)) == 531
    assert level_milli(level(2.5, 5000.0)) == 1867
    # Zero hours: 0. A huge level is capped at 10^12 before the scaling.
    assert level_milli(0.0) == 0 and level_milli(1e300) == 10**15
    # One addition crossing two milestones, z = 0: before 30 000 s (8.33 h), add 50 000 s ->
    # 80 000 s (22.2 h): boundaries 36 000 s (10 h) and 72 000 s (20 h) are crossed.
    # L(0, 10) = 1 - e^-0.01 = 0.0099502 -> 9; L(0, 20) = 1 - e^-0.02 = 0.0198013 -> 19.
    assert milestones(0.0, 30_000, 80_000) == [(10, 9), (20, 19)]
    # Exactly reaching a boundary crosses it; starting on a boundary does not cross it again.
    assert milestones(0.0, 0, 36_000) == [(10, 9)]
    assert milestones(0.0, 36_000, 36_001) == []
    # Nothing crossed below the next boundary.
    assert milestones(0.0, 0, 35_999) == []
    # A big addition crossing three: 0 -> 110 000 s (30.6 h): 10, 20, 30 h.
    # L(0, 30) = 1 - e^-0.03 = 0.0295545 -> 29.
    assert milestones(0.0, 0, 110_000) == [(10, 9), (20, 19), (30, 29)]
    # With talent z = 1: tau = 1000 * e^-0.2 = 818.73 h; L(1, 10) = e^0.25 * (1 - e^(-10 / 818.73))
    # = 1.2840254 * 0.0121400 = 0.015588 -> 15.
    assert milestones(1.0, 0, 36_000) == [(10, 15)]
    print("milestones z=0, 30000 -> 80000 s:", milestones(0.0, 30_000, 80_000))

    # --- Habits (7) -----------------------------------------------------------------------
    # Quarter of the day: 05:59:59 -> 0, 06:00 -> 1, 12:00 -> 2, 18:00 -> 3, 23:59:59 -> 3;
    # the second of day 3 at 07:00 is 86400 * 3 + 25200 -> quarter 1.
    assert [habit_quarter(s) for s in (0, 21_599, 21_600, 43_200, 64_800, 86_399, 3 * DAY + 25_200)] == [0, 0, 1, 2, 3, 3, 1]
    key = habit_key(7, 0, 25_200)  # kind 7, offer 0 at 07:00 -> (7, 0, 1)
    assert key == (7, 0, 1)
    # Two kinds with the same offer index are two habits: each completion gives 100.
    two = habit_complete(habit_complete({}, (2, 0, 1), 0), (3, 0, 1), 0)
    assert habits_at(two, 0) == {(2, 0, 1): 100, (3, 0, 1): 100}
    # Habit 0, then 100, after two days 60, after one more completion 160.
    t0 = 25_200
    h: Habits = {}
    assert habits_at(h, t0).get(key, 0) == 0
    h = habit_complete(h, key, t0)
    assert habits_at(h, t0)[key] == 100
    t2 = t0 + 2 * DAY  # decay 20 * 172800 / 86400 = 40: 100 - 40 = 60
    assert habits_at(h, t2)[key] == 60
    h = habit_complete(h, key, t2)  # 60 + 100 = 160
    assert habits_at(h, t2)[key] == 160
    # Decay is integer division: 20 * 4319 / 86400 = 0.99 -> 0 (nothing lost before 4320 s);
    # 20 * 4320 / 86400 = 1 exactly, so 99.
    assert habit_strength(100, 0, 4_320) == 99
    assert habit_strength(100, 0, 4_319) == 100
    # Cap 1000: 950 + 100 -> 1000.
    capped = habit_complete({key: (950, t0)}, key, t0)
    assert habits_at(capped, t0)[key] == 1000
    # A strength reaching 0 disappears: 100 after 5 days -> 100 - 100 = 0.
    assert habits_at({key: (100, 0)}, 5 * DAY) == {}
    assert habits_at({key: (100, 0)}, 5 * DAY - 1) == {key: 1}
    # At most 8: eight habits with strengths 100..800 for keys (1..8, 0, 0); a ninth key
    # (9, 0, 0) drops the weakest, key (1, 0, 0) at 100.
    full: Habits = {(i, 0, 0): (100 * i, 0) for i in range(1, 9)}
    nine = habit_complete(full, (9, 0, 0), 0)
    assert len(nine) == 8 and (1, 0, 0) not in nine and nine[(9, 0, 0)] == (100, 0)
    # Tie on strength: the smallest key in (kind, offer, quarter) order falls. All eight at
    # 300; (3, 0, 1) and (3, 1, 0) and (4, 0, 0): kind 3 first, then offer 0 before 1, so
    # (3, 0, 1) falls although its quarter is larger than that of (3, 1, 0).
    tie: Habits = {(3, 1, 0): (300, 0), (3, 0, 1): (300, 0), (5, 0, 2): (300, 0), (6, 0, 0): (300, 0),
                   (7, 0, 0): (300, 0), (8, 0, 0): (300, 0), (9, 0, 0): (300, 0), (4, 0, 0): (300, 0)}
    after_tie = habit_complete(tie, (10, 0, 0), 0)
    assert (3, 0, 1) not in after_tie and (3, 1, 0) in after_tie and len(after_tie) == 8
    # Strengthening an existing key never evicts: nine entries cannot appear.
    again = habit_complete(full, (8, 0, 0), 0)
    assert len(again) == 8 and again[(8, 0, 0)] == (800 + 100, 0) and (1, 0, 0) in again
    # Points: 15 * strength: 160 -> 2400; 1000 -> 15000.
    assert habit_points(160) == 2400 and habit_points(1000) == 15_000 and habit_points(0) == 0
    print("habit: 0 ->", 100, "-> two days later", habits_at(habit_complete({}, key, t0), t2)[key], "-> after completion", habits_at(h, t2)[key])
    # Which ends strengthen: done and full without a command; left, critical, obligation,
    # command and collapse never, and a commanded action not even at done or full.
    assert [habit_completes(r, False) for r in ("done", "full", "left", "critical", "obligation", "command", "collapse")] == [True, True, False, False, False, False, False]
    assert not habit_completes("done", True) and not habit_completes("full", True)
    # The leaning of a choice at t2 (07:00 of day 2, quarter 1) with the habit h of kind 7,
    # offer 0 at 160: objects 3 and 4 of kind 7 offer 0 get 2400 each; object 5 of kind 7 offer 1
    # and object 6 of kind 8 offer 0 get nothing. At 12:00 the quarter is 2: no row. In the night
    # no row.
    offers = [(3, 0, 7), (4, 0, 7), (5, 1, 7), (6, 0, 8)]
    lean = leaning_rows(h, offers, t2, False)
    assert lean == [(3, 0, 2400, 0, None, None, None), (4, 0, 2400, 0, None, None, None)]
    assert leaning_rows(h, offers, t2 - 25_200 + 43_200, False) == []
    assert leaning_rows(h, offers, t2, True) == []
    # Five days after its last completion at 160 the habit is 160 - 100 = 60: 900 points; ten
    # days (200 lost) leave nothing, and an entry at 0 gives no row.
    assert leaning_rows(h, offers, t2 + 5 * DAY, False)[0][2] == 900
    assert leaning_rows(h, offers, t2 + 8 * DAY, False) == []
    # A critical need always wins: the most a habit gives is below one point of it.
    assert habit_points(HABIT_MAX) == 15_000 < PER_CRITICAL_POINT
    # By hand (house_choice's flat, an adult of 30): hunger 150 (critical), bladder 500,
    # hygiene 700, fun 800. The table fills hunger +1800 for 20 min = 600, weighing 700 - 150 =
    # 550: 330 000, a walk of 4 s, and (151 - 150) * 10^9; the television with a full habit
    # (1000 -> 15 000): fun 200 * 200 = 40 000, 3 s, + 15 000. The table wins.
    table = 550 * 600 - 50 * 4 + 1 * PER_CRITICAL_POINT
    tv = 200 * 200 - 50 * 3 + habit_points(1000)
    assert (table, tv) == (1_000_329_800, 54_850) and table > tv
    # A habit tips a close choice: at the morning values the television and the bookshelf tie at
    # 39 850, below 40 000 (idle); a habit of 100 on the bookshelf (1500) lifts it to 41 350.
    assert 39_850 + habit_points(100) == 41_350
    print("leaning rows at 07:00 of day 2 with the habit at 160:", lean)

    # --- Niche (7) -------------------------------------------------------------------------
    # lead 300 -> 20 * 300 = 6000; lead 600 -> 12000 clamped to 10000; lead -5 -> -100 -> 0.
    assert niche_points(300) == 6000 and niche_points(600) == 10_000 and niche_points(-5) == 0
    assert niche_points(0) == 0 and niche_points(500) == 10_000 and niche_points(499) == 9980
    # Comparison sibling: highest level_milli; tie: earlier birth; then smaller id.
    # Siblings (id, birth, milli): (5, 100, 800), (3, 200, 900), (4, 150, 900), (2, 150, 900):
    # highest 900 among ids 3, 4, 2; earliest birth 150 among 4 and 2; smaller id 2.
    sibs = [(5, 100, 800), (3, 200, 900), (4, 150, 900), (2, 150, 900)]
    assert niche_lead(1000, sibs) == (100, 2)
    assert niche_lead(1000, [(5, 100, 800)]) == (200, 5)
    assert niche_lead(500, sibs) == (-400, 2)
    assert niche_lead(500, []) is None
    # A sibling with a lead over the own level: lead -400 -> points 0.
    assert niche_points(niche_lead(500, sibs)[0]) == 0
    print("niche points for lead 300, 600, -5:", niche_points(300), niche_points(600), niche_points(-5))

    # --- Neuroticism feeling factor (8) ----------------------------------------------------
    # z_N = 0 -> 1.0 -> 1000; z_N = 1 -> 1.15 -> 1150; z_N = 2 -> 1.3 -> 1300; z_N = 10 ->
    # 2.5 clamped to 1.3 -> 1300; z_N = -2 -> 0.7 -> 700; z_N = -10 -> -0.5 clamped to 0.7 -> 700.
    assert [feeling_factor_milli(z) for z in (0.0, 1.0, 2.0, 10.0, -2.0, -10.0)] == [1000, 1150, 1300, 1300, 700, 700]
    # Applying: +40 at 1150 -> 40 * 1150 / 1000 = 46; -20 at 700 -> -14; +7 at 700 ->
    # 4.9 -> 4; -7 at 700 -> -4.9 -> -4 (toward zero); at 1000 nothing changes.
    assert apply_feeling_factor(40, 1150) == 46 and apply_feeling_factor(-20, 700) == -14
    assert apply_feeling_factor(7, 700) == 4 and apply_feeling_factor(-7, 700) == -4
    assert apply_feeling_factor(-37, 1000) == -37
    print("feeling factor milli for z_N 0, 1, 2, -2:", [feeling_factor_milli(z) for z in (0.0, 1.0, 2.0, -2.0)])

    # --- Helper self-checks ----------------------------------------------------------------
    assert div0(7, 2) == 3 and div0(-7, 2) == -3 and div0(7, -2) == -3 and div0(-7, -2) == 3
    # Exact for big values where int(a / b) would be wrong.
    big = 10**30 + 1
    assert div0(big, 1) == big and div0(-big, 1) == -big and div0(-big, 3) == -(big // 3)
    assert int(big / 1) != big  # the float route loses digits
    # Only the completed habit is stored again: key A (100 at 0) and key B (100 at 0); B is
    # completed at 1.5 days (100 - 30 = 70, + 100 = 170 at that second); A keeps (100, 0).
    pair = {(1, 0, 0): (100, 0), (2, 0, 0): (100, 0)}
    after = habit_complete(pair, (2, 0, 0), 129600)
    assert after[(1, 0, 0)] == (100, 0), after
    assert after[(2, 0, 0)] == (170, 129600), after
    print("all acceptance checks passed")


if __name__ == "__main__":
    main()

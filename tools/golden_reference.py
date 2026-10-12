#!/usr/bin/env python3
"""The goldens of the scenario `tests/scenarios/dummy.json`, computed independently of
the Rust code (ADR-0003 6.3) by the model in tools/world_reference.py: year hashes, log
hash and the golden save, and the goldens of its fork at year 5 with the override
"hereditary diseases on".

The scenario (format 3, design note of the fertility in the world, section 1): seed 1,
founder tag `test:dummy`, the switch off, neither causes of death nor a fertility of its
own; it runs on the repository data (the CLI embeds them): senescence and accident with
the rates of the genetics specification 7.2 (ADR-0017 section 2), its six hereditary
diseases (ADR-0017 section 3), the fertility of 5.6 and 11.3 (ADR-0017 section 4: the
curves, the adult rate 0.20, the wish, the pause of 12 months, ten months of pregnancy),
the separation (ADR-0018 section 3) and the partner market of 11.2 (ADR-0018 section 1,
design note of the market) with the values of the data.
Six founders born 30 years (sim 1), 25 years and 100 days (sim 2), one second (sim 3), 20
years (sim 4) and 25 years (sims 5 and 6) before the start, thirty years (long enough for
the first death, so that the goldens cover every event and log kind). Sim 1 is female
with the dummy allele twice at `core:disease_ar`, sim 2 male with dummy and wild (from
mother and father), sim 3 male with dummy twice; sims 4 (male), 5 (female) and 6 (male)
carry only reference alleles (an empty list of alleles), so the switch of the
hereditary diseases never touches them. Their lineage keys are those of the founders 0 to
5 of the tag (ADR-0001 section 4). Sims 1 and 2 are a couple from the start (partnership
1), and so are sims 5 and 6 (partnership 2); the entries `core:partnered` follow the
founders in the order of the couples.

Sims 4 to 6 came with the separation, so that the event `separation` runs and
`core:separated` is written after the fork point (the coverage of the goldens, ADR-0003
6.3): partnership 1 separates only after about 50 years. The separation of a couple of
founders depends only on their places in the list (their lineage keys), not on their
ages. Of the pairings among the founders 3 to 7 only woman 4 with man 5 (after about 29.9
years), woman 6 with man 4 (5.4 years) and woman 5 with man 7 (22.5 years) separate
within the thirty years and after the fork point; the first needs the fewest founders,
and founder 3 (sim 4, a single man) only fills its place.

What happens (a year has 364 days of 86 400 seconds, 13 months of 28 days):

- Every founder has its deaths of senescence and accident planned at the start, from its
  age then (B19): sim 1 dies of senescence after about 27.2 years (at about 57), sim 2
  would after about 69.8 years, sim 3 after about 87.2 years; their accidents lie
  thousands of years ahead. Sim 4 would die of senescence after about 61.6 years, sim 5
  after about 68.4 years, sim 6 after about 55.3 years. Sim 1's death in year 27 is the
  only one of the thirty years; it ends partnership 1.
- Sims 1 and 3 have `core:disease_ar` (recessive, penetrance 1, onset 0, two steps):
  each an affliction with nothing used, its exposure not running while the switch is
  off, so no death of it is planned. Sim 2 carries one copy and is healthy (no draw).
  No founder has an allele at another disease locus.
- Fertility: sim 1's wish is 3 (`u` of FERTILITY_WISH about 0.741, between the
  thresholds 0.517 and 0.829 of 2 and 3 children with the tilt 0.311). At 30 her ramp
  is long over (her menarche, about 12.8 years, is at most `θ + 3` = 15.3) and the
  decline of 5.6 acts: her factor is `(50 − a) / 20`; sim 2 (25 to 31 years old while
  she tries) has the factor 1. So the chance of the attempt in month `k` is
  `0.2 · (50 − a) / 20` with `a = 30 + k / 13`, 0.199 in month 1. Her series begins in
  month 1 (the first month start after the partnership at 0); she conceives in month 7
  (entries `core:conception` and two `core:meiosis`, chance 0.195) and has a daughter
  (entry `core:birth`) in month 17; after the pause of 12 months her series begins again
  in month 29, she conceives in month 32 (0.175) and bears a son in month 42; then from
  month 54 on, she conceives in month 74 (0.143) and bears a son in month 84. With three
  births her wish is reached: no series follows (32 attempts in all). Her children are
  sims 8, 10 and 12 (SimIds go by birth, and sim 5's children come between). All three
  got the wild allele from their father and carry `core:disease_ar` without an
  affliction; each has its deaths of senescence and accident planned from its birth.
- Sim 5's wish is 3 as well (`u` about 0.747), her menarche about 12.6 years. At 25 her
  ramp is over and the decline begins only at 30 (her 30th birthday is month 65), sim 6
  (25) has the factor 1: every attempt of hers up to month 65 has the chance 0.2 exactly.
  Her series begins in month 1; she conceives in month 2 and bears sim 7 (a son) in month
  12; after the pause from month 24, she conceives in month 31 and bears sim 9 (a son) in
  month 41; from month 53, she conceives in month 63 and bears sim 11 (a son) in month
  73. Her wish is reached (21 attempts). Her children carry no disease allele. Sim 4 is
  in no partnership and never tries.
- Twins (11.3, design note of the twins): each conception draws MULTIPLE_BIRTH at the key
  of its first child against the repository's rates 0.004 (identical) and 0.010
  (fraternal); the draws are about 0.088, 0.685 and 0.405 for sim 1 and 0.530, 0.050 and
  0.308 for sim 5, all at or above 0.014, so every conception gives one child and the
  story is that of one child per conception. Each conception entry names the draw as its
  third cause.
- Separation (ADR-0018 section 3, design note of the separation): the repository's rate
  0.00867 per year. When partnership 1 forms at the start, sim 1's `partnerships` goes
  from 0 (her `k`) to 1, and after her series its separation is planned by the entry
  `core:partnered`: the budget of SEPARATION_BUDGET (tag 18, her key, occasion 0, his key)
  is about 0.433, so it is due after about 49.97 years. Her death in year 27 comes first
  and cancels it (the partnership ends by death); the planned event sits in the queue
  until then. Partnership 2 (sim 5's `k` 0, planned by its entry `core:partnered` after
  her series) has the budget about 0.259: it is due at 940 727 538 s, after about 29.91
  years (month 388.9), and runs, in the run and in the fork: the partnership ends by
  separation, one entry `core:separated` (subjects sims 5 and 6, causes the plan and the
  draw). Sim 5 is not pregnant then and has no series (her wish is reached); both are
  single again.
- The partner market (genetics specification 11.2, design note of the market): `Start`
  plans the series of the rounds right after the year end series (insertion number 2,
  every later one of `Start` one higher), the first at the start of month 1, then every
  month (389 rounds in the thirty years). A round runs only for the single women of 16 or
  more; up to month 224 there are none (sims 1 and 5 are partnered, every child younger).
  Sim 8 (sim 1's daughter, born in month 17) is 16 in month 225: from that round on she
  is drawn (MARKET_SEARCH, `c` 0.01179, `m` 25.23, `w` 7, under 18 times `ρ` 0.0918, so
  about 0.0005 a round before 18 and 0.007 at 18) and searches first in month 260, at
  18.7. The single men of 16 or more then are sims 3 and 4 (founders), 7 and 9 (sim 5's
  sons) and her brother sim 10 (sims 11 and 12 are under 16; sims 2 and 6 are partnered).
  In the order of PAIRING_RANK the list is 7, 3, 4, 9, 10; her brother (φ = 1/4) is
  skipped. With `U = −0.02 · (Δa − 4.69)² + G` she chooses sim 7 (19.1 years old, `G`
  about 2.68, `U` about 2.31): partnership 3 at month 260, its entry `core:partnered` with
  the causes the plan of the round (entry 1 at 0), the rule `core:partner_market` and the
  draw PAIRING_NOISE (her key, 260, his key); her `partnerships` goes to 1, her series
  begins in month 261 and its separation is planned (budget about 2.128, due about 245.5
  years later, after the run). With the chance 0.2 (both ramps over) she conceives in
  month 270 and bears sim 13 (a son, one child) in month 280; her wish is 1, so no series
  follows. After the separation of sim 5 (month 388.9) the round of month 389 draws her
  search at 54.9 (chance about 1.5e-6), which says no. Nobody else ever searches; no man
  under 16 is ever in a list.
- The golden save is taken on day 100 of year 3: nobody has died, sims 7 to 10 are born;
  sim 1 is not pregnant (births 2) and her series waits for month 54, twelve months after
  the birth in month 42; sim 5 is not pregnant (births 2) and her series waits for month
  53, twelve months after the birth in month 41; both separations wait in the queue, and
  the round series waits for month 43.
- Households (ADR-0018 section 4, design note of the households; the life course data of
  format 2 with Dario's table of leaving home, tag LEAVE_HOME 22): after the founders and
  the couples `Start` writes four `core:household_formed` (entries 10 to 13, causes the
  input and the rule `core:household`): household 1 for sims 1 and 2, 2 for sims 5 and 6
  (the couples), 3 for sim 4 (20 years, alone), then 4 for sim 3, one second old, by the
  orphan rule (not moved out, no carer), his `leave_home` planned by his creation entry
  for the age 21 (one second before the end of year 20). Every child moves into its
  mother's household right after its `core:birth` (`core:moved`, causes the plan of the
  birth and the rule), its carer the mother, its `leave_home` planned after its deaths;
  the 18th birthdays end the carers. Sim 8 leaves home at 18 (year 19.3) and sim 7 at 19
  (year 19.9), each into a household of its own; the round of month 260 pairs them, so
  they found household 7 (she moves, then he) and households 5 and 6 end. Sim 3 leaves at
  21, sims 9, 10, 11 and 12 at 21, 22, 21 and 21 (years 24.2 to 27.5); sim 13 is born into
  household 7. Sim 1's death in year 27 leaves household 1 to sim 2 (she cares for nobody
  then). The separation of partnership 2 (year 29.9) moves sim 6 into household 13; no
  dependent child is left. The golden save holds households 1 to 4 and the `leave_home`
  events of sims 3 and 7 to 10.

The fork (ADR-0001 section 6): the same run up to the last second of year 4, then the
input `ForkOverride` with the stamp T_5 - 1 s. It acts at T_5 before every event of
that second and writes one entry `core:param_changed` with the input (sequence number
2) as its cause. The switch is on from then on: the living sims with a lethal affliction
(sims 1 and 3) are exposed since T_5, and that entry plans their deaths of
`core:disease_ar` from their whole budgets: sim 1 (at 35 years, past the change at 18)
after about 42.9 years of exposure, at about 77.9 years of age; sim 3 about 87.0 years
after the start. Sim 1 dies of senescence first, in year 27 as in the run, which cancels
her death of the disease; sim 3's lies beyond the thirty years. The fertility is the
same: the switch changes no chance, and the market reads no disease: sim 8 is paired
with sim 7 in month 260 as in the run (sim 8 has one dummy allele, sim 7 none, so sim 13
carries at most one and has no affliction). Sims 4 to 6 and the children of sims 5 and 6
have no affliction, and partnership 2 separates in the same second as in the run. So the
fork has one entry more than the run (the override), and its year hashes differ from
year 5 on (the switch and the exposures are state, the planned deaths are in the
queue). After the fork point the fork runs every event kind (birthdays, sim 1's
attempts, her conception in month 74 and birth in month 84, the year ends, the
separation of partnership 2 in year 29, sim 1's death in year 27, the rounds of the
market), and the fork's log has every log kind (the run's all but `core:param_changed`);
both are checked here (the coverage of ADR-0003 6.3).

Writes tests/golden/dummy.json, tests/golden/dummy.save and tests/golden/dummy_fork.json
(protected list, ADR-0000). Prints the pedigree values of the CLI export test as well.
Usage: .venv/bin/python tools/golden_reference.py [--check]
"""

import json
import math
import sys
from fractions import Fraction
from pathlib import Path

from postcard_reference import DUMMY, FEMALE, MALE, WILD, genotype

from world_reference import (
    CORE_VERSION,
    DATA_HASH,
    DEATH_KIND,
    ENDED_BY_DEATH,
    ENDED_BY_SEPARATION,
    MARKET_ROUND_KIND,
    MONTH,
    REPOSITORY,
    YEAR,
    World,
    budget_of,
    check_households,
    check_market,
    check_separations,
    check_story_kept,
    copies_at,
    death_key,
)
from rng_reference import chacha_key, child_key, founder_key, rank, str_key, unit, unit_open, unit_open0, words

ROOT = Path(__file__).resolve().parent.parent
DAY = 86_400
SEED = 1
TAG = "test:dummy"
BORN = [-30 * YEAR, -25 * YEAR - 100 * DAY, -1, -20 * YEAR, -25 * YEAR, -25 * YEAR]
SEX = [FEMALE, MALE, MALE, MALE, FEMALE, MALE]
GENES = [genotype(DUMMY, DUMMY), genotype(DUMMY, WILD), genotype(DUMMY, DUMMY)] + [genotype(WILD, WILD)] * 3
FOUNDERS = list(zip(BORN, SEX, GENES))
COUPLES = [(0, 1), (4, 5)]
YEARS = 30
SAVE_AT = 3 * YEAR + 100 * DAY
FORK_YEAR = 5
SENESCENCE, ACCIDENT = "core:senescence", "core:accident"

assert BORN == [-943_488_000, -794_880_000, -1, -628_992_000, -786_240_000, -786_240_000]


def scenario_matches() -> bool:
    """Whether tests/scenarios/dummy.json is the scenario of this reference: format 3, the
    seed, the tag, the switch off, no nutrition, the founders (born, sex, the alleles that
    are not the reference) and the couples in order, until the last second of year 29."""
    scenario = json.loads((ROOT / "tests" / "scenarios" / "dummy.json").read_text())
    start = scenario["start"]

    def alleles(genes) -> list:
        return [] if genes == (WILD, WILD) else [{"locus": "core:disease_ar", "copies": list(genes)}]

    return (
        scenario["format_version"] == 3
        and int(start["world_seed"], 16) == SEED
        and start["founder_tag"] == TAG
        and start["world_params"] == {"hereditary_diseases": False, "nutrition": []}
        and start["founders"]
        == [
            {"born": born, "sex": "female" if sex == FEMALE else "male", "alleles": alleles(genes)}
            for born, sex, genes in FOUNDERS
        ]
        and start["couples"] == [{"woman": woman, "man": man} for woman, man in COUPLES]
        and scenario["until"] == YEARS * YEAR - 1
    )


assert scenario_matches()

# The rates of the repository's data by hand from the genetics specification 7.2 (W7.2b
# to W7.2e, K7.1a, K7.1b): Gompertz a and b and the Makeham term, per sex.
SPEC_RATES = {FEMALE: (3.25e-6, 0.1182, 1.5e-4), MALE: (8.73e-6, 0.1118, 2.0e-4)}


def due_by_hand(index: int, code: str):
    """When founder `index` dies of the cause if nothing else happens first, by hand from
    the genetics specification 7.1 with Python's math (`x = ln(e^(b·x0) + b·B / a) / b`,
    `m` = 1 without a trait of longevity, or `x0 + B / λ`, from the age `x0` at the start),
    due `ceil((x - x0) · YEAR)` seconds after the start."""
    a, b, per_year = SPEC_RATES[SEX[index]]
    budget = budget_of(SEED, founder_key(TAG, index), code)
    x0 = (0 - BORN[index]) / 31_449_600.0
    x = math.log(math.exp(b * x0) + b * budget / a) / b if code == SENESCENCE else x0 + budget / per_year
    return None if x - x0 >= 100_000.0 else math.ceil((x - x0) * 31_449_600.0)


# The story above, checked: when each death of a founder is due, in seconds after the
# start.
DUE = {(sim, code): due_by_hand(sim - 1, code) for sim in range(1, 7) for code in (SENESCENCE, ACCIDENT)}
assert 27 * YEAR < DUE[(1, SENESCENCE)] < 28 * YEAR
assert 69 * YEAR < DUE[(2, SENESCENCE)] < 70 * YEAR < 87 * YEAR < DUE[(3, SENESCENCE)] < 88 * YEAR
assert 61 * YEAR < DUE[(4, SENESCENCE)] < 62 * YEAR and 68 * YEAR < DUE[(5, SENESCENCE)] < 69 * YEAR
assert 55 * YEAR < DUE[(6, SENESCENCE)] < 56 * YEAR
assert all(DUE[(sim, ACCIDENT)] > 1000 * YEAR for sim in range(1, 7))

# The disease of sims 1 and 3 in the data, and the death of sim 1 in the fork by hand from
# the design note of the diseases (section 4): exposed since T_5 at the age 35, past the
# change, so the age at death is `35 + B / h1`, due `ceil((a - 35) * YEAR)` seconds after
# T_5.
(CYSTIC,) = [disease for disease in REPOSITORY.diseases if disease.code == "core:disease_ar"]
AR_KEY = str_key("core:disease_ar")
FORK_AGE = (FORK_YEAR * YEAR - BORN[0]) / 31_449_600.0
assert FORK_AGE == 35.0 and FORK_AGE >= CYSTIC.hazard[1]
HER_AR_AGE = FORK_AGE + budget_of(SEED, founder_key(TAG, 0), "core:disease_ar") / CYSTIC.hazard[2]
HER_AR_DEATH = FORK_YEAR * YEAR + math.ceil((HER_AR_AGE - FORK_AGE) * 31_449_600.0)
assert 47 * YEAR < HER_AR_DEATH < 48 * YEAR


# Her fertility by hand from the genetics specification 5.6 and 11.3 with Python's math,
# straight from the draws (tags 3 CONCEPTION_ATTEMPT and 16 FERTILITY_WISH, 11.7), not from
# the model. The wish shape is K11.3d, the tilt and the adult rate those of the data.
WISH_SHAPE, WISH_TILT, ADULT_RATE = [0.10, 0.15, 0.40, 0.25, 0.10], 0.311, 0.20
assert (REPOSITORY.wish_shape, REPOSITORY.wish_tilt, REPOSITORY.adult_rate) == (WISH_SHAPE, WISH_TILT, ADULT_RATE)
HER_KEY, HIS_KEY = founder_key(TAG, 0), founder_key(TAG, 1)
HER_WISH_UNIT = unit(words(chacha_key(SEED, 16, HER_KEY, 0, 0))[0])
# The second couple: sim 5 (founder 4) and sim 6 (founder 5).
SECOND_KEY, SECOND_HIS_KEY = founder_key(TAG, 4), founder_key(TAG, 5)
SECOND_WISH_UNIT = unit(words(chacha_key(SEED, 16, SECOND_KEY, 0, 0))[0])


def hand_wish(u: float) -> int:
    """11.3: `P_n ∝ p_n · e^(α · n)`, the smallest `n` with `u < P_0 + … + P_n`."""
    weights = [p * math.exp(WISH_TILT * n) for n, p in enumerate(WISH_SHAPE)]
    total, acc = sum(weights), 0.0
    for n, weight in enumerate(weights):
        acc += weight / total
        if u < acc:
            return n
    return len(WISH_SHAPE) - 1


HER_WISH = hand_wish(HER_WISH_UNIT)
assert HER_WISH == 3
SECOND_WISH = hand_wish(SECOND_WISH_UNIT)
assert SECOND_WISH == 3


def hand_chance(month: int) -> float:
    """The chance of her attempt in `month`: she is `a = 30 + month / 13` years old, past
    30, so her factor is the decline `(50 − a) / (50 − 30)` (her ramp ended three years
    after her menarche, at most `θ + 3` = 15.3); his factor is 1 (younger than 45, his ramp
    ended at `θ − 0.7 + 2` = 15.32)."""
    a = 30.0 + month / 13.0
    assert 30.0 < a < 50.0 and -BORN[1] / YEAR + month / 13.0 < 45.0
    return ADULT_RATE * ((50.0 - a) / 20.0)


def second_chance(month: int) -> float:
    """The chance of sim 5's attempt in `month`: she is `a = 25 + month / 13` years old;
    her ramp is over (it ends at most `θ + 3 + 3` = 18.3), and the decline acts only past
    30, so her factor is 1 up to 30 and `(50 − a) / 20` after; his factor is 1 (he is as
    old as she, younger than 45)."""
    a = 25.0 + month / 13.0
    assert 25.0 < a < 45.0
    return ADULT_RATE * (1.0 if a <= 30.0 else (50.0 - a) / 20.0)


def hand_conceptions(her: int, him: int, chance, wish: int, first: int = 1) -> list:
    """(month of the conception, month of the birth) of a woman with one partner (design
    note sections 4 to 6): her series begins in month `first`, an attempt per month
    succeeds if the unit of its draw (subject her key, occasion the month, detail his key)
    is below `chance(month)`; the birth ten months later; with fewer births than her wish
    the next series twelve months after the birth (the pause); up to the end of the thirty
    years."""
    found, month = [], first
    while month < YEARS * 13 and len(found) < wish:
        if unit(words(chacha_key(SEED, 3, her, month, him))[0]) < chance(month):
            found.append((month, month + 10))
            month += 10 + 12
        else:
            month += 1
    return found


HAND_CONCEPTIONS = hand_conceptions(HER_KEY, HIS_KEY, hand_chance, HER_WISH)
assert HAND_CONCEPTIONS == [(7, 17), (32, 42), (74, 84)]
SECOND_CONCEPTIONS = hand_conceptions(SECOND_KEY, SECOND_HIS_KEY, second_chance, SECOND_WISH)
assert SECOND_CONCEPTIONS == [(2, 12), (31, 41), (63, 73)]
# Sim 1 conceives and bears after the fork point too (her third child), so the fork runs
# attempts, a conception and a birth after it.
assert HAND_CONCEPTIONS[2][0] * MONTH > FORK_YEAR * YEAR

# Twins (11.3, design note of the twins, section 1) by hand: per conception `n` the unit of
# the draw MULTIPLE_BIRTH (tag 17) at the key of the first child `child(her, his, n, 0)`;
# identical below 0.004 [K11.3f], fraternal below 0.004 + 0.010 [K11.3g], else one child.
# All three draw one child, so the story above stands with the twins.
TWINS_RATES = (0.004, 0.010)
assert REPOSITORY.twins == TWINS_RATES
MULTIPLE_UNITS = [
    unit(words(chacha_key(SEED, 17, child_key(HER_KEY, HIS_KEY, number, 0), 0, 0))[0])
    for number in range(len(HAND_CONCEPTIONS))
]
assert all(u >= TWINS_RATES[0] + TWINS_RATES[1] for u in MULTIPLE_UNITS)
SECOND_MULTIPLE_UNITS = [
    unit(words(chacha_key(SEED, 17, child_key(SECOND_KEY, SECOND_HIS_KEY, number, 0), 0, 0))[0])
    for number in range(len(SECOND_CONCEPTIONS))
]
assert all(u >= TWINS_RATES[0] + TWINS_RATES[1] for u in SECOND_MULTIPLE_UNITS)

# Her separation by hand (design note of the separation, sections 1 and 3) with Python's
# math: `B = −ln(u)` with `u` = unit_open0 of the draw SEPARATION_BUDGET (tag 18) at (her
# key, occasion 0, his key), due `max(ceil(B / λ · YEAR), 1)` seconds after the start.
SEPARATION_RATE = 0.00867
assert REPOSITORY.separation_rate == SEPARATION_RATE
SEPARATION_BUDGET_VALUE = 0.0 - math.log(unit_open0(words(chacha_key(SEED, 18, HER_KEY, 0, HIS_KEY))[0]))
SEPARATION_DUE = max(math.ceil(SEPARATION_BUDGET_VALUE / SEPARATION_RATE * 31_449_600.0), 1)
assert 49 * YEAR < SEPARATION_DUE < 50 * YEAR
# The same for partnership 2 (sim 5's `k` 0): due after the fork point and before the end.
SECOND_SEPARATION_BUDGET = 0.0 - math.log(unit_open0(words(chacha_key(SEED, 18, SECOND_KEY, 0, SECOND_HIS_KEY))[0]))
SECOND_SEPARATION_DUE = max(math.ceil(SECOND_SEPARATION_BUDGET / SEPARATION_RATE * 31_449_600.0), 1)
assert FORK_YEAR * YEAR < SECOND_SEPARATION_DUE <= YEARS * YEAR - 1
assert 29 * YEAR < SECOND_SEPARATION_DUE < DUE[(5, SENESCENCE)] and SECOND_SEPARATION_DUE < DUE[(6, SENESCENCE)]


# The market by hand (genetics specification 11.2; design note of the market, section 3)
# with Python's math, straight from the draws (tags 19 MARKET_SEARCH, 5 PAIRING_RANK, 21
# PAIRING_NOISE), not from the model. Sim 1's daughter (sim 8, born in month 17, her key
# that of the first child of sims 1 and 2) is single and in the market age from month 225
# (her 16th birthday) on; in a round `n` she searches if `u < p` with `p = c · e^(−z²/2)`,
# `z = (a − m) / 7`, below 1, times `ρ` before 18. The first `n` where she does: her
# candidates are the single men of 16 or more then, by their rank (sims 3 and 4, sims 7 and
# 9 of sim 5, her brother sim 10; sims 11 and 12 are under 16, sims 2 and 6 partnered); her
# brother (φ = 1/4 [W2.7b]) is skipped; `U = −0.02 · (Δa − 4.69)² + G`, `G = −ln(−ln(u))`.
MARKET_C, MARKET_M, MARKET_RHO, MARKET_GAP = 0.01179, 25.23, 0.0918, 4.69
assert (REPOSITORY.market_height, REPOSITORY.market_center, REPOSITORY.market_minor) == (MARKET_C, MARKET_M, MARKET_RHO)
assert REPOSITORY.pairing_gap == MARKET_GAP and REPOSITORY.pairing_weight_age == 0.02 and REPOSITORY.market_width == 7.0
DAUGHTER, DAUGHTER_KEY = 8, child_key(HER_KEY, HIS_KEY, 0, 0)
DAUGHTER_BORN = HAND_CONCEPTIONS[0][1] * MONTH
assert DAUGHTER_BORN + 16 * YEAR == 225 * MONTH


def hand_search_chance(age_seconds: int) -> float:
    z = (age_seconds / 31_449_600.0 - MARKET_M) / 7.0
    return min(1.0, MARKET_C * math.exp(-0.5 * z * z)) * (MARKET_RHO if age_seconds < 18 * YEAR else 1.0)


def searches(key: int, born: int, n: int) -> bool:
    return unit(words(chacha_key(SEED, 19, key, n, 0))[0]) < hand_search_chance(n * MONTH - born)


DAUGHTER_SEARCH = next(n for n in range(225, YEARS * 13) if searches(DAUGHTER_KEY, DAUGHTER_BORN, n))
assert DAUGHTER_SEARCH == 260 and (DAUGHTER_SEARCH * MONTH - DAUGHTER_BORN) >= 18 * YEAR
# The men by SimId and their keys and births (sims 7, 9 and 11 of sim 5, 10 and 12 of sim 1).
MEN = {
    3: (founder_key(TAG, 2), BORN[2]),
    4: (founder_key(TAG, 3), BORN[3]),
    7: (child_key(SECOND_KEY, SECOND_HIS_KEY, 0, 0), SECOND_CONCEPTIONS[0][1] * MONTH),
    9: (child_key(SECOND_KEY, SECOND_HIS_KEY, 1, 0), SECOND_CONCEPTIONS[1][1] * MONTH),
    10: (child_key(HER_KEY, HIS_KEY, 1, 0), HAND_CONCEPTIONS[1][1] * MONTH),
}
assert all(DAUGHTER_SEARCH * MONTH - born >= 16 * YEAR for _, born in MEN.values())
assert DAUGHTER_SEARCH * MONTH - SECOND_CONCEPTIONS[2][1] * MONTH < 16 * YEAR and DAUGHTER_SEARCH * MONTH - HAND_CONCEPTIONS[2][1] * MONTH < 16 * YEAR
DAUGHTER_RANKED = sorted(MEN, key=lambda man: (rank(SEED, 5, DAUGHTER_KEY, DAUGHTER_SEARCH, MEN[man][0]), MEN[man][0]))
DAUGHTER_CANDIDATES = [man for man in DAUGHTER_RANKED if man != 10]


def hand_utility(man: int) -> float:
    key, born = MEN[man]
    gap = (DAUGHTER_BORN - born) / 31_449_600.0
    g = -math.log(-math.log(unit_open(words(chacha_key(SEED, 21, DAUGHTER_KEY, DAUGHTER_SEARCH, key))[0])))
    return -0.02 * (gap - MARKET_GAP) ** 2 + g


DAUGHTER_UTILITIES = {man: hand_utility(man) for man in DAUGHTER_CANDIDATES}
DAUGHTER_PARTNER = max(DAUGHTER_CANDIDATES, key=DAUGHTER_UTILITIES.get)
assert DAUGHTER_PARTNER == 7
# Her conceptions with him by hand: at 18.7 her ramp is long over (her menarche is at most
# `θ + 3` = 15.3) and her decline begins at 30, his ramp is over (19 years): chance 0.2;
# her series from the month after the pairing, the pause of 12 months, her wish.
DAUGHTER_WISH = hand_wish(unit(words(chacha_key(SEED, 16, DAUGHTER_KEY, 0, 0))[0]))
DAUGHTER_CONCEPTIONS = hand_conceptions(DAUGHTER_KEY, MEN[7][0], lambda month: ADULT_RATE, DAUGHTER_WISH, DAUGHTER_SEARCH + 1)
assert all((month * MONTH - DAUGHTER_BORN) / 31_449_600.0 < 30.0 for month, _ in DAUGHTER_CONCEPTIONS)
assert DAUGHTER_CONCEPTIONS == [(270, 280)]
DAUGHTER_MULTIPLE = unit(words(chacha_key(SEED, 17, child_key(DAUGHTER_KEY, MEN[7][0], 0, 0), 0, 0))[0])
assert DAUGHTER_MULTIPLE >= TWINS_RATES[0] + TWINS_RATES[1]
# Sim 5 is single again after her separation (month 388.9), at 54.9: the round of month 389
# draws her search, which says no; nobody else is a single woman of 16 or more.
assert not searches(SECOND_KEY, BORN[4], 389)


def dummy_world() -> World:
    return World(SEED, TAG, False, FOUNDERS, COUPLES)


def ar_deaths(world: World) -> list:
    """The planned deaths of `core:disease_ar` of `world`: (sim, time, plan) by sim."""
    return sorted(
        (event.subject[1], event.time, event.plan)
        for event in world.queue
        if event.kind == DEATH_KIND and death_key(event.payload) == AR_KEY
    )


def run(fork: bool) -> World:
    world = dummy_world()
    if fork:
        world.advance(FORK_YEAR * YEAR - 1)
        world.override(True)
        # The override is the last entry so far; EventIds count from 1. It plans the
        # deaths of the disease of the living with it, sim 1's as by hand.
        entry = len(world.log)
        assert ar_deaths(world)[0] == (1, HER_AR_DEATH, (entry, FORK_YEAR * YEAR))
        assert all(plan == (entry, FORK_YEAR * YEAR) for *_, plan in ar_deaths(world))
    world.advance(YEARS * YEAR - 1)
    return world


# SimIds by birth: sim 5's children (months 12, 41, 73) and sim 1's (17, 42, 84) alternate;
# sim 8's son (month 280) is sim 13.
CHILDREN = [8, 10, 12]
SECOND_CHILDREN = [7, 9, 11]
THIRD_CHILDREN = [13]
CHILD_MONTHS = [birth for _, birth in HAND_CONCEPTIONS]
SECOND_CHILD_MONTHS = [birth for _, birth in SECOND_CONCEPTIONS]
assert sorted(zip(CHILD_MONTHS + SECOND_CHILD_MONTHS, CHILDREN + SECOND_CHILDREN)) == sorted(
    zip(sorted(CHILD_MONTHS + SECOND_CHILD_MONTHS), range(7, 13))
)
FIRST = dummy_world()
assert all(FIRST.cause_due(sim, code) == DUE[(sim, code)] for sim, code in DUE)
assert FIRST.pending_separation(1) == SEPARATION_DUE and FIRST.pending_separation(2) == SECOND_SEPARATION_DUE
assert [FIRST.sims[sim][8] for sim in range(1, 7)] == [1, 0, 0, 0, 1, 0]
check_market(FIRST)
check_households(FIRST)
PARENT = run(fork=False)
FORK = run(fork=True)
assert len(PARENT.year_hashes) == YEARS and len(FORK.year_hashes) == YEARS
assert FORK.year_hashes[:FORK_YEAR] == PARENT.year_hashes[:FORK_YEAR]
assert all(
    ours != theirs
    for ours, theirs in zip(FORK.year_hashes[FORK_YEAR:], PARENT.year_hashes[FORK_YEAR:])
)
assert len(FORK.log) == len(PARENT.log) + 1, "the override; the same death"
# Sim 1 dies of senescence in year 27, in the run and in the fork: the only death.
HER_DEATH = DUE[(1, SENESCENCE)]
for world in (PARENT, FORK):
    assert sorted(world.archive) == [1] and world.archive[1][1:3] == (HER_DEATH, str_key(SENESCENCE))
    assert sorted(world.sims) == list(range(2, 14))
    # Her death comes before the separation (year 49) and cancels it; partnership 2
    # separates in year 29, in the run and in the fork. The market pairs sim 8 with sim 7
    # in month 260 (partnership 3); its separation lies beyond the run.
    assert world.partnerships == {
        1: [1, 2, 0, (HER_DEATH, ENDED_BY_DEATH)],
        2: [5, 6, 0, (SECOND_SEPARATION_DUE, ENDED_BY_SEPARATION)],
        3: [DAUGHTER, DAUGHTER_PARTNER, DAUGHTER_SEARCH * MONTH, None],
    }
    assert HER_DEATH < SEPARATION_DUE and world.pending_separation(1) is None
    assert [(t, number) for t, number, *_ in world.separations] == [(SECOND_SEPARATION_DUE, 2)]
    assert all(world.sims[sim][8] == 0 for sim in world.sims if sim not in (5, 8))
    assert world.sims[5][8] == 1 and world.sims[8][8] == 1
    check_separations(world)
    check_market(world)
    check_households(world)
    # The households change no story (design note of the households, section 8).
    check_story_kept(world)
    # The rounds (cadence, test rule 6): every month, 13 a year; sim 8's choice as by hand.
    assert [kind for _, kind in world.ran].count(MARKET_ROUND_KIND) == YEARS * 13 - 1
    choices = [(record["n"], choice) for record in world.rounds for choice in record["choices"]]
    assert [(n, choice["woman"]) for n, choice in choices] == [(DAUGHTER_SEARCH, DAUGHTER)]
    ((_, choice),) = choices
    assert choice["walked"] == [(man, Fraction(1, 4) if man == 10 else Fraction(0), man == 10) for man in DAUGHTER_RANKED]
    assert [man for man, *_ in choice["candidates"]] == DAUGHTER_CANDIDATES and choice["outcome"] == "paired"
    assert all(abs(u - DAUGHTER_UTILITIES[man]) <= 1e-12 for man, _, _, _, u in choice["candidates"])
    assert [record["n"] for record in world.rounds] == list(range(225, DAUGHTER_SEARCH + 1)) + [389]
assert [PARENT.sims[sim][0] for sim in CHILDREN] == [month * MONTH for month in CHILD_MONTHS]
assert [PARENT.sims[sim][0] for sim in SECOND_CHILDREN] == [month * MONTH for month in SECOND_CHILD_MONTHS]
assert [PARENT.sims[sim][7] for sim in CHILDREN] == [(1, 2, number) for number in range(3)]
assert [PARENT.sims[sim][7] for sim in SECOND_CHILDREN] == [(5, 6, number) for number in range(3)]
assert [PARENT.sims[sim][0] for sim in THIRD_CHILDREN] == [birth * MONTH for _, birth in DAUGHTER_CONCEPTIONS]
assert [PARENT.sims[sim][7] for sim in THIRD_CHILDREN] == [(DAUGHTER, DAUGHTER_PARTNER, 0)]
assert [PARENT.sims[sim][2] for sim in CHILDREN] == [FEMALE, MALE, MALE]
assert [PARENT.sims[sim][2] for sim in SECOND_CHILDREN] == [MALE, MALE, MALE]
# The conceptions, births and wishes as by hand, the chance of every attempt too; sim 1's
# attempts run from month 1 to her third conception, with the pauses after the births,
# sim 5's likewise.
assert not PARENT.pregnancies and PARENT.archive[1][7:9] == (len(CHILDREN), len(CHILDREN))
assert PARENT.sims[5][5:7] == [len(SECOND_CHILDREN), len(SECOND_CHILDREN)]
assert PARENT.wish_of(1) == HER_WISH and PARENT.wish_of(5) == SECOND_WISH
ATTEMPT_MONTHS = list(range(1, 8)) + list(range(29, 33)) + list(range(54, 75))
SECOND_ATTEMPT_MONTHS = list(range(1, 3)) + list(range(24, 32)) + list(range(53, 64))
HER_ATTEMPTS = [attempt for attempt in PARENT.attempts if attempt[1] == 1]
SECOND_ATTEMPTS = [attempt for attempt in PARENT.attempts if attempt[1] == 5]
THIRD_ATTEMPTS = [attempt for attempt in PARENT.attempts if attempt[1] == DAUGHTER]
assert len(HER_ATTEMPTS) + len(SECOND_ATTEMPTS) + len(THIRD_ATTEMPTS) == len(PARENT.attempts)
# Sim 8's attempts: from the month after the pairing to her conception, from twelve months
# after the birth on if her wish allows, each with the chance 0.2.
assert [t // MONTH for t, _, _, _, success in THIRD_ATTEMPTS if success] == [c for c, _ in DAUGHTER_CONCEPTIONS]
assert all(p == ADULT_RATE for _, _, _, p, _ in THIRD_ATTEMPTS) and THIRD_ATTEMPTS[0][0] == (DAUGHTER_SEARCH + 1) * MONTH
assert [t // MONTH for t, *_ in HER_ATTEMPTS] == ATTEMPT_MONTHS
assert [t // MONTH for t, *_ in SECOND_ATTEMPTS] == SECOND_ATTEMPT_MONTHS
assert [t // MONTH for t, _, _, _, success in HER_ATTEMPTS if success] == [c for c, _ in HAND_CONCEPTIONS]
assert [t // MONTH for t, _, _, _, success in SECOND_ATTEMPTS if success] == [c for c, _ in SECOND_CONCEPTIONS]
assert all(abs(p - hand_chance(t // MONTH)) <= 1e-15 for t, _, _, p, _ in HER_ATTEMPTS)
assert all(p == second_chance(t // MONTH) == ADULT_RATE for t, _, _, p, _ in SECOND_ATTEMPTS)
assert FORK.attempts == PARENT.attempts
assert [(t // MONTH, u, kind) for t, woman, _, _, u, kind, _ in PARENT.multiples if woman == 1] == [
    (month, u, "single") for (month, _), u in zip(HAND_CONCEPTIONS, MULTIPLE_UNITS)
]
assert [(t // MONTH, u, kind) for t, woman, _, _, u, kind, _ in PARENT.multiples if woman == 5] == [
    (month, u, "single") for (month, _), u in zip(SECOND_CONCEPTIONS, SECOND_MULTIPLE_UNITS)
]
LETHAL = [sim for sim in sorted(PARENT.sims) if PARENT.sims[sim][4]]
assert LETHAL == [3]
assert all([a.triple() for a in PARENT.sims[sim][4]] == [(AR_KEY, 0.0, (0.0, None))] for sim in LETHAL)
assert all(copies_at(PARENT.sims[sim][3], CYSTIC.locus) == [1, 0] for sim in CHILDREN)
# In the fork sims 1 and 3 are exposed since T_5, and the override planned their deaths
# of the disease; hers went with her death. The override is the first entry the fork
# writes differently.
FORK_OVERRIDE_ENTRY = next(i for i, (ours, theirs) in enumerate(zip(FORK.log, PARENT.log)) if ours != theirs) + 1
assert [a.triple() for a in FORK.archive[1][6]] == [(AR_KEY, 0.0, (0.0, FORK_YEAR * YEAR))]
assert [a.triple() for a in FORK.sims[3][4]] == [(AR_KEY, 0.0, (0.0, FORK_YEAR * YEAR))]
FORK_AR_DEATHS = ar_deaths(FORK)
assert [sim for sim, *_ in FORK_AR_DEATHS] == LETHAL
assert all(plan == (FORK_OVERRIDE_ENTRY, FORK_YEAR * YEAR) for _, _, plan in FORK_AR_DEATHS)
assert all(time > YEARS * YEAR for _, time, _ in FORK_AR_DEATHS)
# The rounds of the fork are those of the run (the switch changes no draw of the market);
# only the EventId of the pairing's entry is one more (the override).
assert [
    (r["n"], r["men"], r["considered"], [{k: v for k, v in c.items() if k != "entry"} for c in r["choices"]]) for r in FORK.rounds
] == [(r["n"], r["men"], r["considered"], [{k: v for k, v in c.items() if k != "entry"} for c in r["choices"]]) for r in PARENT.rounds]

# Coverage (ADR-0003 6.3): after the fork point the fork runs every event kind
# (tests/golden/event_codes.txt: year_end 1 to leave_home 8), and the fork writes every
# log kind (tests/golden/log_codes.txt: payloads 0 to 11; the run all but
# `core:param_changed`).
EVENT_KINDS, LOG_KINDS = set(range(1, 9)), set(range(12))
assert {kind for t, kind in FORK.ran if t >= FORK_YEAR * YEAR} == EVENT_KINDS
assert set(FORK.entry_kinds) == LOG_KINDS and set(PARENT.entry_kinds) == LOG_KINDS - {3}

# The households of the story above (design note of the households).
START_HOMES = [(row[2], row[3], row[4], row[5]) for row in PARENT.home_log if row[0] == 0]
assert START_HOMES == [
    (1, [1, 2], "start couple", 10),
    (2, [5, 6], "start couple", 11),
    (3, [4], "start adult", 12),
    (4, [3], "start minor, orphan rule", 13),
]
LEAVE_AGES = {sim: (PARENT.leave_home_second(sim) - PARENT.born_of(sim)) // YEAR for sim in [3] + CHILDREN + SECOND_CHILDREN}
assert LEAVE_AGES == {3: 21, 7: 19, 8: 18, 9: 21, 10: 22, 11: 21, 12: 21}
LEFT = [(t, sims[0]) for t, kind, _, sims, why, _ in PARENT.home_log if kind == "formed" and why == "leave_home"]
assert [sim for _, sim in LEFT] == [8, 7, 3, 9, 10, 11, 12] and all(t < DAUGHTER_SEARCH * MONTH for t, sim in LEFT if sim in (7, 8))
assert PARENT.households[7] == [DAUGHTER_SEARCH * MONTH, None] and PARENT.members(7) == [7, 8, 13]
assert [row[1:4] for row in PARENT.separation_homes] == [(5, 6, [])] and PARENT.sims[6][9] == 13
assert not [row for row in PARENT.carer_log if row[4] not in ("birth", "18th birthday")]
# The fork has the households of the run; from the fork point on every entry is one later
# (the override).
assert FORK.home_log == [
    row[:5] + (row[5] if row[5] is None or row[0] < FORK_YEAR * YEAR else row[5] + 1,) for row in PARENT.home_log
]


def golden_save() -> bytes:
    """The save on day 100 of year 3: nobody pregnant, sim 1's series pending in month 54,
    sim 5's in month 53, both separations pending, the next round at the start of month 43
    (no single woman of 16 or more yet)."""
    world = dummy_world()
    world.advance(SAVE_AT)
    assert not world.rounds and world.next_event_id == len(world.log) + 1
    (series,) = [event for event in world.queue if event.kind == MARKET_ROUND_KIND]
    assert series.time == 43 * MONTH and SAVE_AT < 43 * MONTH
    check_market(world)
    check_households(world)
    assert not world.archive and sorted(world.sims) == list(range(1, 11))
    assert not world.pregnancies and world.sims[1][5:7] == [2, 2] and world.sims[5][5:7] == [2, 2]
    assert world.pending_series(1) == (CHILD_MONTHS[1] + 12) * MONTH
    assert world.pending_series(5) == (SECOND_CHILD_MONTHS[1] + 12) * MONTH
    assert world.pending_separation(1) == SEPARATION_DUE and world.pending_separation(2) == SECOND_SEPARATION_DUE
    check_separations(world)
    return world.save()


def golden(world: World) -> str:
    """The text of tests/golden/<scenario>.json."""
    content = {
        "core_version": CORE_VERSION,
        "data_hash": f"{DATA_HASH:016x}",
        "year_hashes": [
            {"year": year, "hash": f"{value:016x}"}
            for year, value in enumerate(world.year_hashes)
        ],
        "log_hash": f"{world.log_hash():016x}",
    }
    return json.dumps(content, indent=2) + "\n"


def pedigree_rows() -> list:
    """The values of the pedigree export after thirty years (crates/sim-cli/tests/cli.rs):
    per sim its lineage key, sex, alleles that are not the reference (locus, copies
    maternal then paternal), birth, death and parents; and the partnerships."""
    rows = []
    for sim in sorted(set(PARENT.sims) | set(PARENT.archive)):
        if sim in PARENT.sims:
            born, key, sex, person, _, _, _, parents, *_ = PARENT.sims[sim]
            died = "null"
        else:
            born, left, cause, key, sex, person, _, _, _, parents, *_ = PARENT.archive[sim]
            died = f"{left} of {cause:016x}"
        alleles = [(locus, copies_at(person, PARENT.data.loci[locus])) for locus in PARENT.data.loci]
        alleles = [(locus, copies) for locus, copies in alleles if any(copies)]
        sex_name = "female" if sex == FEMALE else "male"
        rows.append(f"pedigree_sim_{sim} key {key:016x} sex {sex_name} alleles {alleles} born {born} died {died} parents {parents}")
    for number, (woman, man, formed, ended) in sorted(PARENT.partnerships.items()):
        ended_text = "null" if ended is None else f"{ended[0]} by {('death', 'separation')[ended[1]]}"
        rows.append(f"pedigree_partnership_{number} woman {woman} man {man} formed {formed} ended {ended_text}")
    return rows


if __name__ == "__main__":
    text = golden(PARENT)
    fork_text = golden(FORK)
    json_path = ROOT / "tests" / "golden" / "dummy.json"
    save_path = ROOT / "tests" / "golden" / "dummy.save"
    fork_path = ROOT / "tests" / "golden" / "dummy_fork.json"
    if "--check" in sys.argv:
        ok = (
            json_path.read_text() == text
            and save_path.read_bytes() == golden_save()
            and fork_path.read_text() == fork_text
        )
        print("goldens match the reference" if ok else "goldens DIFFER from the reference")
        sys.exit(0 if ok else 1)
    json_path.write_text(text)
    save_path.write_bytes(golden_save())
    fork_path.write_text(fork_text)
    print(text, end="")
    print(f"save: {len(golden_save())} bytes")
    print(fork_text, end="")
    print(f"entries: {len(PARENT.log)} in the run, {len(FORK.log)} in the fork")
    print(f"founder deaths due (seconds after the start): {DUE}")
    children = {sim: PARENT.cause_due(sim, SENESCENCE) for sim in CHILDREN}
    print(f"children: born (months) {CHILD_MONTHS}, senescence due (seconds) {children}")
    second_children = {sim: PARENT.cause_due(sim, SENESCENCE) for sim in SECOND_CHILDREN}
    print(f"children of sim 5: sims {SECOND_CHILDREN} born (months) {SECOND_CHILD_MONTHS}, senescence due (seconds) {second_children}")
    print(f"fertility: her wish {HER_WISH} (u {HER_WISH_UNIT!r}), menarche {PARENT.menarche_of(1)!r}")
    print(f"fertility: conceptions and births (months) {HAND_CONCEPTIONS}")
    print(f"fertility: sim 5 wish {SECOND_WISH} (u {SECOND_WISH_UNIT!r}), menarche {PARENT.menarche_of(5)!r}")
    print(f"fertility: sim 5 conceptions and births (months) {SECOND_CONCEPTIONS}")
    print(f"fertility: twins draw u per conception (month, woman, u, kind) {[(t // MONTH, woman, u, kind) for t, woman, _, _, u, kind, _ in PARENT.multiples]}")
    print(f"fertility: attempts (month, chance, success) {[(t // MONTH, p, ok) for t, _, _, p, ok in HER_ATTEMPTS]}")
    print(f"fertility: sim 5 attempts (month, chance, success) {[(t // MONTH, p, ok) for t, _, _, p, ok in SECOND_ATTEMPTS]}")
    print(f"separation: budget {SEPARATION_BUDGET_VALUE!r}, due {SEPARATION_DUE} ({SEPARATION_DUE / YEAR:.4f} years), cancelled by her death at {HER_DEATH}")
    for name, world in (("run", PARENT), ("fork", FORK)):
        rows = [(t, number, entry, plan) for t, number, entry, plan, _ in world.separations]
        print(f"separation: {name} (time, partnership, entry, plan) {rows}")
    print(
        f"separation: partnership 2 budget {SECOND_SEPARATION_BUDGET!r}, due {SECOND_SEPARATION_DUE}"
        f" ({SECOND_SEPARATION_DUE / YEAR:.4f} years, month {SECOND_SEPARATION_DUE / MONTH:.4f})"
    )
    print(f"fertility: children's wishes {[(sim, PARENT.wish_of(sim)) for sim in CHILDREN + SECOND_CHILDREN if PARENT.sims[sim][2] == FEMALE]}")
    print(f"fork: death of sim 1 of core:disease_ar due at {HER_AR_DEATH} (age {HER_AR_AGE!r}), planned by entry {FORK_OVERRIDE_ENTRY}")
    print(f"market: rounds {[kind for _, kind in PARENT.ran].count(MARKET_ROUND_KIND)} in the run; rounds with a single woman of 16 or more {[r['n'] for r in PARENT.rounds][:3]} ... {[r['n'] for r in PARENT.rounds][-3:]}")
    print(f"market: sim 8 searches first in round {DAUGHTER_SEARCH} at {(DAUGHTER_SEARCH * MONTH - DAUGHTER_BORN) / YEAR:.4f} years, chance {hand_search_chance(DAUGHTER_SEARCH * MONTH - DAUGHTER_BORN)!r}")
    print(f"market: her list by rank {DAUGHTER_RANKED} (sim 10 her brother skipped), utilities by hand {DAUGHTER_UTILITIES}")
    for record in PARENT.rounds:
        for choice in record["choices"]:
            print(f"market: round {record['n']} woman {choice['woman']} walked {[(m, str(phi), skipped) for m, phi, skipped in choice['walked']]} candidates (man, gap, phi, G, U) {choice['candidates']} chosen {choice['chosen']} {choice['outcome']} partnership {choice.get('partnership')} entry {choice.get('entry')}")
    plan = [plan for plan in PARENT.separation_plans if plan[0] == 3][0]
    print(f"market: partnership 3 separation (number, k, budget, due) {plan} ({plan[3] / YEAR:.4f} years)")
    print(f"market: sim 8 wish {DAUGHTER_WISH} conceptions {DAUGHTER_CONCEPTIONS} attempts {[(t // MONTH, ok) for t, _, _, _, ok in THIRD_ATTEMPTS]}")
    print(f"market: sim 5 at round 389 chance {hand_search_chance(389 * MONTH - BORN[4])!r}")
    print(f"coverage: event kinds after the fork point {sorted({kind for t, kind in FORK.ran if t >= FORK_YEAR * YEAR})}, log kinds fork {sorted(set(FORK.entry_kinds))} run {sorted(set(PARENT.entry_kinds))}")
    print(f"fork: deaths of core:disease_ar (sim, time, plan) {FORK_AR_DEATHS}")
    for name, world in (("run", PARENT), ("fork", FORK)):
        rows = [(t, kind, household, sims, why, entry) for t, kind, household, sims, why, entry in world.home_log]
        print(f"households: {name} (time, kind, household, sims, why, entry) {rows}")
    print(f"households: run carers (time, child, old, new, why) {[row for row in PARENT.carer_log if row[4] != 'birth']}")
    print(f"households: run households (id: [formed, ended]) {PARENT.households} next {PARENT.next_household_id}")
    print(f"households: leave_home seconds {[(sim, PARENT.leave_home_second(sim)) for sim in sorted(set(PARENT.sims) | set(PARENT.archive))]}")
    print(f"households: leave_home after the fork point (time, kind) {[(t, kind) for t, kind in FORK.ran if kind == 8 and t >= FORK_YEAR * YEAR]}")
    for row in pedigree_rows():
        print(row)

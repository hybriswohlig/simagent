#!/usr/bin/env python3
"""The hand story of the scenario `tests/scenarios/dummy.json` on the map 3 fixture
(tests/golden/map3 with the repository's life course data, `world_reference.MAP3`): every
event of its thirty years and of its fork at year 5 with the override "hereditary diseases
on", computed by hand from the specification and checked against the model in
tools/world_reference.py, independently of the Rust code.

Until the switch to map 4 this story was tools/golden_reference.py and wrote the goldens of
the scenario, because the repository's data were map 3. Since slice S5 (design note of the
switch, sections 5 and 14, I-f) the repository's data are map 4 with the pool, where every
founder draws the alleles it does not name (F1, tag 28) and the story's deaths follow drawn
longevity; tools/golden_reference.py now writes the goldens on data/ without a story, and
this story stays on map 3, where F1 draws nothing (no additive locus, no frequencies), as
the model's check by hand. `tools/golden_reference.py --story` and `--check` run it;
`--check` also compares the writer's five files on map 3 with this story's (the same run,
two ways). On map 3 the files are the goldens of core 39 but for the core version.

The scenario (format 5 with `pool` null, design note of the emigration, section 1): seed 1, founder tag
`test:dummy`, the switch off, neither causes of death nor a fertility of its own, the
capacity 9; it runs on the map 3 fixture (the CLI's data until S5): senescence and accident
with the rates of the genetics specification 7.2 (ADR-0017 section 2), its six hereditary
diseases (ADR-0017 section 3), the fertility of 5.6 and 11.3 (ADR-0017 section 4: the
curves, the adult rate 0.20, the wish, the pause of 12 months, ten months of pregnancy),
the separation (ADR-0018 section 3), the partner market of 11.2 (ADR-0018 section 1,
design note of the market) with the values of the data, and the emigration by soft
capacity (ADR-0020 section 6, design note of the emigration). The capacity is there so that
the goldens run the event `emigration` and write `core:emigrated` (the coverage of ADR-0003
6.3); 9 is the smallest capacity with which every event and log kind the goldens covered
before stays covered (8 loses the separation, see the notes of the reference), and with 9
one household emigrates, after the fork point.
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
and founder 3 (sim 4, a single man) only fills its place. With the capacity the couple of
sims 5 and 6 emigrates in year 5 before its separation; sim 4, the single man, is then the
partner the market gives sim 8, and that partnership separates in year 27.9, so the
separation still runs after the fork point.

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
- Emigration (design note of the emigration; ADR-0020 section 6): `Start` plans the event
  `Emigration { capacity: 9 }` right after the round series (insertion number 3, every
  later one of `Start` one higher than before), first at the last second of year 0, every
  year. With no death before year 27 the living are the six founders and the children
  born so far: 7 at the end of year 0 (sim 7, month 12), 8 at the ends of years 1 and 2
  (sim 8, month 17), 10 at the ends of years 3 and 4 (sims 9 and 10, months 41 and 42), 11
  at the end of year 5 (sim 11, month 73). So the years 0 to 2 draw nothing (`P ≤ 9`); in
  years 3 and 4 `e = 10 / 9 − 1` (about 0.111) and in year 5 `e = 11 / 9 − 1` (about 0.222).
  Three households have a member of 18 or more and draw (EMIGRATION, tag 24, the smallest
  lineage key of those members, the year, 0): household 1 (sims 1 and 2 with their
  children), household 2 (sims 5 and 6 with sim 3, their foster son, and their sons 7, 9
  and 11) and household 3 (sim 4). Every draw of years 3 and 4 says no (the smallest unit is
  0.177 of household 3 in year 4); in year 5 household 2 draws 0.134 below 0.222 and leaves
  whole at 188 697 599 (the last second of year 5, after the fork point): entries 91 to 96
  `core:emigrated` for sims 3, 5, 6, 7, 9 and 11 (causes the plan of the event by
  `core:started`, the rule `core:capacity`, the draw), each to the archive with
  `LeftBy::Emigration`, their events cancelled (birthdays, deaths, `leave_home`, the
  separation of partnership 2, which ends with `EndedBy::Emigration`), household 2 ends.
  Then six live (sims 1, 2, 4, 8, 10, 12 after month 84), at most 7 later, never above 9:
  no draw after year 5.
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
  her series) has the budget about 0.259: it would be due at 940 727 538 s, after about
  29.91 years; the emigration of sims 5 and 6 in year 5 cancels it (the partnership ends by
  emigration). Partnership 3 (sims 8 and 4, below) separates at 878 816 547 s (year
  27.94), in the run and in the fork: one entry `core:separated` (subjects sims 8 and 4,
  causes the plan and the draw); sim 4 founds household 8, their son sim 13 stays with his
  mother.
- The partner market (genetics specification 11.2, design note of the market): `Start`
  plans the series of the rounds right after the year end series (insertion number 2,
  every later one of `Start` one higher), the first at the start of month 1, then every
  month (389 rounds in the thirty years). A round runs only for the single women of 16 or
  more; up to month 224 there are none (sims 1 and 5 are partnered, every child younger).
  Sim 8 (sim 1's daughter, born in month 17) is 16 in month 225: from that round on she
  is drawn (MARKET_SEARCH, `c` 0.01179, `m` 25.23, `w` 7, under 18 times `ρ` 0.0918, so
  about 0.0005 a round before 18 and 0.007 at 18) and searches first in month 260, at
  18.7. The single men of 16 or more then are sim 4 (founder) and her brother sim 10 (sim
  12 is under 16, sim 2 partnered; sims 3, 7 and 9 emigrated in year 5). In the order of
  PAIRING_RANK the list is 4, 10; her brother (φ = 1/4) is skipped. With `U = −0.02 · (Δa −
  4.69)² + G` she chooses sim 4, the only candidate (40 years old, `G` about 2.11, `U` about
  −3.41): partnership 3 at month 260, its entry `core:partnered` with the causes the plan of
  the round (entry 1 at 0), the rule `core:partner_market` and the draw PAIRING_NOISE (her
  key, 260, his key); her `partnerships` goes to 1, her series begins in month 261 and its
  separation is planned (budget about 0.0689, due at 878 816 547, year 27.94). With the
  chance 0.2 (her ramp is over, he is 40, before his decline at 45) she conceives in month
  264 and bears sim 13 (a son, one child) in month 274; her wish is 1, so no series
  follows. After the separation she is single again: the rounds of months 364 to 389 draw
  her search (at 26.7 to 28.6, about 0.011 each), which always says no. Nobody else ever
  searches; no man under 16 is ever in a list.
- The golden save is taken on day 100 of year 3: nobody has died or emigrated, sims 7 to
  10 are born; sim 1 is not pregnant (births 2) and her series waits for month 54, twelve
  months after the birth in month 42; sim 5 is not pregnant (births 2) and her series waits
  for month 53, twelve months after the birth in month 41; both separations wait in the
  queue, the round series waits for month 43 and the emigration for the last second of
  year 3.
- Households (ADR-0018 section 4, design note of the households; the life course data of
  format 2 with Dario's table of leaving home, tag LEAVE_HOME 22): after the founders and
  the couples `Start` writes four `core:household_formed` (entries 10 to 13, causes the
  input and the rule `core:household`): household 1 for sims 1 and 2, 2 for sims 5 and 6
  (the couples), 3 for sim 4 (20 years, alone); then sim 3, one second old, by the orphan
  rule (households, part 2): founders are unrelated, so a foster household; the women of
  the couples of 18 or more, sims 1 and 5, are ranked by FOSTER_RANK (23; subject his
  lineage key, month 0) and sim 5 comes first: one `core:moved` (entry 13) into household
  2, his carer sim 5, his social parents sims 5 and 6 (in this order), not moved out, his
  `leave_home` planned by his creation entry for the age 21 (one second before the end of
  year 20). Every child moves into its
  mother's household right after its `core:birth` (`core:moved`, causes the plan of the
  birth and the rule), its carer the mother, its `leave_home` planned after its deaths;
  the 18th birthdays end the carers. Household 2 emigrates in year 5 with sim 3 and sims
  7, 9 and 11 (their `leave_home` events go with them); sim 3 keeps his social parents
  sims 5 and 6 in the archive. Sim 8 leaves home at 18 (year 19.3) into household 4; the
  round of month 260 pairs her with sim 4, so they found household 5 (she moves, then he)
  and households 4 and 3 end. Sim 13 is born into household 5. Sims 10 and 12 leave at 22
  and 21 (years 25.2 and 27.5, households 6 and 7). Sim 1's death in year 27 leaves
  household 1 to sim 2 and sim 12 (she cares for nobody then: sim 12 is 20). The separation
  of partnership 3 (year 27.94) moves sim 4 into household 8; sim 13 (dependent, his carer
  his mother) stays. The golden save holds households 1 to 3 (sim 3 in household 2 with his
  foster parents) and the `leave_home` events of sims 3 and 7 to 10.

The fork (ADR-0001 section 6): the same run up to the last second of year 4, then the
input `ForkOverride` with the stamp T_5 - 1 s. It acts at T_5 before every event of
that second and writes one entry `core:param_changed` with the input (sequence number
2) as its cause. The switch is on from then on: the living sims with a lethal affliction
(sims 1 and 3) are exposed since T_5, and that entry plans their deaths of
`core:disease_ar` from their whole budgets: sim 1 (at 35 years, past the change at 18)
after about 42.9 years of exposure, at about 77.9 years of age; sim 3 about 87.0 years
after the start. Sim 3 emigrates with household 2 at the end of year 5, which cancels his;
sim 1 dies of senescence first, in year 27 as in the run, which cancels hers. The
emigration is the same: the switch changes neither the living nor a draw. The fertility is
the same: the switch changes no chance, and the market reads no disease: sim 8 is paired
with sim 4 in month 260 as in the run (sim 8 has one dummy allele, sim 4 none, so sim 13
carries at most one and has no affliction). Sims 4 to 6 and the children of sims 5 and 6
have no affliction, and partnership 3 separates in the same second as in the run. The
override sets the switch on, so in its context, right after its `core:param_changed`, it
writes the onsets of path 3 (ADR-0017 section 3, addendum, "Drei Wege" (3); core 43): sims
1 and 3 are living at T_5 and reached the onset 0 of `core:disease_ar` at their birth (`B =
born ≤ E = 0`), and their creation by `Start` saw the switch off, so nothing reported it
before: one entry `core:disease_onset` each, by SimId (sim 1 since year −30, sim 3 since
year −1, both at 0 years), caused by the input. So the fork has three entries more than the
run (the override and the two onsets), every later entry is three further on, and its year
hashes differ from year 5 on (the switch and the exposures are state, the planned deaths
are in the queue). After the fork point the fork runs every event kind (birthdays, sim 1's
attempts, her conception in month 74 and birth in month 84, the year ends, the
emigration in every year and its departures in year 5, the separation of partnership 3
in year 27.9, sim 1's death in year 27, the rounds of the market), and the fork's log has
every log kind of payloads 0 to 12 (the run's all but `core:param_changed`) and since core
43 `core:disease_onset` too; both are checked here (the coverage of ADR-0003 6.3).

`story_files()` gives the five files of the story on map 3 (dummy.json, dummy.save,
dummy_fork.json, dummy_pedigree.json, dummy_fork_pedigree.json) in the form of the goldens:
the text of `pedigree.json` of the run and of the fork after thirty years (clock 943487999),
compact as serde_json writes it, followed by one newline, as `simagent export` writes it. It
writes nothing. Prints the story's values, the text of both pedigrees and the answers of the
kinship query on the run's save.
Usage: .venv/bin/python tools/dummy_story.py   (or tools/golden_reference.py --story)
"""

import json
import math
import sys
from fractions import Fraction
from pathlib import Path

import xxhash

from postcard_reference import DUMMY, FEMALE, MALE, WILD, genotype, seq, signed, string, variant, varint

from world_reference import (
    CORE_VERSION,
    DEATH_KIND,
    DISEASE_ONSET_ENTRY,
    EMIGRATION_KIND,
    ENDED_BY_DEATH,
    ENDED_BY_EMIGRATION,
    ENDED_BY_SEPARATION,
    MARKET_ROUND_KIND,
    MAP3,
    MONTH,
    YEAR,
    World,
    budget_of,
    check_emigration,
    check_households,
    check_market,
    check_separations,
    check_story_kept,
    copies_at,
    death_key,
    json_number,
)
from rng_reference import chacha_key, child_key, founder_key, rank, str_key, unit, unit_open, unit_open0, words

ROOT = Path(__file__).resolve().parent.parent
# The data of the story: the map 3 fixture (design note of the switch to map 4, section 4).
DATA = MAP3
DATA_HASH = DATA.hash
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
# The soft capacity of the scenario (design note of the emigration, section 1).
CAPACITY = 9
SENESCENCE, ACCIDENT = "core:senescence", "core:accident"

assert BORN == [-943_488_000, -794_880_000, -1, -628_992_000, -786_240_000, -786_240_000]


def scenario_matches() -> bool:
    """Whether tests/scenarios/dummy.json is the scenario of this reference: format 5, the
    seed, the tag, the switch off, no nutrition, the founders (born, sex, the alleles that
    are not the reference), the couples in order, the capacity and no pool, until the last
    second of year 29."""
    scenario = json.loads((ROOT / "tests" / "scenarios" / "dummy.json").read_text())
    start = scenario["start"]

    def alleles(genes) -> list:
        return [] if genes == (WILD, WILD) else [{"locus": "core:disease_ar", "copies": list(genes)}]

    return (
        scenario["format_version"] == 5
        and list(start) == ["world_seed", "founder_tag", "world_params", "founders", "couples", "capacity", "pool"]
        and start["capacity"] == CAPACITY
        and start["pool"] is None
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
(CYSTIC,) = [disease for disease in DATA.diseases if disease.code == "core:disease_ar"]
AR_CODE = "core:disease_ar"
AR_KEY = str_key(AR_CODE)
FORK_AGE = (FORK_YEAR * YEAR - BORN[0]) / 31_449_600.0
assert FORK_AGE == 35.0 and FORK_AGE >= CYSTIC.hazard[1]
HER_AR_AGE = FORK_AGE + budget_of(SEED, founder_key(TAG, 0), "core:disease_ar") / CYSTIC.hazard[2]
HER_AR_DEATH = FORK_YEAR * YEAR + math.ceil((HER_AR_AGE - FORK_AGE) * 31_449_600.0)
assert 47 * YEAR < HER_AR_DEATH < 48 * YEAR


# Her fertility by hand from the genetics specification 5.6 and 11.3 with Python's math,
# straight from the draws (tags 3 CONCEPTION_ATTEMPT and 16 FERTILITY_WISH, 11.7), not from
# the model. The wish shape is K11.3d, the tilt and the adult rate those of the data.
WISH_SHAPE, WISH_TILT, ADULT_RATE = [0.10, 0.15, 0.40, 0.25, 0.10], 0.311, 0.20
assert (DATA.wish_shape, DATA.wish_tilt, DATA.adult_rate) == (WISH_SHAPE, WISH_TILT, ADULT_RATE)
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
assert DATA.twins == TWINS_RATES
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
assert DATA.separation_rate == SEPARATION_RATE
SEPARATION_BUDGET_VALUE = 0.0 - math.log(unit_open0(words(chacha_key(SEED, 18, HER_KEY, 0, HIS_KEY))[0]))
SEPARATION_DUE = max(math.ceil(SEPARATION_BUDGET_VALUE / SEPARATION_RATE * 31_449_600.0), 1)
assert 49 * YEAR < SEPARATION_DUE < 50 * YEAR
# The same for partnership 2 (sim 5's `k` 0): due after the fork point and before the end.
SECOND_SEPARATION_BUDGET = 0.0 - math.log(unit_open0(words(chacha_key(SEED, 18, SECOND_KEY, 0, SECOND_HIS_KEY))[0]))
SECOND_SEPARATION_DUE = max(math.ceil(SECOND_SEPARATION_BUDGET / SEPARATION_RATE * 31_449_600.0), 1)
assert FORK_YEAR * YEAR < SECOND_SEPARATION_DUE <= YEARS * YEAR - 1
assert 29 * YEAR < SECOND_SEPARATION_DUE < DUE[(5, SENESCENCE)] and SECOND_SEPARATION_DUE < DUE[(6, SENESCENCE)]


# The emigration by hand (design note of the emigration, section 2) with Python's float
# division, straight from the draws (tag 24 EMIGRATION), not from the model. Nobody dies before
# year 27, so at the end of year `y` (the second `(y + 1) · YEAR − 1`, after the births of
# that second, which fall on month starts, never on it) the living are the six founders and
# the children born in a month before `13 · (y + 1)`, as long as nobody has emigrated. The
# households then: 1 (sims 1 and 2 with their children), 2 (sims 5 and 6, their foster son
# sim 3 and their sons) and 3 (sim 4); their members of 18 or more are the founders but sim 3
# (one second old at the start). Each draws `u < e` at (her or his smallest key of those,
# the year, 0), `e = P / 9 − 1` held at 0.5, only in a year with `P > 9`.
EMIGRATION_TAG = 24
HOUSEHOLD_ADULTS = {1: [0, 1], 2: [4, 5], 3: [3]}


def hand_emigration() -> list:
    """(year, P, e, {household: (subject key, u, leaves)}) for every year with `P > 9` up to
    and including the first in which a household leaves."""
    rows = []
    for year in range(YEARS):
        last = (year + 1) * YEAR - 1
        living = 6 + sum(1 for _, birth in HAND_CONCEPTIONS + SECOND_CONCEPTIONS if birth * MONTH <= last)
        if living <= CAPACITY:
            continue
        e = min(living / CAPACITY - 1.0, 0.5)
        draws = {}
        for household, founders in HOUSEHOLD_ADULTS.items():
            subject = min(founder_key(TAG, index) for index in founders)
            u = unit(words(chacha_key(SEED, EMIGRATION_TAG, subject, year, 0))[0])
            draws[household] = (subject, u, u < e)
        rows.append((year, living, e, draws))
        if any(leaves for _, _, leaves in draws.values()):
            return rows
    return rows


HAND_EMIGRATION = hand_emigration()
assert [(year, living) for year, living, _, _ in HAND_EMIGRATION] == [(3, 10), (4, 10), (5, 11)]
assert [household for household, (_, _, leaves) in HAND_EMIGRATION[-1][3].items() if leaves] == [2]
EMIGRATION_YEAR = HAND_EMIGRATION[-1][0]
EMIGRATION_AT = (EMIGRATION_YEAR + 1) * YEAR - 1
assert FORK_YEAR * YEAR < EMIGRATION_AT and SAVE_AT < 4 * YEAR - 1
# Household 2 then: sims 5 and 6, sim 3 and their sons 7, 9 and 11 (born in months 12, 41
# and 73, before the end of year 5); afterwards at most seven live (sims 1, 2, 4, 8, 10, 12
# and later sim 13, less sim 1 in year 27), never more than 9.
EMIGRANTS = [3, 5, 6, 7, 9, 11]
assert all(month * MONTH < EMIGRATION_AT for _, month in SECOND_CONCEPTIONS)


# The market by hand (genetics specification 11.2; design note of the market, section 3)
# with Python's math, straight from the draws (tags 19 MARKET_SEARCH, 5 PAIRING_RANK, 21
# PAIRING_NOISE), not from the model. Sim 1's daughter (sim 8, born in month 17, her key
# that of the first child of sims 1 and 2) is single and in the market age from month 225
# (her 16th birthday) on; in a round `n` she searches if `u < p` with `p = c · e^(−z²/2)`,
# `z = (a − m) / 7`, below 1, times `ρ` before 18. The first `n` where she does: her
# candidates are the single men of 16 or more then, by their rank (sim 4 and her brother sim
# 10; sims 3, 7 and 9 emigrated with household 2 in year 5, sim 12 is under 16, sim 2
# partnered); her brother (φ = 1/4 [W2.7b]) is skipped; `U = −0.02 · (Δa − 4.69)² + G`,
# `G = −ln(−ln(u))`.
MARKET_C, MARKET_M, MARKET_RHO, MARKET_GAP = 0.01179, 25.23, 0.0918, 4.69
assert (DATA.market_height, DATA.market_center, DATA.market_minor) == (MARKET_C, MARKET_M, MARKET_RHO)
assert DATA.pairing_gap == MARKET_GAP and DATA.pairing_weight_age == 0.02 and DATA.market_width == 7.0
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
# The men by SimId and their keys and births (sim 10 of sim 1; sims 3, 7 and 9 emigrated).
MEN = {
    4: (founder_key(TAG, 3), BORN[3]),
    10: (child_key(HER_KEY, HIS_KEY, 1, 0), HAND_CONCEPTIONS[1][1] * MONTH),
}
assert all(DAUGHTER_SEARCH * MONTH - born >= 16 * YEAR for _, born in MEN.values())
assert DAUGHTER_SEARCH * MONTH - HAND_CONCEPTIONS[2][1] * MONTH < 16 * YEAR
assert EMIGRATION_AT < DAUGHTER_SEARCH * MONTH
DAUGHTER_RANKED = sorted(MEN, key=lambda man: (rank(SEED, 5, DAUGHTER_KEY, DAUGHTER_SEARCH, MEN[man][0]), MEN[man][0]))
DAUGHTER_CANDIDATES = [man for man in DAUGHTER_RANKED if man != 10]
assert DAUGHTER_RANKED == [4, 10]


def hand_utility(man: int) -> float:
    key, born = MEN[man]
    gap = (DAUGHTER_BORN - born) / 31_449_600.0
    g = -math.log(-math.log(unit_open(words(chacha_key(SEED, 21, DAUGHTER_KEY, DAUGHTER_SEARCH, key))[0])))
    return -0.02 * (gap - MARKET_GAP) ** 2 + g


DAUGHTER_UTILITIES = {man: hand_utility(man) for man in DAUGHTER_CANDIDATES}
DAUGHTER_PARTNER = max(DAUGHTER_CANDIDATES, key=DAUGHTER_UTILITIES.get)
assert DAUGHTER_PARTNER == 4
# Her conceptions with him by hand: at 18.7 her ramp is long over (her menarche is at most
# `θ + 3` = 15.3) and her decline begins at 30; he is 40, so his ramp is over and his
# decline begins only at 45: chance 0.2; her series from the month after the pairing, the
# pause of 12 months, her wish.
DAUGHTER_WISH = hand_wish(unit(words(chacha_key(SEED, 16, DAUGHTER_KEY, 0, 0))[0]))
assert (YEARS * YEAR - BORN[3]) / 31_449_600.0 < 51.0 and (DAUGHTER_SEARCH * MONTH - BORN[3]) / 31_449_600.0 + 1.0 < 45.0
DAUGHTER_CONCEPTIONS = hand_conceptions(DAUGHTER_KEY, MEN[4][0], lambda month: ADULT_RATE, DAUGHTER_WISH, DAUGHTER_SEARCH + 1)
assert all((month * MONTH - DAUGHTER_BORN) / 31_449_600.0 < 30.0 for month, _ in DAUGHTER_CONCEPTIONS)
assert DAUGHTER_CONCEPTIONS == [(264, 274)]
DAUGHTER_MULTIPLE = unit(words(chacha_key(SEED, 17, child_key(DAUGHTER_KEY, MEN[4][0], 0, 0), 0, 0))[0])
assert DAUGHTER_MULTIPLE >= TWINS_RATES[0] + TWINS_RATES[1]
# Partnership 3 separates by hand (her `k` 0, his key; design note of the separation,
# sections 1 and 3), before the end and after the fork point; sim 1 dies before it (27.17
# against 27.94). Then sim 8 is single again: the rounds of months 364 to 389 draw her
# search, which never says yes; nobody else is a single woman of 16 or more (sim 5
# emigrated).
THIRD_SEPARATION_BUDGET = 0.0 - math.log(unit_open0(words(chacha_key(SEED, 18, DAUGHTER_KEY, 0, MEN[4][0]))[0]))
THIRD_SEPARATION_DUE = DAUGHTER_SEARCH * MONTH + max(math.ceil(THIRD_SEPARATION_BUDGET / SEPARATION_RATE * 31_449_600.0), 1)
assert FORK_YEAR * YEAR < THIRD_SEPARATION_DUE < YEARS * YEAR - 1 and THIRD_SEPARATION_DUE // MONTH == 363
assert DUE[(1, SENESCENCE)] < THIRD_SEPARATION_DUE
assert not any(searches(DAUGHTER_KEY, DAUGHTER_BORN, n) for n in range(364, YEARS * 13))


def dummy_world(capacity=CAPACITY) -> World:
    return World(SEED, TAG, False, FOUNDERS, COUPLES, data=DATA, capacity=capacity)


def ar_deaths(world: World) -> list:
    """The planned deaths of `core:disease_ar` of `world`: (sim, time, plan) by sim."""
    return sorted(
        (event.subject[1], event.time, event.plan)
        for event in world.queue
        if event.kind == DEATH_KIND and death_key(event.payload) == AR_KEY
    )


# The deaths of `core:disease_ar` the override of the fork plans at T_5 (sims 1 and 3).
FORK_PLANNED_AR_DEATHS = []


def run(fork: bool, capacity=CAPACITY) -> World:
    world = dummy_world(capacity)
    if fork:
        world.advance(FORK_YEAR * YEAR - 1)
        before = len(world.log)
        world.override(True)
        # The override is the next entry; EventIds count from 1. It plans the deaths of the
        # disease of the living with it, sim 1's as by hand. Right after it come the onsets
        # of path 3 of sims 1 and 3 if living (both are with the capacity 9 and without one;
        # a small capacity of the scan may have sent sim 3 away), caused by the input
        # (sequence number 2): `core:disease_ar` since their birth, sim 1 in year −30, sim 3
        # in year −1, at 0 years.
        entry = before + 1
        cause = variant(2, varint(2))
        living = [(sim, year) for sim, year in [(1, -30), (3, -1)] if sim in world.sims]
        if capacity in (CAPACITY, None):
            assert [sim for sim, _ in living] == [1, 3]
        assert world.log[before + 1:] == [
            varint(entry + 1 + i) + signed(FORK_YEAR * YEAR) + seq([variant(0, varint(sim))]) + seq([cause])
            + variant(DISEASE_ONSET_ENTRY, string(AR_CODE), signed(year), signed(0))
            for i, (sim, year) in enumerate(living)
        ], [line.hex() for line in world.log[before:]]
        if capacity in (CAPACITY, None):
            assert ar_deaths(world)[0] == (1, HER_AR_DEATH, (entry, FORK_YEAR * YEAR))
            assert all(plan == (entry, FORK_YEAR * YEAR) for *_, plan in ar_deaths(world))
        if capacity == CAPACITY:
            FORK_PLANNED_AR_DEATHS[:] = ar_deaths(world)
    world.advance(YEARS * YEAR - 1)
    return world


# SimIds by birth: sim 5's children (months 12, 41, 73) and sim 1's (17, 42, 84) alternate;
# sim 8's son (month 274) is sim 13.
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
# The emigration is planned third, right after the round series (design note of the
# emigration, section 2, as the coordinator fixed it).
assert [(event.insertion, event.kind, event.time, event.period) for event in FIRST.queue if event.subject == (0, 0)] == [
    (1, 1, YEAR - 1, YEAR), (2, MARKET_ROUND_KIND, MONTH, MONTH), (3, EMIGRATION_KIND, YEAR - 1, YEAR)
]
check_market(FIRST)
check_households(FIRST)
check_emigration(FIRST)
PARENT = run(fork=False)
FORK = run(fork=True)
# The golden scenario without a capacity is the golden scenario before the emigration (its
# log byte for byte, its year hashes, the story the households keep; only its saves change),
# and the run with the capacity is that run up to the end of year 5 but for the planned
# event: the same log up to the emigration.
NO_CAPACITY = run(fork=False, capacity=None)
NO_CAPACITY_FORK = run(fork=True, capacity=None)
assert NO_CAPACITY.year_hashes != PARENT.year_hashes and not NO_CAPACITY.emigrations
check_story_kept(NO_CAPACITY)
# The log hash of the golden of core 33 (tests/golden/dummy.json before the emigration): a
# world without a capacity runs as before. Its fork (0x996f_e07f_c38e_7600 in core 33) has
# since core 43 the two onsets of path 3 after the override, and every later entry two
# further on; up to the fork point it is the run's log.
assert NO_CAPACITY.log_hash() == 0x09C1_7754_9EA2_83A5
_point = next(i for i, line in enumerate(NO_CAPACITY.log) if line != NO_CAPACITY_FORK.log[i])
assert len(NO_CAPACITY_FORK.log) == len(NO_CAPACITY.log) + 3 and NO_CAPACITY_FORK.log[:_point] == NO_CAPACITY.log[:_point]
assert [line[0] for line in NO_CAPACITY_FORK.log[_point:_point + 3]] == [_point + 1, _point + 2, _point + 3]
_cut = next(i for i, line in enumerate(PARENT.log) if line != NO_CAPACITY.log[i])
assert all(t == EMIGRATION_AT for t, kind, *_ in PARENT.home_log if kind == "left by emigration")
assert _cut + 1 == min(entry for t, kind, _, _, _, entry in PARENT.home_log if kind == "left by emigration")
assert len(PARENT.year_hashes) == YEARS and len(FORK.year_hashes) == YEARS
assert FORK.year_hashes[:FORK_YEAR] == PARENT.year_hashes[:FORK_YEAR]
assert all(
    ours != theirs
    for ours, theirs in zip(FORK.year_hashes[FORK_YEAR:], PARENT.year_hashes[FORK_YEAR:])
)
assert len(FORK.log) == len(PARENT.log) + 3, "the override and the onsets of sims 1 and 3; the same death"
# Sim 1 dies of senescence in year 27, in the run and in the fork: the only death. Household
# 2 emigrates at the end of year 5 in both.
HER_DEATH = DUE[(1, SENESCENCE)]
for world in (PARENT, FORK):
    assert sorted(world.archive) == sorted([1] + EMIGRANTS) and world.archive[1][1:3] == (HER_DEATH, str_key(SENESCENCE))
    assert all(world.archive[sim][1:3] == (EMIGRATION_AT, None) and world.archive[sim][11] == 2 for sim in EMIGRANTS)
    assert sorted(world.sims) == [2, 4, 8, 10, 12, 13]
    # The emigration as by hand: the years with `P > 9`, `P`, `e` and every draw.
    assert [
        (r["year"], r["P"], r["e"], {d["household"]: (d["subject"], d["u"], d["draw"]) for d in r["draws"]}) for r in world.emigrations if r["e"] is not None
    ] == HAND_EMIGRATION
    assert all(r["P"] <= CAPACITY and not r["draws"] for r in world.emigrations if r["year"] > EMIGRATION_YEAR)
    assert [(r["year"], [(household, [sim for sim, _ in sims]) for household, sims in r["left"]]) for r in world.emigrations if r["left"]] == [
        (EMIGRATION_YEAR, [(2, EMIGRANTS)])
    ]
    assert world.households[2] == [0, EMIGRATION_AT]
    # Her death comes before the separation (year 49) and cancels it; the emigration ends
    # partnership 2 before its separation (year 29.9) and cancels it. The market pairs sim 8
    # with sim 4 in month 260 (partnership 3), which separates in year 27.94.
    assert world.partnerships == {
        1: [1, 2, 0, (HER_DEATH, ENDED_BY_DEATH)],
        2: [5, 6, 0, (EMIGRATION_AT, ENDED_BY_EMIGRATION)],
        3: [DAUGHTER, DAUGHTER_PARTNER, DAUGHTER_SEARCH * MONTH, (THIRD_SEPARATION_DUE, ENDED_BY_SEPARATION)],
    }
    assert HER_DEATH < SEPARATION_DUE and world.pending_separation(1) is None and world.pending_separation(2) is None
    assert [(t, number) for t, number, *_ in world.separations] == [(THIRD_SEPARATION_DUE, 3)]
    assert all(world.sims[sim][8] == 0 for sim in world.sims if sim != 8)
    assert world.archive[5][10] == 1 and world.sims[8][8] == 1
    check_separations(world)
    check_market(world)
    check_households(world)
    check_emigration(world)
    # The rounds (cadence, test rule 6): every month, 13 a year; sim 8's choice as by hand.
    assert [kind for _, kind in world.ran].count(MARKET_ROUND_KIND) == YEARS * 13 - 1
    # The emigration runs in the last second of every year.
    assert [t for t, kind in world.ran if kind == EMIGRATION_KIND] == [(year + 1) * YEAR - 1 for year in range(YEARS)]
    choices = [(record["n"], choice) for record in world.rounds for choice in record["choices"]]
    assert [(n, choice["woman"]) for n, choice in choices] == [(DAUGHTER_SEARCH, DAUGHTER)]
    ((_, choice),) = choices
    assert choice["walked"] == [(man, Fraction(1, 4) if man == 10 else Fraction(0), man == 10) for man in DAUGHTER_RANKED]
    assert [man for man, *_ in choice["candidates"]] == DAUGHTER_CANDIDATES and choice["outcome"] == "paired"
    assert all(abs(u - DAUGHTER_UTILITIES[man]) <= 1e-12 for man, _, _, _, u in choice["candidates"])
    assert [record["n"] for record in world.rounds] == list(range(225, DAUGHTER_SEARCH + 1)) + list(range(364, YEARS * 13))
assert [PARENT.born_of(sim) for sim in CHILDREN] == [month * MONTH for month in CHILD_MONTHS]
assert [PARENT.born_of(sim) for sim in SECOND_CHILDREN] == [month * MONTH for month in SECOND_CHILD_MONTHS]
assert [PARENT.sims[sim][7] for sim in CHILDREN] == [(1, 2, number) for number in range(3)]
assert [PARENT.archive[sim][9] for sim in SECOND_CHILDREN] == [(5, 6, number) for number in range(3)]
assert [PARENT.sims[sim][0] for sim in THIRD_CHILDREN] == [birth * MONTH for _, birth in DAUGHTER_CONCEPTIONS]
assert [PARENT.sims[sim][7] for sim in THIRD_CHILDREN] == [(DAUGHTER, DAUGHTER_PARTNER, 0)]
assert [PARENT.sims[sim][2] for sim in CHILDREN] == [FEMALE, MALE, MALE]
assert [PARENT.sex_of(sim) for sim in SECOND_CHILDREN] == [MALE, MALE, MALE]
# The conceptions, births and wishes as by hand, the chance of every attempt too; sim 1's
# attempts run from month 1 to her third conception, with the pauses after the births,
# sim 5's likewise.
assert not PARENT.pregnancies and PARENT.archive[1][7:9] == (len(CHILDREN), len(CHILDREN))
assert PARENT.archive[5][7:9] == (len(SECOND_CHILDREN), len(SECOND_CHILDREN))
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
# Sims 1 and 3 hold core:disease_ar (both in the archive at the end: she died, he emigrated);
# nobody living does.
LETHAL = [sim for sim in sorted(set(PARENT.sims) | set(PARENT.archive)) if PARENT.creation_of(sim)[6]]
assert LETHAL == [1, 3] and not [sim for sim in PARENT.sims if PARENT.sims[sim][4]]
assert [a.triple() for a in PARENT.archive[3][6]] == [(AR_KEY, 0.0, (0.0, None))]
assert all(copies_at(PARENT.sims[sim][3], CYSTIC.locus) == [1, 0] for sim in CHILDREN)
# In the fork sims 1 and 3 are exposed since T_5, and the override planned their deaths
# of the disease; his went with his emigration, hers with her death. The override is the
# first entry the fork writes differently.
FORK_OVERRIDE_ENTRY = next(i for i, (ours, theirs) in enumerate(zip(FORK.log, PARENT.log)) if ours != theirs) + 1
assert [a.triple() for a in FORK.archive[1][6]] == [(AR_KEY, 0.0, (0.0, FORK_YEAR * YEAR))]
assert [a.triple() for a in FORK.archive[3][6]] == [(AR_KEY, 0.0, (0.0, FORK_YEAR * YEAR))]
FORK_AR_DEATHS = FORK_PLANNED_AR_DEATHS
assert [sim for sim, *_ in FORK_AR_DEATHS] == LETHAL and not ar_deaths(FORK)
assert all(plan == (FORK_OVERRIDE_ENTRY, FORK_YEAR * YEAR) for _, _, plan in FORK_AR_DEATHS)
assert FORK_AR_DEATHS[0][1] == HER_AR_DEATH > HER_DEATH and FORK_AR_DEATHS[1][1] > EMIGRATION_AT
# The rounds of the fork are those of the run (the switch changes no draw of the market);
# only the EventId of the pairing's entry is three more (the override and the two onsets).
assert [
    (r["n"], r["men"], r["considered"], [{k: v for k, v in c.items() if k != "entry"} for c in r["choices"]]) for r in FORK.rounds
] == [(r["n"], r["men"], r["considered"], [{k: v for k, v in c.items() if k != "entry"} for c in r["choices"]]) for r in PARENT.rounds]

# Coverage (ADR-0003 6.3): after the fork point the fork runs every event kind
# (tests/golden/event_codes.txt: year_end 1 to emigration 9), and the fork writes every
# log kind (tests/golden/log_codes.txt: payloads 0 to 12; the run all but
# `core:param_changed`), and since core 43 also `core:disease_onset` (14), by path 3.
EVENT_KINDS, LOG_KINDS = set(range(1, 10)), set(range(13))
assert {kind for t, kind in FORK.ran if t >= FORK_YEAR * YEAR} == EVENT_KINDS
assert set(FORK.entry_kinds) == LOG_KINDS | {DISEASE_ONSET_ENTRY} and set(PARENT.entry_kinds) == LOG_KINDS - {3}

# The households of the story above (design note of the households; part 2: sim 3 is
# fostered at the start by sim 5, the one FOSTER_RANK ranks first of sims 1 and 5).
START_HOMES = [(row[1], row[2], row[3], row[4], row[5]) for row in PARENT.home_log if row[0] == 0]
assert START_HOMES == [
    ("formed", 1, [1, 2], "start couple", 10),
    ("formed", 2, [5, 6], "start couple", 11),
    ("formed", 3, [4], "start adult", 12),
    ("moved", 2, [3], "orphan rule (start): foster", 13),
]
(FOSTER,) = PARENT.placements
assert FOSTER["stage"] == "foster" and FOSTER["women"] == [1, 5] and FOSTER["carer"] == 5 and FOSTER["added"] == {3: [5, 6]}
LEAVE_AGES = {sim: (PARENT.leave_home_second(sim) - PARENT.born_of(sim)) // YEAR for sim in [3] + CHILDREN + SECOND_CHILDREN}
assert LEAVE_AGES == {3: 21, 7: 19, 8: 18, 9: 21, 10: 22, 11: 21, 12: 21}
# The emigrants never leave home (sims 3, 7, 9 and 11); sims 8, 10 and 12 do.
LEFT = [(t, sims[0]) for t, kind, _, sims, why, _ in PARENT.home_log if kind == "formed" and why == "leave_home"]
assert [sim for _, sim in LEFT] == [8, 10, 12] and LEFT[0][0] < DAUGHTER_SEARCH * MONTH
assert PARENT.households[5] == [DAUGHTER_SEARCH * MONTH, None] and PARENT.members(5) == [8, 13]
assert PARENT.households[3] == [0, DAUGHTER_SEARCH * MONTH] and PARENT.households[4][1] == DAUGHTER_SEARCH * MONTH
assert [row[1:5] for row in PARENT.separation_homes] == [(8, 4, [13], [])] and PARENT.sims[4][9] == 8
assert [row for row in PARENT.carer_log if row[4] not in ("birth", "18th birthday")] == [(0, 3, None, 5, "orphan rule: foster")]
assert PARENT.archive[3][13] == (5, 6) and FORK.archive[3][13] == (5, 6)
# Each emigrant's entry `core:emigrated` names household 2, by SimId, EventIds 91 to 96.
EMIGRATION_ENTRIES = [entry for t, kind, _, _, _, entry in PARENT.home_log if kind == "left by emigration"]
assert EMIGRATION_ENTRIES == list(range(91, 97))
# The fork has the households of the run; from the fork point on every entry is three later
# (the override and the two onsets of path 3).
assert FORK.home_log == [
    row[:5] + (row[5] if row[5] is None or row[0] < FORK_YEAR * YEAR else row[5] + 3,) for row in PARENT.home_log
]


def golden_save() -> bytes:
    """The save on day 100 of year 3: nobody pregnant, sim 1's series pending in month 54,
    sim 5's in month 53, both separations pending, the next round at the start of month 43
    (no single woman of 16 or more yet), the emigration at the last second of year 3 (it
    drew nothing so far: `P` was at most 8)."""
    world = dummy_world()
    world.advance(SAVE_AT)
    (emigration,) = [event for event in world.queue if event.kind == EMIGRATION_KIND]
    assert emigration.time == 4 * YEAR - 1 and not [r for r in world.emigrations if r["e"] is not None]
    check_emigration(world)
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
    """The text of a golden of the form of tests/golden/<scenario>.json, on the story's data."""
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


# The pedigree (step 10, design note of the pedigree with kinship and social parents; format 3
# with the emigration, design note of the emigration, section 4) of the run and of the fork
# after thirty years. Sim 3, fostered at the start, has his social parents sims 5 and 6 (in
# this order). In the run the switch of the hereditary diseases is off all the time, so sims
# 1 and 3 (core:disease_ar, onset 0) are not affected; in the fork it is on from T_5, so both
# are (sim 1 at her death in year 27, sim 3 at the second he emigrated, the end of year 5).
# The emigrants (sims 3, 5, 6, 7, 9, 11) have `died` and `cause_of_death` null, `emigrated`
# 188 697 599, household 2, carer null. The partnerships: 1 ended by death (sim 1's), 2 by
# emigration, 3 by separation; every `φ` 0 (founders, and sim 8 with the founder sim 4).
PEDIGREE = json.loads(PARENT.pedigree_json())
FORK_PEDIGREE = json.loads(FORK.pedigree_json())
# Format 4 since the arrival (design note of the arrival, section 8): `pool` null for every
# person of these runs.
assert PEDIGREE["format_version"] == 4 and list(PEDIGREE) == ["format_version", "persons", "partnerships", "households"]
assert all(person["pool"] is None and list(person)[-1] == "pool" for view in (PEDIGREE, FORK_PEDIGREE) for person in view["persons"])
assert list(PEDIGREE["persons"][0])[5:8] == ["died", "cause_of_death", "emigrated"]
assert [(p["sim"], p["died"], p["cause_of_death"], p["household"], p["carer"]) for p in PEDIGREE["persons"] if p["emigrated"] is not None] == [
    (sim, None, None, 2, None) for sim in EMIGRANTS
] and all(p["emigrated"] == EMIGRATION_AT for p in PEDIGREE["persons"] if p["emigrated"] is not None)
assert [person["sim"] for person in PEDIGREE["persons"]] == list(range(1, 14))
assert {person["sim"]: person["social_parents"] for person in PEDIGREE["persons"] if person["social_parents"]} == {3: [5, 6]}
for view, affected in ((PEDIGREE, False), (FORK_PEDIGREE, True)):
    assert [(p["sim"], p["diseases"]) for p in view["persons"] if p["diseases"]] == [
        (sim, [{"disease": "core:disease_ar", "onset": 0.0, "affected": affected}]) for sim in (1, 3)
    ]
    assert [(p["partnership"], p["ended_by"], p["kinship"]) for p in view["partnerships"]] == [
        (1, "death", 0.0), (2, "emigration", 0.0), (3, "separation", 0.0)
    ]
    assert len(view["households"]) == PARENT.next_household_id - 1
# The fork's pedigree differs from the run's only in `affected` of sims 1 and 3.
assert FORK.pedigree_json() == PARENT.pedigree_json().replace('"affected":false', '"affected":true')
assert (PEDIGREE["persons"][0]["died"], PEDIGREE["persons"][0]["cause_of_death"]) == (HER_DEATH, SENESCENCE)

# The kinship query of the CLI (`simagent kinship <run dir> <a> <b>`, design note section 4)
# on the save of the run after thirty years: (relation, a, b, φ exact); sim 14 was never
# given (the SimId counter), so `unknown_sim`.
KINSHIP_CASES = [
    ("parent_child", 1, 8, Fraction(1, 4)),
    ("full_siblings", 8, 10, Fraction(1, 4)),
    ("unrelated_founders", 1, 2, Fraction(0)),
    ("itself_founder", 3, 3, Fraction(1, 2)),
    ("itself_child", 13, 13, Fraction(1, 2)),
    ("grandparent", 2, 13, Fraction(1, 8)),
    ("avuncular", 10, 13, Fraction(1, 8)),
    ("partners", 8, 4, Fraction(0)),
    ("unknown", 14, 1, None),
]
assert PARENT.next_sim_id == 14
for _relation, _a, _b, _phi in KINSHIP_CASES:
    assert PARENT.kinship_of(_a, _b) == PARENT.kinship_of(_b, _a) == (None if _phi is None else float(_phi)), _relation


def coverage(world_run: World, world_fork: World) -> tuple:
    """(the event kinds the fork runs after the fork point, the log kinds of the fork, of the
    run)."""
    return (
        {kind for t, kind in world_fork.ran if t >= FORK_YEAR * YEAR},
        set(world_fork.entry_kinds),
        set(world_run.entry_kinds),
    )


def capacity_scan() -> list:
    """Per capacity 1 to 13 whether the golden scenario keeps every event and log kind (the
    coverage of ADR-0003 6.3, now with `emigration` and `core:emigrated`): the kinds it
    misses and its emigrations (year, households). 13 and more draw nothing (the largest
    population is 13)."""
    rows = []
    for k in range(1, 14):
        world_run, world_fork = run(False, k), run(True, k)
        events, fork_logs, run_logs = coverage(world_run, world_fork)
        left = [(r["year"], [household for household, _ in r["left"]]) for r in world_run.emigrations if r["left"]]
        rows.append((k, f"missing event kinds {sorted(EVENT_KINDS - events)} missing log kinds fork {sorted(LOG_KINDS - fork_logs)} run {sorted((LOG_KINDS - {3}) - run_logs)} emigrations (year, households) {left}"))
    return rows


def pedigree_rows() -> list:
    """The values of the pedigree export after thirty years (crates/sim-cli/tests/cli.rs):
    per sim its lineage key, sex, alleles that are not the reference (locus, copies
    maternal then paternal), birth, death (or "null emigrated" and the second it left, for
    an emigrant) and parents; and the partnerships."""
    rows = []
    for sim in sorted(set(PARENT.sims) | set(PARENT.archive)):
        if sim in PARENT.sims:
            born, key, sex, person, _, _, _, parents, *_ = PARENT.sims[sim]
            died = "null"
        else:
            born, left, cause, key, sex, person, _, _, _, parents, *_ = PARENT.archive[sim]
            died = f"null emigrated {left}" if cause is None else f"{left} of {cause:016x}"
        alleles = [(locus, copies_at(person, PARENT.data.loci[locus])) for locus in PARENT.data.loci]
        alleles = [(locus, copies) for locus, copies in alleles if any(copies)]
        sex_name = "female" if sex == FEMALE else "male"
        rows.append(f"pedigree_sim_{sim} key {key:016x} sex {sex_name} alleles {alleles} born {born} died {died} parents {parents}")
    for number, (woman, man, formed, ended) in sorted(PARENT.partnerships.items()):
        ended_text = "null" if ended is None else f"{ended[0]} by {('death', 'separation', 'emigration')[ended[1]]}"
        rows.append(f"pedigree_partnership_{number} woman {woman} man {man} formed {formed} ended {ended_text}")
    return rows


def story_files() -> dict:
    """The five files of the story on its data (map 3), as the goldens hold them."""
    return {
        "dummy.json": golden(PARENT).encode(),
        "dummy.save": golden_save(),
        "dummy_fork.json": golden(FORK).encode(),
        "dummy_pedigree.json": (PARENT.pedigree_json() + "\n").encode(),
        "dummy_fork_pedigree.json": (FORK.pedigree_json() + "\n").encode(),
    }


def main() -> None:
    text = golden(PARENT)
    fork_text = golden(FORK)
    print(f"story data tests/golden/map3 data_hash {DATA_HASH:016x} core_version {CORE_VERSION}")
    for name, content in story_files().items():
        print(f"story_file {name} bytes {len(content)} xxh3 {xxhash.xxh3_64_intdigest(content, seed=0):016x}")
    print(text, end="")
    print(f"save: {len(golden_save())} bytes")
    print(fork_text, end="")
    print(f"entries: {len(PARENT.log)} in the run, {len(FORK.log)} in the fork")
    print(f"founder deaths due (seconds after the start): {DUE}")
    children = {sim: PARENT.cause_due(sim, SENESCENCE) for sim in CHILDREN}
    print(f"children: born (months) {CHILD_MONTHS}, senescence due (seconds) {children}")
    second_children = {sim: NO_CAPACITY.cause_due(sim, SENESCENCE) for sim in SECOND_CHILDREN}
    print(f"children of sim 5: sims {SECOND_CHILDREN} born (months) {SECOND_CHILD_MONTHS}, senescence due (seconds) {second_children} (planned until they emigrate at {EMIGRATION_AT})")
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
        f" ({SECOND_SEPARATION_DUE / YEAR:.4f} years, month {SECOND_SEPARATION_DUE / MONTH:.4f}), cancelled by the emigration at {EMIGRATION_AT}"
    )
    print(f"fertility: children's wishes {[(sim, PARENT.wish_of(sim)) for sim in CHILDREN + SECOND_CHILDREN if PARENT.sex_of(sim) == FEMALE]}")
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
    print(f"market: sim 8 single again after the separation, rounds 364 to 389 chances {[hand_search_chance(n * MONTH - DAUGHTER_BORN) for n in (364, 389)]!r} (first, last), never searches")
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
    # Step 10: the text of `pedigree.json` (compact, as serde_json writes it, no newline) of
    # the run and of the fork after thirty years, and the kinship query on the run's save.
    for name, world in (("pedigree_json", PARENT), ("pedigree_fork_json", FORK)):
        text = world.pedigree_json()
        print(f"{name}_summary clock {world.closed} length {len(text.encode())} hash {xxhash.xxh3_64_intdigest(text.encode(), seed=0):016x}")
        print(f"{name} {text}")
    for relation, a, b, phi in KINSHIP_CASES:
        answer = "none (unknown_sim)" if phi is None else f"phi {phi} json {json_number(float(phi))}"
        print(f"pedigree_kinship_{relation} sims {a} {b} {answer}")
    # The emigration (design note of the emigration, step 11): the capacity, every year with
    # `P > 9` and its draws, the departure, its entries; and the scan of the capacities that
    # gives 9 as the smallest with the coverage of every kind the goldens covered before.
    print(f"emigration_capacity {CAPACITY} scenario format 5 tests/scenarios/dummy.json start.capacity {CAPACITY}")
    for year, living, e, draws in HAND_EMIGRATION:
        rows = {household: (f"{subject:016x}", u, leaves) for household, (subject, u, leaves) in draws.items()}
        print(f"emigration_year {year} t {(year + 1) * YEAR - 1} P {living} e {e!r} draws (household: subject key, u, leaves) {rows}")
    print(f"emigration_left household 2 at {EMIGRATION_AT} (year {EMIGRATION_AT / YEAR:.6f}) sims {EMIGRANTS} entries {EMIGRATION_ENTRIES} (in the fork {[entry + 3 for entry in EMIGRATION_ENTRIES]})")
    for line in PARENT.log[EMIGRATION_ENTRIES[0] - 1:EMIGRATION_ENTRIES[-1]]:
        print(f"emigration_entry {line.hex()}")
    print(f"emigration_population (year, P) {[(r['year'], r['P']) for r in PARENT.emigrations]}")
    for name, world in (("run", NO_CAPACITY), ("fork", NO_CAPACITY_FORK)):
        print(f"emigration_no_capacity {name} log_hash {world.log_hash():016x} entries {len(world.log)} year_hashes {[format(value, '016x') for value in world.year_hashes]}")
    for k, row in capacity_scan():
        print(f"emigration_capacity_scan K {k} {row}")
    # The measurement (step 11, design note of the measurement): the line `simagent measure
    # <run dir>` prints for the golden run and its fork after thirty years (clock 943487999).
    # Map 3 (the story's data) has no trait and no additive locus, so both lists are empty; only
    # the living are counted.
    for name, world in (("run", PARENT), ("fork", FORK)):
        measurement = world.measure()
        assert measurement is not None and measurement["traits"] == [] and measurement["loci"] == [] and measurement["living"] == len(world.sims)
        print(f"measure_json_{name} clock {world.closed} living {measurement['living']} {world.measure_json()}")


if __name__ == "__main__":
    main()

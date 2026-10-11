#!/usr/bin/env python3
"""The goldens of the scenario `tests/scenarios/dummy.json`, computed independently of
the Rust code (ADR-0003 6.3) by the model in tools/world_reference.py: year hashes, log
hash and the golden save, and the goldens of its fork at year 5 with the override
"hereditary diseases on".

The scenario (format 3, design note of the fertility in the world, section 1): seed 1,
founder tag `test:dummy`, the switch off, neither causes of death nor a fertility of its
own; it runs on the repository data (the CLI embeds them): senescence and accident with
the rates of the genetics specification 7.2 (ADR-0017 section 2), its six hereditary
diseases (ADR-0017 section 3), and the fertility of 5.6 and 11.3 (ADR-0017 section 4: the
curves, the adult rate 0.20, the wish, the pause of 12 months, ten months of pregnancy).
Three founders born 30 years (sim 1), 25 years and 100 days (sim 2) and one second (sim
3) before the start, thirty years (long enough for the first death, so that the goldens
cover every event and log kind). Sim 1 is female with the dummy allele twice at
`core:disease_ar`, sim 2 male with dummy and wild (from mother and father), sim 3 male
with dummy twice. Their lineage keys are those of the founders 0 to 2 of the tag (ADR-0001
section 4). Sims 1 and 2 are a couple from the start (partnership 1, entry
`core:partnered` after the founders).

What happens (a year has 364 days of 86 400 seconds, 13 months of 28 days):

- Every founder has its deaths of senescence and accident planned at the start, from its
  age then (B19): sim 1 dies of senescence after about 27.2 years (at about 57), sim 2
  would after about 69.8 years, sim 3 after about 87.2 years; their accidents lie
  thousands of years ahead. Sim 1's death in year 27 is the only one of the thirty years;
  it ends partnership 1.
- Sims 1 and 3 have `core:disease_ar` (recessive, penetrance 1, onset 0, two steps):
  each an affliction with nothing used, its exposure not running while the switch is
  off, so no death of it is planned. Sim 2 carries one copy and is healthy (no draw).
  No founder has an allele at another disease locus.
- Fertility: sim 1's wish is 3 (`u` of FERTILITY_WISH about 0.741, between the
  thresholds 0.517 and 0.829 of 2 and 3 children with the tilt 0.311). At 30 her ramp is long over (her menarche,
  about 12.8 years, is at most `θ + 3` = 15.3) and the decline of 5.6 acts: her factor is
  `(50 − a) / 20`; sim 2 (25 to 31 years old while she tries) has the factor 1. So the
  chance of the attempt in month `k` is `0.2 · (50 − a) / 20` with `a = 30 + k / 13`,
  0.199 in month 1. Her series begins in month 1 (the first month start after the
  partnership at 0); she conceives in month 7 (entries `core:conception` and two
  `core:meiosis`, chance 0.195) and has the child (sim 4, a daughter, entry `core:birth`)
  in month 17; after the pause of 12 months her series begins again in month 29, she
  conceives in month 32 (0.175) and bears sim 5 (a son) in month 42; then from month 54
  on, she conceives in month 74 (0.143) and bears sim 6 (a son) in month 84. With three
  births her wish is reached: no series follows (32 attempts in all). All three children
  got the wild allele from their father and carry `core:disease_ar` without an
  affliction; each has its deaths of senescence and accident planned from its birth.
  Nobody else is in a partnership, so nobody else tries.
- Twins (11.3, design note of the twins): each conception draws MULTIPLE_BIRTH at the key
  of its first child against the repository's rates 0.004 (identical) and 0.010
  (fraternal); the draws are about 0.088, 0.685 and 0.405, all at or above 0.014, so every
  conception gives one child and the story is that of one child per conception. Each
  conception entry names the draw as its third cause.
- The golden save is taken on day 100 of year 3: nobody has died, sims 4 and 5 are born,
  sim 1 is not pregnant (births 2) and her series waits for month 54, twelve months after
  the birth in month 42.

The fork (ADR-0001 section 6): the same run up to the last second of year 4, then the
input `ForkOverride` with the stamp T_5 - 1 s. It acts at T_5 before every event of
that second and writes one entry `core:param_changed` with the input (sequence number
2) as its cause. The switch is on from then on: the living sims with a lethal affliction
(sims 1 and 3) are exposed since T_5, and that entry plans their deaths of
`core:disease_ar` from their whole budgets: sim 1 (at 35 years, past the change at 18)
after about 42.9 years of exposure, at about 77.9 years of age; sim 3 about 87.0 years
after the start. Sim 1 dies of senescence first, in year 27 as in the run, which cancels
her death of the disease; sim 3's lies beyond the thirty years. Her fertility is the
same: the switch changes no chance. So the fork has one entry more than the run (the
override), and its year hashes differ from year 5 on (the switch and the exposures are
state, the planned deaths are in the queue).

Writes tests/golden/dummy.json, tests/golden/dummy.save and tests/golden/dummy_fork.json
(protected list, ADR-0000). Prints the pedigree values of the CLI export test as well.
Usage: .venv/bin/python tools/golden_reference.py [--check]
"""

import json
import math
import sys
from pathlib import Path

from postcard_reference import DUMMY, FEMALE, MALE, WILD, genotype

from world_reference import (
    CORE_VERSION,
    DATA_HASH,
    DEATH_KIND,
    MONTH,
    REPOSITORY,
    YEAR,
    World,
    budget_of,
    copies_at,
    death_key,
)
from rng_reference import chacha_key, child_key, founder_key, str_key, unit, words

ROOT = Path(__file__).resolve().parent.parent
DAY = 86_400
SEED = 1
TAG = "test:dummy"
BORN = [-30 * YEAR, -25 * YEAR - 100 * DAY, -1]
SEX = [FEMALE, MALE, MALE]
GENES = [genotype(DUMMY, DUMMY), genotype(DUMMY, WILD), genotype(DUMMY, DUMMY)]
FOUNDERS = list(zip(BORN, SEX, GENES))
COUPLES = [(0, 1)]
YEARS = 30
SAVE_AT = 3 * YEAR + 100 * DAY
FORK_YEAR = 5
SENESCENCE, ACCIDENT = "core:senescence", "core:accident"

assert BORN == [-943_488_000, -794_880_000, -1]

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
DUE = {(sim, code): due_by_hand(sim - 1, code) for sim in (1, 2, 3) for code in (SENESCENCE, ACCIDENT)}
assert 27 * YEAR < DUE[(1, SENESCENCE)] < 28 * YEAR
assert 69 * YEAR < DUE[(2, SENESCENCE)] < 70 * YEAR < 87 * YEAR < DUE[(3, SENESCENCE)] < 88 * YEAR
assert all(DUE[(sim, ACCIDENT)] > 1000 * YEAR for sim in (1, 2, 3))

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


def hand_chance(month: int) -> float:
    """The chance of her attempt in `month`: she is `a = 30 + month / 13` years old, past
    30, so her factor is the decline `(50 − a) / (50 − 30)` (her ramp ended three years
    after her menarche, at most `θ + 3` = 15.3); his factor is 1 (younger than 45, his ramp
    ended at `θ − 0.7 + 2` = 15.32)."""
    a = 30.0 + month / 13.0
    assert 30.0 < a < 50.0 and -BORN[1] / YEAR + month / 13.0 < 45.0
    return ADULT_RATE * ((50.0 - a) / 20.0)


def hand_conceptions() -> list:
    """(month of the conception, month of the birth) of sim 1 (design note sections 4 to
    6): her series begins in month 1, an attempt per month succeeds if the unit of its draw
    (subject her key, occasion the month, detail his key) is below `hand_chance`; the birth
    ten months later; with fewer births than her wish the next series twelve months after
    the birth (the pause); up to the end of the thirty years."""
    found, month = [], 1
    while month < YEARS * 13 and len(found) < HER_WISH:
        if unit(words(chacha_key(SEED, 3, HER_KEY, month, HIS_KEY))[0]) < hand_chance(month):
            found.append((month, month + 10))
            month += 10 + 12
        else:
            month += 1
    return found


HAND_CONCEPTIONS = hand_conceptions()
assert HAND_CONCEPTIONS == [(7, 17), (32, 42), (74, 84)]

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


CHILDREN = range(4, 7)
CHILD_MONTHS = [birth for _, birth in HAND_CONCEPTIONS]
FIRST = dummy_world()
assert all(FIRST.cause_due(sim, code) == DUE[(sim, code)] for sim, code in DUE)
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
    assert sorted(world.sims) == [2, 3, *CHILDREN]
    assert world.partnerships == {1: [1, 2, 0, HER_DEATH]}
assert [PARENT.sims[sim][0] for sim in CHILDREN] == [month * MONTH for month in CHILD_MONTHS]
assert all(PARENT.sims[sim][7] == (1, 2, sim - 4) for sim in CHILDREN)
assert [PARENT.sims[sim][2] for sim in CHILDREN] == [FEMALE, MALE, MALE]
# Her conceptions, births and wish as by hand, the chance of every attempt too; the
# attempts run from month 1 to her third conception, with the pauses after the births.
assert not PARENT.pregnancies and PARENT.archive[1][7:9] == (len(CHILDREN), len(CHILDREN))
assert PARENT.wish_of(1) == HER_WISH
ATTEMPT_MONTHS = list(range(1, 8)) + list(range(29, 33)) + list(range(54, 75))
assert [t // MONTH for t, *_ in PARENT.attempts] == ATTEMPT_MONTHS
assert [t // MONTH for t, _, _, _, success in PARENT.attempts if success] == [c for c, _ in HAND_CONCEPTIONS]
assert all(abs(p - hand_chance(t // MONTH)) <= 1e-15 for t, _, _, p, _ in PARENT.attempts)
assert FORK.attempts == PARENT.attempts
assert [(t // MONTH, u, kind) for t, _, _, _, u, kind, _ in PARENT.multiples] == [
    (month, u, "single") for (month, _), u in zip(HAND_CONCEPTIONS, MULTIPLE_UNITS)
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


def golden_save() -> bytes:
    """The save on day 100 of year 3: not pregnant, her series pending in month 54."""
    world = dummy_world()
    world.advance(SAVE_AT)
    assert not world.archive and sorted(world.sims) == [1, 2, 3, 4, 5]
    assert not world.pregnancies and world.sims[1][5:7] == [2, 2]
    assert world.pending_series(1) == (CHILD_MONTHS[1] + 12) * MONTH
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
            born, key, sex, person, _, _, _, parents = PARENT.sims[sim]
            died = "null"
        else:
            born, left, cause, key, sex, person, _, _, _, parents = PARENT.archive[sim]
            died = f"{left} of {cause:016x}"
        alleles = [(locus, copies_at(person, PARENT.data.loci[locus])) for locus in PARENT.data.loci]
        alleles = [(locus, copies) for locus, copies in alleles if any(copies)]
        sex_name = "female" if sex == FEMALE else "male"
        rows.append(f"pedigree_sim_{sim} key {key:016x} sex {sex_name} alleles {alleles} born {born} died {died} parents {parents}")
    for number, (woman, man, formed, ended) in sorted(PARENT.partnerships.items()):
        rows.append(f"pedigree_partnership_{number} woman {woman} man {man} formed {formed} ended {ended}")
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
    print(f"fertility: her wish {HER_WISH} (u {HER_WISH_UNIT!r}), menarche {PARENT.menarche_of(1)!r}")
    print(f"fertility: conceptions and births (months) {HAND_CONCEPTIONS}")
    print(f"fertility: twins draw u per conception (month, u, kind) {[(t // MONTH, u, kind) for t, _, _, _, u, kind, _ in PARENT.multiples]}")
    print(f"fertility: attempts (month, chance, success) {[(t // MONTH, p, ok) for t, _, _, p, ok in PARENT.attempts]}")
    print(f"fertility: children's wishes {[(sim, PARENT.wish_of(sim)) for sim in CHILDREN if PARENT.sims[sim][2] == FEMALE]}")
    print(f"fork: death of sim 1 of core:disease_ar due at {HER_AR_DEATH} (age {HER_AR_AGE!r}), planned by entry {FORK_OVERRIDE_ENTRY}")
    print(f"fork: deaths of core:disease_ar (sim, time, plan) {FORK_AR_DEATHS}")
    for row in pedigree_rows():
        print(row)

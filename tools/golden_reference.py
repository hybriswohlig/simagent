#!/usr/bin/env python3
"""The goldens of the scenario `tests/scenarios/dummy.json`, computed independently of
the Rust code (ADR-0003 6.3) by the model in tools/world_reference.py: year hashes, log
hash and the golden save, and the goldens of its fork at year 5 with the override
"hereditary diseases on".

The scenario: seed 1, founder tag `test:dummy`, the switch off, no causes of death of its
own (they come from the data: senescence and accident with the rates of the genetics
specification 7.2, ADR-0017 section 2), on the repository data with its six hereditary
diseases (ADR-0017 section 3), three founders born 30 years (sim 1), 25 years and 100
days (sim 2) and one second (sim 3) before the start, thirty years (long enough for the
first death, so that the goldens cover every event and log kind). Sim 1 is female with the
dummy allele twice at `core:disease_ar`, sim 2 male with dummy and wild (from mother and
father), sim 3 male with dummy twice. Their lineage keys are those of the founders 0 to 2
of the tag (ADR-0001 section 4). Sims 1 and 2 are a couple from the start (partnership
1, entry `core:partnered` after the founders). Fertility as in the acceptance scenario
of ADR-0001: an attempt per month with 0.05, the mother 18 to 45 years old, ten months of
pregnancy.

What happens (a year has 364 days of 86 400 seconds):

- Every founder has its deaths of senescence and accident planned at the start, from its
  age then (B19): sim 1 dies of senescence after about 27.2 years (at about 57), sim 2
  would after about 69.8 years, sim 3 after about 87.2 years; their accidents lie
  thousands of years ahead. Sim 1's death in year 27 is the only one of the thirty years;
  it ends partnership 1.
- Sims 1 and 3 have `core:disease_ar` (recessive, penetrance 1, onset 0, two steps):
  each an affliction with nothing used, its exposure not running while the switch is
  off, so no death of it is planned. Sim 2 carries one copy and is healthy (no draw).
  No founder has an allele at another disease locus.
- Sim 1 conceives in month 7 (entries `core:conception` and two `core:meiosis`) and has
  the child (sim 4, entry `core:birth`) in month 17; six more follow, in months 38, 49,
  63, 101, 125 and 164 (sims 5 to 10), the last before she turns 45. Sims 7 and 10 got the
  dummy allele from their father as well and have `core:disease_ar` (not exposed); the
  other five got the wild allele from him and are carriers without an affliction. Each
  child has its deaths of senescence (after 77 to 101 years) and accident planned from its
  birth.
- The golden save is taken on day 100 of year 3: nobody has died, sims 4 and 5 are born
  and sim 1 is pregnant with her third child.

The fork (ADR-0001 section 6): the same run up to the last second of year 4, then the
input `ForkOverride` with the stamp T_5 - 1 s. It acts at T_5 before every event of
that second and writes one entry `core:param_changed` with the input (sequence number
2) as its cause. The switch is on from then on: the living sims with a lethal affliction
(sims 1, 3 and 7) are exposed since T_5, and that entry plans their deaths of
`core:disease_ar` from their whole budgets: sim 1 (at 35 years, past the change at 18)
after about 42.9 years of exposure, at about 77.9 years of age; sims 3 and 7 about 87
and 94 years after the start. Sim 10 is exposed from its birth on, its birth plans its
death of the disease about 35.8 years after the start. Sim 1 dies of senescence first, in
year 27 as in the run, which cancels her death of the disease; the others lie beyond the
thirty years. So the fork has one entry more than the run (the override), and its year
hashes differ from year 5 on (the switch and the exposures are state, the planned deaths
are in the queue).

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
from rng_reference import founder_key, str_key

ROOT = Path(__file__).resolve().parent.parent
DAY = 86_400
SEED = 1
TAG = "test:dummy"
BORN = [-30 * YEAR, -25 * YEAR - 100 * DAY, -1]
SEX = [FEMALE, MALE, MALE]
GENES = [genotype(DUMMY, DUMMY), genotype(DUMMY, WILD), genotype(DUMMY, DUMMY)]
FOUNDERS = list(zip(BORN, SEX, GENES))
COUPLES = [(0, 1)]
FERTILITY = (0.05, 18, 45, 10)
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


def dummy_world() -> World:
    return World(SEED, TAG, False, FOUNDERS, COUPLES, FERTILITY)


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


CHILDREN = range(4, 11)
CHILD_MONTHS = [17, 38, 49, 63, 101, 125, 164]
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
assert all(PARENT.sims[sim][6] == (1, 2, sim - 4) for sim in CHILDREN)
assert not PARENT.pregnancies and PARENT.archive[1][7] == len(CHILDREN)
LETHAL = [sim for sim in sorted(PARENT.sims) if PARENT.sims[sim][4]]
assert LETHAL == [3, 7, 10]
assert all([a.triple() for a in PARENT.sims[sim][4]] == [(AR_KEY, 0.0, (0.0, None))] for sim in LETHAL)
assert all(copies_at(PARENT.sims[sim][3], CYSTIC.locus) == [1, 1 if sim in (7, 10) else 0] for sim in CHILDREN)
# In the fork sims 3 and 7 are exposed since T_5, sim 10 since its birth; its birth
# planned its death. The override is the first entry the fork writes differently.
FORK_OVERRIDE_ENTRY = next(i for i, (ours, theirs) in enumerate(zip(FORK.log, PARENT.log)) if ours != theirs) + 1
SINCE = {3: FORK_YEAR * YEAR, 7: FORK_YEAR * YEAR, 10: CHILD_MONTHS[-1] * MONTH}
assert all([a.triple() for a in FORK.sims[sim][4]] == [(AR_KEY, 0.0, (0.0, SINCE[sim]))] for sim in LETHAL)
FORK_AR_DEATHS = ar_deaths(FORK)
assert [sim for sim, *_ in FORK_AR_DEATHS] == LETHAL
assert all(plan == (FORK_OVERRIDE_ENTRY, FORK_YEAR * YEAR) for sim, _, plan in FORK_AR_DEATHS if sim != 10)
assert FORK_AR_DEATHS[-1][2][1] == SINCE[10]
assert all(time > YEARS * YEAR for _, time, _ in FORK_AR_DEATHS)


def golden_save() -> bytes:
    """The save on day 100 of year 3."""
    world = dummy_world()
    world.advance(SAVE_AT)
    assert not world.archive and sorted(world.sims) == [1, 2, 3, 4, 5]
    assert [value[2] for value in world.pregnancies.values()] == [2]
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
            born, key, sex, person, _, _, parents = PARENT.sims[sim]
            died = "null"
        else:
            born, left, cause, key, sex, person, _, _, parents = PARENT.archive[sim]
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
    print(f"fork: death of sim 1 of core:disease_ar due at {HER_AR_DEATH} (age {HER_AR_AGE!r}), planned by entry {FORK_OVERRIDE_ENTRY}")
    print(f"fork: deaths of core:disease_ar (sim, time, plan) {FORK_AR_DEATHS}")
    for row in pedigree_rows():
        print(row)

#!/usr/bin/env python3
"""The goldens of the scenario `tests/scenarios/dummy.json`, computed independently of
the Rust code (ADR-0003 6.3) by the model in tools/world_reference.py: year hashes, log
hash and the golden save, and the goldens of its fork at year 5 with the override
"hereditary diseases on".

The scenario: seed 1, founder tag `test:dummy`, the switch off, the causes of death
`test:base` with 0.04 per year and `test:old_age` with 0.02 per year for everyone and
`test:dummy_disease` with 0.3 per year for the homozygous (C5), three founders born 30
years (sim 1), 25 years and 100 days (sim 2) and one second (sim 3) before the start,
ten years. Sim 1 is female with the dummy allele twice, sim 2 male with dummy and wild
(from mother and father), sim 3 male with dummy twice. Their lineage keys are those of
the founders 0 to 2 of the tag (ADR-0001 section 4). Sims 1 and 2 are a couple from the
start (partnership 1, entry `core:partnered` after the founders). Fertility as in the
acceptance scenario of ADR-0001: an attempt per month with 0.05, the mother 18 to 45
years old, ten months of pregnancy.

What happens (a year has 364 days of 86 400 seconds):

- With the switch off the disease acts on nobody; sims 1 and 3 keep an exposure of
  nothing. Sim 3 dies of `test:base` in year 2 and sim 2 of the same cause near the end
  of year 3; their deaths of old age (due after 26 and 239 years) and their next
  birthdays are cancelled. The death of sim 2 ends partnership 1.
- Sim 1 conceives in month 7 (entries `core:conception` and two `core:meiosis`) and has
  the child (sim 4, entry `core:birth`) in month 17; she has two more, in months 38 and
  49 (sims 5 and 6). The death of sim 2 in month 52 ends her attempts. Sim 1 lives through the ten years; its deaths are due after
  27 years (`test:base`) and 20 years (`test:old_age`).
- The golden save is taken on day 100 of year 3: sim 3 is in the archive, sim 2 still
  lives, sims 4 and 5 are born and sim 1 is pregnant with her third child.

The fork (ADR-0001 section 6): the same run up to the last second of year 4, then the
input `ForkOverride` with the stamp T_5 - 1 s. It acts at T_5 before every event of
that second and writes one entry `core:param_changed` with the input (sequence number
2) as its cause. The switch is on from then on: sim 1, the only homozygous sim alive,
is exposed since T_5, and its death of the disease (after about 4.14 years of exposure)
is planned by that entry. It dies of it in year 9, about 52 days after its birthday.

Writes tests/golden/dummy.json, tests/golden/dummy.save and tests/golden/dummy_fork.json
(protected list, ADR-0000).
Usage: .venv/bin/python tools/golden_reference.py [--check]
"""

import json
import sys
from pathlib import Path

from postcard_reference import DUMMY, FEMALE, MALE, WILD, genotype
from world_reference import CORE_VERSION, EMPTY_DATA_HASH, MONTH, YEAR, World, needed_seconds
from rng_reference import founder_key

ROOT = Path(__file__).resolve().parent.parent
DAY = 86_400
SEED = 1
TAG = "test:dummy"
CAUSES = [("test:base", 0.04), ("test:old_age", 0.02), ("test:dummy_disease", 0.3, "dummy_homozygous")]
BORN = [-30 * YEAR, -25 * YEAR - 100 * DAY, -1]
SEX = [FEMALE, MALE, MALE]
GENES = [genotype(DUMMY, DUMMY), genotype(DUMMY, WILD), genotype(DUMMY, DUMMY)]
FOUNDERS = list(zip(BORN, SEX, GENES))
COUPLES = [(0, 1)]
FERTILITY = (0.05, 18, 45, 10)
YEARS = 10
SAVE_AT = 3 * YEAR + 100 * DAY
FORK_YEAR = 5

assert BORN == [-943_488_000, -794_880_000, -1]

# The story above, checked: when each death is due, in years.
DUE = {
    (sim, code): needed_seconds(SEED, founder_key(TAG, sim - 1), code, rate)
    for sim in (1, 2, 3)
    for code, rate, *_ in CAUSES
}
assert 2 * YEAR < DUE[(3, "test:base")] < 3 * YEAR
assert SAVE_AT < DUE[(2, "test:base")] < 4 * YEAR
assert all(DUE[(1, code)] > YEARS * YEAR for code in ("test:base", "test:old_age"))
assert 9 * YEAR + 51 * DAY < FORK_YEAR * YEAR + DUE[(1, "test:dummy_disease")] < 9 * YEAR + 52 * DAY
assert DUE[(2, "test:old_age")] > DUE[(2, "test:base")]
assert DUE[(3, "test:old_age")] > DUE[(3, "test:base")]


def dummy_world() -> World:
    return World(SEED, TAG, False, CAUSES, FOUNDERS, COUPLES, FERTILITY)


def run(fork: bool) -> World:
    world = dummy_world()
    if fork:
        world.advance(FORK_YEAR * YEAR - 1)
        world.override(True)
    world.advance(YEARS * YEAR - 1)
    return world


PARENT = run(fork=False)
FORK = run(fork=True)
assert len(PARENT.year_hashes) == YEARS and len(FORK.year_hashes) == YEARS
assert FORK.year_hashes[:FORK_YEAR] == PARENT.year_hashes[:FORK_YEAR]
assert all(
    ours != theirs
    for ours, theirs in zip(FORK.year_hashes[FORK_YEAR:], PARENT.year_hashes[FORK_YEAR:])
)
assert len(FORK.log) == len(PARENT.log) + 2, "the override and the death of sim 1"
assert sorted(PARENT.archive) == [2, 3] and sorted(PARENT.sims) == [1, 4, 5, 6]
assert PARENT.partnerships == {1: [1, 2, 0, DUE[(2, "test:base")]]}
assert [PARENT.sims[sim][0] for sim in (4, 5, 6)] == [17 * MONTH, 38 * MONTH, 49 * MONTH]
assert not PARENT.pregnancies
assert sorted(FORK.archive) == [1, 2, 3] and FORK.archive[1][1] == FORK_YEAR * YEAR + DUE[(1, "test:dummy_disease")]


def golden_save() -> bytes:
    """The save on day 100 of year 3."""
    world = dummy_world()
    world.advance(SAVE_AT)
    assert sorted(world.archive) == [3] and sorted(world.sims) == [1, 2, 4, 5]
    assert [value[2] for value in world.pregnancies.values()] == [2]
    return world.save()


def golden(world: World) -> str:
    """The text of tests/golden/<scenario>.json."""
    content = {
        "core_version": CORE_VERSION,
        "data_hash": f"{EMPTY_DATA_HASH:016x}",
        "year_hashes": [
            {"year": year, "hash": f"{value:016x}"}
            for year, value in enumerate(world.year_hashes)
        ],
        "log_hash": f"{world.log_hash():016x}",
    }
    return json.dumps(content, indent=2) + "\n"


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
    print(f"deaths due (seconds): {DUE}")

#!/usr/bin/env python3
"""The goldens of the scenario `tests/scenarios/dummy.json`, computed independently of
the Rust code (ADR-0003 6.3): year hashes, log hash and the golden save, and the goldens
of its fork at year 5 with the override "hereditary diseases on".

The scenario: seed 1, founder tag `test:dummy`, three founders born 30 years (sim 1),
25 years and 100 days (sim 2) and one second (sim 3) before the start, ten years.
Sim 1 is female with the alleles wild and dummy (from mother and father), sim 2 male
with dummy and wild, sim 3 male with dummy twice. Their lineage keys are those of the
founders 0 to 2 of the tag (ADR-0001 section 4).
By hand from ADR-0002 and ADR-0004 (a year has 364 days of 86 400 seconds):

- `Start` writes the entries 1 to 4 at t = 0 and plans the year end series (insertion
  number 1) and the first birthdays (insertion numbers 2 to 4).
- In year k the birthdays fire in the order sim 1 (k * YEAR), sim 2 (day 264),
  sim 3 (last second). They are the entries 5 + 3k, 6 + 3k, 7 + 3k, and each plans the
  next birthday a year later with the insertion numbers 5 + 3k, 6 + 3k, 7 + 3k.
- The year end of year k runs in the last second of the year, after sim 3's birthday,
  and sees the clock one second before.

The fork (ADR-0001 section 6): the same run up to the last second of year 4, then the
input `ForkOverride` with the stamp T_5 - 1 s. It acts at T_5 before every event of
that second and writes one entry `core:param_changed` (the next EventId, 20) with the
input (sequence number 2) as its cause. From then on the projection carries the switch
as on, and every later entry has an EventId one higher than in the parent.

Writes tests/golden/dummy.json, tests/golden/dummy.save and tests/golden/dummy_fork.json
(protected list, ADR-0000).
Usage: .venv/bin/python tools/golden_reference.py [--check]
"""

import json
import sys
from pathlib import Path

import xxhash

from postcard_reference import (
    DUMMY,
    FEMALE,
    MALE,
    WILD,
    founder,
    genotype,
    lineage_key,
    param_change,
    seq,
    signed,
    string,
    variant,
    varint,
    world_params,
)
from rng_reference import founder_key
from state_reference import (
    CORE_VERSION,
    EMPTY_DATA_HASH,
    NONE,
    PROJECTION_VERSION,
    YEAR,
    birthday,
    entry,
    header,
    plan_ref,
    queue_entry,
    sim_state,
    some,
    year_end,
)

ROOT = Path(__file__).resolve().parent.parent
DAY = 86_400
SEED = 1
TAG = "test:dummy"
BORN = [-30 * YEAR, -25 * YEAR - 100 * DAY, -1]
SEX = [FEMALE, MALE, MALE]
GENES = [genotype(WILD, DUMMY), genotype(DUMMY, WILD), genotype(DUMMY, DUMMY)]
KEYS = [founder_key(TAG, index) for index in range(3)]
FOUNDERS = list(zip(BORN, KEYS, SEX, GENES))
YEARS = 10
SAVE_AT = 3 * YEAR + 100 * DAY

assert BORN == [-943_488_000, -794_880_000, -1]


def hash64(data: bytes) -> int:
    return xxhash.xxh3_64_intdigest(data, seed=0)


def birthday_time(sim: int, year: int) -> int:
    """When sim (1 to 3) has its birthday in `year`."""
    return [year * YEAR, year * YEAR + 264 * DAY, (year + 1) * YEAR - 1][sim - 1]


FORK_YEAR = 5


def birthday_entry_id(sim: int, year: int, fork: bool = False) -> int:
    """The entry of the birthday; in the fork the override entry comes before year 5."""
    return 4 + 3 * year + sim + (1 if fork and year >= FORK_YEAR else 0)


def projection(
    closed_through: int, queue: list, next_insertion: int, diseases: bool = False
) -> bytes:
    sims = seq([varint(sim) + sim_state(*state) for sim, state in zip((1, 2, 3), FOUNDERS)])
    return (
        varint(PROJECTION_VERSION)
        + some(varint(SEED))
        + some(world_params(diseases))
        + signed(closed_through)
        + varint(4)
        + seq(queue)
        + varint(next_insertion)
        + sims
        + seq([])
    )


def year_projection(year: int, diseases: bool = False) -> bytes:
    """What the year end observer of `year` sees."""
    now = (year + 1) * YEAR - 1
    queue = [
        queue_entry(birthday_time(1, year + 1), 5 + 3 * year, birthday(1), None),
        queue_entry(birthday_time(2, year + 1), 6 + 3 * year, birthday(2), None),
        queue_entry(birthday_time(3, year + 1), 7 + 3 * year, birthday(3), None),
        queue_entry(now + YEAR, 1, year_end(), YEAR),
    ]
    return projection(now - 1, queue, 8 + 3 * year, diseases)


YEAR_HASHES = [hash64(year_projection(year)) for year in range(YEARS)]
FORK_YEAR_HASHES = [
    hash64(year_projection(year, diseases=year >= FORK_YEAR)) for year in range(YEARS)
]
assert FORK_YEAR_HASHES[:FORK_YEAR] == YEAR_HASHES[:FORK_YEAR]
assert all(a != b for a, b in zip(FORK_YEAR_HASHES[FORK_YEAR:], YEAR_HASHES[FORK_YEAR:]))


def log_entries(fork: bool = False) -> list:
    by_input = variant(2, varint(1))
    entries = [entry(1, 0, [], [by_input], variant(0, varint(len(BORN))))]
    for sim, born, key in zip((1, 2, 3), BORN, KEYS):
        subject = variant(0, varint(sim))
        added = variant(1, signed(born), lineage_key(key))
        entries.append(entry(1 + sim, 0, [subject], [by_input], added))
    ages = {1: 30, 2: 26, 3: 1}
    for year in range(YEARS):
        if fork and year == FORK_YEAR:
            by_override = variant(2, varint(2))
            changed = variant(3, param_change(True))
            entries.append(entry(20, FORK_YEAR * YEAR, [], [by_override], changed))
        for sim in (1, 2, 3):
            if year == 0:
                plan = variant(1, varint(1 + sim), signed(0))
            else:
                plan = variant(
                    1,
                    varint(birthday_entry_id(sim, year - 1, fork)),
                    signed(birthday_time(sim, year - 1)),
                )
            entries.append(
                entry(
                    birthday_entry_id(sim, year, fork),
                    birthday_time(sim, year),
                    [variant(0, varint(sim))],
                    [plan],
                    variant(2, varint(ages[sim] + year)),
                )
            )
    return entries


LOG = log_entries()
assert len(LOG) == 34
LOG_HASH = hash64(b"".join(LOG))
FORK_LOG = log_entries(fork=True)
assert len(FORK_LOG) == 35 and FORK_LOG[:19] == LOG[:19] and FORK_LOG[19] != LOG[19]
FORK_LOG_HASH = hash64(b"".join(FORK_LOG))


def golden_save() -> bytes:
    """The save on day 100 of year 3: sim 1 had its birthday this year, the others not."""
    year = 3
    queue = [
        (birthday_time(2, year), 6 + 3 * (year - 1), birthday(2), None),
        (birthday_time(3, year), 7 + 3 * (year - 1), birthday(3), None),
        ((year + 1) * YEAR - 1, 1, year_end(), YEAR),
        (birthday_time(1, year + 1), 5 + 3 * year, birthday(1), None),
    ]
    plans = [
        plan_ref(birthday_entry_id(2, year - 1), birthday_time(2, year - 1)),
        plan_ref(birthday_entry_id(3, year - 1), birthday_time(3, year - 1)),
        plan_ref(1, 0),
        plan_ref(birthday_entry_id(1, year), birthday_time(1, year)),
    ]
    founders = seq([founder(born, sex, genes) for born, _, sex, genes in FOUNDERS])
    start = variant(0, varint(SEED), string(TAG), world_params(False), founders)
    return (
        header(EMPTY_DATA_HASH)
        + projection(SAVE_AT, [queue_entry(*item) for item in queue], 6 + 3 * year)
        + seq(plans)
        + varint(birthday_entry_id(1, year) + 1)
        + seq([signed(-1) + varint(1) + start])
        + seq([signed(done) + varint(YEAR_HASHES[done]) for done in range(year)])
    )


def golden(year_hashes: list, log_hash_value: int) -> str:
    """The text of tests/golden/<scenario>.json."""
    content = {
        "core_version": CORE_VERSION,
        "data_hash": f"{EMPTY_DATA_HASH:016x}",
        "year_hashes": [
            {"year": year, "hash": f"{value:016x}"} for year, value in enumerate(year_hashes)
        ],
        "log_hash": f"{log_hash_value:016x}",
    }
    return json.dumps(content, indent=2) + "\n"


if __name__ == "__main__":
    text = golden(YEAR_HASHES, LOG_HASH)
    fork_text = golden(FORK_YEAR_HASHES, FORK_LOG_HASH)
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

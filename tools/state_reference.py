#!/usr/bin/env python3
"""Reference bytes and hashes for the canonical projection of sim-core (ADR-0003
section 5), independent of the Rust code.

The bytes follow the postcard wire format (see postcard_reference.py), the field order
follows ADR-0003 5.2 and ADR-0002 ("Speichern, Laden, Hash"). The hash is xxh3-64 with
seed 0 from the C reference implementation (PyPI xxhash, pinned in requirements.txt).

Scenario "tiny": Start with world_seed 0xff and one founder born one second before the
start (ADR-0004). By hand from ADR-0002 and ADR-0004:
- the input is applied at now = 0; the year end series gets insertion number 1 (first at
  YEAR - 1, period YEAR), the founder's first birthday insertion number 2 (at YEAR - 1);
- in the queue the birthday comes first: same second, domain phase before observers.

Usage: .venv/bin/python tools/state_reference.py
"""

import xxhash

from postcard_reference import SAMPLE_ENTRY, seq, signed, string, variant, varint

YEAR = 364 * 86_400

NONE = b"\x00"


def some(data: bytes) -> bytes:
    return b"\x01" + data


def year_end() -> bytes:
    return variant(0)


def birthday(sim: int) -> bytes:
    return variant(1, varint(sim))


def queue_entry(time: int, insertion: int, payload: bytes, period) -> bytes:
    """(time, micro, seq, payload, period), ADR-0002."""
    period_bytes = NONE if period is None else some(signed(period))
    return signed(time) + varint(0) + varint(insertion) + payload + period_bytes


def projection(closed_through: int, queue: list, next_insertion: int) -> bytes:
    return (
        varint(1)  # projection_version
        + some(varint(0xFF))  # world_seed
        + signed(closed_through)
        + varint(2)  # next SimId
        + seq(queue)
        + varint(next_insertion)
        + seq([varint(1) + signed(-1)])  # sims: id 1, born -1
        + seq([])  # archive
    )


# Right after Start: nothing closed yet.
TINY_AFTER_START = projection(
    -1,
    [
        queue_entry(YEAR - 1, 2, birthday(1), None),
        queue_entry(YEAR - 1, 1, year_end(), YEAR),
    ],
    3,
)

# What the year end observer of year 0 sees at t = YEAR - 1: the clock stands at t - 1
# (ADR-0002), the birthday of this second has run and planned the next one (insertion
# number 3), the year end series already holds its next occurrence.
TINY_YEAR_0 = projection(
    YEAR - 2,
    [
        queue_entry(2 * YEAR - 1, 3, birthday(1), None),
        queue_entry(2 * YEAR - 1, 1, year_end(), YEAR),
    ],
    4,
)

# After advance_until(YEAR - 1): the same state with the second closed.
TINY_AFTER_YEAR_0 = projection(
    YEAR - 1,
    [
        queue_entry(2 * YEAR - 1, 3, birthday(1), None),
        queue_entry(2 * YEAR - 1, 1, year_end(), YEAR),
    ],
    4,
)


# Save file of ADR-0006: header, projection, exceptions.
EMPTY_DATA_HASH = xxhash.xxh3_64_intdigest(b"", seed=0)


def header(data_hash: int) -> bytes:
    return b"SIMAGENT" + varint(1) + varint(1) + varint(data_hash)


def plan_ref(by: int, planned_at: int) -> bytes:
    return varint(by) + signed(planned_at)


# The input log of "tiny": Start at stamp t = -1, seq 1, with world_seed 0xff, the
# founder tag `test:tiny` and one founder born at -1.
TINY_INPUT_LOG = seq(
    [
        signed(-1)
        + varint(1)
        + variant(0, varint(0xFF), string("test:tiny"), seq([signed(-1)]))
    ]
)

# Right after Start (the log taken): plan references in queue order (the birthday was
# planned by the founder's entry 2, the year end series by `core:started`, entry 1), the
# next EventId is 3, no year hashes yet.
TINY_SAVE_AFTER_START = (
    header(EMPTY_DATA_HASH)
    + TINY_AFTER_START
    + seq([plan_ref(2, 0), plan_ref(1, 0)])
    + varint(3)
    + TINY_INPUT_LOG
    + seq([])
)

# A world that never started: nothing but the clock and the counters.
EMPTY_PROJECTION = (
    varint(1) + NONE + signed(-1) + varint(1) + seq([]) + varint(1) + seq([]) + seq([])
)
EMPTY_SAVE = header(EMPTY_DATA_HASH) + EMPTY_PROJECTION + seq([]) + varint(1) + seq([]) + seq([])


# The log of "tiny" up to the end of year 0 (ADR-0004 section 7). An entry is
# (id, t, subjects, causes, payload). Subject::Sim = 0. Cause: Event = 0, Plan = 1
# { by, planned_at }, Input = 2, Rule = 3. Payload: core:started = 0 { founders },
# core:founder_added = 1 { born }, core:birthday = 2 { age_years }.
def entry(event_id: int, time: int, subjects: list, causes: list, payload: bytes) -> bytes:
    return varint(event_id) + signed(time) + seq(subjects) + seq(causes) + payload


SIM_1 = variant(0, varint(1))
BY_INPUT_1 = variant(2, varint(1))
TINY_LOG = [
    entry(1, 0, [], [BY_INPUT_1], variant(0, varint(1))),
    entry(2, 0, [SIM_1], [BY_INPUT_1], variant(1, signed(-1))),
    entry(3, YEAR - 1, [SIM_1], [variant(1, varint(2), signed(0))], variant(2, varint(1))),
]

# The save at the end of year 0: the birthday of that second planned the next one
# (entry 3 at YEAR - 1), the year end series keeps the plan of `core:started`; the next
# EventId is 4; one year hash.
TINY_YEAR_0_HASH = xxhash.xxh3_64_intdigest(TINY_YEAR_0, seed=0)
TINY_SAVE_AFTER_YEAR_0 = (
    header(EMPTY_DATA_HASH)
    + TINY_AFTER_YEAR_0
    + seq([plan_ref(3, YEAR - 1), plan_ref(1, 0)])
    + varint(4)
    + TINY_INPUT_LOG
    + seq([signed(0) + varint(TINY_YEAR_0_HASH)])
)


def digest(data: bytes) -> str:
    return f"{xxhash.xxh3_64_intdigest(data, seed=0):016x}"


if __name__ == "__main__":
    for name, data in [
        ("tiny_after_start", TINY_AFTER_START),
        ("tiny_year_0", TINY_YEAR_0),
        ("tiny_after_year_0", TINY_AFTER_YEAR_0),
    ]:
        print(f"{name} bytes {data.hex()}")
        print(f"{name} hash {digest(data)}")
    print(f"empty_data_hash {EMPTY_DATA_HASH:016x}")
    print(f"header {header(EMPTY_DATA_HASH).hex()}")
    print(f"tiny_save_after_start {TINY_SAVE_AFTER_START.hex()}")
    print(f"empty_save {EMPTY_SAVE.hex()}")
    print(f"tiny_log_hash {digest(b''.join(TINY_LOG))}")
    print(f"tiny_save_after_year_0 {TINY_SAVE_AFTER_YEAR_0.hex()}")
    # Log hash: xxh3-64 over the postcard bytes of all entries in order (ADR-0004).
    print(f"log_hash_empty {digest(b'')}")
    print(f"log_hash_entry {digest(SAMPLE_ENTRY)}")
    print(f"log_hash_entry_twice {digest(SAMPLE_ENTRY + SAMPLE_ENTRY)}")

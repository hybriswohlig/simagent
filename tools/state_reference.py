#!/usr/bin/env python3
"""Reference bytes and hashes for the canonical projection of sim-core (ADR-0003
section 5), independent of the Rust code.

The bytes follow the postcard wire format (see postcard_reference.py), the field order
follows ADR-0003 5.2 and ADR-0002 ("Speichern, Laden, Hash"). The hash is xxh3-64 with
seed 0 from the C reference implementation (PyPI xxhash, pinned in requirements.txt).

Scenario "tiny": Start with world_seed 0xff, founder tag `test:tiny`, the switch of the
hereditary diseases off and one founder born
one second before the start, female, with the alleles wild (from the mother) and dummy
(from the father) at the dummy locus (ADR-0004). Her lineage key is that of founder 0
of the tag (ADR-0001 section 4). By hand from ADR-0002 and ADR-0004:
- the input is applied at now = 0; the year end series gets insertion number 1 (first at
  YEAR - 1, period YEAR), the founder's first birthday insertion number 2 (at YEAR - 1);
- in the queue the birthday comes first: same second, domain phase before observers.

Usage: .venv/bin/python tools/state_reference.py
"""

import xxhash

from postcard_reference import (
    DUMMY,
    FEMALE,
    MALE,
    SAMPLE_ENTRY,
    WILD,
    exposure,
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

import world_reference

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


PROJECTION_VERSION = 6
CORE_VERSION = 6


def sim_state(born: int, key: int, sex: bytes, genes: bytes, sick=None) -> bytes:
    """Sim { born, lineage_key, sex, genotype, exposure }: the exposure is None but for
    a sim with the dummy allele twice (C5)."""
    return signed(born) + lineage_key(key) + sex + genes + exposure(sick)


TINY_KEY = founder_key("test:tiny", 0)
TINY_GENES = genotype(WILD, DUMMY)


def projection(
    closed_through: int, queue: list, next_insertion: int, diseases: bool = False
) -> bytes:
    return (
        varint(PROJECTION_VERSION)
        + some(varint(0xFF))  # world_seed
        + some(world_params(diseases))
        + seq([])  # death_causes
        + signed(closed_through)
        + varint(2)  # next SimId
        + varint(1)  # next PartnershipId
        + seq(queue)
        + varint(next_insertion)
        + seq([varint(1) + sim_state(-1, TINY_KEY, FEMALE, TINY_GENES)])  # sims: id 1
        + seq([])  # partnerships
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


# Scenario "pair": seed 0xff, founder tag `test:pair`, a woman born two seconds before
# the start with the alleles wild and dummy, and a man born one second before it with
# dummy twice. Right after Start: her first birthday (insertion number 2) is due at
# YEAR - 2, his (insertion number 3) at YEAR - 1, in the same second as the year end.
PAIR_KEYS = [founder_key("test:pair", index) for index in range(2)]
PAIR_AFTER_START = (
    varint(PROJECTION_VERSION)
    + some(varint(0xFF))
    + some(world_params(False))
    + seq([])  # death_causes
    + signed(-1)
    + varint(3)  # next SimId
    + varint(1)  # next PartnershipId
    + seq(
        [
            queue_entry(YEAR - 2, 2, birthday(1), None),
            queue_entry(YEAR - 1, 3, birthday(2), None),
            queue_entry(YEAR - 1, 1, year_end(), YEAR),
        ]
    )
    + varint(4)
    + seq(
        [
            varint(1) + sim_state(-2, PAIR_KEYS[0], FEMALE, genotype(WILD, DUMMY)),
            # He has the dummy allele twice: nothing closed, the switch is off.
            varint(2) + sim_state(-1, PAIR_KEYS[1], MALE, genotype(DUMMY, DUMMY), (0, None)),
        ]
    )
    + seq([])  # partnerships
    + seq([])  # archive
)

# Save file of ADR-0006: header, projection, exceptions.
EMPTY_DATA_HASH = xxhash.xxh3_64_intdigest(b"", seed=0)


def header(data_hash: int) -> bytes:
    """magic, format_version 1, core_version, data_hash"""
    return b"SIMAGENT" + varint(1) + varint(CORE_VERSION) + varint(data_hash)


def plan_ref(by: int, planned_at: int) -> bytes:
    return varint(by) + signed(planned_at)


# The input log of "tiny": Start at stamp t = -1, seq 1, with world_seed 0xff, the
# founder tag `test:tiny` and the one founder.
TINY_INPUT_LOG = seq(
    [
        signed(-1)
        + varint(1)
        + variant(
            0,
            varint(0xFF),
            string("test:tiny"),
            world_params(False),
            seq([]),  # death_causes
            seq([founder(-1, FEMALE, TINY_GENES)]),
            seq([]),  # couples
        )
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
    varint(PROJECTION_VERSION)
    + NONE  # world_seed
    + NONE  # world_params
    + seq([])  # death_causes
    + signed(-1)
    + varint(1)  # next SimId
    + varint(1)  # next PartnershipId
    + seq([])
    + varint(1)
    + seq([])  # sims
    + seq([])  # partnerships
    + seq([])  # archive
)
EMPTY_SAVE = header(EMPTY_DATA_HASH) + EMPTY_PROJECTION + seq([]) + varint(1) + seq([]) + seq([])


# The log of "tiny" up to the end of year 0 (ADR-0004 section 7). An entry is
# (id, t, subjects, causes, payload). Subject::Sim = 0. Cause: Event = 0, Plan = 1
# { by, planned_at }, Input = 2, Rule = 3. Payload: core:started = 0 { founders },
# core:founder_added = 1 { born, lineage_key }, core:birthday = 2 { age_years },
# core:param_changed = 3 { change }, core:death = 4 { cause }. Cause::Random = 4
# { tag, subject, occasion, detail }.
def entry(event_id: int, time: int, subjects: list, causes: list, payload: bytes) -> bytes:
    return varint(event_id) + signed(time) + seq(subjects) + seq(causes) + payload


SIM_1 = variant(0, varint(1))
BY_INPUT_1 = variant(2, varint(1))
TINY_LOG = [
    entry(1, 0, [], [BY_INPUT_1], variant(0, varint(1))),
    entry(2, 0, [SIM_1], [BY_INPUT_1], variant(1, signed(-1), lineage_key(TINY_KEY))),
    entry(3, YEAR - 1, [SIM_1], [variant(1, varint(2), signed(0))], variant(2, varint(1))),
]

# "tiny" with an override block right after Start (stamp -1, seq 2) that switches the
# hereditary diseases on: one entry `core:param_changed` at now = 0 with the next
# EventId 3 and the input as its cause. Queue and counters of the scheduler stay.
TINY_OVERRIDE_ENTRY = entry(3, 0, [], [variant(2, varint(2))], variant(3, param_change(True)))
TINY_OVERRIDDEN = projection(
    -1,
    [
        queue_entry(YEAR - 1, 2, birthday(1), None),
        queue_entry(YEAR - 1, 1, year_end(), YEAR),
    ],
    3,
    diseases=True,
)
TINY_SAVE_OVERRIDDEN = (
    header(EMPTY_DATA_HASH)
    + TINY_OVERRIDDEN
    + seq([plan_ref(2, 0), plan_ref(1, 0)])
    + varint(4)
    + seq(
        [
            signed(-1)
            + varint(1)
            + variant(
                0,
                varint(0xFF),
                string("test:tiny"),
                world_params(False),
                seq([]),  # death_causes
                seq([founder(-1, FEMALE, TINY_GENES)]),
                seq([]),  # couples
            ),
            signed(-1) + varint(2) + variant(1, seq([param_change(True)])),
        ]
    )
    + seq([])
)

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


# ---------------------------------------------------------------------------------------
# The model of tools/world_reference.py gives the same bytes as the derivations by hand
# above, and the bytes of the scenarios with death.

TINY_FOUNDERS = [(-1, FEMALE, TINY_GENES)]


def tiny_world() -> world_reference.World:
    return world_reference.World(0xFF, "test:tiny", False, [], TINY_FOUNDERS)


def check_the_model_on_tiny() -> None:
    world = tiny_world()
    assert world.projection() == TINY_AFTER_START
    assert world.save() == TINY_SAVE_AFTER_START
    world.advance(YEAR - 1)
    assert world.year_hashes == [TINY_YEAR_0_HASH]
    assert world.projection() == TINY_AFTER_YEAR_0
    assert world.log == TINY_LOG
    assert world.save() == TINY_SAVE_AFTER_YEAR_0
    overridden = tiny_world()
    overridden.override(True)
    assert overridden.projection() == TINY_OVERRIDDEN
    assert overridden.log[-1] == TINY_OVERRIDE_ENTRY
    assert overridden.save() == TINY_SAVE_OVERRIDDEN
    pair = world_reference.World(
        0xFF,
        "test:pair",
        False,
        [],
        [(-2, FEMALE, genotype(WILD, DUMMY)), (-1, MALE, genotype(DUMMY, DUMMY))],
    )
    assert pair.projection() == PAIR_AFTER_START
    assert world_reference.EMPTY_SAVE == EMPTY_SAVE


check_the_model_on_tiny()

# Scenario "mortal": seed 0xff, founder tag `test:mortal`, the switch off, the causes
# `test:base` with 0.5 per year and `test:old_age` with 0.05 per year. A woman born one
# second before the start (wild, dummy) and a man born two seconds before it (dummy
# twice). Her budgets are about 0.729 and 0.189: she dies of `test:base` after about
# 1.46 years, her death of old age (after 3.79 years) and her next birthday are
# cancelled. His are about 2.292 and 0.085: he dies of old age after about 1.71 years,
# his death of `test:base` (after 4.58 years) and his next birthday are cancelled.
MORTAL_CAUSES = [("test:base", 0.5), ("test:old_age", 0.05)]
MORTAL_FOUNDERS = [(-1, FEMALE, genotype(WILD, DUMMY)), (-2, MALE, genotype(DUMMY, DUMMY))]


def mortal_world() -> world_reference.World:
    return world_reference.World(0xFF, "test:mortal", False, MORTAL_CAUSES, MORTAL_FOUNDERS)


MORTAL = mortal_world()
MORTAL_AFTER_START = MORTAL.projection()
MORTAL_SAVE_AFTER_START = MORTAL.save()
MORTAL_DEATHS = [
    world_reference.needed_seconds(0xFF, founder_key("test:mortal", index), code, rate)
    for index in range(2)
    for code, rate in MORTAL_CAUSES
]
MORTAL.advance(2 * YEAR - 1)
MORTAL_AFTER_YEAR_1 = MORTAL.projection()
MORTAL_SAVE_AFTER_YEAR_1 = MORTAL.save()
assert len(MORTAL.log) == 7 and not MORTAL.sims and sorted(MORTAL.archive) == [1, 2]


# Scenario "tie": seed 0xff, founder tag `test:tie`, one woman born one second before
# the start. `test:base` has 5 per year; the hazard of `test:old_age` is chosen so that
# both causes are due in the same second (about 1.63 years). The cause earlier in the
# list wins: its event was planned first.
TIE_KEY = founder_key("test:tie", 0)
TIE_BASE = ("test:base", 5.0)
TIE_DUE = world_reference.needed_seconds(0xFF, TIE_KEY, *TIE_BASE)
TIE_OLD_AGE = ("test:old_age", 0.5934198823895418)
assert world_reference.needed_seconds(0xFF, TIE_KEY, *TIE_OLD_AGE) == TIE_DUE


def tie_world(causes: list) -> world_reference.World:
    world = world_reference.World(0xFF, "test:tie", False, causes, TINY_FOUNDERS)
    world.advance(2 * YEAR - 1)
    return world


TIE_BASE_FIRST = tie_world([TIE_BASE, TIE_OLD_AGE])
TIE_OLD_AGE_FIRST = tie_world([TIE_OLD_AGE, TIE_BASE])
assert TIE_BASE_FIRST.archive[1][2] == world_reference.str_key("test:base")
assert TIE_OLD_AGE_FIRST.archive[1][2] == world_reference.str_key("test:old_age")


# Scenario "sick": seed 0xff, founder tag `test:sick`, `test:base` with 0.02 per year for
# everyone and `test:dummy_disease` with 0.3 per year only for the homozygous while the
# switch of the hereditary diseases is on (C5). The woman (wild, dummy) and the man (dummy
# twice) of "mortal". Only he can have the disease: it needs about 3.21 years of exposure
# to kill him, `test:base` about 19.6 years (her: about 21.0 years).
SICK_BASE = ("test:base", 0.02)
SICK_DISEASE = ("test:dummy_disease", 0.3, "dummy_homozygous")
SICK_KEYS = [founder_key("test:sick", index) for index in range(2)]
HIS_DISEASE = world_reference.needed_seconds(0xFF, SICK_KEYS[1], *SICK_DISEASE[:2])
HIS_BASE = world_reference.needed_seconds(0xFF, SICK_KEYS[1], *SICK_BASE)
HER_BASE = world_reference.needed_seconds(0xFF, SICK_KEYS[0], *SICK_BASE)
assert 3 * YEAR < HIS_DISEASE < 4 * YEAR < 19 * YEAR < HIS_BASE < HER_BASE


def sick_world(on: bool, switches=(), causes=None) -> world_reference.World:
    """"sick" with the switch `on` at the start and an override block (the switch, the
    time it acts) at each of `switches`."""
    world = world_reference.World(
        0xFF, "test:sick", on, causes or [SICK_BASE, SICK_DISEASE], MORTAL_FOUNDERS
    )
    for at, value in switches:
        world.advance(at - 1)
        world.override(value)
    return world


def death_of(world: world_reference.World, sim: int):
    """(time, key of the cause) of the death of the sim, from the archive."""
    _, left, cause, *_ = world.archive[sim]
    return left, cause


DISEASE_KEY = world_reference.str_key("test:dummy_disease")
BASE_KEY = world_reference.str_key("test:base")

# The switch on from the start: he has an exposure since the start and dies of the
# disease; she has none.
SICK_ON = sick_world(True)
SICK_ON_SAVE_AFTER_START = SICK_ON.save()
SICK_ON.advance(4 * YEAR - 1)
assert death_of(SICK_ON, 2) == (HIS_DISEASE, DISEASE_KEY) and sorted(SICK_ON.sims) == [1]
SICK_ON_SAVE_AFTER_YEAR_3 = SICK_ON.save()

# The switch off: no disease, his exposure stays closed at nothing.
SICK_OFF = sick_world(False)
SICK_OFF_SAVE_AFTER_START = SICK_OFF.save()
SICK_OFF.advance(4 * YEAR - 1)
assert sorted(SICK_OFF.sims) == [1, 2]

# Switched on at the start of year 1: the disease needs its whole budget from then on.
# `test:base` comes first in the list, so his death of it keeps its plan.
SWITCHED_ON = sick_world(False, [(YEAR, True)])
SWITCHED_ON_SAVE = SWITCHED_ON.save()
SWITCHED_ON.advance(5 * YEAR - 1)
assert death_of(SWITCHED_ON, 2) == (YEAR + HIS_DISEASE, DISEASE_KEY)

# The same with the disease first in the list: his death of `test:base` comes after it
# and is planned again by the entry of the override, in the same second.
DISEASE_FIRST = sick_world(False, [(YEAR, True)], [SICK_DISEASE, SICK_BASE])
DISEASE_FIRST_SAVE = DISEASE_FIRST.save()

# On, off at the start of year 1, on again at the start of year 2: the year it was on
# counts, he dies one year after the start plus the budget.
ON_OFF_ON = sick_world(True, [(YEAR, False), (2 * YEAR, True)])
ON_OFF_ON_SAVE = ON_OFF_ON.save()
ON_OFF_ON.advance(6 * YEAR - 1)
assert death_of(ON_OFF_ON, 2) == (YEAR + HIS_DISEASE, DISEASE_KEY)

# Two closed periods: on in years 0 and 2, off in years 1 and 3, on again from year 4.
# Two years count, he dies two years after the start plus the budget.
TWO_PERIODS = sick_world(True, [(YEAR, False), (2 * YEAR, True), (3 * YEAR, False), (4 * YEAR, True)])
TWO_PERIODS.advance(7 * YEAR - 1)
assert death_of(TWO_PERIODS, 2) == (2 * YEAR + HIS_DISEASE, DISEASE_KEY)

# Without a hereditary cause the switch changes no rate: his death of `test:base` keeps
# its plan (his creation entry 3 at the start) when the switch goes on.
NO_DISEASE = sick_world(False, [(YEAR, True)], [SICK_BASE])
NO_DISEASE.advance(HIS_BASE)
assert death_of(NO_DISEASE, 2) == (HIS_BASE, BASE_KEY)

# Switched on close to the end of the clock: a death beyond it is never due, one in its
# last second is (ADR-0002).
LATE = sick_world(False, causes=[SICK_DISEASE])
LATE.sims[2][4][1] = world_reference.MAX_ABS_SECS - 1000
assert LATE.due(2, SICK_DISEASE) is None
LATE.sims[2][4][1] = world_reference.MAX_ABS_SECS - HIS_DISEASE
assert LATE.due(2, SICK_DISEASE) == world_reference.MAX_ABS_SECS

# Off in the very second his death of the disease is due: the input acts first, he
# lives, the budget is used up. On again at the start of year 5: he dies in that second.
OFF_AT_DUE = sick_world(True, [(HIS_DISEASE, False)])
OFF_AT_DUE.advance(5 * YEAR - 1)
assert sorted(OFF_AT_DUE.sims) == [1, 2] and OFF_AT_DUE.sims[2][4] == [HIS_DISEASE, None]
OFF_AT_DUE_SAVE = OFF_AT_DUE.save()
OFF_AT_DUE.override(True)
OFF_AT_DUE.advance(5 * YEAR)
assert death_of(OFF_AT_DUE, 2) == (5 * YEAR, DISEASE_KEY)

# An override that keeps the switch as it is changes nothing but the log.
SAME = sick_world(True, [(YEAR, True)])
UNCHANGED = sick_world(True)
UNCHANGED.advance(YEAR - 1)
assert SAME.projection() == UNCHANGED.projection() and len(SAME.log) == len(UNCHANGED.log) + 1

# A tie of the disease and `test:base`: switched on so that both are due in the second
# of his death of `test:base`. The cause earlier in the list wins.
TIE_SWITCH = HIS_BASE - HIS_DISEASE
BASE_WINS = sick_world(False, [(TIE_SWITCH, True)])
BASE_WINS.advance(HIS_BASE)
assert death_of(BASE_WINS, 2) == (HIS_BASE, BASE_KEY)
DISEASE_WINS = sick_world(False, [(TIE_SWITCH, True)], [SICK_DISEASE, SICK_BASE])
DISEASE_WINS.advance(HIS_BASE)
assert death_of(DISEASE_WINS, 2) == (HIS_BASE, DISEASE_KEY)


# Scenario "partners": seed 0xff, founder tag `test:partners`, the causes of "mortal", a
# woman (wild, dummy) and two men (dummy twice; dummy, wild) born one, two and three
# seconds before the start. `Start` pairs her with the first man (partnership 1). He dies
# after about 0.8 years, which ends it; at the start of year 1 `ArrangePartnership` pairs
# her with the second man (partnership 2, by the input with sequence number 2). He dies
# after about 4.66 years, she after about 4.88.
PARTNERS_FOUNDERS = [
    (-1, FEMALE, genotype(WILD, DUMMY)),
    (-2, MALE, genotype(DUMMY, DUMMY)),
    (-3, MALE, genotype(DUMMY, WILD)),
]
PARTNERS_KEYS = [founder_key("test:partners", index) for index in range(3)]
PARTNERS_DEATHS = [
    min(
        world_reference.needed_seconds(0xFF, key, code, rate) for code, rate in MORTAL_CAUSES
    )
    for key in PARTNERS_KEYS
]
assert PARTNERS_DEATHS[1] < YEAR < PARTNERS_DEATHS[2] < PARTNERS_DEATHS[0] < 5 * YEAR


def partners_world() -> world_reference.World:
    return world_reference.World(
        0xFF, "test:partners", False, MORTAL_CAUSES, PARTNERS_FOUNDERS, [(0, 1)]
    )


PARTNERS = partners_world()
PARTNERS_SAVE_AFTER_START = PARTNERS.save()
PARTNERS_STARTED_ENTRY = PARTNERS.log[4]
PARTNERS.advance(YEAR - 1)
assert PARTNERS.partnerships == {1: [1, 2, 0, PARTNERS_DEATHS[1]]}
PARTNERS.arrange(1, 3)
PARTNERS_ARRANGED_ENTRY = PARTNERS.log[-1]
PARTNERS_SAVE_AFTER_ARRANGE = PARTNERS.save()
PARTNERS.advance(5 * YEAR - 1)
assert PARTNERS.partnerships == {
    1: [1, 2, 0, PARTNERS_DEATHS[1]],
    2: [1, 3, YEAR, PARTNERS_DEATHS[2]],
}
assert not PARTNERS.sims
PARTNERS_SAVE_AFTER_YEAR_4 = PARTNERS.save()


def digest(data: bytes) -> str:
    return f"{xxhash.xxh3_64_intdigest(data, seed=0):016x}"


if __name__ == "__main__":
    for name, data in [
        ("tiny_after_start", TINY_AFTER_START),
        ("tiny_year_0", TINY_YEAR_0),
        ("tiny_after_year_0", TINY_AFTER_YEAR_0),
        ("pair_after_start", PAIR_AFTER_START),
        ("tiny_overridden", TINY_OVERRIDDEN),
        ("mortal_after_start", MORTAL_AFTER_START),
        ("mortal_after_year_1", MORTAL_AFTER_YEAR_1),
    ]:
        print(f"{name} bytes {data.hex()}")
        print(f"{name} hash {digest(data)}")
    print(f"empty_data_hash {EMPTY_DATA_HASH:016x}")
    print(f"header {header(EMPTY_DATA_HASH).hex()}")
    print(f"tiny_save_after_start {TINY_SAVE_AFTER_START.hex()}")
    print(f"empty_save {EMPTY_SAVE.hex()}")
    print(f"tiny_log_hash {digest(b''.join(TINY_LOG))}")
    print(f"tiny_save_after_year_0 {TINY_SAVE_AFTER_YEAR_0.hex()}")
    print(f"tiny_override_entry {TINY_OVERRIDE_ENTRY.hex()}")
    print(f"tiny_save_overridden {TINY_SAVE_OVERRIDDEN.hex()}")
    print(f"mortal_save_after_start {MORTAL_SAVE_AFTER_START.hex()}")
    print(f"mortal_keys {[format(founder_key('test:mortal', index), '016x') for index in range(2)]}")
    cause_keys = [format(world_reference.str_key(code), "016x") for code, _ in MORTAL_CAUSES]
    print(f"cause_keys {cause_keys}")
    print(f"tie_key {TIE_KEY:016x} due {TIE_DUE} old_age_rate {TIE_OLD_AGE[1]!r}")
    print(f"tie_base_first_save {TIE_BASE_FIRST.save().hex()}")
    print(f"tie_old_age_first_save {TIE_OLD_AGE_FIRST.save().hex()}")
    print(f"mortal_deaths_due {MORTAL_DEATHS}")
    for number, line in enumerate(MORTAL.log, start=1):
        print(f"mortal_entry_{number} {line.hex()}")
    print(f"mortal_log_hash {digest(b''.join(MORTAL.log))}")
    print(f"mortal_year_hashes {[format(value, '016x') for value in MORTAL.year_hashes]}")
    print(f"mortal_save_after_year_1 {MORTAL_SAVE_AFTER_YEAR_1.hex()}")
    print(f"sick_keys {[format(key, '016x') for key in SICK_KEYS]} disease_key {DISEASE_KEY:016x}")
    print(f"sick_due his_disease {HIS_DISEASE} his_base {HIS_BASE} her_base {HER_BASE}")
    print(f"sick_on_save_after_start {SICK_ON_SAVE_AFTER_START.hex()}")
    print(f"sick_on_save_after_year_3 {SICK_ON_SAVE_AFTER_YEAR_3.hex()}")
    print(f"sick_on_his_death_entry {SICK_ON.log[-1].hex()}")
    print(f"sick_off_save_after_start {SICK_OFF_SAVE_AFTER_START.hex()}")
    print(f"switched_on_save {SWITCHED_ON_SAVE.hex()}")
    print(f"disease_first_save {DISEASE_FIRST_SAVE.hex()}")
    print(f"on_off_on_save {ON_OFF_ON_SAVE.hex()}")
    print(f"off_at_due_save {OFF_AT_DUE_SAVE.hex()}")
    print(f"tie_switch {TIE_SWITCH}")
    print(f"no_disease_his_death {NO_DISEASE.log[-1].hex()}")
    print(f"partners_keys {[format(key, '016x') for key in PARTNERS_KEYS]}")
    print(f"partners_deaths {PARTNERS_DEATHS}")
    print(f"partners_save_after_start {PARTNERS_SAVE_AFTER_START.hex()}")
    print(f"partners_started_entry {PARTNERS_STARTED_ENTRY.hex()}")
    print(f"partners_arranged_entry {PARTNERS_ARRANGED_ENTRY.hex()}")
    print(f"partners_save_after_arrange {PARTNERS_SAVE_AFTER_ARRANGE.hex()}")
    print(f"partners_save_after_year_4 {PARTNERS_SAVE_AFTER_YEAR_4.hex()}")
    print(f"partners_year_hashes {[format(value, '016x') for value in PARTNERS.year_hashes]}")
    # Log hash: xxh3-64 over the postcard bytes of all entries in order (ADR-0004).
    print(f"log_hash_empty {digest(b'')}")
    print(f"log_hash_entry {digest(SAMPLE_ENTRY)}")
    print(f"log_hash_entry_twice {digest(SAMPLE_ENTRY + SAMPLE_ENTRY)}")

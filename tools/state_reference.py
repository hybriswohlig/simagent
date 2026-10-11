#!/usr/bin/env python3
"""Reference bytes and hashes for the canonical projection of sim-core (ADR-0003
section 5), independent of the Rust code.

The bytes follow the postcard wire format (see postcard_reference.py), the field order
follows ADR-0003 5.2 and ADR-0002 ("Speichern, Laden, Hash"). The hash is xxh3-64 with
seed 0 from the C reference implementation (PyPI xxhash, pinned in requirements.txt).

`Start` has no causes of death any more: every sim dies of `core:senescence`,
`core:accident` or a lethal hereditary disease, with the rates of the data (genetics
specification 7.1, 7.2; ADR-0017 section 2; design note of the death from the data). A
scenario without causes of its own ("tiny", "pair") runs on the repository data (six
hereditary diseases, the rates of 7.2). A scenario that had causes runs on test data
(`world_reference.death_test_data`, design note section 4): the accident rate of both
sexes is the rate of its first cause (`test:base`), the Gompertz `a` of both sexes the
rate of its second (`test:old_age` and the like) with `b` = 0.001, or `a` = 1e-15 and
`b` = 0.001 (senescence practically off) without a second cause; the diseases as the
scenario had them (the dummy disease for "sick", the repository's six otherwise).
"afflicted" tests only the diseases: both causes practically off (accident 1e-9, `a` =
1e-15, `b` = 0.001). Where the rates of section 4 cannot keep the idea of a scenario
("tie", "partners", "blight") it says so and why. The death codes in the log are
`core:senescence` and `core:accident`. With the switch off a sim keeps its afflictions
but no disease is planned to kill it.

`Start` has no fertility any more either (ADR-0017 section 4; design note of the
fertility in the world): a world reads it from its data, and a sim holds `births` right
after `conceptions` (projection version 13, core version 27). The scenarios that gave a
fertility of their own ("clan" and its variants) run on test data with the old values as
the form `window` (design note section 8: 0.25 per month from 18 until 45 years, ten
months, no pause, the wish always 40, no twins; `world_reference.window_fertility`). The
scenario with couples and no fertility ("partners") runs on the repository's fertility
(5.6: her series runs with chance 0 while she is a child); the scenarios without couples
on the repository's fertility as well, which they never read.

Twins (genetics specification 11.3, design note of the twins): a pregnancy holds its one
or two children (projection version 14, core version 28), and every conception entry
names the draw MULTIPLE_BIRTH as its third cause. The test data of every scenario but
"twins" keep twins 0; "twins" is "clan" with the rates 0.3 and 0.7, so every conception
gives twins.

Scenario "tiny": Start with world_seed 0xff, founder tag `test:tiny`, the switch of the
hereditary diseases off and one founder born
one second before the start, female, with the alleles wild (from the mother) and dummy
(from the father) at the dummy locus (ADR-0004). Her lineage key is that of founder 0
of the tag (ADR-0001 section 4). By hand from ADR-0002, ADR-0004 and the genetics
specification 7.1, 7.2:
- the input is applied at now = 0; the year end series gets insertion number 1 (first at
  YEAR - 1, period YEAR), the founder's first birthday insertion number 2 (at YEAR - 1),
  her death of senescence 3 and of accident 4 (after about 85 and 16 990 years);
- in the queue the birthday comes first: same second, domain phase before observers; the
  deaths come last.

Usage: .venv/bin/python tools/state_reference.py
"""

import math

import xxhash

from postcard_reference import (
    DUMMY,
    FEMALE,
    MALE,
    SAMPLE_ENTRY,
    WILD,
    afflictions,
    founder,
    genome,
    genome_of,
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
from rng_reference import chacha_key, founder_key, str_key, unit, words

import data_reference
import world_reference

YEAR = 364 * 86_400
MONTH = world_reference.MONTH

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


PROJECTION_VERSION = world_reference.PROJECTION_VERSION
# The version of the model, so that the two references never drift apart.
CORE_VERSION = world_reference.CORE_VERSION

SENESCENCE, ACCIDENT = "core:senescence", "core:accident"
SENESCENCE_KEY, ACCIDENT_KEY = str_key(SENESCENCE), str_key(ACCIDENT)

# The rates of the repository's data by hand from the genetics specification 7.2: the
# Gompertz `a` (W7.2b, W7.2d) and `b` (W7.2c, W7.2e) and the Makeham term (K7.1a,
# K7.1b), per sex.
SPEC_RATES = {FEMALE: (3.25e-6, 0.1182, 1.5e-4), MALE: (8.73e-6, 0.1118, 2.0e-4)}


def hand_due(key: int, born: int, code: str, rates: tuple):
    """When a founder (seed 0xff, origin the start, B19) dies of the cause if nothing else
    happens first, by hand from the genetics specification 7.1 with Python's math: the age
    `x = ln(e^(b·x0) + b·B / (a·m)) / b` (the form of the specification, `m` = 1 without a
    trait of longevity, 11.1) or `x0 + B / λ` from the age `x0` at the start, due
    `ceil((x - x0) · YEAR)` seconds after the start, never at 100 000 years or more.
    `rates` is (a, b, λ) of the founder's sex."""
    a, b, per_year = rates
    budget = world_reference.budget_of(0xFF, key, code)
    x0 = (0 - born) / 31_449_600.0
    if code == SENESCENCE:
        x = math.log(math.exp(b * x0) + b * budget / a) / b
    else:
        x = x0 + budget / per_year
    if x - x0 >= 100_000.0:
        return None
    return math.ceil((x - x0) * 31_449_600.0)


def death(time: int, insertion: int, sim: int, code: str) -> bytes:
    """A planned death in the queue: Event::Death { sim, cause } (variant 2)."""
    return queue_entry(time, insertion, variant(2, varint(sim), varint(str_key(code))), None)


def sim_state(born: int, key: int, sex: bytes, genes: tuple, afflicted=()) -> bytes:
    """Sim { born, lineage_key, sex, genome, afflictions, conceptions, births, parents }:
    the afflictions as triples (disease, onset, exposure), none but for a sim with the
    dummy allele twice (`core:disease_ar`, recessive, penetrance 1, onset 0); no
    conceptions and no births yet (`births` right after `conceptions`, design note of the
    fertility in the world, section 1); a founder has no parents."""
    return (
        signed(born)
        + lineage_key(key)
        + sex
        + genome(sex, genes)
        + afflictions(list(afflicted))
        + varint(0)  # conceptions
        + varint(0)  # births
        + NONE
    )


TINY_KEY = founder_key("test:tiny", 0)
TINY_GENES = genotype(WILD, DUMMY)
# Her deaths, planned by her creation entry 2 at the start (insertion numbers 3 and 4).
TINY_DUE = {code: hand_due(TINY_KEY, -1, code, SPEC_RATES[FEMALE]) for code in (SENESCENCE, ACCIDENT)}
assert 80 * YEAR < TINY_DUE[SENESCENCE] < 90 * YEAR < TINY_DUE[ACCIDENT] < 100_000 * YEAR
TINY_DEATHS = [death(TINY_DUE[SENESCENCE], 3, 1, SENESCENCE), death(TINY_DUE[ACCIDENT], 4, 1, ACCIDENT)]


def projection(
    closed_through: int, queue: list, next_insertion: int, diseases: bool = False
) -> bytes:
    return (
        varint(PROJECTION_VERSION)
        + some(varint(0xFF))  # world_seed
        + some(world_params(diseases))
        + signed(closed_through)
        + varint(2)  # next SimId
        + varint(1)  # next PartnershipId
        + seq(queue)
        + varint(next_insertion)
        + seq([varint(1) + sim_state(-1, TINY_KEY, FEMALE, TINY_GENES)])  # sims: id 1
        + seq([])  # partnerships
        + seq([])  # pregnancies
        + seq([])  # archive
    )


# Right after Start: nothing closed yet.
TINY_AFTER_START = projection(
    -1,
    [
        queue_entry(YEAR - 1, 2, birthday(1), None),
        queue_entry(YEAR - 1, 1, year_end(), YEAR),
    ]
    + TINY_DEATHS,
    5,
)

# What the year end observer of year 0 sees at t = YEAR - 1: the clock stands at t - 1
# (ADR-0002), the birthday of this second has run and planned the next one (insertion
# number 5), the year end series already holds its next occurrence.
TINY_YEAR_0 = projection(
    YEAR - 2,
    [
        queue_entry(2 * YEAR - 1, 5, birthday(1), None),
        queue_entry(2 * YEAR - 1, 1, year_end(), YEAR),
    ]
    + TINY_DEATHS,
    6,
)

# After advance_until(YEAR - 1): the same state with the second closed.
TINY_AFTER_YEAR_0 = projection(
    YEAR - 1,
    [
        queue_entry(2 * YEAR - 1, 5, birthday(1), None),
        queue_entry(2 * YEAR - 1, 1, year_end(), YEAR),
    ]
    + TINY_DEATHS,
    6,
)


# Scenario "pair": seed 0xff, founder tag `test:pair`, a woman born two seconds before
# the start with the alleles wild and dummy, and a man born one second before it with
# dummy twice. Right after Start: her first birthday (insertion number 2) is due at
# YEAR - 2, her deaths of senescence and accident (3, 4) after about 92 and 436 years;
# his first birthday (5) at YEAR - 1, in the same second as the year end, his deaths (6,
# 7) after about 88 and 584 years. He has `core:disease_ar` (recessive, penetrance 1, so
# the draw always says yes; onset fixed at 0): an affliction with nothing used, the
# switch off.
AR_KEY = str_key("core:disease_ar")
PAIR_KEYS = [founder_key("test:pair", index) for index in range(2)]
PAIR_DUE = [
    {code: hand_due(key, born, code, SPEC_RATES[sex]) for code in (SENESCENCE, ACCIDENT)}
    for key, born, sex in ((PAIR_KEYS[0], -2, FEMALE), (PAIR_KEYS[1], -1, MALE))
]
PAIR_DEATHS = sorted(
    [
        (PAIR_DUE[0][SENESCENCE], death(PAIR_DUE[0][SENESCENCE], 3, 1, SENESCENCE)),
        (PAIR_DUE[0][ACCIDENT], death(PAIR_DUE[0][ACCIDENT], 4, 1, ACCIDENT)),
        (PAIR_DUE[1][SENESCENCE], death(PAIR_DUE[1][SENESCENCE], 6, 2, SENESCENCE)),
        (PAIR_DUE[1][ACCIDENT], death(PAIR_DUE[1][ACCIDENT], 7, 2, ACCIDENT)),
    ]
)
assert 2 * YEAR < PAIR_DEATHS[0][0] < PAIR_DEATHS[1][0] < PAIR_DEATHS[2][0] < PAIR_DEATHS[3][0]
PAIR_AFTER_START = (
    varint(PROJECTION_VERSION)
    + some(varint(0xFF))
    + some(world_params(False))
    + signed(-1)
    + varint(3)  # next SimId
    + varint(1)  # next PartnershipId
    + seq(
        [
            queue_entry(YEAR - 2, 2, birthday(1), None),
            queue_entry(YEAR - 1, 5, birthday(2), None),
            queue_entry(YEAR - 1, 1, year_end(), YEAR),
        ]
        + [entry for _, entry in PAIR_DEATHS]
    )
    + varint(8)
    + seq(
        [
            varint(1) + sim_state(-2, PAIR_KEYS[0], FEMALE, genotype(WILD, DUMMY)),
            # He has the dummy allele twice: nothing used, the switch is off.
            varint(2) + sim_state(-1, PAIR_KEYS[1], MALE, genotype(DUMMY, DUMMY), [(AR_KEY, 0.0, (0.0, None))]),
        ]
    )
    + seq([])  # partnerships
    + seq([])  # pregnancies
    + seq([])  # archive
)

# Save file of ADR-0006: header, projection, exceptions.
DATA_HASH = world_reference.DATA_HASH


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
            seq([founder(-1, FEMALE, TINY_GENES)]),
            seq([]),  # couples
        )
    ]
)

# Right after Start (the log taken): plan references in queue order (the birthday was
# planned by the founder's entry 2, the year end series by `core:started`, entry 1, her
# two deaths by her entry 2), the next EventId is 3, no year hashes yet.
TINY_SAVE_AFTER_START = (
    header(DATA_HASH)
    + TINY_AFTER_START
    + seq([plan_ref(2, 0), plan_ref(1, 0), plan_ref(2, 0), plan_ref(2, 0)])
    + varint(3)
    + TINY_INPUT_LOG
    + seq([])
)

# A world that never started: nothing but the clock and the counters.
EMPTY_PROJECTION = (
    varint(PROJECTION_VERSION)
    + NONE  # world_seed
    + NONE  # world_params
    + signed(-1)
    + varint(1)  # next SimId
    + varint(1)  # next PartnershipId
    + seq([])
    + varint(1)
    + seq([])  # sims
    + seq([])  # partnerships
    + seq([])  # pregnancies
    + seq([])  # archive
)
EMPTY_SAVE = header(DATA_HASH) + EMPTY_PROJECTION + seq([]) + varint(1) + seq([]) + seq([])


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
    ]
    + TINY_DEATHS,
    5,
    diseases=True,
)
TINY_SAVE_OVERRIDDEN = (
    header(DATA_HASH)
    + TINY_OVERRIDDEN
    + seq([plan_ref(2, 0), plan_ref(1, 0), plan_ref(2, 0), plan_ref(2, 0)])
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
                seq([founder(-1, FEMALE, TINY_GENES)]),
                seq([]),  # couples
            ),
            signed(-1) + varint(2) + variant(1, seq([param_change(True)])),
        ]
    )
    + seq([])
)

# The save at the end of year 0: the birthday of that second planned the next one
# (entry 3 at YEAR - 1), the year end series keeps the plan of `core:started`, her deaths
# that of her entry 2; the next EventId is 4; one year hash.
TINY_YEAR_0_HASH = xxhash.xxh3_64_intdigest(TINY_YEAR_0, seed=0)
TINY_SAVE_AFTER_YEAR_0 = (
    header(DATA_HASH)
    + TINY_AFTER_YEAR_0
    + seq([plan_ref(3, YEAR - 1), plan_ref(1, 0), plan_ref(2, 0), plan_ref(2, 0)])
    + varint(4)
    + TINY_INPUT_LOG
    + seq([signed(0) + varint(TINY_YEAR_0_HASH)])
)


# ---------------------------------------------------------------------------------------
# The model of tools/world_reference.py gives the same bytes as the derivations by hand
# above, and the bytes of the scenarios with death.

TINY_FOUNDERS = [(-1, FEMALE, TINY_GENES)]


def tiny_world() -> world_reference.World:
    return world_reference.World(0xFF, "test:tiny", False, TINY_FOUNDERS)


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
        [(-2, FEMALE, genotype(WILD, DUMMY)), (-1, MALE, genotype(DUMMY, DUMMY))],
    )
    assert pair.projection() == PAIR_AFTER_START
    assert world_reference.EMPTY_SAVE == EMPTY_SAVE


check_the_model_on_tiny()

# Scenario "mortal": seed 0xff, founder tag `test:mortal`, the switch off, on the test
# data of its old causes (`test:base` 0.5 and `test:old_age` 0.05 per year): accident
# 0.5 per year, senescence `a` = 0.05 and `b` = 0.001, for both sexes, the repository's
# diseases. A woman born one second before the start (wild, dummy) and a man born two
# seconds before it (dummy twice). Her budgets of senescence and accident are about 0.41
# and 2.15: she dies of accident after about 4.29 years, her death of senescence (after
# 8.25 years) and her next birthday are cancelled. His are about 0.33 and 2.83: he dies
# of accident after about 5.66 years, his death of senescence (after 6.60 years) and his
# next birthday are cancelled. Both die of accident (the new budgets; with the old ones
# each cause won once); the run goes to the end of year 5 so that both die in it.
MORTAL_DATA = world_reference.death_test_data(0.5, 0.05)
MORTAL_RATES = (0.05, 0.001, 0.5)
MORTAL_FOUNDERS = [(-1, FEMALE, genotype(WILD, DUMMY)), (-2, MALE, genotype(DUMMY, DUMMY))]
MORTAL_KEYS = [founder_key("test:mortal", index) for index in range(2)]


def mortal_world() -> world_reference.World:
    return world_reference.World(0xFF, "test:mortal", False, MORTAL_FOUNDERS, data=MORTAL_DATA)


MORTAL = mortal_world()
MORTAL_AFTER_START = MORTAL.projection()
MORTAL_SAVE_AFTER_START = MORTAL.save()
# When each founder dies of each cause (seconds after the start), in the order
# (her senescence, her accident, his senescence, his accident).
MORTAL_DEATHS = [MORTAL.cause_due(sim, code) for sim in (1, 2) for code in (SENESCENCE, ACCIDENT)]
assert MORTAL_DEATHS == [
    hand_due(key, born, code, MORTAL_RATES)
    for key, (born, _, _) in zip(MORTAL_KEYS, MORTAL_FOUNDERS)
    for code in (SENESCENCE, ACCIDENT)
]
assert 4 * YEAR < MORTAL_DEATHS[1] < 5 * YEAR < MORTAL_DEATHS[3] < 6 * YEAR < MORTAL_DEATHS[2] < MORTAL_DEATHS[0]
MORTAL.advance(6 * YEAR - 1)
MORTAL_AFTER_YEAR_5 = MORTAL.projection()
MORTAL_SAVE_AFTER_YEAR_5 = MORTAL.save()
assert not MORTAL.sims and sorted(MORTAL.archive) == [1, 2]
assert [MORTAL.archive[sim][1:3] for sim in (1, 2)] == [(MORTAL_DEATHS[1], ACCIDENT_KEY), (MORTAL_DEATHS[3], ACCIDENT_KEY)]
# started, two founders, her four birthdays, his five, two deaths
assert len(MORTAL.log) == 14


# Scenario "tie": seed 0xff, founder tag `test:tie`, the woman of "tiny" (born one second
# before the start). Her senescence and her accident are due in the same second; the cause
# earlier in the list, senescence, wins: its event was planned first. The test data:
# accident 0.2 per year and senescence `b` = 0.001 for both sexes, the Gompertz `a` chosen
# so that both are due in that second (the middle of it: `a = b · B · e^(−b·x0) /
# (e^(b·T) − 1)` with `T` half a second before the end of the second, in 60 digits).
# The rate of the old first cause (5 per year) cannot tie: with her budgets (senescence
# about 3.03, accident about 0.75) it needs `a` = 20.15, beyond the range of `a`
# (mortality::GOMPERTZ_A_RANGE, at most 1); 0.2 per year gives `a` = 0.80. She dies after
# about 3.76 years.
TIE_KEY = founder_key("test:tie", 0)
TIE_ACCIDENT = 0.2
TIE_SENESCENCE_A = 0.8046949992648321
TIE_DATA = world_reference.death_test_data(TIE_ACCIDENT, TIE_SENESCENCE_A)
TIE_RATES = (TIE_SENESCENCE_A, 0.001, TIE_ACCIDENT)
TIE_DUE = hand_due(TIE_KEY, -1, ACCIDENT, TIE_RATES)
assert hand_due(TIE_KEY, -1, SENESCENCE, TIE_RATES) == TIE_DUE
assert 3 * YEAR < TIE_DUE < 4 * YEAR


def tie_world() -> world_reference.World:
    return world_reference.World(0xFF, "test:tie", False, TINY_FOUNDERS, data=TIE_DATA)


TIE = tie_world()
assert [TIE.cause_due(1, code) for code in (SENESCENCE, ACCIDENT)] == [TIE_DUE, TIE_DUE]
TIE_SAVE_AFTER_START = TIE.save()
TIE.advance(4 * YEAR - 1)
assert TIE.archive[1][1:3] == (TIE_DUE, SENESCENCE_KEY)
assert [event.kind for event in TIE.queue] == [world_reference.YEAR_END_KIND]
TIE_SAVE = TIE.save()


# Scenario "sick": seed 0xff, founder tag `test:sick`, on the test data of its old cause
# (`test:base` 0.02 per year) with the dummy disease: accident 0.02 per year, senescence
# practically off (`a` = 1e-15, `b` = 0.001), and as the only disease `test:dummy_disease`
# on `core:disease_ar`, penetrance 1, onset 0, 0.3 per year (design note of the diseases,
# section 6; the old cause for the homozygous acted from the origin on). The woman (wild,
# dummy) and the man (dummy twice) of "mortal". Only he has the disease: an affliction
# with onset 0 whose exposure runs while the switch is on. Its budget lasts about 3.21
# years of exposure, his accident about 58.8 years (hers about 173 years); senescence
# would take some 23 000 years.
DUMMY_DISEASE = "test:dummy_disease"
SICK_DATA = world_reference.death_test_data(0.02, diseases=[world_reference.dummy_disease(0.0)])
SICK_RATES = (1e-15, 0.001, 0.02)
SICK_KEYS = [founder_key("test:sick", index) for index in range(2)]
HIS_BUDGET = world_reference.budget_of(0xFF, SICK_KEYS[1], DUMMY_DISEASE)
HIS_ACCIDENT = hand_due(SICK_KEYS[1], -2, ACCIDENT, SICK_RATES)
HER_ACCIDENT = hand_due(SICK_KEYS[0], -1, ACCIDENT, SICK_RATES)
HIS_BORN = -2


def his_disease_death(since: int, used: float = 0.0) -> int:
    """By hand from the design note (section 4) for him (born two seconds before the
    start, onset 0, a constant 0.3 per year): with his exposure running since `since`
    the age at death is `x0 + max(B - used, 0) / 0.3` with `x0` his age at `since`, and
    the death is due `ceil((a - x0) * YEAR)` seconds after `since`."""
    x0 = (since - HIS_BORN) / 31_449_600.0
    age = x0 + max(HIS_BUDGET - used, 0.0) / 0.3
    return since + math.ceil((age - x0) * 31_449_600.0)


# Exposed from the start on: his death of the disease.
HIS_DISEASE = his_disease_death(0)
assert 3 * YEAR < HIS_DISEASE < 4 * YEAR < 50 * YEAR < HIS_ACCIDENT < HER_ACCIDENT


def sick_world(on: bool, switches=(), data=SICK_DATA) -> world_reference.World:
    """"sick" with the switch `on` at the start and an override block (the switch, the
    time it acts) at each of `switches`."""
    world = world_reference.World(0xFF, "test:sick", on, MORTAL_FOUNDERS, data=data)
    for at, value in switches:
        world.advance(at - 1)
        world.override(value)
    return world


def death_of(world: world_reference.World, sim: int):
    """(time, key of the cause) of the death of the sim, from the archive."""
    _, left, cause, *_ = world.archive[sim]
    return left, cause


def his_exposure(world: world_reference.World):
    """[used, since] of his one affliction (living or archived)."""
    (affliction,) = world.sims[2][4] if 2 in world.sims else world.archive[2][6]
    return affliction.exposure


DISEASE_KEY = world_reference.str_key(DUMMY_DISEASE)

# The switch on from the start: he is exposed since the start and dies of the disease;
# she has no affliction. Each founder has its deaths of senescence and accident planned.
SICK_ON = sick_world(True)
assert [SICK_ON.cause_due(sim, ACCIDENT) for sim in (1, 2)] == [HER_ACCIDENT, HIS_ACCIDENT]
assert [[a.triple() for a in SICK_ON.sims[sim][4]] for sim in (1, 2)] == [[], [(DISEASE_KEY, 0.0, (0.0, 0))]]
SICK_ON_SAVE_AFTER_START = SICK_ON.save()
SICK_ON.advance(4 * YEAR - 1)
assert death_of(SICK_ON, 2) == (HIS_DISEASE, DISEASE_KEY) and sorted(SICK_ON.sims) == [1]
SICK_ON_SAVE_AFTER_YEAR_3 = SICK_ON.save()

# The switch off: no disease, his exposure stays at nothing used, not running.
SICK_OFF = sick_world(False)
SICK_OFF_SAVE_AFTER_START = SICK_OFF.save()
SICK_OFF.advance(4 * YEAR - 1)
assert sorted(SICK_OFF.sims) == [1, 2] and his_exposure(SICK_OFF) == [0.0, None]

# Switched on at the start of year 1: the disease needs its whole budget from then on,
# planned by the entry of the override. His deaths of senescence and accident keep their
# plans.
SWITCHED_ON = sick_world(False, [(YEAR, True)])
SWITCHED_ON_SAVE = SWITCHED_ON.save()
SWITCHED_ON.advance(5 * YEAR - 1)
SWITCHED_ON_DEATH = his_disease_death(YEAR)
assert abs(SWITCHED_ON_DEATH - (YEAR + HIS_DISEASE)) <= 1
assert death_of(SWITCHED_ON, 2) == (SWITCHED_ON_DEATH, DISEASE_KEY)

# On, off at the start of year 1, on again at the start of year 2: the year it was on
# counts (0.3 times his age at the switch minus his age at the start), he dies about one
# year after the start plus the budget.
ON_OFF_ON_USED = world_reference.disease_hazard_used(
    (0.3, 0.0, 0.3), (0 - HIS_BORN) / 31_449_600.0, (YEAR - HIS_BORN) / 31_449_600.0
)
ON_OFF_ON = sick_world(True, [(YEAR, False), (2 * YEAR, True)])
assert his_exposure(ON_OFF_ON) == [ON_OFF_ON_USED, 2 * YEAR]
ON_OFF_ON_SAVE = ON_OFF_ON.save()
ON_OFF_ON.advance(6 * YEAR - 1)
ON_OFF_ON_DEATH = his_disease_death(2 * YEAR, ON_OFF_ON_USED)
assert abs(ON_OFF_ON_DEATH - (YEAR + HIS_DISEASE)) <= 1
assert death_of(ON_OFF_ON, 2) == (ON_OFF_ON_DEATH, DISEASE_KEY)

# Two closed periods: on in years 0 and 2, off in years 1 and 3, on again from year 4.
# Two years count, he dies about two years after the start plus the budget.
TWO_PERIODS = sick_world(True, [(YEAR, False), (2 * YEAR, True), (3 * YEAR, False), (4 * YEAR, True)])
TWO_PERIODS_USED = ON_OFF_ON_USED + world_reference.disease_hazard_used(
    (0.3, 0.0, 0.3), (2 * YEAR - HIS_BORN) / 31_449_600.0, (3 * YEAR - HIS_BORN) / 31_449_600.0
)
assert his_exposure(TWO_PERIODS) == [TWO_PERIODS_USED, 4 * YEAR]
TWO_PERIODS.advance(7 * YEAR - 1)
TWO_PERIODS_DEATH = his_disease_death(4 * YEAR, TWO_PERIODS_USED)
assert abs(TWO_PERIODS_DEATH - (2 * YEAR + HIS_DISEASE)) <= 1
assert death_of(TWO_PERIODS, 2) == (TWO_PERIODS_DEATH, DISEASE_KEY)

# On the test data with the repository's six diseases instead of the dummy one (accident
# 0.02 per year, senescence practically off) the switch plans only the diseases of the
# data: he has `core:disease_ar` (recessive, onset 0, two steps), and switched on at the
# start of year 1 its death is planned by the entry of the override, after about 20.5
# years; his death of accident keeps its plan (his creation entry 3 at the start, after
# about 58.8 years) and comes later. He dies of `core:disease_ar`.
NO_DISEASE_DATA = world_reference.death_test_data(0.02)
NO_DISEASE = sick_world(False, [(YEAR, True)], data=NO_DISEASE_DATA)
assert NO_DISEASE.cause_due(2, ACCIDENT) == HIS_ACCIDENT
(HIS_AR,) = NO_DISEASE.sims[2][4]
NO_DISEASE_HIS_AR_DEATH = NO_DISEASE.disease_due(2, HIS_AR)
assert 20 * YEAR < NO_DISEASE_HIS_AR_DEATH < HIS_ACCIDENT
NO_DISEASE_OVERRIDE_ENTRY = len(NO_DISEASE.log)
assert [
    (event.time, event.plan)
    for event in NO_DISEASE.queue
    if event.kind == world_reference.DEATH_KIND and event.subject == (world_reference.SIM_SUBJECT, 2)
] == [
    (NO_DISEASE.cause_due(2, SENESCENCE), (3, 0)),
    (HIS_ACCIDENT, (3, 0)),
    (NO_DISEASE_HIS_AR_DEATH, (NO_DISEASE_OVERRIDE_ENTRY, YEAR)),
]
NO_DISEASE.advance(NO_DISEASE_HIS_AR_DEATH)
assert death_of(NO_DISEASE, 2) == (NO_DISEASE_HIS_AR_DEATH, AR_KEY)

# Exposed from close to the end of the clock: a death beyond it is never due, one in its
# last second is (ADR-0002).
LATE = sick_world(False)
(LATE_AFFLICTION,) = LATE.sims[2][4]
LATE_AFFLICTION.exposure[1] = world_reference.MAX_ABS_SECS - 1000
assert LATE.disease_due(2, LATE_AFFLICTION) is None
LATE_EDGE = []
for since in range(world_reference.MAX_ABS_SECS - HIS_DISEASE - 3, world_reference.MAX_ABS_SECS - HIS_DISEASE + 4):
    LATE_AFFLICTION.exposure[1] = since
    LATE_EDGE.append(LATE.disease_due(2, LATE_AFFLICTION))
assert world_reference.MAX_ABS_SECS in LATE_EDGE and LATE_EDGE[-1] is None

# Off in the very second his death of the disease is due: the input acts first, he
# lives, the budget is used up. On again at the start of year 5: he dies in that second.
OFF_AT_DUE = sick_world(True, [(HIS_DISEASE, False)])
OFF_AT_DUE.advance(5 * YEAR - 1)
OFF_AT_DUE_USED = world_reference.disease_hazard_used(
    (0.3, 0.0, 0.3), (0 - HIS_BORN) / 31_449_600.0, (HIS_DISEASE - HIS_BORN) / 31_449_600.0
)
assert OFF_AT_DUE_USED >= HIS_BUDGET
assert sorted(OFF_AT_DUE.sims) == [1, 2] and his_exposure(OFF_AT_DUE) == [OFF_AT_DUE_USED, None]
OFF_AT_DUE_SAVE = OFF_AT_DUE.save()
OFF_AT_DUE.override(True)
OFF_AT_DUE.advance(5 * YEAR)
assert death_of(OFF_AT_DUE, 2) == (5 * YEAR, DISEASE_KEY)

# An override that keeps the switch as it is changes nothing but the log.
SAME = sick_world(True, [(YEAR, True)])
UNCHANGED = sick_world(True)
UNCHANGED.advance(YEAR - 1)
assert SAME.projection() == UNCHANGED.projection() and len(SAME.log) == len(UNCHANGED.log) + 1

# A tie of the disease and the accident: switched on so that both are due in the second
# of his death of accident. Accident comes before the diseases in the list, so it wins.
TIE_SWITCHES = [
    at
    for at in range(HIS_ACCIDENT - HIS_DISEASE - 3, HIS_ACCIDENT - HIS_DISEASE + 4)
    if his_disease_death(at) == HIS_ACCIDENT
]
TIE_SWITCH = TIE_SWITCHES[0]
ACCIDENT_WINS = sick_world(False, [(TIE_SWITCH, True)])
assert sorted(event.time for event in ACCIDENT_WINS.queue if event.kind == world_reference.DEATH_KIND).count(
    HIS_ACCIDENT
) == 2
ACCIDENT_WINS.advance(HIS_ACCIDENT)
assert death_of(ACCIDENT_WINS, 2) == (HIS_ACCIDENT, ACCIDENT_KEY)

# Save, load and go on across the switch (crates/sim-core/tests/disease.rs): on, off at
# the start of year 1, on at year 2, off at year 3, on at year 4, off at year 6; he dies
# of the disease in the period from year 4 on.
SAVE_LOAD = sick_world(True, [(YEAR, False), (2 * YEAR, True), (3 * YEAR, False), (4 * YEAR, True), (6 * YEAR, False)])
SAVE_LOAD.advance(7 * YEAR - 1)
assert death_of(SAVE_LOAD, 2) == (TWO_PERIODS_DEATH, DISEASE_KEY)


# ---------------------------------------------------------------------------------------
# Fertility by hand (genetics specification 5.6 and 11.3; design note of the fertility in
# the world, sections 3 to 6), straight from the draws of tools/rng_reference.py and
# Python's math, not from the model: a woman's wish and the months of her conceptions
# with one partner.

CONCEPTION_ATTEMPT_TAG, FERTILITY_WISH_TAG = 3, 16  # tests/golden/rng_tags.txt, 11.7


def hand_wish(seed: int, key: int, shape: list, tilt: float) -> int:
    """Her wish (11.3): `u` = unit of the draw FERTILITY_WISH at her lineage key (occasion
    0, detail 0), `P_n ∝ p_n · e^(α · n)`, the smallest `n` with `u < P_0 + … + P_n`, the
    last `n` if none."""
    u = unit(words(chacha_key(seed, FERTILITY_WISH_TAG, key, 0, 0))[0])
    weights = [p * math.exp(tilt * n) for n, p in enumerate(shape)]
    total, acc = sum(weights), 0.0
    for n, weight in enumerate(weights):
        acc += weight / total
        if u < acc:
            return n
    return len(shape) - 1


def hand_conceptions(seed: int, her: int, him: int, first: int, chance_at, pause: int, pregnancy: int, wish_: int, last: int) -> list:
    """(month of the conception, month of the birth) of a woman with one partner by hand
    (design note sections 4 to 6): her series begins in month `first`; the attempt in
    month `k` succeeds if `unit(draw) < chance_at(k)` (CONCEPTION_ATTEMPT at her key,
    occasion `k`, detail his key); the birth is `pregnancy` months later; with fewer
    births than `wish_` her next series begins `max(pause, 1)` months after the birth.
    Nothing after month `last`."""
    found, month = [], first
    while month <= last and len(found) < wish_:
        word = words(chacha_key(seed, CONCEPTION_ATTEMPT_TAG, her, month, him))[0]
        if unit(word) < chance_at(month):
            found.append((month, month + pregnancy))
            month += pregnancy + max(pause, 1)
        else:
            month += 1
    return found


# Scenario "partners": seed 0xff, founder tag `test:partners`, a woman (wild, dummy) and
# two men (dummy twice; dummy, wild) born one, two and three seconds before the start.
# `Start` pairs her with the first man (partnership 1). He dies of senescence after about
# 0.2 years, which ends it; at the start of year 1 `ArrangePartnership` pairs her with the
# second man (partnership 2, by the input with sequence number 2). She dies of accident
# after about 1.86 years, which ends it; he lives on (senescence after about 13.0 years).
# The test data: accident 0.02 per year, senescence `a` = 0.5 and `b` = 0.001 for both
# sexes (the data of "fever"), the repository's diseases and fertility (design note of the
# fertility in the world, section 8). The rates of its old causes
# (those of "mortal": accident 0.5, `a` = 0.05) cannot keep the idea: with the new budgets
# she would die first, after 0.07 years, and nobody could be paired with her at year 1.
# Fertility (5.6, 11.3): her wish is 3, so each partnership plans her monthly series, the
# first from month 1 (the first month start after the start), ended by his death in month
# 2.6 after the attempts of months 1 and 2; the second from month 14 (the first month start
# after the second YEAR in which the arrangement acts), ended by her death in month 24.2.
# She is a baby (her menarche about 13.1 years, at least `θ − 1` = 11.3), so her factor
# and every chance is 0: thirteen attempts draw and fail, nobody conceives.
PARTNERS_FOUNDERS = [
    (-1, FEMALE, genotype(WILD, DUMMY)),
    (-2, MALE, genotype(DUMMY, DUMMY)),
    (-3, MALE, genotype(DUMMY, WILD)),
]
PARTNERS_KEYS = [founder_key("test:partners", index) for index in range(3)]
PARTNERS_DATA = world_reference.death_test_data(0.02, 0.5)
PARTNERS_RATES = (0.5, 0.001, 0.02)
PARTNERS_DEATHS = [
    min(hand_due(key, born, code, PARTNERS_RATES) for code in (SENESCENCE, ACCIDENT))
    for key, (born, _, _) in zip(PARTNERS_KEYS, PARTNERS_FOUNDERS)
]
assert PARTNERS_DEATHS[1] < YEAR < PARTNERS_DEATHS[0] < 2 * YEAR < 5 * YEAR < PARTNERS_DEATHS[2]


def partners_world() -> world_reference.World:
    return world_reference.World(0xFF, "test:partners", False, PARTNERS_FOUNDERS, [(0, 1)], data=PARTNERS_DATA)


PARTNERS = partners_world()
PARTNERS_WISH = hand_wish(0xFF, PARTNERS_KEYS[0], [0.10, 0.15, 0.40, 0.25, 0.10], 0.311)
assert PARTNERS.wish_of(1) == PARTNERS_WISH == 3
assert PARTNERS.pending_series(1) == MONTH
PARTNERS_SAVE_AFTER_START = PARTNERS.save()
# Started, three founders, then the partnership.
PARTNERS_STARTED_ENTRY = PARTNERS.log[4]
PARTNERS.advance(YEAR - 1)
assert PARTNERS.partnerships == {1: [1, 2, 0, PARTNERS_DEATHS[1]]}
assert PARTNERS.archive[2][2] == SENESCENCE_KEY
assert PARTNERS.pending_series(1) is None
PARTNERS.arrange(1, 3)
PARTNERS_ARRANGED_ENTRY = PARTNERS.log[-1]
assert PARTNERS.pending_series(1) == 14 * MONTH
PARTNERS_SAVE_AFTER_ARRANGE = PARTNERS.save()
PARTNERS.advance(5 * YEAR - 1)
assert PARTNERS.partnerships == {
    1: [1, 2, 0, PARTNERS_DEATHS[1]],
    2: [1, 3, YEAR, PARTNERS_DEATHS[0]],
}
assert sorted(PARTNERS.sims) == [3] and PARTNERS.archive[1][2] == ACCIDENT_KEY
assert [(t // MONTH, p, success) for t, _, _, p, success in PARTNERS.attempts] == [
    (month, 0.0, False) for month in [1, 2] + list(range(14, 25))
]
assert PARTNERS.archive[1][7:9] == (0, 0) and PARTNERS_DEATHS[1] < 3 * MONTH and 24 * MONTH < PARTNERS_DEATHS[0] < 25 * MONTH
PARTNERS_SAVE_AFTER_YEAR_4 = PARTNERS.save()


# Scenario "clan": seed 0xff, founder tag `test:clan`, on the test data of its old cause
# (`test:base` 0.02 per year): accident 0.02 per year, senescence practically off, the
# repository's diseases, and its old fertility as data (design note of the fertility in
# the world, section 8): the form `window` from 18 until 45 years, 0.25 per month, ten
# months of pregnancy, no pause, the wish always 40, no twins. Three founder couples: a
# woman of 20 (wild, dummy) and a man of 22 (dummy, wild); a girl of 12 (dummy twice) and
# a man of 30 (wild twice); a woman and a man of 46 (wild twice). The first woman's
# series begins in month 1 (the first month start after the start); she conceives in
# month 5 and has the child (sim 7) in month 15; without a pause her series begins again
# in month 16, and by the end of year 7 she has conceived eight times (months 5, 17, 28,
# 42, 59, 74, 90, 101), borne seven children (births 7) and is pregnant with the eighth.
# The girl's series begins in month 1 as well and draws with chance 0 until her 18th
# birthday (month 78), then with 0.25; she dies of accident in month 83.9 without having
# conceived, which ends it. The first child dies of accident in month 80.5, the man of 46
# in month 98.7. The woman of 46 is past the end of the window (45 years): no series is
# planned for her. Every woman's wish is 40.
CLAN_FOUNDERS = [
    (-20 * YEAR, FEMALE, genotype(WILD, DUMMY)),
    (-22 * YEAR, MALE, genotype(DUMMY, WILD)),
    (-12 * YEAR, FEMALE, genotype(DUMMY, DUMMY)),
    (-30 * YEAR, MALE, genotype(WILD, WILD)),
    (-46 * YEAR, FEMALE, genotype(WILD, WILD)),
    (-46 * YEAR, MALE, genotype(WILD, WILD)),
]
CLAN_KEYS = [founder_key("test:clan", index) for index in range(6)]
CLAN_COUPLES = [(0, 1), (2, 3), (4, 5)]
CLAN_FERTILITY = (0.25, 18, 45, 10)
CLAN_DATA = world_reference.death_test_data(0.02, fertility=CLAN_FERTILITY)
# By hand: the first woman's conceptions over the eight years (20 years old at the start,
# the window open), and the girl's up to her death in month 83 (12 at the start, chance 0
# before month 78).
CLAN_HAND = hand_conceptions(0xFF, CLAN_KEYS[0], CLAN_KEYS[1], 1, lambda k: 0.25 if 18 <= 20 + k / 13 < 45 else 0.0, 0, 10, 40, 8 * 13 - 1)
assert CLAN_HAND == [(5, 15), (17, 27), (28, 38), (42, 52), (59, 69), (74, 84), (90, 100), (101, 111)]
assert hand_conceptions(0xFF, CLAN_KEYS[2], CLAN_KEYS[3], 1, lambda k: 0.25 if 12 + k / 13 >= 18 else 0.0, 0, 10, 40, 83) == []


def clan_world(data=CLAN_DATA, on: bool = False) -> world_reference.World:
    return world_reference.World(0xFF, "test:clan", on, CLAN_FOUNDERS, CLAN_COUPLES, data=data)


def deaths_in(world: world_reference.World) -> list:
    """(time, sim, key of the cause) of every death, in time order."""
    return sorted((left, sim, cause) for sim, (_, left, cause, *_) in world.archive.items())


CLAN = clan_world()
# The first two women have their series from month 1, the woman of 46 none.
assert [CLAN.pending_series(sim) for sim in (1, 3, 5)] == [MONTH, MONTH, None]
assert all(CLAN.wish_of(sim) == world_reference.TEST_WISH for sim in (1, 3, 5))
CLAN_SAVE_AFTER_START = CLAN.save()
CLAN.advance(5 * MONTH - 1)
assert not CLAN.pregnancies
CLAN_LOG_BEFORE = len(CLAN.log)
CLAN.advance(5 * MONTH)
assert list(CLAN.pregnancies) == [1] and CLAN.pregnancies[1][3] == 5 * MONTH and len(CLAN.pregnancies[1][2]) == 1
CLAN_CONCEPTION_ENTRIES = CLAN.log[CLAN_LOG_BEFORE:]
assert len(CLAN_CONCEPTION_ENTRIES) == 3
CLAN_SAVE_AFTER_CONCEPTION = CLAN.save()
CLAN.advance(15 * MONTH - 1)
CLAN_LOG_BEFORE_BIRTH = len(CLAN.log)
CLAN.advance(15 * MONTH)
assert 7 in CLAN.sims and CLAN.sims[7][7] == (1, 2, 0) and not CLAN.pregnancies
assert CLAN.sims[1][6] == 1 and CLAN.sims[7][6] == 0 and CLAN.pending_series(1) == 16 * MONTH
CLAN_BIRTH_ENTRIES = CLAN.log[CLAN_LOG_BEFORE_BIRTH:]
CLAN_SAVE_AFTER_BIRTH = CLAN.save()
CLAN.advance(8 * YEAR - 1)
assert sorted(CLAN.pregnancies) == [1] and CLAN.pregnancies[1][1] == 7 and CLAN.sims[1][5:7] == [8, 7]
assert CLAN.pregnancies[1][3] == 101 * MONTH
assert [t // MONTH for t, woman, _, _, success in CLAN.attempts if success] == [c for c, _ in CLAN_HAND]
# The girl's attempts: chance 0 before her 18th birthday, 0.25 from it, none succeeds.
assert [(t // MONTH, p) for t, woman, _, p, _ in CLAN.attempts if woman == 3] == [
    (month, 0.0 if month < 78 else 0.25) for month in range(1, 84)
]
CLAN_DEATHS = deaths_in(CLAN)
assert [(sim, cause) for _, sim, cause in CLAN_DEATHS] == [(7, ACCIDENT_KEY), (3, ACCIDENT_KEY), (6, ACCIDENT_KEY)]
# The girl died after her 18th birthday (month 78), before she conceived.
assert 78 * MONTH < CLAN.archive[3][1] < 84 * MONTH and CLAN.archive[3][7:9] == (0, 0)
assert [CLAN.archive[7][0]] + [CLAN.sims[sim][0] for sim in range(8, 14)] == [b * MONTH for _, b in CLAN_HAND[:7]]
CLAN_SAVE_AFTER_YEAR_7 = CLAN.save()

# "clan" with the switch of the hereditary diseases on from the start (the same test
# data): the girl (dummy twice, `core:disease_ar`) is exposed from the start, the first
# child (dummy twice) from its birth in month 15; both have their death of the disease
# planned (`CLAN_ON_AR_DEATHS`), far beyond month 15.
CLAN_ON = clan_world(on=True)
CLAN_ON.advance(15 * MONTH)
assert [a.triple() for a in CLAN_ON.sims[7][4]] == [(AR_KEY, 0.0, (0.0, 15 * MONTH))]
assert [a.triple() for a in CLAN_ON.sims[3][4]] == [(AR_KEY, 0.0, (0.0, 0))]
CLAN_ON_AR_DEATHS = {sim: CLAN_ON.disease_due(sim, CLAN_ON.sims[sim][4][0]) for sim in (3, 7)}
assert all(due > 40 * YEAR for due in CLAN_ON_AR_DEATHS.values())
CLAN_ON_SAVE_AFTER_BIRTH = CLAN_ON.save()

# "clan" with a second cause, `test:fever` with 0.5 per year, as test data: accident 0.02
# per year, senescence `a` = 0.5 and `b` = 0.001, the fertility of "clan". The woman of 46
# dies of senescence in month 2.0, the girl in month 8.2 (her series drew with chance 0 in
# months 1 to 8), the father in month 20.9: the first woman is pregnant since month 17,
# the pregnancy goes on, his child (sim 8) is born in month 27; her partnership has
# ended, so no series follows the birth (births 2). The man of 46 dies in month 51.9. At
# the end of year 3 the first woman, the girl's partner and the two children live.
FEVER_DATA = world_reference.death_test_data(0.02, 0.5, fertility=CLAN_FERTILITY)
FEVER = clan_world(FEVER_DATA)
FEVER.advance(4 * YEAR - 1)
FEVER_DEATHS = deaths_in(FEVER)
assert [(sim, cause) for _, sim, cause in FEVER_DEATHS] == [(sim, SENESCENCE_KEY) for sim in (5, 3, 2, 6)]
assert sorted(FEVER.sims) == [1, 4, 7, 8] and not FEVER.pregnancies
assert FEVER.sims[8][7] == (1, 2, 1) and FEVER.sims[8][0] == 27 * MONTH
assert 17 * MONTH < FEVER.archive[2][1] < 27 * MONTH and FEVER.archive[2][7] == 0
assert FEVER.sims[1][5:7] == [2, 2] and FEVER.pending_series(1) is None
CLAN_FEVER_SAVE_AFTER_YEAR_3 = FEVER.save()

# "clan" with `test:blight` with 0.1 per year instead (the fertility of "clan"): the first
# woman dies of accident in month 22.2, pregnant since month 17 with her second child; the
# pregnancy ends with her, her conception stays counted, her births stay 1. Before her the
# girl (month 16.8) and the man of 46
# (month 19.7) die of accident. The test data take the two old rates the other way round:
# accident 0.1 per year (the old `test:blight`), senescence `a` = 0.02 (the old
# `test:base`) and `b` = 0.001. With them in the order of section 4 (accident 0.02, `a` =
# 0.1) no woman dies pregnant: the first woman dies in month 111.09, just after the birth
# in month 111, and with `b` = 0.001 no `a` up to 1 lets her die within a pregnancy while
# her partner lives. The run goes to the end of year 1.
BLIGHT_DATA = world_reference.death_test_data(0.1, 0.02, fertility=CLAN_FERTILITY)
BLIGHT = clan_world(BLIGHT_DATA)
HER_BLIGHT = BLIGHT.cause_due(1, ACCIDENT)
assert 22 * MONTH < HER_BLIGHT < 23 * MONTH < BLIGHT.cause_due(1, SENESCENCE)
BLIGHT.advance(HER_BLIGHT - 1)
assert BLIGHT.pregnancies[1][1] == 1 and BLIGHT.pregnancies[1][3] == 17 * MONTH
BLIGHT.advance(2 * YEAR - 1)
assert BLIGHT.archive[1][1:3] == (HER_BLIGHT, ACCIDENT_KEY) and BLIGHT.archive[1][7:9] == (2, 1)
assert 1 not in BLIGHT.pregnancies
BLIGHT_DEATHS = deaths_in(BLIGHT)
assert [(sim, cause) for _, sim, cause in BLIGHT_DEATHS] == [(3, ACCIDENT_KEY), (6, ACCIDENT_KEY), (1, ACCIDENT_KEY)]
CLAN_BLIGHT_SAVE_AFTER_YEAR_1 = BLIGHT.save()

# "clan" with a pause (for the load rejections of the pause): the test data of "clan" with
# `core:fertility_pause` 12 months, the wish still always 40. The first birth in month 15
# plans her next series at `15 + 12` = month 27 (design note section 4), not month 16.
CLAN_PAUSE_DATA = world_reference.death_test_data(0.02, fertility=CLAN_FERTILITY, life_values={"core:fertility_pause": 12.0})
assert CLAN_PAUSE_DATA.pause_months == 12 and CLAN_PAUSE_DATA.wish_shape == CLAN_DATA.wish_shape
CLAN_PAUSE = clan_world(CLAN_PAUSE_DATA)
CLAN_PAUSE.advance(15 * MONTH)
assert 7 in CLAN_PAUSE.sims and CLAN_PAUSE.sims[1][5:7] == [1, 1] and not CLAN_PAUSE.pregnancies
assert CLAN_PAUSE.pending_series(1) == 27 * MONTH and CLAN_PAUSE.wish_of(1) == world_reference.TEST_WISH
CLAN_PAUSE_SAVE_AFTER_BIRTH = CLAN_PAUSE.save()

# "clan" with a wish of one (for the load rejections of the wish): the test data of "clan"
# with the wish shape [0, 1] (`P_1` = 1, so every wish is 1), no pause. Her series from
# month 1 and her conception in month 5 are those of "clan"; with the first birth in month
# 15 her births reach her wish, so no series follows.
CLAN_WISH_DATA = world_reference.death_test_data(0.02, fertility=CLAN_FERTILITY, wish_shape=[0.0, 1.0])
assert CLAN_WISH_DATA.pause_months == 0 and CLAN_WISH_DATA.wish_shape == [0.0, 1.0]
CLAN_WISH = clan_world(CLAN_WISH_DATA)
assert [CLAN_WISH.pending_series(sim) for sim in (1, 3, 5)] == [MONTH, MONTH, None]
CLAN_WISH.advance(15 * MONTH)
assert 7 in CLAN_WISH.sims and CLAN_WISH.sims[1][5:7] == [1, 1] and not CLAN_WISH.pregnancies
assert CLAN_WISH.pending_series(1) is None and all(CLAN_WISH.wish_of(sim) == 1 for sim in (1, 3, 5))
CLAN_WISH_SAVE_AFTER_BIRTH = CLAN_WISH.save()

# The first woman's second conception in "clan" (month 17), for tests that build a
# conception by hand: her pregnancy as the projection holds it, and the draw of the attempt
# against the chance 0.25 of the window.
CLAN_SECOND = clan_world()
CLAN_SECOND.advance(17 * MONTH)
SECOND_FATHER, SECOND_NUMBER, SECOND_CHILDREN, SECOND_CONCEIVED = CLAN_SECOND.pregnancies[1]
((SECOND_KEY, SECOND_SEX, SECOND_PERSON),) = SECOND_CHILDREN
assert (SECOND_FATHER, SECOND_NUMBER, SECOND_CONCEIVED) == (2, 1, 17 * MONTH)
assert SECOND_KEY == world_reference.child_key(CLAN_KEYS[0], CLAN_KEYS[1], 1, 0)
SECOND_ALLELES = world_reference.copies_at(SECOND_PERSON, CLAN_DATA.loci["core:disease_ar"])
SECOND_UNIT = unit(words(chacha_key(0xFF, CONCEPTION_ATTEMPT_TAG, CLAN_KEYS[0], 17, CLAN_KEYS[1]))[0])
assert SECOND_UNIT < 0.25
# Pregnancy { father, conception, children: [Unborn { lineage_key, sex, genome }],
# conceived } (projection version 14, design note of the twins, section 4): one child.
SECOND_PREGNANCY_BYTES = (
    varint(1)
    + varint(SECOND_FATHER)
    + varint(SECOND_NUMBER)
    + seq([lineage_key(SECOND_KEY) + SECOND_SEX + genome_of(SECOND_PERSON)])
    + signed(SECOND_CONCEIVED)
)
assert SECOND_PREGNANCY_BYTES in CLAN_SECOND.projection()


# Scenario "twins" (design note of the twins, section 6): "clan" (seed 0xff, founder tag
# `test:clan`, its founders and couples, the switch off) on the test data of "clan" (the
# window from 18 until 45, 0.25 per month, ten months, no pause, the wish always 40) with
# `core:twins_identical` 0.3 and `core:twins_fraternal` 0.7, so every conception gives
# twins (`u < 0.3 + 0.7 = 1.0` always). The attempt draws do not read the children, so
# the first woman conceives in the months of "clan" (5, 17, 28, 42, ...), her children
# born ten months later; the draw of MULTIPLE_BIRTH at the key of child 0 of each
# conception gives fraternal twins in month 5 (two sons, sims 7 and 8, born in month 15),
# identical twins in month 17 (two daughters, sims 9 and 10, month 27), fraternal twins in
# month 28 (a daughter and a son, sims 11 and 12, month 38) and in month 42 (two
# daughters, born in month 52, after the end of year 3). Child 0 of the first conception
# has the key of the first child of "clan". Saves after the first conception, after the
# first birth and at the end of year 3 (pregnant with the fourth conception, births 6).
TWINS_RATES = (0.3, 0.7)
TWINS_DATA = world_reference.death_test_data(
    0.02,
    fertility=CLAN_FERTILITY,
    life_values={"core:twins_identical": TWINS_RATES[0], "core:twins_fraternal": TWINS_RATES[1]},
)
assert TWINS_DATA.twins == TWINS_RATES and TWINS_DATA.pause_months == 0 and TWINS_DATA.wish_shape == CLAN_DATA.wish_shape
MULTIPLE_BIRTH_TAG = 17  # tests/golden/rng_tags.txt, 11.7


def hand_twins(seed: int, her: int, him: int, number: int, rates: tuple) -> tuple:
    """(u, kind, keys) of her conception `number` with him by hand (11.3, design note of the
    twins sections 1 and 2): `u` = unit of the draw MULTIPLE_BIRTH at the key of the first
    child `child(her, him, number, 0)` (occasion 0, detail 0); identical below `p_id`,
    fraternal below `p_id + p_fr`, else one child; the keys of the children 0 (and 1)."""
    first = world_reference.child_key(her, him, number, 0)
    u = unit(words(chacha_key(seed, MULTIPLE_BIRTH_TAG, first, 0, 0))[0])
    kind = "identical" if u < rates[0] else "fraternal" if u < rates[0] + rates[1] else "single"
    keys = [first] if kind == "single" else [first, world_reference.child_key(her, him, number, 1)]
    return u, kind, keys


TWINS_HAND = [hand_twins(0xFF, CLAN_KEYS[0], CLAN_KEYS[1], number, TWINS_RATES) for number in range(4)]
assert [kind for _, kind, _ in TWINS_HAND] == ["fraternal", "identical", "fraternal", "fraternal"]


def hand_zygote(genome_key: int, mother: dict, father: dict) -> dict:
    """The genome of the zygote under `genome_key` from the meiosis reference directly."""
    data = TWINS_DATA
    person, _ = world_reference.meiosis_reference.child_with_mutation(
        0xFF, genome_key, mother, father, data.map_lengths, data.sex_ratio, data.genome_map, data.rates
    )
    return person


def twins_world() -> world_reference.World:
    return clan_world(TWINS_DATA)


TWINS = twins_world()
TWINS_SAVE_AFTER_START = TWINS.save()
TWINS.advance(5 * MONTH - 1)
TWINS_LOG_BEFORE = len(TWINS.log)
TWINS.advance(5 * MONTH)
# The first conception: fraternal twins, a conception entry per child, each followed by
# the two meioses of its own zygote (six entries).
TWINS_FIRST_ENTRIES = TWINS.log[TWINS_LOG_BEFORE:]
assert len(TWINS_FIRST_ENTRIES) == 6
TWINS_FATHER, TWINS_NUMBER, TWINS_FIRST_CHILDREN, TWINS_CONCEIVED = TWINS.pregnancies[1]
assert (TWINS_FATHER, TWINS_NUMBER, TWINS_CONCEIVED) == (2, 0, 5 * MONTH)
assert [key for key, _, _ in TWINS_FIRST_CHILDREN] == TWINS_HAND[0][2]
MOTHER_GENOME, FATHER_GENOME = TWINS.sims[1][3], TWINS.sims[2][3]
assert all(
    genome_of(person) == genome_of(hand_zygote(key, MOTHER_GENOME, FATHER_GENOME)) for key, _, person in TWINS_FIRST_CHILDREN
)
assert TWINS_FIRST_CHILDREN[0][0] == world_reference.child_key(CLAN_KEYS[0], CLAN_KEYS[1], 0, 0)
TWINS_SAVE_AFTER_CONCEPTION = TWINS.save()
TWINS.advance(15 * MONTH - 1)
TWINS_LOG_BEFORE_BIRTH = len(TWINS.log)
TWINS.advance(15 * MONTH)
# The first birth: two creation entries one after the other, consecutive SimIds in
# index order, both born in month 15; her births 2 and her series from month 16 (no
# pause), planned by the creation entry of child 0.
TWINS_BIRTH_ENTRIES = TWINS.log[TWINS_LOG_BEFORE_BIRTH:TWINS_LOG_BEFORE_BIRTH + 2]
assert [TWINS.sims[sim][1] for sim in (7, 8)] == TWINS_HAND[0][2]
assert all(TWINS.sims[sim][0] == 15 * MONTH and TWINS.sims[sim][7] == (1, 2, 0) for sim in (7, 8))
assert TWINS.sims[1][5:7] == [1, 2] and not TWINS.pregnancies and TWINS.next_sim_id == 9
assert TWINS.pending_series(1) == 16 * MONTH
(TWINS_SERIES,) = [event for event in TWINS.queue if event.kind == world_reference.ATTEMPT_KIND and event.subject[1] == 1]
assert TWINS_SERIES.plan == (TWINS_LOG_BEFORE_BIRTH + 1, 15 * MONTH)
TWINS_SAVE_AFTER_BIRTH = TWINS.save()
TWINS.advance(17 * MONTH - 1)
TWINS_LOG_BEFORE_IDENTICAL = len(TWINS.log)
TWINS.advance(17 * MONTH)
# The second conception: identical twins, the entry of child 0, its two meioses, the
# entry of child 1 (four entries); one genome, one sex.
TWINS_IDENTICAL_ENTRIES = TWINS.log[TWINS_LOG_BEFORE_IDENTICAL:]
assert len(TWINS_IDENTICAL_ENTRIES) == 4
_, _, TWINS_IDENTICAL_CHILDREN, _ = TWINS.pregnancies[1]
assert [key for key, _, _ in TWINS_IDENTICAL_CHILDREN] == TWINS_HAND[1][2]
assert TWINS_IDENTICAL_CHILDREN[0][1] == TWINS_IDENTICAL_CHILDREN[1][1]
assert genome_of(TWINS_IDENTICAL_CHILDREN[0][2]) == genome_of(TWINS_IDENTICAL_CHILDREN[1][2])
assert genome_of(TWINS_IDENTICAL_CHILDREN[0][2]) == genome_of(hand_zygote(TWINS_HAND[1][2][0], MOTHER_GENOME, FATHER_GENOME))
TWINS.advance(4 * YEAR - 1)
# The end of year 3: conceptions in the months of "clan", three twin births (sims 7 to 12),
# pregnant with the fourth conception; nobody has died.
assert [t // MONTH for t, *_ in TWINS.multiples] == [c for c, _ in CLAN_HAND[:4]]
assert [(u, kind, [key for key, _ in children]) for *_, u, kind, children in TWINS.multiples] == TWINS_HAND
assert sorted(TWINS.sims) == list(range(1, 13)) and not TWINS.archive
assert [TWINS.sims[sim][0] for sim in range(7, 13)] == [b * MONTH for _, b in CLAN_HAND[:3] for _ in range(2)]
assert [TWINS.sims[sim][7][2] for sim in range(7, 13)] == [0, 0, 1, 1, 2, 2]
assert TWINS.sims[1][5:7] == [4, 6] and TWINS.pregnancies[1][1] == 3 and TWINS.pregnancies[1][3] == 42 * MONTH
TWINS_SAVE_AFTER_YEAR_3 = TWINS.save()


def twins_story(world: world_reference.World) -> list:
    """Per conception: the month, the woman, the conception number, the draw u, the kind,
    the keys and sexes of the children."""
    rows = []
    for t, woman, man, number, u, kind, children in world.multiples:
        kids = ", ".join(f"{key:016x} {'female' if sex == FEMALE else 'male'}" for key, sex in children)
        rows.append(f"twins_conception month {t // MONTH} woman {woman} man {man} number {number} u {u!r} {kind}: {kids}")
    return rows


# Scenario "afflicted": seed 0xff, founder tag `test:afflicted`, the switch on from the
# start, no couples, on test data with the repository's six diseases and both
# causes for everyone practically off, so that only the diseases kill: accident 1e-9 per
# year (the smallest of the range; a budget lasts 100 000 years or more, so never) and
# senescence `a` = 1e-15, `b` = 0.001 (some 24 000 to 29 000 years) for both sexes. Ten founders born one second before the start (sim 9 thirty years before it),
# each with alleles at one disease locus (copies maternal then paternal; a man has one
# X copy, everyone one mt copy):
#   1 man      core:disease_xr [1]            hemizygous: has it (penetrance 1, onset 0)
#   2 woman    core:disease_xr [0, 1]         carrier: healthy, no draw
#   3 woman    core:disease_xr [1, 1]         has it
#   4 man      core:disease_mt [1]            penetrance 0.5 (men), visible, no death
#   5 woman    core:disease_mt [1]            penetrance 0.1 (women)
#   6 woman    core:disease_ad_late [0, 1]    dominant, penetrance 1, onset N(45, 10)
#   7 woman    core:disease_ad_partial [1, 0] penetrance 0.7 (women), onset N(50, 10)
#   8 man      core:disease_ad_partial [1, 0] penetrance 0.1 (men)
#   9 man      core:disease_ar [1, 1]         has it, 30 years old at the start (two steps,
#                                             past the change at 18)
#  10 woman    core:disease_ar [1, 0]         carrier: healthy, no draw
# Nobody has `core:disease_liability` (no liability trait, no draw). A lethal affliction
# is exposed from the start (the switch is on), its death planned at Start after the
# founder's death of senescence (accident: never).
AFFLICTED_FOUNDERS = [
    (-1, MALE, [("core:disease_xr", [1])]),
    (-1, FEMALE, [("core:disease_xr", [0, 1])]),
    (-1, FEMALE, [("core:disease_xr", [1, 1])]),
    (-1, MALE, [("core:disease_mt", [1])]),
    (-1, FEMALE, [("core:disease_mt", [1])]),
    (-1, FEMALE, [("core:disease_ad_late", [0, 1])]),
    (-1, FEMALE, [("core:disease_ad_partial", [1, 0])]),
    (-1, MALE, [("core:disease_ad_partial", [1, 0])]),
    (-30 * YEAR, MALE, [("core:disease_ar", [1, 1])]),
    (-1, FEMALE, [("core:disease_ar", [1, 0])]),
]
AFFLICTED_KEYS = [founder_key("test:afflicted", index) for index in range(len(AFFLICTED_FOUNDERS))]


AFFLICTED_DATA = world_reference.death_test_data(1e-9)


def afflicted_world() -> world_reference.World:
    return world_reference.World(0xFF, "test:afflicted", True, AFFLICTED_FOUNDERS, data=AFFLICTED_DATA)


AFFLICTED = afflicted_world()
AFFLICTED_AFTER_START = AFFLICTED.projection()
AFFLICTED_SAVE_AFTER_START = AFFLICTED.save()
# Carriers and the liability: no affliction. Penetrance 1 with the genotype: always one.
assert [len(AFFLICTED.sims[sim][4]) for sim in (1, 2, 3, 6, 9, 10)] == [1, 0, 1, 1, 1, 0]
assert all(
    affliction.disease.code != "core:disease_liability" for sim in AFFLICTED.sims.values() for affliction in sim[4]
)


def afflicted_rows() -> list:
    """Per founder: its afflictions (code, onset, used, since) and, per lethal one, the
    planned death in seconds or "never"."""
    rows = []
    for sim in sorted(AFFLICTED.sims):
        for affliction in AFFLICTED.sims[sim][4]:
            if affliction.exposure is None:
                exposure_text, death = "exposure none", "visible"
            else:
                used, since = affliction.exposure
                exposure_text = f"used {used!r} since {since}"
                due = AFFLICTED.disease_due(sim, affliction)
                death = "never" if due is None else str(due)
            rows.append(f"afflicted_sim_{sim} {affliction.disease.code} onset {affliction.onset!r} {exposure_text} death {death}")
        if not AFFLICTED.sims[sim][4]:
            rows.append(f"afflicted_sim_{sim} none")
        born, key, sex, person = AFFLICTED.sims[sim][:4]
        for disease in AFFLICTED.data.diseases:
            if disease.locus is None:
                continue
            if world_reference.has_genotype(disease.locus["action"], world_reference.copies_at(person, disease.locus)):
                word = words(chacha_key(0xFF, world_reference.DISEASE_PENETRANCE, key, disease.key, 0))[0]
                rows.append(
                    f"afflicted_sim_{sim} penetrance_draw {disease.code} unit {unit(word)!r} p {disease.penetrance[sex]!r}"
                )
    return rows


# The planned deaths are in the queue, by the creation entry of each founder: (time,
# sim, code), senescence for everyone (accident never), and the lethal diseases.
CODES = {str_key(code): code for code in (SENESCENCE, ACCIDENT)}
CODES.update({disease.key: disease.code for disease in AFFLICTED_DATA.diseases})
AFFLICTED_DEATHS = sorted(
    (event.time, event.subject[1], CODES[world_reference.death_key(event.payload)])
    for event in AFFLICTED.queue
    if event.kind == world_reference.DEATH_KIND
)
assert all(AFFLICTED.cause_due(sim, ACCIDENT) is None for sim in AFFLICTED.sims)
assert all(AFFLICTED.cause_due(sim, SENESCENCE) > 20_000 * YEAR for sim in AFFLICTED.sims)
assert all(
    (AFFLICTED.disease_due(sim, a), sim, a.disease.code) in AFFLICTED_DEATHS
    for sim in AFFLICTED.sims
    for a in AFFLICTED.sims[sim][4]
    if a.exposure is not None and AFFLICTED.disease_due(sim, a) is not None
)
AFFLICTED_DISEASE_DEATHS = [row for row in AFFLICTED_DEATHS if row[2] not in (SENESCENCE, ACCIDENT)]


def digest(data: bytes) -> str:
    return f"{xxhash.xxh3_64_intdigest(data, seed=0):016x}"


def years(seconds) -> str:
    return "never" if seconds is None else f"{seconds} ({seconds / YEAR:.4f} years)"


def cause_name(key: int) -> str:
    return CODES.get(key, f"{key:016x}") if key != DISEASE_KEY else DUMMY_DISEASE


def death_rows(name: str, world: world_reference.World) -> list:
    """Per sim of the world (living and archived): the due time of senescence and
    accident from its origin, and its death if it died."""
    rows = []
    for sim in sorted(set(world.sims) | set(world.archive)):
        if sim in world.sims:
            dues = " ".join(f"{code} {years(world.cause_due(sim, code))}" for code in (SENESCENCE, ACCIDENT))
            rows.append(f"{name}_sim_{sim} living {dues}")
        else:
            _, left, cause, *_ = world.archive[sim]
            rows.append(f"{name}_sim_{sim} died {years(left)} of {cause_name(cause)}")
    return rows


# Scenario "longevity" (crates/sim-core/tests/death.rs): the fixture with traits
# (tests/golden/data_traits) with `test:growth` as the trait of longevity (7.2), seed 0xff,
# tag `test:longevity`, a woman and a man born 70 years before the start. The genetic
# value of `test:growth` by hand from 4.1 and the fixture's map: `test:g1` additive with
# effect 0.5 and `d_ref` 1.25 (load 1), `test:g2` with effect 0.125 and load 2 around
# `d_ref` 0.5, `sigma_ref` 2. The woman has two copies at `test:g1` and none at `test:g2`:
# `(0.5 * (2 - 1.25) + 0.25 * (0 - 0.5)) / 2 = 0.125`; the man none at either:
# `(0.5 * (0 - 1.25) + 0.25 * (0 - 0.5)) / 2 = -0.375`. Senescence with
# `m = e^(-sigma_g * g)` (|g| < 6, nothing held) and without the trait (`m` = 1), accident
# as always; due from the start, seconds rounded up and robust to the last bit of libm.
LONGEVITY_TAG = "test:longevity"
LONGEVITY_BORN = -70 * YEAR
LONGEVITY_VALUES = [(FEMALE, 0.125), (MALE, -0.375)]


def longevity_dues() -> list:
    """Per founder of "longevity": (senescence with the trait, without it, accident), in
    seconds after the start."""
    _, calibration, _ = data_reference.documents(world_reference.ROOT / "tests" / "golden" / "data_traits")
    values = {key: entry["value"] for key, entry in calibration["values"].items()}
    sigma_g = values["core:longevity_sigma_g"]
    x0 = world_reference.as_years(0 - LONGEVITY_BORN)
    rows = []
    for index, (sex, value) in enumerate(LONGEVITY_VALUES):
        name = "female" if sex == FEMALE else "male"
        a, b = values[f"core:senescence_a_{name}"], values[f"core:senescence_b_{name}"]
        per_year = values[f"core:accident_{name}"]
        key = founder_key(LONGEVITY_TAG, index)

        def senescence(m: float):
            def seconds(nudge):
                budget = world_reference.step_ulps(world_reference.budget_of(0xFF, key, SENESCENCE), nudge[0])
                return world_reference.from_years_ceil(
                    world_reference.senescence_age(x0, budget, a, b, m, nudge[1:]) - x0
                )

            return world_reference.robust_seconds(seconds)

        m = world_reference.exp_reference(0.0 - sigma_g * value)[1]
        accident = world_reference.from_years_ceil(
            world_reference.accident_age(x0, world_reference.budget_of(0xFF, key, ACCIDENT), per_year) - x0
        )
        rows.append((senescence(m), senescence(1.0), accident))
    return rows


LONGEVITY_DUES = longevity_dues()
# Senescence comes first for both; the trait moves the woman's death later (g > 0) and
# the man's earlier (g < 0).
assert all(with_trait < accident for with_trait, _, accident in LONGEVITY_DUES)
assert LONGEVITY_DUES[0][0] > LONGEVITY_DUES[0][1] and LONGEVITY_DUES[1][0] < LONGEVITY_DUES[1][1]


TEST_DATA = [
    ("repository", world_reference.REPOSITORY),
    ("mortal", MORTAL_DATA),
    ("tie", TIE_DATA),
    ("sick", SICK_DATA),
    ("no_disease", NO_DISEASE_DATA),
    ("partners", PARTNERS_DATA),
    ("clan", CLAN_DATA),
    ("fever", FEVER_DATA),
    ("blight", BLIGHT_DATA),
    ("clan_pause", CLAN_PAUSE_DATA),
    ("clan_wish", CLAN_WISH_DATA),
    ("twins", TWINS_DATA),
    ("afflicted", AFFLICTED_DATA),
    ("fork", world_reference.FORK_DATA),
]


if __name__ == "__main__":
    print(f"core_version {CORE_VERSION} projection_version {PROJECTION_VERSION}")
    print(f"longevity_dues (senescence with the trait, without, accident) {LONGEVITY_DUES}")
    for name, data in TEST_DATA:
        (fa, fb), (ma, mb) = data.senescence[FEMALE], data.senescence[MALE]
        form = data.fertility
        window = f" from {form.start!r} until {form.until!r}" if form.kind == "window" else ""
        print(
            f"data_hash_{name} {data.hash:016x} accident {data.accident[FEMALE]!r} {data.accident[MALE]!r}"
            f" senescence_a {fa!r} {ma!r} senescence_b {fb!r} {mb!r}"
            f" diseases {[disease.code for disease in data.diseases]}"
            f" fertility {form.kind}{window} adult_rate {data.adult_rate!r} pause_months {data.pause_months}"
            f" pregnancy_months {data.pregnancy_months} wish_weights {len(data.wish_shape)} twins {data.twins!r}"
        )
    print(f"cause_keys senescence {SENESCENCE_KEY:016x} accident {ACCIDENT_KEY:016x}")
    for name, data in [
        ("tiny_after_start", TINY_AFTER_START),
        ("tiny_year_0", TINY_YEAR_0),
        ("tiny_after_year_0", TINY_AFTER_YEAR_0),
        ("pair_after_start", PAIR_AFTER_START),
        ("tiny_overridden", TINY_OVERRIDDEN),
        ("mortal_after_start", MORTAL_AFTER_START),
        ("mortal_after_year_5", MORTAL_AFTER_YEAR_5),
    ]:
        print(f"{name} bytes {data.hex()}")
        print(f"{name} hash {digest(data)}")
    print(f"data_hash {DATA_HASH:016x}")
    print(f"header {header(DATA_HASH).hex()}")
    print(f"tiny_key {TINY_KEY:016x} due senescence {years(TINY_DUE[SENESCENCE])} accident {years(TINY_DUE[ACCIDENT])}")
    print(f"tiny_save_after_start {TINY_SAVE_AFTER_START.hex()}")
    print(f"empty_save {EMPTY_SAVE.hex()}")
    print(f"empty_projection {EMPTY_PROJECTION.hex()}")
    print(f"tiny_log_hash {digest(b''.join(TINY_LOG))}")
    print(f"tiny_input_log {TINY_INPUT_LOG.hex()}")
    print(f"tiny_save_after_year_0 {TINY_SAVE_AFTER_YEAR_0.hex()}")
    print(f"tiny_override_entry {TINY_OVERRIDE_ENTRY.hex()}")
    print(f"tiny_save_overridden {TINY_SAVE_OVERRIDDEN.hex()}")
    for index, dues in enumerate(PAIR_DUE):
        print(f"pair_sim_{index + 1} key {PAIR_KEYS[index]:016x} due senescence {years(dues[SENESCENCE])} accident {years(dues[ACCIDENT])}")
    print(f"mortal_save_after_start {MORTAL_SAVE_AFTER_START.hex()}")
    print(f"mortal_keys {[format(key, '016x') for key in MORTAL_KEYS]}")
    print(f"mortal_budgets {[(code, world_reference.budget_of(0xFF, key, code)) for key in MORTAL_KEYS for code in (SENESCENCE, ACCIDENT)]}")
    print(f"mortal_deaths_due (her senescence, her accident, his senescence, his accident) {MORTAL_DEATHS}")
    for number, line in enumerate(MORTAL.log, start=1):
        print(f"mortal_entry_{number} {line.hex()}")
    print(f"mortal_log_hash {digest(b''.join(MORTAL.log))}")
    print(f"mortal_year_hashes {[format(value, '016x') for value in MORTAL.year_hashes]}")
    print(f"mortal_save_after_year_5 {MORTAL_SAVE_AFTER_YEAR_5.hex()}")
    print(f"tie_key {TIE_KEY:016x} due {TIE_DUE} ({TIE_DUE / YEAR:.6f} years) accident {TIE_ACCIDENT!r} senescence_a {TIE_SENESCENCE_A!r} senescence_b 0.001")
    print(f"tie_budgets senescence {world_reference.budget_of(0xFF, TIE_KEY, SENESCENCE)!r} accident {world_reference.budget_of(0xFF, TIE_KEY, ACCIDENT)!r}")
    print(f"tie_save_after_start {TIE_SAVE_AFTER_START.hex()}")
    print(f"tie_save {TIE_SAVE.hex()}")
    print(f"tie_death_entry {TIE.log[-1].hex()}")
    print(f"tie_year_hashes {[format(value, '016x') for value in TIE.year_hashes]}")
    print(f"sick_keys {[format(key, '016x') for key in SICK_KEYS]} disease_key {DISEASE_KEY:016x}")
    print(f"sick_due his_disease {HIS_DISEASE} his_accident {HIS_ACCIDENT} her_accident {HER_ACCIDENT}")
    fresh = sick_world(True)
    print(f"sick_due_senescence his {fresh.cause_due(2, SENESCENCE)} her {fresh.cause_due(1, SENESCENCE)}")
    print(f"sick_on_save_after_start {SICK_ON_SAVE_AFTER_START.hex()}")
    print(f"sick_on_save_after_year_3 {SICK_ON_SAVE_AFTER_YEAR_3.hex()}")
    print(f"sick_on_his_death_entry {SICK_ON.log[-1].hex()}")
    print(f"sick_off_save_after_start {SICK_OFF_SAVE_AFTER_START.hex()}")
    print(f"switched_on_save {SWITCHED_ON_SAVE.hex()}")
    for name, run in [("sick_on", SICK_ON), ("sick_off", SICK_OFF), ("switched_on", SWITCHED_ON), ("on_off_on", ON_OFF_ON), ("off_at_due", OFF_AT_DUE)]:
        print(f"{name}_year_hashes {[format(value, '016x') for value in run.year_hashes]}")
    print(f"his_budget {HIS_BUDGET!r}")
    print(f"sick_deaths switched_on {SWITCHED_ON_DEATH} on_off_on {ON_OFF_ON_DEATH} two_periods {TWO_PERIODS_DEATH} save_load {death_of(SAVE_LOAD, 2)[0]}")
    print(f"sick_used on_off_on {ON_OFF_ON_USED!r} two_periods {TWO_PERIODS_USED!r} off_at_due {OFF_AT_DUE_USED!r}")
    print(f"on_off_on_save {ON_OFF_ON_SAVE.hex()}")
    print(f"off_at_due_save {OFF_AT_DUE_SAVE.hex()}")
    print(f"tie_switch {TIE_SWITCH} (disease and accident of him due in {HIS_ACCIDENT}; accident wins)")
    print(f"accident_wins_his_death {ACCIDENT_WINS.log[-1].hex()}")
    print(f"no_disease_his_death {NO_DISEASE.log[-1].hex()}")
    print(f"no_disease_his_ar_death {NO_DISEASE_HIS_AR_DEATH} planned by entry {NO_DISEASE_OVERRIDE_ENTRY} at {YEAR}")
    print(f"late_edge {LATE_EDGE}")
    print(f"partners_keys {[format(key, '016x') for key in PARTNERS_KEYS]}")
    print(f"partners_deaths {PARTNERS_DEATHS}")
    print(f"partners_save_after_start {PARTNERS_SAVE_AFTER_START.hex()}")
    print(f"partners_started_entry {PARTNERS_STARTED_ENTRY.hex()}")
    print(f"partners_arranged_entry {PARTNERS_ARRANGED_ENTRY.hex()}")
    print(f"partners_save_after_arrange {PARTNERS_SAVE_AFTER_ARRANGE.hex()}")
    print(f"partners_save_after_year_4 {PARTNERS_SAVE_AFTER_YEAR_4.hex()}")
    print(f"partners_year_hashes {[format(value, '016x') for value in PARTNERS.year_hashes]}")
    for row in death_rows("partners", PARTNERS):
        print(row)
    print(f"partners_wish {PARTNERS_WISH} menarche {PARTNERS.menarche_of(1)!r}")
    print(f"partners_attempts (month, chance, success) {[(t // MONTH, p, ok) for t, _, _, p, ok in PARTNERS.attempts]}")
    print(f"clan_keys {[format(key, '016x') for key in CLAN_KEYS]}")
    print(f"clan_conceptions_by_hand (conception month, birth month) {CLAN_HAND}")
    print(f"clan_births_and_conceptions {[(sim, CLAN.sims[sim][5], CLAN.sims[sim][6]) for sim in (1, 5)]} girl {CLAN.archive[3][7:9]}")
    print(f"clan_save_after_start {CLAN_SAVE_AFTER_START.hex()}")
    for number, line in enumerate(CLAN_CONCEPTION_ENTRIES):
        print(f"clan_conception_entry_{number} {line.hex()}")
    print(f"clan_save_after_conception {CLAN_SAVE_AFTER_CONCEPTION.hex()}")
    print(f"clan_save_after_year_7 {CLAN_SAVE_AFTER_YEAR_7.hex()}")
    print(f"clan_pregnancies {[(mother, (father, key, number, sex, conceived)) for mother, (father, number, ((key, sex, _),), conceived) in sorted(CLAN.pregnancies.items())]}")
    print(f"clan_deaths {[(left, sim, cause_name(cause)) for left, sim, cause in CLAN_DEATHS]}")
    for row in death_rows("clan", CLAN):
        print(row)
    print(f"clan_fever_save_after_year_3 {CLAN_FEVER_SAVE_AFTER_YEAR_3.hex()}")
    print(f"clan_fever_deaths {[(left, sim, cause_name(cause)) for left, sim, cause in FEVER_DEATHS]}")
    print(f"clan_on_save_after_birth {CLAN_ON_SAVE_AFTER_BIRTH.hex()}")
    print(f"clan_on_ar_deaths {CLAN_ON_AR_DEATHS}")
    print(f"clan_blight_save_after_year_1 {CLAN_BLIGHT_SAVE_AFTER_YEAR_1.hex()}")
    print(f"clan_blight_her_death {HER_BLIGHT}")
    print(f"clan_blight_deaths {[(left, sim, cause_name(cause)) for left, sim, cause in BLIGHT_DEATHS]}")
    for number, line in enumerate(CLAN_BIRTH_ENTRIES):
        print(f"clan_birth_entry_{number} {line.hex()}")
    print(f"clan_save_after_birth {CLAN_SAVE_AFTER_BIRTH.hex()}")
    print(f"clan_pause_save_after_birth {CLAN_PAUSE_SAVE_AFTER_BIRTH.hex()}")
    print(f"clan_wish_save_after_birth {CLAN_WISH_SAVE_AFTER_BIRTH.hex()}")
    sex_name = "female" if SECOND_SEX == FEMALE else "male"
    print(
        f"clan_second_pregnancy mother 1 father {SECOND_FATHER} lineage_key {SECOND_KEY:016x} conception {SECOND_NUMBER}"
        f" sex {sex_name} core:disease_ar maternal {SECOND_ALLELES[0]} paternal {SECOND_ALLELES[1]} conceived {SECOND_CONCEIVED}"
    )
    print(f"clan_second_pregnancy genome {genome_of(SECOND_PERSON).hex()} bytes {SECOND_PREGNANCY_BYTES.hex()}")
    print(f"clan_second_attempt month 17 u {SECOND_UNIT!r} chance 0.25 success {SECOND_UNIT < 0.25}")
    print(f"clan_fever_archive {[(sim, left) for sim, (_, left, *_ ) in sorted(FEVER.archive.items())]}")
    print(f"clan_year_hashes {[format(value, '016x') for value in CLAN.year_hashes]}")
    print(f"twins_data_hash {TWINS_DATA.hash:016x} twins {TWINS_DATA.twins!r}")
    print(f"twins_by_hand (u, kind, keys) {[(u, kind, [format(key, '016x') for key in keys]) for u, kind, keys in TWINS_HAND]}")
    for row in twins_story(TWINS):
        print(row)
    print(f"twins_save_after_start {TWINS_SAVE_AFTER_START.hex()}")
    for number, line in enumerate(TWINS_FIRST_ENTRIES):
        print(f"twins_conception_entry_{number} {line.hex()}")
    print(f"twins_save_after_conception {TWINS_SAVE_AFTER_CONCEPTION.hex()}")
    for number, line in enumerate(TWINS_BIRTH_ENTRIES):
        print(f"twins_birth_entry_{number} {line.hex()}")
    print(f"twins_save_after_birth {TWINS_SAVE_AFTER_BIRTH.hex()}")
    for number, line in enumerate(TWINS_IDENTICAL_ENTRIES):
        print(f"twins_identical_conception_entry_{number} {line.hex()}")
    print(f"twins_save_after_year_3 {TWINS_SAVE_AFTER_YEAR_3.hex()}")
    print(f"twins_year_hashes {[format(value, '016x') for value in TWINS.year_hashes]}")
    print(f"twins_log_hash {digest(b''.join(TWINS.log))}")
    for row in death_rows("twins", TWINS):
        print(row)
    print(f"afflicted_keys {[format(key, '016x') for key in AFFLICTED_KEYS]}")
    for row in afflicted_rows():
        print(row)
    print(f"afflicted_deaths {AFFLICTED_DEATHS}")
    print(f"afflicted_disease_deaths {AFFLICTED_DISEASE_DEATHS}")
    print(f"afflicted_after_start bytes {AFFLICTED_AFTER_START.hex()}")
    print(f"afflicted_after_start hash {digest(AFFLICTED_AFTER_START)}")
    print(f"afflicted_save_after_start {AFFLICTED_SAVE_AFTER_START.hex()}")
    # Log hash: xxh3-64 over the postcard bytes of all entries in order (ADR-0004).
    print(f"log_hash_empty {digest(b'')}")
    print(f"log_hash_entry {digest(SAMPLE_ENTRY)}")
    print(f"log_hash_entry_twice {digest(SAMPLE_ENTRY + SAMPLE_ENTRY)}")

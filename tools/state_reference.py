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

Separation (ADR-0018 section 3, design note of the separation): a woman holds
`partnerships` right after `births`, a partnership ends with an `Ending { at, by }`, and
each partnership plans its separation at its formation (projection version 15, core
version 29). The test data of the old model (the form `window`: "clan" and its variants,
"twins", the fork data) have `core:separation_rate` 0, so their stories stay; the
scenarios on the repository's rate 0.00867 ("partners") plan each separation, which a
death cancels. "split" is "clan" with a separation rate the reference picks and one
`ArrangePartnership`.

The partner market (genetics specification 11.2, ADR-0018 section 1, design note of the
market): `Start` plans the series of the rounds right after the year end series
(insertion number 2, first at MONTH, period MONTH), so every later insertion number of
`Start` is one higher; the projection is unchanged (version 15, core version 30). The
test data of the old model have `c` = 0: their rounds run and write nothing. Couples under
16 are rejected by `Start`, so "partners" has founders of 16 years and one to three
seconds and "clan" a girl of 16 (she was 12); their stories change as written there. The
null twin: "clan" with the round switched off (a switch of the reference) has the same log
and census. "market" is a small town on the repository's data with `c` raised.

Similarity (genetics specification 11.2, design note of the similarity): the calibration
has format 7 with `cognition_trait` and `height_trait` (both null in the repository's
data, so every scenario on them keeps its story; only the data hash in the header of a
save changes, core version 31, projection version 15). "similar" runs on the test map
tests/golden/data_pairing (tools/pairing_fixture.py), which names both traits: twelve
single founders of 18 to 28 years with alleles at the 80 trait loci, ten rounds, every
utility with the four penalty terms.

Households, part 1 (ADR-0018 sections 4 and 7, design note of the households): the life
course data have format 2 with the table of leaving home (every data hash changes), the
projection holds the counter of HouseholdId, the households, and per sim its household,
`moved_out` and carer (projection version 16, core version 32). `Start` houses the
founders after the couples (one `core:household_formed` per household: the couples, the
founders of 18 or more, then the minors, each alone; a minor not past its drawn age of
leaving by the orphan rule with its `leave_home` planned); a birth moves the child into
its mother's household, a later pairing, a separation, a `leave_home` and the orphan rule
found new households (`core:household_formed`, `core:moved`). The stories stay: every
scenario run here is run again with the households switched off (a switch of the
reference) and has the same births, deaths, partnerships, separations, attempts and
choices, and the very log of the model before. "homes" is the scenario of the note's
section 8; "homes_orphans" adds the orphan rule after the start.

Households, part 2 (ADR-0018 section 4 "Waisen", "Pflege", section 7; design note of the
households, part 2): a sim and an archived sim hold `social_parents` after `parents`
(projection version 17, core version 33; every save changes by one byte per sim, the data
hashes stay). The orphan rule places a group with its nearest relative of 18 or more, else
in a foster household (FOSTER_RANK, tag 23), else in a household of its own; it runs for a
minor founder at the start, at the death of a carer and after a carer's `leave_home`. A
scenario without a minor founder beside a couple and without orphans keeps its log byte for
byte (only its saves and year hashes change). "homes" (the boy of 12 now in a foster
household) and "homes_orphans" (the orphans now with their father) change; "homes_kin",
"homes_step" and "homes_neither" are new (section 7 and the coordinator's three
separations); `section_6_broken_states` holds one rejected state per new load check.

The pedigree with kinship and social parents (step 10, design note of the pedigree): no run
changes. The text of `pedigree.json` (`World.pedigree_json`) of "homes_step" at the second
of its separation (the stepfather cares without a social parent edge) and at its end,
"homes_kin" and "homes_neither" at their ends (social parents; a carer without an edge after
a separation), "afflicted" at the end of year 37 (the switch on: affected and not, a death
of a disease) and, after an override that switches the diseases off at that clock, at the
end of year 38, and "loci" (new: two founders with alleles at several loci, right after
`Start`). The kinship query's known answers on "homes_step" and "market" at their ends.

The threshold disease in the world (step 11, design note of the threshold disease): no run
above changes (their data name no trait of liability; `check_all` now also checks the
afflictions, `world_reference.check_afflictions`). "threshold" (new) runs on the test map
tests/golden/data_liability (tools/liability_fixture.py), whose threshold disease names its
trait `core:disease_liability`: eight founders with chosen alleles at its eight loci, three
couples, the switch on, 12 years; `L` (as f64 bits) of every person, the affected set, the
saves around the first birth of a child with the disease and around a death of it, the
entries of those seconds, year and log hashes, the pedigree at the end, and two saves the
load check rejects (the disease held without `L > T`, and missing with `L > T`).

Scenario "tiny": Start with world_seed 0xff, founder tag `test:tiny`, the switch of the
hereditary diseases off and one founder born
one second before the start, female, with the alleles wild (from the mother) and dummy
(from the father) at the dummy locus (ADR-0004). Her lineage key is that of founder 0
of the tag (ADR-0001 section 4). By hand from ADR-0002, ADR-0004 and the genetics
specification 7.1, 7.2:
- the input is applied at now = 0; the year end series gets insertion number 1 (first at
  YEAR - 1, period YEAR), the series of the market rounds insertion number 2 (first at
  MONTH, the first month start after the start, period MONTH; design note of the market,
  section 2), the founder's first birthday insertion number 3 (at YEAR - 1), her death of
  senescence 4 and of accident 5 (after about 85 and 16 990 years);
- in the queue the round comes first (MONTH), then the birthday: same second as the year
  end, domain phase before observers; the deaths come last. She is no single woman in the
  market age (one second old), so the rounds draw nothing and write nothing.

Usage: .venv/bin/python tools/state_reference.py
"""

import copy
import json
import math
from fractions import Fraction

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
from rng_reference import chacha_key, founder_key, str_key, unit, unit_open0, words
from rng_reference import rank as rng_rank, unit_open as rng_unit_open

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


def market_round() -> bytes:
    """Event::MarketRound (variant 6, no fields; kind 7, design note of the market)."""
    return variant(6)


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


def sim_state(born: int, key: int, sex: bytes, genes: tuple, afflicted=(), household: int = 1, moved_out: bool = False, carer=None) -> bytes:
    """Sim { born, lineage_key, sex, genome, afflictions, conceptions, births,
    partnerships, household, moved_out, carer, parents }: the afflictions as triples
    (disease, onset, exposure), none but for a sim with the dummy allele twice
    (`core:disease_ar`, recessive, penetrance 1, onset 0); no conceptions and no births yet
    (`births` right after `conceptions`, design note of the fertility in the world, section
    1); no partnerships yet (`partnerships` right after `births`, design note of the
    separation, section 2); then its household, `moved_out` (one byte) and its carer, an
    option (design note of the households, section 2); a founder has no parents; then its
    social parents, a list (part 2, section 2), empty for a founder housed alone."""
    return (
        signed(born)
        + lineage_key(key)
        + sex
        + genome(sex, genes)
        + afflictions(list(afflicted))
        + varint(0)  # conceptions
        + varint(0)  # births
        + varint(0)  # partnerships
        + varint(household)
        + (b"\x01" if moved_out else b"\x00")
        + (NONE if carer is None else some(varint(carer)))
        + NONE  # parents
        + seq([])  # social parents
    )


# The age of leaving home by hand (design note of the households, section 1; Dario's table
# of 2026-10-11, ADR-0018 section 4), with Python's float sum: `u` = unit of the draw
# LEAVE_HOME (tag 22) at the lineage key (occasion 0, detail 0), the age `16 + n` for the
# smallest `n` with `u < (w_0 + … + w_n) / W`.
LEAVE_HOME_TAG = 22
LEAVE_HOME_TABLE = [0.02, 0.03, 0.30, 0.20, 0.15, 0.10, 0.08, 0.05, 0.04, 0.03]
assert world_reference.REPOSITORY.leave_home_weights == LEAVE_HOME_TABLE


def hand_leave_age(seed: int, key: int) -> int:
    u = unit(words(chacha_key(seed, LEAVE_HOME_TAG, key, 0, 0))[0])
    total, acc = sum(LEAVE_HOME_TABLE), 0.0
    for n, weight in enumerate(LEAVE_HOME_TABLE):
        acc += weight
        if u < acc / total:
            return 16 + n
    return 25


def leave_home(sim: int) -> bytes:
    """Event::LeaveHome { sim } (variant 7; kind 8, design note of the households)."""
    return variant(7, varint(sim))


def household_state(formed: int, ended=None) -> bytes:
    """Household { formed, ended: Option<SimTime> }."""
    return signed(formed) + (NONE if ended is None else some(signed(ended)))


HOUSEHOLD_RULE = variant(3, string("core:household"))


TINY_KEY = founder_key("test:tiny", 0)
TINY_GENES = genotype(WILD, DUMMY)
# Her deaths, planned by her creation entry 2 at the start (insertion numbers 4 and 5).
TINY_DUE = {code: hand_due(TINY_KEY, -1, code, SPEC_RATES[FEMALE]) for code in (SENESCENCE, ACCIDENT)}
assert 80 * YEAR < TINY_DUE[SENESCENCE] < 90 * YEAR < TINY_DUE[ACCIDENT] < 100_000 * YEAR
TINY_DEATHS = [death(TINY_DUE[SENESCENCE], 4, 1, SENESCENCE), death(TINY_DUE[ACCIDENT], 5, 1, ACCIDENT)]
# The households (design note of the households, sections 4 and 6): she is a founder under
# 18 in no couple, one second old, far from her drawn age of leaving: a household of her
# own by the orphan rule (household 1, `core:household_formed` entry 3, causes the input
# and the rule), not moved out, no carer; her `leave_home` (insertion number 6, by her
# creation entry 2) at `−1 + age · YEAR`, before her deaths.
TINY_LEAVE_AGE = hand_leave_age(0xFF, TINY_KEY)
TINY_LEAVE = -1 + TINY_LEAVE_AGE * YEAR
assert 2 * YEAR < TINY_LEAVE < TINY_DUE[SENESCENCE]
TINY_LEAVE_EVENT = queue_entry(TINY_LEAVE, 6, leave_home(1), None)


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
        + varint(2)  # next HouseholdId
        + seq(queue)
        + varint(next_insertion)
        + seq([varint(1) + sim_state(-1, TINY_KEY, FEMALE, TINY_GENES)])  # sims: id 1, household 1
        + seq([])  # partnerships
        + seq([varint(1) + household_state(0)])  # households: 1, formed at the start
        + seq([])  # pregnancies
        + seq([])  # archive
    )


# Right after Start: nothing closed yet.
TINY_AFTER_START = projection(
    -1,
    [
        queue_entry(MONTH, 2, market_round(), MONTH),
        queue_entry(YEAR - 1, 3, birthday(1), None),
        queue_entry(YEAR - 1, 1, year_end(), YEAR),
        TINY_LEAVE_EVENT,
    ]
    + TINY_DEATHS,
    7,
)

# What the year end observer of year 0 sees at t = YEAR - 1: the clock stands at t - 1
# (ADR-0002), the birthday of this second has run and planned the next one (insertion
# number 7), the year end series already holds its next occurrence; the twelve rounds of
# months 1 to 12 have run, the next is at the start of month 13 (= YEAR, 13 months of 28
# days).
TINY_YEAR_0 = projection(
    YEAR - 2,
    [
        queue_entry(YEAR, 2, market_round(), MONTH),
        queue_entry(2 * YEAR - 1, 7, birthday(1), None),
        queue_entry(2 * YEAR - 1, 1, year_end(), YEAR),
        TINY_LEAVE_EVENT,
    ]
    + TINY_DEATHS,
    8,
)

# After advance_until(YEAR - 1): the same state with the second closed.
TINY_AFTER_YEAR_0 = projection(
    YEAR - 1,
    [
        queue_entry(YEAR, 2, market_round(), MONTH),
        queue_entry(2 * YEAR - 1, 7, birthday(1), None),
        queue_entry(2 * YEAR - 1, 1, year_end(), YEAR),
        TINY_LEAVE_EVENT,
    ]
    + TINY_DEATHS,
    8,
)


# Scenario "pair": seed 0xff, founder tag `test:pair`, a woman born two seconds before
# the start with the alleles wild and dummy, and a man born one second before it with
# dummy twice. Right after Start: the round series (insertion number 2) at MONTH; her
# first birthday (3) is due at YEAR - 2, her deaths of senescence and accident (4, 5) after
# about 92 and 436 years; his first birthday (6) at YEAR - 1, in the same second as the
# year end, his deaths (7, 8) after about 88 and 584 years. He has `core:disease_ar` (recessive, penetrance 1, so
# the draw always says yes; onset fixed at 0): an affliction with nothing used, the
# switch off. The households after the founders (design note of the households, section
# 4): both are under 18 and in no couple, so each gets a household of its own by the
# orphan rule in founder order (households 1 and 2, entries 4 and 5), and then their
# `leave_home` events are planned in that order (insertion numbers 9 and 10, by their
# creation entries 2 and 3).
AR_KEY = str_key("core:disease_ar")
PAIR_KEYS = [founder_key("test:pair", index) for index in range(2)]
PAIR_DUE = [
    {code: hand_due(key, born, code, SPEC_RATES[sex]) for code in (SENESCENCE, ACCIDENT)}
    for key, born, sex in ((PAIR_KEYS[0], -2, FEMALE), (PAIR_KEYS[1], -1, MALE))
]
PAIR_DEATHS = sorted(
    [
        (PAIR_DUE[0][SENESCENCE], death(PAIR_DUE[0][SENESCENCE], 4, 1, SENESCENCE)),
        (PAIR_DUE[0][ACCIDENT], death(PAIR_DUE[0][ACCIDENT], 5, 1, ACCIDENT)),
        (PAIR_DUE[1][SENESCENCE], death(PAIR_DUE[1][SENESCENCE], 7, 2, SENESCENCE)),
        (PAIR_DUE[1][ACCIDENT], death(PAIR_DUE[1][ACCIDENT], 8, 2, ACCIDENT)),
    ]
)
assert 2 * YEAR < PAIR_DEATHS[0][0] < PAIR_DEATHS[1][0] < PAIR_DEATHS[2][0] < PAIR_DEATHS[3][0]
PAIR_LEAVES = sorted(
    (born + hand_leave_age(0xFF, key) * YEAR, sim, insertion)
    for sim, (key, born, insertion) in enumerate(((PAIR_KEYS[0], -2, 9), (PAIR_KEYS[1], -1, 10)), start=1)
)
assert PAIR_LEAVES[-1][0] < PAIR_DEATHS[0][0]
PAIR_AFTER_START = (
    varint(PROJECTION_VERSION)
    + some(varint(0xFF))
    + some(world_params(False))
    + signed(-1)
    + varint(3)  # next SimId
    + varint(1)  # next PartnershipId
    + varint(3)  # next HouseholdId
    + seq(
        [
            queue_entry(MONTH, 2, market_round(), MONTH),
            queue_entry(YEAR - 2, 3, birthday(1), None),
            queue_entry(YEAR - 1, 6, birthday(2), None),
            queue_entry(YEAR - 1, 1, year_end(), YEAR),
        ]
        + [queue_entry(time, insertion, leave_home(sim), None) for time, sim, insertion in PAIR_LEAVES]
        + [entry for _, entry in PAIR_DEATHS]
    )
    + varint(11)
    + seq(
        [
            varint(1) + sim_state(-2, PAIR_KEYS[0], FEMALE, genotype(WILD, DUMMY), household=1),
            # He has the dummy allele twice: nothing used, the switch is off.
            varint(2) + sim_state(-1, PAIR_KEYS[1], MALE, genotype(DUMMY, DUMMY), [(AR_KEY, 0.0, (0.0, None))], household=2),
        ]
    )
    + seq([])  # partnerships
    + seq([varint(1) + household_state(0), varint(2) + household_state(0)])  # households
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

# Right after Start (the log taken): plan references in queue order (the round series and
# the year end series were planned by `core:started`, entry 1, the birthday, her
# `leave_home` and her two deaths by the founder's entry 2), the next EventId is 4 (entry 3
# is her `core:household_formed`), no year hashes yet.
TINY_SAVE_AFTER_START = (
    header(DATA_HASH)
    + TINY_AFTER_START
    + seq([plan_ref(1, 0), plan_ref(2, 0), plan_ref(1, 0), plan_ref(2, 0), plan_ref(2, 0), plan_ref(2, 0)])
    + varint(4)
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
    + varint(1)  # next HouseholdId
    + seq([])
    + varint(1)
    + seq([])  # sims
    + seq([])  # partnerships
    + seq([])  # households
    + seq([])  # pregnancies
    + seq([])  # archive
)
EMPTY_SAVE = header(DATA_HASH) + EMPTY_PROJECTION + seq([]) + varint(1) + seq([]) + seq([])


# The log of "tiny" up to the end of year 0 (ADR-0004 section 7). An entry is
# (id, t, subjects, causes, payload). Subject::Sim = 0. Cause: Event = 0, Plan = 1
# { by, planned_at }, Input = 2, Rule = 3. Payload: core:started = 0 { founders },
# core:founder_added = 1 { born, lineage_key }, core:birthday = 2 { age_years },
# core:param_changed = 3 { change }, core:death = 4 { cause }, core:household_formed = 10
# { household }. Cause::Random = 4 { tag, subject, occasion, detail }.
def entry(event_id: int, time: int, subjects: list, causes: list, payload: bytes) -> bytes:
    return varint(event_id) + signed(time) + seq(subjects) + seq(causes) + payload


SIM_1 = variant(0, varint(1))
BY_INPUT_1 = variant(2, varint(1))
TINY_LOG = [
    entry(1, 0, [], [BY_INPUT_1], variant(0, varint(1))),
    entry(2, 0, [SIM_1], [BY_INPUT_1], variant(1, signed(-1), lineage_key(TINY_KEY))),
    entry(3, 0, [SIM_1], [BY_INPUT_1, HOUSEHOLD_RULE], variant(10, varint(1))),
    entry(4, YEAR - 1, [SIM_1], [variant(1, varint(2), signed(0))], variant(2, varint(1))),
]

# "tiny" with an override block right after Start (stamp -1, seq 2) that switches the
# hereditary diseases on: one entry `core:param_changed` at now = 0 with the next
# EventId 4 and the input as its cause. Queue and counters of the scheduler stay.
TINY_OVERRIDE_ENTRY = entry(4, 0, [], [variant(2, varint(2))], variant(3, param_change(True)))
TINY_OVERRIDDEN = projection(
    -1,
    [
        queue_entry(MONTH, 2, market_round(), MONTH),
        queue_entry(YEAR - 1, 3, birthday(1), None),
        queue_entry(YEAR - 1, 1, year_end(), YEAR),
        TINY_LEAVE_EVENT,
    ]
    + TINY_DEATHS,
    7,
    diseases=True,
)
TINY_SAVE_OVERRIDDEN = (
    header(DATA_HASH)
    + TINY_OVERRIDDEN
    + seq([plan_ref(1, 0), plan_ref(2, 0), plan_ref(1, 0), plan_ref(2, 0), plan_ref(2, 0), plan_ref(2, 0)])
    + varint(5)
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
# (entry 4 at YEAR - 1), the round series and the year end series keep the plan of
# `core:started`, her `leave_home` and her deaths that of her entry 2; the next EventId is
# 5 (the rounds wrote nothing); one year hash.
TINY_YEAR_0_HASH = xxhash.xxh3_64_intdigest(TINY_YEAR_0, seed=0)
TINY_SAVE_AFTER_YEAR_0 = (
    header(DATA_HASH)
    + TINY_AFTER_YEAR_0
    + seq([plan_ref(1, 0), plan_ref(4, YEAR - 1), plan_ref(1, 0), plan_ref(2, 0), plan_ref(2, 0), plan_ref(2, 0)])
    + varint(5)
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
# started, two founders, their two households (each a minor in no couple: the orphan rule,
# part 1), her four birthdays, his five, two deaths; each death ends its household
assert len(MORTAL.log) == 16
assert MORTAL.households == {1: [0, MORTAL_DEATHS[1]], 2: [0, MORTAL_DEATHS[3]]}


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
assert sorted(event.kind for event in TIE.queue) == [world_reference.YEAR_END_KIND, world_reference.MARKET_ROUND_KIND]
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
# This run is long (58.8 years), and the data of "sick" keep the repository's market (they
# are not of the old model; design note of the market, section 7): from her 16th birthday
# (month 208) she is a single woman in the market age, he a single man. She searches first
# in month 297 (at 22.8) and pairs with him, the only candidate (partnership 1, by the
# market); she bears sims 3 to 6 (months 308, 335, 357, 385; sims 4 and 6 die of accident
# in years 34.9 and 40.3), sim 5 (dummy twice) is exposed from the switch on. His death
# ends the partnership. The tie is the same; only the EventId of his death entry is larger
# than before the market (225, was 121).
assert ACCIDENT_WINS.partnerships == {1: [1, 2, 297 * MONTH, (HIS_ACCIDENT, world_reference.ENDED_BY_DEATH)]}
assert [(r["n"], c["woman"], c["chosen"], c["outcome"]) for r in ACCIDENT_WINS.rounds for c in r["choices"]][0] == (297, 1, 2, "paired")
assert ACCIDENT_WINS.rounds[0]["n"] == 208
world_reference.check_market(ACCIDENT_WINS)
world_reference.check_households(ACCIDENT_WINS)

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
SEPARATION_BUDGET_TAG = 18  # tests/golden/rng_tags.txt, design note of the separation
# The separation rate of the repository's data (data/calibration/demography.json), and
# the reasons a partnership ends (EndedBy, design note of the separation, section 2).
SEPARATION_RATE = 0.00867
DEATH_ENDED, SEPARATION_ENDED = 0, 1
assert world_reference.REPOSITORY.separation_rate == SEPARATION_RATE
assert (world_reference.ENDED_BY_DEATH, world_reference.ENDED_BY_SEPARATION) == (DEATH_ENDED, SEPARATION_ENDED)


def hand_separation(seed: int, her: int, k: int, him: int, rate: float, formed: int) -> int:
    """When her partnership `k` (with him, formed at `formed`) separates if nothing else
    happens first, by hand from the design note of the separation (sections 1 and 3) with
    Python's math: `B = −ln(u)` with `u` = unit_open0 of the draw SEPARATION_BUDGET at
    (her key, `k`, his key), due `formed + max(ceil(B / λ · YEAR), 1)`."""
    u = unit_open0(words(chacha_key(seed, SEPARATION_BUDGET_TAG, her, k, him))[0])
    budget = 0.0 - math.log(u)
    years = budget / rate
    assert years < 100_000.0
    return formed + max(math.ceil(years * 31_449_600.0), 1)



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
# two men (dummy twice; dummy, wild) born 16 years and one, two and three seconds before
# the start (design note of the market, section 7: partnerships never begin below 16, so
# the founders that were one to three seconds old became 16; all else stays).
# `Start` pairs her with the first man (partnership 1). He dies of senescence after about
# 0.2 years, which ends it; at the start of year 1 `ArrangePartnership` pairs her with the
# second man (partnership 2, by the input with sequence number 2; accepted: both are 17).
# She dies of accident after about 1.86 years, which ends it; he lives on (senescence
# after about 12.8 years).
# The test data: accident 0.02 per year, senescence `a` = 0.5 and `b` = 0.001 for both
# sexes (the data of "fever"), the repository's diseases and fertility (design note of the
# fertility in the world, section 8). The rates of its old causes
# (those of "mortal": accident 0.5, `a` = 0.05) cannot keep the idea: with the new budgets
# she would die first, after 0.07 years, and nobody could be paired with her at year 1.
# Fertility (5.6, 11.3): her wish is 3, so each partnership plans her monthly series, the
# first from month 1 (the first month start after the start), ended by his death in month
# 2.6 after the attempts of months 1 and 2; the second from month 14 (the first month start
# after the second YEAR in which the arrangement acts), ended by her death in month 24.2.
# She is 16 (her menarche about 13.13 years, her ramp of three years ends at about 16.13),
# so the chance of month 1 is a little below 0.2 (her factor about 0.9992; his ramp is
# over) and every later one 0.2: thirteen attempts draw and fail, nobody conceives (by
# hand: no draw of these months lies below 0.2).
# Market (design note of the market): the repository's values (`c` 0.01179). From his
# death (month 2.6) to the arrangement she is a single woman in the market age, so the
# rounds of months 3 to 12 draw her search (with `ρ`, she is under 18, about 0.00045
# each); none says yes. Afterwards no single woman is left.
# Separation (design note of the separation, section 7): the data keep the repository's
# rate 0.00867 per year, so each partnership plans its separation after her series, by its
# entry `core:partnered`: partnership 1 (her `k` 0) after about 1.72 years, partnership 2
# (her `k` 1, from year 1) about 24.6 years after it forms. Each death comes first and
# cancels it, so the story stands; the two events take insertion numbers.
PARTNERS_FOUNDERS = [
    (-16 * YEAR - 1, FEMALE, genotype(WILD, DUMMY)),
    (-16 * YEAR - 2, MALE, genotype(DUMMY, DUMMY)),
    (-16 * YEAR - 3, MALE, genotype(DUMMY, WILD)),
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
# Her separations by hand (design note of the separation, sections 1 and 3).
PARTNERS_SEPARATIONS = [
    hand_separation(0xFF, PARTNERS_KEYS[0], 0, PARTNERS_KEYS[1], SEPARATION_RATE, 0),
    hand_separation(0xFF, PARTNERS_KEYS[0], 1, PARTNERS_KEYS[2], SEPARATION_RATE, YEAR),
]
assert PARTNERS_DATA.separation_rate == SEPARATION_RATE
assert PARTNERS_DEATHS[1] < PARTNERS_SEPARATIONS[0] and PARTNERS_DEATHS[0] < PARTNERS_SEPARATIONS[1]
assert PARTNERS.pending_separation(1) == PARTNERS_SEPARATIONS[0]
assert PARTNERS.sims[1][8] == 1 and PARTNERS.sims[2][8] == 0
PARTNERS_SAVE_AFTER_START = PARTNERS.save()
# Started, three founders, then the partnership.
PARTNERS_STARTED_ENTRY = PARTNERS.log[4]
PARTNERS.advance(YEAR - 1)
assert PARTNERS.partnerships == {1: [1, 2, 0, (PARTNERS_DEATHS[1], DEATH_ENDED)]}
assert PARTNERS.archive[2][2] == SENESCENCE_KEY
assert PARTNERS.pending_series(1) is None and PARTNERS.pending_separation(1) is None
PARTNERS.arrange(1, 3)
PARTNERS_ARRANGED_ENTRY = PARTNERS.log[-1]
assert PARTNERS.pending_series(1) == 14 * MONTH
assert PARTNERS.pending_separation(2) == PARTNERS_SEPARATIONS[1] and PARTNERS.sims[1][8] == 2
world_reference.check_separations(PARTNERS)
world_reference.check_market(PARTNERS)
world_reference.check_households(PARTNERS)
PARTNERS_SAVE_AFTER_ARRANGE = PARTNERS.save()
PARTNERS.advance(5 * YEAR - 1)
assert PARTNERS.partnerships == {
    1: [1, 2, 0, (PARTNERS_DEATHS[1], DEATH_ENDED)],
    2: [1, 3, YEAR, (PARTNERS_DEATHS[0], DEATH_ENDED)],
}
assert not PARTNERS.separations and PARTNERS.pending_separation(2) is None
world_reference.check_separations(PARTNERS)
assert sorted(PARTNERS.sims) == [3] and PARTNERS.archive[1][2] == ACCIDENT_KEY
assert [(t // MONTH, success) for t, _, _, _, success in PARTNERS.attempts] == [
    (month, False) for month in [1, 2] + list(range(14, 25))
]
assert 0.199 < PARTNERS.attempts[0][3] < 0.2 and all(p == 0.2 for _, _, _, p, _ in PARTNERS.attempts[1:])
assert hand_conceptions(0xFF, PARTNERS_KEYS[0], PARTNERS_KEYS[1], 1, lambda k: 0.2, 12, 10, 3, 2) == []
assert hand_conceptions(0xFF, PARTNERS_KEYS[0], PARTNERS_KEYS[2], 14, lambda k: 0.2, 12, 10, 3, 24) == []
# The rounds with a single woman in the market age: months 3 to 12, she never searches.
assert [r["n"] for r in PARTNERS.rounds] == list(range(3, 13))
assert all([(sim, searched) for sim, _, _, _, searched, _ in r["considered"]] == [(1, False)] for r in PARTNERS.rounds)
assert all(r["men"] == [3] and not r["choices"] for r in PARTNERS.rounds)
world_reference.check_market(PARTNERS)
world_reference.check_households(PARTNERS)
assert PARTNERS.archive[1][7:9] == (0, 0) and PARTNERS_DEATHS[1] < 3 * MONTH and 24 * MONTH < PARTNERS_DEATHS[0] < 25 * MONTH
PARTNERS_SAVE_AFTER_YEAR_4 = PARTNERS.save()


# Scenario "clan": seed 0xff, founder tag `test:clan`, on the test data of its old cause
# (`test:base` 0.02 per year): accident 0.02 per year, senescence practically off, the
# repository's diseases, and its old fertility as data (design note of the fertility in
# the world, section 8): the form `window` from 18 until 45 years, 0.25 per month, ten
# months of pregnancy, no pause, the wish always 40, no twins. Three founder couples: a
# woman of 20 (wild, dummy) and a man of 22 (dummy, wild); a girl of 16 (dummy twice; she
# was 12 before the market, design note of the market, section 7: partnerships never begin
# below 16) and a man of 30 (wild twice); a woman and a man of 46 (wild twice). The first
# woman's series begins in month 1 (the first month start after the start); she conceives
# in month 5 and has the child (sim 7) in month 15; without a pause her series begins
# again in month 16, and by the end of year 7 she has conceived eight times (months 5, 17,
# 28, 42, 59, 74, 90, 101), borne seven children (births 7: sims 7, 8, 10, 11, 14, 16, 17)
# and is pregnant with the eighth. The girl's series begins in month 1 as well and draws
# with chance 0 until her 18th birthday (month 26, two years after the start), then with
# 0.25: she conceives in months 26, 42, 58 and 70 and bears sims 9, 12, 13 and 15 in
# months 36, 52, 68 and 80 (in month 52 both women give birth, the first woman's child
# first: subject sim 1 before sim 3); she dies of accident in month 83.9, not pregnant,
# which ends her partnership and her series. The first child dies of accident in month
# 80.5, the man of 46 in month 98.7. The woman of 46 is past the end of the window (45
# years): no series is planned for her. Every woman's wish is 40. The test data of the old
# model know no separation (`core:separation_rate` 0, design note of the separation,
# section 7): no separation is planned, every founder woman has `partnerships` 1, and the
# partnerships of the girl and of the woman of 46 end by death. Nor do they know the
# market (`c` = 0, design note of the market, section 7): the round runs every month and
# writes nothing; no woman searches (the widow of 46 is the only single woman in the
# market age, from month 99 on).
CLAN_FOUNDERS = [
    (-20 * YEAR, FEMALE, genotype(WILD, DUMMY)),
    (-22 * YEAR, MALE, genotype(DUMMY, WILD)),
    (-16 * YEAR, FEMALE, genotype(DUMMY, DUMMY)),
    (-30 * YEAR, MALE, genotype(WILD, WILD)),
    (-46 * YEAR, FEMALE, genotype(WILD, WILD)),
    (-46 * YEAR, MALE, genotype(WILD, WILD)),
]
CLAN_KEYS = [founder_key("test:clan", index) for index in range(6)]
CLAN_COUPLES = [(0, 1), (2, 3), (4, 5)]
CLAN_FERTILITY = (0.25, 18, 45, 10)
CLAN_DATA = world_reference.death_test_data(0.02, fertility=CLAN_FERTILITY)
# By hand: the first woman's conceptions over the eight years (20 years old at the start,
# the window open), and the girl's up to her death in month 83 (16 at the start, chance 0
# before month 26).
CLAN_HAND = hand_conceptions(0xFF, CLAN_KEYS[0], CLAN_KEYS[1], 1, lambda k: 0.25 if 18 <= 20 + k / 13 < 45 else 0.0, 0, 10, 40, 8 * 13 - 1)
assert CLAN_HAND == [(5, 15), (17, 27), (28, 38), (42, 52), (59, 69), (74, 84), (90, 100), (101, 111)]
GIRL_HAND = hand_conceptions(0xFF, CLAN_KEYS[2], CLAN_KEYS[3], 1, lambda k: 0.25 if 16 + k / 13 >= 18 else 0.0, 0, 10, 40, 83)
assert GIRL_HAND == [(26, 36), (42, 52), (58, 68), (70, 80)]
# Her months with an attempt: from month 1 to each conception, again from the month after
# each birth (no pause), up to her death in month 83.
GIRL_ATTEMPT_MONTHS = list(range(1, 27)) + list(range(37, 43)) + list(range(53, 59)) + list(range(69, 71)) + list(range(81, 84))


def clan_world(data=CLAN_DATA, on: bool = False, market: bool = True) -> world_reference.World:
    return world_reference.World(0xFF, "test:clan", on, CLAN_FOUNDERS, CLAN_COUPLES, data=data, market=market)


def deaths_in(world: world_reference.World) -> list:
    """(time, sim, key of the cause) of every death, in time order."""
    return sorted((left, sim, cause) for sim, (_, left, cause, *_) in world.archive.items())


CLAN = clan_world()
# The first two women have their series from month 1, the woman of 46 none.
assert [CLAN.pending_series(sim) for sim in (1, 3, 5)] == [MONTH, MONTH, None]
assert CLAN_DATA.separation_rate == 0.0 and all(due is None for *_, due in CLAN.separation_plans)
assert [CLAN.sims[sim][8] for sim in range(1, 7)] == [1, 0, 1, 0, 1, 0]
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
assert [t // MONTH for t, woman, _, _, success in CLAN.attempts if success and woman == 1] == [c for c, _ in CLAN_HAND]
# The girl's attempts: chance 0 before her 18th birthday, 0.25 from it; her conceptions as
# by hand.
assert [(t // MONTH, p) for t, woman, _, p, _ in CLAN.attempts if woman == 3] == [
    (month, 0.0 if month < 26 else 0.25) for month in GIRL_ATTEMPT_MONTHS
]
assert [t // MONTH for t, woman, _, _, success in CLAN.attempts if success and woman == 3] == [c for c, _ in GIRL_HAND]
CLAN_DEATHS = deaths_in(CLAN)
assert [(sim, cause) for _, sim, cause in CLAN_DEATHS] == [(7, ACCIDENT_KEY), (3, ACCIDENT_KEY), (6, ACCIDENT_KEY)]
# The girl died after her fourth birth (month 80), not pregnant.
assert 80 * MONTH < CLAN.archive[3][1] < 84 * MONTH and CLAN.archive[3][7:9] == (4, 4) and 3 not in CLAN.pregnancies


def children_of(world: world_reference.World, mother: int) -> list:
    """(SimId, born) of her children, living or archived, by SimId."""
    rows = [(sim, value[0]) for sim, value in world.sims.items() if value[7] is not None and value[7][0] == mother]
    rows += [(sim, value[0]) for sim, value in world.archive.items() if value[9] is not None and value[9][0] == mother]
    return sorted(rows)


assert children_of(CLAN, 1) == list(zip([7, 8, 10, 11, 14, 16, 17], [b * MONTH for _, b in CLAN_HAND[:7]]))
assert children_of(CLAN, 3) == list(zip([9, 12, 13, 15], [b * MONTH for _, b in GIRL_HAND]))
assert CLAN.partnerships == {
    1: [1, 2, 0, None],
    2: [3, 4, 0, (CLAN.archive[3][1], DEATH_ENDED)],
    3: [5, 6, 0, (CLAN.archive[6][1], DEATH_ENDED)],
}
assert not CLAN.separations and CLAN.archive[7][10] == 0 and all(CLAN.sims[sim][8] == 0 for sim in range(8, 18))
world_reference.check_separations(CLAN)
world_reference.check_market(CLAN)
world_reference.check_households(CLAN)
# The rounds: every month, 103 in the eight years (cadence 13 a year, test rule 6); a
# single woman in the market age only from the death of the man of 46 on, she never
# searches (`c` = 0).
assert [kind for _, kind in CLAN.ran].count(world_reference.MARKET_ROUND_KIND) == 8 * 13 - 1
assert [r["n"] for r in CLAN.rounds] == list(range(99, 104))
assert all([(sim, p, searched) for sim, _, p, _, searched, _ in r["considered"]] == [(5, 0.0, False)] for r in CLAN.rounds)
CLAN_SAVE_AFTER_YEAR_7 = CLAN.save()

# The null twin of the market (test rule 8; design note of the market, section 7): with
# `c` = 0 and `λ_sep` = 0 the round changes nothing but the queue. "clan" with the round
# switched off (a switch of this reference only, no round planned) has the same log and
# the same census at every year end (the projection without the queue).
CLAN_NULL = clan_world(market=False)
CLAN_NULL.advance(8 * YEAR - 1)
assert CLAN_DATA.market_height == 0.0 and CLAN_DATA.separation_rate == 0.0
assert CLAN_NULL.log == CLAN.log and CLAN_NULL.census_hashes == CLAN.census_hashes
assert CLAN_NULL.census() == CLAN.census() and CLAN_NULL.year_hashes != CLAN.year_hashes
assert not [kind for _, kind in CLAN_NULL.ran if kind == world_reference.MARKET_ROUND_KIND]
world_reference.check_market(CLAN_NULL)
world_reference.check_households(CLAN_NULL)

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
world_reference.check_separations(FEVER)
world_reference.check_market(FEVER)
world_reference.check_households(FEVER)
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
world_reference.check_separations(BLIGHT)
world_reference.check_market(BLIGHT)
world_reference.check_households(BLIGHT)
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
# month 28 (a daughter and a son, sims 13 and 14, month 38) and in month 42 (two
# daughters, born in month 52, after the end of year 3). The girl (16 at the start since
# the market) conceives in the months of "clan" too, 26 and 42: fraternal twins both
# times, sims 11 and 12 born in month 36 (before the first woman's third pair). Child 0 of
# the first conception has the key of the first child of "clan". Saves after the first
# conception, after the first birth and at the end of year 3 (the first woman pregnant
# with her fourth conception, births 6; the girl with her second, births 2).
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
# The end of year 3: the first woman's conceptions in the months of "clan", three twin
# births (sims 7 and 8, 9 and 10, 13 and 14), pregnant with the fourth conception; the
# girl's in the months of "clan" too (26 and 42), fraternal twins both times (by hand),
# sims 11 and 12 born in month 36, pregnant with the second; nobody has died.
TWINS_GIRL_HAND = [hand_twins(0xFF, CLAN_KEYS[2], CLAN_KEYS[3], number, TWINS_RATES) for number in range(2)]
assert [kind for _, kind, _ in TWINS_GIRL_HAND] == ["fraternal", "fraternal"]
assert [t // MONTH for t, woman, *_ in TWINS.multiples if woman == 1] == [c for c, _ in CLAN_HAND[:4]]
assert [t // MONTH for t, woman, *_ in TWINS.multiples if woman == 3] == [c for c, _ in GIRL_HAND[:2]]
assert [(u, kind, [key for key, _ in children]) for _, woman, *_, u, kind, children in TWINS.multiples if woman == 1] == TWINS_HAND
assert [(u, kind, [key for key, _ in children]) for _, woman, *_, u, kind, children in TWINS.multiples if woman == 3] == TWINS_GIRL_HAND
assert sorted(TWINS.sims) == list(range(1, 15)) and not TWINS.archive
TWINS_FIRST_WOMAN = [7, 8, 9, 10, 13, 14]
assert [TWINS.sims[sim][0] for sim in TWINS_FIRST_WOMAN] == [b * MONTH for _, b in CLAN_HAND[:3] for _ in range(2)]
assert [TWINS.sims[sim][7] for sim in TWINS_FIRST_WOMAN] == [(1, 2, n) for n in (0, 0, 1, 1, 2, 2)]
assert [(TWINS.sims[sim][0], TWINS.sims[sim][7]) for sim in (11, 12)] == [(36 * MONTH, (3, 4, 0))] * 2
assert TWINS.sims[1][5:7] == [4, 6] and TWINS.pregnancies[1][1] == 3 and TWINS.pregnancies[1][3] == 42 * MONTH
assert TWINS.sims[3][5:7] == [2, 2] and TWINS.pregnancies[3][1] == 1 and TWINS.pregnancies[3][3] == 42 * MONTH
world_reference.check_separations(TWINS)
world_reference.check_market(TWINS)
world_reference.check_households(TWINS)
TWINS_SAVE_AFTER_YEAR_3 = TWINS.save()


def twins_story(world: world_reference.World) -> list:
    """Per conception: the month, the woman, the conception number, the draw u, the kind,
    the keys and sexes of the children."""
    rows = []
    for t, woman, man, number, u, kind, children in world.multiples:
        kids = ", ".join(f"{key:016x} {'female' if sex == FEMALE else 'male'}" for key, sex in children)
        rows.append(f"twins_conception month {t // MONTH} woman {woman} man {man} number {number} u {u!r} {kind}: {kids}")
    return rows


# Scenario "split" (design note of the separation, section 7): "clan" (seed 0xff, founder
# tag `test:clan`, its founders and couples, the switch off) on the test data of "clan"
# with the separation rate `λ`, and one input `ArrangePartnership` that pairs the first
# woman (sim 1) anew with the man of her first partnership (sim 2). The reference picks
# `λ`, the first of 0.05, 0.10, ..., 1.00 that gives in the first eight years: (a)
# partnership 1 ends by separation while she is pregnant (the child is born after it, with
# father sim 2, and no series follows the birth); (b) the girl's partnership 2 is still
# open at her death (her accident in month 83.9), which cancels its separation; (c) the
# input, acting at the first month start after that birth (stamped one second before,
# as in "partners"), is accepted: her `k` is 1 for the new partnership 4, which plans a
# new separation. 0.05 gives all three (so do 0.15 and 0.20; 0.10 and 0.25 separate her
# between two pregnancies, 0.30 to 0.80 separate the girl before her death, from 0.85 on
# partnership 1 ends before her first conception). With `λ` = 0.05 the budgets of the
# three couples are about 0.311, 1.715 and 0.884: partnership 1 is due to separate in
# month 80.93, partnership 2 in month 445.8, partnership 3 in month 229.9 (each planned
# right after the series of its woman). Up to month 80 the story is that of "clan" (the
# draws do not read `λ`); the first child dies in month 80.5. In month 80.93 partnership 1
# ends by separation while the first woman is pregnant since month 74 (one entry
# `core:separated`, the draw (18, her key, 0, his key)). The girl dies in month 83.86 with
# partnership 2 open (she has borne four children by then, as in "clan"), which cancels
# its separation. The child (sim 16) is born in month 84 with father sim 2; no series
# follows. The input acts at month 85 (stamp one second
# before, sequence 2): partnership 4 of sim 1 and sim 2, her `k` 1 (her `partnerships`
# goes to 2), her series from month 86 and its separation in month 156.8 (budget about
# 0.276), after the run. She conceives in month 90 (sim 17, born in month 100) and in
# month 101, as in "clan" (the same pair, the same draws). The man of 46 dies in month
# 98.7, which cancels the separation of partnership 3. The market (`c` = 0 on these data)
# lets nobody search: the first woman and sim 2 are single from month 80.93 to 85 and are
# not paired by it. Saves after the start, after the separation (its second closed), right
# after the re-pairing and at the end of year 7.
SPLIT_CANDIDATES = [index / 20 for index in range(1, 21)]
assert SPLIT_CANDIDATES[0] == 0.05 and SPLIT_CANDIDATES[1] == 0.10 and SPLIT_CANDIDATES[-1] == 1.0
SPLIT_END = 8 * YEAR - 1


def split_data(rate: float) -> world_reference.Data:
    """The test data of "clan" with `core:separation_rate` = `rate`."""
    return world_reference.death_test_data(0.02, fertility=CLAN_FERTILITY, life_values={"core:separation_rate": rate})


def split_parts(rate: float) -> tuple:
    """((a), (b), (c)) for `rate` as booleans, and the time the input acts (None without
    (a)): first a run without the input up to the end of year 7, then one with it."""
    data = split_data(rate)
    world = clan_world(data)
    due = world.pending_separation(1)
    if due is None or due > SPLIT_END:
        a, birth = False, None
    else:
        world.advance(due)
        pregnant = world.partnerships[1][3] == (due, SEPARATION_ENDED) and 1 in world.pregnancies
        birth = world.pregnancies[1][3] + data.pregnancy_months * MONTH if pregnant else None
        if birth is not None and birth <= SPLIT_END:
            world.advance(birth)
            born = [sim for sim, value in world.sims.items() if value[0] == birth and value[7][:2] == (1, 2)]
            a = bool(born) and world.pending_series(1) is None
        else:
            a = False
    world.advance(SPLIT_END)
    girl = world.archive.get(3)
    b = girl is not None and world.partnerships[2][3] == (girl[1], DEATH_ENDED) and world.separation_due(2) is not None
    if not a:
        return (a, b, False), None
    at = world_reference.month_start_after(birth)
    if at > SPLIT_END:
        return (a, b, False), None
    again = clan_world(data)
    again.advance(at - 1)
    alive = 1 in again.sims and 2 in again.sims
    single = alive and again.open_partnership(1) is None and again.open_partnership(2) is None
    if not single:
        return (a, b, False), at
    again.arrange(1, 2)
    c = again.next_partnership_id == 5 and again.sims[1][8] == 2 and again.pending_separation(4) is not None
    return (a, b, c), at


SPLIT_SEARCH = [(rate, *split_parts(rate)) for rate in SPLIT_CANDIDATES]
SPLIT_FULL = [row for row in SPLIT_SEARCH if all(row[1])]
# The first `λ` with all three parts, else the one with the most (the first of those).
SPLIT_RATE, SPLIT_PARTS, SPLIT_INPUT_AT = (SPLIT_FULL or [max(SPLIT_SEARCH, key=lambda row: sum(row[1]))])[0]
SPLIT_MISSING = [name for name, got in zip(("a", "b", "c"), SPLIT_PARTS) if not got]
SPLIT_DATA = split_data(SPLIT_RATE)
assert SPLIT_DATA.separation_rate == SPLIT_RATE


def split_world() -> world_reference.World:
    return clan_world(SPLIT_DATA)


SPLIT = split_world()
# Each partnership plans its separation by hand: partnership n of the couples, her `k` 0.
SPLIT_DUES = [
    hand_separation(0xFF, CLAN_KEYS[woman], 0, CLAN_KEYS[man], SPLIT_RATE, 0) for woman, man in CLAN_COUPLES
]
assert [SPLIT.pending_separation(number) for number in (1, 2, 3)] == SPLIT_DUES
assert [SPLIT.sims[sim][8] for sim in range(1, 7)] == [1, 0, 1, 0, 1, 0]
# The separation is planned after her series (insertion numbers in this order).
(SPLIT_SERIES_1,) = [e for e in SPLIT.queue if e.kind == world_reference.ATTEMPT_KIND and e.subject[1] == 1]
(SPLIT_SEPARATION_1,) = [e for e in SPLIT.queue if e.kind == world_reference.SEPARATION_KIND and e.subject[1] == 1]
assert SPLIT_SERIES_1.insertion + 1 == SPLIT_SEPARATION_1.insertion and SPLIT_SEPARATION_1.plan == (SPLIT_SERIES_1.plan[0], 0)
world_reference.check_separations(SPLIT)
world_reference.check_market(SPLIT)
world_reference.check_households(SPLIT)
SPLIT_SAVE_AFTER_START = SPLIT.save()
SPLIT_SEPARATED_AT = SPLIT_DUES[0]
SPLIT.advance(SPLIT_SEPARATED_AT - 1)
assert 1 in SPLIT.pregnancies and SPLIT.partnerships[1][3] is None
SPLIT_LOG_BEFORE = len(SPLIT.log)
SPLIT.advance(SPLIT_SEPARATED_AT)
SPLIT_SEPARATION_ENTRIES = SPLIT.log[SPLIT_LOG_BEFORE:]
SPLIT_HIS_HOUSEHOLD = SPLIT.sims[2][9]
assert SPLIT.households[SPLIT_HIS_HOUSEHOLD] == [SPLIT_SEPARATED_AT, None] and SPLIT.members(SPLIT_HIS_HOUSEHOLD) == [2]
assert all(SPLIT.sims[sim][11] == 1 for sim in SPLIT.members(SPLIT.sims[1][9]) if SPLIT.dependent(sim, SPLIT_SEPARATED_AT))
assert SPLIT.partnerships[1][3] == (SPLIT_SEPARATED_AT, SEPARATION_ENDED)
assert 1 in SPLIT.pregnancies and SPLIT.pending_series(1) is None and SPLIT.pending_separation(1) is None
world_reference.check_separations(SPLIT)
world_reference.check_market(SPLIT)
world_reference.check_households(SPLIT)
SPLIT_SAVE_AFTER_SEPARATION = SPLIT.save()
SPLIT_PREGNANCY = SPLIT.pregnancies[1]
SPLIT_BIRTH = SPLIT_PREGNANCY[3] + SPLIT_DATA.pregnancy_months * MONTH
SPLIT.advance(SPLIT_INPUT_AT - 1)
SPLIT_CHILD = max(SPLIT.sims)
assert SPLIT.sims[SPLIT_CHILD][0] == SPLIT_BIRTH and SPLIT.sims[SPLIT_CHILD][7][:2] == (1, 2)
assert 1 not in SPLIT.pregnancies and SPLIT.pending_series(1) is None
SPLIT.arrange(1, 2)
SPLIT_REPAIRED_ENTRY = SPLIT.log[-1]
assert SPLIT.partnerships[4] == [1, 2, SPLIT_INPUT_AT, None] and SPLIT.sims[1][8] == 2 and SPLIT.sims[2][8] == 0
SPLIT_DUE_4 = hand_separation(0xFF, CLAN_KEYS[0], 1, CLAN_KEYS[1], SPLIT_RATE, SPLIT_INPUT_AT)
assert SPLIT.pending_separation(4) == SPLIT_DUE_4
assert SPLIT.pending_series(1) == world_reference.month_start_after(SPLIT_INPUT_AT)
world_reference.check_separations(SPLIT)
world_reference.check_market(SPLIT)
world_reference.check_households(SPLIT)
SPLIT_SAVE_AFTER_REPAIRING = SPLIT.save()
SPLIT.advance(SPLIT_END)
world_reference.check_separations(SPLIT)
world_reference.check_market(SPLIT)
world_reference.check_households(SPLIT)
SPLIT_SAVE_AFTER_YEAR_7 = SPLIT.save()
SPLIT_DEATHS = deaths_in(SPLIT)


def separated_entry(entry_id: int, t: int, woman: int, man: int, plan: tuple, key: tuple, number: int) -> bytes:
    """The entry `core:separated { partnership }` by hand (design note of the separation,
    section 4; ADR-0004 section 7): subjects the woman and the man (Subject::Sim 0), causes
    Plan (1) { by, planned_at } and Random (4) { tag, subject, occasion, detail }, the
    payload variant 9 (the last)."""
    causes = [variant(1, varint(plan[0]), signed(plan[1])), variant(4, *(varint(word) for word in key))]
    return entry(entry_id, t, [variant(0, varint(woman)), variant(0, varint(man))], causes, variant(9, varint(number)))


# The entries `core:separated` of the whole run, each as by hand: the plan is the entry
# `core:partnered` of its partnership at its formation, the draw at (18, her key, `k`, his
# key).
def split_separated(world: world_reference.World) -> list:
    """(partnership, entry bytes, fields) of each entry `core:separated` of the run, the
    bytes checked against those by hand."""
    found = []
    for t, number, entry_id, plan, key in world.separations:
        woman, man, formed, _ = world.partnerships[number]
        k = sum(1 for other, value in world.partnerships.items() if other < number and value[0] == woman)
        assert plan[1] == formed and world.log[plan[0] - 1].endswith(variant(5, varint(number)))
        assert key == (SEPARATION_BUDGET_TAG, world.key_of(woman), k, world.key_of(man))
        line = separated_entry(entry_id, t, woman, man, plan, key, number)
        assert world.log[entry_id - 1] == line
        found.append((number, line, (entry_id, t, woman, man, plan, key, number)))
    return found


SPLIT_SEPARATED = split_separated(SPLIT)
if all(SPLIT_PARTS):
    # (a) the separation while pregnant, the child of sim 2 after it, no series after the
    # birth; (b) the girl's partnership ended by her death, its separation cancelled; (c)
    # the new partnership with her `k` 1. The separation writes `core:separated`, then
    # sim 2 founds a household of his own (`core:household_formed`, `core:moved`, causes
    # the entry `core:separated` and the rule); every dependent child stays with sim 1.
    assert len(SPLIT_SEPARATION_ENTRIES) == 3 and SPLIT_SEPARATION_ENTRIES[0] == SPLIT_SEPARATED[0][1]
    _separated_id = SPLIT_SEPARATED[0][2][0]
    _home_causes = [variant(0, varint(_separated_id)), HOUSEHOLD_RULE]
    assert SPLIT_SEPARATION_ENTRIES[1:] == [
        entry(_separated_id + 1, SPLIT_SEPARATED_AT, [variant(0, varint(2))], _home_causes, variant(10, varint(SPLIT_HIS_HOUSEHOLD))),
        entry(_separated_id + 2, SPLIT_SEPARATED_AT, [variant(0, varint(2))], _home_causes, variant(11, varint(SPLIT_HIS_HOUSEHOLD))),
    ]
    assert SPLIT.partnerships[2][3] == (SPLIT.archive[3][1], DEATH_ENDED) and SPLIT.archive[3][1] < SPLIT_DUES[1]
    assert [plan for plan in SPLIT.separation_plans if plan[0] == 4][0][1] == 1


def split_story() -> list:
    """The story of "split" as readable rows: the search, the separations planned (with `k`
    and the budget), the separations that came, the conceptions and births of sim 1, the
    deaths and how each partnership ended."""
    rows = []
    for rate, parts, at in SPLIT_SEARCH:
        input_text = "none" if at is None else f"{at} (month {at / MONTH:.4f})"
        rows.append(f"split_search lambda {rate!r} parts (a, b, c) {parts} input_at {input_text}")
    rows.append(f"split_lambda {SPLIT_RATE!r} parts {SPLIT_PARTS} missing {SPLIT_MISSING or 'none'}")
    rows.append(f"split_input acts at {SPLIT_INPUT_AT} (month {SPLIT_INPUT_AT / MONTH:.4f}), stamp {SPLIT_INPUT_AT - 1}, sequence 2")
    for number, k, budget, due in SPLIT.separation_plans:
        woman, man, formed, ended = SPLIT.partnerships[number]
        due_text = "never" if due is None else f"{due} (month {due / MONTH:.4f})"
        rows.append(
            f"split_partnership_{number} woman {woman} man {man} formed {formed} k {k} budget {budget!r}"
            f" separation_due {due_text} ended {split_ended(ended)}"
        )
    for t, number, entry_id, _, _ in SPLIT.separations:
        rows.append(f"split_separation partnership {number} at {t} (month {t / MONTH:.4f}) entry {entry_id}")
    father, conception, _, conceived = SPLIT_PREGNANCY
    rows.append(
        f"split_pregnant_at_separation father {father} conception {conception} conceived month {conceived // MONTH}"
        f" birth month {SPLIT_BIRTH // MONTH} child sim {SPLIT_CHILD}; no series after the birth"
    )
    for number, k, budget, due in SPLIT.separation_plans:
        woman, man, formed, ended = SPLIT.partnerships[number]
        if due is not None and ended is not None and ended[1] == DEATH_ENDED:
            dead = [sim for sim in (woman, man) if sim in SPLIT.archive and SPLIT.archive[sim][1] == ended[0]]
            rows.append(
                f"split_cancelled partnership {number} separation due month {due / MONTH:.4f} cancelled by the death of"
                f" sim {dead[0]} in month {ended[0] / MONTH:.4f}"
            )
    rows.append(
        f"split_repairing partnership 4 woman 1 man 2 at month {SPLIT_INPUT_AT // MONTH} her k 1 (partnerships 1 -> 2)"
        f" series from month {world_reference.month_start_after(SPLIT_INPUT_AT) // MONTH}"
        f" separation due {SPLIT_DUE_4} (month {SPLIT_DUE_4 / MONTH:.4f})"
    )
    for t, woman, man, number, u, kind, children in SPLIT.multiples:
        rows.append(f"split_conception month {t // MONTH} woman {woman} man {man} number {number} {kind}")
    for sim in sorted(set(SPLIT.sims) | set(SPLIT.archive)):
        value = SPLIT.sims[sim] if sim in SPLIT.sims else SPLIT.archive[sim]
        parents = value[7] if sim in SPLIT.sims else value[9]
        if parents is not None:
            born = value[0]
            rows.append(f"split_child {sim} born {born} (month {born / MONTH:.4f}) parents {parents}")
    for left, sim, cause in SPLIT_DEATHS:
        rows.append(f"split_death sim {sim} at {left} (month {left / MONTH:.4f}) of {cause_name(cause)}")
    rows.append(f"split_her_counters conceptions {SPLIT.sims[1][5]} births {SPLIT.sims[1][6]} partnerships {SPLIT.sims[1][8]}")
    pregnancy = SPLIT.pregnancies.get(1)
    rows.append(
        "split_end_of_year_7 pregnant "
        + ("no" if pregnancy is None else f"father {pregnancy[0]} conception {pregnancy[1]} conceived month {pregnancy[3] // MONTH}")
        + f" pending_series {SPLIT.pending_series(1)} pending_separation_4 {SPLIT.pending_separation(4)}"
    )
    return rows


def split_ended(ended) -> str:
    if ended is None:
        return "open"
    at, by = ended
    return f"{at} (month {at / MONTH:.4f}) by {'separation' if by == SEPARATION_ENDED else 'death'}"


def separated_json(entry_id: int, t: int, woman: int, man: int, plan: tuple, key: tuple, number: int) -> str:
    """The JSON form of a `core:separated` entry as ADR-0004 sections 3 and 7 describe it
    (the IDs and times as numbers, the four fields of the draw as 16 lower hex digits, the
    payload externally tagged by its code); written from the ADR, not from the CLI."""
    return json.dumps(
        {
            "id": entry_id,
            "t": t,
            "subjects": [{"sim": woman}, {"sim": man}],
            "causes": [
                {"plan": {"by": plan[0], "planned_at": plan[1]}},
                {"random": {name: f"{word:016x}" for name, word in zip(("tag", "subject", "occasion", "detail"), key)}},
            ],
            "payload": {"core:separated": {"partnership": number}},
        },
        separators=(",", ":"),
    )


# Scenario "market" (design note of the market, section 7): a small town on the
# repository's data with only `c` (`core:market_search_height_female`) raised, so that
# rounds have several searchers. Seed 0xff, founder tag `test:market`, the switch off, ten
# founders with reference alleles only, forty years. Three founder couples whose children
# reach 16 in the run (sims 1 and 2: a woman of 20 and a man of 24; sims 3 and 4: 21 and
# 25; sims 5 and 6: 22 and 23), and four single founders: two women of 20 and 21 (sims 7
# and 8) and two men of 25 and 40 (sims 9 and 10). The reference picks `c`, the first of
# 0.5, 1.0, ..., 5.0 with which the forty years reach all six situations of the note: a
# round with at least two searchers; a man chosen by two women in one round (the second
# gets nobody); a skipped candidate with `φ ≥ 1/8` who is her brother; a first cousin in a
# list (`φ` = 1/16, his penalty `16 · ln(10) · φ = ln(10)`); a woman of 16 or 17 searching
# (with `ρ`); and a man under 16 never a candidate (every round with a searcher while a
# boy under 16 lives leaves him out; a boy enters the list in the first round on or after
# his 16th birthday). The story is printed by `market_story`.
MARKET_TAG = "test:market"
MARKET_FOUNDERS = [
    (-20 * YEAR, FEMALE, genotype(WILD, WILD)),
    (-24 * YEAR, MALE, genotype(WILD, WILD)),
    (-21 * YEAR, FEMALE, genotype(WILD, WILD)),
    (-25 * YEAR, MALE, genotype(WILD, WILD)),
    (-22 * YEAR, FEMALE, genotype(WILD, WILD)),
    (-23 * YEAR, MALE, genotype(WILD, WILD)),
    (-20 * YEAR, FEMALE, genotype(WILD, WILD)),
    (-21 * YEAR, FEMALE, genotype(WILD, WILD)),
    (-25 * YEAR, MALE, genotype(WILD, WILD)),
    (-40 * YEAR, MALE, genotype(WILD, WILD)),
]
MARKET_COUPLES = [(0, 1), (2, 3), (4, 5)]
MARKET_KEYS = [founder_key(MARKET_TAG, index) for index in range(len(MARKET_FOUNDERS))]
MARKET_YEARS = 40
MARKET_END = MARKET_YEARS * YEAR - 1
MARKET_HEIGHTS = [index / 2 for index in range(1, 11)]
assert MARKET_HEIGHTS[0] == 0.5 and MARKET_HEIGHTS[-1] == 5.0
MARKET_SITUATIONS = [
    "two_searchers",
    "chosen_twice",
    "sibling_skipped",
    "cousin_in_list",
    "minor_searching",
    "boy_never_candidate",
]


def market_data(height: float) -> world_reference.Data:
    """The repository's data with `c` = `height`, nothing else changed."""
    return world_reference.life_test_data({"core:market_search_height_female": height})


def market_world(data: world_reference.Data) -> world_reference.World:
    return world_reference.World(0xFF, MARKET_TAG, False, MARKET_FOUNDERS, MARKET_COUPLES, data=data)


def parents_of(world: world_reference.World, sim: int):
    return world.sims[sim][7] if sim in world.sims else world.archive[sim][9]


def full_siblings(world: world_reference.World, a: int, b: int) -> bool:
    pa, pb = parents_of(world, a), parents_of(world, b)
    return a != b and pa is not None and pb is not None and pa[:2] == pb[:2]


def first_cousins(world: world_reference.World, a: int, b: int) -> bool:
    """A parent of one is a full sibling of a parent of the other."""
    pa, pb = parents_of(world, a), parents_of(world, b)
    return pa is not None and pb is not None and any(full_siblings(world, x, y) for x in pa[:2] for y in pb[:2])


def market_situations(world: world_reference.World) -> dict:
    """The first occurrence of each situation of the note in the rounds of `world`, as
    (round n, details); and the rule that no man under 16 is ever in a list, checked over
    every round."""
    found = {}
    adult = world.data.market_age * YEAR
    for record in world.rounds:
        n, t, choices = record["n"], record["t"], record["choices"]
        assert all(t - world.born_of(man) >= adult for man in record["men"])
        assert not set(record["boys"]) & set(record["men"])
        if len(choices) >= 2:
            found.setdefault("two_searchers", (n, [choice["woman"] for choice in choices]))
        for index, choice in enumerate(choices):
            woman = choice["woman"]
            if choice["outcome"] == "taken":
                first = next(other["woman"] for other in choices[:index] if other["chosen"] == choice["chosen"])
                found.setdefault("chosen_twice", (n, choice["chosen"], first, woman))
            for man, phi, skipped in choice["walked"]:
                assert t - world.born_of(man) >= adult
                if skipped and full_siblings(world, woman, man):
                    found.setdefault("sibling_skipped", (n, woman, man, phi))
                if not skipped and phi == Fraction(1, 16) and first_cousins(world, woman, man):
                    found.setdefault("cousin_in_list", (n, woman, man, phi))
        for woman, age, p, u, searched, minor in record["considered"]:
            if searched and minor:
                found.setdefault("minor_searching", (n, woman, f"age {age / YEAR:.4f}", p, u))
        if choices and record["boys"]:
            found.setdefault("boy_never_candidate", (n, record["boys"]))
    return found


def market_parts(height: float) -> tuple:
    """(the situations reached in the forty years, the world) for `c` = `height`."""
    world = market_world(market_data(height))
    world.advance(MARKET_END)
    return market_situations(world), world


MARKET_TRIED = []
for _height in MARKET_HEIGHTS:
    _found, _world = market_parts(_height)
    MARKET_TRIED.append((_height, sorted(_found, key=MARKET_SITUATIONS.index)))
    if len(_found) == len(MARKET_SITUATIONS):
        break
MARKET_HEIGHT = MARKET_TRIED[-1][0]
MARKET_FOUND = market_situations(_world)
assert len(MARKET_FOUND) == len(MARKET_SITUATIONS), MARKET_TRIED
MARKET_DATA = market_data(MARKET_HEIGHT)
assert MARKET_DATA.market_height == MARKET_HEIGHT and MARKET_DATA.separation_rate == SEPARATION_RATE
FROM_FOUNDERS = market_world(MARKET_DATA)
assert world_reference.start_rejection(MARKET_FOUNDERS, MARKET_COUPLES, MARKET_DATA) is None

# The times the run stops at, from the situations: the round of "chosen_twice" (a pairing
# round with two searchers), the round of "cousin_in_list", and the 16th birthday of the
# first daughter born in the run (an ArrangePartnership acting one second before it is too
# young, one acting in it is accepted; design note of the market, section 4).
MARKET_TWICE_ROUND = MARKET_FOUND["chosen_twice"][0] * MONTH
MARKET_COUSIN_ROUND = MARKET_FOUND["cousin_in_list"][0] * MONTH
MARKET_SIBLING_ROUND = MARKET_FOUND["sibling_skipped"][0] * MONTH
_daughters = sorted(sim for sim in set(_world.sims) | set(_world.archive) if parents_of(_world, sim) is not None and _world.sex_of(sim) == FEMALE)
MARKET_GIRL = _daughters[0]
MARKET_BOY = next(
    sim
    for sim in sorted(set(_world.sims) | set(_world.archive))
    if parents_of(_world, sim) is not None
    and _world.sex_of(sim) == MALE
    and _world.born_of(sim) <= _world.born_of(MARKET_GIRL)
    and _world.kinship(MARKET_GIRL, sim) < Fraction(1, 8)
)
MARKET_GIRL_16 = _world.born_of(MARKET_GIRL) + 16 * YEAR
assert MARKET_TWICE_ROUND < MARKET_GIRL_16 - 1 < MARKET_SIBLING_ROUND < MARKET_COUSIN_ROUND < MARKET_END

MARKET = market_world(MARKET_DATA)
world_reference.check_market(MARKET)
world_reference.check_households(MARKET)
world_reference.check_separations(MARKET)
MARKET_SAVE_AFTER_START = MARKET.save()
MARKET.advance(MARKET_TWICE_ROUND - 1)
world_reference.check_market(MARKET)
world_reference.check_households(MARKET)
MARKET_SAVE_BEFORE_TWICE = MARKET.save()
MARKET_LOG_BEFORE_TWICE = len(MARKET.log)
MARKET.advance(MARKET_TWICE_ROUND)
MARKET_TWICE_ENTRIES = MARKET.log[MARKET_LOG_BEFORE_TWICE:]
world_reference.check_market(MARKET)
world_reference.check_households(MARKET)
world_reference.check_separations(MARKET)
MARKET_SAVE_AFTER_TWICE = MARKET.save()
(MARKET_TWICE_RECORD,) = [record for record in MARKET.rounds if record["t"] == MARKET_TWICE_ROUND]
MARKET_TWICE_PAIRED = [choice for choice in MARKET_TWICE_RECORD["choices"] if choice["outcome"] == "paired"]
assert len(MARKET_TWICE_PAIRED) == 1 and MARKET_TWICE_ENTRIES[0] == MARKET.log[MARKET_TWICE_PAIRED[0]["entry"] - 1]
# The entry `core:partnered` (her series and the separation write nothing), then the
# couple's household: `core:household_formed` (subjects both, by SimId) and a `core:moved`
# for her, then him (no children), each caused by the entry `core:partnered` and the rule.
_twice = MARKET_TWICE_PAIRED[0]
_twice_home = MARKET.sims[_twice["woman"]][9]
_twice_causes = [variant(0, varint(_twice["entry"])), HOUSEHOLD_RULE]
assert MARKET_TWICE_ENTRIES[1:] == [
    entry(_twice["entry"] + 1, MARKET_TWICE_ROUND, [variant(0, varint(sim)) for sim in sorted((_twice["woman"], _twice["chosen"]))], _twice_causes, variant(10, varint(_twice_home))),
    entry(_twice["entry"] + 2, MARKET_TWICE_ROUND, [variant(0, varint(_twice["woman"]))], _twice_causes, variant(11, varint(_twice_home))),
    entry(_twice["entry"] + 3, MARKET_TWICE_ROUND, [variant(0, varint(_twice["chosen"]))], _twice_causes, variant(11, varint(_twice_home))),
]


def copy_world(world: world_reference.World) -> world_reference.World:
    return copy.deepcopy(world, {id(world.data): world.data})


# The rejections of ArrangePartnership (design note of the market, section 4) at the
# girl's 16th birthday, on copies: stamped two seconds before it, the input acts one second
# before it, she is too young; stamped one second before it, it acts in it and is accepted
# (she is exactly 16 at `formed`).
MARKET.advance(MARKET_GIRL_16 - 2)
assert MARKET.open_partnership(MARKET_GIRL) is None and MARKET.open_partnership(MARKET_BOY) is None
MARKET_TOO_YOUNG = copy_world(MARKET).try_arrange(MARKET_GIRL, MARKET_BOY)
assert MARKET_TOO_YOUNG == world_reference.PARTNER_TOO_YOUNG
MARKET.advance(MARKET_GIRL_16 - 1)
_accepted = copy_world(MARKET)
assert _accepted.try_arrange(MARKET_GIRL, MARKET_BOY) is None
MARKET_ARRANGED = (_accepted.next_partnership_id - 1, _accepted.partnerships[_accepted.next_partnership_id - 1], _accepted.log[-1])
assert MARKET_ARRANGED[1] == [MARKET_GIRL, MARKET_BOY, MARKET_GIRL_16, None]
world_reference.check_market(_accepted)
world_reference.check_households(_accepted)
# In the second before the round of "sibling_skipped": her brother is too closely related.
MARKET.advance(MARKET_SIBLING_ROUND - 1)
_, MARKET_SISTER, MARKET_BROTHER, _ = MARKET_FOUND["sibling_skipped"]
MARKET_SIBLINGS_REJECTED = MARKET.try_arrange(MARKET_SISTER, MARKET_BROTHER)
assert MARKET_SIBLINGS_REJECTED == world_reference.PARTNERS_TOO_CLOSELY_RELATED
MARKET.advance(MARKET_COUSIN_ROUND - 1)
world_reference.check_market(MARKET)
world_reference.check_households(MARKET)
world_reference.check_separations(MARKET)
MARKET_SAVE_BEFORE_COUSIN = MARKET.save()
MARKET_LOG_BEFORE_COUSIN = len(MARKET.log)
MARKET.advance(MARKET_COUSIN_ROUND)
MARKET_COUSIN_ENTRIES = MARKET.log[MARKET_LOG_BEFORE_COUSIN:]
world_reference.check_market(MARKET)
world_reference.check_households(MARKET)
MARKET_SAVE_AFTER_COUSIN = MARKET.save()
MARKET.advance(MARKET_END)
world_reference.check_market(MARKET)
world_reference.check_households(MARKET)
world_reference.check_separations(MARKET)
MARKET_SAVE_END = MARKET.save()
# The same run in steps and in one go (the stops and the copies change nothing).
assert MARKET.log == _world.log and MARKET.year_hashes == _world.year_hashes and MARKET.save() == _world.save()
assert market_situations(MARKET) == MARKET_FOUND
# Cadence (test rule 6): 13 rounds a year.
assert [kind for _, kind in MARKET.ran].count(world_reference.MARKET_ROUND_KIND) == MARKET_YEARS * 13 - 1
MARKET_DEATHS = deaths_in(MARKET)
# Every pairing of the market names the searching woman's key in its draw (ADR-0018,
# acceptance test), and no partnership began below 16 (check_market).
for _record in MARKET.rounds:
    for _choice in _record["choices"]:
        if _choice["outcome"] == "paired":
            _line = MARKET.log[_choice["entry"] - 1]
            assert _line.endswith(variant(5, varint(_choice["partnership"])))
            assert variant(4, varint(21), varint(MARKET.key_of(_choice["woman"])), varint(_record["n"])) in _line


# The load checks of the market can fail (ADR-0018 section 7: one rejected state per
# invariant): each broken copy of the end of the run fails `check_market`.
def broken_market_states() -> list:
    def no_round(world):
        world.queue = [event for event in world.queue if event.kind != world_reference.MARKET_ROUND_KIND]

    def late_round(world):
        (event,) = [event for event in world.queue if event.kind == world_reference.MARKET_ROUND_KIND]
        event.time += MONTH

    def two_rounds(world):
        (event,) = [event for event in world.queue if event.kind == world_reference.MARKET_ROUND_KIND]
        world.queue.append(copy.copy(event))

    def off_round(world):
        world.partnerships[7][2] += 1

    def not_single(world):
        world.partnerships[6][2] = 75 * MONTH

    def too_young(world):
        woman, man, formed, _ = world.partnerships[7]
        world.sims[man][0] = formed - 16 * YEAR + 1

    def kin(world):
        world.partnerships[7][1] = 13

    def lost_input(world):
        world.input_pairings.append((YEAR, 1, 9))

    names = []
    for name, mutate in [
        ("no round series", no_round),
        ("round not at the first month start after the clock", late_round),
        ("two round series", two_rounds),
        ("a market partnership not in a round second", off_round),
        ("a partner not single at formed", not_single),
        ("a partner under 16 at formed", too_young),
        ("partners with phi >= 1/8", kin),
        ("an accepted input without its partnership", lost_input),
    ]:
        world = copy_world(MARKET)
        mutate(world)
        try:
            world_reference.check_market(world)
        except AssertionError:
            names.append(name)
            continue
        raise AssertionError(f"check_market accepts a broken state: {name}")
    return names


MARKET_BROKEN = broken_market_states()


# Round of "chosen_twice" by hand (11.2; design note of the market, section 3) with
# Python's math, straight from the draws (tags 19, 20, 5, 21), not from the model: who
# searches (`s = c · e^(−z²/2)`, `z = (a − 25.23) / 7`, below 1, times `ρ` under 18), the
# order of the searchers by MARKET_ORDER, each one's candidates by PAIRING_RANK (founders:
# φ 0), `U = −0.02 · (Δa − 4.69)² + G` with `G = −ln(−ln(u))`, the largest wins; a man
# chosen before in the round is taken.
def hand_round(world_before: world_reference.World, t: int) -> list:
    n = t // MONTH
    values = world_before.data
    singles = [
        sim
        for sim in sorted(world_before.sims)
        if world_before.open_partnership(sim) is None and t - world_before.sims[sim][0] >= 16 * YEAR
    ]
    women = [sim for sim in singles if world_before.sims[sim][2] == FEMALE]
    men = [sim for sim in singles if world_before.sims[sim][2] == MALE]
    key = lambda sim: world_before.sims[sim][1]
    searchers = []
    for woman in women:
        age = t - world_before.sims[woman][0]
        z = (age / 31_449_600.0 - values.market_center) / 7.0
        p = min(1.0, values.market_height * math.exp(-0.5 * z * z)) * (values.market_minor if age < 18 * YEAR else 1.0)
        if unit(words(chacha_key(0xFF, 19, key(woman), n, 0))[0]) < p:
            searchers.append(woman)
    searchers.sort(key=lambda woman: (rng_rank(0xFF, 20, 0, n, key(woman)), key(woman)))
    taken, rows = set(), []
    for woman in searchers:
        candidates = sorted(men, key=lambda man: (rng_rank(0xFF, 5, key(woman), n, key(man)), key(man)))[:12]
        scored = []
        for man in candidates:
            gap = (world_before.sims[woman][0] - world_before.sims[man][0]) / 31_449_600.0
            g = -math.log(-math.log(rng_unit_open(words(chacha_key(0xFF, 21, key(woman), n, key(man)))[0])))
            scored.append((-0.02 * (gap - 4.69) ** 2 + g, man))
        assert all(parents_of(world_before, man) is None for man in candidates) and parents_of(world_before, woman) is None
        best = max(scored)[1]
        rows.append((woman, [man for _, man in scored], best, "taken" if best in taken else "paired"))
        taken.add(best)
    return rows


assert market_world(MARKET_DATA).sims.keys() == FROM_FOUNDERS.sims.keys()
_before_twice = market_world(MARKET_DATA)
_before_twice.advance(MARKET_TWICE_ROUND - 1)
MARKET_TWICE_HAND = hand_round(_before_twice, MARKET_TWICE_ROUND)
assert MARKET_TWICE_HAND == [
    (choice["woman"], [man for man, *_ in choice["candidates"]], choice["chosen"], choice["outcome"])
    for choice in MARKET_TWICE_RECORD["choices"]
]

# The cousin by hand: his φ is 1/16 [W2.7f] and his penalty `16 · ln(10) · 1/16 = ln(10)`
# [W11.2a] beside the rest of his utility.
(MARKET_COUSIN_RECORD,) = [record for record in MARKET.rounds if record["t"] == MARKET_COUSIN_ROUND]
_, MARKET_COUSIN_WOMAN, MARKET_COUSIN, _ = MARKET_FOUND["cousin_in_list"]
(MARKET_COUSIN_CHOICE,) = [choice for choice in MARKET_COUSIN_RECORD["choices"] if choice["woman"] == MARKET_COUSIN_WOMAN]
(MARKET_COUSIN_ROW,) = [row for row in MARKET_COUSIN_CHOICE["candidates"] if row[0] == MARKET_COUSIN]
_, _gap, _phi, _g, _u = MARKET_COUSIN_ROW
assert _phi == 1 / 16 and abs(_u - (-0.02 * (_gap - 4.69) ** 2 - math.log(10.0) + _g)) <= 1e-12


def partnered_json(entry_id: int, t: int, woman: int, man: int, n: int, her_key: int, his_key: int, number: int) -> str:
    """The JSON form of the entry `core:partnered` of a pairing of the market as ADR-0004
    sections 3 and 7 describe it (causes the plan of the round, the rule, the draw with 16
    lower hex digits per field); written from the ADR, not from the CLI."""
    return json.dumps(
        {
            "id": entry_id,
            "t": t,
            "subjects": [{"sim": woman}, {"sim": man}],
            "causes": [
                {"plan": {"by": 1, "planned_at": 0}},
                {"rule": world_reference.PARTNER_MARKET_RULE},
                {"random": {name: f"{word:016x}" for name, word in zip(("tag", "subject", "occasion", "detail"), (21, her_key, n, his_key))}},
            ],
            "payload": {"core:partnered": {"partnership": number}},
        },
        separators=(",", ":"),
    )


def market_entry_rows(label: str, world: world_reference.World, choice: dict, n: int) -> list:
    """The entry `core:partnered` of a pairing of the market: its bytes by hand and from the
    model, and its JSON."""
    woman, man, number = choice["woman"], choice["chosen"], choice["partnership"]
    her_key, his_key = world.key_of(woman), world.key_of(man)
    causes = [
        variant(1, varint(1), signed(0)),
        variant(3, string(world_reference.PARTNER_MARKET_RULE)),
        variant(4, varint(21), varint(her_key), varint(n), varint(his_key)),
    ]
    line = entry(choice["entry"], n * MONTH, [variant(0, varint(woman)), variant(0, varint(man))], causes, variant(5, varint(number)))
    assert world.log[choice["entry"] - 1] == line
    return [
        f"{label} {line.hex()}",
        f"{label}_json {partnered_json(choice['entry'], n * MONTH, woman, man, n, her_key, his_key, number)}",
    ]


def market_origin(world: world_reference.World, number: int) -> str:
    woman, man, formed, _ = world.partnerships[number]
    if (formed, woman, man) in world.input_pairings:
        return "Start" if formed == 0 else "input"
    return f"market round {formed // MONTH}"


def market_story(world: world_reference.World) -> list:
    """The story of "market" as readable rows: the search for `c`, the founders, every round
    with a searcher who had a candidate (the men, the order, per searcher the walked list,
    the candidates with gap, φ, G and U, the choice), the rounds without a candidate per
    woman, the partnerships, separations, deaths, children, and the situations reached."""
    rows = [f"market_search c {height!r} situations {names}" for height, names in MARKET_TRIED]
    rows.append(f"market_c {MARKET_HEIGHT!r} years {MARKET_YEARS} data_hash {MARKET_DATA.hash:016x}")
    for index, (born, sex, _) in enumerate(MARKET_FOUNDERS):
        couple = next((f" couple with sim {m + 1 if w == index else w + 1}" for w, m in MARKET_COUPLES if index in (w, m)), " single")
        rows.append(f"market_founder sim {index + 1} {'female' if sex == FEMALE else 'male'} age {-born / YEAR:g}{couple} key {MARKET_KEYS[index]:016x}")
    empty = {}
    for record in world.rounds:
        n, t = record["n"], record["t"]
        if not record["choices"]:
            continue
        real = [choice for choice in record["choices"] if choice["outcome"] != "no candidate"]
        for choice in record["choices"]:
            if choice["outcome"] == "no candidate":
                skipped = [man for man, _, s in choice["walked"] if s]
                empty.setdefault(choice["woman"], []).append((n, skipped))
        if not real:
            continue
        searched = {woman: (age, p, u, minor) for woman, age, p, u, ok, minor in record["considered"] if ok}
        rows.append(
            f"market_round {n} (year {t / YEAR:.3f}) men {record['men']} single_women {[c[0] for c in record['considered']]}"
            f" searchers_in_order {[choice['woman'] for choice in record['choices']]} boys_under_16 {len(record['boys'])}"
        )
        for choice in record["choices"]:
            woman = choice["woman"]
            age, p, u, minor = searched[woman]
            walked = ", ".join(f"{man} (phi {phi}{' skipped' if s else ''})" for man, phi, s in choice["walked"])
            candidates = ", ".join(
                f"{man} (gap {gap:.4f} phi {phi!r} G {g!r} U {u_!r})" for man, gap, phi, g, u_ in choice["candidates"]
            )
            outcome = choice["outcome"] + (f" partnership {choice['partnership']} entry {choice['entry']}" if choice["outcome"] == "paired" else "")
            rows.append(
                f"market_choice round {n} woman {woman} age {age / YEAR:.4f}{' (rho)' if minor else ''} p {p!r} u {u!r}"
                f" walked [{walked}] candidates [{candidates}] chosen {choice['chosen']} {outcome}"
            )
    for woman, rounds in sorted(empty.items()):
        skipped = sorted({man for _, men in rounds for man in men})
        rows.append(
            f"market_no_candidate woman {woman} in {len(rounds)} rounds (first {rounds[0][0]}, last {rounds[-1][0]})"
            f" skipped kin {skipped}"
        )
    for number, (woman, man, formed, ended) in sorted(world.partnerships.items()):
        rows.append(
            f"market_partnership {number} woman {woman} man {man} formed {formed} (month {formed / MONTH:g}) by"
            f" {market_origin(world, number)} phi {world.kinship(woman, man)} ended {split_ended(ended)}"
        )
    for t, number, entry_id, _, _ in world.separations:
        rows.append(f"market_separation partnership {number} at {t} (month {t / MONTH:.4f}) entry {entry_id}")
    for left, sim, cause in deaths_in(world):
        rows.append(f"market_death sim {sim} at {left} (year {left / YEAR:.4f}) of {cause_name(cause)}")
    for sim in sorted(set(world.sims) | set(world.archive)):
        parents = parents_of(world, sim)
        if parents is not None:
            sex = "female" if world.sex_of(sim) == FEMALE else "male"
            rows.append(f"market_child {sim} {sex} born {world.born_of(sim)} (month {world.born_of(sim) / MONTH:g}) parents {parents}")
    for name in MARKET_SITUATIONS:
        rows.append(f"market_situation {name} reached: round {MARKET_FOUND[name][0]} {MARKET_FOUND[name][1:]}")
    boys = []
    for sim in sorted(set(world.sims) | set(world.archive)):
        birthday = world.born_of(sim) + 16 * YEAR
        if world.sex_of(sim) == MALE and parents_of(world, sim) is not None and birthday <= MARKET_END:
            listed = [record["n"] for record in world.rounds if sim in record["men"]]
            before = [record["n"] for record in world.rounds if sim in record["boys"] and record["choices"]]
            boys.append(
                f"{sim} (16 in month {birthday / MONTH:g}; under 16 alive in {len(before)} rounds with a searcher,"
                f" last {before[-1] if before else None}; first in the men list {listed[0] if listed else None})"
            )
    rows.append("market_boys " + "; ".join(boys))
    rows.append(
        f"market_reject partner_too_young: ArrangePartnership(woman {MARKET_GIRL}, man {MARKET_BOY}) stamped"
        f" {MARKET_GIRL_16 - 2} acts at {MARKET_GIRL_16 - 1}, her age 16 years minus 1 s: code {MARKET_TOO_YOUNG}"
    )
    rows.append(
        f"market_accepted ArrangePartnership(woman {MARKET_GIRL}, man {MARKET_BOY}) stamped {MARKET_GIRL_16 - 1} acts at"
        f" {MARKET_GIRL_16} (her 16th birthday, month {MARKET_GIRL_16 / MONTH:g}) on a copy: partnership {MARKET_ARRANGED[0]}"
        f" {MARKET_ARRANGED[1]} entry {MARKET_ARRANGED[2].hex()}"
    )
    rows.append(
        f"market_reject partners_too_closely_related: ArrangePartnership(woman {MARKET_SISTER}, man {MARKET_BROTHER})"
        f" stamped {MARKET_SIBLING_ROUND - 1}, phi {world.kinship(MARKET_SISTER, MARKET_BROTHER)}: code {MARKET_SIBLINGS_REJECTED}"
    )
    return rows


# Scenario "similar" (design note of the similarity, section 4): the partner market with
# the similarity terms of cognition and height (genetics specification 11.2) on the test
# map tests/golden/data_pairing (written by tools/pairing_fixture.py) with `c` =
# `core:market_search_height_female` = 1.0 as in "market", nothing else changed. Seed 0xff,
# founder tag `test:similar`, the switch off, no couples, twelve founders: six single women
# (sims 1 to 6) and six single men (sims 7 to 12), each sex born 18, 20, 22, 24, 26 and 28
# years before the start. Their alleles at the 80 trait loci come from a fixed generator,
# SplitMix64 (tools/rng_reference.py `mix64`) from the state 0: the k-th output (k = 1, 2,
# ...) is `mix64(k · 0x9E3779B97F4A7C15 mod 2^64)` and its top bit is one allele, taken
# founder by founder (in list order), locus by locus in map order, the maternal copy before
# the paternal one; every other slot holds the reference allele. A founder's `Start` lists
# the loci with an alternative allele in map order (`LocusAlleles { locus, copies }`, the
# copies maternal then paternal). Ten rounds: the run stops at the end of round 10 (the
# clock at `10 · MONTH`). The phenotypes `z_K` and `z_h` of each sim at a round are those of
# 4.2 (`world_reference.phenotype_parts`): a founder draws `C` with its own key as mother and
# father (ADR-0009 section 3).
SIMILAR_TAG = "test:similar"
SIMILAR_DIR = world_reference.ROOT / "tests" / "golden" / "data_pairing"
SIMILAR_HEIGHT = 1.0
SIMILAR_AGES = [18, 20, 22, 24, 26, 28]
SIMILAR_ROUNDS = 10
SIMILAR_END = SIMILAR_ROUNDS * MONTH
SIMILAR_GENERATOR_STATE = 0
PAIRING_FIXTURE = world_reference.Data(*data_reference.documents(SIMILAR_DIR))
SIMILAR_DATA = world_reference.life_test_data({"core:market_search_height_female": SIMILAR_HEIGHT}, SIMILAR_DIR)
assert SIMILAR_DATA.market_height == SIMILAR_HEIGHT and SIMILAR_DATA.separation_rate == SEPARATION_RATE
assert SIMILAR_DATA.cognition_trait.id == "test:cognition" and SIMILAR_DATA.height_trait.id == "test:height"
# The loci of the two traits in map order (the file's order, checked by data_reference).
SIMILAR_LOCI = [
    locus["id"]
    for locus in json.loads((SIMILAR_DIR / "genetics" / "genome_map.json").read_text())["loci"]
    if locus["id"].startswith(("test:cognition_", "test:height_"))
]
assert len(SIMILAR_LOCI) == 80


def splitmix64(state: int):
    """The outputs of SplitMix64 from `state` (Vigna): `mix64(state + k · GOLDEN)`, k = 1, 2,
    ..."""
    from rng_reference import GOLDEN, MASK, mix64

    k = 0
    while True:
        k += 1
        yield mix64((state + k * GOLDEN) & MASK)


def similar_founders() -> list:
    """(born, sex, alleles) of the twelve founders, the alleles as (locus, [maternal,
    paternal]) of the loci with an alternative allele, in map order."""
    bits = (word >> 63 for word in splitmix64(SIMILAR_GENERATOR_STATE))
    founders = []
    for sex in (FEMALE, MALE):
        for age in SIMILAR_AGES:
            genes = []
            for locus in SIMILAR_LOCI:
                copies = [next(bits), next(bits)]
                if any(copies):
                    genes.append((locus, copies))
            founders.append((-age * YEAR, sex, genes))
    return founders


SIMILAR_FOUNDERS = similar_founders()
SIMILAR_KEYS = [founder_key(SIMILAR_TAG, index) for index in range(len(SIMILAR_FOUNDERS))]
# The generator gives frequencies near 0.5 (960 bits per sex).
_ones = sum(sum(copies) for _, _, genes in SIMILAR_FOUNDERS for _, copies in genes)
assert 800 <= _ones <= 1120, _ones


def similar_world(data=SIMILAR_DATA) -> world_reference.World:
    return world_reference.World(0xFF, SIMILAR_TAG, False, SIMILAR_FOUNDERS, (), data=data)


# The whole run in one go, to find the first round that forms a partnership.
_similar_once = similar_world()
_similar_once.advance(SIMILAR_END)
SIMILAR_FIRST_ROUND = next(
    record["n"] for record in _similar_once.rounds if any(choice["outcome"] == "paired" for choice in record["choices"])
) * MONTH

SIMILAR = similar_world()
world_reference.check_market(SIMILAR)
world_reference.check_households(SIMILAR)
world_reference.check_separations(SIMILAR)
SIMILAR_SAVE_AFTER_START = SIMILAR.save()
SIMILAR.advance(SIMILAR_FIRST_ROUND - 1)
world_reference.check_market(SIMILAR)
world_reference.check_households(SIMILAR)
SIMILAR_SAVE_BEFORE_PAIRING = SIMILAR.save()
SIMILAR_LOG_BEFORE_PAIRING = len(SIMILAR.log)
SIMILAR.advance(SIMILAR_FIRST_ROUND)
SIMILAR_PAIRING_ENTRIES = SIMILAR.log[SIMILAR_LOG_BEFORE_PAIRING:]
world_reference.check_market(SIMILAR)
world_reference.check_households(SIMILAR)
world_reference.check_separations(SIMILAR)
SIMILAR_SAVE_AFTER_PAIRING = SIMILAR.save()
SIMILAR.advance(SIMILAR_END)
world_reference.check_market(SIMILAR)
world_reference.check_households(SIMILAR)
world_reference.check_separations(SIMILAR)
SIMILAR_SAVE_END = SIMILAR.save()
assert SIMILAR.log == _similar_once.log and SIMILAR.save() == _similar_once.save()
assert [kind for _, kind in SIMILAR.ran].count(world_reference.MARKET_ROUND_KIND) == SIMILAR_ROUNDS
assert SIMILAR.year_hashes == []
# The entries of the first pairing round: per pairing its `core:partnered`, then the
# couple's `core:household_formed` and a `core:moved` for her and one for him (single
# founders: no children move).
(SIMILAR_FIRST_RECORD,) = [record for record in SIMILAR.rounds if record["t"] == SIMILAR_FIRST_ROUND]
assert len(SIMILAR_PAIRING_ENTRIES) == 4 * sum(1 for choice in SIMILAR_FIRST_RECORD["choices"] if choice["outcome"] == "paired")
assert [line[-2] for line in SIMILAR_PAIRING_ENTRIES[::4]] == [5] * (len(SIMILAR_PAIRING_ENTRIES) // 4)

# The same founders on the fixture with the two traits not named (the calibration's fields
# null, nothing else changed): the market of the market note, for comparison.
_without = copy.deepcopy(data_reference.documents(SIMILAR_DIR))
_without[0]["values"]["core:market_search_height_female"]["value"] = SIMILAR_HEIGHT
_without[1]["cognition_trait"] = None
_without[1]["height_trait"] = None
SIMILAR_WITHOUT_DATA = world_reference.Data(*_without)
SIMILAR_WITHOUT = similar_world(SIMILAR_WITHOUT_DATA)
SIMILAR_WITHOUT.advance(SIMILAR_END)
world_reference.check_market(SIMILAR_WITHOUT)
world_reference.check_households(SIMILAR_WITHOUT)


# The phenotypes by hand (genetics specification 4.1, 4.2; ADR-0009 sections 3 and 4) with
# Python's math, from the founders' allele lists, not from the model: `G = Σ (d − 1) /
# sqrt(20)` over the 40 loci of the trait, `s = 1 / (1 + e^((12 − x) / 2))`, the weights,
# `C` and `E` as `sqrt(−2 ln(u0)) · cos(2π u1)` at their keys, `z = sqrt(h2) G + sqrt(c2) C +
# sqrt(e2) E`.
def hand_phenotype(index: int, trait: str, weights: tuple, t: int) -> float:
    born, _, genes = SIMILAR_FOUNDERS[index]
    alleles = dict(genes)
    total = sum(sum(alleles.get(locus, [0, 0])) - 1 for locus in SIMILAR_LOCI if locus.startswith(trait + "_"))
    g = total / math.sqrt(20.0)
    x = (t - born) / 31_449_600.0
    s = 1.0 / (1.0 + math.exp((12.0 - x) / 2.0))
    h2 = weights[0] + (weights[2] - weights[0]) * s
    c2 = weights[1] + (weights[3] - weights[1]) * s
    key = SIMILAR_KEYS[index]

    def normal(tag: int, detail: int) -> float:
        w = words(chacha_key(0xFF, tag, key, str_key(trait), detail))
        return math.sqrt(-2.0 * math.log(unit_open0(w[0]))) * math.cos(2.0 * math.pi * unit(w[1]))

    return math.sqrt(h2) * g + math.sqrt(c2) * normal(10, key) + math.sqrt(1.0 - h2 - c2) * normal(11, 0)


SIMILAR_WEIGHTS = {"test:cognition": (0.5, 0.25, 0.75, 0.05), "test:height": (0.8, 0.05, 0.8, 0.05)}


# The first pairing round by hand (11.2; design note of the similarity, section 2) with
# Python's math, from the draws and the hand phenotypes, not from the model: who searches,
# their order, each one's candidates by PAIRING_RANK (all founders, φ 0), `U = −0.377 · dK²
# − 0.164 · dH² − 0.02 · (Δa − 4.69)² + G`, the largest wins, a man chosen before is taken.
def similar_hand_round(t: int) -> list:
    n = t // MONTH
    women = [index for index, (_, sex, _) in enumerate(SIMILAR_FOUNDERS) if sex == FEMALE]
    men = [index for index, (_, sex, _) in enumerate(SIMILAR_FOUNDERS) if sex == MALE]
    born = [founder[0] for founder in SIMILAR_FOUNDERS]
    z = {
        index: [hand_phenotype(index, trait, SIMILAR_WEIGHTS[trait], t) for trait in ("test:cognition", "test:height")]
        for index in women + men
    }
    searchers = []
    for woman in women:
        age = t - born[woman]
        a = age / 31_449_600.0
        p = min(1.0, SIMILAR_HEIGHT * math.exp(-0.5 * ((a - 25.23) / 7.0) ** 2)) * (0.0918 if age < 18 * YEAR else 1.0)
        if unit(words(chacha_key(0xFF, 19, SIMILAR_KEYS[woman], n, 0))[0]) < p:
            searchers.append(woman)
    searchers.sort(key=lambda woman: (rng_rank(0xFF, 20, 0, n, SIMILAR_KEYS[woman]), SIMILAR_KEYS[woman]))
    taken, rows = set(), []
    for woman in searchers:
        her = SIMILAR_KEYS[woman]
        candidates = sorted(men, key=lambda man: (rng_rank(0xFF, 5, her, n, SIMILAR_KEYS[man]), SIMILAR_KEYS[man]))[:12]
        scored = []
        for man in candidates:
            gap = (born[woman] - born[man]) / 31_449_600.0
            g = -math.log(-math.log(rng_unit_open(words(chacha_key(0xFF, 21, her, n, SIMILAR_KEYS[man]))[0])))
            u = -0.377 * (z[woman][0] - z[man][0]) ** 2 - 0.164 * (z[woman][1] - z[man][1]) ** 2 - 0.02 * (gap - 4.69) ** 2 + g
            scored.append((u, man + 1))
        best = max(scored)[1]
        rows.append((woman + 1, [man for _, man in scored], [u for u, _ in scored], best, "taken" if best in taken else "paired"))
        taken.add(best)
    return rows


SIMILAR_HAND = similar_hand_round(SIMILAR_FIRST_ROUND)
assert [(woman, men, best, outcome) for woman, men, _, best, outcome in SIMILAR_HAND] == [
    (choice["woman"], [man for man, *_ in choice["candidates"]], choice["chosen"], choice["outcome"])
    for choice in SIMILAR_FIRST_RECORD["choices"]
]
for (_, _, _hand_us, _, _), _choice in zip(SIMILAR_HAND, SIMILAR_FIRST_RECORD["choices"]):
    assert all(abs(a - b) <= 1e-12 for a, b in zip(_hand_us, [row[4] for row in _choice["candidates"]]))


def similar_phenotype_rows(world: world_reference.World, t: int) -> list:
    """Per founder its phenotypes at `t` with their parts, the hand values and the margins of
    the libm calls (exp of `s`, ln and cos of both draws) to their rounding boundaries."""
    rows, worst = [], 0.5
    for sim in sorted(world.sims):
        index = sim - 1
        texts = []
        for name, trait in (("K", world.data.cognition_trait), ("h", world.data.height_trait)):
            parts = world.phenotype_parts(sim, trait, t)
            hand = hand_phenotype(index, trait.id, SIMILAR_WEIGHTS[trait.id], t)
            assert abs(hand - parts["z"]) <= 1e-12, (sim, trait.id, hand, parts["z"])
            margins = world_reference.phenotype_margins(0xFF, trait, world.key_of(sim), world.parent_keys(sim), t - world.born_of(sim))
            worst = min([worst] + [margin for _, margin in margins])
            texts.append(
                f"z_{name} {parts['z']!r} (G {parts['G']!r} C {parts['C']!r} E {parts['E']!r} s {parts['s']!r}"
                f" h2 {parts['h2']!r} c2 {parts['c2']!r} e2 {parts['e2']!r}; by hand with math {hand!r})"
            )
        rows.append(f"similar_phenotypes round {t // MONTH} sim {sim} age {(t - world.born_of(sim)) / YEAR!r} " + " ".join(texts))
    rows.append(f"similar_phenotype_libm_margin round {t // MONTH} smallest {worst!r} ulps (0.5 is an f64, 0 a midpoint)")
    return rows


def similar_story(world: world_reference.World) -> list:
    """The story of "similar": per round the searchers and per choice the candidates with
    the four penalty terms and U, the choice; the partnerships, attempts and conceptions."""
    rows = []
    for record in world.rounds:
        n = record["n"]
        rows.append(
            f"similar_round {n} men {record['men']} single_women {[c[0] for c in record['considered']]}"
            f" searchers {[c[0] for c in record['considered'] if c[4]]} in_order {[choice['woman'] for choice in record['choices']]}"
        )
        searched = {woman: (p, u) for woman, _, p, u, ok, _ in record["considered"]}
        for woman, age, p, u, ok, _ in record["considered"]:
            if not ok:
                rows.append(f"similar_no_search round {n} woman {woman} age {age / YEAR:.4f} p {p!r} u {u!r}")
        for choice in record["choices"]:
            woman = choice["woman"]
            p, u = searched[woman]
            if choice["outcome"] == "no candidate":
                rows.append(f"similar_choice round {n} woman {woman} p {p!r} u {u!r} no candidate")
                continue
            z = choice["phenotypes"]
            candidates = "; ".join(
                f"{man} (gap {gap!r} phi {phi!r} z_K {z[man][0]!r} z_h {z[man][1]!r} p_K {terms[0]!r} p_H {terms[1]!r}"
                f" p_a {terms[2]!r} p_phi {terms[3]!r} G {g!r} U {u_!r})"
                for (man, gap, phi, g, u_), terms in zip(choice["candidates"], choice["terms"])
            )
            outcome = choice["outcome"] + (f" partnership {choice['partnership']} entry {choice['entry']}" if choice["outcome"] == "paired" else "")
            rows.append(
                f"similar_choice round {n} woman {woman} p {p!r} u {u!r} z_K {z[woman][0]!r} z_h {z[woman][1]!r}"
                f" candidates [{candidates}] chosen {choice['chosen']} {outcome}"
            )
    for number, (woman, man, formed, ended) in sorted(world.partnerships.items()):
        rows.append(f"similar_partnership {number} woman {woman} man {man} formed {formed} (round {formed // MONTH}) ended {split_ended(ended)}")
    rows.append(f"similar_attempts (month, woman, man, chance, success) {[(t // MONTH, w, m, p, ok) for t, w, m, p, ok in world.attempts]}")
    rows.append(f"similar_conceptions (month, woman, man, number, u, kind, children) {[(t // MONTH, w, m, k, u, kind, [(f'{key:016x}', 'female' if sex == FEMALE else 'male') for key, sex in children]) for t, w, m, k, u, kind, children in world.multiples]}")
    rows.append(f"similar_deaths {[(left, sim, cause_name(cause)) for left, sim, cause in deaths_in(world)]}")
    return rows


def similar_entry_rows(world: world_reference.World) -> list:
    """Every `core:partnered` entry of the run: its bytes (checked against a hand build)
    and its JSON."""
    rows = []
    for record in world.rounds:
        for choice in record["choices"]:
            if choice["outcome"] == "paired":
                rows += market_entry_rows(f"similar_partnered_entry_{choice['partnership']}", world, choice, record["n"])
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


# Scenario "homes" (design note of the households, section 8): seed 0xff, founder tag
# `test:homes`, the switch off, on the test data of "clan" (accident 0.02 per year,
# senescence practically off, the repository's diseases, the old fertility window: 0.25 per
# month from 18 until 45 years, ten months, no pause, the wish always 40, no twins, `c` 0)
# with a separation rate `λ` the reference picks, as "split" does (clan's test data have
# `λ` = 0 and so no separation; the note asks for one). Seven founders with reference
# alleles only, in this order (their places fix their lineage keys and so their deaths:
# with accident 0.02 founder 0 dies after about 22.4 years, founder 3 after about 1.3,
# every other one after 30 or more):
#   sim 1  woman 20  couple with sim 3 (partnership 1)
#   sim 2  woman 21  couple with sim 7 (partnership 2)
#   sim 3  man 22
#   sim 4  man 30    single
#   sim 5  boy 12    single, under 18: by the orphan rule (part 2) into a foster household
#   sim 6  woman 25  single
#   sim 7  man 23
# and one input `ArrangePartnership` that pairs the first daughter born in the run who is
# single and not moved out one month after her 16th birthday (the first month start after
# it, stamped one second before) with sim 5, then about 29. `λ` is the first of 0.005,
# 0.010, ..., 0.050 with which the 24 years reach all six situations of the note: a couple
# with children; a child born in the run who leaves home by its drawn age; a girl of 16 or
# 17 paired by the input who moves out with her partner (her `leave_home` cancelled); a
# separation in which a dependent child stays with the mother; a mother who dies so that
# the father in the household becomes the carer; and (part 2, section 7, which replaces part
# 1's "a minor founder alone who gets a household of his own at the start") the boy of 12 in
# a foster household at the start: founders are unrelated, so the women of the couples of 18
# or more (sims 1 and 2) are the candidates of FOSTER_RANK; he joins the household of the
# chosen one with one `core:moved` (no `core:household_formed` of his own), she is his carer,
# she and her partner his social parents. The story is printed by `homes_story`.
HOMES_TAG = "test:homes"
HOMES_YEARS = 24
HOMES_END = HOMES_YEARS * YEAR - 1


def _founder(age: int, sex: bytes) -> tuple:
    return (-age * YEAR, sex, genotype(WILD, WILD))


HOMES_FOUNDERS = [
    _founder(20, FEMALE),
    _founder(21, FEMALE),
    _founder(22, MALE),
    _founder(30, MALE),
    _founder(12, MALE),
    _founder(25, FEMALE),
    _founder(23, MALE),
]
HOMES_COUPLES = [(0, 2), (1, 6)]
HOMES_BOY = 5
HOMES_KEYS = [founder_key(HOMES_TAG, index) for index in range(len(HOMES_FOUNDERS))]
HOMES_RATES = [index / 200 for index in range(1, 11)]
assert HOMES_RATES[0] == 0.005 and HOMES_RATES[1] == 0.01 and HOMES_RATES[-1] == 0.05
HOMES_SITUATIONS = [
    "couple_with_children",
    "child_leaves_home",
    "minor_pairs_and_moves_out",
    "separation_child_stays",
    "mother_dies_father_carer",
    "minor_founder_fostered",
]


def homes_data(rate: float) -> world_reference.Data:
    """The test data of "clan" with `core:separation_rate` = `rate`."""
    return world_reference.death_test_data(0.02, fertility=CLAN_FERTILITY, life_values={"core:separation_rate": rate})


def homes_world(data: world_reference.Data) -> world_reference.World:
    return world_reference.World(0xFF, HOMES_TAG, False, HOMES_FOUNDERS, HOMES_COUPLES, data=data)


def homes_girl(data: world_reference.Data):
    """(the girl, the second the input acts) or (None, None): the first daughter born in the
    run whose 16th birthday plus one month lies in the run and who then lives, is single and
    not moved out, while sim 5 lives and is single (the input is accepted)."""
    first = homes_world(data)
    first.advance(HOMES_END)
    daughters = sorted(sim for sim in set(first.sims) | set(first.archive) if parents_of(first, sim) is not None and first.sex_of(sim) == FEMALE)
    for girl in daughters:
        acts = first.born_of(girl) + 16 * YEAR + MONTH
        if acts > HOMES_END:
            continue
        probe = homes_world(data)
        probe.advance(acts - 1)
        if girl in probe.sims and not probe.sims[girl][10] and probe.arrange_rejection(girl, HOMES_BOY) is None:
            return girl, acts
    return None, None


def homes_run(data: world_reference.Data, stops=(), keep=None):
    """The run with the input: (the world, the girl, the second the input acts). `stops`
    are seconds to stop at (the clock), `keep(world, stop)` sees the world at each."""
    girl, acts = homes_girl(data)
    world = homes_world(data)
    plan = sorted(set(stops) | ({acts - 1} if acts is not None else set()))
    for stop in plan:
        world.advance(stop)
        if acts is not None and stop == acts - 1:
            if keep is not None:
                keep(world, "before_input")
            world.arrange(girl, HOMES_BOY)
            if keep is not None:
                keep(world, "after_input")
        elif keep is not None:
            keep(world, stop)
    world.advance(HOMES_END)
    return world, girl, acts


def homes_situations(world: world_reference.World, girl, acts) -> dict:
    """The first occurrence of each situation of the note in the run, with its details."""
    found = {}
    for number, (woman, man, formed, _) in sorted(world.partnerships.items()):
        children = [sim for sim in sorted(set(world.sims) | set(world.archive)) if parents_of(world, sim) is not None and parents_of(world, sim)[:2] == (woman, man)]
        if children:
            found.setdefault("couple_with_children", (number, woman, man, len(children)))
    for t, kind, household, sims, why, entry_id in world.home_log:
        if kind == "formed" and why == "leave_home" and parents_of(world, sims[0]) is not None:
            sim = sims[0]
            found.setdefault("child_leaves_home", (t, sim, (t - world.born_of(sim)) // YEAR, household, entry_id))
    if girl is not None:
        moves = [(t, household, entry_id) for t, kind, household, sims, why, entry_id in world.home_log if kind == "moved" and sims == [girl] and t == acts and why.startswith("partnership")]
        if moves and 16 * YEAR <= acts - world.born_of(girl) < 18 * YEAR:
            found["minor_pairs_and_moves_out"] = (acts, girl, HOMES_BOY, moves[0][1], moves[0][2])
    for t, child, old, new, why in world.carer_log:
        parents = parents_of(world, child)
        if why == "succession: the other parent" and old == parents[0] and new == parents[1]:
            found.setdefault("mother_dies_father_carer", (t, old, new, child))
    for t, woman, man, stayed, moved in world.separation_homes:
        if stayed:
            found.setdefault("separation_child_stays", (t, woman, man, stayed, moved))
    for record in world.placements:
        if record["t"] == 0 and record["stage"] == "foster":
            (sim,) = record["group"]
            carer = record["carer"]
            found.setdefault(
                "minor_founder_fostered",
                (sim, carer, world.partnerships[next(n for n, v in world.partnerships.items() if v[0] == carer)][1], record["household"], record["entries"], record["added"][sim]),
            )
    return found


HOMES_TRIED = []
for _rate in HOMES_RATES:
    _world, _girl, _acts = homes_run(homes_data(_rate))
    _found = homes_situations(_world, _girl, _acts)
    HOMES_TRIED.append((_rate, sorted(_found, key=HOMES_SITUATIONS.index)))
    if len(_found) == len(HOMES_SITUATIONS):
        break
HOMES_RATE = HOMES_TRIED[-1][0]
assert len(HOMES_TRIED[-1][1]) == len(HOMES_SITUATIONS), HOMES_TRIED
HOMES_DATA = homes_data(HOMES_RATE)
HOMES_FOUND = homes_situations(_world, _girl, _acts)
HOMES_GIRL, HOMES_ACTS = _girl, _acts
HOMES_SEPARATED_AT = HOMES_FOUND["separation_child_stays"][0]
HOMES_MOTHER_DIES_AT = HOMES_FOUND["mother_dies_father_carer"][0]
HOMES_LEAVES_AT = HOMES_FOUND["child_leaves_home"][0]

# The run again with stops: saves after the start, before and after the input, after the
# second of the separation, after the second of the mother's death, after the second the
# child leaves home, at the end; each state passes every load check.
HOMES_SAVES = {}
HOMES_ENTRIES = {}


def _keep(world: world_reference.World, stop) -> None:
    for check in (world_reference.check_households, world_reference.check_separations, world_reference.check_market):
        check(world)
    HOMES_SAVES[stop] = world.save()


HOMES = homes_world(HOMES_DATA)
_keep(HOMES, "after_start")
HOMES_AFTER_START_LOG = list(HOMES.log)
_marks = {}
for _stop in sorted({HOMES_ACTS - 1, HOMES_SEPARATED_AT - 1, HOMES_SEPARATED_AT, HOMES_MOTHER_DIES_AT - 1, HOMES_MOTHER_DIES_AT, HOMES_LEAVES_AT - 1, HOMES_LEAVES_AT}):
    HOMES.advance(_stop)
    _marks[_stop] = len(HOMES.log)
    if _stop == HOMES_ACTS - 1:
        _keep(HOMES, "before_input")
        HOMES_GIRL_BEFORE = (HOMES.sims[HOMES_GIRL][9], HOMES.sims[HOMES_GIRL][10], HOMES.sims[HOMES_GIRL][11], HOMES.leave_home_second(HOMES_GIRL))
        assert [e.time for e in HOMES.queue if e.kind == world_reference.LEAVE_HOME_KIND and e.subject[1] == HOMES_GIRL] == [HOMES_GIRL_BEFORE[3]]
        HOMES.arrange(HOMES_GIRL, HOMES_BOY)
        HOMES_ENTRIES["input"] = HOMES.log[_marks[_stop]:]
        assert not [e for e in HOMES.queue if e.kind == world_reference.LEAVE_HOME_KIND and e.subject[1] == HOMES_GIRL]
        assert HOMES.sims[HOMES_GIRL][10] and HOMES.sims[HOMES_GIRL][11] is None
        _keep(HOMES, "after_input")
    elif _stop in (HOMES_SEPARATED_AT, HOMES_MOTHER_DIES_AT, HOMES_LEAVES_AT):
        _name = {HOMES_SEPARATED_AT: "separation", HOMES_MOTHER_DIES_AT: "mother_dies", HOMES_LEAVES_AT: "leave_home"}[_stop]
        HOMES_ENTRIES[_name] = HOMES.log[_marks[_stop - 1]:]
        _keep(HOMES, f"after_{_name}")
HOMES.advance(HOMES_END)
_keep(HOMES, "end")
# The run in steps is the run in one go.
assert HOMES.log == _world.log and HOMES.save() == _world.save()
assert homes_situations(HOMES, HOMES_GIRL, HOMES_ACTS) == HOMES_FOUND
# The minor founder at the start (part 2): sim 5 fostered, both foster parents his social
# parents, one `core:moved` at the start, not moved out then (he has left home by the end).
assert HOMES_FOUND["minor_founder_fostered"][0] == HOMES_BOY and HOMES.sims[HOMES_BOY][10]
assert HOMES.sims[HOMES_BOY][12] == list(HOMES_FOUND["minor_founder_fostered"][1:3]) == HOMES_FOUND["minor_founder_fostered"][5]
HOMES_STORY_TWIN = world_reference.check_story_kept(HOMES)


def homes_broken_states() -> list:
    """The load checks of the households can fail (ADR-0018 section 7: one rejected state
    per invariant): each broken copy of the end of "homes" fails `check_households`."""
    world0 = HOMES
    living = sorted(world0.sims)
    dependent = [sim for sim in living if world0.dependent(sim, world0.closed)]
    cared = [sim for sim in dependent if world0.sims[sim][11] is not None]
    ended = [number for number, (_, end) in world0.households.items() if end is not None]
    moved = [sim for sim in living if world0.sims[sim][10]]
    staying = [sim for sim in living if not world0.sims[sim][10]]
    adults_in_carers_home = [sim for sim in living if not world0.dependent(sim, world0.closed) and any(world0.sims[c][9] == world0.sims[sim][9] for c in cared) and world0.sims[sim][11] is None]

    def in_ended(world):
        world.sims[living[0]][9] = ended[0]

    def empty_open(world):
        world.households[ended[0]][1] = None

    def counter(world):
        world.next_household_id -= 1

    def archived_unknown(world):
        sim = sorted(world.archive)[0]
        value = list(world.archive[sim])
        value[11] = world.next_household_id + 5
        world.archive[sim] = tuple(value)

    def moved_wrong(world):
        world.sims[moved[0]][10] = False

    def no_leave_home(world):
        world.queue = [e for e in world.queue if not (e.kind == world_reference.LEAVE_HOME_KIND and e.subject[1] == staying[0])]

    def late_leave_home(world):
        (event,) = [e for e in world.queue if e.kind == world_reference.LEAVE_HOME_KIND and e.subject[1] == staying[0]]
        event.time += 1

    def extra_leave_home(world):
        (event,) = [e for e in world.queue if e.kind == world_reference.LEAVE_HOME_KIND and e.subject[1] == staying[0]]
        twin = copy.copy(event)
        twin.subject = (world_reference.SIM_SUBJECT, moved[0])
        twin.payload = variant(7, varint(moved[0]))
        world.queue.append(twin)

    def no_carer(world):
        world.sims[cared[0]][11] = None

    def carer_elsewhere(world):
        child = cared[0]
        other = next(sim for sim in living if world.sims[sim][9] != world.sims[child][9] and not world.dependent(sim, world.closed))
        world.sims[child][11] = other

    def dependent_carer(world):
        child = cared[0]
        world.sims[child][11] = next(sim for sim in dependent if sim != child and world.sims[sim][9] == world.sims[child][9])

    def adult_with_carer(world):
        world.sims[adults_in_carers_home[0]][11] = world.sims[cared[0]][11]

    names = []
    for name, mutate in [
        ("a living person in a household that has ended", in_ended),
        ("a household not ended without a living member", empty_open),
        ("the counter not above every household", counter),
        ("an archived person in a household that never existed", archived_unknown),
        ("moved_out not as the rule gives it", moved_wrong),
        ("no leave_home for a person not moved out", no_leave_home),
        ("a leave_home not at the drawn second", late_leave_home),
        ("a leave_home for a person moved out", extra_leave_home),
        ("a dependent child without a carer in a household with its parents", no_carer),
        ("a carer in another household", carer_elsewhere),
        ("a carer who is dependent", dependent_carer),
        ("a person not dependent with a carer", adult_with_carer),
    ]:
        world = copy_world(world0)
        mutate(world)
        try:
            world_reference.check_households(world)
        except AssertionError:
            names.append(name)
            continue
        raise AssertionError(f"check_households accepts a broken state: {name}")
    return names


HOMES_BROKEN = homes_broken_states()


# "homes_orphans" (part 1 beyond the note's list; part 2, section 7: orphans now go to a
# relative): the founders and couples of "homes" without the input, `λ` = 0.05, 24 years.
# Partnership 1 separates in year 6.1, so sim 1 lives alone with her children when she dies
# in year 22.4; the two children still dependent then (sims 15 and 16) have no successor in
# her household (their father lives elsewhere, she has no partner): one group for the orphan
# rule. Their nearest relatives of 18 or more are their father sim 3 and their adult full
# siblings sims 11 and 14, all with `φ` = 1/4; the smallest lineage key is the father's: he
# becomes their carer, a biological parent and so no social parent, and both move into his
# household (part 2, section 7: "a biological parent chosen as relative gets no edge"; the
# orphans "move to their nearest relative in another household").
HOMES_ORPHANS_RATE = 0.05
HOMES_ORPHANS_DATA = homes_data(HOMES_ORPHANS_RATE)
_orphans_once = homes_world(HOMES_ORPHANS_DATA)
_orphans_once.advance(HOMES_END)
(HOMES_ORPHANS_PLACEMENT,) = [record for record in _orphans_once.placements if record["t"] > 0]
HOMES_ORPHANED_AT = HOMES_ORPHANS_PLACEMENT["t"]
HOMES_ORPHANS = HOMES_ORPHANS_PLACEMENT["group"]
HOMES_ORPHANS_HOUSEHOLD = HOMES_ORPHANS_PLACEMENT["household"]
assert HOMES_ORPHANS_PLACEMENT["stage"] == "relative" and HOMES_ORPHANS_PLACEMENT["carer"] == 3
assert all(parents_of(_orphans_once, sim)[1] == 3 and HOMES_ORPHANS_PLACEMENT["added"][sim] == [] for sim in HOMES_ORPHANS)
assert HOMES_ORPHANS_PLACEMENT["home"] != HOMES_ORPHANS_HOUSEHOLD
HOMES_ORPHANS_WORLD = homes_world(HOMES_ORPHANS_DATA)
HOMES_ORPHANS_WORLD.advance(HOMES_ORPHANED_AT - 1)
world_reference.check_households(HOMES_ORPHANS_WORLD)
HOMES_ORPHANS_SAVE_BEFORE = HOMES_ORPHANS_WORLD.save()
_before = len(HOMES_ORPHANS_WORLD.log)
HOMES_ORPHANS_WORLD.advance(HOMES_ORPHANED_AT)
HOMES_ORPHANS_ENTRIES = HOMES_ORPHANS_WORLD.log[_before:]
world_reference.check_households(HOMES_ORPHANS_WORLD)
world_reference.check_separations(HOMES_ORPHANS_WORLD)
world_reference.check_market(HOMES_ORPHANS_WORLD)
HOMES_ORPHANS_SAVE_AFTER = HOMES_ORPHANS_WORLD.save()
assert len(HOMES_ORPHANS) >= 2 and set(HOMES_ORPHANS) <= set(HOMES_ORPHANS_WORLD.members(HOMES_ORPHANS_HOUSEHOLD))
assert all(HOMES_ORPHANS_WORLD.sims[sim][11] == 3 and HOMES_ORPHANS_WORLD.sims[sim][12] == [] for sim in HOMES_ORPHANS)
HOMES_ORPHANS_WORLD.advance(HOMES_END)
world_reference.check_households(HOMES_ORPHANS_WORLD)
assert HOMES_ORPHANS_WORLD.log == _orphans_once.log
world_reference.check_story_kept(HOMES_ORPHANS_WORLD)
HOMES_ORPHANS_SAVE_END = HOMES_ORPHANS_WORLD.save()


# ---------------------------------------------------------------------------------------
# Households, part 2 (design note, section 7): three new scenarios, each run in steps
# (`staged`) with saves and entries around its seconds, and in one go (the same log and
# save). The founders' places among the founders were searched (their lineage keys fix
# their deaths, separations and draws), see the notes of the reference.


def check_all(world: world_reference.World) -> None:
    for check in (world_reference.check_households, world_reference.check_separations, world_reference.check_market, world_reference.check_afflictions):
        check(world)


def staged(make, inputs: list, marks: dict, end: int) -> tuple:
    """The world `make()` run to `end` with `inputs` (label, the second it acts, woman, man;
    each stamped one second before) and stops around each second of `marks` (label: second):
    the save at the second before (`before_<label>`) and at the second (`after_<label>`) and
    the entries of the second; around each input the save before and after it (both with the
    clock one second before it acts) and the entries it wrote. Every save passes the load
    checks. Returns (world, saves, entries); `saves` starts with `after_start` and ends with
    `end`."""
    world = make()
    check_all(world)
    saves, entries = {"after_start": world.save()}, {"start": list(world.log)}
    stops = []
    for label, t in marks.items():
        stops.append((t - 1, 0, "before", label))
        stops.append((t, 1, "after", label))
    for label, acts, woman, man in inputs:
        stops.append((acts - 1, 2, "input", (label, woman, man)))
    mark = {}
    for clock, _, kind, what in sorted(stops, key=lambda row: (row[0], row[1])):
        if clock > world.closed:
            world.advance(clock)
        assert world.closed == clock
        if kind == "before":
            check_all(world)
            saves[f"before_{what}"] = world.save()
            mark[what] = len(world.log)
        elif kind == "after":
            check_all(world)
            saves[f"after_{what}"] = world.save()
            entries[what] = world.log[mark[what]:]
        else:
            label, woman, man = what
            check_all(world)
            saves[f"before_{label}"] = world.save()
            before = len(world.log)
            world.arrange(woman, man)
            entries[label] = world.log[before:]
            check_all(world)
            saves[f"after_{label}"] = world.save()
    world.advance(end)
    check_all(world)
    saves["end"] = world.save()
    once = make()
    for _, acts, woman, man in sorted(inputs, key=lambda row: row[1]):
        once.advance(acts - 1)
        once.arrange(woman, man)
    once.advance(end)
    assert once.log == world.log and once.save() == world.save(), "the run in steps is the run in one go"
    world_reference.check_story_kept(world)
    return world, saves, entries


def placements_of(world: world_reference.World, **match) -> list:
    return [record for record in world.placements if all(record[key] == value for key, value in match.items())]


def biological(world: world_reference.World, child: int) -> tuple:
    parents = parents_of(world, child)
    return parents[:2] if parents is not None else ()


# "homes_kin" (part 2, section 7: a placement in a foster household after the start, an adult
# relative in the same household who takes over without a move, a carer who is not moved out
# leaves home and the children follow the rule; the coordinator's (a): a separation while an
# adult child lives in the couple's household). Seed 0xff, tag `test:kin9`, clan's test data
# with `λ` = 0.025, 32 years, no input. Seven founders with reference alleles:
#   sim 1  woman 20  couple with sim 7 (partnership 2)
#   sim 2  man 40    single (fills a place)
#   sim 3  woman 18  couple with sim 5 (partnership 1)
#   sim 4  man 40    single
#   sim 5  man 20
#   sim 6  man 40    single
#   sim 7  man 22
# Sim 7 dies in year 3.75, sim 1 in year 5.19: her children have no relative of 18 or more
# and go to the foster household of sim 3 (the only woman of 18 or more in an open
# partnership), she and sim 5 their social parents. Partnership 1 separates in year 21.14
# with two adults not moved out in the household (a foster child and her own). Sim 3 dies in
# year 30.01: the representative's (sim 19) nearest relatives (`φ` = 1/4) are her adult full
# siblings and her father; the smallest key is her sister sim 17, who still lives in the
# household: she takes over without a move. She leaves home in year 30.85: the children she cared for have no
# other parent in the household; the rule chooses her again and they follow her.
KIN_TAG = "test:kin9"
KIN_RATE = 0.025
KIN_YEARS = 32
KIN_END = KIN_YEARS * YEAR - 1
KIN_FOUNDERS = [
    _founder(20, FEMALE),
    _founder(40, MALE),
    _founder(18, FEMALE),
    _founder(40, MALE),
    _founder(20, MALE),
    _founder(40, MALE),
    _founder(22, MALE),
]
KIN_COUPLES = [(2, 4), (0, 6)]
KIN_DATA = homes_data(KIN_RATE)
KIN_KEYS = [founder_key(KIN_TAG, index) for index in range(len(KIN_FOUNDERS))]


def kin_world() -> world_reference.World:
    return world_reference.World(0xFF, KIN_TAG, False, KIN_FOUNDERS, KIN_COUPLES, data=KIN_DATA)


_kin_once = kin_world()
_kin_once.advance(KIN_END)
(KIN_FOSTER,) = placements_of(_kin_once, stage="foster")
(KIN_SAME,) = [r for r in placements_of(_kin_once, stage="relative", trigger="death of the carer") if r["home"] == r["household"]]
(KIN_LEFT,) = placements_of(_kin_once, trigger="carer left home")
(KIN_SEPARATION,) = [d for d in _kin_once.separation_details if d["adults_at_home"]]
assert KIN_FOSTER["t"] > 0 and KIN_FOSTER["relatives"] == [] and KIN_FOSTER["carer"] == 3
assert all(KIN_FOSTER["added"][child] == [3, 5] for child in KIN_FOSTER["group"]) and KIN_FOSTER["home"] != KIN_FOSTER["household"]
assert KIN_LEFT["carer"] == KIN_SAME["carer"] and KIN_LEFT["home"] != KIN_LEFT["household"]
KIN_MARKS = {"foster": KIN_FOSTER["t"], "separation": KIN_SEPARATION["t"], "relative_same_household": KIN_SAME["t"], "carer_leaves_home": KIN_LEFT["t"]}
KIN, KIN_SAVES, KIN_ENTRIES = staged(kin_world, [], KIN_MARKS, KIN_END)
assert KIN.log == _kin_once.log


# "homes_step" (part 2, section 7 from ADR-0018's acceptance list: a mother of 17 keeps her
# child; the mother dies, the stepfather cares on, he pairs anew and the child moves with
# him, he separates and the child goes with him (the coordinator's (b)); and a group with
# neither a relative nor a foster woman: a household of their own after the start). Seed
# 0xff, tag `test:step`, the repository's data with accident 0.02 for both sexes,
# senescence practically off, the fertility window from 16 (0.25 per month from 16 until 45,
# ten months, no pause, the wish always 40, no twins, `c` 0) and `λ` = 0.05, 14 years.
# Seven founders with reference alleles:
#   sim 1  man 26    single at the start (the stepfather)
#   sim 2  woman 16.5 couple with sim 7 (partnership 1)
#   sim 3  woman 24  single at the start
#   sims 4 to 6  men 40, single (they fill places)
#   sim 7  man 18
# and two inputs `ArrangePartnership`: sim 2 with sim 1 one month after the first month
# start after partnership 1 ends (sim 7 dies in year 0.87), and sim 3 with sim 1 one month
# after the first month start after sim 2's death (year 2.28).
STEP_TAG = "test:step"
STEP_RATE = 0.05
STEP_FERTILITY = (0.25, 16, 45, 10)
STEP_YEARS = 14
STEP_END = STEP_YEARS * YEAR - 1
STEP_FOUNDERS = [
    _founder(26, MALE),
    (-(16 * YEAR + YEAR // 2), FEMALE, genotype(WILD, WILD)),
    _founder(24, FEMALE),
    _founder(40, MALE),
    _founder(40, MALE),
    _founder(40, MALE),
    _founder(18, MALE),
]
STEP_COUPLES = [(1, 6)]
STEP_DATA = world_reference.death_test_data(0.02, fertility=STEP_FERTILITY, life_values={"core:separation_rate": STEP_RATE})
STEP_KEYS = [founder_key(STEP_TAG, index) for index in range(len(STEP_FOUNDERS))]
STEP_MOTHER, STEP_FATHER, STEP_STEPFATHER, STEP_SECOND = 2, 7, 1, 3


def step_world() -> world_reference.World:
    return world_reference.World(0xFF, STEP_TAG, False, STEP_FOUNDERS, STEP_COUPLES, data=STEP_DATA)


def step_inputs() -> list:
    """The two inputs by their rule: (label, the second each acts, woman, man)."""
    probe = step_world()
    while probe.partnerships[1][3] is None:
        probe.advance(probe.closed + MONTH)
    first = world_reference.month_start_after(probe.partnerships[1][3][0]) + MONTH
    probe.advance(first - 1)
    probe.arrange(STEP_MOTHER, STEP_STEPFATHER)
    while STEP_MOTHER in probe.sims:
        probe.advance(probe.closed + MONTH)
    second = world_reference.month_start_after(probe.archive[STEP_MOTHER][1]) + MONTH
    return [("input_1", first, STEP_MOTHER, STEP_STEPFATHER), ("input_2", second, STEP_SECOND, STEP_STEPFATHER)]


STEP_INPUTS = step_inputs()
_step_once = step_world()
for _, _acts, _woman, _man in STEP_INPUTS:
    _step_once.advance(_acts - 1)
    _step_once.arrange(_woman, _man)
_step_once.advance(STEP_END)
STEP_CHILD = min(sim for sim in set(_step_once.sims) | set(_step_once.archive) if parents_of(_step_once, sim) is not None and parents_of(_step_once, sim)[:2] == (STEP_MOTHER, STEP_FATHER))
STEP_BIRTH = _step_once.born_of(STEP_CHILD)
STEP_MOTHER_DIES = _step_once.archive[STEP_MOTHER][1]
(STEP_STEP_SUCCESSION,) = [row for row in _step_once.carer_log if row[1] == STEP_CHILD and row[4] == "succession: the partner (step-parent)"]
(STEP_SEPARATION,) = _step_once.separation_details
(STEP_OWN,) = placements_of(_step_once, stage="own household")
assert 17 * YEAR <= STEP_BIRTH - _step_once.born_of(STEP_MOTHER) < 18 * YEAR, "a mother of 17"
assert STEP_STEP_SUCCESSION[0] == STEP_MOTHER_DIES and STEP_STEP_SUCCESSION[3] == STEP_STEPFATHER
assert STEP_CHILD in STEP_SEPARATION["moved"] and STEP_SEPARATION["man"] == STEP_STEPFATHER
assert STEP_OWN["t"] > 0 and STEP_CHILD in STEP_OWN["group"] and STEP_OWN["relatives"] == [] and STEP_OWN["women"] == []
STEP_MARKS = {
    "birth_mother_17": STEP_BIRTH,
    "mother_dies": STEP_MOTHER_DIES,
    "separation": STEP_SEPARATION["t"],
    "own_household": STEP_OWN["t"],
}
STEP, STEP_SAVES, STEP_ENTRIES = staged(step_world, STEP_INPUTS, STEP_MARKS, STEP_END)
assert STEP.log == _step_once.log


# "homes_neither" (part 2, section 7: orphans who move to their nearest relative in another
# household, their social parent; the coordinator's (c): a separation in which a dependent
# child in the couple's household is cared for by neither partner, and (a) again). Seed 0xff,
# tag `test:neither17`, the data of "homes" (`λ` = 0.01), 34 years. Seven founders with
# reference alleles: sim 6 woman 18 and sim 2 man 20 a couple (partnership 1), sims 1, 3, 4,
# 5, 7 men of 20, single. One input `ArrangePartnership`: their daughter sim 10 with sim 5
# at the first month start after her 18th birthday. Sim 5 dies in year 27.41, sim 10 in year
# 30.32: her children's nearest relatives of 18 or more are their grandparents and their
# aunts and uncles (`φ` = 1/8); the smallest key is an aunt, sim 17, who still lives at home
# with the couple: the children move into the couple's household, sim 17 their carer and
# social parent. Partnership 1 separates in year 33.02: the children's carer is neither
# partner; literally (part 1, section 4, kept by part 2, section 5) the woman takes over;
# adults not moved out stay without a carer.
NEITHER_TAG = "test:neither17"
NEITHER_YEARS = 34
NEITHER_END = NEITHER_YEARS * YEAR - 1
NEITHER_FOUNDERS = [_founder(20, MALE)] * 5 + [_founder(18, FEMALE), _founder(20, MALE)]
NEITHER_COUPLES = [(5, 1)]
NEITHER_DATA = homes_data(0.01)
assert NEITHER_DATA.hash == HOMES_DATA.hash
NEITHER_KEYS = [founder_key(NEITHER_TAG, index) for index in range(len(NEITHER_FOUNDERS))]
NEITHER_DAUGHTER, NEITHER_PARTNER = 10, 5


def neither_world() -> world_reference.World:
    return world_reference.World(0xFF, NEITHER_TAG, False, NEITHER_FOUNDERS, NEITHER_COUPLES, data=NEITHER_DATA)


_probe = neither_world()
_probe.advance(NEITHER_END)
assert parents_of(_probe, NEITHER_DAUGHTER)[:2] == (6, 2) and _probe.sex_of(NEITHER_DAUGHTER) == FEMALE
NEITHER_INPUTS = [("input", world_reference.month_start_after(_probe.born_of(NEITHER_DAUGHTER) + 18 * YEAR), NEITHER_DAUGHTER, NEITHER_PARTNER)]
_neither_once = neither_world()
_neither_once.advance(NEITHER_INPUTS[0][1] - 1)
_neither_once.arrange(NEITHER_DAUGHTER, NEITHER_PARTNER)
_neither_once.advance(NEITHER_END)
(NEITHER_PLACED,) = placements_of(_neither_once, trigger="death of the carer")
(NEITHER_SEPARATION,) = _neither_once.separation_details
assert NEITHER_PLACED["stage"] == "relative" and NEITHER_PLACED["home"] != NEITHER_PLACED["household"]
assert all(NEITHER_PLACED["added"][child] == [NEITHER_PLACED["carer"]] for child in NEITHER_PLACED["group"])
assert NEITHER_SEPARATION["cared_by_neither"] and all(carer == NEITHER_PLACED["carer"] for _, carer in NEITHER_SEPARATION["cared_by_neither"])
assert NEITHER_PLACED["carer"] in NEITHER_SEPARATION["adults_at_home"]
NEITHER_MARKS = {"relative_elsewhere": NEITHER_PLACED["t"], "separation": NEITHER_SEPARATION["t"]}
NEITHER, NEITHER_SAVES, NEITHER_ENTRIES = staged(neither_world, NEITHER_INPUTS, NEITHER_MARKS, NEITHER_END)
assert NEITHER.log == _neither_once.log


# The review exception of part 1 (part 2, section 6): "homes" up to one second before the
# 18th birthday of sim 8 (a son of sims 2 and 7, cared for by his mother since the
# separation), then `ArrangePartnership(2, 7)` stamped in that second: it acts in his
# birthday second, sims 2 and 7 found a new household there, he (of age then) stays and
# keeps his carer until his birthday later in that second. The save at that clock loads
# (`check_households` accepts it); two broken copies of it are rejected.
REVIEW_CHILD, REVIEW_MOTHER, REVIEW_FATHER = 8, 2, 7
REVIEW_STAMP = HOMES.born_of(REVIEW_CHILD) + 18 * YEAR - 1
assert REVIEW_STAMP < HOMES_END and REVIEW_STAMP != HOMES_ACTS - 1
REVIEW = homes_world(HOMES_DATA)
REVIEW.advance(HOMES_ACTS - 1)
REVIEW.arrange(HOMES_GIRL, HOMES_BOY)
REVIEW.advance(REVIEW_STAMP)
assert REVIEW.sims[REVIEW_CHILD][11] == REVIEW_MOTHER and REVIEW.dependent(REVIEW_CHILD, REVIEW_STAMP)
assert REVIEW.arrange_rejection(REVIEW_MOTHER, REVIEW_FATHER) is None
REVIEW.arrange(REVIEW_MOTHER, REVIEW_FATHER)
check_all(REVIEW)
REVIEW_SAVE = REVIEW.save()
REVIEW_HOUSEHOLD = REVIEW.sims[REVIEW_MOTHER][9]
assert REVIEW.households[REVIEW_HOUSEHOLD][0] == REVIEW_STAMP + 1 and REVIEW.sims[REVIEW_CHILD][9] != REVIEW_HOUSEHOLD
assert REVIEW.sims[REVIEW_CHILD][11] == REVIEW_MOTHER


def section_6_broken_states() -> list:
    """One rejected state per load check of part 2, section 6: the social parents (on the end
    of "homes": sim 5 has the social parents [2, 7]), the review exception (on `REVIEW`) and
    the world that never started (`check_unstarted_households`)."""
    child = next(sim for sim in sorted(HOMES.sims) if parents_of(HOMES, sim) is not None)
    other = next(
        sim for sim in sorted(REVIEW.sims)
        if REVIEW.dependent(sim, REVIEW.closed + 1) and REVIEW.sims[sim][9] != REVIEW_HOUSEHOLD and REVIEW.sims[sim][11] is not None
    )

    def social(value):
        def mutate(world):
            world.sims[HOMES_BOY][12] = list(value)
        return mutate

    def mother_as_social(world):
        world.sims[child][12] = [parents_of(world, child)[0]]

    def archived_twice(world):
        sim = next(sim for sim in sorted(world.archive) if world.archive[sim][9] is not None)
        stranger = 6
        assert stranger not in world.archive[sim][9][:2] and stranger != sim
        value = list(world.archive[sim])
        value[13] = (stranger, stranger)
        world.archive[sim] = tuple(value)

    def dependent_next_second(world):
        world.sims[other][11] = REVIEW_MOTHER

    def formed_before(world):
        world.households[REVIEW_HOUSEHOLD][0] = REVIEW_STAMP

    cases = [
        ("a social parent who is no person (SimId 0)", HOMES, social([0, 7])),
        ("a social parent not below the SimId counter", HOMES, social([2, HOMES.next_sim_id])),
        ("a person her own social parent", HOMES, social([2, HOMES_BOY])),
        ("a biological parent as a social parent", HOMES, mother_as_social),
        ("a social parent listed twice", HOMES, social([2, 7, 2])),
        ("an archived person with a social parent listed twice", HOMES, archived_twice),
        ("a carer in a household formed in the second after the clock, the child still dependent then", REVIEW, dependent_next_second),
        ("a carer in another household formed before the second after the clock, the child of age then", REVIEW, formed_before),
    ]
    names = []
    for name, world0, mutate in cases:
        world = copy_world(world0)
        mutate(world)
        try:
            world_reference.check_households(world)
        except AssertionError:
            names.append(name)
            continue
        raise AssertionError(f"check_households accepts a broken state: {name}")
    for name, counter, households in [
        ("a world that never started with the household counter at 2", 2, {}),
        ("a world that never started with a household", 2, {1: [0, None]}),
    ]:
        try:
            world_reference.check_unstarted_households(counter, households)
        except AssertionError:
            names.append(name)
            continue
        raise AssertionError(f"check_unstarted_households accepts a broken state: {name}")
    world_reference.check_unstarted_households(1, {})
    return names


SECTION_6_BROKEN = section_6_broken_states()


def loosened_carer_check() -> list:
    """The coordinator's decision on finding (c) of part 2: a dependent child without a carer
    is valid if no member of its household is moved out. Accepted (in memory, not reached by
    a run): "homes_kin" at the second sim 17 took over without a move (943744849), with the
    carer of the youngest of her group cleared: a child without a carer beside children
    cared for by an adult relative who never left home, the shape of finding (c); part 1's
    check (every member without a carer) rejected it. Rejected: the end of "homes", a
    dependent child of household 1 (where its father, moved out, lives) with its carer
    cleared."""
    world = kin_world()
    world.advance(KIN_SAME["t"])
    child = KIN_SAME["group"][-1]
    home = world.sims[child][9]
    assert all(not world.sims[other][10] for other in world.members(home))
    world.sims[child][11] = None
    world_reference.check_households(world)
    members = world.members(home)
    assert not all(world.sims[other][11] is None for other in members), "part 1's check would accept it too"
    accepted = (
        f"homes_kin at {KIN_SAME['t']} with the carer of sim {child} cleared: household {home} members {members},"
        f" none moved out, carers {[(other, world.sims[other][11]) for other in members]}"
    )
    broken = copy_world(HOMES)
    (child,) = [sim for sim in sorted(broken.members(1)) if broken.dependent(sim, broken.closed) and broken.sims[sim][11] is not None][:1]
    broken.sims[child][11] = None
    assert any(broken.sims[other][10] for other in broken.members(1))
    try:
        world_reference.check_households(broken)
    except AssertionError:
        return [accepted, f"rejected: end of homes, sim {child} of household 1 (members {broken.members(1)}, moved out {[o for o in broken.members(1) if broken.sims[o][10]]}) with its carer cleared"]
    raise AssertionError("check_households accepts a dependent child without a carer beside a moved-out member")


LOOSENED_CARER_CHECK = loosened_carer_check()
# The bytes of the two saves of a world that never started that the loader rejects.
UNSTARTED_COUNTER_2 = EMPTY_SAVE.replace(EMPTY_PROJECTION, EMPTY_PROJECTION[:6] + varint(2) + EMPTY_PROJECTION[7:])
UNSTARTED_WITH_HOUSEHOLD = EMPTY_SAVE.replace(
    EMPTY_PROJECTION, EMPTY_PROJECTION[:6] + varint(2) + EMPTY_PROJECTION[7:11] + seq([varint(1) + household_state(0)]) + EMPTY_PROJECTION[12:]
)

def summary(record: dict) -> tuple:
    """(second, group, stage, carer, household before -> after, social parents added, entries)."""
    return (record["t"], record["group"], record["stage"], record["carer"], f"{record['home']} -> {record['household']}", record["added"], record["entries"])


HOMES_ORPHANS_SITUATIONS = {
    "orphans_to_their_father_in_another_household_no_edge": summary(HOMES_ORPHANS_PLACEMENT),
}


def _moves_of(world, child: int, why_prefix: str) -> list:
    return [(t, household, entry) for t, kind, household, sims, why, entry in world.home_log if kind == "moved" and sims == [child] and why.startswith(why_prefix)]


KIN_SITUATIONS = {
    "foster_household_after_start": summary(KIN_FOSTER) + (f"foster parents (she, then her partner) {KIN_FOSTER['added'][KIN_FOSTER['representative']]}",),
    "separation_adult_child_stays": (KIN_SEPARATION["t"], KIN_SEPARATION["woman"], KIN_SEPARATION["man"], f"adults not moved out {KIN_SEPARATION['adults_at_home']} stay in household {KIN_SEPARATION['home']}", f"moved with him {KIN_SEPARATION['moved']}"),
    "adult_relative_same_household_no_move": summary(KIN_SAME),
    "carer_leaves_home_rule_chooses_her_again": summary(KIN_LEFT),
}
assert KIN_SAME["entries"] == [] and KIN_LEFT["entries"]

STEP_GROUP_AT_DEATH = [row for row in STEP.carer_log if row[0] == STEP_MOTHER_DIES]
STEP_SITUATIONS = {
    "mother_of_17_keeps_her_child": (
        STEP_BIRTH, f"mother {STEP_MOTHER} age {(STEP_BIRTH - STEP.born_of(STEP_MOTHER)) / YEAR:.4f}", f"child {STEP_CHILD}",
        [row for row in STEP.carer_log if row[1] == STEP_CHILD and row[0] == STEP_BIRTH], "no placement" if not placements_of(STEP, t=STEP_BIRTH) else "PLACEMENT",
    ),
    "stepfather_cares_on": (STEP_MOTHER_DIES, STEP_GROUP_AT_DEATH, f"social parents of {STEP_CHILD}: {STEP.sims[STEP_CHILD][12] if STEP_CHILD in STEP.sims else STEP.archive[STEP_CHILD][13]}"),
    "stepfather_pairs_child_moves": (STEP_INPUTS[1][1], f"partnership {[n for n, v in STEP.partnerships.items() if v[:2] == [STEP_SECOND, STEP_STEPFATHER]]}", _moves_of(STEP, STEP_CHILD, "partnership")),
    "stepfather_separates_child_goes": (STEP_SEPARATION["t"], STEP_SEPARATION["woman"], STEP_SEPARATION["man"], f"moved with him {STEP_SEPARATION['moved']}", f"stayed with her {STEP_SEPARATION['stayed']}", _moves_of(STEP, STEP_CHILD, "separation")),
    "own_household_after_start": summary(STEP_OWN),
}
assert STEP_SITUATIONS["mother_of_17_keeps_her_child"][4] == "no placement"
assert all(row[3] == STEP_STEPFATHER for row in STEP_GROUP_AT_DEATH)

NEITHER_SITUATIONS = {
    "orphans_to_relative_in_another_household_social_parent": summary(NEITHER_PLACED),
    "separation_child_cared_for_by_neither": (
        NEITHER_SEPARATION["t"], NEITHER_SEPARATION["woman"], NEITHER_SEPARATION["man"], f"(child, carer) {NEITHER_SEPARATION['cared_by_neither']}",
        [row for row in NEITHER.carer_log if row[0] == NEITHER_SEPARATION["t"]],
    ),
    "separation_adult_child_stays": (NEITHER_SEPARATION["t"], f"adults not moved out {NEITHER_SEPARATION['adults_at_home']}", f"moved with him {NEITHER_SEPARATION['moved']}"),
}


def placement_row(world: world_reference.World, record: dict) -> str:
    """One application of the orphan rule as a readable row."""
    if record["stage"] == "relative":
        candidates = " relatives (sim, phi, key) " + str([(sim, str(phi), f"{key:016x}") for sim, phi, key in record["relatives"]])
    elif record["stage"] == "foster":
        candidates = " foster women (sim, key, FOSTER_RANK) " + str([(sim, f"{key:016x}", f"{value:016x}") for sim, key, value in record["ranks"]])
        candidates += f" month {record['t'] // MONTH}"
    else:
        candidates = f" no relative, foster women {record['women']}"
    bio = {child: biological(world, child) for child in record["group"]}
    return (
        f"t {record['t']} (year {record['t'] / YEAR:.4f}) trigger {record['trigger']} group {record['group']} representative"
        f" {record['representative']} (key {world.key_of(record['representative']):016x}) stage {record['stage']}{candidates}"
        f" carer {record['carer']} household {record['home']} -> {record['household']} biological parents {bio}"
        f" social parents added {record['added']} entries {record['entries']}"
    )


def scenario_rows(name: str, world: world_reference.World, founders: list, couples: list, keys: list, inputs: list, situations: dict) -> list:
    """The story of a scenario of the households, part 2, as readable rows."""
    rows = []
    for index, (born, sex, _) in enumerate(founders):
        sim = index + 1
        couple = next((f" couple with sim {m + 1 if w == index else w + 1}" for w, m in couples if index in (w, m)), " single")
        rows.append(
            f"{name}_founder sim {sim} {'female' if sex == FEMALE else 'male'} born {born} (age {-born / YEAR:g}){couple} key {keys[index]:016x}"
            f" leave_age {hand_leave_age(0xFF, keys[index])}"
        )
    for label, acts, woman, man in inputs:
        rows.append(f"{name}_{label} ArrangePartnership(woman {woman}, man {man}) stamped {acts - 1} acts at {acts} (year {acts / YEAR:.4f})")
    for key, value in situations.items():
        rows.append(f"{name}_situation {key} reached: {value}")
    for record in world.placements:
        rows.append(f"{name}_placement {placement_row(world, record)}")
    for number, (formed, ended) in sorted(world.households.items()):
        members = sorted({sim for t, kind, household, sims, *_ in world.home_log if household == number and kind in ("formed", "moved") for sim in sims})
        rows.append(f"{name}_household {number} formed {formed} (year {formed / YEAR:.4f}) ended {'open' if ended is None else f'{ended} (year {ended / YEAR:.4f})'} ever {members} now {world.members(number)}")
    for t, kind, household, sims, why, entry_id in world.home_log:
        rows.append(f"{name}_home_event year {t / YEAR:.4f} t {t} {kind} household {household} sims {sims} ({why}) entry {entry_id}")
    for t, child, old, new, why in world.carer_log:
        rows.append(f"{name}_carer year {t / YEAR:.4f} t {t} child {child} {old} -> {new} ({why})")
    for number, (woman, man, formed, ended) in sorted(world.partnerships.items()):
        rows.append(f"{name}_partnership {number} woman {woman} man {man} formed {formed} ended {split_ended(ended)}")
    for detail in world.separation_details:
        rows.append(
            f"{name}_separation year {detail['t'] / YEAR:.4f} t {detail['t']} woman {detail['woman']} man {detail['man']} household {detail['home']}"
            f" stayed_with_her {detail['stayed']} moved_with_him {detail['moved']} adults_not_moved_out_staying {detail['adults_at_home']}"
            f" cared_for_by_neither (child, carer) {detail['cared_by_neither']}"
        )
    for left, sim, cause in deaths_in(world):
        rows.append(f"{name}_death sim {sim} at {left} (year {left / YEAR:.4f}) of {cause_name(cause)}")
    for sim in sorted(set(world.sims) | set(world.archive)):
        parents = parents_of(world, sim)
        value = world.sims.get(sim)
        state = (
            f" household {value[9]} moved_out {value[10]} carer {value[11]} social_parents {value[12]}"
            if value is not None
            else f" archived household {world.archive[sim][11]} moved_out {world.archive[sim][12]} social_parents {list(world.archive[sim][13])}"
        )
        if parents is not None:
            rows.append(
                f"{name}_child {sim} {'female' if world.sex_of(sim) == FEMALE else 'male'} born {world.born_of(sim)} (year {world.born_of(sim) / YEAR:.4f})"
                f" parents {parents} leave_age {hand_leave_age(0xFF, world.key_of(sim))} at the end:{state}"
            )
        else:
            rows.append(f"{name}_founder_end sim {sim} at the end:{state}")
    return rows


def scenario_output(name: str, world: world_reference.World, saves: dict, entries: dict) -> list:
    """Saves, entries (bytes and JSON), year hashes and the log hash of a scenario."""
    rows = [f"{name}_save_{label} {save.hex()}" for label, save in saves.items()]
    for label, lines in entries.items():
        for line in lines:
            rows.append(f"{name}_{label}_entry {line.hex()}")
            if entry_json(line) is not None:
                rows.append(f"{name}_{label}_entry_json {entry_json(line)}")
    rows.append(f"{name}_year_hashes {[format(value, '016x') for value in world.year_hashes]}")
    rows.append(f"{name}_log_hash {digest(b''.join(world.log))} entries {len(world.log)}")
    return rows


def entry_json(line: bytes) -> str:
    """The JSON form (ADR-0004 sections 3 and 7) of an entry of the households or of the
    entries around them, read back from its bytes: subjects `{"sim": id}`, causes `{"event":
    id}`, `{"plan": {"by", "planned_at"}}`, `{"input": seq}`, `{"rule": code}` or
    `{"random": {...}}` (16 hex digits), the payload by its code (only the kinds of these
    rows); written from the ADR, not from the CLI."""
    position = 0

    def number() -> int:
        nonlocal position
        value, shift = 0, 0
        while True:
            byte = line[position]
            position += 1
            value |= (byte & 0x7F) << shift
            shift += 7
            if not byte & 0x80:
                return value

    def zigzag() -> int:
        value = number()
        return (value >> 1) ^ -(value & 1)

    def text() -> str:
        nonlocal position
        length = number()
        value = line[position:position + length].decode()
        position += length
        return value

    entry_id, t = number(), zigzag()
    subjects = []
    for _ in range(number()):
        assert number() == 0
        subjects.append({"sim": number()})
    causes = []
    for _ in range(number()):
        kind = number()
        if kind == 0:
            causes.append({"event": number()})
        elif kind == 1:
            causes.append({"plan": {"by": number(), "planned_at": zigzag()}})
        elif kind == 2:
            causes.append({"input": number()})
        elif kind == 3:
            causes.append({"rule": text()})
        else:
            causes.append({"random": {name: f"{number():016x}" for name in ("tag", "subject", "occasion", "detail")}})
    kind = number()
    readers = {
        2: lambda: {"core:birthday": {"age_years": number()}},
        4: lambda: {"core:death": {"cause": text()}},
        5: lambda: {"core:partnered": {"partnership": number()}},
        9: lambda: {"core:separated": {"partnership": number()}},
        10: lambda: {"core:household_formed": {"household": number()}},
        11: lambda: {"core:moved": {"household": number()}},
    }
    if kind not in readers:
        return None
    payload = readers[kind]()
    assert position == len(line), line.hex()
    return json.dumps({"id": entry_id, "t": t, "subjects": subjects, "causes": causes, "payload": payload}, separators=(",", ":"))


def homes_story(world: world_reference.World) -> list:
    """The story of "homes" as readable rows: the search for `λ`, the founders, the
    situations, every household (formed, ended, members over time), every entry of the
    households, every change of a carer, the partnerships, separations, deaths, children
    with their drawn ages of leaving."""
    rows = [f"homes_search lambda {rate!r} situations {names}" for rate, names in HOMES_TRIED]
    rows.append(f"homes_lambda {HOMES_RATE!r} years {HOMES_YEARS} data_hash {HOMES_DATA.hash:016x}")
    for index, (born, sex, _) in enumerate(HOMES_FOUNDERS):
        sim = index + 1
        couple = next((f" couple with sim {m + 1 if w == index else w + 1}" for w, m in HOMES_COUPLES if index in (w, m)), " single")
        rows.append(
            f"homes_founder sim {sim} {'female' if sex == FEMALE else 'male'} age {-born / YEAR:g}{couple} key {HOMES_KEYS[index]:016x}"
            f" leave_age {hand_leave_age(0xFF, HOMES_KEYS[index])} accident_due {years(world.cause_due(sim, ACCIDENT)) if sim in world.sims else 'died'}"
        )
    for name in HOMES_SITUATIONS:
        rows.append(f"homes_situation {name} reached: {HOMES_FOUND[name]}")
    rows.append(
        f"homes_input ArrangePartnership(woman {HOMES_GIRL}, man {HOMES_BOY}) stamped {HOMES_ACTS - 1} acts at {HOMES_ACTS}"
        f" (year {HOMES_ACTS / YEAR:.4f}, her age {(HOMES_ACTS - world.born_of(HOMES_GIRL)) / YEAR:.4f}); before it: household"
        f" {HOMES_GIRL_BEFORE[0]} moved_out {HOMES_GIRL_BEFORE[1]} carer {HOMES_GIRL_BEFORE[2]} leave_home at {HOMES_GIRL_BEFORE[3]}"
        f" (age {(HOMES_GIRL_BEFORE[3] - world.born_of(HOMES_GIRL)) // YEAR}), cancelled"
    )
    for number, (formed, ended) in sorted(world.households.items()):
        members = sorted({sim for t, kind, household, sims, *_ in world.home_log if household == number and kind in ("formed", "moved") for sim in sims})
        rows.append(f"homes_household {number} formed {formed} (year {formed / YEAR:.4f}) ended {'open' if ended is None else f'{ended} (year {ended / YEAR:.4f})'} ever {members} now {world.members(number)}")
    for t, kind, household, sims, why, entry_id in world.home_log:
        rows.append(f"homes_home_event year {t / YEAR:.4f} t {t} {kind} household {household} sims {sims} ({why}) entry {entry_id}")
    for t, child, old, new, why in world.carer_log:
        rows.append(f"homes_carer year {t / YEAR:.4f} child {child} {old} -> {new} ({why})")
    for number, (woman, man, formed, ended) in sorted(world.partnerships.items()):
        rows.append(f"homes_partnership {number} woman {woman} man {man} formed {formed} ended {split_ended(ended)}")
    for t, woman, man, stayed, moved in world.separation_homes:
        rows.append(f"homes_separation year {t / YEAR:.4f} woman {woman} man {man} stayed_with_her {stayed} moved_with_him {moved}")
    for left, sim, cause in deaths_in(world):
        rows.append(f"homes_death sim {sim} at {left} (year {left / YEAR:.4f}) of {cause_name(cause)}")
    for sim in sorted(set(world.sims) | set(world.archive)):
        parents = parents_of(world, sim)
        if parents is not None:
            rows.append(
                f"homes_child {sim} {'female' if world.sex_of(sim) == FEMALE else 'male'} born {world.born_of(sim)} (year {world.born_of(sim) / YEAR:.4f})"
                f" parents {parents} leave_age {hand_leave_age(0xFF, world.key_of(sim))}"
            )
    return rows


# The households change no story (design note of the households, section 8): every
# scenario run here, run again with the households switched off (a switch of this
# reference), has the same births, deaths, partnerships, separations, attempts,
# conceptions, pregnancies and choices of the market. The twin's log is the log of the model
# before the households (no household entry, no `leave_home`, the same insertion numbers),
# so its hash is the log hash printed before this step.
_tiny_end = tiny_world()
_tiny_end.advance(YEAR - 1)
STORY_TWINS = {
    name: world_reference.check_story_kept(world)
    for name, world in [
        ("tiny", _tiny_end),
        ("mortal", MORTAL),
        ("tie", TIE),
        ("sick_on", SICK_ON),
        ("sick_off", SICK_OFF),
        ("switched_on", SWITCHED_ON),
        ("on_off_on", ON_OFF_ON),
        ("two_periods", TWO_PERIODS),
        ("no_disease", NO_DISEASE),
        ("off_at_due", OFF_AT_DUE),
        ("same", SAME),
        ("accident_wins", ACCIDENT_WINS),
        ("save_load", SAVE_LOAD),
        ("partners", PARTNERS),
        ("clan", CLAN),
        ("clan_null", CLAN_NULL),
        ("clan_on", CLAN_ON),
        ("fever", FEVER),
        ("blight", BLIGHT),
        ("clan_pause", CLAN_PAUSE),
        ("clan_wish", CLAN_WISH),
        ("clan_second", CLAN_SECOND),
        ("twins", TWINS),
        ("split", SPLIT),
        ("market", MARKET),
        ("similar", SIMILAR),
        ("similar_without_terms", SIMILAR_WITHOUT),
        ("afflicted", AFFLICTED),
        ("homes", HOMES),
    ]
}
assert b"".join(STORY_TWINS["tiny"].log) == b"".join(TINY_LOG[:2] + [entry(3, YEAR - 1, [SIM_1], [variant(1, varint(2), signed(0))], variant(2, varint(1)))])
for _name, _world in [("clan", CLAN), ("twins", TWINS), ("split", SPLIT), ("market", MARKET), ("partners", PARTNERS)]:
    world_reference.check_households(_world)


def household_rows(name: str, world: world_reference.World) -> list:
    """Per household its formation, end and members; per sim its household, `moved_out`
    and carer at the end; the changes of carers."""
    rows = []
    for number, (formed, ended) in sorted(world.households.items()):
        rows.append(f"{name}_household {number} formed {formed} ended {ended} members {world.members(number)}")
    rows.append(
        f"{name}_homes (sim, household, moved_out, carer) {[(sim, value[9], value[10], value[11]) for sim, value in sorted(world.sims.items())]}"
        f" archived (sim, household, moved_out) {[(sim, value[11], value[12]) for sim, value in sorted(world.archive.items())]}"
    )
    rows.append(f"{name}_carers (time, child, old, new, why) {world.carer_log}")
    return rows


# ---------------------------------------------------------------------------------------
# The pedigree with kinship and social parents (step 10, design note of the pedigree): the
# text of `pedigree.json` (`World.pedigree_json`, compact as serde_json writes it) of the
# household scenarios of part 2, of "afflicted" with the switch on (and then off), of a
# small scenario "loci" (the order of the loci and of the afflictions), and the known
# answers of the kinship query (`World.kinship_of`). Nothing of a run changes: every world
# here is one of the scenarios above, or run again to a second of it.


def world_at(make, inputs: list, clock: int) -> world_reference.World:
    """`make()` run to `clock` with those of `inputs` (label, the second it acts, woman, man;
    each stamped one second before) that act at or before `clock`."""
    world = make()
    for _, acts, woman, man in sorted(inputs, key=lambda row: row[1]):
        assert acts - 1 != clock, "no export at the stamp of an input"
        if acts <= clock:
            world.advance(acts - 1)
            world.arrange(woman, man)
    world.advance(clock)
    check_all(world)
    return world


def view_of(world: world_reference.World) -> dict:
    """The pedigree read back from its text: what the export holds."""
    return json.loads(world.pedigree_json())


def person_of(view: dict, sim: int) -> dict:
    (person,) = [person for person in view["persons"] if person["sim"] == sim]
    return person


# "homes_step" at the second of the separation (clock 190743122): the stepfather sim 1 cares
# for sim 8 (his mother sim 2 died, his father sim 7 too) without a social parent edge;
# partnership 3 ended by separation, partnerships 1 and 2 by death. Its save is the save
# `homes_step_save_after_separation`.
STEP_AT_SEPARATION = world_at(step_world, STEP_INPUTS, STEP_SEPARATION["t"])
assert STEP_AT_SEPARATION.save() == STEP_SAVES["after_separation"]
_view = view_of(STEP_AT_SEPARATION)
_child = person_of(_view, STEP_CHILD)
assert (_child["mother"], _child["father"], _child["carer"], _child["social_parents"]) == (STEP_MOTHER, STEP_FATHER, STEP_STEPFATHER, [])
assert [(p["partnership"], p["ended_by"]) for p in _view["partnerships"]] == [(1, "death"), (2, "death"), (3, "separation")]
assert all(not person["social_parents"] for person in _view["persons"])
# "homes_kin" at the end: the foster children 8, 9, 12 with the social parents [3, 5] (she,
# then her partner), the children of sim 3 with their sister sim 17; partnership 1 ended by
# separation, partnership 2 by death.
_view = view_of(KIN)
assert {p["sim"]: p["social_parents"] for p in _view["persons"] if p["social_parents"]} == {
    8: [3, 5], 9: [3, 5], 12: [3, 5], 19: [17], 20: [17], 21: [17], 24: [17], 25: [17], 26: [17]
}
assert [(p["partnership"], p["ended_by"]) for p in _view["partnerships"]] == [(1, "separation"), (2, "death")]
# "homes_neither" at the end: sims 31, 32, 34 with the social parent [17] (their aunt) and
# the carer sim 6 (their grandmother, who took them over at the separation of partnership 1
# without an edge: the separation's rule for a child cared for by neither partner).
_view = view_of(NEITHER)
assert [(p["sim"], p["social_parents"], p["carer"]) for p in _view["persons"] if p["social_parents"]] == [
    (31, [17], 6), (32, [17], 6), (34, [17], 6), (36, [17], None)
]
assert [(p["partnership"], p["ended_by"]) for p in _view["partnerships"]] == [(1, "separation"), (2, "death")]

# "afflicted" (the switch on from the start) at the end of year 37 (clock 38 · YEAR − 1):
# sim 9 died of core:disease_ar in year 37.96 (affected at his death); sims 1 and 3 (and
# sim 17, born in the run) have core:disease_xr with the onset 0 (affected); sim 6
# (core:disease_ad_late, onset 48.0) and sim 7 (core:disease_ad_partial, onset 53.5) have not
# reached their onsets (not affected). Then an override block switches the diseases off,
# stamped at that clock: it is in the state at once, but holds only from the next second, so
# the export at the stamp is that of the switch on; at the end of year 38 every living person
# is not affected, and sim 9 (dead while the switch was on) still is.
AFFLICTED_CLOCK = 38 * YEAR - 1
AFFLICTED_AT_38 = afflicted_world()
AFFLICTED_AT_38.advance(AFFLICTED_CLOCK)
check_all(AFFLICTED_AT_38)
AFFLICTED_JSON_38 = AFFLICTED_AT_38.pedigree_json()
_view = json.loads(AFFLICTED_JSON_38)
_dead = person_of(_view, 9)
assert (_dead["died"], _dead["cause_of_death"], _dead["diseases"]) == (
    AFFLICTED_DISEASE_DEATHS[0][0], "core:disease_ar", [{"disease": "core:disease_ar", "onset": 0.0, "affected": True}]
)
AFFLICTED_AFFECTED = sorted((p["sim"], d["disease"], d["affected"]) for p in _view["persons"] for d in p["diseases"])
assert AFFLICTED_AFFECTED == [
    (1, "core:disease_xr", True), (3, "core:disease_xr", True), (6, "core:disease_ad_late", False),
    (7, "core:disease_ad_partial", False), (9, "core:disease_ar", True), (17, "core:disease_xr", True),
]
AFFLICTED_OFF = copy_world(AFFLICTED_AT_38)
AFFLICTED_OFF.override(False)
assert AFFLICTED_OFF.switch is False and AFFLICTED_OFF.closed == AFFLICTED_CLOCK
AFFLICTED_JSON_OFF_AT_STAMP = AFFLICTED_OFF.pedigree_json()
assert AFFLICTED_JSON_OFF_AT_STAMP == AFFLICTED_JSON_38, "the override holds from the next second"
AFFLICTED_OFF.advance(39 * YEAR - 1)
check_all(AFFLICTED_OFF)
AFFLICTED_JSON_OFF_39 = AFFLICTED_OFF.pedigree_json()
_view = json.loads(AFFLICTED_JSON_OFF_39)
assert sorted((p["sim"], d["affected"]) for p in _view["persons"] for d in p["diseases"]) == [
    (1, False), (3, False), (6, False), (7, False), (9, True), (17, False)
]
assert sorted(AFFLICTED_OFF.archive) == [9], "nobody else died in year 38"

# "loci" (new, the pedigree only): seed 0xff, tag `test:loci2`, the repository's data, the
# switch on, no couple; a woman of 30 with alleles at all five disease loci and a man of 30
# with three (in the founders' list in the map's order of the file). The export right after
# `Start` (clock −1, so `r` = 0): `alleles` in the order of the genome map's file (ar,
# ad_late, ad_partial, xr, mt; not by data ID, which would be ad_late, ad_partial, ar, mt,
# xr), `diseases` in the order of the calibration's list (ar, ad_late, ad_partial, xr, mt). Her afflictions are
# ar and xr (onset 0, affected), ad_late and ad_partial (onsets 48.5 and 58.3, not
# affected); his are xr (affected) and the visible core:disease_mt (onset 27.3, affected).
# The tag was chosen among `test:loci`, `test:loci2`, `test:loci3`, `test:pedigree`,
# `test:pedigree_loci` as the first whose man has core:disease_mt (penetrance 0.5) with an
# onset under 30.
LOCI_TAG = "test:loci2"
LOCI_FOUNDERS = [
    (-30 * YEAR, FEMALE, [("core:disease_ar", [1, 1]), ("core:disease_ad_late", [1, 0]), ("core:disease_ad_partial", [0, 1]), ("core:disease_xr", [1, 1]), ("core:disease_mt", [1])]),
    (-30 * YEAR, MALE, [("core:disease_ar", [0, 1]), ("core:disease_xr", [1]), ("core:disease_mt", [1])]),
]
LOCI = world_reference.World(0xFF, LOCI_TAG, True, LOCI_FOUNDERS)
check_all(LOCI)
LOCI_SAVE_AFTER_START = LOCI.save()
LOCI_JSON = LOCI.pedigree_json()
_view = json.loads(LOCI_JSON)
assert [[a["locus"] for a in p["alleles"]] for p in _view["persons"]] == [
    ["core:disease_ar", "core:disease_ad_late", "core:disease_ad_partial", "core:disease_xr", "core:disease_mt"],
    ["core:disease_ar", "core:disease_xr", "core:disease_mt"],
]
assert [[(d["disease"], d["affected"]) for d in p["diseases"]] for p in _view["persons"]] == [
    [("core:disease_ar", True), ("core:disease_ad_late", False), ("core:disease_ad_partial", False), ("core:disease_xr", True)],
    [("core:disease_xr", True), ("core:disease_mt", True)],
]

PEDIGREE_EXPORTS = [
    ("homes_step_after_separation", STEP_AT_SEPARATION, "homes_step_save_after_separation"),
    ("homes_step_end", STEP, "homes_step_save_end"),
    ("homes_kin_end", KIN, "homes_kin_save_end"),
    ("homes_neither_end", NEITHER, "homes_neither_save_end"),
    ("afflicted_year_38", AFFLICTED_AT_38, "afflicted_save_year_38"),
    ("afflicted_off_year_39", AFFLICTED_OFF, "afflicted_off_save_year_39"),
    ("loci_after_start", LOCI, "loci_save_after_start"),
]


# The known answers of the kinship query (design note of the pedigree, sections 4 and 6):
# per case the scenario, its save (label) and clock, the two SimIds, `φ` exact and as the
# JSON number. "homes_step" at its end has half siblings, "market" at its end first
# cousins; both have the others. The relation of each pair is checked from the parents.


def half_siblings(world: world_reference.World, a: int, b: int) -> bool:
    pa, pb = parents_of(world, a), parents_of(world, b)
    return a != b and pa is not None and pb is not None and len(set(pa[:2]) & set(pb[:2])) == 1


def kinship_cases(name: str, world: world_reference.World, save: str, pairs: list) -> list:
    """(label, scenario, save, clock, a, b, φ or None) per pair (relation, a, b), the relation
    checked; φ symmetric."""
    rows = []
    for relation, a, b in pairs:
        known = [sim in world.sims or sim in world.archive for sim in (a, b)]
        if relation == "parent_child":
            assert a in parents_of(world, b)[:2]
        elif relation == "full_siblings":
            assert full_siblings(world, a, b)
        elif relation == "half_siblings":
            assert half_siblings(world, a, b)
        elif relation == "first_cousins":
            assert first_cousins(world, a, b) and not full_siblings(world, a, b) and not half_siblings(world, a, b)
        elif relation == "unrelated_founders":
            assert parents_of(world, a) is None and parents_of(world, b) is None and a != b
        elif relation.startswith("itself"):
            assert a == b and all(known)
        else:
            assert relation == "unknown" and not all(known)
        value = world.kinship_of(a, b)
        assert value == world.kinship_of(b, a)
        rows.append((f"kinship_{name}_{relation}", name, save, world.closed, a, b, value))
    return rows


KINSHIP_CASES = kinship_cases(
    "homes_step",
    STEP,
    "homes_step_save_end",
    [
        ("parent_child", STEP_MOTHER, STEP_CHILD),
        ("full_siblings", 10, 11),
        ("half_siblings", STEP_CHILD, 9),
        ("unrelated_founders", STEP_STEPFATHER, STEP_SECOND),
        ("itself_founder", STEP_STEPFATHER, STEP_STEPFATHER),
        ("itself_child", STEP_CHILD, STEP_CHILD),
        ("unknown", STEP.next_sim_id, STEP_CHILD),
    ],
) + kinship_cases(
    "market",
    MARKET,
    "market_save_end_of_year_39",
    [
        ("parent_child", 1, 12),
        ("full_siblings", 13, 14),
        ("first_cousins", 24, 26),
        ("unrelated_founders", 1, 2),
        ("itself_founder", 1, 1),
        ("itself_child", 24, 24),
        ("unknown", MARKET.next_sim_id, 1),
    ],
)
assert [(label, value) for label, *_, value in KINSHIP_CASES] == [
    ("kinship_homes_step_parent_child", 0.25),
    ("kinship_homes_step_full_siblings", 0.25),
    ("kinship_homes_step_half_siblings", 0.125),
    ("kinship_homes_step_unrelated_founders", 0.0),
    ("kinship_homes_step_itself_founder", 0.5),
    ("kinship_homes_step_itself_child", 0.5),
    ("kinship_homes_step_unknown", None),
    ("kinship_market_parent_child", 0.25),
    ("kinship_market_full_siblings", 0.25),
    ("kinship_market_first_cousins", 0.0625),
    ("kinship_market_unrelated_founders", 0.0),
    ("kinship_market_itself_founder", 0.5),
    ("kinship_market_itself_child", 0.5),
    ("kinship_market_unknown", None),
], "the values of spec 2.7 (W2.7a to W2.7c, W2.7f): exact fractions without inbreeding"


def kinship_row(label: str, name: str, save: str, clock: int, a: int, b: int, value) -> str:
    answer = "none (unknown_sim)" if value is None else f"phi {Fraction(value)} json {world_reference.json_number(value)}"
    return f"{label} scenario {name} save {save} clock {clock} sims {a} {b} {answer}"


# ---------------------------------------------------------------------------------------
# The threshold disease in the world (step 11, design note of the threshold disease;
# genetics specification 3.6, ADR-0019): scenario "threshold" on the test map
# tests/golden/data_liability (tools/liability_fixture.py: the repository's data plus the
# trait `core:disease_liability`, eight additive loci `test:liability_1` to `_8` with
# effect 1 and `d_ref` 1, `sigma_ref` 2, the phenotype row h² 0.6, c² 0 in both ages, and
# the threshold disease naming it; `K` 0.01, so `T` = probit(0.99) with the recorded bits
# 40029c5c4630ff0c). Everything else is the repository's: the market, the separation, the
# fertility of 5.6, twins, households. Seed 0xff, the switch on (the diseases kill), eight
# founders in this order (their places fix their lineage keys):
#   sim 1  woman 24  every liability locus [1, 1] (G = 4)     couple with sim 2
#   sim 2  man 26    every liability locus [1, 1] (G = 4)
#   sim 3  woman 24  every liability locus [1, 0] (G = 0)     couple with sim 4
#   sim 4  man 26    every liability locus [1, 1] (G = 4)
#   sim 5  woman 30  loci 1 to 4 [1, 1], 5 to 8 [1, 0] (G = 2) couple with sim 8
#   sim 6  man 40    every liability locus [1, 1] (G = 4)     single
#   sim 7  woman 45  core:disease_ad_late [1, 0] and every liability locus [1, 1] (G = 4),
#                    single: two afflictions, ad_late before the liability (data order)
#   sim 8  man 30    no allele 1 (G = −4)
# and 12 years (to clock 12 · YEAR − 1). The tag is the first of `test:threshold`,
# `test:threshold2`, ..., `test:threshold40` with which the 12 years have a child born with
# the disease (`L > T`), a child born without it, and a death of core:disease_liability:
# `test:threshold5`. With it the founders 1, 2, 4, 6, 7 have the disease (sim 2 with G = 4
# only just, `L` 2.43), 3, 5, 8 not; children 10 and 13 (of sims 1 and 2, G = 4) have it,
# 9, 11, 12, 14, 15, 16 not (sim 12 just below `T`: `L` 2.322, G = 1.5; sims 14 and 15
# identical twins, the same G, their own E); sim 4 dies of core:disease_liability in year
# 11.47 (his onset 23.06 was before the start, so his exposure runs from it).
THRESHOLD_DIR = world_reference.ROOT / "tests" / "golden" / "data_liability"
THRESHOLD_DATA = world_reference.Data(*data_reference.documents(THRESHOLD_DIR))
(THRESHOLD_DISEASE,) = [disease for disease in THRESHOLD_DATA.diseases if disease.liability is not None]
THRESHOLD_T = THRESHOLD_DISEASE.threshold
assert THRESHOLD_DISEASE.code == THRESHOLD_DISEASE.liability.id == "core:disease_liability"
assert data_reference.f64_bits(THRESHOLD_T) == "40029c5c4630ff0c" and THRESHOLD_T == 2.32634787404084
THRESHOLD_YEARS = 12
THRESHOLD_END = THRESHOLD_YEARS * YEAR - 1
THRESHOLD_TAGS = ["test:threshold"] + [f"test:threshold{number}" for number in range(2, 41)]


def liability_alleles(doses: list) -> list:
    """The founders' alleles at `test:liability_1` to `_8` for the doses (2: [1, 1], 1:
    [1, 0], 0: none)."""
    return [(f"test:liability_{n}", [1, 1] if dose == 2 else [1, 0]) for n, dose in enumerate(doses, start=1) if dose]


THRESHOLD_FOUNDERS = [
    (-24 * YEAR, FEMALE, liability_alleles([2] * 8)),
    (-26 * YEAR, MALE, liability_alleles([2] * 8)),
    (-24 * YEAR, FEMALE, liability_alleles([1] * 8)),
    (-26 * YEAR, MALE, liability_alleles([2] * 8)),
    (-30 * YEAR, FEMALE, liability_alleles([2, 2, 2, 2, 1, 1, 1, 1])),
    (-40 * YEAR, MALE, liability_alleles([2] * 8)),
    (-45 * YEAR, FEMALE, [("core:disease_ad_late", [1, 0])] + liability_alleles([2] * 8)),
    (-30 * YEAR, MALE, []),
]
THRESHOLD_COUPLES = [(0, 1), (2, 3), (4, 7)]


def threshold_world_of(tag: str) -> world_reference.World:
    return world_reference.World(0xFF, tag, True, THRESHOLD_FOUNDERS, THRESHOLD_COUPLES, data=THRESHOLD_DATA)


def has_threshold(world: world_reference.World, sim: int) -> bool:
    """Whether the sim holds core:disease_liability (living or archived)."""
    return any(affliction.disease.code == THRESHOLD_DISEASE.code for affliction in world.creation_of(sim)[6])


def threshold_criteria(world: world_reference.World) -> tuple:
    """(a child with the disease, a child without it, a death of the disease)."""
    children = [sim for sim in sorted(set(world.sims) | set(world.archive)) if world.parent_keys(sim) is not None]
    affected = [sim for sim in children if has_threshold(world, sim)]
    deaths = [sim for sim, value in world.archive.items() if value[2] == THRESHOLD_DISEASE.key]
    return bool(affected), len(affected) < len(children), bool(deaths)


THRESHOLD_TRIED = []
for _tag in THRESHOLD_TAGS:
    _world = threshold_world_of(_tag)
    _world.advance(THRESHOLD_END)
    THRESHOLD_TRIED.append((_tag, threshold_criteria(_world)))
    if all(THRESHOLD_TRIED[-1][1]):
        break
THRESHOLD_TAG = THRESHOLD_TRIED[-1][0]
assert THRESHOLD_TAG == "test:threshold5", THRESHOLD_TRIED
THRESHOLD_KEYS = [founder_key(THRESHOLD_TAG, index) for index in range(len(THRESHOLD_FOUNDERS))]


def threshold_world() -> world_reference.World:
    return threshold_world_of(THRESHOLD_TAG)


_probe = threshold_world()
_probe.advance(THRESHOLD_END)
THRESHOLD_AFFECTED = [sim for sim in sorted(set(_probe.sims) | set(_probe.archive)) if has_threshold(_probe, sim)]
assert THRESHOLD_AFFECTED == [1, 2, 4, 6, 7, 10, 13], THRESHOLD_AFFECTED
(THRESHOLD_DEAD,) = [sim for sim, value in _probe.archive.items() if value[2] == THRESHOLD_DISEASE.key]
THRESHOLD_DEATH = _probe.archive[THRESHOLD_DEAD][1]
THRESHOLD_FIRST_CHILD = min(sim for sim in THRESHOLD_AFFECTED if _probe.parent_keys(sim) is not None)
THRESHOLD_BIRTH = _probe.born_of(THRESHOLD_FIRST_CHILD)
assert (THRESHOLD_DEAD, THRESHOLD_FIRST_CHILD) == (4, 10)
THRESHOLD_MARKS = {"affected_birth": THRESHOLD_BIRTH, "liability_death": THRESHOLD_DEATH}
THRESHOLD, THRESHOLD_SAVES, THRESHOLD_ENTRIES = staged(threshold_world, [], THRESHOLD_MARKS, THRESHOLD_END)
assert THRESHOLD.log == _probe.log
THRESHOLD_SIMS = sorted(set(THRESHOLD.sims) | set(THRESHOLD.archive))


def hand_liability(world: world_reference.World, sim: int) -> float:
    """`L` by hand from the genetics specification 3.6 and 4.1 with Python's math, straight
    from the genome and the draw, not from the model: `G = (Σ (d − 1)) / 2` over the eight
    loci in map order (effect 1, load 1, `d_ref` 1, `sigma_ref` 2), `E = normal(0, 1)` at
    (ENV_INDIVIDUAL = 11, lineage key, `str_key("core:disease_liability")`, 0), `L = √0.6 ·
    G + √(1 − 0.6) · E` (the form of 3.6, without the C term: its weight is 0)."""
    key, _, person, _, _, _, _ = world.creation_of(sim)
    acc = 0.0
    for n in range(1, 9):
        locus = THRESHOLD_DATA.loci[f"test:liability_{n}"]
        acc = acc + 1.0 * (float(sum(world_reference.copies_at(person, locus))) - 1.0)
    g = acc / 2.0
    draw = words(chacha_key(0xFF, 11, key, str_key("core:disease_liability"), 0))
    e = world_reference.normal_reference(draw[0], draw[1], 0.0, 1.0)[1]
    return math.sqrt(0.6) * g + math.sqrt(1.0 - 0.6) * e


def threshold_margins(world: world_reference.World, sim: int) -> tuple:
    """The libm margins (`world_reference.libm_margin`, ulps of the result from the rounding
    boundary) of the ln and cos of the draw `E` and, if the sim has the disease, of its
    onset draw (DISEASE_ONSET): a pinned libm within an ulp gives the same bits wherever
    the margin is not tiny."""
    from rng_reference import TAU

    key = world.creation_of(sim)[0]
    tags = [(world_reference.ENV_INDIVIDUAL, "E")]
    if has_threshold(world, sim):
        tags.append((world_reference.DISEASE_ONSET, "onset"))
    margins = []
    for tag, name in tags:
        draw = words(chacha_key(0xFF, tag, key, THRESHOLD_DISEASE.key, 0))
        margins.append((name, world_reference.libm_margin("ln", unit_open0(draw[0])), world_reference.libm_margin("cos", TAU * unit(draw[1]))))
    return tuple(margins)


def disease_due_nudged(world: world_reference.World, sim: int, ulps: int):
    """`World.disease_due` of the sim's affliction core:disease_liability with its budget
    moved by `ulps` ulps (the death second must not hang on the last bit of ln)."""
    (affliction,) = [a for a in world.creation_of(sim)[6] if a.disease.code == THRESHOLD_DISEASE.code]
    born = world.born_of(sim)
    used, since = affliction.exposure
    at_since = world_reference.as_years(since - born)
    x0 = max(at_since, affliction.onset)
    budget = world_reference.step_ulps(world_reference.budget_of(0xFF, world.key_of(sim), THRESHOLD_DISEASE.code), ulps)
    age = world_reference.disease_age(x0, max(budget - used, 0.0), THRESHOLD_DISEASE.hazard)
    return since + world_reference.from_years_ceil(age - at_since)


THRESHOLD_ROWS = []
for _sim in THRESHOLD_SIMS:
    _key, _sex, _person, _parents, _origin, _age, _held = THRESHOLD.creation_of(_sim)
    _parts = world_reference.phenotype_parts(0xFF, THRESHOLD_DISEASE.liability, _person, _key, _parents, _age)
    _l = THRESHOLD.liability_of(_sim, THRESHOLD_DISEASE)
    assert _l == _parts["z"] == hand_liability(THRESHOLD, _sim), _sim
    assert (_l > THRESHOLD_T) == has_threshold(THRESHOLD, _sim) == (_sim in THRESHOLD_AFFECTED)
    _margins = threshold_margins(THRESHOLD, _sim)
    assert min(min(m[1], m[2]) for m in _margins) > 1e-3, (_sim, _margins)
    THRESHOLD_ROWS.append((_sim, _key, _parents, _parts, _l, _held, _margins))
# The founders 1 to 8 are created at Start (age 0 − born), the children at their births
# (age 0); the weights do not depend on the age, so the phenotype at any other age is the
# same bits (checked at 0 and 50 years).
assert all(
    world_reference.phenotype(0xFF, THRESHOLD_DISEASE.liability, THRESHOLD.creation_of(row[0])[2], row[1], row[2], age) == row[4]
    for row in THRESHOLD_ROWS
    for age in (0, 50 * YEAR)
)
THRESHOLD_CLOSEST = min(abs(row[4] - THRESHOLD_T) for row in THRESHOLD_ROWS)
assert THRESHOLD_CLOSEST > 1e-3
assert all(disease_due_nudged(THRESHOLD, THRESHOLD_DEAD, ulps) == THRESHOLD_DEATH for ulps in (-2, -1, 0, 1, 2))
# The order of the afflictions: sim 7 holds core:disease_ad_late, then core:disease_liability
# (data order), both lethal, both planned at Start after her senescence and accident.
assert [a.disease.code for a in THRESHOLD.sims[7][4]] == ["core:disease_ad_late", "core:disease_liability"]


def threshold_broken_saves() -> list:
    """The load check of the afflictions (design note section 3) rejects a state in which a
    person holds core:disease_liability without `L > T`, and one in which a person lacks it
    with `L > T`. Both are the save after Start (clock −1) changed in one person, and both
    keep everything else consistent: (held) sim 3 (`L` −0.874) gets the affliction with the
    onset of her draw and the exposure (0.0, since 0), appended after her (no) other
    afflictions, and its death planned as Start plans it (by her creation entry at 0; the
    next insertion number, so the counter grows by one); (missing) sim 1 (`L` 3.835) loses
    her affliction and its planned death (the event and its plan; the counter stays).
    `check_afflictions` rejects both and accepts the save as it is. Returns (name, save)."""
    base = threshold_world()
    world_reference.check_afflictions(base)
    assert base.save() == THRESHOLD_SAVES["after_start"]
    held = copy_world(base)
    key = held.key_of(3)
    _, mean, sd, low, high = THRESHOLD_DISEASE.onset
    draw = words(chacha_key(0xFF, world_reference.DISEASE_ONSET, key, THRESHOLD_DISEASE.key, 0))
    onset = min(max(world_reference.normal_reference(draw[0], draw[1], mean, sd)[1], low), high)
    affliction = world_reference.Affliction(THRESHOLD_DISEASE, onset, 0)
    assert not has_threshold(held, 3) and THRESHOLD_ROWS[2][4] <= THRESHOLD_T
    held.sims[3][4].append(affliction)
    due = held.disease_due(3, affliction)
    assert due is not None
    held._schedule(due, world_reference.DEATH, (world_reference.SIM_SUBJECT, 3), world_reference.DEATH_KIND, world_reference.death_payload(3, THRESHOLD_DISEASE.key), None, held.created_by[3], 0)
    missing = copy_world(base)
    assert has_threshold(missing, 1)
    missing.sims[1][4] = [a for a in missing.sims[1][4] if a.disease.code != THRESHOLD_DISEASE.code]
    death = world_reference.death_payload(1, THRESHOLD_DISEASE.key)
    assert len([e for e in missing.queue if e.payload == death]) == 1
    missing.queue = [e for e in missing.queue if e.payload != death]
    saves = []
    for name, world in [("held_without_L_above_T sim 3", held), ("missing_with_L_above_T sim 1", missing)]:
        try:
            world_reference.check_afflictions(world)
        except AssertionError:
            saves.append((name, world.save()))
            continue
        raise AssertionError(f"check_afflictions accepts a broken state: {name}")
    return saves, onset, due


THRESHOLD_BROKEN, THRESHOLD_HELD_ONSET, THRESHOLD_HELD_DUE = threshold_broken_saves()
THRESHOLD_PEDIGREE = THRESHOLD.pedigree_json()


def threshold_rows() -> list:
    """The story of "threshold" as readable rows."""
    rows = [
        f"threshold_data_hash {THRESHOLD_DATA.hash:016x} fixture tests/golden/data_liability T {THRESHOLD_T!r} bits {data_reference.f64_bits(THRESHOLD_T)}"
        f" prevalence 0.01 1-K bits {data_reference.f64_bits(1.0 - 0.01)} weights {THRESHOLD_DISEASE.liability.weights!r} sigma_ref {THRESHOLD_DISEASE.liability.sigma_ref!r}"
        f" str_key {THRESHOLD_DISEASE.key:016x} onset {THRESHOLD_DISEASE.onset!r} hazard {THRESHOLD_DISEASE.hazard!r}",
        f"threshold_tags_tried (tag, (child with, child without, death of it)) {THRESHOLD_TRIED}",
        f"threshold_scenario seed 0xff tag {THRESHOLD_TAG} switch on couples {[(w + 1, m + 1) for w, m in THRESHOLD_COUPLES]} years {THRESHOLD_YEARS} end {THRESHOLD_END}",
    ]
    for index, (born, sex, genes) in enumerate(THRESHOLD_FOUNDERS):
        rows.append(
            f"threshold_founder sim {index + 1} {'female' if sex == FEMALE else 'male'} born {born} key {THRESHOLD_KEYS[index]:016x}"
            f" alleles {json.dumps([[locus, copies] for locus, copies in genes], separators=(',', ':'))}"
        )
    for sim, key, parents, parts, value, held, margins in THRESHOLD_ROWS:
        origin = "founder" if parents is None else f"child of {parents_of(THRESHOLD, sim)[:2]} born {THRESHOLD.born_of(sim)}"
        has = has_threshold(THRESHOLD, sim)
        onset = [a.onset for a in held if a.disease.code == THRESHOLD_DISEASE.code]
        rows.append(
            f"threshold_liability sim {sim} {origin} key {key:016x} G {parts['G']!r} E {parts['E']!r} L {value!r}"
            f" L_bits {data_reference.f64_bits(value)} L>T {value > THRESHOLD_T} has_disease {has}"
            + (f" onset {onset[0]!r}" if has else "")
            + f" afflictions {[a.disease.code for a in held]} libm_margins {[(name, round(ln, 4), round(cos, 4)) for name, ln, cos in margins]}"
        )
    rows.append(f"threshold_affected {THRESHOLD_AFFECTED} closest |L - T| {THRESHOLD_CLOSEST!r}")
    for left, sim, cause in deaths_in(THRESHOLD):
        rows.append(f"threshold_death sim {sim} at {left} (year {left / YEAR:.4f}) of {cause_name(cause)}")
    rows.append(f"threshold_liability_death sim {THRESHOLD_DEAD} at {THRESHOLD_DEATH} budget {world_reference.budget_of(0xFF, THRESHOLD.key_of(THRESHOLD_DEAD), THRESHOLD_DISEASE.code)!r} (the same second with the budget moved by up to two ulps)")
    for number, (woman, man, formed, ended) in sorted(THRESHOLD.partnerships.items()):
        rows.append(f"threshold_partnership {number} woman {woman} man {man} formed {formed} ended {split_ended(ended)}")
    for record in THRESHOLD.multiples:
        rows.append(f"threshold_conception t {record[0]} woman {record[1]} man {record[2]} kind {record[5]}")
    planned = sorted(
        (event.time, event.subject[1], cause_name(world_reference.death_key(event.payload)))
        for event in THRESHOLD.queue
        if event.kind == world_reference.DEATH_KIND and world_reference.death_key(event.payload) == THRESHOLD_DISEASE.key
    )
    rows.append(f"threshold_planned_liability_deaths (time, sim) {[(t, sim) for t, sim, _ in planned]}")
    return rows


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
    ("split", SPLIT_DATA),
    ("market", MARKET_DATA),
    ("pairing", PAIRING_FIXTURE),
    ("similar", SIMILAR_DATA),
    ("similar_without_terms", SIMILAR_WITHOUT_DATA),
    ("afflicted", AFFLICTED_DATA),
    ("homes", HOMES_DATA),
    ("homes_kin", KIN_DATA),
    ("homes_step", STEP_DATA),
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
    for name, data in TEST_DATA:
        print(f"separation_rate_{name} {data.separation_rate!r}")
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
    print(f"split_data_hash {SPLIT_DATA.hash:016x} separation_rate {SPLIT_DATA.separation_rate!r}")
    for row in split_story():
        print(row)
    print(f"split_save_after_start {SPLIT_SAVE_AFTER_START.hex()}")
    print(f"split_save_after_separation {SPLIT_SAVE_AFTER_SEPARATION.hex()}")
    for number, line, fields in SPLIT_SEPARATED:
        label = "split_separation_entry" if number == 1 else f"split_separation_entry_partnership_{number}"
        print(f"{label} {line.hex()}")
        print(f"{label}_json {separated_json(*fields)}")
    print(f"split_repaired_entry {SPLIT_REPAIRED_ENTRY.hex()}")
    print(f"split_save_after_repairing {SPLIT_SAVE_AFTER_REPAIRING.hex()}")
    print(f"split_save_after_year_7 {SPLIT_SAVE_AFTER_YEAR_7.hex()}")
    print(f"split_year_hashes {[format(value, '016x') for value in SPLIT.year_hashes]}")
    print(f"split_log_hash {digest(b''.join(SPLIT.log))}")
    for row in death_rows("split", SPLIT):
        print(row)
    print(f"partners_separations_due {PARTNERS_SEPARATIONS}")
    print(f"partners_rounds (month, her search chance, u) {[(r['n'], r['considered'][0][2], r['considered'][0][3]) for r in PARTNERS.rounds]}")
    # The null twin of the market on "clan" (design note of the market, section 7).
    print(f"clan_null_twin log_hash with_round {digest(b''.join(CLAN.log))} without_round {digest(b''.join(CLAN_NULL.log))}")
    print(f"clan_null_twin census_hashes with_round {[format(value, '016x') for value in CLAN.census_hashes]}")
    print(f"clan_null_twin census_hashes without_round {[format(value, '016x') for value in CLAN_NULL.census_hashes]}")
    print(f"clan_null_twin census_end with_round {CLAN.census().hex()}")
    print(f"clan_null_twin census_end without_round {CLAN_NULL.census().hex()}")
    print(f"clan_null_twin year_hashes without_round {[format(value, '016x') for value in CLAN_NULL.year_hashes]}")
    print(f"clan_log_hash {digest(b''.join(CLAN.log))}")
    print(f"market_data_hash {MARKET_DATA.hash:016x} c {MARKET_DATA.market_height!r}")
    for row in market_story(MARKET):
        print(row)
    print(f"market_save_after_start {MARKET_SAVE_AFTER_START.hex()}")
    print(f"market_save_before_pairing_round (round {MARKET_TWICE_ROUND // MONTH}, clock {MARKET_TWICE_ROUND - 1}) {MARKET_SAVE_BEFORE_TWICE.hex()}")
    print(f"market_save_after_pairing_round (clock {MARKET_TWICE_ROUND}) {MARKET_SAVE_AFTER_TWICE.hex()}")
    print(f"market_pairing_round_by_hand (woman, candidates, chosen, outcome) {MARKET_TWICE_HAND}")
    for row in market_entry_rows("market_partnered_entry", MARKET, MARKET_TWICE_PAIRED[0], MARKET_TWICE_ROUND // MONTH):
        print(row)
    print(f"market_save_before_cousin_round (round {MARKET_COUSIN_ROUND // MONTH}, clock {MARKET_COUSIN_ROUND - 1}) {MARKET_SAVE_BEFORE_COUSIN.hex()}")
    print(f"market_save_after_cousin_round (clock {MARKET_COUSIN_ROUND}) {MARKET_SAVE_AFTER_COUSIN.hex()}")
    for choice in MARKET_COUSIN_RECORD["choices"]:
        if choice["outcome"] == "paired":
            for row in market_entry_rows(f"market_cousin_round_partnered_entry_{choice['partnership']}", MARKET, choice, MARKET_COUSIN_ROUND // MONTH):
                print(row)
    print(
        f"market_cousin woman {MARKET_COUSIN_WOMAN} cousin {MARKET_COUSIN} phi {_phi!r} penalty {world_reference.kinship_weight(MARKET_DATA) * _phi!r}"
        f" gap {_gap!r} G {_g!r} U {_u!r}"
    )
    print(f"market_save_end_of_year_{MARKET_YEARS - 1} {MARKET_SAVE_END.hex()}")
    print(f"market_year_hashes {[format(value, '016x') for value in MARKET.year_hashes]}")
    print(f"market_load_checks_reject {MARKET_BROKEN}")
    print(f"market_log_hash {digest(b''.join(MARKET.log))} entries {len(MARKET.log)}")
    for row in death_rows("market", MARKET):
        print(row)
    # The scenario "similar" (design note of the similarity, section 4).
    print(
        f"similar_data_hash {SIMILAR_DATA.hash:016x} c {SIMILAR_DATA.market_height!r} fixture data_hash_pairing"
        f" {PAIRING_FIXTURE.hash:016x} without_terms {SIMILAR_WITHOUT_DATA.hash:016x}"
        f" w_K {SIMILAR_DATA.pairing_weight_cognition!r} w_H {SIMILAR_DATA.pairing_weight_height!r}"
        f" sigma_ref {SIMILAR_DATA.cognition_trait.sigma_ref!r} {SIMILAR_DATA.height_trait.sigma_ref!r}"
        f" weights cognition {SIMILAR_DATA.cognition_trait.weights!r} height {SIMILAR_DATA.height_trait.weights!r}"
    )
    print(f"similar_trait_keys test:cognition {SIMILAR_DATA.cognition_trait.key:016x} test:height {SIMILAR_DATA.height_trait.key:016x}")
    print(f"similar_generator splitmix64 state {SIMILAR_GENERATOR_STATE} alleles 1 {_ones} of {len(SIMILAR_FOUNDERS) * 160}")
    for index, (born, sex, genes) in enumerate(SIMILAR_FOUNDERS):
        print(
            f"similar_founder sim {index + 1} {'female' if sex == FEMALE else 'male'} born {born} key {SIMILAR_KEYS[index]:016x}"
            f" alleles {json.dumps([[locus, copies] for locus, copies in genes], separators=(',', ':'))}"
        )
    _at_start = similar_world()
    for row in similar_phenotype_rows(_at_start, MONTH):
        print(row)
    for row in similar_story(SIMILAR):
        print(row)
    print(f"similar_without_terms partnerships {[(number, value[0], value[1], value[2] // MONTH) for number, value in sorted(SIMILAR_WITHOUT.partnerships.items())]} log_hash {digest(b''.join(SIMILAR_WITHOUT.log))}")
    print(f"similar_save_after_start {SIMILAR_SAVE_AFTER_START.hex()}")
    print(f"similar_save_before_pairing_round (round {SIMILAR_FIRST_ROUND // MONTH}, clock {SIMILAR_FIRST_ROUND - 1}) {SIMILAR_SAVE_BEFORE_PAIRING.hex()}")
    print(f"similar_save_after_pairing_round (clock {SIMILAR_FIRST_ROUND}) {SIMILAR_SAVE_AFTER_PAIRING.hex()}")
    print(f"similar_save_end_of_round_{SIMILAR_ROUNDS} (clock {SIMILAR_END}) {SIMILAR_SAVE_END.hex()}")
    for row in similar_entry_rows(SIMILAR):
        print(row)
    print(f"similar_pairing_round_by_hand (woman, candidates, U, chosen, outcome) {SIMILAR_HAND}")
    print(f"similar_year_hashes {[format(value, '016x') for value in SIMILAR.year_hashes]}")
    print(f"similar_log_hash {digest(b''.join(SIMILAR.log))} entries {len(SIMILAR.log)}")
    print(f"afflicted_keys {[format(key, '016x') for key in AFFLICTED_KEYS]}")
    for row in afflicted_rows():
        print(row)
    print(f"afflicted_deaths {AFFLICTED_DEATHS}")
    print(f"afflicted_disease_deaths {AFFLICTED_DISEASE_DEATHS}")
    print(f"afflicted_after_start bytes {AFFLICTED_AFTER_START.hex()}")
    print(f"afflicted_after_start hash {digest(AFFLICTED_AFTER_START)}")
    print(f"afflicted_save_after_start {AFFLICTED_SAVE_AFTER_START.hex()}")
    # The households (design note of the households).
    print(f"tiny_leave_home age {TINY_LEAVE_AGE} at {TINY_LEAVE}")
    print(f"pair_leave_home (time, sim, insertion) {PAIR_LEAVES}")
    for name, twin in STORY_TWINS.items():
        print(f"households_null_twin {name} story kept; log_hash without households {digest(b''.join(twin.log))} entries {len(twin.log)}")
    for name, world in [("mortal", MORTAL), ("partners", PARTNERS), ("clan", CLAN), ("blight", BLIGHT), ("split", SPLIT), ("twins", TWINS)]:
        for row in household_rows(name, world):
            print(row)
    for number, line in enumerate(SPLIT_SEPARATION_ENTRIES):
        print(f"split_separation_entries_{number} {line.hex()}")
        print(f"split_separation_entries_{number}_json {entry_json(line)}")
    for number, line in enumerate(MARKET_TWICE_ENTRIES):
        print(f"market_twice_round_entries_{number} {line.hex()}")
    print(f"homes_data_hash {HOMES_DATA.hash:016x} separation_rate {HOMES_DATA.separation_rate!r}")
    for row in homes_story(HOMES):
        print(row)
    for stop, save in HOMES_SAVES.items():
        label = stop if isinstance(stop, str) else f"clock_{stop}"
        print(f"homes_save_{label} {save.hex()}")
    for number, line in enumerate(HOMES_AFTER_START_LOG):
        print(f"homes_start_entry_{number + 1} {line.hex()}")
    for name, lines in HOMES_ENTRIES.items():
        for line in lines:
            print(f"homes_{name}_entry {line.hex()}")
            if entry_json(line) is not None:
                print(f"homes_{name}_entry_json {entry_json(line)}")
    print(f"homes_year_hashes {[format(value, '016x') for value in HOMES.year_hashes]}")
    print(f"homes_log_hash {digest(b''.join(HOMES.log))} entries {len(HOMES.log)}")
    print(f"homes_load_checks_reject {HOMES_BROKEN}")
    print(f"homes_load_checks_reject_part_2 {SECTION_6_BROKEN}")
    print(f"homes_load_check_no_carer accepted: {LOOSENED_CARER_CHECK[0]}; {LOOSENED_CARER_CHECK[1]}")
    print(f"homes_review stamp {REVIEW_STAMP} acts {REVIEW_STAMP + 1}: ArrangePartnership({REVIEW_MOTHER}, {REVIEW_FATHER}) on top of the input of homes; household {REVIEW_HOUSEHOLD} formed at {REVIEW_STAMP + 1}; sim {REVIEW_CHILD} (born {REVIEW.born_of(REVIEW_CHILD)}) stays in household {REVIEW.sims[REVIEW_CHILD][9]} with carer {REVIEW.sims[REVIEW_CHILD][11]}; loads")
    print(f"homes_review_save_after_input {REVIEW_SAVE.hex()}")
    print(f"unstarted_save_household_counter_2 (rejected) {UNSTARTED_COUNTER_2.hex()}")
    print(f"unstarted_save_with_household (rejected) {UNSTARTED_WITH_HOUSEHOLD.hex()}")
    world = HOMES_ORPHANS_WORLD
    print(f"homes_orphans lambda {HOMES_ORPHANS_RATE!r} data_hash {HOMES_ORPHANS_DATA.hash:016x} separations {[(t, number) for t, number, *_ in world.separations]}")
    print(
        f"homes_orphans death of sim 1 at {HOMES_ORPHANED_AT} (year {HOMES_ORPHANED_AT / YEAR:.4f}): {HOMES_ORPHANS} (born {[world.born_of(sim) for sim in HOMES_ORPHANS]})"
        f" move into household {HOMES_ORPHANS_HOUSEHOLD} of their father sim 3, their carer, no social parent"
    )
    for key, value in HOMES_ORPHANS_SITUATIONS.items():
        print(f"homes_orphans_situation {key} reached: {value}")
    print(f"homes_orphans_placement {placement_row(world, HOMES_ORPHANS_PLACEMENT)}")
    for row in [row for row in world.carer_log if row[0] == HOMES_ORPHANED_AT]:
        print(f"homes_orphans_carer {row}")
    for line in HOMES_ORPHANS_ENTRIES:
        print(f"homes_orphans_entry {line.hex()}")
        if entry_json(line) is not None:
            print(f"homes_orphans_entry_json {entry_json(line)}")
    print(f"homes_orphans_save_before_death (clock {HOMES_ORPHANED_AT - 1}) {HOMES_ORPHANS_SAVE_BEFORE.hex()}")
    print(f"homes_orphans_save_after_death (clock {HOMES_ORPHANED_AT}) {HOMES_ORPHANS_SAVE_AFTER.hex()}")
    print(f"homes_orphans_save_end {HOMES_ORPHANS_SAVE_END.hex()}")
    print(f"homes_orphans_year_hashes {[format(value, '016x') for value in world.year_hashes]}")
    print(f"homes_orphans_log_hash {digest(b''.join(world.log))} entries {len(world.log)}")
    # The households, part 2: the new scenarios (design note, section 7).
    for name, world, data, founders, couples, keys, inputs, situations, saves, entries, extra in [
        ("homes_kin", KIN, KIN_DATA, KIN_FOUNDERS, KIN_COUPLES, KIN_KEYS, [], KIN_SITUATIONS, KIN_SAVES, KIN_ENTRIES, f"tag {KIN_TAG} lambda {KIN_RATE!r} years {KIN_YEARS} clan's test data"),
        ("homes_step", STEP, STEP_DATA, STEP_FOUNDERS, STEP_COUPLES, STEP_KEYS, STEP_INPUTS, STEP_SITUATIONS, STEP_SAVES, STEP_ENTRIES, f"tag {STEP_TAG} lambda {STEP_RATE!r} years {STEP_YEARS} fertility window {STEP_FERTILITY}"),
        ("homes_neither", NEITHER, NEITHER_DATA, NEITHER_FOUNDERS, NEITHER_COUPLES, NEITHER_KEYS, NEITHER_INPUTS, NEITHER_SITUATIONS, NEITHER_SAVES, NEITHER_ENTRIES, f"tag {NEITHER_TAG} lambda 0.01 years {NEITHER_YEARS} the data of homes"),
    ]:
        print(f"{name}_data_hash {data.hash:016x} {extra}")
        for row in scenario_rows(name, world, founders, couples, keys, inputs, situations):
            print(row)
        for row in scenario_output(name, world, saves, entries):
            print(row)
    # Log hash: xxh3-64 over the postcard bytes of all entries in order (ADR-0004).
    print(f"log_hash_empty {digest(b'')}")
    print(f"log_hash_entry {digest(SAMPLE_ENTRY)}")
    print(f"log_hash_entry_twice {digest(SAMPLE_ENTRY + SAMPLE_ENTRY)}")
    # The pedigree (step 10): the saves of the new stops, then per export its clock, length,
    # xxh3-64 of the UTF-8 text and the text of `pedigree.json` (compact, no newline).
    print(f"loci_founders tag {LOCI_TAG} switch on {[(born, 'female' if sex == FEMALE else 'male', alleles) for born, sex, alleles in LOCI_FOUNDERS]} keys {[format(founder_key(LOCI_TAG, index), '016x') for index in range(len(LOCI_FOUNDERS))]}")
    print(f"loci_save_after_start {LOCI_SAVE_AFTER_START.hex()}")
    print(f"afflicted_save_year_38 (clock {AFFLICTED_CLOCK}) {AFFLICTED_AT_38.save().hex()}")
    print(f"afflicted_off_save_year_39 (override off stamped {AFFLICTED_CLOCK}, clock {AFFLICTED_OFF.closed}) {AFFLICTED_OFF.save().hex()}")
    print(f"afflicted_affected_year_38 (sim, disease, affected) {AFFLICTED_AFFECTED}")
    print(f"afflicted_off_at_stamp_pedigree (override off stamped {AFFLICTED_CLOCK}, clock {AFFLICTED_CLOCK}, the switch of the state off) equals afflicted_year_38_pedigree_json: {AFFLICTED_JSON_OFF_AT_STAMP == AFFLICTED_JSON_38}")
    for name, world, save in PEDIGREE_EXPORTS:
        text = world.pedigree_json()
        print(f"{name}_pedigree clock {world.closed} save {save} length {len(text.encode())} hash {digest(text.encode())}")
        print(f"{name}_pedigree_json {text}")
    for case in KINSHIP_CASES:
        print(kinship_row(*case))
    # The threshold disease in the world (step 11, design note of the threshold disease).
    for row in threshold_rows():
        print(row)
    for row in household_rows("threshold", THRESHOLD):
        print(row)
    for row in scenario_output("threshold", THRESHOLD, THRESHOLD_SAVES, THRESHOLD_ENTRIES):
        print(row)
    print(f"threshold_load_check broken held: onset {THRESHOLD_HELD_ONSET!r} death planned at {THRESHOLD_HELD_DUE}")
    for name, save in THRESHOLD_BROKEN:
        print(f"threshold_load_rejected {name} {save.hex()}")
    print(f"threshold_end_pedigree clock {THRESHOLD.closed} save threshold_save_end length {len(THRESHOLD_PEDIGREE.encode())} hash {digest(THRESHOLD_PEDIGREE.encode())}")
    print(f"threshold_end_pedigree_json {THRESHOLD_PEDIGREE}")

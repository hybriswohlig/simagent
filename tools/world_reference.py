#!/usr/bin/env python3
"""A model of the world of sim-core, written from the ADRs and independent of the Rust
code: the event queue (ADR-0002), the log entries (ADR-0004), the canonical projection
(ADR-0003 5.2), the save (ADR-0006 section 2) and death through hazard budgets
(ADR-0001). The goldens and the reference bytes of the tests come from it.

What the model knows:

- `Start` writes `core:started`, plans the year end series (insertion number 1, first at
  the last second of year 0, period one year) and then per founder, in list order: the
  creation entry, the first birthday (the first anniversary at or after the start) and
  the deaths in the order of the causes.
- Events run in the order of their key: time, phase (0 domain, 1 observer), micro step,
  class (general 500, death 900, observer 1000), subject (kind, ID), kind (year end 1,
  birthday 2, death 3), insertion number.
- A birthday writes `core:birthday` and plans the next one a year later. The year end
  observer sees the clock one second before its own second, with its next occurrence
  already in the queue.
- Death: per person and cause a budget `B = exponential(DEATH_BUDGET, 1.0)` at the key
  (tag DEATH_BUDGET, subject lineage key, occasion `str_key` of the cause, detail 0). A
  cause with the hazard `r` per year must act `ceil(B / r * YEAR)` seconds on a person
  until the person dies of it, never if `B / r` is 100 000 years or more (ADR-0002). A
  cause for everyone acts on a founder from the start on. A cause for the homozygous
  (C5) acts only on a sim with the dummy allele twice and only while the switch of the
  hereditary diseases is on; such a sim keeps its exposure (the time the switch was on
  in closed periods, and since when it is on now). The deaths of a sim are planned in
  the order of the causes. The death writes `core:death` with the plan and the draw as
  causes, moves the sim with its exposure to the archive and cancels its other pending
  events.
- An override block is an input: stamped with the last closed second, acting in the
  next one, one entry `core:param_changed` per change.
- Couples: after the founders, `Start` forms a partnership per couple of its list (a
  woman and a man by their places among the founders); `ArrangePartnership` forms one
  between two living sims. Each gets the next PartnershipId and one entry
  `core:partnered` (subjects the woman and the man). A death ends the open partnership
  of the sim: it keeps the second of the death. If the switch flips, every living
  homozygous sim (by ID) updates its exposure, and if a cause for the homozygous exists,
  its deaths from the first such cause on are cancelled and planned again in the order
  of the causes, by that entry.

Usage: .venv/bin/python tools/world_reference.py (prints nothing; the other tools use it)
"""

import math

import xxhash

from postcard_reference import (
    DUMMY,
    couple,
    death_cause,
    exposure,
    genotype,
    f64,
    founder,
    lineage_key,
    param_change,
    partnership,
    seq,
    signed,
    string,
    variant,
    varint,
    world_params,
)
from rng_reference import chacha_key, exponential_reference, founder_key, str_key, words

YEAR = 364 * 86_400
NEVER_YEARS = 100_000.0
# The range of the clock (ADR-0002): a time beyond it is never.
MAX_ABS_SECS = 1_000_000 * YEAR
# tests/golden/rng_tags.txt
DEATH_BUDGET = 4

PROJECTION_VERSION = 6
CORE_VERSION = 6
FORMAT_VERSION = 1
EMPTY_DATA_HASH = xxhash.xxh3_64_intdigest(b"", seed=0)

NONE = b"\x00"

# Event classes and kinds (tests/golden/event_codes.txt).
GENERAL, DEATH, OBSERVER = 500, 900, 1000
YEAR_END_KIND, BIRTHDAY_KIND, DEATH_KIND = 1, 2, 3
WORLD_SUBJECT, SIM_SUBJECT = 0, 1
HOMOZYGOUS = genotype(DUMMY, DUMMY)
APPLIES_TO_INDEX = {"everyone": 0, "dummy_homozygous": 1}


def some(data: bytes) -> bytes:
    return b"\x01" + data


def hash64(data: bytes) -> int:
    return xxhash.xxh3_64_intdigest(data, seed=0)


def needed_seconds(seed: int, key: int, code: str, per_year: float):
    """How long the cause must act until the person with this lineage key dies of it."""
    word = words(chacha_key(seed, DEATH_BUDGET, key, str_key(code), 0))[0]
    _, budget = exponential_reference(word, 1.0)
    years = budget / per_year
    if years >= NEVER_YEARS:
        return None
    return math.ceil(years * 31_449_600.0)


class Event:
    def __init__(self, time, cls, subject, kind, insertion, payload, period, plan):
        self.time = time
        self.cls = cls
        self.subject = subject
        self.kind = kind
        self.insertion = insertion
        self.payload = payload
        self.period = period
        self.plan = plan

    def key(self):
        phase = 1 if self.cls >= OBSERVER else 0
        return (self.time, phase, 0, self.cls, self.subject[0], self.subject[1], self.kind, self.insertion)

    def queue_bytes(self) -> bytes:
        """(time, micro, seq, payload, period), ADR-0002."""
        period = NONE if self.period is None else some(signed(self.period))
        return signed(self.time) + varint(0) + varint(self.insertion) + self.payload + period


class World:
    """A started world. `causes` is a list of (code, hazard per year) for everyone or
    (code, hazard per year, "dummy_homozygous"), `founders` a list of (born, sex,
    genotype) with the last two as postcard bytes, `couples` a list of (woman, man) by
    their places among the founders."""

    def __init__(self, seed: int, tag: str, diseases: bool, causes: list, founders: list, couples=()):
        self.seed = seed
        self.tag = tag
        self.diseases = diseases
        self.causes = [cause if len(cause) == 3 else (*cause, "everyone") for cause in causes]
        self.closed = -1
        self.queue = []
        self.next_insertion = 1
        self.next_event_id = 1
        self.next_sim_id = 1
        self.next_partnership_id = 1
        self.partnerships = {}
        self.sims = {}
        self.archive = {}
        self.log = []
        self.year_hashes = []
        start = variant(
            0,
            varint(seed),
            string(tag),
            world_params(diseases),
            seq([death_cause(*cause) for cause in self.causes]),
            seq([founder(born, sex, genes) for born, sex, genes in founders]),
            seq([couple(woman, man) for woman, man in couples]),
        )
        self.inputs = [signed(-1) + varint(1) + start]
        self._start(founders)
        for woman, man in couples:
            self._partner(woman + 1, man + 1, variant(2, varint(1)), 0)

    def _partner(self, woman: int, man: int, cause: bytes, now: int) -> None:
        number = self.next_partnership_id
        self.next_partnership_id += 1
        subjects = [variant(0, varint(woman)), variant(0, varint(man))]
        self._entry(now, subjects, [cause], variant(5, varint(number)))
        self.partnerships[number] = [woman, man, now, None]

    def arrange(self, woman: int, man: int) -> None:
        """`ArrangePartnership` of two living sims, submitted right now."""
        number = len(self.inputs) + 1
        self.inputs.append(signed(self.closed) + varint(number) + variant(2, varint(woman), varint(man)))
        self._partner(woman, man, variant(2, varint(number)), self.closed + 1)

    # ------------------------------------------------------------------ inputs

    def _start(self, founders: list) -> None:
        now = 0
        by_input = variant(2, varint(1))
        started = self._entry(now, [], [by_input], variant(0, varint(len(founders))))
        self._schedule(YEAR - 1, OBSERVER, (WORLD_SUBJECT, 0), YEAR_END_KIND, variant(0), YEAR, started, now)
        for index, (born, sex, genes) in enumerate(founders):
            sim = self.next_sim_id
            self.next_sim_id += 1
            key = founder_key(self.tag, index)
            subject = variant(0, varint(sim))
            added = self._entry(now, [subject], [by_input], variant(1, signed(born), lineage_key(key)))
            # The exposure of a homozygous founder: nothing closed, on from the start if
            # the switch is on.
            sick = [0, now if self.diseases else None] if genes == HOMOZYGOUS else None
            self.sims[sim] = (born, key, sex, genes, sick)
            whole = (now - born) // YEAR
            on_the_day = born + whole * YEAR == now
            age = whole if on_the_day and whole >= 1 else whole + 1
            self._plan_birthday(sim, born + age * YEAR, added, now)
            self._plan_deaths(sim, 0, added, now)

    def due(self, sim: int, cause: tuple):
        """When the sim dies of the cause if nothing else happens first, or None."""
        code, rate, applies_to = cause
        _, key, _, _, sick = self.sims[sim]
        needed = needed_seconds(self.seed, key, code, rate)
        if needed is None:
            return None
        if applies_to == "everyone":
            due = 0 + needed  # a founder: from the start on
        elif sick is None or sick[1] is None:
            return None
        else:
            before, since = sick
            assert before <= needed
            due = since + needed - before
        return due if abs(due) <= MAX_ABS_SECS else None

    def _plan_deaths(self, sim: int, first: int, by: int, now: int) -> None:
        for cause in self.causes[first:]:
            due = self.due(sim, cause)
            if due is None:
                continue
            payload = variant(2, varint(sim), varint(str_key(cause[0])))
            self._schedule(due, DEATH, (SIM_SUBJECT, sim), DEATH_KIND, payload, None, by, now)

    def override(self, diseases: bool) -> None:
        """An override block with the one change, submitted right now."""
        number = len(self.inputs) + 1
        self.inputs.append(signed(self.closed) + varint(number) + variant(1, seq([param_change(diseases)])))
        now = self.closed + 1
        flips = diseases != self.diseases
        self.diseases = diseases
        changed = self._entry(now, [], [variant(2, varint(number))], variant(3, param_change(diseases)))
        if not flips:
            return
        first = next(
            (index for index, cause in enumerate(self.causes) if cause[2] == "dummy_homozygous"), None
        )
        for sim in sorted(self.sims):
            sick = self.sims[sim][4]
            if sick is None:
                continue
            if diseases:
                sick[1] = now
            else:
                sick[0] += now - sick[1]
                sick[1] = None
            if first is None:
                continue
            later = {variant(2, varint(sim), varint(str_key(cause[0]))) for cause in self.causes[first:]}
            self.queue = [event for event in self.queue if event.payload not in later]
            self._plan_deaths(sim, first, changed, now)

    # ------------------------------------------------------------------ events

    def _entry(self, time: int, subjects: list, causes: list, payload: bytes) -> int:
        event_id = self.next_event_id
        self.next_event_id += 1
        self.log.append(varint(event_id) + signed(time) + seq(subjects) + seq(causes) + payload)
        return event_id

    def _schedule(self, time, cls, subject, kind, payload, period, by, planned_at) -> None:
        insertion = self.next_insertion
        self.next_insertion += 1
        self.queue.append(Event(time, cls, subject, kind, insertion, payload, period, (by, planned_at)))

    def _plan_birthday(self, sim: int, time: int, by: int, now: int) -> None:
        self._schedule(time, GENERAL, (SIM_SUBJECT, sim), BIRTHDAY_KIND, variant(1, varint(sim)), None, by, now)

    def advance(self, limit: int) -> None:
        """Runs every event up to and including `limit`."""
        while True:
            due = [event for event in self.queue if event.time <= limit]
            if not due:
                break
            event = min(due, key=Event.key)
            self.queue.remove(event)
            now = event.time
            plan = variant(1, varint(event.plan[0]), signed(event.plan[1]))
            if event.kind == YEAR_END_KIND:
                # The next occurrence is in the queue before the observer looks.
                event.time += event.period
                self.queue.append(event)
                self.year_hashes.append(hash64(self.projection(now - 1)))
            elif event.kind == BIRTHDAY_KIND:
                sim = event.subject[1]
                age = (now - self.sims[sim][0]) // YEAR
                subject = variant(0, varint(sim))
                entry = self._entry(now, [subject], [plan], variant(2, varint(age)))
                self._plan_birthday(sim, now + YEAR, entry, now)
            else:
                self._die(event, now, plan)
        self.closed = limit

    def _die(self, event: Event, now: int, plan: bytes) -> None:
        sim = event.subject[1]
        born, key, sex, genes, sick = self.sims.pop(sim)
        code = next(
            code for code, _, _ in self.causes if event.payload == variant(2, varint(sim), varint(str_key(code)))
        )
        draw = variant(4, varint(DEATH_BUDGET), varint(key), varint(str_key(code)), varint(0))
        self._entry(now, [variant(0, varint(sim))], [plan, draw], variant(4, string(code)))
        self.queue = [other for other in self.queue if other.subject != (SIM_SUBJECT, sim)]
        for couple_ in self.partnerships.values():
            if couple_[3] is None and sim in couple_[:2]:
                couple_[3] = now
        # The exposure stays as it was: whether the disease acted at the death.
        self.archive[sim] = (born, now, str_key(code), key, sex, genes, sick)

    # ------------------------------------------------------------------ views

    def ordered_queue(self) -> list:
        return sorted(self.queue, key=Event.key)

    def projection(self, closed=None) -> bytes:
        closed = self.closed if closed is None else closed
        sims = [
            varint(sim) + signed(born) + lineage_key(key) + sex + genes + exposure(sick)
            for sim, (born, key, sex, genes, sick) in sorted(self.sims.items())
        ]
        archive = [
            varint(sim)
            + signed(born)
            + signed(left)
            + variant(0, varint(cause))
            + lineage_key(key)
            + sex
            + genes
            + exposure(sick)
            for sim, (born, left, cause, key, sex, genes, sick) in sorted(self.archive.items())
        ]
        return (
            varint(PROJECTION_VERSION)
            + some(varint(self.seed))
            + some(world_params(self.diseases))
            + seq(
                [
                    varint(str_key(code)) + f64(rate) + variant(APPLIES_TO_INDEX[applies_to])
                    for code, rate, applies_to in self.causes
                ]
            )
            + signed(closed)
            + varint(self.next_sim_id)
            + varint(self.next_partnership_id)
            + seq([event.queue_bytes() for event in self.ordered_queue()])
            + varint(self.next_insertion)
            + seq(sims)
            + seq([varint(number) + partnership(*value) for number, value in sorted(self.partnerships.items())])
            + seq(archive)
        )

    def state_hash(self) -> int:
        return hash64(self.projection())

    def log_hash(self) -> int:
        return hash64(b"".join(self.log))

    def save(self) -> bytes:
        """Header, projection, exceptions (ADR-0006 section 2)."""
        plans = [varint(event.plan[0]) + signed(event.plan[1]) for event in self.ordered_queue()]
        years = [signed(year) + varint(value) for year, value in enumerate(self.year_hashes)]
        return (
            header(EMPTY_DATA_HASH)
            + self.projection()
            + seq(plans)
            + varint(self.next_event_id)
            + seq(self.inputs)
            + seq(years)
        )


def header(data_hash: int) -> bytes:
    """magic, format_version, core_version, data_hash"""
    return b"SIMAGENT" + varint(FORMAT_VERSION) + varint(CORE_VERSION) + varint(data_hash)


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

#!/usr/bin/env python3
"""Reference for the upbringing table of growing up (ADR-0033 sections 4, 5 and 9): the rows of
a talk (praise, tease, apology, the talk itself), of coming home from school, of reading or
playing to the end by one's own choice, of a need at 0 with a parent at home, and of a child
under 6 overhearing a parent praise, tease or apologize to another adult in its room.
Written from the ADR alone; the core's tests take their expected values from here.

What it implements:

- The table file `data/social/development.json` and its checks, and the postcard bytes the
  data hash appends for it (after the social data).
- Who is a parent of a child, who an older sibling (section 5), from the state of the moment.
- The steps of one occasion for each child it reaches, in the order the core writes
  `core:developed`: the children in the order of the occasion's subjects, per child the axes
  in the order openness, conscientiousness, extraversion, agreeableness, neuroticism, per axis
  the rows in table order, each with the age factor and the saturation of section 4.

READING (section 9, "je Aenderung von D"): a step that changes nothing (0 after the age factor,
or D already at its bound) writes no `core:developed`.

READING (section 5, "mit einem Elternteil im Haus"): a parent (as `parent_of`) whose place in
the house is not away (at work or school); a walk inside the house counts as at home.

READING (section 5, overhearing): the half of each row of the topic, step / 2 toward zero, with
age factor 1 (the child is under 6, where the factor is 1 anyway), saturated; the children in
the order of their ids; the source is the actor. The table has no row for it; its code exists
for the log only, and a row naming it is refused.

READING (section 5, "einmal je Beduerfnis und Tag"): the calendar day of the second is stored
per child and need when the row applies (the child develops and a parent is at home), whether or
not D changed; a second need at 0 the same day counts again, the same need does not.

Usage: ~/.cache/simagent-tools-venv/bin/python upbringing_reference.py   (from tools/)
"""

import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Optional, Tuple

from data_reference import varint, zigzag
from development_reference import YEAR, age_factor_step, div0, saturate

REPOSITORY_TABLE = Path(__file__).resolve().parent.parent / "data" / "social" / "development.json"

AXES = ("openness", "conscientiousness", "extraversion", "agreeableness", "neuroticism")
# The occasions of the table in the order of the core's enum (its postcard index).
OCCASIONS = ("praise", "tease", "apology", "school", "talk", "done", "need_empty", "overheard")
ROWS_MAX = 32
STEP_BOUND = 1000

# The start values of section 5, in table order (the overheard talk has no row of its own).
ADR_ROWS = [
    {"occasion": "praise", "neuroticism": -40, "agreeableness": 20},
    {"occasion": "tease", "neuroticism": 40, "agreeableness": -20},
    {"occasion": "apology", "neuroticism": -20, "agreeableness": 20},
    {"occasion": "done", "conscientiousness": 10},
    {"occasion": "school", "conscientiousness": 10},
    {"occasion": "talk", "extraversion": 10},
    {"occasion": "done", "openness": 10},
    {"occasion": "need_empty", "neuroticism": 10},
]

# The offers whose end counts as finished reading or play.
DONE_OFFERS = ("read", "play")

# A child takes part in a talk row only from 6 (section 5, "das Kind ab 6").
TALK_MIN_AGE = 6 * YEAR


def table_checked(doc) -> List[Dict[str, int]]:
    """The rows of a table file: one field `rows`, each row an `occasion` of OCCASIONS and steps
    of -1000 to 1000 per axis (an absent axis is 0), at most 32 rows (failures `development
    fields` and `development.rows`). Returns each row with all five axes."""
    assert isinstance(doc, dict) and set(doc) == {"rows"}, "development fields"
    rows = doc["rows"]
    assert isinstance(rows, list), "development fields"
    out = []
    for row in rows:
        assert isinstance(row, dict) and "occasion" in row and set(row) <= {"occasion", *AXES}, "development fields"
        assert row["occasion"] in OCCASIONS, "development fields"
        assert row["occasion"] != "overheard", "development.rows"
        steps = {axis: row.get(axis, 0) for axis in AXES}
        assert all(isinstance(v, int) and not isinstance(v, bool) and -(2**15) <= v < 2**15 for v in steps.values()), "development fields"
        out.append({"occasion": row["occasion"], **steps})
    assert len(out) <= ROWS_MAX, "development.rows"
    assert all(-STEP_BOUND <= row[axis] <= STEP_BOUND for row in out for axis in AXES), "development.rows"
    return out


def development_suffix(doc) -> bytes:
    """The postcard that the data hash appends after the social data:
    DevelopmentTable { rows: [Row { occasion: Upbringing (index in OCCASIONS), openness,
    conscientiousness, extraversion, agreeableness, neuroticism: i16 }] }. Empty without a file."""
    if doc is None:
        return b""
    rows = table_checked(doc)
    out = varint(len(rows))
    for row in rows:
        out += varint(OCCASIONS.index(row["occasion"]))
        for axis in AXES:
            out += zigzag(row[axis])
    return out


# ---------------------------------------------------------------------------------------
# The people of the moment


@dataclass
class Person:
    born: int
    household: int
    mother: Optional[int] = None
    father: Optional[int] = None
    social_parents: Tuple[int, ...] = ()
    raised_since: Optional[int] = None
    d: List[int] = field(default_factory=lambda: [0, 0, 0, 0, 0])
    away: bool = False
    # The room a child under 6 is in (its day room, its night room) or None (kindergarten).
    room: Optional[int] = None
    # Per need (hunger, energy, bladder, hygiene, fun) the calendar day last counted.
    counted: List[Optional[int]] = field(default_factory=lambda: [None] * 5)


def parent_of(people: Dict[int, Person], p: int, k: int) -> bool:
    """`p` is a parent of `k`: living (in `people`), in the same household, and the mother or
    father from the birth or one of `k`'s social parents."""
    if p not in people or p == k:
        return False
    child = people[k]
    return people[p].household == child.household and (
        p in (child.mother, child.father) or p in child.social_parents
    )


def parents(people: Dict[int, Person], k: int) -> set:
    return {p for p in people if parent_of(people, p, k)}


def older_sibling_of(people: Dict[int, Person], s: int, k: int, now: int) -> bool:
    """`s` is an older sibling of `k`: another living person under 18 in the same household
    who shares at least one parent (in the sense of `parent_of`) with `k`, born strictly
    earlier."""
    if s not in people or s == k:
        return False
    sib, child = people[s], people[k]
    young = now - sib.born < 18 * YEAR
    shared = parents(people, s) & parents(people, k)
    return young and sib.household == child.household and bool(shared) and sib.born < child.born


def develops(people: Dict[int, Person], k: int, now: int) -> bool:
    """D of `k` changes at `now`: raised, under 18 (the household lives in a house in every
    occasion of this part)."""
    person = people.get(k)
    return person is not None and person.raised_since is not None and now - person.born < 18 * YEAR


@dataclass(frozen=True)
class Developed:
    child: int
    source: int
    axis: str
    before: int
    after: int
    step: int
    occasion: str


def apply(people, table, k: int, reached: List[Tuple[str, int]], now: int) -> List[Developed]:
    """The steps of the occasions `reached` (occasion, source) for child `k`: per axis in order,
    the rows in table order whose occasion `k` reached, each age-scaled and saturated."""
    if not develops(people, k, now):
        return []
    age = now - people[k].born
    out = []
    for index, axis in enumerate(AXES):
        for row in table:
            for occasion, source in reached:
                if row["occasion"] != occasion or row[axis] == 0:
                    continue
                step = age_factor_step(row[axis], age)
                before = people[k].d[index]
                after = saturate(before, step)
                if after != before:
                    people[k].d[index] = after
                    out.append(Developed(k, source, axis, before, after, step, occasion))
    return out


def talk(people, table, a: int, b: int, topic: str, actor: Optional[int], now: int) -> List[Developed]:
    """A talk of `a` and `b` (topic one of everyday, praise, tease, gossip, recall, apology;
    `actor` the one who praises, teases or apologizes). Children in the order a, b."""
    out = []
    for k, other in ((a, b), (b, a)):
        reached = []
        if topic in ("praise", "tease", "apology") and actor == other:
            parent = parent_of(people, other, k)
            elder = older_sibling_of(people, other, k, now)
            if parent or (topic != "praise" and elder):
                reached.append((topic, other))
        if now - people[k].born >= TALK_MIN_AGE:
            reached.append(("talk", k))
        out += apply(people, table, k, reached, now)
    return out


def came_home(people, table, k: int, place: str, now: int) -> List[Developed]:
    """`k` came home from `place` (school or work)."""
    return apply(people, table, k, [("school", k)] if place == "school" else [], now)


NEEDS = ("hunger", "energy", "bladder", "hygiene", "fun")

YOUNG = 6 * YEAR


def overheard(people, table, a: int, b: int, topic: str, actor: Optional[int], room: int, now: int) -> List[Developed]:
    """A talk of `a` and `b`, both 18 or older, at furniture in `room`: each child under 6 in
    `room` whose parent is the actor of praise, tease or apology gets half of that topic's rows."""
    adults = all(now - people[x].born >= 18 * YEAR for x in (a, b))
    if not adults or topic not in ("praise", "tease", "apology") or actor not in (a, b):
        return []
    out = []
    for k in sorted(people):
        child = people[k]
        young = now - child.born < YOUNG
        if not (young and child.room == room and parent_of(people, actor, k) and develops(people, k, now)):
            continue
        for index, axis in enumerate(AXES):
            for row in table:
                if row["occasion"] != topic or row[axis] == 0:
                    continue
                step = div0(row[axis], 2)
                before = child.d[index]
                after = saturate(before, step)
                if after != before:
                    child.d[index] = after
                    out.append(Developed(k, actor, axis, before, after, step, "overheard"))
    return out


def finished(people, table, k: int, offer: str, reason: str, commanded: bool, now: int) -> List[Developed]:
    """`k` ended `offer` for `reason` (done, full, left, ...); `commanded` if a command chose it.
    Reading or playing to its end by its own choice reaches the done rows."""
    own = offer in DONE_OFFERS and reason == "done" and not commanded
    return apply(people, table, k, [("done", k)] if own else [], now)


def need_empty(people, table, k: int, need: str, now: int) -> List[Developed]:
    """`need` of `k` reached 0 at `now`: once per need and calendar day, with a parent at home."""
    if not develops(people, k, now):
        return []
    home = any(parent_of(people, p, k) and not people[p].away for p in people)
    day = now // 86_400
    index = NEEDS.index(need)
    if not home or people[k].counted[index] == day:
        return []
    people[k].counted[index] = day
    return apply(people, table, k, [("need_empty", k)], now)


# ---------------------------------------------------------------------------------------


def family(now: int) -> Dict[int, Person]:
    """Mother 1 and father 2 (adults, not raised), daughter 3 (8 years), son 4 (14 years),
    twins 5 and 6 (10 years, the same second), all raised; stepdaughter 7 (9 years) of the father
    from an earlier partnership, the mother her social parent; a neighbour 8 in household 2;
    a girl 9 (16 years), daughter of the mother."""
    born = lambda years: now - years * YEAR  # noqa: E731
    return {
        1: Person(born(38), 1),
        2: Person(born(40), 1),
        3: Person(born(8), 1, 1, 2, raised_since=born(8)),
        4: Person(born(14), 1, 1, 2, raised_since=born(10)),
        5: Person(born(10), 1, 1, 2, raised_since=born(10)),
        6: Person(born(10), 1, 1, 2, raised_since=born(10)),
        7: Person(born(9), 1, 20, 2, social_parents=(1,), raised_since=born(9)),
        8: Person(born(30), 2),
        9: Person(born(16), 1, 1, 21, raised_since=born(12)),
    }


def main() -> None:
    table = table_checked({"rows": ADR_ROWS})
    assert table_checked(json.loads(REPOSITORY_TABLE.read_text())) == table, "file is the ADR table"
    suffix = development_suffix(json.loads(REPOSITORY_TABLE.read_text()))
    # 8 rows; praise: index 0, O 0, C 0, E 0, A 20 -> zigzag 40 = 0x28, N -40 -> zigzag 79 = 0x4f;
    # the last, need_empty: index 6, N +10 -> zigzag 20 = 0x14.
    assert suffix[:7] == bytes([8, 0, 0, 0, 0, 0x28, 0x4F]), suffix.hex()
    assert suffix[-6:] == bytes([6, 0, 0, 0, 0, 0x14]), suffix.hex()
    print("development_suffix", suffix.hex())

    now = 100 * YEAR
    people = family(now)
    # Mother praises the daughter of 8. The mother (adult, not raised) gets nothing. The daughter:
    # E +10 (talk), A +20 and N -40 (praise), age factor 1: 0 -> 10, 0 -> 20, 0 -> -40.
    got = talk(people, table, 1, 3, "praise", 1, now)
    assert got == [
        Developed(3, 3, "extraversion", 0, 10, 10, "talk"),
        Developed(3, 1, "agreeableness", 0, 20, 20, "praise"),
        Developed(3, 1, "neuroticism", 0, -40, -40, "praise"),
    ], got
    # The son of 14 teases the daughter: an older sibling (shares both parents, born earlier).
    # Son first (a): talk, age factor 1/2: E 10 * 2 / 4 = 5. Daughter: E 10 -> 10 + 10 * 2990 /
    # 3000 = 10 + 9 = 19; A 20 + (-20 * 3020) / 3000 = 20 - 20 = 0; N -40 + 40 * 3040 / 3000 =
    # -40 + 40 = 0.
    got = talk(people, table, 4, 3, "tease", 4, now)
    assert got == [
        Developed(4, 4, "extraversion", 0, 5, 5, "talk"),
        Developed(3, 3, "extraversion", 10, 19, 10, "talk"),
        Developed(3, 4, "agreeableness", 20, 0, -20, "tease"),
        Developed(3, 4, "neuroticism", -40, 0, 40, "tease"),
    ], got
    # The daughter praises the son: she is no parent; only the talk rows. Son E 5 + 5 * 2995 /
    # 3000 = 5 + 4 = 9; daughter E 19 + 10 * 2981 / 3000 = 19 + 9 = 28.
    got = talk(people, table, 3, 4, "praise", 3, now)
    assert [(x.child, x.after, x.occasion) for x in got] == [(3, 28, "talk"), (4, 9, "talk")], got
    # Twins born in the same second are not older for each other: a tease gives only the talk.
    got = talk(people, table, 5, 6, "tease", 5, now)
    assert [(x.child, x.axis, x.occasion) for x in got] == [(5, "extraversion", "talk"), (6, "extraversion", "talk")], got
    # The stepdaughter of 9 shares the father with the son: he is her older sibling; the mother is
    # her social parent: her apology counts. Son teases her: A -20, N +40 (and both talk rows).
    got = talk(people, table, 7, 4, "tease", 4, now)
    assert [(x.child, x.axis, x.after) for x in got] == [
        (7, "extraversion", 10), (7, "agreeableness", -20), (7, "neuroticism", 40), (4, "extraversion", 13),
    ], got
    got = talk(people, table, 1, 7, "apology", 1, now)
    # A -20 + 20 * 3020 / 3000 = -20 + 20 = 0; N 40 + (-20 * 3040) / 3000 = 40 - 20 = 20.
    assert [(x.axis, x.before, x.after) for x in got] == [
        ("extraversion", 10, 19), ("agreeableness", -20, 0), ("neuroticism", 40, 20),
    ], got
    # The neighbour praises the daughter: no parent in her household; the neighbour is an adult.
    got = talk(people, table, 8, 3, "praise", 8, now)
    assert [(x.child, x.occasion) for x in got] == [(3, "talk")], got
    # The girl of 16, apology from the mother: age factor 1/4. E 10 / 4 = 2 (2.5 toward 0);
    # A 20 / 4 = 5; N -20 / 4 = -5.
    got = talk(people, table, 9, 1, "apology", 1, now)
    assert [(x.axis, x.step, x.after) for x in got] == [
        ("extraversion", 2, 2), ("agreeableness", 5, 5), ("neuroticism", -5, -5),
    ], got
    # Gossip between the girl of 16 and the son: the son is no older sibling of hers (born
    # later); topic without an actor: only talk rows. Girl E 2 + 2 * 2998 / 3000 = 2 + 1 = 3.
    got = talk(people, table, 9, 4, "gossip", None, now)
    assert [(x.child, x.after) for x in got] == [(9, 3), (4, 17)], got
    # Saturation: N at 2990, a tease +40 at 8: 2990 + 40 * 10 / 3000 = 2990 + 0: no entry.
    people[3].d[4] = 2990
    got = [x for x in talk(people, table, 4, 3, "tease", 4, now) if x.axis == "neuroticism"]
    assert got == [], got
    # School: the son of 14 comes home from school: C 10 * 2 / 4 = 5. From work: nothing.
    assert came_home(people, table, 4, "school", now) == [Developed(4, 4, "conscientiousness", 0, 5, 5, "school")]
    assert came_home(people, table, 4, "work", now) == []
    # A child never raised, or one who turned 18, changes nothing.
    people[10] = Person(now - 8 * YEAR, 1, 1, 2)
    assert came_home(people, table, 10, "school", now) == []
    people[11] = Person(now - 18 * YEAR, 1, 1, 2, raised_since=now - 18 * YEAR)
    assert came_home(people, table, 11, "school", now) == []
    # One second before 18: age factor 1/4, C 10 / 4 = 2.
    people[12] = Person(now - 18 * YEAR + 1, 1, 1, 2, raised_since=now - 18 * YEAR + 1)
    assert came_home(people, table, 12, "school", now)[0].after == 2
    # An empty table changes nothing.
    assert talk(family(now), [], 1, 3, "praise", 1, now) == []

    # Reading to the end by her own choice (the daughter, 8): the done rows, per axis: O +10
    # (row 7), then C +10 (row 4). Commanded, ended full or by leaving, or watching TV: nothing.
    people = family(now)
    got = finished(people, table, 3, "read", "done", False, now)
    assert got == [
        Developed(3, 3, "openness", 0, 10, 10, "done"),
        Developed(3, 3, "conscientiousness", 0, 10, 10, "done"),
    ], got
    assert finished(people, table, 3, "play", "done", True, now) == []
    assert finished(people, table, 3, "play", "full", False, now) == []
    assert finished(people, table, 3, "play", "left", False, now) == []
    assert finished(people, table, 3, "watch", "done", False, now) == []
    # Playing at 14: half. O 10 * 2 / 4 = 5, C 5; then the daughter plays: O 10 + 10 * 2990 /
    # 3000 = 19, C 19.
    assert [(x.axis, x.after) for x in finished(people, table, 4, "play", "done", False, now)] == [
        ("openness", 5), ("conscientiousness", 5),
    ]
    assert [(x.axis, x.after) for x in finished(people, table, 3, "play", "done", False, now)] == [
        ("openness", 19), ("conscientiousness", 19),
    ]

    # A need at 0 (the daughter, 8, at second `now`, a day start): N +10 with her mother at home;
    # the same need again that day: nothing; another need: N 10 + 10 * 2990 / 3000 = 19; the
    # next day the first need again: 19 + 10 * 2981 / 3000 = 28.
    people = family(now)
    assert need_empty(people, table, 3, "hunger", now) == [Developed(3, 3, "neuroticism", 0, 10, 10, "need_empty")]
    assert need_empty(people, table, 3, "hunger", now + 3600) == []
    assert [x.after for x in need_empty(people, table, 3, "fun", now + 7200)] == [19]
    assert [x.after for x in need_empty(people, table, 3, "hunger", now + 86_400)] == [28]
    # Both parents away (at work): nothing, and the day is not used up: when the mother is back
    # the same day, the need counts.
    people = family(now)
    people[1].away = people[2].away = True
    assert need_empty(people, table, 3, "energy", now) == []
    people[1].away = False
    assert [x.after for x in need_empty(people, table, 3, "energy", now + 60)] == [10]
    # The stepdaughter's social parent at home counts; a neighbour at home does not.
    people = family(now)
    people[2].away = True
    assert [x.after for x in need_empty(people, table, 7, "bladder", now)] == [10]
    people[1].away = True
    assert need_empty(people, table, 7, "hygiene", now) == []
    # At 16 a quarter: 10 / 4 = 2.
    assert [x.step for x in need_empty(family(now), table, 9, "fun", now)] == [2]

    # Overhearing: a baby 10 (2 years) and a toddler 11 (5 years) of the mother and father, a
    # baby 12 (1 year) of the neighbour; the talk at furniture in room 2.
    people = family(now)
    people[10] = Person(now - 2 * YEAR, 1, 1, 2, raised_since=now - 2 * YEAR, room=2)
    people[11] = Person(now - 5 * YEAR, 1, 1, 2, raised_since=now - 5 * YEAR, room=2)
    people[13] = Person(now - 3 * YEAR, 1, 1, 2, raised_since=now - 3 * YEAR, room=None)
    # The father praises the mother: half of praise, A 20 / 2 = 10, N -40 / 2 = -20, for both in
    # room 2, in id order; the child at the kindergarten (room None) hears nothing.
    got = overheard(people, table, 2, 1, "praise", 2, 2, now)
    assert got == [
        Developed(10, 2, "agreeableness", 0, 10, 10, "overheard"),
        Developed(10, 2, "neuroticism", 0, -20, -20, "overheard"),
        Developed(11, 2, "agreeableness", 0, 10, 10, "overheard"),
        Developed(11, 2, "neuroticism", 0, -20, -20, "overheard"),
    ], got
    # A tease: A -20 / 2 = -10, so 10 + (-10 * 3010) / 3000 = 10 - 10 = 0; N 40 / 2 = 20:
    # -20 + 20 * 3020 / 3000 = -20 + 20 = 0. An apology: A 10, N -10.
    got = overheard(people, table, 1, 2, "tease", 1, 2, now)
    assert [(x.child, x.axis, x.after) for x in got] == [
        (10, "agreeableness", 0), (10, "neuroticism", 0), (11, "agreeableness", 0), (11, "neuroticism", 0),
    ], got
    got = overheard(people, table, 1, 2, "apology", 1, 2, now)
    assert [(x.child, x.step) for x in got] == [(10, 10), (10, -10), (11, 10), (11, -10)], got
    # Another room, an everyday topic, no actor, or a talk with a child of 16 (the girl 9, not
    # an adult): nothing.
    assert overheard(people, table, 1, 2, "praise", 1, 3, now) == []
    assert overheard(people, table, 1, 2, "everyday", None, 2, now) == []
    assert overheard(people, table, 1, 9, "apology", 1, 2, now) == []
    # The neighbour praises the mother: not the parent of the small children.
    assert overheard(people, table, 8, 1, "praise", 8, 2, now) == []
    # A row of its own is refused.
    try:
        table_checked({"rows": [{"occasion": "overheard", "openness": 1}]})
        raise SystemExit("an overheard row was kept")
    except AssertionError as error:
        assert str(error) == "development.rows"
    print("upbringing: all worked values hold")


if __name__ == "__main__":
    main()

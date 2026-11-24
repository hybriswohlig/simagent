#!/usr/bin/env python3
"""Reference for the upbringing table of growing up (ADR-0033 sections 4, 5 and 9), first part:
the rows of a talk (praise, tease, apology, the talk itself) and of coming home from school.
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

Usage: ~/.cache/simagent-tools-venv/bin/python upbringing_reference.py   (from tools/)
"""

import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Optional, Tuple

from data_reference import varint, zigzag
from development_reference import YEAR, age_factor_step, saturate

REPOSITORY_TABLE = Path(__file__).resolve().parent.parent / "data" / "social" / "development.json"

AXES = ("openness", "conscientiousness", "extraversion", "agreeableness", "neuroticism")
# The occasions of the table in the order of the core's enum (its postcard index).
OCCASIONS = ("praise", "tease", "apology", "school", "talk")
ROWS_MAX = 32
STEP_BOUND = 1000

# The start values of section 5 for the occasions of this part, in table order.
ADR_ROWS = [
    {"occasion": "praise", "neuroticism": -40, "agreeableness": 20},
    {"occasion": "tease", "neuroticism": 40, "agreeableness": -20},
    {"occasion": "apology", "neuroticism": -20, "agreeableness": 20},
    {"occasion": "school", "conscientiousness": 10},
    {"occasion": "talk", "extraversion": 10},
]

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
    # 5 rows; praise: index 0, O 0, C 0, E 0, A 20 -> zigzag 40 = 0x28, N -40 -> zigzag 79 = 0x4f.
    assert suffix[:7] == bytes([5, 0, 0, 0, 0, 0x28, 0x4F]), suffix.hex()
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
    print("upbringing: all worked values hold")


if __name__ == "__main__":
    main()

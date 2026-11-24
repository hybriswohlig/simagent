#!/usr/bin/env python3
"""Reference for the practice of skills in growing up (ADR-0033 sections 6, 9 and 10): who
practises language, reading or play on which occasion, the effective seconds it adds and the
`core:practiced` milestones it crosses. Written from the ADR alone; the core's tests take their
expected values from here. The pure formulas (window, diligence, effective seconds, level,
milestones) are those of `development_reference.py`; this file adds the occasions.

What it implements:

- Language: a talk of two people of 18 or more at furniture in a room; each child under 18 of
  the talk's household in that room, other than the two, hears it once: `talk_delay_seconds`
  (600) raw seconds of language.
- Reading: an action at the bookshelf that ends for any reason but a command and was not begun
  by one, with the seconds it ran at the bookshelf (from its arrival there, not the walk); and
  3600 raw seconds per return from school.
- Play: the same for an action at the toy chest.
- Per occasion and person: effective = floor((raw * window) * f), f from conscientiousness at
  the second; the practice seconds grow by it; each crossed boundary of 10 effective hours
  writes one `core:practiced {skill, hours, level_milli}`, ascending, the level at the boundary
  with the talent of that skill.

READING (section 6, "Zustand je Person"): practice lives with `raised_since`, so only a raised
person practises, and only at an occasion in a house (a talk there, a return home, an action's
end there); it practises at every age (the window factor outside the window applies to adults
too). A person never raised has no practice.

READING (section 6, "Abbruch durch ... Auszug"): leaving the house in the middle of reading or
play (the household moving out, or the person leaving it) ends the action with reason left; the
seconds it ran at the object count, whatever household the person belongs to afterwards.

READING (sections 6 and 9, the second the action runs from): the arrival at the object, which
writes down the needs its offer sets; the loader asks the stored second to be exactly that of the
first need the offer sets.

READING (section 6, "jedes Gespraech zweier Erwachsener im Raum des Kindes"): the hearers are
the living, raised persons under 18 of the household whose house it is, other than the two
talkers, in the room of the talk's furniture at that second: a person with needs by its room
(on a walk it is in no room), a small child (no needs) by its night room in bed, its day room
by day and no room at the kindergarten. Hearers in the order of their ids.

READING (sections 6 and 10, order): the practice of an occasion is written before its
`core:developed`, so the talent and the diligence are those before the occasion's steps of D.

READING (section 6, talent): the phenotype of the talent's trait at the age of 30 years (30 *
364 days) with the current D; the weight w at that age is its value at the 18th birthday.
Conscientiousness for f is the phenotype at the age of the second.

READING (section 6, an action ended on its way): it never ran at its object; 0 raw seconds, no
practice and no entry.

Usage: ~/.cache/simagent-tools-venv/bin/python practice_reference.py   (from tools/)
"""

from dataclasses import dataclass, field
from typing import Dict, List, Optional, Tuple

from development_reference import (
    MILESTONE_SECONDS,
    YEAR,
    diligence_factor,
    effective_seconds,
    level,
    level_milli,
    milestones,
    window_factor,
)

SKILLS = ("language", "reading", "play")
TALK_SECONDS = 600  # talk_delay_seconds of data/social/social.json
SCHOOL_SECONDS = 3600
KINDS = {"core:bookshelf": "reading", "core:toy_chest": "play"}
ADULT = 18 * YEAR


@dataclass
class Person:
    born: int
    household: int
    raised_since: Optional[int] = None
    # The room it is in now: a room number, or None (walking, away, kindergarten).
    room: Optional[int] = None
    practice: List[int] = field(default_factory=lambda: [0, 0, 0])
    # Talent per skill (language, reading, play) and conscientiousness now.
    talent: Tuple[float, float, float] = (0.0, 0.0, 0.0)
    z_c: float = 0.0


@dataclass(frozen=True)
class Practiced:
    sim: int
    skill: str
    hours: int
    level_milli: int


def practises(people: Dict[int, Person], k: int, housed: set) -> bool:
    """Living and raised; `housed` the households in a house, for the occasions of talks and
    returns, which happen only there (an action's end in a house passes its household)."""
    person = people.get(k)
    return person is not None and person.raised_since is not None and person.household in housed


def practise(people, housed, k: int, skill: str, raw: int, now: int) -> List[Practiced]:
    """`k` practises `skill` for `raw` seconds at `now`."""
    if raw <= 0 or not practises(people, k, housed):
        return []
    person = people[k]
    index = SKILLS.index(skill)
    window = window_factor(skill, now - person.born)
    added = effective_seconds(raw, window, diligence_factor(person.z_c))
    before = person.practice[index]
    after = before + added
    person.practice[index] = after
    return [Practiced(k, skill, h, m) for h, m in milestones(person.talent[index], before, after)]


def talk(people, housed, a: int, b: int, home: int, room: int, now: int) -> List[Practiced]:
    """A talk of `a` and `b` at furniture in `room` of the house of household `home`."""
    adults = all(now - people[x].born >= ADULT for x in (a, b))
    if not adults:
        return []
    out = []
    for k in sorted(people):
        person = people[k]
        hears = (
            k not in (a, b)
            and person.household == home
            and now - person.born < ADULT
            and person.room == room
        )
        if hears:
            out += practise(people, housed, k, "language", TALK_SECONDS, now)
    return out


def came_home(people, housed, k: int, place: str, now: int) -> List[Practiced]:
    return practise(people, housed, k, "reading", SCHOOL_SECONDS, now) if place == "school" else []


def ended(people, housed, k: int, kind: str, reason: str, commanded: bool, reached: Optional[int], now: int) -> List[Practiced]:
    """`k` ended its action at furniture of `kind` for `reason`; `reached` the second it arrived
    there (None if it ended on the way)."""
    skill = KINDS.get(kind)
    if skill is None or commanded or reason == "command" or reached is None:
        return []
    return practise(people, housed, k, skill, now - reached, now)


def main() -> None:
    now = 100 * YEAR + 10 * 3600  # some weekday morning, far from the start of the clock
    born = lambda years: now - years * YEAR  # noqa: E731

    def family() -> Dict[int, Person]:
        return {
            1: Person(born(38), 1, room=2),
            2: Person(born(40), 1, room=2),
            3: Person(born(8), 1, born(8), room=2),  # in the living room
            4: Person(born(3), 1, born(3), room=2),  # small child, day room = living room here
            5: Person(born(10), 1, born(10), room=None),  # walking
            6: Person(born(12), 1, None, room=2),  # never raised
            7: Person(born(9), 2, born(9), room=2),  # another household
            8: Person(born(17), 1, born(10), room=3),  # other room
        }

    housed = {1, 2}

    # Language: hearers 3 (8 y: 600 * 1/4 = 150) and 4 (3 y: 600 in full); 5 walks, 6 never
    # raised, 7 of another household, 8 in another room. No milestone.
    people = family()
    assert talk(people, housed, 1, 2, 1, 2, now) == []
    assert [p.practice[0] for p in (people[3], people[4], people[5], people[6], people[7], people[8])] == [150, 600, 0, 0, 0, 0]
    # A talk of a child of 8 and an adult: no hearer.
    people = family()
    assert talk(people, housed, 3, 1, 1, 2, now) == [] and people[4].practice[0] == 0
    # A household that no longer lives in a house practises nothing.
    people = family()
    talk(people, set(), 1, 2, 1, 2, now)
    assert people[4].practice[0] == 0
    # Diligence: z_C = 2 -> f = 1.4: 600 * 1 * 1.4 = 840 for the small child.
    people = family()
    people[4].z_c = 2.0
    talk(people, housed, 1, 2, 1, 2, now)
    assert people[4].practice[0] == 840
    # A milestone: 35 900 s before, + 600 -> 36 500: crosses 10 h; talent 1 -> level 15.
    people = family()
    people[4].practice[0] = 35_900
    people[4].talent = (1.0, 0.0, 0.0)
    assert talk(people, housed, 1, 2, 1, 2, now) == [Practiced(4, "language", 10, 15)]
    print("language milestone:", Practiced(4, "language", 10, 15))

    # School: 3600 raw reading at 8 (window): 3600. At 14: 900.
    people = family()
    assert came_home(people, housed, 3, "school", now) == [] and people[3].practice[1] == 3600
    people[8].practice[1] = 0
    came_home(people, housed, 8, "school", now)
    assert people[8].practice[1] == 900
    assert came_home(people, housed, 3, "work", now) == [] and people[3].practice[1] == 3600

    # Reading at the bookshelf, arrived 1 h 30 min ago, any reason but a command: 5400 s at 8.
    for reason in ("done", "full", "left", "critical", "obligation", "collapse"):
        people = family()
        ended(people, housed, 3, "core:bookshelf", reason, False, now - 5400, now)
        assert people[3].practice[1] == 5400, reason
    # Begun or ended by a command: nothing. Ended on the way: nothing.
    people = family()
    ended(people, housed, 3, "core:bookshelf", "done", True, now - 5400, now)
    ended(people, housed, 3, "core:bookshelf", "command", False, now - 5400, now)
    ended(people, housed, 3, "core:bookshelf", "done", False, None, now)
    assert people[3].practice[1] == 0
    # Play at 8 (window): full; at 17: half. Another kind: nothing.
    people = family()
    ended(people, housed, 3, "core:toy_chest", "done", False, now - 3601, now)
    ended(people, housed, 8, "core:toy_chest", "done", False, now - 3601, now)
    ended(people, housed, 3, "core:sofa", "done", False, now - 3601, now)
    assert people[3].practice[2] == 3601 and people[8].practice[2] == 1800
    # Leaving in the middle (reason left) after 10 minutes at the bookshelf: 600 s at 8. The
    # household's house is the occasion's even if the person belongs elsewhere afterwards.
    people = family()
    ended(people, housed, 3, "core:bookshelf", "left", False, now - 600, now)
    assert people[3].practice[1] == 600
    # Two milestones in one long play (20 h at the chest, 8 y, z = 0): 10 h and 20 h,
    # L(0, 10) -> 9, L(0, 20) -> 19.
    people = family()
    out = ended(people, housed, 3, "core:toy_chest", "done", False, now - 20 * 3600, now)
    assert out == [Practiced(3, "play", 10, 9), Practiced(3, "play", 20, 19)]
    print("play milestones:", out)
    # Talent of play 0.5 at 25 h -> 30 h crossing 30 h: level_milli(L(0.5, 30)).
    people = family()
    people[3].practice[2] = 25 * 3600
    people[3].talent = (0.0, 0.0, 0.5)
    out = ended(people, housed, 3, "core:toy_chest", "done", False, now - 5 * 3600, now)
    want = level_milli(level(0.5, 30.0))
    assert out == [Practiced(3, "play", 30, want)]
    print("play milestone at talent 0.5, 30 h:", want)
    assert MILESTONE_SECONDS == 36_000
    print("ok")


if __name__ == "__main__":
    main()

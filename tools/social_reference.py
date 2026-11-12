#!/usr/bin/env python3
"""Reference for the social rules of phase 3 (memory, bonds, talks, beliefs, feelings), written
from ADR-0030 (sections 1 to 4), ADR-0031 (sections 1 to 6) and ADR-0032 (sections 1 to 5)
alone, without a look at the Rust code. The shared code is the independent draw reference tools/rng_reference.py
(chacha_key, words, uniform_below); everything else is plain integers, no float.

What it implements (section numbers refer to the ADRs):

- Memory (ADR-0030 1): strength at a second, the second of the next zero, recall,
  insertion with eviction (the newcomer counts), the keep-8 rule on move-out.
- Bonds (ADR-0030 2): start values by role, the analytic fade of familiarity and grudge,
  clamped steps, eviction of a bond.
- Talks (ADR-0030 3): planning and execution predicate, the same-second order, the six
  topics with conditions, actor rule and outcome steps, interests (TALK_INTEREST = 35),
  the topic choice.
- Choice bonus (ADR-0030 4): `liked` / `avoided`, company and avoid per furniture.
- Beliefs (ADR-0031 1 to 5): statement keys, heard certainty, confirmation, seen
  replacing heard, belief eviction, gossip material and speaker, the bond steps for heard
  deeds, the lie decision (TALK_LIE = 36), the refutation decision (TALK_REFUTE = 37) and
  its effects.
- Paternity at birth (ADR-0031 6): the four rules.
- Feelings and temper (ADR-0032 1 to 5): analytic channel fade and temper return,
  applying an occasion (band factor per row, rows summed, one clamp, temper from the
  clamped deltas, no write when nothing changes), the occasion table, contagion on
  entering a room, the temper-scaled company.

Times are whole seconds, SimIds and keys plain integers. Where the ADR text is open the
literal reading is taken and marked `READING` with the section in a comment.

This script is independent of the Rust code (ADR-0030 acceptance test): the core's tests
take their expected values from here.

Usage: ~/.cache/simagent-tools-venv/bin/python tools/social_reference.py --check
         (assert every worked value of the three ADRs and print the pinned values)
       ~/.cache/simagent-tools-venv/bin/python tools/social_reference.py
         (print the pinned values only)
"""

import sys
from dataclasses import dataclass, replace
from typing import Callable, Dict, List, Optional, Tuple

from rng_reference import chacha_key, founder_key, uniform_below, words

DAY = 86_400

# ---------------------------------------------------------------------------------------
# Limits and values of the ADRs (the data of data/social/social.json carries them)

MEMORY_CAP = 32  # ADR-0030 1
BOND_CAP = 32  # ADR-0030 2
KEEP_ON_MOVE_OUT = 8  # ADR-0030 1
BELIEF_CAP = 32  # ADR-0031 2
DEED_CAP = 64  # ADR-0031 1
DEED_WINDOW_DAYS = 30  # ADR-0031 1
TALK_DELAY = 600  # ADR-0030 3
PAIR_GAP = 3600  # ADR-0030 3
TALKS_PER_DAY = 8  # ADR-0030 3
ENTRY_CAP = 34  # ADR-0030 3 and 5
TALK_ENTRY_NEED = 16  # ADR-0030 3 and 5
TALK_MIN_AGE = 6  # ADR-0030 3

RECALL_STEP = 300
STRENGTH_MAX = 1000
FAMILIARITY_FADE = 5  # per day
GRUDGE_FADE = 10  # per day

COMPANY = 8_000  # ADR-0030 4 (start value)
AVOID = 8_000
COMPANY_MIN_SUM = 600
AVOID_BELOW_TRUST = 400

LIE_PPM = 50_000  # ADR-0031 4
REFUTE_PPM = 500_000  # ADR-0031 5
LIE_GRUDGE = 700
LIE_CERTAINTY = 900
HEARD_CAP = 900
CONFIRM_STEP = 50
CONFIRM_CAP = 900

# Draw tags (ADR-0030 3, ADR-0031 7)
TALK_INTEREST = 35
TALK_LIE = 36
TALK_REFUTE = 37

# Bond value indices (ADR-0030 2)
AFFECTION, TRUST, RESPECT, ATTRACTION, FAMILIARITY, GRUDGE = range(6)

# Start values by role, in the order of the first matching role (ADR-0030 2).
ROLE_ORDER = ("partner", "parent", "child", "sibling", "kin", "housemate")
START_VALUES = {
    "partner": (700, 700, 500, 500, 600, 0),
    "parent": (650, 650, 600, 0, 600, 0),  # the child's edge to the parent
    "child": (700, 600, 300, 0, 600, 0),  # the parent's edge to the child
    "sibling": (550, 550, 400, 0, 500, 0),
    "kin": (500, 500, 450, 0, 300, 0),  # other relatives from phi 1/8
    "housemate": (400, 450, 400, 0, 200, 0),  # also a first talk without an edge
}
# phi from 1/8 as integers: a coefficient num/den is kin when 8 * num >= den.
KIN_PHI = (1, 8)

# Topics (ADR-0030 3)
TOPIC_ALLTAG, TOPIC_LOB, TOPIC_NECKEN, TOPIC_KLATSCH, TOPIC_ERINNERN, TOPIC_ENTSCHULDIGUNG = range(6)
TOPIC_NAMES = ("Alltag", "Lob", "Necken", "Klatsch", "Erinnern", "Entschuldigung")
LOB_MIN_AFFECTION = 500
NECKEN_MAX_AGE = 60  # both under 60
ENTSCHULDIGUNG_MIN_GRUDGE = 300

# Deed kinds (ADR-0031 1), numbered in the order the ADR lists them. READING (ADR-0031 1):
# the numbers are mine; only the order of the keys depends on them.
NECKT, LAESTERT, LUEGT = 0, 1, 2

# Bases of a belief (ADR-0031 2)
BETEILIGT, GESEHEN, GEHOERT, GESCHLOSSEN, WIDERLEGT = (
    "beteiligt",
    "gesehen",
    "gehört",
    "geschlossen",
    "widerlegt",
)


def clamp(value: int, low: int = 0, high: int = 1000) -> int:
    return max(low, min(high, value))


def ceil_div(a: int, b: int) -> int:
    return -((-a) // b)


# ---------------------------------------------------------------------------------------
# Draws (ADR-0001 through rng_reference): one draw per key, word 0 of its key stream


def draw_word(seed: int, tag: int, subject: int, occasion: int, detail: int) -> int:
    return words(chacha_key(seed, tag, subject, occasion, detail))[0]


def interest(seed: int, lineage_key: int, topic: int) -> int:
    """ADR-0030 3: uniform_below(1001) of TALK_INTEREST, subject the lineage key,
    occasion the topic number, detail 0."""
    return uniform_below(draw_word(seed, TALK_INTEREST, lineage_key, topic, 0), 1001)


def lie_draw(seed: int, speaker_key: int, second: int, hearer_key: int) -> int:
    """ADR-0031 4: uniform_below(1_000_000) of TALK_LIE; subject the speaker's lineage
    key, occasion the second of the Talk, detail the hearer's lineage key."""
    return uniform_below(draw_word(seed, TALK_LIE, speaker_key, second, hearer_key), 1_000_000)


def refute_draw(seed: int, refuter_key: int, second: int, hearer_key: int) -> int:
    """ADR-0031 5: uniform_below(1_000_000) of TALK_REFUTE; subject the refuter's lineage
    key, occasion the second, detail the hearer's."""
    return uniform_below(draw_word(seed, TALK_REFUTE, refuter_key, second, hearer_key), 1_000_000)


def lies(u: int, lie_ppm: int = LIE_PPM) -> bool:
    return u < lie_ppm


def refutes(u: int, refute_ppm: int = REFUTE_PPM) -> bool:
    return u < refute_ppm


# ---------------------------------------------------------------------------------------
# Memory (ADR-0030 section 1)


def opt_key(x: Optional[int]) -> Tuple[int, int]:
    """Option<SimId> orders None before Some."""
    return (0, 0) if x is None else (1, x)


@dataclass(frozen=True)
class Memory:
    kind: int
    with_: Tuple[Optional[int], Optional[int]]  # the other participants, ascending
    at: int
    valence: int
    strength: int
    since: int
    fade: int  # loss per day

    @property
    def key(self) -> tuple:
        """The key (at, kind, with) of ADR-0030 1."""
        return (self.at, self.kind, opt_key(self.with_[0]), opt_key(self.with_[1]))


def strength_at(m: Memory, t: int) -> int:
    """max(0, strength - fade * (t - since) / 86400) in i64, / truncating toward zero
    (the product is not negative, so a floor)."""
    assert t >= m.since
    return max(0, m.strength - (m.fade * (t - m.since)) // DAY)


def zero_second(m: Memory) -> Optional[int]:
    """since + ceil(strength * 86400 / fade); none for fade 0 (never fades)."""
    if m.fade == 0:
        return None
    return m.since + ceil_div(m.strength * DAY, m.fade)


def next_forget(memories: List[Memory]) -> Optional[int]:
    """The second of the one open Forget event: the earliest zero of the list."""
    zeros = [z for z in (zero_second(m) for m in memories) if z is not None]
    return min(zeros) if zeros else None


def displacement_order(m: Memory, now: int) -> tuple:
    """The weakest goes first: strength at now, then the older `at`, then the smaller key.
    (The key starts with `at`, so the middle term only spells the rule out.)"""
    return (strength_at(m, now), m.at, m.key)


def insert_memory(
    memories: List[Memory], new: Memory, now: int, cap: int = MEMORY_CAP
) -> Tuple[List[Memory], Optional[Memory], bool]:
    """ADR-0030 1 'Neu': returns (list sorted by key, displaced memory or None, accepted).
    A full list displaces the weakest of the old ones and the newcomer together; if the
    newcomer is that one it is not taken in. READING (ADR-0030 1): a newcomer whose key
    is already in the list is an invariant break (the list has no double keys)."""
    assert new.since == now
    assert all(m.key != new.key for m in memories), "double memory key"
    if len(memories) < cap:
        return sorted(memories + [new], key=lambda m: m.key), None, True
    pool = memories + [new]
    victim = min(pool, key=lambda m: displacement_order(m, now))
    if victim is new:
        return list(memories), None, False
    kept = [m for m in memories if m is not victim] + [new]
    return sorted(kept, key=lambda m: m.key), victim, True


def recall_one(m: Memory, now: int) -> Memory:
    """Recall (ADR-0030 1 'Abruf'): min(1000, strength at now + 300), since = now, for
    each side's own copy."""
    return replace(m, strength=min(STRENGTH_MAX, strength_at(m, now) + RECALL_STEP), since=now)


def common_memory(
    a: int, mem_a: List[Memory], b: int, mem_b: List[Memory], now: int
) -> Optional[Tuple[Memory, Memory]]:
    """The strongest common memory of a and b (ADR-0030 1 'Abruf'): a's memory names b,
    b's names a, both with the same `at` and `kind` (ADR-0030 1: 'gleiche at und kind bei
    beiden, das Gegenüber in with'). The strongest is the one with the larger sum of both
    strengths at now, on a tie the younger `at`, then the smaller `kind` (the key starts
    with `at, kind`; the rest of the key only makes the order total)."""
    best = None
    for ma in mem_a:
        if b not in ma.with_:
            continue
        for mb in mem_b:
            if mb.at == ma.at and mb.kind == ma.kind and a in mb.with_:
                rank = (
                    -(strength_at(ma, now) + strength_at(mb, now)),
                    -ma.at,
                    ma.key,
                    mb.key,
                )
                if best is None or rank < best[0]:
                    best = (rank, ma, mb)
    return None if best is None else (best[1], best[2])


def recall_pair(
    a: int, mem_a: List[Memory], b: int, mem_b: List[Memory], now: int
) -> Optional[Tuple[List[Memory], List[Memory]]]:
    """Both copies of the common memory rise (topic Erinnern); None if there is none."""
    found = common_memory(a, mem_a, b, mem_b, now)
    if found is None:
        return None
    ma, mb = found
    return (
        [recall_one(m, now) if m is ma else m for m in mem_a],
        [recall_one(m, now) if m is mb else m for m in mem_b],
    )


def keep_on_move_out(memories: List[Memory], now: int) -> List[Memory]:
    """The 8 strongest in the order of displacement (strongest first), frozen as
    ADR-0030 1 'Auszug und Tod' says: strength at the move-out second, since = that second;
    the list is sorted by key again."""
    ranked = sorted(memories, key=lambda m: displacement_order(m, now), reverse=True)
    kept = [replace(m, strength=strength_at(m, now), since=now) for m in ranked[:KEEP_ON_MOVE_OUT]]
    return sorted(kept, key=lambda m: m.key)


# ---------------------------------------------------------------------------------------
# Bonds (ADR-0030 section 2)


@dataclass(frozen=True)
class Bond:
    other: int
    values: Tuple[int, int, int, int, int, int]
    since: int


def start_role(
    partner: bool, other_is_my_parent: bool, other_is_my_child: bool, sibling: bool,
    phi: Tuple[int, int],
) -> str:
    """The first matching role in the order of the table (ADR-0030 2). Everyone else
    (housemate, or a first talk without an edge) is 'housemate'."""
    if partner:
        return "partner"
    if other_is_my_parent:
        return "parent"
    if other_is_my_child:
        return "child"
    if sibling:
        return "sibling"
    if KIN_PHI[1] * phi[0] >= KIN_PHI[0] * phi[1]:  # phi = (numerator, denominator), integers
        return "kin"
    return "housemate"


def bond_values_at(b: Bond, t: int) -> Tuple[int, ...]:
    """Familiarity and grudge fall per day like a memory (analytic, truncated); the other
    four stay."""
    assert t >= b.since
    elapsed = t - b.since
    v = list(b.values)
    v[FAMILIARITY] = max(0, v[FAMILIARITY] - (FAMILIARITY_FADE * elapsed) // DAY)
    v[GRUDGE] = max(0, v[GRUDGE] - (GRUDGE_FADE * elapsed) // DAY)
    return tuple(v)


def new_bond(other: int, role: str, now: int) -> Bond:
    return Bond(other, START_VALUES[role], now)


def bond_apply(b: Bond, now: int, steps: Dict[int, int]) -> Bond:
    """Write both fading values at now, then apply each step clamped to 0..1000. One call
    is one `core:bond_changed` entry (before = values at now, after = the result)."""
    v = list(bond_values_at(b, now))
    for index, delta in steps.items():
        v[index] = clamp(v[index] + delta)
    return Bond(b.other, tuple(v), now)


def insert_bond(
    bonds: List[Bond],
    other: int,
    role: str,
    now: int,
    protected: Callable[[int], bool],
    cap: int = BOND_CAP,
) -> Tuple[List[Bond], Optional[int], bool]:
    """ADR-0030 2 'Entstehen': returns (list sorted by `other`, dropped other or None,
    formed). A full list drops the edge with the smallest affection + familiarity at now
    (a tie: the larger SimId), never one to a housemate or to kin from phi 1/8
    (`protected`); if there is none the new edge is not formed. READING: the newcomer
    does not compete (unlike a memory): only old edges can fall."""
    assert all(b.other != other for b in bonds), "the edge exists"
    dropped = None
    if len(bonds) >= cap:
        candidates = [b for b in bonds if not protected(b.other)]
        if not candidates:
            return list(bonds), None, False

        def sum_then_id(b: Bond) -> tuple:
            v = bond_values_at(b, now)
            return (v[AFFECTION] + v[FAMILIARITY], -b.other)

        victim = min(candidates, key=sum_then_id)
        dropped = victim.other
        bonds = [b for b in bonds if b is not victim]
    out = sorted(bonds + [new_bond(other, role, now)], key=lambda b: b.other)
    return out, dropped, True


# ---------------------------------------------------------------------------------------
# Talk planning and execution (ADR-0030 section 3)


def house_day(t: int, move_in: int) -> int:
    """The house day: from one HouseMorning (06:00) to the next, counted from 0 at move-in.
    ADR-0030 3 'Haustag': the number of HouseMorning since move-in, that is the 06:00
    boundaries between move-in (exclusive) and t (inclusive). READING: SimTime 0 is
    midnight."""
    six = 6 * 3600
    return (t - six) // DAY - (move_in - six) // DAY


def talk_conditions(
    planned: int,
    *,
    open_talk: bool,
    last_talk: Optional[int],
    talks_a: int,
    talks_b: int,
    entries_a: int,
    entries_b: int,
    age_a: int,
    age_b: int,
    check_open: bool = True,
) -> bool:
    """The planning condition (check_open=True, planned = now + 600) and, without the open
    Talk clause (check_open=False, planned = the second of execution), the execution
    condition (ADR-0030 3 'Beginn'). `last_talk` is the second of the pair's last Talk that
    took place (a cancelled one does not count)."""
    if age_a < TALK_MIN_AGE or age_b < TALK_MIN_AGE:
        return False
    if check_open and open_talk:
        return False
    if last_talk is not None and planned - last_talk < PAIR_GAP:
        return False
    if talks_a >= TALKS_PER_DAY or talks_b >= TALKS_PER_DAY:
        return False
    if ENTRY_CAP - entries_a < TALK_ENTRY_NEED or ENTRY_CAP - entries_b < TALK_ENTRY_NEED:
        return False
    return True


def same_second_order(second_event: str) -> List[str]:
    """ADR-0030 3 'Gleiche Sekunde': what happens, in order, when an open Talk is due in
    the second a participant's action ends or aborts (`action_end`), or in which a death
    (`death`, class 900) or a move-out (`move_out`, an input before the second's events)
    falls."""
    if second_event == "action_end":
        return [
            "run the Talk (steps 0 to 7; step 8 is skipped, someone stands up)",
            "cancel the Talk event",
            "release the seat",
        ]
    if second_event == "death":
        return ["run the Talk (steps 0 to 8)", "death (class 900)"]
    if second_event == "move_out":
        return ["move-out cancels the Talk event (no Talk takes place)"]
    raise ValueError(second_event)


TALK_STEPS = (
    "0 bond_formed for a missing edge",
    "1 the topic",
    "2 the outcome on the edges (bond_changed per changed edge)",
    "3 Necken: the seen deed `neckt` for everyone from 6 at the furniture",
    "4 Klatsch: speaker, lie roll, tell or lie, the hearer's belief and its effect on the "
    "edge; then, if something was said, the seen deed `lästert`",
    "5 the refutation, if one is due",
    "6 memory `Gespräch` for both, `Widerlegung` if 5 happened",
    "7 core:talked { other, object, topic, actor }",
    "8 the next Talk of the pair at now + 3600 if both stay seated and the conditions hold",
)


@dataclass(frozen=True)
class TalkPair:
    """What the topic rules read, for the pair a < b at the second of the Talk. The edges
    are the values at now (bond_values_at); a talk without an edge has had it formed
    first (step 0)."""

    a: int
    b: int
    age_a: int
    age_b: int
    edge_ab: Tuple[int, ...]  # a's edge to b
    edge_ba: Tuple[int, ...]
    interest_a: Tuple[int, ...]  # a's interests, indexed by topic
    interest_b: Tuple[int, ...]
    common_memory: bool  # a common memory exists (Erinnern)
    gossip: bool  # a has material or a reason to lie for b, or b for a (Klatsch)

    def __post_init__(self):
        assert self.a < self.b


def topic_actor(topic: int, p: TalkPair) -> Optional[int]:
    """Who praises, teases or apologises (ADR-0030 3). Lob, Necken: the one with the
    larger interest in the topic, a tie the smaller SimId. Entschuldigung: the one grudged
    against; with a grudge on both sides the one with the smaller grudge, then the smaller
    SimId. READING: 'der mit dem kleineren Groll' is the smaller grudge of the person's own
    edge; this also covers the one-sided case (the one without a grudge above the other's
    is the one grudged against), so one rule serves both. Other topics have no actor
    (None); READING: Klatsch's speaker is ADR-0031's, not an actor here."""
    if topic in (TOPIC_LOB, TOPIC_NECKEN):
        ia, ib = p.interest_a[topic], p.interest_b[topic]
        return p.b if ib > ia else p.a
    if topic == TOPIC_ENTSCHULDIGUNG:
        ga, gb = p.edge_ab[GRUDGE], p.edge_ba[GRUDGE]
        return p.b if gb < ga else p.a
    return None


def topic_allowed(topic: int, p: TalkPair) -> bool:
    if topic == TOPIC_ALLTAG:
        return True
    if topic == TOPIC_LOB:
        # READING (ADR-0030 3): the Lobende is the actor by the interest rule, and the
        # condition is read for him.
        actor = topic_actor(TOPIC_LOB, p)
        edge = p.edge_ab if actor == p.a else p.edge_ba
        return edge[AFFECTION] >= LOB_MIN_AFFECTION
    if topic == TOPIC_NECKEN:
        return p.age_a < NECKEN_MAX_AGE and p.age_b < NECKEN_MAX_AGE
    if topic == TOPIC_KLATSCH:
        return p.gossip
    if topic == TOPIC_ERINNERN:
        return p.common_memory
    if topic == TOPIC_ENTSCHULDIGUNG:
        return max(p.edge_ab[GRUDGE], p.edge_ba[GRUDGE]) >= ENTSCHULDIGUNG_MIN_GRUDGE
    raise ValueError(topic)


def choose_topic(p: TalkPair) -> int:
    """The allowed topic with the largest sum of both interests, the smallest number on a
    tie. Alltag is always allowed."""
    best = None
    for topic in range(6):
        if not topic_allowed(topic, p):
            continue
        score = (-(p.interest_a[topic] + p.interest_b[topic]), topic)
        if best is None or score < best:
            best = score
    return best[1]


def topic_outcome(topic: int, p: TalkPair) -> List[Tuple[int, int, Dict[int, int]]]:
    """The steps of the outcome on the edges, as (owner, other, steps) in the order a's
    edge first. Klatsch (ADR-0031) and the recall of Erinnern (memory) are not here."""
    a, b = p.a, p.b
    if topic == TOPIC_ALLTAG:
        return [(a, b, {FAMILIARITY: 40}), (b, a, {FAMILIARITY: 40})]
    if topic == TOPIC_LOB:
        praiser = topic_actor(topic, p)
        praised, other = (b, a) if praiser == a else (a, b)
        return [(praised, praiser, {AFFECTION: 30, RESPECT: 20})]
    if topic == TOPIC_NECKEN:
        teaser = topic_actor(topic, p)
        teased = b if teaser == a else a
        return [(teased, teaser, {GRUDGE: 60})]
    if topic == TOPIC_ERINNERN:
        return [(a, b, {AFFECTION: 20}), (b, a, {AFFECTION: 20})]
    if topic == TOPIC_ENTSCHULDIGUNG:
        apologiser = topic_actor(topic, p)
        grudging = b if apologiser == a else a
        return [(grudging, apologiser, {GRUDGE: -200, TRUST: 60})]
    return []


# ---------------------------------------------------------------------------------------
# Choice bonus (ADR-0030 section 4)


@dataclass(frozen=True)
class Company:
    """One `core:company` row; avoid is positive and is subtracted."""

    liked: Optional[int]
    company: int
    avoided: Optional[int]
    avoid: int

    @property
    def net(self) -> int:
        return self.company - self.avoid


def choice_bonus(
    present: List[int], edges_at_now: Dict[int, Tuple[int, ...]], company: int = COMPANY, avoid: int = AVOID
) -> Company:
    """The two components for one sociable furniture, outside the mood: `present` are the
    other household members sitting there or on their way (reserved seat), `edges_at_now`
    the chooser's edges to them (bond_values_at). `liked`: among those with affection +
    familiarity >= 600 the one with the largest sum (then the smaller SimId); `avoided`:
    among those with trust < 400 the one with the smallest trust (then the smaller SimId).
    Each counts at most once per offer. READING (ADR-0030 4): 'Sitzt dort jemand' for the
    avoid is taken as the same set as for the company (seated or on the way). A person
    without an edge is not considered."""
    liked = avoided = None
    best_sum = -1
    worst_trust = None
    for sim in sorted(present):
        edge = edges_at_now.get(sim)
        if edge is None:
            continue
        s = edge[AFFECTION] + edge[FAMILIARITY]
        if s >= COMPANY_MIN_SUM and s > best_sum:
            liked, best_sum = sim, s
        if edge[TRUST] < AVOID_BELOW_TRUST and (worst_trust is None or edge[TRUST] < worst_trust):
            avoided, worst_trust = sim, edge[TRUST]
    return Company(liked, company if liked is not None else 0, avoided, avoid if avoided is not None else 0)


# ---------------------------------------------------------------------------------------
# Beliefs (ADR-0031 sections 1 and 2)
#
# A statement key is a tuple whose first element orders the two variants: (0, child) for
# VaterVon, (1, doer, art, other, day) for Tat. READING (ADR-0031 1): VaterVon sorts before
# Tat, in the order the ADR lists them.


def vater_key(child: int) -> tuple:
    return (0, child)


def tat_key(doer: int, art: int, other: int, day: int) -> tuple:
    return (1, doer, art, other, day)


@dataclass(frozen=True)
class Belief:
    key: tuple
    man: Optional[int]  # the named man of a VaterVon, None for a Tat
    certainty: int
    since: int
    source: Optional[int]
    origin: Optional[int]
    basis: str
    secret: bool = False


def heard_certainty(speaker_certainty: int, trust: int) -> int:
    """min(900, floor(c * (500 + trust) / 1500)) in integers (ADR-0031 2)."""
    return min(HEARD_CAP, (speaker_certainty * (500 + trust)) // 1500)


def hear(
    held: Optional[Belief],
    key: tuple,
    man: Optional[int],
    speaker: int,
    speaker_belief: Optional[Belief],
    speaker_certainty: int,
    trust: int,
    now: int,
) -> Tuple[Optional[Belief], str]:
    """The hearer takes over a sentence (ADR-0031 2 'Gehört'). Returns (belief or None if
    nothing changes, kind), kind one of 'new', 'confirmed', 'replaced', 'unchanged'.

    - a held belief with basis `widerlegt` never changes by hearing;
    - the same statement (same key and man) held: +50 up to 900 (confirmation), source and
      origin stay. READING: a held certainty above 900 (seen, 1000) stays, the cap never
      lowers it;
    - another man under the same key: replaced by the heard one (replaced=True);
    - else new: certainty heard_certainty, source the speaker, origin the speaker's origin
      if his basis is `gehört`, else the speaker (he saw, took part in or lied it; a lie has
      no speaker belief, `speaker_belief` None)."""
    if held is not None and held.basis == WIDERLEGT:
        return None, "unchanged"
    if held is not None and held.man == man:
        raised = max(held.certainty, min(CONFIRM_CAP, held.certainty + CONFIRM_STEP))
        if raised == held.certainty:
            return None, "unchanged"
        return replace(held, certainty=raised, since=now), "confirmed"
    if speaker_belief is not None and speaker_belief.basis == GEHOERT:
        origin = speaker_belief.origin
    else:
        origin = speaker
    new = Belief(key, man, heard_certainty(speaker_certainty, trust), now, speaker, origin, GEHOERT)
    return new, ("replaced" if held is not None else "new")


def see(held: Optional[Belief], key: tuple, now: int) -> Tuple[Optional[Belief], str]:
    """A deed seen in a talk: certainty 1000, basis `gesehen`, source and origin empty. It
    replaces a heard sentence of the same key. READING (ADR-0031 2): it adds a new one, and
    leaves every other held basis (gesehen, beteiligt, geschlossen, widerlegt) alone."""
    new = Belief(key, None, 1000, now, None, None, GESEHEN)
    if held is None:
        return new, "new"
    if held.basis == GEHOERT:
        return new, "replaced"
    return None, "unchanged"


def belief_eviction_order(b: Belief) -> tuple:
    return (b.certainty, b.since, b.key)


def insert_belief(
    beliefs: Dict[tuple, Belief], new: Belief, own_children: set, cap: int = BELIEF_CAP
) -> Tuple[Dict[tuple, Belief], Optional[tuple], bool]:
    """Adding a belief of a key not held yet (ADR-0031 2). A full list drops the belief
    with the smallest certainty (then the older `since`, then the smaller key), never one
    with `secret` and never a VaterVon of the holder's own child; the newcomer counts as
    in ADR-0030 1 and, if it is the weakest, is not taken in; if there is no old one that
    may fall, the newcomer is not taken in. The protection holds for the newcomer too: a
    protected newcomer is never the one that falls (ADR-0031 2 "nie einer mit secret").
    Returns (beliefs, dropped key, accepted)."""
    assert new.key not in beliefs
    if len(beliefs) < cap:
        out = dict(beliefs)
        out[new.key] = new
        return out, None, True

    def protected(b: Belief) -> bool:
        return b.secret or (b.key[0] == 0 and b.key[1] in own_children)

    pool = [b for b in beliefs.values() if not protected(b)]
    if not pool:
        return dict(beliefs), None, False
    victim = min(pool if protected(new) else pool + [new], key=belief_eviction_order)
    if victim is new:
        return dict(beliefs), None, False
    out = {k: v for k, v in beliefs.items() if k != victim.key}
    out[new.key] = new
    return out, victim.key, True


def heard_bond_steps(hearer: int, key: tuple) -> Optional[Dict[int, int]]:
    """The steps on the hearer's edge to the doer W for a heard deed (ADR-0031 3): a
    `lästert` or `lügt` against the hearer himself: trust -80 and grudge +100 (one entry);
    any other deed of W, `neckt` included: trust -20. A confirmation has no step (the
    caller does not ask), and the one who merely sees the deed gets none. None for a
    VaterVon."""
    if key[0] != 1:
        return None
    _, doer, art, other, _ = key
    if other == hearer and art in (LAESTERT, LUEGT):
        return {TRUST: -80, GRUDGE: 100}
    return {TRUST: -20}


# ---------------------------------------------------------------------------------------
# Truth of deeds (ADR-0031 1)


@dataclass
class Deeds:
    """A person's own deeds of the last 30 house days, at most 64, in the order they
    came about; the truth for a Tat (ADR-0031 1 'Wahrheit': the window is today and the 29 days
    before). When the 65th arrives the oldest falls (within a
    day the one that came first) and the window is shorter: no day up to the day of a fallen
    deed can be judged any more."""

    items: List[Tuple[int, int, int]]  # (art, other, day)
    dropped_day: Optional[int] = None

    def add(self, art: int, other: int, day: int) -> None:
        self.items.append((art, other, day))
        if len(self.items) > DEED_CAP:
            _, _, fallen_day = self.items.pop(0)
            self.dropped_day = fallen_day if self.dropped_day is None else max(self.dropped_day, fallen_day)

    def window_start(self, today: int) -> int:
        start = today - (DEED_WINDOW_DAYS - 1)
        if self.dropped_day is not None:
            start = max(start, self.dropped_day + 1)
        return start

    def knows(self, art: int, other: int, day: int, today: int) -> Optional[bool]:
        """True: done. False: certainly not done (the day is in the window). None: the day
        is outside the window, the list cannot say."""
        if day > today or day < self.window_start(today):
            return None
        return (art, other, day) in self.items


# ---------------------------------------------------------------------------------------
# Gossip (ADR-0031 sections 3 and 4)


def gossip_material(
    giver: int,
    giver_beliefs: Dict[tuple, Belief],
    hearer: int,
    hearer_beliefs: Dict[tuple, Belief],
    seated: set,
) -> List[Belief]:
    """The material of the giver for the hearer: a belief of the kind Tat (a VaterVon is
    never material) about a third party, the doer (neither hearer nor giver, and not seated
    at the same furniture), not secret, with certainty above 0, that the hearer does not
    hold as `widerlegt` and does not hold at least as surely (ADR-0031 3 'Stoff'; a
    sentence the hearer holds as widerlegt is no material, so nobody repeats it forever).
    READING: 'Subjekt' is the doer; 'mindestens so sicher' compares with the giver's own
    certainty."""
    out = []
    for key, belief in giver_beliefs.items():
        if key[0] != 1 or belief.secret or belief.certainty <= 0:
            continue
        doer = key[1]
        if doer in (giver, hearer) or doer in seated:
            continue
        held = hearer_beliefs.get(key)
        if held is not None and (held.basis == WIDERLEGT or held.certainty >= belief.certainty):
            continue
        out.append(belief)
    return sorted(out, key=lambda b: b.key)


def lie_reason(
    speaker: int, hearer: int, household: List[int], speaker_grudge_at_now: Dict[int, int]
) -> Optional[int]:
    """The W of ADR-0031 4: grudge from 700 on a person W of the household who is not the
    hearer (nor the speaker), the smallest SimId among several."""
    for w in sorted(household):
        if w in (speaker, hearer):
            continue
        if speaker_grudge_at_now.get(w, 0) >= LIE_GRUDGE:
            return w
    return None


def choose_speaker(
    a: int, material_a: List[Belief], reason_a: Optional[int],
    b: int, material_b: List[Belief], reason_b: Optional[int],
) -> Optional[int]:
    """ADR-0031 3: the speaker is who has material or a reason; if both, the larger sum of
    the certainties of his material, then the smaller SimId. None: no speaker, so Klatsch
    is not allowed."""
    a_ok = bool(material_a) or reason_a is not None
    b_ok = bool(material_b) or reason_b is not None
    if a_ok and not b_ok:
        return a
    if b_ok and not a_ok:
        return b
    if not a_ok:
        return None
    sa = sum(m.certainty for m in material_a)
    sb = sum(m.certainty for m in material_b)
    return b if sb > sa else a


def said_belief(material: List[Belief]) -> Optional[Belief]:
    """The sentence said: the surest material, the smaller key on a tie."""
    if not material:
        return None
    return min(material, key=lambda m: (-m.certainty, m.key))


def lie_statement(w: int, hearer: int, today: int) -> Tuple[tuple, int]:
    """What a liar says: Tat(W, lästert, hearer, today) with certainty 900."""
    return tat_key(w, LAESTERT, hearer, today), LIE_CERTAINTY


# ---------------------------------------------------------------------------------------
# Refutation (ADR-0031 section 5)


def can_refute(
    refuter: int,
    refuter_beliefs: Dict[tuple, Belief],
    doer_deeds: Deeds,
    belief: Belief,
    true_father: Callable[[int], Optional[int]],
    today: int,
) -> bool:
    """Whether `refuter` can refute the hearer's belief. Certainty above 0, basis not
    `widerlegt`, not secret, and the statement known false now.
    A Tat is known false only when the alleged doer's deeds say he did not do it (`knows`
    is False). A day outside his window, or a day the cap closed, is unknown, so nobody
    can refute it. Only then the doer himself, or anyone who already holds the key as
    `widerlegt`. A VaterVon is known false when the named man is not the genealogical
    father; then anyone who holds the true man at 1000 with no secret. A true statement
    is never refuted. `doer_deeds` are the alleged doer's (empty for a VaterVon)."""
    if belief.certainty <= 0 or belief.basis == WIDERLEGT or belief.secret:
        return False
    key = belief.key
    if key[0] == 0:
        truth = true_father(key[1])
        if belief.man == truth:
            return False
        mine = refuter_beliefs.get(key)
        return mine is not None and mine.man == truth and mine.certainty == 1000 and not mine.secret
    _, doer, art, other, day = key
    if doer_deeds.knows(art, other, day, today) is not False:
        return False
    if refuter == doer:
        return True
    mine = refuter_beliefs.get(key)
    return mine is not None and mine.basis == WIDERLEGT


def pick_refutation(
    a: int, beliefs_a: Dict[tuple, Belief], deeds_a: Deeds,
    b: int, beliefs_b: Dict[tuple, Belief], deeds_b: Deeds,
    true_father: Callable[[int], Optional[int]], today: int,
    deeds_of: Optional[Callable[[int], Deeds]] = None,
) -> Optional[Tuple[int, int, tuple]]:
    """ADR-0031 5: at most one refutation per Talk: first a as the refuter, then b, in each
    case the smallest key; only this one roll. A Tat is judged on the alleged doer's deeds:
    a's, b's, or `deeds_of` for anyone else (missing is an empty list). Returns
    (refuter, hearer, key) or None."""
    for refuter, r_beliefs, r_deeds, hearer, h_beliefs, h_deeds in (
        (a, beliefs_a, deeds_a, b, beliefs_b, deeds_b),
        (b, beliefs_b, deeds_b, a, beliefs_a, deeds_a),
    ):
        def deeds_for(
            bel: Belief,
            r_deeds: Deeds = r_deeds,
            h_deeds: Deeds = h_deeds,
            refuter: int = refuter,
            hearer: int = hearer,
        ) -> Deeds:
            key = bel.key
            if key[0] != 1:
                return Deeds([])
            doer = key[1]
            if doer == refuter:
                return r_deeds
            if doer == hearer:
                return h_deeds
            if deeds_of is None:
                return Deeds([])
            return deeds_of(doer)

        keys = sorted(
            k
            for k, bel in h_beliefs.items()
            if can_refute(refuter, r_beliefs, deeds_for(bel), bel, true_father, today)
        )
        if keys:
            return refuter, hearer, keys[0]
    return None


def refute(
    belief: Belief, refuter: int, true_man: Optional[int], now: int
) -> Tuple[Belief, Optional[int], Optional[Dict[int, int]]]:
    """The effects (ADR-0031 5 'Wirkung'): (the hearer's new belief, the immediate source of
    the false belief, the steps on the hearer's edge to that source). A false Tat becomes a
    belief with certainty 0, basis `widerlegt`; a false VaterVon the true man with 1000,
    `widerlegt`, no secret; since = now, source and origin the refuter. Trust in the source
    -120; if the source is also the origin of the lie (origin = source on a false Tat, which
    only a liar's sentence has), grudge +150 as well; no source, no step. READING: the two
    steps are one entry (as the +100/-80 of the gossip)."""
    if belief.key[0] == 0:
        new = Belief(belief.key, true_man, 1000, now, refuter, refuter, WIDERLEGT, False)
    else:
        new = Belief(belief.key, None, 0, now, refuter, refuter, WIDERLEGT, False)
    source = belief.source
    if source is None:
        return new, None, None
    steps = {TRUST: -120}
    if belief.origin == source:
        steps[GRUDGE] = 150
    return new, source, steps


# ---------------------------------------------------------------------------------------
# Paternity at birth (ADR-0031 section 6)


def paternity_beliefs(
    mother: int,
    child: int,
    father: int,
    father_in_town: bool,
    partner: Optional[int],
    household: List[Tuple[int, int]],
    now: int,
) -> List[Tuple[int, Belief]]:
    """The sentences written at the birth of `child` (only if the mother's household has
    moved in; the caller checks that). `father` is F, the biological father; `partner` is P,
    the man of the mother's open partnership or None; `household` the (SimId, age) of the
    mother's household, the child excluded (living persons). S is P if there is one, else F.
    Returns (holder, belief) in the order of the rules, then by SimId:
    1 the mother: VaterVon(child, F), 1000, beteiligt, secret only if P exists and is not F;
    2 F: the same sentence, 1000, beteiligt, also outside the household, if F is a person of
      the town;
    3 P, if P exists and is not F: VaterVon(child, P), 900, geschlossen;
    4 every other person from 6 in the household: VaterVon(child, S), 900, geschlossen.
    One sentence per person and child: the earlier rule wins."""
    key = vater_key(child)
    has_p = partner is not None
    secret = has_p and partner != father
    out = [(mother, Belief(key, father, 1000, now, None, None, BETEILIGT, secret))]
    given = {mother}
    if father_in_town and father not in given:
        out.append((father, Belief(key, father, 1000, now, None, None, BETEILIGT, False)))
        given.add(father)
    if has_p and partner != father and partner not in given:
        out.append((partner, Belief(key, partner, 900, now, None, None, GESCHLOSSEN, False)))
        given.add(partner)
    s = partner if has_p else father
    for sim, age in sorted(household):
        if sim in given or sim == child or age < TALK_MIN_AGE:
            continue
        out.append((sim, Belief(key, s, 900, now, None, None, GESCHLOSSEN, False)))
        given.add(sim)
    return out


# ---------------------------------------------------------------------------------------
# Feelings and temper (ADR-0032 sections 1 to 5)
#
# Four channels (joy, anger, sadness, fear), each 0..1000, with one shared `since`, and a
# temper -1000..1000. Everything is analytic: no event, i64 semantics. Division truncates
# toward zero, so `div0` is used wherever the dividend can be negative.

JOY, ANGER, SADNESS, FEAR = range(4)
CHANNEL_NAMES = ("joy", "anger", "sadness", "fear")
FEEL_RATES = (300, 300, 150, 400)  # per day, ADR-0032 1 (start values)
MOOD_RETURN = 100  # per day, ADR-0032 2 (start value)
FEEL_MIN_AGE = 6  # ADR-0032 1: channels from 6 years, in a moved-in household
TEMPER_MAX = 1000
BAND_MIN_AFFECTION = 600  # ADR-0032 3
BAND_NUM, BAND_DEN = 3, 2
CONTAGION_MIN_ANGER = 700  # ADR-0032 4
CONTAGION_STEP = 150
CONTAGION_CAP = 699


def div0(a: int, b: int) -> int:
    """i64 division as Rust does it: truncating toward zero (Python's // floors)."""
    assert b != 0
    q = abs(a) // abs(b)
    return q if (a >= 0) == (b > 0) else -q


def channel_at(w: int, rate: int, since: int, t: int) -> int:
    """ADR-0032 1: max(0, w - (rate * max(0, t - since)) / 86400); the product is not
    negative, so the truncation is a floor."""
    return max(0, w - div0(rate * max(0, t - since), DAY))


def temper_at(s: int, since: int, t: int, mood_return: int = MOOD_RETURN) -> int:
    """ADR-0032 2: |s(t)| = max(0, |s| - (mood_return * max(0, t - since)) / 86400), with the
    sign of s. READING (ADR-0032 2): at magnitude 0 the sign is moot, the result is 0."""
    mag = max(0, abs(s) - div0(mood_return * max(0, t - since), DAY))
    return mag if s >= 0 else -mag


@dataclass(frozen=True)
class Feel:
    """Stored state of one person: the four channel values, the temper and the shared
    `since`. READING (ADR-0032 1 and 2): the ADR gives the channels a shared `since` and the
    temper its own, but every write sets both to `now` (section 1: 'schreibt die Werte mit
    since = now'), and the move-in sets both to the move-in second, so one `since` carries
    both."""

    values: Tuple[int, int, int, int] = (0, 0, 0, 0)
    temper: int = 0
    since: int = 0


def feel_at(f: Feel, t: int, rates: Tuple[int, ...] = FEEL_RATES, mood_return: int = MOOD_RETURN) -> Tuple[Tuple[int, ...], int]:
    """The four channels and the temper read analytically at second t >= since."""
    assert t >= f.since
    vals = tuple(channel_at(f.values[c], rates[c], f.since, t) for c in range(4))
    return vals, temper_at(f.temper, f.since, t, mood_return)


def band_step(step: int, affection: Optional[int]) -> int:
    """ADR-0032 3: (step * 3) / 2 toward 0 when the evaluating person's affection to the
    band person is >= 600 at `now` (negative steps too: -200 becomes -300); factor 1
    otherwise and when the row has no band (affection None)."""
    if affection is not None and affection >= BAND_MIN_AFFECTION:
        return div0(step * BAND_NUM, BAND_DEN)
    return step


Row = Tuple[Tuple[int, int, int, int], Optional[int]]  # (steps joy/anger/sadness/fear, affection to band person or None)


def apply_occasion(
    f: Feel, now: int, rows: List[Row], rates: Tuple[int, ...] = FEEL_RATES, mood_return: int = MOOD_RETURN
) -> Tuple[Feel, bool, Tuple[int, int, int, int]]:
    """One occasion for one person (ADR-0032 1 to 3): read channels and temper at `now`,
    band-factor each step of each row, sum all rows (a Talk has several), clamp each channel
    once to 0..1000, take the clamped deltas, temper step (dJoy - dAnger - dSadness - dFear)
    / 4 toward 0, clamp the temper to -1000..1000. Returns (new state, changed, deltas).
    If neither a clamped channel nor the temper changes, the state is returned untouched
    (no write, `since` unchanged) and `changed` is False."""
    vals, temper = feel_at(f, now, rates, mood_return)
    total = [0, 0, 0, 0]
    for steps, affection in rows:
        for c in range(4):
            total[c] += band_step(steps[c], affection)
    new = tuple(clamp(vals[c] + total[c]) for c in range(4))
    delta = tuple(new[c] - vals[c] for c in range(4))
    step = div0(delta[JOY] - delta[ANGER] - delta[SADNESS] - delta[FEAR], 4)
    new_temper = clamp(temper + step, -TEMPER_MAX, TEMPER_MAX)
    if new == vals and new_temper == temper:
        return f, False, delta
    return Feel(new, new_temper, now), True, delta


# The occasion table (ADR-0032 3, start values): steps (joy, anger, sadness, fear).
# An absent row means all 0.
STEPS_TALK_ALLTAG = (40, 0, 0, 0)
STEPS_TALK_LOB = (150, 0, 0, 0)
STEPS_TALK_NECKEN = (0, 100, 0, 0)
STEPS_TALK_ERINNERN = (100, 0, 0, 0)
STEPS_TALK_ENTSCHULDIGUNG = (50, -200, 0, 0)
STEPS_HEARD = (0, 200, 100, 0)  # gehoert: laestert / luegt against the hearer
STEPS_REFUTED = (0, 150, 0, 0)
STEPS_BIRTH = (800, 0, 0, 0)
STEPS_PARTNER_DEATH = (0, 0, 1000, 200)
STEPS_KIN_DEATH = (0, 0, 800, 100)
STEPS_PARTNERSHIP = (600, 0, 0, 0)
STEPS_SEPARATION = (0, 300, 600, 0)
STEPS_NEED_EMPTY = (0, 150, 0, 100)

# An effect: (the person who feels, the steps, the person the band is read toward or None).
Effect = Tuple[int, Tuple[int, int, int, int], Optional[int]]


def effect_talk_alltag(a: int, b: int) -> List[Effect]:
    return [(a, STEPS_TALK_ALLTAG, b), (b, STEPS_TALK_ALLTAG, a)]


def effect_talk_lob(praised: int, praiser: int) -> List[Effect]:
    return [(praised, STEPS_TALK_LOB, praiser)]


def effect_talk_necken(teased: int, teaser: int) -> List[Effect]:
    return [(teased, STEPS_TALK_NECKEN, teaser)]


def effect_talk_erinnern(a: int, b: int) -> List[Effect]:
    return [(a, STEPS_TALK_ERINNERN, b), (b, STEPS_TALK_ERINNERN, a)]


def effect_talk_entschuldigung(grudger: int, other: int) -> List[Effect]:
    return [(grudger, STEPS_TALK_ENTSCHULDIGUNG, other)]


def effect_heard(hearer: int, about: int) -> List[Effect]:
    """A heard deed (laestert / luegt) against the hearer; the band is read toward W, the
    person the sentence is about. The mild heard deed (trust -20, also neckt) has no row."""
    return [(hearer, STEPS_HEARD, about)]


def effect_refutation(hearer: int, source: Optional[int]) -> List[Effect]:
    """The band is read toward the source; without a source, no band (factor 1). READING
    (ADR-0032 3): 'ohne Quelle Band 1' is factor 1, i.e. the row is not banded."""
    return [(hearer, STEPS_REFUTED, source)]


def effect_birth(eligible_parents: List[int]) -> List[Effect]:
    """Mother and father, whoever may receive the memory (the caller passes them); no band.
    READING (ADR-0032 3): 'wer die Erinnerung bekommen darf' is the caller's list, the same
    entitlement as the memory (ADR-0030 1), not whether the entry made it into the list."""
    return [(p, STEPS_BIRTH, None) for p in eligible_parents]


def effect_death(dead: int, partner: Optional[int], kin_eligible: List[int]) -> List[Effect]:
    """The partner's death replaces the kin row for the survivor: one row even if the dead
    was also kin. Everyone else who may receive the memory gets the kin row. The band is
    read toward the dead."""
    out: List[Effect] = []
    if partner is not None:
        out.append((partner, STEPS_PARTNER_DEATH, dead))
    out += [(k, STEPS_KIN_DEATH, dead) for k in kin_eligible if k != partner]
    return out


def effect_partnership(a: int, b: int) -> List[Effect]:
    return [(a, STEPS_PARTNERSHIP, None), (b, STEPS_PARTNERSHIP, None)]


def effect_separation(a: int, b: int) -> List[Effect]:
    return [(a, STEPS_SEPARATION, None), (b, STEPS_SEPARATION, None)]


def effect_need_empty(sim: int, needs_reaching_zero: List[int]) -> List[Effect]:
    """Once per settle, however many needs reach 0 in the same second; none if none do.
    READING (ADR-0032 3): 'einmal je settle' means one row per person and call."""
    return [(sim, STEPS_NEED_EMPTY, None)] if needs_reaching_zero else []


def rows_for(sim: int, effects: List[Effect], affection_to: Callable[[int, int], Optional[int]]) -> List[Row]:
    """The rows one person gets from a list of effects (a Talk bundles several into one
    occasion); `affection_to(sim, other)` is the affection of sim's edge to other at `now`,
    None without an edge (then no band)."""
    return [(steps, None if band is None else affection_to(sim, band)) for who, steps, band in effects if who == sim]


def enters(previous: Optional[int], arrival: int) -> bool:
    """ADR-0032 4, addendum 2026-11-12: contagion only when the arrival's room differs from
    the room before the walk. `previous is None` is from outside (came home) and counts as
    entering. A walk to another object in the same room does not."""
    return previous != arrival


def contagion(
    arriver: int,
    arriver_anger: int,
    room: List[Tuple[int, int, int]],
    min_anger: int = CONTAGION_MIN_ANGER,
    step: int = CONTAGION_STEP,
    cap: int = CONTAGION_CAP,
) -> List[Tuple[int, int, int]]:
    """ADR-0032 4: on entering a room with anger >= min_anger (at the arrival second; the start
    values are 700, 150 and 699), every other person from 6 in that room (`room` = (sim, age,
    anger at now) of everyone whose place is the room) in SimId order gets
    max(anger, min(anger + step, cap)) if below min_anger; at min_anger or above the anger
    stays; the arriver is not touched. Call this only when `enters` is true: the room is
    given, not whether the arriver was already in it. Returns (sim, new anger, delta) for
    every eligible person (delta 0 for those unchanged), so the temper can follow by section 2
    (apply the delta through apply_occasion with the row (0, delta, 0, 0), no band). Nobody
    is infected twice by one push and nobody's anger falls, even when cap is below that anger."""
    if arriver_anger < min_anger:
        return []
    out = []
    for sim, age, anger in sorted(room):
        if sim == arriver or age < FEEL_MIN_AGE:
            continue
        new = max(anger, min(anger + step, cap)) if anger < min_anger else anger
        out.append((sim, new, new - anger))
    return out


def scaled_company(company: int, temper: int) -> int:
    """ADR-0032 5: company * (1000 + s) / 1000 in i64 toward 0; at -1000 it vanishes, at
    +1000 it doubles. `avoid` stays unscaled."""
    return div0(company * (1000 + temper), 1000)


# ---------------------------------------------------------------------------------------
# --check: every worked value of the two ADRs that the text fixes

CHECKS = 0


def expect(name: str, got, want) -> None:
    global CHECKS
    CHECKS += 1
    assert got == want, f"FAIL {name}: got {got!r}, want {want!r}"
    print(f"ok  {name}: {got!r}")


def mem(kind=0, with_=(None, None), at=0, strength=500, since=0, fade=0, valence=0) -> Memory:
    return Memory(kind, with_, at, valence, strength, since, fade)


def check_memory() -> None:
    m = mem(strength=800, fade=100, since=1000)
    expect("memory 800/100 after 4 days", strength_at(m, 1000 + 4 * DAY), 400)
    expect("memory zero second", zero_second(m), 1000 + 691_200)
    expect("memory zero at exactly 691200 s", strength_at(m, 1000 + 691_200), 0)
    expect("memory one second before zero", strength_at(m, 1000 + 691_199), 1)
    expect("memory at the start", strength_at(m, 1000), 800)
    expect("fade 0 never zero", zero_second(mem(fade=0)), None)
    expect("forget of an all-fade-0 list", next_forget([mem(fade=0), mem(kind=1)]), None)
    expect("forget at the earliest zero", next_forget([m, mem(strength=100, fade=100, since=0)]), 86_400)
    expect("zero second is the first second at 0", [
        strength_at(mem(strength=1000, fade=7), z) for z in (zero_second(mem(strength=1000, fade=7)) - 1, zero_second(mem(strength=1000, fade=7)))
    ], [1, 0])
    # Recall: 400 lifts to 700, since = now; capped at 1000.
    r = recall_one(mem(strength=800, fade=100, since=0), 4 * DAY)
    expect("recall 400 -> 700", (r.strength, r.since), (700, 4 * DAY))
    expect("recall cap 1000", recall_one(mem(strength=900), 5).strength, 1000)
    expect("recall zero second moves", zero_second(r), 4 * DAY + 7 * DAY)

    # Eviction: 32 memories, the 33rd.
    def full(strengths_at: Dict[int, tuple], base=500) -> List[Memory]:
        out = []
        for k in range(32):
            s, at = strengths_at.get(k, (base, 100 + k))
            out.append(mem(kind=k, at=at, strength=s))
        return sorted(out, key=lambda x: x.key)

    now = 10_000
    lst = full({5: (100, 50)})
    new = mem(kind=40, at=now, strength=600, since=now)
    out, victim, ok = insert_memory(lst, new, now)
    expect("evict weakest", (victim.kind, ok, len(out)), (5, True, 32))
    lst = full({5: (100, 60), 9: (100, 50)})
    out, victim, ok = insert_memory(lst, mem(kind=40, at=now, strength=600, since=now), now)
    expect("evict tie: older at", victim.kind, 9)
    lst = full({7: (100, 50), 3: (100, 50)})
    out, victim, ok = insert_memory(lst, mem(kind=40, at=now, strength=600, since=now), now)
    expect("evict tie: smaller key (kind)", victim.kind, 3)
    lst = full({5: (100, 50)})
    out, victim, ok = insert_memory(lst, mem(kind=40, at=now, strength=50, since=now), now)
    expect("newcomer weaker: not taken in", (victim, ok, out == lst), (None, False, True))
    out, victim, ok = insert_memory(lst, mem(kind=40, at=40, strength=100, since=now), now)
    expect("newcomer tie, older at: not taken in", (victim, ok), (None, False))
    out, victim, ok = insert_memory(lst, mem(kind=40, at=70, strength=100, since=now), now)
    expect("newcomer tie, younger at: old one falls", (victim.kind, ok), (5, True))
    lst = sorted(
        [mem(kind=0, at=0, strength=500, fade=100, since=0)]
        + [mem(kind=k, at=k, strength=300) for k in range(1, 32)],
        key=lambda x: x.key,
    )
    out, victim, ok = insert_memory(lst, mem(kind=50, at=4 * DAY, strength=700, since=4 * DAY), 4 * DAY)
    expect("evict by faded strength at now (500 -> 100)", victim.kind, 0)
    out, victim, ok = insert_memory(lst[:31], mem(kind=50, at=4 * DAY, strength=1, since=4 * DAY), 4 * DAY)
    expect("room: nothing falls", (victim, ok, len(out)), (None, True, 32))

    # Move-out: keep the 8 strongest, frozen.
    lst = [mem(kind=k, at=k, strength=100 * (k % 10 + 1), fade=10) for k in range(12)]
    kept = keep_on_move_out(lst, DAY)
    expect(
        "move-out keeps 8, strongest first by displacement order",
        sorted(m.kind for m in kept),
        sorted(
            m.kind
            for m in sorted(lst, key=lambda m: displacement_order(m, DAY), reverse=True)[:8]
        ),
    )
    expect("move-out kept strengths are frozen at now", {(m.since, m.strength) for m in kept} <= {
        (DAY, s) for s in range(0, 1001)
    }, True)
    expect("move-out kept strength value", [m.strength for m in kept if m.kind == 9], [990])
    # Recall pair: common memory choice
    a_mems = [mem(kind=1, with_=(2, None), at=100, strength=400), mem(kind=1, with_=(2, None), at=200, strength=400)]
    b_mems = [mem(kind=1, with_=(1, None), at=100, strength=400), mem(kind=1, with_=(1, None), at=200, strength=400)]
    found = common_memory(1, a_mems, 2, b_mems, 300)
    expect("common memory tie: younger at", found[0].at, 200)
    a_mems[0] = mem(kind=1, with_=(2, None), at=100, strength=800)
    found = common_memory(1, a_mems, 2, b_mems, 300)
    expect("common memory: larger summed strength", found[0].at, 100)
    expect("common memory: none for a stranger", common_memory(1, a_mems, 3, b_mems, 300), None)


def check_bonds() -> None:
    expect("start partner", START_VALUES["partner"], (700, 700, 500, 500, 600, 0))
    expect("start parent", START_VALUES["parent"], (650, 650, 600, 0, 600, 0))
    expect("start child", START_VALUES["child"], (700, 600, 300, 0, 600, 0))
    expect("start sibling", START_VALUES["sibling"], (550, 550, 400, 0, 500, 0))
    expect("start kin", START_VALUES["kin"], (500, 500, 450, 0, 300, 0))
    expect("start housemate", START_VALUES["housemate"], (400, 450, 400, 0, 200, 0))
    expect("role order", ROLE_ORDER, ("partner", "parent", "child", "sibling", "kin", "housemate"))
    expect("role: partner beats kin", start_role(True, False, False, False, (1, 2)), "partner")
    expect("role: parent beats sibling", start_role(False, True, False, True, (1, 2)), "parent")
    expect("role: child", start_role(False, False, True, False, (1, 2)), "child")
    expect("role: sibling", start_role(False, False, False, True, (1, 2)), "sibling")
    expect("role: kin at phi 1/8", start_role(False, False, False, False, (1, 8)), "kin")
    expect("role: below 1/8 housemate", start_role(False, False, False, False, (1, 16)), "housemate")

    b = Bond(7, (500, 500, 500, 0, 600, 300), 0)
    expect("fade after 10 days", bond_values_at(b, 10 * DAY), (500, 500, 500, 0, 550, 200))
    expect("fade truncates (1 s)", bond_values_at(b, 1), (500, 500, 500, 0, 600, 300))
    expect("fade truncates (17280 s = 0.2 day -> 1)", bond_values_at(b, 17_280), (500, 500, 500, 0, 599, 298))
    expect("fade clamps at 0", bond_values_at(b, 40 * DAY), (500, 500, 500, 0, 400, 0))
    expect("fade familiarity to 0", bond_values_at(Bond(7, (0, 0, 0, 0, 100, 0), 0), 30 * DAY)[FAMILIARITY], 0)
    c = bond_apply(b, 10 * DAY, {GRUDGE: 60})
    expect("step writes fade then adds", (c.values, c.since), ((500, 500, 500, 0, 550, 260), 10 * DAY))
    c = bond_apply(Bond(7, (990, 20, 500, 0, 995, 995), 0), 0, {AFFECTION: 30, TRUST: -80, FAMILIARITY: 40, GRUDGE: 100})
    expect("clamp 0..1000", c.values, (1000, 0, 500, 0, 1000, 1000))

    # Eviction: 32 edges, others 100..131; 100..109 are housemates or kin (protected).
    protected = lambda o: o < 110
    bonds = [Bond(o, (300, 500, 400, 0, 200, 0), 0) for o in range(100, 132)]
    bonds[10 + 5] = Bond(115, (100, 500, 400, 0, 100, 0), 0)  # sum 200
    bonds[10 + 12] = Bond(122, (100, 500, 400, 0, 100, 0), 0)  # sum 200, tie: larger SimId falls
    bonds[3] = Bond(103, (0, 500, 400, 0, 0, 0), 0)  # smallest sum, but protected
    out, dropped, formed = insert_bond(bonds, 200, "housemate", 0, protected)
    expect("bond eviction: tie on the sum, larger SimId", (dropped, formed, len(out)), (122, True, 32))
    expect("bond eviction: list sorted", [b.other for b in out] == sorted(b.other for b in out), True)
    expect("bond eviction: new edge has the start values", [b.values for b in out if b.other == 200], [(400, 450, 400, 0, 200, 0)])
    # Familiarity fade enters the sum at now.
    bonds2 = [Bond(o, (300, 500, 400, 0, 200, 0), 0) for o in range(100, 132)]
    bonds2[20] = Bond(120, (300, 500, 400, 0, 250, 0), 0)  # at day 10: 200 -> sum 500
    bonds2 = [Bond(b.other, b.values, 0) for b in bonds2]
    out, dropped, formed = insert_bond(bonds2, 200, "housemate", 10 * DAY, lambda o: o < 110)
    # all others: aff 300 + fam 200-50 = 150 -> 450; 120: 300 + 200 = 500; smallest sum 450 ties: larger SimId 131
    expect("bond eviction at now (faded sums, larger SimId)", dropped, 131)
    out, dropped, formed = insert_bond(bonds, 200, "housemate", 0, lambda o: True)
    expect("bond eviction: all protected, no new edge", (dropped, formed, len(out)), (None, False, 32))
    out, dropped, formed = insert_bond(bonds[:31], 200, "partner", 0, protected)
    expect("bond room: nothing falls", (dropped, formed, len(out)), (None, True, 32))


def check_talk_rules() -> None:
    base = dict(
        open_talk=False, last_talk=None, talks_a=0, talks_b=0, entries_a=0, entries_b=0, age_a=30, age_b=30
    )
    planned = 100_000 + TALK_DELAY
    expect("talk plan: free pair", talk_conditions(planned, **base), True)
    expect("talk plan: open Talk blocks", talk_conditions(planned, **{**base, "open_talk": True}), False)
    expect(
        "talk run: open clause not checked",
        talk_conditions(planned, **{**base, "open_talk": True}, check_open=False),
        True,
    )
    expect("talk gap: 3599 s", talk_conditions(planned, **{**base, "last_talk": planned - 3599}), False)
    expect("talk gap: exactly 3600 s", talk_conditions(planned, **{**base, "last_talk": planned - 3600}), True)
    expect("talks per day: 7 ok", talk_conditions(planned, **{**base, "talks_b": 7}), True)
    expect("talks per day: 8 not", talk_conditions(planned, **{**base, "talks_a": 8}), False)
    expect("entries: 18 used -> 16 free", talk_conditions(planned, **{**base, "entries_a": 18}), True)
    expect("entries: 19 used -> 15 free", talk_conditions(planned, **{**base, "entries_b": 19}), False)
    expect("age 5 no talk", talk_conditions(planned, **{**base, "age_a": 5}), False)
    expect("age 6 talk", talk_conditions(planned, **{**base, "age_b": 6}), True)
    expect("house day: move-in 20:00, next 06:00", house_day(6 * 3600 + DAY, 20 * 3600), 1)
    expect("house day: move-in 20:00, 05:59 next day", house_day(6 * 3600 + DAY - 1, 20 * 3600), 0)
    expect("house day: second morning counts again", house_day(6 * 3600 + 2 * DAY, 20 * 3600), 2)
    expect("same second: action end", same_second_order("action_end")[0][:12], "run the Talk")
    expect("same second: death after the Talk", same_second_order("death")[-1], "death (class 900)")
    expect("same second: move-out cancels", len(same_second_order("move_out")), 1)

    # Topics, conditions, actors, outcomes.
    def pair(ea, eb, ia, ib, age_a=30, age_b=32, memory=False, gossip=False) -> TalkPair:
        return TalkPair(1, 2, age_a, age_b, ea, eb, ia, ib, memory, gossip)

    low = (400, 450, 400, 0, 200, 0)
    zeros = (0,) * 6
    p = pair(low, low, zeros, zeros)
    expect("topic: only Alltag/Necken at the housemate start", [topic_allowed(t, p) for t in range(6)],
           [True, False, True, False, False, False])
    expect("choose: all interests 0 -> smallest number", choose_topic(p), 0)
    ia = (10, 0, 500, 0, 0, 0)
    ib = (10, 0, 300, 0, 0, 0)
    expect("choose: largest sum", choose_topic(pair(low, low, ia, ib)), 2)
    expect("choose: a tie goes to the smallest number", choose_topic(pair(low, low, (500, 0, 500, 0, 0, 0), zeros)), 0)
    expect("Necken: someone from 60 forbids it", topic_allowed(2, pair(low, low, zeros, zeros, age_b=60)), False)
    expect("Necken: 59 and 59 allows it", topic_allowed(2, pair(low, low, zeros, zeros, 59, 59)), True)
    like = (500, 450, 400, 0, 200, 0)
    # Lob: the actor is the one with the larger interest; the condition reads his affection.
    expect("Lob: actor larger interest (b)", topic_actor(1, pair(like, low, (0, 100, 0, 0, 0, 0), (0, 200, 0, 0, 0, 0))), 2)
    expect("Lob: b actor, b's affection 400 -> not allowed", topic_allowed(1, pair(like, low, (0, 100, 0, 0, 0, 0), (0, 200, 0, 0, 0, 0))), False)
    expect("Lob: a actor on interest tie (smaller SimId)", topic_actor(1, pair(like, low, (0, 100, 0, 0, 0, 0), (0, 100, 0, 0, 0, 0))), 1)
    expect("Lob: a's affection 500 -> allowed", topic_allowed(1, pair(like, low, (0, 100, 0, 0, 0, 0), (0, 100, 0, 0, 0, 0))), True)
    expect("Lob outcome", topic_outcome(1, pair(like, low, (0, 100, 0, 0, 0, 0), (0, 100, 0, 0, 0, 0))),
           [(2, 1, {AFFECTION: 30, RESPECT: 20})])
    expect("Necken outcome (b teases)", topic_outcome(2, pair(low, low, (0, 0, 100, 0, 0, 0), (0, 0, 200, 0, 0, 0))),
           [(1, 2, {GRUDGE: 60})])
    expect("Alltag outcome", topic_outcome(0, p), [(1, 2, {FAMILIARITY: 40}), (2, 1, {FAMILIARITY: 40})])
    expect("Erinnern allowed only with a common memory", (topic_allowed(4, p), topic_allowed(4, pair(low, low, zeros, zeros, memory=True))), (False, True))
    expect("Erinnern outcome", topic_outcome(4, p), [(1, 2, {AFFECTION: 20}), (2, 1, {AFFECTION: 20})])
    expect("Klatsch allowed only with gossip", (topic_allowed(3, p), topic_allowed(3, pair(low, low, zeros, zeros, gossip=True))), (False, True))
    grudge = lambda g: (400, 450, 400, 0, 200, g)
    expect("Entschuldigung: grudge 299 on both sides -> no", topic_allowed(5, pair(grudge(299), grudge(299), zeros, zeros)), False)
    expect("Entschuldigung: grudge 300 on one side -> yes", topic_allowed(5, pair(grudge(300), grudge(0), zeros, zeros)), True)
    q = pair(grudge(300), grudge(0), zeros, zeros)  # a grudges b: b apologises
    expect("Entschuldigung: actor is the one grudged against", topic_actor(5, q), 2)
    expect("Entschuldigung outcome: grudger's edge", topic_outcome(5, q), [(1, 2, {GRUDGE: -200, TRUST: 60})])
    q2 = pair(grudge(500), grudge(400), zeros, zeros)  # both: the smaller grudge apologises
    expect("Entschuldigung: both grudges, smaller grudge acts", (topic_actor(5, q2), topic_outcome(5, q2)),
           (2, [(1, 2, {GRUDGE: -200, TRUST: 60})]))
    q3 = pair(grudge(400), grudge(400), zeros, zeros)
    expect("Entschuldigung: equal grudges, smaller SimId acts", topic_actor(5, q3), 1)
    expect("Entschuldigung: b grudges a", topic_outcome(5, pair(grudge(0), grudge(350), zeros, zeros)),
           [(2, 1, {GRUDGE: -200, TRUST: 60})])


def check_choice_bonus() -> None:
    e = lambda aff, trust, fam: (aff, trust, 400, 0, fam, 0)
    c = choice_bonus([5], {5: e(400, 450, 200)})
    expect("bonus: 600 exactly is company", (c.liked, c.company), (5, 8000))
    c = choice_bonus([5], {5: e(400, 450, 199)})
    expect("bonus: 599 is not", (c.liked, c.company, c.avoided, c.avoid), (None, 0, None, 0))
    c = choice_bonus([4, 5, 6], {4: e(400, 450, 300), 5: e(450, 450, 250), 6: e(500, 450, 100)})
    expect("liked: largest sum, then smaller SimId", c.liked, 4)
    c = choice_bonus([4, 5], {4: e(400, 450, 300), 5: e(500, 450, 300)})
    expect("liked: larger sum wins", c.liked, 5)
    c = choice_bonus([7, 8], {7: e(0, 399, 0), 8: e(0, 399, 0)})
    expect("avoided: 399 avoided, tie smaller SimId", (c.avoided, c.avoid, c.net), (7, 8000, -8000))
    c = choice_bonus([7, 8], {7: e(0, 400, 0), 8: e(0, 320, 0)})
    expect("avoided: 400 is not, smallest trust", (c.avoided, c.avoid), (8, 8000))
    c = choice_bonus([5, 9], {5: e(500, 450, 300), 9: e(0, 320, 0)})
    expect("company and avoid both count, sum 0", (c.liked, c.company, c.avoided, c.avoid, c.net), (5, 8000, 9, 8000, 0))
    expect("person without edge is skipped", choice_bonus([5], {}), Company(None, 0, None, 0))


def check_beliefs() -> None:
    k = tat_key(3, LAESTERT, 1, 4)
    # The heard chain
    expect("heard 1000 @600", heard_certainty(1000, 600), 733)
    expect("heard 733 @500", heard_certainty(733, 500), 488)
    expect("heard 488 @0", heard_certainty(488, 0), 162)
    expect("heard cap 900", heard_certainty(1000, 1000), 900)
    expect("lie 900 @600", heard_certainty(900, 600), 660)
    b1, kind = hear(None, k, None, 7, Belief(k, None, 1000, 0, None, None, GESEHEN), 1000, 600, 50)
    expect("hear: new", (b1.certainty, b1.source, b1.origin, b1.basis, b1.since, b1.secret, kind), (733, 7, 7, GEHOERT, 50, False, "new"))
    b2, _ = hear(None, k, None, 8, b1, b1.certainty, 500, 60)
    expect("hear: origin stays the first one's", (b2.certainty, b2.source, b2.origin), (488, 8, 7))
    b3, _ = hear(None, k, None, 9, b2, b2.certainty, 0, 70)
    expect("hear: 488 @0", (b3.certainty, b3.source, b3.origin), (162, 9, 7))
    lie, kind = hear(None, k, None, 7, None, 900, 600, 10)
    expect("lie: hearer holds 660, origin = liar", (lie.certainty, lie.origin, kind), (660, 7, "new"))
    # Confirmation
    held = Belief(k, None, 700, 5, 2, 2, GEHOERT)
    c, kind = hear(held, k, None, 9, b1, 733, 600, 99)
    expect("confirm +50", (c.certainty, c.source, c.origin, c.since, kind), (750, 2, 2, 99, "confirmed"))
    c, _ = hear(replace(held, certainty=880), k, None, 9, b1, 733, 600, 99)
    expect("confirm capped at 900", c.certainty, 900)
    c, kind = hear(replace(held, certainty=900), k, None, 9, b1, 733, 600, 99)
    expect("confirm at 900 changes nothing", (c, kind), (None, "unchanged"))
    c, kind = hear(Belief(k, None, 1000, 5, None, None, GESEHEN), k, None, 9, b1, 733, 600, 99)
    expect("a seen 1000 is not lowered", (c, kind), (None, "unchanged"))
    c, kind = hear(Belief(k, None, 0, 5, 2, 2, WIDERLEGT), k, None, 9, b1, 733, 600, 99)
    expect("widerlegt does not change by hearing", (c, kind), (None, "unchanged"))
    c, kind = hear(Belief((0, 1), 5, 700, 5, 2, 2, GEHOERT), (0, 1), 6, 9, None, 900, 600, 99)
    expect("another man: replaced", (c.man, c.certainty, kind), (6, 660, "replaced"))
    # Seen
    s, kind = see(None, k, 12)
    expect("seen: 1000, empty source/origin", (s.certainty, s.basis, s.source, s.origin, kind), (1000, GESEHEN, None, None, "new"))
    s, kind = see(b1, k, 12)
    expect("seen replaces heard", (s.certainty, s.basis, kind), (1000, GESEHEN, "replaced"))
    expect("seen leaves widerlegt", see(Belief(k, None, 0, 1, 2, 2, WIDERLEGT), k, 12), (None, "unchanged"))

    # The three lies in the chain of ADR-0031 acceptance 6: trust 600 -> 520 -> 440 -> 360
    bond = Bond(4, (500, 600, 400, 0, 300, 0), 0)
    for day, want in enumerate((520, 440, 360)):
        key = tat_key(4, LAESTERT, 1, day)  # Tat(son, lästert, grandmother, day)
        steps = heard_bond_steps(1, key)
        bond = bond_apply(bond, 0, steps)
        expect(f"trust after lie {day + 1}", bond.values[TRUST], want)
    expect("grudge +100 each", bond.values[GRUDGE], 300)
    expect("avoidance under 400", bond.values[TRUST] < AVOID_BELOW_TRUST, True)
    expect("steps: lügt against the hearer", heard_bond_steps(1, tat_key(4, LUEGT, 1, 0)), {TRUST: -80, GRUDGE: 100})
    expect("steps: neckt, even against the hearer", heard_bond_steps(1, tat_key(4, NECKT, 1, 0)), {TRUST: -20})
    expect("steps: lästert against another", heard_bond_steps(1, tat_key(4, LAESTERT, 9, 0)), {TRUST: -20})
    expect("steps: none for a VaterVon", heard_bond_steps(1, vater_key(5)), None)

    # Belief eviction
    def full_beliefs(over=None) -> Dict[tuple, Belief]:
        out = {}
        for d in range(32):
            kk = tat_key(50, NECKT, 51, d)
            out[kk] = Belief(kk, None, 500, 100 + d, 2, 2, GEHOERT)
        out.update(over or {})
        return out

    kk = lambda d: tat_key(50, NECKT, 51, d)
    new = Belief(tat_key(60, NECKT, 61, 0), None, 800, 1000, 2, 2, GEHOERT)
    full = full_beliefs({kk(5): Belief(kk(5), None, 100, 105, 2, 2, GEHOERT)})
    out, dropped, ok = insert_belief(full, new, set())
    expect("belief eviction: smallest certainty", (dropped, ok, len(out)), (kk(5), True, 32))
    full = full_beliefs({kk(5): Belief(kk(5), None, 100, 105, 2, 2, GEHOERT), kk(9): Belief(kk(9), None, 100, 99, 2, 2, GEHOERT)})
    out, dropped, ok = insert_belief(full, new, set())
    expect("belief eviction tie: older since", dropped, kk(9))
    full = full_beliefs({kk(5): Belief(kk(5), None, 100, 105, 2, 2, GEHOERT), kk(9): Belief(kk(9), None, 100, 105, 2, 2, GEHOERT)})
    out, dropped, ok = insert_belief(full, new, set())
    expect("belief eviction tie: smaller key", dropped, kk(5))
    weak = Belief(tat_key(60, NECKT, 61, 1), None, 50, 1000, 2, 2, GEHOERT)
    out, dropped, ok = insert_belief(full_beliefs(), weak, set())
    expect("belief: weaker newcomer not taken in", (dropped, ok), (None, False))
    full = full_beliefs({kk(5): Belief(kk(5), None, 100, 105, 2, 2, GEHOERT, secret=True)})
    out, dropped, ok = insert_belief(full, replace(new, certainty=0), set())
    expect("belief: a secret never falls", (dropped, ok), (None, False))
    vk = vater_key(77)
    full = full_beliefs({vk: Belief(vk, 5, 100, 5, None, None, BETEILIGT)})
    out, dropped, ok = insert_belief(full, new, {77})
    expect("belief: own child's VaterVon never falls", (vk in out, dropped == vk), (True, False))
    out, dropped, ok = insert_belief(full, new, set())
    expect("belief: another child's VaterVon can fall", dropped, vk)
    weak_secret = Belief(vater_key(78), 5, 10, 1000, None, None, BETEILIGT, secret=True)
    out, dropped, ok = insert_belief(full_beliefs(), weak_secret, set())
    expect("belief: a weak secret newcomer still enters", (ok, dropped is not None, vater_key(78) in out), (True, True, True))
    weak_own = Belief(vater_key(79), 6, 10, 1000, None, None, GESCHLOSSEN)
    out, dropped, ok = insert_belief(full_beliefs(), weak_own, {79})
    expect("belief: a weak own-child newcomer still enters", (ok, vater_key(79) in out), (True, True))
    allp = {kk(d): Belief(kk(d), None, 500, 100, 2, 2, GEHOERT, secret=True) for d in range(32)}
    out, dropped, ok = insert_belief(allp, new, set())
    expect("belief: only protected ones, newcomer not made", (dropped, ok), (None, False))

    # Truth window
    d = Deeds([])
    d.add(NECKT, 9, 3)
    expect("deed window: done", d.knows(NECKT, 9, 3, 10), True)
    expect("deed window: not done", d.knows(LAESTERT, 9, 3, 10), False)
    expect("deed window: day 29 days back is in", d.knows(NECKT, 9, 3, 32), True)
    expect("deed window: 30 days back is out", d.knows(NECKT, 9, 3, 33), None)
    expect("deed window: the future is out", d.knows(NECKT, 9, 11, 10), None)
    d = Deeds([])
    for i in range(65):
        d.add(NECKT, 9, i // 5)  # days 0..12, the first (day 0) falls at the 65th
    expect("deed cap: 64 kept, oldest fell", (len(d.items), d.dropped_day), (64, 0))
    expect("deed cap: the day of the fallen deed cannot be judged", d.knows(LAESTERT, 9, 0, 13), None)
    expect("deed cap: later days still judged", d.knows(LAESTERT, 9, 1, 13), False)

    # Gossip material, speaker, lie reason
    kx = tat_key(30, NECKT, 1, 2)  # a Tat of the third party 30
    giver = {kx: Belief(kx, None, 733, 5, 7, 7, GEHOERT)}
    expect("material: yes", [m.key for m in gossip_material(1, giver, 2, {}, set())], [kx])
    expect("material: doer seated at the furniture", gossip_material(1, giver, 2, {}, {30}), [])
    expect("material: doer is the hearer", gossip_material(1, giver, 30, {}, set()), [])
    expect("material: doer is the speaker", gossip_material(30, giver, 2, {}, set()), [])
    expect("material: secret", gossip_material(1, {kx: replace(giver[kx], secret=True)}, 2, {}, set()), [])
    expect("material: certainty 0", gossip_material(1, {kx: replace(giver[kx], certainty=0)}, 2, {}, set()), [])
    expect("material: hearer holds as surely", gossip_material(1, giver, 2, {kx: replace(giver[kx], certainty=733)}, set()), [])
    expect("material: hearer holds less surely", len(gossip_material(1, giver, 2, {kx: replace(giver[kx], certainty=732)}, set())), 1)
    expect(
        "material: the hearer holds it widerlegt (certainty 0), no material",
        gossip_material(1, giver, 2, {kx: Belief(kx, None, 0, 1, 5, 5, WIDERLEGT)}, set()),
        [],
    )
    expect("material: a widerlegt belief of the giver is no material either", gossip_material(1, {kx: Belief(kx, None, 0, 1, 5, 5, WIDERLEGT)}, 2, {}, set()), [])
    expect("material: VaterVon never", gossip_material(1, {vk: Belief(vk, 5, 1000, 5, None, None, BETEILIGT)}, 2, {}, set()), [])
    m1 = Belief(tat_key(30, NECKT, 1, 2), None, 900, 5, 7, 7, GEHOERT)
    m2 = Belief(tat_key(30, NECKT, 1, 1), None, 900, 5, 7, 7, GEHOERT)
    m3 = Belief(tat_key(30, NECKT, 1, 3), None, 500, 5, 7, 7, GEHOERT)
    expect("said: surest, then smaller key", said_belief([m1, m2, m3]).key, m2.key)
    expect("said: none", said_belief([]), None)
    expect("speaker: only one has material", choose_speaker(1, [m1], None, 2, [], None), 1)
    expect("speaker: only b has a reason", choose_speaker(1, [], None, 2, [], 5), 2)
    expect("speaker: nobody", choose_speaker(1, [], None, 2, [], None), None)
    expect("speaker: larger sum of certainties", choose_speaker(1, [m3], None, 2, [m1], None), 2)
    expect("speaker: tie, smaller SimId", choose_speaker(1, [m1], None, 2, [m2], None), 1)
    expect("speaker: reason without material counts 0 against material", choose_speaker(1, [], 40, 2, [m3], None), 2)
    expect("lie reason: grudge 700", lie_reason(1, 2, [1, 2, 3, 4], {3: 700}), 3)
    expect("lie reason: 699 is none", lie_reason(1, 2, [1, 2, 3, 4], {3: 699}), None)
    expect("lie reason: smallest SimId", lie_reason(1, 2, [1, 2, 3, 4], {4: 900, 3: 700}), 3)
    expect("lie reason: the hearer is not W", lie_reason(1, 2, [1, 2, 3, 4], {2: 900}), None)
    expect("lie statement", lie_statement(3, 2, 6), (tat_key(3, LAESTERT, 2, 6), 900))

    # Refutation
    truth = lambda child: 20
    kf = tat_key(3, LAESTERT, 1, 4)  # false: 3 did not gossip about 1 on day 4
    false_belief = Belief(kf, None, 660, 10, 5, 3, GEHOERT)  # source 5, origin 3 (a lie of 3?) see below
    deeds3 = Deeds([])
    deeds_none = Deeds([])
    expect("refute: the alleged doer can", can_refute(3, {}, deeds3, false_belief, truth, 6), True)
    expect("refute: a stranger cannot", can_refute(9, {}, deeds_none, false_belief, truth, 6), False)
    deeds3.add(LAESTERT, 1, 4)
    expect("refute: a true statement never", can_refute(3, {}, deeds3, false_belief, truth, 6), False)
    expect("refute: doer out of window cannot", can_refute(3, {}, Deeds([]), false_belief, truth, 40), False)
    expect(
        "refute: someone holding it widerlegt can",
        can_refute(9, {kf: Belief(kf, None, 0, 1, 2, 2, WIDERLEGT)}, deeds_none, false_belief, truth, 6),
        True,
    )
    expect(
        "refute: a holder cannot refute a deed the doer did",
        can_refute(9, {kf: Belief(kf, None, 0, 1, 2, 2, WIDERLEGT)}, deeds3, false_belief, truth, 6),
        False,
    )
    expect(
        "refute: a holder cannot refute a day outside the window",
        can_refute(9, {kf: Belief(kf, None, 0, 1, 2, 2, WIDERLEGT)}, Deeds([]), false_belief, truth, 40),
        False,
    )
    expect("refute: not an already widerlegt belief", can_refute(3, {}, Deeds([]), replace(false_belief, basis=WIDERLEGT, certainty=0), truth, 6), False)
    expect("refute: not certainty 0", can_refute(3, {}, Deeds([]), replace(false_belief, certainty=0), truth, 6), False)
    expect("refute: not a secret", can_refute(3, {}, Deeds([]), replace(false_belief, secret=True), truth, 6), False)
    vb = Belief(vater_key(7), 21, 900, 1, None, None, GESCHLOSSEN)
    holds = {vater_key(7): Belief(vater_key(7), 20, 1000, 1, None, None, BETEILIGT)}
    expect("refute VaterVon: holder of the true man with 1000", can_refute(40, holds, Deeds([]), vb, truth, 6), True)
    expect("refute VaterVon: not with a secret", can_refute(40, {vater_key(7): replace(holds[vater_key(7)], secret=True)}, Deeds([]), vb, truth, 6), False)
    expect("refute VaterVon: not with less than 1000", can_refute(40, {vater_key(7): replace(holds[vater_key(7)], certainty=900)}, Deeds([]), vb, truth, 6), False)
    expect("refute VaterVon: a true belief never", can_refute(40, holds, Deeds([]), replace(vb, man=20), truth, 6), False)

    # pick: a first, then b, smallest key; and effects
    ka1, ka2 = tat_key(2, LAESTERT, 1, 1), tat_key(2, LAESTERT, 1, 2)
    hb = {ka2: Belief(ka2, None, 600, 1, 5, 2, GEHOERT), ka1: Belief(ka1, None, 600, 1, 5, 2, GEHOERT)}
    # a=1 refutes nothing; b=2 is the alleged doer of both and refutes for the smallest key.
    expect("pick: smallest key of the second refuter", pick_refutation(1, hb, Deeds([]), 2, {}, Deeds([]), truth, 5), (2, 1, ka1))
    expect("pick: nothing", pick_refutation(1, {}, Deeds([]), 2, {}, Deeds([]), truth, 5), None)
    pa = {tat_key(2, LAESTERT, 1, 1): Belief(tat_key(2, LAESTERT, 1, 1), None, 600, 1, 5, 2, GEHOERT)}  # held by a, alleged doer b
    pb = {tat_key(1, NECKT, 2, 1): Belief(tat_key(1, NECKT, 2, 1), None, 600, 1, 5, 1, GEHOERT)}  # held by b, alleged doer a
    expect(
        "pick: a refutes first when both could",
        pick_refutation(1, pa, Deeds([]), 2, pb, Deeds([]), truth, 5),
        (1, 2, tat_key(1, NECKT, 2, 1)),
    )
    expect(
        "pick: b when only b can",
        pick_refutation(1, pa, Deeds([]), 2, {}, Deeds([]), truth, 5),
        (2, 1, tat_key(2, LAESTERT, 1, 1)),
    )
    held_off = {kf: Belief(kf, None, 0, 1, 2, 2, WIDERLEGT)}
    expect(
        "pick: a holder refutes a false deed in the window",
        pick_refutation(9, held_off, Deeds([]), 1, {kf: false_belief}, Deeds([]), truth, 6),
        (9, 1, kf),
    )
    expect(
        "pick: a holder cannot refute a deed the doer did",
        pick_refutation(
            9, held_off, Deeds([]), 1, {kf: false_belief}, Deeds([]), truth, 6,
            deeds_of=lambda sim: deeds3 if sim == 3 else Deeds([]),
        ),
        None,
    )
    expect(
        "pick: a holder cannot refute a day outside the window",
        pick_refutation(9, held_off, Deeds([]), 1, {kf: false_belief}, Deeds([]), truth, 40),
        None,
    )
    # Effects: trust -120 in the source; grudge +150 if the source is the origin of the lie.
    lie_held = Belief(kf, None, 660, 10, 3, 3, GEHOERT)  # source 3 = origin 3, a liar's sentence
    new, source, steps = refute(lie_held, 9, None, 77)
    expect("refute effects: lie origin", (new.certainty, new.basis, new.since, new.source, new.origin, new.secret, source, steps),
           (0, WIDERLEGT, 77, 9, 9, False, 3, {TRUST: -120, GRUDGE: 150}))
    second_hand = Belief(kf, None, 488, 10, 5, 3, GEHOERT)  # source 5 passed on the lie of 3
    new, source, steps = refute(second_hand, 9, None, 77)
    expect("refute effects: passed on, only trust", (source, steps), (5, {TRUST: -120}))
    new, source, steps = refute(vb, 40, 20, 77)
    expect("refute effects: VaterVon, no source, no step", (new.man, new.certainty, new.basis, new.secret, new.since, new.source, new.origin, source, steps),
           (20, 1000, WIDERLEGT, False, 77, 40, 40, None, None))
    bond = bond_apply(Bond(3, (500, 600, 400, 0, 300, 100), 0), 0, {TRUST: -120, GRUDGE: 150})
    expect("refute applied to an edge", (bond.values[TRUST], bond.values[GRUDGE]), (480, 250))
    expect("draw rule: lie below 50000", (lies(49_999), lies(50_000)), (True, False))
    expect("draw rule: refute below 500000", (refutes(499_999), refutes(500_000)), (True, False))


def check_paternity() -> None:
    mother, child, f, p = 10, 99, 20, 30
    hh = [(10, 32), (30, 35), (40, 7), (41, 4), (50, 60)]
    out = paternity_beliefs(mother, child, f, True, p, hh, 1000)
    summary = [(h, b.man, b.certainty, b.basis, b.secret) for h, b in out]
    expect(
        "paternity with P (F outside, siblings 7 and 4): the 4-year-old gets nothing",
        summary,
        [
            (10, 20, 1000, BETEILIGT, True),
            (20, 20, 1000, BETEILIGT, False),
            (30, 30, 900, GESCHLOSSEN, False),
            (40, 30, 900, GESCHLOSSEN, False),
            (50, 30, 900, GESCHLOSSEN, False),
        ],
    )
    expect("paternity: no source/origin, key is the child", {(b.source, b.origin, b.key) for _, b in out}, {(None, None, vater_key(99))})
    out = paternity_beliefs(mother, child, f, True, None, hh, 1000)
    expect(
        "paternity without P: the house names F, nothing secret",
        [(h, b.man, b.certainty, b.basis, b.secret) for h, b in out],
        [
            (10, 20, 1000, BETEILIGT, False),
            (20, 20, 1000, BETEILIGT, False),
            (30, 20, 900, GESCHLOSSEN, False),
            (40, 20, 900, GESCHLOSSEN, False),
            (50, 20, 900, GESCHLOSSEN, False),
        ],
    )
    out = paternity_beliefs(mother, child, f, True, f, hh + [(20, 33)], 1000)
    expect(
        "paternity P = F (also in the house): one sentence each, not secret, no 900 for P",
        [(h, b.man, b.certainty, b.basis, b.secret) for h, b in out],
        [
            (10, 20, 1000, BETEILIGT, False),
            (20, 20, 1000, BETEILIGT, False),
            (30, 20, 900, GESCHLOSSEN, False),
            (40, 20, 900, GESCHLOSSEN, False),
            (50, 20, 900, GESCHLOSSEN, False),
        ],
    )
    out = paternity_beliefs(mother, child, f, False, p, hh, 1000)
    expect("paternity: F not a person of the town, no sentence for him", [h for h, _ in out], [10, 30, 40, 50])
    expect("paternity: mother's sentence kept, secret", out[0][1].secret, True)
    out = paternity_beliefs(mother, child, f, True, p, hh + [(20, 40)], 1000)
    expect("paternity: F living in the house keeps rule 2 only", [(h, b.certainty, b.basis) for h, b in out if h == 20], [(20, 1000, BETEILIGT)])
    out = paternity_beliefs(mother, child, f, True, p, hh + [(99, 0)], 1000)
    expect("paternity: the child itself gets nothing", [h for h, _ in out if h == 99], [])


def check_feelings() -> None:
    # div0 truncates toward zero (Rust), unlike Python's //
    expect("div0: toward zero", (div0(-37, 4), div0(-150, 4), div0(150, 4), div0(-3, 2), div0(3, 2), div0(-1, 2), div0(7, -2)), (-9, -37, 37, -1, 1, 0, -3))

    # Acceptance 1: joy 800 at 300 per day is 0 at 800 * 86400 / 300 = 230400, 1 one second before.
    expect("1: joy 800 at 230400", channel_at(800, 300, 0, 230_400), 0)
    expect("1: joy 800 one second before", channel_at(800, 300, 0, 230_399), 1)
    expect("1: 230400 is 800 * 86400 / 300", 800 * DAY // 300, 230_400)
    expect("channel: no fade before since (max(0, t - since))", channel_at(800, 300, 500, 100), 800)
    expect("channel: shifted since", channel_at(800, 300, 1000, 1000 + 230_400), 0)
    expect("channel: never below 0 long after", channel_at(800, 300, 0, 400 * DAY), 0)
    expect("channel: rates sadness 150 and fear 400 (day 1)", (channel_at(500, 150, 0, DAY), channel_at(500, 400, 0, DAY)), (350, 100))
    expect("channel: truncation (rate 400, 100 s)", channel_at(500, 400, 0, 100), 500 - 0)  # 40000 / 86400 = 0
    expect("channel: truncation (rate 400, 216 s)", channel_at(500, 400, 0, 216), 500 - 1)  # 86400 / 86400 = 1

    # Acceptance 2: birth, joy +800 and temper +200; temper 1 one second before 2 days, 0 after.
    mother = Feel()
    mother2, changed, delta = apply_occasion(mother, 0, rows_for(10, effect_birth([10, 11]), lambda s, o: None))
    expect("2: birth joy and temper", (mother2.values, mother2.temper, mother2.since, changed, delta), ((800, 0, 0, 0), 200, 0, True, (800, 0, 0, 0)))
    expect("2: temper one second before two days", temper_at(200, 0, 2 * DAY - 1), 1)
    expect("2: temper at two days", temper_at(200, 0, 2 * DAY), 0)
    expect("2: temper after two days", temper_at(200, 0, 2 * DAY + 5000), 0)
    expect("2: through feel_at", feel_at(mother2, 2 * DAY - 1)[1], 1)
    expect("2: birth has no band", [e[2] for e in effect_birth([10, 11])], [None, None])

    # Acceptance 3
    lob, _, d = apply_occasion(Feel(), 100, rows_for(1, effect_talk_lob(1, 2), lambda s, o: 640))
    expect("3: praise with affection 640: joy +225", (lob.values[JOY], d[JOY]), (225, 225))
    expect("3: praise temper (225 / 4 = 56)", lob.temper, 56)
    stored = Feel((0, 500, 0, 0), 0, 0)
    ap, _, d = apply_occasion(stored, 0, rows_for(1, effect_talk_entschuldigung(1, 2), lambda s, o: 600))
    expect("3: apology in the band: anger -300, joy +75", (d[ANGER], d[JOY], ap.values), (-300, 75, (75, 200, 0, 0)))
    expect("3: apology temper (75 + 300) / 4 = 93", ap.temper, 93)
    ap, _, d = apply_occasion(stored, 0, rows_for(1, effect_talk_entschuldigung(1, 2), lambda s, o: 599))
    expect("3: apology just outside the band: -200, +50", (d[ANGER], d[JOY]), (-200, 50))
    ang, _, d = apply_occasion(Feel(), 5, [((0, 150, 0, 0), None)])
    expect("3: anger +150 alone: temper -37", (ang.values, ang.temper), ((0, 150, 0, 0), -37))
    talk, _, d = apply_occasion(Feel(), 5, [((50, 0, 0, 0), None), ((0, 0, 100, 0), None)])
    expect("3: Talk with joy +50 and sadness +100: temper -12 (summed, divided once)", (talk.values, talk.temper), ((50, 0, 100, 0), -12))
    t1, _, _ = apply_occasion(Feel(), 5, [((50, 0, 0, 0), None)])
    t2, _, _ = apply_occasion(t1, 5, [((0, 0, 100, 0), None)])
    expect("3: the same rows one by one would give -13 (12 and -25)", t2.temper, 12 - 25)

    # Band edges: threshold, negative steps toward 0, per row before the sum
    expect("band: 600 yes, 599 no", (band_step(150, 600), band_step(150, 599)), (225, 150))
    expect("band: no band person", band_step(150, None), 150)
    expect("band: negative toward 0 (-200 -> -300, -1 -> -1, 1 -> 1)", (band_step(-200, 700), band_step(-1, 700), band_step(1, 700)), (-300, -1, 1))
    _, _, d = apply_occasion(Feel(), 0, [((150, 0, 0, 0), 700), ((40, 0, 0, 0), None)])
    expect("band: applied per row before the sum (225 + 40)", d[JOY], 265)
    _, _, d = apply_occasion(Feel(), 0, [((1, 0, 0, 0), 700), ((1, 0, 0, 0), 700)])
    expect("band: truncation per row (1 stays 1, not 3/2 of the sum 2 = 3)", d[JOY], 2)

    # Acceptance 4: contagion. The push itself takes the room; whether the walk entered it is separate.
    expect("4: sofa to television in the same room is not an entering", enters(1, 1), False)
    expect("4: living to kitchen is an entering", enters(1, 2), True)
    expect("4: coming home from outside is an entering", enters(None, 0), True)
    room = [(7, 30, 650), (5, 30, 0), (9, 30, 800), (3, 30, 720)]
    out = contagion(3, 720, room)
    expect("4: A 150, B 699, C 800 in SimId order, the arriver is not in it", out, [(5, 150, 150), (7, 699, 49), (9, 800, 0)])
    expect("4: the arriver stays 720 (not touched)", [o for o in out if o[0] == 3], [])
    expect("4: B (now 699) does not infect anyone on arrival", contagion(7, 699, [(5, 30, 150), (9, 30, 800)]), [])
    expect("4: threshold 700 infects", contagion(3, 700, [(5, 30, 0)]), [(5, 150, 150)])
    expect("4: 699 below 700 stays 699 (cap)", contagion(3, 900, [(5, 30, 699)]), [(5, 699, 0)])
    expect("4: 550 + 150 = 700 is capped at 699", contagion(3, 900, [(5, 30, 550)]), [(5, 699, 149)])
    expect(
        "4: cap 600 does not lower 650",
        contagion(3, 900, [(5, 30, 650)], min_anger=700, step=150, cap=600),
        [(5, 650, 0)],
    )
    expect("4: at 700 or above unchanged, never lowered", contagion(3, 900, [(5, 30, 700), (6, 30, 1000)]), [(5, 700, 0), (6, 1000, 0)])
    expect("4: a child under 6 is not infected", contagion(3, 900, [(4, 5, 0), (5, 6, 0)]), [(5, 150, 150)])
    expect("4: the temper follows the delta (150 -> -37, 49 -> -12)", [apply_occasion(Feel((0, a, 0, 0), 0, 0), 0, [((0, dl, 0, 0), None)])[0].temper for _, a, dl in [(5, 0, 150), (7, 650, 49)]], [-37, -12])
    # Unchanged person: no write, since stays
    c_state = Feel((0, 800, 0, 0), -300, 40)
    same, changed, _ = apply_occasion(c_state, 40, [((0, 0, 0, 0), None)])
    expect("4: a delta 0 writes nothing", (same is c_state, changed), (True, False))

    # Acceptance 5: scaled company
    expect("5: temper -500 halves company", scaled_company(8000, -500), 4000)
    expect("5: temper -1 gives 7992 (toward 0)", scaled_company(8000, -1), 7992)
    expect("5: temper 0 unchanged", scaled_company(8000, 0), 8000)
    expect("5: temper -1000 removes it, +1000 doubles it", (scaled_company(8000, -1000), scaled_company(8000, 1000)), (0, 16000))
    expect("5: truncation toward 0 (company 1, temper 500)", scaled_company(1, 500), 1)
    expect("5: truncation toward 0 (company -1, temper -500)", scaled_company(-1, -500), 0)
    expect("5: company 0 stays 0", scaled_company(0, 700), 0)

    # Acceptance 6: partner death with affection 700
    eff = effect_death(dead=20, partner=10, kin_eligible=[10, 11])
    expect("6: one row for the survivor even if kin, the kin row for the other", [(e[0], e[1]) for e in eff], [(10, STEPS_PARTNER_DEATH), (11, STEPS_KIN_DEATH)])
    survivor, _, d = apply_occasion(Feel(), 3000, rows_for(10, eff, lambda s, o: 700 if (s, o) == (10, 20) else None))
    expect("6: sadness 1000 (clamped from 1500), fear +300", (survivor.values, d), ((0, 0, 1000, 300), (0, 0, 1000, 300)))
    expect("6: temper (0 - 0 - 1000 - 300) / 4 = -325", survivor.temper, -325)
    kin, _, d = apply_occasion(Feel(), 3000, rows_for(11, eff, lambda s, o: 500))
    expect("6: kin row without band", d, (0, 0, 800, 100))
    kin, _, d = apply_occasion(Feel(), 3000, rows_for(11, eff, lambda s, o: 600))
    expect("6: kin row in the band: 1200 clamped to 1000, fear 150", d, (0, 0, 1000, 150))
    expect("6: no partner: only kin rows", [e[0] for e in effect_death(20, None, [10, 11])], [10, 11])

    # The table
    expect("table: Alltag both, band to the other", effect_talk_alltag(1, 2), [(1, (40, 0, 0, 0), 2), (2, (40, 0, 0, 0), 1)])
    expect("table: Lob, the praised, band to the praiser", effect_talk_lob(1, 2), [(1, (150, 0, 0, 0), 2)])
    expect("table: Necken, the teased, band to the teaser", effect_talk_necken(1, 2), [(1, (0, 100, 0, 0), 2)])
    expect("table: Erinnern both", effect_talk_erinnern(1, 2), [(1, (100, 0, 0, 0), 2), (2, (100, 0, 0, 0), 1)])
    expect("table: Entschuldigung, the grudger only", effect_talk_entschuldigung(1, 2), [(1, (50, -200, 0, 0), 2)])
    expect("table: heard deed, the hearer, band to W", effect_heard(1, 9), [(1, (0, 200, 100, 0), 9)])
    expect("table: refutation with and without a source", (effect_refutation(1, 4), effect_refutation(1, None)), ([(1, (0, 150, 0, 0), 4)], [(1, (0, 150, 0, 0), None)]))
    expect("table: partnership and separation, no band", (effect_partnership(1, 2), effect_separation(1, 2)), ([(1, (600, 0, 0, 0), None), (2, (600, 0, 0, 0), None)], [(1, (0, 300, 600, 0), None), (2, (0, 300, 600, 0), None)]))
    expect("table: need_empty once per settle however many needs", (effect_need_empty(1, [0, 2, 3]), effect_need_empty(1, [2]), effect_need_empty(1, [])), ([(1, (0, 150, 0, 100), None)], [(1, (0, 150, 0, 100), None)], []))
    expect("table: need_empty temper (-150 - 100) / 4 = -62", apply_occasion(Feel(), 0, rows_for(1, effect_need_empty(1, [0, 2]), lambda s, o: None))[0].temper, -62)
    expect("table: separation temper (-300 - 600) / 4 = -225", apply_occasion(Feel(), 0, rows_for(1, effect_separation(1, 2), lambda s, o: None))[0].temper, -225)
    expect("table: partnership temper 600 / 4 = 150", apply_occasion(Feel(), 0, rows_for(1, effect_partnership(1, 2), lambda s, o: None))[0].temper, 150)
    expect("rows_for: only the person's own effects", rows_for(2, effect_talk_alltag(1, 2), lambda s, o: 700), [((40, 0, 0, 0), 700)])

    # Edge cases: temper return keeps the sign
    expect("temper: negative returns toward 0 keeping the sign", (temper_at(-500, 0, DAY), temper_at(-500, 0, 4 * DAY), temper_at(-500, 0, 5 * DAY), temper_at(-500, 0, 400 * DAY)), (-400, -100, 0, 0))
    expect("temper: positive returns", (temper_at(500, 0, DAY), temper_at(500, 0, 5 * DAY)), (400, 0))
    expect("temper: -37 truncated at 21600 s (25 per quarter day)", (temper_at(-37, 0, 21_600), temper_at(-37, 0, 43_200)), (-12, 0))
    expect("temper: custom mood_return 10000 per day", temper_at(1000, 0, 8640, 10_000), 0)
    expect("temper: no change before since", temper_at(-200, 100, 50), -200)

    # Clamping
    full = Feel((1000, 0, 0, 0), 0, 0)
    same, changed, d = apply_occasion(full, 0, [((800, 0, 0, 0), None)])
    expect("clamp: joy at 1000 plus 800 is no change, no write", (same is full, changed, d), (True, False, (0, 0, 0, 0)))
    nine, changed, d = apply_occasion(Feel((900, 0, 0, 0), 0, 0), 0, [((800, 0, 0, 0), None)])
    expect("clamp: joy 900 + 800 clamps to 1000, delta 100, temper 25", (nine.values[JOY], d[JOY], nine.temper), (1000, 100, 25))
    ap, changed, d = apply_occasion(Feel((0, 500, 0, 0), 0, 0), DAY, rows_for(1, effect_talk_entschuldigung(1, 2), lambda s, o: 700))
    expect("clamp: anger 500 decays to 200 in a day, -300 clamps to 0, delta -200 (not the step), temper (75 + 200) / 4 = 68",
           (ap.values, d, ap.temper, ap.since), ((75, 0, 0, 0), (75, -200, 0, 0), 68, DAY))
    hot, _, _ = apply_occasion(Feel((0, 0, 0, 0), 900, 0), 0, [((800, 0, 0, 0), None)])
    expect("clamp: temper 900 + 200 clamps to 1000", hot.temper, 1000)
    cold, _, _ = apply_occasion(Feel((0, 0, 0, 0), -900, 0), 0, [((0, 1000, 0, 0), None)])
    expect("clamp: temper -900 - 250 clamps to -1000", cold.temper, -1000)
    low, changed, _ = apply_occasion(Feel(), 0, [((0, -200, 0, 0), None)])
    expect("clamp: anger at 0 minus 200 is no change", (low, changed), (Feel(), False))

    # Reads use the analytic value at `now`; a write resets since
    aged = Feel((800, 0, 0, 0), 100, 1000)
    new, changed, d = apply_occasion(aged, 1000 + 86_400, [((0, 0, 200, 0), None)])
    expect("analytic read at now: joy 800 -> 500 after a day, sadness +200, temper 100 -> 0 after a day, then sadness -50",
           (new.values, new.temper, new.since, d), ((500, 0, 200, 0), -50, 1000 + 86_400, (0, 0, 200, 0)))
    nochange, changed, _ = apply_occasion(aged, 1000 + 86_400, [((0, 0, 0, 0), None)])
    expect("no-change occasion keeps the old values and since (even after a day of decay)", (nochange, changed), (aged, False))
    chan, changed, d = apply_occasion(Feel((4, 4, 0, 0), 0, 0), 0, [((4, 4, 0, 0), None)])
    expect("channels change but the temper step is 0: still a write with since = now", (chan.values, chan.temper, changed), ((8, 8, 0, 0), 0, True))
    nowrite, changed, _ = apply_occasion(Feel((0, 0, 3, 0), 0, 7), 99, [((0, 0, 0, 0), None)])
    expect("empty rows: nothing changes, since stays 7", (nowrite.since, changed), (7, False))


def check_all() -> None:
    check_memory()
    check_bonds()
    check_talk_rules()
    check_choice_bonus()
    check_beliefs()
    check_paternity()
    check_feelings()
    print(f"{CHECKS} checks passed")


# ---------------------------------------------------------------------------------------
# Pinned values for the Rust tests


def print_pins() -> None:
    seed = 0x5EED
    tag = "test:social"
    keys = [founder_key(tag, i) for i in range(4)]
    print()
    print(f"pins: world seed {seed:#x}, founder tag {tag!r}")
    for i, k in enumerate(keys):
        print(f"founder {i} lineage_key {k:016x}")
    print("interests (TALK_INTEREST, subject lineage_key, occasion topic, detail 0):")
    interests = []
    for i, k in enumerate(keys):
        row = tuple(interest(seed, k, t) for t in range(6))
        interests.append(row)
        print(f"  founder {i}: " + " ".join(f"{TOPIC_NAMES[t]}={v}" for t, v in enumerate(row)))
    for x, y in ((0, 1), (1, 3), (2, 3)):
        for label, ea, eb, memory, gossip in (
            ("housemate start edges", START_VALUES["housemate"], START_VALUES["housemate"], False, False),
            ("partner start edges", START_VALUES["partner"], START_VALUES["partner"], False, False),
            ("partners, common memory and gossip", START_VALUES["partner"], START_VALUES["partner"], True, True),
            ("a grudges b (400)", (400, 450, 400, 0, 200, 400), START_VALUES["housemate"], False, False),
        ):
            p = TalkPair(1, 2, 30, 32, ea, eb, interests[x], interests[y], memory, gossip)
            allowed = [TOPIC_NAMES[t] for t in range(6) if topic_allowed(t, p)]
            topic = choose_topic(p)
            actor = topic_actor(topic, p)
            sums = [p.interest_a[t] + p.interest_b[t] for t in range(6)]
            who = None if actor is None else f"founder {x if actor == 1 else y}"
            print(
                f"  topic, a=founder {x}, b=founder {y}, ages 30/32, {label}: allowed {allowed},"
                f" sums {sums}, chosen {topic} {TOPIC_NAMES[topic]}, actor {who}"
            )
    print("TALK_LIE (u in 0..999999, lie if < 50000) and TALK_REFUTE (refute if < 500000),")
    print("seed 0x5EED, occasion = second, subject = speaker/refuter, detail = hearer:")
    for second in (6 * 3600, 6 * 3600 + 3600, 2 * DAY + 18 * 3600):
        for s_i, h_i in ((0, 1), (1, 0), (2, 3), (3, 2)):
            ul = lie_draw(seed, keys[s_i], second, keys[h_i])
            ur = refute_draw(seed, keys[s_i], second, keys[h_i])
            print(
                f"  second {second:>7} founder {s_i}->{h_i}: lie u={ul:>6} {'LIE' if lies(ul) else 'no  '}"
                f"  refute u={ur:>6} {'REFUTE' if refutes(ur) else 'no    '}"
            )

    print("first second (a multiple of 600 from 06:00) with a hit, for founder 0 -> 1:")
    lie_hit = next(t for t in range(6 * 3600, 400 * DAY, 600) if lies(lie_draw(seed, keys[0], t, keys[1])))
    ref_miss = next(t for t in range(6 * 3600, 400 * DAY, 600) if not refutes(refute_draw(seed, keys[0], t, keys[1])))
    print(f"  lie hit at second {lie_hit} (u={lie_draw(seed, keys[0], lie_hit, keys[1])})")
    print(f"  first refute miss at second {ref_miss} (u={refute_draw(seed, keys[0], ref_miss, keys[1])})")

if __name__ == "__main__":
    if "--check" in sys.argv[1:]:
        check_all()
    print_pins()

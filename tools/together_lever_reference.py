#!/usr/bin/env python3
"""The lever test of "Zeit zu zweit" (ADR-0023, addendum of 2026-11-01, "Hebeltests"; Grok's
rounds 43, 44 and 48, R4 and W1), written from the ADR, independent of the Rust code. The
model is tools/world_reference.py, the draws tools/rng_reference.py.

The scenario tests/scenarios/dummy.json on the repository's data as the CLI runs it
(`golden_reference.make_world` on `wr.REPOSITORY`, with the template `core:time_together`:
three month starts, rate 0.30). An attempt at the month start `m` conceives iff its draw `u`
(purpose tag CONCEPTION_ATTEMPT, her key, `m / MONTH`, his key) is below its chance
`(rate · F_w) · F_m` (genetics specification 11.3). The lever is the first attempt of the twin
(the run without the input) with `(0.20 · F_w) · F_m <= u < (0.30 · F_w) · F_m`: it fails in
the twin and conceives in a month start of a window. Its month start `m` is also the first
for which the attempts of her couple in the three month starts before it do not conceive at
the boosted rate either, so every variant below runs as the twin until `m`, and the first
with `m >= 4 MONTH`, so every stamp below is a second of the run after `Start`.

Variants, each the input `TimeTogether { woman, man }` stamped at the given second (it acts
one second later; W1: the window is the three month starts strictly after that second):
- "first", stamp `m − MONTH`: the window `m` to `m + 2 MONTH`, `m` its first; conceives at `m`.
- "last", stamp `m − 3 MONTH`: the window `m − 2 MONTH` to `m`, `m` its last; conceives at `m`.
- "fourth", stamp `m − 4 MONTH`: the window `m − 3 MONTH` to `m − MONTH`; `m` is the fourth
  month start and has the adult rate, no conception at `m`.
- "own_second", stamp `m − 1`: it acts in `m` itself, the window is `m + MONTH` to
  `m + 3 MONTH`; no conception at `m`.
- "again", stamps `m − 4 MONTH` and `m − 2 MONTH`: the second click moves the window to
  `m − MONTH` to `m + MONTH` (no stacking); conceives at `m`.

Usage: $HOME/.cache/simagent-tools-venv/bin/python together_lever_reference.py   (from tools/)
"""

import sys

import golden_reference as gr
import world_reference as wr
from rng_reference import bernoulli, chacha_key, unit, words
from world_reference import MONTH, YEAR

SCENARIO = gr.SCENARIOS / "dummy.json"
ADULT = None
BOOST = None
VARIANTS = {
    "first": ([-MONTH], True),
    "last": ([-3 * MONTH], True),
    "fourth": ([-4 * MONTH], False),
    "own_second": ([-1], False),
    "again": ([-4 * MONTH, -2 * MONTH], True),
}


def twin_attempts(s: dict) -> list:
    """Every attempt of the twin to `until`: (m, woman, man, F_w, F_m, u, chance, boosted)."""
    rows = []
    original = wr.World._attempt

    def recording(self, event, now, plan):
        woman = event.subject[1]
        man = self.partner_of(woman)
        f_w, f_m = self.factors(woman, man, now)
        word = words(chacha_key(self.seed, wr.CONCEPTION_ATTEMPT, self.sims[woman][1], now // MONTH, self.sims[man][1]))[0]
        rows.append((now, woman, man, f_w, f_m, word, wr.chance(ADULT, f_w, f_m), wr.chance(BOOST, f_w, f_m)))
        original(self, event, now, plan)

    wr.World._attempt = recording
    try:
        world = gr.make_world(s, wr.REPOSITORY)
        world.advance(s["until"])
    finally:
        wr.World._attempt = original
    assert [(m, woman, man, bernoulli(word, p)) for m, woman, man, _, _, word, p, _ in rows] == [(m, woman, man, ok) for m, woman, man, _, ok in world.attempts]
    return rows


def lever(rows: list):
    """The first attempt from `4 MONTH` on that fails at the adult rate and conceives at the
    boost, whose couple has no attempt in the three month starts before it that conceives at
    the boost."""
    for m, woman, man, f_w, f_m, word, p, boosted in rows:
        if m < 4 * MONTH or bernoulli(word, p) or not bernoulli(word, boosted):
            continue
        before = [row for row in rows if row[1] == woman and m - 3 * MONTH <= row[0] < m]
        if any(bernoulli(row[5], row[7]) for row in before):
            print(f"(a) candidate m {m} woman {woman} man {man}: an attempt before it conceives at the boost {[row[0] for row in before if bernoulli(row[5], row[7])]}")
            continue
        return m, woman, man, f_w, f_m, word, p, boosted, before
    return None


def variant(s: dict, stamps: list, woman: int, man: int, m: int):
    """The run with the inputs at `stamps` (ascending), to `m`: (`m` if her attempt at `m`
    conceives, else None; her window; the entries of the inputs)."""
    world = gr.make_world(s, wr.REPOSITORY)
    entries = []
    for stamp in stamps:
        world.advance(stamp)
        assert world.time_together_rejection(woman, man) is None
        first = world.next_event_id
        world.time_together(woman, man)
        entries += world.log[first - 1 : world.next_event_id - 1]
    window = world.together[woman]
    world.advance(m)
    gr.check_all(world)
    (attempt,) = [row for row in world.attempts if row[1] == woman and row[0] == m]
    return (m if attempt[4] else None), window, entries, world


def main(argv) -> int:
    global ADULT, BOOST
    ADULT = wr.REPOSITORY.adult_rate
    template = wr.REPOSITORY.shocks[wr.TIME_TOGETHER_TEMPLATE]
    assert template["kind"] == "fertility_boost" and template["months"] == 3, template
    BOOST = float(template["rate"])
    print(f"data data_hash {wr.REPOSITORY.hash:016x} adult_rate {ADULT!r} boost {BOOST!r} months {template['months']} core_version {wr.CORE_VERSION}")
    s = gr.scenario(SCENARIO)
    rows = twin_attempts(s)
    print(f"(a) twin attempts {len(rows)} conceived {sum(bernoulli(row[5], row[6]) for row in rows)}")
    found = lever(rows)
    if found is None:
        print("(a) no lever in the dummy")
        return 1
    m, woman, man, f_w, f_m, word, p, boosted, before = found
    print(f"(a) lever m {m} (year {m // YEAR}, month start {(m % YEAR) // MONTH} of it) woman {woman} man {man}")
    print(f"(a) F_w {f_w!r} F_m {f_m!r} u {unit(word)!r} word {word:016x} adult chance {p!r} boosted chance {boosted!r}")
    for row in before:
        print(f"(a) her attempt before at {row[0]} u {unit(row[5])!r} boosted chance {row[7]!r} conceives at the boost {bernoulli(row[5], row[7])}")
    earlier = [row[0] for row in rows if row[1] == woman and row[0] < m - 3 * MONTH and bernoulli(row[5], row[6])]
    print(f"(a) the twin: her conceptions before m − 3 MONTH (the same in every variant) {earlier}; none from there to m")
    ok = True
    for name, (offsets, conceives) in VARIANTS.items():
        stamps = [m + offset for offset in offsets]
        conceived, window, entries, world = variant(s, stamps, woman, man, m)
        want = m if conceives else None
        print(f"(b) {name} stamps {stamps} window {window} conceived {conceived} want {want}")
        for line in entries:
            print(f"(b) {name} entry {line.hex()}")
        ok = ok and conceived == want
    print(f"lever {'ok' if ok else 'WRONG'}")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))

#!/usr/bin/env python3
"""The man's answer and the target ages (genetics specification 11.2a, ADR-0018 addendum of
2026-11-06) on small worlds, computed by the model of tools/world_reference.py: the months and
partners of the scenarios of crates/sim-core/tests/market_answer.rs, from the draws of
tools/rng_reference.py and the rule of 11.2 and 11.2a, not from the Rust code.

The data: the map 3 fixture's calibration with the repository's life course data, the search
height `c` = 1 and almost no deaths (accident 1e-9 per year, senescence off), and, if the
scenario has the group, the six values of 11.2a with the weight `w_am` and the taste's spread
`σ_ν` the scenario names (slope 0.4, from 22, cap 15, minimum 21). The test map names no
cognition or height trait, so the similarity terms drop out (11.1).

Scenarios: "answer" (seed 0xff, tag `test:answer`: a woman of 20 and a man of 22, single) and
"choice" (a woman of 20, a man of 26 and a man of 21, tag `test:answer2`).

Usage: python3 tools/market_answer_reference.py   (asserts the pinned values, prints the rounds)
"""

import data_reference
import world_reference as wr
from postcard_reference import FEMALE, MALE, WILD, genotype, variant, varint
from rng_reference import founder_key

YEAR, MONTH = wr.YEAR, wr.MONTH
GROUP = (
    ("core:pairing_taste_sd", "year"),
    ("core:pairing_male_target_slope", "ratio"),
    ("core:pairing_male_target_from", "year"),
    ("core:pairing_male_target_cap", "year"),
    ("core:pairing_male_target_min", "year"),
    ("core:pairing_weight_age_male", "logit"),
)


def data(answer=None) -> wr.Data:
    """The data of the scenarios; `answer` is `(w_am, σ_ν)` or None for the market without the
    group."""
    demography, calibration, genome_map = data_reference.documents(wr.MAP3_DIR)
    values = calibration["values"]
    for sex in ("female", "male"):
        values[f"core:accident_{sex}"]["value"] = 1e-9
        values[f"core:senescence_a_{sex}"]["value"] = 1e-15
        values[f"core:senescence_b_{sex}"]["value"] = 0.001
    demography["values"]["core:market_search_height_female"]["value"] = 1.0
    if answer is not None:
        weight, spread = answer
        for (key, unit), value in zip(GROUP, (spread, 0.4, 22.0, 15.0, 21.0, weight)):
            demography["values"][key] = wr.test_value(value, unit)
    return wr.Data(demography, calibration, genome_map)


def world(answer, seed: int, tag: str, ages: list) -> wr.World:
    """Single founders born `years` before the start, in this order: sim 1, 2, ..."""
    founders = [(-years * YEAR, sex, genotype(WILD, WILD)) for years, sex in ages]
    return wr.World(seed, tag, False, founders, [], data=data(answer))


def answer_world(answer) -> wr.World:
    return world(answer, 0xFF, "test:answer", [(20, FEMALE), (22, MALE)])


def choice_world(answer, seed: int) -> wr.World:
    return world(answer, seed, "test:answer2", [(20, FEMALE), (26, MALE), (21, MALE)])


def pairs(w: wr.World) -> list:
    """The partnerships formed so far as (woman, man, formed)."""
    return [(w.partnerships[number][0], w.partnerships[number][1], w.partnerships[number][2]) for number in sorted(w.partnerships)]


def first_month(w: wr.World, months: int):
    """The first month of a partnership within `months` months, or None."""
    w.advance(months * MONTH)
    found = pairs(w)
    return found[0][2] // MONTH if found else None


def draw_cause(tag: int, subject: int, month: int, detail: int) -> bytes:
    """The cause `Random` of a draw as the log writes it (variant 4)."""
    return variant(4, varint(tag), varint(subject), varint(month), varint(detail))


def first_entry(w: wr.World) -> bytes:
    """The log entry of the first partnership."""
    (record,) = [choice for rnd in w.rounds for choice in rnd["choices"] if choice["outcome"] == "paired"][:1]
    return w.log[record["entry"] - 1]


def draws_in(entry: bytes, month: int, woman: int, man: int, key_of) -> tuple:
    """Whether the entry names her `PAIRING_NOISE` draw and his `PAIRING_ACCEPT` draw (in this
    order) at the month; the position of each, or None."""
    hers = draw_cause(wr.PAIRING_NOISE, key_of(woman), month, key_of(man))
    his = draw_cause(wr.PAIRING_ACCEPT, key_of(man), month, key_of(woman))
    return (entry.find(hers) if hers in entry else None, entry.find(his) if his in entry else None)


def rounds_text(w: wr.World) -> list:
    """The rounds with a proposal: month, woman, chosen, her utilities, his answer."""
    rows = []
    for rnd in w.rounds:
        for choice in rnd["choices"]:
            if choice["chosen"] is None:
                continue
            row = f"month {rnd['n']} sim {choice['woman']} proposes to {choice['chosen']} (U {[round(c[4], 4) for c in choice['candidates']]})"
            answer = choice.get("answer")
            if answer is not None:
                row += f" target {answer['target']:.4f} taste {answer['taste']:.4f} G_m {answer['noise']:.4f} U_m {answer['utility']:.4f}"
            rows.append(row + f": {choice['outcome']}")
    return rows


def main() -> None:
    print(f"taste_sd 3 seed 0xff test:answer founder 1 {wr.taste_draw(0xFF, founder_key('test:answer', 1), 3.0)!r}")
    # 1. The man accepts by his weight and his taste: the first month of a partnership in 140.
    for weight, spread, month in [(1e-6, 3.0, 3), (0.06, 3.0, 29), (0.5, 3.0, None), (0.5, 0.0, 3), (1e6, 3.0, None)]:
        w = answer_world((weight, spread))
        got = first_month(w, 140)
        assert got == month, (weight, spread, got)
        print(f"answer w_am {weight} sigma {spread}: first partnership month {got}")
    # The rounds of the first one: ν of the man +5.4009 (he aims at 27.4), his Gumbel at the
    # rounds she searches -0.2653 (month 1), 2.8330 (month 3).
    w = answer_world((1e-6, 3.0))
    w.advance(3 * MONTH)
    for row in rounds_text(w):
        print("  " + row)
    searched = [rnd["n"] for rnd in w.rounds if any(choice["chosen"] is not None for choice in rnd["choices"])]
    assert searched == [1, 3], searched
    # 2. A refusal is no state: no partnership before month 3; then (1, 2) with both draws.
    w = answer_world((1e-6, 3.0))
    w.advance(3 * MONTH - 1)
    assert pairs(w) == [] and not w.partnerships
    w.advance(3 * MONTH)
    assert pairs(w) == [(1, 2, 3 * MONTH)]
    entry = first_entry(w)
    found = draws_in(entry, 3, 1, 2, w.key_of)
    assert found[0] is not None and found[1] is not None and found[0] < found[1]
    assert (f"{w.key_of(1):016x}", f"{w.key_of(2):016x}") == ("09e2636c710c3b7a", "480f66c0286df3b5")
    # 3. "choice", seed 4: no second proposal after a refusal; the pair forms in month 3.
    w = choice_world((1e-6, 3.0), 4)
    w.advance(MONTH)
    assert pairs(w) == []
    w.advance(2 * MONTH)
    assert pairs(w) == []
    w.advance(3 * MONTH)
    assert pairs(w) == [(1, 2, 3 * MONTH)]
    for row in rounds_text(w):
        print("  " + row)
    entry = first_entry(w)
    found = draws_in(entry, 3, 1, 2, w.key_of)
    assert found[0] is not None and found[1] is not None and found[0] < found[1]
    assert (f"{w.key_of(1):016x}", f"{w.key_of(2):016x}") == ("5972c50bc8af3f94", "700b34fbb9d7b242")
    # 4. Without the group the chosen man does not answer: month 1, no draw of his.
    w = choice_world(None, 4)
    w.advance(MONTH)
    assert pairs(w) == [(1, 2, MONTH)]
    found = draws_in(first_entry(w), 1, 1, 2, w.key_of)
    assert found[0] is not None and found[1] is None
    # 5. Her taste moves her choice (seed 78, both men accept).
    without = choice_world((1e-6, 0.0), 78)
    without.advance(MONTH)
    assert pairs(without) == [(1, 2, MONTH)]
    with_taste = choice_world((1e-6, 3.0), 78)
    with_taste.advance(MONTH)
    assert pairs(with_taste) == [(1, 3, MONTH)]
    (taste_choice,) = [choice for choice in with_taste.rounds[0]["choices"] if choice["chosen"] is not None]
    assert abs(taste_choice["tastes"][1] - -4.18) < 0.005
    print(f"her taste at seed 78: {taste_choice['tastes'][1]:.4f}")
    # 6. An arrangement does not ask the man: no PAIRING_ACCEPT in its entry, and the market
    # then pairs nobody else.
    w = answer_world((1e6, 3.0))
    stamp = 100 * 86_400
    w.advance(stamp)
    assert pairs(w) == []
    w.arrange(1, 2)
    assert pairs(w) == [(1, 2, stamp + 1)]
    assert variant(4, varint(wr.PAIRING_ACCEPT)) not in w.log[-1] and variant(4, varint(wr.PAIRING_NOISE)) not in w.log[-1]
    w.advance(stamp + 20 * MONTH)
    assert pairs(w) == [(1, 2, stamp + 1)]
    print("market_answer_reference: every scenario as pinned")


if __name__ == "__main__":
    main()

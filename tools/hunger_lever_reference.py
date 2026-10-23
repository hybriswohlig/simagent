#!/usr/bin/env python3
"""The lever test of the hunger's height (ADR-0023 section 5, "Hunger, die Größe"; the shock of
ADR-0023 section 3 and its addendum before S1b; the loss of ADR-0013 section 4; the means of
the addendum of 2026-10-21 to ADR-0021 section 3, "Mittelwerte, genau" and "Erbe"), written
from the ADRs, independent of the Rust code. The model is tools/world_reference.py, the census
tools/census_reference.py (`census_row` at each year end, as `run_fixture` writes it).

The scenario tests/scenarios/fork_no_density.json (32 founders, 16 couples, no pool) on the
repository's data as the CLI runs it (`golden_reference.make_world` on `wr.REPOSITORY`), three
times: the twin without an input after `Start`; "one", `core:hunger_one_year` stamped in the
middle of year `Y` (`Y · YEAR + YEAR // 2 − 1`); "three", `core:hunger_three_years` stamped in
the last second of year `Y`, submitted after the row of year `Y` (the census of second `t`
comes before the input stamped `t`, which acts in `t + 1`). Both set the first hunger year
`Y + 1` (`wr.shock_years`). `Y` is the smallest year in 0..=34 whose cohort (born in year `Y`,
exactly 25 full years at the year end of `Y + 25`, column `age25`) has a woman and a man in the
twin's row `Y + 25` and no clamped `height_cm` in that row of any run.

By hand (ADR-0013 section 4, k = 5 cm, windows [0, 2) weight 1, [2, 5) weight 0.5, the
puberty window from max(5, θ_i − 2)): a member of cohort `Y` born at the offset `f` (in years)
is aged in [1 − f, 2 − f) in year `Y + 1`, so one year at 0.7 costs 5 · 0.3 · 1 = 1.5 cm; three
years cost 1.5 + 1.5 · (f + 0.5 · (1 − f)) + 1.5 · 0.5 = 3.0 + 0.75 · f cm (the last year
ends at the age 4 − f, before every puberty window). Only the body reads the nutrition years,
so every census column but the height means is the twin's in every row.

Usage: $HOME/.cache/simagent-tools-venv/bin/python hunger_lever_reference.py   (from tools/)
"""

import json
import struct
import sys
import time

import body_reference as br
import census_reference as cr
import data_reference
import golden_reference as gr
import world_reference as wr
from arrival_reference import entry_json
from pool_reference import Cursor
from postcard_reference import FEMALE
from world_reference import YEAR, as_years

SCENARIO = gr.SCENARIOS / "fork_no_density.json"
SHOCKS = {"one": "core:hunger_one_year", "three": "core:hunger_three_years"}
LAST_Y = 34
LATER = 25
K, INDEX = 5.0, 0.7
HEIGHT = cr.CLAMPED_KEYS.index("height_cm")
HEIGHT_MEANS = {f"{column}.{sex}.mm" for column in ("adults", "age25") for sex in ("female", "male")}


def stamp(run: str, y: int) -> int:
    return y * YEAR + YEAR // 2 - 1 if run == "one" else cr.year_end(y)


def cohort(evaluator: cr.Evaluator, y: int, t: int) -> list:
    """Every sim born in year `y` (living or archived) by SimId at `t`: (sim, sex, born, f,
    alive, measured, the contract `height_cm` or None, θ_i)."""
    world = evaluator.world
    out = []
    for sim in sorted(set(world.sims) | set(world.archive)):
        born = world.born_of(sim)
        if not y * YEAR <= born < (y + 1) * YEAR:
            continue
        parts = evaluator.person_parts(sim)
        theta = br.individual_curve(evaluator.body.values, parts["sex"], parts["z"]["z_h"], parts["z"]["z_p"])["theta_i"]
        alive = sim in world.sims
        measured = alive and evaluator.body.accepted
        height = evaluator.evaluate(sim, t)["params"]["height_cm"][0] if measured else None
        assert not alive or (t - born) // YEAR == LATER
        out.append((sim, parts["sex"], born, as_years(born - y * YEAR), alive, measured, height, theta))
    return out


def run_rows(s: dict, last: int, run=None, y=None) -> tuple:
    """Rows 0..=last of the scenario's census (`census_row` per year end, as `run_fixture`), the
    persons of each row (living, archived with their leaving second) and, from row 25 on, the
    cohort `k − 25` at row `k`; with `run`, its shock stamped at `stamp(run, y)`."""
    world = gr.make_world(s, wr.REPOSITORY)
    evaluator = cr.Evaluator(world, cr.BodyCensusData(world.data, data_reference.documents(wr.ROOT / "data")))
    from_log = cr.LogPopulation(world, cr.death_causes(world.data))
    rows, persons, cohorts = [], [], {}
    for k in range(last + 1):
        if run is not None and k == y and stamp(run, y) < cr.year_end(k):
            world.advance(stamp(run, y))
            world.trigger_shock(SHOCKS[run])
        world.advance(cr.year_end(k))
        row, _ = cr.census_row(evaluator, k)
        assert {key: row[key] for key in ("births", "arrivals", "emigrations", "deaths")} == from_log.row(k), k
        assert row["arrivals"] == 0 and row["body_evaluated"] == row["living"], k
        rows.append(row)
        persons.append((sorted(world.sims), sorted((sim, value[1]) for sim, value in world.archive.items())))
        if run is not None and k == y and stamp(run, y) == cr.year_end(k):
            world.trigger_shock(SHOCKS[run])
        if k >= LATER:
            cohorts[k] = cohort(evaluator, k - LATER, cr.year_end(k))
    gr.check_all(world)
    return world, rows, persons, cohorts


def columns(row: dict, data: wr.Data) -> dict:
    """A census row flattened to its columns by name."""
    names = {"clamped": cr.CLAMPED_KEYS, "deaths": cr.death_causes(data), "affected": [d.code for d in data.diseases]}
    out = {}
    for key, value in row.items():
        if key in ("adults", "age25"):
            for sex, (count, mm) in zip(("female", "male"), value):
                out[f"{key}.{sex}.count"], out[f"{key}.{sex}.mm"] = count, mm
        elif isinstance(value, list):
            assert len(value) == len(names[key]), key
            out.update({f"{key}.{name}": item for name, item in zip(names[key], value)})
        else:
            out[key] = value
    return out


def entry_text(line: bytes) -> str:
    """An entry of the shock as JSON: `core:shock_triggered` by `arrival_reference.entry_json`;
    `core:param_changed` (payload 3) with `core:nutrition_year` (change 1: year, f64) here."""
    text = entry_json(line)
    if text is not None:
        return text
    c = Cursor(line)
    entry_id, t = c.varint(), c.signed()
    assert c.varint() == 0
    causes = []
    for _ in range(c.varint()):
        assert c.varint() == 0
        causes.append({"event": c.varint()})
    assert c.varint() == 3 and c.varint() == 1
    year, index = c.signed(), struct.unpack("<d", c.take(8))[0]
    assert c.left() == 0
    payload = {"core:param_changed": {"change": {"core:nutrition_year": {"year": year, "index": index}}}}
    return json.dumps({"id": entry_id, "t": t, "subjects": [], "causes": causes, "payload": payload}, separators=(",", ":"))


def hand_loss(run: str, f: float) -> float:
    return K * (1.0 - INDEX) * (1.0 if run == "one" else 1.0 + (f + 0.5 * (1.0 - f)) + 0.5)


def means(people: list) -> list:
    """[[count, mm, unrounded mm]] for women, then men, over the measured, by SimId."""
    out = []
    for female in (True, False):
        values = [height * 10.0 for _, sex, _, _, _, measured, height, _ in people if measured and (sex == FEMALE) == female]
        total = 0.0
        for value in values:
            total += value
        out.append([len(values), cr.mean_rounded(values), total / len(values) if values else None])
    return out


def main(argv) -> int:
    s = gr.scenario(SCENARIO)
    assert s["pool"] is None and s["until"] == cr.year_end(LAST_Y + LATER)
    mark = time.time()
    twin, twin_rows, twin_persons, twin_cohorts = run_rows(s, LAST_Y + LATER)
    print(f"time twin rows 0..={LAST_Y + LATER} {time.time() - mark:.1f} s")
    candidates = []
    for y in range(LAST_Y + 1):
        row = twin_rows[y + LATER]
        counts, clamped = (row["age25"][0][0], row["age25"][1][0]), row["clamped"][HEIGHT]
        print(f"(a) Y {y} twin row {y + LATER} age25 (women, men) {counts} clamped height_cm {clamped}")
        if min(counts) >= 1 and clamped == 0:
            candidates.append(y)
    print(f"(a) candidates {candidates}")
    for y in candidates:
        runs = {}
        for run in SHOCKS:
            mark = time.time()
            runs[run] = run_rows(s, y + LATER, run, y)
            print(f"time {run} for Y {y} rows 0..={y + LATER} {time.time() - mark:.1f} s")
        held = {run: runs[run][1][y + LATER]["clamped"][HEIGHT] for run in SHOCKS}
        print(f"(a) Y {y} clamped height_cm in row {y + LATER} {held}")
        if all(value == 0 for value in held.values()):
            break
    else:
        print("(a) no Y qualifies")
        return 1
    row_y = y + LATER
    print(f"(a) chosen Y {y} (row {row_y})")
    for run, (world, _, _, _) in runs.items():
        (record,) = world.shocks
        print(f"(b) {run} {SHOCKS[run]} stamp {stamp(run, y)} acts {record['second']} input {record['input']} first_year {record['years'][0]} last_year {record['years'][-1]} index {record['index']!r}")
        assert record["years"] == list(wr.shock_years(stamp(run, y), len(record["years"]))) and record["years"][0] == y + 1
        print(f"(b) {run} nutrition years after the shock {world.nutrition}")
        for entry in [record["entry"]] + record["changes"]:
            line = world.log[entry - 1]
            print(f"(b) {run} entry {entry_text(line)} hex {line.hex()}")
    people = {"twin": twin_cohorts[row_y]} | {run: runs[run][3][row_y] for run in SHOCKS}
    for run, group in people.items():
        rows = twin_rows if run == "twin" else runs[run][1]
        cells = means(group)
        assert [cell[:2] for cell in cells] == rows[row_y]["age25"], (run, cells)
        print(f"(c) {run} row {row_y} age25 {rows[row_y]['age25']} unrounded mm (women, men) {[cell[2] for cell in cells]}")
        print(f"(c) {run} row {row_y} adults {rows[row_y]['adults']}")
    for run in SHOCKS:
        drops = [a[2] - b[2] for a, b in zip(means(people["twin"]), means(people[run]))]
        print(f"(c) {run} drop of the unrounded means in mm (women, men) {drops}")
    for index, (sim, sex, born, f, alive, measured, height, theta) in enumerate(people["twin"]):
        losses = []
        for run in SHOCKS:
            other = people[run][index]
            assert other[:6] == (sim, sex, born, f, alive, measured), (run, other)
            hand = hand_loss(run, f)
            loss = height - other[6] if measured else None
            assert not measured or abs(loss - hand) <= 1e-9, (sim, run, loss, hand)
            losses.append(f"{run} loss {loss!r} hand {hand!r}")
        label = "female" if sex == FEMALE else "male"
        print(f"(d) sim {sim} {label} born {born} f {f!r} theta_i {theta!r} alive {alive} measured {measured} twin height_cm {height!r} {'; '.join(losses)}")
    for run in SHOCKS:
        _, rows, persons, _ = runs[run]
        differ = [(k, name) for k in range(row_y + 1) for name, value in columns(rows[k], twin.data).items() if value != columns(twin_rows[k], twin.data)[name]]
        same_persons = persons == twin_persons[: row_y + 1]
        print(f"(e) {run} differing (row, column) {differ}")
        assert all(name in HEIGHT_MEANS for _, name in differ), (run, differ)
        assert all(k > y for k, _ in differ) and same_persons, (run, differ)
        print(f"(e) {run} only height means differ, rows 0..={y} equal in full, same persons in every row; first differing row {min((k for k, _ in differ), default=None)}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

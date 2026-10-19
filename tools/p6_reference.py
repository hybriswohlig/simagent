#!/usr/bin/env python3
"""Criterion P6, the shortage cohort (genetics specification 8.2 P6 and 5.4; ADR-0001 section
6, ADR-0013 sections 2 and 4, ADR-0015 section 4, ADR-0020 section 8), the known answers of
the gate test, written from the specification, the ADRs and the plan of step 11 (section 2,
P6, with Grok K4 (a): P6 counts town-born persons, in a run without immigration),
independent of the Rust code. The world is tools/world_reference.py on the repository's data
(map 4 and the pool, `world_reference.REPOSITORY`), the pure height is
tools/census_reference.py (`heights`, on tools/body_reference.py).

The runs (fixed before any run):

- The town: the pool of the repository, `n_start` 1000, immigration off, capacity none, the
  switch of the hereditary diseases on, no nutrition year at the start, world seed
  0x5eed_0006_0000_0001, founder tag `core:criteria_p6`; `Y` = 1, so `T_Y = year_start(1)`.
- The base run to the end of year 20 (`21 · YEAR − 1 s`, year hashes 0 to 20).
- The fork: the base world at the fork point `T_Y − 1 s` saved and loaded (a copy whose save
  is the base's save there, byte for byte, and passes every load check), then one override
  block `[NutritionYear { year: 1, index: 0.7 }]` stamped `T_Y − 1 s`
  (`World.override_nutrition`), then to the end of year 20. The same fork by replay from
  `t0` (ADR-0001 section 6: a new run to `T_Y − 1 s`, the block, to the end) is asserted to
  give the same save at the end.
- The negative twin: the same fork point, the override on year `Y + 2` = 3 instead. Its
  variant `twin_late` (the fork point `year_start(3) − 1 s`, the same block) gives the same
  heights bit for bit; only its hashes differ.

The comparison, by lineage key (never by SimId, ADR-0001 section 6):

- Everyone conceived before `T_Y` (the arrivals at the start, the pool children of the
  carried pregnancies, and every child of a town conception before `T_Y`): the same person
  (sex, genome, biological parents' keys) with the same birth and the same leaving (second
  and cause) in both runs. The whole story (`world_reference.story`) is asserted equal too:
  nothing but the body mapping reads the list.
- Cohort `c` = the persons born in `[year_start(c), year_start(c + 1))`. "Adult in both
  runs": alive at `born + 18 years` in both (left at or after that second; a death exactly at
  the 18th birthday counts, as in the criteria kit).
- The pure height at 18: `κ_i · H(18; θ_i) − v_i(18) − Verlust(b)` (census_reference
  `heights`, never clamped; `b` = 18, so `Verlust` is 0), in the base (no nutrition year)
  and in the fork (`[(1, 0.7)]`). `d = h_base − h_fork` in f64.
- Expected (specification, never the run): cohort `Y − 1` = 0: 1.50 cm [W5.4c]; cohort `Y`
  = 1 conceived before `T_Y`: `k · 0.3 · (Y + 1 − birth)` with `k` = 5 cm [K5.4a] and
  `birth = born / YEAR` in years, here `1.5 · (2 − born / YEAR)`, exact as a fraction. The
  deviation `|d − expected|` is computed exactly from the f64 `d`; the bound is 1e-9 cm.
- The twin (year 3): cohort 0 is aged 2 to 4 in year 3, the window 0.5: it loses
  `5 · 0.5 · 0.3` = 0.75 cm, so the 1.50 assertion fails by 0.75. Cohort 1 (born `b` in
  [1, 2)) is aged `3 − b` to `4 − b`: `1.5 · ((b − 1) + 0.5 · (2 − b)) = 0.75 · b`. Both
  twin values are asserted too (as the twin's own rule, to 1e-9).
- Cross-checks, not the criterion: the same differences at the end of year 20 for the units
  living then (the loss of year 1 is complete from age 2 on, so any later age gives it);
  every child born in the town was conceived `pregnancy_months` months before its birth;
  the town births per year; the arrivals born in year 0 (born at the second 0 of the start,
  not town-born: Grok K4 (a) counts town-born persons, the rule holds for them as well, G6
  keeps year 1 for an arrival of second 0).

Usage: $HOME/.cache/simagent-tools-venv/bin/python tools/p6_reference.py [--out DIR]
       (default DIR ~/.cache/simagent-notes/ref-p6; writes p6.json and p6.txt, prints the
       `label value` lines of p6.txt)
"""

import copy
import json
import struct
import sys
import time
from fractions import Fraction
from pathlib import Path

import arrival_reference
import census_reference
import data_reference
import world_reference as wr
from postcard_reference import genome_of
from world_reference import MONTH, YEAR, hash64

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_OUT = Path.home() / ".cache" / "simagent-notes" / "ref-p6"

SEED = 0x5EED_0006_0000_0001
TAG = "core:criteria_p6"
N_START = 1000
IMMIGRATION = False
SWITCH = True
CAPACITY = None
Y = 1
SHOCK = 0.7
TWIN_YEAR = Y + 2
END_YEAR = 20
END = (END_YEAR + 1) * YEAR - 1
T_Y = Y * YEAR
FORK_STAMP = T_Y - 1
TWIN_LATE_STAMP = TWIN_YEAR * YEAR - 1
ADULT = 18 * YEAR
TOLERANCE = 1e-9
# The specification's numbers (5.4): k [K5.4a], the windows and their ages, the cohort Y − 1
# [W5.4c]; the shortage 1 − N of the shock as the exact 3/10.
K = Fraction(5)
SHORTAGE = Fraction(3, 10)
W54C = Fraction(3, 2)
SPEC_WINDOWS = {
    "core:nutrition_loss": 5.0,
    "core:nutrition_weight_infant": 1.0,
    "core:nutrition_weight_child": 0.5,
    "core:nutrition_weight_puberty": 0.25,
    "core:nutrition_age_infant": 2.0,
    "core:nutrition_age_child": 5.0,
    "core:nutrition_puberty_before": 2.0,
    "core:nutrition_puberty_after": 1.0,
}
assert K * SHORTAGE * 1 == W54C


def hexed(value: int) -> str:
    return f"{value:016x}"


def bits(value: float) -> str:
    return struct.pack(">d", value).hex()


def copy_world(world: wr.World) -> wr.World:
    """A loaded world: a deep copy that shares the data (as the references copy worlds)."""
    return copy.deepcopy(world, {id(world.data): world.data})


def make_world(data: wr.Data) -> wr.World:
    pool = wr.pool_start(data, SEED, N_START, IMMIGRATION)
    return wr.World(SEED, TAG, SWITCH, [], (), data, capacity=CAPACITY, pool=pool)


def carried_conceptions(world: wr.World) -> dict:
    """{child key: conceived} of the pregnancies the start brought (the pool's, shifted)."""
    return {key: conceived for _, (_, _, children, conceived) in world.pregnancies.items() for key, _, _ in children}


def conceptions(world: wr.World, carried: dict) -> dict:
    """{child key: conceived} of every conception the world knows: the carried ones and the
    town's (`World.multiples`)."""
    known = dict(carried)
    for t, _, _, _, _, _, children in world.multiples:
        for key, _ in children:
            assert key not in known
            known[key] = t
    return known


def persons(world: wr.World, conceived: dict) -> dict:
    """{lineage key: record} of every sim, living or archived."""
    out = {}
    for sim in sorted(set(world.sims) | set(world.archive)):
        if sim in world.sims:
            born, key, sex, person = world.sims[sim][:4]
            left, cause = None, None
        else:
            born, left, cause, key, sex, person = world.archive[sim][:6]
        assert key not in out
        out[key] = {
            "sim": sim,
            "key": key,
            "born": born,
            "left": left,
            "cause": None if left is None else ("emigration" if cause is None else world.cause_code(cause)),
            "sex": "female" if sex == wr.FEMALE else "male",
            "genome": genome_of(person),
            "parents": world.biological_keys(sim),
            "kind": arrival_reference.env_kind(world, sim),
            "conceived": conceived.get(key),
        }
    return out


def conceived_before(record: dict, t: int) -> bool:
    """Conceived before `t`: an arrival or founder (born at or before the start, its conception
    before it) or a child whose conception second is before `t`."""
    if record["conceived"] is None:
        assert record["born"] <= 0 and record["kind"] in ("arrival", "founder"), record["kind"]
        return True
    return record["conceived"] < t


def adult(record: dict) -> bool:
    return record["left"] is None or record["left"] >= record["born"] + ADULT


def cohort(record: dict) -> int:
    return record["born"] // YEAR


def check_all(world: wr.World) -> None:
    arrival_reference.check_all(world)


class Run:
    """A finished world with its persons and its body evaluator."""

    def __init__(self, name: str, world: wr.World, carried: dict, body: census_reference.BodyCensusData):
        self.name = name
        self.world = world
        self.persons = persons(world, conceptions(world, carried))
        self.evaluator = census_reference.Evaluator(world, body)

    def pure_at_18(self, key: int) -> float:
        record = self.persons[key]
        before_age_loss, pure = census_reference.heights(self.evaluator, record["sim"], record["born"] + ADULT)
        assert before_age_loss == pure, "no loss of age at 18"
        return pure

    def pure_at(self, key: int, t: int) -> float:
        return census_reference.heights(self.evaluator, self.persons[key]["sim"], t)[1]

    def hashes(self) -> dict:
        world = self.world
        save = world.save()
        return {
            "state_hash": hexed(world.state_hash()),
            "log_hash": hexed(world.log_hash()),
            "entries": len(world.log),
            "save_bytes": len(save),
            "save_hash": hexed(hash64(save)),
            "year_hashes": [hexed(value) for value in world.year_hashes],
            "nutrition": [[year, index] for year, index in world.nutrition],
            "inputs": len(world.inputs),
            "living": len(world.sims),
            "archived": len(world.archive),
        }


def same_persons(base: Run, other: Run) -> dict:
    """The identity across the fork: everyone conceived before `T_Y` is the same person with
    the same birth and leaving; returns the counts and whether all persons agree."""
    fields = ("born", "left", "cause", "sex", "genome", "parents", "kind", "conceived")
    before = [key for key, record in base.persons.items() if conceived_before(record, T_Y)]
    for key in before:
        assert key in other.persons, f"{other.name}: {key:016x} missing"
        for field in fields:
            assert base.persons[key][field] == other.persons[key][field], (other.name, hexed(key), field)
    everyone = set(base.persons) == set(other.persons) and all(
        all(base.persons[key][field] == other.persons[key][field] for field in fields) for key in base.persons
    )
    return {"conceived_before_T_Y": len(before), "persons_base": len(base.persons), "persons_other": len(other.persons), "all_persons_identical": everyone}


def expected_fork(record: dict) -> Fraction:
    """The rule of P6 (spec 8.2, 5.4) for the fork on year Y."""
    c = cohort(record)
    if c == Y - 1:
        return W54C
    assert c == Y
    return K * SHORTAGE * (Y + 1 - Fraction(record["born"], YEAR))


def expected_twin(record: dict) -> Fraction:
    """The twin's own value (the override on year 3), from the windows of 5.4: cohort 0 in the
    window 0.5 for the whole year; cohort 1 (born at `b` years) `0.75 · b`."""
    c = cohort(record)
    b = Fraction(record["born"], YEAR)
    if c == 0:
        return K * SHORTAGE * Fraction(1, 2)
    assert c == 1
    a0 = TWIN_YEAR - b
    a1 = a0 + 1
    infant = max(Fraction(0), min(a1, 2) - max(a0, 0))
    child = Fraction(1, 2) * max(Fraction(0), min(a1, 5) - max(a0, 2))
    value = K * SHORTAGE * (infant + child)
    assert value == Fraction(3, 4) * b
    return value


def units(base: Run, other: Run) -> list:
    """The persons of cohorts 0 and 1 the criterion measures: born in year 0, or born in year 1
    and conceived before `T_Y`; adult in both runs. Ascending lineage key."""
    chosen = []
    for key in sorted(base.persons):
        record = base.persons[key]
        c = cohort(record)
        if c not in (Y - 1, Y):
            continue
        if c == Y and not conceived_before(record, T_Y):
            continue
        assert key in other.persons
        if adult(record) and adult(other.persons[key]):
            chosen.append(key)
    return chosen


def compare(base: Run, other: Run, expected) -> dict:
    rows, deviations = [], []
    for key in units(base, other):
        record = base.persons[key]
        h_base, h_other = base.pure_at_18(key), other.pure_at_18(key)
        d = h_base - h_other
        e = expected(record)
        deviation = abs(Fraction(d) - e)
        deviations.append(deviation)
        # The same difference at the end of year 20 (any age past the shock year gives the same
        # loss), for a person living at the end in both runs: a cross-check, not the criterion.
        at_end = None
        if record["left"] is None and other.persons[key]["left"] is None:
            d_end = base.pure_at(key, END) - other.pure_at(key, END)
            at_end = float(abs(Fraction(d_end) - e))
        rows.append(
            {
                "cohort": cohort(record),
                "key": hexed(key),
                "sim": record["sim"],
                "kind": record["kind"],
                "town_born": record["kind"] != "arrival",
                "sex": record["sex"],
                "born": record["born"],
                "conceived": record["conceived"],
                "base": bits(h_base),
                "fork": bits(h_other),
                "difference": bits(d),
                "base_cm": repr(h_base),
                "fork_cm": repr(h_other),
                "difference_cm": repr(d),
                "expected_cm": repr(float(e)),
                "deviation_cm": float(deviation),
                "deviation_at_end_cm": at_end,
            }
        )
    per_cohort = {}
    for c in (Y - 1, Y):
        mine = [row for row in rows if row["cohort"] == c]
        per_cohort[str(c)] = {
            "n": len(mine),
            "town_born": sum(1 for row in mine if row["town_born"]),
            "max_deviation_cm": max((row["deviation_cm"] for row in mine), default=0.0),
            "min_deviation_cm": min((row["deviation_cm"] for row in mine), default=0.0),
            "living_at_end": sum(1 for row in mine if row["deviation_at_end_cm"] is not None),
            "max_deviation_at_end_cm": max((row["deviation_at_end_cm"] for row in mine if row["deviation_at_end_cm"] is not None), default=0.0),
            "kinds": {kind: sum(1 for row in mine if row["kind"] == kind) for kind in sorted({row["kind"] for row in mine})},
        }
    return {
        "rows": rows,
        "cohorts": per_cohort,
        "max_deviation_cm": float(max(deviations, default=Fraction(0))),
        "min_deviation_cm": float(min(deviations, default=Fraction(0))),
    }


def cohort_counts(run: Run) -> dict:
    """Everyone of cohorts 0 and 1 in a run, by what the criterion needs."""
    out = {}
    for c in (Y - 1, Y):
        mine = [record for record in run.persons.values() if cohort(record) == c]
        out[str(c)] = {
            "born": len(mine),
            "conceived_before_T_Y": sum(1 for r in mine if conceived_before(r, T_Y)),
            "conceived_at_or_after_T_Y": sum(1 for r in mine if not conceived_before(r, T_Y)),
            "adult": sum(1 for r in mine if adult(r)),
            "died_before_18": sum(1 for r in mine if not adult(r)),
            "kinds": {kind: sum(1 for r in mine if r["kind"] == kind) for kind in sorted({r["kind"] for r in mine})},
            "conceived_range": [min((r["conceived"] for r in mine if r["conceived"] is not None), default=None), max((r["conceived"] for r in mine if r["conceived"] is not None), default=None)],
            "born_range": [min((r["born"] for r in mine), default=None), max((r["born"] for r in mine), default=None)],
            "conceived_at_or_after": [[hexed(r["key"]), r["born"], r["conceived"]] for r in sorted(mine, key=lambda r: r["key"]) if not conceived_before(r, T_Y)],
        }
    # Every child born in the town was conceived `pregnancy_months` months before its birth.
    months = run.world.data.pregnancy_months * MONTH
    children = [r for r in run.persons.values() if r["conceived"] is not None]
    out["conceived_is_born_minus_pregnancy"] = all(r["born"] - r["conceived"] == months for r in children)
    out["children_with_conception"] = len(children)
    years = {}
    for r in run.persons.values():
        if r["kind"] != "arrival":
            years[r["born"] // YEAR] = years.get(r["born"] // YEAR, 0) + 1
    out["town_births_per_year"] = [years.get(year, 0) for year in range(END_YEAR + 1)]
    out["arrivals_born_in_year_0"] = sorted(hexed(r["key"]) for r in run.persons.values() if r["kind"] == "arrival" and cohort(r) == 0)
    return out


def main(argv) -> int:
    out = DEFAULT_OUT
    if "--out" in argv:
        out = Path(argv[argv.index("--out") + 1])
    out.mkdir(parents=True, exist_ok=True)
    timings = {}
    clock = time.time()

    def lap(label: str) -> None:
        nonlocal clock
        now = time.time()
        timings[label] = round(now - clock, 2)
        clock = now
        print(f"progress {label} {timings[label]} s", file=sys.stderr, flush=True)

    data = wr.REPOSITORY
    documents = data_reference.documents(ROOT / "data")
    body = census_reference.BodyCensusData(data, documents)
    assert body.accepted, body.reason
    values = body.values.v
    for name, value in SPEC_WINDOWS.items():
        assert values[name] == value, (name, values[name])
    assert YEAR == 13 * MONTH
    lap("data")

    # The base run, with copies at the fork points (a save loaded: the copy's save is the base's).
    base = make_world(data)
    carried = carried_conceptions(base)
    carried_pregnancies = len(base.pregnancies)
    start_input = base.inputs[0]
    check_all(base)
    lap("start")
    base.advance(FORK_STAMP)
    check_all(base)
    fork_point_save = base.save()
    fork_point_year_hashes = list(base.year_hashes)
    fork = copy_world(base)
    twin = copy_world(base)
    assert fork.save() == fork_point_save and twin.save() == fork_point_save
    check_all(fork)
    lap("fork_point")
    base.advance(TWIN_LATE_STAMP)
    twin_late_point_save = base.save()
    twin_late = copy_world(base)
    assert twin_late.save() == twin_late_point_save
    base.advance(END)
    check_all(base)
    lap("base")

    entries_before = len(fork.log)
    fork.override_nutrition(Y, SHOCK)
    override_input = fork.inputs[-1]
    override_entry = fork.log[-1]
    assert len(fork.log) == entries_before + 1
    fork.advance(END)
    check_all(fork)
    lap("fork")

    twin.override_nutrition(TWIN_YEAR, SHOCK)
    twin_input = twin.inputs[-1]
    twin.advance(END)
    check_all(twin)
    lap("twin")

    twin_late.override_nutrition(TWIN_YEAR, SHOCK)
    twin_late_input = twin_late.inputs[-1]
    twin_late.advance(END)
    check_all(twin_late)
    lap("twin_late")

    # ADR-0001 section 6: the fork by replay from t0 is the fork by save and load.
    replay = make_world(data)
    replay.advance(FORK_STAMP)
    assert replay.save() == fork_point_save
    replay.override_nutrition(Y, SHOCK)
    replay.advance(END)
    assert replay.save() == fork.save() and replay.log == fork.log and replay.year_hashes == fork.year_hashes
    del replay
    lap("replay")

    # Identity across the forks and the year hashes.
    for other in (fork, twin):
        assert other.year_hashes[:Y] == base.year_hashes[:Y] == fork_point_year_hashes
        assert all(a != b for a, b in zip(other.year_hashes[Y:], base.year_hashes[Y:]))
        assert len(other.log) == len(base.log) + 1
        assert wr.story(other) == wr.story(base), "nutrition changed a story"
    assert twin_late.year_hashes[:TWIN_YEAR] == base.year_hashes[:TWIN_YEAR]
    assert all(a != b for a, b in zip(twin_late.year_hashes[TWIN_YEAR:], base.year_hashes[TWIN_YEAR:]))
    assert wr.story(twin_late) == wr.story(base)

    runs = {name: Run(name, world, carried, body) for name, world in (("base", base), ("fork", fork), ("twin", twin), ("twin_late", twin_late))}
    identity = {name: same_persons(runs["base"], runs[name]) for name in ("fork", "twin", "twin_late")}
    p6 = compare(runs["base"], runs["fork"], expected_fork)
    twin_vs_rule = compare(runs["base"], runs["twin"], expected_fork)
    twin_own = compare(runs["base"], runs["twin"], expected_twin)
    late_vs_rule = compare(runs["base"], runs["twin_late"], expected_fork)
    assert [row["fork"] for row in late_vs_rule["rows"]] == [row["fork"] for row in twin_vs_rule["rows"]], "twin_late heights equal the twin's"
    assert p6["max_deviation_cm"] <= TOLERANCE, p6["max_deviation_cm"]
    assert twin_own["max_deviation_cm"] <= TOLERANCE, twin_own["max_deviation_cm"]
    assert twin_vs_rule["cohorts"]["0"]["max_deviation_cm"] > TOLERANCE
    assert all(row["deviation_cm"] > TOLERANCE for row in twin_vs_rule["rows"] if row["cohort"] == 0)
    assert all(value["max_deviation_at_end_cm"] <= TOLERANCE for value in p6["cohorts"].values())
    lap("compare")

    counts = cohort_counts(runs["base"])
    document = {
        "written_by": "tools/p6_reference.py (independent of crates/)",
        "config": {
            "data_hash": hexed(data.hash),
            "pool_hash": hexed(data.pool.hash),
            "pool_seed": hexed(data.pool.seed),
            "core_version": wr.CORE_VERSION,
            "seed": hexed(SEED),
            "tag": TAG,
            "n_start": N_START,
            "immigration": IMMIGRATION,
            "capacity": CAPACITY,
            "switch": SWITCH,
            "nutrition_at_start": [],
            "households_at_start": len(base.pool_start["households"]),
            "persons_at_start": sum(1 for record in runs["base"].persons.values() if record["kind"] == "arrival"),
            "carried_pregnancies": carried_pregnancies,
            "carried_children": len(carried),
            "Y": Y,
            "T_Y": T_Y,
            "fork_stamp": FORK_STAMP,
            "twin_year": TWIN_YEAR,
            "twin_late_stamp": TWIN_LATE_STAMP,
            "end": END,
            "pregnancy_months": data.pregnancy_months,
            "MONTH": MONTH,
            "YEAR": YEAR,
            "tolerance_cm": TOLERANCE,
        },
        "start_input": {"bytes": len(start_input), "hash": hexed(hash64(start_input))},
        "fork_point": {
            "clock": FORK_STAMP,
            "save_bytes": len(fork_point_save),
            "save_hash": hexed(hash64(fork_point_save)),
            "year_hashes": [hexed(v) for v in fork_point_year_hashes],
        },
        "twin_late_point": {"clock": TWIN_LATE_STAMP, "save_bytes": len(twin_late_point_save), "save_hash": hexed(hash64(twin_late_point_save))},
        "override_inputs": {
            "fork": override_input.hex(),
            "twin": twin_input.hex(),
            "twin_late": twin_late_input.hex(),
            "fork_entry": override_entry.hex(),
        },
        "runs": {name: run.hashes() for name, run in runs.items()},
        "identity": identity,
        "cohorts_base": counts,
        "p6": p6,
        "twin_against_the_rule": twin_vs_rule,
        "twin_against_its_own_value": {key: value for key, value in twin_own.items() if key != "rows"},
        "twin_late_against_the_rule": {key: value for key, value in late_vs_rule.items() if key != "rows"},
        "timings_s": timings,
    }
    (out / "p6.json").write_text(json.dumps(document, indent=1) + "\n")
    rows = text_rows(document)
    (out / "p6.txt").write_text("\n".join(rows) + "\n")
    for row in rows:
        print(row)
    return 0


def text_rows(document: dict) -> list:
    config = document["config"]
    rows = [
        "p6_config " + " ".join(f"{key} {value}" for key, value in config.items()),
        f"p6_start_input bytes {document['start_input']['bytes']} xxh3 {document['start_input']['hash']}",
        f"p6_fork_point clock {document['fork_point']['clock']} save_bytes {document['fork_point']['save_bytes']} save_xxh3 {document['fork_point']['save_hash']} year_hashes {document['fork_point']['year_hashes']}",
        f"p6_twin_late_point clock {document['twin_late_point']['clock']} save_bytes {document['twin_late_point']['save_bytes']} save_xxh3 {document['twin_late_point']['save_hash']}",
    ]
    for name, value in document["override_inputs"].items():
        rows.append(f"p6_override_{name} {value}")
    for name, run in document["runs"].items():
        rows.append(
            f"p6_run {name} state_hash {run['state_hash']} log_hash {run['log_hash']} entries {run['entries']} save_bytes {run['save_bytes']}"
            f" save_xxh3 {run['save_hash']} living {run['living']} archived {run['archived']} inputs {run['inputs']} nutrition {run['nutrition']}"
        )
        rows.append(f"p6_year_hashes {name} {' '.join(run['year_hashes'])}")
    for name, value in document["identity"].items():
        rows.append(f"p6_identity {name} " + " ".join(f"{k} {v}" for k, v in value.items()))
    for c, value in document["cohorts_base"].items():
        if isinstance(value, dict):
            rows.append(f"p6_cohort_base {c} " + " ".join(f"{k} {v}" for k, v in value.items()).replace(", ", ","))
        else:
            rows.append(f"p6_base {c} {value}".replace(", ", ","))
    for label in ("p6", "twin_against_the_rule"):
        result = document[label]
        for c, value in result["cohorts"].items():
            rows.append(
                f"{label}_cohort {c} n {value['n']} town_born {value['town_born']} max_deviation_cm {value['max_deviation_cm']!r}"
                f" min_deviation_cm {value['min_deviation_cm']!r} living_at_end {value['living_at_end']}"
                f" max_deviation_at_end_cm {value['max_deviation_at_end_cm']!r} kinds {value['kinds']}".replace(", ", ",")
            )
        rows.append(
            f"{label}_max_deviation_cm {result['max_deviation_cm']!r} min_deviation_cm {result['min_deviation_cm']!r}"
            f" within_1e-9 {result['max_deviation_cm'] <= TOLERANCE}"
        )
        for row in result["rows"]:
            rows.append(
                f"{label}_person cohort {row['cohort']} key {row['key']} sim {row['sim']} kind {row['kind']} town_born {row['town_born']} sex {row['sex']} born {row['born']}"
                f" conceived {row['conceived']} base {row['base']} fork {row['fork']} difference {row['difference']} ({row['difference_cm']})"
                f" expected {row['expected_cm']} deviation {row['deviation_cm']!r}"
            )
    for label in ("twin_against_its_own_value", "twin_late_against_the_rule"):
        result = document[label]
        for c, value in result["cohorts"].items():
            rows.append(f"{label}_cohort {c} n {value['n']} max_deviation_cm {value['max_deviation_cm']!r} min_deviation_cm {value['min_deviation_cm']!r}")
        rows.append(f"{label}_max_deviation_cm {result['max_deviation_cm']!r} min_deviation_cm {result['min_deviation_cm']!r}")
    rows.append("p6_timings_s " + " ".join(f"{k} {v}" for k, v in document["timings_s"].items()))
    return rows


if __name__ == "__main__":
    sys.exit(main(sys.argv))

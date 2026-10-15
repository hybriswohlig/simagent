#!/usr/bin/env python3
"""Pool persons in the town (step 11; design note of the arrival, version 2; ADR-0020
sections 4 and 5), the known answers, written from the note, the ADRs and the genetics
specification, independent of the Rust code. The model is tools/world_reference.py (the
pool in the data, `Start` with a pool, the draw with POOL_RANK, the event `Immigration`, the
arrival, the pool children, the draws with the pool seed, the death budgets with the arrival
year, the kinship over lineage keys, the orphan succession by key, the pedigree format 4, the
save with the new fields and the load checks `check_pool`); this tool runs its scenarios on
the template of map 4 (tests/golden/map4_template with the repository's life course data,
nutrition index 1 all along: no nutrition year) with pool files of tools/pool_reference.py,
and prints `label value` lines.

The scenarios (each its world seed, its pool, `n_start`, the switch of the hereditary
diseases on; NOTES.md of the reference says why each was chosen and how to rebuild it):

- "arrival" (tests/golden/pool/pool_c.bin, 75 members in 43 households, a pool run of two
  years: young couples, babies, pregnancies; `n_start` 50, immigration on, `q` = 1, seed
  0xff): `Start` takes 29 households (51 persons); one household a year arrives from second
  0 of year 0 on; the pool runs out in year 14 (`pool.empty` at `14 · YEAR`). It holds a pool
  pregnancy whose father died in the pool run (a record only): the child is born in the town
  without a father among the subjects of its `core:birth`; a carried separation; dependent
  children with their carers; children born in the town of two pool persons (`C` with the
  pool seed, `E` with the world seed); an immigrant of year 1 who dies (the detail of the
  budget its arrival year). The same world at the end of year 13 has not stopped.
- "orphans" (tests/golden/pool/pool_d.bin, the pool of `pool_a` with frequent alleles: many
  lethal Mendelian diseases; `n_start` 30, `q` 0, seed 5): a dependent pool child's carer
  dies and the other parent in the household (found by the child's PoolOrigin keys) becomes
  the carer; later a child whose carers are gone goes to its nearest relative.
- "records" (pool_d, `n_start` 30, seed 6): the orphan rule chooses a relative whose
  kinship with the child exists only through records of the pool (no common ancestor in the
  town), and `φ` of chosen pairs over records (a town-born grandchild of the pool and a pool
  cousin, among others), each with its set `S`. An ArrangePartnership pairs a town-born
  person with a pool person; their child is a town person with one pool parent (`E` and `C`
  with the world seed).
- "micro" (pool_c with one carried time moved to `E_p`, written to the output folder): an
  immigrant whose carried conception attempt falls exactly on its arrival second runs in
  micro step 1, after the market round of that second.
- "empty_at_start" (tests/golden/pool/pool_b.bin, 34 members; `n_start` 50): `Start` takes
  every household; the immigration of year 0 at second 0 finds the pool empty.
- The rejections of `Start` and of the data check, each in its order.
- States the load checks reject (`load_reject`, from "arrival" at the end of year 2) and,
  after the review of the reference (NOTES2.md), further states the rules can never produce
  (`load_reject_more`, each from a pinned save: a carried father dropped, a series of the
  town's rule missing, two immigrations swapped, a world that never started with a used
  household), and one accepted state: a carried leaving home due exactly at second 0
  (`leave_home_at_start`).

Usage: .venv/bin/python tools/arrival_reference.py [--out DIR]
       (with --out the constructed pool files are written to DIR as well)
       .venv/bin/python tools/arrival_reference.py --scan POOL N_START YEARS SEED...
       (the cases of the scenarios per seed; for choosing the seeds, not a pin)
"""

import copy
import json
import sys
from fractions import Fraction
from pathlib import Path

import data_reference
import pool_reference
import world_reference as wr
from postcard_reference import FEMALE, seq, signed, string, varint, variant, world_params
from world_reference import MONTH, YEAR, hash64

ROOT = Path(__file__).resolve().parent.parent
TEMPLATE = ROOT / "tests" / "golden" / "map4_template"
POOLS = ROOT / "tests" / "golden" / "pool"
TAG = "test:arrival"


def hexed(value: int) -> str:
    return f"{value:016x}"


# ---------------------------------------------------------------------------------------
# Data


def template_documents() -> tuple:
    """The template of map 4 with the repository's life course data (the documents of the
    town; ADR-0020 section 1)."""
    return data_reference.documents(TEMPLATE)


def golden_pool(name: str) -> bytes:
    return (POOLS / f"{name}.bin").read_bytes()


def town_data(pool: bytes, documents=None) -> wr.Data:
    return wr.Data(*(template_documents() if documents is None else documents), pool=pool)


def with_carried_at_frozen(pool: bytes, member_key: int, field: str) -> bytes:
    """The pool with one member's carried time `field` moved to `E_p` (the arrival second
    after the shift): a constructed pool for the micro steps (note section 10)."""
    file = pool_reference.read_pool(pool)
    for household in file["households"]:
        for member in household["members"]:
            if member["lineage_key"] == member_key:
                assert member[field] is not None
                member[field] = file["frozen_at"]
    data = pool_reference.encode_pool(file)
    assert pool_reference.read_pool(data) == file
    return data


# ---------------------------------------------------------------------------------------
# Running a scenario


def check_all(world: wr.World, deaths: bool = True) -> None:
    for check in (wr.check_households, wr.check_separations, wr.check_market, wr.check_afflictions, wr.check_emigration, wr.check_pool):
        check(world)
    if deaths:
        wr.check_deaths(world)


def make_world(data: wr.Data, seed: int, n_start: int, immigration: bool, switch: bool = True) -> wr.World:
    return wr.World(seed, TAG, switch, [], (), data, pool=wr.pool_start(data, seed, n_start, immigration))


def staged(make, marks: dict, inputs: list, end: int, panic=None) -> tuple:
    """The world `make()` run to `end` with `inputs` (label, the second it acts, woman, man;
    each stamped one second before) and stops around each second of `marks` (label:
    second): the save at the second before (`before_<label>`) and at it (`after_<label>`) and
    the entries of the second; around each input the saves before and after it and its
    entries. Every save passes the load checks. With `panic` (a second after `end`) the world
    is then advanced to it and must stop with an invariant there (`pool.empty`). Returns
    (world, saves, entries, stopped) with `stopped` (code, second) or None; the run in one go
    gives the same log and save at `end`."""
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
    stopped = None
    if panic is not None:
        assert panic > end
        before = len(world.log)
        try:
            world.advance(panic)
        except wr.Invariant as error:
            stopped = (error.code, world.immigrations[-1]["t"], world.log[before:])
        assert stopped is not None, "the run did not stop"
    return world, saves, entries, stopped


# ---------------------------------------------------------------------------------------
# Reading the log back (ADR-0004 section 7)


def entry_json(line: bytes):
    """The JSON form of an entry whose payload names are known (ADR-0004 section 7, the forms
    of tools/state_reference.py, and `core:arrived { lineage_key }` of the design note of
    the arrival, section 5, the key as 16 hex digits); None for the other kinds (their
    bytes are pinned)."""
    c = pool_reference.Cursor(line)
    entry_id, t = c.varint(), c.signed()
    subjects = []
    for _ in range(c.varint()):
        assert c.varint() == 0
        subjects.append({"sim": c.varint()})
    causes = []
    for _ in range(c.varint()):
        kind = c.varint()
        if kind == 0:
            causes.append({"event": c.varint()})
        elif kind == 1:
            causes.append({"plan": {"by": c.varint(), "planned_at": c.signed()}})
        elif kind == 2:
            causes.append({"input": c.varint()})
        elif kind == 3:
            causes.append({"rule": c.string()})
        else:
            causes.append({"random": {name: hexed(c.varint()) for name in ("tag", "subject", "occasion", "detail")}})
    kind = c.varint()
    readers = {
        2: lambda: {"core:birthday": {"age_years": c.varint()}},
        4: lambda: {"core:death": {"cause": c.string()}},
        5: lambda: {"core:partnered": {"partnership": c.varint()}},
        9: lambda: {"core:separated": {"partnership": c.varint()}},
        10: lambda: {"core:household_formed": {"household": c.varint()}},
        11: lambda: {"core:moved": {"household": c.varint()}},
        12: lambda: {"core:emigrated": {"household": c.varint()}},
        wr.ARRIVED_VARIANT: lambda: {"core:arrived": {"lineage_key": hexed(c.varint())}},
    }
    if kind not in readers:
        return None
    payload = readers[kind]()
    assert c.left() == 0, line.hex()
    return json.dumps({"id": entry_id, "t": t, "subjects": subjects, "causes": causes, "payload": payload}, separators=(",", ":"))


KIND_NAMES = {
    0: "core:started", 1: "core:founder_added", 2: "core:birthday", 3: "core:param_changed", 4: "core:death",
    5: "core:partnered", 6: "core:conception", 7: "core:meiosis", 8: "core:birth", 9: "core:separated",
    10: "core:household_formed", 11: "core:moved", 12: "core:emigrated", 13: "core:arrived",
}


def entry_summary(line: bytes) -> str:
    """id, second, subjects and kind of an entry, for the readable rows."""
    event_id, t, subjects, kind, _ = pool_reference.parse_entry(line)
    return f"{event_id}@{t}:{KIND_NAMES[kind]}{subjects}"


def scenario_output(name: str, world: wr.World, saves: dict, entries: dict) -> list:
    rows = [f"{name}_save_{label} {save.hex()}" for label, save in saves.items()]
    for label, lines in entries.items():
        for line in lines:
            rows.append(f"{name}_{label}_entry {line.hex()}")
            text = entry_json(line)
            if text is not None:
                rows.append(f"{name}_{label}_entry_json {text}")
    rows.append(f"{name}_year_hashes {[hexed(value) for value in world.year_hashes]}")
    rows.append(f"{name}_log_hash {hexed(world.log_hash())} entries {len(world.log)}")
    rows.append(f"{name}_state_hash {hexed(world.state_hash())} clock {world.closed}")
    return rows


# ---------------------------------------------------------------------------------------
# Readable rows of a pool town


def env_kind(world: wr.World, sim: int) -> str:
    if world.is_arrival(sim):
        return "arrival"
    if world.is_pool_person(sim):
        return "pool_child"
    parents = world.sims[sim][7] if sim in world.sims else world.archive[sim][9]
    if parents is None:
        return "founder"
    pool_parents = [world.is_pool_person(parent) for parent in parents[:2]]
    return {2: "town_of_two_pool_persons", 1: "town_of_a_pool_and_a_town_person", 0: "town_of_town_persons"}[sum(pool_parents)]


def person_rows(name: str, world: wr.World) -> list:
    """Per sim (living and archived): its kind, keys, born, the origin and detail of its
    deaths, the seeds of `E`, `C`, wish and menarche and the keys `C` reads (note section 6.2),
    its PoolOrigin."""
    rows = []
    pool_seed = world.data.pool.seed
    seed_name = lambda value: "pool" if value == pool_seed and value != world.seed else ("world" if value == world.seed else hexed(value))
    for sim in sorted(set(world.sims) | set(world.archive)):
        e_seed, c_seed, parents = world.env_of(sim)
        origin = world.origins.get(sim)
        rows.append(
            f"{name}_person sim {sim} kind {env_kind(world, sim)} key {hexed(world.key_of(sim))} sex {'female' if world.sex_of(sim) == FEMALE else 'male'}"
            f" born {world.born_of(sim)} deaths_origin {world.origin_of(sim)} detail {world.death_detail(sim)}"
            f" E {seed_name(e_seed)} C {seed_name(c_seed)} C_keys {None if parents is None else [hexed(k) for k in parents]}"
            f" wish_menarche {seed_name(world.own_seed(sim))}"
            f" pool {None if origin is None else json.dumps(world.pool_view(sim), separators=(',', ':'))}"
            f" {'living' if sim in world.sims else ('died ' + str(world.archive[sim][1]) if world.archive[sim][2] is not None else 'emigrated')}"
        )
    return rows


def draw_rows(name: str, world: wr.World, occasion: int, used: list) -> list:
    pool = world.data.pool
    order = wr.pool_order(pool, world.seed, occasion, used)
    return [
        f"{name}_draw_order occasion {occasion} "
        + " ".join(f"{index}:{hexed(pool.candidates[index])}:{hexed(wr.rank(world.seed, wr.POOL_RANK, wr.WORLD_KEY, occasion, pool.candidates[index]))}:{pool.sizes[index]}" for index in order)
    ]


def arrival_rows(name: str, world: wr.World) -> list:
    """Per arrival (a household of `Start` or of an immigration, in rank order): where it
    came from, its sims, household and partnership, and its entries (bytes and JSON)."""
    rows = []
    for record in world.arrivals:
        rows.append(
            f"{name}_arrival t {record['t']} year {record['year']} by {record['why']} pool_household {record['index']} household {record['household']}"
            f" sims {record['sims']} partnership {record.get('partnership')} delta {record['delta']}"
            f" entries {[entry_summary(world.log[i - 1]) for i in record['entries']]}"
        )
        for i in record["entries"]:
            line = world.log[i - 1]
            rows.append(f"{name}_arrival_entry household {record['household']} {line.hex()}")
            if entry_json(line) is not None:
                rows.append(f"{name}_arrival_entry_json household {record['household']} {entry_json(line)}")
    for record in world.immigrations:
        rows.append(f"{name}_immigration t {record['t']} year {record['year']} q {record['q']} free_households {record['free']} free_members {record['free_members']} draw {record.get('draw')} empty {record.get('empty', False)}")
    return rows


def death_rows(name: str, world: wr.World) -> list:
    rows = []
    for sim, value in sorted(world.archive.items()):
        if value[2] is None:
            continue
        rows.append(
            f"{name}_death sim {sim} t {value[1]} cause {world.cause_code(value[2])} kind {env_kind(world, sim)}"
            f" detail {world.death_detail(sim)} origin {world.origin_of(sim)} arrival_year {world.origins.get(sim, {}).get('arrival_year')}"
        )
    return rows


def phi_row(name: str, world: wr.World, label: str, a: int, b: int) -> str:
    """`φ` of two sims over lineage keys with the set `S` of the query (sorted keys), the
    number of pairs the recursion visited, and `φ` if the records of the pool were not there
    (the town's persons only)."""
    value, s, visited = wr.kinship_by_keys(world.key_of(a), world.key_of(b), world.kin_of)
    assert value == world.kinship(a, b) and Fraction(world.kinship_of(a, b)) == value
    town = wr.kinship_by_keys(world.key_of(a), world.key_of(b), town_only(world))[0]
    in_town = sorted(hexed(key) for key in s if key in world.by_key)
    records = sorted(hexed(key) for key in s if key not in world.by_key)
    return (
        f"{name}_phi {label} sims {a} {b} keys {hexed(world.key_of(a))} {hexed(world.key_of(b))} phi {value} json {wr.json_number(float(value))}"
        f" S_town {in_town} S_records {records} visited {len(visited)} phi_without_records {town}"
    )


def town_only(world: wr.World):
    """The kin of a key with every parent that is no person of the town left out (to show
    what the records add)."""

    def kin(key):
        order, mother, father = world.kin_of(key)
        return order, (mother if mother in world.by_key else None), (father if father in world.by_key else None)

    return kin


def fired_rows(name: str, world: wr.World, t: int) -> list:
    """The events that ran in second `t` in their order, with their keys (time, phase, micro
    step, class, subject kind, subject, kind, insertion number) and the first entry each
    wrote (ADR-0002; note section 5, the micro steps)."""
    rows = []
    fired = [(key, first) for key, first in world.fired if key[0] == t]
    for index, (key, first) in enumerate(fired):
        following = fired[index + 1][1] if index + 1 < len(fired) else None
        rows.append(f"{name}_fired t {t} micro {key[2]} class {key[3]} subject {key[4]}:{key[5]} kind {key[6]} seq {key[7]} first_entry {first} next_entry {following}")
    return rows


def pedigree_rows(name: str, label: str, world: wr.World) -> list:
    text = world.pedigree_json()
    return [f"{name}_pedigree_{label}_summary clock {world.closed} length {len(text.encode())} hash {hexed(hash64(text.encode()))}", f"{name}_pedigree_{label} {text}"]


def start_record(world: wr.World) -> bytes:
    (record,) = world.inputs[:1]
    return record


def scenario_json(seed: int, n_start: int, immigration: bool, until: int) -> str:
    """The scenario of format 5 (note section 3) the CLI would run (informative: the field
    names follow tests/scenarios/dummy.json and the note; the CLI fills `pool_hash` and the
    households from the data)."""
    return json.dumps(
        {
            "format_version": 5,
            "start": {
                "world_seed": hexed(seed),
                "founder_tag": TAG,
                "world_params": {"hereditary_diseases": True, "nutrition": []},
                "founders": [],
                "couples": [],
                "pool": {"n_start": n_start, "immigration": immigration},
            },
            "until": until,
        },
        separators=(",", ":"),
    )


def config_row(name: str, pool_name: str, data: wr.Data, seed: int, n_start: int, immigration: bool, end: int) -> str:
    return (
        f"{name}_config pool {pool_name} pool_hash {hexed(data.pool.hash)} pool_seed {hexed(data.pool.seed)} frozen_at {data.pool.frozen_at}"
        f" data_hash {hexed(data.hash)} seed {hexed(seed)} tag {TAG} n_start {n_start} immigration {immigration}"
        f" q {wr.immigration_persons(n_start)} switch on nutrition [] end {end}"
    )


def common_rows(name: str, pool_name: str, world: wr.World, n_start: int, immigration: bool, end: int) -> list:
    rows = [config_row(name, pool_name, world.data, world.seed, n_start, immigration, end)]
    rows.append(f"{name}_scenario_json {scenario_json(world.seed, n_start, immigration, end)}")
    rows.append(f"{name}_start_input {start_record(world).hex()}")
    rows += draw_rows(name, world, 0, [])
    rows += arrival_rows(name, world)
    rows += death_rows(name, world)
    rows += person_rows(name, world)
    rows.append(f"{name}_used {world.used}")
    rows.append(f"{name}_partnerships {[(n, v[0], v[1], v[2], v[3], 'arrival' if world.arrival_partnership(n) else 'town') for n, v in sorted(world.partnerships.items())]}")
    rows.append(f"{name}_carers {[row for row in world.carer_log if row[4] != 'birth']}")
    for record in world.placements:
        rows.append(
            f"{name}_placement t {record['t']} trigger {record['trigger']} group {record['group']} stage {record['stage']} carer {record['carer']}"
            f" added {record['added']} relatives {[(sim, str(phi), hexed(key)) for sim, phi, key in record['relatives']]}"
        )
    return rows


# ---------------------------------------------------------------------------------------
# The scenario "arrival" (pool_c, n_start 50, immigration, seed 0xff)

ARRIVAL_POOL = "pool_c"
ARRIVAL_SEED, ARRIVAL_N_START = 0xFF, 50
ARRIVAL_STOP_YEAR = 14


def arrival_world() -> wr.World:
    return make_world(town_data(golden_pool(ARRIVAL_POOL)), ARRIVAL_SEED, ARRIVAL_N_START, True)


def arrival_marks(world: wr.World) -> dict:
    """The seconds of the cases in a run of "arrival" (found in the run, then pinned): the
    immigration of year 0 at second 0 and of year 1; the birth of the pool child whose
    father is a record only (its `core:birth` without the father); the first child born of a
    conception in the town by two pool persons; the carried separation; the death of the
    immigrant of year 1."""
    import pool_reference as pr

    without_father = [pr.parse_entry(line)[1] for line in world.log if pr.parse_entry(line)[3] == 8 and len(pr.parse_entry(line)[2]) == 2]
    town = sorted(world.born_of(sim) for sim in set(world.sims) | set(world.archive) if env_kind(world, sim) == "town_of_two_pool_persons")
    carried = [t for t, number, *_ in world.separations if world.arrival_partnership(number)]
    deaths = sorted(value[1] for sim, value in world.archive.items() if value[2] is not None and world.is_arrival(sim) and world.death_detail(sim) > 0)
    return {
        "immigration_0": 0,
        "pool_child_without_father": without_father[0],
        "town_child_of_pool_persons": town[0],
        "immigration_1": YEAR,
        "carried_separation": carried[0],
        "immigrant_death": deaths[0],
    }


ARRIVAL_MARKS = {
    "immigration_0": 0,
    "pool_child_without_father": 14515200,
    "town_child_of_pool_persons": 26611200,
    "immigration_1": YEAR,
    "carried_separation": 84829122,
    "immigrant_death": 183152461,
}


# The saves the further rejected states start from, as their sections pinned them (label:
# bytes), so that `more_broken_rows` can assert it starts from the same states.
BASE_SAVES = {}


def arrival_rows_all() -> list:
    probe = arrival_world()
    probe.advance(ARRIVAL_STOP_YEAR * YEAR - 1)
    assert arrival_marks(probe) == ARRIVAL_MARKS, arrival_marks(probe)
    end = ARRIVAL_STOP_YEAR * YEAR - 1
    world, saves, entries, stopped = staged(arrival_world, ARRIVAL_MARKS, [], end, panic=ARRIVAL_STOP_YEAR * YEAR)
    BASE_SAVES["arrival_save_after_start"] = saves["after_start"]
    BASE_SAVES["arrival_save_after_pool_child_without_father"] = saves["after_pool_child_without_father"]
    name = "arrival"
    rows = common_rows(name, ARRIVAL_POOL, world, ARRIVAL_N_START, True, end)
    rows += [f"{name}_mark {label} {t}" for label, t in ARRIVAL_MARKS.items()]
    rows += draw_rows(name, world, ARRIVAL_STOP_YEAR - 1, world.used[:-1])
    rows += scenario_output(name, world, saves, entries)
    rows += fired_rows(name, world, 0)
    rows += pool_child_rows(name, world)
    rows += pedigree_rows(name, "end", world)
    code, at, written = stopped
    rows.append(f"{name}_not_stopped through {end} (the end of year {ARRIVAL_STOP_YEAR - 1}): the save {name}_save_end")
    rows.append(f"{name}_stopped invariant {code} at {at} year {at // YEAR} entries_before_the_stop {[line.hex() for line in written]} immigration {world.immigrations[-1]}")
    return rows


def pool_child_rows(name: str, world: wr.World) -> list:
    """`φ` of each pool child (born in the town of a pool pregnancy) with its mother, and of
    the child whose father is a record only with that record (no person of the town)."""
    rows = []
    for sim in sorted(set(world.sims) | set(world.archive)):
        if env_kind(world, sim) != "pool_child":
            continue
        origin = world.origins[sim]
        mother = world.by_key[origin["mother"]]
        father = world.by_key.get(origin["father"])
        rows.append(phi_row(name, world, f"pool_child_{sim}_mother", sim, mother))
        if father is not None:
            rows.append(phi_row(name, world, f"pool_child_{sim}_father", sim, father))
        else:
            value, s, _ = wr.kinship_by_keys(world.key_of(sim), origin["father"], world.kin_of)
            rows.append(f"{name}_phi pool_child_{sim}_father_record keys {hexed(world.key_of(sim))} {hexed(origin['father'])} phi {value} S {sorted(hexed(k) for k in s)} (the father is a record only)")
    return rows


# ---------------------------------------------------------------------------------------
# "orphans" and "records" (pool_d, n_start 30)

KIN_POOL, KIN_N_START = "pool_d", 30
ORPHANS_SEED, RECORDS_SEED = 5, 6


def kin_world(seed: int):
    return lambda: make_world(town_data(golden_pool(KIN_POOL)), seed, KIN_N_START, True)


ORPHANS_MARKS = {"succession_first": 17104015, "succession_second": 33206750, "orphan_rule": 119487822}
ORPHANS_END = 5 * YEAR - 1


def orphans_rows_all() -> list:
    world, saves, entries, _ = staged(kin_world(ORPHANS_SEED), ORPHANS_MARKS, [], ORPHANS_END)
    BASE_SAVES["orphans_save_end"] = saves["end"]
    name = "orphans"
    succession = [(t, child, old, new) for t, child, old, new, why in world.carer_log if why == "succession: the other parent"]
    assert [row[0] for row in succession] == [17104015, 33206750, 33206750], succession
    for t, child, old, new in succession:
        # The other parent is found by the child's PoolOrigin keys among the members of its
        # household (note section 7): the new carer's key is the child's mother or father.
        assert world.key_of(new) in world.biological_keys(child) and world.is_pool_person(child)
    (placement,) = world.placements
    assert placement["t"] == ORPHANS_MARKS["orphan_rule"] and placement["stage"] == "relative"
    rows = common_rows(name, KIN_POOL, world, KIN_N_START, True, ORPHANS_END)
    rows += [f"{name}_mark {label} {t}" for label, t in ORPHANS_MARKS.items()]
    rows += [f"{name}_succession t {t} child {child} carer_was {old} carer {new} by_key {hexed(world.key_of(new))} child_pool_parents {[hexed(k) if k else None for k in world.biological_keys(child)]}" for t, child, old, new in succession]
    rows.append(phi_row(name, world, "orphan_and_carer", placement["representative"], placement["carer"]))
    rows += scenario_output(name, world, saves, entries)
    rows += pedigree_rows(name, "end", world)
    return rows


RECORDS_MARKS = {"orphan_rule": 48234390, "mixed_child": 595123200}
# ArrangePartnership of the arrival sim 26 (a woman, 9.2 at her arrival) and the town-born sim
# 31 (a man of two pool persons, 16 at 17.38 years), acting at 18 years.
RECORDS_INPUTS = [("arrange", 18 * YEAR, 26, 31)]
RECORDS_END = 19 * YEAR - 1


def records_rows_all() -> list:
    world, saves, entries, _ = staged(kin_world(RECORDS_SEED), RECORDS_MARKS, RECORDS_INPUTS, RECORDS_END)
    BASE_SAVES["records_save_after_arrange"] = saves["after_arrange"]
    name = "records"
    (placement,) = world.placements
    assert placement["t"] == RECORDS_MARKS["orphan_rule"] and placement["stage"] == "relative"
    rep, carer = placement["representative"], placement["carer"]
    only_records = wr.kinship_by_keys(world.key_of(rep), world.key_of(carer), town_only(world))[0]
    assert only_records == 0 and world.kinship(rep, carer) > 0, "the relative is found only through records"
    mixed = [sim for sim in sorted(world.sims) if env_kind(world, sim) == "town_of_a_pool_and_a_town_person"]
    assert mixed and world.born_of(mixed[0]) == RECORDS_MARKS["mixed_child"]
    rows = common_rows(name, KIN_POOL, world, KIN_N_START, True, RECORDS_END)
    rows += [f"{name}_mark {label} {t}" for label, t in RECORDS_MARKS.items()]
    rows += [f"{name}_input {label} acts {acts} stamped {acts - 1} woman {woman} man {man}" for label, acts, woman, man in RECORDS_INPUTS]
    rows.append(phi_row(name, world, "orphan_and_relative_only_through_records", rep, carer))
    rows.append(phi_row(name, world, "arranged_couple", RECORDS_INPUTS[0][2], RECORDS_INPUTS[0][3]))
    # A town-born grandchild and a pool cousin: the first pair (town-born of two pool persons,
    # an arrival) by SimIds whose `φ` is 1/16 and needs the records (less without them).
    town = [sim for sim in sorted(set(world.sims) | set(world.archive)) if env_kind(world, sim) == "town_of_two_pool_persons"]
    arrivals = [sim for sim in sorted(set(world.sims) | set(world.archive)) if world.is_arrival(sim)]
    cousins = [
        (g, k)
        for g in town
        for k in arrivals
        if world.kinship(g, k) == Fraction(1, 16) and wr.kinship_by_keys(world.key_of(g), world.key_of(k), town_only(world))[0] < Fraction(1, 16)
    ]
    assert cousins, "a town-born grandchild and a pool cousin"
    rows.append(phi_row(name, world, "town_born_grandchild_and_pool_cousin", *cousins[0]))
    rows.append(phi_row(name, world, "mixed_child_and_its_mother", mixed[0], RECORDS_INPUTS[0][2]))
    rows.append(phi_row(name, world, "mixed_child_and_the_pool_cousin", mixed[0], cousins[0][1]))
    rows.append(phi_row(name, world, "arrival_self", arrivals[0], arrivals[0]))
    rows += scenario_output(name, world, saves, entries)
    rows += pedigree_rows(name, "end", world)
    return rows


# ---------------------------------------------------------------------------------------
# "micro": a carried event exactly at the arrival second

MICRO_YEAR = 3
MICRO_POOL_HOUSEHOLD = 41


def micro_pool() -> tuple:
    """pool_c with the carried separation of the woman of its household 41 (who arrives in
    year 3 in "arrival") moved to `E_p`: after the shift it is due exactly at her arrival
    second. Returns (bytes, her key)."""
    file = pool_reference.read_pool(golden_pool(ARRIVAL_POOL))
    household = file["households"][MICRO_POOL_HOUSEHOLD]
    (woman,) = [m for m in household["members"] if m["sex"] == 0 and m["separation_at"] is not None]
    return with_carried_at_frozen(golden_pool(ARRIVAL_POOL), woman["lineage_key"], "separation_at"), woman["lineage_key"]


def micro_world() -> wr.World:
    data, _ = micro_pool()
    return make_world(town_data(data), ARRIVAL_SEED, ARRIVAL_N_START, True)


def micro_rows_all() -> list:
    data, woman_key = micro_pool()
    at = MICRO_YEAR * YEAR
    end = (MICRO_YEAR + 1) * YEAR - 1
    world, saves, entries, _ = staged(micro_world, {"arrival_with_carried_event": at}, [], end)
    reference = arrival_world()
    reference.advance(at - 1)
    # Until the arrival second the run is that of "arrival" (only the data hash differs).
    assert world.log[: len(reference.log)] == reference.log
    woman = world.by_key[woman_key]
    number = world.open_partnership(woman) if world.open_partnership(woman) is not None else [n for n, v in world.partnerships.items() if v[0] == woman][0]
    separated = [(t, n) for t, n, *_ in world.separations if n == number]
    assert separated == [(at, number)], separated
    micro = [key for key, _ in world.fired if key[0] == at and key[2] == 1]
    assert micro and all(key[6] == wr.SEPARATION_KIND for key in micro)
    name = "micro"
    rows = [f"{name}_pool_file constructed from {ARRIVAL_POOL}: household {MICRO_POOL_HOUSEHOLD} woman {hexed(woman_key)} separation_at = frozen_at; bytes {len(data)} pool_hash {hexed(hash64(data))} hex {data.hex()}"]
    rows += common_rows(name, "micro_pool", world, ARRIVAL_N_START, True, end)
    rows.append(f"{name}_mark arrival_with_carried_event {at} woman {woman} partnership {number}")
    rows += fired_rows(name, world, at)
    rows += scenario_output(name, world, saves, entries)
    return rows


# ---------------------------------------------------------------------------------------
# "empty_at_start": a pool too small for `n_start`

EMPTY_POOL, EMPTY_N_START = "pool_b", 50


def empty_rows_all() -> list:
    data = town_data(golden_pool(EMPTY_POOL))
    world = make_world(data, ARRIVAL_SEED, EMPTY_N_START, True)
    check_all(world)
    assert world.used == wr.pool_order(data.pool, ARRIVAL_SEED, 0, []) and sum(data.pool.sizes) < EMPTY_N_START
    save = world.save()
    before = len(world.log)
    try:
        world.advance(0)
    except wr.Invariant as error:
        stopped = (error.code, world.immigrations[-1])
    else:
        raise AssertionError("the pool is not empty")
    name = "empty_at_start"
    rows = [config_row(name, EMPTY_POOL, data, ARRIVAL_SEED, EMPTY_N_START, True, 0)]
    rows.append(f"{name}_start_input {start_record(world).hex()}")
    rows += draw_rows(name, world, 0, [])
    rows.append(f"{name}_used {world.used} members {sum(data.pool.sizes)} < n_start {EMPTY_N_START}: Start takes every household")
    rows.append(f"{name}_save_after_start {save.hex()}")
    for line in world.log[:before]:
        rows.append(f"{name}_start_entry {line.hex()}")
        if entry_json(line) is not None:
            rows.append(f"{name}_start_entry_json {entry_json(line)}")
    rows.append(f"{name}_stopped invariant {stopped[0]} at 0 (the immigration of year 0) record {stopped[1]} entries_written {len(world.log) - before}")
    return rows


# ---------------------------------------------------------------------------------------
# The rejections of `Start` (note section 3) and of the data check (section 2)


def start_input(seed: int, founders: list, couples: list, capacity, pool) -> bytes:
    """The input record of `Start` at stamp −1, seq 1 (the first entry of the input log)."""
    return (
        signed(-1)
        + varint(1)
        + variant(
            0,
            varint(seed),
            string(TAG),
            world_params(True),
            seq([wr.founder(born, sex, genes) for born, sex, genes in founders]),
            seq([wr.couple(woman, man) for woman, man in couples]),
            wr.capacity_bytes(capacity),
            wr.pool_start_bytes(pool),
        )
    )


def start_rejection_rows() -> list:
    """Each rejected `Start` (the first rule that applies, in the note's order after the old
    checks): its case, the code, the input bytes. On the data with pool_c unless named."""
    seed = ARRIVAL_SEED
    data = town_data(golden_pool(ARRIVAL_POOL))
    plain = wr.Data(*template_documents())
    other = town_data(golden_pool(EMPTY_POOL))
    good = wr.pool_start(data, seed, ARRIVAL_N_START, True)
    founder = [(-30 * YEAR, FEMALE, [])]
    man = [(-30 * YEAR, FEMALE, []), (-31 * YEAR, wr.MALE, [])]

    def changed(**fields):
        out = dict(good)
        out.update(fields)
        return out

    drawn = good["households"]
    altered = copy.deepcopy(drawn)
    altered[0]["members"][0]["conceptions"] += 1
    other_seed = wr.pool_start(data, 0xFE, ARRIVAL_N_START, True)["households"]
    extra = drawn + [data.pool.households[wr.pool_order(data.pool, seed, 0, [])[len(drawn)]]]
    cases = [
        ("founders_with_pool", data, founder, [], None, good),
        ("founders_and_couple_with_pool", data, man, [(0, 1)], None, good),
        ("no_pool_in_the_data", plain, [], [], None, good),
        ("pool_hash_not_the_data_pool", data, [], [], None, changed(pool_hash=data.pool.hash ^ 1)),
        ("pool_of_other_data", other, [], [], None, good),
        ("n_start_0_no_households", data, [], [], None, changed(n_start=0, households=[])),
        ("n_start_0_with_households", data, [], [], None, changed(n_start=0)),
        ("households_reversed", data, [], [], None, changed(households=list(reversed(drawn)))),
        ("households_one_missing", data, [], [], None, changed(households=drawn[:-1])),
        ("households_one_extra", data, [], [], None, changed(households=extra)),
        ("households_of_another_seed", data, [], [], None, changed(households=other_seed)),
        ("households_one_altered", data, [], [], None, changed(households=altered)),
        ("order_founders_before_missing_pool", plain, founder, [], None, changed(pool_hash=1)),
        ("order_missing_pool_before_n_start", plain, [], [], None, changed(n_start=0)),
        ("order_n_start_before_households", data, [], [], None, changed(n_start=0, households=list(reversed(drawn)))),
        ("decision_capacity_zero_before_pool", data, [], [], 0, changed(pool_hash=1)),
        ("decision_couple_checks_before_pool", data, [], [(0, 1)], None, good),
    ]
    rows = []
    for name, case_data, founders, couples, capacity, pool in cases:
        code = wr.start_rejection(founders, couples, case_data, capacity, pool, seed)
        assert code is not None, name
        record = start_input(seed, founders, couples, capacity, pool)
        rows.append(
            f"start_reject {name} code {code} {wr.REJECT_NAMES[code]} data_hash {hexed(case_data.hash)} input_bytes {len(record)} input_hash {hexed(hash64(record))} input {record.hex()}"
        )
    # The accepted input of "arrival" for comparison.
    assert wr.start_rejection([], [], data, None, good, seed) is None
    world = arrival_world()
    assert start_record(world) == start_input(seed, [], [], None, good)
    rows.append(f"start_accepted arrival input {start_record(world).hex()}")
    return rows


def without_locus(documents: tuple, locus: str) -> tuple:
    demography, calibration, genome_map = (copy.deepcopy(doc) for doc in documents)
    genome_map["loci"] = [entry for entry in genome_map["loci"] if entry["id"] != locus]
    return demography, calibration, genome_map


def modified_pool(changes) -> bytes:
    """pool_c with `changes(file)` applied, written again (it passes the reader)."""
    file = pool_reference.read_pool(golden_pool(ARRIVAL_POOL))
    changes(file)
    data = pool_reference.encode_pool(file)
    pool_reference.read_pool(data)
    return data


def data_rejection_rows() -> list:
    """Each pool the data check rejects (note section 2, in its order), from pool_c, and the
    reader's rejected files of tests/golden/pool/reject, which the data cannot hold either."""
    file = pool_reference.read_pool(golden_pool(ARRIVAL_POOL))
    # A dependent child of household 21 and a member of household 20.
    (child,) = [m for m in file["households"][21]["members"] if m["carer"] is not None]
    stranger = file["households"][20]["members"][0]["lineage_key"]
    single = file["households"][0]["members"][0]
    unborn = file["households"][28]["pregnancies"][0]["children"][0]

    def carer(f):
        for m in f["households"][21]["members"]:
            if m["lineage_key"] == child["lineage_key"]:
                m["carer"] = stranger

    def sex(f):
        f["households"][0]["members"][0]["sex"] = 1 - single["sex"]

    def unborn_sex(f):
        f["households"][28]["pregnancies"][0]["children"][0]["sex"] = 1 - unborn["sex"]

    def both(f):
        carer(f)
        sex(f)

    template = template_documents()
    cases = [
        ("carer_not_member", modified_pool(carer), template, f"household 21 member {hexed(child['lineage_key'])} carer {hexed(stranger)} of household 20"),
        ("sex_of_a_member", modified_pool(sex), template, f"household 0 member {hexed(single['lineage_key'])} sex flipped"),
        ("sex_of_an_unborn_child", modified_pool(unborn_sex), template, f"household 28 unborn {hexed(unborn['lineage_key'])} sex flipped"),
        ("locus_mc1r_missing", golden_pool(ARRIVAL_POOL), without_locus(template, "core:mc1r"), "the map without core:mc1r"),
        ("locus_herc2_missing", golden_pool(ARRIVAL_POOL), without_locus(template, "core:herc2"), "the map without core:herc2"),
        ("order_carer_before_sex", modified_pool(both), template, "both"),
        ("order_sex_before_locus", modified_pool(sex), without_locus(template, "core:mc1r"), "both"),
        ("order_carer_before_locus", modified_pool(carer), without_locus(template, "core:herc2"), "both"),
    ]
    rows = []
    for name, pool, documents, how in cases:
        try:
            wr.Data(*documents, pool=pool)
        except wr.PoolDataRejected as error:
            rows.append(f"data_reject {name} code {error.code} how {how} pool_bytes {len(pool)} pool_hash {hexed(hash64(pool))} pool {pool.hex()}")
            continue
        raise AssertionError(f"the data check accepts {name}")
    for path in sorted((POOLS / "reject").glob("*.bin")):
        try:
            wr.Data(*template, pool=path.read_bytes())
        except pool_reference.Rejected as error:
            rows.append(f"data_reject_reader {path.name} rule {error.rule}")
            continue
        raise AssertionError(path)
    return rows


def carried_before_pool() -> tuple:
    """pool_c with the carried `leave_home` of the dependent child of household 33 (which
    `Start` of "arrival" draws) one second before `E_p`: the arrival breaks the invariant
    `pool.carried_before_arrival` (note section 5). Returns (bytes, the child's key)."""
    file = pool_reference.read_pool(golden_pool(ARRIVAL_POOL))
    (child,) = [m for m in file["households"][33]["members"] if m["leave_home_at"] is not None]
    child["leave_home_at"] = file["frozen_at"] - 1
    data = pool_reference.encode_pool(file)
    pool_reference.read_pool(data)
    return data, child["lineage_key"]


def invariant_rows() -> list:
    pool, key = carried_before_pool()
    data = town_data(pool)
    start = wr.pool_start(data, ARRIVAL_SEED, ARRIVAL_N_START, True)
    try:
        wr.World(ARRIVAL_SEED, TAG, True, [], (), data, pool=start)
    except wr.Invariant as error:
        return [
            f"invariant_carried_before_arrival code {error.code} detail {error} pool household 33 child {hexed(key)} leave_home_at frozen_at - 1;"
            f" Start of \"arrival\" on it (seed 0xff, n_start 50) panics; data_hash {hexed(data.hash)} start_input {start_input(ARRIVAL_SEED, [], [], None, start).hex()}"
            f" pool_bytes {len(pool)} pool_hash {hexed(hash64(pool))} pool {pool.hex()}"
        ]
    raise AssertionError("no invariant break")


# ---------------------------------------------------------------------------------------
# States the load checks of section 9 reject


def rejecting_checks(world: wr.World) -> list:
    """The model's load checks that reject the state; a check that fails by another error
    than an assertion (a broken state it cannot even read) is named with `!` and the error."""
    names = []
    for check in (wr.check_households, wr.check_separations, wr.check_market, wr.check_afflictions, wr.check_emigration, wr.check_pool, wr.check_deaths):
        try:
            check(world)
        except AssertionError:
            names.append(check.__name__)
        except Exception as error:  # noqa: BLE001 (a crash of a check is a rejection too)
            names.append(f"{check.__name__}!{type(error).__name__}")
    return names


def broken_rows() -> list:
    """States of "arrival" at the end of year 2 (clock 3 · YEAR − 1) changed in one respect,
    each a save with the load checks that reject it."""
    base = arrival_world()
    base.advance(3 * YEAR - 1)
    check_all(base)
    copy_world = lambda: copy.deepcopy(base, {id(base.data): base.data})
    cases = []

    broken = copy_world()
    arrival = 52
    broken.sims[arrival][0] += 1
    cases.append(("arrival_born_off_by_one", f"sim {arrival} born {broken.sims[arrival][0]}", broken))

    broken = copy_world()
    broken.used.append(broken.used[0])
    cases.append(("used_household_twice", f"used {broken.used}", broken))

    broken = copy_world()
    (child,) = [sim for sim in sorted(broken.sims) if env_kind(broken, sim) == "pool_child"][:1]
    broken.origins[child]["arrived"] += 1
    cases.append(("pool_child_arrived_not_born", f"sim {child}", broken))

    broken = copy_world()
    broken.origins[61]["arrived"] += 1
    broken.sims[61][0] += 1
    cases.append(("arrival_not_at_a_year_start", f"sim 61 arrived {broken.origins[61]['arrived']}", broken))

    broken = copy_world()
    broken.origins[61]["arrival_year"] = 2
    cases.append(("arrival_year_not_its_year", "sim 61 arrival_year 2", broken))

    broken = copy_world()
    town = [mother for mother in sorted(broken.pregnancies) if mother not in broken.pool_fathers]
    if town:
        value = broken.pregnancies[town[0]]
        broken.pregnancies[town[0]] = (None,) + value[1:]
        cases.append(("town_pregnancy_without_father", f"mother {town[0]}", broken))

    broken = copy_world()
    broken.partnerships[1][2] += 1
    cases.append(("arrival_partnership_formed_off", f"partnership 1 formed {broken.partnerships[1][2]}", broken))

    broken = copy_world()
    del broken.origins[52]
    cases.append(("arrival_without_pool", "sim 52 pool None", broken))

    rows = []
    for name, how, world in cases:
        checks = rejecting_checks(world)
        assert checks, f"no load check rejects {name}"
        rows.append(f"load_reject {name} how {how} checks {checks} save {world.save().hex()}")
    rows.append(f"load_base arrival at {base.closed} save {base.save().hex()}")
    BASE_SAVES["load_base"] = base.save()
    return rows


def unstarted_save(data: wr.Data, used: list) -> bytes:
    """The save of a world that never started (no input) on `data`, with `used` as its used
    households (the last field of the projection): the empty save of the model with the
    data's hash."""
    empty = wr.used_households_bytes([])
    assert wr.EMPTY_PROJECTION.endswith(empty)
    projection = wr.EMPTY_PROJECTION[: -len(empty)] + wr.used_households_bytes(used)
    return wr.header(data.hash) + projection + seq([]) + varint(1) + seq([]) + seq([])


def unstarted_rejecting_checks(next_household_id: int, households: dict, used: list) -> list:
    """The load checks of a world that never started that reject its state."""
    names = []
    for check, args in ((wr.check_unstarted_households, (next_household_id, households)), (wr.check_unstarted_pool, (used,))):
        try:
            check(*args)
        except AssertionError:
            names.append(check.__name__)
    return names


def more_broken_rows() -> list:
    """Further states the rules of the note can never produce (review of the reference,
    NOTES2.md), each a save changed in one respect from a pinned state, with the load checks
    that reject it: `load_reject_more <name> base <label of the pinned save> clock <clock> how
    <what changed> checks [...] save <hex>`; a case the scenarios do not hold is a row
    `load_reject_more_skipped <name> reason ...`."""
    copy_world = lambda world: copy.deepcopy(world, {id(world.data): world.data})
    rows = []

    def row(name: str, base: str, world: wr.World, how: str) -> None:
        checks = rejecting_checks(world)
        assert checks, f"no load check rejects {name}"
        rows.append(f"load_reject_more {name} base {base} clock {world.closed} how {how} checks {checks} save {world.save().hex()}")

    # (a) "arrival" right after `Start`: the first mother in SimId order whose pool father
    # arrived in her household or an earlier one (attached, decision 8) loses him.
    start = arrival_world()
    check_all(start)
    assert BASE_SAVES.get("arrival_save_after_start", start.save()) == start.save()
    used, members = start.used, start.data.pool.members

    def father_came_no_later(mother: int) -> bool:
        his = members[start.pool_fathers[mother]][0] if start.pool_fathers[mother] in members else None
        return his is not None and his in used and used.index(his) <= used.index(members[start.key_of(mother)][0])

    attached = [mother for mother in sorted(start.pregnancies) if mother in start.pool_fathers and father_came_no_later(mother)]
    mother = attached[0]
    father = start.pregnancies[mother][0]
    assert (mother, father) == (9, 8), (mother, father)
    household = members[start.key_of(mother)][0]
    assert members[start.pool_fathers[mother]][0] == household
    broken = copy_world(start)
    broken.pregnancies[mother] = (None,) + broken.pregnancies[mother][1:]
    row(
        "carried_father_dropped", "arrival_save_after_start", broken,
        f"mother {mother} (pool household {household}, arrival position {used.index(household)}) father {father} (key {hexed(start.pool_fathers[mother])}, the same household) set to None",
    )

    # (b) A pool pregnancy whose father arrives only in a later household or year: none in
    # the pools of the scenarios.
    late = []
    for pool_name, scenarios in (("pool_c", "arrival, micro"), ("pool_d", "orphans, records")):
        pool = town_data(golden_pool(pool_name)).pool
        for index, household_ in enumerate(pool.households):
            for pregnancy in household_["pregnancies"]:
                his = pool.members[pregnancy["father"]][0] if pregnancy["father"] in pool.members else None
                if his != index:
                    late.append((pool_name, scenarios, index, pregnancy["father"], his))
    assert late == [("pool_c", "arrival, micro", 28, 0xB9B8567FF6EC1066, None)], late
    rows.append(
        "load_reject_more_skipped carried_father_attached_late reason no pool pregnancy of pool_c (arrival, micro) or pool_d (orphans, records)"
        " has its father in another household of the pool: every father is a member of the mother's household (attached at her arrival)"
        f" but the father {hexed(late[0][3])} of pool_c household 28, a record only (he died in the pool run), who never arrives"
    )

    # (c) "records" right after the arrangement of sims 26 and 31 (the input stamped
    # 18 · YEAR − 1): woman 26, an arrival, has a partnership formed in the town; her series of
    # attempts (planned by the town's rule) is removed with its plan reference.
    label, acts, woman, man = RECORDS_INPUTS[0]
    arranged = kin_world(RECORDS_SEED)()
    arranged.advance(acts - 1)
    arranged.arrange(woman, man)
    check_all(arranged)
    assert BASE_SAVES.get("records_save_after_arrange", arranged.save()) == arranged.save()
    number = arranged.open_partnership(woman)
    assert arranged.is_arrival(woman) and arranged.arrival_partnership(number) is None and arranged.partnerships[number][2] == acts
    (series,) = [event for event in arranged.queue if event.kind == wr.ATTEMPT_KIND and event.subject == (wr.SIM_SUBJECT, woman)]
    assert series.time == wr.month_start_after(acts) == arranged.series_due(woman)
    broken = copy_world(arranged)
    broken.queue = [event for event in broken.queue if not (event.kind == wr.ATTEMPT_KIND and event.subject == (wr.SIM_SUBJECT, woman))]
    plans = len(arranged.queue) - len(broken.queue)
    assert plans == 1
    row(
        "town_series_missing", "records_save_after_arrange", broken,
        f"woman {woman} (arrival) partnership {number} formed {acts} with sim {man} in the town; her series at {series.time} (plan by {series.plan[0]} at {series.plan[1]}) removed from the queue with its plan reference",
    )

    # (d) "arrival" at the end of year 2: the last two used households swapped.
    base = arrival_world()
    base.advance(3 * YEAR - 1)
    check_all(base)
    assert BASE_SAVES.get("load_base", base.save()) == base.save()
    years = {record["index"]: record["year"] for record in base.arrivals if record["why"] == "immigration"}
    broken = copy_world(base)
    i, j = len(broken.used) - 2, len(broken.used) - 1
    a, b = broken.used[i], broken.used[j]
    assert (a, b) == (5, 12) and (years[a], years[b]) == (1, 2), (a, b)
    broken.used[i], broken.used[j] = b, a
    row(
        "immigration_order_swapped", "load_base", broken,
        f"used positions {i} and {j} (households {a} of year {years[a]} and {b} of year {years[b]}) swapped: used {broken.used}",
    )

    # (e) A world that never started, on the data with pool_c, with the used households [0].
    data = town_data(golden_pool(ARRIVAL_POOL))
    assert unstarted_rejecting_checks(1, {}, []) == []
    checks = unstarted_rejecting_checks(1, {}, [0])
    assert checks
    rows.append(
        f"load_reject_more unstarted_with_used_household base unstarted (no input, data with pool_c, data_hash {hexed(data.hash)}, save {unstarted_save(data, []).hex()})"
        f" clock -1 how used [0] instead of [] checks {checks} save {unstarted_save(data, [0]).hex()}"
    )
    return rows


# ---------------------------------------------------------------------------------------
# A carried leaving home due exactly at second 0 (accepted)


def leave_home_at_start_pool() -> tuple:
    """pool_c with the carried `leave_home` of the dependent child of household 33 (the
    household and child of `carried_before_pool`) at exactly `E_p`: after the shift it is
    due at second 0, the arrival second of `Start`, which is no invariant break (note
    section 5). Returns (bytes, the child's key, its old `leave_home_at`)."""
    file = pool_reference.read_pool(golden_pool(ARRIVAL_POOL))
    (child,) = [m for m in file["households"][33]["members"] if m["leave_home_at"] is not None]
    old = child["leave_home_at"]
    child["leave_home_at"] = file["frozen_at"]
    data = pool_reference.encode_pool(file)
    pool_reference.read_pool(data)
    return data, child["lineage_key"], old


def leave_home_at_start_rows() -> list:
    """`Start` of "arrival" (seed 0xff, `n_start` 50, immigration on) on `leave_home_at_start_pool`:
    the child (a newborn of household 33, which `Start` draws) has its leaving home planned at
    second 0 (micro step 0: `Start` is an input); it runs after the immigration of year 0
    (class 100) in that second. Both saves, right after `Start` (clock −1, the child not moved
    out, its `leave_home` pending at 0) and after second 0 (moved out), pass every load check
    of the model."""
    pool, key, old = leave_home_at_start_pool()
    data = town_data(pool)
    world = make_world(data, ARRIVAL_SEED, ARRIVAL_N_START, True)
    sim = world.by_key[key]
    frozen = data.pool.frozen_at
    name = "leave_home_at_start"
    assert hash64(pool) == data.pool.hash
    rows = [
        f"{name} pool constructed from {ARRIVAL_POOL}: household 33 child {hexed(key)} (sim {sim}, born_pool {world.origins[sim]['born_pool']}) leave_home_at {frozen} (= frozen_at; was {old})"
        f" pool_bytes {len(pool)} pool_xxh3 {hexed(hash64(pool))} pool {pool.hex()}",
        f"{name} data_hash {hexed(data.hash)} pool_hash {hexed(data.pool.hash)} pool_seed {hexed(data.pool.seed)}",
        f"{name} start_input {start_record(world).hex()}",
    ]
    states = []
    leave = [event for event in world.queue if event.kind == wr.LEAVE_HOME_KIND and event.subject == (wr.SIM_SUBJECT, sim)]
    assert [(event.time, event.micro) for event in leave] == [(0, 0)] and not world.sims[sim][10]
    checks = rejecting_checks(world)
    assert checks == [], checks
    pending = [f"t {event.time} micro {event.micro} class {event.cls} seq {event.insertion} plan {event.plan}" for event in leave]
    states.append(f"{name} save_after_start clock {world.closed} sim {sim} moved_out {world.sims[sim][10]} household {world.sims[sim][9]} carer {world.sims[sim][11]} leave_home_pending {pending} rejecting_checks {checks} save {world.save().hex()}")
    before = len(world.log)
    world.advance(0)
    assert world.sims[sim][10]
    checks = rejecting_checks(world)
    assert checks == [], checks
    states.append(f"{name} save_after_second_0 clock {world.closed} sim {sim} moved_out {world.sims[sim][10]} household {world.sims[sim][9]} carer {world.sims[sim][11]} rejecting_checks {checks} save {world.save().hex()}")
    rows.append(states[0])
    rows += [f"{name} {row[len(name) + 1:]}" for row in fired_rows(name, world, 0)]
    for line in world.log[before:]:
        rows.append(f"{name} entry_second_0 {line.hex()}")
        if entry_json(line) is not None:
            rows.append(f"{name} entry_second_0_json {entry_json(line)}")
    rows.append(states[1])
    return rows


# ---------------------------------------------------------------------------------------
# One field of the pool load check at a time (review of the reference, NOTES2.md section 5b)

# A world started with founders and no pool (the template of map 4 without a pool): a couple
# and two old founders; at 16934400 the woman is pregnant and both old founders have died.
FOUNDERS_SEED = 0xFF
FOUNDERS = [(-25 * YEAR, FEMALE, []), (-27 * YEAR, wr.MALE, []), (-88 * YEAR, FEMALE, []), (-90 * YEAR, wr.MALE, [])]
FOUNDERS_COUPLES = [(0, 1)]
FOUNDERS_CLOCK = 16934400


def pin(save: bytes) -> str:
    return f"{len(save)} {hexed(hash64(save))}"


def save_hunks(base: bytes, changed: bytes) -> list:
    """The changed save as hunks of the base, `(offset in the base, removed bytes, put
    bytes)` in ascending offset (equal runs shorter than 8 bytes folded into a hunk);
    applying them to the base gives the changed save (asserted)."""
    import difflib

    merged = []
    for tag, i1, i2, j1, j2 in difflib.SequenceMatcher(None, base, changed, autojunk=False).get_opcodes():
        if tag == "equal":
            continue
        if merged and i1 - merged[-1][1] < 8:
            merged[-1][1], merged[-1][3] = i2, j2
        else:
            merged.append([i1, i2, j1, j2])
    hunks = [(i1, base[i1:i2], changed[j1:j2]) for i1, i2, j1, j2 in merged]
    out, at = b"", 0
    for offset, removed, put in hunks:
        out += base[at:offset] + put
        at = offset + len(removed)
    assert out + base[at:] == changed
    return hunks


def field_row(name: str, base_label: str, base: bytes, world: wr.World, how: str) -> str:
    """`pool_field_reject <name> base <label> base_pin <length> <xxh3> hunks <n> <offset>:<removed
    hex>:<put hex> ... changed_pin <length> <xxh3> checks [...] how <text>`."""
    world.kinships = {}
    checks = rejecting_checks(world)
    assert checks, f"no load check rejects {name}"
    changed = world.save()
    hunks = save_hunks(base, changed)
    return (
        f"pool_field_reject {name} base {base_label} base_pin {pin(base)} hunks {len(hunks)} "
        + " ".join(f"{offset}:{removed.hex()}:{put.hex()}" for offset, removed, put in hunks)
        + f" changed_pin {pin(changed)} checks {checks} how {how}"
    )


def set_archived(world: wr.World, sim: int, index: int, value) -> None:
    row = list(world.archive[sim])
    row[index] = value
    world.archive[sim] = tuple(row)


def flip_allele(world: wr.World, person: dict, locus_id: str) -> str:
    """Flips the maternal allele of an autosomal locus in `person` (the form of the meiosis
    reference); returns where."""
    locus = world.data.loci[locus_id]
    assert locus["chromosome"] == "autosome"
    person["autosomes_maternal"][locus["segment"]][locus["slot"]] ^= 1
    return f"{locus_id} (autosome segment {locus['segment']} slot {locus['slot']}, maternal)"


def replan_deaths(world: wr.World, sim: int) -> None:
    """Sets each planned death of a living sim to the second its cause gives now (after a
    change of its origin, detail or birth); the causes stay the ones planned."""
    for event in world.queue:
        if event.kind != wr.DEATH_KIND or event.subject != (wr.SIM_SUBJECT, sim):
            continue
        code = world.cause_code(wr.death_key(event.payload))
        if code in (wr.SENESCENCE, wr.ACCIDENT):
            due = world.cause_due(sim, code)
        else:
            (affliction,) = [a for a in world.sims[sim][4] if a.disease.code == code]
            due = world.disease_due(sim, affliction)
        assert due is not None and due > world.closed, (sim, code, due)
        event.time = due


def next_birthday(born: int, first: int, clock: int) -> int:
    """The pending birthday at the clock of a sim born at `born` whose first one in the town is
    `first`: `first` if it lies after the clock, else the next anniversary after it."""
    if first > clock:
        return first
    return born + ((clock - born) // YEAR + 1) * YEAR


def shift_household(world: wr.World, index: int, d: int) -> list:
    """Moves the arrival of the pool household `index` by `d` (a whole number of years) with
    everything the arrival second decides (note sections 5 and 6): per member `arrived`,
    `arrival_year`, `born` (the new `Δ`), the exposure from `A`, the carried events (leaving
    home, birth, separation, series) and their plans, the first birthday, the deaths (origin
    and detail); the household formed at `A`; the brought partnership formed at `formed_pool
    + Δ`; a carried pregnancy's `conceived`. Returns the members."""
    assert d % YEAR == 0
    (record,) = [r for r in world.arrivals if r["index"] == index]
    members, old = record["sims"], record["t"]
    new = old + d
    assert new >= 0
    by = {world.arrived_by[sim] for sim in members}
    for sim in members:
        origin = world.origins[sim]
        assert origin["arrived"] == old
        origin["arrived"], origin["arrival_year"] = new, new // YEAR
        afflictions = world.sims[sim][4] if sim in world.sims else world.archive[sim][6]
        for affliction in afflictions:
            if affliction.exposure is not None and affliction.exposure[1] == old:
                affliction.exposure[1] = new
        if sim in world.sims:
            world.sims[sim][0] += d
        else:
            assert world.archive[sim][1] >= new, "a member died before the new arrival second"
            set_archived(world, sim, 0, world.archive[sim][0] + d)
        if sim in world.pregnancies:
            father, number, children, conceived = world.pregnancies[sim]
            world.pregnancies[sim] = (father, number, children, conceived + d)
    world.households[record["household"]][0] = new
    if record.get("partnership") is not None:
        world.partnerships[record["partnership"]][2] += d
    for event in world.queue:
        if event.plan[0] in by and event.plan[1] == old:
            event.plan = (event.plan[0], new)
            if event.kind in (wr.LEAVE_HOME_KIND, wr.BIRTH_KIND, wr.SEPARATION_KIND):
                event.time += d
    for sim in members:
        if sim not in world.sims:
            continue
        born = world.sims[sim][0]
        (birthday,) = [e for e in world.queue if e.kind == wr.BIRTHDAY_KIND and e.subject == (wr.SIM_SUBJECT, sim)]
        birthday.time = next_birthday(born, wr.first_birthday(born, new), world.closed)
        replan_deaths(world, sim)
        series = [e for e in world.queue if e.kind == wr.ATTEMPT_KIND and e.subject == (wr.SIM_SUBJECT, sim)]
        if series:
            (event,) = series
            event.time = world.series_due(sim)
    return members


def household_moved_rows(orphans: wr.World, base: bytes, copy_world) -> list:
    """A2: the first household of `Start` in "orphans" (in arrival order, at least 2 members)
    whose members all lived past second YEAR, moved as a whole to YEAR (`shift_household`):
    its draw is that of `Start` (second 0), so only "each household arrives in the second of
    its draw" breaks. Households whose move other checks reject as well are listed."""
    tried = []
    for index in orphans.used:
        (record,) = [r for r in orphans.arrivals if r["index"] == index]
        if len(record["sims"]) < 2 or any(sim in orphans.archive and orphans.archive[sim][1] < YEAR for sim in record["sims"]):
            continue
        w = copy_world(orphans)
        members = shift_household(w, index, YEAR)
        w.kinships = {}
        checks = rejecting_checks(w)
        tried.append((index, members, checks))
        if checks == ["check_pool"]:
            others = "; ".join(f"household {i} {m} {c}" for i, m, c in tried[:-1])
            return [field_row(
                "arrival_household_moved_to_another_year", "orphans_save_end", base, w,
                f"household {index} of pool_d (sims {members}, drawn by Start, second 0) moved whole to second {YEAR} (arrival_year 1): born, carried events,"
                f" exposure, first birthday, deaths, household and partnership formed with the new Δ (tried before: {others or 'none'})",
            )]
    raise AssertionError(f"no household moves with only check_pool rejecting: {tried}")


def close_kin_rows(orphans: wr.World, base: bytes, copy_world) -> list:
    """k2: an open partnership of two persons with `φ >= 1/8`, at least one an arrival: the first
    such single pair of "orphans" at its end, paired by an ArrangePartnership acting in the
    second after the clock as if it had been accepted (the input in the log, the partnership,
    her series, its separation, their household as the rules make them)."""
    w = copy_world(orphans)
    limit = Fraction(w.data.kinship_limit)
    adult = lambda sim: w.closed + 1 - w.sims[sim][0] >= w.data.market_age * YEAR
    pairs = [
        (woman, man)
        for woman in sorted(w.sims)
        for man in sorted(w.sims)
        if w.sex_of(woman) == FEMALE and w.sex_of(man) == wr.MALE and (w.is_arrival(woman) or w.is_arrival(man))
        and w.open_partnership(woman) is None and w.open_partnership(man) is None and adult(woman) and adult(man)
        and w.kinship(woman, man) >= limit
    ]
    if not pairs:
        return ["pool_field_skipped arrival_partnership_close_kin reason no single pair with phi >= 1/8 and an arrival in orphans_save_end"]
    woman, man = pairs[0]
    phi = w.kinship(woman, man)
    w.kinship = lambda x, y: Fraction(0)
    w.arrange(woman, man)
    del w.kinship
    number = w.open_partnership(woman)
    return [field_row(
        "arrival_partnership_close_kin", "orphans_save_end", base, w,
        f"partnership {number} of sim {woman} ({env_kind(w, woman)}) and sim {man} ({env_kind(w, man)}) with phi {phi}, by an ArrangePartnership stamped at the clock"
        f" (accepted as if the kinship were 0), with her series, its separation and their household",
    )]


def pool_field_rows() -> list:
    """States changed in exactly one field of the pool load check (note section 9; review,
    NOTES2.md section 5b), each as hunks of a pinned save with the model checks that reject it.
    Bases: `orphans_save_end` (household 20 of pool_d: sims 21 to 24, 23 archived, 24 living),
    `arrival_save_after_pool_child_without_father` (the pool child 59 born at the clock, its
    mother 36 alone in pool household 28, its father a record), and a new `founders_world`
    (no pool)."""
    copy_world = lambda world: copy.deepcopy(world, {id(world.data): world.data})
    rows = []

    orphans = kin_world(ORPHANS_SEED)()
    orphans.advance(ORPHANS_END)
    check_all(orphans)
    o_base = orphans.save()
    assert BASE_SAVES.get("orphans_save_end", o_base) == o_base
    child_world = arrival_world()
    child_world.advance(ARRIVAL_MARKS["pool_child_without_father"])
    check_all(child_world)
    c_base = child_world.save()
    assert BASE_SAVES.get("arrival_save_after_pool_child_without_father", c_base) == c_base
    plain = wr.Data(*template_documents())
    founders = wr.World(FOUNDERS_SEED, TAG, True, FOUNDERS, FOUNDERS_COUPLES, plain)
    f_start = start_record(founders)
    founders.advance(FOUNDERS_CLOCK)
    check_all(founders)
    f_base = founders.save()
    assert sorted(founders.pregnancies) == [1] and sorted(founders.archive) == [3, 4]
    rows.append(f"pool_field_base orphans_save_end clock {orphans.closed} pin {pin(o_base)} (the row orphans_save_end)")
    rows.append(f"pool_field_base arrival_save_after_pool_child_without_father clock {child_world.closed} pin {pin(c_base)} (the row arrival_save_after_pool_child_without_father)")
    rows.append(
        f"pool_field_base founders_world clock {founders.closed} pin {pin(f_base)} data template of map 4 without a pool data_hash {hexed(plain.hash)}"
        f" seed {hexed(FOUNDERS_SEED)} tag {TAG} switch on founders {[(born, 'female' if sex == FEMALE else 'male', genes) for born, sex, genes in FOUNDERS]}"
        f" couples {FOUNDERS_COUPLES} pool None start_input {f_start.hex()} save {f_base.hex()}"
    )

    # A: an arrival. Pool household 20 of pool_d at `Start`: sims 21, 22, 23, 24.
    pool = orphans.data.pool
    (record,) = [r for r in orphans.arrivals if r["sims"][:1] == [21]]
    assert record["sims"] == [21, 22, 23, 24] and pool.sizes[record["index"]] == 4
    dead, alive = 23, 24
    assert dead in orphans.archive and alive in orphans.sims and orphans.is_arrival(dead) and orphans.is_arrival(alive)
    assert not [n for n, v in orphans.partnerships.items() if dead in v[:2] or alive in v[:2]]
    records = sorted(pool.records)
    o, a = "orphans_save_end", f"household {record['index']} of pool_d"

    w = copy_world(orphans)
    w.origins[dead]["arrived"] += 1
    rows.append(field_row("arrival_arrived_one_second_late", o, o_base, w, f"{a}: sim {dead} (archived, third member) arrived 1 instead of 0, the rest at 0"))

    w = copy_world(orphans)
    w.origins[dead]["arrival_year"] += 1
    rows.append(field_row("arrival_year_off_by_one", o, o_base, w, f"{a}: sim {dead} (archived) arrival_year 1 instead of 0"))

    w = copy_world(orphans)
    w.origins[alive]["born_pool"] += 1
    rows.append(field_row("arrival_born_pool_changed", o, o_base, w, f"{a}: sim {alive} (living) born_pool + 1 (born unchanged)"))

    w = copy_world(orphans)
    w.origins[alive]["genome_key"] ^= 1
    rows.append(field_row("arrival_genome_key_changed", o, o_base, w, f"{a}: sim {alive} (living) genome_key xor 1"))

    mother, father = orphans.origins[alive]["mother"], orphans.origins[alive]["father"]
    other_mother = next(key for key in records if key not in (mother, father))
    other_father = next(key for key in records if key not in (mother, father, other_mother))
    w = copy_world(orphans)
    w.origins[alive]["mother"] = other_mother
    rows.append(field_row("arrival_pool_mother_changed", o, o_base, w, f"{a}: sim {alive} (living) PoolOrigin mother {hexed(mother)} -> {hexed(other_mother)} (a record of the pool)"))

    w = copy_world(orphans)
    w.origins[alive]["father"] = other_father
    rows.append(field_row("arrival_pool_father_changed", o, o_base, w, f"{a}: sim {alive} (living) PoolOrigin father {hexed(father)} -> {hexed(other_father)} (a record of the pool)"))

    w = copy_world(orphans)
    w.origins[alive]["social_parents"] = list(w.origins[alive]["social_parents"]) + [other_mother]
    rows.append(field_row("arrival_pool_social_parents_changed", o, o_base, w, f"{a}: sim {alive} (living) PoolOrigin social_parents {[hexed(k) for k in orphans.origins[alive]['social_parents']]} + {hexed(other_mother)}"))

    w = copy_world(orphans)
    set_archived(w, dead, 4, FEMALE if w.archive[dead][4] == wr.MALE else wr.MALE)
    rows.append(field_row("arrival_sex_flipped", o, o_base, w, f"{a}: sim {dead} (archived, no partnership) sex flipped to {'female' if w.archive[dead][4] == FEMALE else 'male'}"))

    w = copy_world(orphans)
    where = flip_allele(w, w.sims[alive][3], "core:herc2")
    rows.append(field_row("arrival_genome_changed", o, o_base, w, f"{a}: sim {alive} (living) one allele flipped: {where}"))

    w = copy_world(orphans)
    parents = (orphans.by_key[mother], orphans.by_key[father], 0)
    w.sims[alive][7] = parents
    rows.append(field_row("arrival_town_parents_set", o, o_base, w, f"{a}: sim {alive} (living) parents Some{parents} (her pool parents' SimIds, conception 0)"))

    # A2: a whole household moved to another year start than its draw.
    rows += [row for row in household_moved_rows(orphans, o_base, copy_world)]

    # B: a pool child: sim 59, born at the clock (14515200) of the pregnancy of sim 36.
    child, mother_sim = 59, 36
    origin = child_world.origins[child]
    assert origin["born_pool"] is None and child_world.born_of(child) == child_world.closed and child_world.by_key[origin["mother"]] == mother_sim
    assert child_world.pool_member(mother_sim) is not None and child_world.data.pool.sizes[child_world.data.pool.members[child_world.key_of(mother_sim)][0]] == 1
    c, b = "arrival_save_after_pool_child_without_father", f"pool child {child} (born {child_world.closed}, mother {mother_sim})"
    p_records = sorted(child_world.data.pool.records)
    p_other = [key for key in p_records if key not in (origin["mother"], origin["father"])][:2]

    w = copy_world(child_world)
    w.origins[child]["genome_key"] ^= 1
    rows.append(field_row("pool_child_genome_key_changed", c, c_base, w, f"{b}: genome_key xor 1"))

    w = copy_world(child_world)
    w.origins[child]["arrived"] -= 1
    rows.append(field_row("pool_child_arrived_not_born", c, c_base, w, f"{b}: arrived born - 1"))

    w = copy_world(child_world)
    w.origins[child]["arrival_year"] += 1
    rows.append(field_row("pool_child_arrival_year_off_by_one", c, c_base, w, f"{b}: arrival_year 1 instead of 0"))

    w = copy_world(child_world)
    w.origins[child]["mother"] = p_other[0]
    rows.append(field_row("pool_child_pool_mother_changed", c, c_base, w, f"{b}: PoolOrigin mother {hexed(origin['mother'])} -> {hexed(p_other[0])} (a record of the pool)"))

    w = copy_world(child_world)
    w.origins[child]["father"] = p_other[1]
    rows.append(field_row("pool_child_pool_father_changed", c, c_base, w, f"{b}: PoolOrigin father {hexed(origin['father'])} -> {hexed(p_other[1])} (a record of the pool)"))

    w = copy_world(child_world)
    w.origins[child]["social_parents"] = [origin["mother"]]
    rows.append(field_row("pool_child_social_parents_set", c, c_base, w, f"{b}: PoolOrigin social_parents [{hexed(origin['mother'])}] instead of []"))

    w = copy_world(child_world)
    w.sims[child][2] = FEMALE if w.sims[child][2] == wr.MALE else wr.MALE
    rows.append(field_row("pool_child_sex_flipped", c, c_base, w, f"{b}: sex flipped to {'female' if w.sims[child][2] == FEMALE else 'male'} (living: its deaths follow the sex)"))

    w = copy_world(child_world)
    where = flip_allele(w, w.sims[child][3], "core:herc2")
    rows.append(field_row("pool_child_genome_changed", c, c_base, w, f"{b}: one allele flipped: {where}"))

    w = copy_world(child_world)
    man = min(sim for sim in w.sims if w.sex_of(sim) == wr.MALE and w.sims[sim][0] + 16 * YEAR <= child_world.closed)
    w.sims[child][7] = (mother_sim, man, 0)
    rows.append(field_row("pool_child_town_parents_set", c, c_base, w, f"{b}: parents Some({mother_sim}, {man}, 0)"))

    w = copy_world(child_world)
    later = w.closed + 1
    w.sims[child][0] = later
    w.origins[child]["arrived"] = later
    w.origins[child]["arrival_year"] = later // YEAR
    for affliction in w.sims[child][4]:
        if affliction.exposure is not None and affliction.exposure[1] == child_world.closed:
            affliction.exposure[1] = later
    for event in w.queue:
        if event.subject == (wr.SIM_SUBJECT, child) and event.kind == wr.BIRTHDAY_KIND:
            event.time = later + YEAR
        elif event.subject == (wr.SIM_SUBJECT, child) and event.kind == wr.LEAVE_HOME_KIND:
            event.time = w.leave_home_second(child)
    replan_deaths(w, child)
    rows.append(field_row(
        "pool_child_born_after_the_clock", c, c_base, w,
        f"{b}: born (and arrived) {later} = the clock + 1, its first birthday, leaving home and deaths from there (the base itself, born = the clock, is accepted)",
    ))

    w = copy_world(child_world)
    w.origins.pop(mother_sim)
    w.used = [index for index in w.used if index != w.data.pool.members[w.key_of(mother_sim)][0]]
    count = w.sims[mother_sim][8]
    w.sims[mother_sim][8] = sum(1 for value in w.partnerships.values() if value[0] == mother_sim)
    rows.append(field_row(
        "pool_child_mother_not_an_arrival", c, c_base, w,
        f"{b}: sim {mother_sim} a town person instead of an arrival (no PoolOrigin, no parents, her partnerships counter {count} -> {w.sims[mother_sim][8]},"
        f" a town person's) and her household {child_world.data.pool.members[child_world.key_of(mother_sim)][0]} out of the used households",
    ))

    w = copy_world(child_world)
    left = child_world.closed - 1
    value = w.sims.pop(mother_sim)
    born_, key_, sex_, person_, afflictions_, conceptions_, births_, parents_, partnerships_, household_, moved_out_, _, social_ = value
    w.archive[mother_sim] = (born_, left, wr.str_key(wr.ACCIDENT), key_, sex_, person_, afflictions_, conceptions_, births_, parents_, partnerships_, household_, moved_out_, tuple(social_))
    w.queue = [event for event in w.queue if event.subject != (wr.SIM_SUBJECT, mother_sim)]
    w.sims[child][11] = None
    rows.append(field_row(
        "pool_child_mother_left_before_the_birth", c, c_base, w,
        f"{b}: sim {mother_sim} archived (death by core:accident) at {left}, one second before the birth; her events gone, the child without a carer in her household",
    ))

    # C: a world started without a pool.
    fw = "founders_world"
    w = copy_world(founders)
    w.origins[2] = {"genome_key": 0, "born_pool": w.sims[2][0], "arrived": 0, "arrival_year": 0, "mother": None, "father": None, "social_parents": []}
    rows.append(field_row("no_pool_living_with_pool_origin", fw, f_base, w, "sim 2 (living founder) with a PoolOrigin (genome_key 0, born_pool its born, arrived 0)"))

    w = copy_world(founders)
    w.origins[3] = {"genome_key": 0, "born_pool": w.archive[3][0], "arrived": 0, "arrival_year": 0, "mother": None, "father": None, "social_parents": []}
    rows.append(field_row("no_pool_archived_with_pool_origin", fw, f_base, w, "sim 3 (archived founder) with a PoolOrigin (genome_key 0, born_pool its born, arrived 0)"))

    w = copy_world(founders)
    w.used = [0]
    rows.append(field_row("no_pool_used_household", fw, f_base, w, "used households [0] instead of []"))

    w = copy_world(founders)
    w.pregnancies[1] = (None,) + w.pregnancies[1][1:]
    rows.append(field_row("no_pool_pregnancy_without_father", fw, f_base, w, "the pregnancy of sim 1: father None instead of 2"))

    w = copy_world(founders)
    w.pool_fathers[1] = w.key_of(2)
    rows.append(field_row("no_pool_pregnancy_with_pool_father", fw, f_base, w, f"the pregnancy of sim 1: pool_father Some({hexed(w.key_of(2))}) (its father's key) instead of None"))

    # D: the kinship of a pool person.
    w = copy_world(orphans)
    parent = orphans.by_key[father] if father in orphans.by_key else orphans.by_key[mother]
    w.sims[alive][12] = [parent]
    rows.append(field_row("arrival_social_parent_is_its_pool_parent", o, o_base, w, f"{a}: sim {alive} social_parents [{parent}], its biological parent by its PoolOrigin key only (no town parents)"))

    rows += close_kin_rows(orphans, o_base, copy_world)
    return rows

# "arrival" with another seed: the woman of pool household 28 (her pool pregnancy's father is a
# record only, so she is single) gives birth at second 14515200, a month start, and the round
# of that second (n = 6; births, class 200, run before it, class 500) pairs her. Found by a
# scan (not a pin): of the seeds 0 to 4095 with household 28 at second 0 (`Start` with `n_start`
# 50 or the immigration of year 0), 14 have her MARKET_SEARCH draw of round 6 below her chance
# (0.0064 at 33 years), and in each of the 14 runs she is paired then; 0xef is the smallest.
TIE_SEED = 0xEF
TIE_SECOND = 14515200
TIE_WOMAN_KEY = 0x6A9C7AFCA6158ECD


def birth_pairing_tie_rows() -> list:
    """The tie of a birth and a pairing in one second: her series starts from the pairing (the
    first month start after it), not from the pause after the birth (`World.series_due`; the
    round runs after the births of its second). The saves at the clock of that second and just
    before her first attempt pass every load check of the model; the same state with her series
    moved to the second the pause after the birth would give is rejected."""
    name = "birth_pairing_tie"
    data = town_data(golden_pool(ARRIVAL_POOL))
    world = make_world(data, TIE_SEED, ARRIVAL_N_START, True)
    check_all(world)
    world.advance(TIE_SECOND)
    woman = world.by_key[TIE_WOMAN_KEY]
    number = world.open_partnership(woman)
    man = world.partnerships[number][1]
    births = world.town_births(woman)
    (choice,) = [c for r in world.rounds if r["t"] == TIE_SECOND for c in r["choices"] if c["woman"] == woman]
    assert births == [TIE_SECOND] and choice["outcome"] == "paired" and choice["partnership"] == number
    assert world.partnerships[number][2] == TIE_SECOND and (TIE_SECOND, woman, man) not in world.input_pairings
    assert not [n for n, value in world.partnerships.items() if value[0] == woman and n != number]
    (child,) = [sim for sim in set(world.sims) | set(world.archive) if world.born_of(sim) == TIE_SECOND and world.is_pool_person(sim) and world.origins[sim]["mother"] == TIE_WOMAN_KEY]
    first = world.series_due(woman)
    assert first == wr.month_start_after(TIE_SECOND) == world.pending_series(woman)
    counted = wr.month_start_at_or_after(TIE_SECOND + max(world.data.pause_months * MONTH, 1))
    assert counted != first
    fired = [key for key, _ in world.fired if key[0] == TIE_SECOND]
    birth_at = [i for i, key in enumerate(fired) if key[6] == wr.BIRTH_KIND and key[5] == woman]
    round_at = [i for i, key in enumerate(fired) if key[6] == wr.MARKET_ROUND_KIND]
    assert birth_at and round_at and birth_at[0] < round_at[0]
    pool = data.pool
    rows = [
        f"{name} config pool {ARRIVAL_POOL} (tests/golden/pool/{ARRIVAL_POOL}.bin) pool_bytes {len(golden_pool(ARRIVAL_POOL))} pool_xxh3 {hexed(hash64(golden_pool(ARRIVAL_POOL)))}"
        f" pool_hash {hexed(pool.hash)} pool_seed {hexed(pool.seed)} frozen_at {pool.frozen_at} data template of map 4 (tests/golden/map4_template, the repository's life course data)"
        f" data_hash {hexed(data.hash)} seed {hexed(TIE_SEED)} tag {TAG} n_start {ARRIVAL_N_START} immigration True q {wr.immigration_persons(ARRIVAL_N_START)} switch on nutrition [] founders [] couples []",
        f"{name} scenario_json {scenario_json(TIE_SEED, ARRIVAL_N_START, True, first - 1)}",
        f"{name} start_input {start_record(world).hex()}",
        f"{name} case woman {woman} ({env_kind(world, woman)}, key {hexed(TIE_WOMAN_KEY)}, pool household {pool.members[TIE_WOMAN_KEY][0]}, her pool pregnancy's father"
        f" {hexed(world.origins[child]['father'])} a record only) gives birth at t {TIE_SECOND} (child sim {child}); the market round n {TIE_SECOND // MONTH} of t pairs her"
        f" with man {man} ({env_kind(world, man)}) partnership {number} formed {TIE_SECOND}; births {world.sims[woman][6]} wish {world.wish_of(woman)}",
        f"{name} series_due {first} (the first month start after the pairing); counting the birth would give {counted} (the first month start at or after t + pause {world.data.pause_months} months)",
    ]
    rows += [f"{name} {row[len(name) + 1:]}" for row in fired_rows(name, world, TIE_SECOND)]
    checks = rejecting_checks(world)
    assert checks == [], checks
    rows.append(f"{name} save_at_t clock {world.closed} pending_series {world.pending_series(woman)} rejecting_checks {checks} save {world.save().hex()}")
    broken = copy.deepcopy(world, {id(world.data): world.data})
    (event,) = [e for e in broken.queue if e.kind == wr.ATTEMPT_KIND and e.subject == (wr.SIM_SUBJECT, woman)]
    event.time = counted
    wrong = rejecting_checks(broken)
    assert wrong == ["check_pool"], wrong
    rows.append(f"{name} counted_birth clock {broken.closed} her series moved to {counted} rejecting_checks {wrong} save {broken.save().hex()}")
    world.advance(first - 1)
    assert world.pending_series(woman) == first == world.series_due(woman) and world.open_partnership(woman) == number
    checks = rejecting_checks(world)
    assert checks == [], checks
    rows.append(f"{name} save_before_first_attempt clock {world.closed} pending_series {world.pending_series(woman)} rejecting_checks {checks} save {world.save().hex()}")
    world.advance(first)
    assert [row[:3] for row in world.attempts if row[1] == woman][:1] == [(first, woman, man)]
    return rows


# ---------------------------------------------------------------------------------------
# Codes, data hashes and the main


def code_rows() -> list:
    rows = [
        f"arrival_codes tag POOL_RANK {wr.POOL_RANK} class immigration {wr.IMMIGRATION_CLASS} kind immigration {wr.IMMIGRATION_KIND}"
        f" queue_payload Immigration variant {wr.IMMIGRATION_VARIANT} (persons q: {variant(wr.IMMIGRATION_VARIANT, varint(1)).hex()} for q 1)"
        f" log_payload core:arrived {wr.ARRIVED_VARIANT}",
        f"arrival_versions core {wr.CORE_VERSION} projection {wr.PROJECTION_VERSION} save_format {wr.FORMAT_VERSION} pedigree {wr.PEDIGREE_FORMAT_VERSION} scenario 5",
        "arrival_lines_for_the_code_lists (protected goldens, not added here):",
        f"line_for_rng_tags tag {wr.POOL_RANK} POOL_RANK",
        f"line_for_event_codes class immigration {wr.IMMIGRATION_CLASS}",
        f"line_for_event_codes kind immigration {wr.IMMIGRATION_KIND}",
        f"line_for_log_codes payload {wr.ARRIVED_VARIANT} core:arrived",
    ]
    for code in wr.NEW_CODE_LINES_OF_THE_ARRIVAL:
        rows.append(f"line_for_log_codes reject {code} {wr.REJECT_NAMES[code]}")
    rows.append("arrival_invariants pool.empty pool.carried_before_arrival; data checks pool.carer_not_member pool.sex pool.locus")
    template = template_documents()
    plain = wr.Data(*template)
    rows.append(f"data_hash template_without_pool {hexed(plain.hash)}")
    for name in ("pool_a", "pool_b", "pool_c", "pool_d", "pool_e"):
        pool = golden_pool(name)
        data = town_data(pool)
        rows.append(
            f"data_hash template_with_{name} {hexed(data.hash)} pool_seed {hexed(data.pool.seed)} pool_hash {hexed(data.pool.hash)}"
            f" appended {(varint(data.pool.seed) + varint(data.pool.hash)).hex()} households {len(data.pool.households)} members {sum(data.pool.sizes)}"
        )
    return rows


def placement_through(world: wr.World, record: dict) -> str:
    """For a placement with a relative: `only_records` if its kinship with the
    representative is 0 without the records of the pool, `records_add` if less, else
    `town`; `-` for the other stages."""
    if record["stage"] != "relative":
        return "-"
    rep, carer = world.key_of(record["representative"]), world.key_of(record["carer"])
    full = wr.kinship_by_keys(rep, carer, world.kin_of)[0]
    town = wr.kinship_by_keys(rep, carer, town_only(world))[0]
    return "only_records" if town == 0 else ("records_add" if town < full else "town")


def scan(pool_name: str, n_start: int, years: int, seeds: list) -> None:
    """The cases per seed (not a pin): who died (arrivals with their arrival year), the
    successions and placements, the town-born children and the separations."""
    data = town_data(golden_pool(pool_name))
    for seed in seeds:
        world = make_world(data, seed, n_start, True)
        stopped = None
        try:
            world.advance(years * YEAR - 1)
        except wr.Invariant as error:
            stopped = (error.code, world.immigrations[-1]["t"])
        deaths = [(sim, value[1], world.death_detail(sim)) for sim, value in world.archive.items() if value[2] is not None]
        print(
            f"scan {pool_name} n_start {n_start} seed {seed:x} stopped {stopped} deaths {deaths}"
            f" successions {[(row[0], row[1]) for row in world.carer_log if row[4].startswith('succession')]}"
            f" placements {[(p['t'], p['stage'], p['group'], p['carer'], placement_through(world, p)) for p in world.placements]}",
            flush=True,
        )


def main(argv) -> int:
    if "--scan" in argv:
        at = argv.index("--scan")
        scan(argv[at + 1], int(argv[at + 2]), int(argv[at + 3]), [int(value, 16) for value in argv[at + 4 :]])
        return 0
    out = Path(argv[argv.index("--out") + 1]) if "--out" in argv else None
    sections = [
        ("codes", code_rows),
        ("start_rejections", start_rejection_rows),
        ("data_rejections", data_rejection_rows),
        ("invariants", invariant_rows),
        ("arrival", arrival_rows_all),
        ("orphans", orphans_rows_all),
        ("records", records_rows_all),
        ("micro", micro_rows_all),
        ("empty_at_start", empty_rows_all),
        ("load_rejections", broken_rows),
        ("load_rejections_more", more_broken_rows),
        ("leave_home_at_start", leave_home_at_start_rows),
        ("birth_pairing_tie", birth_pairing_tie_rows),
        ("pool_fields", pool_field_rows),
    ]
    for _, rows_of in sections:
        for row in rows_of():
            print(row, flush=True)
    if out is not None:
        out.mkdir(parents=True, exist_ok=True)
        data, _ = micro_pool()
        (out / "micro_pool.bin").write_bytes(data)
        data, _ = carried_before_pool()
        (out / "carried_before_pool.bin").write_bytes(data)
        data, _, _ = leave_home_at_start_pool()
        (out / "leave_home_at_start_pool.bin").write_bytes(data)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

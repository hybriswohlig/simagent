#!/usr/bin/env python3
"""The village start mode of the pool (step 11, criterion P4 village; ADR-0020 section 8
"P4 Dorf", "Das Dorf, genau (G4)"; design note of the village, sections 5 (Grok V1 (a), V2
(a)) and 6), the known answers, written from the note and the ADR, independent of the Rust
code. The model is tools/world_reference.py (`village_candidates`, `village_draw`,
`village_person`, `village_households`, `pool_start(..., village=True)`, `PoolStart.village`
in `pool_start_bytes`, the reject codes 28 to 30 in `start_rejection`, the arrival with
`households`, the village branch of `check_pool`), on the repository's data as the CLI builds
them (data/ with map 4 and the pool, `world_reference.REPOSITORY`).

The village `Start`: world seed the seed, founder tag `core:criteria_village`, the switch of
the hereditary diseases on, no nutrition year, no founders, no couples, capacity 80, `pool`
`{n_start: 80, immigration: false, village: true}` (the CLI fills `pool_hash` and the
households). Per sex the 40 candidates (members of the pool, not unborn children, aged from
18 to before 41 at the freeze) with the smallest `(rank, lineage key)`, `rank = rank(world
seed, POOL_RANK = 27, SubjectKey::WORLD, occasion 1, lineage key)`; the households of
`PoolStart` one per person, the women in rank order, then the men (decision of the
coordinator, 2026-10-19; so are an empty `pool_used`, the carer struck, `partnerships` kept).
`village` is not a field of the CLI's scenario file: only xtask builds a village `Start`.

Prints `label value` rows:
- `village_codes`, `line_for_log_codes`: the constants and the new lines of the code lists;
- per seed of SEEDS (`--select`, also by default): `village_<seed>_woman i` and
  `village_<seed>_man i` (lineage key, rank, the age at the freeze in seconds and years, the
  file household, what was struck), `village_<seed>_start_input` (bytes, xxh3, hex) and the
  `Start` it encodes;
- `village_reject` (`--rejections`, also by default): each rejected village `Start`, its
  code and input;
- `--run YEARS` (default 30, the first seed): the village run to the last second of year
  YEARS, the year hashes, the log hash, the living, births, deaths and emigrations per year,
  the load checks at every year end, the save (bytes, xxh3; with `--out DIR` written to
  DIR/village_<seed>_<YEARS>y.save with the start input), and the year hashes of the
  variant `pool_used` = the file households of the chosen persons (not the reference's
  choice; NOTES.md of the village);
- `--export E1,E2,...` (the runner of `criteria village`, design note sections 7 and 7.2):
  the village of `--seed` with capacity 160 (Grok D1; `--capacity N` for another) run to the
  last second of each end, its export in the criteria kit's format
  (`criteria_reference.export_world` on the documents under data/, the switch on; with `--out
  DIR` written to DIR/village_<seed>_c<capacity>_<E>y.ndjson), the export masked as the Rust
  kit compares it (`masked`), read back with `read_export`, and the village block of the
  report (`village_obj`: `g*`, `n`, the units' hash, `V`, `E`, the blocks, the living);
- `--aggregate` (with `--out DIR`: DIR/aggregate.json): `village_aggregate` (K2, the twin of
  Grok D3 shifted to mean 1) on four synthetic sets of 20 villages (`AGGREGATE_SETS`), the
  decisions at 1200 and 2100, the twin beside the former rescaled one, and the selection of
  the counted villages of D1 on 30 villages (`selection_example`);
- `--moved-out`: the forged saves of the load check of the moved out
  (`world_reference.check_moved_out_households`, ADR-0006 section 5: the living members of a
  household with `moved_out` are at most one, or the woman and the man of one open
  partnership; the village start puts each person alone, G4, which the loader did not check),
  in the form of the load rejections of tools/arrival_reference.py (`moved_out_reject ...`
  rows: hunks of a genuine save, both pins, the checks, the code): (a) the village of the
  first seed at the end of year 9, (b) the save `load_base` of tools/arrival_reference.py
  ("arrival" at the end of year 2). In each a sim that is moved out, alone in its household
  and in no open partnership moves into the household of another such sim; its emptied
  household ends at the clock, so that only the new check rejects the save (asserted: every
  other load check passes, `check_households` without the new part too). With `--out DIR`
  the genuine and the forged saves go to DIR.

Usage: .venv/bin/python tools/village_reference.py [--select] [--rejections] [--run [YEARS]]
       [--moved-out] [--seed HEX] [--out DIR]
       (no option: --select and --rejections)
       ... village_reference.py --export 48,60 [--seed HEX] [--capacity N] [--out DIR]
       ... village_reference.py --aggregate [--out DIR]
"""

import copy
import json
import math
import re
import sys
import time
from pathlib import Path

import arrival_reference as ar
import pool_reference
import world_reference as wr
from postcard_reference import FEMALE
from world_reference import YEAR, hash64

TAG = "core:criteria_village"
SEEDS = (0x5EED_0004_0000_0002, 0x5EED_0004_0000_0003)
N_START = 80
CAPACITY = 80
RUN_YEARS = 30


def hexed(value: int) -> str:
    return f"{value:016x}"


def village_pool(data: wr.Data, seed: int, n_start: int = N_START) -> dict:
    return wr.pool_start(data, seed, n_start, False, village=True)


def village_world(data: wr.Data, seed: int, pool=None, capacity: int = CAPACITY) -> wr.World:
    """The world of the village `Start` (module doc), after `Start`; `capacity` 160 for the
    runner of `criteria village` (Grok D1)."""
    pool = village_pool(data, seed) if pool is None else pool
    return wr.World(seed, TAG, True, [], (), data, capacity=capacity, pool=pool, nutrition=())


def start_text(seed: int) -> str:
    """The village `Start` in words (xtask builds it; not a scenario file of the CLI)."""
    return (
        f"world_seed {hexed(seed)} founder_tag {TAG} hereditary_diseases true nutrition [] founders [] couples []"
        f" capacity {CAPACITY} pool {{pool_hash: the data's, n_start: {N_START}, immigration: false, village: true, households: the {N_START} single households}}"
    )


def check_all(world: wr.World) -> None:
    ar.check_all(world)


# ---------------------------------------------------------------------------------------
# Codes


def code_rows() -> list:
    rows = [
        f"village_codes tag POOL_RANK {wr.POOL_RANK} occasion {wr.VILLAGE_OCCASION} ages {wr.VILLAGE_AGES[0]} to before {wr.VILLAGE_AGES[1]}"
        f" core {wr.CORE_VERSION} projection {wr.PROJECTION_VERSION} save_format {wr.FORMAT_VERSION} scenario 5",
        "village_lines_for_the_code_lists (protected goldens, not added here):",
    ]
    for code in wr.NEW_CODE_LINES_OF_THE_VILLAGE:
        rows.append(f"line_for_log_codes reject {code} {wr.REJECT_NAMES[code]}")
    return rows


# ---------------------------------------------------------------------------------------
# The persons of the village per seed


def struck(member: dict) -> str:
    names = [name for name in ("conception_at", "birth_at", "separation_at", "leave_home_at") if member[name] is not None]
    if not member["moved_out"]:
        names.append("moved_out")
    return ",".join(names) if names else "-"


def selection_rows(data: wr.Data, seed: int) -> list:
    pool = data.pool
    women, men = wr.village_draw(pool, seed, N_START)
    label = f"village_{hexed(seed)}"
    rows = [
        f"{label}_config data_hash {hexed(data.hash)} pool_hash {hexed(pool.hash)} pool_seed {hexed(pool.seed)} frozen_at {pool.frozen_at}"
        f" candidates women {len(wr.village_candidates(pool, 0))} men {len(wr.village_candidates(pool, 1))}"
    ]
    for kind, keys in (("woman", women), ("man", men)):
        for i, key in enumerate(keys):
            index, member = pool.members[key]
            age = pool.frozen_at - member["born"]
            r = wr.rank(seed, wr.POOL_RANK, wr.WORLD_KEY, wr.VILLAGE_OCCASION, key)
            rows.append(
                f"{label}_{kind} {i} key {hexed(key)} rank {hexed(r)} age_seconds {age} age_years {age / YEAR:.4f} household {index}"
                f" household_size {len(pool.households[index]['members'])} conceptions {member['conceptions']} births {member['births']}"
                f" partnerships {member['partnerships']} struck {struck(member)}"
            )
    for kind, keys in (("women", women), ("men", men)):
        rows.append(f"{label}_{kind}_keys {' '.join(hexed(key) for key in keys)}")
    # Pairs of chosen persons from one household of the file (their partnership does not come
    # along), and siblings among the chosen (for the market's exclusion).
    chosen = women + men
    by_household = {}
    for key in chosen:
        by_household.setdefault(pool.members[key][0], []).append(key)
    shared = {index: keys for index, keys in by_household.items() if len(keys) > 1}
    rows.append(f"{label}_shared_households {len(shared)} {sorted((index, [hexed(k) for k in keys]) for index, keys in shared.items())}")
    world = village_world(data, seed)
    start = world.inputs[0]
    rows.append(f"{label}_start_input bytes {len(start)} xxh3 {hexed(hash64(start))}")
    rows.append(f"{label}_start_input_hex {start.hex()}")
    rows.append(f"{label}_start {start_text(seed)}")
    rows.append(
        f"{label}_after_start living {len(world.sims)} households {len(world.households)} partnerships {len(world.partnerships)}"
        f" pregnancies {len(world.pregnancies)} used {world.used} entries {len(world.log)} state_hash {hexed(world.state_hash())}"
        f" save bytes {len(world.save())} xxh3 {hexed(hash64(world.save()))}"
    )
    check_all(world)
    # Kin among the chosen (their pool keys and the pool's records; the market excludes a pair
    # at or above the data's limit): every pair with `φ > 0`.
    kin = []
    sims = sorted(world.sims)
    for i, a in enumerate(sims):
        for b in sims[i + 1 :]:
            phi = world.kinship(a, b)
            if phi > 0:
                kin.append((a, b, phi))
    limit = wr.Fraction(world.data.kinship_limit)
    rows.append(
        f"{label}_kin pairs_with_phi_above_0 {len(kin)} at_or_above_the_limit {sum(1 for _, _, phi in kin if phi >= limit)} (limit {world.data.kinship_limit})"
        f" {[(a, b, str(phi)) for a, b, phi in kin]}"
    )
    # The women are SimIds 1 to 40, the men 41 to 80, each in its own household (1 to 80).
    assert [world.key_of(sim) for sim in sorted(world.sims)] == chosen
    assert all(world.sims[sim][2] == (FEMALE if sim <= len(women) else wr.MALE) for sim in world.sims)
    return rows


# ---------------------------------------------------------------------------------------
# The rejections of a village Start


def rejection_rows(data: wr.Data) -> list:
    seed = SEEDS[0]
    good = village_pool(data, seed)
    drawn = good["households"]
    women = drawn[: N_START // 2]
    men = drawn[N_START // 2 :]
    altered = copy.deepcopy(drawn)
    altered[0]["members"][0]["leave_home_at"] = 1
    # The first chosen person whose record the village changes, as the file holds it.
    carried = copy.deepcopy(drawn)
    (place, key) = next((i, h["members"][0]["lineage_key"]) for i, h in enumerate(drawn) if struck(data.pool.members[h["members"][0]["lineage_key"]][1]) != "-")
    carried[place]["members"][0] = copy.deepcopy(data.pool.members[key][1])
    town_draw = wr.pool_start(data, seed, N_START, False)["households"]
    other_seed = village_pool(data, SEEDS[1])["households"]
    women_count = len(wr.village_candidates(data.pool, 0))
    men_count = len(wr.village_candidates(data.pool, 1))
    too_many = 2 * min(women_count, men_count) + 2

    def changed(**fields):
        out = dict(good)
        out.update(fields)
        return out

    cases = [
        ("odd_n_start", data, changed(n_start=79), None),
        ("odd_n_start_no_households", data, changed(n_start=79, households=[]), None),
        ("immigration_on", data, changed(immigration=True), None),
        ("immigration_on_no_households", data, changed(immigration=True, households=[]), None),
        ("odd_n_start_before_immigration", data, changed(n_start=81, immigration=True), None),
        ("n_start_0_before_village", data, changed(n_start=0, households=[]), None),
        (f"too_few_candidates_n_start_{too_many}", data, changed(n_start=too_many, households=[]), None),
        ("immigration_before_candidates", data, changed(n_start=too_many, immigration=True, households=[]), None),
        ("men_first", data, changed(households=men + women), None),
        ("one_missing", data, changed(households=drawn[:-1]), None),
        ("one_altered_leave_home", data, changed(households=altered), None),
        (f"record_unstruck_place_{place}_{struck(data.pool.members[key][1])}", data, changed(households=carried), None),
        ("the_town_draw", data, changed(households=town_draw), None),
        ("another_seed", data, changed(households=other_seed), None),
        ("village_false_with_the_village_households", data, changed(village=False), None),
        ("decision_capacity_zero_before_pool", data, changed(n_start=79), 0),
    ]
    rows = [f"village_reject_candidates women {women_count} men {men_count} (largest even n_start {2 * min(women_count, men_count)})"]
    # The reject code list (tests/golden/log_codes.txt) by number: the codes of the pool start,
    # the village's three after 27 (the world rules after them are not codes of `Start`).
    rows += [
        f"village_reject_code {code} {wr.REJECT_NAMES[code]}"
        for code in sorted(wr.REJECT_NAMES)
        if wr.START_POOL_AND_FOUNDERS <= code <= wr.START_POOL_VILLAGE_CANDIDATES
    ]
    for name, case_data, pool, capacity in cases:
        code = wr.start_rejection([], [], case_data, CAPACITY if capacity is None else capacity, pool, seed)
        assert code is not None, name
        record = wr.signed(-1) + wr.varint(1) + start_variant(seed, CAPACITY if capacity is None else capacity, pool)
        # The full input where it is short (the Start of a core test).
        shown = f" input {record.hex()}" if len(record) < 200 else ""
        rows.append(
            f"village_reject {name} code {code} {wr.REJECT_NAMES[code]} input_bytes {len(record)} input_hash {hexed(hash64(record))}{shown}"
        )
    largest = 2 * min(women_count, men_count)
    assert wr.start_rejection([], [], data, CAPACITY, good, seed) is None
    assert wr.village_draw(data.pool, seed, largest) is not None and wr.village_draw(data.pool, seed, largest + 2) is None
    rows.append(f"village_reject_boundary n_start {largest} accepted by the draw, {largest + 2} rejected")
    return rows


def start_variant(seed: int, capacity, pool) -> bytes:
    """The postcard of `Start` (the input) of the village scenario with `capacity` and `pool`."""
    return wr.variant(
        0,
        wr.varint(seed),
        wr.string(TAG),
        wr.world_params(True, []),
        wr.seq([]),
        wr.seq([]),
        wr.capacity_bytes(capacity),
        wr.pool_start_bytes(pool),
    )


# ---------------------------------------------------------------------------------------
# The run


def run_rows(data: wr.Data, seed: int, years: int, out) -> list:
    pool = data.pool
    world = village_world(data, seed)
    women, men = wr.village_draw(pool, seed, N_START)
    # The variant of `pool_used` (not the reference's choice): the file households of the
    # chosen persons, in arrival order, each once.
    variant_used = []
    for key in women + men:
        index = pool.members[key][0]
        if index not in variant_used:
            variant_used.append(index)
    # The year end hashes the projection (and the census, which may read it again) at its
    # second: the first call per year end is the year hash's.
    variant_by_closed = {}
    plain_projection = world.projection

    def projection(closed=None):
        out_ = plain_projection(closed)
        if world.current is not None and world.current.kind == wr.YEAR_END_KIND and closed not in variant_by_closed:
            tail = wr.used_households_bytes(world.used)
            assert out_.endswith(tail) and hash64(out_) not in variant_by_closed.values()
            variant_by_closed[closed] = hash64(out_[: len(out_) - len(tail)] + wr.used_households_bytes(variant_used))
        return out_

    world.projection = projection
    label = f"village_{hexed(seed)}_run"
    rows = [f"{label}_config years {years} until {years * YEAR - 1} capacity {CAPACITY} n_start {N_START} tag {TAG} start_input_xxh3 {hexed(hash64(world.inputs[0]))}"]
    started = time.time()
    living = []
    for year in range(years):
        world.advance((year + 1) * YEAR - 1)
        living.append(len(world.sims))
        check_all(world)
    elapsed = time.time() - started
    variant_hashes = list(variant_by_closed.values())
    assert len(variant_hashes) == years == len(world.year_hashes)
    persons = sorted(set(world.sims) | set(world.archive))
    town_born = [sim for sim in persons if not world.is_pool_person(sim)]
    births = [0] * years
    for sim in town_born:
        births[world.born_of(sim) // YEAR] += 1
    deaths = [0] * years
    emigrated = [0] * years
    for sim, value in world.archive.items():
        if value[2] is None:
            emigrated[value[1] // YEAR] += 1
        else:
            deaths[value[1] // YEAR] += 1
    for year in range(years):
        rows.append(
            f"{label}_year {year} hash {hexed(world.year_hashes[year])} living {living[year]} births {births[year]} deaths {deaths[year]}"
            f" emigrated {emigrated[year]} variant_used_hash {hexed(variant_hashes[year])}"
        )
    save = world.save()
    rows.append(f"{label}_log_hash {hexed(world.log_hash())} entries {len(world.log)}")
    rows.append(
        f"{label}_end living {len(world.sims)} archived {len(world.archive)} town_born {len(town_born)} births {sum(births)} deaths {sum(deaths)}"
        f" emigrated {sum(emigrated)} partnerships {len(world.partnerships)} households {len(world.households)} pregnancies {len(world.pregnancies)}"
        f" used {world.used} variant_used {len(variant_used)} households"
    )
    rows.append(f"{label}_causes {sorted({world.cause_code(v[2]) for v in world.archive.values() if v[2] is not None})}")
    rows.append(f"{label}_state_hash {hexed(world.state_hash())} save bytes {len(save)} xxh3 {hexed(hash64(save))}")
    rows.append(f"{label}_seconds {elapsed:.1f}")
    if out is not None:
        out.mkdir(parents=True, exist_ok=True)
        (out / f"village_{hexed(seed)}_{years}y.save").write_bytes(save)
        (out / f"village_{hexed(seed)}_start_input.bin").write_bytes(world.inputs[0])
        (out / f"village_{hexed(seed)}_{years}y.golden.json").write_text(
            json.dumps(
                {
                    "core_version": wr.CORE_VERSION,
                    "data_hash": hexed(data.hash),
                    "year_hashes": [{"year": year, "hash": hexed(value)} for year, value in enumerate(world.year_hashes)],
                    "log_hash": hexed(world.log_hash()),
                },
                indent=2,
            )
            + "\n"
        )
    return rows


# ---------------------------------------------------------------------------------------
# The load check of the moved out: forged saves (module doc, `--moved-out`)

MOVED_OUT_YEAR = 9
MOVED_OUT_CODE = "LoadError::Inconsistent"


def alone_moved_out(world: wr.World) -> list:
    """The living sims that are moved out, in no open partnership, alone in their household and
    nobody's carer, by SimId."""
    carers = {value[11] for value in world.sims.values() if value[11] is not None}
    return [
        sim
        for sim in sorted(world.sims)
        if world.sims[sim][10] and world.open_partnership(sim) is None and world.members(world.sims[sim][9]) == [sim] and sim not in carers
    ]


def moved_out_verdicts(world: wr.World) -> dict:
    """Every load check of the reference (tools/arrival_reference.py `ALL_CHECKS`), with
    `check_households` without its new part, then the new check alone: check name, None or
    the error."""
    world.kinships = {}
    out = {}
    checks = [(f"{check.__name__}_without_the_moved_out" if check is wr.check_households else check.__name__, check) for check in ar.ALL_CHECKS]
    checks.append(("check_moved_out_households", wr.check_moved_out_households))
    for name, check in checks:
        try:
            if check is wr.check_households:
                check(world, pairs=False)
            else:
                check(world)
            out[name] = None
        except AssertionError as error:
            out[name] = str(error)
    return out


def rust_hash(value: int) -> str:
    text = hexed(value)
    return "0x" + "_".join(text[i : i + 4] for i in range(0, 16, 4))


def moved_out_reject_rows(name: str, label: str, base: wr.World, first: int, second: int, out) -> list:
    """`second` moves into the household of `first` in a copy of `base`; its emptied household
    ends at the clock. The rows: the two sims, the pins, every check's verdict, the hunks of
    the save (as `pool_field_reject` of tools/arrival_reference.py and as the Rust tuple of
    `Hunks` in tests/arrival.rs) and the code."""
    genuine = base.save()
    assert all(error is None for error in moved_out_verdicts(base).values()), "the genuine save loads"
    world = copy.deepcopy(base, {id(base.data): base.data})
    target, emptied = world.sims[first][9], world.sims[second][9]
    assert target != emptied and world.members(target) == [first] and world.members(emptied) == [second]
    world.sims[second][9] = target
    world.households[emptied][1] = world.closed
    forged = world.save()
    verdicts = moved_out_verdicts(world)
    assert [check for check, error in verdicts.items() if error is not None] == ["check_moved_out_households"], verdicts
    assert ar.rejecting_checks(world, ar.ALL_CHECKS) == ["check_households"]
    hunks = ar.save_hunks(genuine, forged)
    rows = [f"moved_out_base {name} {label} clock {base.closed} living {len(base.sims)} alone_and_moved_out {alone_moved_out(base)}"]
    for role, sim in (("stays", first), ("moves", second)):
        value = base.sims[sim]
        rows.append(
            f"moved_out_sim {name} {role} sim {sim} {'woman' if value[2] == FEMALE else 'man'} key {hexed(value[1])} born {value[0]}"
            f" age_years {(base.closed - value[0]) / YEAR:.4f} household {value[9]} formed {base.households[value[9]][0]} partnerships {value[8]}"
        )
    rows.append(
        f"moved_out_reject {name} base {label} base_pin {len(genuine)} {hexed(hash64(genuine))} hunks {len(hunks)} "
        + " ".join(f"{offset}:{removed.hex()}:{put.hex()}" for offset, removed, put in hunks)
        + f" changed_pin {len(forged)} {hexed(hash64(forged))} checks {ar.rejecting_checks(world, ar.ALL_CHECKS)} code {MOVED_OUT_CODE}"
        f" failed {[check for check, error in verdicts.items() if error is not None]} by {verdicts['check_moved_out_households']}"
        f" how sim {second} moves into household {target} of sim {first}, household {emptied} ends at the clock {world.closed}"
    )
    tuple_ = ", ".join(f'({offset}, {len(removed)}, "{put.hex()}")' for offset, removed, put in hunks)
    rows.append(f'moved_out_rust ("{name}", "{label}", ({len(genuine)}, {rust_hash(hash64(genuine))}), &[{tuple_}], ({len(forged)}, {rust_hash(hash64(forged))})),')
    if out is not None:
        out.mkdir(parents=True, exist_ok=True)
        (out / f"{label}.save").write_bytes(genuine)
        (out / f"{name}.save").write_bytes(forged)
    return rows


def moved_out_rows(data: wr.Data, seed: int, out) -> list:
    """(a) The village of `seed` at the end of year 9: the first woman alone moved out keeps her
    household, the first such man moves in. (b) `load_base` of tools/arrival_reference.py: the
    first two adults alone moved out, the second moves in with the first."""
    village = village_world(data, seed)
    village.advance((MOVED_OUT_YEAR + 1) * YEAR - 1)
    check_all(village)
    singles = alone_moved_out(village)
    woman = next(sim for sim in singles if village.sims[sim][2] == FEMALE)
    man = next(sim for sim in singles if village.sims[sim][2] == wr.MALE)
    rows = moved_out_reject_rows("village_moved_in", f"village_{hexed(seed)}_end_of_year_{MOVED_OUT_YEAR}", village, woman, man, out)
    base = ar.arrival_world()
    base.advance(3 * YEAR - 1)
    ar.check_all(base)
    adults = [sim for sim in alone_moved_out(base) if base.closed - base.sims[sim][0] >= 18 * YEAR]
    rows += moved_out_reject_rows("load_base_moved_in", "load_base", base, adults[0], adults[1], out)
    return rows


# The runner of `criteria village` (design note of the village, sections 7 and 7.2, Grok D1
# to D4): one village's export in the criteria kit's format and its block read back; the
# aggregate, its twin and the selection of the counted villages on given inputs

RUNNER_CAPACITY = 160  # Grok D1: capacity 160, 80 founders, G4 otherwise


def masked(text: str) -> str:
    """The export with the values a reader recomputes through `ln` or `Φ` masked, as the Rust
    kit compares it: in every line the content between `"z":[` and `]` and between `"gl":"`
    and `"` replaced by `_`."""
    out = re.sub(r'"z":\[[^\]]*\]', '"z":[_]', text)
    return re.sub(r'"gl":"[^"]*"', '"gl":"_"', out)


def export_rows(data: wr.Data, seed: int, ends: list, out, capacity: int = RUNNER_CAPACITY) -> list:
    """The village of `seed` with `capacity` (the runner's 160 by default) run to the last
    second of each year count in `ends` (ascending, one world advanced on), at each its export
    as `criteria_reference.py export` writes one (`export_world` on the repository's documents
    under data/, `export_text`), its masked form (`masked`), read back with `read_export`, and
    the village block of the report (`village_obj`). With `out` the exports are written to
    `out/village_<seed>_c<capacity>_<E>y.ndjson`."""
    import criteria_reference as cr
    import data_reference

    documents = data_reference.documents(wr.ROOT / "data")
    cr.assert_village_h2(documents)
    world = village_world(data, seed, capacity=capacity)
    label = f"village_{hexed(seed)}_c{capacity}_export"
    rows = [
        f"{label}_config ends {ends} capacity {capacity} n_start {N_START} documents data/ (demography, calibration/genetics.json, genetics/genome_map.json)"
        f" with the pool data/pool/pool.bin and data/pool/pool_frequencies.json data_hash {hexed(data.hash)} switch on"
        f" start_input bytes {len(world.inputs[0])} xxh3 {hexed(hash64(world.inputs[0]))}"
    ]
    year = 0
    for end_years in ends:
        started = time.time()
        while year < end_years:
            world.advance((year + 1) * YEAR - 1)
            year += 1
        run_s = time.time() - started
        end = end_years * YEAR - 1
        started = time.time()
        text = cr.export_text(cr.export_world(world, end, documents))
        raw = text.encode("ascii")
        mask = masked(text).encode("ascii")
        back = cr.read_export(text)
        assert cr.export_text(back) == text, "the reader and the writer round-trip"
        assert not any(c["carried"] for c in back["couples"]), "the village carries no partnership"
        rows_ = cr.Rows(back)
        obj = cr.village_obj(rows_)
        units = cr.village_block(rows_)["units"]
        export_s = time.time() - started
        if out is not None:
            out.mkdir(parents=True, exist_ok=True)
            (out / f"village_{hexed(seed)}_c{capacity}_{end_years}y.ndjson").write_bytes(raw)
        h = back["header"]
        rows.append(
            f"{label}_{end_years}y end {end} bytes {len(raw)} xxh3 {hexed(hash64(raw))} masked_bytes {len(mask)} masked_xxh3 {hexed(hash64(mask))}"
            f" persons {h['persons']} couples {h['couples']} core {h['core_version']} data_hash {hexed(h['data_hash'])}"
            f" world_seed {hexed(h['world_seed'])} pool_seed {hexed(h['pool_seed'])}"
        )
        rows.append(
            f"{label}_{end_years}y_block end_years {obj['end_years']} g_star {obj['g_star']} n {obj['n']} units {obj['units']}"
            f" V {obj['variance']} ({cr.from_bits(obj['variance']) if obj['variance'] else None!r}) E {obj['expectation']}"
            f" ({cr.from_bits(obj['expectation']) if obj['expectation'] else None!r}) h2 {cr.VILLAGE_H2!r} blocks {obj['blocks']}"
            f" living {obj['living']} finding {obj['finding']} measurable {cr.village_measurable(obj)}"
        )
        rows.append(f"{label}_{end_years}y_unit_keys {' '.join(hexed(p['key']) for p in units)}")
        rows.append(f"{label}_{end_years}y_unit_z_h {' '.join(cr.bits(p['z'][cr.Z_H]) for p in units)}")
        rows.append(f"{label}_{end_years}y_log_hash {hexed(world.log_hash())} entries {len(world.log)} year_hash_last {hexed(world.year_hashes[-1])}")
        rows.append(f"{label}_{end_years}y_seconds run {run_s:.1f} export_and_read {export_s:.1f}")
    rows.append(f"{label}_year_hashes {' '.join(hexed(h) for h in world.year_hashes)}")
    return rows


# Four synthetic sets of 20 villages for the aggregate and the extension rule
# (`criteria_reference.village_aggregate`, K2): name -> (V̄ target, s target, Ē target).
# Each `V_i` is the sample variance of 24 to 30 synthetic `z_h`, as a village's would be.
AGGREGATE_SETS = {
    "green_power_high": (0.79, 0.15, 0.80),
    "green_power_low": (0.86, 0.18, 0.85),
    "red_power_high": (0.95, 0.14, 0.70),
    "red_power_low": (0.60, 0.20, 0.90),
}
AGGREGATE_SEED = 0x5EED_0004_0000_00A0


def standard(values: list) -> list:
    """`values` shifted to fold mean 0 and scaled to sample variance 1 (the kit's estimators)."""
    import criteria_reference as cr

    m = cr.mean(values)
    sd = math.sqrt(cr.variance(values))
    return [(x - m) / sd for x in values]


def irwin_hall(rng, n: int) -> list:
    """`n` values, each the sum of 12 uniforms `u / 2^64` from SplitMix64 minus 6 (exact
    arithmetic and folds only, so the values do not depend on the platform's libm)."""
    out = []
    for _ in range(n):
        acc = 0.0
        for _ in range(12):
            acc = acc + rng.next() / 18446744073709551616.0
        out.append(acc - 6.0)
    return out


def aggregate_sets() -> dict:
    import criteria_reference as cr

    sets = {}
    for index, (name, (v_bar, s, e_bar)) in enumerate(AGGREGATE_SETS.items()):
        rng = cr.SplitMix64(AGGREGATE_SEED + index)
        w = standard(irwin_hall(rng, cr.VILLAGE_SEEDS))
        u = standard(irwin_hall(rng, cr.VILLAGE_SEEDS))
        villages = []
        for i in range(cr.VILLAGE_SEEDS):
            target = v_bar + s * w[i]
            assert target > 0.0
            zs = [z * math.sqrt(target) for z in standard(irwin_hall(rng, 24 + i % 7))]
            e = round(e_bar + 0.003 * u[i], 4)
            villages.append({"v": cr.variance(zs), "e": e})
        sets[name] = villages
    return sets


def rescaled_twin(vs: list, e_bar: float) -> dict:
    """The kit's former twin (before Grok D3), for comparison only: `V_i / V̄`, its fold mean,
    its sample spread recomputed, green iff `|t̄ − Ē| ≤ 4 se_t`."""
    import criteria_reference as cr

    n = cr.exact(len(vs))
    v_bar = cr.fold(vs) / n
    ts = [v / v_bar for v in vs]
    t_bar = cr.fold(ts) / n
    s_t = math.sqrt(cr.variance(ts))
    se_t = s_t / math.sqrt(n)
    return {"v_bar": t_bar, "s": s_t, "se": se_t, "tol": 4.0 * se_t, "pass": abs(t_bar - e_bar) <= 4.0 * se_t}


def b(value):
    import criteria_reference as cr

    return cr.bits(value) if isinstance(value, float) else value


def aggregate_rows(out) -> list:
    """The aggregate (K2 with Grok D3) on the sets above: `V̄`, `Ē`, `s`, `se`, pass, power,
    the decision at 1200 and at 2100, the twin (`criteria_reference.village_twin`, shifted to
    mean 1) beside the former rescaled one; then the selection of D1 (`selection_rows`)."""
    import criteria_reference as cr

    rows, fixture = [], {}
    sets = aggregate_sets()
    for name, villages in sets.items():
        vs = [v["v"] for v in villages]
        es = [v["e"] for v in villages]
        at_1200 = cr.village_aggregate(vs, es, 1200)
        at_2100 = cr.village_aggregate(vs, es, 2100)
        a = at_1200
        twin = cr.village_twin(vs, a["v_bar"], a["e_bar"])
        assert (twin["v_bar"], twin["se"], twin["pass"]) == (a["twin_v_bar"], a["twin_se"], a["twin_pass"])
        old = rescaled_twin(vs, a["e_bar"])
        rows.append(
            f"aggregate {name} V_bar {b(a['v_bar'])} ({a['v_bar']:.6f}) E_bar {b(a['e_bar'])} ({a['e_bar']:.6f}) s {b(a['s'])} ({a['s']:.6f})"
            f" se {b(a['se'])} ({a['se']:.6f}) tol {b(a['tol'])} diff {abs(a['v_bar'] - a['e_bar']):.6f} pass {a['pass']} power {b(a['power'])} ({a['power']:.6f})"
            f" decision_1200 {a['decision']} decision_2100 {at_2100['decision']}"
        )
        for label, t in (("shifted", twin), ("rescaled_old", old)):
            rows.append(
                f"aggregate {name} twin_{label} V_bar {b(t['v_bar'])} ({t['v_bar']:.17g}) s {b(t['s'])} ({t['s']:.6f}) se {b(t['se'])} ({t['se']:.6f})"
                f" tol {b(t['tol'])} diff {abs(t['v_bar'] - a['e_bar']):.6f} pass {t['pass']}"
            )
        rows.append(f"aggregate {name} twin_shifted s_bits_equal_s {b(twin['s']) == b(a['s'])} se_bits_equal_se {b(twin['se']) == b(a['se'])}")
        fixture[name] = {
            "villages": [{"v": cr.bits(v["v"]), "e": cr.bits(v["e"]), "e_decimal": v["e"]} for v in villages],
            "aggregate_1200": {k: b(x) for k, x in at_1200.items()},
            "decision_2100": at_2100["decision"],
            "twin_shifted": {k: b(x) for k, x in twin.items()},
            "twin_rescaled_old": {k: b(x) for k, x in old.items()},
        }
    selection, sel_rows = selection_example(sets["green_power_high"])
    rows += sel_rows
    if out is not None:
        out.mkdir(parents=True, exist_ok=True)
        (out / "aggregate.json").write_text(json.dumps({"seed": hexed(AGGREGATE_SEED), "sets": fixture, "selection": selection}, indent=1) + "\n")
    return rows


# The selection example of D1: 30 villages in list order; findings at 2, 5 and 11 (case
# `twenty`), and at 2, 5, 11 and 22 to 29 (case `nineteen`, 19 measurable).
SELECTION_FINDINGS = {"twenty": {2: "no_block", 5: "fewer_than_two_units", 11: "fewer_than_two_units"}}
SELECTION_FINDINGS["nineteen"] = {**SELECTION_FINDINGS["twenty"], **{i: "fewer_than_two_units" for i in range(22, 30)}}
# A measurable village that is not counted carries values that would move the aggregate.
UNCOUNTED_V, UNCOUNTED_E = 3.0, 0.5


def selection_example(villages: list) -> tuple:
    """The 30 village objects of each case (the fields `village_measurable` reads, `V`, `E`),
    the counted indices (`criteria_reference.village_counted`) and the aggregate over the
    counted ones: the `j`-th counted village takes the `j`-th (V, E) of `villages` (the set
    green_power_high), every other measurable village `UNCOUNTED_V`, `UNCOUNTED_E`."""
    import criteria_reference as cr

    out, rows = {}, []
    for case, findings in SELECTION_FINDINGS.items():
        objs = []
        for i in range(cr.VILLAGE_RUN):
            if i in findings:
                g_star, n = (None, 0) if findings[i] == "no_block" else (0, 1)
                objs.append({"index": i, "g_star": g_star, "n": n, "finding": findings[i], "v": None, "e": None})
            else:
                objs.append({"index": i, "g_star": 0, "n": 14, "finding": None, "v": None, "e": None})
        sel = cr.village_counted(objs)
        for j, i in enumerate(sel["counted"]):
            objs[i]["v"], objs[i]["e"] = villages[j]["v"], villages[j]["e"]
        for obj in objs:
            if cr.village_measurable(obj) and obj["index"] not in sel["counted"]:
                obj["v"], obj["e"] = UNCOUNTED_V, UNCOUNTED_E
        result = None
        if sel["finding"] is None:
            vs = [objs[i]["v"] for i in sel["counted"]]
            es = [objs[i]["e"] for i in sel["counted"]]
            result = cr.village_aggregate(vs, es, 1200)
        reported = [obj["index"] for obj in objs if cr.village_measurable(obj) and obj["index"] not in sel["counted"]]
        rows.append(
            f"selection {case} findings {sorted(findings)} measurable {sel['measurable']} counted {sel['counted']} reported_not_counted {reported}"
            f" finding {sel['finding']}"
        )
        if result is not None:
            rows.append(
                f"selection {case} aggregate V_bar {b(result['v_bar'])} E_bar {b(result['e_bar'])} s {b(result['s'])} se {b(result['se'])} tol {b(result['tol'])}"
                f" pass {result['pass']} power {b(result['power'])} decision {result['decision']} twin_v_bar {b(result['twin_v_bar'])}"
                f" twin_se {b(result['twin_se'])} twin_pass {result['twin_pass']}"
            )
        out[case] = {
            "villages": [{k: b(x) for k, x in obj.items()} for obj in objs],
            "counted": sel["counted"], "measurable": sel["measurable"], "finding": sel["finding"],
            "aggregate_1200": None if result is None else {k: b(x) for k, x in result.items()},
        }
    return out, rows


def main(argv) -> int:
    out = Path(argv[argv.index("--out") + 1]) if "--out" in argv else None
    seed = int(argv[argv.index("--seed") + 1], 16) if "--seed" in argv else SEEDS[0]
    if "--export" in argv:
        ends = [int(x) for x in argv[argv.index("--export") + 1].split(",")]
        assert ends == sorted(ends) and ends[0] > 0
        capacity = int(argv[argv.index("--capacity") + 1]) if "--capacity" in argv else RUNNER_CAPACITY
        for row in export_rows(wr.REPOSITORY, seed, ends, out, capacity):
            print(row, flush=True)
        return 0
    if "--aggregate" in argv:
        for row in aggregate_rows(out):
            print(row, flush=True)
        return 0
    run = "--run" in argv
    moved_out = "--moved-out" in argv
    years = RUN_YEARS
    if run:
        at = argv.index("--run")
        if at + 1 < len(argv) and argv[at + 1].isdigit():
            years = int(argv[at + 1])
    select = "--select" in argv or not (run or moved_out or "--rejections" in argv)
    rejections = "--rejections" in argv or not (run or moved_out or "--select" in argv)
    data = wr.REPOSITORY
    for row in code_rows():
        print(row, flush=True)
    if select:
        for s in SEEDS:
            for row in selection_rows(data, s):
                print(row, flush=True)
    if rejections:
        for row in rejection_rows(data):
            print(row, flush=True)
    if run:
        for row in run_rows(data, seed, years, out):
            print(row, flush=True)
    if moved_out:
        for row in moved_out_rows(data, seed, out):
            print(row, flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

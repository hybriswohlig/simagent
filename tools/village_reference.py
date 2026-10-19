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
  choice; NOTES.md of the village).

Usage: .venv/bin/python tools/village_reference.py [--select] [--rejections] [--run [YEARS]]
       [--seed HEX] [--out DIR]
       (no option: --select and --rejections)
"""

import copy
import json
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


def village_world(data: wr.Data, seed: int, pool=None) -> wr.World:
    """The world of the village `Start` (module doc), after `Start`."""
    pool = village_pool(data, seed) if pool is None else pool
    return wr.World(seed, TAG, True, [], (), data, capacity=CAPACITY, pool=pool, nutrition=())


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
    # The reject code list (tests/golden/log_codes.txt) by number: the three after 27.
    rows += [f"village_reject_code {code} {wr.REJECT_NAMES[code]}" for code in sorted(wr.REJECT_NAMES) if code >= wr.START_POOL_AND_FOUNDERS]
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


def main(argv) -> int:
    out = Path(argv[argv.index("--out") + 1]) if "--out" in argv else None
    seed = int(argv[argv.index("--seed") + 1], 16) if "--seed" in argv else SEEDS[0]
    run = "--run" in argv
    years = RUN_YEARS
    if run:
        at = argv.index("--run")
        if at + 1 < len(argv) and argv[at + 1].isdigit():
            years = int(argv[at + 1])
    select = "--select" in argv or not (run or "--rejections" in argv)
    rejections = "--rejections" in argv or not (run or "--select" in argv)
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
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

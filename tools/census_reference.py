#!/usr/bin/env python3
"""The yearly census with the clamp counts of the body mapping (step 11; design note of the
census with the clamp rate, section 3, mainly 3.7; ADR-0020 section 7, ADR-0005 sections 1
and 8, ADR-0015 sections 2 to 4, ADR-0013 section 4, ADR-0017 section 2), written from the
note, the ADRs and the genetics specification, independent of the Rust code. The model is
tools/world_reference.py (a world, its pool persons and their seeds), the mapping is
tools/body_reference.py (`body_params`, `bio_age`, the roles of map 4).

Per year end of a run (the observer `YearEnd` at `t = year_start(k + 1) − 1 s`, after every
domain event of year `k`; `World.advance(t)` runs that second fully):

- `living`: the number of living sims (`World.sims`), children and arrivals included, never
  the archive.
- `body_evaluated`, the 21 clamp counts in `CLAMPED_KEYS` order, `curve_clamped`: if the data
  pass `BodyData::new` (`body_reference.body_data_check` with the 35 traits of map 4: each
  trait in the map with a phenotype row whose child and adult weights are equal bit for bit,
  `core:mc1r` and `core:herc2` recessive on an autosome), every living sim in ascending SimId
  is evaluated once; else every count is 0 (note section 3.2: "all zero if it fails").
  Integers only.
- `CLAMPED_KEYS`: the parameters of `contracts/body.json` whose unit is `z` or `cm`, in the
  contract's order (`height_cm`, the eight body `*_z`, `face_axis_01` to `face_axis_12`).

One evaluation (note sections 1 and 3.1), for the living sim `s` at `t`:

- genome, sex, lineage key and born from the state; `age = t − born` in seconds; `x =
  as_years(age)` (`age / 31 449 600` as f64).
- `E` and `C` from the person's seeds and keys (`World.env_of`: a pool person both with the
  pool seed at its PoolOrigin keys; a town person of two pool persons `E` with the world seed
  and `C` with the pool seed at its parents' keys; anyone else both with the world seed at its
  parents' keys; the self rule, its own key for both parents, only without keys).
- the 35 phenotypes `z` of `body_reference.BODY_TRAITS_MAP4` (genetics specification 4.2,
  `world_reference.phenotype_parts`) at the age 0 (the weights do not depend on the age,
  `BodyData::new` checks that), each read by its roles (`body_reference.ROLES`, map 4 names).
- the copies of allele 1 at `core:mc1r` and `core:herc2`.
- `g_L = genetic_value` of `longevity_trait` (`A / σ_ref`, or `(A − m_ref) / σ_ref` with a
  `mean_ref` in the map, Grok GF-1), `ĝ` held on [−6, 6]
  (`longevity_input`), `Δ = ((−σ_g) · ĝ) / b_sex` with `b_sex` the Gompertz `b` of the sex
  (ADR-0017 section 2, left to right as written; `Δ` is 0 without a trait of longevity).
- `b = bio_age(x, Δ, w)` (7.4, `w` = `core:bio_age_weight`).
- the nutrition years of the world (`World.nutrition`, the `WorldParams` of `Start`); for an
  arrival (a pool person with `born_pool`) without every entry whose year ends at or before
  its arrival second (G6: `year_start(year + 1) ≤ arrived`); a pool child and a town person
  keep the whole list (the loss skips years before the birth).
- `body_reference.body_params`: the raw values of 10, `z` and `cm` held on the contract's
  range with a clamp flag each (clamped iff the raw value lies truly outside), the curve flag
  (`|z_h|` or `|z_p|` above 6, ADR-0012; exactly 6 is not held).

The selection rule and bound of the reference population (note section 3.4, ADR-0005
section 8, ADR-0020 section 7): `check_record`.

The fixtures (NOTES.md says why each was chosen and what is unclear):

- (a) `a_template_extremes`: the template of map 4 with the repository's life course data,
  27 `Start` founders naming every locus of the map (`founder_alleles`), nine couples, two
  nutrition shocks, 20 years (year ends 0 to 19).
- (b) `b_pool_c_immigration`: the pool `pool_c` (the only golden pool that can feed an
  immigration of `q` ≥ 1 for more than a second; `pool_b` holds 33 persons, so `n_start` ≥ 50
  takes them all at the start and the immigration of year 0 stops the run with `pool.empty`),
  the scenario "arrival" of tools/arrival_reference.py (seed 0xff, `n_start` 50, immigration
  on, switch on) with three nutrition shocks, year ends 0 to 13 (the pool runs out in year 14).
- (b2) `b2_pool_b_start`: the pool `pool_b` at the start without immigration (`n_start` 50:
  every household), two nutrition shocks, 20 years.
- (c) `c_prerun_small`: the small run of tools/prerun_reference.py (40 founders, seed
  0x11b019a1, tag `core:prerun_measure`, capacity 40, the pre-run data, 30 years). The
  pre-run data drop `core:mc1r` and `core:herc2`, so `BodyData::new` rejects them and every
  count is 0 (the note's rule). (c') `c_prerun_small_reference_alleles` evaluates the same
  run with MC1R and HERC2 read as 0 copies (the note's section 1 assumption); clamp counts do
  not depend on them (pigment is never clamped).

Usage: $HOME/.cache/simagent-tools-venv/bin/python tools/census_reference.py [--out DIR]
       (default DIR ~/.cache/simagent-notes/ref-census; writes census.json and census.txt)
       ... census_reference.py --scan-a FIRST LAST   (seeds of fixture (a) and their founders
       whose curve is held; for choosing the seed, not a pin)
       ... census_reference.py --phenotypes FILE   (the golden phenotypes.json, compact)
       ... census_reference.py --pure-heights [--out DIR]   (pure_heights.json: the pure height of
       ADR-0015 4 and spec 10.2, and the height before the loss of age)
       ... census_reference.py --records [--out DIR]   (records_phenotypes.json: the 35 `z`
       of the living at the last year end of the scenario "records" of arrival_reference.py)
"""

import json
import math
import struct
import sys
import time
from pathlib import Path

import arrival_reference
import body_reference as br
import data_reference
import prerun_reference
import world_reference as wr
from postcard_reference import FEMALE, MALE, varint
from rng_reference import founder_key
from world_reference import YEAR, as_years, copies_at, genetic_value, hash64, longevity_input, phenotype_parts

ROOT = Path(__file__).resolve().parent.parent
TEMPLATE = ROOT / "tests" / "golden" / "map4_template"
CONTRACT_PATH = ROOT / "contracts" / "body.json"
DEFAULT_OUT = Path.home() / ".cache" / "simagent-notes" / "ref-census"

CONTRACT = json.loads(CONTRACT_PATH.read_text())["parameters"]
PARAM_KEYS = [entry["key"] for entry in CONTRACT]
CLAMPED_KEYS = [entry["key"] for entry in CONTRACT if entry["unit"] in ("z", "cm")]
assert len(PARAM_KEYS) == 44 and len(CLAMPED_KEYS) == 21 and CLAMPED_KEYS[0] == "height_cm"
assert CLAMPED_KEYS[1:9] == [
    "leg_ratio_z",
    "head_size_z",
    "body_fat_z",
    "fat_distribution_z",
    "muscle_z",
    "shoulder_width_z",
    "chest_depth_z",
    "hip_width_z",
]
assert CLAMPED_KEYS[9:] == [f"face_axis_{n:02d}" for n in range(1, 13)]
# Every other unit is bounded by its formula and never clamped (ADR-0005 section 1).
assert {entry["unit"] for entry in CONTRACT if entry["key"] not in CLAMPED_KEYS} <= {"years", "fraction", "stage", "melanin"}

# The reference population (note section 3.4, ADR-0005 section 8): at least this many
# evaluations, per key at most one per thousand.
MIN_EVALUATIONS = 100_000
RATE_DENOMINATOR = 1000
LAST_YEARS = 10


def bits(value: float) -> str:
    return data_reference.f64_bits(value)


def hexed(value: int) -> str:
    return f"{value:016x}"


def year_end(year: int) -> int:
    """The second of the observer `YearEnd` of year `year` (ADR-0002)."""
    return (year + 1) * YEAR - 1


# ---------------------------------------------------------------------------------------
# The body data of a world


class BodyCensusData:
    """What the census reads from the data (`BodyData::new`, ADR-0015 section 2): the
    calibration's body values, the 35 traits of map 4 as phenotype traits, MC1R and HERC2,
    `σ_g`, the Gompertz `b` per sex and the trait of longevity; or the reason the data are
    rejected (`reason`, then the census counts nothing). `reference_alleles` (fixture c'
    only, not the note's rule): accept data without `core:mc1r` and `core:herc2` and read
    both as 0 copies."""

    def __init__(self, data: wr.Data, documents: tuple, reference_alleles: bool = False):
        _, calibration, genome_map = documents
        self.data = data
        self.reason = br.body_data_check((calibration, genome_map), br.BODY_TRAITS_MAP4)
        self.reference_alleles = False
        if self.reason is not None and reference_alleles:
            loci = {locus["id"] for locus in genome_map["loci"]}
            assert "core:mc1r" not in loci and "core:herc2" not in loci, "only for data without both loci"
            assert self.reason.startswith("data.body_traits: core:mc1r"), self.reason
            self.reason = None
            self.reference_alleles = True
        if self.reason is not None:
            return
        stripped_calibration = data_reference.strip(calibration)
        stripped_map = data_reference.strip(genome_map)
        rows = {row["trait"]: row for row in stripped_calibration["traits"]}
        self.traits = {trait: wr.PhenotypeTrait(trait, stripped_map, rows[trait]) for trait in br.BODY_TRAITS_MAP4}
        self.values = br.BodyValues(stripped_calibration)
        loci = {locus["id"]: locus for locus in stripped_map["loci"]}
        self.mc1r = loci.get("core:mc1r")
        self.herc2 = loci.get("core:herc2")
        assert (self.mc1r is None) == self.reference_alleles and (self.herc2 is None) == self.reference_alleles

    @property
    def accepted(self) -> bool:
        return self.reason is None


# ---------------------------------------------------------------------------------------
# One evaluation


def delta_of(data: wr.Data, genome: dict, sex: bytes) -> tuple:
    """(`g_L`, `ĝ`, `Δ`) of a person (ADR-0017 section 2; genetics specification 7.2, 11.4):
    `Δ = ((−σ_g) · ĝ) / b_sex`; without a trait of longevity (None, None, 0.0)."""
    if data.longevity is None:
        return None, None, 0.0
    g = genetic_value(genome, data.longevity)
    held = longevity_input(g)
    b_sex = data.senescence[sex][1]
    return g, held, ((-data.sigma_g) * held) / b_sex


def nutrition_of(world: wr.World, sim: int) -> list:
    """The nutrition years the mapping reads for a living sim (note section 3.1, G6): the
    world's list; for an arrival without every year that ends at or before its arrival."""
    years = list(world.nutrition)
    if world.is_arrival(sim):
        arrived = world.origins[sim]["arrived"]
        years = [(year, index) for year, index in years if not (year + 1) * YEAR <= arrived]
    return years


class Evaluator:
    """The evaluations of one world: the parts of a person that do not change with its age
    (its phenotypes, copies and `Δ`) are kept per SimId."""

    def __init__(self, world: wr.World, body: BodyCensusData):
        self.world = world
        self.body = body
        self.fixed = {}

    def person_parts(self, sim: int) -> dict:
        if sim in self.fixed:
            return self.fixed[sim]
        world, body = self.world, self.body
        if sim in world.sims:
            born, key, sex, genome = world.sims[sim][:4]
        else:
            # An archived sim (its heights at its leaving second): (born, left, cause, key, sex, genome, ...).
            value = world.archive[sim]
            born, key, sex, genome = value[0], value[3], value[4], value[5]
        e_seed, c_seed, parents = world.env_of(sim)
        z_of, parts = {}, {}
        for trait, terms in body.traits.items():
            part = phenotype_parts(e_seed, terms, genome, key, parents, 0, c_seed)
            z_of[trait], parts[trait] = part["z"], part
        if body.reference_alleles:
            mc1r = herc2 = 0
        else:
            mc1r, herc2 = sum(copies_at(genome, body.mc1r)), sum(copies_at(genome, body.herc2))
        g, held, delta = delta_of(world.data, genome, sex)
        fixed = {
            "born": born,
            "key": key,
            "sex": sex,
            "z_of": z_of,
            "z": {role: z_of[pair[1]] for role, pair in br.ROLES.items()},
            "mc1r": mc1r,
            "herc2": herc2,
            "g_L": g,
            "g_hat": held,
            "delta": delta,
            "seeds": (e_seed, c_seed, parents),
        }
        self.fixed[sim] = fixed
        return fixed

    def evaluate(self, sim: int, t: int, g6: bool = True) -> dict:
        """`body_at(sim, t)` (note section 3.1). `g6` False is a switch of this reference only
        (the arrival's whole list), to show what G6 changes."""
        parts = self.person_parts(sim)
        age = t - parts["born"]
        assert age >= 0, (sim, t)
        x = as_years(age)
        b = br.bio_age(x, parts["delta"], self.body.values.bio_age_weight)
        nutrition = nutrition_of(self.world, sim) if g6 else list(self.world.nutrition)
        person = {
            "sex": parts["sex"],
            "age": age,
            "b": b,
            "born": parts["born"],
            "nutrition": nutrition,
            "z": parts["z"],
            "mc1r": parts["mc1r"],
            "herc2": parts["herc2"],
        }
        params, curve = br.body_params(self.body.values, CONTRACT, person)
        assert list(params) == PARAM_KEYS
        for key, (value, clamped) in params.items():
            if key not in CLAMPED_KEYS:
                assert not clamped
        raw = br.raw_values(self.body.values, person)[0]
        return {"sim": sim, "age": age, "x": x, "b": b, "nutrition": nutrition, "params": params, "curve": curve, "parts": parts, "raw": raw}


RANGES = {entry["key"]: (float(entry["range"][0]), float(entry["range"][1])) for entry in CONTRACT}


class Margins:
    """How close any count came to flipping (not part of the census): over every evaluation,
    the smallest `|raw − bound|` of a clamped key (with the nearer bound) and the smallest
    `||z_h| − 6|`, `||z_p| − 6|`; each with its year and sim. A distance of a few ulps would
    make a count depend on the last bit of the core's arithmetic."""

    def __init__(self):
        self.key = None
        self.curve = None

    def add(self, year: int, evaluation: dict) -> None:
        sim = evaluation["sim"]
        for key in CLAMPED_KEYS:
            value = evaluation["raw"][key]
            for bound in RANGES[key]:
                distance = abs(value - bound)
                if self.key is None or distance < self.key[0]:
                    self.key = (distance, key, bound, value, year, sim)
        z = evaluation["parts"]["z"]
        for role in ("z_h", "z_p"):
            distance = abs(abs(z[role]) - 6.0)
            if self.curve is None or distance < self.curve[0]:
                self.curve = (distance, role, z[role], year, sim)

    def json(self):
        if self.key is None:
            return None
        distance, key, bound, value, year, sim = self.key
        c_distance, role, z, c_year, c_sim = self.curve
        return {
            "nearest_clamp_bound": {"distance": distance, "ulps_of_bound": distance / math.ulp(bound), "key": key, "bound": bound, "raw": value, "raw_bits": bits(value), "year": year, "sim": sim},
            "nearest_curve_bound": {"distance": c_distance, "role": role, "z": z, "z_bits": bits(z), "year": c_year, "sim": c_sim},
        }


def lived_secs(world: wr.World, year: int) -> int:
    """The seconds lived in calendar year `year` by every sim of the state at its year end,
    living or archived (ADR-0004 section 8): per sim the length of `[max(born, year_start),
    min(left, year_start(year + 1)))`, the second of birth lived, the second of leaving
    (death, emigration) not; a living sim to the end of the year. An integer."""
    start, end = year * YEAR, (year + 1) * YEAR
    total = 0
    for value in world.sims.values():
        assert value[0] < end
        total += end - max(value[0], start)
    for value in world.archive.values():
        born, left = value[0], value[1]
        total += max(0, min(left, end) - max(born, start))
    return total


def census_row(evaluator: Evaluator, year: int, margins=None) -> tuple:
    """The census of year `year` at its year end (the world advanced to it): the row and the
    curve-held persons (sim, lineage key, `z_h`, `z_p` before the hold) for the trace."""
    world = evaluator.world
    t = year_end(year)
    assert world.closed == t
    row = {"year": year, "living": len(world.sims), "lived_secs": lived_secs(world, year), "body_evaluated": 0, "clamped": [0] * len(CLAMPED_KEYS), "curve_clamped": 0}
    held = []
    if not evaluator.body.accepted:
        return row, held
    for sim in sorted(world.sims):
        evaluation = evaluator.evaluate(sim, t)
        if margins is not None:
            margins.add(year, evaluation)
        row["body_evaluated"] += 1
        for index, key in enumerate(CLAMPED_KEYS):
            row["clamped"][index] += evaluation["params"][key][1]
        if evaluation["curve"]["clamped"]:
            row["curve_clamped"] += 1
            z = evaluation["parts"]["z"]
            held.append([sim, hexed(evaluation["parts"]["key"]), bits(z["z_h"]), bits(z["z_p"])])
    assert row["body_evaluated"] == row["living"]
    return row, held


def params_bytes(params: dict) -> bytes:
    return b"".join(struct.pack("<d", params[key][0]) for key in PARAM_KEYS)


def snapshot(evaluator: Evaluator, year: int, kind_of) -> dict:
    """`body_at` of every living person at the year end of `year`: per person its inputs and
    the 44 parameters as f64 bits in the contract's order; `hash` per person is xxh3-64 of
    the 44 values as little-endian f64 in that order, `hash` of the snapshot xxh3-64 of
    (u64 LE SimId + those 352 bytes) per person in ascending SimId."""
    world = evaluator.world
    t = year_end(year)
    persons, whole = [], b""
    for sim in sorted(world.sims):
        evaluation = evaluator.evaluate(sim, t)
        parts = evaluation["parts"]
        raw = params_bytes(evaluation["params"])
        whole += struct.pack("<Q", sim) + raw
        e_seed, c_seed, parents = parts["seeds"]
        entry = {
            "sim": sim,
            "lineage_key": hexed(parts["key"]),
            "kind": kind_of(sim),
            "sex": "female" if parts["sex"] == FEMALE else "male",
            "born": parts["born"],
            "age_secs": evaluation["age"],
            "seed_e": hexed(e_seed),
            "seed_c": hexed(c_seed),
            "c_keys": None if parents is None else [hexed(parents[0]), hexed(parents[1])],
            "g_L": None if parts["g_L"] is None else bits(parts["g_L"]),
            "delta": bits(parts["delta"]),
            "bio_age": bits(evaluation["b"]),
            "nutrition": [[year_, index] for year_, index in evaluation["nutrition"]],
            "mc1r": parts["mc1r"],
            "herc2": parts["herc2"],
            "theta_i": bits(evaluation["curve"]["theta_i"]),
            "kappa": bits(evaluation["curve"]["kappa"]),
            "curve_clamped": evaluation["curve"]["clamped"],
            "clamped": [key for key in CLAMPED_KEYS if evaluation["params"][key][1]],
            "z": [bits(parts["z_of"][trait]) for trait in br.BODY_TRAITS_MAP4],
            "params": [bits(evaluation["params"][key][0]) for key in PARAM_KEYS],
            "hash": hexed(hash64(raw)),
        }
        persons.append(entry)
    return {"year": year, "t": t, "living": len(persons), "hash": hexed(hash64(whole)), "persons": persons}


# ---------------------------------------------------------------------------------------
# The selection rule and the bound (note section 3.4)


def select_rows(rows: list, last_year: int) -> list:
    """The tool's selection from every census row of a run whose last year end is
    `last_year`: the last ten years, then earlier ones one at a time while the sum of
    `body_evaluated` is below 100 000 (as long as there are rows)."""
    by_year = {row["year"]: row for row in rows}
    chosen, total, year = [], 0, last_year
    while year in by_year and (len(chosen) < LAST_YEARS or total < MIN_EVALUATIONS):
        chosen.append(by_year[year])
        total += by_year[year]["body_evaluated"]
        year -= 1
    return chosen


def check_record(rows: list, last_year: int) -> dict:
    """The gate's check of a record (note section 3.4): its years are exactly `last_year`,
    `last_year − 1`, … without a gap, at least ten, an eleventh or later year only while
    the sum of the later ones is below 100 000; `n = Σ body_evaluated ≥ 100 000`; per key
    `1000 · count ≤ n`. Returns {"green", "reason", "n", "counts"} (the first reason that
    fails, in that order)."""
    ordered = sorted(rows, key=lambda row: -row["year"])
    n = sum(row["body_evaluated"] for row in ordered)
    counts = [sum(row["clamped"][index] for row in ordered) for index in range(len(CLAMPED_KEYS))]
    result = {"n": n, "counts": counts, "green": False, "reason": None}
    years = [row["year"] for row in ordered]
    if not years or years[0] != last_year:
        result["reason"] = "not_the_last_year"
        return result
    if years != list(range(last_year, last_year - len(years), -1)):
        result["reason"] = "year_gap"
        return result
    if len(years) < LAST_YEARS:
        result["reason"] = "fewer_than_ten_years"
        return result
    total = 0
    for index, row in enumerate(ordered):
        if index >= LAST_YEARS and total >= MIN_EVALUATIONS:
            result["reason"] = "year_not_needed"
            return result
        total += row["body_evaluated"]
    if n < MIN_EVALUATIONS:
        result["reason"] = "fewer_than_100000"
        return result
    for index, key in enumerate(CLAMPED_KEYS):
        if RATE_DENOMINATOR * counts[index] > n:
            result["reason"] = f"rate {key}"
            return result
    result["green"] = True
    return result


def synthetic_rows(spec: list, clamped_at=None) -> list:
    """Rows (year, body_evaluated) with every count 0, one key's counts per row given by
    `clamped_at` = (key, [count per row]) or None."""
    rows = []
    for index, (year, evaluated) in enumerate(spec):
        clamped = [0] * len(CLAMPED_KEYS)
        if clamped_at is not None:
            clamped[CLAMPED_KEYS.index(clamped_at[0])] = clamped_at[1][index]
        rows.append({"year": year, "living": evaluated, "body_evaluated": evaluated, "clamped": clamped, "curve_clamped": 0})
    return rows


def selection_cases() -> list:
    """The cases of the bound and the selection (note section 3.4, the negative twins), on a
    run whose last year end is 749."""
    last = 749
    ten = [(749 - i, 10_000) for i in range(10)]
    cases = []

    def case(name, rows, expect):
        verdict = check_record(rows, last)
        assert verdict["green"] == (expect == "green") and (expect == "green" or verdict["reason"] == expect), (name, verdict)
        cases.append(
            {
                "name": name,
                "last_year": last,
                "rows": [[row["year"], row["body_evaluated"], {key: row["clamped"][i] for i, key in enumerate(CLAMPED_KEYS) if row["clamped"][i]}] for row in rows],
                "n": verdict["n"],
                "green": verdict["green"],
                "reason": verdict["reason"],
            }
        )

    # n = 100 000; a key at n / 1000 = 100 is green, at 101 red (the one-sided bound).
    case("bound_exact_green", synthetic_rows(ten, ("muscle_z", [10] * 10)), "green")
    case("bound_plus_one_red", synthetic_rows(ten, ("muscle_z", [10] * 9 + [11])), "rate muscle_z")
    case("height_at_bound_green", synthetic_rows(ten, ("height_cm", [100] + [0] * 9)), "green")
    # 99 999 over ten years: red without the eleventh year, green with it.
    short = [(749 - i, 10_000 if i else 9_999) for i in range(10)]
    case("ten_years_99999_red", synthetic_rows(short), "fewer_than_100000")
    case("eleven_years_green", synthetic_rows(short + [(739, 3)]), "green")
    # A gap among the years, the last year missing, an extra year not needed, nine years.
    case("year_gap_red", synthetic_rows([(749 - i, 10_000) for i in range(9)] + [(739, 10_000)]), "year_gap")
    case("last_year_missing_red", synthetic_rows([(748 - i, 10_000) for i in range(10)]), "not_the_last_year")
    case("extra_year_red", synthetic_rows(ten + [(739, 5)]), "year_not_needed")
    case("nine_years_red", synthetic_rows([(749 - i, 20_000) for i in range(9)]), "fewer_than_ten_years")
    # The tool's selection on rows of 40 000 a year from year 0 to 749 takes the last ten.
    many = synthetic_rows([(year, 40_000) for year in range(750)])
    assert [row["year"] for row in select_rows(many, last)] == list(range(749, 739, -1))
    # With 9 000 a year it takes twelve (108 000), stopping as soon as the sum reaches 100 000.
    few = synthetic_rows([(year, 9_000) for year in range(750)])
    chosen = select_rows(few, last)
    assert [row["year"] for row in chosen] == list(range(749, 737, -1)) and check_record(chosen, last)["green"]
    cases.append({"name": "select_rows_40000", "years": [row["year"] for row in select_rows(many, last)]})
    cases.append({"name": "select_rows_9000", "years": [row["year"] for row in chosen]})
    return cases


# ---------------------------------------------------------------------------------------
# Fixture (a): the template of map 4 with explicit founders

A_TAG = "test:census"
A_YEARS = 20
# Two nutrition shocks (note section 3.7 (a)): one before the start (the childhood of the
# child founders aged 4 and 7) and a total famine in year 6 (children born in the run and the
# child founders in their puberty window).
A_NUTRITION = [(-3, 0.5), (6, 0.0)]
# The seed, chosen by `--scan-a 1 2500` (the first seed with at least three founders whose
# curve is held; here two low founders below −6 and one high child above 6), then pinned.
A_SEED = 0x294
A_SNAPSHOTS = (0, 7, 19)


def map_loci(genome_map: dict) -> list:
    return list(genome_map["loci"])


def copies_count(locus: dict, sex: bytes) -> int:
    chromosome = locus["chromosome"]
    if chromosome == "autosome":
        return 2
    if chromosome == "x":
        return 2 if sex == FEMALE else 1
    if chromosome == "y":
        return 1 if sex == MALE else 0
    assert chromosome == "mt"
    return 1


def pattern_bit(index: int, locus_index: int, copy: int) -> int:
    """The allele of the pattern "mixed" for founder `index`, the locus at `locus_index` of
    the map and the copy (0 maternal, 1 paternal): bit 13 of `(index · 131 + locus_index · 31
    + copy · 17 + 7) · 2 654 435 761 mod 2^32`."""
    return (((index * 131 + locus_index * 31 + copy * 17 + 7) * 2_654_435_761) % 2**32 >> 13) & 1


def founder_alleles(genome_map: dict, sex: bytes, pattern: str, index: int) -> list:
    """Every locus of the map in map order with its copies (note: never `alleles: []`):
    "low" the reference allele everywhere; "high" allele 1 at every additive locus except the
    loci of `core:disease_liability` ([1, 0], dose 1 = `d_ref`), MC1R and HERC2 [1, 1], the
    five disease loci the reference allele; "mixed" `pattern_bit` at every additive locus and at
    MC1R and HERC2, the disease loci the reference allele."""
    out = []
    for locus_index, locus in enumerate(map_loci(genome_map)):
        n = copies_count(locus, sex)
        if n == 0:
            continue
        action = locus["action"]
        additive = isinstance(action, dict)
        liability = additive and any(load["trait"] == "core:disease_liability" for load in list(action.values())[0]["loads"])
        pigment = locus["id"] in ("core:mc1r", "core:herc2")
        if pattern == "low" or not (additive or pigment):
            copies = [0] * n
        elif pattern == "high":
            copies = [1, 0] if liability else [1] * n
        else:
            assert pattern == "mixed"
            copies = [pattern_bit(index, locus_index, copy) for copy in range(n)]
        out.append((locus["id"], copies))
    return out


def a_founder_plan() -> tuple:
    """The 27 founders of (a) as (sex, age in whole years, extra days, pattern) and the nine
    couples: per pattern low, high, mixed three couples (women 20 to 22, men 22 to 24) and
    three child founders (ages 1, 4, 7; girl, boy, girl). Born `−(age · YEAR + days · 86 400)`.
    Founders in `Start` by couple (woman, man), then the children."""
    plan, couples = [], []
    patterns = ("low", "high", "mixed")
    for p_index, pattern in enumerate(patterns):
        for k in range(3):
            n = p_index * 3 + k
            woman, man = len(plan), len(plan) + 1
            plan.append((FEMALE, 20 + k, 11 + 17 * n, pattern))
            plan.append((MALE, 22 + k, 5 + 23 * n, pattern))
            couples.append((woman, man))
    for p_index, pattern in enumerate(patterns):
        for k, (sex, age) in enumerate(((FEMALE, 1), (MALE, 4), (FEMALE, 7))):
            plan.append((sex, age, 3 + 29 * (p_index * 3 + k), pattern))
    return plan, couples


def a_founders(genome_map: dict) -> tuple:
    plan, couples = a_founder_plan()
    founders = []
    for index, (sex, age, days, pattern) in enumerate(plan):
        born = -(age * YEAR + days * 86_400)
        founders.append((born, sex, founder_alleles(genome_map, sex, pattern, index)))
    return founders, couples, plan


def a_data() -> tuple:
    documents = data_reference.documents(TEMPLATE)
    return wr.Data(*documents), documents


def held_founders(data: wr.Data, documents: tuple, seed: int, founders: list) -> list:
    """The founders whose curve is held (`|z_h|` or `|z_p|` above 6) with the world seed."""
    body = BodyCensusData(data, documents)
    out = []
    for index, (born, sex, genes) in enumerate(founders):
        genome = wr.founder_genome(sex, genes, data.loci)
        key = founder_key(A_TAG, index)
        z_h = wr.phenotype(seed, body.traits["core:growth"], genome, key, None, 0)
        z_p = wr.phenotype(seed, body.traits["core:puberty"], genome, key, None, 0)
        if abs(z_h) > 6.0 or abs(z_p) > 6.0:
            out.append((index, z_h, z_p))
    return out


def scan_a(first: int, last: int) -> int:
    data, documents = a_data()
    founders, _, _ = a_founders(documents[2])
    for seed in range(first, last + 1):
        held = held_founders(data, documents, seed, founders)
        print(f"scan_a seed {seed:#x} held {len(held)} {[(i, round(z_h, 3), round(z_p, 3)) for i, z_h, z_p in held]}")
    return 0


# ---------------------------------------------------------------------------------------
# Running a fixture


def scenario_start(world: wr.World, founders: list, couples, plan=None) -> dict:
    """The `Start` of a fixture in the form of the scenario files (format 5 as
    tools/arrival_reference.py writes it): the CLI fills `pool_hash` and the households."""
    start = {
        "world_seed": hexed(world.seed),
        "founder_tag": world.tag,
        "world_params": {"hereditary_diseases": world.switch, "nutrition": [{"year": y, "index": n} for y, n in world.nutrition]},
        "founders": [
            {"born": born, "sex": "female" if sex == FEMALE else "male", "alleles": [{"locus": locus, "copies": copies} for locus, copies in genes]}
            for born, sex, genes in founders
        ],
        "couples": [{"woman": woman, "man": man} for woman, man in couples],
    }
    if world.capacity is not None:
        start["capacity"] = world.capacity
    if world.pool_start is not None:
        start["pool"] = {"n_start": world.pool_start["n_start"], "immigration": world.pool_start["immigration"]}
    return start


def run_fixture(name: str, world: wr.World, body: BodyCensusData, years: int, snapshot_years, kind_of, scenario: dict, data_note: str, documents=None) -> tuple:
    evaluator = Evaluator(world, body)
    margins = Margins()
    rows, held, snapshots, effects = [], {}, [], []
    started = time.time()
    for year in range(years):
        world.advance(year_end(year))
        row, curve_held = census_row(evaluator, year, margins)
        rows.append(row)
        if curve_held:
            held[str(year)] = curve_held
        if year in snapshot_years and body.accepted:
            snapshots.append(snapshot(evaluator, year, kind_of))
            if world.pool_start is not None:
                effects += g6_effects(evaluator, year)
    elapsed = time.time() - started
    record = {
        "data": data_note,
        "data_hash": hexed(world.data.hash),
        "body_data": "accepted" if body.accepted and not body.reference_alleles else ("accepted with MC1R and HERC2 read as 0 copies (not the note's rule)" if body.accepted else f"rejected {body.reason}"),
        "scenario": {"format_version": 5, "start": scenario, "until": year_end(years - 1)},
        "start_input": world.inputs[0].hex(),
        "start_input_hash": hexed(hash64(world.inputs[0])),
        "year_hashes": [hexed(value) for value in world.year_hashes],
        "log_hash": hexed(world.log_hash()),
        "rows": rows,
        "curve_clamped_sims": held,
        "margins": margins.json(),
        "snapshots": snapshots,
    }
    if world.pool_start is not None:
        record["pool"] = {"pool_hash": hexed(world.data.pool.hash), "pool_seed": hexed(world.data.pool.seed), "frozen_at": world.data.pool.frozen_at}
        record["g6_effects"] = effects
    return record, evaluator, elapsed


def g6_effects(evaluator: Evaluator, year: int) -> list:
    """At the year end of `year` (the world advanced to it) the living arrivals whose
    parameters change if the whole list were read (G6 off): (year, sim, arrival second, born,
    the dropped years, height with G6 and without, also as bits)."""
    world = evaluator.world
    out = []
    t = year_end(year)
    if True:
        for sim in sorted(world.sims):
            if not world.is_arrival(sim):
                continue
            with_g6 = evaluator.evaluate(sim, t)
            without = evaluator.evaluate(sim, t, g6=False)
            if params_bytes(with_g6["params"]) != params_bytes(without["params"]):
                dropped = [y for y, _ in world.nutrition if (y, _) not in with_g6["nutrition"]]
                out.append(
                    {
                        "year": year,
                        "sim": sim,
                        "arrived": world.origins[sim]["arrived"],
                        "born": world.born_of(sim),
                        "dropped_years": dropped,
                        "height_cm": bits(with_g6["params"]["height_cm"][0]),
                        "height_cm_without_g6": bits(without["params"]["height_cm"][0]),
                        "height": [with_g6["params"]["height_cm"][0], without["params"]["height_cm"][0]],
                    }
                )
    return out


def fixture_a() -> tuple:
    data, documents = a_data()
    founders, couples, plan = a_founders(documents[2])
    world = wr.World(A_SEED, A_TAG, True, founders, couples, data, nutrition=A_NUTRITION)
    body = BodyCensusData(data, documents)
    assert body.accepted, body.reason
    kind_of = lambda sim: arrival_reference.env_kind(world, sim)
    record, evaluator, elapsed = run_fixture(
        "a", world, body, A_YEARS, A_SNAPSHOTS, kind_of, scenario_start(world, founders, couples),
        "tests/golden/map4_template with data/calibration/demography.json",
    )
    record["founder_plan"] = [
        {"index": i, "sex": "female" if sex == FEMALE else "male", "age_years": age, "extra_days": days, "pattern": pattern, "lineage_key": hexed(founder_key(A_TAG, i))}
        for i, (sex, age, days, pattern) in enumerate(plan)
    ]
    record["held_founders_at_start"] = [[i, bits(z_h), bits(z_p)] for i, z_h, z_p in held_founders(data, documents, A_SEED, founders)]
    return record, evaluator, elapsed


B_POOL, B_SEED, B_N_START = "pool_c", 0xFF, 50
B_YEARS = 14
B_NUTRITION = [(-2, 0.5), (3, 0.0), (8, 0.7)]
B_SNAPSHOTS = (0, 9, 13)
B2_POOL, B2_SEED, B2_N_START = "pool_b", 0xB2, 50
B2_YEARS = 20
B2_NUTRITION = [(-1, 0.5), (5, 0.7)]
B2_SNAPSHOTS = (0, 9, 19)


def pool_fixture(pool_name: str, seed: int, n_start: int, immigration: bool, years: int, nutrition, snapshot_years) -> tuple:
    documents = arrival_reference.template_documents()
    data = arrival_reference.town_data(arrival_reference.golden_pool(pool_name), documents)
    world = wr.World(seed, arrival_reference.TAG, True, [], (), data, pool=wr.pool_start(data, seed, n_start, immigration), nutrition=nutrition)
    body = BodyCensusData(data, documents)
    assert body.accepted, body.reason
    kind_of = lambda sim: arrival_reference.env_kind(world, sim)
    record, evaluator, elapsed = run_fixture(
        pool_name, world, body, years, snapshot_years, kind_of, scenario_start(world, [], ()),
        f"tests/golden/map4_template with data/calibration/demography.json and the pool tests/golden/pool/{pool_name}.bin",
    )
    record["arrivals"] = [
        {"t": rec["t"] if "t" in rec else None, "year": rec.get("year"), "draw": rec.get("draw")} for rec in world.immigrations
    ]
    return record, evaluator, elapsed


def fixture_c() -> tuple:
    template = prerun_reference.template_documents()
    prerun = prerun_reference.prerun_documents(template)
    data = prerun_reference.prerun_data(prerun)
    table = prerun_reference.weight_table(data)
    count = prerun_reference.counts(table, 40)
    loci = [locus["id"] for locus in data.measured_loci]
    founder_list = prerun_reference.founders(count, prerun_reference.MEASURE_SEED, prerun_reference.MEASURE_TAG, loci)
    founders = [founder for founder, _ in founder_list]
    out = {}
    for name, reference_alleles in (("c_prerun_small", False), ("c_prerun_small_reference_alleles", True)):
        world = wr.World(prerun_reference.MEASURE_SEED, prerun_reference.MEASURE_TAG, True, founders, (), data, capacity=40)
        body = BodyCensusData(data, prerun, reference_alleles=reference_alleles)
        kind_of = lambda sim, world=world: arrival_reference.env_kind(world, sim)
        record, evaluator, elapsed = run_fixture(
            name, world, body, 30, (), kind_of, scenario_start(world, founders, ()),
            "the pre-run data of tools/prerun_reference.py (the template without the five disease loci, MC1R, HERC2 and the Mendelian diseases)",
        )
        out[name] = (record, evaluator, elapsed)
    return out


# ---------------------------------------------------------------------------------------
# Output


def text_rows(name: str, record: dict) -> list:
    lines = [
        f"census_{name}_config data_hash {record['data_hash']} body_data {record['body_data']} start_input_hash {record['start_input_hash']}"
        f" log_hash {record['log_hash']} until {record['scenario']['until']} nutrition {record['scenario']['start']['world_params']['nutrition']}"
    ]
    for row in record["rows"]:
        counts = " ".join(f"{key}={row['clamped'][i]}" for i, key in enumerate(CLAMPED_KEYS))
        lines.append(f"census_{name}_row year {row['year']} living {row['living']} lived_secs {row['lived_secs']} body_evaluated {row['body_evaluated']} {counts} curve_clamped {row['curve_clamped']}")
    totals = [sum(row["clamped"][i] for row in record["rows"]) for i in range(len(CLAMPED_KEYS))]
    lines.append(
        f"census_{name}_totals evaluations {sum(row['body_evaluated'] for row in record['rows'])} curve_clamped {sum(row['curve_clamped'] for row in record['rows'])} "
        + " ".join(f"{key}={totals[i]}" for i, key in enumerate(CLAMPED_KEYS))
    )
    if record["margins"] is not None:
        lines.append(f"census_{name}_margins {json.dumps(record['margins'], separators=(',', ':'))}")
    for year, held in record["curve_clamped_sims"].items():
        lines.append(f"census_{name}_curve_held year {year} {held}")
    for snap in record["snapshots"]:
        lines.append(f"census_{name}_snapshot year {snap['year']} t {snap['t']} living {snap['living']} hash {snap['hash']}")
        for person in snap["persons"]:
            lines.append(
                f"census_{name}_person year {snap['year']} sim {person['sim']} kind {person['kind']} key {person['lineage_key']} hash {person['hash']}"
                f" clamped {','.join(person['clamped']) or '-'} curve {str(person['curve_clamped']).lower()}"
            )
    for effect in record.get("g6_effects", []):
        lines.append(
            f"census_{name}_g6 year {effect['year']} sim {effect['sim']} arrived {effect['arrived']} born {effect['born']} dropped {effect['dropped_years']}"
            f" height_cm {effect['height'][0]!r} without_g6 {effect['height'][1]!r}"
        )
    return lines


# The scenario "records" of tools/arrival_reference.py (pool_d, seed 6, `n_start` 30,
# immigration on, the template of map 4): the phenotypes of the living at its last year end.
# Its town child of one pool and one town person gets the kind `town_of_pool_and_town`.
RECORDS_KINDS = {"town_of_a_pool_and_a_town_person": "town_of_pool_and_town"}


def records_phenotypes() -> dict:
    """`--records`: the world of "records" (`kin_world(RECORDS_SEED)`), its one input
    (ArrangePartnership of sims 26 and 31, stamped `18 · YEAR − 1`), run to `RECORDS_END`;
    per living sim (ascending SimId) its kind and the 35 phenotypes `z` at the age 0 in
    `BODY_TRAITS_MAP4` order (the seeds and keys of `World.env_of`), as f64 bits."""
    world = arrival_reference.kin_world(arrival_reference.RECORDS_SEED)()
    ((label, acts, woman, man),) = arrival_reference.RECORDS_INPUTS
    assert label == "arrange"
    world.advance(acts - 1)
    world.arrange(woman, man)
    end = arrival_reference.RECORDS_END
    world.advance(end)
    assert (end + 1) % YEAR == 0
    year = (end + 1) // YEAR - 1
    body = BodyCensusData(world.data, arrival_reference.template_documents())
    assert body.accepted, body.reason
    evaluator = Evaluator(world, body)
    persons, counts = [], {}
    for sim in sorted(world.sims):
        parts = evaluator.person_parts(sim)
        kind = arrival_reference.env_kind(world, sim)
        kind = RECORDS_KINDS.get(kind, kind)
        counts[kind] = counts.get(kind, 0) + 1
        if kind == "town_of_pool_and_town":
            # Both draws with the world seed at its parents' keys (one parent is no pool person).
            e_seed, c_seed, parents = parts["seeds"]
            assert e_seed == c_seed == world.seed and parents == world.parent_keys(sim)
        persons.append([sim, kind, [bits(parts["z_of"][trait]) for trait in br.BODY_TRAITS_MAP4]])
    mixed = [sim for sim, kind, _ in persons if kind == "town_of_pool_and_town"]
    assert [world.born_of(sim) for sim in mixed] == [arrival_reference.RECORDS_MARKS["mixed_child"]], mixed
    return {
        "scenario": "records of tools/arrival_reference.py: pool_d, world seed 0x6, tag test:arrival, n_start 30, immigration on, switch on, no nutrition year, template of map 4",
        "t": end,
        "year": year,
        "z_traits": br.BODY_TRAITS_MAP4,
        "data_hash": hexed(world.data.hash),
        "pool_hash": hexed(world.data.pool.hash),
        "start_input_hash": hexed(hash64(world.inputs[0])),
        "inputs": [
            {"input": "ArrangePartnership", "woman": woman, "man": man, "stamped": acts - 1, "acts": acts, "record_hex": world.inputs[1].hex(), "record_hash": hexed(hash64(world.inputs[1]))}
        ],
        "year_hash_last": hexed(world.year_hashes[-1]),
        "log_hash": hexed(world.log_hash()),
        "kind_counts": counts,
        "persons": persons,
    }


# ---------------------------------------------------------------------------------------
# The phenotypes golden and the pure height (fixtures a, b, b2 and "records")

PHENOTYPES_NOTE = (
    "each living person's 35 body phenotypes z at age 0 (weights of age 0) at the snapshots of census.json, as f64 bits, with its kind"
)


def a_world() -> tuple:
    data, documents = a_data()
    founders, couples, _ = a_founders(documents[2])
    return wr.World(A_SEED, A_TAG, True, founders, couples, data, nutrition=A_NUTRITION), documents


def pool_world(pool_name: str, seed: int, n_start: int, immigration: bool, nutrition) -> tuple:
    documents = arrival_reference.template_documents()
    data = arrival_reference.town_data(arrival_reference.golden_pool(pool_name), documents)
    world = wr.World(seed, arrival_reference.TAG, True, [], (), data, pool=wr.pool_start(data, seed, n_start, immigration), nutrition=nutrition)
    return world, documents


# The worlds of the fixtures as `fixture_a` and `pool_fixture` build them: (make, years,
# snapshot years).
FIXTURE_RUNS = {
    "a_template_extremes": (a_world, A_YEARS, A_SNAPSHOTS),
    "b_pool_c_immigration": (lambda: pool_world(B_POOL, B_SEED, B_N_START, True, B_NUTRITION), B_YEARS, B_SNAPSHOTS),
    "b2_pool_b_start": (lambda: pool_world(B2_POOL, B2_SEED, B2_N_START, False, B2_NUTRITION), B2_YEARS, B2_SNAPSHOTS),
}


def kind_name(world: wr.World, sim: int) -> str:
    """`arrival_reference.env_kind` with the town child of one pool and one town person named
    `town_of_pool_and_town`."""
    kind = arrival_reference.env_kind(world, sim)
    return RECORDS_KINDS.get(kind, kind)


def walk(name: str, at_snapshot, at_end) -> None:
    """Runs fixture `name` year by year, calling `at_snapshot(evaluator, year)` at each
    snapshot year end and `at_end(evaluator, end)` after the last year end."""
    make, years, snapshot_years = FIXTURE_RUNS[name]
    world, documents = make()
    body = BodyCensusData(world.data, documents)
    assert body.accepted, body.reason
    evaluator = Evaluator(world, body)
    for year in range(years):
        world.advance(year_end(year))
        if year in snapshot_years:
            at_snapshot(evaluator, year)
    at_end(evaluator, year_end(years - 1))


def records_world() -> tuple:
    world = arrival_reference.kin_world(arrival_reference.RECORDS_SEED)()
    ((_, acts, woman, man),) = arrival_reference.RECORDS_INPUTS
    world.advance(acts - 1)
    world.arrange(woman, man)
    world.advance(arrival_reference.RECORDS_END)
    return world, arrival_reference.template_documents()


def phenotypes_document() -> dict:
    """The golden `tests/golden/census/phenotypes.json`: per fixture a, b, b2 and snapshot the
    living by SimId with kind and the 35 `z` (as `census_details.json`)."""
    fixtures = {}
    for name in FIXTURE_RUNS:
        snapshots = []

        def at_snapshot(evaluator, year):
            world = evaluator.world
            persons = []
            for sim in sorted(world.sims):
                parts = evaluator.person_parts(sim)
                persons.append([sim, kind_name(world, sim), [bits(parts["z_of"][trait]) for trait in br.BODY_TRAITS_MAP4]])
            snapshots.append({"year": year, "t": year_end(year), "persons": persons})

        walk(name, at_snapshot, lambda evaluator, end: None)
        fixtures[name] = snapshots
    return {"written_by": "tools/census_reference.py (independent of crates/)", "z_traits": br.BODY_TRAITS_MAP4, "note": PHENOTYPES_NOTE, "fixtures": fixtures}


def heights(evaluator: Evaluator, sim: int, t: int) -> tuple:
    """(`height_before_age_loss`, `pure_height`) of a sim at `t`. `height_before_age_loss =
    κ_i · H(x; θ_i) − v_i(x)` with `x = as_years(min(t − born, 25 · YEAR))` and the nutrition
    loss `v_i` over `[born, born + min(age, 25 years))` (G6 for an arrival), as
    `body_reference.raw_values` computes it before the loss of age. The pure height (ADR-0015
    section 4, genetics specification 10.2, criterion P6) is `κ_i · H − v_i − Verlust(b)`:
    that minus the loss of age at `b = bio_age` (ADR-0014), never clamped; it is the raw
    `height_cm` before the clamp."""
    parts = evaluator.person_parts(sim)
    values = evaluator.body.values
    age = t - parts["born"]
    assert age >= 0, (sim, t)
    x_seconds = min(age, 25 * YEAR)
    x = as_years(x_seconds)
    curve = br.individual_curve(values, parts["sex"], parts["z"]["z_h"], parts["z"]["z_p"])
    height_before_age_loss = curve["kappa"] * br.mean_height(values, parts["sex"], x, curve["theta_i"])
    height_before_age_loss = height_before_age_loss - br.nutrition_loss(
        values, nutrition_of(evaluator.world, sim), parts["born"], x_seconds, curve["theta_i"]
    )
    b = br.bio_age(as_years(age), parts["delta"], values.bio_age_weight)
    pure_height = height_before_age_loss - br.old_age_loss(values, parts["sex"], b)
    return height_before_age_loss, pure_height


def pure_living(evaluator: Evaluator, t: int) -> list:
    world = evaluator.world
    persons = []
    for sim in sorted(world.sims):
        before_age_loss, pure = heights(evaluator, sim, t)
        # The mapping's raw height (before the clamp) is the pure height, bit for bit.
        assert bits(evaluator.evaluate(sim, t)["raw"]["height_cm"]) == bits(pure), sim
        # Columns: sim, kind, height before the loss of age, pure height.
        persons.append([sim, kind_name(world, sim), bits(before_age_loss), bits(pure)])
    return persons


def pure_archived(evaluator: Evaluator) -> list:
    world = evaluator.world
    persons = []
    for sim in sorted(world.archive):
        left, cause = world.archive[sim][1], world.archive[sim][2]
        before_age_loss, pure = heights(evaluator, sim, left)
        # Columns: sim, kind, leaving second, cause, height before the loss of age, pure height.
        persons.append([sim, kind_name(world, sim), left, "emigration" if cause is None else world.cause_code(cause), bits(before_age_loss), bits(pure)])
    return persons


def pure_heights_document() -> dict:
    fixtures = {}
    for name in FIXTURE_RUNS:
        record = {"snapshots": []}

        def at_snapshot(evaluator, year):
            record["snapshots"].append({"year": year, "t": year_end(year), "persons": pure_living(evaluator, year_end(year))})

        def at_end(evaluator, end):
            record["archived"] = {"run_end": end, "persons": pure_archived(evaluator)}

        walk(name, at_snapshot, at_end)
        fixtures[name] = record
    world, documents = records_world()
    body = BodyCensusData(world.data, documents)
    evaluator = Evaluator(world, body)
    end = arrival_reference.RECORDS_END
    fixtures["records"] = {
        "snapshots": [{"year": (end + 1) // YEAR - 1, "t": end, "persons": pure_living(evaluator, end)}],
        "archived": {"run_end": end, "persons": pure_archived(evaluator)},
    }
    return {
        "written_by": "tools/census_reference.py (independent of crates/)",
        "note": (
            "pure height (ADR-0015 section 4, genetics specification 10.2, criterion P6) = kappa_i * H(x; theta_i) - v_i - Verlust(b), "
            "never clamped (= raw height_cm before the clamp); height_before_age_loss = kappa_i * H(x; theta_i) - v_i; "
            "x = as_years(min(age, 25 years)), v_i the nutrition loss over [born, born + min(age, 25 years)) with G6 for an arrival, "
            "Verlust(b) the loss of age at b = bio_age (ADR-0014); "
            "snapshot persons [sim, kind, height_before_age_loss bits, pure height bits]; "
            "archived persons [sim, kind, leaving second, cause, the same two values at the leaving second]"
        ),
        "fixtures": fixtures,
    }


def compact(record: dict) -> dict:
    """The golden candidate of a fixture: everything but the scenario's founders (in its
    own file), the bytes of `Start` (its hash only) and the per-person details (in
    census_details.json); per snapshot person (SimId, lineage key, hash)."""
    out = {key: value for key, value in record.items() if key not in ("scenario", "start_input", "snapshots", "founder_plan")}
    out["until"] = record["scenario"]["until"]
    out["snapshots"] = [
        {"year": snap["year"], "t": snap["t"], "living": snap["living"], "hash": snap["hash"], "persons": [[p["sim"], p["lineage_key"], p["hash"]] for p in snap["persons"]]}
        for snap in record["snapshots"]
    ]
    out["g6_effects"] = [{k: v for k, v in effect.items() if k != "height"} for effect in record.get("g6_effects", [])] if "g6_effects" in record else None
    if out["g6_effects"] is None:
        del out["g6_effects"]
    return out


def main(argv) -> int:
    if "--scan-a" in argv:
        i = argv.index("--scan-a")
        return scan_a(int(argv[i + 1], 0), int(argv[i + 2], 0))
    if "--phenotypes" in argv:
        target = Path(argv[argv.index("--phenotypes") + 1])
        target.write_text(json.dumps(phenotypes_document(), separators=(",", ":")))
        print(f"wrote {target}")
        return 0
    if "--pure-heights" in argv:
        out = Path(argv[argv.index("--out") + 1]) if "--out" in argv else DEFAULT_OUT
        document = pure_heights_document()
        (out / "pure_heights.json").write_text(json.dumps(document, indent=1) + "\n")
        for name, record in document["fixtures"].items():
            print(f"pure_heights {name} snapshots {[(snap['year'], len(snap['persons'])) for snap in record['snapshots']]} archived {len(record['archived']['persons'])}")
        return 0
    if "--records" in argv:
        out = Path(argv[argv.index("--out") + 1]) if "--out" in argv else DEFAULT_OUT
        document = records_phenotypes()
        (out / "records_phenotypes.json").write_text(json.dumps(document, indent=1) + "\n")
        print(f"records t {document['t']} year {document['year']} living {len(document['persons'])} kinds {document['kind_counts']} start_input_hash {document['start_input_hash']}")
        return 0
    out = Path(argv[argv.index("--out") + 1]) if "--out" in argv else DEFAULT_OUT
    (out / "scenarios").mkdir(parents=True, exist_ok=True)
    fixtures, timings = {}, {}
    record, _, elapsed = fixture_a()
    fixtures["a_template_extremes"], timings["a_template_extremes"] = record, elapsed
    record, _, elapsed = pool_fixture(B_POOL, B_SEED, B_N_START, True, B_YEARS, B_NUTRITION, B_SNAPSHOTS)
    fixtures["b_pool_c_immigration"], timings["b_pool_c_immigration"] = record, elapsed
    record, _, elapsed = pool_fixture(B2_POOL, B2_SEED, B2_N_START, False, B2_YEARS, B2_NUTRITION, B2_SNAPSHOTS)
    fixtures["b2_pool_b_start"], timings["b2_pool_b_start"] = record, elapsed
    for name, (record, _, elapsed) in fixture_c().items():
        fixtures[name], timings[name] = record, elapsed
    head = {
        "format_version": 1,
        "written_by": "tools/census_reference.py (independent of crates/)",
        "core_version_modelled": wr.CORE_VERSION,
        "projection_version_modelled": wr.PROJECTION_VERSION,
        "evaluation_second": "year_end(k) = (k + 1) * YEAR - 1, YEAR = 31449600",
        "clamped_keys": CLAMPED_KEYS,
        "param_keys": PARAM_KEYS,
        "z_traits": br.BODY_TRAITS_MAP4,
        "hashes": "xxh3-64 seed 0; person hash = xxh3_64(the 44 params as f64 LE in param_keys order); snapshot hash = xxh3_64(concat over the living by ascending SimId of (u64 LE SimId ++ those 352 bytes)); start_input_hash = xxh3_64(the input record of Start: i64 stamp -1, sequence 1, the postcard of Start)",
    }
    for name, record in fixtures.items():
        (out / "scenarios" / f"{name}.json").write_text(json.dumps(record["scenario"], separators=(",", ":")) + "\n")
    golden = dict(head, scenarios="scenarios/<fixture>.json (format 5 as tools/arrival_reference.py writes it; the CLI fills pool_hash and households)")
    golden["fixtures"] = {name: compact(record) for name, record in fixtures.items()}
    golden["selection_cases"] = selection_cases()
    (out / "census.json").write_text(json.dumps(golden, indent=1) + "\n")
    details = dict(head)
    details["fixtures"] = {
        name: {
            "start_input": record["start_input"],
            "founder_plan": record.get("founder_plan"),
            "snapshots": record["snapshots"],
            "g6_effects": record.get("g6_effects"),
        }
        for name, record in fixtures.items()
    }
    (out / "census_details.json").write_text(json.dumps(details, separators=(",", ":")) + "\n")
    lines = []
    for name, record in fixtures.items():
        lines += text_rows(name, record)
    for case in golden["selection_cases"]:
        lines.append(f"census_selection {json.dumps(case, separators=(',', ':'))}")
    (out / "census.txt").write_text("\n".join(lines) + "\n")
    for name, seconds in timings.items():
        print(f"census_timing {name} {seconds:.1f} s (not a pin)", file=sys.stderr)
    print(f"wrote {out}/census.json, census_details.json, census.txt, scenarios/")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

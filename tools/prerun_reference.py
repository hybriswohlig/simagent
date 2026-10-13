#!/usr/bin/env python3
"""The pre-run tool (`cargo xtask prerun`, step 11; design note of the pre-run tool, ADR-0020
section 3 "Gründer, genau", "Messung, genau", "Iteration"), written from the note, the
ADRs and the genetics specification, independent of the Rust code: the data of a pre-run,
its founders, a run and its measurement, the iteration and its stop, the frozen map.

1. The data of a pre-run (note section 1): the template of map 4
   (tests/golden/map4_template, `cargo xtask map4-template`) with the repository's life
   course data (data/calibration/demography.json, which a directory without its own takes),
   the map without the five disease loci and without `core:mc1r` and `core:herc2` (their
   slots are free, carry the reference allele and never mutate: the meiosis mutates only
   the loci of the map), the calibration without the five Mendelian diseases (carrier
   `locus`; the threshold disease stays with its liability), everything else as in the
   template, the texts too; per run the `d_ref` of every additive locus and the `sigma_ref`
   of every trait of the iteration. Checked with the rules of `Data::new` (as
   tools/map4_reference.py checks the template, without its counts) and hashed
   (tools/data_reference.py).
2. The founders (note section 2): per sex `s` and whole age `a` from 18 to 60 the weight
   `w = p_s · I_s(a)`, `p_female = 20.0 / 41.0`, `p_male = 21.0 / 41.0` (f64 divisions),
   `I_s(a)` Simpson's rule with 64 subintervals over `[a, a + 1]`: `h = 1/64`, nodes `x_i = a
   + i / 64` (exact), coefficients 1, 4, 2, …, 4, 1, the sum `Σ c_i · S̄(x_i)` folded from
   +0.0 in ascending `i`, then `(sum · h) / 3` (`sum · h` is exact, so this is `sum / 192`
   rounded once). `S̄_s(x)` the trapezoidal rule over `g_k = −8 + k / 64`, `k` = 0 to 1024
   (exact): `Σ t_k · φ(g_k) · exp(−H_s(x; g_k))` folded from +0.0 in ascending `k`, `t_k` =
   1/64, 1/128 at both ends (powers of two: where the factor stands changes no bit),
   `φ(g) = exp(−(g · g) / 2) / sqrt(2 · π)` (`g · g` and the halving exact, `π` the f64,
   sqrt correctly rounded), `H_s(x; g) = (c_s · x) + (((a_s · m(g)) / b_s) · (exp(b_s · x) −
   1))` with the data's `c_s` (`core:accident_<sex>`), `a_s`, `b_s`
   (`core:senescence_a_<sex>`, `_b_<sex>`) and `m(g) = exp(−(σ_g · ĝ))`, `ĝ` = `g` held on
   [−6, 6] (`world_reference.longevity_input`, the core's `mortality::multiplier(Some(g),
   σ_g)`), `σ_g` = `core:longevity_sigma_g`. exp correctly rounded (`exp_cr`: a 120-bit exp
   rounded once to the nearest f64, and `kat_reference.exp_reference` wherever that could
   be a double rounding). The total `Σ w` over the 86 groups (female 18 to 60, then male 18
   to 60) from +0.0; `q = (N · w) / Σ w` (`N` as f64), `c = floor(q)`, the remainder `q − c`
   (exact); the remaining `N − Σ c` founders one each to the groups with the largest
   remainders (ties: women before men, then the younger age).
   In `Start` women first, then men, each sex by ascending age, each group by `j` from 0 to
   `c − 1`; the `j`-th of `c` is born `a · YEAR + ((2 j + 1) · YEAR) div (2 c)` seconds
   before the start (integer division). Its lineage key `LineageKey::founder(founder_tag,
   index)`, `index` its place in `Start`. Its alleles: at every additive locus of the
   pre-run map (map order), copies 0 (maternal) and 1 (paternal), `u = unit` of the draw
   (the run's world seed, `PRERUN_ALLELE` = 25, the founder's lineage key, `str_key` of the
   locus, the copy); the allele 1 iff `u < 0.5`; the founder lists the loci where a copy is
   1, in map order, each with both copies `[m, p]`.
3. A run (note section 3): `World(seed, founder_tag, switch on, the founders, no couples,
   the pre-run data, capacity N)` (tools/world_reference.py, which knows the traits of
   longevity and of the puberty timing since this slice), run to `Y · YEAR − 1` (the last
   year end); the census per year (the living after each year end, with the births, deaths
   and emigrants of the year); the measurement (`World.measure`, ADR-0020 "Messung, genau"):
   the living, per trait mean and `σ` of `A`, per additive locus the mean dose; per block of
   30 birth years (`born div (30 · YEAR)`, the persons born in the run, living, dead and
   emigrated, by their genome) the distinct mothers and fathers, `N_e = (4 · N_m · N_f) /
   (N_m + N_f)` (the integers exact, one division) and the variance of `A` of `core:growth`
   and `core:cognition` (divisor `n`, both sums from +0.0 in ascending lineage key, the
   square one product).
4. The iteration (note section 4): the map of run `k + 1` is that of run `k` with every
   additive locus's `d_ref` its measured mean dose and every trait's `sigma_ref` its
   measured `σ`; `r = |σ_out − σ_in| / σ_in` per trait with `σ_in` the run's `sigma_ref`;
   stop after the first run with `r < 0.02` for `core:growth`, `core:cognition`,
   `core:longevity`, `core:puberty` and `core:disease_liability` (run 1 may stop); at most
   five runs, after run 5 with one of the five at `r ≥ 0.02` no freeze; the report lists
   every other trait with `r ≥ 0.05`. The freeze: the template's map with the measured
   `d_ref` and `sigma_ref` (the disease loci, MC1R and HERC2 as in the template,
   `map_version` 4) and the template's calibration, written as the data files
   (`genetics/genome_map.json`, `calibration/genetics.json`, JSON as serde_json's pretty
   printer writes it, tools/map4_reference.py).

The note's tests (section 5): the weights and counts for N = 4000, 40 000 and 40, the birth
seconds and alleles of the first and last founder of some groups, the data hash; the small
run (N = 40, 30 years, one iteration step) with its census, measurement, the map of run 2
and `r`; the stop rule on given measurements. A run of 4000 or 40 000 founders over 750
years is out of reach here (the Rust tool runs it).

Usage: .venv/bin/python tools/prerun_reference.py [--out DIR]   (print the pins; with --out
       also write the frozen map of the small run's measurement to DIR, as if it stopped)
       .venv/bin/python tools/prerun_reference.py --trial [--out DIR]   (the data and the
       optional larger trial: 400 founders, 30 years, labels prerun_trial_...)
"""

import copy
import math
from fractions import Fraction
import sys
import time
from pathlib import Path

from mpmath import libmp

import data_reference
import map4_reference
import world_reference
from kat_reference import exp_reference
from postcard_reference import FEMALE, MALE
from rng_reference import chacha_key, founder_key, str_key, unit, words
from world_reference import PRERUN_ALLELE, YEAR, as_years, genetic_value, longevity_input

ROOT = Path(__file__).resolve().parent.parent
TEMPLATE = ROOT / "tests" / "golden" / "map4_template"

# The two runs (ADR-0020 section 3; note section 2).
MEASURE_SEED, POOL_SEED = 0x11B019A1, 0x11B019A2
MEASURE_TAG, POOL_TAG = "core:prerun_measure", "core:prerun_pool"
MEASURE_FOUNDERS, POOL_FOUNDERS, YEARS = 4000, 40_000, 750
# The iteration (ADR-0020 section 3, note section 4).
STOP_TRAITS = ("core:growth", "core:cognition", "core:longevity", "core:puberty", "core:disease_liability")
STOP_BOUND, REPORT_BOUND, MAX_RUNS = 0.02, 0.05, 5
# The founders (note section 2).
AGES = range(18, 61)
SEXES = ((FEMALE, "female", 20.0 / 41.0), (MALE, "male", 21.0 / 41.0))
GRID = 1024
SIMPSON = 64
BLOCK_YEARS = 30
# The loci a pre-run drops besides the five disease loci (note section 1).
PIGMENT_LOCI = ("core:mc1r", "core:herc2")


def bits(value: float) -> str:
    return data_reference.f64_bits(value)


# ---------------------------------------------------------------------------------------
# 1. The data of a pre-run


def template_documents():
    """(demography, calibration, genome map) of the template with the repository's life
    course data, texts and all."""
    return data_reference.documents(TEMPLATE)


def mendelian(calibration: dict) -> list:
    """The diseases of the calibration whose carrier is a locus (the Mendelian ones)."""
    return [disease for disease in calibration["diseases"] if "locus" in disease["carrier"]]


def dropped_loci(calibration: dict) -> list:
    """The loci a pre-run removes: the loci of the Mendelian diseases and MC1R, HERC2."""
    return [disease["carrier"]["locus"]["locus"] for disease in mendelian(calibration)] + list(PIGMENT_LOCI)


def prerun_documents(documents) -> tuple:
    """The pre-run documents of a map of the template's form and its calibration (note
    section 1): copies without the dropped loci and without the Mendelian diseases."""
    demography, calibration, genome_map = (copy.deepcopy(doc) for doc in documents)
    dropped = set(dropped_loci(calibration))
    assert len(dropped) == 7, dropped
    genome_map["loci"] = [locus for locus in genome_map["loci"] if locus["id"] not in dropped]
    calibration["diseases"] = [disease for disease in calibration["diseases"] if "locus" not in disease["carrier"]]
    assert all(isinstance(locus["action"], dict) and "additive" in locus["action"] for locus in genome_map["loci"])
    return demography, calibration, genome_map


def with_reference(genome_map: dict, doses: dict, sigmas: dict) -> dict:
    """The map with `d_ref` of every additive locus in `doses` and `sigma_ref` of every
    trait in `sigmas` set (the iteration, note section 4); every other value as it is."""
    out = copy.deepcopy(genome_map)
    for locus in out["loci"]:
        if locus["id"] in doses:
            locus["action"]["additive"]["d_ref"] = doses[locus["id"]]
    for trait in out["traits"]:
        if trait["id"] in sigmas:
            trait["sigma_ref"] = sigmas[trait["id"]]
    return out


def check_prerun(calibration: dict, genome_map: dict) -> list:
    """The rules of `Data::new` for the pre-run documents: those
    tools/map4_reference.py checks on the template (`check_map`, `check_calibration`)
    without the counts of the template, with `d_ref` anywhere in [0, 2] for the bound of
    a trait (data.trait_bounds); then the documents' own checks (`documents_hash`)."""
    loci, traits = genome_map["loci"], genome_map["traits"]
    for locus in loci:
        assert locus["chromosome"] == "autosome" and 0 <= locus["segment"] < 8 and 0 <= locus["slot"] < 24, locus["id"]
    keys = [map4_reference.map_order(locus) for locus in loci]
    assert keys == sorted(keys) and len(set(keys)) == len(keys), "data.locus_order"
    ids = [trait["id"] for trait in traits]
    assert ids == sorted(ids) and len(set(ids)) == len(ids), "data.trait_order"
    names = ids + [locus["id"] for locus in loci]
    assert len(set(names)) == len(names) and len({str_key(name) for name in names}) == len(names), "data.locus_key"
    by_id = {trait["id"]: trait for trait in traits}
    loaded = {trait: [] for trait in ids}
    for locus in loci:
        ((kind, weights),) = locus["action"].items()
        assert kind == "additive" and locus["mark"] == "small" and locus["mutation"] == "both", locus["id"]
        assert weights["effect"] > 0.0 and 0.0 <= weights["d_ref"] <= 2.0, "data.locus_weights"
        named = [load["trait"] for load in weights["loads"]]
        assert 1 <= len(named) <= 2 and len(set(named)) == len(named), "data.locus_weights"
        for load in weights["loads"]:
            assert load["trait"] in by_id and load["weight"] != 0.0, "data.locus_load"
            loaded[load["trait"]].append(locus)
        assert not [name for name in named if by_id[name]["exclusive"]] or len(named) == 1, "data.load_exclusive"
    for trait in traits:
        assert trait["sigma_ref"] > 0.0 and math.isfinite(trait["sigma_ref"]), "data.trait_sigma"
        for entry in trait["includes"]:
            other = by_id[entry["trait"]]
            assert entry["trait"] != trait["id"] and other["includes"] == [] and not other["exclusive"], "data.trait_include"
        assert loaded[trait["id"]] or trait["includes"], "data.trait_unused"

        def largest(trait_id: str) -> float:
            total = 0.0
            for locus in loaded[trait_id]:
                weights = locus["action"]["additive"]
                for load in weights["loads"]:
                    if load["trait"] == trait_id:
                        total += abs(load["weight"] * weights["effect"]) * max(weights["d_ref"], 2.0 - weights["d_ref"])
            return total

        bound = largest(trait["id"]) + sum(abs(entry["weight"]) * largest(entry["trait"]) for entry in trait["includes"])
        assert math.isfinite(bound / trait["sigma_ref"]) and bound / trait["sigma_ref"] <= 2.0**1022, "data.trait_bounds"
    rows = [row["trait"] for row in calibration["traits"]]
    assert rows == sorted(rows) and set(rows) <= set(ids), "data.calibration_unknown"
    for field in data_reference.NAMED_TRAITS:
        assert calibration[field] in ids, f"data.trait_named {field}"
    assert all("threshold" in disease["carrier"] for disease in calibration["diseases"])
    data_reference.check_liability(calibration, genome_map)
    return [
        f"prerun_data_check loci {len(loci)} (all additive, small, both, autosomal) traits {len(traits)}"
        f" diseases {[disease['code'] for disease in calibration['diseases']]}"
        " rules data.locus_position data.locus_order data.locus_key data.trait_order data.trait_sigma data.trait_include"
        " data.trait_unused data.locus_weights data.locus_load data.load_exclusive data.trait_bounds data.calibration_unknown"
        " data.trait_named data.disease (liability): met"
    ]


def prerun_data(documents) -> world_reference.Data:
    """The pre-run data the world runs on (the documents checked and hashed)."""
    demography, calibration, genome_map = documents
    check_prerun(calibration, genome_map)
    return world_reference.Data(demography, calibration, genome_map)


def frozen_documents(template, measurement: dict) -> tuple:
    """The freeze (note section 4): the template's map with `d_ref` = the measured mean
    dose of every additive locus and `sigma_ref` = the measured `σ` of every trait (the
    disease loci, MC1R and HERC2 as in the template, `map_version` 4), and the template's
    calibration."""
    _, calibration, genome_map = template
    doses = dict(measurement["loci"])
    sigmas = {trait: sigma for trait, _, sigma in measurement["traits"]}
    frozen = with_reference(genome_map, doses, sigmas)
    assert frozen["map_version"] == 4
    assert {locus["id"] for locus in frozen["loci"] if isinstance(locus["action"], dict)} == set(doses)
    assert {trait["id"] for trait in frozen["traits"]} == set(sigmas)
    return copy.deepcopy(calibration), frozen


def write_frozen(out: Path, calibration: dict, genome_map: dict) -> dict:
    """Writes the frozen map and the calibration in the layout of the data files; returns
    the texts by relative path."""
    files = {
        Path("genetics") / "genome_map.json": map4_reference.text(genome_map),
        Path("calibration") / "genetics.json": map4_reference.text(calibration),
    }
    for path, content in files.items():
        (out / path).parent.mkdir(parents=True, exist_ok=True)
        (out / path).write_text(content)
    return files


# ---------------------------------------------------------------------------------------
# 2. The founders


def exp_cr(x: float) -> float:
    """The correctly rounded exp: mpmath's exp at 120 bits rounded once to 53 bits (to
    nearest, ties to even); where the 120-bit value lies within 2^-100 (relative) of the
    middle between two f64 (a possible double rounding), `kat_reference.exp_reference` at 60
    digits decides. Checked against exp_reference on a sample at import."""
    if x == 0.0:
        return 1.0
    sign, man, exp, bc = libmp.mpf_exp(libmp.from_float(x), 120, libmp.round_nearest)
    assert sign == 0 and bc <= 120
    man <<= 120 - bc
    rest = man & ((1 << 67) - 1)
    if abs(rest - (1 << 66)) <= 1 << 20:
        return exp_reference(x)[1]
    return libmp.to_float(libmp.normalize(0, man, exp - (120 - bc), 120, 53, libmp.round_nearest))


for _x in [k / 37.0 - 9.0 for k in range(500)] + [-(k / 97.0) ** 1.5 for k in range(1, 400)]:
    assert exp_cr(_x) == exp_reference(_x)[1], _x


def death_values(data: world_reference.Data, sex: bytes) -> tuple:
    """(c, a, b) of the sex: the Makeham term and the Gompertz `a`, `b` of the data."""
    a, b = data.senescence[sex]
    return data.accident[sex], a, b


def weight_table(data: world_reference.Data, exp=exp_cr) -> dict:
    """The weights of note section 2 with their parts: {"sqrt_2pi", "grid" [(g, ĝ, m, φ,
    t)], "weights" {(sex, age): (w, I, sum)}, "total"}. `exp` is the exp used (the
    correctly rounded one; a probe passes a port of libm)."""
    sqrt_2pi = math.sqrt(2.0 * math.pi)
    grid = []
    for k in range(GRID + 1):
        g = -8.0 + k / 64.0
        held = longevity_input(g)
        m = exp(-(data.sigma_g * held))
        phi = exp(-(g * g) / 2.0) / sqrt_2pi
        t = 1.0 / 128.0 if k in (0, GRID) else 1.0 / 64.0
        grid.append((g, held, m, phi, t))
    weights = {}
    for sex, _, p in SEXES:
        c, a, b = death_values(data, sex)
        survival = {}

        def s_bar(x: float) -> float:
            if x not in survival:
                growth = exp(b * x) - 1.0
                acc = 0.0
                for _, _, m, phi, t in grid:
                    hazard = (c * x) + (((a * m) / b) * growth)
                    acc = acc + (t * phi) * exp(-hazard)
                survival[x] = acc
            return survival[x]

        for age in AGES:
            acc = 0.0
            for i in range(SIMPSON + 1):
                x = age + i / 64.0
                coefficient = 1.0 if i in (0, SIMPSON) else (4.0 if i % 2 == 1 else 2.0)
                acc = acc + coefficient * s_bar(x)
            integral = (acc * (1.0 / 64.0)) / 3.0
            weights[(sex, age)] = (p * integral, integral, acc)
    total = 0.0
    for sex, _, _ in SEXES:
        for age in AGES:
            total = total + weights[(sex, age)][0]
    return {"sqrt_2pi": sqrt_2pi, "grid": grid, "weights": weights, "total": total}


def counts(table: dict, n: int) -> dict:
    """The founders per group for `n` founders (note section 2): `q = (N · w) / Σ w`, `c =
    floor(q)`, the rest by the largest remainders (ties women first, then the younger age).
    Returns {"groups": [(sex, age, q, floor, remainder, extra, count)], "rank": the groups
    in the order of the remainders, "extra": N − Σ floor}."""
    groups = []
    order = 0
    for sex, _, _ in SEXES:
        for age in AGES:
            w = table["weights"][(sex, age)][0]
            q = (float(n) * w) / table["total"]
            floor = math.floor(q)
            remainder = q - floor
            assert remainder == float(Fraction(q) - floor)
            groups.append([sex, age, q, floor, remainder, 0, order])
            order += 1
    extra = n - sum(group[3] for group in groups)
    assert 0 <= extra <= len(groups)
    rank = sorted(groups, key=lambda group: (-group[4], group[6]))
    for group in rank[:extra]:
        group[5] = 1
    out = [(sex, age, q, floor, remainder, add, floor + add) for sex, age, q, floor, remainder, add, _ in groups]
    assert sum(group[6] for group in out) == n
    return {"groups": out, "rank": [(group[0], group[1], group[4]) for group in rank], "extra": extra}


def born_of(age: int, j: int, c: int) -> int:
    """The birth second of the `j`-th of `c` founders of the age group `age`."""
    return -(age * YEAR + ((2 * j + 1) * YEAR) // (2 * c))


def founder_order(count: dict) -> list:
    """(sex, age, j, c) per founder in the order of `Start`."""
    order = []
    for sex, _, _ in SEXES:
        for group in count["groups"]:
            if group[0] != sex:
                continue
            for j in range(group[6]):
                order.append((sex, group[1], j, group[6]))
    return order


def alleles_of(seed: int, key: int, loci: list) -> list:
    """The founder's alleles (note section 2): per additive locus of the pre-run map in map
    order, `[m, p]` with copy `d` 1 iff `unit(PRERUN_ALLELE, key, str_key(locus), d) <
    0.5`; only the loci where a copy is 1."""
    listed = []
    for locus in loci:
        copies = [1 if unit(words(chacha_key(seed, PRERUN_ALLELE, key, str_key(locus), copy_))[0]) < 0.5 else 0 for copy_ in (0, 1)]
        if any(copies):
            listed.append((locus, copies))
    return listed


def genotype_text(alleles: list, loci: list) -> str:
    """The genotype as one string: per locus in map order its two copies (`mp`)."""
    by_locus = dict(alleles)
    return "".join("".join(str(allele) for allele in by_locus.get(locus, [0, 0])) for locus in loci)


def founders(count: dict, seed: int, tag: str, loci: list) -> list:
    """The founders of `Start` as (born, sex, alleles) with (sex, age, j, c, key)."""
    out = []
    for index, (sex, age, j, c) in enumerate(founder_order(count)):
        key = founder_key(tag, index)
        out.append(((born_of(age, j, c), sex, alleles_of(seed, key, loci)), (sex, age, j, c, key)))
    return out


# ---------------------------------------------------------------------------------------
# 3. A run and its measurement


def run(data: world_reference.Data, founder_list: list, seed: int, tag: str, capacity: int, years: int) -> tuple:
    """The world of a pre-run, advanced year by year to `years · YEAR − 1`; with the census
    per year as (year, clock, living, born, died, emigrated) of that year."""
    world = world_reference.World(seed, tag, True, [founder for founder, _ in founder_list], (), data, capacity=capacity)
    census = []
    for year in range(years):
        end = (year + 1) * YEAR - 1
        world.advance(end)
        start = year * YEAR
        born = sum(1 for value in list(world.sims.values()) + list(world.archive.values()) if start <= value[0] <= end)
        died = sum(1 for value in world.archive.values() if start <= value[1] <= end and value[2] is not None)
        left = sum(1 for value in world.archive.values() if start <= value[1] <= end and value[2] is None)
        census.append((year, end, len(world.sims), born, died, left))
    return world, census


def everyone(world: world_reference.World, emigrants: bool = True) -> list:
    """(sim, born, lineage key, genome, parents) of every person ever in the run (without
    the emigrants if `emigrants` is False: the alternative of decision 11 of the notes)."""
    people = [(sim, value[0], value[1], value[3], value[7]) for sim, value in world.sims.items()]
    people += [
        (sim, value[0], value[3], value[5], value[9]) for sim, value in world.archive.items() if emigrants or value[2] is not None
    ]
    return sorted(people)


def variance_of(values: list) -> float:
    """`Σ (A − m)² / n` with `m = Σ A / n`, both sums from +0.0 in the given order."""
    count = float(len(values))
    total = 0.0
    for value in values:
        total = total + value
    mean = total / count
    squares = 0.0
    for value in values:
        deviation = value - mean
        squares = squares + deviation * deviation
    return squares / count


def generations(world: world_reference.World, emigrants: bool = True) -> list:
    """Per block of 30 birth years with a birth (note section 3): (block, n, mothers,
    fathers, N_e, var A growth, var A cognition); the emigrants born in the run count
    (ADR-0020 section 8), or not with `emigrants` False."""
    traits = {trait.id: trait for trait in world.data.traits}
    blocks = {}
    for sim, born, key, genome, parents in everyone(world, emigrants):
        if parents is None:
            continue
        assert born >= 0
        blocks.setdefault(born // (BLOCK_YEARS * YEAR), []).append((key, genome, parents))
    out = []
    for block in sorted(blocks):
        people = sorted(blocks[block], key=lambda person: person[0])
        mothers = len({parents[0] for _, _, parents in people})
        fathers = len({parents[1] for _, _, parents in people})
        n_e = float(4 * fathers * mothers) / float(fathers + mothers)
        variances = [variance_of([world_reference.additive_value(genome, traits[name]) for _, genome, _ in people]) for name in ("core:growth", "core:cognition")]
        out.append((block, len(people), mothers, fathers, n_e, *variances))
    return out


# ---------------------------------------------------------------------------------------
# 4. Iteration, stop, freeze


def r_of(sigma_out: float, sigma_in: float) -> float:
    """`r = |σ_out − σ_in| / σ_in`."""
    return abs(sigma_out - sigma_in) / sigma_in


def changes(genome_map: dict, measurement: dict) -> list:
    """(trait, σ_in, σ_out, r) per trait of the map, map order."""
    sigma_in = {trait["id"]: float(trait["sigma_ref"]) for trait in genome_map["traits"]}
    return [(trait, sigma_in[trait], sigma, r_of(sigma, sigma_in[trait])) for trait, _, sigma in measurement["traits"]]


def stop_rule(run_index: int, r: dict) -> tuple:
    """The decision after run `run_index` (1 to 5) with `r` per trait: ("stop", reported)
    if `r < 0.02` for the five, ("continue", reported) before run 5, else ("no_freeze",
    reported); `reported` the other traits with `r ≥ 0.05` (map order of `r`)."""
    assert 1 <= run_index <= MAX_RUNS
    reported = [trait for trait, value in r.items() if trait not in STOP_TRAITS and value >= REPORT_BOUND]
    if all(r[trait] < STOP_BOUND for trait in STOP_TRAITS):
        return "stop", reported
    return ("no_freeze" if run_index == MAX_RUNS else "continue"), reported


def next_map(genome_map: dict, measurement: dict) -> dict:
    """The map of the next run: `d_ref` the mean doses, `sigma_ref` the `σ`."""
    return with_reference(genome_map, dict(measurement["loci"]), {trait: sigma for trait, _, sigma in measurement["traits"]})


# ---------------------------------------------------------------------------------------
# The pins


def print_data(template, prerun) -> world_reference.Data:
    demography, calibration, genome_map = prerun
    print(f"prerun_template_data_hash {data_reference.documents_hash(*template):016x}")
    print(
        f"prerun_data removed {' '.join(dropped_loci(template[1]))} diseases_removed {' '.join(d['code'] for d in mendelian(template[1]))}"
        f" loci {len(genome_map['loci'])} additive {len(genome_map['loci'])} traits {len(genome_map['traits'])}"
        f" diseases {' '.join(d['code'] for d in calibration['diseases'])} map_version {genome_map['map_version']}"
        f" named {' '.join(f'{field}={calibration[field]}' for field in data_reference.NAMED_TRAITS)}"
    )
    for line in check_prerun(calibration, genome_map):
        print(line)
    data = prerun_data(prerun)
    print(f"prerun_data_hash run1 {data.hash:016x}")
    for code, trait, heritability, prevalence, value in data_reference.liability_diseases(calibration):
        print(f"prerun_threshold {code} liability {trait} prevalence {prevalence!r} T {value!r} bits {bits(value)}")
    print(f"prerun_measured_loci {len(data.measured_loci)} first {data.measured_loci[0]['id']} last {data.measured_loci[-1]['id']}")
    return data


def print_weights(data, table) -> None:
    print(f"prerun_weight_inputs sqrt_2pi {table['sqrt_2pi']!r} bits {bits(table['sqrt_2pi'])} sigma_g {data.sigma_g!r}")
    for sex, name, p in SEXES:
        c, a, b = death_values(data, sex)
        print(f"prerun_weight_inputs {name} p {p!r} bits {bits(p)} c {c!r} a {a!r} b {b!r}")
    for k in (0, 1, 128, 512, 513, 896, 1023, 1024):
        g, held, m, phi, t = table["grid"][k]
        print(f"prerun_weight_grid k {k} g {g!r} held {held!r} m {m!r} bits {bits(m)} phi {phi!r} bits {bits(phi)} t {t!r}")
    for sex, name, _ in SEXES:
        for age in AGES:
            w, integral, acc = table["weights"][(sex, age)]
            print(f"prerun_weight {name} {age} w {w!r} bits {bits(w)} integral {integral!r} bits {bits(integral)} simpson_sum {acc!r} bits {bits(acc)}")
    print(f"prerun_weight_total {table['total']!r} bits {bits(table['total'])}")


def print_counts(table, n: int) -> dict:
    count = counts(table, n)
    for sex, age, q, floor, remainder, extra, total in count["groups"]:
        name = "female" if sex == FEMALE else "male"
        print(f"prerun_counts {n} {name} {age} q {q!r} bits {bits(q)} floor {floor} remainder {remainder!r} extra {extra} count {total}")
    women = sum(group[6] for group in count["groups"] if group[0] == FEMALE)
    rank = count["rank"]
    extra = count["extra"]
    cut = rank[extra - 1][2] if extra else None
    after = rank[extra][2] if extra < len(rank) else None
    gaps = [abs(rank[i][2] - rank[i + 1][2]) for i in range(len(rank) - 1)]
    nearest_integer = min(min(group[2] - group[3], group[3] + 1 - group[2]) for group in count["groups"])
    print(
        f"prerun_counts_summary {n} floors {n - extra} extra {extra} women {women} men {n - women}"
        f" last_extra_remainder {cut!r} first_without {after!r} gap_at_cut {(cut - after) if cut is not None and after is not None else None!r}"
        f" smallest_gap_between_remainders {min(gaps)!r} q_nearest_integer {nearest_integer!r}"
    )
    print(
        f"prerun_counts_table {n} "
        + " ".join(f"{'f' if group[0] == FEMALE else 'm'}{group[1]}={group[6]}" for group in count["groups"])
    )
    return count


def print_founders(count: dict, n: int, seed: int, tag: str, loci: list, groups) -> None:
    order = founder_order(count)
    starts = {}
    for index, (sex, age, j, c) in enumerate(order):
        starts.setdefault((sex, age), []).append(index)
    for sex, age in groups:
        indexes = starts.get((sex, age))
        if not indexes:
            continue
        for index in sorted({indexes[0], indexes[-1]}):
            _, _, j, c = order[index]
            key = founder_key(tag, index)
            alleles = alleles_of(seed, key, loci)
            name = "female" if sex == FEMALE else "male"
            print(
                f"prerun_founder {n} {tag} seed {seed:#x} index {index} {name} age {age} j {j} c {c} born {born_of(age, j, c)}"
                f" key {key:016x} listed {len(alleles)} doses {sum(sum(copies) for _, copies in alleles)}"
                f" genotype {genotype_text(alleles, loci)}"
            )
            print(f"prerun_founder_alleles {n} index {index} " + " ".join(f"{locus}=[{m},{p}]" for locus, (m, p) in alleles))


def disease_robust(world: world_reference.World, sim: int, born: int, key: int, affliction) -> None:
    """The death second of a lethal affliction does not hang on the last bits of libm: with
    the switch on from the origin (`used` 0), `ceil((a − x_since) · YEAR)` from the closed
    form (`world_reference.disease_age`) is the same with the budget moved by one ulp either
    way and the onset (a normal draw, within 2^-45 · sd of the reference) by ±1e-12 years."""
    if affliction.exposure is None:
        return
    used, since = affliction.exposure
    assert used == 0.0 and since is not None
    at_since = as_years(since - born)
    budget = world_reference.budget_of(world.seed, key, affliction.disease.code)
    seconds = set()
    for budget_ulps in (-1, 0, 1):
        for onset in (affliction.onset - 1e-12, affliction.onset, affliction.onset + 1e-12):
            nudged = world_reference.step_ulps(budget, budget_ulps)
            age = world_reference.disease_age(max(at_since, onset), nudged, affliction.disease.hazard)
            seconds.add(None if age is None else world_reference.from_years_ceil(age - at_since))
    assert len(seconds) == 1, (sim, seconds)


def person_checks(world: world_reference.World) -> tuple:
    """Per person of the run (living, dead, emigrated), by SimId, a line with what the two
    traits of this slice give, checked by hand: `g_L` (its `A` an exact sum here, since
    `d_ref` is 1 in run 1: `G` is the correctly rounded `A / σ_ref`), `m = exp(−(σ_g · ĝ))`,
    `z_p` at the age 0 and at its age at the end (equal bit for bit: the weights do not
    depend on the age), `θ_i`, the woman's menarche, `L` and its distance to `T` (affected
    iff `L > T`; the death second of each affliction robust, `disease_robust`); for a
    founder whether `m` moved its planned death of senescence (the same budget with `m` =
    1). Returns (lines, smallest |L − T| with its sim, founders whose senescence `m` moved)."""
    data = world.data
    (disease,) = [disease for disease in data.diseases if disease.liability is not None]
    lines, nearest, moved = [], None, 0
    sims = sorted(set(world.sims) | set(world.archive))
    for sim in sims:
        if sim in world.sims:
            born, key, sex, genome = world.sims[sim][:4]
            status = "living"
        else:
            value = world.archive[sim]
            born, key, sex, genome = value[0], value[3], value[4], value[5]
            status = f"emigrated {value[1]}" if value[2] is None else f"died {value[1]} {world.cause_code(value[2])}"
        a_exact = Fraction(0)
        for loci in data.longevity.groups:
            for locus, coefficient, d_ref in loci:
                a_exact += Fraction(coefficient) * (Fraction(world_reference.dose(genome, locus)) - Fraction(d_ref))
        g = genetic_value(genome, data.longevity)
        if float(a_exact) == world_reference.additive_value(genome, data.longevity) and Fraction(float(a_exact)) == a_exact:
            assert g == float(a_exact / Fraction(data.longevity.sigma_ref)), sim
        m = data.multiplier(genome)
        assert m == exp_reference(-(data.sigma_g * longevity_input(g)))[1]
        end = world.closed
        theta_0 = world.theta_of(sim, born)
        theta_end = world.theta_of(sim, end)
        assert theta_0 == theta_end, sim
        z_p = world_reference.phenotype(world.seed, data.puberty, genome, key, world.parent_keys(sim), 0)
        theta = world_reference.theta_i(data.puberty_curve[sex], z_p)
        assert theta == theta_end
        liability = world.liability_of(sim, disease)
        margin = abs(liability - disease.threshold)
        if nearest is None or margin < nearest[0]:
            nearest = (margin, sim)
        afflictions = world.sims[sim][4] if sim in world.sims else world.archive[sim][6]
        affected = any(affliction.disease is disease for affliction in afflictions)
        assert affected == (liability > disease.threshold)
        for affliction in afflictions:
            disease_robust(world, sim, born, key, affliction)
        extra = ""
        if sex == FEMALE:
            menarche = world.menarche_of(sim, end)
            extra = f" menarche {menarche!r} bits {bits(menarche)}"
        if world.parent_keys(sim) is None:
            a, b = data.senescence[sex]
            x0 = as_years(0 - born)
            budget = world_reference.budget_of(world.seed, key, world_reference.SENESCENCE)
            with_m = world_reference.from_years_ceil(world_reference.senescence_age(x0, budget, a, b, m) - x0)
            without = world_reference.from_years_ceil(world_reference.senescence_age(x0, budget, a, b, 1.0) - x0)
            moved += with_m != without
            extra += f" senescence_due {with_m} (with m 1: {without})"
        parents = world.sims[sim][7] if sim in world.sims else world.archive[sim][9]
        lines.append(
            f"sim {sim} {'female' if sex == FEMALE else 'male'} born {born} key {key:016x}"
            f" parents {'-' if parents is None else f'{parents[0]} {parents[1]}'} {status}"
            f" g_L {g!r} bits {bits(g)} m {m!r} bits {bits(m)} z_p {z_p!r} bits {bits(z_p)} theta {theta!r} bits {bits(theta)}"
            f" L {liability!r} bits {bits(liability)} affected {affected}{extra}"
        )
    return lines, nearest, moved


def attempt_checks(world: world_reference.World) -> tuple:
    """Every attempt of the run recomputed by hand (5.6, 11.3): `θ_i` of each partner from
    the phenotype `z_p` of the puberty trait (its genome, key, parents' keys), her menarche
    from it and her draw `δ`, the factors at both ages, `(rate · F_w) · F_m`; equal bit for
    bit to the chance the world drew against. Returns (attempts, those whose chance differs
    with `z_p` = 0, conceptions)."""
    data = world.data
    form = data.fertility

    def person(sim):
        if sim in world.sims:
            born, key, sex, genome = world.sims[sim][:4]
        else:
            value = world.archive[sim]
            born, key, sex, genome = value[0], value[3], value[4], value[5]
        return born, key, sex, genome

    def theta(sim, z_zero=False):
        born, key, sex, genome = person(sim)
        z_p = 0.0 if z_zero else world_reference.phenotype(world.seed, data.puberty, genome, key, world.parent_keys(sim), 0)
        return world_reference.theta_i(data.puberty_curve[sex], z_p)

    differ = 0
    for t, woman, man, p, _ in world.attempts:
        chances = []
        for z_zero in (False, True):
            her_born, her_key = person(woman)[:2]
            delta = world_reference.menarche_delta(world.seed, her_key, form)
            menarche = world_reference.menarche(theta(woman, z_zero), delta, form)
            f_w = world_reference.female_factor(as_years(t - her_born), menarche, form)
            f_m = world_reference.male_factor(as_years(t - person(man)[0]), theta(man, z_zero), form)
            chances.append(world_reference.chance(data.adult_rate, f_w, f_m))
        assert chances[0] == p, (t, woman, man, p, chances)
        differ += chances[1] != p
    return len(world.attempts), differ, sum(1 for attempt in world.attempts if attempt[4])


def print_run(label: str, template, prerun, data, table, n: int, years: int, out=None) -> None:
    """One step of the iteration (note section 5): the founders, the run, its census, the
    measurement, the generations, `r`, the stop rule after run 1, the map of run 2 and the
    freeze as if run 1 stopped, under the labels `<label>_...`."""
    loci = [locus["id"] for locus in data.measured_loci]
    count = counts(table, n)
    founder_list = founders(count, MEASURE_SEED, MEASURE_TAG, loci)
    women = sum(1 for (_, sex, _), _ in founder_list if sex == FEMALE)
    print(
        f"{label} founders {n} (women {women}, men {n - women}) years {years} seed {MEASURE_SEED:#x} tag {MEASURE_TAG} capacity {n}"
        f" switch on nutrition none couples none data_hash {data.hash:016x}"
    )
    for index, ((born, sex, alleles), (_, age, j, c, key)) in enumerate(founder_list):
        name = "female" if sex == FEMALE else "male"
        print(f"{label}_founder {index} {name} age {age} j {j} c {c} born {born} key {key:016x} genotype {genotype_text(alleles, loci)}")
    started = time.time()
    world, census = run(data, founder_list, MEASURE_SEED, MEASURE_TAG, n, years)
    elapsed = time.time() - started
    for year, clock, living, born, died, left in census:
        print(f"{label}_census year {year} clock {clock} living {living} born {born} died {died} emigrated {left}")
    print(f"{label}_year_hashes {' '.join(f'{value:016x}' for value in world.year_hashes)}")
    print(f"{label}_log_hash {world.log_hash():016x} entries {len(world.log)} state_hash {world.state_hash():016x} save_bytes {len(world.save())}")
    deaths = {}
    for value in world.archive.values():
        cause = "emigration" if value[2] is None else world.cause_code(value[2])
        deaths[cause] = deaths.get(cause, 0) + 1
    twins = sum(1 for record in world.multiples if record[5] != world_reference.SINGLE)
    print(
        f"{label}_story ever {len(world.sims) + len(world.archive)} living {len(world.sims)} archived {len(world.archive)} by {sorted(deaths.items())}"
        f" partnerships {len(world.partnerships)} separations {len(world.separations)} conceptions {len(world.multiples)} twins {twins}"
        f" open_pregnancies {len(world.pregnancies)} households {len(world.households)} affected"
        f" {sum(1 for value in world.sims.values() if value[4]) + sum(1 for value in world.archive.values() if value[6])}"
        f" emigration_years (year, P, households that left) {[(record['year'], record['P'], len(record['left'])) for record in world.emigrations if record['e'] is not None]}"
    )
    lines, nearest, moved = person_checks(world)
    for line in lines:
        print(f"{label}_person {line}")
    print(
        f"{label}_person_checks g_L (exact A over sigma_ref), m, z_p at two ages, theta, affected iff L > T, disease death seconds robust: met;"
        f" smallest |L - T| {nearest[0]!r} (sim {nearest[1]}); founders whose senescence m moved {moved}"
    )
    attempts, differ, conceived = attempt_checks(world)
    print(f"{label}_attempt_checks attempts {attempts} conceptions {conceived} chance recomputed from z_p, theta, menarche, factors: met; chances that differ with z_p = 0: {differ}")
    measurement = world.measure()
    genome_map = prerun[2]
    print(f"{label}_measure clock {world.closed} living {measurement['living']}")
    print(f"{label}_measure_json {world.measure_json()}")
    r = {}
    for (trait, mean, sigma), (_, sigma_in, sigma_out, value) in zip(measurement["traits"], changes(genome_map, measurement)):
        assert sigma == sigma_out
        r[trait] = value
        print(
            f"{label}_measure_trait {trait} mean {mean!r} bits {bits(mean)} sigma {sigma!r} bits {bits(sigma)}"
            f" sigma_in {sigma_in!r} r {value!r} bits {bits(value)}{' stop_trait' if trait in STOP_TRAITS else ''}"
        )
    for locus, mean_dose in measurement["loci"]:
        print(f"{label}_measure_locus {locus} mean_dose {mean_dose!r} bits {bits(mean_dose)}")
    for name, emigrants in (("generation", True), ("generation_without_emigrants", False)):
        for block, size, mothers, fathers, n_e, var_growth, var_cognition in generations(world, emigrants):
            assert n_e == float(Fraction(4 * mothers * fathers, mothers + fathers))
            print(
                f"{label}_{name} block {block} years [{block * BLOCK_YEARS}, {(block + 1) * BLOCK_YEARS}) born {size}"
                f" mothers {mothers} fathers {fathers} N_e {n_e!r} bits {bits(n_e)} var_A core:growth {var_growth!r} bits {bits(var_growth)}"
                f" core:cognition {var_cognition!r} bits {bits(var_cognition)}"
            )
    decision, reported = stop_rule(1, r)
    print(f"{label}_stop_rule run 1 decision {decision} stop_traits {' '.join(f'{trait}={r[trait] < STOP_BOUND}' for trait in STOP_TRAITS)} reported {' '.join(reported) or '-'}")
    second = next_map(genome_map, measurement)
    second_data = prerun_data((prerun[0], prerun[1], second))
    print(f"{label}_next_data_hash run2 {second_data.hash:016x}")
    for trait in second["traits"]:
        print(f"{label}_next_map trait {trait['id']} sigma_ref {trait['sigma_ref']!r} bits {bits(trait['sigma_ref'])}")
    for locus in second["loci"]:
        d_ref = locus["action"]["additive"]["d_ref"]
        print(f"{label}_next_map locus {locus['id']} d_ref {d_ref!r} bits {bits(d_ref)}")
    calibration, frozen = frozen_documents(template, measurement)
    checks = map4_reference.check_map(frozen) + map4_reference.check_calibration(calibration, frozen)
    frozen_hash = data_reference.documents_hash(template[0], calibration, frozen)
    texts = {"genome_map.json": map4_reference.text(frozen), "genetics.json": map4_reference.text(calibration)}
    assert texts["genetics.json"] == (TEMPLATE / "calibration" / "genetics.json").read_text()
    print(
        f"{label}_frozen (as if run 1 stopped) data_hash {frozen_hash:016x} genome_map.json bytes {len(texts['genome_map.json'].encode())}"
        f" xxh3 {world_reference.hash64(texts['genome_map.json'].encode()):016x} genetics.json bytes {len(texts['genetics.json'].encode())}"
        f" (the template's, unchanged) checks {len(checks)} met"
    )
    if out is not None:
        write_frozen(Path(out), calibration, frozen)
        assert data_reference.data_hash(Path(out)) == frozen_hash
        print(f"{label}_frozen_written {out}")
    print(f"{label}_seconds {elapsed:.1f} (the run alone, this machine; not a pin)", file=sys.stderr)


def print_stop_cases() -> None:
    """The stop rule on given measurements (note section 5): per case the run, `σ_in` (the
    run's `sigma_ref`) and `σ_out` per trait, `r` with its bits and the decision. `σ_in` of
    the first cases are the template's start values; run 1 stops, run 5 does not freeze,
    and the cases between (a trait not among the five at `r ≥ 0.05` is reported; `r`
    exactly 0.02, from `σ_in` 3.125 and `σ_out` 3.1875, is not below the bound)."""
    start = {"core:growth": 2.8284271247461903, "core:cognition": 2.8284271247461903, "core:longevity": 2.0,
             "core:puberty": 1.7320508075688772, "core:disease_liability": 2.0, "core:face_01": 0.824621125123532,
             "core:hip_width": 0.99498743710662}
    calm = {"core:growth": 2.86, "core:cognition": 2.80, "core:longevity": 2.03, "core:puberty": 1.75,
            "core:disease_liability": 1.98, "core:face_01": 0.83, "core:hip_width": 1.0}
    exact = dict(start, **{"core:growth": 3.125})
    cases = [
        ("run1_stops", 1, start, calm),
        ("run1_at_bound", 1, start, dict(calm, **{"core:growth": 2.8284271247461903 * 1.02})),
        ("run3_growth_moves", 3, start, dict(calm, **{"core:growth": 3.4})),
        ("run2_stops_reported", 2, start, dict(calm, **{"core:face_01": 0.9, "core:hip_width": 0.9})),
        ("run4_exactly_002", 4, exact, dict(calm, **{"core:growth": 3.1875})),
        ("run4_below_002", 4, exact, dict(calm, **{"core:growth": math.nextafter(3.1875, -math.inf)})),
        ("run5_no_freeze", 5, start, dict(calm, **{"core:cognition": 2.9})),
        ("run5_stops", 5, start, calm),
    ]
    for name, run_index, sigma_in, sigma_out in cases:
        r = {trait: r_of(sigma_out[trait], sigma_in[trait]) for trait in sigma_in}
        decision, reported = stop_rule(run_index, r)
        print(
            f"prerun_stop {name} run {run_index} decision {decision} reported {' '.join(reported) or '-'} "
            + " ".join(f"{trait} in {sigma_in[trait]!r} out {sigma_out[trait]!r} r {r[trait]!r} bits {bits(r[trait])}" for trait in sigma_in)
        )
    assert r_of(3.1875, 3.125) == 0.02


def main(argv) -> int:
    out = argv[argv.index("--out") + 1] if "--out" in argv else None
    template = template_documents()
    prerun = prerun_documents(template)
    data = print_data(template, prerun)
    table = weight_table(data)
    if "--trial" in argv:
        # The optional, larger trial (400 founders, 30 years): one step of the iteration
        # with more births, twins, separations and emigrations than the note's small run.
        print_run("prerun_trial", template, prerun, data, table, 400, 30, out=out)
        return 0
    print_weights(data, table)
    loci = [locus["id"] for locus in data.measured_loci]
    groups = [(FEMALE, 18), (FEMALE, 39), (FEMALE, 60), (MALE, 18), (MALE, 39), (MALE, 60)]
    for n, seed, tag in ((MEASURE_FOUNDERS, MEASURE_SEED, MEASURE_TAG), (POOL_FOUNDERS, POOL_SEED, POOL_TAG), (40, MEASURE_SEED, MEASURE_TAG)):
        count = print_counts(table, n)
        print_founders(count, n, seed, tag, loci, groups)
    print_stop_cases()
    print_run("prerun_small", template, prerun, data, table, 40, 30, out=out)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

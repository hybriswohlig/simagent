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
  pass `BodyData::census` (`census_body_check`, note section 6.1: the check of
  `BodyData::new`, `body_reference.body_data_check` with the 35 traits of map 4: each trait in
  the map with a phenotype row whose child and adult weights are equal bit for bit,
  `core:mc1r` and `core:herc2` recessive on an autosome; except that both loci may be absent
  from the map together, then both are read as 0 copies), every living sim in ascending SimId
  is evaluated once; else every count is 0 (note section 3.2: "all zero if it fails"). One
  of the two loci without the other, or a present one not recessive on an autosome, still
  fails. Integers only.
- `CLAMPED_KEYS`: the parameters of `contracts/body.json` whose unit is `z` or `cm`, in the
  contract's order (`height_cm`, the eight body `*_z`, `face_axis_01` to `face_axis_12`).
- `births`, `arrivals`, `emigrations` and `deaths` (ADR-0021 section 3, addendum of
  2026-10-21, "Allgemein", "Reihenfolge" and "Bevölkerung"; core 44), after the columns above,
  integers counted over the living and the archive together; "in the year" means
  `year_start(k) <= t < year_start(k + 1)`:
  - `births`: the persons born in the town in the year (`born_in_town`): a child of two town
    parents, or the child of a pregnancy brought from the pool and born in the town;
  - `arrivals`: the arrivals (a pool person with `born_pool`) whose arrival second lies in
    the year, without the persons `Start` created: the SimIds 1 to `N`, `N` its founders plus
    the members of the households of its pool (a village's persons), read from the input
    `Start` (`start_persons`), not `n_start`. An immigration of second 0 counts;
  - `emigrations`: the persons who left the town by emigration in the year;
  - `deaths`: per cause of `death_causes` (`core:senescence`, `core:accident`, then each
    disease of the data that can kill, effect `death` or `death_in_two_steps`, in the data's
    order) the persons who died of it in the year; each fixture names the list
    (`death_causes` in census.json) and each row holds the counts in its order.
  Each row is checked against the year's entries of the log (`LogPopulation`: `core:birth`,
  `core:arrived` not caused by the input `Start`, `core:emigrated`, `core:death` per cause),
  and at the end of a fixture `N` against `World.made_by_start`.
- `diseases_on`, `affected`, `adults`, `age25` and `inbreeding_e4` (the same addendum,
  "Mittelwerte, genau", "Schalter" and "Erbe"; Grok R1 to R3 and S1; core 45), after the
  population columns, integers (`inheritance`); each fixture names the data's diseases in
  their order (`diseases` in census.json):
  - `diseases_on`: 1 if the switch of the hereditary diseases is on at `t` (`World.switch_at`,
    after every input acting by `t`), else 0;
  - `affected`: per disease of the data in its order the living whose genetics card says
    "affected" at `t`: the switch on and `as_years(t − born)` at least the onset of an
    affliction of that disease; all 0 with the switch off. Checked against the card's lines
    (`golden_reference.disease_rows`);
  - `adults`, `age25`: `[[count, mm], [count, mm]]`, women then men: the living whose body
    the census evaluates (none if the body data fail) with at least 25, and with exactly 25,
    full years at `t` (`⌊(t − born) / YEAR⌋`), their count and the mean of the body's
    `height_cm` (the contract value, as the card shows it) times 10;
  - `inbreeding_e4`: over the `births` of the year (living and archived) the mean of `F · 10
    000`, `F = φ(mother, father)` of the two parent lineage keys up to the great-grandparents
    of both (`wr.kinship_by_keys` with `World.kin_of`: town, archive and pool; checked by
    Wright's path counting), 0 without a birth;
  - a mean (`mean_rounded`): each value scaled, added one after the other in ascending SimId
    in binary64, divided once by the count, rounded once half away from zero; 0 for no one.
  Every fixture starts with the switch on and has no input after `Start`, so `diseases_on`
  is 1 in every row, and no birth in them has related parents (`inbreeding_e4` 0 throughout);
  `--inbreeding-check` runs the rule on "kin_deep", whose cousins give it values.

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
- the copies of allele 1 at `core:mc1r` and `core:herc2`; 0 at both on data without both loci
  (note section 6.1).
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
  on, switch on, on the documents of `father_dies` that pool_c was made from:
  `arrival_reference.pool_documents`) with three nutrition shocks, year ends 0 to 13 (the pool
  runs out in year 14).
- (b2) `b2_pool_b_start`: the pool `pool_b` at the start without immigration (`n_start` 50:
  every household), two nutrition shocks, 20 years.
- (c) `c_prerun_small`: the small run of tools/prerun_reference.py (40 founders, seed
  0x11b019a1, tag `core:prerun_measure`, capacity 40, the pre-run data, 30 years). The
  pre-run data drop `core:mc1r` and `core:herc2`, so `BodyData::census` evaluates every
  living body with both read as 0 copies (note section 6.1; before it, `BodyData::new`
  rejected these data and every count was 0). (c') `c_prerun_small_reference_alleles`, from
  before section 6.1, evaluated the same run with MC1R and HERC2 read as 0 copies as this
  reference's assumption; since 6.1 that is the rule, so (c') equals (c) row for row and keeps
  only its old label `body_data` (kept byte for byte). Clamp counts do not depend on the two
  loci (pigment is never clamped).

Usage: $HOME/.cache/simagent-tools-venv/bin/python tools/census_reference.py [--out DIR]
       (default DIR ~/.cache/simagent-notes/ref-census; writes census.json and census.txt)
       ... census_reference.py --scan-a FIRST LAST   (seeds of fixture (a) and their founders
       whose curve is held; for choosing the seed, not a pin)
       ... census_reference.py --phenotypes FILE   (the golden phenotypes.json, compact)
       ... census_reference.py --pure-heights [--out DIR]   (pure_heights.json: the pure height of
       ADR-0015 4 and spec 10.2, and the height before the loss of age)
       ... census_reference.py --records [--out DIR]   (records_phenotypes.json: the 35 `z`
       of the living at the last year end of the scenario "records" of arrival_reference.py)
       ... census_reference.py --pigment-check [--out DIR]   (pigment_check.json: the cases of
       `census_body_check` on altered data, and one living person of (c) evaluated on the
       pre-run data and on data with both loci present and both counts forced to 0)
       ... census_reference.py --inbreeding-check   (`inbreeding_e4` of the run "kin_deep" of
       tools/state_reference.py against its values by hand)
"""

import copy
import json
import math
import struct
import sys
import time
from fractions import Fraction
from pathlib import Path

import arrival_reference
import body_reference as br
import data_reference
import map4_reference
import pool_reference
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


PIGMENT_LOCI = ("core:mc1r", "core:herc2")


def census_body_check(calibration: dict, genome_map: dict) -> tuple:
    """`BodyData::census` (note section 6.1; ADR-0015 section 2 as amended there): the check
    of `BodyData::new` (`body_reference.body_data_check` with the 35 traits of map 4), except
    that `core:mc1r` and `core:herc2` may be absent from the map together; then only the
    traits are checked (`body_reference.body_traits_check`) and the census reads 0 copies at
    both (the reference allele, ADR-0020 section 3). One locus without the other, or a present
    one not recessive on an autosome, fails as in `BodyData::new`. Returns (the reason or None,
    whether both loci are absent)."""
    in_map = {locus["id"] for locus in genome_map["loci"]}
    if not any(locus_id in in_map for locus_id in PIGMENT_LOCI):
        return br.body_traits_check((calibration, genome_map), br.BODY_TRAITS_MAP4), True
    return br.body_data_check((calibration, genome_map), br.BODY_TRAITS_MAP4), False


class BodyCensusData:
    """What the census reads from the data (`BodyData::census`, `census_body_check`): the
    calibration's body values, the 35 traits of map 4 as phenotype traits, MC1R and HERC2 (or
    `pigment_absent`: neither in the map, both read as 0 copies), `σ_g`, the Gompertz `b` per
    sex and the trait of longevity; or the reason the data are rejected (`reason`, then the
    census counts nothing). `reference_alleles` (fixture c' only) changes no value: c' was
    this reference's assumption before note section 6.1 made it the rule, and keeps its old
    label (`body_data_label`). `strict` (`pigment_check` only) checks with `BodyData::new`
    instead, as every reader other than the census does."""

    def __init__(self, data: wr.Data, documents: tuple, reference_alleles: bool = False, strict: bool = False):
        _, calibration, genome_map = documents
        self.data = data
        if strict:
            self.reason, self.pigment_absent = br.body_data_check((calibration, genome_map), br.BODY_TRAITS_MAP4), False
        else:
            self.reason, self.pigment_absent = census_body_check(calibration, genome_map)
        self.reference_alleles = reference_alleles
        if reference_alleles:
            assert self.reason is None and self.pigment_absent, "only for data without both loci"
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
        assert (self.mc1r is None) == self.pigment_absent and (self.herc2 is None) == self.pigment_absent

    @property
    def accepted(self) -> bool:
        return self.reason is None


def body_data_label(body: BodyCensusData) -> str:
    """The field `body_data` of a fixture in census.json."""
    if not body.accepted:
        return f"rejected {body.reason}"
    if body.reference_alleles:
        # Fixture c', its label from before note section 6.1, kept byte for byte.
        return "accepted with MC1R and HERC2 read as 0 copies (the twin of c, which reaches the same rows by the rule of note section 6.1)"
    if body.pigment_absent:
        return "accepted without core:mc1r and core:herc2 in the map, both read as 0 copies (note section 6.1)"
    return "accepted"


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
        if body.pigment_absent:
            mc1r = herc2 = 0
        else:
            mc1r, herc2 = sum(copies_at(genome, body.mc1r)), sum(copies_at(genome, body.herc2))
        # The data of the body (the world's in every fixture; other data only in
        # `pigment_check`, the same person on data with both loci).
        g, held, delta = delta_of(body.data, genome, sex)
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
    curve-held persons (sim, lineage key, `z_h`, `z_p` before the hold) for the trace. The
    population columns follow the body's (`population`), whatever the body data."""
    world = evaluator.world
    t = year_end(year)
    assert world.closed == t
    row = {"year": year, "living": len(world.sims), "lived_secs": lived_secs(world, year), "body_evaluated": 0, "clamped": [0] * len(CLAMPED_KEYS), "curve_clamped": 0}
    row.update(population(world, year, death_causes(world.data)))
    held = []
    # (sim, sex, full years at `t`, the body's `height_cm`) of every evaluated living sim, by SimId.
    heights = []
    if evaluator.body.accepted:
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
            heights.append((sim, evaluation["parts"]["sex"], evaluation["age"] // YEAR, evaluation["params"]["height_cm"][0]))
        assert row["body_evaluated"] == row["living"]
    row.update(inheritance(world, year, heights, row["births"]))
    return row, held


# ---------------------------------------------------------------------------------------
# The population columns (ADR-0021 section 3, addendum of 2026-10-21, "Allgemein",
# "Reihenfolge" and "Bevölkerung")


def death_causes(data: wr.Data) -> list:
    """The causes of death of the census in their order: `core:senescence`, `core:accident`,
    then each disease of the data that can kill (effect `death` or `death_in_two_steps`; a
    `visible` one has no hazard and never kills) in the order of the data's list `diseases`."""
    return [wr.SENESCENCE, wr.ACCIDENT] + [disease.code for disease in data.diseases if disease.hazard is not None]


def start_persons(world: wr.World) -> int:
    """`N` of the column `arrivals`: the persons `Start` created, read from the input `Start`
    alone (its founders plus the members of the households of its pool; a village's persons,
    one per household), not the requested `n_start`. They hold the SimIds 1 to `N`."""
    pool = world.pool_start
    members = 0 if pool is None else sum(len(household["members"]) for household in pool["households"])
    return len(world.args["founders"]) + members


def born_in_town(world: wr.World, sim: int) -> bool:
    """A living or archived sim born in the town: a child of two town parents (`parents` set
    at its birth) or the child of a pregnancy brought from the pool and born here (a pool child,
    its PoolOrigin without `born_pool`). Founders and arrivals were not born here."""
    parents = world.sims[sim][7] if sim in world.sims else world.archive[sim][9]
    if parents is not None:
        return True
    return world.is_pool_person(sim) and world.origins[sim]["born_pool"] is None


def population(world: wr.World, year: int, causes: list) -> dict:
    """The population columns of the row of `year` (the world advanced to its year end), over
    the living and the archive together, "in the year" meaning `year · YEAR <= t < (year + 1) ·
    YEAR`: `births` the sims born in the town (`born_in_town`) with `born` in the year;
    `arrivals` the arrivals (`is_arrival`: a pool person with `born_pool`) with `arrived` in the
    year and a SimId above `N` (`start_persons`; an immigration of second 0 counts, the persons
    `Start` made do not); `emigrations` the archived sims that left by emigration (no cause) in
    the year; `deaths` per cause of `causes` the archived sims that died of it in the year."""
    start, end = year * YEAR, (year + 1) * YEAR
    n = start_persons(world)
    births = arrivals = emigrations = 0
    deaths = [0] * len(causes)
    for sim in sorted(set(world.sims) | set(world.archive)):
        if born_in_town(world, sim) and start <= world.born_of(sim) < end:
            births += 1
        if world.is_arrival(sim) and sim > n and start <= world.origins[sim]["arrived"] < end:
            arrivals += 1
    for sim, value in world.archive.items():
        left, cause = value[1], value[2]
        if not start <= left < end:
            continue
        if cause is None:
            emigrations += 1
        else:
            deaths[causes.index(world.cause_code(cause))] += 1
    return {"births": births, "arrivals": arrivals, "emigrations": emigrations, "deaths": deaths}


# ---------------------------------------------------------------------------------------
# The inheritance columns (ADR-0021 section 3, addendum of 2026-10-21, "Allgemein",
# "Mittelwerte, genau", "Reihenfolge", "Schalter" and "Erbe"; Grok R1 to R3 and S1)

ADULT_YEARS = 25


def mean_rounded(values: list) -> int:
    """The mean of the addendum ("Mittelwerte, genau"): the scaled values added one after the
    other in the given order (ascending SimId) in binary64, not `math.fsum`; the sum divided
    once by the count; `m` (never negative) rounded once: `n = ⌊m⌋`, `n + 1` if `m − n ≥ 0.5`
    (half away from zero measured on the binary64 `m`; not `round`, which rounds half to
    even). No value: 0."""
    if not values:
        return 0
    total = 0.0
    for value in values:
        total += value
    m = total / float(len(values))
    assert m >= 0.0
    n = math.floor(m)
    return n + 1 if m - n >= 0.5 else n


# The rounding by hand: one exact half (1652.5 → 1653), 312.5 → 313 (not 312 as `round` gives),
# 625 / 3 → 208, 78.125 → 78, the largest binary64 below 0.5 → 0, nothing → 0.
assert mean_rounded([1652.0, 1653.0]) == 1653 and mean_rounded([625.0, 0.0]) == 313 and round(312.5) == 312
assert mean_rounded([625.0, 0.0, 0.0]) == 208 and mean_rounded([156.25, 0.0]) == 78
assert mean_rounded([0.5]) == 1 and mean_rounded([math.nextafter(0.5, 0.0)]) == 0 and mean_rounded([]) == 0


def affected_counts(world: wr.World, t: int) -> tuple:
    """(`diseases_on`, `affected`) at `t`: the switch of the hereditary diseases at `t`
    (`World.switch_at`, after every input acting by `t`) as 1 or 0; per disease of the data in
    its order the living sims whose genetics card says "affected" (the card's rule): the switch
    on and the age `as_years(t − born)` at least the onset of an affliction of that disease.
    With the switch off every count is 0."""
    on = world.switch_at(t)
    order = {disease.code: index for index, disease in enumerate(world.data.diseases)}
    affected = [0] * len(order)
    if on:
        for sim in sorted(world.sims):
            born, afflictions = world.sims[sim][0], world.sims[sim][4]
            x = as_years(t - born)
            for affliction in afflictions:
                if x >= affliction.onset:
                    affected[order[affliction.disease.code]] += 1
    return int(on), affected


def card_affected(world: wr.World, t: int) -> list:
    """The same counts read from the disease lines of the genetics card of every living sim at
    `t` (`golden_reference.disease_rows`): a check of `affected_counts`, not a definition."""
    import golden_reference

    order = {disease.code: index for index, disease in enumerate(world.data.diseases)}
    counts = [0] * len(order)
    for card in golden_reference.disease_rows(world, t, chosen=list(world.sims)):
        for line in card["diseases"]:
            if line["status"]["kind"] == "affected":
                counts[order[line["disease"]]] += 1
    return counts


def inbreeding_by_paths(world: wr.World, mother: int, father: int, depth=wr.KINSHIP_DEPTH) -> Fraction:
    """`φ(mother, father)` of two lineage keys by Wright's path counting (an independent check
    of `wr.kinship_by_keys`, as `state_reference.kinship_by_paths` checks it on SimIds): the set
    `S` of the two and their ancestors up to `depth` generations through `World.kin_of` (town,
    archive and pool records), the edges from a key to its parents in `S`; the sum over the
    common ancestors `A` and the pairs of paths that meet only in `A` of `(1/2)^(n1 + n2 + 1) ·
    (1 + F_A)`, `F_A` the same within `S` (0 unless both its parents are in `S`)."""
    parents_of = {}

    def parents(key) -> list:
        if key not in parents_of:
            parents_of[key] = [parent for parent in world.kin_of(key)[1:] if parent is not None]
        return parents_of[key]

    def ancestors(key) -> set:
        found, frontier = set(), {key}
        for _ in range(depth):
            frontier = {parent for person in frontier for parent in parents(person)}
            found |= frontier
        return found

    s = {mother, father} | ancestors(mother) | ancestors(father)

    def inside(key) -> list:
        return [parent for parent in parents(key) if parent in s]

    def up(key) -> list:
        paths, stack = [], [(key,)]
        while stack:
            path = stack.pop()
            paths.append(path)
            stack.extend(path + (parent,) for parent in inside(path[-1]))
        return paths

    inbreeding = {}

    def big_f(key) -> Fraction:
        if key not in inbreeding:
            known = inside(key)
            inbreeding[key] = phi(*known) if len(known) == 2 else Fraction(0)
        return inbreeding[key]

    def phi(a, b) -> Fraction:
        if a == b:
            return (1 + big_f(a)) / 2
        total = Fraction(0)
        for first in up(a):
            for second in up(b):
                if first[-1] == second[-1] and set(first) & set(second) == {first[-1]}:
                    total += Fraction(1, 2 ** (len(first) + len(second) - 1)) * (1 + big_f(first[-1]))
        return total

    return phi(mother, father)


def newborn_inbreeding(world: wr.World, sim: int) -> Fraction:
    """`F = φ(mother, father)` of a sim born in the town: the kinship of its biological
    parents' lineage keys (`World.biological_keys`: its parents' keys for a child of two town
    parents, its PoolOrigin keys for the child of a pregnancy from the pool, the father in the
    town or never come) up to the great-grandparents of both (`wr.kinship_by_keys` with
    `World.kin_of`, the kinship percent's calculation), checked by path counting. Exact."""
    mother, father = world.biological_keys(sim)
    assert mother is not None and father is not None, f"sim {sim} is born in the town without both parent keys"
    value = wr.kinship_by_keys(mother, father, world.kin_of)[0]
    assert value == inbreeding_by_paths(world, mother, father), sim
    if not world.is_pool_person(sim):
        # A child of two town parents: the market's φ of their SimIds is the same.
        parents = world.sims[sim][7] if sim in world.sims else world.archive[sim][9]
        assert world.kinship(parents[0], parents[1]) == value, sim
    return value


def inheritance(world: wr.World, year: int, heights: list, births: int) -> dict:
    """The inheritance columns of the row of `year` (the world advanced to its year end `t`),
    after the population columns:

    - `diseases_on`, `affected` (`affected_counts`, checked against the card's lines);
    - `adults` and `age25`, each `[[count, mm], [count, mm]]` for women then men: the
      evaluated living (`heights`, every living sim if the body data pass, else none) with at
      least 25 and with exactly 25 full years at `t` (`⌊(t − born) / YEAR⌋`), the count and the
      mean (`mean_rounded`) of their `height_cm` times 10 in ascending SimId;
    - `inbreeding_e4`: over the persons born in the town in the year (`births`, living and
      archived, ascending SimId) the mean of `F · 10 000` (`newborn_inbreeding`, `F` as f64,
      exact), 0 without a birth."""
    t = year_end(year)
    on, affected = affected_counts(world, t)
    assert affected == card_affected(world, t), (year, affected)
    out = {"diseases_on": on, "affected": affected}
    for name, member in (("adults", lambda years: years >= ADULT_YEARS), ("age25", lambda years: years == ADULT_YEARS)):
        cells = []
        for sex in (FEMALE, MALE):
            values = [height * 10.0 for _, of, years, height in heights if of == sex and member(years)]
            cells.append([len(values), mean_rounded(values)])
        out[name] = cells
    start, end = year * YEAR, (year + 1) * YEAR
    values = []
    for sim in sorted(set(world.sims) | set(world.archive)):
        if born_in_town(world, sim) and start <= world.born_of(sim) < end:
            f = newborn_inbreeding(world, sim)
            value = float(f)
            assert Fraction(value) == f
            values.append(value * 10_000.0)
    assert len(values) == births, (year, len(values), births)
    out["inbreeding_e4"] = mean_rounded(values)
    return out


def inbreeding_check() -> list:
    """`--inbreeding-check`: `inbreeding_e4` per year of the run "kin_deep" of
    tools/state_reference.py (its world at its end: nobody dies or leaves there, and `F` of a
    birth is fixed by the pedigree at the birth), against the values by hand from its pedigree
    (first cousins 1/16, second cousins 1/64): year 1 one birth 0, year 2 two births 0, year 4
    no birth 0, year 38 three births 625 / 3 → 208, year 39 two births 312.5 → 313, year 59
    two births 156.25 / 2 → 78."""
    import state_reference

    world = state_reference.KINDEEP
    by_year = {}
    for sim in sorted(set(world.sims) | set(world.archive)):
        if born_in_town(world, sim):
            by_year.setdefault(world.born_of(sim) // YEAR, []).append(float(newborn_inbreeding(world, sim)) * 10_000.0)
    rows = {year: (len(values), mean_rounded(values)) for year, values in sorted(by_year.items())}
    by_hand = {1: (1, 0), 2: (2, 0), 4: (0, 0), 38: (3, 208), 39: (2, 313), 59: (2, 78)}
    for year, expected in by_hand.items():
        assert rows.get(year, (0, 0)) == expected, (year, rows.get(year), expected)
    return [f"inbreeding_kin_deep year {year} births {n} inbreeding_e4 {mean} values {by_year[year]}" for year, (n, mean) in rows.items()]


class LogPopulation:
    """The same columns counted from the year's entries of the log (a check of `population`,
    not a definition): `core:birth` per child, `core:arrived` not caused by the input `Start`
    (sequence 1), `core:emigrated` per person, `core:death` per cause. Reads the entries the
    world wrote since the last call."""

    # The payload variants of `core:birth` and `core:death` (tests/golden/log_codes.txt).
    BIRTH, DEATH = 8, 4

    def __init__(self, world: wr.World, causes: list):
        self.world, self.causes, self.read, self.years = world, causes, 0, {}

    def row(self, year: int) -> dict:
        for line in self.world.log[self.read :]:
            c = pool_reference.Cursor(line)
            c.varint()
            t = c.signed()
            for _ in range(c.varint()):
                assert c.varint() == 0, "a subject is a sim"
                c.varint()
            by_start = False
            for _ in range(c.varint()):
                kind = c.varint()
                if kind in (0, 2):
                    value = c.varint()
                    by_start = by_start or (kind == 2 and value == 1)
                elif kind == 1:
                    c.varint()
                    c.signed()
                elif kind == 3:
                    c.string()
                else:
                    assert kind == 4
                    for _ in range(4):
                        c.varint()
            kind = c.varint()
            counts = self.years.setdefault(t // YEAR, {"births": 0, "arrivals": 0, "emigrations": 0, "deaths": [0] * len(self.causes)})
            if kind == self.BIRTH:
                counts["births"] += 1
            elif kind == wr.ARRIVED_VARIANT and not by_start:
                counts["arrivals"] += 1
            elif kind == wr.EMIGRATED_VARIANT:
                counts["emigrations"] += 1
            elif kind == self.DEATH:
                counts["deaths"][self.causes.index(c.string())] += 1
        self.read = len(self.world.log)
        return self.years.get(year, {"births": 0, "arrivals": 0, "emigrations": 0, "deaths": [0] * len(self.causes)})


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
        key = founder_key(A_TAG, index)
        # The founder's genome under F1 (slice S2b): (a) names every locus, so nothing is drawn.
        genome = wr.drawn_founder_genome(data, seed, key, sex, genes)
        assert genome == wr.founder_genome(sex, genes, data.loci)
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
    causes = death_causes(world.data)
    from_log = LogPopulation(world, causes)
    started = time.time()
    world_seconds = census_seconds = 0.0
    for year in range(years):
        mark = time.time()
        world.advance(year_end(year))
        world_seconds += time.time() - mark
        mark = time.time()
        row, curve_held = census_row(evaluator, year, margins)
        census_seconds += time.time() - mark
        # The state's population columns are the year's entries of the log.
        assert {key: row[key] for key in ("births", "arrivals", "emigrations", "deaths")} == from_log.row(year), (name, year)
        rows.append(row)
        if curve_held:
            held[str(year)] = curve_held
        if year in snapshot_years and body.accepted:
            snapshots.append(snapshot(evaluator, year, kind_of))
            if world.pool_start is not None:
                effects += g6_effects(evaluator, year)
    # Seconds of the whole loop, of the world alone and of the census rows (not a pin).
    elapsed = (time.time() - started, world_seconds, census_seconds)
    # `N` read from `Start` is the world's own record of what `Start` made.
    n = start_persons(world)
    assert all((sim <= n) == world.made_by_start(sim) for sim in set(world.sims) | set(world.archive)), name
    record = {
        "data": data_note,
        "data_hash": hexed(world.data.hash),
        "body_data": body_data_label(body),
        "scenario": {"format_version": 5, "start": scenario, "until": year_end(years - 1)},
        "start_input": world.inputs[0].hex(),
        "start_input_hash": hexed(hash64(world.inputs[0])),
        "year_hashes": [hexed(value) for value in world.year_hashes],
        "log_hash": hexed(world.log_hash()),
        "death_causes": causes,
        "diseases": [disease.code for disease in world.data.diseases],
        "rows": rows,
        "curve_clamped_sims": held,
        "margins": margins.json(),
        "snapshots": snapshots,
    }
    if world.pool_start is not None:
        record["pool"] = {"pool_hash": hexed(world.data.pool.hash), "pool_seed": hexed(world.data.pool.seed), "frozen_at": world.data.pool.frozen_at}
        record["g6_effects"] = effects
    # For census.txt only (not in census.json): `N`, the persons `Start` made.
    record["start_persons"] = n
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


# The documents of a pool town (design note of the pool's records, section 6): those its pool
# was extracted from (`arrival_reference.pool_documents`), named in the record.
DOCUMENTS_NOTE = {
    "pool_c": "the documents of father_dies (tools/pool_reference.py: tests/golden/map4_template with data/calibration/demography.json and the scenario's values)",
}


def pool_fixture(pool_name: str, seed: int, n_start: int, immigration: bool, years: int, nutrition, snapshot_years) -> tuple:
    documents = arrival_reference.pool_documents(pool_name)
    # With the frequencies the pool was made with (slice S2a of the switch to map 4).
    data = arrival_reference.town_data(arrival_reference.golden_pool(pool_name), documents, arrival_reference.pool_frequencies(pool_name))
    world = wr.World(seed, arrival_reference.TAG, True, [], (), data, pool=wr.pool_start(data, seed, n_start, immigration), nutrition=nutrition)
    body = BodyCensusData(data, documents)
    assert body.accepted, body.reason
    kind_of = lambda sim: arrival_reference.env_kind(world, sim)
    record, evaluator, elapsed = run_fixture(
        pool_name, world, body, years, snapshot_years, kind_of, scenario_start(world, [], ()),
        DOCUMENTS_NOTE.get(pool_name, "tests/golden/map4_template with data/calibration/demography.json") + f" and the pool tests/golden/pool/{pool_name}.bin",
    )
    record["arrivals"] = [
        {"t": rec["t"] if "t" in rec else None, "year": rec.get("year"), "draw": rec.get("draw")} for rec in world.immigrations
    ]
    return record, evaluator, elapsed


C_FOUNDERS, C_CAPACITY, C_YEARS = 40, 40, 30


def c_inputs() -> tuple:
    """(template, pre-run documents, pre-run data, founders) of fixture (c)."""
    template = prerun_reference.template_documents()
    prerun = prerun_reference.prerun_documents(template)
    data = prerun_reference.prerun_data(prerun)
    table = prerun_reference.weight_table(data)
    count = prerun_reference.counts(table, C_FOUNDERS)
    loci = [locus["id"] for locus in data.measured_loci]
    founder_list = prerun_reference.founders(count, prerun_reference.MEASURE_SEED, prerun_reference.MEASURE_TAG, loci)
    return template, prerun, data, [founder for founder, _ in founder_list]


def c_world(data: wr.Data, founders: list) -> wr.World:
    return wr.World(prerun_reference.MEASURE_SEED, prerun_reference.MEASURE_TAG, True, founders, (), data, capacity=C_CAPACITY)


def fixture_c() -> tuple:
    _, prerun, data, founders = c_inputs()
    out = {}
    for name, reference_alleles in (("c_prerun_small", False), ("c_prerun_small_reference_alleles", True)):
        world = c_world(data, founders)
        body = BodyCensusData(data, prerun, reference_alleles=reference_alleles)
        kind_of = lambda sim, world=world: arrival_reference.env_kind(world, sim)
        record, evaluator, elapsed = run_fixture(
            name, world, body, C_YEARS, (), kind_of, scenario_start(world, founders, ()),
            "the pre-run data of tools/prerun_reference.py (the template without the five disease loci, MC1R, HERC2 and the Mendelian diseases)",
        )
        out[name] = (record, evaluator, elapsed)
    # Since note section 6.1, (c') is (c) with another label.
    rows_c, rows_c2 = out["c_prerun_small"][0]["rows"], out["c_prerun_small_reference_alleles"][0]["rows"]
    assert rows_c == rows_c2 and all(row["body_evaluated"] == row["living"] for row in rows_c)
    return out


# ---------------------------------------------------------------------------------------
# The rule of note section 6.1 checked on its own (`--pigment-check`)

# The person of the check: SimId 44 of (c), a town child of two founders (its `C` from its
# parents' keys), at the year end of year 19 (age about 12.2 years, in its puberty window).
PIGMENT_SIM, PIGMENT_YEAR = 44, 19


def prerun_with_pigment(template: tuple) -> tuple:
    """The pre-run documents with `core:mc1r` and `core:herc2` kept (the template's two
    entries in their places): the template without the five disease loci and the Mendelian
    diseases only."""
    demography, calibration, genome_map = (copy.deepcopy(doc) for doc in template)
    dropped = set(prerun_reference.dropped_loci(calibration)) - set(PIGMENT_LOCI)
    assert len(dropped) == 5, dropped
    genome_map["loci"] = [locus for locus in genome_map["loci"] if locus["id"] not in dropped]
    calibration["diseases"] = [disease for disease in calibration["diseases"] if "locus" not in disease["carrier"]]
    return demography, calibration, genome_map


def rule_cases(prerun: tuple, with_pigment: tuple) -> list:
    """`census_body_check` and `BodyData::new` on the pre-run documents and altered copies:
    (name, what changed, census reason, both absent, strict reason)."""
    template_loci = {locus["id"]: locus for locus in with_pigment[2]["loci"]}
    cases = []

    def case(name: str, change: str, base: tuple, alter, expect_census, expect_strict):
        _, calibration, genome_map = (copy.deepcopy(doc) for doc in base)
        alter(calibration, genome_map)
        reason, absent = census_body_check(calibration, genome_map)
        strict = br.body_data_check((calibration, genome_map), br.BODY_TRAITS_MAP4)
        assert reason == expect_census and strict == expect_strict, (name, reason, strict)
        cases.append({"name": name, "change": change, "census": reason or "accepted", "both_absent": absent, "body_data_new": strict or "accepted"})

    def add(*ids):
        def alter(calibration, genome_map):
            loci = genome_map["loci"] + [copy.deepcopy(template_loci[locus_id]) for locus_id in ids]
            genome_map["loci"] = sorted(loci, key=map4_order)
        return alter

    def set_locus(locus_id, field, value):
        def alter(calibration, genome_map):
            next(locus for locus in genome_map["loci"] if locus["id"] == locus_id)[field] = value
        return alter

    def age_weights(trait):
        def alter(calibration, genome_map):
            row = next(row for row in calibration["traits"] if row["trait"] == trait)
            row["h2_child"]["value"] = row["h2_child"]["value"] + 0.01
        return alter

    def drop_trait_row(trait):
        def alter(calibration, genome_map):
            calibration["traits"] = [row for row in calibration["traits"] if row["trait"] != trait]
        return alter

    none = lambda calibration, genome_map: None
    mc1r_reason = "data.body_traits: core:mc1r is no recessive autosomal locus"
    herc2_reason = "data.body_traits: core:herc2 is no recessive autosomal locus"
    age_reason = "data.body_traits: the weights of core:growth depend on the age"
    row_reason = "data.body_traits: core:muscle has no phenotype row"
    case("prerun", "none: the pre-run documents, both loci absent", prerun, none, None, mc1r_reason)
    case("prerun_mc1r_only", "core:mc1r of the template added", prerun, add("core:mc1r"), herc2_reason, herc2_reason)
    case("prerun_herc2_only", "core:herc2 of the template added", prerun, add("core:herc2"), mc1r_reason, mc1r_reason)
    case("prerun_both", "core:mc1r and core:herc2 of the template added", prerun, add("core:mc1r", "core:herc2"), None, None)
    case("both_mc1r_dominant", "both present, core:mc1r action dominant", with_pigment, set_locus("core:mc1r", "action", "dominant"), mc1r_reason, mc1r_reason)
    case("both_herc2_on_x", "both present, core:herc2 on the chromosome x", with_pigment, set_locus("core:herc2", "chromosome", "x"), herc2_reason, herc2_reason)
    case("herc2_only_dominant", "core:herc2 alone, action dominant", prerun, lambda c, m: (add("core:herc2")(c, m), set_locus("core:herc2", "action", "dominant")(c, m)), mc1r_reason, mc1r_reason)
    case("prerun_growth_weights_age", "both absent, h2_child of core:growth + 0.01", prerun, age_weights("core:growth"), age_reason, age_reason)
    case("prerun_muscle_no_row", "both absent, the phenotype row of core:muscle removed", prerun, drop_trait_row("core:muscle"), row_reason, row_reason)
    return cases


def map4_order(locus: dict) -> tuple:
    return map4_reference.map_order(locus)


def params_bits(evaluation: dict) -> dict:
    return {key: bits(evaluation["params"][key][0]) for key in PARAM_KEYS}


def evaluate_forced(world: wr.World, body: BodyCensusData, sim: int, t: int, mc1r: int, herc2: int) -> tuple:
    """`body_at(sim, t)` on `body`'s data with both counts forced: (the counts read from the
    genome, or None on data without the loci, and the evaluation)."""
    evaluator = Evaluator(world, body)
    parts = evaluator.person_parts(sim)
    read = None if body.pigment_absent else (parts["mc1r"], parts["herc2"])
    parts["mc1r"], parts["herc2"] = mc1r, herc2
    return read, evaluator.evaluate(sim, t)


def timed_c(data: wr.Data, prerun: tuple, founders: list, strict: bool) -> dict:
    """Fixture (c) run once, the census with `BodyData::new` (strict: nobody evaluated, the
    rule before note section 6.1) or with `BodyData::census`: seconds (not a pin)."""
    world = c_world(data, founders)
    body = BodyCensusData(data, prerun, strict=strict)
    assert body.accepted != strict
    kind_of = lambda sim: arrival_reference.env_kind(world, sim)
    record, _, (total, world_seconds, census_seconds) = run_fixture(
        "c", world, body, C_YEARS, (), kind_of, scenario_start(world, founders, ()), "timing only"
    )
    evaluations = sum(row["body_evaluated"] for row in record["rows"])
    return {"rule": "BodyData::new" if strict else "BodyData::census", "total_s": round(total, 2), "world_s": round(world_seconds, 2), "census_rows_s": round(census_seconds, 2), "evaluations": evaluations}


def pigment_check() -> dict:
    """The rule's cases (`rule_cases`) and one living person of (c) (`PIGMENT_SIM` at the year
    end of `PIGMENT_YEAR`) evaluated (1) on the pre-run data by the census (both loci absent,
    0 copies), (2) on the pre-run documents with both loci kept and (3) on the template, both
    accepted by `BodyData::new`, each with both counts forced to 0 (the counts its genome holds
    there are read and reported too); (1) to (3) agree bit for bit in all 44 parameters. (4)
    as (2) with both counts forced to 2: which keys move (only pigment, never a clamped key)."""
    template, prerun, data, founders = c_inputs()
    with_pigment = prerun_with_pigment(template)
    cases = rule_cases(prerun, with_pigment)
    world = c_world(data, founders)
    t = year_end(PIGMENT_YEAR)
    world.advance(t)
    sim = PIGMENT_SIM
    assert sim in world.sims
    census_body = BodyCensusData(data, prerun)
    assert census_body.accepted and census_body.pigment_absent
    evaluator = Evaluator(world, census_body)
    census = evaluator.evaluate(sim, t)
    parts = census["parts"]
    e_seed, c_seed, parents = parts["seeds"]
    person = {
        "fixture": "c_prerun_small",
        "year": PIGMENT_YEAR,
        "t": t,
        "sim": sim,
        "lineage_key": hexed(parts["key"]),
        "kind": arrival_reference.env_kind(world, sim),
        "sex": "female" if parts["sex"] == FEMALE else "male",
        "born": parts["born"],
        "age_secs": census["age"],
        "seed_e": hexed(e_seed),
        "seed_c": hexed(c_seed),
        "c_keys": None if parents is None else [hexed(parents[0]), hexed(parents[1])],
        "g_L": None if parts["g_L"] is None else bits(parts["g_L"]),
        "delta": bits(parts["delta"]),
        "bio_age": bits(census["b"]),
        "nutrition": [[year, index] for year, index in census["nutrition"]],
        "curve_clamped": census["curve"]["clamped"],
        "clamped": [key for key in CLAMPED_KEYS if census["params"][key][1]],
        "z": {trait: bits(parts["z_of"][trait]) for trait in br.BODY_TRAITS_MAP4},
    }
    reference = params_bits(census)
    evaluations = [
        {
            "data": "the pre-run data (BodyData::census: both loci absent, read as 0 copies)",
            "data_hash": hexed(data.hash),
            "mc1r_read": None,
            "herc2_read": None,
            "mc1r": parts["mc1r"],
            "herc2": parts["herc2"],
            "params": reference,
            "params_hash": hexed(hash64(params_bytes(census["params"]))),
        }
    ]
    others = (
        ("the pre-run documents with core:mc1r and core:herc2 kept (BodyData::new)", with_pigment),
        ("the template tests/golden/map4_template with data/calibration/demography.json (BodyData::new)", template),
    )
    for label, documents in others:
        other_data = wr.Data(*documents)
        body = BodyCensusData(other_data, documents, strict=True)
        assert body.accepted and not body.pigment_absent
        read, evaluation = evaluate_forced(world, body, sim, t, 0, 0)
        assert read == (0, 0), read
        values = params_bits(evaluation)
        assert values == reference, [key for key in PARAM_KEYS if values[key] != reference[key]]
        assert all(bits(evaluation["parts"]["z_of"][trait]) == person["z"][trait] for trait in br.BODY_TRAITS_MAP4)
        assert bits(evaluation["parts"]["delta"]) == person["delta"] and bits(evaluation["b"]) == person["bio_age"]
        evaluations.append(
            {
                "data": label,
                "data_hash": hexed(other_data.hash),
                "mc1r_read": read[0],
                "herc2_read": read[1],
                "mc1r": 0,
                "herc2": 0,
                "identical_to_census": True,
                "params_hash": hexed(hash64(params_bytes(evaluation["params"]))),
            }
        )
    other_data = wr.Data(*with_pigment)
    _, twin = evaluate_forced(world, BodyCensusData(other_data, with_pigment, strict=True), sim, t, 2, 2)
    moved = [key for key in PARAM_KEYS if params_bits(twin)[key] != reference[key]]
    assert moved and not set(moved) & set(CLAMPED_KEYS), moved
    twin_entry = {
        "data": others[0][0],
        "mc1r": 2,
        "herc2": 2,
        "moved": {key: [reference[key], params_bits(twin)[key]] for key in moved},
    }
    timing = [timed_c(data, prerun, founders, strict=True), timed_c(data, prerun, founders, strict=False)]
    return {
        "written_by": "tools/census_reference.py --pigment-check (independent of crates/)",
        "rule": "note section 6.1: on data whose map holds neither core:mc1r nor core:herc2 the census evaluates every living body with both allele counts 0; one locus alone, or a present one not recessive on an autosome, still evaluates nobody",
        "rule_cases": cases,
        "param_keys": PARAM_KEYS,
        "person": person,
        "evaluations": evaluations,
        "negative_twin_counts_2": twin_entry,
        "timing_fixture_c_not_a_pin": timing,
    }


# ---------------------------------------------------------------------------------------
# Output


def text_rows(name: str, record: dict) -> list:
    lines = [
        f"census_{name}_config data_hash {record['data_hash']} body_data {record['body_data']} start_input_hash {record['start_input_hash']}"
        f" log_hash {record['log_hash']} until {record['scenario']['until']} nutrition {record['scenario']['start']['world_params']['nutrition']}",
        f"census_{name}_population start_persons {record['start_persons']} death_causes {','.join(record['death_causes'])} diseases {','.join(record['diseases'])}",
    ]
    causes = record["death_causes"]

    def population_text(counts: dict) -> str:
        deaths = " ".join(f"deaths.{code}={counts['deaths'][i]}" for i, code in enumerate(causes))
        return f"births {counts['births']} arrivals {counts['arrivals']} emigrations {counts['emigrations']} {deaths}"

    def inheritance_text(row: dict) -> str:
        affected = " ".join(f"affected.{code}={row['affected'][i]}" for i, code in enumerate(record["diseases"]))
        # The names of the CLI's header (ADR-0021, addendum: `adults_female`, `adult_height_mm_female`, ...).
        cells = " ".join(
            f"{group}_{sex} {row[group][s][0]} {mean}_height_mm_{sex} {row[group][s][1]}"
            for group, mean in (("adults", "adult"), ("age25", "age25"))
            for s, sex in enumerate(("female", "male"))
        )
        return f"diseases_on {row['diseases_on']} {affected} {cells} inbreeding_e4 {row['inbreeding_e4']}"

    for row in record["rows"]:
        counts = " ".join(f"{key}={row['clamped'][i]}" for i, key in enumerate(CLAMPED_KEYS))
        lines.append(
            f"census_{name}_row year {row['year']} living {row['living']} lived_secs {row['lived_secs']} body_evaluated {row['body_evaluated']} {counts} curve_clamped {row['curve_clamped']}"
            f" {population_text(row)} {inheritance_text(row)}"
        )
    totals = [sum(row["clamped"][i] for row in record["rows"]) for i in range(len(CLAMPED_KEYS))]
    population_totals = {key: sum(row[key] for row in record["rows"]) for key in ("births", "arrivals", "emigrations")}
    population_totals["deaths"] = [sum(row["deaths"][i] for row in record["rows"]) for i in range(len(causes))]
    lines.append(
        f"census_{name}_totals evaluations {sum(row['body_evaluated'] for row in record['rows'])} curve_clamped {sum(row['curve_clamped'] for row in record['rows'])} "
        + " ".join(f"{key}={totals[i]}" for i, key in enumerate(CLAMPED_KEYS))
        + f" {population_text(population_totals)}"
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
    documents = arrival_reference.pool_documents(pool_name)
    # With the frequencies the pool was made with (slice S2a of the switch to map 4).
    data = arrival_reference.town_data(arrival_reference.golden_pool(pool_name), documents, arrival_reference.pool_frequencies(pool_name))
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
    out = {key: value for key, value in record.items() if key not in ("scenario", "start_input", "snapshots", "founder_plan", "start_persons")}
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
    if "--pigment-check" in argv:
        out = Path(argv[argv.index("--out") + 1]) if "--out" in argv else DEFAULT_OUT
        out.mkdir(parents=True, exist_ok=True)
        document = pigment_check()
        (out / "pigment_check.json").write_text(json.dumps(document, indent=1) + "\n")
        for case in document["rule_cases"]:
            print(f"pigment_rule {case['name']} census {case['census']} both_absent {case['both_absent']} new {case['body_data_new']}")
        for evaluation in document["evaluations"]:
            print(f"pigment_person data_hash {evaluation['data_hash']} params_hash {evaluation['params_hash']} read {evaluation['mc1r_read']},{evaluation['herc2_read']}")
        print(f"pigment_twin moved {sorted(document['negative_twin_counts_2']['moved'])}")
        for entry in document["timing_fixture_c_not_a_pin"]:
            print(f"pigment_timing {entry}")
        return 0
    if "--inbreeding-check" in argv:
        for row in inbreeding_check():
            print(row)
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
    for name, (seconds, world_seconds, census_seconds) in timings.items():
        print(f"census_timing {name} {seconds:.2f} s, world {world_seconds:.2f} s, census rows {census_seconds:.2f} s (not a pin)", file=sys.stderr)
    print(f"wrote {out}/census.json, census_details.json, census.txt, scenarios/")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

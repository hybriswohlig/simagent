#!/usr/bin/env python3
"""The mapping of a person onto the body contract (genetics specification 10; ADR-0005,
ADR-0015) with the names of map 4 (ADR-0020 section 1; design note of the template of map
4, section 3), and its test map `tests/golden/data_body`, written from the specification
and the ADRs, independent of the Rust code.

Map 4 changes what the mapping reads (ADR-0020 section 1, changing ADR-0015 section 2):
`z_h` is the phenotype of `core:growth` (no trait `core:height`), `eyebrow_density` reads
`z` of `core:hair_density` (no `core:brow`), `beard_coverage` reads `z` of `core:beard` (no
`core:beard_coverage`). `BodyData::new` requires the 35 traits of `BODY_TRAITS_MAP4`
(present, with a phenotype row whose child and adult weights are equal bit for bit) and
`core:mc1r` and `core:herc2` as recessive loci on an autosome (`data.body_traits`).

The test map (`tests/golden/data_body`, ADR-0015 section 5): built first as it stood with
the names of map 3 (`fixture_map3`: its 37 traits each with one own additive locus, `p` =
0.5, `d_ref` 1, `sigma_ref` sqrt(0.5), `core:pigment` exclusive; the loci of
`BODY_TRAITS_MAP3` in that order from autosome 1 slot 0 on, then MC1R and HERC2; the
calibration the repository's without diseases and with the weights of 10.7), whose data
hash is asserted to be that of the file before this change (`MAP3_DATA_HASH`); then
changed as the note says (`fixture_map4`): `core:height` renamed `core:growth` in the
trait, its loads and its phenotype row; `core:brow` and `core:beard_coverage` removed with
their loci and rows. Nothing else changes (the locus keeps its ID `core:height_locus` and
its place, every other locus its place, `map_version` stays 2, the texts stay). The files
are JSON with an indent of two as the old ones (`json.dumps`).

The mapping (`body_params`), each formula of 10 with the numbers of the calibration
(`core:body_*`, ADR-0015 section 1), left to right as written, `exp` correctly rounded
(tools/kat_reference.py), `L(u) = 1 / (1 + exp(−u))`, `S` the smoothstep of 5.1
(`world_reference.smoothstep`), `min` and `max` plain:

- `body_form_age = face_form_age = min(b, 130)`.
- `height_cm = ((κ_i · H(x; θ_i)) − v_i(x)) − loss(b)` with `x = as_years(min(age, 25
  years))`; the individual curve of ADR-0012 (`z_h`, `z_p` held on [−6, 6], `θ_i = θ + σ_θ
  · z_p`, `h1_i = (h1 + σ · z_h) + γ · (θ_i − θ)`, `κ_i = h1_i / h1`), the mean curve of
  ADR-0010 section 3 (`q = (L24 − L12) / (L12 − L0)`, `c = −ln(q)`, `B = (L12 − L0) / (1 −
  q)`; up to 2 years `L0 + B · (1 − exp(−c · t))`, from 3 years `h1 − 2 · gap / (exp(s0 ·
  (t − θ_i)) + exp(s1 · (t − θ_i)))` with `gap = (h1 − hθ) · exp(−s0 · (θ_i − θ))`, between
  `(1 − w) · I + w · PB`, `w = 3 · (u · u) − 2 · ((u · u) · u)`, `u = t − 2`), the loss of
  nutrition of ADR-0013 section 4, the loss in old age of ADR-0014 section 2 (`(a · (b −
  b0)) · (b − b0)` for `b > b0`, else 0).
- The `z` parameters: `z` of their trait (10.3).
- Pigment, development, hair, skin and the signs of age as 10.4 to 10.6 write them
  (`raw_values`).
- `z` and `cm` held on the range of the contract and marked; every other value must lie in
  its range and every value must be finite (ADR-0005 section 1).

The inputs (`inputs_of`, ADR-0015 section 3): the phenotypes of the traits read, at the age
0 (`world_reference.phenotype`; the weights do not depend on the age), and the copies of
allele 1 at MC1R and HERC2. `b = bio_age(x, Δ, w)` of 7.4 (`25 + (x − 25) · exp((w · Δ) /
50)` after 25).

Prints the data hashes, the layout, the acceptance of `BodyData::new`, the constants of the
mean curves, and per pinned person its inputs, the `z` it reads, its 44 parameters (map 4)
and every parameter whose bits change from the names of map 3 to those of map 4, with the
reason.

Usage: .venv/bin/python tools/body_reference.py           (write tests/golden/data_body)
       .venv/bin/python tools/body_reference.py --check   (compare, write nothing)
"""

import copy
import json
import math
import sys
from pathlib import Path

import data_reference
from kat_reference import exp_reference, ln_reference
from rng_reference import str_key
from world_reference import (
    FEMALE,
    MALE,
    YEAR,
    PhenotypeTrait,
    as_years,
    copies_at,
    founder_genome,
    phenotype,
    smoothstep,
)

ROOT = Path(__file__).resolve().parent.parent
# The base of the fixture: map 3 and its calibration, the map 3 fixture tests/golden/map3 (data/
# held them before the switch to map 4, byte for byte; design note of the switch, section 4).
# The life course data stay the repository's (data/calibration/demography.json).
REPOSITORY = ROOT / "tests" / "golden" / "map3"
LIFE_COURSE = ROOT / "data" / "calibration" / "demography.json"
FIXTURE = ROOT / "tests" / "golden" / "data_body"
CONTRACT = ROOT / "contracts" / "body.json"

# The data hash of tests/golden/data_body before this change (names of map 3): the
# fixture as `fixture_map3` builds it must give exactly this.
MAP3_DATA_HASH = 0x8DD9852B584C748F

FACES = [f"core:face_{n:02d}" for n in range(1, 13)]
# The traits the mapping reads with the names of map 3, in the order of the loci of the
# test map (ADR-0015 section 2: `z_h`, `z_p`, the traits of 10.3, the traits of 10.1).
BODY_TRAITS_MAP3 = [
    "core:height",
    "core:puberty",
    "core:leg_ratio",
    "core:head_size",
    "core:body_fat",
    "core:fat_distribution",
    "core:muscle",
    "core:shoulder_width",
    "core:chest_depth",
    "core:hip_width",
    *FACES,
    "core:pigment",
    "core:hair_darkness",
    "core:androgen",
    "core:estrogen",
    "core:breast",
    "core:curl",
    "core:freckle",
    "core:brow",
    "core:beard",
    "core:beard_coverage",
    "core:body_hair",
    "core:bald",
    "core:hair_density",
    "core:grey",
    "core:skin_age",
]
RENAMED = {"core:height": "core:growth"}
REMOVED = ("core:brow", "core:beard_coverage")
BODY_TRAITS_MAP4 = [RENAMED.get(trait, trait) for trait in BODY_TRAITS_MAP3 if trait not in REMOVED]
assert len(BODY_TRAITS_MAP3) == 37 and len(BODY_TRAITS_MAP4) == 35

# What each input of the mapping reads (genetics specification 10.1 to 10.3): the role and
# its trait with the names of map 3 and of map 4.
ROLES = {
    "z_h": ("core:height", "core:growth"),
    "z_p": ("core:puberty", "core:puberty"),
    "z_pig": ("core:pigment", "core:pigment"),
    "z_hair": ("core:hair_darkness", "core:hair_darkness"),
    "z_and": ("core:androgen", "core:androgen"),
    "z_est": ("core:estrogen", "core:estrogen"),
    "z_breast": ("core:breast", "core:breast"),
    "z_curl": ("core:curl", "core:curl"),
    "z_freckle": ("core:freckle", "core:freckle"),
    "z_brow": ("core:brow", "core:hair_density"),
    "z_beard": ("core:beard", "core:beard"),
    "z_cov": ("core:beard_coverage", "core:beard"),
    "z_body": ("core:body_hair", "core:body_hair"),
    "z_bald": ("core:bald", "core:bald"),
    "z_den": ("core:hair_density", "core:hair_density"),
    "z_grey": ("core:grey", "core:grey"),
    "z_skin": ("core:skin_age", "core:skin_age"),
    "leg_ratio_z": ("core:leg_ratio", "core:leg_ratio"),
    "head_size_z": ("core:head_size", "core:head_size"),
    "body_fat_z": ("core:body_fat", "core:body_fat"),
    "fat_distribution_z": ("core:fat_distribution", "core:fat_distribution"),
    "muscle_z": ("core:muscle", "core:muscle"),
    "shoulder_width_z": ("core:shoulder_width", "core:shoulder_width"),
    "chest_depth_z": ("core:chest_depth", "core:chest_depth"),
    "hip_width_z": ("core:hip_width", "core:hip_width"),
    **{f"face_axis_{n:02d}": (face, face) for n, face in enumerate(FACES, start=1)},
}
for _names, _list in ((0, BODY_TRAITS_MAP3), (1, BODY_TRAITS_MAP4)):
    assert sorted({names[_names] for names in ROLES.values()}) == sorted(_list)

# The fixture's texts (as they stand in the file before this change).
TRAIT_TEXT = "A trait of the body mapping with one own locus (ADR-0015 fixture)."
ROW_SOURCE, ROW_AS_OF, ROW_NOTE = "docs/genetik-spezifikation.md 4.2 and 10.7", "2026-10-08", "test fixture of the body mapping (ADR-0015)"
DISEASE_AR_TEXT = "The mendelian locus of the walking skeleton (step 5); the hereditary diseases of step 9 replace it."
PIGMENT_TEXT = "A large recessive pigment locus (genetics specification 10.4)."
# The weights of 10.7 (h² child, c² child, h² adult, c² adult) in the test map.
ROW_WEIGHTS = {
    "core:height": (0.8, 0.05, 0.8, 0.05),
    "core:pigment": (0.8, 0.0, 0.8, 0.0),
    "core:puberty": (0.6, 0.1, 0.6, 0.1),
    "core:body_fat": (0.5, 0.1, 0.5, 0.1),
    **{face: (0.7, 0.0, 0.7, 0.0) for face in FACES},
}
OTHER_WEIGHTS = (0.6, 0.0, 0.6, 0.0)


def fixture_map3() -> tuple:
    """The test map with the names of map 3 (as the file stood before this change): the
    calibration and the genome map."""
    loci = [
        {
            "id": "core:disease_ar",
            "chromosome": "autosome",
            "segment": 0,
            "slot": 0,
            "action": "recessive",
            "mark": "disease",
            "mutation": "to_alt",
            "description": DISEASE_AR_TEXT,
        }
    ]
    for index, trait in enumerate(BODY_TRAITS_MAP3):
        loci.append(
            {
                "id": f"{trait}_locus",
                "chromosome": "autosome",
                "segment": 1 + index // 24,
                "slot": index % 24,
                "action": {"additive": {"effect": 1.0, "d_ref": 1.0, "loads": [{"trait": trait, "weight": 1.0}]}},
                "mark": "small",
                "mutation": "both",
                "description": f"The own locus of {trait}.",
            }
        )
    for locus_id, slot in (("core:mc1r", 13), ("core:herc2", 14)):
        loci.append(
            {
                "id": locus_id,
                "chromosome": "autosome",
                "segment": 2,
                "slot": slot,
                "action": "recessive",
                "mark": "large",
                "mutation": "both",
                "description": PIGMENT_TEXT,
            }
        )
    genome_map = {
        "format_version": 2,
        "map_version": 2,
        "traits": [
            {"id": trait, "sigma_ref": math.sqrt(0.5), "exclusive": trait == "core:pigment", "includes": [], "description": TRAIT_TEXT}
            for trait in sorted(BODY_TRAITS_MAP3)
        ],
        "loci": loci,
    }
    base = json.loads((REPOSITORY / "calibration" / "genetics.json").read_text())
    calibration = copy.deepcopy(base)
    calibration["diseases"] = []
    calibration["traits"] = [weights_row(trait) for trait in sorted(BODY_TRAITS_MAP3)]
    assert calibration["sex_differences"] == [] and all(calibration[field] is None for field in data_reference.NAMED_TRAITS)
    return calibration, genome_map


def weights_row(trait: str) -> dict:
    row = {"trait": trait}
    for name, value in zip(data_reference.WEIGHTS, ROW_WEIGHTS.get(trait, OTHER_WEIGHTS)):
        row[name] = {"value": value, "unit": "fraction", "source": ROW_SOURCE, "as_of": ROW_AS_OF, "note": ROW_NOTE}
    return row


def fixture_map4(map3: tuple, rename_locus: bool = False) -> tuple:
    """The test map changed to the names of map 4 (design note section 3): `core:height`
    renamed `core:growth` (trait, loads, phenotype row), `core:brow` and
    `core:beard_coverage` removed with their loci and rows; the traits and rows sorted again.
    `rename_locus` (the rejected alternative of decision 5 of the notes): the locus also
    renamed `core:growth_locus` with its text."""
    calibration, genome_map = copy.deepcopy(map3[0]), copy.deepcopy(map3[1])
    traits = []
    for entry in genome_map["traits"]:
        if entry["id"] in REMOVED:
            continue
        entry["id"] = RENAMED.get(entry["id"], entry["id"])
        traits.append(entry)
    genome_map["traits"] = sorted(traits, key=lambda entry: entry["id"])
    loci = []
    for locus in genome_map["loci"]:
        action = locus["action"]
        if isinstance(action, dict):
            loads = action["additive"]["loads"]
            if any(load["trait"] in REMOVED for load in loads):
                continue
            for load in loads:
                load["trait"] = RENAMED.get(load["trait"], load["trait"])
            if rename_locus and locus["id"] == "core:height_locus":
                locus["id"], locus["description"] = "core:growth_locus", "The own locus of core:growth."
        loci.append(locus)
    genome_map["loci"] = loci
    rows = []
    for row in calibration["traits"]:
        if row["trait"] in REMOVED:
            continue
        row["trait"] = RENAMED.get(row["trait"], row["trait"])
        rows.append(row)
    calibration["traits"] = sorted(rows, key=lambda row: row["trait"])
    return calibration, genome_map


def text(doc: dict) -> str:
    return json.dumps(doc, indent=2, ensure_ascii=False) + "\n"


def documents_hash(fixture: tuple) -> int:
    demography = json.loads(LIFE_COURSE.read_text())
    return data_reference.documents_hash(demography, fixture[0], fixture[1])


def body_data_check(fixture: tuple, traits_read: list) -> str:
    """`BodyData::new` (ADR-0015 section 2 with the traits of `traits_read`): None if it
    accepts, else the first reason (`data.body_traits`): first the traits
    (`body_traits_check`), then `core:mc1r` and `core:herc2`."""
    reason = body_traits_check(fixture, traits_read)
    if reason is not None:
        return reason
    _, genome_map = fixture
    loci = {locus["id"]: locus for locus in genome_map["loci"]}
    for locus_id in ("core:mc1r", "core:herc2"):
        locus = loci.get(locus_id)
        if locus is None or locus["action"] != "recessive" or locus["chromosome"] != "autosome":
            return f"data.body_traits: {locus_id} is no recessive autosomal locus"
    return None


def body_traits_check(fixture: tuple, traits_read: list) -> str:
    """The traits part of `BodyData::new`: every trait of `traits_read` in the map with a
    phenotype row whose child and adult weights are equal bit for bit. None if they are,
    else the first reason (`data.body_traits`)."""
    calibration, genome_map = fixture
    in_map = {entry["id"] for entry in genome_map["traits"]}
    rows = {row["trait"]: row for row in calibration["traits"]}
    for trait in traits_read:
        if trait not in in_map:
            return f"data.body_traits: {trait} is no trait of the map"
        if trait not in rows:
            return f"data.body_traits: {trait} has no phenotype row"
        row = rows[trait]
        for child, adult in (("h2_child", "h2_adult"), ("c2_child", "c2_adult")):
            if data_reference.f64_bits(float(row[child]["value"])) != data_reference.f64_bits(float(row[adult]["value"])):
                return f"data.body_traits: the weights of {trait} depend on the age"
    return None


# ---------------------------------------------------------------------------------------
# The mapping (genetics specification 10).


def exp(x: float) -> float:
    return exp_reference(x)[1]


def logistic(u: float) -> float:
    """`L(u) = 1 / (1 + exp(−u))` (10.1)."""
    return 1.0 / (1.0 + exp(-u))


def bio_age(x: float, delta: float, w: float) -> float:
    """`bio_age` of 7.4: `x` up to 25 years, then `25 + (x − 25) · ρ`, `ρ = exp((w · Δ) /
    50)`."""
    if x <= 25.0:
        return x
    return 25.0 + (x - 25.0) * exp((w * delta) / 50.0)


class BodyValues:
    """The numbers the mapping reads from the calibration (parsed, text stripped)."""

    def __init__(self, calibration: dict):
        values = {key: float(entry["value"]) for key, entry in calibration["values"].items()}
        self.v = values
        self.curve = {}
        for sex, name in ((FEMALE, "female"), (MALE, "male")):
            l0, l12, l24 = (values[f"core:length_{when}_{name}"] for when in ("birth", "12_months", "24_months"))
            q = (l24 - l12) / (l12 - l0)
            c = -ln_reference(q)[1]
            b = (l12 - l0) / (1.0 - q)
            self.curve[sex] = {
                "h1": values[f"core:height_adult_{name}"],
                "theta": values[f"core:growth_theta_{name}"],
                "h_theta": values[f"core:growth_height_theta_{name}"],
                "s0": values[f"core:growth_s0_{name}"],
                "s1": values[f"core:growth_s1_{name}"],
                "l0": l0,
                "q": q,
                "c": c,
                "b": b,
                "sd": values[f"core:height_sd_{name}"],
                "theta_sd": values[f"core:growth_theta_sd_{name}"],
                "loss": values[f"core:height_loss_{name}"],
            }
        self.gamma = values["core:growth_gamma"]
        self.loss_onset = values["core:height_loss_onset"]
        self.bio_age_weight = values["core:bio_age_weight"]

    def __getitem__(self, name: str) -> float:
        return self.v[f"core:body_{name}"]


CURVE_INPUT_BOUND = 6.0


def held(z: float) -> tuple:
    """`z` held on [−6, 6] (ADR-0012 section 1) and whether it was held (exactly ±6 is not)."""
    if z > CURVE_INPUT_BOUND:
        return CURVE_INPUT_BOUND, True
    if z < -CURVE_INPUT_BOUND:
        return -CURVE_INPUT_BOUND, True
    return z, False


def individual_curve(values: BodyValues, sex: bytes, z_h: float, z_p: float) -> dict:
    """`IndividualCurve::new` (ADR-0012 section 3)."""
    curve = values.curve[sex]
    zh, held_h = held(z_h)
    zp, held_p = held(z_p)
    theta_i = curve["theta"] + curve["theta_sd"] * zp
    h1_i = (curve["h1"] + curve["sd"] * zh) + values.gamma * (theta_i - curve["theta"])
    kappa = h1_i / curve["h1"]
    return {"theta_i": theta_i, "h1_i": h1_i, "kappa": kappa, "clamped": held_h or held_p}


def mean_height(values: BodyValues, sex: bytes, t: float, theta_i: float) -> float:
    """`H(t; θ_i)` (ADR-0010 section 3)."""
    curve = values.curve[sex]

    def infant() -> float:
        return curve["l0"] + curve["b"] * (1.0 - exp(-curve["c"] * t))

    def preece_baines() -> float:
        gap = (curve["h1"] - curve["h_theta"]) * exp(-curve["s0"] * (theta_i - curve["theta"]))
        return curve["h1"] - 2.0 * gap / (exp(curve["s0"] * (t - theta_i)) + exp(curve["s1"] * (t - theta_i)))

    if t <= 2.0:
        return infant()
    if t >= 3.0:
        return preece_baines()
    u = t - 2.0
    w = 3.0 * (u * u) - 2.0 * ((u * u) * u)
    return (1.0 - w) * infant() + w * preece_baines()


def nutrition_loss(values: BodyValues, nutrition: list, born: int, age_seconds: int, theta_i: float) -> float:
    """`v_i` (ADR-0013 section 4) over the years of `nutrition` ((year, N), ascending) with
    `N < 1`: per year the cut of `[year · YEAR, (year + 1) · YEAR)` with `[born, born + age)`
    as ages `[a0, a1)`, the windows in their order, times `(1 − N)`; the sum times `k`."""
    v = values.v
    e1, e2 = v["core:nutrition_age_infant"], v["core:nutrition_age_child"]
    windows = (
        (v["core:nutrition_weight_infant"], 0.0, e1),
        (v["core:nutrition_weight_child"], e1, e2),
        (v["core:nutrition_weight_puberty"], max(e2, theta_i - v["core:nutrition_puberty_before"]), theta_i + v["core:nutrition_puberty_after"]),
    )
    total = 0.0
    for year, index in nutrition:
        if not index < 1.0:
            continue
        start, end = max(year * YEAR, born), min((year + 1) * YEAR, born + age_seconds)
        if end <= start:
            continue
        a0, a1 = as_years(start - born), as_years(end - born)
        integral = 0.0
        for weight, lo, hi in windows:
            integral = integral + weight * max(0.0, min(a1, hi) - max(a0, lo))
        total = total + integral * (1.0 - index)
    return v["core:nutrition_loss"] * total


def old_age_loss(values: BodyValues, sex: bytes, b: float) -> float:
    """The loss in old age (ADR-0014 section 2): `(a · (b − b0)) · (b − b0)` for `b > b0`."""
    b0 = values.loss_onset
    if not b > b0:
        return 0.0
    return (values.curve[sex]["loss"] * (b - b0)) * (b - b0)


def s_curve(start: float, end: float, t: float) -> float:
    """`S(a, c, t)` of 10.1: the smoothstep of 5.1 from `a` to `c`; here `a` is always 0
    (no value, ADR-0015 section 1), so `u = t / c`."""
    assert start == 0.0
    return smoothstep(t / end)


def raw_values(values: BodyValues, person: dict) -> dict:
    """The raw values of 10.2 to 10.6 for `person` (its inputs: sex, age in seconds, `b`,
    born, nutrition, `z` by role, the copies at MC1R and HERC2)."""
    sex, b, z = person["sex"], person["b"], person["z"]
    men = sex == MALE
    curve = individual_curve(values, sex, z["z_h"], z["z_p"])
    theta_i = curve["theta_i"]
    x_seconds = min(person["age"], 25 * YEAR)
    x = as_years(x_seconds)
    raw = {}
    raw["body_form_age"] = min(b, 130.0)
    raw["face_form_age"] = min(b, 130.0)
    height = curve["kappa"] * mean_height(values, sex, x, theta_i)
    height = height - nutrition_loss(values, person["nutrition"], person["born"], x_seconds, theta_i)
    raw["height_cm"] = height - old_age_loss(values, sex, b)
    for key in ("leg_ratio_z", "head_size_z", "body_fat_z", "fat_distribution_z", "muscle_z", "shoulder_width_z", "chest_depth_z", "hip_width_z"):
        raw[key] = z[key]
    for n in range(1, 13):
        raw[f"face_axis_{n:02d}"] = z[f"face_axis_{n:02d}"]
    # 10.4 pigment.
    z_pig = z["z_pig"]
    mc1r, herc2 = person["mc1r"], person["herc2"]
    eta_s = values["pigment_skin_offset"] + values["pigment_skin_slope"] * z_pig
    eta_h = (values["pigment_hair_slope"] * z_pig + values["pigment_hair_residual"] * z["z_hair"]) - values["pigment_hair_darkening"] * (
        1.0 - s_curve(0.0, values["pigment_hair_darkening_age"], b)
    )
    eta_i = (values["pigment_iris_offset"] + values["pigment_iris_slope"] * z_pig) - values["pigment_iris_darkening"] * (
        1.0 - s_curve(0.0, values["pigment_iris_darkening_age"], b)
    )
    skin_eu, hair_eu, iris = logistic(eta_s), logistic(eta_h), logistic(eta_i)
    skin_pheo = logistic(values["pigment_skin_pheo_offset"] + values["pigment_skin_pheo_slope"] * z_pig)
    hair_pheo = logistic(values["pigment_hair_pheo_offset"] + values["pigment_hair_pheo_slope"] * z_pig)
    if mc1r == 2:
        hair_eu = hair_eu * values["mc1r_hair_eu"]
        skin_eu = skin_eu * values["mc1r_skin_eu"]
        hair_pheo = values["mc1r_hair_pheo"]
        skin_pheo = values["mc1r_skin_pheo"]
    if herc2 == 2:
        iris = iris * values["herc2_iris"]
    raw["skin_eumelanin"], raw["skin_pheomelanin"] = skin_eu, skin_pheo
    raw["hair_eumelanin"], raw["hair_pheomelanin"] = hair_eu, hair_pheo
    raw["iris_melanin"] = iris
    # 10.6 hair.
    raw["hair_curl"] = logistic(values["curl_offset"] + values["curl_slope"] * z["z_curl"])
    raw["hair_density"] = logistic(values["hair_density_offset"] + z["z_den"]) * (
        1.0 - values["hair_density_loss"] * logistic((b - values["hair_density_center"]) / values["hair_density_tempo"])
    )
    if men:
        stage = values["bald_men_stage"] * logistic(
            (b - ((values["bald_men_center"] - values["bald_men_slope"] * z["z_bald"]) - values["bald_men_androgen"] * z["z_and"])) / values["bald_men_tempo"]
        )
    else:
        stage = values["bald_women_stage"] * logistic(
            (b - (values["bald_women_center"] - values["bald_women_slope"] * z["z_bald"])) / values["bald_women_tempo"]
        )
    raw["balding_stage"] = stage
    raw["hair_grey_fraction"] = logistic((b - (values["grey_center"] - values["grey_slope"] * z["z_grey"])) / values["grey_tempo"])
    raw["eyebrow_density"] = logistic(z["z_brow"])
    if men:
        g = logistic(((b - theta_i) - values["beard_lag"]) / values["beard_tempo"])
        raw["beard_density"] = g * logistic((values["beard_offset"] + z["z_beard"]) + values["beard_androgen"] * z["z_and"])
        raw["beard_coverage"] = g * logistic(z["z_cov"])
    else:
        raw["beard_density"], raw["beard_coverage"] = 0.0, 0.0
    g = logistic(((b - theta_i) - values["body_hair_lag"]) / values["body_hair_tempo"])
    body_hair = g * logistic(z["z_body"] + values["body_hair_androgen"] * z["z_and"])
    raw["body_hair"] = body_hair if men else values["body_hair_women"] * body_hair
    raw["freckle_density"] = logistic(
        ((values["freckle_offset"] + values["freckle_slope"] * z["z_freckle"]) + values["freckle_pigment"] * z_pig) + values["mc1r_freckle"] * float(mc1r)
    )
    # 10.6 skin and the signs of age.
    raw["face_wrinkles"] = logistic(((b - values["wrinkles_center"]) + values["wrinkles_slope"] * z["z_skin"]) / values["wrinkles_tempo"])
    raw["skin_laxity"] = logistic(((b - values["laxity_center"]) + values["laxity_slope"] * z["z_skin"]) / values["laxity_tempo"])
    raw["age_spots"] = logistic(
        (((b - values["spots_center"]) + values["spots_skin"] * z["z_skin"]) - values["spots_pigment"] * z_pig) / values["spots_tempo"]
    )
    raw["posture_stoop"] = logistic(((b - values["stoop_center"]) + values["stoop_skin"] * z["z_skin"]) / values["stoop_tempo"])
    # 10.5 development.
    lag, lead = values["development_androgen_lag"], values["development_estrogen_lead"]
    tempo = values["development_tempo"]
    s_a = logistic(((b - theta_i) - lag) / tempo)
    s_e = logistic(((b - theta_i) + lead) / tempo)
    s_b = logistic(((b - theta_i) + values["development_breast_lead"]) / tempo)
    androgen = logistic(values["development_androgen_offset"] + z["z_and"])
    estrogen = logistic(values["development_estrogen_offset"] + z["z_est"])
    if men:
        raw["androgenic_development"] = s_a * androgen
        raw["estrogenic_development"] = values["development_estrogen_men"] * s_e * estrogen
        raw["breast_development"] = 0.0
    else:
        raw["androgenic_development"] = values["development_androgen_women"] * s_a * androgen
        raw["estrogenic_development"] = s_e * estrogen
        raw["breast_development"] = s_b * logistic(z["z_breast"])
    return raw, curve


def body_params(values: BodyValues, contract: list, person: dict) -> dict:
    """`body_params` (ADR-0005 section 1, ADR-0015 section 3): `b` finite and at least 0;
    the raw values, each finite; `z` and `cm` held on the contract's range and marked; every
    other value inside its range. Returns {key: (value, clamped)} in the contract's order and
    the individual curve."""
    assert math.isfinite(person["b"]) and person["b"] >= 0.0, "body_input_age"
    raw, curve = raw_values(values, person)
    out = {}
    for entry in contract:
        key, unit, (low, high) = entry["key"], entry["unit"], entry["range"]
        value = raw[key]
        assert math.isfinite(value), f"body_param_finite {key}"
        clamped = False
        if unit in ("z", "cm"):
            if value < low:
                value, clamped = float(low), True
            elif value > high:
                value, clamped = float(high), True
        else:
            assert low <= value <= high, f"body_param_bounded {key} {value}"
        out[key] = (value, clamped)
    assert set(out) == set(raw)
    return out, curve


def spec_checks(values: BodyValues, contract: list) -> list:
    """The rounded values 10.4 to 10.6 print, from `body_params` of a person with every `z`
    0 (and `z_pig` 2 for the second skin value), no MC1R or HERC2 copy, `θ_i = θ`: pigment
    at 30 years skin 0.23, hair 0.50, iris 0.62, skin 0.75 at `z_pig` = 2; androgenic
    development of adults 0.82 (men) and 0.12 (women), breast 0.5; balding of the mean man
    at 50 about 1.38 (`7 · L(−1.4)`); curl about 0.20; and the adult height of `z` = 0 at
    25 years near `h1` (6.1)."""

    def evaluate(sex, years, **overrides):
        z = {role: 0.0 for role in ROLES}
        z.update(overrides)
        person = {"sex": sex, "age": years * YEAR, "b": float(years), "born": -years * YEAR, "nutrition": [], "z": z, "mc1r": 0, "herc2": 0}
        return {key: value for key, (value, _) in body_params(values, contract, person)[0].items()}

    woman, man = evaluate(FEMALE, 30), evaluate(MALE, 30)
    checks = [
        ("skin_eumelanin z_pig 0", woman["skin_eumelanin"], 0.23, 0.005),
        ("hair_eumelanin z_pig 0", woman["hair_eumelanin"], 0.50, 0.005),
        ("iris_melanin z_pig 0", woman["iris_melanin"], 0.62, 0.005),
        ("skin_eumelanin z_pig 2", evaluate(FEMALE, 30, z_pig=2.0)["skin_eumelanin"], 0.75, 0.005),
        ("androgenic_development men", man["androgenic_development"], 0.82, 0.005),
        ("androgenic_development women", woman["androgenic_development"], 0.12, 0.005),
        ("breast_development women", woman["breast_development"], 0.5, 0.005),
        ("balding_stage men 50", evaluate(MALE, 50)["balding_stage"], 1.38, 0.005),
        ("hair_curl", woman["hair_curl"], 0.20, 0.005),
        ("height_cm men 25", evaluate(MALE, 25)["height_cm"], values.curve[MALE]["h1"], 0.5),
        ("height_cm women 25", evaluate(FEMALE, 25)["height_cm"], values.curve[FEMALE]["h1"], 0.5),
    ]
    lines = []
    for name, value, want, tolerance in checks:
        assert abs(value - want) <= tolerance, (name, value, want)
        lines.append(f"body_spec_check {name} {value!r} (the specification prints {want!r}, within {tolerance!r})")
    return lines


# ---------------------------------------------------------------------------------------
# The pinned persons.

SEED = 0xFF


def doses(index: int, person_index: int) -> list:
    """The two copies (maternal, paternal) at the own locus of the trait at `index` of
    `BODY_TRAITS_MAP3` for the person at `person_index`: a fixed pattern that gives every
    dose 0, 1, 2 across the traits."""
    k = index * 7 + person_index * 5
    return [(k % 4) // 2, k % 2]


# (name, sex, lineage key, parents' keys or None, age in seconds, Δ, nutrition, MC1R, HERC2)
PERSONS = [
    ("woman_30", FEMALE, 0x1001, None, 30 * YEAR, 0.0, [], [1, 1], [1, 0]),
    ("man_30", MALE, 0x1002, None, 30 * YEAR, 0.0, [], [0, 1], [1, 1]),
    ("boy_14", MALE, 0x1003, (0x1001, 0x1002), 14 * YEAR + 26 * 7 * 86_400, 0.0, [], [1, 0], [0, 0]),
    ("girl_5", FEMALE, 0x1004, (0x1001, 0x1002), 5 * YEAR + 13 * 7 * 86_400, 0.0, "calendar_year_-4_0.7", [0, 0], [1, 1]),
    ("girl_1", FEMALE, 0x1005, (0x1001, 0x1002), YEAR + 26 * 7 * 86_400, 0.0, [], [1, 1], [0, 0]),
    ("boy_2", MALE, 0x1006, (0x1001, 0x1002), 2 * YEAR + 26 * 7 * 86_400, 0.0, [], [0, 0], [0, 1]),
    ("man_75", MALE, 0x1007, None, 75 * YEAR, 4.0, [], [1, 1], [1, 1]),
    ("woman_82", FEMALE, 0x1008, None, 82 * YEAR, -6.0, [], [0, 1], [0, 0]),
]
# Every person is born at `born = −age` (the clock 0 is now). `girl_5` is born at −(5 ·
# YEAR + 13 weeks); the calendar year −4 with `N` = 0.7 is her life from 1.25 to 2.25 years,
# so it costs in the window of the infant (to 2) and of the child (from 2).
NUTRITION = {"calendar_year_-4_0.7": [(-4, 0.7)]}


def genome_of(index: int, sex: bytes, mc1r: list, herc2: list, loci3: dict) -> dict:
    genes = [(f"{trait}_locus", doses(k, index)) for k, trait in enumerate(BODY_TRAITS_MAP3)]
    genes += [("core:mc1r", mc1r), ("core:herc2", herc2)]
    return founder_genome(sex, genes, loci3)


def inputs_of(fixture: tuple, names: int, sex: bytes, genome: dict, key: int, parents) -> tuple:
    """`BodyInputs::of_person` (ADR-0015 section 3): `z` per role from the traits of map 3
    (`names` 0) or map 4 (`names` 1), each the phenotype at the age 0; the copies of allele
    1 at MC1R and HERC2."""
    calibration, genome_map = data_reference.strip(fixture[0]), data_reference.strip(fixture[1])
    rows = {row["trait"]: row for row in calibration["traits"]}
    loci = {locus["id"]: locus for locus in genome_map["loci"]}
    z_of = {}
    for trait in (BODY_TRAITS_MAP3, BODY_TRAITS_MAP4)[names]:
        z_of[trait] = phenotype(SEED, PhenotypeTrait(trait, genome_map, rows[trait]), genome, key, parents, 0)
    z = {role: z_of[pair[names]] for role, pair in ROLES.items()}
    return z, z_of, sum(copies_at(genome, loci["core:mc1r"])), sum(copies_at(genome, loci["core:herc2"]))


REASONS = {
    "height_cm": "z_h is the phenotype of core:growth, not core:height: same locus and G, other C and E (occasion str_key)",
    "eyebrow_density": "reads z of core:hair_density, not of core:brow",
    "beard_coverage": "reads z of core:beard, not of core:beard_coverage (men only; women 0 in both)",
}


def bits(value: float) -> str:
    return data_reference.f64_bits(value)


def main(argv) -> int:
    map3 = fixture_map3()
    map4 = fixture_map4(map3)
    alternative = fixture_map4(map3, rename_locus=True)
    files = {
        FIXTURE / "calibration" / "genetics.json": text(map4[0]),
        FIXTURE / "genetics" / "genome_map.json": text(map4[1]),
    }
    if "--check" in argv:
        same = all(path.exists() and path.read_text() == content for path, content in files.items())
        print("data_body matches the reference" if same else "data_body DIFFERS from the reference")
        if not same:
            return 1
    else:
        for path, content in files.items():
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content)
    hash3, hash4, hash_alt = documents_hash(map3), documents_hash(map4), documents_hash(alternative)
    assert hash3 == MAP3_DATA_HASH, f"{hash3:016x}"
    assert data_reference.data_hash(FIXTURE) == hash4
    print(f"data_body_hash map4 {hash4:016x} (tests/golden/data_body, with the repository's demography)")
    print(f"data_body_hash map3 {hash3:016x} (the fixture before this change, rebuilt by fixture_map3)")
    print(f"data_body_hash alternative_locus_renamed {hash_alt:016x} (rejected alternative: core:height_locus renamed core:growth_locus)")
    print(
        f"data_body_counts map4 traits {len(map4[1]['traits'])} loci {len(map4[1]['loci'])} rows {len(map4[0]['traits'])}"
        f" map3 traits {len(map3[1]['traits'])} loci {len(map3[1]['loci'])} rows {len(map3[0]['traits'])} map_version {map4[1]['map_version']}"
    )
    print(
        "data_body_layout map4 "
        + " ".join(f"{locus['id']}@{locus['chromosome'][0]}{locus['segment']}.{locus['slot']}" for locus in map4[1]["loci"])
    )
    print(f"data_body_str_key core:growth {str_key('core:growth'):016x} core:height {str_key('core:height'):016x}")
    # BodyData::new with the traits of map 4 (and, for comparison, of map 3).
    template = ROOT / "tests" / "golden" / "map4_template"
    cases = [("data_body_map4", map4, BODY_TRAITS_MAP4), ("data_body_map3", map3, BODY_TRAITS_MAP4), ("data_body_map3_with_map3_names", map3, BODY_TRAITS_MAP3)]
    if template.exists():
        _, template_calibration, template_map = data_reference.documents(template)
        cases.append(("map4_template", (template_calibration, template_map), BODY_TRAITS_MAP4))
    no_growth = copy.deepcopy(map4)
    no_growth[0]["traits"] = [row for row in no_growth[0]["traits"] if row["trait"] != "core:growth"]
    cases.append(("data_body_map4_without_growth_row", no_growth, BODY_TRAITS_MAP4))
    for name, fixture, read in cases:
        reason = body_data_check(fixture, read)
        print(f"body_data {name} reading {'map4' if read is BODY_TRAITS_MAP4 else 'map3'} names: {'accepted' if reason is None else 'rejected ' + reason}")
    contract = json.loads(CONTRACT.read_text())["parameters"]
    assert len(contract) == 44
    values = BodyValues(data_reference.strip(map4[0]))
    assert BodyValues(data_reference.strip(map3[0])).v == values.v
    for sex, name in ((FEMALE, "female"), (MALE, "male")):
        curve = values.curve[sex]
        print(f"body_curve {name} q {curve['q']!r} bits {bits(curve['q'])} c {curve['c']!r} bits {bits(curve['c'])} B {curve['b']!r} bits {bits(curve['b'])}")
    for line in spec_checks(values, contract):
        print(line)
    loci3 = {locus["id"]: locus for locus in map3[1]["loci"]}
    changed_keys = set()
    for index, (name, sex, key, parents, age, delta, nutrition, mc1r, herc2) in enumerate(PERSONS):
        genome = genome_of(index, sex, mc1r, herc2, loci3)
        born = -age
        x = as_years(age)
        b = bio_age(x, delta, values.bio_age_weight)
        nutrition = NUTRITION[nutrition] if isinstance(nutrition, str) else nutrition
        evaluations = []
        for names, fixture in ((0, map3), (1, map4)):
            z, z_of, n_mc1r, n_herc2 = inputs_of(fixture, names, sex, genome, key, parents)
            person = {"sex": sex, "age": age, "b": b, "born": born, "nutrition": nutrition, "z": z, "mc1r": n_mc1r, "herc2": n_herc2}
            params, curve = body_params(values, contract, person)
            evaluations.append((z, z_of, params, curve, n_mc1r, n_herc2))
        z, z_of, params, curve, n_mc1r, n_herc2 = evaluations[1]
        print(
            f"body_person {name} sex {'female' if sex == FEMALE else 'male'} key {key:#x} parents"
            f" {'none' if parents is None else ' '.join(f'{p:#x}' for p in parents)} age_seconds {age} born {born}"
            f" x {x!r} delta {delta!r} b {b!r} bits {bits(b)} nutrition {nutrition} mc1r {n_mc1r} herc2 {n_herc2}"
            f" theta_i {curve['theta_i']!r} bits {bits(curve['theta_i'])} kappa {curve['kappa']!r} bits {bits(curve['kappa'])}"
            f" curve_clamped {str(curve['clamped']).lower()}"
        )
        print(
            f"body_doses {name} (copies at the own loci of the test map of map 3; core:brow_locus and"
            " core:beard_coverage_locus are slots the map of map 4 does not read) "
            + " ".join(f"{trait}_locus={doses(k, index)}" for k, trait in enumerate(BODY_TRAITS_MAP3))
            + f" core:mc1r={mc1r} core:herc2={herc2}"
        )
        for trait in BODY_TRAITS_MAP4:
            print(f"body_z {name} {trait} {z_of[trait]!r} bits {bits(z_of[trait])}")
        for key_, (value, clamped) in params.items():
            print(f"body_param {name} {key_} {value!r} bits {bits(value)} clamped {str(clamped).lower()}")
        old_z, old_z_of, old_params, old_curve, _, _ = evaluations[0]
        for trait in ("core:height", "core:brow", "core:beard_coverage"):
            print(f"body_z_map3 {name} {trait} {old_z_of[trait]!r} bits {bits(old_z_of[trait])}")
        assert {t: v for t, v in old_z_of.items() if t in z_of} == {t: v for t, v in z_of.items() if t in old_z_of}
        assert old_curve["theta_i"] == curve["theta_i"]
        for key_ in params:
            if bits(params[key_][0]) != bits(old_params[key_][0]) or params[key_][1] != old_params[key_][1]:
                changed_keys.add(key_)
                print(
                    f"body_change {name} {key_} map3 {old_params[key_][0]!r} bits {bits(old_params[key_][0])}"
                    f" -> map4 {params[key_][0]!r} bits {bits(params[key_][0])}: {REASONS.get(key_, 'UNEXPECTED')}"
                )
    assert changed_keys <= set(REASONS), changed_keys
    print(f"body_change_summary parameters changed {sorted(changed_keys)}; the other {44 - len(changed_keys)} equal bit for bit for every person")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

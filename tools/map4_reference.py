#!/usr/bin/env python3
"""The template of the base map 4 (design note of the template of map 4, step 11; ADR-0020
section 1, which replaces `core:height` of ADR-0015 section 2), written from the note and
the repository's data, independent of the Rust code: the golden template
`tests/golden/map4_template/{calibration/genetics.json, genetics/genome_map.json}`, the
pre-run's starting point (ADR-0020 section 3: `d_ref` = 1 at every additive locus, `sigma_ref`
= the start value `σ_start`). The repository's data stay map 3; the life course data of the
template are the repository's (`data/calibration/demography.json`, which a directory without
its own takes, tools/data_reference.py).

The map (format 2, `map_version` 4):

- Traits in ascending data ID (ADR-0008 section 1), each with `sigma_ref = σ_start =
  sqrt(n_own · 0.5 + w · w · n_s · 0.5)` in f64 (`n_own` its own loci, `n_s` the own loci of
  the included factor, `w` the include's weight; every locus additive with effect 1, load 1,
  frequency 0.5, so its genic variance is 1/2; `math.sqrt` is correctly rounded, as
  `sim_math::sqrt`), `exclusive` only for `core:pigment`, `includes` as `TRAITS` lists them.
  The products `w · w · n_s · 0.5` scale `w · w` by powers of two only, so the order of the
  factors cannot change a bit.
- Loci in map order (autosomes by segment and slot, then X, Y, mt): the five disease loci of
  map 3 as they are; `core:mc1r` (autosome 1, slot 12) and `core:herc2` (autosome 3, slot 12),
  `recessive`, `large`, mutation `both`; per trait `n_own` additive loci `<trait>_<NN>` (`NN`
  from 01), effect 1.0, `d_ref` 1.0, one load 1.0 on the trait, `small`, `both`.
- Placement (ADR-0020 section 1): the traits with own loci in ascending data ID get the
  ordinal `o` from 0 (here every trait has own loci, so `o` is the trait's index); locus `i`
  (from 0) of trait `o` goes to autosome segment `(o · 3 + i) mod 8`, there to the free slot
  (0 to 23) with the largest smallest distance to the loci of the same trait already in that
  segment, or the smallest free slot if there is none yet; ties to the smaller slot. Traits
  in ordinal order, each by `i`. Free: no locus there (the disease loci, MC1R, HERC2 and the
  loci placed before). A full segment is an error.

The calibration (format 7): the repository's, with the phenotype rows of ADR-0020 section 1
for every trait but `core:longevity` (which has none, `longevity_trait`), `cognition_trait`
`core:cognition`, `height_trait` `core:growth`, `puberty_trait` `core:puberty`,
`longevity_trait` `core:longevity`, the sex difference of `core:cognition` (shift 0, variance
ratio 1.12, genetics specification 6.2, K6.2b and K6.2c) and the threshold disease naming its
liability `core:disease_liability` (its heritability 0.6 equals the row bit for bit).

The files are JSON as serde_json's pretty printer writes them (two spaces, the keys in the
order of the data files, f64 by `world_reference.json_number`, a final newline). Prints the
trait table, the placement of every locus, the occupancy of each segment, the data hash of
the template with the repository's demography, `T` and the checks: the rules of
`Data::new` the template must meet (ADR-0007, ADR-0008, ADR-0009, ADR-0014, ADR-0017,
the threshold of ADR-0020 section 2) and those of `BodyData::new` with the names of map 4
(ADR-0015 section 2 as ADR-0020 section 1 changes it; tools/body_reference.py).

Usage: .venv/bin/python tools/map4_reference.py           (write the files)
       .venv/bin/python tools/map4_reference.py --check   (compare, write nothing)
"""

import copy
import json
import math
import sys
from pathlib import Path

import data_reference
from rng_reference import str_key
from world_reference import json_number

ROOT = Path(__file__).resolve().parent.parent
# The base of the fixture: map 3 and its calibration, the map 3 fixture tests/golden/map3 (data/
# held them before the switch to map 4, byte for byte; design note of the switch, section 4).
# The life course data stay the repository's (data/calibration/demography.json).
REPOSITORY = ROOT / "tests" / "golden" / "map3"
TEMPLATE = ROOT / "tests" / "golden" / "map4_template"

MAP_VERSION = 4
AS_OF = "2026-10-12"
SEGMENTS, SLOTS = 8, 24
GROWTH, ANDROGEN, ESTROGEN, PIGMENT = "core:growth", "core:androgen", "core:estrogen", "core:pigment"
LONGEVITY, LIABILITY = "core:longevity", "core:disease_liability"
# The weights of the includes, the f64 nearest the decimals of ADR-0020 section 1.
W_GROWTH, W_ANDROGEN, W_ESTROGEN = 0.15, 0.50, 0.70
FACES = [f"core:face_{n:02d}" for n in range(1, 13)]
# Every trait of map 4: (own loci, include or None) (ADR-0020 section 1).
TRAITS = {
    GROWTH: (16, None),
    PIGMENT: (16, None),
    "core:cognition": (16, None),
    ANDROGEN: (8, None),
    LONGEVITY: (8, None),
    LIABILITY: (8, None),
    "core:puberty": (6, None),
    ESTROGEN: (2, None),
    **{face: (1, (GROWTH, W_GROWTH)) for face in FACES},
    "core:leg_ratio": (1, (GROWTH, W_GROWTH)),
    "core:head_size": (1, (GROWTH, W_GROWTH)),
    "core:chest_depth": (1, (GROWTH, W_GROWTH)),
    "core:muscle": (2, (ANDROGEN, W_ANDROGEN)),
    "core:shoulder_width": (1, (ANDROGEN, W_ANDROGEN)),
    "core:fat_distribution": (1, (ANDROGEN, W_ANDROGEN)),
    "core:beard": (2, (ANDROGEN, W_ANDROGEN)),
    "core:body_hair": (1, (ANDROGEN, W_ANDROGEN)),
    "core:bald": (2, (ANDROGEN, W_ANDROGEN)),
    "core:hip_width": (1, (ESTROGEN, W_ESTROGEN)),
    "core:breast": (2, (ESTROGEN, W_ESTROGEN)),
    "core:hair_darkness": (2, None),
    "core:curl": (2, None),
    "core:freckle": (2, None),
    "core:hair_density": (2, None),
    "core:grey": (2, None),
    "core:skin_age": (2, None),
    "core:body_fat": (2, None),
}
EXCLUSIVE = {PIGMENT}
# The phenotype rows (h² child, c² child, h² adult, c² adult) of ADR-0020 section 1; every
# trait but longevity has one, the faces 0.70 and 0, the others not named 0.60 and 0.
NAMED_ROWS = {
    GROWTH: (0.80, 0.05, 0.80, 0.05),
    PIGMENT: (0.80, 0.0, 0.80, 0.0),
    "core:cognition": (0.50, 0.25, 0.75, 0.05),
    ANDROGEN: (0.60, 0.0, 0.60, 0.0),
    LIABILITY: (0.60, 0.0, 0.60, 0.0),
    "core:puberty": (0.60, 0.10, 0.60, 0.10),
    ESTROGEN: (0.60, 0.0, 0.60, 0.0),
    "core:body_fat": (0.50, 0.10, 0.50, 0.10),
    **{face: (0.70, 0.0, 0.70, 0.0) for face in FACES},
}
OTHER_ROW = (0.60, 0.0, 0.60, 0.0)
# The named traits of the calibration (ADR-0020 section 1).
NAMED_TRAITS = {
    "longevity_trait": LONGEVITY,
    "puberty_trait": "core:puberty",
    "cognition_trait": "core:cognition",
    "height_trait": GROWTH,
}
# The sex difference of cognition (genetics specification 6.2, K6.2b, K6.2c); no other trait
# of the map is in the table of 6.2.
COGNITION_SHIFT, COGNITION_VARIANCE_RATIO = 0.0, 1.12
# MC1R and HERC2 (ADR-0020 section 1): autosome segment and slot.
PIGMENT_LOCI = (("core:mc1r", 1, 12, "MC1R"), ("core:herc2", 3, 12, "HERC2"))
# The traits the body mapping reads with the names of map 4 (ADR-0015 section 2 with
# ADR-0020 section 1; tools/body_reference.py `BODY_TRAITS_MAP4`, the same list).
BODY_TRAITS = [
    GROWTH,
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
    PIGMENT,
    "core:hair_darkness",
    ANDROGEN,
    ESTROGEN,
    "core:breast",
    "core:curl",
    "core:freckle",
    "core:beard",
    "core:body_hair",
    "core:bald",
    "core:hair_density",
    "core:grey",
    "core:skin_age",
]


def loci_word(n: int) -> str:
    return "locus" if n == 1 else "loci"


def sigma_start(trait: str) -> float:
    """`σ_start = sqrt(n_own · 0.5 + w · w · n_s · 0.5)` (ADR-0020 section 3) in f64, left to
    right; without an include `sqrt(n_own · 0.5)`."""
    own, include = TRAITS[trait]
    variance = float(own) * 0.5
    if include is not None:
        factor, weight = include
        variance = variance + weight * weight * float(TRAITS[factor][0]) * 0.5
    return math.sqrt(variance)


def trait_description(trait: str) -> str:
    own, include = TRAITS[trait]
    text = f"Trait of the template of map 4 with {own} own additive {loci_word(own)}"
    if include is not None:
        text += f", including {include[0]} with weight {include[1]!r}"
    if trait in EXCLUSIVE:
        text += ", exclusive"
    return text + "; sigma_ref is its start value of the pre-run (ADR-0020 sections 1 and 3)."


def trait_entries() -> list:
    entries = []
    for trait in sorted(TRAITS):
        own, include = TRAITS[trait]
        entries.append(
            {
                "id": trait,
                "sigma_ref": sigma_start(trait),
                "exclusive": trait in EXCLUSIVE,
                "includes": [] if include is None else [{"trait": include[0], "weight": include[1]}],
                "description": trait_description(trait),
            }
        )
    return entries


def map_order(locus) -> tuple:
    """Chromosome type (autosome, X, Y, mt), segment, slot (ADR-0007 `data.locus_order`)."""
    return (data_reference.CHROMOSOMES.index(locus["chromosome"]), locus["segment"], locus["slot"])


def pigment_locus(locus_id: str, segment: int, slot: int, name: str) -> dict:
    return {
        "id": locus_id,
        "chromosome": "autosome",
        "segment": segment,
        "slot": slot,
        "action": "recessive",
        "mark": "large",
        "mutation": "both",
        "description": f"{name}, a large recessive pigment locus (genetics specification 10.4, ADR-0020 section 1).",
    }


def placement(base_loci: list) -> list:
    """The placement of every new additive locus (ADR-0020 section 1) on the autosomes
    around `base_loci` (the disease loci, MC1R and HERC2): a list of (ordinal, trait, i,
    segment, slot, the smallest distance to the trait's loci already in the segment or
    None) in the order of placing."""
    taken = [set() for _ in range(SEGMENTS)]
    for locus in base_loci:
        if locus["chromosome"] == "autosome":
            assert locus["slot"] not in taken[locus["segment"]]
            taken[locus["segment"]].add(locus["slot"])
    placed = []
    for ordinal, trait in enumerate(sorted(TRAITS)):
        own = TRAITS[trait][0]
        mine = [[] for _ in range(SEGMENTS)]
        for i in range(own):
            segment = (ordinal * 3 + i) % SEGMENTS
            free = [slot for slot in range(SLOTS) if slot not in taken[segment]]
            if not free:
                raise ValueError(f"segment {segment} is full when {trait} locus {i} comes")
            if not mine[segment]:
                slot, distance = free[0], None
            else:
                # The largest smallest distance; `max` keeps the first of equal keys, and
                # `free` ascends, so a tie goes to the smaller slot.
                slot = max(free, key=lambda s: min(abs(s - m) for m in mine[segment]))
                distance = min(abs(slot - m) for m in mine[segment])
            taken[segment].add(slot)
            mine[segment].append(slot)
            placed.append((ordinal, trait, i, segment, slot, distance))
    return placed


def additive_locus(trait: str, i: int, segment: int, slot: int) -> dict:
    return {
        "id": f"{trait}_{i + 1:02d}",
        "chromosome": "autosome",
        "segment": segment,
        "slot": slot,
        "action": {"additive": {"effect": 1.0, "d_ref": 1.0, "loads": [{"trait": trait, "weight": 1.0}]}},
        "mark": "small",
        "mutation": "both",
        "description": f"Additive locus {i + 1} of {trait} (ADR-0020 section 1).",
    }


def genome_map() -> tuple:
    """The template map and the placement (`placement`)."""
    base = json.loads((REPOSITORY / "genetics" / "genome_map.json").read_text())
    assert base["format_version"] == 2 and base["map_version"] == 3 and base["traits"] == []
    assert [locus["mark"] for locus in base["loci"]] == ["disease"] * 5, "map 3 has the five disease loci only"
    fixed = copy.deepcopy(base["loci"]) + [pigment_locus(*entry) for entry in PIGMENT_LOCI]
    placed = placement(fixed)
    loci = fixed + [additive_locus(trait, i, segment, slot) for _, trait, i, segment, slot, _ in placed]
    doc = {
        "format_version": 2,
        "map_version": MAP_VERSION,
        "traits": trait_entries(),
        "loci": sorted(loci, key=map_order),
    }
    return doc, placed


def calibrated(value: float, unit: str, source: str, note: str) -> dict:
    return {"value": value, "unit": unit, "source": source, "as_of": AS_OF, "note": note}


WEIGHT_NOTES = ("h2 of the child", "c2 of the child", "h2 of the adult", "c2 of the adult")


def weights_row(trait: str) -> dict:
    row = {"trait": trait}
    for name, value, note in zip(data_reference.WEIGHTS, NAMED_ROWS.get(trait, OTHER_ROW), WEIGHT_NOTES):
        row[name] = calibrated(value, "fraction", "ADR-0020 section 1 (genetics specification 4.2 and 10.7)", f"{note}, template of map 4")
    return row


def threshold_carrier(doc: dict) -> dict:
    """The carrier of the threshold disease `core:disease_liability` in a calibration."""
    ((carrier,),) = [[d["carrier"]["threshold"]] for d in doc["diseases"] if d["code"] == LIABILITY]
    return carrier


def calibration() -> dict:
    """The repository's calibration with the rows, the named traits, the sex difference of
    cognition and the liability of the threshold disease (note section 2)."""
    base = json.loads((REPOSITORY / "calibration" / "genetics.json").read_text())
    assert base["traits"] == [] and base["sex_differences"] == [] and threshold_carrier(base)["liability"] is None
    assert all(base[field] is None for field in NAMED_TRAITS)
    doc = copy.deepcopy(base)
    doc["traits"] = [weights_row(trait) for trait in sorted(TRAITS) if trait != LONGEVITY]
    doc["sex_differences"] = [
        {
            "trait": "core:cognition",
            "shift": calibrated(
                COGNITION_SHIFT,
                "standard_deviation",
                "docs/genetik-spezifikation.md 6.2 (K6.2b), Deary et al. (2003)",
                "no shift of men against women, template of map 4",
            ),
            "variance_ratio": calibrated(
                COGNITION_VARIANCE_RATIO,
                "ratio",
                "docs/genetik-spezifikation.md 6.2 (K6.2c), Deary et al. (2003)",
                "variance of men over women, template of map 4",
            ),
        }
    ]
    threshold_carrier(doc)["liability"] = LIABILITY
    for field, trait in NAMED_TRAITS.items():
        doc[field] = trait
    return doc


def pretty(value, indent: int = 0) -> str:
    """JSON as serde_json's `to_string_pretty` writes it: two spaces per level, `": "`
    after a key, an empty list `[]`, f64 by `json_number`, integers plain, strings with
    the escapes of `json.dumps` (none occur but the quote and the backslash)."""
    pad, inner = "  " * indent, "  " * (indent + 1)
    if value is None:
        return "null"
    if value is True:
        return "true"
    if value is False:
        return "false"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        return json_number(value)
    if isinstance(value, str):
        return json.dumps(value, ensure_ascii=False)
    if isinstance(value, list):
        if not value:
            return "[]"
        return "[\n" + ",\n".join(inner + pretty(item, indent + 1) for item in value) + "\n" + pad + "]"
    assert isinstance(value, dict)
    if not value:
        return "{}"
    return "{\n" + ",\n".join(f"{inner}{json.dumps(key)}: {pretty(item, indent + 1)}" for key, item in value.items()) + "\n" + pad + "}"


def text(doc: dict) -> str:
    out = pretty(doc) + "\n"
    assert json.loads(out) == doc
    return out


def f64_bits(value: float) -> str:
    return data_reference.f64_bits(value)


def check_map(doc: dict) -> list:
    """The rules of `Data::new` for the map (ADR-0007 section 2, ADR-0008 section 5,
    ADR-0017 section 1) that the template must meet, as a reader checks them; returns the
    lines of what was checked."""
    loci, traits = doc["loci"], doc["traits"]
    bounds = {"autosome": (8, 24), "x": (1, 16), "y": (1, 8), "mt": (1, 8)}
    for locus in loci:
        segments, slots = bounds[locus["chromosome"]]
        assert 0 <= locus["segment"] < segments and 0 <= locus["slot"] < slots, locus["id"]
    keys = [map_order(locus) for locus in loci]
    assert keys == sorted(keys) and len(set(keys)) == len(keys), "data.locus_order"
    ids = [trait["id"] for trait in traits]
    assert ids == sorted(ids) and len(set(ids)) == len(ids), "data.trait_order"
    names = ids + [locus["id"] for locus in loci]
    assert len(set(names)) == len(names), "data.locus_id / data.trait_key"
    assert len({str_key(name) for name in names}) == len(names), "data.locus_key / data.trait_key"
    for locus in loci:
        if isinstance(locus["action"], str):
            assert locus["mark"] in ("large", "disease"), f"data.locus_mark {locus['id']}"
        if locus["mark"] == "disease":
            assert locus["mutation"] == "to_alt", f"data.locus_mark {locus['id']}"
    by_id = {trait["id"]: trait for trait in traits}
    loaded = {trait: [] for trait in ids}
    for locus in loci:
        if isinstance(locus["action"], str):
            continue
        ((kind, weights),) = locus["action"].items()
        assert kind in ("additive", "threshold") and locus["chromosome"] == "autosome", "data.locus_additive"
        assert weights["effect"] > 0.0 and 0.0 <= weights["d_ref"] <= 2.0, "data.locus_weights"
        named = [load["trait"] for load in weights["loads"]]
        assert 1 <= len(named) <= 2 and len(set(named)) == len(named), "data.locus_weights"
        for load in weights["loads"]:
            assert load["trait"] in by_id and load["weight"] != 0.0, "data.locus_load"
            loaded[load["trait"]].append(locus["id"])
        exclusive = [name for name in named if by_id[name]["exclusive"]]
        assert not exclusive or len(named) == 1, "data.load_exclusive"
    for trait in traits:
        assert trait["sigma_ref"] > 0.0, "data.trait_sigma"
        included = [entry["trait"] for entry in trait["includes"]]
        assert included == sorted(set(included)), "data.trait_include"
        for entry in trait["includes"]:
            other = by_id[entry["trait"]]
            assert entry["trait"] != trait["id"] and other["includes"] == [] and entry["weight"] != 0.0, "data.trait_include"
            assert not other["exclusive"], "data.load_exclusive"
            assert not set(loaded[trait["id"]]) & set(loaded[entry["trait"]]), "data.trait_include"
        assert loaded[trait["id"]] or trait["includes"], "data.trait_unused"
        # data.trait_bounds: the fold of the largest terms (|λ β| · 2 at d_ref 1, d 0 or 2:
        # each term at most 1 here) over sigma_ref is finite and at most 2^1022.
        largest = float(len(loaded[trait["id"]])) + sum(
            abs(entry["weight"]) * len(loaded[entry["trait"]]) for entry in trait["includes"]
        )
        mean = data_reference.mean_ref_of(trait)  # absent (the template), or finite (Grok GF-1)
        if mean is not None:
            largest = largest + abs(mean)  # |mean_ref| last, before the division
        assert math.isfinite(largest / trait["sigma_ref"]) and largest / trait["sigma_ref"] <= 2.0**1022, "data.trait_bounds"
    additive = [locus for locus in loci if isinstance(locus["action"], dict)]
    autosomal = [locus for locus in loci if locus["chromosome"] == "autosome"]
    lines = [
        f"check_map loci {len(loci)} additive {len(additive)} autosomal {len(autosomal)} other {len(loci) - len(additive)}"
        f" ({' '.join(locus['id'] for locus in loci if isinstance(locus['action'], str))})",
        f"check_map traits {len(traits)} str_keys distinct {len(names)} (traits and loci) positions distinct {len(set(keys))}",
        "check_map rules data.locus_position data.locus_order data.locus_id data.locus_key data.locus_mark"
        " data.trait_order data.trait_key data.trait_sigma data.trait_include data.trait_unused data.locus_weights"
        " data.locus_load data.load_exclusive data.locus_additive data.trait_bounds: met",
    ]
    assert len(loci) == 128 and len(additive) == 121 and len(autosomal) == 126 and len(loci) - len(additive) == 7
    assert sum(own for own, _ in TRAITS.values()) == 121
    return lines


def check_calibration(doc: dict, map_doc: dict) -> list:
    """The rules of `Data::new` for the rows and the sex differences (ADR-0009 section 2,
    ADR-0014 section 1, ADR-0017 section 1, the threshold disease); the rest of the
    calibration is the repository's, which `Data::new` takes."""
    traits = [trait["id"] for trait in map_doc["traits"]]
    rows = [row["trait"] for row in doc["traits"]]
    assert rows == sorted(rows) and len(set(rows)) == len(rows) and set(rows) <= set(traits), "data.calibration_unknown"
    assert set(traits) - set(rows) == {LONGEVITY}
    for row in doc["traits"]:
        values = [row[name]["value"] for name in data_reference.WEIGHTS]
        assert all(row[name]["unit"] == "fraction" for name in data_reference.WEIGHTS), "data.calibration_unit"
        assert all(0.0 <= value <= 1.0 for value in values), "data.calibration_range"
        assert values[0] + values[1] <= 1.0 and values[2] + values[3] <= 1.0, "data.calibration_range"
    differences = [row["trait"] for row in doc["sex_differences"]]
    assert differences == sorted(differences) and set(differences) <= set(rows), "data.calibration_unknown"
    for row in doc["sex_differences"]:
        assert row["shift"]["unit"] == "standard_deviation" and row["variance_ratio"]["unit"] == "ratio"
        assert -10.0 <= row["shift"]["value"] <= 10.0 and 0.0 < row["variance_ratio"]["value"] <= 10.0
    for field in data_reference.NAMED_TRAITS:
        assert doc[field] in traits, f"data.trait_named {field}"
    data_reference.check_liability(doc, map_doc)
    return [
        f"check_calibration rows {len(rows)} (every trait but {LONGEVITY}) sex_differences {differences}"
        f" named {' '.join(f'{field}={doc[field]}' for field in data_reference.NAMED_TRAITS)}: met"
    ]


def check_body(doc: dict, map_doc: dict) -> list:
    """`BodyData::new` with the names of map 4 (ADR-0015 section 2, ADR-0020 section 1):
    every trait the body reads is in the map and has a row whose child and adult weights are
    equal bit for bit; MC1R and HERC2 are recessive loci on an autosome."""
    traits = {trait["id"] for trait in map_doc["traits"]}
    rows = {row["trait"]: row for row in doc["traits"]}
    for trait in BODY_TRAITS:
        assert trait in traits and trait in rows, f"data.body_traits {trait}"
        row = rows[trait]
        assert f64_bits(row["h2_child"]["value"]) == f64_bits(row["h2_adult"]["value"]), f"data.body_traits {trait}"
        assert f64_bits(row["c2_child"]["value"]) == f64_bits(row["c2_adult"]["value"]), f"data.body_traits {trait}"
    by_id = {locus["id"]: locus for locus in map_doc["loci"]}
    for locus_id, _, _, _ in PIGMENT_LOCI:
        locus = by_id[locus_id]
        assert locus["action"] == "recessive" and locus["chromosome"] == "autosome", f"data.body_traits {locus_id}"
    assert len(BODY_TRAITS) == 35 and len(set(BODY_TRAITS)) == 35
    return [f"check_body traits {len(BODY_TRAITS)} with equal weights, core:mc1r and core:herc2 recessive autosomal: met"]


def report(map_doc: dict, calibration_doc: dict, placed: list) -> list:
    lines = []
    for ordinal, trait in enumerate(sorted(TRAITS)):
        own, include = TRAITS[trait]
        sigma = sigma_start(trait)
        variance = float(own) * 0.5 if include is None else float(own) * 0.5 + include[1] * include[1] * float(TRAITS[include[0]][0]) * 0.5
        lines.append(
            f"trait {ordinal:2d} {trait} own {own} includes {'-' if include is None else f'{include[0]} {include[1]!r}'}"
            f" exclusive {str(trait in EXCLUSIVE).lower()} variance {variance!r} sigma_ref {sigma!r} bits {f64_bits(sigma)}"
        )
    for ordinal, trait, i, segment, slot, distance in placed:
        lines.append(
            f"placement {trait}_{i + 1:02d} ordinal {ordinal} i {i} segment {segment} slot {slot}"
            f" smallest_distance {'-' if distance is None else distance}"
        )
    for segment in range(SEGMENTS):
        here = {locus["slot"]: locus["id"] for locus in map_doc["loci"] if locus["chromosome"] == "autosome" and locus["segment"] == segment}
        lines.append(
            f"segment {segment} occupied {len(here)} free {SLOTS - len(here)}: "
            + " ".join(f"{slot}={here[slot]}" if slot in here else f"{slot}=-" for slot in range(SLOTS))
        )
    for locus in map_doc["loci"]:
        if locus["chromosome"] != "autosome":
            lines.append(f"segment {locus['chromosome']} {locus['segment']} slot {locus['slot']}={locus['id']}")
    lines.append(
        "map_order " + " ".join(f"{locus['id']}@{locus['chromosome'][0]}{locus['segment']}.{locus['slot']}" for locus in map_doc["loci"])
    )
    for row in calibration_doc["traits"]:
        lines.append(
            f"row {row['trait']} " + " ".join(f"{name} {row[name]['value']!r}" for name in data_reference.WEIGHTS)
        )
    for row in calibration_doc["sex_differences"]:
        lines.append(f"sex_difference {row['trait']} shift {row['shift']['value']!r} variance_ratio {row['variance_ratio']['value']!r}")
    return lines


def main(argv) -> int:
    map_doc, placed = genome_map()
    calibration_doc = calibration()
    checks = check_map(map_doc) + check_calibration(calibration_doc, map_doc) + check_body(calibration_doc, map_doc)
    files = {
        TEMPLATE / "genetics" / "genome_map.json": text(map_doc),
        TEMPLATE / "calibration" / "genetics.json": text(calibration_doc),
    }
    if "--check" in argv:
        same = all(path.exists() and path.read_text() == content for path, content in files.items())
        print("map4_template matches the reference" if same else "map4_template DIFFERS from the reference")
        if not same:
            return 1
    else:
        for path, content in files.items():
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content)
    demography, calibration_read, map_read = data_reference.documents(TEMPLATE)
    assert calibration_read == calibration_doc and map_read == map_doc
    print(f"data_hash_map4_template {data_reference.data_hash(TEMPLATE):016x} (with the repository's demography)")
    for code, trait, heritability, prevalence, value in data_reference.liability_diseases(calibration_doc):
        print(
            f"threshold {code} liability {trait} heritability {heritability!r} prevalence {prevalence!r}"
            f" 1-K bits {f64_bits(1.0 - prevalence)} T {value!r} bits {f64_bits(value)}"
        )
    print(
        f"files genome_map.json bytes {len(files[TEMPLATE / 'genetics' / 'genome_map.json'].encode())}"
        f" genetics.json bytes {len(files[TEMPLATE / 'calibration' / 'genetics.json'].encode())}"
        f" map_version {map_doc['map_version']} format_version {map_doc['format_version']} calibration format {calibration_doc['format_version']}"
    )
    for line in checks:
        print(line)
    for line in report(map_doc, calibration_doc, placed):
        print(line)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

#!/usr/bin/env python3
"""The test map of the partner market with similarity (design note of the similarity,
section 3), written from the note and the repository's data, independent of the Rust code:
`tests/golden/data_pairing`, the layout of `tests/golden/data_traits` (a calibration and a
genome map; the life course data are the repository's, `data/calibration/demography.json`,
which a directory without its own takes, tools/data_reference.py).

- The genome map is that of `data/genetics/genome_map.json` (format 2, its `map_version`,
  its five disease loci where they are) plus two traits, `test:cognition` and
  `test:height`, each a factor of its own (`exclusive` false, no `includes`) with
  `sigma_ref` = sqrt(40 · 2 · 0.5 · 0.5) = sqrt(20) (the f64 of `math.sqrt(20.0)`), so that
  `G` has variance 1 for alleles drawn independently with frequency 0.5 (genetics
  specification 4.1), and 40 additive loci of its own each, effect 1, `d_ref` 1, one load
  with weight 1, mark `small`, mutation `both`, on autosomes. The 80 loci take free slots:
  on each of the 8 autosome segments (index 0 to 7) the even slots 2 to 20, cognition on
  2, 6, 10, 14, 18 and height on 4, 8, 12, 16, 20; slot 0 (where the disease loci of
  segments 0, 2 and 4 are) and every odd slot stay free. Their IDs are
  `test:cognition_01` to `_40` and `test:height_01` to `_40`, numbered in map order
  (segment, then slot); the loci stand in map order (ADR-0007 `data.locus_order`).
- The calibration is that of `data/calibration/genetics.json` (format 7) with the rows of
  the phenotype weights of the genetics specification 4.2 in `traits` (ADR-0009, ascending
  by trait): `test:cognition` h² 0.5, c² 0.25 (child), 0.75, 0.05 (adult);
  `test:height` h² 0.8, c² 0.05 (child and adult); and `cognition_trait` =
  `test:cognition`, `height_trait` = `test:height`. Everything else is the repository's.

The files are JSON with an indent of two (the numbers as Python writes them, so `5e-5`
of the repository's file reads `5e-05` here: the same f64). Prints the data hash of the
fixture (tools/data_reference.py) and the layout.

Usage: .venv/bin/python tools/pairing_fixture.py           (write the files)
       .venv/bin/python tools/pairing_fixture.py --check   (compare, write nothing)
"""

import copy
import json
import math
import sys
from pathlib import Path

import data_reference
from rng_reference import str_key

ROOT = Path(__file__).resolve().parent.parent
REPOSITORY = ROOT / "data"
FIXTURE = ROOT / "tests" / "golden" / "data_pairing"

COGNITION, HEIGHT = "test:cognition", "test:height"
LOCI_PER_TRAIT = 40
SEGMENTS = 8
# The even slots 2 to 20 of every autosome segment, alternating between the traits.
SLOTS = {COGNITION: (2, 6, 10, 14, 18), HEIGHT: (4, 8, 12, 16, 20)}
# sqrt(40 · 2 · p · (1 − p)) with p = 0.5: the standard deviation of the raw value
# Σ (d − 1) over 40 independent loci with dose variance 2 · 0.5 · 0.5.
SIGMA_REF = math.sqrt(40 * 2 * 0.5 * 0.5)
assert SIGMA_REF == math.sqrt(20.0)
AS_OF = "2026-10-11"

# Genetics specification 4.2, table: (h² child, c² child, h² adult, c² adult) with the
# labels of the specification, where it has one.
WEIGHTS = {
    COGNITION: (
        (0.5, "docs/genetik-spezifikation.md 4.2, table, cognitive ability, h2 child"),
        (0.25, "docs/genetik-spezifikation.md 4.2, table, cognitive ability, c2 child"),
        (0.75, "docs/genetik-spezifikation.md 4.2 [K4.2d]"),
        (0.05, "docs/genetik-spezifikation.md 4.2 [K4.2e]"),
    ),
    HEIGHT: (
        (0.8, "docs/genetik-spezifikation.md 4.2, table, height (target height), h2 child"),
        (0.05, "docs/genetik-spezifikation.md 4.2, table, height (target height), c2 child"),
        (0.8, "docs/genetik-spezifikation.md 4.2 [K4.2a]"),
        (0.05, "docs/genetik-spezifikation.md 4.2 [K4.2b]"),
    ),
}
DESCRIPTIONS = {
    COGNITION: "Cognition of the test map of the partner market (P7): a factor of its own with 40 additive loci",
    HEIGHT: "Height (target height) of the test map of the partner market (P7): a factor of its own with 40 additive loci",
}


def map_order(locus) -> tuple:
    """Chromosome type (autosome, X, Y, mt), segment, slot (ADR-0007 `data.locus_order`)."""
    return (data_reference.CHROMOSOMES.index(locus["chromosome"]), locus["segment"], locus["slot"])


def trait_loci(trait: str) -> list:
    """The 40 loci of a trait in map order: segment by segment, its five slots."""
    loci = []
    for segment in range(SEGMENTS):
        for slot in SLOTS[trait]:
            number = len(loci) + 1
            loci.append(
                {
                    "id": f"{trait}_{number:02d}",
                    "chromosome": "autosome",
                    "segment": segment,
                    "slot": slot,
                    "action": {"additive": {"effect": 1.0, "d_ref": 1.0, "loads": [{"trait": trait, "weight": 1.0}]}},
                    "mark": "small",
                    "mutation": "both",
                    "description": f"Locus {number} of {trait} in the test map of the partner market: effect 1, d_ref 1.",
                }
            )
    assert len(loci) == LOCI_PER_TRAIT
    return loci


def genome_map() -> dict:
    """The repository's map with the two traits and their 80 loci in map order."""
    base = json.loads((REPOSITORY / "genetics" / "genome_map.json").read_text())
    assert base["traits"] == [], "the base map has no traits until the pre-run (ADR-0008 section 4)"
    doc = copy.deepcopy(base)
    doc["traits"] = [
        {
            "id": trait,
            "sigma_ref": SIGMA_REF,
            "exclusive": False,
            "includes": [],
            "description": f"{DESCRIPTIONS[trait]} (effect 1, d_ref 1); sigma_ref sqrt(20), so G has variance 1 for alleles drawn with frequency 0.5.",
        }
        for trait in (COGNITION, HEIGHT)
    ]
    doc["loci"] = sorted(base["loci"] + trait_loci(COGNITION) + trait_loci(HEIGHT), key=map_order)
    return doc


def weights_row(trait: str) -> dict:
    row = {"trait": trait}
    for name, (value, source) in zip(data_reference.WEIGHTS, WEIGHTS[trait]):
        row[name] = {
            "value": value,
            "unit": "fraction",
            "source": source,
            "as_of": AS_OF,
            "note": "phenotype weight of the test map of the partner market (design note of the similarity, section 3)",
        }
    return row


def calibration() -> dict:
    """The repository's calibration with the phenotype weights and the two named traits."""
    base = json.loads((REPOSITORY / "calibration" / "genetics.json").read_text())
    assert base["traits"] == [] and base["cognition_trait"] is None and base["height_trait"] is None
    doc = copy.deepcopy(base)
    doc["traits"] = [weights_row(COGNITION), weights_row(HEIGHT)]
    doc["cognition_trait"] = COGNITION
    doc["height_trait"] = HEIGHT
    return doc


def check(map_doc: dict, calibration_doc: dict) -> None:
    """The rules of ADR-0007, ADR-0008 and ADR-0009 the fixture must meet, as a reader
    checks them (`data.locus_order`, `data.trait_order`, `data.trait_key`,
    `data.trait_sigma`, `data.trait_unused`, `data.locus_weights`, `data.locus_load`,
    `data.locus_additive`; the calibration's rows in order, unit, range)."""
    keys = [map_order(locus) for locus in map_doc["loci"]]
    assert keys == sorted(keys) and len(set(keys)) == len(keys), "loci strictly in map order"
    ids = [trait["id"] for trait in map_doc["traits"]]
    assert ids == sorted(ids) and len(set(ids)) == len(ids)
    names = ids + [locus["id"] for locus in map_doc["loci"]]
    assert len({str_key(name) for name in names}) == len(set(names)) == len(names)
    for trait in map_doc["traits"]:
        assert trait["sigma_ref"] > 0.0 and trait["includes"] == []
    loaded = {}
    for locus in map_doc["loci"]:
        action = locus["action"]
        if isinstance(action, str):
            continue
        ((kind, weights),) = action.items()
        assert kind in ("additive", "threshold") and locus["chromosome"] == "autosome"
        assert weights["effect"] > 0.0 and 0.0 <= weights["d_ref"] <= 2.0 and 1 <= len(weights["loads"]) <= 2
        for load in weights["loads"]:
            assert load["trait"] in ids and load["weight"] != 0.0
            loaded[load["trait"]] = loaded.get(load["trait"], 0) + 1
    assert loaded == {COGNITION: LOCI_PER_TRAIT, HEIGHT: LOCI_PER_TRAIT}, loaded
    rows = [row["trait"] for row in calibration_doc["traits"]]
    assert rows == sorted(rows) == ids
    for row in calibration_doc["traits"]:
        values = [row[name]["value"] for name in data_reference.WEIGHTS]
        assert all(row[name]["unit"] == "fraction" for name in data_reference.WEIGHTS)
        assert all(0.0 <= value <= 1.0 for value in values)
        assert values[0] + values[1] <= 1.0 and values[2] + values[3] <= 1.0


def text(doc: dict) -> str:
    return json.dumps(doc, indent=2, ensure_ascii=False) + "\n"


def layout(map_doc: dict) -> list:
    rows = []
    for trait in (COGNITION, HEIGHT):
        mine = [locus for locus in map_doc["loci"] if locus["id"].startswith(trait + "_")]
        per_segment = {}
        for locus in mine:
            per_segment.setdefault(locus["segment"], []).append(locus["slot"])
        rows.append(
            f"layout {trait}: {len(mine)} loci {mine[0]['id']} to {mine[-1]['id']}, autosome segments and slots "
            + " ".join(f"{segment}:{slots}" for segment, slots in sorted(per_segment.items()))
        )
    rows.append(
        "layout map order: "
        + " ".join(f"{locus['id']}@{locus['chromosome'][0]}{locus['segment']}.{locus['slot']}" for locus in map_doc["loci"])
    )
    return rows


def main(argv) -> int:
    map_doc, calibration_doc = genome_map(), calibration()
    check(map_doc, calibration_doc)
    files = {
        FIXTURE / "genetics" / "genome_map.json": text(map_doc),
        FIXTURE / "calibration" / "genetics.json": text(calibration_doc),
    }
    if "--check" in argv:
        same = all(path.exists() and path.read_text() == content for path, content in files.items())
        print("data_pairing matches the reference" if same else "data_pairing DIFFERS from the reference")
        return 0 if same else 1
    for path, content in files.items():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)
    print(f"data_hash_pairing {data_reference.data_hash(FIXTURE):016x}")
    print(f"sigma_ref {SIGMA_REF!r} map_version {map_doc['map_version']} loci {len(map_doc['loci'])}")
    for row in layout(map_doc):
        print(row)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

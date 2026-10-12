#!/usr/bin/env python3
"""The test map of the threshold disease (design note of the threshold disease in the
world, section 4; genetics specification 3.6, ADR-0008, ADR-0009, ADR-0019), written from
the note and the repository's data, independent of the Rust code:
`tests/golden/data_liability`, the layout of `tests/golden/data_traits` and
`tests/golden/data_pairing` (a calibration and a genome map; the life course data are the
repository's, `data/calibration/demography.json`, which a directory without its own takes,
tools/data_reference.py).

- The genome map is that of `data/genetics/genome_map.json` (format 2, its `map_version`,
  its five disease loci where they are) plus one trait, `core:disease_liability`: its data
  ID is the code of the threshold disease (ADR-0008 section 1: the liability carries the
  disease's data ID, the occasion of its environment draw), a factor of its own
  (`exclusive` false, no `includes`) with `sigma_ref` = sqrt(8 · 2 · 0.5 · 0.5) = 2 (exact),
  so that `G` has variance 1 for alleles drawn independently with frequency 0.5 (genetics
  specification 4.1). Eight additive loci of its own, `test:liability_1` to
  `test:liability_8`, each effect 1, `d_ref` 1, one load with weight 1, mark `small`,
  mutation `both`, one per autosome segment: `test:liability_n` on segment n − 1, slot 12
  (no other locus of the map is there; the disease loci of segments 0, 2 and 4 are on
  slot 0). The loci stand in map order (ADR-0007 `data.locus_order`). `G` ranges from −4
  (no allele 1) to 4 (every copy allele 1) in steps of 1/2.
- The calibration is that of `data/calibration/genetics.json` (format 7) with one row of
  phenotype weights in `traits` (ADR-0009): `core:disease_liability` with h² 0.6 and c² 0
  for the child and the adult (genetics specification 3.6, K3.6a: the liability has no
  shared environment, its weights do not depend on the age), and the threshold carrier of
  `core:disease_liability` naming it (`liability`: `core:disease_liability`); its
  `heritability` (0.6) and `prevalence` (0.01) stay the repository's. Everything else is
  the repository's (the other five diseases, the rates, the fertility, no named trait
  but the liability).

The files are JSON with an indent of two (the numbers as Python writes them, so `5e-5`
of the repository's file reads `5e-05` here: the same f64). Prints the data hash of the
fixture (tools/data_reference.py), `T`, the layout and the cases the data check rejects
(each a copy of the fixture's documents changed in one place, `DataError::Disease` in the
core; design note section 4).

Usage: .venv/bin/python tools/liability_fixture.py           (write the files)
       .venv/bin/python tools/liability_fixture.py --check   (compare, write nothing)
"""

import copy
import json
import math
import struct
import sys
from pathlib import Path

import data_reference
from rng_reference import str_key

ROOT = Path(__file__).resolve().parent.parent
REPOSITORY = ROOT / "data"
FIXTURE = ROOT / "tests" / "golden" / "data_liability"

LIABILITY = "core:disease_liability"
LOCI = 8
SLOT = 12
# sqrt(8 · 2 · p · (1 − p)) with p = 0.5: the standard deviation of the raw value
# Σ (d − 1) over 8 independent loci with dose variance 2 · 0.5 · 0.5.
SIGMA_REF = math.sqrt(LOCI * 2 * 0.5 * 0.5)
assert SIGMA_REF == 2.0
AS_OF = "2026-10-12"
# The phenotype row (h² child, c² child, h² adult, c² adult): genetics specification 3.6.
WEIGHTS = (
    (0.6, "docs/genetik-spezifikation.md 3.6 (K3.6a), h2 of the liability (child)"),
    (0.0, "docs/genetik-spezifikation.md 3.6, no shared environment (child)"),
    (0.6, "docs/genetik-spezifikation.md 3.6 (K3.6a), h2 of the liability (adult)"),
    (0.0, "docs/genetik-spezifikation.md 3.6, no shared environment (adult)"),
)


def map_order(locus) -> tuple:
    """Chromosome type (autosome, X, Y, mt), segment, slot (ADR-0007 `data.locus_order`)."""
    return (data_reference.CHROMOSOMES.index(locus["chromosome"]), locus["segment"], locus["slot"])


def liability_loci() -> list:
    """The eight loci of the liability in map order: segment 0 to 7, slot 12."""
    return [
        {
            "id": f"test:liability_{number}",
            "chromosome": "autosome",
            "segment": number - 1,
            "slot": SLOT,
            "action": {"additive": {"effect": 1.0, "d_ref": 1.0, "loads": [{"trait": LIABILITY, "weight": 1.0}]}},
            "mark": "small",
            "mutation": "both",
            "description": f"Locus {number} of the liability of the threshold disease in its test map: effect 1, d_ref 1.",
        }
        for number in range(1, LOCI + 1)
    ]


def genome_map() -> dict:
    """The repository's map with the trait of liability and its eight loci in map order."""
    base = json.loads((REPOSITORY / "genetics" / "genome_map.json").read_text())
    assert base["traits"] == [], "the base map has no traits until the pre-run (ADR-0008 section 4)"
    doc = copy.deepcopy(base)
    doc["traits"] = [
        {
            "id": LIABILITY,
            "sigma_ref": SIGMA_REF,
            "exclusive": False,
            "includes": [],
            "description": "Liability of the threshold disease (genetics specification 3.6) in its test map: a factor of its own with 8 additive loci (effect 1, d_ref 1); sigma_ref 2, so G has variance 1 for alleles drawn with frequency 0.5.",
        }
    ]
    doc["loci"] = sorted(base["loci"] + liability_loci(), key=map_order)
    return doc


def weights_row() -> dict:
    row = {"trait": LIABILITY}
    for name, (value, source) in zip(data_reference.WEIGHTS, WEIGHTS):
        row[name] = {
            "value": value,
            "unit": "fraction",
            "source": source,
            "as_of": AS_OF,
            "note": "phenotype weight of the liability in the test map of the threshold disease (design note, section 4)",
        }
    return row


def threshold_carrier(doc: dict) -> dict:
    """The carrier of the threshold disease `core:disease_liability` in a calibration."""
    ((carrier,),) = [[d["carrier"]["threshold"]] for d in doc["diseases"] if d["code"] == LIABILITY]
    return carrier


def calibration() -> dict:
    """The repository's calibration with the row of the liability and the disease naming it."""
    base = json.loads((REPOSITORY / "calibration" / "genetics.json").read_text())
    assert base["traits"] == [] and threshold_carrier(base)["liability"] is None
    doc = copy.deepcopy(base)
    doc["traits"] = [weights_row()]
    threshold_carrier(doc)["liability"] = LIABILITY
    return doc


def check(map_doc: dict, calibration_doc: dict) -> None:
    """The rules of ADR-0007, ADR-0008 and ADR-0009 the fixture must meet, as a reader
    checks them (as tools/pairing_fixture.py), and the rules of the liability
    (`data_reference.check_liability`)."""
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
    assert loaded == {LIABILITY: LOCI}, loaded
    rows = [row["trait"] for row in calibration_doc["traits"]]
    assert rows == sorted(rows) == ids
    for row in calibration_doc["traits"]:
        values = [row[name]["value"] for name in data_reference.WEIGHTS]
        assert all(row[name]["unit"] == "fraction" for name in data_reference.WEIGHTS)
        assert all(0.0 <= value <= 1.0 for value in values)
        assert values[0] + values[1] <= 1.0 and values[2] + values[3] <= 1.0
    data_reference.check_liability(calibration_doc, map_doc)


def text(doc: dict) -> str:
    return json.dumps(doc, indent=2, ensure_ascii=False) + "\n"


def layout(map_doc: dict) -> list:
    return [
        "layout map order: "
        + " ".join(f"{locus['id']}@{locus['chromosome'][0]}{locus['segment']}.{locus['slot']}" for locus in map_doc["loci"])
    ]


# The rejection cases of the note's section 4 (and two more of section 1), each the
# fixture's documents changed in one place: (name, change of (calibration, map)). Each must
# fail `data_reference.documents_hash` with `data.disease`; the core answers
# `DataError::Disease`.
def _row(calibration_doc: dict) -> dict:
    (row,) = calibration_doc["traits"]
    return row


def _set_row(*values):
    def change(calibration_doc, map_doc):
        for name, value in zip(data_reference.WEIGHTS, values):
            _row(calibration_doc)[name]["value"] = value
    return change


def _heritability(value):
    def change(calibration_doc, map_doc):
        threshold_carrier(calibration_doc)["heritability"]["value"] = value
    return change


def _renamed(calibration_doc, map_doc):
    """The trait, its loads and its row renamed `test:liability` (no clash with a locus: the
    loci are `test:liability_1` to `_8`), the disease naming it: everything valid but the
    ID, which is not the disease's code."""
    new = "test:liability"
    map_doc["traits"][0]["id"] = new
    for locus in map_doc["loci"]:
        action = locus["action"]
        if isinstance(action, dict):
            for load in action["additive"]["loads"]:
                if load["trait"] == LIABILITY:
                    load["trait"] = new
    _row(calibration_doc)["trait"] = new
    threshold_carrier(calibration_doc)["liability"] = new


def _missing_trait(calibration_doc, map_doc):
    """The disease names `test:no_such_trait`, which the map does not have (the map and the
    row of the liability stay)."""
    threshold_carrier(calibration_doc)["liability"] = "test:no_such_trait"


def _no_row(calibration_doc, map_doc):
    """The calibration without the row of the liability (the map's trait stays)."""
    calibration_doc["traits"] = []


NEXT_AFTER_06 = struct.unpack("<d", struct.pack("<q", struct.unpack("<q", struct.pack("<d", 0.6))[0] + 1))[0]
REJECTIONS = [
    ("h2 0.5 in both ages, the heritability 0.6", _set_row(0.5, 0.0, 0.5, 0.0)),
    ("the heritability one ulp above 0.6 (0.6000000000000001), the row 0.6", _heritability(NEXT_AFTER_06)),
    ("c2 0.05 in both ages (h2 0.6)", _set_row(0.6, 0.05, 0.6, 0.05)),
    ("h2 child 0.5, adult 0.6 (c2 0)", _set_row(0.5, 0.0, 0.6, 0.0)),
    ("c2 child 0.05, adult 0 (h2 0.6)", _set_row(0.6, 0.05, 0.6, 0.0)),
    ("the trait test:liability (in the map, row h2 0.6 c2 0) is not the code core:disease_liability", _renamed),
    ("the disease names test:no_such_trait, no trait of the map", _missing_trait),
    ("no phenotype row for core:disease_liability", _no_row),
]
assert NEXT_AFTER_06 == 0.6000000000000001


def rejection_rows(map_doc: dict, calibration_doc: dict) -> list:
    """Each rejection case: the reference's message (`data.disease: ...`). The fixture
    itself is accepted."""
    demography = json.loads((REPOSITORY / "calibration" / "demography.json").read_text())
    data_reference.documents_hash(demography, calibration_doc, map_doc)
    rows = []
    for name, change in REJECTIONS:
        changed_calibration, changed_map = copy.deepcopy(calibration_doc), copy.deepcopy(map_doc)
        change(changed_calibration, changed_map)
        try:
            data_reference.documents_hash(demography, changed_calibration, changed_map)
        except AssertionError as error:
            message = str(error)
            assert message.startswith("data.disease: "), message
            rows.append(f"liability_rejected {name}: {message}")
            continue
        raise AssertionError(f"the data check accepts: {name}")
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
        print("data_liability matches the reference" if same else "data_liability DIFFERS from the reference")
        if not same:
            return 1
    else:
        for path, content in files.items():
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content)
    print(f"data_hash_liability {data_reference.data_hash(FIXTURE):016x}")
    for code, trait, heritability, prevalence, value in data_reference.liability_diseases(calibration_doc):
        print(
            f"threshold {code} liability {trait} heritability {heritability!r} prevalence {prevalence!r}"
            f" 1-K bits {data_reference.f64_bits(1.0 - prevalence)} T {value!r} bits {data_reference.f64_bits(value)}"
        )
    print(f"sigma_ref {SIGMA_REF!r} map_version {map_doc['map_version']} loci {len(map_doc['loci'])} str_key {str_key(LIABILITY):016x}")
    for row in layout(map_doc):
        print(row)
    for row in rejection_rows(map_doc, calibration_doc):
        print(row)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

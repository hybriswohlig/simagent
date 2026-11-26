#!/usr/bin/env python3
"""The scenario of the sheet of portraits (ADR-0021 section 5, G10, and its addendum of
2026-10-24; Grok's round 37, B1 to B8), written from those decisions and the map
`data/genetics/genome_map.json`, independent of the Rust code.

Ten founders, every one naming every locus of the map in its order, so that none draws a
locus (a founder draws the loci it does not name): five variants, each a woman, then a man,
in this order (B8): early maturation, the base, late maturation, red hair, a light iris. All
born at −61 years, so that at the second 0 each has reached every age the sheet shows (2, 8,
12, 16, 25, 40 and 60); the switch of the hereditary diseases off, no nutrition years, no
couples, no capacity, no pool; the run ends at the second 0.

The genomes, by locus (B1 to B6):

- every additive autosomal locus but the liability loci `core:disease_liability_*`: `[0, 1]`
  (the mother's copy, then the father's), the dose 1 of the mean phenotype (B1);
- the liability loci, `core:disease_ar`, `core:disease_ad_late` and
  `core:disease_ad_partial`: `[0, 0]`; `core:disease_mt`: `[0]`; `core:disease_xr`: a woman
  `[0, 0]`, a man the woman's first copy, `[0]` (B5, B6);
- `core:puberty_01` to `_06`: `[0, 0]` for early maturation, `[1, 1]` for late (allele 1 raises
  the phenotype and so the timing), `[0, 1]` otherwise (B2);
- `core:mc1r`: `[1, 1]` for red hair, `[0, 0]` otherwise (B3);
- `core:herc2`: `[1, 1]` for a light iris, `[0, 0]` otherwise (B4).

Checked: 161 additive autosomal loci on map 5 (40 of them the temperament loci), 163 on map 6
(ADR-0034: the two loci of `core:hair_thickness` added, dose 1 by B1), of them 8 liability
loci, and the 7 others the map has
(`core:disease_ar`, `core:mc1r`, `core:herc2`, `core:disease_ad_late`,
`core:disease_ad_partial` on autosomes, `core:disease_xr` on the X, `core:disease_mt` on the
mt); no Y locus.

Usage: $HOME/.cache/simagent-tools-venv/bin/python sheet_reference.py [--write]   (from tools/;
with --write the scenario goes to data/scenarios/bogen.json, else the summary only; the summary
ends with the scenario of map 6 built in memory, `data_reference.map6_documents`)
"""

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MAP = ROOT / "data/genetics/genome_map.json"
OUT = ROOT / "data/scenarios/bogen.json"

YEAR = 364 * 86_400
BORN = -61 * YEAR
VARIANTS = ("early", "base", "late", "red", "light_iris")
PUBERTY = tuple(f"core:puberty_{n:02}" for n in range(1, 7))
LIABILITY = tuple(f"core:disease_liability_{n:02}" for n in range(1, 9))
OTHERS = {
    "core:disease_ar": "autosome",
    "core:mc1r": "autosome",
    "core:herc2": "autosome",
    "core:disease_ad_late": "autosome",
    "core:disease_ad_partial": "autosome",
    "core:disease_xr": "x",
    "core:disease_mt": "mt",
}


# The additive autosomal loci per map version: map 5 (temperament), map 6 (and hair).
ADDITIVE = {5: 161, 6: 163}


def loci(genome_map=None) -> list:
    """The map's loci in its order (data/genetics/genome_map.json, or `genome_map`), each
    (id, chromosome, additive), with the counts above."""
    genome_map = json.loads(MAP.read_text()) if genome_map is None else genome_map
    found = [
        (locus["id"], locus["chromosome"], isinstance(locus["action"], dict) and "additive" in locus["action"])
        for locus in genome_map["loci"]
    ]
    additive = [lid for lid, chromosome, add in found if add and chromosome == "autosome"]
    others = {lid: chromosome for lid, chromosome, add in found if not add}
    assert len(additive) == ADDITIVE[genome_map["map_version"]], len(additive)
    assert set(LIABILITY) <= set(additive) and set(PUBERTY) <= set(additive)
    assert others == OTHERS, others
    assert all(chromosome in ("autosome", "x", "mt") for _, chromosome, _ in found), "no Y locus"
    return found


def copies(lid: str, chromosome: str, additive: bool, variant: str, sex: str) -> list:
    """The copies of the locus `lid` for the variant and the sex (B1 to B6)."""
    if lid in PUBERTY:
        return {"early": [0, 0], "late": [1, 1]}.get(variant, [0, 1])
    if lid == "core:mc1r":
        return [1, 1] if variant == "red" else [0, 0]
    if lid == "core:herc2":
        return [1, 1] if variant == "light_iris" else [0, 0]
    if chromosome == "mt":
        return [0]
    if chromosome == "x":
        return [0, 0] if sex == "female" else [0]
    if additive and lid not in LIABILITY:
        return [0, 1]
    return [0, 0]


def founders(genome_map=None) -> list:
    """The ten founders in the order of B8, each a woman, then a man."""
    found = loci(genome_map)
    return [
        {
            "born": BORN,
            "sex": sex,
            "alleles": [
                {"locus": lid, "copies": copies(lid, chromosome, additive, variant, sex)}
                for lid, chromosome, additive in found
            ],
        }
        for variant in VARIANTS
        for sex in ("female", "male")
    ]


def scenario_text(genome_map=None) -> str:
    """The scenario, format 5, as the repository writes scenarios: two spaces, one allele per
    line."""
    people = []
    for founder in founders(genome_map):
        alleles = ",\n".join(f"          {json.dumps(allele)}" for allele in founder["alleles"])
        people.append(
            "      {\n"
            f'        "born": {founder["born"]},\n'
            f'        "sex": "{founder["sex"]}",\n'
            '        "alleles": [\n'
            f"{alleles}\n"
            "        ]\n"
            "      }"
        )
    body = ",\n".join(people)
    return (
        "{\n"
        '  "format_version": 5,\n'
        '  "start": {\n'
        '    "world_seed": "000000000000b06e",\n'
        '    "founder_tag": "base:bogen",\n'
        '    "world_params": {\n'
        '      "hereditary_diseases": false,\n'
        '      "nutrition": []\n'
        "    },\n"
        '    "founders": [\n'
        f"{body}\n"
        "    ],\n"
        '    "couples": [],\n'
        '    "capacity": null,\n'
        '    "pool": null\n'
        "  },\n"
        '  "until": 0\n'
        "}\n"
    )


def main(argv) -> int:
    text = scenario_text()
    parsed = json.loads(text)
    assert len(parsed["start"]["founders"]) == 10
    for variant, people in zip(VARIANTS, zip(*[iter(parsed["start"]["founders"])] * 2)):
        named = {allele["locus"]: allele["copies"] for allele in people[0]["alleles"]}
        print(f"{variant}: puberty_01 {named['core:puberty_01']}, mc1r {named['core:mc1r']}, herc2 {named['core:herc2']}")
    # Map 6 (ADR-0034), built in memory: 163 additive loci, the hair loci at dose 1 (B1).
    import data_reference
    import xxhash

    _, _, map6 = data_reference.map6_documents()
    six = scenario_text(map6)
    named = {allele["locus"]: allele["copies"] for allele in json.loads(six)["start"]["founders"][0]["alleles"]}
    additive = sum(1 for _, chromosome, add in loci(map6) if add and chromosome == "autosome")
    print(
        f"map6: additive {additive}, alleles per founder {len(named)}, hair_thickness_01 {named['core:hair_thickness_01']},"
        f" hair_thickness_02 {named['core:hair_thickness_02']}, scenario xxh3 {xxhash.xxh3_64_intdigest(six.encode(), seed=0):016x}"
    )
    if "--write" in argv[1:]:
        OUT.write_text(text)
        print(f"wrote {OUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

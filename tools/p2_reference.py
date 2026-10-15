#!/usr/bin/env python3
"""Expected values of criterion P2 (recessive disease), from the specification alone.

Sources: docs/genetik-spezifikation.md 8.1 and 8.2 (P2), 2.3 and 2.4 (meiosis and
mutation, through tools/meiosis_reference.py), 2.6 (W2.6a), 3.1 and 3.2 (penetrance and
the switch); ADR-0020 section 8; the plan of step 11, section 2, P2. The Rust core was not
read.

Fixed inputs, chosen before any run:
- Seed 0x5eed_0000_0000_0002, genome keys 0 .. 1999.
- Mother "Aa": the alternative allele (1) of core:disease_ar on her maternal autosome
  copy, the reference allele (0) on her paternal copy, 0 at every other slot. Two X
  (x_maternal, x_paternal), both 0; mt 0.
- Father "Aa": the same autosomes; x_maternal 0, paternal_sex {"y": 0 x 8}, mt 0.
- Negative twin: father "AA", every allele 0; same mother, seed and keys.
- The repository data: map lengths, sex ratio, mutation rates (data/calibration/
  genetics.json) and the genome map (data/genetics/genome_map.json). The data map writes
  X and mt loci with segment 0 and the direction as "mutation"; meiosis_reference wants
  X at segment 8 and the key "direction" (the same conversion as world_reference.py).

Each child is meiosis_reference.child_with_mutation: both meioses (2.3), then mutation of
each side (2.4). d is the number of alternative alleles of the child at core:disease_ar,
after mutation (the genome the world sees).

Tolerance (8.1): 4 standard errors of the share under the model, at the actual n:
tol = 4 sqrt(p0 (1 - p0) / n), p0 = 1/4. Power against 1/3 at n: P(|share - 1/4| > tol)
with share ~ N(1/3, (1/3)(2/3) / n). At n = 2000 they are W8.2a and W8.2b; the script
checks its formulas against the values printed by tools/spec_reference.py --fast.

Usage: python tools/p2_reference.py --out DIR
       (writes DIR/p2_expected.json and prints `label value` lines)
"""

import argparse
import json
import subprocess
import sys
from fractions import Fraction
from pathlib import Path

import mpmath

import meiosis_reference as mr

ROOT = Path(__file__).resolve().parent.parent
SEED = 0x5EED_0000_0000_0002
N = 2000
LOCUS = "core:disease_ar"
P0 = Fraction(1, 4)
P_ALT = Fraction(1, 3)
Z_TOL = 4


def repository():
    """Map lengths, sex ratio, rates and the genome map in meiosis_reference form."""
    calibration = json.loads((ROOT / "data" / "calibration" / "genetics.json").read_text())
    data_map = json.loads((ROOT / "data" / "genetics" / "genome_map.json").read_text())
    values = calibration["values"]
    rates = {
        "standard": values["core:mutation_rate"]["value"],
        "disease": values["core:mutation_rate_disease"]["value"],
    }
    genome_map = [
        {
            "id": locus["id"],
            "chromosome": locus["chromosome"],
            "segment": locus["segment"]
            if locus["chromosome"] == "autosome"
            else (mr.X_SEGMENT if locus["chromosome"] == "x" else 0),
            "slot": locus["slot"],
            "mark": locus["mark"],
            "direction": locus["mutation"],
        }
        for locus in data_map["loci"]
    ]
    penetrance = None
    for disease in calibration["diseases"]:
        if disease["code"] == LOCUS:
            carrier = disease["carrier"]["locus"]
            assert carrier["locus"] == LOCUS
            penetrance = (carrier["penetrance_female"]["value"], carrier["penetrance_male"]["value"])
    assert penetrance is not None, "no calibration entry for core:disease_ar"
    return mr.load_map_lengths(), mr.load_sex_ratio(), rates, genome_map, data_map, penetrance


def place(data_map):
    (locus,) = [entry for entry in data_map["loci"] if entry["id"] == LOCUS]
    assert locus["chromosome"] == "autosome", locus
    assert locus["action"] == "recessive" and locus["mark"] == "disease", locus
    assert locus["mutation"] == "to_alt", locus
    return locus["segment"], locus["slot"], locus


def zero_autosomes():
    return [[0] * mr.AUTOSOME_SLOTS for _ in range(mr.AUTOSOME_SEGMENTS)]


def mother_aa(segment, slot):
    maternal = zero_autosomes()
    maternal[segment][slot] = 1
    return {
        "autosomes_maternal": maternal,
        "autosomes_paternal": zero_autosomes(),
        "x_maternal": [0] * mr.X_SLOTS,
        "x_paternal": [0] * mr.X_SLOTS,
        "mt": [0] * mr.MT_SLOTS,
    }


def father(segment, slot, carrier):
    maternal = zero_autosomes()
    if carrier:
        maternal[segment][slot] = 1
    return {
        "autosomes_maternal": maternal,
        "autosomes_paternal": zero_autosomes(),
        "x_maternal": [0] * mr.X_SLOTS,
        "paternal_sex": {"y": [0] * mr.Y_SLOTS},
        "mt": [0] * mr.MT_SLOTS,
    }


def genome_words(person):
    """The packed words of tools/postcard_reference.genome_of: autosome segment s, slot i
    is bit 24 s + i of three u64 words; X, Y and mt slot i is bit i."""

    def autosomes(rows):
        words = [0, 0, 0]
        for segment, row in enumerate(rows):
            for slot, allele in enumerate(row):
                at = 24 * segment + slot
                words[at // 64] |= allele << (at % 64)
        return [f"{word:016x}" for word in words]

    def bits(slots):
        return sum(allele << slot for slot, allele in enumerate(slots))

    out = {
        "autosomes_maternal": autosomes(person["autosomes_maternal"]),
        "autosomes_paternal": autosomes(person["autosomes_paternal"]),
        "x_maternal": bits(person["x_maternal"]),
    }
    if "x_paternal" in person:
        out["x_paternal"] = bits(person["x_paternal"])
    else:
        ((name, slots),) = person["paternal_sex"].items()
        out["paternal_sex"] = {name: bits(slots)}
    out["mt"] = bits(person["mt"])
    return out


def side_detail(parent, record, segment, slot):
    """Start strand, crossovers, transmitted strand and allele at the locus before
    mutation, and the mutations of that side at the locus."""
    (seg,) = [s for s in record["segments"] if s["segment"] == segment]
    strand = mr.transmitted_strand(seg["start_strand"], seg["crossover_positions"], slot, mr.AUTOSOME_SLOTS)
    copy = parent["autosomes_maternal"] if strand == 0 else parent["autosomes_paternal"]
    return {
        "start_strand": seg["start_strand"],
        "crossover_positions": [p.hex() for p in seg["crossover_positions"]],
        "transmitted_strand": strand,
        "allele_before_mutation": copy[segment][slot],
        "mutations_at_locus": [m for m in record["mutations"] if m["locus"] == LOCUS],
        "mutations": record["mutations"],
    }


def run(mother, dad, data, segment, slot):
    map_lengths, sex_ratio, rates, genome_map = data
    d_string = []
    sexes = []
    counts = [0, 0, 0]
    detail_by_key = {}
    all_mutations = []
    for key in range(N):
        genome, records = mr.child_with_mutation(
            SEED, key, mother, dad, map_lengths, sex_ratio, genome_map, rates
        )
        d = genome["autosomes_maternal"][segment][slot] + genome["autosomes_paternal"][segment][slot]
        counts[d] += 1
        d_string.append(str(d))
        sexes.append("m" if "y" in genome["paternal_sex"] else "f")
        m_side = side_detail(mother, records["mother"], segment, slot)
        f_side = side_detail(dad, records["father"], segment, slot)
        # Cross-check: allele after mutation = allele before, flipped by each mutation at the locus.
        for side, got in ((m_side, genome["autosomes_maternal"][segment][slot]),
                          (f_side, genome["autosomes_paternal"][segment][slot])):
            allele = side["allele_before_mutation"]
            for m in side["mutations_at_locus"]:
                assert m["old"] == allele
                allele = m["new"]
            assert allele == got, (key, side, got)
        for name, rec in (("mother", records["mother"]), ("father", records["father"])):
            for m in rec["mutations"]:
                all_mutations.append({"genome_key": f"{key:016x}", "side": name, **m})
        detail_by_key[key] = {"mother": m_side, "father": f_side, "sex": sexes[-1]}
    return "".join(d_string), "".join(sexes), counts, detail_by_key, all_mutations


def tolerance_and_power(n):
    """W8.2a and W8.2b formulas (spec 8.1, tools/spec_reference.proportion) at n."""
    with mpmath.workdps(40):
        p0 = mpmath.mpf(1) / 4
        alt = mpmath.mpf(1) / 3
        se0 = mpmath.sqrt(p0 * (1 - p0) / n)
        tol = Z_TOL * se0
        se_alt = mpmath.sqrt(alt * (1 - alt) / n)
        pw = mpmath.ncdf((p0 - tol - alt) / se_alt) + 1 - mpmath.ncdf((p0 + tol - alt) / se_alt)
        return float(tol), float(pw), mpmath.nstr(tol, 20), mpmath.nstr(pw, 20)


def spec_values():
    out = subprocess.run(
        [sys.executable, str(ROOT / "tools" / "spec_reference.py"), "--fast"],
        check=True, capture_output=True, text=True,
    ).stdout
    values = dict(line.split("\t") for line in out.splitlines() if "\t" in line)
    return values["W8.2a"], values["W8.2b"], values["W2.6a"]


def german_fix(value, digits):
    return f"{value:.{digits}f}".replace(".", ",")


def verdict(count2, n, tol):
    share = Fraction(count2, n)
    deviation = abs(share - P0)
    return share, deviation, float(deviation) <= tol


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()
    assert mpmath.mp.dps == 60, "meiosis_reference needs mpmath at 60 digits"

    map_lengths, sex_ratio, rates, genome_map, data_map, penetrance = repository()
    segment, slot, locus = place(data_map)
    data = (map_lengths, sex_ratio, rates, genome_map)

    w82a, w82b, w26a = spec_values()
    tol, pw, tol_text, pw_text = tolerance_and_power(N)
    assert german_fix(tol, 4) == w82a, (tol, w82a)
    assert german_fix(pw, 4) == w82b, (pw, w82b)
    assert w26a == "1/4", w26a

    mother = mother_aa(segment, slot)
    father_aa_carrier = father(segment, slot, True)
    father_hom_ref = father(segment, slot, False)

    p2 = run(mother, father_aa_carrier, data, segment, slot)
    twin = run(mother, father_hom_ref, data, segment, slot)

    p2_share, p2_dev, p2_green = verdict(p2[2][2], N, tol)
    tw_share, tw_dev, tw_green = verdict(twin[2][2], N, tol)

    twin_d2 = []
    for key, d in enumerate(twin[0]):
        if d == "2":
            det = twin[3][key]
            new_at_locus = det["father"]["mutations_at_locus"] + det["mother"]["mutations_at_locus"]
            twin_d2.append({"genome_key": f"{key:016x}", "record": det,
                            "has_new_mutation_at_locus": any(m["new"] == 1 for m in new_at_locus)})
    # Every d = 2 child of the twin must carry a new mutation at the locus (plan, I-d).
    twin_d2_without_mutation = [c["genome_key"] for c in twin_d2 if not c["has_new_mutation_at_locus"]]

    p2_d2_with_mutation = [
        f"{k:016x}" for k in range(N)
        if p2[3][k]["mother"]["mutations_at_locus"] or p2[3][k]["father"]["mutations_at_locus"]
    ]

    result = {
        "seed": f"{SEED:016x}",
        "genome_keys": f"0 .. {N - 1}",
        "locus": {"id": LOCUS, "chromosome": locus["chromosome"], "segment": segment, "slot": slot,
                  "action": locus["action"], "mark": locus["mark"], "mutation": locus["mutation"]},
        "data": {"map_lengths": map_lengths, "sex_ratio": sex_ratio, "rates": rates,
                 "genome_map_version": data_map["map_version"],
                 "penetrance_female_male": list(penetrance)},
        "parents": {
            "mother_Aa": {"lists": mother, "words": genome_words(mother)},
            "father_Aa": {"lists": father_aa_carrier, "words": genome_words(father_aa_carrier)},
            "father_AA": {"lists": father_hom_ref, "words": genome_words(father_hom_ref)},
        },
        "tolerance": {"n": N, "p0": "1/4", "p_alt": "1/3", "tol": tol, "tol_40_digits": tol_text,
                      "power": pw, "power_40_digits": pw_text,
                      "spec_reference": {"W8.2a": w82a, "W8.2b": w82b, "W2.6a": w26a}},
        "p2": {"counts_d0_d1_d2": p2[2], "share_d2": str(p2_share), "share_d2_float": float(p2_share),
               "abs_deviation": float(p2_dev), "within_tolerance": p2_green,
               "sons": p2[1].count("m"), "daughters": p2[1].count("f"),
               "affected_switch_on": p2[2][2], "affected_d1": 0, "affected_switch_off": 0,
               "children_with_mutation_at_locus": p2_d2_with_mutation,
               "all_mutations": p2[4], "d_by_key": p2[0], "sex_by_key": p2[1]},
        "twin": {"counts_d0_d1_d2": twin[2][:], "share_d2": str(tw_share), "share_d2_float": float(tw_share),
                 "abs_deviation": float(tw_dev), "within_tolerance": tw_green, "p2_check_red": not tw_green,
                 "d2_children": twin_d2, "d2_without_new_mutation": twin_d2_without_mutation,
                 "sons": twin[1].count("m"), "daughters": twin[1].count("f"),
                 "all_mutations": twin[4], "d_by_key": twin[0], "sex_by_key": twin[1]},
    }
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "p2_expected.json").write_text(json.dumps(result, indent=1) + "\n")

    print(f"seed {SEED:016x}")
    print(f"locus {LOCUS} {locus['chromosome']} segment {segment} slot {slot}")
    print(f"tol {tol!r} (W8.2a {w82a})")
    print(f"power {pw!r} (W8.2b {w82b})")
    print(f"p2 counts d0 d1 d2 {p2[2]}")
    print(f"p2 share_d2 {p2_share} = {float(p2_share)!r} deviation {float(p2_dev)!r} within {p2_green}")
    print(f"p2 sons {p2[1].count('m')} daughters {p2[1].count('f')}")
    print(f"p2 mutations {len(p2[4])} at locus {p2_d2_with_mutation}")
    print(f"twin counts d0 d1 d2 {twin[2]}")
    print(f"twin share_d2 {tw_share} = {float(tw_share)!r} deviation {float(tw_dev)!r} red {not tw_green}")
    print(f"twin d2 children {[c['genome_key'] for c in twin_d2]} without new mutation {twin_d2_without_mutation}")
    print(f"twin mutations {len(twin[4])}")
    if twin_d2_without_mutation:
        raise SystemExit("a d = 2 child of the twin has no new mutation at the locus")
    if tw_green:
        raise SystemExit("the P2 check is green on the twin")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Independent reference for meiosis, from the specification alone.

Sources: docs/genetik-spezifikation.md section 2 (2.1 layout, 2.2 map, 2.3
meiosis) and the addressed draws of docs/adr/0001-zufall-und-gabeln.md sections
2 and 3. Mutation (section 2.4) is not implemented. The Rust core was not read
(docs/vorbereitung-phase-0.md B13). Randomness goes only through
tools/rng_reference.py.

Layout (2.1, 2.2)
- Every occupied slot is a locus with alleles 0 and 1. This reference transmits
  every slot; it does not consult the genome map.
- Autosomes: 8 segments (indexes 0..7), 24 slots each.
- X: 16 slots, one segment, index 8. Y: 8 slots. mt: 8 slots.
- Two autosomal copies, maternal and paternal. A woman has two X copies. A man
  has the X he received from his mother and a Y. mt is one copy, from the mother.
- Slot i of a segment with n slots lies at u_i = (i + 0.5) / n, evaluated as a
  Python float. Two slots of the same segment, in a meiosis of sex s, are
  |u_i - u_j| * L_s apart. Slots on different segments are unlinked.

Map lengths (2.2), in Morgan, passed in as map_lengths
- "autosome_female": L_f, mother's autosomes
  (calibration core:map_length_autosome_female)
- "autosome_male": L_m, father's autosomes
  (core:map_length_autosome_male)
- "x_female": L_X, mother's X (core:map_length_x_female)
The father's X does not recombine. Y and mt never recombine. The CLI reads the
three values from data/calibration/genetics.json.

Draws (2.3, ADR-0001 sections 2 and 3)
- key = chacha_key(seed, tag, subject, occasion, detail); the distribution uses
  words(key) from the start, so only words[0].
- uniform_below(words[0], n), unit(words[0]), and poisson_reference(words[0], L).
  A None from poisson_reference is an error (the reference itself returns the
  exact count, or asserts).
- Subject of every meiosis draw: the child's genome_key.
- Occasion: the side, 0 mother, 1 father. SEX_CHROMOSOME always uses occasion 1
  and detail 0, as specified, not the side argument.
- Tags: MEIOSIS_START_STRAND = 1, SEX_CHROMOSOME = 2, CROSSOVER_COUNT = 6,
  CROSSOVER_POSITION = 7. The specification names the two crossover tags and
  calls them new; 6 and 7 are the next integers after PAIRING_RANK = 5 in
  ADR-0001 section 5. They are not in that table yet.
- Detail of the start strand and of the crossover count: the segment index s.
- Detail of crossover position j on segment s: (s << 32) | j, j = 0, 1, ...

For each segment the parent carries in two copies (autosomes, and the mother's
X), in segment order:

1. Start strand: uniform_below(word, 2). 0 is the copy the parent received from
   its mother, 1 the copy from its father.
2. Crossover count k = poisson(L), L the map length of that segment for the
   parent's sex.
3. Positions c_j = unit(), one draw each, kept in draw order j = 0 .. k-1.
   They are not sorted.
4. Slot i is copied from strand
   start XOR ((number of j with c_j < u_i) mod 2).
   Equality c_j == u_i does not count. For a start bit in {0, 1} this is the
   same as (start XOR that number) mod 2.

Sex chromosomes and mt (2.3)
- The father draws SEX_CHROMOSOME: uniform_below(word, 2), 0 transmits his X
  (x_maternal) unchanged, 1 transmits his Y unchanged. No further draws.
- The mother transmits an X by steps 1 to 4 (segment 8).
- mt is copied from the mother. No draw.
- No draws for the father's X, for Y beyond the sex-chromosome choice, or for mt.

Genome dict
- "autosomes_maternal" and "autosomes_paternal": 8 lists of 24 ints, each 0 or 1.
- "x_maternal": 16 ints.
- "paternal_sex": {"x": 16 ints} or {"y": 8 ints}, the sex chromosome received
  from the father. A female meiosis parent may instead carry that second X as
  "x_paternal" (16 ints). Vector mothers are written that way. Strand 1 of the
  mother's X reads x_paternal if present, otherwise paternal_sex["x"].
- "mt": 8 ints.

meiosis(seed, genome_key, side, parent, map_lengths) returns (gamete, record).
The gamete is {"autosomes": 8 lists of 24, "sex": {"x": ...} or {"y": ...},
"mt": 8 ints for a mother and None for a father}.
The record is {"segments": [{"segment", "start_strand", "crossover_positions"},
...]} in segment order (0..7, and for the mother 8). The father's record also
has "sex_chromosome": "x" or "y".

child(seed, genome_key, mother, father, map_lengths) runs both meioses with the
same seed and genome_key. The child genome takes autosomes and X and mt from
the mother and the paternal autosomes and paternal_sex from the father.
Returns (child genome, {"mother": record, "father": record}).

Vector alleles (--vectors), a rule of this file, not of the specification:
the low bit of xxh3_64 (seed 0) of the UTF-8 text
"{case}:{who}:{track}:{segment}:{slot}". case is the case index 0..5, who is
"mother" or "father", track names the copy. X, Y and mt use segment 0.

Self-check (--self-check): 20000 maternal meioses with seed 1 and genome_key
0 .. 19999. The mother's maternal autosome alleles are 0 and the paternal ones
are 1, so a slot's allele is the transmitted strand. The recombinant fraction
between slot 0 and slot 23 of segment 0 is compared with the Haldane value
(1 - exp(-2 d)) / 2, d = (23 / 24) * L_f (section 2.3). The check passes within
0.01.

Usage:
  python3 tools/meiosis_reference.py --vectors
  python3 tools/meiosis_reference.py --self-check
"""

import argparse
import json
import math
import sys
from pathlib import Path

import mpmath
import xxhash

from rng_reference import chacha_key, poisson_reference, uniform_below, unit, words

# poisson_reference sums the distribution at 60 decimal digits (ADR-0001 section 3).
mpmath.mp.dps = 60

MEIOSIS_START_STRAND = 1
SEX_CHROMOSOME = 2
CROSSOVER_COUNT = 6
CROSSOVER_POSITION = 7

AUTOSOME_SEGMENTS = 8
AUTOSOME_SLOTS = 24
X_SLOTS = 16
Y_SLOTS = 8
MT_SLOTS = 8
X_SEGMENT = 8

SIDE_MOTHER = 0
SIDE_FATHER = 1

U64_MAX = (1 << 64) - 1

# (seed, genome_key), paired in the order given for --vectors.
VECTOR_CASES = [
    (1, 7),
    (2, 0x8000000000000000),
    (0xFF, 1),
    (0x0123456789ABCDEF, 3),
    (U64_MAX, 0xFFFFFFFFFFFFFFFF),
    (42, 0x2545F4914F6CDD1D),
]

SELF_CHECK_N = 20000
SELF_CHECK_SEED = 1
SELF_CHECK_SEGMENT = 0


def u64(name, value):
    if type(value) is not int or not 0 <= value <= U64_MAX:
        raise ValueError(f"{name} must be an integer in 0 .. 2^64-1")
    return value


def alleles(values, n, label):
    if not isinstance(values, list) or len(values) != n:
        raise ValueError(f"{label} must be a list of {n} alleles")
    for allele in values:
        if type(allele) is not int or allele not in (0, 1):
            raise ValueError(f"{label} alleles must be 0 or 1")
    return values


def autosome_copies(parent, label):
    rows = parent[label]
    if not isinstance(rows, list) or len(rows) != AUTOSOME_SEGMENTS:
        raise ValueError(f"{label} must be {AUTOSOME_SEGMENTS} segments")
    for segment, row in enumerate(rows):
        alleles(row, AUTOSOME_SLOTS, f"{label}[{segment}]")
    return rows


def map_length(map_lengths, name):
    if name not in map_lengths:
        raise ValueError(f"map_lengths is missing {name}")
    value = float(map_lengths[name])
    if not 0.0 <= value <= 30.0:
        raise ValueError(f"{name} = {value} is outside the poisson domain [0, 30]")
    return value


def load_map_lengths(path=None):
    """L_f, L_m and L_X from data/calibration/genetics.json (spec 2.2)."""
    if path is None:
        path = Path(__file__).resolve().parent.parent / "data" / "calibration" / "genetics.json"
    raw = json.loads(Path(path).read_text(encoding="utf-8"))
    values = raw["values"]
    return {
        "autosome_female": values["core:map_length_autosome_female"]["value"],
        "autosome_male": values["core:map_length_autosome_male"]["value"],
        "x_female": values["core:map_length_x_female"]["value"],
    }


def first_word(seed, tag, subject, occasion, detail):
    key = chacha_key(seed, tag, subject, occasion, detail)
    return words(key)[0]


def poisson_count(word, lam):
    count = poisson_reference(word, lam)
    if count is None:
        raise RuntimeError(f"poisson_reference returned None for lambda {lam}")
    return count


def slot_position(index, n):
    """u_i = (i + 0.5) / n as a Python float (spec 2.2)."""
    return (index + 0.5) / n


def transmitted_strand(start, positions, index, n):
    """Start strand XOR (count of crossovers strictly below u_i, mod 2)."""
    u = slot_position(index, n)
    below = 0
    for position in positions:
        if position < u:
            below += 1
    return start ^ (below % 2)


def recombine(start, positions, maternal, paternal):
    n = len(maternal)
    if len(paternal) != n:
        raise ValueError("the two copies of a segment differ in length")
    transmitted = []
    for index in range(n):
        strand = transmitted_strand(start, positions, index, n)
        transmitted.append(maternal[index] if strand == 0 else paternal[index])
    return transmitted


def crossover_segment(seed, genome_key, side, segment, length, maternal, paternal):
    """Steps 1 to 4 of spec 2.3 for one segment. Returns (alleles, record)."""
    start_word = first_word(seed, MEIOSIS_START_STRAND, genome_key, side, segment)
    start = uniform_below(start_word, 2)
    count_word = first_word(seed, CROSSOVER_COUNT, genome_key, side, segment)
    k = poisson_count(count_word, length)
    positions = []
    for j in range(k):
        detail = (segment << 32) | j
        position_word = first_word(seed, CROSSOVER_POSITION, genome_key, side, detail)
        positions.append(unit(position_word))
    return recombine(start, positions, maternal, paternal), {
        "segment": segment,
        "start_strand": start,
        "crossover_positions": positions,
    }


def mother_x_copies(parent):
    """The two X copies. Strand 0 is x_maternal, strand 1 is from her father."""
    maternal = alleles(parent["x_maternal"], X_SLOTS, "x_maternal")
    if "x_paternal" in parent:
        paternal = alleles(parent["x_paternal"], X_SLOTS, "x_paternal")
    else:
        sex = parent.get("paternal_sex")
        if not isinstance(sex, dict) or "x" not in sex:
            raise ValueError("mother is missing the X from her father (x_paternal or paternal_sex.x)")
        paternal = alleles(sex["x"], X_SLOTS, "paternal_sex.x")
    return maternal, paternal


def meiosis(seed, genome_key, side, parent, map_lengths):
    """One parent's gamete and the draw record (spec 2.3). side 0 mother, 1 father."""
    seed = u64("seed", seed)
    genome_key = u64("genome_key", genome_key)
    if side not in (SIDE_MOTHER, SIDE_FATHER):
        raise ValueError("side must be 0 (mother) or 1 (father)")
    maternal_autosomes = autosome_copies(parent, "autosomes_maternal")
    paternal_autosomes = autosome_copies(parent, "autosomes_paternal")
    if side == SIDE_MOTHER:
        autosome_length = map_length(map_lengths, "autosome_female")
        x_length = map_length(map_lengths, "x_female")
    else:
        autosome_length = map_length(map_lengths, "autosome_male")
        x_length = None

    gamete_autosomes = []
    segments = []
    for segment in range(AUTOSOME_SEGMENTS):
        transmitted, record = crossover_segment(
            seed,
            genome_key,
            side,
            segment,
            autosome_length,
            maternal_autosomes[segment],
            paternal_autosomes[segment],
        )
        gamete_autosomes.append(transmitted)
        segments.append(record)

    if side == SIDE_MOTHER:
        x_maternal, x_paternal = mother_x_copies(parent)
        transmitted_x, x_record = crossover_segment(
            seed, genome_key, side, X_SEGMENT, x_length, x_maternal, x_paternal
        )
        segments.append(x_record)
        sex = {"x": transmitted_x}
        mt = list(alleles(parent["mt"], MT_SLOTS, "mt"))
        record = {"segments": segments}
    else:
        # The X a man passes on is the one he received from his mother, intact.
        x_maternal = alleles(parent["x_maternal"], X_SLOTS, "x_maternal")
        sex_parent = parent.get("paternal_sex")
        if not isinstance(sex_parent, dict) or "y" not in sex_parent:
            raise ValueError("father is missing paternal_sex.y")
        y_copy = alleles(sex_parent["y"], Y_SLOTS, "paternal_sex.y")
        choice_word = first_word(seed, SEX_CHROMOSOME, genome_key, SIDE_FATHER, 0)
        choice = uniform_below(choice_word, 2)
        if choice == 0:
            sex = {"x": list(x_maternal)}
            sex_name = "x"
        else:
            sex = {"y": list(y_copy)}
            sex_name = "y"
        mt = None
        record = {"segments": segments, "sex_chromosome": sex_name}

    gamete = {"autosomes": gamete_autosomes, "sex": sex, "mt": mt}
    return gamete, record


def child(seed, genome_key, mother, father, map_lengths):
    """Both gametes at one conception. Returns (child genome, both records)."""
    mother_gamete, mother_record = meiosis(seed, genome_key, SIDE_MOTHER, mother, map_lengths)
    father_gamete, father_record = meiosis(seed, genome_key, SIDE_FATHER, father, map_lengths)
    if "x" not in mother_gamete["sex"] or mother_gamete["mt"] is None:
        raise RuntimeError("the mother's gamete is missing X or mt")
    genome = {
        "autosomes_maternal": mother_gamete["autosomes"],
        "autosomes_paternal": father_gamete["autosomes"],
        "x_maternal": mother_gamete["sex"]["x"],
        "paternal_sex": father_gamete["sex"],
        "mt": mother_gamete["mt"],
    }
    return genome, {"mother": mother_record, "father": father_record}


def vector_allele(case_index, who, track, segment, slot):
    """Low bit of xxh3_64 over '{case}:{who}:{track}:{segment}:{slot}'."""
    text = f"{case_index}:{who}:{track}:{segment}:{slot}"
    return xxhash.xxh3_64_intdigest(text.encode("utf-8"), seed=0) & 1


def fill_autosomes(case_index, who, track):
    return [
        [vector_allele(case_index, who, track, segment, slot) for slot in range(AUTOSOME_SLOTS)]
        for segment in range(AUTOSOME_SEGMENTS)
    ]


def fill_slots(case_index, who, track, n):
    return [vector_allele(case_index, who, track, 0, slot) for slot in range(n)]


def vector_mother(case_index):
    return {
        "autosomes_maternal": fill_autosomes(case_index, "mother", "autosomes_maternal"),
        "autosomes_paternal": fill_autosomes(case_index, "mother", "autosomes_paternal"),
        "x_maternal": fill_slots(case_index, "mother", "x_maternal", X_SLOTS),
        "x_paternal": fill_slots(case_index, "mother", "x_paternal", X_SLOTS),
        "mt": fill_slots(case_index, "mother", "mt", MT_SLOTS),
    }


def vector_father(case_index):
    return {
        "autosomes_maternal": fill_autosomes(case_index, "father", "autosomes_maternal"),
        "autosomes_paternal": fill_autosomes(case_index, "father", "autosomes_paternal"),
        "x_maternal": fill_slots(case_index, "father", "x_maternal", X_SLOTS),
        "paternal_sex": {"y": fill_slots(case_index, "father", "y", Y_SLOTS)},
        "mt": fill_slots(case_index, "father", "mt", MT_SLOTS),
    }


def vector_cases(map_lengths=None):
    if map_lengths is None:
        map_lengths = load_map_lengths()
    cases = []
    for case_index, (seed, genome_key) in enumerate(VECTOR_CASES):
        mother = vector_mother(case_index)
        father = vector_father(case_index)
        genome, records = child(seed, genome_key, mother, father, map_lengths)
        cases.append(
            {
                "seed": seed,
                "genome_key": f"{genome_key:016x}",
                "mother": mother,
                "father": father,
                "child": genome,
                "records": records,
            }
        )
    return cases


def contrast_mother():
    """Maternal autosome alleles 0, paternal 1, so the gamete allele is the strand."""
    return {
        "autosomes_maternal": [[0] * AUTOSOME_SLOTS for _ in range(AUTOSOME_SEGMENTS)],
        "autosomes_paternal": [[1] * AUTOSOME_SLOTS for _ in range(AUTOSOME_SEGMENTS)],
        "x_maternal": [0] * X_SLOTS,
        "x_paternal": [1] * X_SLOTS,
        "mt": [0] * MT_SLOTS,
    }


def haldane(distance):
    """r(d) = (1 - exp(-2 d)) / 2 (spec 2.3, Haldane 1919)."""
    return (1.0 - math.exp(-2.0 * distance)) / 2.0


def self_check(map_lengths=None):
    if map_lengths is None:
        map_lengths = load_map_lengths()
    length = map_length(map_lengths, "autosome_female")
    distance = (23 / 24) * length
    expected = haldane(distance)
    parent = contrast_mother()
    recombinants = 0
    for genome_key in range(SELF_CHECK_N):
        gamete, _record = meiosis(
            SELF_CHECK_SEED, genome_key, SIDE_MOTHER, parent, map_lengths
        )
        alleles_row = gamete["autosomes"][SELF_CHECK_SEGMENT]
        if alleles_row[0] != alleles_row[23]:
            recombinants += 1
    observed = recombinants / SELF_CHECK_N
    difference = abs(observed - expected)
    print(f"n {SELF_CHECK_N}")
    print(f"seed {SELF_CHECK_SEED}")
    print(f"segment {SELF_CHECK_SEGMENT}")
    print("slots 0 23")
    print(f"L_f {length!r}")
    print(f"d {distance!r}")
    print(f"haldane {expected!r}")
    print(f"observed {observed!r}")
    print(f"abs_diff {difference!r}")
    if difference > 0.01:
        raise SystemExit(
            f"self-check failed: recombinant fraction {observed} differs from "
            f"Haldane {expected} by {difference}, more than 0.01"
        )
    print("within 0.01 of the Haldane value")


def main():
    parser = argparse.ArgumentParser(description="Independent meiosis reference (spec 2.1 to 2.3).")
    parser.add_argument("--vectors", action="store_true", help="print six JSON cases")
    parser.add_argument("--self-check", action="store_true", help="Haldane check on 20000 meioses")
    args = parser.parse_args()
    if args.vectors == args.self_check:
        parser.error("pass exactly one of --vectors or --self-check")
    if args.vectors:
        json.dump({"cases": vector_cases()}, sys.stdout, indent=1)
        sys.stdout.write("\n")
    else:
        self_check()


if __name__ == "__main__":
    main()

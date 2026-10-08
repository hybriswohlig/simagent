#!/usr/bin/env python3
"""Independent reference for meiosis and mutation, from the specification alone.

Sources: docs/genetik-spezifikation.md section 2 (2.1 layout, 2.2 map, 2.3
meiosis, 2.4 mutation, 2.5 Wirkungsweisen) and the addressed draws of
docs/adr/0001-zufall-und-gabeln.md sections 2 and 3. The Rust core was not read
(docs/vorbereitung-phase-0.md B13). Randomness goes only through
tools/rng_reference.py. Section 2.5 names how a locus acts on a trait. Mutation
does not read that, and this reference does not apply it.

Layout (2.1, 2.2)
- Every occupied slot is a locus with alleles 0 and 1. Meiosis transmits every
  slot and does not consult the genome map. Mutation (2.4) consults the map.
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
  CROSSOVER_POSITION = 7, MUTATION_COUNT = 8, MUTATION_SITE = 9. The
  specification names the crossover and mutation tags and calls them new.
  6 and 7, then 8 and 9, are the next integers after PAIRING_RANK = 5 in
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

Mutation (2.4), one side, after that side's recombination (2.3)
- A map locus has "id", "chromosome" (autosome, x, y, mt), "segment", "slot",
  "mark" (small, large, disease) and "direction" (both, to_alt, to_ref).
  The rate is the disease rate when the mark is disease, otherwise the standard
  rate. The specification states those two rates (standard 5e-5, disease 1e-6)
  and a mark on the locus. This reference takes the two numbers as "rates"
  and does not read a numeric rate off the locus. Callers pass them in.
- The mother haplotype is her gamete autosomes, her gamete X and the mt (the
  child's mt is hers). The father haplotype is his gamete autosomes and the X
  or the Y he passes. Y is not on the mother haplotype. mt is not on the father
  haplotype. The sex chromosome he did not pass is not on his haplotype.
- E is the map loci on that haplotype whose transmitted allele can still change
  in their direction: both always, to_alt only when the allele is 0, to_ref
  only when the allele is 1. Order is map order: autosomes by segment then
  slot, then X by slot, then Y by slot, then mt by slot. X is segment 8 (2.2).
  Y and mt have no segment index in the specification (they never recombine,
  2.2). This file stores segment 0 for them and does not use it to address the
  allele. The same chromosome, segment and slot may occur only once. Free
  slots are absent from the map, so they are absent from E and do not mutate
  (2.1).
- Lambda is the sum of the rates in E, added left to right as Python floats.
- m is the sum of the draws poisson(min(30, Lambda - 30*c)) with
  MUTATION_COUNT, detail c, for c = 0, 1, ... while Lambda - 30*c > 0, and
  always at least the draw for c = 0. Lambda = 0 still draws poisson(0) once,
  at detail 0. 30 is the largest mean poisson accepts (ADR-0001 section 3).
  Subject is the child's genome_key. Occasion is the side, 0 mother, 1 father.
  For Lambda <= 30 this is one draw poisson(Lambda) at detail 0.
- For j = 0 .. m-1, while E is not empty: u = unit(word) with MUTATION_SITE,
  same subject and occasion, detail j. total is the rates of the loci still
  in E, added left to right. threshold = u * total. Walk that list in order,
  adding rates left to right, and take the first locus whose running sum is
  strictly greater than threshold. If none is (rounding: the running sum never
  passes the threshold), take the last locus of the remaining list. Flip its
  allele (0 becomes 1, 1 becomes 0) and remove it from E. When m is longer
  than E, stop once E is empty, and do not draw a site for the remaining j.
- The side records the mutations in the order they were chosen:
  {"locus": id, "old": a, "new": b}.

mutate(seed, genome_key, side, gamete, genome_map, rates) returns
(gamete, mutations). The returned gamete is a copy with those alleles flipped.
child_with_mutation(seed, genome_key, mother, father, map_lengths, genome_map,
rates) runs both meioses, then mutates each side. The child genome is built
from the mutated gametes, the same way child() builds it. Each record keeps
the meiosis segments (and the father's sex_chromosome) and adds "mutations".

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

Test map (--mutation-vectors), a rule of this file, not of the specification:
24 loci, ids "test:m00" .. "test:m23", already in map order. 17 autosomal loci
cover every segment, plus 3 X loci (slots 0, 7, 15), 2 Y loci (slots 1, 6) and
2 mt loci (slots 0, 4). Locus i has mark (small, large, disease)[i % 3] and
direction (both, to_ref, to_alt)[i % 3], so every disease locus is to_alt
(section 3.3). Rates are standard 0.05 and disease 0.02, high so that
mutations occur in the six cases. Those cases use the same seeds, genome keys
and parental alleles as --vectors. The child is the genome after mutation of
both sides. The printed object is {"genome_map", "rates", "cases", "parts"}.
The first three keys are the six cases above.

Parts (--mutation-vectors, key "parts"), a rule of this file, not of the
specification: {"genome_map", "rates", "cases"}. Forty autosomal loci.
Construction index i lies on segment i % 8 and slot i // 8. The list is sorted
by segment, then slot, and the ids "test:p00" .. "test:p39" are numbered in
that order. Every mark is "small" and every direction is "both". Rates are
standard 0.9 and disease 0.02. Every locus is eligible on every gamete, so
each side's Lambda is the left-to-right sum of forty standard rates and the
count takes two parts, min(30, Lambda) and Lambda - 30. The cases are the
first three VECTOR_CASES pairs, with the same parents as --vectors and the
same record format as "cases".

Self-check (--self-check-mutation): 20000 maternal meioses, seed 1, genome_key
0 .. 19999, the test rates, and the test map with every direction set to both.
The contrast mother is the parent, so every transmitted allele is 0 or 1 and,
with direction both, every locus on her haplotype is eligible. Her haplotype
does not depend on the sex-chromosome draw, so Lambda is the same each time:
the left-to-right rate sum of the autosomal, X and mt loci. Y is not on that
haplotype (2.3). The mean number of recorded mutations must lie within 0.02
of that Lambda.

Self-check (--self-check-parts): 4000 maternal meioses, seed 1, genome_key
0 .. 3999, the parts map and the contrast mother. The mean of m (the Poisson
count, before the list E runs out) must lie within 4 standard errors of 36.
The standard error is sqrt(36 / 4000).

Usage:
  python3 tools/meiosis_reference.py --vectors
  python3 tools/meiosis_reference.py --self-check
  python3 tools/meiosis_reference.py --mutation-vectors
  python3 tools/meiosis_reference.py --self-check-mutation
  python3 tools/meiosis_reference.py --self-check-parts
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
MUTATION_COUNT = 8
MUTATION_SITE = 9

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

# Test map positions, already in map order (2.4). Autosomes: every segment.
TEST_AUTOSOMAL_POSITIONS = [
    (0, 0),
    (0, 5),
    (0, 12),
    (1, 1),
    (1, 8),
    (2, 3),
    (2, 15),
    (3, 2),
    (3, 20),
    (4, 0),
    (4, 11),
    (5, 6),
    (5, 18),
    (6, 4),
    (6, 22),
    (7, 7),
    (7, 14),
]
TEST_X_SLOTS = (0, 7, 15)
TEST_Y_SLOTS = (1, 6)
TEST_MT_SLOTS = (0, 4)
TEST_MARKS = ("small", "large", "disease")
TEST_DIRECTIONS = ("both", "to_ref", "to_alt")
TEST_RATES = {"standard": 0.05, "disease": 0.02}
MUTATION_SELF_CHECK_N = 20000
# Loci on the mother haplotype of the test map: 24 minus the 2 Y loci.
MOTHER_HAPLOTYPE_LOCI = 22
MOTHER_DISEASE_LOCI = 7

# Parts map: 40 autosomal loci, five slots on each of the eight segments.
PARTS_LOCI = 40
PARTS_RATES = {"standard": 0.9, "disease": 0.02}
PARTS_SELF_CHECK_N = 4000
PARTS_REFERENCE_MEAN = 36.0
PARTS_STANDARD_ERRORS = 4.0


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


def build_test_genome_map():
    """24 loci, ids test:m00 .. test:m23, in map order. See the module docstring."""
    positions = [("autosome", segment, slot) for segment, slot in TEST_AUTOSOMAL_POSITIONS]
    positions.extend(("x", X_SEGMENT, slot) for slot in TEST_X_SLOTS)
    positions.extend(("y", 0, slot) for slot in TEST_Y_SLOTS)
    positions.extend(("mt", 0, slot) for slot in TEST_MT_SLOTS)
    if len(positions) != 24:
        raise RuntimeError(f"the test map has {len(positions)} loci, not 24")
    genome_map = []
    for index, (chromosome, segment, slot) in enumerate(positions):
        genome_map.append(
            {
                "id": f"test:m{index:02d}",
                "chromosome": chromosome,
                "segment": segment,
                "slot": slot,
                "mark": TEST_MARKS[index % 3],
                "direction": TEST_DIRECTIONS[index % 3],
            }
        )
    if [locus["id"] for locus in genome_map] != [f"test:m{i:02d}" for i in range(24)]:
        raise RuntimeError("test locus ids are not test:m00 .. test:m23")
    keys = [map_order_key(locus) for locus in genome_map]
    if keys != sorted(keys):
        raise RuntimeError("the test map is not in map order")
    disease_to_alt = [
        locus
        for locus in genome_map
        if locus["mark"] == "disease" and locus["direction"] == "to_alt"
    ]
    if not disease_to_alt:
        raise RuntimeError("the test map has no disease locus with direction to_alt")
    return genome_map


def build_parts_genome_map():
    """40 autosomal loci, ids test:p00 .. test:p39, in map order.

    Construction index i is segment i % 8 and slot i // 8. The loci are then
    sorted by segment and slot, and the ids are numbered in that order. Every
    mark is small and every direction is both.
    """
    positions = sorted(
        (index % AUTOSOME_SEGMENTS, index // AUTOSOME_SEGMENTS) for index in range(PARTS_LOCI)
    )
    if len(set(positions)) != PARTS_LOCI:
        raise RuntimeError("parts map positions are not unique")
    genome_map = []
    for number, (segment, slot) in enumerate(positions):
        genome_map.append(
            {
                "id": f"test:p{number:02d}",
                "chromosome": "autosome",
                "segment": segment,
                "slot": slot,
                "mark": "small",
                "direction": "both",
            }
        )
    if [locus["id"] for locus in genome_map] != [f"test:p{i:02d}" for i in range(PARTS_LOCI)]:
        raise RuntimeError("parts locus ids are not test:p00 .. test:p39")
    ordered = loci_in_map_order(genome_map)
    if [(locus["segment"], locus["slot"]) for locus in ordered] != positions:
        raise RuntimeError("the parts map is not in map order")
    if [locus["id"] for locus in ordered] != [locus["id"] for locus in genome_map]:
        raise RuntimeError("parts ids are not numbered in map order")
    return genome_map


def map_order_key(locus):
    """Autosomes by segment then slot, then X, Y, mt by slot (spec 2.4)."""
    chromosome = locus["chromosome"]
    if chromosome == "autosome":
        return (0, locus["segment"], locus["slot"], locus["id"])
    if chromosome == "x":
        return (1, 0, locus["slot"], locus["id"])
    if chromosome == "y":
        return (2, 0, locus["slot"], locus["id"])
    if chromosome == "mt":
        return (3, 0, locus["slot"], locus["id"])
    raise ValueError(f"unknown chromosome {chromosome}")


def checked_locus(locus):
    if not isinstance(locus, dict):
        raise ValueError("a genome map locus must be an object")
    chromosome = locus.get("chromosome")
    if chromosome not in ("autosome", "x", "y", "mt"):
        raise ValueError(f"unknown chromosome {chromosome}")
    segment = locus.get("segment")
    slot = locus.get("slot")
    if type(segment) is not int or type(slot) is not int:
        raise ValueError(f"{locus.get('id')} segment and slot must be integers")
    if chromosome == "autosome":
        if not 0 <= segment < AUTOSOME_SEGMENTS or not 0 <= slot < AUTOSOME_SLOTS:
            raise ValueError(f"{locus.get('id')} is outside the autosomes")
    elif chromosome == "x":
        if segment != X_SEGMENT or not 0 <= slot < X_SLOTS:
            raise ValueError(f"{locus.get('id')} is outside the X segment")
    elif chromosome == "y":
        if segment != 0 or not 0 <= slot < Y_SLOTS:
            raise ValueError(f"{locus.get('id')} is outside the Y")
    else:
        if segment != 0 or not 0 <= slot < MT_SLOTS:
            raise ValueError(f"{locus.get('id')} is outside mt")
    if locus.get("mark") not in ("small", "large", "disease"):
        raise ValueError(f"{locus.get('id')} has an unknown mark")
    if locus.get("direction") not in ("both", "to_alt", "to_ref"):
        raise ValueError(f"{locus.get('id')} has an unknown direction")
    locus_id = locus.get("id")
    if not isinstance(locus_id, str) or locus_id == "":
        raise ValueError("a locus id must be a non-empty string")
    return locus


def loci_in_map_order(genome_map):
    if not isinstance(genome_map, list):
        raise ValueError("genome_map must be a list of loci")
    checked = [checked_locus(locus) for locus in genome_map]
    ordered = sorted(checked, key=map_order_key)
    seen = set()
    for locus in ordered:
        if locus["chromosome"] == "autosome":
            key = ("autosome", locus["segment"], locus["slot"])
        else:
            key = (locus["chromosome"], locus["slot"])
        if key in seen:
            raise ValueError(f"two loci occupy {key}")
        seen.add(key)
    return ordered


def locus_rate(locus, rates):
    """Disease mark uses the disease rate. Every other mark uses the standard rate."""
    name = "disease" if locus["mark"] == "disease" else "standard"
    if name not in rates:
        raise ValueError(f"rates is missing {name}")
    value = float(rates[name])
    if not 0.0 <= value <= 30.0:
        raise ValueError(f"{name} rate {value} is outside the poisson domain [0, 30]")
    return value


def rate_sum(loci, rates):
    """Left-to-right sum of Python floats (spec 2.4, Lambda and the site total)."""
    total = 0.0
    for locus in loci:
        total += locus_rate(locus, rates)
    return total


def on_haplotype(locus, gamete):
    chromosome = locus["chromosome"]
    if chromosome == "autosome":
        return True
    if chromosome == "x":
        return "x" in gamete["sex"]
    if chromosome == "y":
        return "y" in gamete["sex"]
    return gamete["mt"] is not None


def read_allele(gamete, locus):
    slot = locus["slot"]
    chromosome = locus["chromosome"]
    if chromosome == "autosome":
        return gamete["autosomes"][locus["segment"]][slot]
    if chromosome == "x":
        return gamete["sex"]["x"][slot]
    if chromosome == "y":
        return gamete["sex"]["y"][slot]
    return gamete["mt"][slot]


def write_allele(gamete, locus, allele):
    slot = locus["slot"]
    chromosome = locus["chromosome"]
    if chromosome == "autosome":
        gamete["autosomes"][locus["segment"]][slot] = allele
    elif chromosome == "x":
        gamete["sex"]["x"][slot] = allele
    elif chromosome == "y":
        gamete["sex"]["y"][slot] = allele
    else:
        gamete["mt"][slot] = allele


def can_change(direction, allele):
    if direction == "both":
        return True
    if direction == "to_alt":
        return allele == 0
    if direction == "to_ref":
        return allele == 1
    raise ValueError(f"unknown direction {direction}")


def copy_gamete(gamete):
    sex = gamete["sex"]
    if not isinstance(sex, dict):
        raise ValueError("gamete sex must be an object")
    if "x" in sex and "y" in sex:
        raise ValueError("a gamete carries one sex chromosome")
    if "x" in sex:
        sex_copy = {"x": list(alleles(sex["x"], X_SLOTS, "gamete sex.x"))}
    elif "y" in sex:
        sex_copy = {"y": list(alleles(sex["y"], Y_SLOTS, "gamete sex.y"))}
    else:
        raise ValueError("a gamete is missing its sex chromosome")
    rows = gamete["autosomes"]
    if not isinstance(rows, list) or len(rows) != AUTOSOME_SEGMENTS:
        raise ValueError(f"gamete autosomes must be {AUTOSOME_SEGMENTS} segments")
    autosomes = []
    for segment, row in enumerate(rows):
        autosomes.append(list(alleles(row, AUTOSOME_SLOTS, f"gamete autosomes[{segment}]")))
    mt = gamete["mt"]
    if mt is None:
        mt_copy = None
    else:
        mt_copy = list(alleles(mt, MT_SLOTS, "gamete mt"))
    return {"autosomes": autosomes, "sex": sex_copy, "mt": mt_copy}


def eligible_loci(genome_map, gamete):
    """E: haplotype loci that can still change, in map order (spec 2.4)."""
    eligible = []
    for locus in loci_in_map_order(genome_map):
        if not on_haplotype(locus, gamete):
            continue
        if can_change(locus["direction"], read_allele(gamete, locus)):
            eligible.append(locus)
    return eligible


def choose_index(remaining, rates, draw):
    """First locus whose running rate sum is strictly above draw * total.

    If rounding leaves every running sum at or below the threshold, spec 2.4
    takes the last locus of the remaining list.
    """
    if not remaining:
        raise ValueError("mutation site draw with an empty list")
    total = rate_sum(remaining, rates)
    threshold = draw * total
    running = 0.0
    for index, locus in enumerate(remaining):
        running += locus_rate(locus, rates)
        if running > threshold:
            return index
    return len(remaining) - 1


def mutation_parts(lam):
    """Poisson means min(30, Lambda - 30*c) for spec 2.4, at least c = 0.

    The parts are Python floats. Callers pass the left-to-right rate sum, so a
    Lambda of 40 * 0.9 is the repeated sum of the float 0.9, not the real
    product 36, and the second part is that sum minus 30.
    """
    if type(lam) is not float or not math.isfinite(lam) or lam < 0.0:
        raise ValueError(f"Lambda {lam!r} is not a finite sum of rates at least 0")
    parts = []
    detail = 0
    while detail == 0 or lam - 30.0 * detail > 0.0:
        parts.append(min(30.0, lam - 30.0 * detail))
        detail += 1
    return parts


def mutation_draw_count(seed, genome_key, side, lam):
    """m, the sum of the MUTATION_COUNT draws in spec 2.4."""
    count = 0
    for detail, part in enumerate(mutation_parts(lam)):
        word = first_word(seed, MUTATION_COUNT, genome_key, side, detail)
        count += poisson_count(word, part)
    return count


def mutate(seed, genome_key, side, gamete, genome_map, rates):
    """Mutation of one side after recombination (spec 2.4). Returns (gamete, mutations)."""
    seed = u64("seed", seed)
    genome_key = u64("genome_key", genome_key)
    if side not in (SIDE_MOTHER, SIDE_FATHER):
        raise ValueError("side must be 0 (mother) or 1 (father)")
    gamete = copy_gamete(gamete)
    eligible = eligible_loci(genome_map, gamete)
    lam = rate_sum(eligible, rates)
    count = mutation_draw_count(seed, genome_key, side, lam)
    mutations = []
    for detail in range(count):
        if not eligible:
            break
        site_word = first_word(seed, MUTATION_SITE, genome_key, side, detail)
        index = choose_index(eligible, rates, unit(site_word))
        locus = eligible.pop(index)
        old = read_allele(gamete, locus)
        new = 1 - old
        write_allele(gamete, locus, new)
        mutations.append({"locus": locus["id"], "old": old, "new": new})
    return gamete, mutations


def child_with_mutation(seed, genome_key, mother, father, map_lengths, genome_map, rates):
    """Both meioses, then mutation of each side. Returns (child genome, both records)."""
    mother_gamete, mother_record = meiosis(seed, genome_key, SIDE_MOTHER, mother, map_lengths)
    father_gamete, father_record = meiosis(seed, genome_key, SIDE_FATHER, father, map_lengths)
    mother_gamete, mother_mutations = mutate(
        seed, genome_key, SIDE_MOTHER, mother_gamete, genome_map, rates
    )
    father_gamete, father_mutations = mutate(
        seed, genome_key, SIDE_FATHER, father_gamete, genome_map, rates
    )
    if "x" not in mother_gamete["sex"] or mother_gamete["mt"] is None:
        raise RuntimeError("the mother's gamete is missing X or mt")
    genome = {
        "autosomes_maternal": mother_gamete["autosomes"],
        "autosomes_paternal": father_gamete["autosomes"],
        "x_maternal": mother_gamete["sex"]["x"],
        "paternal_sex": father_gamete["sex"],
        "mt": mother_gamete["mt"],
    }
    records = {
        "mother": {"segments": mother_record["segments"], "mutations": mother_mutations},
        "father": {
            "segments": father_record["segments"],
            "sex_chromosome": father_record["sex_chromosome"],
            "mutations": father_mutations,
        },
    }
    return genome, records


TEST_GENOME_MAP = build_test_genome_map()
PARTS_GENOME_MAP = build_parts_genome_map()


def map_directions_both(genome_map):
    """Same loci and marks, every direction set to both."""
    return [
        {
            "id": locus["id"],
            "chromosome": locus["chromosome"],
            "segment": locus["segment"],
            "slot": locus["slot"],
            "mark": locus["mark"],
            "direction": "both",
        }
        for locus in genome_map
    ]


def mutation_vector_cases(map_lengths=None, genome_map=None, rates=None):
    if map_lengths is None:
        map_lengths = load_map_lengths()
    if genome_map is None:
        genome_map = TEST_GENOME_MAP
    if rates is None:
        rates = TEST_RATES
    cases = []
    for case_index, (seed, genome_key) in enumerate(VECTOR_CASES):
        mother = vector_mother(case_index)
        father = vector_father(case_index)
        genome, records = child_with_mutation(
            seed, genome_key, mother, father, map_lengths, genome_map, rates
        )
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
    parts_cases = []
    for case_index, (seed, genome_key) in enumerate(VECTOR_CASES[:3]):
        mother = vector_mother(case_index)
        father = vector_father(case_index)
        genome, records = child_with_mutation(
            seed, genome_key, mother, father, map_lengths, PARTS_GENOME_MAP, PARTS_RATES
        )
        parts_cases.append(
            {
                "seed": seed,
                "genome_key": f"{genome_key:016x}",
                "mother": mother,
                "father": father,
                "child": genome,
                "records": records,
            }
        )
    return {
        "genome_map": genome_map,
        "rates": rates,
        "cases": cases,
        "parts": {
            "genome_map": PARTS_GENOME_MAP,
            "rates": PARTS_RATES,
            "cases": parts_cases,
        },
    }


def self_check_mutation(map_lengths=None):
    """Mean mutation count on the mother side, against Lambda (spec 2.4)."""
    if map_lengths is None:
        map_lengths = load_map_lengths()
    genome_map = map_directions_both(TEST_GENOME_MAP)
    rates = TEST_RATES
    parent = contrast_mother()
    on_mother = [locus for locus in genome_map if locus["chromosome"] != "y"]
    if len(on_mother) != MOTHER_HAPLOTYPE_LOCI:
        raise SystemExit(
            f"self-check mutation failed: expected {MOTHER_HAPLOTYPE_LOCI} loci "
            f"on the mother haplotype, found {len(on_mother)}"
        )
    disease = sum(1 for locus in on_mother if locus["mark"] == "disease")
    if disease != MOTHER_DISEASE_LOCI:
        raise SystemExit(
            f"self-check mutation failed: expected {MOTHER_DISEASE_LOCI} disease "
            f"loci on the mother haplotype, found {disease}"
        )
    total_mutations = 0
    expected = None
    for genome_key in range(MUTATION_SELF_CHECK_N):
        gamete, _record = meiosis(
            SELF_CHECK_SEED, genome_key, SIDE_MOTHER, parent, map_lengths
        )
        eligible = eligible_loci(genome_map, gamete)
        if len(eligible) != MOTHER_HAPLOTYPE_LOCI:
            raise SystemExit(
                f"self-check mutation failed: genome_key {genome_key} left "
                f"{len(eligible)} loci eligible, not {MOTHER_HAPLOTYPE_LOCI}"
            )
        lam = rate_sum(eligible, rates)
        if expected is None:
            expected = lam
        elif lam != expected:
            raise SystemExit(
                f"self-check mutation failed: Lambda changed from {expected!r} to {lam!r}"
            )
        _gamete, mutations = mutate(
            SELF_CHECK_SEED, genome_key, SIDE_MOTHER, gamete, genome_map, rates
        )
        total_mutations += len(mutations)
    mean = total_mutations / MUTATION_SELF_CHECK_N
    difference = abs(mean - expected)
    print(f"n {MUTATION_SELF_CHECK_N}")
    print(f"seed {SELF_CHECK_SEED}")
    print(f"side {SIDE_MOTHER}")
    print(f"eligible {MOTHER_HAPLOTYPE_LOCI}")
    print(f"lambda {expected!r}")
    print(f"mean {mean!r}")
    print(f"abs_diff {difference!r}")
    if difference > 0.02:
        raise SystemExit(
            f"self-check mutation failed: mean {mean} differs from Lambda {expected} "
            f"by {difference}, more than 0.02"
        )
    print("within 0.02 of Lambda")


def self_check_parts(map_lengths=None):
    """Mean of m on the parts map, within 4 Poisson standard errors of 36."""
    if map_lengths is None:
        map_lengths = load_map_lengths()
    genome_map = PARTS_GENOME_MAP
    rates = PARTS_RATES
    parent = contrast_mother()
    total = 0
    lam = None
    parts = None
    for genome_key in range(PARTS_SELF_CHECK_N):
        gamete, _record = meiosis(
            SELF_CHECK_SEED, genome_key, SIDE_MOTHER, parent, map_lengths
        )
        eligible = eligible_loci(genome_map, gamete)
        if len(eligible) != PARTS_LOCI:
            raise SystemExit(
                f"self-check parts failed: genome_key {genome_key} left "
                f"{len(eligible)} loci eligible, not {PARTS_LOCI}"
            )
        observed_lambda = rate_sum(eligible, rates)
        observed_parts = mutation_parts(observed_lambda)
        if lam is None:
            lam = observed_lambda
            parts = observed_parts
        elif observed_lambda != lam or observed_parts != parts:
            raise SystemExit(
                f"self-check parts failed: Lambda changed from {lam!r} to {observed_lambda!r}"
            )
        count = mutation_draw_count(SELF_CHECK_SEED, genome_key, SIDE_MOTHER, lam)
        _mutated, mutations = mutate(
            SELF_CHECK_SEED, genome_key, SIDE_MOTHER, gamete, genome_map, rates
        )
        placed = min(count, len(eligible))
        if len(mutations) != placed:
            raise SystemExit(
                f"self-check parts failed: genome_key {genome_key} recorded "
                f"{len(mutations)} mutations, not min(m, |E|) = {placed}"
            )
        total += count
    if lam is None or parts is None:
        raise SystemExit("self-check parts failed: no meioses")
    if len(parts) != 2 or parts[0] != 30.0:
        raise SystemExit(
            f"self-check parts failed: expected two parts starting at 30, found {parts!r}"
        )
    mean = total / PARTS_SELF_CHECK_N
    standard_error = math.sqrt(PARTS_REFERENCE_MEAN / PARTS_SELF_CHECK_N)
    limit = PARTS_STANDARD_ERRORS * standard_error
    difference = abs(mean - PARTS_REFERENCE_MEAN)
    print(f"n {PARTS_SELF_CHECK_N}")
    print(f"seed {SELF_CHECK_SEED}")
    print(f"side {SIDE_MOTHER}")
    print(f"eligible {PARTS_LOCI}")
    print(f"lambda {lam!r}")
    print(f"parts {parts!r}")
    print(f"reference {PARTS_REFERENCE_MEAN!r}")
    print(f"mean {mean!r}")
    print(f"standard_error {standard_error!r}")
    print(f"abs_diff {difference!r}")
    print(f"limit {limit!r}")
    if difference > limit:
        raise SystemExit(
            f"self-check parts failed: mean {mean} differs from {PARTS_REFERENCE_MEAN} "
            f"by {difference}, more than {PARTS_STANDARD_ERRORS:g} standard errors ({limit})"
        )
    print(f"within {PARTS_STANDARD_ERRORS:g} standard errors of 36")


def main():
    parser = argparse.ArgumentParser(
        description="Independent meiosis and mutation reference (spec 2.1 to 2.4)."
    )
    parser.add_argument("--vectors", action="store_true", help="print six JSON cases")
    parser.add_argument("--self-check", action="store_true", help="Haldane check on 20000 meioses")
    parser.add_argument(
        "--mutation-vectors",
        action="store_true",
        help="print the test map, rates, six cases, and the parts vector set",
    )
    parser.add_argument(
        "--self-check-mutation",
        action="store_true",
        help="mean mutation count against Lambda on 20000 meioses",
    )
    parser.add_argument(
        "--self-check-parts",
        action="store_true",
        help="mean Poisson count within 4 standard errors of 36 on 4000 meioses",
    )
    args = parser.parse_args()
    selected = (
        args.vectors,
        args.self_check,
        args.mutation_vectors,
        args.self_check_mutation,
        args.self_check_parts,
    )
    if sum(selected) != 1:
        parser.error(
            "pass exactly one of --vectors, --self-check, "
            "--mutation-vectors, --self-check-mutation or --self-check-parts"
        )
    if args.vectors:
        json.dump({"cases": vector_cases()}, sys.stdout, indent=1)
        sys.stdout.write("\n")
    elif args.self_check:
        self_check()
    elif args.mutation_vectors:
        json.dump(mutation_vector_cases(), sys.stdout, indent=1)
        sys.stdout.write("\n")
    elif args.self_check_mutation:
        self_check_mutation()
    else:
        self_check_parts()


if __name__ == "__main__":
    main()

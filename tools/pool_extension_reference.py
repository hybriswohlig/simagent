#!/usr/bin/env python3
"""Load-time extension of the founder pool (ADR-0033 section 2, decisions 5 and 6; ADR-0034
section 2).

The frozen pool file is not rewritten. The extensions are an ordered table
(`data_reference.EXTENSIONS`): temperament (map 5, the five traits' 40 additive autosomal
loci) and hair (map 6, the two loci of `core:hair_thickness`, `core:hair_thickness_01` at
segment 0 slot 17 and `_02` at segment 1 slot 19, after the 40). A map declares a prefix of
the table; every pool genome gains the loci of its declared extensions. A genome with no
parent among the living pool members draws both copies independently (`POOL_ALLELE`, pool
seed, subject the genome key, occasion `str_key` of the locus, detail the copy 0 or 1,
`unit(key) < 0.5`). Otherwise each living parent transmits one copy by the same Mendelian
draw as `assign_locus` (addressed by the child's genome key, the locus and the copy); a
missing parent's copy is drawn independently at p = 0.5. Order: living members by
`(born, lineage_key)`, then unborn children by the mother's pending birth (`birth_at`)
and the child's lineage key. Identical twins share a genome key, so they draw the same
alleles. Applying the extension twice changes nothing. Slots other than the new ones stay as
they are. Every draw is per locus, so the 40 temperament alleles are the same bits whether
the hair loci are written or not (asserted on the real pool).

The reference values of each temperament trait (`d_ref` 1, `sigma_ref` 2, `mean_ref` 0) and
of `core:hair_thickness` (`d_ref` 1, `sigma_ref` 1, `mean_ref` 0) are the exact parameters of
the Hardy-Weinberg distribution at p = 0.5 (`Var(A) = n · 2p(1-p)` for n loci), an exception
to measuring them. This script does not measure.

Usage:
  python tools/pool_extension_reference.py fixture
  python tools/pool_extension_reference.py real [POOL]
"""

import sys
from pathlib import Path

import data_reference
import pool_reference
import world_reference
from rng_reference import chacha_key, founder_key, str_key, unit, words

ROOT = Path(__file__).resolve().parent.parent
POOL_ALLELE = pool_reference.POOL_ALLELE
MASK64 = (1 << 64) - 1
# Precommitted before the real-pool frequency is known (ADR-0033 section 2). Relatedness
# means the draws are not an independent binomial; this bound still rejects a systematic bias.
FREQUENCY_BOUND = 0.01
# The prior count in ADR-0033 section 2: genomes with at least one living pool parent.
MENDELIAN_GENOMES = 29_942
POOL_GENOMES = 40_418

# The extensions' loci as (id, chromosome, segment, slot), from the table of
# data_reference: the 40 temperament loci (map 5), then the two hair loci (map 6). Their
# placement is asserted against the rule of ADR-0020 section 1 there (`check_placements`).
TEMPERAMENT_LOCI = data_reference.EXTENSIONS[0].loci
HAIR_LOCI = data_reference.EXTENSIONS[1].loci
LOCI = TEMPERAMENT_LOCI
MAP6_LOCI = TEMPERAMENT_LOCI + HAIR_LOCI
assert [locus[0] for locus in HAIR_LOCI] == ["core:hair_thickness_01", "core:hair_thickness_02"]
assert [locus[2:] for locus in HAIR_LOCI] == [(0, 17), (1, 19)]
OCCASION = {locus_id: str_key(locus_id) for locus_id, _, _, _ in MAP6_LOCI}
# The checksum of the 40 temperament alleles of the real pool (the regression value of the
# map-5 extension, ADR-0034 section 2): unchanged when the hair loci are added.
TEMPERAMENT_CHECKSUM = 0x7151A1C895A73F91
# Extended file bytes keyed by (xxh3 of the raw file, the loci with their places), so one
# process extends a pool once per set of loci.
_EXTENDED_BYTES = {}


def declares_temperament(locus_ids) -> bool:
    """True when every temperament locus id is present. `locus_ids` is a set of ids or a
    dict keyed by id."""
    return all(locus_id in locus_ids for locus_id, _, _, _ in TEMPERAMENT_LOCI)


def extended_bytes(raw: bytes, loci=LOCI) -> bytes:
    """The pool file with the loci `loci` ((id, chromosome, segment, slot), the declared
    extensions' loci in table order) written, cached by the raw file's hash and the loci.

    The file on disk is not rewritten. A second call with the same bytes and loci returns the
    cached encoding. The extension is idempotent, so extending an already extended encoding
    yields the same bytes.
    """
    loci = tuple(tuple(locus) for locus in loci)
    key = (world_reference.hash64(raw), loci)
    found = _EXTENDED_BYTES.get(key)
    if found is not None:
        return found
    pool = pool_reference.read_pool(raw)
    extend_pool(pool, loci)
    encoded = pool_reference.encode_pool(pool)
    _EXTENDED_BYTES[key] = encoded
    return encoded


def allele_at(words, segment, slot) -> int:
    bit = segment * 24 + slot
    return (words[bit // 64] >> (bit % 64)) & 1


def with_allele(words, segment, slot, allele) -> tuple:
    bit = segment * 24 + slot
    word, shift = bit // 64, bit % 64
    updated = list(words)
    if allele:
        updated[word] = (updated[word] | (1 << shift)) & MASK64
    else:
        updated[word] = (updated[word] & ~(1 << shift)) & MASK64
    return tuple(updated)


def copies_at(genome, segment, slot) -> tuple:
    maternal, paternal, *_ = genome
    return allele_at(maternal, segment, slot), allele_at(paternal, segment, slot)


def write_copies(genome, segment, slot, pair) -> tuple:
    maternal, paternal, x_maternal, paternal_sex, mt = genome
    return (
        with_allele(maternal, segment, slot, pair[0]),
        with_allele(paternal, segment, slot, pair[1]),
        x_maternal,
        paternal_sex,
        mt,
    )


def draw_unit(pool_seed: int, genome_key: int, locus_id: str, detail: int) -> float:
    """`unit` of POOL_ALLELE (pool seed, genome key, str_key of the locus, copy)."""
    key = chacha_key(pool_seed, POOL_ALLELE, genome_key, OCCASION[locus_id], detail)
    return unit(words(key)[0])


def blank_genome(sex: int) -> tuple:
    """All reference alleles. Sex 0 is female (paternal X), 1 is male (paternal Y)."""
    if sex == 0:
        return ((0, 0, 0), (0, 0, 0), 0, (0, 0), 0)
    if sex == 1:
        return ((0, 0, 0), (0, 0, 0), 0, (1, 0), 0)
    raise ValueError(sex)


def iter_genomes(pool: dict):
    """Living members by (born, lineage key), then unborn by (mother's birth_at, key).

    Yields (person dict, mother key or None, father key or None). The person dict is the
    one stored in the pool, so writing `person["genome"]` updates the pool.
    """
    living = []
    for household in pool["households"]:
        living.extend(household["members"])
    living.sort(key=lambda person: (person["born"], person["lineage_key"]))
    for person in living:
        yield person, person["mother"], person["father"]
    mothers = {person["lineage_key"]: person for person in living}
    unborn = []
    for household in pool["households"]:
        for pregnancy in household["pregnancies"]:
            due = mothers[pregnancy["mother"]]["birth_at"]
            if due is None:
                raise ValueError("a pregnancy whose mother has no pending birth")
            for child in pregnancy["children"]:
                unborn.append((due, child, pregnancy["mother"], pregnancy["father"]))
    unborn.sort(key=lambda item: (item[0], item[1]["lineage_key"]))
    for _, child, mother, father in unborn:
        yield child, mother, father


def extend_pool(pool: dict, loci=LOCI) -> dict:
    """Writes `loci` ((id, chromosome, segment, slot), autosomal) into every genome. Returns
    counts for the report: `allele_checksum` over every new copy (per genome, per locus in
    the order of `loci`, per copy), and `checksums` per extension of the table over its loci
    alone in the same order (`allele_bytes` holds those bytes).

    A parent transmits only when its lineage key is a living member already visited.
    That is `assign_locus`: the assigned map holds the living people in birth order,
    and a parent who is not among them (dead, emigrated, or not yet visited) is missing.
    """
    assert all(chromosome == "autosome" for _, chromosome, _, _ in loci)
    groups = {
        extension.name: {locus[0] for locus in extension.loci}
        for extension in data_reference.EXTENSIONS
        if any(locus[0] in {own[0] for own in extension.loci} for locus in loci)
    }
    living = {person["lineage_key"] for household in pool["households"] for person in household["members"]}
    assigned = {}
    by_genome_key = {}
    mendelian = 0
    alt = 0
    copies = 0
    allele_bytes = bytearray()
    group_bytes = {name: bytearray() for name in groups}
    for person, mother_key, father_key in iter_genomes(pool):
        mother_genome = assigned.get(mother_key) if mother_key in living else None
        father_genome = assigned.get(father_key) if father_key in living else None
        if mother_genome is not None or father_genome is not None:
            mendelian += 1
        genome = person["genome"]
        # A genome key written before (an identical twin, whatever the order of birth
        # seconds and keys) gives its new alleles unchanged.
        first = by_genome_key.get(person["genome_key"])
        for locus_id, _, segment, slot in loci:
            drawn = []
            for detail, parent in ((0, mother_genome), (1, father_genome)):
                u = draw_unit(pool["pool_seed"], person["genome_key"], locus_id, detail)
                if first is not None:
                    allele = copies_at(first, segment, slot)[detail]
                elif parent is None:
                    allele = 1 if u < 0.5 else 0
                else:
                    parent_copies = copies_at(parent, segment, slot)
                    allele = parent_copies[0] if u < 0.5 else parent_copies[1]
                drawn.append(allele)
                allele_bytes.append(allele)
                for name, ids in groups.items():
                    if locus_id in ids:
                        group_bytes[name].append(allele)
                alt += allele
                copies += 1
            genome = write_copies(genome, segment, slot, drawn)
        person["genome"] = genome
        by_genome_key.setdefault(person["genome_key"], genome)
        assigned[person["lineage_key"]] = genome
    return {
        "genomes": copies // (2 * len(loci)),
        "mendelian": mendelian,
        "alt": alt,
        "copies": copies,
        "frequency": alt / copies,
        "allele_checksum": world_reference.hash64(bytes(allele_bytes)),
        "checksums": {name: world_reference.hash64(bytes(data)) for name, data in group_bytes.items()},
        "allele_bytes": {name: bytes(data) for name, data in group_bytes.items()},
    }


def old_slots_unchanged(before: dict, after: dict, loci=LOCI) -> bool:
    """Every autosome bit outside the new slots of `loci` is the same genome for genome."""
    new_bits = {(segment, slot) for _, _, segment, slot in loci}
    pairs = list(iter_genomes(before)), list(iter_genomes(after))
    if len(pairs[0]) != len(pairs[1]):
        return False
    for (left, _, _), (right, _, _) in zip(*pairs):
        for side in (0, 1):
            for segment in range(8):
                for slot in range(24):
                    if (segment, slot) in new_bits:
                        continue
                    if allele_at(left["genome"][side], segment, slot) != allele_at(right["genome"][side], segment, slot):
                        return False
        if left["genome"][2:] != right["genome"][2:]:
            return False
    return True


def twin_genomes_match(pool: dict) -> bool:
    """Genomes that share a genome key are identical, including the new slots."""
    seen = {}
    for person, _, _ in iter_genomes(pool):
        previous = seen.get(person["genome_key"])
        if previous is not None and previous != person["genome"]:
            return False
        seen[person["genome_key"]] = person["genome"]
    return True


def child_shares(pool: dict, child_key: int, mother_key: int, father_key: int, loci=LOCI) -> bool:
    """Each new copy of the child equals the parent copy the draw selected."""
    genomes = {person["lineage_key"]: person for person, _, _ in iter_genomes(pool)}
    child = genomes[child_key]
    mother = genomes[mother_key]["genome"]
    father = genomes[father_key]["genome"]
    for locus_id, _, segment, slot in loci:
        maternal, paternal = copies_at(child["genome"], segment, slot)
        u0 = draw_unit(pool["pool_seed"], child["genome_key"], locus_id, 0)
        u1 = draw_unit(pool["pool_seed"], child["genome_key"], locus_id, 1)
        mother_copies = copies_at(mother, segment, slot)
        father_copies = copies_at(father, segment, slot)
        if maternal != (mother_copies[0] if u0 < 0.5 else mother_copies[1]):
            return False
        if paternal != (father_copies[0] if u1 < 0.5 else father_copies[1]):
            return False
    return True


def snapshot(pool: dict) -> dict:
    """A deep copy of the genomes and the links the extension reads."""
    import copy
    return copy.deepcopy(pool)


def fixture_pool() -> dict:
    """Founders, a child of both, a child with one missing parent, identical twins, and
    an unborn pair that shares a genome key. Keys are founders of `test:temperament`,
    the same construction as `LineageKey::founder`."""
    tag = "test:temperament"
    keys = [founder_key(tag, index) for index in range(10)]
    mother, father, child, half, twin_a, twin_b, unborn, unborn_twin, absent = keys[:9]

    def person(key, sex, born, parents, genome_key=None, birth_at=None):
        genome = blank_genome(sex)
        return {
            "lineage_key": key,
            "genome_key": key if genome_key is None else genome_key,
            "sex": sex,
            "genome": genome,
            "born": born,
            "mother": parents[0] if parents else None,
            "father": parents[1] if parents else None,
            "birth_at": birth_at,
        }

    due = 5_000_000
    members = [
        person(mother, 0, 0, None, birth_at=due),
        person(father, 1, 10, None),
        person(child, 0, 100, (mother, father)),
        person(half, 1, 110, (mother, absent)),
        person(twin_a, 0, 200, (mother, father)),
        person(twin_b, 0, 200, (mother, father), genome_key=twin_a),
    ]
    # An old slot (segment 0 slot 0, core:disease_ar) and a new slot preset to 1, so the
    # extension must keep the old bit and overwrite the new one.
    members[0]["genome"] = write_copies(members[0]["genome"], 0, 0, (1, 0))
    members[0]["genome"] = write_copies(members[0]["genome"], 0, 12, (1, 0))
    unborn_children = [
        {"lineage_key": unborn, "genome_key": unborn, "sex": 0, "genome": blank_genome(0)},
        {"lineage_key": unborn_twin, "genome_key": unborn, "sex": 0, "genome": blank_genome(0)},
    ]
    return {
        "pool_seed": 0x5EED,
        "households": [
            {
                "members": members,
                "pregnancies": [
                    {"mother": mother, "father": father, "children": unborn_children},
                ],
            }
        ],
        "_keys": {
            "mother": mother,
            "father": father,
            "child": child,
            "half": half,
            "twin_a": twin_a,
            "twin_b": twin_b,
            "unborn": unborn,
            "unborn_twin": unborn_twin,
            "absent": absent,
        },
    }


def alleles_of(person, loci=LOCI) -> str:
    bits = []
    for _, _, segment, slot in loci:
        bits.extend(str(bit) for bit in copies_at(person["genome"], segment, slot))
    return "".join(bits)


def run_fixture() -> None:
    pool = fixture_pool()
    keys = pool.pop("_keys")
    before = snapshot(pool)
    report = extend_pool(pool)
    again = snapshot(pool)
    extend_pool(pool)
    people = {person["lineage_key"]: person for person, _, _ in iter_genomes(pool)}
    mother = people[keys["mother"]]
    print(f"extension_version 1")
    print(f"loci {len(LOCI)}")
    print(f"fixture_seed {pool['pool_seed']:x}")
    for name, key in keys.items():
        print(f"fixture_key {name} {key:016x}")
    for name in ("mother", "father", "child", "half", "twin_a", "twin_b", "unborn", "unborn_twin"):
        print(f"fixture_alleles {name} {alleles_of(people[keys[name]])}")
    print(f"fixture_checksum {report['allele_checksum']:016x}")
    print(f"fixture_mendelian {report['mendelian']}")
    print(f"fixture_genomes {report['genomes']}")
    print(f"fixture_old_slot {copies_at(mother['genome'], 0, 0)[0]}")
    print(f"fixture_old_slots_unchanged {int(old_slots_unchanged(before, pool))}")
    print(f"fixture_idempotent {int(snapshot(pool)['households'] == again['households'])}")
    print(f"fixture_twins_equal {int(people[keys['twin_a']]['genome'] == people[keys['twin_b']]['genome'])}")
    print(f"fixture_unborn_twins_equal {int(people[keys['unborn']]['genome'] == people[keys['unborn_twin']]['genome'])}")
    print(f"fixture_child_shares {int(child_shares(pool, keys['child'], keys['mother'], keys['father']))}")
    # The mother born at the twins' second: whatever the key order, the twins stay equal.
    late = fixture_pool()
    late_keys = late.pop("_keys")
    for household in late["households"]:
        for member in household["members"]:
            if member["lineage_key"] == late_keys["mother"]:
                member["born"] = 200
    extend_pool(late)
    late_people = {person["lineage_key"]: person for person, _, _ in iter_genomes(late)}
    print(f"fixture_late_mother_twins_equal {int(late_people[late_keys['twin_a']]['genome'] == late_people[late_keys['twin_b']]['genome'])}")
    print(f"fixture_late_mother_twin_a {alleles_of(late_people[late_keys['twin_a']])}")
    # The half-orphan's paternal copy is the independent draw, not a parent's allele.
    half = people[keys["half"]]
    independent = True
    for locus_id, _, segment, slot in LOCI:
        u = draw_unit(pool["pool_seed"], half["genome_key"], locus_id, 1)
        if copies_at(half["genome"], segment, slot)[1] != (1 if u < 0.5 else 0):
            independent = False
    print(f"fixture_half_father_independent {int(independent)}")
    print(f"fixture_frequency_bound {FREQUENCY_BOUND}")
    run_fixture_map6(people, keys, report)


def run_fixture_map6(people: dict, keys: dict, report: dict) -> None:
    """Map 6 (ADR-0034 section 2): the same fixture extended with the 42 loci. The 40
    temperament alleles of every genome are those of the map-5 extension above, bit for bit;
    the lines give the hair alleles (`_01` maternal, paternal, then `_02`) and the checksums."""
    pool = fixture_pool()
    pool.pop("_keys")
    before = snapshot(pool)
    six = extend_pool(pool, MAP6_LOCI)
    again = snapshot(pool)
    extend_pool(pool, MAP6_LOCI)
    sixes = {person["lineage_key"]: person for person, _, _ in iter_genomes(pool)}
    names = ("mother", "father", "child", "half", "twin_a", "twin_b", "unborn", "unborn_twin")
    for name in names:
        assert alleles_of(sixes[keys[name]], TEMPERAMENT_LOCI) == alleles_of(people[keys[name]]), name
    assert six["checksums"]["temperament"] == report["allele_checksum"]
    print("fixture_map6_extension_version 2")
    print(f"fixture_map6_loci {len(MAP6_LOCI)}")
    for name in names:
        print(f"fixture_map6_hair_alleles {name} {alleles_of(sixes[keys[name]], HAIR_LOCI)}")
    print(f"fixture_map6_checksum {six['allele_checksum']:016x}")
    print(f"fixture_map6_checksum_temperament {six['checksums']['temperament']:016x} (the map-5 fixture checksum)")
    print(f"fixture_map6_checksum_hair {six['checksums']['hair']:016x}")
    print(f"fixture_map6_mendelian {six['mendelian']}")
    print(f"fixture_map6_old_slots_unchanged {int(old_slots_unchanged(before, pool, MAP6_LOCI))}")
    print(f"fixture_map6_idempotent {int(snapshot(pool)['households'] == again['households'])}")
    print(f"fixture_map6_twins_equal {int(sixes[keys['twin_a']]['genome'] == sixes[keys['twin_b']]['genome'])}")
    print(f"fixture_map6_unborn_twins_equal {int(sixes[keys['unborn']]['genome'] == sixes[keys['unborn_twin']]['genome'])}")
    print(f"fixture_map6_child_shares {int(child_shares(pool, keys['child'], keys['mother'], keys['father'], MAP6_LOCI))}")


def run_real(path: Path) -> bytes:
    """The map-5 extension (40 loci) of the real pool; returns the temperament allele bytes."""
    raw = path.read_bytes()
    pool = pool_reference.read_pool(raw)
    file_hash = world_reference.hash64(raw)
    encoded = pool_reference.encode_pool(pool)
    if encoded != raw:
        raise SystemExit("the pool file does not round-trip through the reference encoder")
    before = snapshot(pool)
    report = extend_pool(pool)
    second = snapshot(pool)
    extend_pool(pool)
    frequency_ok = abs(report["frequency"] - 0.5) < FREQUENCY_BOUND
    print(f"file_hash {file_hash:016x}")
    print(f"frozen_hash {pool['frozen_hash']:016x}")
    print(f"pool_seed {pool['pool_seed']:016x}")
    print(f"genomes {report['genomes']}")
    print(f"mendelian {report['mendelian']}")
    print(f"mendelian_expected {MENDELIAN_GENOMES}")
    print(f"alt {report['alt']}")
    print(f"copies {report['copies']}")
    print(f"frequency {report['frequency']:.16f}")
    print(f"frequency_within_bound {int(frequency_ok)}")
    print(f"frequency_bound {FREQUENCY_BOUND}")
    print(f"allele_checksum {report['allele_checksum']:016x}")
    print(f"extended_hash {world_reference.hash64(pool_reference.encode_pool(pool)):016x}")
    print(f"old_slots_unchanged {int(old_slots_unchanged(before, pool))}")
    print(f"idempotent {int(pool['households'] == second['households'])}")
    print(f"twins_match {int(twin_genomes_match(pool))}")
    if report["genomes"] != POOL_GENOMES:
        raise SystemExit(f"genome count {report['genomes']} is not {POOL_GENOMES}")
    if report["mendelian"] != MENDELIAN_GENOMES:
        raise SystemExit(
            f"mendelian count {report['mendelian']} is not {MENDELIAN_GENOMES}; "
            "check the living-parent definition before changing the assertion"
        )
    if report["allele_checksum"] != TEMPERAMENT_CHECKSUM:
        raise SystemExit(f"temperament checksum {report['allele_checksum']:016x} is not {TEMPERAMENT_CHECKSUM:016x}")
    return report["allele_bytes"]["temperament"]


def run_real_map6(path: Path, temperament: bytes) -> None:
    """The map-6 extension (42 loci: the 40 temperament loci, then `core:hair_thickness_01`
    and `_02`) of the real pool, from a fresh read of the file. The 40 temperament alleles
    must equal those of the map-5 extension byte for byte (`temperament`, per genome, per
    locus, per copy), and their checksum stays the regression value."""
    raw = path.read_bytes()
    pool = pool_reference.read_pool(raw)
    before = snapshot(pool)
    report = extend_pool(pool, MAP6_LOCI)
    second = snapshot(pool)
    extend_pool(pool, MAP6_LOCI)
    same = report["allele_bytes"]["temperament"] == temperament
    hair = report["allele_bytes"]["hair"]
    hair_alt = sum(hair)
    extended = pool_reference.encode_pool(pool)
    assert extended == extended_bytes(raw, MAP6_LOCI)
    print("map6_extension_version 2")
    print(f"map6_loci {len(MAP6_LOCI)}")
    print("map6_placement " + " ".join(f"{locus_id}@{segment}.{slot}" for locus_id, _, segment, slot in HAIR_LOCI))
    print(f"map6_genomes {report['genomes']}")
    print(f"map6_mendelian {report['mendelian']}")
    print(f"map6_alt {report['alt']}")
    print(f"map6_copies {report['copies']}")
    print(f"map6_frequency {report['frequency']:.16f}")
    print(f"map6_hair_alt {hair_alt}")
    print(f"map6_hair_copies {len(hair)}")
    print(f"map6_hair_frequency {hair_alt / len(hair):.16f}")
    print(f"map6_hair_frequency_within_bound {int(abs(hair_alt / len(hair) - 0.5) < FREQUENCY_BOUND)}")
    print(f"map6_allele_checksum {report['allele_checksum']:016x}")
    print(f"map6_allele_checksum_temperament {report['checksums']['temperament']:016x}")
    print(f"map6_allele_checksum_hair {report['checksums']['hair']:016x}")
    print(f"map6_temperament_alleles_equal_map5 {int(same)}")
    print(f"map6_extended_hash {world_reference.hash64(extended):016x}")
    print(f"map6_old_slots_unchanged {int(old_slots_unchanged(before, pool, MAP6_LOCI))}")
    print(f"map6_idempotent {int(pool['households'] == second['households'])}")
    print(f"map6_twins_match {int(twin_genomes_match(pool))}")
    if not same or report["checksums"]["temperament"] != TEMPERAMENT_CHECKSUM:
        raise SystemExit("the temperament alleles of map 6 differ from those of map 5")
    if report["genomes"] != POOL_GENOMES or report["mendelian"] != MENDELIAN_GENOMES:
        raise SystemExit(f"map 6 counts {report['genomes']} {report['mendelian']}")


def main() -> None:
    command = sys.argv[1] if len(sys.argv) > 1 else "fixture"
    if command == "fixture":
        run_fixture()
    elif command == "real":
        path = Path(sys.argv[2]) if len(sys.argv) > 2 else ROOT / "data" / "pool" / "pool.bin"
        temperament = run_real(path)
        run_real_map6(path, temperament)
    else:
        raise SystemExit("usage: pool_extension_reference.py fixture | real [POOL]")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""The pool file (`cargo xtask prerun pool`, step 11; design note of the pool file, ADR-0020
sections 4 and 5, Grok's G1 to G3), written from the note, the ADRs and the genetics
specification, independent of the Rust code: the pool run, the genome keys from its log,
who is in the pool, the alleles at the seven loci, the afflictions after the assignment,
what a pool person and a pool household carry, the ancestors, the file and its reader.

1. The pool run (note section 1; design note of the pre-run, sections 1 to 3): the pre-run
   data of the frozen map (`prerun_reference.prerun_documents`: without the five disease
   loci, MC1R and HERC2, without the five Mendelian diseases; `pool_frequencies` is not
   part of them, decision 1 of NOTES.md), the founders of the pre-run (`prerun_reference.
   founders` with `POOL_SEED` 0x11b019a2 and the tag `core:prerun_pool`), the switch on, no
   couples, capacity `K` = the number of founders, run to the second `E_p = years · YEAR`
   inclusive (`World.advance`, year by year; the steps change nothing). The genome keys come
   from the log: every `core:meiosis` entry (payload 7) names the genome key of its zygote;
   a `core:conception` entry (payload 6) whose lineage key is such a genome key is the first
   child of its zygote (its genome key is its lineage key), any other is an identical
   multiple of the child of the conception entry before it (the same second, the same
   mother: asserted) and takes that child's genome key. A founder's genome key is its
   lineage key. Checked against the record of the conceptions (`World.multiples`).
2. Who (note section 2): at `E_p`, after every event of that second. Assigned: every living
   sim and every unborn child of a running pregnancy. Pool households: not ended, with a
   living member of 18 or more (`E_p − born ≥ 18 · YEAR`), ascending HouseholdId. Pool
   persons: their living members and the unborn children of their members' pregnancies.
3. The alleles (note section 3, G1, G2): the order is the living by (born, lineage key),
   then the unborn by (the due second of the mother's pending birth, lineage key). Draw
   `u = unit` at (pool seed, `POOL_ALLELE` = 26, the genome key, `str_key` of the locus,
   the gamete: 0 maternal, 1 paternal). A parent counts iff it is living at `E_p` (in
   `World.sims`); an unborn child's parents are the pregnant woman and the pregnancy's
   father. Autosomal (`core:disease_ar`, `core:disease_ad_late`, `core:disease_ad_partial`,
   `core:mc1r`, `core:herc2`), gamete 0 then 1: from a living parent its maternal copy if
   `u < 0.5`, else its paternal copy (drawn even when both are equal); from a missing parent
   1 if `u < q`, else 0. `core:disease_xr`: gamete 0 as above from the mother's two X
   copies; a daughter's gamete 1 is the living father's one X allele without a draw, else
   1 if `u < q`; a son has no gamete 1. `core:disease_mt`: one copy, gamete 0: the living
   mother's copy without a draw, else 1 if `u < q`. The alleles are written into a copy of
   the person's genome at the frozen map's positions; an unborn child keeps the rest of its
   genome. Identical multiples share the genome key, so they draw the same `u`.
4. The afflictions (note section 4): per pool person, living and unborn, as at its creation
   (ADR-0017 section 3; `World.afflictions_of` of tools/world_reference.py on the data of the
   frozen map, its full calibration, with the pool seed): a founder of the pool run at the
   age `0 − born` without parents' keys, a child at the age 0 with its parents' keys. The
   threshold disease must come out as the person had it in the pool run (asserted for the
   living). Exposures are not carried.
5. The fields (note section 5) and 6. the ancestors (G3): see `person_entry`,
   `household_entry` and `ancestor_entries`.
7. The file: `b"SIMPOOL"`, `varint(1)`, then the postcard of `Pool` (tools/
   postcard_reference.py conventions; the layout is `encode_pool`'s docstring). The pool
   hash is xxh3-64 (seed 0) of the whole file. The reader (`read_pool`: `decode_pool`, then
   `violations`) rejects a wrong magic, a wrong version, trailing bytes, a household without
   a member of 18 or more at `frozen_at`, members out of order, ancestors out of order, a
   parent key of a person that is no record, an open partnership whose partners are not
   members (`RULES`, in this order; a body that does not decode is `decode`, no rule of the
   note); it round-trips byte for byte.

The `pool_frequencies` calibration entry (note section 3): `with_pool_frequencies` adds it
to a frozen calibration document, `check_pool_frequencies` is the pool's data check
(exactly the seven loci, unit `fraction`, texts filled, each `q` in (0, 1), each a locus of
the frozen map), `pool_frequencies_json` is its exact JSON text.

Usage: .venv/bin/python tools/pool_reference.py [--out DIR]
       (prints the known answers as `label value` lines; with --out also writes
       `<DIR>/<run>/pool.bin` per run, the rejected files `<DIR>/reject/<rule>.bin`, the
       frozen map of the known answers `<DIR>/frozen_map` (the template with
       `pool_frequencies`) and that of each scenario `<DIR>/frozen_map_<scenario>`)
       .venv/bin/python tools/pool_reference.py --scan FOUNDERS YEARS [SCENARIO]
       (one run, the cases of note section 8 present at every year end second
       `y · YEAR`, y = 1 to YEARS; for choosing the runs, not a pin)
"""

import copy
import math
import struct
import sys
import time
import types
from pathlib import Path

import data_reference
import map4_reference
import prerun_reference
import world_reference
from postcard_reference import FEMALE, f64, flag, genome_of, seq, signed, string, varint
from rng_reference import chacha_key, str_key, unit, words
from world_reference import (
    ATTEMPT_KIND,
    BIRTH_KIND,
    IDENTICAL,
    LEAVE_HOME_KIND,
    PARTNERSHIP_SUBJECT,
    SEPARATION_KIND,
    SIM_SUBJECT,
    YEAR,
    hash64,
)

ROOT = Path(__file__).resolve().parent.parent
TEMPLATE = ROOT / "tests" / "golden" / "map4_template"

# The purpose tag of the pool's alleles (note section 3, ADR-0020 section 4): the next free
# tag after PRERUN_ALLELE (25) in tests/golden/rng_tags.txt.
POOL_ALLELE = 26
POOL_SEED, POOL_TAG = prerun_reference.POOL_SEED, prerun_reference.POOL_TAG
# The file (note section 7).
POOL_MAGIC = b"SIMPOOL"
POOL_FORMAT_VERSION = 1
# The seven loci and their frequencies `q` of the alternative allele (note section 3, ADR-0020
# section 4), in the order of the note's table.
POOL_FREQUENCIES = (
    ("core:disease_ar", 0.02),
    ("core:disease_ad_late", 3e-5),
    ("core:disease_ad_partial", 1e-3),
    ("core:disease_xr", 2.5e-4),
    ("core:disease_mt", 1.2e-4),
    ("core:mc1r", 0.15),
    ("core:herc2", 0.75),
)
AUTOSOMAL = ("core:disease_ar", "core:disease_ad_late", "core:disease_ad_partial", "core:mc1r", "core:herc2")
X_LOCUS, MT_LOCUS = "core:disease_xr", "core:disease_mt"
ADULT = 18 * YEAR
DEPTH = 3
# The JSON texts of the entry (note section 3; only the numbers and the unit enter a check).
FREQUENCY_AS_OF = "2026-10-13"
FREQUENCY_SOURCES = {
    "core:disease_ar": "ADR-0020 section 4 (Grok, F3, 2026-10-12)",
    "core:disease_ad_late": "ADR-0020 section 4 (Grok, F3, 2026-10-12)",
    "core:disease_ad_partial": "ADR-0020 section 4 (Grok, F3, 2026-10-12)",
    "core:disease_xr": "ADR-0020 section 4 (Grok, F3, 2026-10-12)",
    "core:disease_mt": "ADR-0020 section 4 (Grok, F3, 2026-10-12)",
    "core:mc1r": "ADR-0020 section 4, game value K8.9a",
    "core:herc2": "ADR-0020 section 4, game value of the one northern European profile (Dario)",
}
FREQUENCY_NOTE = "frequency of the alternative allele for a missing parent in the pool (G2)"


def bits(value: float) -> str:
    return data_reference.f64_bits(value)


# ---------------------------------------------------------------------------------------
# The calibration entry `pool_frequencies` (note section 3)


def pool_frequencies_entry() -> dict:
    """The object `pool_frequencies`: locus ID to a calibrated number (value, unit
    `fraction`, source, as_of, note), the keys in ascending string order (the order a
    BTreeMap<DataId, Calibrated> writes, decision 2 of NOTES.md)."""
    return {
        locus: {
            "value": q,
            "unit": "fraction",
            "source": FREQUENCY_SOURCES[locus],
            "as_of": FREQUENCY_AS_OF,
            "note": FREQUENCY_NOTE,
        }
        for locus, q in sorted(POOL_FREQUENCIES)
    }


def with_pool_frequencies(calibration: dict) -> dict:
    """A copy of a frozen calibration document with `pool_frequencies` added as its last
    field (after `fertility`)."""
    assert "pool_frequencies" not in calibration
    out = copy.deepcopy(calibration)
    out["pool_frequencies"] = pool_frequencies_entry()
    return out


def without_pool_frequencies(calibration: dict) -> dict:
    """The calibration of format 7 the core reads today (the pool run's data, decision 1)."""
    out = copy.deepcopy(calibration)
    out.pop("pool_frequencies", None)
    return out


def check_pool_frequencies(calibration: dict, genome_map: dict) -> dict:
    """The pool's data check (note section 3): `pool_frequencies` holds exactly the seven
    loci, each a locus of the frozen map with an object of the unit `fraction`, its texts
    filled, its value finite and `0 < q < 1`. Returns {locus: q}. Each failure an
    AssertionError naming its rule (`pool.frequencies_loci`, `pool.frequency_unit`,
    `pool.frequency_text`, `pool.frequency_range`)."""
    entry = calibration.get("pool_frequencies")
    wanted = {locus for locus, _ in POOL_FREQUENCIES}
    assert isinstance(entry, dict) and set(entry) == wanted, f"pool.frequencies_loci {sorted(entry) if isinstance(entry, dict) else entry}"
    loci = {locus["id"] for locus in genome_map["loci"]}
    assert wanted <= loci, f"pool.frequencies_loci not in the map: {sorted(wanted - loci)}"
    out = {}
    for locus in sorted(entry):
        value = entry[locus]
        assert value.get("unit") == "fraction", f"pool.frequency_unit {locus}"
        for name in ("source", "as_of", "note"):
            assert isinstance(value.get(name), str) and value[name].strip(), f"pool.frequency_text {locus} {name}"
        assert set(value) == {"value", "unit", "source", "as_of", "note"}, f"pool.frequencies_loci {locus} fields {sorted(value)}"
        q = float(value["value"])
        assert math.isfinite(q) and 0.0 < q < 1.0, f"pool.frequency_range {locus} {q!r}"
        out[locus] = q
    return out


def pool_frequencies_json(entry=None) -> str:
    """The entry (the note's by default) as serde_json's pretty printer writes it as the
    last member of the calibration's top-level object (two spaces of indentation): `
    "pool_frequencies": {...}` (the text of `frozen_calibration_text` from that line on,
    without the final `}`)."""
    return '  "pool_frequencies": ' + map4_reference.pretty(pool_frequencies_entry() if entry is None else entry, 1)


def frozen_calibration_text(calibration: dict) -> str:
    """The frozen calibration with `pool_frequencies` as a file (serde_json's pretty
    printer, tools/map4_reference.py `text`)."""
    text = map4_reference.text(calibration)
    assert list(calibration)[-1] == "pool_frequencies"
    assert text.endswith(pool_frequencies_json(calibration["pool_frequencies"]) + "\n}\n")
    return text


def pool_frequencies_bytes() -> bytes:
    """The postcard of the entry as a BTreeMap<DataId, Calibrated> (texts stripped): what
    the data hash would add if the field were hashed (not hashed in this slice, decision 1;
    printed for the arrival slice)."""
    entry = data_reference.strip(pool_frequencies_entry())
    out = varint(len(entry))
    for locus in sorted(entry):
        out += string(locus) + data_reference.calibrated_bytes(entry[locus])
    return out


# ---------------------------------------------------------------------------------------
# 1. The pool run and the genome keys


class Cursor:
    """Reads postcard values from bytes (tools/postcard_reference.py conventions)."""

    def __init__(self, data: bytes, at: int = 0):
        self.data = data
        self.at = at

    def left(self) -> int:
        return len(self.data) - self.at

    def take(self, n: int) -> bytes:
        if n > self.left():
            raise DecodeError("decode: unexpected end")
        out = self.data[self.at : self.at + n]
        self.at += n
        return out

    def byte(self) -> int:
        return self.take(1)[0]

    def varint(self, bits_: int = 64) -> int:
        value, shift = 0, 0
        for _ in range((bits_ + 6) // 7):
            b = self.byte()
            value |= (b & 0x7F) << shift
            shift += 7
            if not b & 0x80:
                if value >= 1 << bits_:
                    raise DecodeError("decode: varint out of range")
                return value
        raise DecodeError("decode: varint too long")

    def signed(self) -> int:
        z = self.varint(64)
        return (z >> 1) if not z & 1 else -((z + 1) >> 1)

    def string(self) -> str:
        return self.take(self.varint(64)).decode("utf-8")

    def boolean(self) -> bool:
        b = self.byte()
        if b > 1:
            raise DecodeError("decode: bool")
        return b == 1

    def option(self, read):
        tag = self.byte()
        if tag > 1:
            raise DecodeError("decode: option tag")
        return read() if tag == 1 else None

    def vec(self, read) -> list:
        return [read() for _ in range(self.varint(64))]

    def f64(self) -> float:
        return struct.unpack("<d", self.take(8))[0]


class DecodeError(Exception):
    pass


def parse_entry(entry: bytes) -> tuple:
    """(id, time, subjects as SimIds, payload variant, the payload's first field for the
    variants 6 and 7) of one log entry (ADR-0004 section 2; tools/world_reference.py
    `_entry`): `varint(id) + signed(t) + seq(subjects) + seq(causes) + payload`."""
    c = Cursor(entry)
    event_id = c.varint()
    t = c.signed()
    subjects = []
    for _ in range(c.varint()):
        assert c.varint() == 0, "a subject is a sim"
        subjects.append(c.varint())
    for _ in range(c.varint()):
        cause = c.varint()
        if cause in (0, 2):
            c.varint()
        elif cause == 1:
            c.varint()
            c.signed()
        elif cause == 3:
            c.string()
        else:
            assert cause == 4
            for _ in range(4):
                c.varint()
    kind = c.varint()
    first = c.varint() if kind in (6, 7) else None
    return event_id, t, subjects, kind, first


def genome_keys_from_log(log: list) -> tuple:
    """The conceived children and their genome keys (note section 1) from the log entries:
    returns (conceptions as [(time, mother, father, lineage key, genome key)] in log order,
    {lineage key: genome key})."""
    parsed = [parse_entry(entry) for entry in log]
    meiosis = {first for _, _, _, kind, first in parsed if kind == 7}
    conceptions, keys = [], {}
    for _, t, subjects, kind, key in parsed:
        if kind != 6:
            continue
        mother, father = subjects
        if key in meiosis:
            genome_key = key
        else:
            # An identical multiple: the child of the conception entry before it, the same
            # second and the same mother.
            before = conceptions[-1]
            assert before[0] == t and before[1] == mother, (t, mother, before)
            genome_key = before[4]
        assert key not in keys
        keys[key] = genome_key
        conceptions.append((t, mother, father, key, genome_key))
    return conceptions, keys


def check_genome_keys(world, conceptions: list) -> None:
    """The keys of the log against the conceptions the model recorded (`World.multiples`):
    each child its own genome key, but the second of identical twins that of the first."""
    expected = []
    for t, woman, man, _, _, kind, children in world.multiples:
        first = children[0][0]
        for index, (key, _) in enumerate(children):
            expected.append((t, woman, man, key, first if kind == IDENTICAL and index == 1 else key))
    assert expected == conceptions, "the genome keys of the log differ from the conceptions"


def frozen_template_documents() -> tuple:
    """The frozen map of these known answers: the template of map 4 (tests/golden/
    map4_template) with the repository's life course data, as `prerun pool --map` reads a
    frozen map, and its calibration with `pool_frequencies`."""
    demography, calibration, genome_map = prerun_reference.template_documents()
    return demography, with_pool_frequencies(calibration), genome_map


def scenario_documents(documents: tuple, scenario) -> tuple:
    """A scenario of the pool data (note section 8: a case the natural runs lack): values
    of the calibration (`calibration`), of its diseases (`diseases`: a threshold carrier's
    `prevalence`, a `death` effect's `per_year`), of its `pool_frequencies` and of the life
    course data (`demography`) changed in the frozen documents before the pre-run data are
    made from them."""
    demography, calibration, genome_map = (copy.deepcopy(doc) for doc in documents)
    for code, value in scenario.get("demography", {}).items():
        demography["values"][code]["value"] = value
    for code, value in scenario.get("calibration", {}).items():
        calibration["values"][code]["value"] = value
    for locus, value in scenario.get("pool_frequencies", {}).items():
        calibration["pool_frequencies"][locus]["value"] = value
    for code, changes in scenario.get("diseases", {}).items():
        (disease,) = [entry for entry in calibration["diseases"] if entry["code"] == code]
        for name, value in changes.items():
            if name == "prevalence":
                disease["carrier"]["threshold"]["prevalence"]["value"] = value
            else:
                assert name == "per_year"
                disease["effect"]["death"]["per_year"]["value"] = value
    return demography, calibration, genome_map


def pool_data(frozen: tuple) -> tuple:
    """(the pre-run data the pool run runs on, the data of the frozen map, the frequencies):
    the pool's data check on the frozen calibration, then both without `pool_frequencies`."""
    demography, calibration, genome_map = frozen
    q = check_pool_frequencies(calibration, genome_map)
    plain = (demography, without_pool_frequencies(calibration), genome_map)
    run_data = prerun_reference.prerun_data(prerun_reference.prerun_documents(plain))
    frozen_data = world_reference.Data(*plain)
    return run_data, frozen_data, q


def pool_run(run_data, n: int, years: int, observe=None):
    """The pool run with `n` founders to `E_p = years · YEAR` inclusive. `observe(world,
    y)` is called after each second `y · YEAR` (y = 1 to `years`), for the scan."""
    table = prerun_reference.weight_table(run_data)
    count = prerun_reference.counts(table, n)
    loci = [locus["id"] for locus in run_data.measured_loci]
    founder_list = prerun_reference.founders(count, POOL_SEED, POOL_TAG, loci)
    world = world_reference.World(POOL_SEED, POOL_TAG, True, [founder for founder, _ in founder_list], (), run_data, capacity=n)
    for year in range(years):
        world.advance((year + 1) * YEAR - 1)
        world.advance((year + 1) * YEAR)
        if observe is not None:
            observe(world, year + 1)
    assert world.closed == years * YEAR
    return world, founder_list


# ---------------------------------------------------------------------------------------
# 2. Who is in the pool


def pool_households(world, frozen_at: int) -> list:
    """The pool households, ascending by HouseholdId: not ended, with a living member of 18
    or more at `frozen_at`."""
    out = []
    for household in sorted(world.households):
        if world.households[household][1] is not None:
            continue
        members = world.members(household)
        if any(frozen_at - world.sims[sim][0] >= ADULT for sim in members):
            out.append(household)
    return out


def assigned_persons(world, genome_keys: dict) -> list:
    """Every person section 3 assigns, in its order: the living by (born, lineage key), then
    the unborn by (due second, lineage key). A person is a dict: `key`, `genome_key`,
    `sex`, `genome` (a copy), `sim` (None for the unborn), `born` or `due`, `mother` and
    `father` as SimIds (None for a founder of the run)."""
    living = []
    for sim, value in world.sims.items():
        born, key, sex, genome = value[:4]
        parents = value[7]
        living.append(
            {
                "key": key,
                "genome_key": key if parents is None else genome_keys[key],
                "sex": sex,
                "genome": copy.deepcopy(genome),
                "sim": sim,
                "born": born,
                "due": None,
                "mother": None if parents is None else parents[0],
                "father": None if parents is None else parents[1],
                "unborn": False,
            }
        )
    living.sort(key=lambda person: (person["born"], person["key"]))
    unborn = []
    for mother, (father, conception_number, children, conceived) in world.pregnancies.items():
        (due,) = [event.time for event in world.queue if event.kind == BIRTH_KIND and event.subject == (SIM_SUBJECT, mother)]
        # ADR-0020 section 4: the due second is the conception plus the pregnancy.
        assert due == conceived + world.data.pregnancy_months * world_reference.MONTH
        for index, (key, sex, genome) in enumerate(children):
            unborn.append(
                {
                    "key": key,
                    "genome_key": genome_keys[key],
                    "sex": sex,
                    "genome": copy.deepcopy(genome),
                    "sim": None,
                    "born": None,
                    "due": due,
                    "mother": mother,
                    "father": father,
                    "unborn": True,
                    "conception": conception_number,
                    "conceived": conceived,
                    "index": index,
                }
            )
    unborn.sort(key=lambda person: (person["due"], person["key"]))
    return living + unborn


# ---------------------------------------------------------------------------------------
# 3. The alleles


def positions(genome_map: dict) -> dict:
    """The seven loci of the frozen map by ID (chromosome, segment, slot)."""
    loci = {locus["id"]: locus for locus in genome_map["loci"]}
    out = {}
    for locus, _ in POOL_FREQUENCIES:
        entry = loci[locus]
        out[locus] = entry
    assert all(out[locus]["chromosome"] == "autosome" for locus in AUTOSOMAL)
    assert out[X_LOCUS]["chromosome"] == "x" and out[MT_LOCUS]["chromosome"] == "mt"
    return out


def copies_of(genome: dict, locus: dict) -> list:
    return world_reference.copies_at(genome, locus)


def write_copies(genome: dict, locus: dict, copies: list) -> None:
    """Writes the copies (maternal then paternal; one X copy for a man, one mt copy) at the
    locus's position."""
    chromosome, slot = locus["chromosome"], locus["slot"]
    if chromosome == "autosome":
        genome["autosomes_maternal"][locus["segment"]][slot], genome["autosomes_paternal"][locus["segment"]][slot] = copies
    elif chromosome == "x":
        genome["x_maternal"][slot] = copies[0]
        # The X from the father: `x_paternal` (a founder) or `paternal_sex` {"x": slots} (a
        # daughter of the meiosis); a son's is a Y.
        name, second = world_reference.from_father(genome)
        if name == "x":
            (second[slot],) = copies[1:]
        else:
            assert len(copies) == 1
    else:
        assert chromosome == "mt" and len(copies) == 1
        genome["mt"][slot] = copies[0]


def pool_unit(genome_key: int, locus: str, gamete: int) -> float:
    """`u = unit` of the draw (pool seed, POOL_ALLELE, the genome key, `str_key` of the
    locus, the gamete)."""
    return unit(words(chacha_key(POOL_SEED, POOL_ALLELE, genome_key, str_key(locus), gamete))[0])


def assign(world, persons: list, loci: dict, q: dict) -> list:
    """Section 3: the alleles of every person, in order; writes them into each person's
    genome copy and records per person `alleles` {locus: copies} and `draws` [(locus,
    gamete, source, u or None, allele)] with the source `mother`, `father` (a living
    parent), `frequency` (a missing parent) or `copy` (no draw: the father's X, the mother's
    mt)."""
    by_sim = {}
    for person in persons:
        living_parents = {}
        for side in ("mother", "father"):
            sim = person[side]
            if sim is not None and sim in world.sims:
                assert sim in by_sim, "a living parent comes before its child"
                living_parents[side] = by_sim[sim]
        person["living_parents"] = sorted(living_parents)
        person["alleles"], person["draws"] = {}, []
        for locus, _ in POOL_FREQUENCIES:
            freq = q[locus]

            def gamete(side: str, number: int, source_copies) -> int:
                """One gamete with a draw: from the living parent's copies, else from q."""
                u = pool_unit(person["genome_key"], locus, number)
                if side in living_parents:
                    allele = source_copies[0] if u < 0.5 else source_copies[1]
                    person["draws"].append((locus, number, side, u, allele))
                else:
                    allele = 1 if u < freq else 0
                    person["draws"].append((locus, number, "frequency", u, allele))
                return allele

            def parent_copies(side: str):
                return person_copies(living_parents[side], loci[locus]) if side in living_parents else None

            if locus in AUTOSOMAL:
                copies = [gamete("mother", 0, parent_copies("mother")), gamete("father", 1, parent_copies("father"))]
            elif locus == X_LOCUS:
                mother_x = parent_copies("mother")
                assert mother_x is None or len(mother_x) == 2
                copies = [gamete("mother", 0, mother_x)]
                if person["sex"] == FEMALE:
                    if "father" in living_parents:
                        (allele,) = parent_copies("father")
                        person["draws"].append((locus, 1, "copy", None, allele))
                        copies.append(allele)
                    else:
                        copies.append(gamete("father", 1, None))
            else:
                assert locus == MT_LOCUS
                if "mother" in living_parents:
                    (allele,) = parent_copies("mother")
                    person["draws"].append((locus, 0, "copy", None, allele))
                    copies = [allele]
                else:
                    copies = [gamete("mother", 0, None)]
            assert copies_of(person["genome"], loci[locus]) == [0] * len(copies), "the pool run left the reference allele"
            write_copies(person["genome"], loci[locus], copies)
            assert copies_of(person["genome"], loci[locus]) == copies
            person["alleles"][locus] = copies
        if person["sim"] is not None:
            by_sim[person["sim"]] = person
    return persons


def person_copies(person: dict, locus: dict) -> list:
    return copies_of(person["genome"], locus)


# ---------------------------------------------------------------------------------------
# 4. The afflictions


def afflictions_after(world, frozen_data, person: dict) -> list:
    """The afflictions of a pool person after section 3 (note section 4), as at its
    creation: `World.afflictions_of` on the frozen map's data with the pool seed; a founder
    of the run at the age `0 − born` without parents' keys, a child at the age 0 with its
    parents' keys. Returns [(disease key, onset)] in data order."""
    stub = types.SimpleNamespace(data=frozen_data, seed=POOL_SEED, switch=False)
    if person["mother"] is None:
        parents, age = None, 0 - person["born"]
    else:
        parents, age = (world.key_of(person["mother"]), world.key_of(person["father"])), 0
    found = world_reference.World.afflictions_of(stub, person["key"], person["sex"], person["genome"], None, parents, age)
    return [(affliction.disease.key, affliction.onset) for affliction in found]


def check_threshold(world, frozen_data, person: dict, afflictions: list) -> None:
    """The threshold disease recomputed must be the pool run's (note section 4)."""
    if person["sim"] is None:
        return
    (disease,) = [d for d in frozen_data.diseases if d.liability is not None]
    had = [(a.disease.key, a.onset) for a in world.sims[person["sim"]][4] if a.disease.code == disease.code]
    now = [(key, onset) for key, onset in afflictions if key == disease.key]
    assert [(k, bits(o)) for k, o in had] == [(k, bits(o)) for k, o in now], ("the threshold disease differs", person["key"])


# ---------------------------------------------------------------------------------------
# 5. and 6. The pool's entries


def genome_raw(genome: dict) -> tuple:
    """The genome as the file holds it: (maternal autosome words, paternal autosome words,
    the maternal X, (0, the paternal X) or (1, the Y), the mt), from the bytes of
    `postcard_reference.genome_of`."""
    c = Cursor(genome_of(genome))
    raw = decode_genome(c)
    assert c.left() == 0
    return raw


def pending(world, kind: int, subject: tuple):
    times = [event.time for event in world.queue if event.kind == kind and event.subject == subject]
    assert len(times) <= 1, (kind, subject, times)
    return times[0] if times else None


def sex_code(sex: bytes) -> int:
    return 0 if sex == FEMALE else 1


def person_entry(world, person: dict, afflictions: list, frozen_at: int) -> dict:
    """A pool person, living (note section 5), in the order of its fields."""
    sim = person["sim"]
    value = world.sims[sim]
    born, key, sex = value[:3]
    conceptions, births, parents, partnerships, _, moved_out, carer, social = value[5:13]
    woman = sex == FEMALE
    open_ = world.open_partnership(sim)
    separation = None
    if woman and open_ is not None:
        separation = pending(world, SEPARATION_KIND, (PARTNERSHIP_SUBJECT, open_))
    conception_at = pending(world, ATTEMPT_KIND, (SIM_SUBJECT, sim))
    birth_at = pending(world, BIRTH_KIND, (SIM_SUBJECT, sim))
    assert woman or (conception_at is None and birth_at is None)
    leave_home_at = pending(world, LEAVE_HOME_KIND, (SIM_SUBJECT, sim))
    for at in (conception_at, separation, leave_home_at, birth_at):
        assert at is None or at > frozen_at
    return {
        "lineage_key": key,
        "genome_key": person["genome_key"],
        "sex": sex_code(sex),
        "genome": genome_raw(person["genome"]),
        "born": born,
        "mother": None if parents is None else world.key_of(parents[0]),
        "father": None if parents is None else world.key_of(parents[1]),
        "conceptions": conceptions,
        "births": births,
        "partnerships": partnerships,
        "moved_out": moved_out,
        "carer": None if carer is None else world.key_of(carer),
        "social_parents": [world.key_of(other) for other in social],
        "afflictions": afflictions,
        "conception_at": conception_at,
        "separation_at": separation,
        "leave_home_at": leave_home_at,
        "birth_at": birth_at,
    }


def household_entry(world, household: int, by_key: dict, afflictions: dict, frozen_at: int) -> dict:
    """A pool household (note section 5): its members by (born, lineage key), its open
    partnership (woman, man, formed), the running pregnancies of its members in the order of
    the members (decision 6 of NOTES.md), each with its unborn children in multiple order."""
    members = sorted(world.members(household), key=lambda sim: (world.sims[sim][0], world.sims[sim][1]))
    entries = [person_entry(world, by_key[world.sims[sim][1]], afflictions[world.sims[sim][1]], frozen_at) for sim in members]
    partnership = None
    opens = sorted({world.open_partnership(sim) for sim in members} - {None})
    assert len(opens) <= 1, opens
    if opens:
        woman, man, formed, ended = world.partnerships[opens[0]]
        assert ended is None and woman in members and man in members, "both partners are members (ADR-0018)"
        partnership = (world.key_of(woman), world.key_of(man), formed)
    pregnancies = []
    for sim in members:
        if sim not in world.pregnancies:
            continue
        father, conception_number, children, conceived = world.pregnancies[sim]
        unborn = []
        for key, sex, _ in children:
            person = by_key[key]
            unborn.append(
                {
                    "lineage_key": key,
                    "genome_key": person["genome_key"],
                    "sex": sex_code(sex),
                    "genome": genome_raw(person["genome"]),
                    "afflictions": afflictions[key],
                }
            )
        pregnancies.append(
            {
                "mother": world.key_of(sim),
                "father": world.key_of(father),
                "conception": conception_number,
                "conceived": conceived,
                "children": unborn,
            }
        )
    return {"members": entries, "partnership": partnership, "pregnancies": pregnancies}


def ancestor_entries(world, pool_sims: list, unborn_parents: list) -> tuple:
    """The records (G3, note section 6): every pool person living and its ancestors up to
    depth 3; for every unborn pool person its parents (the mother and the pregnancy's
    father) and their ancestors up to depth 2 (its own depth 3). Each once, ascending by
    lineage key, each with its birth second in the pool run and each parent's key only if
    that parent is itself a record. Returns (records, the SimIds per depth of the deepest
    ancestor found, for the cases)."""
    pedigree = world.pedigree
    records = set()
    for sim in pool_sims:
        records |= {sim} | pedigree.ancestors(sim, DEPTH)
    for mother, father in unborn_parents:
        records |= {mother, father} | pedigree.ancestors(mother, DEPTH - 1) | pedigree.ancestors(father, DEPTH - 1)
    out = []
    for sim in records:
        mother, father = pedigree.parents[sim]
        out.append(
            {
                "lineage_key": world.key_of(sim),
                "born": world.born_of(sim),
                "mother": world.key_of(mother) if mother is not None and mother in records else None,
                "father": world.key_of(father) if father is not None and father in records else None,
                "sim": sim,
            }
        )
    out.sort(key=lambda record: record["lineage_key"])
    return out


def depth_of(world, pool_sim: int) -> int:
    """How many generations of ancestors a living pool person has in the pool run (up to 3):
    the depth of its deepest ancestor at most 3 back."""
    frontier, depth = {pool_sim}, 0
    while depth < DEPTH:
        frontier = {parent for person in frontier for parent in world.pedigree.parents[person] if parent is not None}
        if not frontier:
            break
        depth += 1
    return depth


# ---------------------------------------------------------------------------------------
# 7. The file


def encode_genome(raw: tuple) -> bytes:
    maternal, paternal, x_maternal, (kind, second), mt = raw
    out = b"".join(varint(word) for word in maternal) + b"".join(varint(word) for word in paternal) + varint(x_maternal)
    out += varint(kind) + (varint(second) if kind == 0 else bytes([second]))
    return out + bytes([mt])


def decode_genome(c: Cursor) -> tuple:
    maternal = tuple(c.varint(64) for _ in range(3))
    paternal = tuple(c.varint(64) for _ in range(3))
    x_maternal = c.varint(16)
    kind = c.varint(32)
    if kind == 0:
        second = c.varint(16)
    elif kind == 1:
        second = c.byte()
    else:
        raise DecodeError("decode: paternal sex chromosome")
    return maternal, paternal, x_maternal, (kind, second), c.byte()


def option(value, encode) -> bytes:
    return b"\x00" if value is None else b"\x01" + encode(value)


def encode_afflictions(items: list) -> bytes:
    """Vec<PoolAffliction>, PoolAffliction { disease: u64 (str_key of the code), onset:
    FiniteF64 (years) }."""
    return seq([varint(key) + f64(onset) for key, onset in items])


def encode_person(p: dict) -> bytes:
    return (
        varint(p["lineage_key"])
        + varint(p["genome_key"])
        + varint(p["sex"])
        + encode_genome(p["genome"])
        + signed(p["born"])
        + option(p["mother"], varint)
        + option(p["father"], varint)
        + varint(p["conceptions"])
        + varint(p["births"])
        + varint(p["partnerships"])
        + flag(p["moved_out"])
        + option(p["carer"], varint)
        + seq([varint(key) for key in p["social_parents"]])
        + encode_afflictions(p["afflictions"])
        + option(p["conception_at"], signed)
        + option(p["separation_at"], signed)
        + option(p["leave_home_at"], signed)
        + option(p["birth_at"], signed)
    )


def encode_unborn(u: dict) -> bytes:
    return varint(u["lineage_key"]) + varint(u["genome_key"]) + varint(u["sex"]) + encode_genome(u["genome"]) + encode_afflictions(u["afflictions"])


def encode_pregnancy(p: dict) -> bytes:
    return varint(p["mother"]) + varint(p["father"]) + varint(p["conception"]) + signed(p["conceived"]) + seq([encode_unborn(u) for u in p["children"]])


def encode_household(h: dict) -> bytes:
    partnership = option(h["partnership"], lambda value: varint(value[0]) + varint(value[1]) + signed(value[2]))
    return seq([encode_person(p) for p in h["members"]]) + partnership + seq([encode_pregnancy(p) for p in h["pregnancies"]])


def encode_ancestor(a: dict) -> bytes:
    return varint(a["lineage_key"]) + signed(a["born"]) + option(a["mother"], varint) + option(a["father"], varint)


def encode_pool(pool: dict) -> bytes:
    """The file: `b"SIMPOOL"`, `varint(1)`, then postcard of

        Pool { pool_seed: u64, frozen_at: SimTime, data_hash: u64,
               households: Vec<PoolHousehold>, ancestors: Vec<Ancestor> }
        PoolHousehold { members: Vec<PoolPerson>, partnership: Option<PoolPartnership>,
                        pregnancies: Vec<PoolPregnancy> }
        PoolPerson { lineage_key: LineageKey, genome_key: GenomeKey, sex: Sex,
                     genome: Genome, born: SimTime, mother: Option<LineageKey>,
                     father: Option<LineageKey>, conceptions: u32, births: u32,
                     partnerships: u32, moved_out: bool, carer: Option<LineageKey>,
                     social_parents: Vec<LineageKey>, afflictions: Vec<PoolAffliction>,
                     conception_at: Option<SimTime>, separation_at: Option<SimTime>,
                     leave_home_at: Option<SimTime>, birth_at: Option<SimTime> }
        PoolAffliction { disease: u64, onset: FiniteF64 }
        PoolPartnership { woman: LineageKey, man: LineageKey, formed: SimTime }
        PoolPregnancy { mother: LineageKey, father: LineageKey, conception: u32,
                        conceived: SimTime, children: Vec<PoolUnborn> }
        PoolUnborn { lineage_key: LineageKey, genome_key: GenomeKey, sex: Sex,
                     genome: Genome, afflictions: Vec<PoolAffliction> }
        Ancestor { lineage_key: LineageKey, born: SimTime, mother: Option<LineageKey>,
                   father: Option<LineageKey> }

    with u64 and u32 as varints, SimTime zigzag, the keys plain u64 varints, Sex and the
    paternal sex chromosome as variant indexes, Genome as in the projection
    (`postcard_reference.genome_of`), f64 eight bytes little endian, Option a tag byte."""
    body = (
        varint(pool["pool_seed"])
        + signed(pool["frozen_at"])
        + varint(pool["data_hash"])
        + seq([encode_household(h) for h in pool["households"]])
        + seq([encode_ancestor(a) for a in pool["ancestors"]])
    )
    return POOL_MAGIC + varint(POOL_FORMAT_VERSION) + body


def decode_person(c: Cursor) -> dict:
    return {
        "lineage_key": c.varint(),
        "genome_key": c.varint(),
        "sex": decode_sex(c),
        "genome": decode_genome(c),
        "born": c.signed(),
        "mother": c.option(c.varint),
        "father": c.option(c.varint),
        "conceptions": c.varint(32),
        "births": c.varint(32),
        "partnerships": c.varint(32),
        "moved_out": c.boolean(),
        "carer": c.option(c.varint),
        "social_parents": c.vec(c.varint),
        "afflictions": c.vec(lambda: (c.varint(), c.f64())),
        "conception_at": c.option(c.signed),
        "separation_at": c.option(c.signed),
        "leave_home_at": c.option(c.signed),
        "birth_at": c.option(c.signed),
    }


def decode_sex(c: Cursor) -> int:
    value = c.varint(32)
    if value > 1:
        raise DecodeError("decode: sex")
    return value


def decode_unborn(c: Cursor) -> dict:
    return {
        "lineage_key": c.varint(),
        "genome_key": c.varint(),
        "sex": decode_sex(c),
        "genome": decode_genome(c),
        "afflictions": c.vec(lambda: (c.varint(), c.f64())),
    }


def decode_pregnancy(c: Cursor) -> dict:
    return {
        "mother": c.varint(),
        "father": c.varint(),
        "conception": c.varint(32),
        "conceived": c.signed(),
        "children": c.vec(lambda: decode_unborn(c)),
    }


def decode_household(c: Cursor) -> dict:
    return {
        "members": c.vec(lambda: decode_person(c)),
        "partnership": c.option(lambda: (c.varint(), c.varint(), c.signed())),
        "pregnancies": c.vec(lambda: decode_pregnancy(c)),
    }


def decode_ancestor(c: Cursor) -> dict:
    return {"lineage_key": c.varint(), "born": c.signed(), "mother": c.option(c.varint), "father": c.option(c.varint)}


class Rejected(Exception):
    """A file the reader rejects; `rule` names the rule."""

    def __init__(self, rule: str, detail: str = ""):
        super().__init__(f"{rule} {detail}".strip())
        self.rule = rule


# The rules of the reader in the order it checks them (note section 7), each with the name
# the known answers use.
RULES = (
    "magic",
    "version",
    "trailing_bytes",
    "household_without_adult",
    "members_out_of_order",
    "ancestors_out_of_order",
    "parent_not_a_record",
    "partnership_not_members",
)


def decode_pool(data: bytes) -> dict:
    """`Pool::from_bytes` without the checks of the content: the magic, the version, the
    postcard body (a decode error is `decode`, not a rule of the note), no trailing bytes."""
    if data[: len(POOL_MAGIC)] != POOL_MAGIC:
        raise Rejected("magic")
    c = Cursor(data, len(POOL_MAGIC))
    try:
        version = c.varint(32)
    except DecodeError as error:
        raise Rejected("decode", str(error))
    if version != POOL_FORMAT_VERSION:
        raise Rejected("version", str(version))
    try:
        pool = {
            "pool_seed": c.varint(),
            "frozen_at": c.signed(),
            "data_hash": c.varint(),
            "households": c.vec(lambda: decode_household(c)),
            "ancestors": c.vec(lambda: decode_ancestor(c)),
        }
    except DecodeError as error:
        raise Rejected("decode", str(error))
    if c.left():
        raise Rejected("trailing_bytes", f"{c.left()} bytes")
    return pool


def violations(pool: dict) -> list:
    """Every rule of the content the pool breaks, in the order of `RULES`, each once."""
    found = []
    at = pool["frozen_at"]
    if any(not any(at - p["born"] >= ADULT for p in h["members"]) for h in pool["households"]):
        found.append("household_without_adult")
    for h in pool["households"]:
        order = [(p["born"], p["lineage_key"]) for p in h["members"]]
        if any(order[i] >= order[i + 1] for i in range(len(order) - 1)):
            found.append("members_out_of_order")
            break
    keys = [a["lineage_key"] for a in pool["ancestors"]]
    if any(keys[i] >= keys[i + 1] for i in range(len(keys) - 1)):
        found.append("ancestors_out_of_order")
    records = set(keys)
    parents = [key for h in pool["households"] for p in h["members"] for key in (p["mother"], p["father"]) if key is not None]
    parents += [key for a in pool["ancestors"] for key in (a["mother"], a["father"]) if key is not None]
    parents += [key for h in pool["households"] for g in h["pregnancies"] for key in (g["mother"], g["father"])]
    if any(key not in records for key in parents):
        found.append("parent_not_a_record")
    for h in pool["households"]:
        members = {p["lineage_key"] for p in h["members"]}
        if h["partnership"] is not None and not {h["partnership"][0], h["partnership"][1]} <= members:
            found.append("partnership_not_members")
            break
    return found


def read_pool(data: bytes) -> dict:
    """`Pool::from_bytes`: decodes and checks; raises `Rejected` with the first rule."""
    pool = decode_pool(data)
    found = violations(pool)
    if found:
        raise Rejected(found[0])
    return pool


# ---------------------------------------------------------------------------------------
# The whole pool of a run


def build_pool(world, run_data, frozen_data, frozen_genome_map: dict, q: dict, genome_keys: dict) -> dict:
    """Sections 2 to 7 on a world at `E_p`: returns a dict with the persons (in the order of
    section 3), the pool households, the pool persons, the afflictions, the file's structure
    and its bytes."""
    frozen_at = world.closed
    persons = assigned_persons(world, genome_keys)
    loci = positions(frozen_genome_map)
    assign(world, persons, loci, q)
    by_key = {person["key"]: person for person in persons}
    households = pool_households(world, frozen_at)
    pool_sims = [sim for household in households for sim in world.members(household)]
    unborn = [
        child[0]
        for household in households
        for sim in world.members(household)
        if sim in world.pregnancies
        for child in world.pregnancies[sim][2]
    ]
    pool_keys = [world.key_of(sim) for sim in pool_sims] + unborn
    afflictions = {}
    for key in pool_keys:
        afflictions[key] = afflictions_after(world, frozen_data, by_key[key])
        check_threshold(world, frozen_data, by_key[key], afflictions[key])
    entries = [household_entry(world, household, by_key, afflictions, frozen_at) for household in households]
    unborn_parents = [
        (sim, world.pregnancies[sim][0]) for household in households for sim in world.members(household) if sim in world.pregnancies
    ]
    ancestors = ancestor_entries(world, pool_sims, unborn_parents)
    pool = {
        "pool_seed": POOL_SEED,
        "frozen_at": frozen_at,
        "data_hash": run_data.hash,
        "households": entries,
        "ancestors": [{k: v for k, v in record.items() if k != "sim"} for record in ancestors],
    }
    data = encode_pool(pool)
    assert read_pool(data) == pool_view(pool)
    assert encode_pool(read_pool(data)) == data, "the reader round-trips"
    return {
        "persons": persons,
        "households": households,
        "pool_sims": pool_sims,
        "pool_keys": pool_keys,
        "unborn_parents": unborn_parents,
        "afflictions": afflictions,
        "ancestors": ancestors,
        "pool": pool,
        "bytes": data,
    }


def pool_view(pool: dict) -> dict:
    """The pool as the reader returns it (tuples for the genome and the afflictions)."""
    out = copy.deepcopy(pool)
    for h in out["households"]:
        for p in h["members"]:
            p["afflictions"] = [tuple(a) for a in p["afflictions"]]
        for g in h["pregnancies"]:
            for u in g["children"]:
                u["afflictions"] = [tuple(a) for a in u["afflictions"]]
        if h["partnership"] is not None:
            h["partnership"] = tuple(h["partnership"])
    return out


# ---------------------------------------------------------------------------------------
# The cases of note section 8


def cases(world, built: dict) -> dict:
    """Which cases of note section 8 the pool of a run has, each with its witnesses as
    strings (lineage keys in hex): `dependent_child` (a pool member under 18, not moved
    out), `open_partnership` (the woman), `running_pregnancy` (the mother),
    `unborn_father_died` (`mother/father` of a running pregnancy whose father died at or
    after the conception), `unborn_father_not_living` (died or emigrated),
    `born_father_died_in_pregnancy` (a living pool member whose father died between its
    conception and its birth), `identical_pair` (assigned persons sharing a genome key,
    `key+key`), `identical_pair_in_pool`, `identical_pair_unborn`, `depth_3` (a living pool
    member with an ancestor three generations back), `frequency_alternative` (a pool
    person's gamete from a missing parent that came out 1, `key:locus:gamete`),
    `one_parent_living` (a pool person with exactly one living parent),
    `household_not_in_pool` (an open household without a member of 18 or more,
    `id:members`), `parent_emigrated` (an assigned person with an emigrated parent,
    `key:side`)."""
    frozen_at = world.closed
    out = {}
    pool_sims = built["pool_sims"]
    hexes = lambda keys: [f"{key:016x}" for key in keys]
    out["dependent_child"] = hexes(world.key_of(sim) for sim in pool_sims if world.dependent(sim, frozen_at))
    out["open_partnership"] = hexes(e["partnership"][0] for e in built["pool"]["households"] if e["partnership"] is not None)
    out["running_pregnancy"] = hexes(g["mother"] for e in built["pool"]["households"] for g in e["pregnancies"])
    dead = {sim for sim, value in world.archive.items() if value[2] is not None}
    out["unborn_father_died"] = [
        f"{world.key_of(mother):016x}/{world.key_of(father):016x}"
        for mother, father in built["unborn_parents"]
        if father in dead and world.archive[father][1] >= world.pregnancies[mother][3]
    ]
    out["unborn_father_not_living"] = [
        f"{world.key_of(mother):016x}/{world.key_of(father):016x}" for mother, father in built["unborn_parents"] if father not in world.sims
    ]
    born_father = []
    for sim in pool_sims:
        parents = world.sims[sim][7]
        if parents is None or parents[1] not in dead:
            continue
        died = world.archive[parents[1]][1]
        # Conceived `pregnancy_months` months before the birth.
        if world.sims[sim][0] - world.data.pregnancy_months * world_reference.MONTH <= died < world.sims[sim][0]:
            born_father.append(world.key_of(sim))
    out["born_father_died_in_pregnancy"] = hexes(born_father)
    pool_keys = set(built["pool_keys"])
    shared = {}
    for person in built["persons"]:
        shared.setdefault(person["genome_key"], []).append(person)
    groups = [group for group in shared.values() if len(group) > 1]
    out["identical_pair"] = ["+".join(f"{p['key']:016x}" for p in group) for group in groups]
    in_pool = [group for group in groups if all(p["key"] in pool_keys for p in group)]
    out["identical_pair_in_pool"] = ["+".join(f"{p['key']:016x}" for p in group) for group in in_pool]
    out["identical_pair_unborn"] = ["+".join(f"{p['key']:016x}" for p in group) for group in in_pool if all(p["unborn"] for p in group)]
    out["depth_3"] = hexes(world.key_of(sim) for sim in pool_sims if depth_of(world, sim) == 3)
    out["frequency_alternative"] = [
        f"{person['key']:016x}:{locus}:{gamete}"
        for person in built["persons"]
        if person["key"] in pool_keys
        for locus, gamete, source, u, allele in person["draws"]
        if source == "frequency" and allele == 1
    ]
    out["one_parent_living"] = hexes(
        person["key"] for person in built["persons"] if person["key"] in pool_keys and len(person["living_parents"]) == 1
    )
    # A household that has not ended but has no member of 18 or more: not in the pool, its
    # members are assigned but no pool persons (note section 2).
    out["household_not_in_pool"] = [
        f"{household}:" + "+".join(f"{world.key_of(sim):016x}" for sim in world.members(household))
        for household in sorted(world.households)
        if world.households[household][1] is None and household not in built["households"]
    ]
    # An assigned person with an emigrated parent (missing like a dead one, D9 of NOTES.md).
    out["parent_emigrated"] = [
        f"{person['key']:016x}:{side}"
        for person in built["persons"]
        for side in ("mother", "father")
        if person[side] is not None and person[side] in world.archive and world.archive[person[side]][2] is None
    ]
    return out


# ---------------------------------------------------------------------------------------
# The known answers


def print_frequencies(frozen: tuple) -> None:
    _, calibration, genome_map = frozen
    q = check_pool_frequencies(calibration, genome_map)
    print(f"pool_tag POOL_ALLELE {POOL_ALLELE} line_for_rng_tags tag {POOL_ALLELE} POOL_ALLELE")
    print("pool_frequencies " + " ".join(f"{locus}={q[locus]!r}" for locus in sorted(q)))
    for locus, value in POOL_FREQUENCIES:
        print(f"pool_frequency {locus} q {value!r} bits {bits(value)} str_key {str_key(locus):016x}")
    print("pool_frequencies_json_begin")
    print(pool_frequencies_json())
    print("pool_frequencies_json_end")
    print(f"pool_frequencies_bytes_if_hashed {pool_frequencies_bytes().hex()}")
    # The data check's rejections on altered entries (each one change).
    base = pool_frequencies_entry()
    bad = []
    missing = copy.deepcopy(base)
    del missing["core:herc2"]
    bad.append(("missing_locus", missing))
    extra = copy.deepcopy(base)
    extra["core:growth_01"] = copy.deepcopy(base["core:mc1r"])
    bad.append(("extra_locus", extra))
    for name, value in (("zero", 0.0), ("one", 1.0), ("negative", -0.1), ("above_one", 1.5)):
        changed = copy.deepcopy(base)
        changed["core:mc1r"]["value"] = value
        bad.append((f"q_{name}", changed))
    unit_ = copy.deepcopy(base)
    unit_["core:mc1r"]["unit"] = "ratio"
    bad.append(("unit_ratio", unit_))
    for name, entry in bad:
        doc = copy.deepcopy(calibration)
        doc["pool_frequencies"] = entry
        try:
            check_pool_frequencies(doc, genome_map)
            raise AssertionError(f"the data check accepted {name}")
        except AssertionError as error:
            rule = str(error).split()[0]
            assert rule.startswith("pool."), error
            print(f"pool_frequencies_rejected {name} rule {rule}")
    print("pool_frequencies_accepted the seven loci of the note, unit fraction, 0 < q < 1")


def print_run(label: str, frozen: tuple, n: int, years: int, out_dir) -> tuple:
    """One pool run and its pool, under the labels `<label>_...`."""
    run_data, frozen_data, q = pool_data(frozen)
    started = time.time()
    world, founder_list = pool_run(run_data, n, years)
    elapsed = time.time() - started
    frozen_at = world.closed
    log_conceptions, genome_keys = genome_keys_from_log(world.log)
    check_genome_keys(world, log_conceptions)
    built = build_pool(world, run_data, frozen_data, frozen[2], q, genome_keys)
    persons = built["persons"]
    pool_keys = set(built["pool_keys"])
    print(
        f"{label}_config founders {n} years {years} seed {POOL_SEED:#x} tag {POOL_TAG} capacity {n} frozen_at {frozen_at}"
        f" run_data_hash {run_data.hash:016x} frozen_data_hash {frozen_data.hash:016x} pool_allele_tag {POOL_ALLELE}"
    )
    print(
        f"{label}_story living {len(world.sims)} archived {len(world.archive)} log_entries {len(world.log)}"
        f" log_hash {world.log_hash():016x} state_hash {world.state_hash():016x} conceptions {len(log_conceptions)}"
        f" pregnancies {len(world.pregnancies)} households_open {sum(1 for v in world.households.values() if v[1] is None)}"
        f" pool_households {len(built['households'])} pool_persons {len(built['pool_keys'])} assigned {len(persons)}"
        f" records {len(built['ancestors'])} most_pregnancies_in_a_household"
        f" {max((len(e['pregnancies']) for e in built['pool']['households']), default=0)}"
    )
    # Section 1: the genome key of every conceived child, in log order.
    for index, (t, mother, father, key, genome_key) in enumerate(log_conceptions):
        print(
            f"{label}_genome_key {index} child {key:016x} genome_key {genome_key:016x} conceived {t}"
            f" mother {world.key_of(mother):016x} father {world.key_of(father):016x} {'own' if key == genome_key else 'identical_of_first'}"
        )
    # Section 3: the order and the alleles.
    for index, person in enumerate(persons):
        when = f"due {person['due']}" if person["unborn"] else f"born {person['born']}"
        parents = " ".join(
            f"{side} {'-' if person[side] is None else format(world.key_of(person[side]), '016x')}"
            f" {'living' if side in person['living_parents'] else 'missing'}"
            for side in ("mother", "father")
        )
        print(
            f"{label}_order {index} {'unborn' if person['unborn'] else 'living'} key {person['key']:016x}"
            f" genome_key {person['genome_key']:016x} {when} sex {'female' if person['sex'] == FEMALE else 'male'}"
            f" {parents} in_pool {person['key'] in pool_keys}"
        )
    for person in persons:
        print(
            f"{label}_alleles {person['key']:016x} "
            + " ".join(f"{locus}={''.join(str(a) for a in person['alleles'][locus])}" for locus, _ in POOL_FREQUENCIES)
        )
    for person in persons:
        for locus, gamete, source, u, allele in person["draws"]:
            drawn = "none" if u is None else f"{u!r} bits {bits(u)}"
            print(f"{label}_draw {person['key']:016x} {locus} gamete {gamete} source {source} u {drawn} allele {allele}")
    # The report of section 3 (over the pool persons).
    report = []
    for locus, _ in POOL_FREQUENCIES:
        count = sum(1 for person in persons if person["key"] in pool_keys and 1 in person["alleles"][locus])
        report.append(f"{locus}={count}")
    print(f"{label}_report persons_with_alternative " + " ".join(report))
    if not any(1 in person["alleles"]["core:disease_ad_late"] for person in persons if person["key"] in pool_keys):
        print(f"{label}_report core:disease_ad_late none in the pool (reported, not redrawn)")
    # Section 4: the afflictions.
    codes = {disease.key: disease.code for disease in frozen_data.diseases}
    for key in built["pool_keys"]:
        items = built["afflictions"][key]
        text = " ".join(f"{codes[k]} onset {o!r} bits {bits(o)}" for k, o in items) or "-"
        print(f"{label}_afflictions {key:016x} count {len(items)} {text}")
    # Sections 5 and 6: the structure in words.
    for number, (household, entry) in enumerate(zip(built["households"], built["pool"]["households"])):
        members = " ".join(f"{p['lineage_key']:016x}" for p in entry["members"])
        partnership = "-" if entry["partnership"] is None else f"{entry['partnership'][0]:016x} {entry['partnership'][1]:016x} {entry['partnership'][2]}"
        pregnancies = " ".join(f"{g['mother']:016x}:{len(g['children'])}" for g in entry["pregnancies"]) or "-"
        print(f"{label}_household {number} id {household} members {members} partnership {partnership} pregnancies {pregnancies}")
        for p in entry["members"]:
            opt = lambda value: "-" if value is None else str(value)
            key = lambda value: "-" if value is None else f"{value:016x}"
            print(
                f"{label}_person {p['lineage_key']:016x} genome_key {p['genome_key']:016x} sex {p['sex']} born {p['born']}"
                f" mother {key(p['mother'])} father {key(p['father'])} conceptions {p['conceptions']} births {p['births']}"
                f" partnerships {p['partnerships']} moved_out {p['moved_out']} carer {key(p['carer'])}"
                f" social_parents {','.join(f'{s:016x}' for s in p['social_parents']) or '-'} afflictions {len(p['afflictions'])}"
                f" conception_at {opt(p['conception_at'])} separation_at {opt(p['separation_at'])}"
                f" leave_home_at {opt(p['leave_home_at'])} birth_at {opt(p['birth_at'])}"
            )
        for g in entry["pregnancies"]:
            children = " ".join("%016x/%016x/%d" % (u["lineage_key"], u["genome_key"], u["sex"]) for u in g["children"])
            print(
                f"{label}_pregnancy mother {g['mother']:016x} father {g['father']:016x} conception {g['conception']} conceived {g['conceived']}"
                f" children {children}"
            )
    for record in built["ancestors"]:
        key = lambda value: "-" if value is None else f"{value:016x}"
        print(f"{label}_ancestor {record['lineage_key']:016x} born {record['born']} mother {key(record['mother'])} father {key(record['father'])}")
    # The cases of note section 8.
    found = cases(world, built)
    for name, witnesses in found.items():
        print(f"{label}_case {name} count {len(witnesses)} witnesses {','.join(witnesses) or '-'}")
    # Section 7: the file.
    data = built["bytes"]
    print(f"{label}_file_bytes {len(data)}")
    print(f"{label}_file_hex {data.hex()}")
    print(f"{label}_pool_hash {hash64(data):016x}")
    print(f"{label}_round_trip read and written again: identical")
    if out_dir is not None:
        path = Path(out_dir) / label
        path.mkdir(parents=True, exist_ok=True)
        (path / "pool.bin").write_bytes(data)
    print(f"{label}_seconds {elapsed:.1f} (the run alone, this machine; not a pin)", file=sys.stderr)
    return world, built


def rejected_files(data: bytes) -> list:
    """One rejected file per rule of the reader (note section 7), each from the valid file
    `data` with one change that breaks that rule alone (asserted: every other rule holds).
    Returns [(rule, how, bytes)]."""
    pool = read_pool(data)
    out = [("magic", "the first byte of the magic changed (b'XIMPOOL')", b"X" + data[1:])]
    out.append(("version", "format version 2", POOL_MAGIC + varint(2) + data[len(POOL_MAGIC) + 1 :]))
    out.append(("trailing_bytes", "one byte 0x00 appended", data + b"\x00"))
    at = pool["frozen_at"]
    # A household without an adult: the first household with no partnership and no
    # pregnancy, every member shifted by the same seconds so that its oldest is 17 at
    # `frozen_at` (the order stays).
    changed = copy.deepcopy(pool)
    plain = [h for h in changed["households"] if h["partnership"] is None and not h["pregnancies"]]
    h = ([h for h in plain if len(h["members"]) == 1] + plain)[0]
    oldest = min(p["born"] for p in h["members"])
    shift = (at - 17 * YEAR) - oldest
    assert shift > 0
    for p in h["members"]:
        p["born"] += shift
    out.append(("household_without_adult", f"a household without partnership and pregnancy (one member if any): its members born {shift} s later, the oldest 17", encode_pool(changed)))
    # Members out of order: the first two members of the first household with two swapped.
    changed = copy.deepcopy(pool)
    h = [h for h in changed["households"] if len(h["members"]) >= 2][0]
    h["members"][0], h["members"][1] = h["members"][1], h["members"][0]
    out.append(("members_out_of_order", "the first two members of the first household with two swapped", encode_pool(changed)))
    # Ancestors out of order: the first two records swapped.
    changed = copy.deepcopy(pool)
    changed["ancestors"][0], changed["ancestors"][1] = changed["ancestors"][1], changed["ancestors"][0]
    out.append(("ancestors_out_of_order", "the first two records swapped", encode_pool(changed)))
    # A parent key that is no record: the mother of the first member with a mother becomes
    # her key with the lowest bit flipped (no record).
    changed = copy.deepcopy(pool)
    records = {a["lineage_key"] for a in changed["ancestors"]}
    p = [p for h in changed["households"] for p in h["members"] if p["mother"] is not None][0]
    p["mother"] ^= 1
    assert p["mother"] not in records
    out.append(("parent_not_a_record", "the first member with a mother: her key with bit 0 flipped", encode_pool(changed)))
    # An open partnership whose partners are not members: the first household with one
    # gets the man of another household's member (its first member's key) as the man.
    changed = copy.deepcopy(pool)
    index = [i for i, h in enumerate(changed["households"]) if h["partnership"] is not None][0]
    other = [h for i, h in enumerate(changed["households"]) if i != index][0]
    woman, _, formed = changed["households"][index]["partnership"]
    changed["households"][index]["partnership"] = (woman, other["members"][0]["lineage_key"], formed)
    out.append(("partnership_not_members", "the first partnership's man is the first member of another household", encode_pool(changed)))
    for rule, _, content in out:
        try:
            decoded = decode_pool(content)
        except Rejected as error:
            assert error.rule == rule, (rule, error.rule)
            continue
        assert violations(decoded) == [rule], (rule, violations(decoded))
        try:
            read_pool(content)
            raise AssertionError(f"accepted {rule}")
        except Rejected as error:
            assert error.rule == rule
    assert [rule for rule, _, _ in out] == list(RULES)
    return out


def scan(n: int, years: int, scenario) -> None:
    """The cases of note section 8 at every second `y · YEAR` of one run (not a pin)."""
    frozen = frozen_template_documents()
    if scenario is not None:
        frozen = scenario_documents(frozen, SCENARIOS[scenario])
    run_data, frozen_data, q = pool_data(frozen)
    print(f"scan founders {n} years {years} scenario {scenario} run_data_hash {run_data.hash:016x}")

    def observe(world, y):
        # Sections 2 to 7 read the world and write only copies of the genomes.
        _, keys = genome_keys_from_log(world.log)
        built = build_pool(world, run_data, frozen_data, frozen[2], q, keys)
        found = cases(world, built)
        print(
            f"scan y {y} living {len(world.sims)} pool_households {len(built['households'])} pool_persons {len(built['pool_keys'])} "
            + " ".join(f"{name}={len(witnesses)}" for name, witnesses in found.items()),
            flush=True,
        )

    pool_run(run_data, n, years, observe)


# Scenarios of the pool data for cases the natural runs lack (note section 8), by name: values
# of the frozen documents changed before the pre-run data are made from them.
SCENARIOS = {
    # A father who dies during the pregnancy, and identical twins in the womb: the threshold
    # disease afflicts 30 percent (`K` 0.3, a prevalence with a recorded probit of 0.7) and
    # kills at 1 per year from its onset (normal 25, sd 5, held to [15, 45]); women search
    # the market more often (`c` 0.2); one conception in five is identical twins. The rates
    # of death of the founders' weights stay, so do the founders.
    "father_dies": {
        "diseases": {"core:disease_liability": {"prevalence": 0.3, "per_year": 1.0}},
        "demography": {"core:market_search_height_female": 0.2, "core:twins_identical": 0.2},
    },
    # Alternative alleles common enough that the Mendelian diseases afflict pool persons
    # (penetrance and onset drawn after the assignment, note section 4): other frequencies
    # in the data check's range (0, 1); the pool run itself does not read them, so it is the
    # natural run.
    "frequent_alleles": {
        "pool_frequencies": {
            "core:disease_ar": 0.4,
            "core:disease_ad_late": 0.3,
            "core:disease_ad_partial": 0.35,
            "core:disease_xr": 0.45,
            "core:disease_mt": 0.25,
            "core:mc1r": 0.6,
            "core:herc2": 0.55,
        }
    },
}


def main(argv) -> int:
    if "--scan" in argv:
        at = argv.index("--scan")
        scenario = argv[at + 3] if len(argv) > at + 3 else None
        scan(int(argv[at + 1]), int(argv[at + 2]), scenario)
        return 0
    out = argv[argv.index("--out") + 1] if "--out" in argv else None
    frozen = frozen_template_documents()
    print_frequencies(frozen)
    # The frozen map of the known answers as files (the template with `pool_frequencies`):
    # the directory `prerun pool --map` reads.
    texts = {
        Path("calibration") / "genetics.json": frozen_calibration_text(frozen[1]),
        Path("genetics") / "genome_map.json": map4_reference.text(frozen[2]),
    }
    assert texts[Path("genetics") / "genome_map.json"] == (TEMPLATE / "genetics" / "genome_map.json").read_text()
    print(
        "pool_frozen_map (tests/golden/map4_template with pool_frequencies)"
        + "".join(f" {path.name} bytes {len(text.encode())} xxh3 {hash64(text.encode()):016x}" for path, text in texts.items())
    )
    if out is not None:
        for path, text in texts.items():
            (Path(out) / "frozen_map" / path).parent.mkdir(parents=True, exist_ok=True)
            (Path(out) / "frozen_map" / path).write_text(text)
        # Read back as a loader reads a data directory (the life course data from data/).
        assert data_reference.documents(Path(out) / "frozen_map") == frozen
    for name, scenario in SCENARIOS.items():
        # The scenario's frozen documents as files (all three: its life course data change).
        documents = scenario_documents(frozen, scenario)
        texts = {
            Path("calibration") / "demography.json": map4_reference.text(documents[0]),
            Path("calibration") / "genetics.json": frozen_calibration_text(documents[1]),
            Path("genetics") / "genome_map.json": map4_reference.text(documents[2]),
        }
        print(
            f"pool_scenario {name} changes {scenario}"
            + "".join(f" {path.name} bytes {len(text.encode())} xxh3 {hash64(text.encode()):016x}" for path, text in texts.items())
        )
        if out is not None:
            for path, text in texts.items():
                (Path(out) / f"frozen_map_{name}" / path).parent.mkdir(parents=True, exist_ok=True)
                (Path(out) / f"frozen_map_{name}" / path).write_text(text)
            assert data_reference.documents(Path(out) / f"frozen_map_{name}") == documents
    for label, n, years, scenario in RUNS:
        documents = frozen if scenario is None else scenario_documents(frozen, SCENARIOS[scenario])
        world, built = print_run(label, documents, n, years, out)
        if label == REJECT_RUN:
            for rule, how, content in rejected_files(built["bytes"]):
                print(f"pool_reject_how {rule} from {label}: {how}")
                print(f"pool_reject {rule} bytes {len(content)} hash {hash64(content):016x} hex {content.hex()}")
                if out is not None:
                    path = Path(out) / "reject"
                    path.mkdir(parents=True, exist_ok=True)
                    (path / f"{rule}.bin").write_bytes(content)
    return 0


# The runs of the known answers: (label, founders, years, scenario or None). `pool_a` and
# `pool_b` are the note's natural runs (section 8); `pool_c` is the scenario `father_dies`
# with 80 founders over 2 years, chosen by `--scan 80 2 father_dies` (a running pregnancy
# whose father died, identical twins unborn and living; NOTES.md); `pool_d` is `pool_a`'s
# run with the frequencies of `frequent_alleles` (Mendelian afflictions after the
# assignment); `pool_e` is the natural run of 60 founders to year 81, the first year end of
# `--scan 60 90` with an open household outside the pool (no member of 18 or more). The
# rejected files are made from `pool_b`'s file.
RUNS = [
    ("pool_a", 60, 60, None),
    ("pool_b", 40, 30, None),
    ("pool_c", 80, 2, "father_dies"),
    ("pool_d", 60, 60, "frequent_alleles"),
    ("pool_e", 60, 81, None),
    # The longest run. Meant to reach beyond the great-grandparents, it does not: its
    # records are the whole known ancestry of its persons, so the depth of the records
    # does not decide here (a test of the core holds it on a pedigree built by hand).
    ("pool_f", 60, 130, None),
]
REJECT_RUN = "pool_b"


if __name__ == "__main__":
    sys.exit(main(sys.argv))

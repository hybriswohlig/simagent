#!/usr/bin/env python3
"""Reference data hash of the definitions under data/ (ADR-0007), independent of the Rust code.

The core sees each file without its text fields (`source`, `as_of`, `note`, `description`) and
hashes the postcard bytes of the three definition sets in the string order of their data IDs:
`core:calibration_demography` (calibration/demography.json, ADR-0017 section 1), then
`core:calibration_genetics` (calibration/genetics.json), then `core:genome_map`
(genetics/genome_map.json), with xxh3-64 and seed 0 over the concatenation (ADR-0003
StableHasher, ADR-0004 section 2). A data directory without its own demography file (the
fixtures tests/golden/data_traits, tests/golden/data_body and tests/golden/data_pairing)
takes the repository's.

Layout of the core types (postcard: structs as their fields in order, enums as the variant
index as varint, strings and sequences with a varint length, maps as a varint length and the
pairs in key order, u8 and bool as one raw byte, u32 as varint, f64 as eight bytes little
endian; an enum variant with data is its index followed by the data; ADR-0008):

  DemographyCalibration { format_version: u32, values: BTreeMap<DataId, Calibrated>,
                          fertility_wish_shape: Vec<Calibrated>,   (ADR-0017 section 1)
                          leave_home_ages: Vec<Calibrated> }   (format 2, design note of
                          the households, section 1: the ten weights of leaving home at
                          16 to 25, unit fraction, each in [0, 1], at least one above 0)
  GeneticsCalibration { format_version: u32, values: BTreeMap<DataId, Calibrated>,
                        traits: Vec<PhenotypeWeights>,
                        sex_differences: Vec<SexDifference>,   (ADR-0014)
                        diseases: Vec<Disease>,   (ADR-0017)
                        longevity_trait: Option<DataId>,   (format 5, ADR-0017 section 1:
                        a trait of the map or none, 0 for none, 1 then the ID)
                        puberty_trait: Option<DataId>,   (format 6, the trait of the
                        puberty timing z_p, genetics specification 5.2: as longevity_trait)
                        cognition_trait: Option<DataId>,   (format 7, the traits of the
                        height_trait: Option<DataId>,       similarity terms of the partner
                        market, genetics specification 11.1, 11.2: as longevity_trait)
                        fertility: Fertility }   (format 6, ADR-0017 section 1)
  Fertility: puberty { menarche_mean: Calibrated, menarche_sd: Calibrated,
                       menarche_low: Calibrated, menarche_high: Calibrated,
                       female_ramp: Calibrated, female_decline_from: Calibrated,
                       female_decline_to: Calibrated, male_lead: Calibrated,
                       male_ramp: Calibrated, male_decline_from: Calibrated,
                       male_floor: Calibrated, male_decline_to: Calibrated }
                       (genetics specification 5.6),
             window { from: Calibrated, until: Calibrated }   (test data only, ADR-0017
                       section 5)
  Disease { code: DataId, carrier: Carrier, onset: Onset, effect: Effect }
  Carrier: locus { locus: DataId, penetrance_female: Calibrated,
                   penetrance_male: Calibrated },
           threshold { liability: Option<DataId>, heritability: Calibrated,
                       prevalence: Calibrated }   (an option: 0 for none, 1 then the value;
                       a named trait of liability is checked by `check_liability`, design
                       note of the threshold disease, section 1)
  Onset: fixed { age: Calibrated },
         normal { mean: Calibrated, sd: Calibrated, low: Calibrated, high: Calibrated }
  Effect: death { per_year: Calibrated },
          death_in_two_steps { before: Calibrated, change_age: Calibrated,
                               after: Calibrated },
          visible
  SexDifference { trait: DataId, shift: Calibrated, variance_ratio: Calibrated }
  Calibrated { value: f64, unit: Unit }
  PhenotypeWeights { trait: DataId, h2_child: Calibrated, c2_child: Calibrated,
                     h2_adult: Calibrated, c2_adult: Calibrated }   (ADR-0009)
  Unit: morgan, per_transmission, fraction, centimeter, year, per_year (ADR-0010),
        centimeter_per_year (ADR-0012), centimeter_per_year_squared,
        standard_deviation, ratio (ADR-0014), logit, stage (ADR-0015),
        month, count, hour, per_standard_deviation (ADR-0017 section 1); one enum for
        both calibration files
  GenomeMap { format_version: u32, map_version: u32, traits: Vec<Trait>, loci: Vec<Locus> }
  Trait { id: DataId, sigma_ref: f64, mean_ref: Option<f64>, exclusive: bool,
          includes: Vec<Load> }   (`mean_ref`, Grok GF-1, genetics specification 4.1: the
          optional field of the map's trait entry after `sigma_ref`, a JSON number; skipped
          when absent (`skip_serializing_if`), so a map without it hashes as before, byte
          for byte; when present the byte 0x01 then the f64, as postcard writes `Some(f64)`.
          JSON `null` is absent, as serde reads an `Option`)
  Load { trait: DataId, weight: f64 }
  Locus { id: DataId, chromosome: Chromosome, segment: u8, slot: u8, action: Action,
          mark: Mark, mutation: MutationDirection }
  Chromosome: autosome, x, y, mt
  Action: additive(Weights), recessive, dominant, threshold(Weights)
  Weights { effect: f64, d_ref: f64, loads: Vec<Load> }
  Mark: small, large, disease
  MutationDirection: both, to_alt, to_ref

With a pool (slice S2a of the switch to map 4, design note of the switch, sections 2 and 12)
the data also carry the frequencies of the pool's seven loci (`frequencies_of`, the rules
`frequencies_rejection`), and the data hash is xxh3-64 of the definitions' postcard followed
by the postcard of `(pool_seed, pool_hash, frequencies)` (`pool_suffix`,
`data_hash_with_pool`); `frozen_hash` stays the hash of the definitions alone.

The repository's data (slice S5 of the switch, design note section 14): data/ holds map 4,
its calibration and the life course data, and under data/pool/ the pool `pool.bin` with its
frequencies `pool_frequencies.json` (the wrapped form). The CLI builds its data with that
pool and those frequencies, so the repository's data hash is `data_hash_with_pool`
(`repository_hash`); the hash of the definitions alone is the pool's `frozen_hash`. A data
directory without a `pool/` (every fixture, tests/golden/map3) has no pool: its data hash is
that of its definitions, as before.

The shock templates (ADR-0023 section 3 and its addendum before S1b): data/shocks.json
(`{"format": "simagent.shocks", "version": 1, "shocks": [{id, kind, years, index, source}]}`,
every field required, none other; `source` is text and not hashed) holds the templates of
`TriggerShock`, the kind `hunger` only, `years` at least 1 (a u32), `index` in [0, 1), an ID
at most once (`shocks_of`). Data that hold templates append them to the data hash as their
own data set `core:shocks`, after the definitions and after the pool if there is one: the
postcard of the list of templates sorted by ID, `varint(len)`, then per template `string(id)`,
`varint(kind)` (hunger 0), `varint(years)`, the f64 `index` little endian (`shocks_suffix`).
Data without templates (every fixture, every test data set) append nothing, so their hash
stays; the pool's check still compares with the hash of the definitions alone
(`frozen_hash`). The CLI's data carry the repository's templates, so `repository_hash` is
xxh3-64 of the definitions, the pool suffix and the templates' suffix.

The houses (ADR-0025 sections 1 to 4, ADR-0026 sections 1 and 2, ADR-0024 section 4):
data/house/needs.json (the rates per need and age band, awake and asleep, and each need's
comfort), data/house/day.json (the rules of the day: school, work, the small children, the jumps
on coming home, the night, what a need at 0 does, the mood of a choice, the longest time-filler
and the least time left for one),
data/house/kinds.json (the kinds of furniture with their offers) and one file per house under
data/house/houses/, which name rooms and offers. `houses_checked` repeats the checks of the
reader (sim-cli: names once, one entrance, named rooms there) and of the core, each failure
naming its code (the day's in `day_checked`). Data that hold houses append `houses_suffix`
after the templates: the rates, the day, the kinds by ID, the houses by ID with their rooms as
numbers in the file's order; names are not hashed. The CLI's data hold the repository's
houses, so `repository_hash` ends with that suffix.

The game's demography (ADR-0017, addendum of 2026-11-05): calibration/demography.json stays
the document the pool was extracted from (22 values; the pool's `frozen_hash` is the hash of
the definitions with it). data/calibration/demography_game.json (`{"values": {...}}`, each
entry a calibrated value with its text) is the overlay of the game: its entries go into the
`values` map of that document (`overlaid`: a key there is replaced, a new key added), and the
CLI's data hash covers the result in place of the extract's (`repository_hash`;
`Data::with_calibration` after the pool, the templates and the houses). The three values of
the extra child, `core:fertility_count_until` (year), `core:fertility_extra` and
`core:fertility_grown_from` (count) and `core:fertility_grown_chance` (fraction), are one
group: none of them and exactly 22 values (the rule without an extra child), or all four and
exactly 26 (`extra_child`). A directory without the overlay file has none, and its hash is the
one before.

Usage: python3 tools/data_reference.py [DATA_DIR]
       (with DATA_DIR/pool/: the definitions' hash, the data hash with the pool, its
       frequencies and, with DATA_DIR/shocks.json, the templates; the pool's seed, hash,
       frozen hash and the frequency rules, the templates' suffix)
       python3 tools/data_reference.py [DATA_DIR] --pool POOL --frequencies FILE
       (the definitions' hash and the data hash with the pool and its frequencies)
"""

import json
import math
import struct
import sys
from pathlib import Path

import xxhash

TEXT_FIELDS = ("source", "as_of", "note", "description")
UNITS = [
    "morgan",
    "per_transmission",
    "fraction",
    "centimeter",
    "year",
    "per_year",
    "centimeter_per_year",
    "centimeter_per_year_squared",
    "standard_deviation",
    "ratio",
    "logit",
    "stage",
    "month",
    "count",
    "hour",
    "per_standard_deviation",
]
WEIGHTS = ("h2_child", "c2_child", "h2_adult", "c2_adult")
SEX_DIFFERENCE = ("shift", "variance_ratio")
# The forms of the fertility and their numbers in the order of the enum (design note of
# the fertility curves, section 1): variant 0 `puberty` (5.6), variant 1 `window`.
FERTILITY_FORMS = ["puberty", "window"]
FERTILITY_FIELDS = [
    (
        "menarche_mean",
        "menarche_sd",
        "menarche_low",
        "menarche_high",
        "female_ramp",
        "female_decline_from",
        "female_decline_to",
        "male_lead",
        "male_ramp",
        "male_decline_from",
        "male_floor",
        "male_decline_to",
    ),
    ("from", "until"),
]
# The calibration's fields that name a trait of the map, or none (ADR-0017 section 1), in
# the order of the struct; format 7 adds the two traits of the partner market (design note
# of the similarity, section 1).
NAMED_TRAITS = ("longevity_trait", "puberty_trait", "cognition_trait", "height_trait")
# Those of them a rule reads as a phenotype (genetics specification 4.2): a named trait must
# have its phenotype weights in the calibration's `traits` as well. Longevity reads the
# genetic value only (7.2, ADR-0009 section 1), so it needs none.
PHENOTYPE_TRAITS = ("puberty_trait", "cognition_trait", "height_trait")
# The calibration's format and its fields, every one required, none other (ADR-0007
# section 1: `deny_unknown_fields`, no `serde(default)`).
CALIBRATION_FORMAT = 7
CALIBRATION_FIELDS = (
    "format_version",
    "values",
    "traits",
    "sex_differences",
    "diseases",
    "longevity_trait",
    "puberty_trait",
    "cognition_trait",
    "height_trait",
    "fertility",
)
# The life course data's format and fields, every one required, none other (ADR-0007
# section 1); format 2 adds the table of leaving home (design note of the households,
# section 1), after the wish shape.
DEMOGRAPHY_FORMAT = 2
DEMOGRAPHY_FIELDS = ("format_version", "values", "fertility_wish_shape", "leave_home_ages")
# The ages of that table: 16 to 25, one weight each.
LEAVE_HOME_AGES = 10
CHROMOSOMES = ["autosome", "x", "y", "mt"]
ACTIONS = ["additive", "recessive", "dominant", "threshold"]
MARKS = ["small", "large", "disease"]
DIRECTIONS = ["both", "to_alt", "to_ref"]


def varint(value: int) -> bytes:
    assert value >= 0
    out = bytearray()
    while True:
        byte = value & 0x7F
        value >>= 7
        if value:
            out.append(byte | 0x80)
        else:
            out.append(byte)
            return bytes(out)


def string(text: str) -> bytes:
    data = text.encode("utf-8")
    return varint(len(data)) + data


def f64(value: float) -> bytes:
    return struct.pack("<d", value)


def u8(value: int) -> bytes:
    assert 0 <= value < 256
    return bytes([value])


CALIBRATED_TEXT = ("source", "as_of", "note")


def filled(entry, names) -> None:
    """Each text field is there and not blank, as the core readers demand (ADR-0007)."""
    for name in names:
        text = entry.get(name)
        assert isinstance(text, str) and text.strip(), f"text field {name} missing or blank"


def check_text(demography, calibration, genome_map) -> None:
    """The text fields of every number of the life course data (each value and each entry
    of the wish shape, ADR-0017 section 1), of every calibration value, every number of a
    phenotype row and of a sex difference (ADR-0009, ADR-0014), of every number of a
    disease and of the fertility (ADR-0017 section 1), and of every trait and locus
    (ADR-0008)."""
    for entry in list(demography["values"].values()) + demography["fertility_wish_shape"] + demography["leave_home_ages"]:
        filled(entry, CALIBRATED_TEXT)
    for entry in calibration["values"].values():
        filled(entry, CALIBRATED_TEXT)
    for row in calibration["traits"]:
        for weight in WEIGHTS:
            filled(row[weight], CALIBRATED_TEXT)
    for row in calibration["sex_differences"]:
        for number in SEX_DIFFERENCE:
            filled(row[number], CALIBRATED_TEXT)
    for entry in genome_map["traits"] + genome_map["loci"]:
        filled(entry, ("description",))
    for disease in calibration["diseases"]:
        check_numbers(disease)
    index, form = variant(calibration["fertility"], FERTILITY_FORMS)
    for name in FERTILITY_FIELDS[index]:
        filled(form[name], CALIBRATED_TEXT)


def check_numbers(value) -> None:
    """Every number inside a disease (an object with a unit) has its text (ADR-0017)."""
    if isinstance(value, dict) and "unit" in value:
        filled(value, CALIBRATED_TEXT)
    elif isinstance(value, dict):
        for inner in value.values():
            check_numbers(inner)
    elif isinstance(value, list):
        for inner in value:
            check_numbers(inner)


# The values of the life course data without the group of the extra child, and the group
# (ADR-0017, addendum of 2026-11-05): key, unit, whole range.
DEMOGRAPHY_VALUES = 22
EXTRA_CHILD = (
    ("core:fertility_count_until", "year", 1, 120),
    ("core:fertility_extra", "count", 1, 64),
    ("core:fertility_grown_from", "count", 1, 64),
)
# The fourth value of the group: the chance of a woman's draw that lets a grown child reopen
# her, a fraction in [0, 1].
GROWN_CHANCE = "core:fertility_grown_chance"


def extra_child(values) -> "tuple | None":
    """The group of the extra child from a `values` map (text stripped or not): None without
    any of its four keys, else `(count_until, extra, grown_from, grown_chance)`, the first three
    whole numbers, each of its unit and in its range, the chance a fraction in [0, 1]. Some but
    not all of them is an error (`data.calibration_missing` in the core), as is a count of
    values other than 22 without the group or 26 with it (`data.calibration_unknown`)."""
    keys = [key for key, _, _, _ in EXTRA_CHILD] + [GROWN_CHANCE]
    present = [key for key in keys if key in values]
    if not present:
        assert len(values) == DEMOGRAPHY_VALUES, f"{len(values)} values without the extra child"
        return None
    assert len(present) == len(keys), f"the extra child is a group: only {present}"
    assert len(values) == DEMOGRAPHY_VALUES + len(keys), f"{len(values)} values with the extra child"
    out = []
    for key, unit, low, high in EXTRA_CHILD:
        entry = values[key]
        assert entry["unit"] == unit, (key, entry["unit"])
        number = float(entry["value"])
        assert number == int(number) and low <= int(number) <= high, (key, number)
        out.append(int(number))
    entry = values[GROWN_CHANCE]
    assert entry["unit"] == "fraction", (GROWN_CHANCE, entry["unit"])
    chance = float(entry["value"])
    assert 0.0 <= chance <= 1.0, (GROWN_CHANCE, chance)
    out.append(chance)
    return tuple(out)


def overlaid(demography, overlay):
    """The game's life course data: a copy of `demography` with the entries of the overlay's
    `values` in its `values` map (replaced or added). The overlay has the field `values` only,
    each entry a calibrated value with its three text fields."""
    assert set(overlay) == {"values"}, f"overlay fields {sorted(overlay)}"
    out = json.loads(json.dumps(demography))
    for key, entry in overlay["values"].items():
        assert set(entry) == {"value", "unit", *CALIBRATED_TEXT}, (key, sorted(entry))
        out["values"][key] = json.loads(json.dumps(entry))
    return out


def game_overlay(data_dir: Path):
    """The overlay of the game, `data_dir/calibration/demography_game.json`, or None."""
    path = Path(data_dir) / "calibration" / "demography_game.json"
    return json.loads(path.read_text()) if path.exists() else None


def game_documents(data_dir: Path):
    """`documents` with the game's overlay on the life course data, if the directory has one."""
    demography, calibration, genome_map = documents(data_dir)
    overlay = game_overlay(data_dir)
    if overlay is not None:
        demography = overlaid(demography, overlay)
    return demography, calibration, genome_map


def check_demography_format(demography) -> None:
    """The life course data have format 2 and exactly their fields, and the table of
    leaving home is valid (design note of the households, section 1): ten entries of the
    unit `fraction`, each in [0, 1], at least one above 0."""
    assert set(demography) == set(DEMOGRAPHY_FIELDS), f"demography fields {sorted(demography)}"
    assert demography["format_version"] == DEMOGRAPHY_FORMAT, demography["format_version"]
    table = demography["leave_home_ages"]
    assert len(table) == LEAVE_HOME_AGES, f"leave_home_ages has {len(table)} entries"
    for entry in table:
        assert entry["unit"] == "fraction", entry
        assert 0.0 <= float(entry["value"]) <= 1.0, entry
    assert any(float(entry["value"]) > 0.0 for entry in table), "leave_home_ages: no weight above 0"
    extra_child(demography["values"])


def check_calibration_format(calibration) -> None:
    """The calibration has format 7 and exactly its fields (design note of the
    similarity, section 1: `cognition_trait` and `height_trait` are required)."""
    assert set(calibration) == set(CALIBRATION_FIELDS), f"calibration fields {sorted(calibration)}"
    assert calibration["format_version"] == CALIBRATION_FORMAT, calibration["format_version"]


def check_named_traits(calibration, genome_map) -> None:
    """A trait the calibration names (the trait of longevity, of the puberty timing, of
    cognition and of height) is a trait of the map (ADR-0017 section 1, `data.trait_named`);
    none is allowed. One read as a phenotype (`PHENOTYPE_TRAITS`) also has its row in the
    calibration's `traits` (design note of the similarity, section 1)."""
    traits = {entry["id"] for entry in genome_map["traits"]}
    weighted = {row["trait"] for row in calibration["traits"]}
    for field in NAMED_TRAITS:
        named = calibration[field]
        assert named is None or named in traits, f"{field} {named} is no trait of the map"
        if field in PHENOTYPE_TRAITS:
            assert named is None or named in weighted, f"{field} {named} has no phenotype weights"


def trait_bound(genome_map, trait_id: str) -> float:
    """The bound of a trait's `A` (ADR-0008 section 3, `data.trait_bounds`): the fold from
    +0.0 of the largest magnitude of each term over the doses 0, 1 and 2, in the order of
    `A` (first the loci that load on the trait, in map order, coefficient `λ · β`; then per
    included trait in the order of `includes` the loci that load on it, coefficient `(w · λ)
    · β`; additive and threshold loci), each term `|c · (d − d_ref)|`; with a `mean_ref`
    (Grok GF-1) its magnitude is added last. Each term must be finite and, for a load (not
    an include), one of its three values not 0; the fold must be finite."""
    traits = {entry["id"]: entry for entry in genome_map["traits"]}
    entry = traits[trait_id]
    acc = 0.0

    def group(target: str, include_weight):
        nonlocal acc
        for locus in genome_map["loci"]:
            action = locus["action"]
            if isinstance(action, str):
                continue
            ((_, weights),) = action.items()
            for load in weights["loads"]:
                if load["trait"] != target:
                    continue
                lam, beta, d_ref = float(load["weight"]), float(weights["effect"]), float(weights["d_ref"])
                coefficient = lam * beta if include_weight is None else (include_weight * lam) * beta
                terms = [coefficient * (dose - d_ref) for dose in (0.0, 1.0, 2.0)]
                assert all(math.isfinite(term) for term in terms), f"data.trait_bounds {trait_id}: a term of {locus['id']} is not finite"
                assert include_weight is not None or any(term != 0.0 for term in terms), f"data.trait_bounds {trait_id}: every term of {locus['id']} is 0"
                acc = acc + max(abs(term) for term in terms)

    group(trait_id, None)
    for include in entry["includes"]:
        group(include["trait"], float(include["weight"]))
    mean = mean_ref_of(entry)
    if mean is not None:
        acc = acc + abs(mean)
    assert math.isfinite(acc), f"data.trait_bounds {trait_id}: the bound is not finite"
    return acc


def check_trait_bounds(calibration, genome_map) -> None:
    """`data.trait_bounds` (ADR-0008 section 3, ADR-0009 section 2): per trait the bound
    (`trait_bound`, with `|mean_ref|` when present) divided by `sigma_ref` is finite, and at
    most 2^1022 for a trait with a phenotype row in the calibration. Without a `mean_ref`
    nothing is added."""
    rows = {row["trait"] for row in calibration["traits"]}
    for entry in genome_map["traits"]:
        quotient = trait_bound(genome_map, entry["id"]) / float(entry["sigma_ref"])
        assert math.isfinite(quotient), f"data.trait_bounds {entry['id']}: the bound over sigma_ref is not finite"
        assert entry["id"] not in rows or quotient <= 2.0**1022, f"data.trait_bounds {entry['id']}: the bound over sigma_ref is above 2^1022"


def f64_bits(value: float) -> str:
    """The bits of an f64 as 16 hex digits (big endian), for comparisons bit for bit."""
    return struct.pack(">d", value).hex()


def check_liability(calibration, genome_map) -> None:
    """A threshold carrier that names its trait of liability (design note of the threshold
    disease, section 1; genetics specification 3.6; ADR-0008 section 1, ADR-0009 sections 1
    and 2, ADR-0020 section 2), each rule `DataError::Disease` (`data.disease`) in the core:
    the trait is a trait of the map; its data ID is the code of the disease; the
    calibration has a phenotype row for it; that row's child and adult weights are equal
    bit for bit (h² child = h² adult, c² child = c² adult); its h² equals the disease's
    `heritability` bit for bit; its c² is 0 (in both ages, by the equality); `1 − K` is
    below 1 in f64 (else `probit(1 − K)` is NaN; review of the threshold slice). A carrier
    without a named trait (`null`) is not checked here: nobody has its disease."""
    traits = {entry["id"] for entry in genome_map["traits"]}
    rows = {row["trait"]: row for row in calibration["traits"]}
    for disease in calibration["diseases"]:
        index, carrier = variant(disease["carrier"], ["locus", "threshold"])
        if index == 0 or carrier["liability"] is None:
            continue
        code, named = disease["code"], carrier["liability"]
        assert named in traits, f"data.disease: {code} names the liability trait {named}, no trait of the map"
        assert named == code, f"data.disease: {code} names the liability trait {named}, not its own code"
        assert named in rows, f"data.disease: {code}: the liability trait {named} has no phenotype row"
        h2_child, c2_child, h2_adult, c2_adult = (f64_bits(float(rows[named][name]["value"])) for name in WEIGHTS)
        assert h2_child == h2_adult and c2_child == c2_adult, f"data.disease: {code}: child and adult weights of {named} differ"
        heritability = f64_bits(float(carrier["heritability"]["value"]))
        assert h2_adult == heritability, f"data.disease: {code}: h2 of {named} is not the heritability bit for bit"
        assert c2_adult == f64_bits(0.0), f"data.disease: {code}: c2 of {named} is not 0"
        prevalence = float(carrier["prevalence"]["value"])
        assert 1.0 - prevalence < 1.0, f"data.disease: {code}: 1 - K rounds to 1, so T = probit(1 - K) is no number"


# The recorded known answers of `sim_math::probit` (ADR-0019, AS241): the threshold of a
# threshold disease is `T = probit(1 − K)`, computed once by `Data::new` (design note of the
# threshold disease, section 1). The reference takes the recorded bits of the core's AS241
# (the column `expected_bits`, written by `cargo xtask kat-record`), not the correctly
# rounded value (`reference_bits`, within a few ulp of it), so that both sides compare
# against the same `T`.
PROBIT_KAT = Path(__file__).resolve().parent.parent / "tests" / "golden" / "kat" / "probit.csv"


def recorded_probit(p: float) -> float:
    """The recorded `probit(p)` of tests/golden/kat/probit.csv (a row of class P for the
    input bits of `p`, its expected bits), checked against the bound of ADR-0019 section 2
    (2^-48 relative to the correctly rounded reference column)."""
    rows = {}
    for line in PROBIT_KAT.read_text(encoding="utf-8").splitlines()[2:]:
        input_bits, expected, klass, reference = line.split(",")
        rows[input_bits] = (expected, klass, reference)
    key = f64_bits(p)
    assert key in rows, f"probit({p!r}) (input {key}) has no recorded row in {PROBIT_KAT.name}"
    expected, klass, reference = rows[key]
    assert klass == "P", (key, klass)
    value = struct.unpack(">d", bytes.fromhex(expected))[0]
    exact = struct.unpack(">d", bytes.fromhex(reference))[0]
    assert abs(value - exact) <= 2.0**-48 * abs(exact), (key, expected, reference)
    return value


def threshold(prevalence: float) -> float:
    """`T = probit(1 − K)` (genetics specification 3.6, ADR-0019) for the prevalence `K`:
    `1.0 − K` in f64, then the recorded `probit` (`recorded_probit`)."""
    assert 0.0 < prevalence < 1.0, prevalence
    return recorded_probit(1.0 - prevalence)


def liability_diseases(calibration) -> list:
    """(code, trait, heritability, prevalence, T) of each threshold disease of the
    calibration (parsed JSON) that names its trait of liability, in the order of the data."""
    found = []
    for disease in calibration["diseases"]:
        index, carrier = variant(disease["carrier"], ["locus", "threshold"])
        if index == 1 and carrier["liability"] is not None:
            prevalence = float(carrier["prevalence"]["value"])
            found.append((disease["code"], carrier["liability"], float(carrier["heritability"]["value"]), prevalence, threshold(prevalence)))
    return found


def strip(value):
    """The file without its text fields, which every place that has them must fill."""
    if isinstance(value, dict):
        return {k: strip(v) for k, v in value.items() if k not in TEXT_FIELDS}
    if isinstance(value, list):
        return [strip(v) for v in value]
    return value


def calibrated_bytes(entry) -> bytes:
    return f64(float(entry["value"])) + varint(UNITS.index(entry["unit"]))


def demography_bytes(doc) -> bytes:
    """DemographyCalibration: format_version, the values in key order, the wish shape in
    file order, the table of leaving home in file order (the ages 16 to 25)."""
    values = doc["values"]
    out = varint(doc["format_version"]) + varint(len(values))
    for key in sorted(values):
        out += string(key) + calibrated_bytes(values[key])
    for name in ("fertility_wish_shape", "leave_home_ages"):
        out += varint(len(doc[name]))
        for entry in doc[name]:
            out += calibrated_bytes(entry)
    return out


def calibration_bytes(doc) -> bytes:
    values = doc["values"]
    out = varint(doc["format_version"]) + varint(len(values))
    for key in sorted(values):
        out += string(key) + calibrated_bytes(values[key])
    out += varint(len(doc["traits"]))
    for row in doc["traits"]:
        out += string(row["trait"])
        for weight in WEIGHTS:
            out += calibrated_bytes(row[weight])
    out += varint(len(doc["sex_differences"]))
    for row in doc["sex_differences"]:
        out += string(row["trait"])
        for number in SEX_DIFFERENCE:
            out += calibrated_bytes(row[number])
    out += varint(len(doc["diseases"]))
    for disease in doc["diseases"]:
        out += disease_bytes(disease)
    for field in NAMED_TRAITS:
        out += option_id(doc[field])
    out += fertility_bytes(doc["fertility"])
    return out


def fertility_bytes(fertility) -> bytes:
    """Fertility: the variant index, then its numbers in the order of the enum (the twelve
    of `puberty` in the order of the design note's table, or `from`, `until`)."""
    index, form = variant(fertility, FERTILITY_FORMS)
    names = FERTILITY_FIELDS[index]
    assert set(form) == set(names), f"fertility {FERTILITY_FORMS[index]}: fields {sorted(form)}"
    out = varint(index)
    for name in names:
        out += calibrated_bytes(form[name])
    return out


def option_id(value) -> bytes:
    """Option<DataId>: 0 for none, 1 then the string."""
    return varint(0) if value is None else varint(1) + string(value)


def variant(value, names):
    """The variant of an externally tagged enum: its index and its fields (or none)."""
    if isinstance(value, str):
        return names.index(value), {}
    (name, fields), = value.items()
    return names.index(name), fields


def disease_bytes(disease) -> bytes:
    out = string(disease["code"])
    index, carrier = variant(disease["carrier"], ["locus", "threshold"])
    out += varint(index)
    if index == 0:
        out += string(carrier["locus"])
        out += calibrated_bytes(carrier["penetrance_female"])
        out += calibrated_bytes(carrier["penetrance_male"])
    else:
        liability = carrier["liability"]
        out += option_id(liability)
        out += calibrated_bytes(carrier["heritability"]) + calibrated_bytes(carrier["prevalence"])
    index, onset = variant(disease["onset"], ["fixed", "normal"])
    out += varint(index)
    names = ["age"] if index == 0 else ["mean", "sd", "low", "high"]
    for name in names:
        out += calibrated_bytes(onset[name])
    index, effect = variant(disease["effect"], ["death", "death_in_two_steps", "visible"])
    out += varint(index)
    names = [["per_year"], ["before", "change_age", "after"], []][index]
    for name in names:
        out += calibrated_bytes(effect[name])
    return out


def boolean(value: bool) -> bytes:
    assert isinstance(value, bool)
    return bytes([1 if value else 0])


def loads_bytes(loads) -> bytes:
    out = varint(len(loads))
    for load in loads:
        out += string(load["trait"]) + f64(float(load["weight"]))
    return out


def action_bytes(action) -> bytes:
    """A unit variant is a string in the file, a variant with weights an object."""
    if isinstance(action, str):
        assert action in ("recessive", "dominant"), action
        return varint(ACTIONS.index(action))
    [(name, weights)] = action.items()
    assert name in ("additive", "threshold"), name
    return (
        varint(ACTIONS.index(name))
        + f64(float(weights["effect"]))
        + f64(float(weights["d_ref"]))
        + loads_bytes(weights["loads"])
    )


def mean_ref_of(entry) -> "float | None":
    """The `mean_ref` of a trait entry of the map (Grok GF-1, genetics specification 4.1):
    None if the field is absent or `null` (a missing serde `Option`, no `serde(default)`,
    ADR-0007), else the
    JSON number as f64, finite and not -0.0 (a `FiniteF64`, ADR-0008 section 2; a number,
    not a bool)."""
    value = entry.get("mean_ref")
    if value is None:
        return None
    assert isinstance(value, (int, float)) and not isinstance(value, bool), f"data.trait_mean {entry['id']}: {value!r}"
    value = float(value)
    assert math.isfinite(value), f"data.trait_mean {entry['id']}: {value!r}"
    # A `FiniteF64` (ADR-0008 section 2): finite, and -0.0 is a read error.
    assert not (value == 0.0 and math.copysign(1.0, value) < 0.0), f"data.trait_mean {entry['id']}: -0.0"
    return value


def trait_bytes(entry) -> bytes:
    """Trait: `id`, `sigma_ref`, `mean_ref` (only when present: 0x01 then the f64; nothing
    when absent, `skip_serializing_if`), `exclusive`, `includes`."""
    out = string(entry["id"]) + f64(float(entry["sigma_ref"]))
    mean = mean_ref_of(entry)
    if mean is not None:
        out += u8(1) + f64(mean)
    return out + boolean(entry["exclusive"]) + loads_bytes(entry["includes"])


def genome_map_bytes(doc) -> bytes:
    out = varint(doc["format_version"]) + varint(doc["map_version"])
    out += varint(len(doc["traits"]))
    for entry in doc["traits"]:
        out += trait_bytes(entry)
    out += varint(len(doc["loci"]))
    for locus in doc["loci"]:
        out += string(locus["id"])
        out += varint(CHROMOSOMES.index(locus["chromosome"]))
        out += u8(locus["segment"]) + u8(locus["slot"])
        out += action_bytes(locus["action"])
        out += varint(MARKS.index(locus["mark"]))
        out += varint(DIRECTIONS.index(locus["mutation"]))
    return out


REPOSITORY_DATA = Path(__file__).resolve().parent.parent / "data"


def documents(data_dir: Path):
    """The life course data, the calibration and the genome map under `data_dir` as parsed
    JSON, text fields and all, in the order of their data IDs. Without its own
    calibration/demography.json the directory takes the repository's."""
    demography_path = data_dir / "calibration" / "demography.json"
    if not demography_path.exists():
        demography_path = REPOSITORY_DATA / "calibration" / "demography.json"
    demography = json.loads(demography_path.read_text())
    calibration = json.loads((data_dir / "calibration" / "genetics.json").read_text())
    genome_map = json.loads((data_dir / "genetics" / "genome_map.json").read_text())
    return demography, calibration, genome_map


def definitions_bytes(demography, calibration, genome_map) -> bytes:
    """The postcard of the definitions (the life course data, a calibration and a genome map
    given as parsed JSON, test data built in memory or the files): the text fields checked,
    then stripped, as the core sees them. A named trait of liability is checked
    (`check_liability`); its threshold `T` is kept with the data, not hashed on its own (the
    hash covers `K`). The bounds of the traits are checked (`check_trait_bounds`)."""
    check_demography_format(demography)
    check_calibration_format(calibration)
    check_text(demography, calibration, genome_map)
    check_named_traits(calibration, genome_map)
    check_liability(calibration, genome_map)
    check_trait_bounds(calibration, genome_map)
    demography, calibration, genome_map = strip(demography), strip(calibration), strip(genome_map)
    return demography_bytes(demography) + calibration_bytes(calibration) + genome_map_bytes(genome_map)


def documents_hash(demography, calibration, genome_map) -> int:
    """The data hash of the definitions without a pool (`definitions_bytes`), xxh3-64 with
    seed 0. With a pool it is also the pool's `frozen_hash` (design note of the pool's
    records, section 6)."""
    return xxhash.xxh3_64_intdigest(definitions_bytes(demography, calibration, genome_map), seed=0)


def data_hash(data_dir: Path) -> int:
    return documents_hash(*documents(data_dir))


# ---------------------------------------------------------------------------------------
# The frequencies of the pool (slice S2a of the switch to map 4, design note of the switch,
# sections 2 and 12): data with a pool carry the frequency `q` of the alternative allele at
# each of the seven loci of the pool (design note of the pool file, section 3), and the data
# hash covers them.

# The seven loci of the pool, in the order of the pool file's table (design note of the pool
# file, section 3).
POOL_LOCI = (
    "core:disease_ar",
    "core:disease_ad_late",
    "core:disease_ad_partial",
    "core:disease_xr",
    "core:disease_mt",
    "core:mc1r",
    "core:herc2",
)
FREQUENCY_FIELDS = ("value", "unit", "source", "as_of", "note")


def frequencies_of(document) -> dict:
    """{locus: q} of a frequencies file in the wrapped form `{"pool_frequencies": {locus:
    {value, unit, source, as_of, note}}}` (tests/golden/pool/pool_frequencies.json, from S5
    data/pool/pool_frequencies.json), parsed JSON. The checks of the file reader (xtask
    `take_frequencies`, which keeps them): the one field `pool_frequencies` holding an
    object, each entry exactly the five fields, the unit `fraction`, the texts filled (each
    failure an AssertionError naming its rule: `pool.frequencies_loci`, `pool.frequency_unit`,
    `pool.frequency_text`); the texts are removed. The rules of the data
    (`frequencies_rejection`) are not checked here: `Data::with_pool` checks them."""
    assert isinstance(document, dict) and list(document) == ["pool_frequencies"], "pool.frequencies_loci: no object pool_frequencies"
    entry = document["pool_frequencies"]
    assert isinstance(entry, dict), "pool.frequencies_loci: no object pool_frequencies"
    out = {}
    for locus, value in entry.items():
        assert isinstance(value, dict) and set(value) == set(FREQUENCY_FIELDS), f"pool.frequencies_loci {locus} fields {sorted(value) if isinstance(value, dict) else value}"
        assert value["unit"] == "fraction", f"pool.frequency_unit {locus}"
        for name in CALIBRATED_TEXT:
            assert isinstance(value[name], str) and value[name].strip(), f"pool.frequency_text {locus} {name}"
        q = value["value"]
        assert isinstance(q, (int, float)) and not isinstance(q, bool), f"pool.frequency_range {locus} {q!r}"
        out[locus] = float(q)
    return out


def frequencies_file(path: Path) -> dict:
    """`frequencies_of` the file at `path`."""
    return frequencies_of(json.loads(Path(path).read_text()))


def frequencies_rejection(frequencies: dict):
    """The first rule of the data the frequencies break, or None. `Data::with_pool` checks them
    after its pool checks (`pool.carer_not_member`, `pool.sex`, `pool.locus`,
    `pool.frozen_hash`), in this order: `pool.frequencies_loci` (the loci are not exactly the
    seven of the pool: one missing, one more, or none at all), then `pool.frequency_range`
    (a `q` not in the open interval (0, 1): 0, 1, below 0, above 1; a NaN or an infinity is
    no `FiniteF64` and fails the same rule here)."""
    if set(frequencies) != set(POOL_LOCI):
        return "pool.frequencies_loci"
    for locus in sorted(frequencies):
        q = float(frequencies[locus])
        if not (math.isfinite(q) and 0.0 < q < 1.0):
            return "pool.frequency_range"
    return None


def pool_suffix(pool_seed: int, pool_hash: int, frequencies: dict) -> bytes:
    """The postcard of `(pool_seed, pool_hash, frequencies)` that the data hash appends to the
    definitions with a pool (design note of the switch, section 12): `pool_seed` and
    `pool_hash` as varints, the frequencies as a sequence of `(locus ID as a string, q as an
    f64)` ascending by ID (a `BTreeMap<DataId, FiniteF64>` writes the same bytes: the length,
    then the pairs in key order)."""
    out = varint(pool_seed) + varint(pool_hash) + varint(len(frequencies))
    for locus in sorted(frequencies):
        out += string(locus) + f64(float(frequencies[locus]))
    return out


def data_hash_with_pool(definitions: bytes, pool_seed: int, pool_hash: int, frequencies: dict) -> int:
    """The data hash with a pool: xxh3-64 (seed 0) of the definitions' postcard followed by
    `pool_suffix`. `frozen_hash` stays the hash of the definitions alone."""
    return xxhash.xxh3_64_intdigest(definitions + pool_suffix(pool_seed, pool_hash, frequencies), seed=0)


# ---------------------------------------------------------------------------------------
# The shock templates (ADR-0023 section 3, addendum before S1b): data/shocks.json, their own
# data set `core:shocks` in the data hash, after the definitions and the pool.

SHOCKS_FORMAT = "simagent.shocks"
SHOCKS_VERSION = 1
SHOCKS_FILE_FIELDS = ("format", "version", "shocks")
# The fields of a template by its kind (ADR-0023 section 3 and its addendum of 2026-11-01):
# exactly these, `source` the text.
SHOCK_FIELDS = {
    "hunger": ("id", "kind", "years", "index", "source"),
    "fertility_boost": ("id", "kind", "months", "rate", "source"),
}
# The kinds of a template in the order of the enum: variant 0 `hunger`, variant 1
# `fertility_boost` ("Zeit zu zweit").
SHOCK_KINDS = ["hunger", "fertility_boost"]
# `years` and `months` are u32.
SHOCK_YEARS_MAX = 2**32 - 1


def finite(value, rule: str) -> float:
    """`value` as a `FiniteF64`: a number, no NaN, no infinity, no -0.0; else `rule` fails."""
    assert isinstance(value, (int, float)) and not isinstance(value, bool), rule
    value = float(value)
    assert math.isfinite(value) and not (value == 0.0 and math.copysign(1.0, value) < 0.0), rule
    return value


def templates_checked(templates) -> list:
    """The templates checked by the rules of the data (ADR-0023 section 3 and its addendum of
    2026-11-01) and sorted by ID, each failure an AssertionError naming its rule:
    `shocks.duplicate` (an ID twice), `shocks.kind` (neither `hunger` nor `fertility_boost`);
    a hunger (`years`, `index`): `shocks.years` (below 1, or no u32), `shocks.index` (not a
    finite number in [0, 1)); a boost (`months`, `rate`): `shocks.months` (below 1, or no
    u32), `shocks.rate` (not a finite number in (0, 1]). A `source` is ignored. Returns the
    templates as dicts of their hashed fields, ascending by ID."""
    out = []
    for template in templates:
        identifier, kind = template["id"], template["kind"]
        assert isinstance(identifier, str) and identifier, f"shocks.id {identifier!r}"
        assert kind in SHOCK_KINDS, f"shocks.kind {identifier} {kind!r}"
        count = template["years"] if kind == "hunger" else template["months"]
        rule = "shocks.years" if kind == "hunger" else "shocks.months"
        assert isinstance(count, int) and not isinstance(count, bool) and 1 <= count <= SHOCK_YEARS_MAX, f"{rule} {identifier} {count!r}"
        if kind == "hunger":
            index = finite(template["index"], f"shocks.index {identifier} {template['index']!r}")
            assert 0.0 <= index < 1.0, f"shocks.index {identifier} {index!r}"
            out.append({"id": identifier, "kind": kind, "years": count, "index": index})
        else:
            rate = finite(template["rate"], f"shocks.rate {identifier} {template['rate']!r}")
            assert 0.0 < rate <= 1.0, f"shocks.rate {identifier} {rate!r}"
            out.append({"id": identifier, "kind": kind, "months": count, "rate": rate})
    ids = [template["id"] for template in out]
    assert len(set(ids)) == len(ids), f"shocks.duplicate {sorted(ids)}"
    return sorted(out, key=lambda template: template["id"].encode("utf-8"))


def shocks_of(document) -> list:
    """The templates of a shocks file (data/shocks.json), parsed JSON: exactly the fields
    `format` (`simagent.shocks`), `version` (1) and `shocks` (a list), each template exactly
    the fields of its kind (`SHOCK_FIELDS`; `deny_unknown_fields`, every field required;
    the text `source` filled, as every text of the data, and not hashed); then
    `templates_checked`. Returns them without their text, ascending by ID."""
    assert isinstance(document, dict) and set(document) == set(SHOCKS_FILE_FIELDS), f"shocks.fields {sorted(document) if isinstance(document, dict) else document!r}"
    assert document["format"] == SHOCKS_FORMAT, f"shocks.format {document['format']!r}"
    assert document["version"] == SHOCKS_VERSION, f"shocks.version {document['version']!r}"
    assert isinstance(document["shocks"], list), "shocks.fields shocks"
    for template in document["shocks"]:
        fields = SHOCK_FIELDS.get(template.get("kind")) if isinstance(template, dict) else None
        assert fields is not None and set(template) == set(fields), f"shocks.fields {sorted(template) if isinstance(template, dict) else template!r}"
        filled(template, ("source",))
    return templates_checked(document["shocks"])


def shock_templates(data_dir: Path):
    """The templates of `data_dir/shocks.json` (`shocks_of`), or None without that file (every
    fixture: test data hold no templates)."""
    path = Path(data_dir) / "shocks.json"
    if not path.exists():
        return None
    return shocks_of(json.loads(path.read_text()))


def shocks_suffix(templates) -> bytes:
    """The postcard of the templates that the data hash appends (ADR-0023 section 3, addendum
    before S1b, addendum of 2026-11-01): the list sorted by ID, `varint(len)`, then per template
    `string(id)`, `varint(kind)`, then the kind's fields: a hunger (0) `varint(years)` and
    `index` as an f64 little endian, a boost (1) `varint(months)` and `rate` likewise. Empty
    (nothing appended) for data without templates (None or an empty list)."""
    if not templates:
        return b""
    out = varint(len(templates))
    for template in templates_checked(templates):
        out += string(template["id"]) + varint(SHOCK_KINDS.index(template["kind"]))
        if template["kind"] == "hunger":
            out += varint(template["years"]) + f64(template["index"])
        else:
            out += varint(template["months"]) + f64(template["rate"])
    return out


# ---------------------------------------------------------------------------------------
# The houses (ADR-0025 sections 1 to 4, ADR-0026 sections 1 and 2, ADR-0024 section 4):
# data/house/needs.json, data/house/day.json, data/house/kinds.json and
# data/house/houses/*.json, their own data set after the definitions, the pool and the shock
# templates.

NEEDS = ["hunger", "energy", "bladder", "hygiene", "fun"]
OLDEST = 130
AGE_BANDS = [(6, 12), (13, 17), (18, 66), (67, OLDEST)]
ESSENTIALS = ["core:dining_table", "core:fridge", "core:toilet", "core:shower", "core:sofa"]
SOFA = "core:sofa"
OFFER_FIELDS = {"id", "rates", "seconds", "until_full", "max_seconds", "sleep", "min_age", "max_age", "fallback"}
# A need's comfort lies above the critical level and at most at full (ADR-0026, addendum of
# 2026-11-04 on comfort and the time-filler).
COMFORT = (151, 1000)
# Whom a bed is for (ADR-0026, addendum of 2026-11-04 after the playtest), in the order of the
# core's `BedFor`.
BED_FOR = ["couples", "children"]
HOUSE_FIELDS = {"id", "capacity", "rooms", "child_day_room", "child_night_room", "dwell_room", "doors", "objects"}
# The fields of day.json in the order of `DayRules` (ADR-0024 section 4), every one required,
# none other, each an integer of its type: u8, u16, u32 or i32.
U8, U16, U32, I32 = (0, 2**8), (0, 2**16), (0, 2**32), (-(2**31), 2**31)
HOURS_FORMAT = {"depart": U32, "back": U32}
DAY_FORMAT = {
    "day_plan_at": U32,
    "wake_before": U32,
    "school": {"from_age": U8, "until_age": U8, "depart": U32, "back": U32},
    "work": {
        "from_age": U8,
        "until_age": U8,
        "full_time_below": U8,
        "part_time_below": U8,
        "full_time": HOURS_FORMAT,
        "part_time": HOURS_FORMAT,
    },
    "return_jumps": {need: I32 for need in NEEDS},
    "young": {"until_age": U8, "kita_from": U32, "kita_to": U32, "bed_from": U32, "bed_to": U32},
    # The night per age band (ADR-0026, addendum of 2026-11-04): four u32 each; the end of a night
    # before a Saturday or a Sunday morning since its addendum after the playtest.
    "night": {"from": (U32, 4), "until": (U32, 4), "until_weekend": (U32, 4)},
    # What a need at 0 does (ADR-0026, addendum of 2026-11-04 on a need at 0).
    "zero": {"collapse_energy_rate": I32, "stand_up_at": U16, "accident_hygiene_at_most": U16, "hygiene_empty_fun": I32},
    # The most the mood moves the points of a choice, in percent (its addendum on the mood).
    "choice_mood_percent": U8,
    # The longest a time-filler lasts (its addendum on comfort and the time-filler).
    "fallback_max_seconds": U32,
    # The least time left for a time-filler to be taken (its addendum of 2026-11-05 on the
    # time-filler's length).
    "fallback_min_seconds": U32,
    # The reserve at hunger 0 (its addendum of 2026-11-06 on hunger at 0): the capacities of a
    # thin, an ordinary and a heavy body, the drain per age band awake and asleep, the refill, the
    # energy's extra when weak and when wasted, the walk factor.
    "reserve": {
        "capacity": (U16, 3),
        "drain": {"awake": (I32, 4), "asleep": (I32, 4)},
        "refill": I32,
        "weak_energy": I32,
        "wasted_energy": I32,
        "walk_factor": U32,
    },
}
SECONDS_OF_DAY = 86_400


def zigzag(value: int) -> bytes:
    """An i32 as postcard writes it: zigzag, then varint."""
    assert -(2**31) <= value < 2**31, value
    return varint((value << 1) ^ (value >> 31))


def option(value, encode) -> bytes:
    return b"\x00" if value is None else b"\x01" + encode(value)


def house_files(data_dir: Path):
    """(needs, day, kinds, houses) of `data_dir/house/`, parsed JSON, the houses in the order
    of their file names, or None without that folder (every fixture: test data hold no
    houses)."""
    folder = Path(data_dir) / "house"
    if not folder.exists():
        return None
    # A drawing `<name>.layout.json` is no house and not in the data hash (ADR-0025 section 1).
    paths = [path for path in sorted((folder / "houses").glob("*.json")) if not path.name.endswith(".layout.json")]
    houses = [json.loads(path.read_text()) for path in paths]
    needs, day, kinds = (json.loads((folder / name).read_text()) for name in ("needs.json", "day.json", "kinds.json"))
    return needs, day, kinds, houses


def band(age: int):
    return next((at for at, (low, high) in enumerate(AGE_BANDS) if low <= age <= high), None)


def offer_rate(needs: dict, offer: dict, need: str, at: int) -> int:
    """The rate of `need` during `offer` in the age band `at` (ADR-0026 sections 1 and 2): the
    offer's own rate, else the sleep column for a sleeping offer, else the awake column."""
    rates = offer.get("rates", {})
    if need in rates:
        return rates[need]
    return needs["asleep" if offer.get("sleep", False) else "awake"][need][at]


def raises(needs: dict, offer: dict, need: str, age: int) -> bool:
    at = band(age)
    open_at = offer["min_age"] <= age <= offer["max_age"]
    return open_at and at is not None and offer_rate(needs, offer, need, at) > 0


def day_fields(entry, names: dict) -> None:
    """The reader's part of day.json: every field of `names` and none other, each an integer
    in the range of its type (`deny_unknown_fields`)."""
    assert isinstance(entry, dict) and set(entry) == set(names), f"day fields {entry}"
    for name, bounds in names.items():
        if isinstance(bounds, dict):
            day_fields(entry[name], bounds)
        elif isinstance(bounds[0], tuple):
            # An array of fixed length (serde reads `[u32; 4]` only with exactly four).
            (low, high), length = bounds
            values = entry[name]
            assert isinstance(values, list) and len(values) == length, f"day {name} {values}"
            assert all(type(value) is int and low <= value < high for value in values), f"day {name} {values}"
        else:
            value = entry[name]
            assert type(value) is int and bounds[0] <= value < bounds[1], f"day {name} {value}"


def day_checked(day: dict) -> None:
    """The core's checks of the day (ADR-0024 section 4) in its order, after the reader's: the
    times (each below 24 h, each departure before its return, the small children in bed until
    `bed_to`, then in the kindergarten, in bed again from `bed_from`), the wake (after the day
    plan, not before midnight), the ages (small children, school, work in turn), the shares of
    the work plan (0 < full time < part time <= 100), the jumps on coming home (-1000 to 0), the
    night (ADR-0026, addendum of 2026-11-04: each bound below 24 h, each end at or after the day
    plan and at or before every wake, each start after every return), a need at 0 (its addendum
    of 2026-11-04 on a need at 0: energy rising on the floor, the level to get up at from 1 to
    1000, the accident's hygiene at most 1000, the change of fun at hygiene 0 from -1000 to 0),
    the mood (at most 50 percent either way), the longest time-filler (60 s to 12 h, as an
    offer's length; its addendum on comfort and the time-filler), the least time left for one
    at most that (its addendum of 2026-11-05 on the time-filler's length) and the reserve at
    hunger 0 (its addendum of 2026-11-06: the capacities rising from 2, the drain below 0 in every
    band, the refill above 0, the extras at most 0 and the wasted one at most the weak one, the
    walk factor from 1 to 10)."""
    day_fields(day, DAY_FORMAT)
    school, work, young, jumps = day["school"], day["work"], day["young"], day["return_jumps"]
    hours = [(school["depart"], school["back"])] + [(work[k]["depart"], work[k]["back"]) for k in ("full_time", "part_time")]
    young_times = [young["bed_to"], young["kita_from"], young["kita_to"], young["bed_from"]]
    times = [time for pair in hours for time in pair] + young_times + [day["day_plan_at"]]
    assert all(time < SECONDS_OF_DAY for time in times), "house.day_time"
    assert all(depart < back for depart, back in hours), "house.day_time"
    assert all(a < b for a, b in zip(young_times, young_times[1:])), "house.day_time"
    wake = day["wake_before"]
    assert wake > 0 and all(wake <= depart and depart - wake > day["day_plan_at"] for depart, _ in hours), "house.day_wake"
    assert young["until_age"] <= school["from_age"] < school["until_age"] <= work["from_age"] < work["until_age"], "house.day_age"
    assert 0 < work["full_time_below"] < work["part_time_below"] <= 100, "house.day_share"
    assert all(-1000 <= jumps[need] <= 0 for need in NEEDS), "house.day_jump"
    night = day["night"]
    wakes = [depart - wake for depart, _ in hours]
    ends = night["until"] + night["until_weekend"]
    assert all(time < SECONDS_OF_DAY for time in night["from"] + ends), "house.day_night"
    assert all(day["day_plan_at"] <= until and all(until <= w for w in wakes) for until in night["until"]), "house.day_night"
    # A weekend morning has no wake (its addendum after the playtest).
    assert all(day["day_plan_at"] <= until for until in night["until_weekend"]), "house.day_night"
    assert all(all(start > back for _, back in hours) for start in night["from"]), "house.day_night"
    # The small children's bed holds its places from `bed_from`: every night starts at or after
    # it and ends at or before it.
    assert all(young["bed_from"] <= start for start in night["from"]), "house.day_night"
    assert all(until <= young["bed_from"] for until in ends), "house.day_night"
    zero = day["zero"]
    assert (
        zero["collapse_energy_rate"] > 0
        and 1 <= zero["stand_up_at"] <= 1000
        and zero["accident_hygiene_at_most"] <= 1000
        and -1000 <= zero["hygiene_empty_fun"] <= 0
    ), "house.day_zero"
    assert day["choice_mood_percent"] <= 50, "house.day_mood"
    assert 60 <= day["fallback_max_seconds"] <= 43_200, "house.day_fallback"
    assert day["fallback_min_seconds"] <= day["fallback_max_seconds"], "house.day_fallback"
    reserve = day["reserve"]
    thin, ordinary, heavy = reserve["capacity"]
    assert (
        2 <= thin < ordinary < heavy
        and all(rate < 0 for rate in reserve["drain"]["awake"] + reserve["drain"]["asleep"])
        and reserve["refill"] > 0
        and reserve["wasted_energy"] <= reserve["weak_energy"] <= 0
        and 1 <= reserve["walk_factor"] <= 10
    ), "house.day_reserve"


def houses_checked(needs, day, kinds, houses):
    """The checks of the day (ADR-0024 section 4), of ADR-0025 section 2 and of the offers'
    format (ADR-0026 section 2), in the order of the reader and the core; a failed one names
    its code. Returns (needs, day, kinds by ID, houses by ID) with the rooms numbered in each
    file's order."""
    assert isinstance(needs, dict) and set(needs) == {"awake", "asleep", "comfort"}, "needs fields"
    for column in (needs["awake"], needs["asleep"]):
        assert set(column) == set(NEEDS), "needs fields"
        assert all(len(rates) == 4 and all(isinstance(r, int) and -(2**31) <= r < 2**31 for r in rates) for rates in column.values()), "needs rates"
    comfort = needs["comfort"]
    assert isinstance(comfort, list) and len(comfort) == 5, "needs comfort"
    assert all(type(value) is int and 0 <= value < 2**16 for value in comfort), "needs comfort"
    # The core's first check: each comfort above 150 and at most 1000.
    assert all(COMFORT[0] <= value <= COMFORT[1] for value in comfort), "house.comfort"
    day_checked(day)
    for kind in kinds:
        assert {"id", "slots", "offers"} <= set(kind) <= {"id", "slots", "offers", "bed_for"}, f"kind fields {kind}"
        assert kind.get("bed_for", "couples") in BED_FOR, f"kind bed_for {kind}"
        names = [offer["id"] for offer in kind["offers"]]
        assert len(set(names)) == len(names), "house.offer_duplicate"
        for offer in kind["offers"]:
            assert set(offer) <= OFFER_FIELDS and {"id", "min_age", "max_age"} <= set(offer), f"offer fields {offer}"
            assert set(offer.get("rates", {})) <= set(NEEDS), f"offer rates {offer}"
    kinds = sorted(kinds, key=lambda kind: kind["id"].encode())
    ids = [kind["id"] for kind in kinds]
    assert len(set(ids)) == len(ids), "house.kind_duplicate"
    for kind in kinds:
        assert 1 <= kind["slots"] <= 8, "house.kind_slots"
        for offer in kind["offers"]:
            shape = (offer.get("seconds") is not None, offer.get("until_full") is not None, offer.get("max_seconds") is not None)
            # A sleep has no length of its own: it lasts the night (ADR-0026, addendum of
            # 2026-11-04).
            sleeps = offer.get("sleep", False)
            allowed = ((False, False, False),) if sleeps else ((True, False, False), (False, True, True))
            assert shape in allowed, "house.offer_length"
            if not sleeps:
                length = offer["seconds"] if shape[0] else offer["max_seconds"]
                assert 60 <= length <= 43_200, "house.offer_length"
            assert offer["min_age"] <= offer["max_age"] <= OLDEST, "house.offer_ages"
            ages = range(max(offer["min_age"], AGE_BANDS[0][0]), offer["max_age"] + 1)
            if shape[1]:
                assert all(raises(needs, offer, offer["until_full"], age) for age in ages), "house.offer_full"
            # A time-filler passes the day: no sleep, fun rising at every age with needs it is
            # open to (its addendum on comfort and the time-filler).
            if offer.get("fallback", False):
                assert not sleeps and all(raises(needs, offer, "fun", age) for age in ages), "house.offer_fallback"
                # Its own length, a fixed one (its addendum of 2026-11-05 on the time-filler's
                # length).
                assert "seconds" in offer, "house.fallback_length"
        # A bed only sleeps; only a bed says whom it is for (its addendum after the playtest).
        bed = is_bed(kind)
        sleeps_only = all(offer.get("sleep", False) for offer in kind["offers"])
        assert not (bed and not sleeps_only) and not ("bed_for" in kind and not bed), "house.bed_for"
    by_id = {kind["id"]: kind for kind in kinds}
    numbered = [house_numbered(house) for house in houses]
    numbered.sort(key=lambda house: house["id"].encode())
    house_ids = [house["id"] for house in numbered]
    assert len(set(house_ids)) == len(house_ids), "house.duplicate"
    capacities = [house["capacity"] for house in numbered]
    assert len(set(capacities)) == len(capacities) and all(1 <= c <= 8 for c in capacities), "house.capacity"
    for house in numbered:
        house_checked(needs, by_id, house)
    return needs, day, kinds, numbered


def is_bed(kind: dict) -> bool:
    """A kind with an offer of sleep other than the sofa (ADR-0025 section 3)."""
    return kind["id"] != SOFA and any(offer.get("sleep", False) for offer in kind["offers"])


def house_numbered(house: dict) -> dict:
    """The reader's part (sim-cli): each room once, at most 255, one entrance, every named
    room there; the rooms numbered in the file's order; per room whether one stays there idle,
    true unless it says `dwell: false` (ADR-0026, addendum of 2026-11-04 after the playtest)."""
    assert set(house) == HOUSE_FIELDS, f"house fields {house.get('id')}"
    names = [room["id"] for room in house["rooms"]]
    assert all(set(room) <= {"id", "entrance", "dwell"} for room in house["rooms"]), "room fields"
    assert len(set(names)) == len(names), "house.room_duplicate"
    assert len(names) <= 255, "house.limit"
    entrances = [at for at, room in enumerate(house["rooms"]) if room.get("entrance", False)]
    assert len(entrances) == 1, "house.entrance"

    def room(name, code):
        assert name in names, code
        return names.index(name)

    return {
        "id": house["id"],
        "capacity": house["capacity"],
        "rooms": len(names),
        "entrance": entrances[0],
        "child_day_room": room(house["child_day_room"], "house.child_room"),
        "child_night_room": room(house["child_night_room"], "house.child_room"),
        "dwell_room": room(house["dwell_room"], "house.dwell_room"),
        "dwell": [room.get("dwell", True) for room in house["rooms"]],
        "doors": [(room(d["a"], "house.door_room"), room(d["b"], "house.door_room"), d["seconds"]) for d in house["doors"]],
        "objects": [(o["kind"], room(o["room"], "house.object_room")) for o in house["objects"]],
    }


def house_checked(needs: dict, kinds: dict, house: dict) -> None:
    """The core's part for one numbered house (ADR-0025 section 2)."""
    for a, b, seconds in house["doors"]:
        assert a != b, "house.door_room"
        assert 1 <= seconds <= 120, "house.door_seconds"
    pairs = [(min(a, b), max(a, b)) for a, b, _ in house["doors"]]
    assert len(set(pairs)) == len(pairs), "house.door_duplicate"
    reached, todo = {house["entrance"]}, [house["entrance"]]
    while todo:
        at = todo.pop()
        for a, b, _ in house["doors"]:
            for here, there in ((a, b), (b, a)):
                if here == at and there not in reached:
                    reached.add(there)
                    todo.append(there)
    assert len(reached) == house["rooms"], "house.disconnected"
    # One goes from a room one does not stay in to one one stays in, other than the entrance.
    dwell = house["dwell"]
    assert len(dwell) == house["rooms"] and dwell[house["dwell_room"]], "house.dwell_room"
    assert house["dwell_room"] != house["entrance"], "house.dwell_room"
    objects = []
    for kind, _ in house["objects"]:
        assert kind in kinds, "house.object_kind"
        objects.append(kinds[kind])
    assert len(objects) <= 32 and sum(len(kind["offers"]) for kind in objects) <= 64, "house.limit"
    present = {kind["id"] for kind in objects}
    assert all(essential in present for essential in ESSENTIALS), "house.essentials"
    beds = sum(kind["slots"] for kind in objects if is_bed(kind))
    assert beds >= house["capacity"], "house.sleep_places"
    offers = [offer for kind in objects for offer in kind["offers"]]
    for need in NEEDS:
        for age in range(AGE_BANDS[0][0], OLDEST + 1):
            assert any(raises(needs, offer, need, age) for offer in offers), f"house.need_unserved {house['id']} {need} {age}"


def houses_suffix(files) -> bytes:
    """The postcard that the data hash appends for the houses: `(rates, day, kinds, houses)`
    with `NeedRates { awake, asleep, comfort: [u16; 5] }` (each `BandRates` the five needs in
    order, each four i32; the comfort of ADR-0026, addendum of 2026-11-04 on comfort and the
    time-filler, a fixed array as its five elements), the day as `DayRules { day_plan_at: u32, wake_before: u32, school: SchoolRules {
    from_age: u8, until_age: u8, depart: u32, back: u32 }, work: WorkRules { from_age: u8,
    until_age: u8, full_time_below: u8, part_time_below: u8, full_time: Hours { depart: u32,
    back: u32 }, part_time: Hours }, return_jumps: NeedJumps { hunger: i32, energy: i32,
    bladder: i32, hygiene: i32, fun: i32 }, young: YoungRules { until_age: u8, kita_from: u32,
    kita_to: u32, bed_from: u32, bed_to: u32 }, night: NightRules { from: [u32; 4], until: [u32;
    4], until_weekend: [u32; 4] }, zero: ZeroRules { collapse_energy_rate: i32, stand_up_at: u16, accident_hygiene_at_most:
    u16, hygiene_empty_fun: i32 }, choice_mood_percent: u8, fallback_max_seconds: u32,
    fallback_min_seconds: u32, reserve: ReserveRules { capacity: [u16; 3], drain: Drain { awake:
    [i32; 4], asleep: [i32; 4] }, refill: i32, weak_energy: i32, wasted_energy: i32, walk_factor:
    u32 } }` (ADR-0024 section 4; the reserve at hunger 0 of ADR-0026, addendum of 2026-11-06,
    fixed arrays as their elements; the night of
    ADR-0026, its addendum of 2026-11-04: a fixed array as its four elements, no length; a need
    at 0 and the mood of its addenda of 2026-11-04; the weekend's end of the night of its addendum
    after the playtest; the longest time-filler of its addendum on comfort and the time-filler,
    the least time left for one of its addendum of 2026-11-05 on the time-filler's length),
    the kinds by ID as `KindDef { id, slots: u8, offers: [OfferDef { rates: BTreeMap<Need, i32>,
    seconds: Option<u32>, until_full: Option<Need>, max_seconds: Option<u32>, sleep: bool,
    min_age: u8, max_age: u8, fallback: bool }], bed_for: Option<BedFor> }` (an
    offer's `id` is a name, not hashed; `BedFor` the index in `BED_FOR`), and the houses by ID
    as `HouseDef { id, capacity: u8, rooms: u8, entrance: u8, child_day_room: u8,
    child_night_room: u8, dwell_room: u8, dwell: [bool], doors: [DoorDef { a: u8, b: u8,
    seconds: u32 }], objects: [ObjectDef { kind, room: u8 }] }` (rooms are numbers, their names
    are not hashed). Empty for data without houses (None)."""
    if files is None:
        return b""
    needs, day, kinds, houses = houses_checked(*files)
    out = b""
    for column in ("awake", "asleep"):
        for need in NEEDS:
            out += b"".join(zigzag(rate) for rate in needs[column][need])
    out += b"".join(varint(value) for value in needs["comfort"])
    school, work, jumps, young = day["school"], day["work"], day["return_jumps"], day["young"]
    out += varint(day["day_plan_at"]) + varint(day["wake_before"])
    out += u8(school["from_age"]) + u8(school["until_age"]) + varint(school["depart"]) + varint(school["back"])
    out += u8(work["from_age"]) + u8(work["until_age"]) + u8(work["full_time_below"]) + u8(work["part_time_below"])
    out += b"".join(varint(work[hours][end]) for hours in ("full_time", "part_time") for end in ("depart", "back"))
    out += b"".join(zigzag(jumps[need]) for need in NEEDS)
    out += u8(young["until_age"]) + b"".join(varint(young[time]) for time in ("kita_from", "kita_to", "bed_from", "bed_to"))
    out += b"".join(varint(time) for bound in ("from", "until", "until_weekend") for time in day["night"][bound])
    zero = day["zero"]
    out += zigzag(zero["collapse_energy_rate"]) + varint(zero["stand_up_at"])
    out += varint(zero["accident_hygiene_at_most"]) + zigzag(zero["hygiene_empty_fun"])
    out += u8(day["choice_mood_percent"]) + varint(day["fallback_max_seconds"])
    out += varint(day["fallback_min_seconds"])
    reserve = day["reserve"]
    out += b"".join(varint(capacity) for capacity in reserve["capacity"])
    out += b"".join(zigzag(rate) for column in ("awake", "asleep") for rate in reserve["drain"][column])
    out += zigzag(reserve["refill"]) + zigzag(reserve["weak_energy"]) + zigzag(reserve["wasted_energy"])
    out += varint(reserve["walk_factor"])
    out += varint(len(kinds))
    for kind in kinds:
        out += string(kind["id"]) + u8(kind["slots"]) + varint(len(kind["offers"]))
        for offer in kind["offers"]:
            rates = sorted(offer.get("rates", {}).items(), key=lambda pair: NEEDS.index(pair[0]))
            out += varint(len(rates)) + b"".join(varint(NEEDS.index(need)) + zigzag(rate) for need, rate in rates)
            out += option(offer.get("seconds"), varint)
            out += option(offer.get("until_full"), lambda need: varint(NEEDS.index(need)))
            out += option(offer.get("max_seconds"), varint)
            out += boolean(offer.get("sleep", False)) + u8(offer["min_age"]) + u8(offer["max_age"])
            out += boolean(offer.get("fallback", False))
        out += option(kind.get("bed_for"), lambda sleepers: varint(BED_FOR.index(sleepers)))
    out += varint(len(houses))
    for house in houses:
        out += string(house["id"]) + u8(house["capacity"]) + u8(house["rooms"]) + u8(house["entrance"])
        out += u8(house["child_day_room"]) + u8(house["child_night_room"])
        out += u8(house["dwell_room"]) + varint(len(house["dwell"])) + b"".join(boolean(dwells) for dwells in house["dwell"])
        out += varint(len(house["doors"])) + b"".join(u8(a) + u8(b) + varint(s) for a, b, s in house["doors"])
        out += varint(len(house["objects"])) + b"".join(string(kind) + u8(room) for kind, room in house["objects"])
    return out

# ---------------------------------------------------------------------------------------
# The repository's pool (slice S5): data/pool/pool.bin and data/pool/pool_frequencies.json.

POOL_MAGIC = b"SIMPOOL"


def read_varint(data: bytes, at: int) -> tuple:
    """(value, next position) of the unsigned varint at `at`."""
    value, shift = 0, 0
    while True:
        byte = data[at]
        at += 1
        value |= (byte & 0x7F) << shift
        if not byte & 0x80:
            return value, at
        shift += 7


def pool_head(pool: bytes) -> dict:
    """The first fields of a pool file (tools/pool_reference.py `encode_pool`, format 2):
    the magic `SIMPOOL`, the version, then `pool_seed`, `frozen_at` (zigzag), `data_hash`,
    `frozen_hash`. Only what the data hash needs; the reader's rules are
    `pool_reference.read_pool`'s (tools/world_reference.py `DataPool` runs them)."""
    assert pool[: len(POOL_MAGIC)] == POOL_MAGIC, "magic"
    version, at = read_varint(pool, len(POOL_MAGIC))
    assert version == 2, f"version {version}"
    seed, at = read_varint(pool, at)
    zigzag, at = read_varint(pool, at)
    frozen_at = (zigzag >> 1) ^ -(zigzag & 1)
    run_hash, at = read_varint(pool, at)
    frozen_hash, at = read_varint(pool, at)
    return {"version": version, "pool_seed": seed, "frozen_at": frozen_at, "data_hash": run_hash, "frozen_hash": frozen_hash}


def pool_files(data_dir: Path):
    """The pool of a data directory: (pool bytes, frequencies {locus: q}) from
    `data_dir/pool/pool.bin` and `data_dir/pool/pool_frequencies.json`, or None without a
    `pool/` (a pool without its frequencies, or the other way round, is an error: they travel
    together)."""
    folder = Path(data_dir) / "pool"
    pool_path, frequencies_path = folder / "pool.bin", folder / "pool_frequencies.json"
    if not pool_path.exists() and not frequencies_path.exists():
        return None
    assert pool_path.exists() and frequencies_path.exists(), f"{folder}: the pool and its frequencies travel together"
    return pool_path.read_bytes(), frequencies_file(frequencies_path)


def repository_hash(data_dir: Path) -> int:
    """The data hash of a data directory as the CLI builds its data: the definitions, with a
    `pool/` followed by `(pool_seed, pool_hash, frequencies)` (`pool_suffix`, the frequencies
    checked by `frequencies_rejection`), with a `shocks.json` followed by its templates
    (`shocks_suffix`, ADR-0023 section 3); xxh3-64 with seed 0 of the concatenation. Without
    either it is the definitions alone (`data_hash`); with a `house/` followed by the houses
    (`houses_suffix`, ADR-0025)."""
    files = pool_files(data_dir)
    out = definitions_bytes(*game_documents(data_dir))
    if files is not None:
        pool, frequencies = files
        assert frequencies_rejection(frequencies) is None, frequencies_rejection(frequencies)
        head = pool_head(pool)
        out += pool_suffix(head["pool_seed"], xxhash.xxh3_64_intdigest(pool, seed=0), frequencies)
    out += shocks_suffix(shock_templates(data_dir))
    out += houses_suffix(house_files(data_dir))
    return xxhash.xxh3_64_intdigest(out, seed=0)


def main(argv):
    if "--pool" in argv:
        # The data with a pool (S2a): the definitions of DATA_DIR, the pool file and its
        # frequencies file, every rule of `Data::with_pool` (tools/world_reference.py).
        import world_reference

        at = argv.index("--pool")
        pool_path, frequencies_path = Path(argv[at + 1]), Path(argv[argv.index("--frequencies") + 1])
        rest = [arg for i, arg in enumerate(argv[1:], 1) if i not in (at, at + 1, argv.index("--frequencies"), argv.index("--frequencies") + 1)]
        data_dir = Path(rest[0]) if rest else REPOSITORY_DATA
        frequencies = frequencies_file(frequencies_path)
        data = world_reference.Data(*documents(data_dir), pool=pool_path.read_bytes(), frequencies=frequencies)
        print(f"definitions_hash {data_hash(data_dir):016x}")
        print(
            f"data_hash_with_pool {data.hash:016x} pool_seed {data.pool.seed:016x} pool_hash {data.pool.hash:016x}"
            f" frozen_hash {data.pool.frozen_hash:016x} suffix {pool_suffix(data.pool.seed, data.pool.hash, frequencies).hex()}"
        )
        return
    data_dir = Path(argv[1]) if len(argv) > 1 else REPOSITORY_DATA
    files = pool_files(data_dir)
    if files is None:
        # Without a pool: the definitions, followed by the templates if there are any.
        print(f"data_hash {repository_hash(data_dir):016x}")
    else:
        # The repository's data (S5): the definitions, then the data with the pool and its
        # frequencies as the CLI builds them.
        pool, frequencies = files
        head = pool_head(pool)
        pool_hash = xxhash.xxh3_64_intdigest(pool, seed=0)
        definitions = data_hash(data_dir)
        print(f"definitions_hash {definitions:016x} (the pool's frozen_hash {head['frozen_hash']:016x}: {'equal' if definitions == head['frozen_hash'] else 'DIFFERENT'})")
        print(
            f"data_hash {repository_hash(data_dir):016x} pool_seed {head['pool_seed']:016x} pool_hash {pool_hash:016x} pool_bytes {len(pool)}"
            f" frozen_at {head['frozen_at']} run_data_hash {head['data_hash']:016x}"
            f" suffix {pool_suffix(head['pool_seed'], pool_hash, frequencies).hex()}"
        )
        print(f"frequencies rules {frequencies_rejection(frequencies) or 'passed'} " + " ".join(f"{locus}={q!r}" for locus, q in sorted(frequencies.items())))
        # The game's overlay (ADR-0017, addendum of 2026-11-05): the definitions above are the
        # extract's (the pool's frozen_hash); the data hash covers the overlaid ones.
        overlay = game_overlay(data_dir)
        if overlay is None:
            print("game overlay none")
        else:
            game = game_documents(data_dir)
            listed = " ".join(f"{key}={entry['value']!r}" for key, entry in sorted(overlay["values"].items()))
            print(f"game overlay {listed} extra_child {extra_child(game[0]['values'])} game_definitions_hash {documents_hash(*game):016x}")
        # The shock templates (ADR-0023 section 3): the data hash above ends with them; the
        # hash of the definitions and the pool without them, for comparison.
        templates = shock_templates(data_dir)
        definitions_and_pool = definitions_bytes(*game_documents(data_dir)) + pool_suffix(head["pool_seed"], pool_hash, frequencies)
        listed = " ".join(
            "{}:{}:{}:{!r}".format(
                t["id"], t["kind"], t.get("years", t.get("months")), t.get("index", t.get("rate"))
            )
            for t in templates or []
        )
        print(
            f"shocks {listed or 'none'} suffix {shocks_suffix(templates).hex()}"
            f" data_hash_without_shocks {xxhash.xxh3_64_intdigest(definitions_and_pool, seed=0):016x}"
            f" data_hash {repository_hash(data_dir):016x} ({repository_hash(data_dir)})"
        )
        # The houses (ADR-0025): the data hash above ends with them; the hash without them.
        files = house_files(data_dir)
        listed = " ".join(f"{h['id']}:{h['capacity']}" for h in houses_checked(*files)[3]) if files else "none"
        without = definitions_and_pool + shocks_suffix(templates)
        print(
            f"houses {listed} suffix_bytes {len(houses_suffix(files))}"
            f" data_hash_without_houses {xxhash.xxh3_64_intdigest(without, seed=0):016x}"
            f" data_hash {repository_hash(data_dir):016x}"
        )
    # The threshold of each threshold disease that names its trait of liability (none in the
    # repository's data, so their output stays one line).
    _, calibration, _ = documents(data_dir)
    for code, trait, heritability, prevalence, value in liability_diseases(calibration):
        print(
            f"threshold {code} liability {trait} heritability {heritability!r} prevalence {prevalence!r}"
            f" 1-K bits {f64_bits(1.0 - prevalence)} T {value!r} bits {f64_bits(value)}"
        )


if __name__ == "__main__":
    main(sys.argv)

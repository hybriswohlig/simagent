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
                          fertility_wish_shape: Vec<Calibrated> }   (ADR-0017 section 1)
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
                       prevalence: Calibrated }   (an option: 0 for none, 1 then the value)
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
  Trait { id: DataId, sigma_ref: f64, exclusive: bool, includes: Vec<Load> }
  Load { trait: DataId, weight: f64 }
  Locus { id: DataId, chromosome: Chromosome, segment: u8, slot: u8, action: Action,
          mark: Mark, mutation: MutationDirection }
  Chromosome: autosome, x, y, mt
  Action: additive(Weights), recessive, dominant, threshold(Weights)
  Weights { effect: f64, d_ref: f64, loads: Vec<Load> }
  Mark: small, large, disease
  MutationDirection: both, to_alt, to_ref

Usage: python3 tools/data_reference.py [DATA_DIR]
"""

import json
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
    for entry in list(demography["values"].values()) + demography["fertility_wish_shape"]:
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
    file order."""
    values = doc["values"]
    out = varint(doc["format_version"]) + varint(len(values))
    for key in sorted(values):
        out += string(key) + calibrated_bytes(values[key])
    out += varint(len(doc["fertility_wish_shape"]))
    for entry in doc["fertility_wish_shape"]:
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


def genome_map_bytes(doc) -> bytes:
    out = varint(doc["format_version"]) + varint(doc["map_version"])
    out += varint(len(doc["traits"]))
    for entry in doc["traits"]:
        out += string(entry["id"]) + f64(float(entry["sigma_ref"]))
        out += boolean(entry["exclusive"]) + loads_bytes(entry["includes"])
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


def documents_hash(demography, calibration, genome_map) -> int:
    """The data hash of the life course data, a calibration and a genome map given as
    parsed JSON (test data built in memory, or the files): the text fields checked, then
    stripped, as the core sees them."""
    check_calibration_format(calibration)
    check_text(demography, calibration, genome_map)
    check_named_traits(calibration, genome_map)
    demography, calibration, genome_map = strip(demography), strip(calibration), strip(genome_map)
    return xxhash.xxh3_64_intdigest(
        demography_bytes(demography) + calibration_bytes(calibration) + genome_map_bytes(genome_map),
        seed=0,
    )


def data_hash(data_dir: Path) -> int:
    return documents_hash(*documents(data_dir))


def main(argv):
    data_dir = Path(argv[1]) if len(argv) > 1 else REPOSITORY_DATA
    print(f"data_hash {data_hash(data_dir):016x}")


if __name__ == "__main__":
    main(sys.argv)

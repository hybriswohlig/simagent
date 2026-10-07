#!/usr/bin/env python3
"""Reference data hash of the definitions under data/ (ADR-0007), independent of the Rust code.

The core sees each file without its text fields (`source`, `as_of`, `note`, `description`) and
hashes the postcard bytes of the two definition sets in the order of their data IDs:
`core:calibration_genetics`, then `core:genome_map`, with xxh3-64 and seed 0 over the
concatenation (ADR-0003 StableHasher, ADR-0004 section 2).

Layout of the core types (postcard: structs as their fields in order, enums as the variant
index as varint, strings and sequences with a varint length, maps as a varint length and the
pairs in key order, u8 as one raw byte, u32 as varint, f64 as eight bytes little endian):

  GeneticsCalibration { format_version: u32, values: BTreeMap<DataId, Calibrated> }
  Calibrated { value: f64, unit: Unit }
  Unit: morgan, per_transmission
  GenomeMap { format_version: u32, map_version: u32, loci: Vec<Locus> }
  Locus { id: DataId, chromosome: Chromosome, segment: u8, slot: u8, action: Action,
          mark: Mark, mutation: MutationDirection }
  Chromosome: autosome, x, y, mt
  Action: additive, recessive, dominant, threshold
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
UNITS = ["morgan", "per_transmission"]
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


def strip(value):
    """The file without its text fields, which every place that has them must fill."""
    if isinstance(value, dict):
        return {k: strip(v) for k, v in value.items() if k not in TEXT_FIELDS}
    if isinstance(value, list):
        return [strip(v) for v in value]
    return value


def calibration_bytes(doc) -> bytes:
    values = doc["values"]
    out = varint(doc["format_version"]) + varint(len(values))
    for key in sorted(values):
        entry = values[key]
        out += string(key) + f64(float(entry["value"])) + varint(UNITS.index(entry["unit"]))
    return out


def genome_map_bytes(doc) -> bytes:
    out = varint(doc["format_version"]) + varint(doc["map_version"]) + varint(len(doc["loci"]))
    for locus in doc["loci"]:
        out += string(locus["id"])
        out += varint(CHROMOSOMES.index(locus["chromosome"]))
        out += u8(locus["segment"]) + u8(locus["slot"])
        out += varint(ACTIONS.index(locus["action"]))
        out += varint(MARKS.index(locus["mark"]))
        out += varint(DIRECTIONS.index(locus["mutation"]))
    return out


def data_hash(data_dir: Path) -> int:
    calibration = strip(json.loads((data_dir / "calibration" / "genetics.json").read_text()))
    genome_map = strip(json.loads((data_dir / "genetics" / "genome_map.json").read_text()))
    return xxhash.xxh3_64_intdigest(calibration_bytes(calibration) + genome_map_bytes(genome_map), seed=0)


def main(argv):
    data_dir = Path(argv[1]) if len(argv) > 1 else Path(__file__).resolve().parent.parent / "data"
    print(f"data_hash {data_hash(data_dir):016x}")


if __name__ == "__main__":
    main(sys.argv)

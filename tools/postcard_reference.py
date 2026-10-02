#!/usr/bin/env python3
"""Reference bytes for the postcard pins in crates/sim-core/tests/formats.rs.

Written from the postcard wire format specification (https://postcard.jamesmunns.com/
wire-format), independently of the Rust code: unsigned integers are LEB128 varints,
signed integers are zigzag encoded first, strings and sequences carry a varint length,
an enum variant is its index as a varint followed by its fields, a struct is its fields
in declaration order, a float is its eight bytes in little endian. No dependencies.

Usage: python3 tools/postcard_reference.py
"""


import struct


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


def signed(value: int) -> bytes:
    """i64: zigzag, then varint."""
    return varint((value << 1) ^ (value >> 63) if value >= 0 else ((-value) << 1) - 1)


def string(text: str) -> bytes:
    data = text.encode("utf-8")
    return varint(len(data)) + data


def seq(items: list) -> bytes:
    return varint(len(items)) + b"".join(items)


def variant(index: int, *fields: bytes) -> bytes:
    return varint(index) + b"".join(fields)


# The genome of the Durchstich: enums without fields are their variant index.
# Sex: female = 0, male = 1. Allele: wild = 0, dummy = 1.
FEMALE, MALE = variant(0), variant(1)
WILD, DUMMY = variant(0), variant(1)


def genotype(maternal: bytes, paternal: bytes) -> bytes:
    """Genotype { maternal: Allele, paternal: Allele }"""
    return maternal + paternal


def lineage_key(key: int) -> bytes:
    """LineageKey(u64): the plain number, like every full-range u64."""
    return varint(key)


def flag(on: bool) -> bytes:
    """A bool is one byte, 0 or 1."""
    return b"\x01" if on else b"\x00"


def world_params(hereditary_diseases: bool) -> bytes:
    """WorldParams { hereditary_diseases: bool }"""
    return flag(hereditary_diseases)


def param_change(hereditary_diseases: bool) -> bytes:
    """ParamChange::HereditaryDiseases(bool), variant 0"""
    return variant(0, flag(hereditary_diseases))


def f64(value: float) -> bytes:
    """A float is eight bytes, little endian."""
    return struct.pack("<d", value)


def death_cause(code: str, per_year: float) -> bytes:
    """DeathCause { cause: Code, per_year: FiniteF64 }"""
    return string(code) + f64(per_year)


def founder(born: int, sex: bytes, genes: bytes) -> bytes:
    """Founder { born: SimTime, sex: Sex, genotype: Genotype }"""
    return signed(born) + sex + genes


# Input::Start (variant 0) { world_seed: u64, founder_tag: DataId, world_params:
# WorldParams, death_causes: [DeathCause], founders: [Founder] }
SAMPLE_INPUT = variant(
    0,
    varint(0xFF),
    string("test:fork_no_density"),
    world_params(False),
    seq([death_cause("test:base", 0.02)]),
    seq([founder(-946_080_000, FEMALE, genotype(WILD, DUMMY))]),
)

# Cause::Random (variant 4) { tag, subject, occasion, detail }: four plain u64.
SAMPLE_RANDOM_CAUSE = variant(
    4, varint(4), varint(0xCFD3D4551FB363C2), varint(0x0123456789ABCDEF), varint(0)
)

# LogPayload::Death (variant 4) { cause: Code }
SAMPLE_DEATH_PAYLOAD = variant(4, string("test:base"))

# Input::ForkOverride (variant 1) { changes: [ParamChange] }: the switch goes on.
SAMPLE_OVERRIDE = variant(1, seq([param_change(True)]))

# InputStamp { t: SimTime, seq: InputSeq }
SAMPLE_STAMP = signed(5) + varint(1)

# InputRecord { t: SimTime, seq: InputSeq, input: Input }
SAMPLE_RECORD = signed(-1) + varint(1) + SAMPLE_INPUT

# LogEntry { id, t, subjects, causes, payload }; Subject::Sim = 0;
# Cause: Event = 0, Plan = 1 { by, planned_at }, Input = 2, Rule = 3; LogPayload::Birthday = 2
SAMPLE_ENTRY = (
    varint(7)
    + signed(86_400)
    + seq([variant(0, varint(42))])
    + seq(
        [
            variant(1, varint(3), signed(0)),
            variant(3, string("core:gompertz_hazard")),
            variant(0, varint(5)),
            variant(2, varint(1)),
        ]
    )
    + variant(2, varint(1))
)

# InputStamp with a negative time and the largest sequence number.
NEGATIVE_STAMP = signed(-5) + varint((1 << 53) - 1)

# InputRejected { code: RejectCode::AlreadyStarted (variant 0) }
REJECTED = variant(0)

# SimDuration at its lower bound, -1 000 000 years of 364 days.
MIN_DURATION = signed(-1_000_000 * 364 * 86_400)

# The largest ID and the full-range seed, to pin the varint widths.
MAX_ID = varint((1 << 53) - 1)
MAX_U64 = varint((1 << 64) - 1)

# The first founder of `test:fork_no_density` (ADR-0001 section 4): the value that
# tools/rng_reference.py prints first under `founders`.
SAMPLE_LINEAGE_KEY = lineage_key(0xCFD3D4551FB363C2)

if __name__ == "__main__":
    for name, data in [
        ("sample_input", SAMPLE_INPUT),
        ("sample_override", SAMPLE_OVERRIDE),
        ("sample_random_cause", SAMPLE_RANDOM_CAUSE),
        ("sample_death_payload", SAMPLE_DEATH_PAYLOAD),
        ("death_cause", death_cause("test:base", 0.02)),
        ("sample_stamp", SAMPLE_STAMP),
        ("sample_record", SAMPLE_RECORD),
        ("sample_entry", SAMPLE_ENTRY),
        ("negative_stamp", NEGATIVE_STAMP),
        ("rejected", REJECTED),
        ("min_duration", MIN_DURATION),
        ("max_id", MAX_ID),
        ("max_u64", MAX_U64),
        ("genotype_wild_dummy", genotype(WILD, DUMMY)),
        ("genotype_dummy_wild", genotype(DUMMY, WILD)),
        ("sample_lineage_key", SAMPLE_LINEAGE_KEY),
    ]:
        print(f"{name} {data.hex()}")

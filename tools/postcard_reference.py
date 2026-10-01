#!/usr/bin/env python3
"""Reference bytes for the postcard pins in crates/sim-core/tests/formats.rs.

Written from the postcard wire format specification (https://postcard.jamesmunns.com/
wire-format), independently of the Rust code: unsigned integers are LEB128 varints,
signed integers are zigzag encoded first, strings and sequences carry a varint length,
an enum variant is its index as a varint followed by its fields, a struct is its fields
in declaration order. No dependencies.

Usage: python3 tools/postcard_reference.py
"""


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


# Input::Start (variant 0) { world_seed: u64, founder_tag: DataId, founders: [Founder { born: SimTime }] }
SAMPLE_INPUT = variant(
    0,
    varint(0xFF),
    string("test:fork_no_density"),
    seq([signed(-946_080_000)]),
)

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

if __name__ == "__main__":
    for name, data in [
        ("sample_input", SAMPLE_INPUT),
        ("sample_stamp", SAMPLE_STAMP),
        ("sample_record", SAMPLE_RECORD),
        ("sample_entry", SAMPLE_ENTRY),
        ("negative_stamp", NEGATIVE_STAMP),
        ("rejected", REJECTED),
        ("min_duration", MIN_DURATION),
        ("max_id", MAX_ID),
        ("max_u64", MAX_U64),
    ]:
        print(f"{name} {data.hex()}")

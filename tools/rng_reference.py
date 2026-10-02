#!/usr/bin/env python3
"""Reference for the addressed random draws of sim-core (ADR-0001 sections 1 and 2),
written from the ADR alone, without a look at the Rust code.

- mix64: the SplitMix64 finaliser.
- absorb(domain, lane, words): h = mix64(domain ^ ((lane + 1) * 0x9e3779b97f4a7c15)),
  then h = mix64(h ^ w) per word. Domains are 8 ASCII bytes read as little-endian u64.
- ChaCha key of a draw: for lane 0 to 3, absorb(DRAW, lane, [seed, tag, subject,
  occasion, detail]) as 8 little-endian bytes each, 32 bytes in all.
- Words: the key stream of ChaCha with 8 rounds, 256-bit key, 64-bit block counter 0
  and 64-bit stream 0 (the original ChaCha layout), block 0, as 8 little-endian u64.

The ChaCha8 here is an own implementation and is checked against test vector TC1 of
draft-strombergson-chacha-test-vectors-01 (zero key, zero IV, 8 rounds) on every run.

Usage: .venv/bin/python tools/rng_reference.py   (xxhash only for the str_key example)
"""

import struct

import xxhash

MASK = (1 << 64) - 1
GOLDEN = 0x9E3779B97F4A7C15


def mix64(z: int) -> int:
    z &= MASK
    z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK
    z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK
    return z ^ (z >> 31)


def domain(name: bytes) -> int:
    assert len(name) == 8
    return int.from_bytes(name, "little")


DRAW = domain(b"sa.draw1")
RANK = domain(b"sa.rank1")
CHILD = domain(b"sa.chld1")
FOUNDER = domain(b"sa.fndr1")
IMMIGRANT = domain(b"sa.immi1")


def absorb(dom: int, lane: int, words: list) -> int:
    h = mix64(dom ^ (((lane + 1) * GOLDEN) & MASK))
    for word in words:
        h = mix64(h ^ word)
    return h


def chacha_key(seed: int, tag: int, subject: int, occasion: int, detail: int) -> bytes:
    lanes = [absorb(DRAW, lane, [seed, tag, subject, occasion, detail]) for lane in range(4)]
    return b"".join(lane.to_bytes(8, "little") for lane in lanes)


def rotl32(value: int, count: int) -> int:
    return ((value << count) | (value >> (32 - count))) & 0xFFFFFFFF


def quarter_round(state: list, a: int, b: int, c: int, d: int) -> None:
    state[a] = (state[a] + state[b]) & 0xFFFFFFFF
    state[d] = rotl32(state[d] ^ state[a], 16)
    state[c] = (state[c] + state[d]) & 0xFFFFFFFF
    state[b] = rotl32(state[b] ^ state[c], 12)
    state[a] = (state[a] + state[b]) & 0xFFFFFFFF
    state[d] = rotl32(state[d] ^ state[a], 8)
    state[c] = (state[c] + state[d]) & 0xFFFFFFFF
    state[b] = rotl32(state[b] ^ state[c], 7)


def chacha8_block(key: bytes, counter: int = 0, stream: int = 0) -> bytes:
    """One 64-byte block of ChaCha with 8 rounds (4 double rounds)."""
    assert len(key) == 32
    constants = struct.unpack("<4I", b"expand 32-byte k")
    initial = list(constants) + list(struct.unpack("<8I", key))
    initial += [counter & 0xFFFFFFFF, counter >> 32, stream & 0xFFFFFFFF, stream >> 32]
    state = list(initial)
    for _ in range(4):
        quarter_round(state, 0, 4, 8, 12)
        quarter_round(state, 1, 5, 9, 13)
        quarter_round(state, 2, 6, 10, 14)
        quarter_round(state, 3, 7, 11, 15)
        quarter_round(state, 0, 5, 10, 15)
        quarter_round(state, 1, 6, 11, 12)
        quarter_round(state, 2, 7, 8, 13)
        quarter_round(state, 3, 4, 9, 14)
    out = [(word + start) & 0xFFFFFFFF for word, start in zip(state, initial)]
    return struct.pack("<16I", *out)


def words(key: bytes) -> list:
    return list(struct.unpack("<8Q", chacha8_block(key)))


# TC1, 256-bit key, 8 rounds, key stream block 0, copied from the draft.
TC1 = bytes.fromhex(
    "3e00ef2f895f40d6" "7f5bb8e81f09a5a1" "2c840ec3ce9a7f3b" "181be188ef711a1e"
    "984ce172b9216f41" "9f445367456d5619" "314a42a3da86b001" "387bfdb80e0cfe42"
)
assert chacha8_block(bytes(32)) == TC1, "the ChaCha8 of this script fails TC1"

# SplitMix64 (Vigna) with start state 0: the first three outputs.
assert [mix64(k * GOLDEN) for k in (1, 2, 3)] == [
    0xE220A8397B1DCDAF,
    0x6E789E6AA1B965F4,
    0x06C45D188009454F,
]


def str_key(text: str) -> int:
    return xxhash.xxh3_64_intdigest(text.encode("utf-8"), seed=0)


# (seed, tag, subject, occasion, detail)
DRAW_KEYS = [
    (0, 1, 0, 0, 0),
    (1, 1, 0, 0, 0),
    (0, 2, 0, 0, 0),
    (0x0123456789ABCDEF, 3, 0xFEDCBA9876543210, 7, (5 << 32) | 9),
    (MASK, 0xFFFFFFFF, MASK, MASK, MASK),
    (0x5EED, 4, 0, str_key("test:base"), 0),
]

if __name__ == "__main__":
    print("tc1_words", " ".join(f"{word:016x}" for word in words(bytes(32))))
    for seed, tag, subject, occasion, detail in DRAW_KEYS:
        key = chacha_key(seed, tag, subject, occasion, detail)
        print(f"draw seed={seed:016x} tag={tag:016x} subject={subject:016x} occasion={occasion:016x} detail={detail:016x}")
        print("  key  ", key.hex())
        print("  words", " ".join(f"{word:016x}" for word in words(key)))
    print("absorb_founder", f"{absorb(FOUNDER, 0, [str_key('test:fork_no_density'), 0]):016x}")
    print("absorb_rank", f"{absorb(RANK, 0, [7, 5, 11, 13, 17]):016x}")

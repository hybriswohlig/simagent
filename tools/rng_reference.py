#!/usr/bin/env python3
"""Reference for the addressed random draws of sim-core (ADR-0001 sections 1 to 3),
written from the ADR alone, without a look at the Rust code.

- mix64: the SplitMix64 finaliser.
- absorb(domain, lane, words): h = mix64(domain ^ ((lane + 1) * 0x9e3779b97f4a7c15)),
  then h = mix64(h ^ w) per word. Domains are 8 ASCII bytes read as little-endian u64.
- ChaCha key of a draw: for lane 0 to 3, absorb(DRAW, lane, [seed, tag, subject,
  occasion, detail]) as 8 little-endian bytes each, 32 bytes in all.
- Words: the key stream of ChaCha with 8 rounds, 256-bit key, 64-bit block counter 0
  and 64-bit stream 0 (the original ChaCha layout), block 0, as 8 little-endian u64.
- Distributions (section 3): unit, unit_open0, uniform_below, bernoulli and poisson are
  class E (integer and power-of-two arithmetic, exact here). exponential and normal are
  class P: every basic step is done in f64 as the ADR writes it, only ln, sqrt and cos
  come from mpmath (dps = 60, rounded to f64 by kat_reference.round_to_f64). The bits
  recorded by `cargo xtask kat-record` may differ from this reference by 2^-48 relative
  (exponential) and 2^-45 absolute (normal with mean 0, sd 1).

The ChaCha8 here is an own implementation and is checked against test vector TC1 of
draft-strombergson-chacha-test-vectors-01 (zero key, zero IV, 8 rounds) on every run.

Usage: .venv/bin/python tools/rng_reference.py           (print the draw values, write
                                                          the files of the distributions)
       .venv/bin/python tools/rng_reference.py --check   (compare, write nothing)
"""

import struct

import sys

import mpmath
import xxhash

import kat_reference
from kat_reference import bits, cos_reference, ln_reference, sqrt_reference, value_bits

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

# ---------------------------------------------------------------------------------------
# Distributions (ADR-0001 section 3)

TWO_POW_MINUS_53 = 2.0**-53
TAU = 6.283185307179586

# The words every distribution is tested at, and some in between.
EDGE_WORDS = [0, 1, 2**11 - 1, 2**11, 2**63, MASK]
MID_WORDS = [0x1999999999999999, 0x0123456789ABCDEF, 0xE666666666666666, 0xFEDCBA9876543210]
WORDS = EDGE_WORDS + MID_WORDS


def unit(w: int) -> float:
    """b = w >> 11 is below 2^53 and so exact as f64; times 2^-53 is exact. In [0, 1)."""
    return float(w >> 11) * TWO_POW_MINUS_53


def unit_open0(w: int) -> float:
    """As unit with b + 1: in (0, 1]."""
    return float((w >> 11) + 1) * TWO_POW_MINUS_53


def uniform_below(w: int, n: int) -> int:
    """The upper 64 bits of the 128-bit product."""
    assert n >= 1
    return (w * n) >> 64


def bernoulli(w: int, p: float) -> bool:
    assert 0.0 <= p <= 1.0
    return unit(w) < p


def exponential_reference(w: int, rate: float):
    """(class, value) of (0.0 - ln(unit_open0(w))) / rate. Exact where ln is (u = 1)."""
    kind, log_u = ln_reference(unit_open0(w))
    return kind, (0.0 - log_u) / rate


def normal_reference(w0: int, w1: int, mean: float, sd: float):
    """(class, value) of mean + sd * (sqrt(0.0 - 2.0 * ln(unit_open0(w0))) * cos(TAU *
    unit(w1))). Exact where the product is: sd = 0, or a radius of zero (u0 = 1)."""
    kind_ln, log_u = ln_reference(unit_open0(w0))
    _, radius = sqrt_reference(0.0 - 2.0 * log_u)
    _, cosine = cos_reference(TAU * unit(w1))
    value = mean + sd * (radius * cosine)
    exact = sd == 0.0 or kind_ln == "E"
    return ("E" if exact else "P"), value


POISSON_MARGIN = mpmath.mpf("1e-9")


def poisson_reference(w: int, lam: float) -> int:
    """Inversion with exact u and a 60-digit distribution function: the smallest k with
    u < F(k). Class E holds only where u keeps at least 1e-9 from every threshold F(k),
    so that the rounding of the f64 sum cannot move the result; lambda = 0 needs no
    margin below the single threshold 1 because exp(0) = 1 is exact."""
    assert 0.0 <= lam <= 30.0
    u = mpmath.mpf(w >> 11) / mpmath.mpf(2) ** 53
    lam_mp = mpmath.mpf(lam)
    term = mpmath.exp(-lam_mp)
    total = mpmath.mpf(0)
    result = None
    for k in range(256):
        total += term
        assert abs(u - total) >= POISSON_MARGIN, (
            f"u of word {w:016x} is within 1e-9 of F({k}) for lambda {lam}"
        )
        if result is None and u < total:
            result = k
        if result is not None and k > result + 4 and mpmath.mpf(1) - total < POISSON_MARGIN:
            # Every later threshold lies between this one and 1.
            assert mpmath.mpf(1) - u >= POISSON_MARGIN + (mpmath.mpf(1) - total)
            break
        term = term * lam_mp / (k + 1)
    assert result is not None
    return result


# (lambda, words): each word keeps the margin, the assertion above checks it.
POISSON_CASES = [
    (0.0, [0, 1, 2**11 - 1, 2**11, 2**63] + MID_WORDS),
    (0.5, [0, 1, 2**11 - 1, 2**11, 2**63] + MID_WORDS),
    (5.0, [0, 1, 2**11 - 1, 2**11, 2**63] + MID_WORDS),
    (30.0, [2**63] + MID_WORDS),
]

UNIFORM_N = [1, 7, 2**40 + 3, MASK]
BERNOULLI_P = [0.0, 1.0, 0.3, 0.5, TWO_POW_MINUS_53]
EXPONENTIAL_RATES = [2.0, 1.0, 0.02, 0.3]
# (w0, w1) at mean 0 and sd 1.
NORMAL_PAIRS = [
    (0, 0), (0, MASK), (MASK, 0), (MASK, MASK), (1, 2**11), (2**11 - 1, 2**63),
    (2**63, 2**63), (2**63, 2**62), (2**63, 3 * 2**62),
    (0x1999999999999999, 0x0123456789ABCDEF), (0xE666666666666666, 0xFEDCBA9876543210),
    (0x0123456789ABCDEF, 0x1999999999999999), (0xFEDCBA9876543210, 0xE666666666666666),
]


def class_p_row(name: str, inputs: str, kind: str, value: float, known: dict) -> str:
    ref = value_bits(value)
    expected = ref if kind == "E" else known.get((inputs, ref), kat_reference.UNRECORDED)
    return f"{inputs},{expected},{kind},{ref}"


def write(name: str, header: str, rows: list) -> None:
    kat_reference.write(name, header, rows, tool="tools/rng_reference.py")


def write_distributions() -> None:
    header = "input_bits,expected_bits,class,reference_bits"
    write(
        "unit", header, [f"{w:016x},{bits(unit(w))},E,{bits(unit(w))}" for w in WORDS]
    )
    write(
        "unit_open0",
        header,
        [f"{w:016x},{bits(unit_open0(w))},E,{bits(unit_open0(w))}" for w in WORDS],
    )
    write(
        "uniform_below",
        "w,n,expected,class",
        [f"{w:016x},{n:016x},{uniform_below(w, n):016x},E" for n in UNIFORM_N for w in WORDS],
    )
    write(
        "bernoulli",
        "w,p_bits,expected,class",
        [f"{w:016x},{bits(p)},{int(bernoulli(w, p))},E" for p in BERNOULLI_P for w in WORDS],
    )
    write(
        "poisson",
        "w,lambda_bits,expected,class",
        [
            f"{w:016x},{bits(lam)},{poisson_reference(w, lam)},E"
            for lam, words_of in POISSON_CASES
            for w in words_of
        ],
    )

    known = kat_reference.recorded("exponential")
    rows = []
    for rate in EXPONENTIAL_RATES:
        for w in WORDS:
            kind, value = exponential_reference(w, rate)
            rows.append(class_p_row("exponential", f"{w:016x};{bits(rate)}", kind, value, known))
    write("exponential", header, rows)

    known = kat_reference.recorded("normal")
    rows = []
    for w0, w1 in NORMAL_PAIRS:
        kind, value = normal_reference(w0, w1, 0.0, 1.0)
        inputs = f"{w0:016x};{w1:016x};{bits(0.0)};{bits(1.0)}"
        rows.append(class_p_row("normal", inputs, kind, value, known))
    # sd = 0 gives the mean exactly, whatever the words.
    for mean in (1.0, -2.5):
        for w0, w1 in NORMAL_PAIRS[:4]:
            kind, value = normal_reference(w0, w1, mean, 0.0)
            assert kind == "E" and value == mean
            inputs = f"{w0:016x};{w1:016x};{bits(mean)};{bits(0.0)}"
            rows.append(class_p_row("normal", inputs, kind, value, known))
    write("normal", header, rows)


if __name__ == "__main__":
    print("tc1_words", " ".join(f"{word:016x}" for word in words(bytes(32))))
    for seed, tag, subject, occasion, detail in DRAW_KEYS:
        key = chacha_key(seed, tag, subject, occasion, detail)
        print(f"draw seed={seed:016x} tag={tag:016x} subject={subject:016x} occasion={occasion:016x} detail={detail:016x}")
        print("  key  ", key.hex())
        print("  words", " ".join(f"{word:016x}" for word in words(key)))
    print("absorb_founder", f"{absorb(FOUNDER, 0, [str_key('test:fork_no_density'), 0]):016x}")
    print("absorb_rank", f"{absorb(RANK, 0, [7, 5, 11, 13, 17]):016x}")
    write_distributions()
    if kat_reference.MISMATCHES:
        sys.exit(f"{len(kat_reference.MISMATCHES)} file(s) differ from the reference")

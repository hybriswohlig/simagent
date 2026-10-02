#!/usr/bin/env python3
"""Reference values for the known-answer tests of sim-math (ADR-0003, section 3).

Class E (exact): the expected value follows from IEEE 754 / C99 Annex F or from an exactly
rounded function and is computed here, independent of libm and of the Rust code.

Class P (platform-equal): exp, ln and cos. The expected bits are those of libm 0.2.16,
recorded by `cargo xtask kat-record`; this script only supplies the inputs and the
reference column: the value computed with mpmath (dps = 60) and rounded to f64 half to
even by `round_to_f64`. A recorded value may differ from its reference by at most 1 ulp.

Writes tests/golden/kat/<function>.csv. The files are on the protected list (ADR-0000):
regenerate them only on purpose, in a `bless:` commit with a five-line report. The
recorded bits of a class P row survive a rerun as long as its input and reference stay.

Usage: .venv/bin/python tools/kat_reference.py           (write the files)
       .venv/bin/python tools/kat_reference.py --check   (compare, write nothing)
"""

import math
import struct
import sys
from fractions import Fraction
from pathlib import Path

import mpmath

mpmath.mp.dps = 60

OUT = Path(__file__).resolve().parent.parent / "tests" / "golden" / "kat"

TWO_52 = 2.0**52
TWO_53 = 2**53
TWO_63 = 2**63
MAX = 1.7976931348623157e308
MIN_SUBNORMAL = 5e-324
LARGEST_BELOW_TWO_63 = 9223372036854774784.0  # 2^63 - 1024


def bits(x: float) -> str:
    return "%016x" % struct.unpack("<Q", struct.pack("<d", x))[0]


def ceil_reference(x: float) -> float:
    """C99 F.9.6.1: ceil(+-0) = +-0, ceil(+-inf) = +-inf, NaN stays NaN; a result of
    zero from a negative argument is -0."""
    if math.isnan(x) or math.isinf(x):
        return x
    exact = Fraction(x)
    if exact.denominator == 1:
        return x  # integers, including +-0, are returned unchanged
    result = math.ceil(exact)  # exact integer
    if result == 0 and x < 0:
        return -0.0
    return float(result)  # exact: |result| <= 2^53 for non-integral inputs


def to_i64_exact_reference(x: float):
    """The integer x if x is finite, integral and inside the i64 range, else None."""
    if math.isnan(x) or math.isinf(x):
        return None
    exact = Fraction(x)
    if exact.denominator != 1:
        return None
    value = int(exact)
    if value < -TWO_63 or value > TWO_63 - 1:
        return None
    return value


def i64_to_f64_exact_reference(n: int):
    """The f64 equal to n if |n| <= 2^53 (every such integer is representable)."""
    if abs(n) > TWO_53:
        return None
    return float(n)


CEIL_INPUTS = [
    0.0, -0.0, 1.0, -1.0, 0.5, -0.5, 1.5, -1.5, 2.5, -2.5,
    0.9999999999999999, -0.9999999999999999, 1.0000000000000002,
    MIN_SUBNORMAL, -MIN_SUBNORMAL, 1e-310, -1e-310,
    TWO_52 - 0.5, -(TWO_52 - 0.5), TWO_52, 2.0**53 + 2.0,
    MAX, -MAX, 31449600.5, -31449600.5,
    math.inf, -math.inf, math.nan,
]

TO_I64_INPUTS = [
    0.0, -0.0, 1.0, -1.0, 0.5, -0.5, 1.5,
    2.0**53, -(2.0**53), LARGEST_BELOW_TWO_63, -LARGEST_BELOW_TWO_63,
    2.0**63, -(2.0**63), 1e300, -1e300,
    MIN_SUBNORMAL, TWO_52 - 0.5, 31449600.0,
    math.inf, -math.inf, math.nan,
]

I64_TO_F64_INPUTS = [
    0, 1, -1, 2**32, -(2**32), 2**32 - 1, -(2**32) - 1, 31449600,
    TWO_53, -TWO_53, TWO_53 + 1, -(TWO_53 + 1),
    TWO_63 - 1, -TWO_63,
]


def from_bits(hex_bits: str) -> float:
    return struct.unpack("<d", struct.pack("<Q", int(hex_bits, 16)))[0]


def round_to_f64(q: Fraction) -> float:
    """The f64 nearest to the positive rational q, ties to even, by integer arithmetic.
    Below the normal range the grid is that of the subnormals; above it the result is
    +inf (IEEE 754 round to nearest)."""
    assert q > 0
    # k with 2^k <= q < 2^(k + 1)
    k = q.numerator.bit_length() - q.denominator.bit_length()
    if Fraction(2) ** k > q:
        k -= 1
    assert Fraction(2) ** k <= q < Fraction(2) ** (k + 1)
    exponent = max(k - 52, -1074)  # the grid is 2^exponent
    scaled = q / Fraction(2) ** exponent
    n = scaled.numerator // scaled.denominator
    rest = scaled - n
    if rest > Fraction(1, 2) or (rest == Fraction(1, 2) and n % 2 == 1):
        n += 1
    try:
        return math.ldexp(float(n), exponent)  # n <= 2^53: both steps are exact
    except OverflowError:
        return math.inf


def mp_round(value) -> float:
    """An mpmath result, rounded to f64 by round_to_f64 (never by mpmath or by float())."""
    value = mpmath.mpf(value)
    if value == 0:
        return 0.0
    # 2^(magnitude - 1) <= |value| < 2^magnitude. Far outside the f64 range the result is
    # clear without building the rational (exp(1e308) has an exponent of 1e308 bits).
    magnitude = int(value.exp) + int(abs(value.man)).bit_length()
    sign = -1.0 if value < 0 else 1.0
    if magnitude > 1025:
        return sign * math.inf
    if magnitude < -1075:
        return sign * 0.0
    # mpmath keeps the mantissa unsigned; the sign is taken from the comparison above.
    q = Fraction(int(abs(value.man))) * Fraction(2) ** int(value.exp)
    return sign * round_to_f64(q)


def exp_reference(x: float):
    """(class, value). C99 F.9.3.1: exp(+-0) = 1, exp(-inf) = +0, exp(+inf) = +inf."""
    if math.isnan(x):
        return "E", math.nan
    if x == 0.0:
        return "E", 1.0
    if math.isinf(x):
        return "E", (math.inf if x > 0 else 0.0)
    return "P", mp_round(mpmath.exp(mpmath.mpf(x)))


def ln_reference(x: float):
    """(class, value). C99 F.9.3.7: log(+-0) = -inf, log(1) = +0, log(x < 0) = NaN,
    log(+inf) = +inf."""
    if math.isnan(x) or x < 0.0:
        return "E", math.nan
    if x == 0.0:
        return "E", -math.inf
    if x == 1.0:
        return "E", 0.0
    if math.isinf(x):
        return "E", math.inf
    return "P", mp_round(mpmath.log(mpmath.mpf(x)))


def cos_reference(x: float):
    """(class, value). C99 F.9.1.5: cos(+-0) = 1, cos(+-inf) = NaN."""
    if math.isnan(x) or math.isinf(x):
        return "E", math.nan
    if x == 0.0:
        return "E", 1.0
    return "P", mp_round(mpmath.cos(mpmath.mpf(x)))


def sqrt_reference(x: float):
    """(class, value). IEEE 754: sqrt is exactly rounded, sqrt(+-0) = +-0, sqrt(x < 0) =
    NaN, sqrt(+inf) = +inf. The root of an f64 never lies on a rounding midpoint, and it
    keeps a relative distance of more than 2^-110 from each, so 60 digits decide."""
    if math.isnan(x) or x < 0.0:
        return "E", math.nan
    if x == 0.0 or math.isinf(x):
        return "E", x
    return "E", mp_round(mpmath.sqrt(mpmath.mpf(x)))


MIN_NORMAL = 2.2250738585072014e-308
TAU = 6.283185307179586
PI = 3.141592653589793

EXP_INPUTS = [
    0.0, -0.0, 1.0, -1.0, 0.5, -0.5, 2.0, 10.0, -10.0, 100.0,
    MIN_SUBNORMAL, -MIN_SUBNORMAL, 1e-300, 1e-10, -1e-10,
    -5.0, -30.0,  # exp(0 - lambda) of the Poisson draw (ADR-0001)
    709.0, 709.782712893384, 709.7827128933841, 710.0, MAX,
    -708.0, -708.3964185322641, -745.0, -745.1332191019411, -745.2, -746.0, -MAX,
    math.inf, -math.inf, math.nan,
]

LN_INPUTS = [
    0.0, -0.0, 1.0, -1.0, 0.5, 2.0, 2.718281828459045, 10.0,
    1.0000000000000002,
    2.0**-53, 1.0 - 2.0**-53,  # the smallest and the largest unit_open0 below 1 (ADR-0001)
    MIN_SUBNORMAL, 1e-310, MIN_NORMAL, 1e-300, 1e300, MAX,
    -MIN_SUBNORMAL, math.inf, -math.inf, math.nan,
]

COS_INPUTS = [
    0.0, -0.0, 1.0, -1.0, 0.5, MIN_SUBNORMAL, 1e-10,
    PI / 2.0, PI, TAU, 3.0 * PI / 2.0,
    TAU * (1.0 - 2.0**-53),  # the largest argument of the normal draw (ADR-0001)
    1e5, -1e5, 1e10, 1e22, 2.0**1000, MAX, -MAX,
    math.inf, -math.inf, math.nan,
]

SQRT_INPUTS = [
    0.0, -0.0, 1.0, 4.0, 2.0, 0.5, 0.25, 3.0, 1e300, MAX,
    0.9999999999999999, 1.0000000000000002, 2.0**53 + 2.0,
    MIN_SUBNORMAL, 1e-310, MIN_NORMAL,
    73.47368314329845,  # about 0 - 2 ln(2^-53), the largest radicand of the normal draw
    -1.0, -MIN_SUBNORMAL, math.inf, -math.inf, math.nan,
]

UNRECORDED = "unrecorded"
CHECK = "--check" in sys.argv[1:]
MISMATCHES = []


def value_bits(x: float) -> str:
    return "nan" if math.isnan(x) else bits(x)


def recorded(name: str) -> dict:
    """The recorded bits of the class P rows of an existing file, by (input, reference)."""
    path = OUT / f"{name}.csv"
    if not path.exists():
        return {}
    found = {}
    for line in path.read_text(encoding="utf-8").splitlines()[2:]:
        fields = line.split(",")
        if len(fields) == 4 and fields[2] == "P":
            found[(fields[0], fields[3])] = fields[1]
    return found


def unary_rows(name: str, inputs: list, reference) -> list:
    """Rows input_bits,expected_bits,class,reference_bits. Class E expects the reference;
    class P keeps its recorded bits or is marked for `cargo xtask kat-record`."""
    known = recorded(name)
    rows = []
    for x in inputs:
        kind, value = reference(x)
        ref = value_bits(value)
        expected = ref if kind == "E" else known.get((bits(x), ref), UNRECORDED)
        rows.append(f"{bits(x)},{expected},{kind},{ref}")
    return rows


def write(name: str, header: str, rows: list, tool: str = "tools/kat_reference.py") -> None:
    path = OUT / f"{name}.csv"
    lines = [
        f"# generated by {tool}, do not edit by hand (ADR-0003)",
        header,
    ] + rows
    text = "\n".join(lines) + "\n"
    shown = path.relative_to(OUT.parent.parent.parent)
    if CHECK:
        if not path.exists() or path.read_text(encoding="utf-8") != text:
            MISMATCHES.append(str(shown))
            print(f"{shown}: differs from the reference")
        else:
            print(f"{shown}: {len(rows)} rows, equal")
        return
    path.write_text(text, encoding="utf-8")
    print(f"{shown}: {len(rows)} rows")


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    ceil_rows = []
    for x in CEIL_INPUTS:
        expected = ceil_reference(x)
        expected_bits = "nan" if math.isnan(expected) else bits(expected)
        ceil_rows.append(f"{bits(x)},{expected_bits},E,{expected_bits}")
    write("ceil", "input_bits,expected_bits,class,reference_bits", ceil_rows)

    to_i64_rows = []
    for x in TO_I64_INPUTS:
        expected = to_i64_exact_reference(x)
        to_i64_rows.append(f"{bits(x)},{'none' if expected is None else expected},E")
    write("to_i64_exact", "input_bits,expected,class", to_i64_rows)

    to_f64_rows = []
    for n in I64_TO_F64_INPUTS:
        expected = i64_to_f64_exact_reference(n)
        to_f64_rows.append(f"{n},{'none' if expected is None else bits(expected)},E")
    write("i64_to_f64_exact", "input,expected_bits,class", to_f64_rows)

    header = "input_bits,expected_bits,class,reference_bits"
    write("exp", header, unary_rows("exp", EXP_INPUTS, exp_reference))
    write("ln", header, unary_rows("ln", LN_INPUTS, ln_reference))
    write("cos", header, unary_rows("cos", COS_INPUTS, cos_reference))
    write("sqrt", header, unary_rows("sqrt", SQRT_INPUTS, sqrt_reference))

    if MISMATCHES:
        sys.exit(f"{len(MISMATCHES)} file(s) differ from the reference")


if __name__ == "__main__":
    main()

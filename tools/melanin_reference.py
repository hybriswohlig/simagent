#!/usr/bin/env python3
"""Reference vectors for `melanin_to_srgb` in contracts/body.json (ADR-0005 section 5),
written from the rule, independent of the Rust and TypeScript readers.

The rule: `skin` and `hair` are grids (`eumelanin`, `pheomelanin`, `srgb[i][j]` belongs to
`eumelanin[i]` and `pheomelanin[j]`), `iris` is a ramp (`melanin`, `srgb`). A reader

1. decodes each table colour `#rrggbb` to three bytes, each channel c = byte / 255;
2. linearises per IEC 61966-2-1: c <= 0.04045 gives c / 12.92, otherwise
   ((c + 0.055) / 1.055) ** 2.4;
3. clamps an input melanin value to [0, 1];
4. finds the cell of support points around the input (the smallest index i with
   points[i] <= x <= points[i + 1], so the value of the last point falls into the last
   cell), t = (x - x_i) / (x_{i+1} - x_i), and interpolates each linear channel:
   grid (1-t)(1-u) L[i][j] + t(1-u) L[i+1][j] + (1-t)u L[i][j+1] + t u L[i+1][j+1]
   (t along eumelanin, u along pheomelanin), ramp (1-t) L[i] + t L[i+1];
5. converts back: l <= 0.0031308 gives 12.92 l, otherwise 1.055 * l ** (1 / 2.4) - 0.055,
   clamps to [0, 1], byte = floor(value * 255 + 0.5), hex in lower case.

Everything is computed in IEEE double (Python float, `**` is the platform pow). The same
rule is computed again with mpmath at 60 digits on the exact double inputs as a cross
check, and with two other float orderings of the interpolation (lerp form and nested
lerps); each case's `margin` is the distance of its nearest scaled channel to a .5
rounding boundary, so a case with a small margin could round differently in another
language.

The boundaries file holds `bytes` (every byte 0 to 255 linearised by step 2, and the byte
step 5 gives back for that value), `beside` (inputs 1e-9 below and above each support
point, on each axis of skin and hair with the other axis at 0.4, and on the iris; the rule
clamps those outside [0, 1]) and `ties` (double inputs whose scaled channel is exactly
k + 0.5 under the rule in double, found by bisection along the `beside` lines; per table
the first with an even k whose 60-digit value lies below k + 0.5, so the rule's
floor(v + 0.5) gives k + 1 where half-even rounding and exact arithmetic give k).

Usage: python3 tools/melanin_reference.py [vectors.json [boundaries.json]]
(without a path the vectors go to stdout; the summary always goes to stderr)
"""

import json
import math
import sys
import time
from pathlib import Path

import mpmath

ROOT = Path(__file__).resolve().parent.parent
CONTRACT = ROOT / "contracts" / "body.json"

GRID_E = [0.1, 0.33, 0.6, 0.9, 0.125, 0.875]
GRID_P = [0.05, 0.4, 0.7, 0.95]
GRID_EXTRA = [(0.5, 0.125), (0.125, 0.5)]
IRIS_M = [0.1, 0.33, 0.6, 0.9, 0.125, 0.875, 0.2]
CLAMP_CASES = [
    ("skin", [-0.5, 1.5], [0.0, 1.0]),
    ("hair", [2.0, -1.0], [1.0, 0.0]),
    ("iris", [-0.1], [0.0]),
    ("iris", [1.1], [1.0]),
]
SUPPORTS = [0.0, 0.25, 0.5, 0.75, 1.0]


# ---------------------------------------------------------------- float (double) path

def decode(hex_colour: str) -> list:
    assert len(hex_colour) == 7 and hex_colour[0] == "#", hex_colour
    assert hex_colour == hex_colour.lower(), f"not lower case: {hex_colour}"
    assert hex_colour != "#000000", "the contract forbids #000000"
    return [int(hex_colour[k:k + 2], 16) for k in (1, 3, 5)]


def to_linear(byte: int) -> float:
    c = byte / 255
    if c <= 0.04045:
        return c / 12.92
    return ((c + 0.055) / 1.055) ** 2.4


def from_linear(l: float) -> float:
    if l <= 0.0031308:
        v = 12.92 * l
    else:
        v = 1.055 * l ** (1 / 2.4) - 0.055
    return min(max(v, 0.0), 1.0)


def clamp01(x: float) -> float:
    return min(max(x, 0.0), 1.0)


def cell(points: list, x: float, last: bool = False):
    """Smallest i with points[i] <= x <= points[i+1] (largest with `last`, a cross check
    for an input on an interior point); t within that cell."""
    order = range(len(points) - 2, -1, -1) if last else range(len(points) - 1)
    for i in order:
        if points[i] <= x <= points[i + 1]:
            return i, (x - points[i]) / (points[i + 1] - points[i])
    raise ValueError(f"{x} outside the support points")


_LINEAR = {}


def lin_table(table: dict) -> list:
    """The table's colours linearised (step 2), computed once per table."""
    key = id(table)
    if key not in _LINEAR:
        rows = table["srgb"]
        if isinstance(rows[0], list):
            _LINEAR[key] = [[[to_linear(b) for b in decode(h)] for h in row] for row in rows]
        else:
            _LINEAR[key] = [[to_linear(b) for b in decode(h)] for h in rows]
    return _LINEAR[key]


def grid_linear(table: dict, e: float, p: float, last: bool = False) -> list:
    lin = lin_table(table)
    i, t = cell(table["eumelanin"], clamp01(e), last)
    j, u = cell(table["pheomelanin"], clamp01(p), last)
    return [
        (1 - t) * (1 - u) * lin[i][j][k]
        + t * (1 - u) * lin[i + 1][j][k]
        + (1 - t) * u * lin[i][j + 1][k]
        + t * u * lin[i + 1][j + 1][k]
        for k in range(3)
    ]


def ramp_linear(table: dict, m: float, last: bool = False) -> list:
    lin = lin_table(table)
    i, t = cell(table["melanin"], clamp01(m), last)
    return [(1 - t) * lin[i][k] + t * lin[i + 1][k] for k in range(3)]


def finish(linear: list) -> dict:
    scaled = [from_linear(l) * 255 for l in linear]
    channels = [math.floor(s + 0.5) for s in scaled]
    margin = min(abs((s - math.floor(s)) - 0.5) for s in scaled)
    return {
        "linear": linear,
        "scaled": scaled,
        "hex": "#" + "".join(f"{c:02x}" for c in channels),
        "margin": margin,
    }


# ------------------------------------------- alternative float orderings (cross check)

def lerp(a: float, b: float, t: float) -> float:
    return a + t * (b - a)


def grid_linear_alt(table: dict, e: float, p: float, order: str) -> list:
    lin = lin_table(table)
    i, t = cell(table["eumelanin"], clamp01(e))
    j, u = cell(table["pheomelanin"], clamp01(p))
    out = []
    for k in range(3):
        if order == "e_first":
            a = lerp(lin[i][j][k], lin[i + 1][j][k], t)
            b = lerp(lin[i][j + 1][k], lin[i + 1][j + 1][k], t)
            out.append(lerp(a, b, u))
        else:
            a = lerp(lin[i][j][k], lin[i][j + 1][k], u)
            b = lerp(lin[i + 1][j][k], lin[i + 1][j + 1][k], u)
            out.append(lerp(a, b, t))
    return out


def ramp_linear_alt(table: dict, m: float) -> list:
    lin = lin_table(table)
    i, t = cell(table["melanin"], clamp01(m))
    return [lerp(lin[i][k], lin[i + 1][k], t) for k in range(3)]


# ------------------------------------------------------------ high precision path

mpmath.mp.dps = 60


def mp_to_linear(byte: int):
    c = mpmath.mpf(byte) / 255
    if c <= mpmath.mpf("0.04045"):
        return c / mpmath.mpf("12.92")
    return ((c + mpmath.mpf("0.055")) / mpmath.mpf("1.055")) ** mpmath.mpf("2.4")


def mp_from_linear(l):
    if l <= mpmath.mpf("0.0031308"):
        v = mpmath.mpf("12.92") * l
    else:
        v = mpmath.mpf("1.055") * l ** (1 / mpmath.mpf("2.4")) - mpmath.mpf("0.055")
    return min(max(v, mpmath.mpf(0)), mpmath.mpf(1))


def mp_cell(points: list, x):
    for i in range(len(points) - 1):
        lo, hi = mpmath.mpf(points[i]), mpmath.mpf(points[i + 1])
        if lo <= x <= hi:
            return i, (x - lo) / (hi - lo)
    raise ValueError(x)


def mp_scaled(table: dict, inp: list) -> list:
    """The rule in 60 digits, on the exact double values of the inputs."""
    xs = [min(max(mpmath.mpf(x), mpmath.mpf(0)), mpmath.mpf(1)) for x in inp]
    if len(inp) == 2:
        lin = [[[mp_to_linear(b) for b in decode(h)] for h in row] for row in table["srgb"]]
        i, t = mp_cell(table["eumelanin"], xs[0])
        j, u = mp_cell(table["pheomelanin"], xs[1])
        ls = [
            (1 - t) * (1 - u) * lin[i][j][k] + t * (1 - u) * lin[i + 1][j][k]
            + (1 - t) * u * lin[i][j + 1][k] + t * u * lin[i + 1][j + 1][k]
            for k in range(3)
        ]
    else:
        lin = [[mp_to_linear(b) for b in decode(h)] for h in table["srgb"]]
        i, t = mp_cell(table["melanin"], xs[0])
        ls = [(1 - t) * lin[i][k] + t * lin[i + 1][k] for k in range(3)]
    return [mp_from_linear(l) * 255 for l in ls]


# ------------------------------------------------------------------------ boundaries

BESIDE_DELTA = 1e-9
OTHER_AXIS = 0.4
TIE_SCAN = 64


def byte_rows() -> list:
    """Every byte linearised (step 2) and converted back (step 5)."""
    rows = []
    for b in range(256):
        lin = to_linear(b)
        rows.append({"byte": b, "linear": lin, "back": math.floor(from_linear(lin) * 255 + 0.5)})
    return rows


def axis_inputs(name: str, x: float) -> list:
    """The input with `x` on one axis; for skin and hair as (e, p) pairs on both axes."""
    if name == "iris":
        return [[x]]
    return [[x, OTHER_AXIS], [OTHER_AXIS, x]]


def beside_cases(tables: dict) -> list:
    """Inputs 1e-9 below and above every support point (the rule clamps those outside
    [0, 1]); `input` holds the unclamped value."""
    cases = []
    for name in ("skin", "hair", "iris"):
        for axis in (0, 1) if name != "iris" else (0,):
            for s in SUPPORTS:
                for x in (s - BESIDE_DELTA, s + BESIDE_DELTA):
                    cases.append(evaluate(tables, name, axis_inputs(name, x)[axis]))
    return cases


def scaled_at(tables: dict, name: str, inp: list) -> list:
    table = tables[name]
    linear = grid_linear(table, *inp) if len(inp) == 2 else ramp_linear(table, inp[0])
    return [from_linear(l) * 255 for l in linear]


def find_ties(tables: dict) -> tuple:
    """Search the lines of `beside` (each axis of skin and hair with the other at 0.4, the
    iris ramp) for double inputs whose scaled channel is exactly k + 0.5. Within a cell a
    channel is monotone in exact arithmetic, so every k + 0.5 between the cell's end
    values is crossed once: bisect over doubles to the crossing, then test TIE_SCAN
    neighbouring doubles on each side. Returns (crossings examined, crossings with at
    least one tie, ties found as (table, input, channel, k))."""
    crossings = 0
    with_tie = 0
    ties = []
    for name in ("skin", "hair", "iris"):
        for axis in (0, 1) if name != "iris" else (0,):
            def at(x, name=name, axis=axis):
                return scaled_at(tables, name, axis_inputs(name, x)[axis])
            for c in range(len(SUPPORTS) - 1):
                a, b = SUPPORTS[c], SUPPORTS[c + 1]
                sa, sb = at(a), at(b)
                for ch in range(3):
                    lo, hi = min(sa[ch], sb[ch]), max(sa[ch], sb[ch])
                    for k in range(math.floor(lo - 0.5), math.floor(hi - 0.5) + 1):
                        target = k + 0.5
                        if not lo < target < hi:
                            continue
                        crossings += 1
                        x0, x1 = a, b
                        below = sa[ch] < target
                        while True:
                            mid = (x0 + x1) / 2
                            if mid <= x0 or mid >= x1:
                                break
                            if (at(mid)[ch] < target) == below:
                                x0 = mid
                            else:
                                x1 = mid
                        found = set()
                        for start, direction in ((x0, a), (x1, b)):
                            x = start
                            for _ in range(TIE_SCAN + 1):
                                if at(x)[ch] == target:
                                    found.add(x)
                                if x == direction:
                                    break
                                x = math.nextafter(x, direction)
                        with_tie += bool(found)
                        for x in sorted(found):
                            ties.append((name, axis_inputs(name, x)[axis], ch, k))
    return crossings, with_tie, ties


def tie_report(tables: dict, name: str, inp: list, ch: int) -> dict:
    """How robust one tie is: the 60-digit value, the other orderings, and whether the
    platform pow was correctly rounded on its path."""
    table = tables[name]
    mp = mp_scaled(table, inp)[ch]
    if len(inp) == 2:
        alts = [grid_linear_alt(table, *inp, order=o) for o in ("e_first", "p_first")]
        linear = grid_linear(table, *inp)
    else:
        alts = [ramp_linear_alt(table, inp[0])]
        linear = ramp_linear(table, inp[0])
    alt_scaled = [from_linear(a[ch]) * 255 for a in alts]
    l = linear[ch]
    pow_cr = float(mpmath.mpf(l) ** mpmath.mpf(1 / 2.4))
    return {
        "exact_scaled": mpmath.nstr(mp, 25),
        "exact_byte": int(mpmath.floor(mp + mpmath.mpf("0.5"))),
        "alt_scaled": alt_scaled,
        "pow_correctly_rounded": l ** (1 / 2.4) == pow_cr,
    }


def pow_check() -> int:
    """Bytes whose platform pow in step 2 differs from the correctly rounded double."""
    bad = 0
    for b in range(256):
        c = b / 255
        if c > 0.04045:
            base = (c + 0.055) / 1.055
            if base ** 2.4 != float(mpmath.mpf(base) ** mpmath.mpf(2.4)):
                bad += 1
    return bad


# ---------------------------------------------------------------------------- driver

def evaluate(tables: dict, name: str, inp: list) -> dict:
    table = tables[name]
    linear = grid_linear(table, *inp) if len(inp) == 2 else ramp_linear(table, inp[0])
    return {"table": name, "input": inp, **finish(linear)}


def check_tables(tables: dict) -> None:
    assert set(tables) == {"skin", "hair", "iris"}, sorted(tables)
    for name in ("skin", "hair"):
        t = tables[name]
        assert set(t) == {"eumelanin", "pheomelanin", "srgb"}, (name, sorted(t))
        assert t["eumelanin"] == SUPPORTS and t["pheomelanin"] == SUPPORTS, name
        assert len(t["srgb"]) == 5 and all(len(row) == 5 for row in t["srgb"]), name
        for row in t["srgb"]:
            for h in row:
                decode(h)
    t = tables["iris"]
    assert set(t) == {"melanin", "srgb"}, sorted(t)
    assert t["melanin"] == SUPPORTS and len(t["srgb"]) == 5
    for h in t["srgb"]:
        decode(h)


def cross_check(tables: dict, cases: list, findings: list) -> float:
    """The 60-digit rule and two other float orderings must give the same hex; returns
    the largest |scaled(double) - scaled(60 digits)|."""
    max_mp_diff = 0.0
    for case in cases:
        table = tables[case["table"]]
        mp = mp_scaled(table, case["input"])
        mp_hex = "#" + "".join(f"{int(mpmath.floor(s + mpmath.mpf('0.5'))):02x}" for s in mp)
        if mp_hex != case["hex"]:
            findings.append(f"mpmath {case['table']} {case['input']}: {mp_hex} != {case['hex']}")
        max_mp_diff = max(max_mp_diff, max(float(abs(mpmath.mpf(f) - s))
                                           for f, s in zip(case["scaled"], mp)))
        if len(case["input"]) == 2:
            alts = [grid_linear_alt(table, *case["input"], order=o) for o in ("e_first", "p_first")]
        else:
            alts = [ramp_linear_alt(table, case["input"][0])]
        for alt in alts:
            if finish(alt)["hex"] != case["hex"]:
                findings.append(f"ordering {case['table']} {case['input']}: differs")
    return max_mp_diff


def low_margins(cases: list, err) -> None:
    low = sorted((c for c in cases if c["margin"] < 0.01), key=lambda c: c["margin"])
    smallest = min(cases, key=lambda c: c["margin"])
    print(f"  smallest margin: {smallest['margin']!r} at {smallest['table']} "
          f"{smallest['input']} -> {smallest['hex']}", file=err)
    print(f"  cases with margin < 0.01: {len(low)}", file=err)
    for c in low:
        print(f"    {c['table']} {c['input']} -> {c['hex']} margin {c['margin']!r} "
              f"scaled {c['scaled']}", file=err)


def main() -> int:
    start = time.perf_counter()
    tables = json.loads(CONTRACT.read_text())["melanin_to_srgb"]
    check_tables(tables)
    findings = []

    # Every byte survives the round trip linearise, delinearise (the support points rest
    # on this), with its margin.
    byte_margin = 1.0
    for b in range(256):
        s = from_linear(to_linear(b)) * 255
        assert math.floor(s + 0.5) == b, (b, s)
        byte_margin = min(byte_margin, abs((s - math.floor(s)) - 0.5))

    cases = []

    # Support points: they must give back the table colour exactly.
    for name in ("skin", "hair"):
        t = tables[name]
        for i, e in enumerate(t["eumelanin"]):
            for j, p in enumerate(t["pheomelanin"]):
                case = evaluate(tables, name, [e, p])
                if case["hex"] != t["srgb"][i][j]:
                    findings.append(f"{name} ({e}, {p}): {case['hex']} != {t['srgb'][i][j]}")
                # The other admissible cell at an interior point gives the same values.
                if grid_linear(t, e, p, last=True) != case["linear"]:
                    findings.append(f"{name} ({e}, {p}): the cell choice matters")
                cases.append(case)
    t = tables["iris"]
    for i, m in enumerate(t["melanin"]):
        case = evaluate(tables, "iris", [m])
        if case["hex"] != t["srgb"][i]:
            findings.append(f"iris ({m}): {case['hex']} != {t['srgb'][i]}")
        if ramp_linear(t, m, last=True) != case["linear"]:
            findings.append(f"iris ({m}): the cell choice matters")
        cases.append(case)
    support_count = len(cases)

    for name in ("skin", "hair"):
        for e in GRID_E:
            for p in GRID_P:
                cases.append(evaluate(tables, name, [e, p]))
        for e, p in GRID_EXTRA:
            cases.append(evaluate(tables, name, [e, p]))
    for m in IRIS_M:
        cases.append(evaluate(tables, "iris", [m]))

    for name, inp, clamped in CLAMP_CASES:
        case = evaluate(tables, name, inp)
        ref = evaluate(tables, name, clamped)
        if case["hex"] != ref["hex"] or case["linear"] != ref["linear"]:
            findings.append(f"clamp {name} {inp}: {case['hex']} != {ref['hex']}")
        cases.append(case)

    max_mp_diff = cross_check(tables, cases, findings)
    vectors_time = time.perf_counter() - start

    # Boundaries: bytes, inputs beside the support points, exact ties.
    bytes_rows = byte_rows()
    for row in bytes_rows:
        if row["back"] != row["byte"]:
            findings.append(f"byte {row['byte']} comes back as {row['back']}")
    pow_bad = pow_check()

    beside = beside_cases(tables)
    beside_mp_diff = cross_check(tables, beside, findings)
    max_jump = 0.0
    for case in beside:
        # Against the same line exactly at the support point: no jump at the cell change.
        inp = [clamp01(x) for x in case["input"]]
        nearest = [min(SUPPORTS, key=lambda s: abs(s - x)) for x in inp]
        at_point = [n if abs(n - x) <= 2 * BESIDE_DELTA else x for n, x in zip(nearest, inp)]
        ref = evaluate(tables, case["table"], at_point)
        if ref["hex"] != case["hex"]:
            findings.append(f"beside {case['table']} {case['input']}: {case['hex']} != {ref['hex']}")
        max_jump = max(max_jump, max(abs(a - b) for a, b in zip(case["scaled"], ref["scaled"])))

    crossings, crossings_with_tie, ties = find_ties(tables)
    # Per table the first tie (in search order) with an even k whose 60-digit value lies
    # below k + 0.5: the rule gives k + 1, half-even rounding and exact arithmetic give k.
    tie_cases, tie_reports = [], []
    for name in ("skin", "hair", "iris"):
        for tname, inp, ch, k in ties:
            if tname != name or k % 2:
                continue
            report = tie_report(tables, name, inp, ch)
            if report["exact_byte"] == k:
                case = evaluate(tables, name, inp)
                assert case["scaled"][ch] == k + 0.5 and case["margin"] == 0.0
                tie_cases.append(case)
                tie_reports.append((case, ch, k, report))
                break
    elapsed = time.perf_counter() - start

    text = json.dumps(cases, indent=1)
    if len(sys.argv) > 1:
        Path(sys.argv[1]).write_text(text + "\n")
    else:
        print(text)
    if len(sys.argv) > 2:
        boundaries = {"bytes": bytes_rows, "beside": beside, "ties": tie_cases}
        Path(sys.argv[2]).write_text(json.dumps(boundaries, indent=1) + "\n")

    err = sys.stderr
    print(f"vectors: {len(cases)} cases ({support_count} support points)", file=err)
    print(f"  byte round trip: all 256 bytes ok, smallest margin {byte_margin!r}", file=err)
    low_margins(cases, err)
    print(f"  max |scaled(double) - scaled(60 digits)|: {max_mp_diff!r}", file=err)
    print(f"  time: {vectors_time:.3f} s", file=err)
    print("boundaries:", file=err)
    print(f"  bytes: 256, every byte comes back: "
          f"{all(r['back'] == r['byte'] for r in bytes_rows)}; "
          f"platform pow not correctly rounded in step 2 for {pow_bad} bytes", file=err)
    print(f"  beside: {len(beside)} cases, largest scaled jump against the support point "
          f"{max_jump!r}", file=err)
    low_margins(beside, err)
    print(f"  max |scaled(double) - scaled(60 digits)|: {beside_mp_diff!r}", file=err)
    print(f"  ties: {crossings} crossings of k + 0.5 searched, {crossings_with_tie} with at "
          f"least one exact tie within {TIE_SCAN} doubles, {len(ties)} tie inputs", file=err)
    for case, ch, k, report in tie_reports:
        print(f"    {case['table']} {case['input']!r} channel {'rgb'[ch]}: scaled "
              f"{case['scaled'][ch]!r} -> {case['hex']}; 60 digits {report['exact_scaled']} "
              f"(byte {report['exact_byte']}); other orderings {report['alt_scaled']!r}; "
              f"pow correctly rounded {report['pow_correctly_rounded']}", file=err)
    print(f"findings: {len(findings)}", file=err)
    for f in findings:
        print(f"  {f}", file=err)
    print(f"time: {elapsed:.3f} s", file=err)
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())

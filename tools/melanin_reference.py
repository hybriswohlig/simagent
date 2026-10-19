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

Usage: python3 tools/melanin_reference.py [vectors.json]
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


def grid_linear(table: dict, e: float, p: float, last: bool = False) -> list:
    lin = [[[to_linear(b) for b in decode(h)] for h in row] for row in table["srgb"]]
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
    lin = [[to_linear(b) for b in decode(h)] for h in table["srgb"]]
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
    lin = [[[to_linear(b) for b in decode(h)] for h in row] for row in table["srgb"]]
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
    lin = [[to_linear(b) for b in decode(h)] for h in table["srgb"]]
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


def main() -> int:
    start = time.perf_counter()
    tables = json.loads(CONTRACT.read_text())["melanin_to_srgb"]
    check_tables(tables)

    # Every byte survives the round trip linearise, delinearise (the support points rest
    # on this), with its margin.
    byte_margin = 1.0
    for b in range(256):
        s = from_linear(to_linear(b)) * 255
        assert math.floor(s + 0.5) == b, (b, s)
        byte_margin = min(byte_margin, abs((s - math.floor(s)) - 0.5))

    cases = []
    findings = []

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

    # Cross checks: 60-digit rule and two other float orderings.
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

    elapsed = time.perf_counter() - start
    low = sorted((c for c in cases if c["margin"] < 0.01), key=lambda c: c["margin"])
    smallest = min(cases, key=lambda c: c["margin"])

    text = json.dumps(cases, indent=1)
    if len(sys.argv) > 1:
        Path(sys.argv[1]).write_text(text + "\n")
    else:
        print(text)

    err = sys.stderr
    print(f"cases: {len(cases)} ({support_count} support points)", file=err)
    print(f"byte round trip: all 256 bytes ok, smallest margin {byte_margin!r}", file=err)
    print(f"smallest margin: {smallest['margin']!r} at {smallest['table']} "
          f"{smallest['input']} -> {smallest['hex']}", file=err)
    print(f"cases with margin < 0.01: {len(low)}", file=err)
    for c in low:
        print(f"  {c['table']} {c['input']} -> {c['hex']} margin {c['margin']!r} "
              f"scaled {c['scaled']}", file=err)
    print(f"max |scaled(double) - scaled(60 digits)|: {max_mp_diff!r}", file=err)
    print(f"findings: {len(findings)}", file=err)
    for f in findings:
        print(f"  {f}", file=err)
    print(f"time: {elapsed:.3f} s", file=err)
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())

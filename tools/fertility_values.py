#!/usr/bin/env python3
"""The values of the fertility curves for the check against the Rust core (design note of
the fertility curves, section 3), from the pure functions of tools/world_reference.py on
the repository data: the curve `(θ, σ_θ)` of each sex and the form `puberty` from
data/calibration/genetics.json, the wish shape and its tilt from
data/calibration/demography.json. Every number is printed as the `repr` of the f64.

Before it prints, it checks the values the genetics specification states by hand (W5.6a,
W5.6b, W5.6c, W11.3a; `world_reference.check_fertility_spec`) and that the data hold the
numbers the note names.

Usage: .venv/bin/python tools/fertility_values.py
"""

import copy
import math

import mpmath

import data_reference
import world_reference
from postcard_reference import FEMALE, MALE
from world_reference import (
    REPOSITORY,
    ROOT,
    FertilityForm,
    chance,
    check_fertility_spec,
    female_factor,
    male_factor,
    menarche,
    theta_i,
    wish,
    wish_mean,
    wish_thresholds,
    wish_weights,
)

FORM = REPOSITORY.fertility
DEMOGRAPHY, _, _ = data_reference.documents(ROOT / "data")
SHAPE = [entry["value"] for entry in DEMOGRAPHY["fertility_wish_shape"]]
TILT = DEMOGRAPHY["values"]["core:fertility_wish_tilt"]["value"]
ADULT_RATE = DEMOGRAPHY["values"]["core:fertility_adult_rate"]["value"]

# The numbers of the note, section 1 and 3, held against the data.
assert FORM.kind == "puberty"
assert [getattr(FORM, name) for name in data_reference.FERTILITY_FIELDS[0]] == [
    0.2, 0.6, -1.0, 3.0, 3.0, 30.0, 50.0, 0.7, 2.0, 45.0, 0.4, 70.0
]
assert REPOSITORY.puberty_curve == {FEMALE: (12.3, 0.89), MALE: (14.02, 0.81)}
assert SHAPE == world_reference.WISH_SHAPE_SPEC and TILT == 0.311 and ADULT_RATE == 0.2
check_fertility_spec(FORM, REPOSITORY.puberty_curve, SHAPE)

WINDOW = FertilityForm(
    {
        "window": {
            "from": {"value": 18.0, "unit": "year"},
            "until": {"value": 45.0, "unit": "year"},
        }
    }
)

Z_P = [0.0, 1.5, -7.0]
DELTAS = [0.2, -1.5, 3.5, -1.0]
FEMALE_AGES = [12.0, 12.5, 13.0, 14.0, 15.5, 16.0, 20.0, 30.0, 35.0, 40.0, 49.9, 50.0, 60.0]
MALE_AGES = [12.0, 12.8, 13.3, 13.8, 14.8, 30.0, 45.0, 50.0, 57.5, 70.0, 80.0]
WINDOW_FEMALE_AGES = [17.9, 18.0, 44.9, 45.0]
UNITS = [0.0, 0.05, 0.1, 0.25, 0.5, 0.65, 0.9, 0.99, 0.999999]


def exp_margin(x: float) -> float:
    """How far the exact `e^x` lies from the rounding boundary of its f64, in ulps of
    the result (0.5 on an f64, 0 on a midpoint): a libm that is not correctly rounded
    can differ in the last bit only where this is small."""
    value = world_reference.exp_reference(x)[1]
    with mpmath.workdps(60):
        exact = mpmath.exp(mpmath.mpf(x))
        return float(0.5 - abs(exact - mpmath.mpf(value)) / mpmath.mpf(math.ulp(value)))


def window_data_hash() -> int:
    """The data hash of the repository data with the form `window` from 18 until 45
    (years) instead of `puberty`: the encoding of variant 1."""
    demography, calibration, genome_map = data_reference.documents(ROOT / "data")
    calibration = copy.deepcopy(calibration)
    text = {"source": "test data (ADR-0017 section 5)", "as_of": "2026-10-11", "note": "test value"}
    calibration["fertility"] = {
        "window": {
            "from": {"value": 18.0, "unit": "year", **text},
            "until": {"value": 45.0, "unit": "year", **text},
        }
    }
    return data_reference.documents_hash(demography, calibration, genome_map)


def main() -> None:
    print("spec checks W5.6a W5.6b W5.6c W11.3a: ok")
    for sex, name in ((FEMALE, "female"), (MALE, "male")):
        theta, sd = REPOSITORY.puberty_curve[sex]
        print(f"curve_{name} theta {theta!r} sigma_theta {sd!r}")
        for z_p in Z_P:
            print(f"theta_i_{name} z_p {z_p!r} -> {theta_i(REPOSITORY.puberty_curve[sex], z_p)!r}")
    for delta in DELTAS:
        print(f"menarche theta_i 12.0 delta {delta!r} -> {menarche(12.0, delta, FORM)!r}")
    for age in FEMALE_AGES:
        print(f"female_factor a {age!r} m 12.5 -> {female_factor(age, 12.5, FORM)!r}")
    for age in MALE_AGES:
        print(f"male_factor a {age!r} theta_i 13.5 -> {male_factor(age, 13.5, FORM)!r}")
    for age in WINDOW_FEMALE_AGES:
        print(f"window 18..45 female_factor a {age!r} -> {female_factor(age, 0.0, WINDOW)!r}")
    print(f"window 18..45 male_factor a 10.0 -> {male_factor(10.0, 0.0, WINDOW)!r}")
    print(f"chance 0.2 0.75 0.5 -> {chance(0.2, 0.75, 0.5)!r}")
    for alpha in (TILT, 0.0):
        weights, total = wish_weights(SHAPE, alpha)
        print(f"wish alpha {alpha!r} shape {SHAPE!r}")
        exps = [world_reference.exp_reference(alpha * float(n))[1] for n in range(len(SHAPE))]
        margins = [round(exp_margin(alpha * float(n)), 4) for n in range(len(SHAPE))]
        print(f"  exp(alpha * n) {exps!r} margin to the rounding boundary (ulps) {margins}")
        print(f"  weights {weights!r} total {total!r}")
        print(f"  thresholds acc {wish_thresholds(SHAPE, alpha)!r}")
        print(f"  expected wish {wish_mean(SHAPE, alpha)!r}")
        for u in UNITS:
            print(f"  wish u {u!r} -> {wish(u, SHAPE, alpha)}")
    print(f"extra: data_hash of the repository data with fertility window 18..45 0x{window_data_hash():016x}")


if __name__ == "__main__":
    main()

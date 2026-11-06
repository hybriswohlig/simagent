#!/usr/bin/env python3
"""The reserve at hunger 0 in the house (ADR-0026, addendum of 2026-11-06 on hunger at 0,
Grok's decision, option a), written from the decision alone, independent of the Rust code: the
rule, its pins, and the seconds of the lever test `crates/sim-core/tests/house_hunger.rs`.

The rule:
- Capacity, fixed when the needs begin: `body_fat_z` below -1 gives 500, from -1 up to 1 gives
  1000, 1 or above 1500; a missing z counts as 0 (1000). The core tests' data (the map 3 fixture)
  hold no body, so every test person there has 1000.
- A sample like a need: value at a second, a rate per hour, the value at t
  `clamp(value + rate * s / 3600, 0, capacity)` truncated toward zero, the crossing of a level
  `at + ceil(3600 * (value - level) / |rate|)`; written down anew only when the rate changes.
- The rate: away or on the way out 0; while an offer raises hunger +960 (0 once full); hunger at
  0 the drain of the age band, awake 80 60 40 50, asleep (a sleeping offer, or lying on the floor)
  32 24 16 20; else 0.
- The band: hunger at 0 and not rising, the reserve above half `draws`, at half or below `weak`,
  at 0 `wasted`.
- Weak or wasted: a new walk takes twice the house's seconds; a command for an offer that does
  not raise hunger is refused `weak`; at the next change of rates energy carries -40 (weak) or
  -80 (wasted), not while lying (the collapse's rate) or out, not while an action at its object
  sets energy (it keeps the rate it began with).
- The crossings of half and 0, planned while the rate is negative, log `core:weakened` and
  `core:wasted`.

The lever test (all from the decision, the test house and its rates, by hand; the house rules of
ADR-0024 to ADR-0026 for the choices, each carried by a critical need or by the night's order):
the test house of the file (hall 0 the entrance, living 1 3 s, bath 2 2 s, bedroom 3 4 s from
the hall, 5 s to an object in one's own room); a woman of 30 (sim 1) and a man of 70 (sim 2) whose
needs stand still, a girl of 8 (sim 3) whose hunger falls 1000 an hour and energy 61 an hour
awake, rising 80 asleep. The fridge (one place) feeds +1 an hour for 12 h, the table only from 18.
First morning on a Saturday at 6:00 (`M`). The woman is told to the fridge at `M + 1` and again in
the second her snack ends; the girl, waiting for the fridge from her hunger at 150 on, is told to
the television (fun +1 an hour for 12 h) the second after her hunger reaches 0.

Usage: $HOME/.cache/simagent-tools-venv/bin/python reserve_reference.py [--family]   (from tools/;
--family adds the capacity of every person of the test family's weeks)
"""

HOUR = 3600
CAPACITY = (500, 1000, 1500)
DRAIN_AWAKE = (-80, -60, -40, -50)
DRAIN_ASLEEP = (-32, -24, -16, -20)
REFILL = 960
WEAK_ENERGY, WASTED_ENERGY = -40, -80
WALK_FACTOR = 2


def size(z):
    """The index into CAPACITY of a body_fat_z, None for a missing one."""
    if z is not None and z < -1.0:
        return 0
    if z is not None and z >= 1.0:
        return 2
    return 1


def trunc_div(a: int, b: int) -> int:
    """Division truncating toward zero, as Rust's `/` on integers."""
    q = abs(a) // abs(b)
    return q if (a >= 0) == (b > 0) else -q


def value_at(sample, t: int, top: int = 1000) -> int:
    value, at, rate = sample
    assert t >= at
    return max(0, min(top, value + trunc_div(rate * (t - at), HOUR)))


def crossing(sample, level: int):
    value, at, rate = sample
    if rate >= 0 or value <= level:
        return None
    speed = -rate
    return at + (HOUR * (value - level) + speed - 1) // speed


def band(hunger, reserve, t: int, capacity: int):
    if hunger[2] > 0 or value_at(hunger, t) != 0:
        return None
    value = value_at(reserve, t, capacity)
    if value == 0:
        return "wasted"
    if value <= capacity // 2:
        return "weak"
    return "draws"


def extra(b) -> int:
    return {"weak": WEAK_ENERGY, "wasted": WASTED_ENERGY}.get(b, 0)


def walk(secs: int, b) -> int:
    return secs * WALK_FACTOR if b in ("weak", "wasted") else secs


def pins() -> None:
    """Grok's numbers."""
    adult = (1000, 0, DRAIN_AWAKE[2])
    assert crossing(adult, 500) == 45_000 and crossing(adult, 0) == 90_000
    thin_child = (500, 0, DRAIN_AWAKE[0])
    assert crossing(thin_child, 250) == 11_250 and crossing(thin_child, 0) == 22_500
    eating = (0, 0, REFILL)
    assert value_at(eating, 1_200) == 320 and value_at(eating, 300) == 80
    assert [size(z) for z in (-1.0001, -1.0, 0.0, 0.9999, 1.0, None)] == [0, 1, 1, 1, 2, 1]
    assert -40 + WASTED_ENERGY == -120 and 70 + WASTED_ENERGY == -10 and 80 + WASTED_ENERGY == 0
    # An adult, wasted after standing: 250 at -120 reaches 0 in 7 500 s.
    assert crossing((250, 0, -40 + WASTED_ENERGY), 0) == 7_500
    print("pins ok")


def lever() -> dict:
    """The girl's day, every second after the first morning `M` (0 here)."""
    out = {}
    hunger = (600, 0, -1000)
    energy = (900, 0, -61)
    # Hunger's crossings: 400, 150, 50 and 0.
    out["hunger_400"] = crossing(hunger, 400)
    out["hunger_150"] = crossing(hunger, 150)
    out["hunger_50"] = crossing(hunger, 50)
    zero = crossing(hunger, 0)
    out["hunger_0"] = zero
    # The woman: told at M + 1, 5 s to the fridge in the hall, 12 h; told again at her end.
    first_end = 1 + 5 + 43_200
    second_end = first_end + 5 + 43_200
    out["woman_first_end"], out["woman_second_end"] = first_end, second_end
    # The reserve drains from hunger's 0, awake, 80 an hour (the girl's band 6 to 12).
    reserve = (1000, zero, DRAIN_AWAKE[0])
    out["weakened"] = crossing(reserve, 500)
    out["wasted"] = crossing(reserve, 0)
    # Told to the television the second after: 3 s to the living room, not weak.
    told = zero + 1
    assert band((0, zero, -1000), reserve, told, 1000) == "draws"
    tv_arrive = told + walk(3, "draws")
    out["tv_arrive"] = tv_arrive
    # Energy 150 ends the television (it does not raise energy), before its 12 h.
    tv_stop = crossing(energy, 150)
    assert first_end < tv_stop < tv_arrive + 43_200, (first_end, tv_stop)
    out["tv_stop"] = tv_stop
    b = band((0, zero, -1000), reserve, tv_stop, 1000)
    assert b == "weak", b
    out["reserve_at_tv_stop"] = value_at(reserve, tv_stop)
    # The change of rates there gives energy the weak extra.
    e_value = value_at(energy, tv_stop)
    assert e_value == 150
    energy = (e_value, tv_stop, -61 + extra(b))
    out["energy_after_tv"] = energy
    out["energy_50"] = crossing(energy, 50)
    collapse = crossing(energy, 0)
    out["collapse"] = collapse
    assert out["wasted"] < collapse < 14 * HOUR, (out["wasted"], collapse)
    # Lying: energy +50 from 0 (the collapse's rate, no extra), the reserve at the asleep drain
    # (32, at 0 it stays 0); up at 250 after 18 000 s, before the night that began after the fall
    # would end.
    up = collapse + (250 * HOUR + 50 - 1) // 50
    out["stand_up"] = up
    # She waits for the fridge where the television left her, in the living room, and falls
    # there. Up: awake -61 and the wasted extra; in the night (from 20:00, M + 50 400) a bed,
    # from the living room through the hall, 3 + 4 s, twice.
    energy = (250, up, -61 + extra("wasted"))
    out["energy_after_up"] = energy
    bed_walk = walk(3 + 4, "wasted")
    out["bed_walk"] = bed_walk
    bed_arrive = up + bed_walk
    out["bed_arrive"] = bed_arrive
    asleep_energy = (value_at(energy, bed_arrive), bed_arrive, 80 + extra("wasted"))
    out["energy_asleep"] = asleep_energy
    # The night ends at 6:00 of Sunday (M + 86 400): awake again with the wasted extra.
    night_end = 86_400
    energy = (value_at(asleep_energy, night_end), night_end, -61 + extra("wasted"))
    out["energy_at_dawn"] = energy
    # The woman's second snack ends at M + 86 411: the place goes to the girl, from the bedroom to
    # the hall, 4 s, twice.
    assert night_end < second_end
    fridge_walk = walk(4, "wasted")
    out["fridge_walk"] = fridge_walk
    fridge_arrive = second_end + fridge_walk
    out["fridge_arrive"] = fridge_arrive
    # At the fridge hunger rises: no band, energy back to -61, the reserve +960 from 0.
    out["energy_at_fridge"] = (value_at(energy, fridge_arrive), fridge_arrive, -61)
    out["reserve_after_20_min"] = value_at((0, fridge_arrive, REFILL), fridge_arrive + 1_200)
    return out


# The test family of `crates/sim-core/tests/house_week.rs`: seed and tag, and the ages of family i.
SEED_WEEK = 0x5EED_0002_0005_0001
FAMILIES = 20


def ages(i: int) -> list:
    return [30 + i % 16, 30 + (7 * i + 3) % 16, 6 + i % 10, 6 + (3 * i + 5) % 10]


def family_sizes() -> dict:
    """The capacity of every person of the test family's 20 weeks on the repository's data: its
    `body_fat_z` from tools/census_reference.py (the body mapping of the specification; the z is
    the trait's phenotype, the same at every age), each founder of `Start` as `Run::founded`
    makes it (a woman, a man, then two girls; no named alleles), then CAPACITY by `size`."""
    import census_reference as cr
    import data_reference
    import world_reference as wr
    from postcard_reference import FEMALE, MALE

    data = wr.repository_data()
    documents = data_reference.documents(wr.ROOT / "data")
    body = cr.BodyCensusData(data, documents, strict=True)
    assert body.reason is None, body.reason
    sizes = {}
    for i in range(FAMILIES):
        founders = [(-years * wr.YEAR, MALE if index == 1 else FEMALE, []) for index, years in enumerate(ages(i))]
        world = wr.World(SEED_WEEK + i, "test:week", False, founders, [(0, 1)], data)
        evaluator = cr.Evaluator(world, body)
        for sim in range(1, 5):
            params = evaluator.evaluate(sim, 0)["params"]
            z = params["body_fat_z"][0]
            sizes[(i, sim)] = (z, CAPACITY[size(z)])
    return sizes



def couple_sizes() -> list:
    """The capacity of the couple of `crates/sim-cli/tests/view.rs` (`COUPLE`, world seed 7, tag
    `test:house`, a woman born at -943 488 000 and a man at -1 006 387 200, each with both copies of
    `core:disease_ar` the reference allele) on the repository's data, as `family_sizes`."""
    import census_reference as cr
    import data_reference
    import world_reference as wr
    from postcard_reference import FEMALE, MALE

    data = wr.repository_data()
    body = cr.BodyCensusData(data, data_reference.documents(wr.ROOT / "data"), strict=True)
    alleles = [("core:disease_ar", [0, 0])]
    founders = [(-943_488_000, FEMALE, alleles), (-1_006_387_200, MALE, alleles)]
    world = wr.World(7, "test:house", False, founders, [(0, 1)], data)
    evaluator = cr.Evaluator(world, body)
    out = []
    for sim in (1, 2):
        z = evaluator.evaluate(sim, 0)["params"]["body_fat_z"][0]
        out.append((sim, z, CAPACITY[size(z)]))
    return out


def main(argv) -> int:
    pins()
    for key, value in lever().items():
        print(key, value)
    if "--family" in argv:
        sizes = family_sizes()
        counts = {capacity: 0 for capacity in CAPACITY}
        for (i, sim), (z, capacity) in sorted(sizes.items()):
            counts[capacity] += 1
            print(f"family {i} sim {sim} body_fat_z {z!r} capacity {capacity}")
        print("capacities", counts)
        for sim, z, capacity in couple_sizes():
            print(f"view couple sim {sim} body_fat_z {z!r} capacity {capacity}")
    return 0


if __name__ == "__main__":
    import sys

    raise SystemExit(main(sys.argv[1:]))

#!/usr/bin/env python3
"""A model of the world of sim-core, written from the ADRs and independent of the Rust
code: the event queue (ADR-0002), the log entries (ADR-0004), the canonical projection
(ADR-0003 5.2), the save (ADR-0006 section 2), death through hazard budgets (ADR-0001)
by senescence and accident from the data (genetics specification 7.1, 7.2, 11.4;
ADR-0017 section 2) and the hereditary diseases of the data (genetics specification 3.1
to 3.4, 7.1; ADR-0017 sections 2, 3 and 5). The goldens and the reference bytes of the
tests come from it.

What the model knows:

- A world runs on its data (ADR-0007): the calibration and the genome map as documents,
  by default those under data/ (`REPOSITORY`). Test data (`death_test_data`) differ from
  them only in the rates of senescence and accident and the list `diseases`; the save
  names the hash of the data a world runs on. The map lengths, the sex ratio, the map in
  the form of the meiosis reference and the mutation rates come from the documents too.
- `Start` (world seed, founder tag, world parameters, founders, couples, fertility; no
  causes of death any more) writes `core:started`, plans the year end series (insertion
  number 1, first at the last second of year 0, period one year) and then per founder,
  in list order: the creation entry, the first birthday (the first anniversary at or
  after the start) and the deaths in the order of its causes.
- Events run in the order of their key: time, phase (0 domain, 1 observer), micro step,
  class (general 500, death 900, observer 1000), subject (kind, ID), kind (year end 1,
  birthday 2, death 3), insertion number.
- A birthday writes `core:birthday` and plans the next one a year later. The year end
  observer sees the clock one second before its own second, with its next occurrence
  already in the queue.
- Death (genetics specification 7.1, design note of the death from the data): the causes
  of every sim, in this order, are `core:senescence`, `core:accident`, then its lethal
  afflictions in data order. Per sim and cause a budget `B = exponential(DEATH_BUDGET,
  1.0)` at the key (tag DEATH_BUDGET, subject lineage key, occasion `str_key` of the
  code, detail 0). The origin is `o = max(born, start)` (a founder from the start, B19; a
  child from its birth), the origin age `x0 = as_years(o - born)`. Senescence (Gompertz
  `m · a · e^(b·x)`, `a` and `b` of the sex): the age at death
  `x = x0 + ln(1 + b · B / (a · m) · e^(−b·x0)) / b`, computed in that order as
  `mortality::senescence_age`, with `m` = 1 exactly without a trait of longevity (the data
  of these tests name none; this model refuses a trait, see `Data`). Accident (Makeham
  `λ` of the sex): `x = x0 + B / λ`. Both are due at `o + ceil((x − x0) · YEAR)`, never at
  100 000 years or more or beyond the clock (ADR-0002). exp and ln are the correctly
  rounded values (tools/kat_reference.py); each due second is checked not to hang on the
  last bit of exp, ln or the budget (`robust_seconds`), so the pinned libm of the core
  gives the same second.
- Afflictions (genetics specification 3.1, 3.4; ADR-0017 section 3): once, at its
  creation (a founder at `Start`, a child at its birth), a sim gets one affliction per
  disease of the data list `diseases` (in data order) that it has, whatever the switch
  of the hereditary diseases. A disease with a locus carrier needs the genotype:
  `recessive`, every copy the sim has at the locus carries allele 1 and it has at least
  one copy (two autosomal copies; a woman's two X, a man's one X; a man's Y, none for a
  woman; one mt); `dominant`, at least one copy carries allele 1. With the genotype the
  penetrance of its sex decides, `bernoulli(p)` at the key (DISEASE_PENETRANCE = 13,
  lineage key, `str_key` of the code, 0); without it there is no draw. A threshold
  carrier without a liability trait afflicts nobody, without a draw (a liability trait
  comes in step 11; this model refuses it as `Data::new` does). The onset in years is
  the fixed age, or `x = normal(mean, sd)` at the key (DISEASE_ONSET = 14, lineage key,
  `str_key` of the code, 0) held by `min(max(x, low), high)`.
- A lethal affliction (effect `death` or `death_in_two_steps`) keeps an exposure: `used`,
  the hazard used up in the closed periods of the switch (0.0 at the creation), and
  `since`, when the current period began: the origin (the start for a founder, the
  birth for a child) if the switch is on at the creation, else none. A visible one
  (effect `visible`) has no exposure and never kills.
- The hazard of a disease is a step hazard (before, change_age, after); `death
  { per_year: h }` is (h, 0, h). It acts from the onset on. The age of a sim at `t` is
  `(t - born)` seconds as f64 over the seconds of a year (`as_years`). While its
  exposure runs (since `ts`), a lethal affliction kills at the age
  `a = disease_age(x0, max(B - used, 0), hazard)` with `x0 = max(age(ts), onset)`, the
  closed form of 3.1, at `ts + ceil((a - age(ts)) * YEAR)` (counted from the second
  the period began, so a used-up budget dies in the very second the switch goes on
  again); never if there is no such age, if `a - age(ts)` is 100 000 years or more, or
  if the time is beyond the clock (ADR-0002).
- The deaths of a sim are planned in the order of its causes: senescence, accident, then
  its lethal afflictions whose exposure runs, in data order; so in a tie of one second
  senescence comes before accident before the diseases (insertion order). The death
  writes `core:death` naming the cause code (`core:senescence`, `core:accident` or the
  disease) with the plan and the draw as causes, moves the sim with its afflictions as
  they are to the archive and cancels its other pending events.
- An override block is an input: stamped with the last closed second, acting in the
  next one, one entry `core:param_changed` per change. If the switch flips at `now`,
  every living sim (by ID) updates each lethal affliction (in data order): switched on,
  `since = now`; switched off, the period closes: with `from = max(age(since), onset)`
  and `to = age(now)`, `used` grows by `disease_hazard_used(hazard, from, to)` if
  `to > from`, and `since` is none. Then its pending deaths of a disease are cancelled
  and planned again in data order, by the entry `core:param_changed`. An override that
  keeps the switch as it is changes nothing but the log.
- Couples: after the founders, `Start` forms a partnership per couple of its list (a
  woman and a man by their places among the founders); `ArrangePartnership` forms one
  between two living sims. Each gets the next PartnershipId and one entry
  `core:partnered` (subjects the woman and the man). A death ends the open partnership
  of the sim: it keeps the second of the death.
- Conception (ADR-0001 scenario `fork_no_density`, ADR-0004 section 7): a partnership
  plans a monthly series of attempts (period MONTH, by its entry) from the first month
  start at or after the forming and her `from_years` birthday, if she is younger than
  `until_years` then and not pregnant. An attempt draws
  `bernoulli(CONCEPTION_ATTEMPT, per_month)` (subject
  her lineage key, occasion the month index, detail his lineage key); on success she
  conceives her conception number n: the child key `child(her, his, n, 0)`, per parent
  the start strand `uniform_below(MEIOSIS_START_STRAND, 2)` (subject the genome key,
  occasion 0 mother and 1 father) and the sex `bernoulli(SEX_CHROMOSOME, p_Y)`
  (occasion 1, true for a son; `p_Y = r / (1 + r)` from `core:sex_ratio_at_birth`,
  genetics specification 2.3 and 11.3). Entries: `core:conception` (her, him; plan and draw), then
  `core:meiosis` of her and of him (event, rule `core:meiosis`, the strand draw, for him
  also the sex draw). The series ends; the pregnancy is state, and the birth is planned
  `pregnancy_months` months later (class birth 200, by the conception). A failed attempt
  ends the series if the next month start is her `until_years` birthday or later. The
  death of either ends the series; her death ends the pregnancy.
- Birth: a new sim with the next SimId and the entry `core:birth` (subjects child, mother,
  father; the plan and the rule `core:gestation`); born now, with parents and the
  conception number, its afflictions from its genome with the birth as origin. Its first
  birthday a year later, its deaths from its birth on. If the mother is in an open
  partnership, her attempts begin again from the first month start after the birth second.

Usage: .venv/bin/python tools/world_reference.py (prints the data hashes and the checks
of the step hazard; the other tools use the model)
"""

import itertools
import math
from pathlib import Path

import xxhash

import data_reference

from postcard_reference import (
    FEMALE,
    MALE,
    MATERNAL,
    PATERNAL,
    X,
    Y_CHROMOSOME,
    afflictions as afflictions_bytes,
    couple,
    fertility,
    genome_of,
    founder,
    lineage_key,
    param_change,
    partnership,
    mutation,
    segment_meiosis,
    seq,
    signed,
    string,
    variant,
    varint,
    world_params,
)
from rng_reference import (
    bernoulli,
    chacha_key,
    child_key,
    exponential_reference,
    founder_key,
    normal_reference,
    str_key,
    words,
)

from kat_reference import exp_reference, ln_reference

import meiosis_reference

ROOT = Path(__file__).resolve().parent.parent


YEAR = 364 * 86_400
MONTH = 28 * 86_400
NEVER_YEARS = 100_000.0
# The range of the clock (ADR-0002): a time beyond it is never.
MAX_ABS_SECS = 1_000_000 * YEAR
# tests/golden/rng_tags.txt
MEIOSIS_START_STRAND, SEX_CHROMOSOME, CONCEPTION_ATTEMPT, DEATH_BUDGET = 1, 2, 3, 4
DISEASE_PENETRANCE, DISEASE_ONSET = 13, 14

PROJECTION_VERSION = 12
CORE_VERSION = 24

# A scenario without its own fertility: nobody ever conceives.
BARREN = (0.0, 18, 45, 10)
FORMAT_VERSION = 1

NONE = b"\x00"

# Event classes and kinds (tests/golden/event_codes.txt).
BIRTH, GENERAL, DEATH, OBSERVER = 200, 500, 900, 1000
YEAR_END_KIND, BIRTHDAY_KIND, DEATH_KIND, ATTEMPT_KIND, BIRTH_KIND = 1, 2, 3, 4, 5
WORLD_SUBJECT, SIM_SUBJECT = 0, 1
# The causes of death every sim has before its diseases, in this order (design note of
# the death from the data, section 2).
SENESCENCE, ACCIDENT = "core:senescence", "core:accident"


def some(data: bytes) -> bytes:
    return b"\x01" + data


def hash64(data: bytes) -> int:
    return xxhash.xxh3_64_intdigest(data, seed=0)


# ---------------------------------------------------------------------------------------
# Data


def number(entry) -> float:
    """A calibrated number of the data: its value as f64."""
    return float(entry["value"])


def only(enum) -> tuple:
    """(name, fields) of an externally tagged variant with fields."""
    ((name, fields),) = enum.items()
    return name, fields


class Disease:
    """One entry of the calibration's list `diseases` (ADR-0017 section 1), without its
    texts: the code, the carrier (a locus with the penetrance per sex, or nothing for a
    threshold without a liability trait), the onset and the step hazard (None for a
    visible disease)."""

    def __init__(self, entry: dict, loci: dict):
        self.code = entry["code"]
        self.key = str_key(self.code)
        kind, carrier = only(entry["carrier"])
        if kind == "locus":
            self.locus = loci[carrier["locus"]]
            assert self.locus["mark"] == "disease", f"{self.code}: the locus is no disease locus"
            self.penetrance = {
                FEMALE: number(carrier["penetrance_female"]),
                MALE: number(carrier["penetrance_male"]),
            }
        else:
            assert kind == "threshold"
            # Data::new rejects a liability trait until step 11 (`data.disease`).
            assert carrier["liability"] is None, f"{self.code}: a liability trait is not modelled yet"
            self.locus = None
        kind, onset = only(entry["onset"])
        if kind == "fixed":
            self.onset = ("fixed", number(onset["age"]))
        else:
            assert kind == "normal"
            self.onset = ("normal", *(number(onset[name]) for name in ("mean", "sd", "low", "high")))
        effect = entry["effect"]
        if effect == "visible":
            self.hazard = None
        else:
            kind, numbers = only(effect)
            if kind == "death":
                h = number(numbers["per_year"])
                self.hazard = (h, 0.0, h)
            else:
                assert kind == "death_in_two_steps"
                self.hazard = tuple(number(numbers[name]) for name in ("before", "change_age", "after"))


# The ranges `mortality` accepts (crates/sim-core/src/mortality.rs, a contract of the
# merged pure functions): a world never calls them outside, the data keep inside.
GOMPERTZ_A_RANGE = (1e-15, 1.0)
GOMPERTZ_B_RANGE = (1e-3, 1.0)
MAX_SIGMA_G = 5.0
MULTIPLIER_RANGE = (1e-14, 1e14)
MAX_BUDGET = 1000.0
ACCIDENT_RANGE = (1e-9, 1000.0)


def within(x: float, bounds: tuple) -> bool:
    return bounds[0] <= x <= bounds[1]


class Data:
    """The definitions a world runs on: the calibration and the genome map as documents
    (parsed JSON with their texts), their data hash (tools/data_reference.py), the loci
    by ID, the diseases in data order, the rates of death per sex (Gompertz `a` and `b`,
    the Makeham term `λ`), `σ_g` and the trait of longevity, and what the meiosis
    reference reads: the map lengths, the sex ratio at birth, the map in its form and
    the mutation rates."""

    def __init__(self, calibration: dict, genome_map: dict):
        self.hash = data_reference.documents_hash(calibration, genome_map)
        self.loci = {locus["id"]: locus for locus in genome_map["loci"]}
        stripped = data_reference.strip(calibration)
        self.diseases = [Disease(entry, self.loci) for entry in stripped["diseases"]]
        values = stripped["values"]
        self.senescence = {
            sex: (number(values[f"core:senescence_a_{name}"]), number(values[f"core:senescence_b_{name}"]))
            for sex, name in ((FEMALE, "female"), (MALE, "male"))
        }
        self.accident = {sex: number(values[f"core:accident_{name}"]) for sex, name in ((FEMALE, "female"), (MALE, "male"))}
        self.sigma_g = number(values["core:longevity_sigma_g"])
        for a, b in self.senescence.values():
            assert within(a, GOMPERTZ_A_RANGE) and within(b, GOMPERTZ_B_RANGE), (a, b)
        assert all(within(rate, ACCIDENT_RANGE) for rate in self.accident.values())
        assert within(self.sigma_g, (0.0, MAX_SIGMA_G))
        # With a trait of longevity `m = e^(−σ_g · ĝ)` needs the genetic value of the
        # trait (genetics specification 4.1), which no Python reference computes yet; the
        # data of these tests name none (`m` = 1 exactly, 11.1).
        self.longevity_trait = stripped["longevity_trait"]
        assert self.longevity_trait is None, "a trait of longevity is not modelled by this reference"
        self.map_lengths = {
            "autosome_female": values["core:map_length_autosome_female"]["value"],
            "autosome_male": values["core:map_length_autosome_male"]["value"],
            "x_female": values["core:map_length_x_female"]["value"],
        }
        self.sex_ratio = values["core:sex_ratio_at_birth"]["value"]
        self.genome_map = [
            {
                "id": locus["id"],
                "chromosome": locus["chromosome"],
                "segment": locus["segment"]
                if locus["chromosome"] == "autosome"
                else (8 if locus["chromosome"] == "x" else 0),
                "slot": locus["slot"],
                "mark": locus["mark"],
                "direction": locus["mutation"],
            }
            for locus in genome_map["loci"]
        ]
        self.rates = {
            "standard": values["core:mutation_rate"]["value"],
            "disease": values["core:mutation_rate_disease"]["value"],
        }

    def multiplier(self, person: dict) -> float:
        """`m` of the Gompertz term (7.2): 1 exactly without a trait of longevity
        (`exp(−0.0)` is 1, 11.1)."""
        assert self.longevity_trait is None
        return 1.0


def repository_data() -> Data:
    return Data(*data_reference.documents(ROOT / "data"))


def death_test_data(accident: float, senescence_a: float = 1e-15, senescence_b: float = 0.001, diseases=None) -> Data:
    """Test data for the deaths (design note of the death from the data, section 4): the
    repository's calibration and map with the Makeham term `accident` and the Gompertz
    `a`, `b` for both sexes, and `diseases` (a list of disease documents) instead of the
    repository's six if given. A scenario that had causes of `Start` takes the rate of
    its first cause as `accident` and that of its second as `senescence_a` with `b` =
    0.001; without a second cause `a` = 1e-15 and `b` = 0.001, senescence practically
    off (its death lies some 20 000 years ahead)."""
    calibration, genome_map = data_reference.documents(ROOT / "data")
    values = calibration["values"]
    for sex in ("female", "male"):
        values[f"core:accident_{sex}"]["value"] = accident
        values[f"core:senescence_a_{sex}"]["value"] = senescence_a
        values[f"core:senescence_b_{sex}"]["value"] = senescence_b
    if diseases is not None:
        calibration["diseases"] = diseases
    return Data(calibration, genome_map)


def dummy_disease(onset_years: float) -> dict:
    """The disease of the walking skeleton as data (design note section 6, ADR-0017
    section 5): `test:dummy_disease` on `core:disease_ar`, penetrance 1 for both sexes,
    the onset fixed at `onset_years`, death with 0.3 per year."""

    def calibrated(value: float, unit: str) -> dict:
        return {
            "value": value,
            "unit": unit,
            "source": "test data of the walking skeleton (ADR-0017 section 5)",
            "as_of": "2026-10-10",
            "note": "test value, not calibrated",
        }

    return {
        "code": "test:dummy_disease",
        "carrier": {
            "locus": {
                "locus": "core:disease_ar",
                "penetrance_female": calibrated(1.0, "fraction"),
                "penetrance_male": calibrated(1.0, "fraction"),
            }
        },
        "onset": {"fixed": {"age": calibrated(onset_years, "year")}},
        "effect": {"death": {"per_year": calibrated(0.3, "per_year")}},
    }


REPOSITORY = repository_data()
# The "fork data" of the acceptance scenario `fork_no_density` (ADR-0017 section 5): the
# old model, accident 0.02 per year, senescence practically off (`a` = 1e-12, `b` = 0.1),
# the dummy disease from 18.
FORK_DATA = death_test_data(0.02, 1e-12, 0.1, [dummy_disease(18.0)])
# The data hash of the definitions under data/ (ADR-0007), from their own reference.
DATA_HASH = REPOSITORY.hash
assert DATA_HASH == data_reference.data_hash(ROOT / "data")


# ---------------------------------------------------------------------------------------
# Genomes


def founder_genome(sex: bytes, genes, loci=None) -> dict:
    """The genome of a founder in the form of tools/meiosis_reference.py, from its
    alleles: the pair at `core:disease_ar` (from `genotype`) or a list of (locus, copies).
    The copies are maternal then paternal; their number follows the chromosome and the
    sex: two on an autosome, two on a woman's X and one on a man's, one on a man's Y,
    one mt. Every slot not named holds the reference allele."""
    loci = REPOSITORY.loci if loci is None else loci
    if isinstance(genes, tuple):
        genes = [("core:disease_ar", list(genes))]
    maternal = [[0] * 24 for _ in range(8)]
    paternal = [[0] * 24 for _ in range(8)]
    x_maternal, x_paternal, y, mt = [0] * 16, [0] * 16, [0] * 8, [0] * 8
    for locus_id, copies in genes:
        locus = loci[locus_id]
        chromosome, slot = locus["chromosome"], locus["slot"]
        if chromosome == "autosome":
            assert len(copies) == 2, f"{locus_id}: two copies on an autosome"
            maternal[locus["segment"]][slot], paternal[locus["segment"]][slot] = copies
        elif chromosome == "x":
            assert len(copies) == (2 if sex == FEMALE else 1), f"{locus_id}: the X copies of the sex"
            x_maternal[slot] = copies[0]
            if sex == FEMALE:
                x_paternal[slot] = copies[1]
        elif chromosome == "y":
            assert sex == MALE and len(copies) == 1, f"{locus_id}: one Y copy, men only"
            y[slot] = copies[0]
        else:
            assert len(copies) == 1, f"{locus_id}: one mt copy"
            mt[slot] = copies[0]
    person = {"autosomes_maternal": maternal, "autosomes_paternal": paternal, "x_maternal": x_maternal, "mt": mt}
    if sex == FEMALE:
        person["x_paternal"] = x_paternal
    else:
        person["paternal_sex"] = {"y": y}
    return person


def from_father(person: dict) -> tuple:
    """("x" or "y", slots) of the sex chromosome a sim has from its father."""
    if "x_paternal" in person:
        return "x", person["x_paternal"]
    return only(person["paternal_sex"])


def copies_at(person: dict, locus: dict) -> list:
    """The alleles a sim has at the locus, one per copy it has (genetics specification
    2.1, 3.4)."""
    chromosome, slot = locus["chromosome"], locus["slot"]
    if chromosome == "autosome":
        segment = locus["segment"]
        return [person["autosomes_maternal"][segment][slot], person["autosomes_paternal"][segment][slot]]
    name, second = from_father(person)
    if chromosome == "x":
        return [person["x_maternal"][slot]] + ([second[slot]] if name == "x" else [])
    if chromosome == "y":
        return [second[slot]] if name == "y" else []
    assert chromosome == "mt"
    return [person["mt"][slot]]


def has_genotype(action, copies: list) -> bool:
    """Whether the genotype can carry the disease (3.4): recessive, every copy carries
    allele 1 and there is at least one; dominant, at least one copy carries it."""
    if action == "recessive":
        return len(copies) >= 1 and all(allele == 1 for allele in copies)
    assert action == "dominant", f"a disease locus acts recessive or dominant, not {action}"
    return any(allele == 1 for allele in copies)


# ---------------------------------------------------------------------------------------
# The step hazard (genetics specification 3.1)


def disease_age(x0: float, budget: float, hazard: tuple):
    """The age at which a budget is used up from the age x0 on (3.1): at or after the
    change `x0 + B / h1`; before it `H = h0 * (s - x0)`, with a positive `h0` and
    `B <= H` the age `x0 + B / h0`, else `s + (B - H) / h1`. An `h0` of 0 is the old edge:
    nobody dies before `s`. Without `h1` a rest beyond `H` is never used up (None)."""
    before, change, after = hazard
    if x0 < change:
        used = before * (change - x0)
        if before > 0.0 and budget <= used:
            return x0 + budget / before
        if after == 0.0:
            return None
        return change + (budget - used) / after
    if after == 0.0:
        return None
    return x0 + budget / after


def disease_hazard_used(hazard: tuple, from_: float, to: float) -> float:
    """The hazard used up from the age `from_` to the age `to` (3.1): `h0` on the part
    before the change, `h1` on the part after it."""
    before, change, after = hazard
    assert 0.0 <= from_ <= to and math.isfinite(to)
    return before * max(0.0, min(to, change) - min(from_, change)) + after * max(0.0, to - max(from_, change))


def as_years(seconds: int) -> float:
    """SimDuration::as_years: the seconds exactly as f64, over the seconds of a year."""
    assert abs(seconds) <= 2**53
    return float(seconds) / 31_449_600.0


def from_years_ceil(years: float):
    """SimDuration::from_years_ceil (ADR-0002): None (never) at 100 000 years or more,
    else the seconds rounded up."""
    assert math.isfinite(years) and years > -NEVER_YEARS
    if years >= NEVER_YEARS:
        return None
    return math.ceil(years * 31_449_600.0)


# The ages at death and the hazard used up of crates/sim-core/tests/disease_hazard.rs: the
# values there come from Python's math, independent of this model (W3.1).
CYSTIC = (0.02 / 18.0, 18.0, 0.014_322_280_437_445_644)
HAEMOPHILIA = (0.000_555_555_555_555_555_6, 18.0, 0.005)
for origin, budget, hazard, want in [
    (0.0, 0.01, CYSTIC, 9.0),
    (0.0, math.log(2.0), CYSTIC, 65.0),
    (20.0, 0.5, CYSTIC, 54.910_641_652_620_384),
    (10.0, 0.05, CYSTIC, 20.870_430_535_882_12),
    (5.0, 0.1, (0.0, 18.0, 0.04), 20.5),
    (45.0, 1.0, (0.04, 0.0, 0.04), 70.0),
    (3.0, 0.004, HAEMOPHILIA, 10.2),
]:
    assert abs(disease_age(origin, budget, hazard) - want) <= 1e-13 * want, (origin, budget, want)
assert disease_age(5.0, 0.0, (0.0, 18.0, 0.04)) == 18.0
assert disease_age(5.0, 0.0, CYSTIC) == 5.0 and disease_age(20.0, 0.0, CYSTIC) == 20.0
assert disease_age(0.0, 1.5, (0.1, 10.0, 0.0)) is None and disease_age(12.0, 0.1, (0.1, 10.0, 0.0)) is None
assert disease_age(0.0, 0.0, (1e-9, 0.25, 0.0)) == 0.0
for low, high, want in [(0.0, 18.0, 0.02), (10.0, 30.0, 0.180_756_254_138_236_62), (25.0, 40.0, 0.214_834_206_561_684_64), (7.0, 7.0, 0.0)]:
    assert abs(disease_hazard_used(CYSTIC, low, high) - want) <= 1e-15 + 1e-13 * want, (low, high)


def budget_of(seed: int, key: int, code: str) -> float:
    """The budget `B` of the cause or disease with this code for this lineage key."""
    return exponential_reference(words(chacha_key(seed, DEATH_BUDGET, key, str_key(code), 0))[0], 1.0)[1]


def step_ulps(value: float, ulps: int) -> float:
    """`value` moved by whole ulps (the check that a result does not hang on the last bit
    of a libm function)."""
    for _ in range(abs(ulps)):
        value = math.nextafter(value, math.inf if ulps > 0 else -math.inf)
    return value


def senescence_age(x0: float, budget: float, a: float, b: float, m: float, nudge=(0, 0)) -> float:
    """`mortality::senescence_age` (genetics specification 7.1): the age at which the
    Gompertz term `m · a · e^(b·x)` acting from `x0` on uses up `budget`, computed as
    `x0 + ln(1 + b · B / (a · m) · e^(−b·x0)) / b` in this order. exp and ln are correctly
    rounded; `nudge` moves their results by whole ulps (exact results stay)."""
    assert x0 >= 0.0 and math.isfinite(x0) and within(budget, (0.0, MAX_BUDGET))
    assert within(a, GOMPERTZ_A_RANGE) and within(b, GOMPERTZ_B_RANGE) and within(m, MULTIPLIER_RANGE)
    kind, e = exp_reference(0.0 - b * x0)
    if kind == "P":
        e = step_ulps(e, nudge[0])
    ahead = b * budget / (a * m) * e
    kind, log = ln_reference(1.0 + ahead)
    if kind == "P":
        log = step_ulps(log, nudge[1])
    return x0 + log / b


def accident_age(x0: float, budget: float, per_year: float) -> float:
    """`mortality::accident_age` (7.1): `x0 + B / λ`."""
    assert x0 >= 0.0 and math.isfinite(x0) and within(budget, (0.0, MAX_BUDGET))
    assert within(per_year, ACCIDENT_RANGE)
    return x0 + budget / per_year


# The ages at death of crates/sim-core/tests/mortality.rs (W7.1, from Python's math there,
# independent of this model): the closed forms here meet them.
for origin, budget, (a, b), m, want in [
    (0.0, 1.0, (3.25e-6, 0.1182), 1.0, 88.845_227_436_585_18),
    (30.0, 0.5, (8.73e-6, 0.1118), 1.0, 78.434_825_532_710_79),
    (0.0, 2.0, (3.25e-6, 0.1182), exp_reference(-0.724 * 1.5)[1], 103.897_040_684_885_16),
    (64.0, 0.01, (8.73e-6, 0.1118), exp_reference(0.724 * 2.0)[1], 64.207_782_683_835_45),
]:
    assert abs(senescence_age(origin, budget, a, b, m) - want) <= 1e-13 * want, (origin, budget, want)
assert abs(accident_age(20.0, 0.7, 1.5e-4) - 4_686.666_666_666_667) <= 1e-13 * 4_686.666_666_666_667
assert senescence_age(40.0, 0.0, 8.73e-6, 0.1118, 1.0) == 40.0 and accident_age(40.0, 0.0, 2.0e-4) == 40.0


class Affliction:
    """A disease a sim has: the disease, the onset in years and, for a lethal one, the
    exposure [used, since]."""

    def __init__(self, disease: Disease, onset: float, since):
        self.disease = disease
        self.onset = onset
        self.exposure = None if disease.hazard is None else [0.0, since]

    def triple(self) -> tuple:
        return (self.disease.key, self.onset, None if self.exposure is None else tuple(self.exposure))


class Event:
    def __init__(self, time, cls, subject, kind, insertion, payload, period, plan):
        self.time = time
        self.cls = cls
        self.subject = subject
        self.kind = kind
        self.insertion = insertion
        self.payload = payload
        self.period = period
        self.plan = plan

    def key(self):
        phase = 1 if self.cls >= OBSERVER else 0
        return (self.time, phase, 0, self.cls, self.subject[0], self.subject[1], self.kind, self.insertion)

    def queue_bytes(self) -> bytes:
        """(time, micro, seq, payload, period), ADR-0002."""
        period = NONE if self.period is None else some(signed(self.period))
        return signed(self.time) + varint(0) + varint(self.insertion) + self.payload + period


def death_payload(sim: int, code_key: int) -> bytes:
    return variant(2, varint(sim), varint(code_key))


def death_key(payload: bytes) -> int:
    """The key of the cause in the payload of a planned death: three varints, the variant
    2, the sim and the key."""
    numbers, value, shift = [], 0, 0
    for byte in payload:
        value |= (byte & 0x7F) << shift
        shift += 7
        if not byte & 0x80:
            numbers.append(value)
            value, shift = 0, 0
    assert shift == 0 and len(numbers) == 3 and numbers[0] == 2, payload.hex()
    return numbers[2]


class World:
    """A started world. `switch` is the switch of the hereditary diseases at the start,
    `founders` a list of (born, sex, alleles) with the sex as postcard bytes and the
    alleles as for `founder_genome`, `couples` a list of (woman, man) by their places
    among the founders, `data` the definitions it runs on (the repository's by default,
    or test data)."""

    def __init__(self, seed: int, tag: str, switch: bool, founders: list, couples=(), fertility_=BARREN, data=None):
        self.seed = seed
        self.tag = tag
        self.switch = switch
        self.data = REPOSITORY if data is None else data
        self.closed = -1
        self.queue = []
        self.next_insertion = 1
        self.next_event_id = 1
        self.next_sim_id = 1
        self.next_partnership_id = 1
        self.partnerships = {}
        self.fertility = fertility_
        self.pregnancies = {}
        # born, lineage key, sex, genome (form of tools/meiosis_reference.py),
        # afflictions, conceptions, parents
        self.sims = {}
        self.archive = {}
        self.log = []
        self.year_hashes = []
        start = variant(
            0,
            varint(seed),
            string(tag),
            world_params(switch),
            seq([founder(born, sex, genes) for born, sex, genes in founders]),
            seq([couple(woman, man) for woman, man in couples]),
            fertility(*fertility_),
        )
        self.inputs = [signed(-1) + varint(1) + start]
        self._start(founders)
        for woman, man in couples:
            self._partner(woman + 1, man + 1, variant(2, varint(1)), 0)

    # ------------------------------------------------------------------ afflictions

    def afflictions_of(self, key: int, sex: bytes, person: dict, origin: int) -> list:
        """The afflictions of a sim at its creation, in data order (design note
        section 1)."""
        found = []
        for disease in self.data.diseases:
            if disease.locus is None:
                continue
            if not has_genotype(disease.locus["action"], copies_at(person, disease.locus)):
                continue
            word = words(chacha_key(self.seed, DISEASE_PENETRANCE, key, disease.key, 0))[0]
            if not bernoulli(word, disease.penetrance[sex]):
                continue
            if disease.onset[0] == "fixed":
                onset = disease.onset[1]
            else:
                _, mean, sd, low, high = disease.onset
                draw = words(chacha_key(self.seed, DISEASE_ONSET, key, disease.key, 0))
                _, x = normal_reference(draw[0], draw[1], mean, sd)
                onset = min(max(x, low), high)
            found.append(Affliction(disease, onset, origin if self.switch else None))
        return found

    def disease_due(self, sim: int, affliction: Affliction):
        """When the sim dies of the lethal affliction if nothing else happens first, or
        None (design note section 4)."""
        born, key = self.sims[sim][0], self.sims[sim][1]
        used, since = affliction.exposure
        if since is None:
            return None
        at_since = as_years(since - born)
        x0 = max(at_since, affliction.onset)
        remaining = max(budget_of(self.seed, key, affliction.disease.code) - used, 0.0)
        # A budget the closed periods used up kills in the second the switch goes on
        # again, whatever hazard the age has then (spec 7.1; review of the design).
        if used > 0.0 and remaining <= 0.0:
            return since if abs(since) <= MAX_ABS_SECS else None
        age = disease_age(x0, remaining, affliction.disease.hazard)
        if age is None:
            return None
        seconds = from_years_ceil(age - at_since)
        if seconds is None:
            return None
        due = since + seconds
        return due if abs(due) <= MAX_ABS_SECS else None

    def cause_seconds(self, sim: int, code: str, nudge=(0, 0, 0)):
        """How many seconds after its origin the sim dies of senescence or accident if
        nothing else happens first, or None (never). `nudge` moves the budget, exp and ln
        by whole ulps."""
        born, key, sex, person = self.sims[sim][:4]
        origin = max(born, 0)
        x0 = as_years(origin - born)
        budget = step_ulps(budget_of(self.seed, key, code), nudge[0])
        if code == SENESCENCE:
            a, b = self.data.senescence[sex]
            x = senescence_age(x0, budget, a, b, self.data.multiplier(person), nudge[1:])
        else:
            assert code == ACCIDENT
            x = accident_age(x0, budget, self.data.accident[sex])
        return from_years_ceil(x - x0)

    def cause_due(self, sim: int, code: str):
        """When the sim dies of `core:senescence` or `core:accident` if nothing else
        happens first, or None: from its origin second (a founder from the start, a child
        from its birth), never beyond the clock."""
        seconds = robust_seconds(lambda nudge: self.cause_seconds(sim, code, nudge))
        if seconds is None:
            return None
        due = max(self.sims[sim][0], 0) + seconds
        return due if abs(due) <= MAX_ABS_SECS else None

    def _plan_deaths(self, sim: int, by: int, now: int) -> None:
        for code in (SENESCENCE, ACCIDENT):
            due = self.cause_due(sim, code)
            if due is not None:
                payload = death_payload(sim, str_key(code))
                self._schedule(due, DEATH, (SIM_SUBJECT, sim), DEATH_KIND, payload, None, by, now)
        self._plan_disease_deaths(sim, by, now)

    def _plan_disease_deaths(self, sim: int, by: int, now: int) -> None:
        for affliction in self.sims[sim][4]:
            if affliction.exposure is None:
                continue
            due = self.disease_due(sim, affliction)
            if due is None:
                continue
            payload = death_payload(sim, affliction.disease.key)
            self._schedule(due, DEATH, (SIM_SUBJECT, sim), DEATH_KIND, payload, None, by, now)

    # ------------------------------------------------------------------ inputs

    def _partner(self, woman: int, man: int, cause: bytes, now: int) -> None:
        number_ = self.next_partnership_id
        self.next_partnership_id += 1
        subjects = [variant(0, varint(woman)), variant(0, varint(man))]
        entry = self._entry(now, subjects, [cause], variant(5, varint(number_)))
        self.partnerships[number_] = [woman, man, now, None]
        self._plan_attempts(woman, entry, now, now)

    def _plan_attempts(self, woman: int, by: int, now: int, from_: int) -> None:
        """The monthly series of a woman in a partnership from `from_` on, if she can
        conceive at all."""
        _, from_years, until_years, _ = self.fertility
        born = self.sims[woman][0]
        earliest = max(from_, born + from_years * YEAR)
        first = -(-earliest // MONTH) * MONTH
        if woman in self.pregnancies or first >= born + until_years * YEAR:
            return
        payload = variant(3, varint(woman))
        self._schedule(first, GENERAL, (SIM_SUBJECT, woman), ATTEMPT_KIND, payload, MONTH, by, now)

    def _attempt(self, event: Event, now: int, plan: bytes) -> None:
        woman = event.subject[1]
        per_month, _, until_years, _ = self.fertility
        her = self.sims[woman]
        man = next(p[1] for p in self.partnerships.values() if p[0] == woman and p[3] is None)
        him = self.sims[man]
        draw = (CONCEPTION_ATTEMPT, her[1], now // MONTH, him[1])
        if not bernoulli(words(chacha_key(self.seed, *draw))[0], per_month):
            # Her last attempt: the next one would be at or after her `until_years` birthday.
            if now + MONTH >= her[0] + until_years * YEAR:
                self.queue.remove(event)
            return
        self.queue.remove(event)
        conception_number = her[5]
        her[5] += 1
        child = child_key(her[1], him[1], conception_number, 0)
        random = lambda key: variant(4, *(varint(word) for word in key))
        conception = self._entry(
            now,
            [variant(0, varint(woman)), variant(0, varint(man))],
            [plan, random(draw)],
            variant(6, varint(child)),
        )
        # The meiosis of the independent reference (tools/meiosis_reference.py, written
        # from the genetics specification 2.3 by another model).
        data = self.data
        person, records = meiosis_reference.child_with_mutation(
            self.seed, child, her[3], him[3], data.map_lengths, data.sex_ratio, data.genome_map, data.rates
        )
        for side, parent in ((0, woman), (1, man)):
            record = records["mother" if side == 0 else "father"]
            segments = seq(
                [
                    segment_meiosis(s["segment"], [MATERNAL, PATERNAL][s["start_strand"]], s["crossover_positions"])
                    for s in record["segments"]
                ]
            )
            chromosome = Y_CHROMOSOME if record.get("sex_chromosome") == "y" else X
            mutations = seq([mutation(m["locus"], m["old"], m["new"]) for m in record["mutations"]])
            # ADR-0004 section 7: the conception and the rule; the draws follow from the
            # genome key and the side.
            causes = [variant(0, varint(conception)), variant(3, string("core:meiosis"))]
            payload = variant(7, varint(child), segments, chromosome, mutations)
            self._entry(now, [variant(0, varint(parent))], causes, payload)
        sex = MALE if "y" in person["paternal_sex"] else FEMALE
        self.pregnancies[woman] = (man, child, conception_number, sex, person, now)
        months = self.fertility[3]
        payload = variant(4, varint(woman))
        self._schedule(now + months * MONTH, BIRTH, (SIM_SUBJECT, woman), BIRTH_KIND, payload, None, conception, now)

    def _birth(self, event: Event, now: int, plan: bytes) -> None:
        mother = event.subject[1]
        father, key, conception_number, sex, person, _ = self.pregnancies.pop(mother)
        child = self.next_sim_id
        self.next_sim_id += 1
        subjects = [variant(0, varint(child)), variant(0, varint(mother)), variant(0, varint(father))]
        born = self._entry(now, subjects, [plan, variant(3, string("core:gestation"))], variant(8, varint(key)))
        afflictions = self.afflictions_of(key, sex, person, now)
        self.sims[child] = [now, key, sex, person, afflictions, 0, (mother, father, conception_number)]
        self._plan_birthday(child, now + YEAR, born, now)
        self._plan_deaths(child, born, now)
        if any(p[0] == mother and p[3] is None for p in self.partnerships.values()):
            self._plan_attempts(mother, born, now, now + 1)

    def arrange(self, woman: int, man: int) -> None:
        """`ArrangePartnership` of two living sims, submitted right now."""
        sequence = len(self.inputs) + 1
        self.inputs.append(signed(self.closed) + varint(sequence) + variant(2, varint(woman), varint(man)))
        self._partner(woman, man, variant(2, varint(sequence)), self.closed + 1)

    def _start(self, founders: list) -> None:
        now = 0
        by_input = variant(2, varint(1))
        started = self._entry(now, [], [by_input], variant(0, varint(len(founders))))
        self._schedule(YEAR - 1, OBSERVER, (WORLD_SUBJECT, 0), YEAR_END_KIND, variant(0), YEAR, started, now)
        for index, (born, sex, genes) in enumerate(founders):
            sim = self.next_sim_id
            self.next_sim_id += 1
            key = founder_key(self.tag, index)
            subject = variant(0, varint(sim))
            added = self._entry(now, [subject], [by_input], variant(1, signed(born), lineage_key(key)))
            person = founder_genome(sex, genes, self.data.loci)
            # A founder's origin is the start (B19).
            afflictions = self.afflictions_of(key, sex, person, now)
            self.sims[sim] = [born, key, sex, person, afflictions, 0, None]
            whole = (now - born) // YEAR
            on_the_day = born + whole * YEAR == now
            age = whole if on_the_day and whole >= 1 else whole + 1
            self._plan_birthday(sim, born + age * YEAR, added, now)
            self._plan_deaths(sim, added, now)

    def override(self, switch: bool) -> None:
        """An override block with the one change, submitted right now."""
        sequence = len(self.inputs) + 1
        self.inputs.append(signed(self.closed) + varint(sequence) + variant(1, seq([param_change(switch)])))
        now = self.closed + 1
        flips = switch != self.switch
        self.switch = switch
        changed = self._entry(now, [], [variant(2, varint(sequence))], variant(3, param_change(switch)))
        if not flips:
            return
        for sim in sorted(self.sims):
            born = self.sims[sim][0]
            lethal = [affliction for affliction in self.sims[sim][4] if affliction.exposure is not None]
            if not lethal:
                continue
            for affliction in lethal:
                if switch:
                    affliction.exposure[1] = now
                    continue
                used, since = affliction.exposure
                from_ = max(as_years(since - born), affliction.onset)
                to = as_years(now - born)
                if to > from_:
                    affliction.exposure[0] = used + disease_hazard_used(affliction.disease.hazard, from_, to)
                affliction.exposure[1] = None
            pending = {death_payload(sim, affliction.disease.key) for affliction in lethal}
            self.queue = [event for event in self.queue if event.payload not in pending]
            self._plan_disease_deaths(sim, changed, now)

    # ------------------------------------------------------------------ events

    def _entry(self, time: int, subjects: list, causes: list, payload: bytes) -> int:
        event_id = self.next_event_id
        self.next_event_id += 1
        self.log.append(varint(event_id) + signed(time) + seq(subjects) + seq(causes) + payload)
        return event_id

    def _schedule(self, time, cls, subject, kind, payload, period, by, planned_at) -> None:
        insertion = self.next_insertion
        self.next_insertion += 1
        self.queue.append(Event(time, cls, subject, kind, insertion, payload, period, (by, planned_at)))

    def _plan_birthday(self, sim: int, time: int, by: int, now: int) -> None:
        self._schedule(time, GENERAL, (SIM_SUBJECT, sim), BIRTHDAY_KIND, variant(1, varint(sim)), None, by, now)

    def advance(self, limit: int) -> None:
        """Runs every event up to and including `limit`."""
        while True:
            due = [event for event in self.queue if event.time <= limit]
            if not due:
                break
            event = min(due, key=Event.key)
            self.queue.remove(event)
            now = event.time
            plan = variant(1, varint(event.plan[0]), signed(event.plan[1]))
            if event.kind == YEAR_END_KIND:
                # The next occurrence is in the queue before the observer looks.
                event.time += event.period
                self.queue.append(event)
                self.year_hashes.append(hash64(self.projection(now - 1)))
            elif event.kind == ATTEMPT_KIND:
                event.time += event.period
                self.queue.append(event)
                self._attempt(event, now, plan)
            elif event.kind == BIRTH_KIND:
                self._birth(event, now, plan)
            elif event.kind == BIRTHDAY_KIND:
                sim = event.subject[1]
                age = (now - self.sims[sim][0]) // YEAR
                subject = variant(0, varint(sim))
                entry = self._entry(now, [subject], [plan], variant(2, varint(age)))
                self._plan_birthday(sim, now + YEAR, entry, now)
            else:
                self._die(event, now, plan)
        self.closed = limit

    def _die(self, event: Event, now: int, plan: bytes) -> None:
        sim = event.subject[1]
        born, key, sex, person, afflictions, conceptions, parents = self.sims.pop(sim)
        codes = [SENESCENCE, ACCIDENT] + [
            affliction.disease.code for affliction in afflictions if affliction.exposure is not None
        ]
        code = next(code for code in codes if event.payload == death_payload(sim, str_key(code)))
        draw = variant(4, varint(DEATH_BUDGET), varint(key), varint(str_key(code)), varint(0))
        self._entry(now, [variant(0, varint(sim))], [plan, draw], variant(4, string(code)))
        self.queue = [other for other in self.queue if other.subject != (SIM_SUBJECT, sim)]
        for couple_ in self.partnerships.values():
            if couple_[3] is None and sim in couple_[:2]:
                couple_[3] = now
                # The partnership ends, and with it her series of attempts.
                attempts = variant(3, varint(couple_[0]))
                self.queue = [other for other in self.queue if other.payload != attempts]
        # Her pregnancy ends with her.
        self.pregnancies.pop(sim, None)
        # The afflictions stay as they were at the death.
        self.archive[sim] = (born, now, str_key(code), key, sex, person, afflictions, conceptions, parents)

    # ------------------------------------------------------------------ views

    def ordered_queue(self) -> list:
        return sorted(self.queue, key=Event.key)

    def projection(self, closed=None) -> bytes:
        closed = self.closed if closed is None else closed
        sims = [
            varint(sim)
            + signed(born)
            + lineage_key(key)
            + sex
            + genome_of(person)
            + afflictions_bytes([affliction.triple() for affliction in afflictions])
            + varint(conceptions)
            + parents_bytes(parents)
            for sim, (born, key, sex, person, afflictions, conceptions, parents) in sorted(self.sims.items())
        ]
        archive = [
            varint(sim)
            + signed(born)
            + signed(left)
            + variant(0, varint(cause))
            + lineage_key(key)
            + sex
            + genome_of(person)
            + afflictions_bytes([affliction.triple() for affliction in afflictions])
            + varint(conceptions)
            + parents_bytes(parents)
            for sim, (born, left, cause, key, sex, person, afflictions, conceptions, parents) in sorted(
                self.archive.items()
            )
        ]
        pregnancies = [
            varint(mother)
            + varint(father)
            + varint(key)
            + varint(conception_number)
            + sex
            + genome_of(person)
            + signed(conceived)
            for mother, (father, key, conception_number, sex, person, conceived) in sorted(self.pregnancies.items())
        ]
        return (
            varint(PROJECTION_VERSION)
            + some(varint(self.seed))
            + some(world_params(self.switch))
            + some(fertility(*self.fertility))
            + signed(closed)
            + varint(self.next_sim_id)
            + varint(self.next_partnership_id)
            + seq([event.queue_bytes() for event in self.ordered_queue()])
            + varint(self.next_insertion)
            + seq(sims)
            + seq([varint(n) + partnership(*value) for n, value in sorted(self.partnerships.items())])
            + seq(pregnancies)
            + seq(archive)
        )

    def state_hash(self) -> int:
        return hash64(self.projection())

    def log_hash(self) -> int:
        return hash64(b"".join(self.log))

    def save(self) -> bytes:
        """Header, projection, exceptions (ADR-0006 section 2)."""
        plans = [varint(event.plan[0]) + signed(event.plan[1]) for event in self.ordered_queue()]
        years = [signed(year) + varint(value) for year, value in enumerate(self.year_hashes)]
        return (
            header(self.data.hash)
            + self.projection()
            + seq(plans)
            + varint(self.next_event_id)
            + seq(self.inputs)
            + seq(years)
        )


def robust_seconds(seconds_of):
    """The seconds of a death that do not hang on the last bit of libm: `seconds_of(nudge)`
    gives the same with the budget, exp and ln moved by one ulp either way, so the pinned
    libm of the core (within an ulp of the correctly rounded value) gives this second."""
    nominal = seconds_of((0, 0, 0))
    for nudge in itertools.product((-1, 0, 1), repeat=3):
        assert seconds_of(nudge) == nominal, f"a due second hangs on the last bit: {nudge}"
    return nominal


def parents_bytes(parents) -> bytes:
    """Option<Parents>, Parents { mother: SimId, father: SimId, conception: u32 }"""
    if parents is None:
        return NONE
    mother, father, conception_number = parents
    return some(varint(mother) + varint(father) + varint(conception_number))


def header(data_hash: int) -> bytes:
    """magic, format_version, core_version, data_hash"""
    return b"SIMAGENT" + varint(FORMAT_VERSION) + varint(CORE_VERSION) + varint(data_hash)


# A world that never started: nothing but the clock and the counters.
EMPTY_PROJECTION = (
    varint(PROJECTION_VERSION)
    + NONE  # world_seed
    + NONE  # world_params
    + NONE  # fertility
    + signed(-1)
    + varint(1)  # next SimId
    + varint(1)  # next PartnershipId
    + seq([])
    + varint(1)
    + seq([])  # sims
    + seq([])  # partnerships
    + seq([])  # pregnancies
    + seq([])  # archive
)
EMPTY_SAVE = header(DATA_HASH) + EMPTY_PROJECTION + seq([]) + varint(1) + seq([]) + seq([])


if __name__ == "__main__":
    print(f"core_version {CORE_VERSION} projection_version {PROJECTION_VERSION}")
    print(f"data_hash_repository {REPOSITORY.hash:016x}")
    print(f"data_hash_fork {FORK_DATA.hash:016x}")
    print("diseases " + " ".join(f"{d.code}={d.key:016x}" for d in REPOSITORY.diseases))
    print(f"causes {SENESCENCE}={str_key(SENESCENCE):016x} {ACCIDENT}={str_key(ACCIDENT):016x}")
    for name, data in (("repository", REPOSITORY), ("fork", FORK_DATA)):
        rates = " ".join(
            f"{sex_name} a {data.senescence[sex][0]!r} b {data.senescence[sex][1]!r} accident {data.accident[sex]!r}"
            for sex, sex_name in ((FEMALE, "female"), (MALE, "male"))
        )
        print(f"death_rates_{name} {rates} sigma_g {data.sigma_g!r} longevity_trait {data.longevity_trait}")
    print("step hazard checks (disease_age, disease_hazard_used): ok")
    print("closed form checks (senescence_age, accident_age against mortality.rs W7.1): ok")

#!/usr/bin/env python3
"""A model of the world of sim-core, written from the ADRs and independent of the Rust
code: the event queue (ADR-0002), the log entries (ADR-0004), the canonical projection
(ADR-0003 5.2), the save (ADR-0006 section 2), death through hazard budgets (ADR-0001)
and the hereditary diseases of the data (genetics specification 3.1 to 3.4, 7.1;
ADR-0017 sections 2, 3 and 5). The goldens and the reference bytes of the tests come
from it.

What the model knows:

- A world runs on its data (ADR-0007): the calibration and the genome map, by default
  those under data/ (`REPOSITORY`). Test data differ from them only in the list
  `diseases` (`dummy_disease_data`); the save names the hash of the data a world runs on.
- `Start` writes `core:started`, plans the year end series (insertion number 1, first at
  the last second of year 0, period one year) and then per founder, in list order: the
  creation entry, the first birthday (the first anniversary at or after the start) and
  the deaths in the order of its causes.
- Events run in the order of their key: time, phase (0 domain, 1 observer), micro step,
  class (general 500, death 900, observer 1000), subject (kind, ID), kind (year end 1,
  birthday 2, death 3), insertion number.
- A birthday writes `core:birthday` and plans the next one a year later. The year end
  observer sees the clock one second before its own second, with its next occurrence
  already in the queue.
- Death: per person and cause a budget `B = exponential(DEATH_BUDGET, 1.0)` at the key
  (tag DEATH_BUDGET, subject lineage key, occasion `str_key` of the cause, detail 0). A
  cause of `Start` (each acts on everyone) with the hazard `r` per year must act
  `ceil(B / r * YEAR)` seconds on a person until the person dies of it, never if `B / r`
  is 100 000 years or more (ADR-0002); it acts on a founder from the start on.
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
- The deaths of a sim are planned in the order of its causes: those of `Start`, then
  its lethal afflictions whose exposure runs, in data order. The death writes
  `core:death` with the plan and the draw as causes, moves the sim with its afflictions
  as they are to the archive and cancels its other pending events.
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

import json
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
    death_cause,
    fertility,
    genome_of,
    f64,
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

import meiosis_reference

ROOT = Path(__file__).resolve().parent.parent

# The map lengths and the sex ratio at birth of data/calibration/genetics.json. Test data
# differ from the repository only in `diseases`, so these hold for every world here.
MAP_LENGTHS = meiosis_reference.load_map_lengths()
SEX_RATIO = meiosis_reference.load_sex_ratio()


def _definitions():
    """The genome map of data/genetics/genome_map.json in the form of the meiosis
    reference, and the mutation rates of data/calibration/genetics.json."""
    root = ROOT / "data"
    loci = json.loads((root / "genetics" / "genome_map.json").read_text())["loci"]
    values = json.loads((root / "calibration" / "genetics.json").read_text())["values"]
    genome_map = [
        {
            "id": locus["id"],
            "chromosome": locus["chromosome"],
            "segment": locus["segment"] if locus["chromosome"] == "autosome" else (8 if locus["chromosome"] == "x" else 0),
            "slot": locus["slot"],
            "mark": locus["mark"],
            "direction": locus["mutation"],
        }
        for locus in loci
    ]
    rates = {"standard": values["core:mutation_rate"]["value"], "disease": values["core:mutation_rate_disease"]["value"]}
    return genome_map, rates


GENOME_MAP, RATES = _definitions()


YEAR = 364 * 86_400
MONTH = 28 * 86_400
NEVER_YEARS = 100_000.0
# The range of the clock (ADR-0002): a time beyond it is never.
MAX_ABS_SECS = 1_000_000 * YEAR
# tests/golden/rng_tags.txt
MEIOSIS_START_STRAND, SEX_CHROMOSOME, CONCEPTION_ATTEMPT, DEATH_BUDGET = 1, 2, 3, 4
DISEASE_PENETRANCE, DISEASE_ONSET = 13, 14

PROJECTION_VERSION = 11
CORE_VERSION = 23

# A scenario without its own fertility: nobody ever conceives.
BARREN = (0.0, 18, 45, 10)
FORMAT_VERSION = 1

NONE = b"\x00"

# Event classes and kinds (tests/golden/event_codes.txt).
BIRTH, GENERAL, DEATH, OBSERVER = 200, 500, 900, 1000
YEAR_END_KIND, BIRTHDAY_KIND, DEATH_KIND, ATTEMPT_KIND, BIRTH_KIND = 1, 2, 3, 4, 5
WORLD_SUBJECT, SIM_SUBJECT = 0, 1
# The causes of `Start` act on everyone, AppliesTo variant 0.
EVERYONE = 0


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


class Data:
    """The definitions a world runs on: the calibration and the genome map as documents
    (parsed JSON with their texts), their data hash (tools/data_reference.py), the loci
    by ID and the diseases in data order."""

    def __init__(self, calibration: dict, genome_map: dict):
        self.hash = data_reference.documents_hash(calibration, genome_map)
        self.loci = {locus["id"]: locus for locus in genome_map["loci"]}
        stripped = data_reference.strip(calibration)
        self.diseases = [Disease(entry, self.loci) for entry in stripped["diseases"]]


def repository_data() -> Data:
    return Data(*data_reference.documents(ROOT / "data"))


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


def dummy_disease_data(onset_years: float) -> Data:
    """The repository's calibration and map with `diseases` replaced by the dummy disease
    alone: onset 0 is the "dummy disease data" of the tests that had the old cause for the
    homozygous (it acted from the origin on), onset 18 the "fork data" of the acceptance
    scenario `fork_no_density`."""
    calibration, genome_map = data_reference.documents(ROOT / "data")
    calibration["diseases"] = [dummy_disease(onset_years)]
    return Data(calibration, genome_map)


REPOSITORY = repository_data()
DUMMY_DISEASE_DATA = dummy_disease_data(0.0)
FORK_DATA = dummy_disease_data(18.0)
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


def needed_seconds(seed: int, key: int, code: str, per_year: float):
    """How long a cause of `Start` must act until the person with this lineage key dies
    of it."""
    word = words(chacha_key(seed, DEATH_BUDGET, key, str_key(code), 0))[0]
    _, budget = exponential_reference(word, 1.0)
    years = budget / per_year
    if years >= NEVER_YEARS:
        return None
    return math.ceil(years * 31_449_600.0)


def budget_of(seed: int, key: int, code: str) -> float:
    """The budget `B` of the cause or disease with this code for this lineage key."""
    return exponential_reference(words(chacha_key(seed, DEATH_BUDGET, key, str_key(code), 0))[0], 1.0)[1]


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


class World:
    """A started world. `switch` is the switch of the hereditary diseases at the start,
    `causes` a list of (code, hazard per year) for everyone, `founders` a list of (born,
    sex, alleles) with the sex as postcard bytes and the alleles as for `founder_genome`,
    `couples` a list of (woman, man) by their places among the founders, `data` the
    definitions it runs on."""

    def __init__(
        self, seed: int, tag: str, switch: bool, causes: list, founders: list, couples=(), fertility_=BARREN, data=None
    ):
        self.seed = seed
        self.tag = tag
        self.switch = switch
        self.data = REPOSITORY if data is None else data
        assert all(len(cause) == 2 for cause in causes), "a cause of Start acts on everyone"
        self.causes = list(causes)
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
            seq([death_cause(code, rate) for code, rate in self.causes]),
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

    def cause_due(self, sim: int, cause: tuple):
        """When the sim dies of the cause of `Start` if nothing else happens first."""
        code, rate = cause
        born, key = self.sims[sim][0], self.sims[sim][1]
        needed = needed_seconds(self.seed, key, code, rate)
        if needed is None:
            return None
        due = max(born, 0) + needed  # from the birth on, a founder from the start
        return due if abs(due) <= MAX_ABS_SECS else None

    def _plan_deaths(self, sim: int, by: int, now: int) -> None:
        for cause in self.causes:
            due = self.cause_due(sim, cause)
            if due is not None:
                payload = death_payload(sim, str_key(cause[0]))
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
        person, records = meiosis_reference.child_with_mutation(
            self.seed, child, her[3], him[3], MAP_LENGTHS, SEX_RATIO, GENOME_MAP, RATES
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
        codes = [code for code, _ in self.causes] + [
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
            + seq([varint(str_key(code)) + f64(rate) + variant(EVERYONE) for code, rate in self.causes])
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
    + seq([])  # death_causes
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
    print(f"data_hash_repository {REPOSITORY.hash:016x}")
    print(f"data_hash_dummy_disease {DUMMY_DISEASE_DATA.hash:016x}")
    print(f"data_hash_fork {FORK_DATA.hash:016x}")
    print("diseases " + " ".join(f"{d.code}={d.key:016x}" for d in REPOSITORY.diseases))
    print("step hazard checks (disease_age, disease_hazard_used): ok")

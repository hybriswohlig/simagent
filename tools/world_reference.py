#!/usr/bin/env python3
"""A model of the world of sim-core, written from the ADRs and independent of the Rust
code: the event queue (ADR-0002), the log entries (ADR-0004), the canonical projection
(ADR-0003 5.2), the save (ADR-0006 section 2), death through hazard budgets (ADR-0001)
by senescence and accident from the data (genetics specification 7.1, 7.2, 11.4;
ADR-0017 section 2) and the hereditary diseases of the data (genetics specification 3.1
to 3.4, 7.1; ADR-0017 sections 2, 3 and 5). The goldens and the reference bytes of the
tests come from it.

What the model knows:

- A world runs on its data (ADR-0007): the life course data, the calibration and the
  genome map as documents, by default those under data/ (`REPOSITORY`). Test data
  (`death_test_data`) differ from them only in the rates of senescence and accident, the
  list `diseases` and, for a scenario that gave a fertility in `Start` before, the
  fertility in the form `window` (design note of the fertility in the world, section 8:
  the rate, the window, the months of pregnancy, no pause, the wish always 40, no
  twins; no separation, design note of the separation, section 7; and the market off,
  `c` = 0, design note of the market, section 7), and for the acceptance scenario the
  market off and no separation (ADR-0017 section 5); `life_test_data` changes values of
  the life course data only (the scenario "market"; "similar" on the fixture
  tests/golden/data_pairing); the save names the hash of the data a world runs on. The
  map lengths, the sex ratio, the map in the form of the meiosis reference and the
  mutation rates come from the documents too.
- `Start` (world seed, founder tag, world parameters, founders, couples; no causes of
  death and no fertility any more, ADR-0017 sections 2 and 4) writes `core:started`,
  plans the year end series (insertion number 1, first at the last second of year 0,
  period one year), the series of the market rounds (insertion number 2, first at MONTH,
  period MONTH; design note of the market, section 2) and then per founder, in list
  order: the creation entry, the first birthday (the first anniversary at or after the
  start) and the deaths in the order of its causes. A couple with a founder under 16 at
  the start (`0 − born < 16 · YEAR`) rejects `Start` with `partner_too_young`
  (`start_rejection`).
- Events run in the order of their key: time, phase (0 domain, 1 observer), micro step,
  class (birth 200, general 500, death 900, observer 1000), subject (kind: world 0, sim 1,
  partnership 2; ID), kind (year end 1, birthday 2, death 3, attempt 4, birth 5,
  separation 6, market round 7), insertion number. In one second: births, the round
  (500, the world), the attempts (500, sims), separations (500, partnerships), deaths.
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
  between two living sims, unless it is rejected (`arrange_rejection`, in this order:
  `unknown_sim`, `sim_not_alive`, `partners_of_wrong_sex`, `already_partnered`, each for
  both, then `partner_too_young` (either under 16 in the second the input acts, the stamp
  plus one) and `partners_too_closely_related` (`φ ≥ 1/8`); a rejected input changes
  nothing and takes no sequence number). Each gets the next PartnershipId and one entry
  `core:partnered` (subjects the woman and the man; cause the input). A death ends the open partnership
  of the sim: it keeps the second of the death and the reason `Death`
  (`ended: Option<Ending { at, by }>`, design note of the separation, section 2).
- Separation (ADR-0018 section 3, design note of the separation): `λ` is
  `core:separation_rate` of the life course data (per year). A woman holds `partnerships`,
  the number of partnerships she has formed (0 for every founder and newborn, always 0
  for a man), right after `births`. When her partnership forms at `f`, `k` is her
  `partnerships`, which then grows by one; after her series (so its insertion number
  comes after the series') the event `separation { partnership }` (class general 500,
  subject the partnership, kind 6) is planned by the entry `core:partnered` at
  `f + max(from_years_ceil(B / λ), 1 s)` with `B = exponential(SEPARATION_BUDGET, 1)` at
  (SEPARATION_BUDGET = 18, her lineage key, occasion `k`, detail his lineage key); nothing
  if `λ` is 0, if `from_years_ceil` says never or the time leaves the clock. A separation
  at `t` ends the partnership (`Ending { at: t, by: Separation }`) and her series (a
  running pregnancy stays: the child is born with its father, and no series follows) and
  writes `core:separated { partnership }` (subjects the woman and the man, causes the plan
  and the draw). The death of either partner cancels the pending separation; one due in
  the second of a death comes first (class 500 before 900).
- The partner market (genetics specification 11.2, ADR-0018 section 1, design note of the
  market): the values of the life course data (`core:market_age` 16, `c`, `m`, the width
  `w` 7, `ρ`, `k` 12, `w_a`, `μ_a`, the limit 1/8, the odds of a cousin 10). A round at
  `t` (`n = t / MONTH`): the living men without an open partnership with `t − born ≥ 16
  · YEAR`, held for the round; every such woman searches iff `bernoulli(p)` at
  (MARKET_SEARCH = 19, her key, `n`, 0) with `p = s_f` (`ρ · s_f` while `t − born < 18 ·
  YEAR`), `s_f = min(1, c · exp(((−0.5) · z) · z))`, `z = (a − m) / w`, `a` her age in
  years (`search_chance`); the searchers in the order of `(rank(MARKET_ORDER = 20,
  WORLD, n, her key), her key)`; each in turn walks the men by `(rank(PAIRING_RANK = 5,
  her key, n, his key), his key)`, skipping `φ ≥ 1/8`, until `k` are kept; the utility
  `U = ((−(w_a · (d · d))) − (K · φ)) + G` with `d = Δa − μ_a`, `Δa` his age minus hers,
  `K = 16 · ln(odds)`, `G = gumbel` of (PAIRING_NOISE = 21, her key, `n`, his key); the
  largest `U` wins, a tie the earlier in the list; if he has no open partnership now the
  partnership forms at `t` as any pairing (entry `core:partnered` with the causes the
  plan of the round, the rule `core:partner_market` and the draw; her series from the
  next month start; the separation), else nothing. `φ` is the kinship of the genetics
  specification 2.7 over the pedigree of every sim, living or archived (the recursion of
  `spec_reference.Pedigree`), exact. Each search draw keeps `DRAW_MARGIN` from its chance
  and the best utility `UTILITY_MARGIN` from every other, so the pinned libm of the core
  makes the same choices. `market=False` (a switch of this reference only) plans no
  round: the null twin.
- Similarity (genetics specification 11.2, design note of the similarity): the calibration
  (format 7) names `cognition_trait` and `height_trait`, each a trait of the map with
  phenotype weights or none. For a named trait the utility gets its term before the age
  term: with `dK = z_K,i − z_K,j` and `dH = z_h,i − z_h,j` the penalties in the order of
  11.2 are `w_K · (dK · dK)` (`core:pairing_weight_cognition`), `w_H · (dH · dH)`
  (`core:pairing_weight_height`), `w_a · (d · d)`, `K · φ`, and `U = (((−p1) − p2) − … −
  pn) + G` over the present terms left to right; a term whose trait is not named is left
  out, so without both traits `U` is the market's. `z` is the phenotype of the genetics
  specification 4.2 (ADR-0008 section 3, ADR-0009 sections 3 and 4; `phenotype_parts`) of
  the sim at the round, from its genome, its lineage key, its parents' keys (its own for
  both as a founder) and its age `t − born` in seconds; exp, ln and cos correctly rounded,
  sqrt exact. The woman's once per round, each candidate's once per candidate.
- Fertility (genetics specification 5.6 and 11.3, ADR-0017 section 4, design note of the
  fertility in the world): the data give the form of the curves (`puberty` or `window`),
  the adult rate, the pause after a birth and the months of pregnancy (whole months),
  the wish shape and its tilt. Per woman, pure and not held: her menarche
  `menarche(θ_i, δ, form)` with `δ` the draw FERTILITY_MENARCHE (15; her lineage key,
  occasion 0, detail 0; the form `puberty` only) and her wish `wish(u, shape, tilt)` with
  `u` the draw FERTILITY_WISH (16; the same key). `θ_i` of a sim is that of the curve of
  its sex with `z_p` = 0 (the data of these tests name no trait of the puberty timing;
  this model refuses one). The end of her fertility `E` is `female_decline_to` (puberty)
  or `until` (window). A sim holds `births`, the children it bore alive (0 for every
  founder, every newborn and every man), right after `conceptions`.
- The series of attempts (design note section 4): a woman has at most one monthly series
  (period MONTH). It is planned at `s` if and only if, then, she has an open
  partnership, is not pregnant, `births < wish` and `age(w, s) < E`: after a partnership
  forms at `f` (a couple of `Start` at 0, `ArrangePartnership` in the second after its
  stamp) at `s` = the first month start strictly after `f`, by the entry
  `core:partnered`; after a birth at `b` in an open partnership at `s` = the first month
  start at or after `b + max(pause · MONTH, 1 s)`, by the entry `core:birth`. It ends
  when she conceives, when the partnership ends, or after an attempt at `t` that fails
  when `age(w, t + MONTH) >= E`.
- An attempt at `t` (design note section 5): with `m` her partner in the open
  partnership, `p = chance(adult_rate, female_factor(age(w, t), menarche(w)),
  male_factor(age(m, t), θ_i(m)))`, drawn `bernoulli(p)` at (CONCEPTION_ATTEMPT, her
  lineage key, the month index of `t`, his lineage key); a chance of 0 draws like any
  other and fails. Each draw is checked not to lie within 1e-12 of its chance (and each
  wish draw not within 1e-12 of a threshold), so the pinned libm of the core gives the
  same outcome. On success she conceives her conception number n.
- Twins (genetics specification 11.3, ADR-0017 section 4, design note of the twins): at
  every conception `u = unit(draw)` at (MULTIPLE_BIRTH = 17, the lineage key of the first
  child `child(her, his, n, 0)`, occasion 0, detail 0); with `p_id` =
  `core:twins_identical` and `p_fr` = `core:twins_fraternal` of the life course data,
  identical twins if `u < p_id`, fraternal twins if `u < p_id + p_fr` (the sum as
  written), else one child; no triplets. (A unit draw against a data value needs no
  margin: no libm enters.) Child `i` (0, and 1 for twins) has the key `child(her, his, n,
  i)`. A zygote under a genome key gives, per parent, the start strands
  `uniform_below(MEIOSIS_START_STRAND, 2)` (subject the genome key, occasion 0 mother and 1
  father) and the sex `bernoulli(SEX_CHROMOSOME, p_Y)` (occasion 1, true for a son;
  `p_Y = r / (1 + r)` from `core:sex_ratio_at_birth`, genetics specification 2.3 and
  11.3). One child: one zygote under its own key. Identical twins: one zygote under the
  key of child 0, the same genome and sex for both. Fraternal twins: one zygote per child
  under its own key.
- The entries of a conception, all with the same subjects (her, him): per child in index
  order `core:conception` (its key; causes the plan of the attempt, the attempt draw, the
  draw of MULTIPLE_BIRTH, in this order, the same for both twins); after the entry of a
  child with its own zygote (child 0; child 1 of fraternal twins) the two `core:meiosis`
  of that zygote, her side then his (causes the event of that conception entry and the
  rule `core:meiosis`; payload the genome key of the zygote). Identical twins share the
  two meioses of child 0. The series ends; the pregnancy is state (father, conception
  number, the children as (key, sex, genome) in index order, conceived), and one birth is
  planned `pregnancy_months` months later (class birth 200, by the conception entry of
  child 0). The death of either ends the series; her death ends the pregnancy and forgets
  all its keys.
- Birth: per child in index order a new sim with the next SimId and its creation entry
  `core:birth` (subjects child, mother, father; the plan and the rule `core:gestation`),
  the entries one after the other; each born now, with parents and the conception
  number, `births` 0, `partnerships` 0, its afflictions from its genome and its own key
  with the birth as origin. Then per child in index order its first birthday a year
  later and its deaths from its birth on, planned by its own creation entry. The mother's `births` grows by
  the number of children; then, if she is in an open partnership, her series after a
  birth, planned by the creation entry of child 0. The wish is checked only when a
  series is planned, so twins of the last birth may exceed it by one.

Usage: .venv/bin/python tools/world_reference.py (prints the data hashes, the checks of
the step hazard and pure values of the market; the other tools use the model)
"""

import copy
import itertools
import math
from fractions import Fraction
from pathlib import Path

import mpmath
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
    gumbel_reference,
    normal_reference,
    rank,
    str_key,
    unit,
    words,
)

from kat_reference import cos_reference, exp_reference, ln_reference

import meiosis_reference

# The kinship of the genetics specification 2.7 (the recursion of tools/spec_reference.py
# over a pedigree, exact as fractions). That module sets the precision of mpmath to 40
# digits when it is imported; the correctly rounded exp and ln here need the 60 of
# tools/kat_reference.py, so the precision is put back (Pedigree reads no mpmath).
_PRECISION = mpmath.mp.dps
from spec_reference import Pedigree  # noqa: E402

mpmath.mp.dps = _PRECISION
assert mpmath.mp.dps == 60

ROOT = Path(__file__).resolve().parent.parent


YEAR = 364 * 86_400
MONTH = 28 * 86_400
NEVER_YEARS = 100_000.0
# The range of the clock (ADR-0002): a time beyond it is never.
MAX_ABS_SECS = 1_000_000 * YEAR
# tests/golden/rng_tags.txt
MEIOSIS_START_STRAND, SEX_CHROMOSOME, CONCEPTION_ATTEMPT, DEATH_BUDGET = 1, 2, 3, 4
DISEASE_PENETRANCE, DISEASE_ONSET = 13, 14
# New with the fertility in the world (design note of the fertility in the world, section
# 1; genetics specification 11.7).
FERTILITY_MENARCHE, FERTILITY_WISH = 15, 16
# New with the twins (design note of the twins, section 1; genetics specification 11.7).
MULTIPLE_BIRTH = 17
# New with the separation (design note of the separation, section 1; ADR-0018 section 3).
SEPARATION_BUDGET = 18
# The partner market (design note of the market, section 1; genetics specification 11.7):
# PAIRING_RANK is old, the other three are new.
PAIRING_RANK = 5
MARKET_SEARCH, MARKET_ORDER, PAIRING_NOISE = 19, 20, 21
# SubjectKey::WORLD, the subject of the worldwide rank MARKET_ORDER (ADR-0018 section 1).
WORLD_KEY = 0
# The draws of the environment of a phenotype (genetics specification 4.2, ADR-0009 section
# 3): the shared one `C_t` and the individual one `E_t`.
ENV_SHARED, ENV_INDIVIDUAL = 10, 11
# The rule a pairing of the market names as its cause (design note of the market, section
# 3, step 6).
PARTNER_MARKET_RULE = "core:partner_market"
# The margin the best utility keeps from every other of the list, so that the last bits
# of `gumbel` (2^-45 against the reference, ADR-0018 section 6) and of `ln` cannot change
# the choice.
UTILITY_MARGIN = 1e-9

# The projection is that of the separation (design note of the separation, section 2): the
# round is a queue entry only, and the phenotypes of the similarity terms are pure
# functions, not state (ADR-0009 section 4). The core version goes up with the market (30)
# and with the similarity terms (31: the calibration of format 7 changes the data hash of
# the repository, design note of the similarity, section 1).
PROJECTION_VERSION = 15
CORE_VERSION = 31

FORMAT_VERSION = 1
# The margin a draw keeps from what it is compared with (the chance of an attempt, a
# threshold of the wish), so that a last bit of libm in the core cannot flip it.
DRAW_MARGIN = 1e-12

NONE = b"\x00"

# Event classes and kinds (tests/golden/event_codes.txt).
BIRTH, GENERAL, DEATH, OBSERVER = 200, 500, 900, 1000
YEAR_END_KIND, BIRTHDAY_KIND, DEATH_KIND, ATTEMPT_KIND, BIRTH_KIND = 1, 2, 3, 4, 5
# New with the separation (design note of the separation, section 3).
SEPARATION_KIND = 6
# New with the market (design note of the market, section 2): a periodic world event.
MARKET_ROUND_KIND = 7
# The queue payload of the round: the next variant after Separation (5), without fields.
MARKET_ROUND_PAYLOAD = variant(6)
WORLD_SUBJECT, SIM_SUBJECT, PARTNERSHIP_SUBJECT = 0, 1, 2
# Reject codes of the inputs (tests/golden/log_codes.txt); the two of the market come at
# the end of the list (design note of the market, section 4).
UNKNOWN_FOUNDER, UNKNOWN_SIM, SIM_NOT_ALIVE, PARTNERS_OF_WRONG_SEX, ALREADY_PARTNERED = 13, 9, 10, 11, 12
PARTNER_TOO_YOUNG, PARTNERS_TOO_CLOSELY_RELATED = 21, 22
REJECT_NAMES = {
    UNKNOWN_SIM: "unknown_sim",
    SIM_NOT_ALIVE: "sim_not_alive",
    PARTNERS_OF_WRONG_SEX: "partners_of_wrong_sex",
    ALREADY_PARTNERED: "already_partnered",
    UNKNOWN_FOUNDER: "unknown_founder",
    PARTNER_TOO_YOUNG: "partner_too_young",
    PARTNERS_TOO_CLOSELY_RELATED: "partners_too_closely_related",
}
# EndedBy (design note of the separation, section 2): the variant index after the time.
ENDED_BY_DEATH, ENDED_BY_SEPARATION = 0, 1
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


class FertilityForm:
    """The calibration's `fertility` (ADR-0017 section 1; design note of the fertility
    curves, section 1) without its texts. `kind` "puberty": the twelve numbers of the
    genetics specification 5.6 as attributes of their names (`menarche_mean` to
    `male_decline_to`; years, `male_floor` a fraction). `kind` "window" (test data only,
    ADR-0017 section 5): `start` (the field `from`) and `until` in years."""

    def __init__(self, entry: dict):
        self.kind, fields = only(entry)
        names = data_reference.FERTILITY_FIELDS[data_reference.FERTILITY_FORMS.index(self.kind)]
        assert set(fields) == set(names), f"fertility {self.kind}: fields {sorted(fields)}"
        if self.kind == "puberty":
            for name in names:
                setattr(self, name, number(fields[name]))
        else:
            self.start, self.until = number(fields["from"]), number(fields["until"])


class PhenotypeTrait:
    """A trait of the map read as a phenotype (genetics specification 4.1, 4.2; ADR-0008
    sections 1 and 3, ADR-0009): its data ID and `str_key` (the occasion of its environment
    draws), `sigma_ref`, the groups of its genetic value (first the loci that load on it,
    then per included trait in the order of `includes` the loci that load on that one; each
    group in map order, each locus with its coefficient `λ · β`, or `(w · λ) · β` in an
    included group, and `d_ref`), and its phenotype weights `(h²_child, c²_child,
    h²_adult, c²_adult)` (the calibration's row)."""

    def __init__(self, trait_id: str, genome_map: dict, row: dict):
        traits = {entry["id"]: entry for entry in genome_map["traits"]}
        entry = traits[trait_id]
        self.id = trait_id
        self.key = str_key(trait_id)
        self.sigma_ref = float(entry["sigma_ref"])
        assert self.sigma_ref > 0.0

        def group(target: str, include_weight):
            loci = []
            for locus in genome_map["loci"]:
                action = locus["action"]
                if isinstance(action, str):
                    continue
                ((kind, weights),) = action.items()
                assert kind in ("additive", "threshold") and locus["chromosome"] == "autosome"
                for load in weights["loads"]:
                    if load["trait"] != target:
                        continue
                    lam, beta = float(load["weight"]), float(weights["effect"])
                    coefficient = lam * beta if include_weight is None else (include_weight * lam) * beta
                    loci.append((locus, coefficient, float(weights["d_ref"])))
            return loci

        self.groups = [group(trait_id, None)] + [
            group(include["trait"], float(include["weight"])) for include in entry["includes"]
        ]
        self.weights = tuple(number(row[name]) for name in data_reference.WEIGHTS)


def dose(person: dict, locus: dict) -> float:
    """The dose `d` of the locus (genetics specification 2.1): the copies with allele 1, as
    f64."""
    return float(sum(copies_at(person, locus)))


def genetic_value(person: dict, trait: PhenotypeTrait) -> float:
    """`G_t = A_t / σ_t,ref` (ADR-0008 section 3): `A_t` folded from 0.0, group by group,
    each locus in map order adding `coefficient · (d − d_ref)`."""
    acc = 0.0
    for loci in trait.groups:
        for locus, coefficient, d_ref in loci:
            acc = acc + coefficient * (dose(person, locus) - d_ref)
    return acc / trait.sigma_ref


def held_weight(value: float) -> float:
    """A weight held in 0 to 1 before its root (ADR-0009 section 4): 0 if it is not greater
    than 0 (so -0.0 becomes +0.0), 1 if it is greater than 1."""
    if not value > 0.0:
        return 0.0
    return 1.0 if value > 1.0 else value


def standard_normal(seed: int, tag: int, subject: int, occasion: int, detail: int) -> float:
    """`normal(0, 1)` at the key (tools/rng_reference.py: `0 + 1 · (sqrt(−2 ln(u0)) ·
    cos(2π u1))` with the correctly rounded ln and cos)."""
    draw = words(chacha_key(seed, tag, subject, occasion, detail))
    return normal_reference(draw[0], draw[1], 0.0, 1.0)[1]


def phenotype_parts(seed: int, trait: PhenotypeTrait, person: dict, key: int, parents, age_seconds: int) -> dict:
    """The phenotype `z_t` of a sim at the age `age_seconds` (a duration from its birth)
    and its parts (genetics specification 4.2, ADR-0009 sections 3 and 4): `x =
    as_years(age)`, `s = 1 / (1 + exp((12 − x) / 2))` with the correctly rounded exp, `h2 =
    h2_child + (h2_adult − h2_child) · s` and `c2` likewise, `e2 = (1 − h2) − c2`, each held in
    0 to 1; `C` = normal(ENV_SHARED, the mother's lineage key, `str_key` of the trait, the
    father's lineage key), with its own key for both without parents (a founder); `E` =
    normal(ENV_INDIVIDUAL, its own key, `str_key` of the trait, 0); `z = ((sqrt(h2) · G) +
    (sqrt(c2) · C)) + (sqrt(e2) · E)`. `parents` is (mother's key, father's key) or None."""
    g = genetic_value(person, trait)
    x = as_years(age_seconds)
    s = 1.0 / (1.0 + exp_reference((12.0 - x) / 2.0)[1])
    h2_child, c2_child, h2_adult, c2_adult = trait.weights
    h2 = h2_child + (h2_adult - h2_child) * s
    c2 = c2_child + (c2_adult - c2_child) * s
    e2 = (1.0 - h2) - c2
    h2, c2, e2 = held_weight(h2), held_weight(c2), held_weight(e2)
    mother, father = (key, key) if parents is None else parents
    shared = standard_normal(seed, ENV_SHARED, mother, trait.key, father)
    own = standard_normal(seed, ENV_INDIVIDUAL, key, trait.key, 0)
    z = ((math.sqrt(h2) * g) + (math.sqrt(c2) * shared)) + (math.sqrt(e2) * own)
    return {"G": g, "x": x, "s": s, "h2": h2, "c2": c2, "e2": e2, "C": shared, "E": own, "z": z, "env": (mother, father)}


def phenotype(seed: int, trait: PhenotypeTrait, person: dict, key: int, parents, age_seconds: int) -> float:
    """`z_t` of `phenotype_parts`."""
    return phenotype_parts(seed, trait, person, key, parents, age_seconds)["z"]


def libm_margin(name: str, x: float) -> float:
    """How far the exact value of exp, ln or cos at `x` lies from the rounding boundary of
    its f64, in ulps of the result (0.5 at an f64, 0 at a midpoint): only where this is
    small can a libm that is not correctly rounded differ in the last bit."""
    function = {"exp": (exp_reference, mpmath.exp), "ln": (ln_reference, mpmath.log), "cos": (cos_reference, mpmath.cos)}[name]
    kind, value = function[0](x)
    if kind == "E":
        return 0.5
    with mpmath.workdps(60):
        exact = function[1](mpmath.mpf(x))
        return float(0.5 - abs(exact - mpmath.mpf(value)) / mpmath.mpf(math.ulp(value)))


def phenotype_margins(seed: int, trait: PhenotypeTrait, key: int, parents, age_seconds: int) -> list:
    """The margins (`libm_margin`) of the exp of `s` and of the ln and cos of both
    environment draws of a phenotype."""
    from rng_reference import TAU, unit_open0

    x = as_years(age_seconds)
    margins = [("exp", libm_margin("exp", (12.0 - x) / 2.0))]
    mother, father = (key, key) if parents is None else parents
    for tag, address in ((ENV_SHARED, (mother, trait.key, father)), (ENV_INDIVIDUAL, (key, trait.key, 0))):
        draw = words(chacha_key(seed, tag, *address))
        margins.append(("ln", libm_margin("ln", unit_open0(draw[0]))))
        margins.append(("cos", libm_margin("cos", TAU * unit(draw[1]))))
    return margins


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


def whole_months(entry) -> int:
    """A number of the life course data in months that the world counts in whole months
    (the pause after a birth, the months of pregnancy): its value as an integer. This
    model takes whole months only (a fraction would need a rounding rule)."""
    assert entry["unit"] == "month", entry
    value = number(entry)
    assert value >= 0.0 and value.is_integer(), f"{value} is no whole number of months"
    return int(value)


def whole_number(entry, unit: str) -> int:
    """A number of the life course data the market counts whole (the market age in years,
    the number of candidates): its value as an integer."""
    assert entry["unit"] == unit, entry
    value = number(entry)
    assert value >= 0.0 and value.is_integer(), f"{value} is no whole number"
    return int(value)


class Data:
    """The definitions a world runs on: the life course data, the calibration and the
    genome map as documents (parsed JSON with their texts, in the order of their data
    IDs), their data hash (tools/data_reference.py), the loci
    by ID, the diseases in data order, the rates of death per sex (Gompertz `a` and `b`,
    the Makeham term `λ`), `σ_g` and the trait of longevity, the fertility (its form, the
    end of a woman's fertility `E`, the trait of the puberty timing and per sex the curve
    `(θ, σ_θ)` of 5.2 that `θ_i` reads; from the life course data the adult rate, the
    pause after a birth and the months of pregnancy, the wish shape and its tilt, and the
    rates of twins `(p_id, p_fr)`), and what the meiosis reference reads:
    the map lengths, the sex ratio at birth, the map in its form and the mutation rates; the
    market's values with the weights of the similarity terms and the traits of cognition
    and height (`PhenotypeTrait` or None)."""

    def __init__(self, demography: dict, calibration: dict, genome_map: dict):
        self.hash = data_reference.documents_hash(demography, calibration, genome_map)
        self.loci = {locus["id"]: locus for locus in genome_map["loci"]}
        life = data_reference.strip(demography)
        self.adult_rate = number(life["values"]["core:fertility_adult_rate"])
        self.pause_months = whole_months(life["values"]["core:fertility_pause"])
        self.pregnancy_months = whole_months(life["values"]["core:pregnancy_length"])
        assert self.pregnancy_months >= 1
        self.wish_tilt = number(life["values"]["core:fertility_wish_tilt"])
        self.wish_shape = [number(entry) for entry in life["fertility_wish_shape"]]
        self.twins = (number(life["values"]["core:twins_identical"]), number(life["values"]["core:twins_fraternal"]))
        assert all(0.0 <= rate <= 1.0 for rate in self.twins), self.twins
        # `λ` of the separation, per year (design note of the separation, section 1); 0 is
        # a world without separation.
        self.separation_rate = number(life["values"]["core:separation_rate"])
        assert math.isfinite(self.separation_rate) and self.separation_rate >= 0.0, self.separation_rate
        # The market (design note of the market, section 1; genetics specification 11.2):
        # the market age in whole years, `c`, `m`, the width `w`, `ρ`, `k`, `w_a`, `μ_a`, the
        # limit of the kinship and the odds of a cousin.
        market = life["values"]
        self.market_age = whole_number(market["core:market_age"], "year")
        self.market_height = number(market["core:market_search_height_female"])
        self.market_center = number(market["core:market_search_center_female"])
        self.market_width = number(market["core:market_search_width"])
        self.market_minor = number(market["core:market_search_factor_minor"])
        self.market_candidates = whole_number(market["core:market_candidates"], "count")
        self.pairing_weight_age = number(market["core:pairing_weight_age"])
        self.pairing_gap = number(market["core:pairing_age_gap_preferred"])
        self.kinship_limit = number(market["core:pairing_kinship_limit"])
        self.cousin_odds = number(market["core:pairing_cousin_odds"])
        # The weights of the similarity terms (genetics specification 11.2; design note of the
        # similarity, section 1): per squared difference of the phenotypes.
        self.pairing_weight_cognition = number(market["core:pairing_weight_cognition"])
        self.pairing_weight_height = number(market["core:pairing_weight_height"])
        assert self.market_age == 16 and self.kinship_limit == 0.125, "the note fixes 16 years and 1/8"
        for weight in (self.pairing_weight_cognition, self.pairing_weight_height):
            assert math.isfinite(weight) and weight >= 0.0, weight
        assert math.isfinite(self.market_height) and self.market_height >= 0.0, self.market_height
        assert math.isfinite(self.market_center) and self.market_width > 0.0 and math.isfinite(self.market_width)
        assert 0.0 < self.market_minor <= 1.0 and self.market_candidates >= 1
        assert math.isfinite(self.pairing_weight_age) and self.pairing_weight_age >= 0.0
        assert math.isfinite(self.pairing_gap) and math.isfinite(self.cousin_odds) and self.cousin_odds >= 1.0
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
        # Likewise `θ_i` with a trait of the puberty timing needs its phenotype `z_p`; the
        # data of these tests name none (`z_p` = 0, `θ_i = θ` exactly, 11.1).
        self.puberty_trait = stripped["puberty_trait"]
        assert self.puberty_trait is None, "a trait of the puberty timing is not modelled by this reference"
        # The traits of the similarity terms of the market (design note of the similarity,
        # sections 1 and 2): each a trait of the map with phenotype weights, or None (the
        # term is left out, 11.1).
        self.cognition_trait = self.phenotype_trait(stripped["cognition_trait"], stripped, genome_map)
        self.height_trait = self.phenotype_trait(stripped["height_trait"], stripped, genome_map)
        self.fertility = FertilityForm(stripped["fertility"])
        # The end of her fertility (design note of the fertility in the world, section 2):
        # from this age on her factor stays 0.
        if self.fertility.kind == "puberty":
            self.fertility_end = self.fertility.female_decline_to
        else:
            self.fertility_end = self.fertility.until
        self.puberty_curve = {
            sex: (number(values[f"core:growth_theta_{name}"]), number(values[f"core:growth_theta_sd_{name}"]))
            for sex, name in ((FEMALE, "female"), (MALE, "male"))
        }
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

    @staticmethod
    def phenotype_trait(named, calibration: dict, genome_map: dict):
        """The trait a field of the calibration names, as `PhenotypeTrait`, or None."""
        if named is None:
            return None
        (row,) = [row for row in calibration["traits"] if row["trait"] == named]
        return PhenotypeTrait(named, data_reference.strip(genome_map), row)

    def similarity_traits(self) -> list:
        """The present similarity terms in the order of 11.2: (name, trait, weight) for
        cognition, then height, each only if its trait is named."""
        terms = [("cognition", self.cognition_trait, self.pairing_weight_cognition), ("height", self.height_trait, self.pairing_weight_height)]
        return [term for term in terms if term[1] is not None]


def repository_data() -> Data:
    return Data(*data_reference.documents(ROOT / "data"))


def test_value(value: float, unit: str) -> dict:
    """A number of test data with its texts (they do not enter the data hash)."""
    return {
        "value": value,
        "unit": unit,
        "source": "test data (ADR-0017 section 5)",
        "as_of": "2026-10-11",
        "note": "test value, not calibrated",
    }


# The wish of the test data with the window: always 40 (41 weights for 0 to 40 children,
# all 0 but the last), so the wish never stops a series (design note of the fertility in
# the world, section 8; ADR-0017 section 5).
TEST_WISH = 40


def window_fertility(demography: dict, calibration: dict, fertility: tuple) -> None:
    """The fertility of a scenario that gave one in `Start` (design note of the fertility in
    the world, section 8), written into the documents: `fertility` is (per month `r`, from
    `a`, until `b` years, `k` months) as the old `Start.fertility`; the form `window` from
    `a` until `b`, `core:fertility_adult_rate` = `r`, `core:pregnancy_length` = `k`,
    `core:fertility_pause` 0, the wish shape of `TEST_WISH` + 1 weights all 0 but the last
    1 (the tilt stays), both rates of twins 0, `core:separation_rate` 0 (the old model
    knows no separation, ADR-0017 section 5; design note of the separation, section 7) and
    `core:market_search_height_female` (`c`) 0: no woman ever searches, the round runs and
    writes nothing (design note of the market, section 7)."""
    per_month, start, until, months = fertility
    assert per_month > 0.0
    calibration["fertility"] = {
        "window": {"from": test_value(float(start), "year"), "until": test_value(float(until), "year")}
    }
    values = demography["values"]
    values["core:fertility_adult_rate"]["value"] = per_month
    values["core:pregnancy_length"]["value"] = float(months)
    values["core:fertility_pause"]["value"] = 0.0
    values["core:twins_identical"]["value"] = 0.0
    values["core:twins_fraternal"]["value"] = 0.0
    values["core:separation_rate"]["value"] = 0.0
    values["core:market_search_height_female"]["value"] = 0.0
    demography["fertility_wish_shape"] = [test_value(0.0, "fraction") for _ in range(TEST_WISH)] + [
        test_value(1.0, "fraction")
    ]


def death_test_data(
    accident: float,
    senescence_a: float = 1e-15,
    senescence_b: float = 0.001,
    diseases=None,
    fertility=None,
    life_values=None,
    wish_shape=None,
) -> Data:
    """Test data for the deaths (design note of the death from the data, section 4): the
    repository's life course data, calibration and map with the Makeham term `accident`
    and the Gompertz `a`, `b` for both sexes, and `diseases` (a list of disease
    documents) instead of the repository's six if given. A scenario that had causes of `Start` takes the rate of
    its first cause as `accident` and that of its second as `senescence_a` with `b` =
    0.001; without a second cause `a` = 1e-15 and `b` = 0.001, senescence practically
    off (its death lies some 20 000 years ahead). A scenario that had a fertility in
    `Start` gives it as `fertility` (`window_fertility`); otherwise the fertility is the
    repository's. `life_values` (a dict of code to value) sets values of the life course
    data after that, `wish_shape` (a list of weights) the wish shape."""
    demography, calibration, genome_map = data_reference.documents(ROOT / "data")
    values = calibration["values"]
    for sex in ("female", "male"):
        values[f"core:accident_{sex}"]["value"] = accident
        values[f"core:senescence_a_{sex}"]["value"] = senescence_a
        values[f"core:senescence_b_{sex}"]["value"] = senescence_b
    if diseases is not None:
        calibration["diseases"] = diseases
    if fertility is not None:
        window_fertility(demography, calibration, fertility)
    for code, value in (life_values or {}).items():
        demography["values"][code]["value"] = value
    if wish_shape is not None:
        demography["fertility_wish_shape"] = [test_value(float(weight), "fraction") for weight in wish_shape]
    return Data(demography, calibration, genome_map)


def life_test_data(life_values: dict, data_dir: Path = ROOT / "data") -> Data:
    """The repository's data (or those of the fixture `data_dir`, which take the
    repository's life course data) with values of the life course data changed
    (`life_values`, a dict of code to value) and nothing else: the rates of death, the
    diseases and the fertility stay those of the directory (the scenario "market", design
    note of the market, section 7; "similar" on tests/golden/data_pairing, design note of
    the similarity, section 4)."""
    demography, calibration, genome_map = data_reference.documents(data_dir)
    for code, value in life_values.items():
        demography["values"][code]["value"] = value
    return Data(demography, calibration, genome_map)


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
# the dummy disease from 18; the fertility `window` from 18 until 45 with 0.05 per month,
# ten months of pregnancy, no pause, the wish always 40, no twins (design note of the
# fertility in the world, section 8); the market off (`c` = 0) and no separation.
FORK_FERTILITY = (0.05, 18, 45, 10)
FORK_LIFE_VALUES = {"core:market_search_height_female": 0.0, "core:separation_rate": 0.0}
FORK_DATA = death_test_data(
    0.02, 1e-12, 0.1, [dummy_disease(18.0)], fertility=FORK_FERTILITY, life_values=FORK_LIFE_VALUES
)
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


def copy_genome(person: dict) -> dict:
    """A genome of the form of tools/meiosis_reference.py, copied (identical twins have
    equal genomes, each its own)."""
    return copy.deepcopy(person)


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


# ---------------------------------------------------------------------------------------
# Fertility (genetics specification 5.6 and 11.3; design note of the fertility curves,
# section 2): pure functions, ages in years as f64 (11.1: `(t − born).as_years()`). The
# world reads them in its attempts (design note of the fertility in the world).


def month_start_after(t: int) -> int:
    """The smallest `k · MONTH > t`, k an integer (design note of the fertility in the
    world, section 2)."""
    return (t // MONTH + 1) * MONTH


def month_start_at_or_after(t: int) -> int:
    """The smallest `k · MONTH >= t`, k an integer."""
    return -(-t // MONTH) * MONTH


assert month_start_after(0) == MONTH and month_start_after(-1) == 0 and month_start_after(MONTH - 1) == MONTH
assert month_start_at_or_after(0) == 0 and month_start_at_or_after(1) == MONTH and month_start_at_or_after(-1) == 0
assert month_start_at_or_after(-MONTH - 1) == -MONTH and month_start_after(-MONTH) == 0

# The bound of the inputs of the individual curve (ADR-0012 section 1).
CURVE_INPUT_BOUND = 6.0


def smoothstep(u: float) -> float:
    """`s(u)` of 5.1 and 6.2 as `growth::puberty_progress` evaluates it: 0 for `u <= 0`,
    1 for `u >= 1`, else `3.0 * (u * u) - 2.0 * ((u * u) * u)`."""
    if u <= 0.0:
        return 0.0
    if u >= 1.0:
        return 1.0
    return 3.0 * (u * u) - 2.0 * ((u * u) * u)


def theta_i(curve: tuple, z_p: float) -> float:
    """The puberty timing `θ_i = θ + σ_θ · ẑ_p` (5.2) on the curve `(θ, σ_θ)` of a sex
    (`Data.puberty_curve`), with `ẑ_p` the phenotype `z_p` held on [−6, 6] (above 6 is 6,
    below −6 is −6, ADR-0012 section 1): `IndividualCurve::new(curve, 0.0, z_p).theta`.
    Without a trait of the puberty timing `z_p` is 0 and `θ_i = θ` exactly (11.1)."""
    theta, sigma_theta = curve
    if z_p > CURVE_INPUT_BOUND:
        z_p = CURVE_INPUT_BOUND
    elif z_p < -CURVE_INPUT_BOUND:
        z_p = -CURVE_INPUT_BOUND
    return theta + sigma_theta * z_p


def menarche(theta: float, delta: float, form: FertilityForm) -> float:
    """The menarche `m_i` of a woman with the puberty timing `theta` (5.6): `m = θ_i + δ`,
    then held, `m = max(m, θ_i + menarche_low)`, then `m = min(m, θ_i + menarche_high)`.
    `δ` is `menarche_delta`; the menarche is a pure function, not state."""
    assert form.kind == "puberty"
    m = theta + delta
    m = max(m, theta + form.menarche_low)
    return min(m, theta + form.menarche_high)


def menarche_delta(seed: int, key: int, form: FertilityForm) -> float:
    """`δ = normal(menarche_mean, menarche_sd)` at the key (FERTILITY_MENARCHE, the
    woman's lineage key, occasion 0, detail 0; 11.7)."""
    draw = words(chacha_key(seed, FERTILITY_MENARCHE, key, 0, 0))
    return normal_reference(draw[0], draw[1], form.menarche_mean, form.menarche_sd)[1]


def female_factor(age: float, m: float, form: FertilityForm) -> float:
    """The woman's factor `F_w` at the age `age` (5.6). `puberty`: the ramp
    `s((age − m) / female_ramp)` from her menarche `m`, times the decline, 1 for
    `age <= female_decline_from`, 0 for `age >= female_decline_to`, else
    `(to − age) / (to − from)`. `window`: 1 if `from <= age < until`, else 0 (`m` is not
    read)."""
    if form.kind == "window":
        return 1.0 if form.start <= age < form.until else 0.0
    ramp = smoothstep((age - m) / form.female_ramp)
    start, end = form.female_decline_from, form.female_decline_to
    if age <= start:
        decline = 1.0
    elif age >= end:
        decline = 0.0
    else:
        decline = (end - age) / (end - start)
    return ramp * decline


def male_factor(age: float, theta: float, form: FertilityForm) -> float:
    """The man's factor `F_m` at the age `age` with his puberty timing `theta` (5.6).
    `puberty`: the ramp `s((age − (θ_i − male_lead)) / male_ramp)`, times the decline, 1
    for `age <= male_decline_from`, `male_floor` for `age >= male_decline_to`, else
    `1.0 - (1.0 - floor) * ((age - from) / (to - from))`. `window`: 1."""
    if form.kind == "window":
        return 1.0
    ramp = smoothstep((age - (theta - form.male_lead)) / form.male_ramp)
    start, end, floor = form.male_decline_from, form.male_decline_to, form.male_floor
    if age <= start:
        decline = 1.0
    elif age >= end:
        decline = floor
    else:
        decline = 1.0 - (1.0 - floor) * ((age - start) / (end - start))
    return ramp * decline


def chance(adult_rate: float, f_w: float, f_m: float) -> float:
    """The chance of an attempt (11.3): `(adult_rate * f_w) * f_m`."""
    return (adult_rate * f_w) * f_m


def wish_weights(shape: list, alpha: float) -> tuple:
    """`(w, total)` of the wish (11.3): `w_n = p_n * exp(α * n)` with `n` as f64 and the
    correctly rounded exp (`sim_math::exp`), `total = w_0 + w_1 + …` in order."""
    weights = [p * exp_reference(alpha * float(n))[1] for n, p in enumerate(shape)]
    total = 0.0
    for w in weights:
        total += w
    return weights, total


def wish_thresholds(shape: list, alpha: float) -> list:
    """The thresholds of the wish (11.3): `acc += w_n / total` in order, `acc` after each
    `n`."""
    weights, total = wish_weights(shape, alpha)
    thresholds, acc = [], 0.0
    for w in weights:
        acc += w / total
        thresholds.append(acc)
    return thresholds


def wish(u: float, shape: list, alpha: float) -> int:
    """The wish of a woman (11.3): the smallest `n` with `u < acc` after `n`
    (`wish_thresholds`), the last `n` if none."""
    for n, acc in enumerate(wish_thresholds(shape, alpha)):
        if u < acc:
            return n
    return len(shape) - 1


def wish_unit(seed: int, key: int) -> float:
    """`u = unit(draw)` of the wish at the key (FERTILITY_WISH, the woman's lineage key,
    occasion 0, detail 0; 11.7)."""
    return unit(words(chacha_key(seed, FERTILITY_WISH, key, 0, 0))[0])


SINGLE, IDENTICAL, FRATERNAL = "single", "identical", "fraternal"


def multiple_unit(seed: int, first_child: int) -> float:
    """`u = unit(draw)` of the twins at the key (MULTIPLE_BIRTH, the lineage key of the
    first child of the conception, occasion 0, detail 0; 11.7)."""
    return unit(words(chacha_key(seed, MULTIPLE_BIRTH, first_child, 0, 0))[0])


def multiple_kind(u: float, twins: tuple) -> str:
    """The kind of a conception (11.3, design note of the twins section 1): identical if
    `u < p_id`, fraternal if `u < p_id + p_fr` (the sum computed as written), else one
    child."""
    p_id, p_fr = twins
    if u < p_id:
        return IDENTICAL
    if u < p_id + p_fr:
        return FRATERNAL
    return SINGLE


assert multiple_kind(0.0, (0.0, 0.0)) == SINGLE and multiple_kind(0.0039, (0.004, 0.010)) == IDENTICAL
assert multiple_kind(0.004, (0.004, 0.010)) == FRATERNAL and multiple_kind(0.014, (0.004, 0.010)) == SINGLE
assert multiple_kind(0.3, (0.3, 0.7)) == FRATERNAL and multiple_kind(0.9999999, (0.3, 0.7)) == FRATERNAL


def wish_mean(shape: list, alpha: float) -> float:
    """The expected wish `Σ n · w_n / total`: the products `n · w_n` summed in order,
    divided by `total` once."""
    weights, total = wish_weights(shape, alpha)
    acc = 0.0
    for n, w in enumerate(weights):
        acc += float(n) * w
    return acc / total


def check_fertility_spec(form: FertilityForm, curves: dict, shape: list) -> None:
    """The values the genetics specification states, by hand: `s(1/2) = 3/4 − 2/8 = 1/2`,
    so 1.5 years after the menarche (before 30) the woman's factor is 0.5 [W5.6a]; with
    40 the ramp is over for `m <= 37` and the decline is `(50 − 40) / (50 − 30) = 0.5`
    [W5.6b]; with 57.5 the man's ramp is over and his decline is
    `1 − 0.6 · 12.5 / 25 = 0.7` [W5.6c]; at `α` = 0 the wish has the shape's mean
    `0.15 + 0.80 + 0.75 + 0.40 = 2.10` [W11.3a]."""
    assert form.kind == "puberty"
    assert smoothstep(0.5) == 0.5
    assert female_factor(12.5 + 1.5, 12.5, form) == 0.5
    for tenth in range(60, 280):
        m = tenth / 10.0
        assert abs(female_factor(m + 1.5, m, form) - 0.5) <= 1e-12, m
    for tenth in range(60, 371):
        assert female_factor(40.0, tenth / 10.0, form) == 0.5, tenth
    for curve in curves.values():
        for z_p in (-7.0, -6.0, -1.0, 0.0, 1.0, 6.0, 7.0):
            assert male_factor(57.5, theta_i(curve, z_p), form) == 0.7, (curve, z_p)
    assert abs(wish_mean(shape, 0.0) - 2.10) <= 1e-12


# The wish shape p_0 to p_4 of the genetics specification 11.3 [K11.3d].
WISH_SHAPE_SPEC = [0.10, 0.15, 0.40, 0.25, 0.10]
check_fertility_spec(REPOSITORY.fertility, REPOSITORY.puberty_curve, WISH_SHAPE_SPEC)


# ---------------------------------------------------------------------------------------
# The partner market (genetics specification 11.2; design note of the market, section 3):
# pure functions of the data, ages as `(t − born).as_years()`.


def search_chance(age_seconds: int, data: Data) -> float:
    """The chance that a single woman of `age_seconds` (at least the market age) searches in
    a round (step 2): `a = as_years(age)`, `z = (a − m) / w`, `s = c · exp(((−0.5) · z) · z)`
    with the correctly rounded exp, `s_f = s` if `s < 1` else 1, times `ρ` while she is
    under 18 (integer seconds)."""
    a = as_years(age_seconds)
    z = (a - data.market_center) / data.market_width
    s = data.market_height * exp_reference(((-0.5) * z) * z)[1]
    s_f = s if s < 1.0 else 1.0
    return data.market_minor * s_f if age_seconds < 18 * YEAR else s_f


def kinship_weight(data: Data) -> float:
    """`K = 16 · ln(cousin_odds)` with the correctly rounded ln (step 5)."""
    return 16.0 * ln_reference(data.cousin_odds)[1]


def penalties(gap: float, phi: float, data: Data, similarity=()) -> list:
    """The penalty terms of a candidate in the order of 11.2 (design note of the similarity,
    section 2): the similarity terms given (`w_K · (dK · dK)` if a trait of cognition is
    named, then `w_H · (dH · dH)` if one of height is: `similarity` is the list of the
    present (weight, difference) pairs in that order), then `w_a · (d · d)` with `d = Δa −
    μ_a`, then `K · φ`."""
    d = gap - data.pairing_gap
    terms = [weight * (difference * difference) for weight, difference in similarity]
    return terms + [data.pairing_weight_age * (d * d), kinship_weight(data) * phi]


def utility_of(terms: list, noise: float) -> float:
    """`U = (((−p1) − p2) − … − pn) + G` over the present terms, left to right."""
    acc = -terms[0]
    for term in terms[1:]:
        acc = acc - term
    return acc + noise


def utility(gap: float, phi: float, noise: float, data: Data, similarity=()) -> float:
    """The utility of a candidate (step 5; design note of the similarity, section 2). Without
    similarity terms `U = ((−(w_a · (d · d))) − (K · φ)) + G`, the market note exactly."""
    return utility_of(penalties(gap, phi, data, similarity), noise)


def ranked(seed: int, tag: int, subject: int, n: int, keys: list) -> list:
    """The keys ascending by `(rank(tag, subject, n, key), key)` (ADR-0001 section 5,
    `rank = absorb(RANK, 0, [seed, tag, subject, n, key])`)."""
    return sorted(keys, key=lambda key: (rank(seed, tag, subject, n, key), key))


def noise_of(seed: int, her_key: int, n: int, his_key: int) -> float:
    """`G = gumbel(w)` of the draw PAIRING_NOISE (her key, `n`, his key): `0 − ln(0 −
    ln(unit_open(w)))` with the correctly rounded ln (tools/rng_reference.py)."""
    return gumbel_reference(words(chacha_key(seed, PAIRING_NOISE, her_key, n, his_key))[0])[1]


# Spec 11.2 by hand: a cousin's penalty is ln(10) = 2.303 [W11.2a], so with the same rest a
# stranger is chosen ten times as often; at φ = 1/32 the odds are √10 = 3.162 [W11.2b].
assert abs(kinship_weight(REPOSITORY) / 16.0 - math.log(10.0)) <= 1e-15
assert abs(utility(3.0, 0.0625, 0.0, REPOSITORY) - utility(3.0, 0.0, 0.0, REPOSITORY) + math.log(10.0)) <= 1e-14
assert abs(math.exp(kinship_weight(REPOSITORY) / 32.0) - 3.1622776601683795) <= 1e-14
assert search_chance(18 * YEAR, life_test_data({"core:market_search_height_female": 0.0})) == 0.0


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
    or test data). A sim is [born, lineage key, sex, genome (form of
    tools/meiosis_reference.py), afflictions, conceptions, births, parents, partnerships];
    an archived one (born, left, key of the cause, lineage key, sex, genome, afflictions,
    conceptions, births, parents, partnerships) (`partnerships` last in these lists, after
    `births` in the projection); a partnership [woman, man, formed, ended] with `ended`
    None or (at, ENDED_BY_DEATH or ENDED_BY_SEPARATION); a pregnancy, by the mother,
    (father, conception number, children as [(lineage key, sex, genome)] in index order,
    conceived)."""

    def __init__(self, seed: int, tag: str, switch: bool, founders: list, couples=(), data=None, market=True):
        self.seed = seed
        self.tag = tag
        self.switch = switch
        self.data = REPOSITORY if data is None else data
        # `market` False is a switch of this reference only, not of the core: no round is
        # planned (the null twin of the market, design note of the market, section 7).
        self.market = market
        rejection = start_rejection(founders, couples, self.data)
        assert rejection is None, f"Start is rejected: {REJECT_NAMES[rejection]}"
        # The pedigree of every sim, living or archived, for the kinship (spec 2.7): not
        # state, it follows from the parents; a cache of its values, keyed by the pair.
        self.pedigree = Pedigree()
        self.kinships = {}
        # Every event that ran (time, kind), for the coverage of the goldens; every round
        # with at least one single woman in the market age (its record, see
        # `_market_round`); the census at each year end (`census`); not state.
        self.ran = []
        self.rounds = []
        self.census_hashes = []
        # The payload variant of every log entry, in log order (the coverage of the log
        # kinds); not state.
        self.entry_kinds = []
        # The accepted inputs ArrangePartnership as (the second they acted, woman, man), and
        # the couples of Start as SimIds: what a loader reads from the input log.
        self.input_pairings = [(0, woman + 1, man + 1) for woman, man in couples]
        self.closed = -1
        self.queue = []
        self.next_insertion = 1
        self.next_event_id = 1
        self.next_sim_id = 1
        self.next_partnership_id = 1
        self.partnerships = {}
        self.pregnancies = {}
        self.sims = {}
        self.archive = {}
        self.log = []
        self.year_hashes = []
        # Every attempt (time, woman, man, chance, success) and every conception (time,
        # woman, man, conception number, the draw u of MULTIPLE_BIRTH, the kind, the
        # children as (key, sex)), for the stories of the tools; not state.
        self.attempts = []
        self.multiples = []
        # Every separation planned (partnership, k, the budget B, due or None) and every
        # one that came (time, partnership, its entry, the plan (by, planned_at), the key of
        # the draw), for the stories of the tools; not state.
        self.separation_plans = []
        self.separations = []
        start = variant(
            0,
            varint(seed),
            string(tag),
            world_params(switch),
            seq([founder(born, sex, genes) for born, sex, genes in founders]),
            seq([couple(woman, man) for woman, man in couples]),
        )
        self.inputs = [signed(-1) + varint(1) + start]
        self._start(founders)
        for woman, man in couples:
            self._partner(woman + 1, man + 1, [variant(2, varint(1))], 0)

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

    def _partner(self, woman: int, man: int, causes: list, now: int) -> int:
        """A partnership of the woman and the man at `now`, its entry `core:partnered` with
        `causes` (the input; or the plan of the round, the rule and the draw)."""
        assert self.sims[woman][2] == FEMALE and self.sims[man][2] == MALE
        assert self.partner_of(woman) is None and self.open_partnership(man) is None
        # Partnerships never begin below the market age, never between close kin (design
        # note of the market, section 4; ADR-0018 section 5).
        assert all(now - self.sims[sim][0] >= self.data.market_age * YEAR for sim in (woman, man))
        assert self.kinship(woman, man) < Fraction(self.data.kinship_limit)
        number_ = self.next_partnership_id
        self.next_partnership_id += 1
        subjects = [variant(0, varint(woman)), variant(0, varint(man))]
        entry = self._entry(now, subjects, causes, variant(5, varint(number_)))
        self.partnerships[number_] = [woman, man, now, None]
        # Her counter (design note of the separation, section 3): `k` before, then one
        # more.
        k = self.sims[woman][8]
        self.sims[woman][8] += 1
        # The series after a partnership forms (design note of the fertility in the world,
        # section 4), then the separation, after her attempts.
        self._plan_series(woman, month_start_after(now), entry, now)
        self._plan_separation(number_, k, entry, now)
        return entry

    # ------------------------------------------------------------------ separation

    def separation_budget(self, woman_key: int, k: int, man_key: int) -> float:
        """`B = exponential(SEPARATION_BUDGET, 1)` at (SEPARATION_BUDGET, her lineage key,
        `k`, his lineage key) (design note of the separation, section 1)."""
        return exponential_reference(words(chacha_key(self.seed, SEPARATION_BUDGET, woman_key, k, man_key))[0], 1.0)[1]

    def separation_seconds(self, budget: float):
        """`max(from_years_ceil(B / λ), 1 s)` (design note section 3), or None: `λ` = 0 or
        never. Each due second is checked not to hang on the last bit of the budget (the
        only libm in it, `ln`), so the pinned libm of the core gives the same second."""
        rate = self.data.separation_rate
        if rate == 0.0:
            return None

        def seconds(ulps: int):
            years = step_ulps(budget, ulps) / rate
            whole = from_years_ceil(years)
            return None if whole is None else max(whole, 1)

        nominal = seconds(0)
        assert all(seconds(ulps) == nominal for ulps in (-1, 1)), f"a separation hangs on the last bit of {budget!r}"
        return nominal

    def separation_due(self, number_: int):
        """When the partnership separates if nothing else happens first, or None (design
        note section 3; section 6: `k` = the partnerships of the same woman with a smaller
        PartnershipId)."""
        woman, man, formed, _ = self.partnerships[number_]
        k = sum(1 for other, value in self.partnerships.items() if other < number_ and value[0] == woman)
        seconds = self.separation_seconds(self.separation_budget(self.key_of(woman), k, self.key_of(man)))
        if seconds is None:
            return None
        due = formed + seconds
        return due if abs(due) <= MAX_ABS_SECS else None

    def _plan_separation(self, number_: int, k: int, by: int, now: int) -> None:
        """The event `separation { partnership }` (variant 5, class general, subject the
        partnership, kind 6) at its due, planned by the entry `by` (design note section
        3)."""
        woman, man = self.partnerships[number_][:2]
        budget = self.separation_budget(self.key_of(woman), k, self.key_of(man))
        due = self.separation_due(number_)
        self.separation_plans.append((number_, k, budget, due))
        if due is None:
            return
        payload = variant(5, varint(number_))
        self._schedule(due, GENERAL, (PARTNERSHIP_SUBJECT, number_), SEPARATION_KIND, payload, None, by, now)

    def open_partnership(self, sim: int):
        """The number of the open partnership of the sim (woman or man), or None."""
        open_ = [n for n, p in self.partnerships.items() if sim in p[:2] and p[3] is None]
        assert len(open_) <= 1
        return open_[0] if open_ else None

    def pending_separation(self, number_: int):
        """The time of the pending separation of the partnership, or None."""
        times = [e.time for e in self.queue if e.kind == SEPARATION_KIND and e.subject == (PARTNERSHIP_SUBJECT, number_)]
        assert len(times) <= 1
        return times[0] if times else None

    def _separate(self, event: Event, now: int, plan: bytes) -> None:
        """A separation at `now` (design note section 4): the partnership ends by
        separation, her series ends (a running pregnancy stays), one entry
        `core:separated { partnership }` (variant 9) with the subjects her and him and the
        causes the plan and the draw."""
        number_ = event.subject[1]
        couple_ = self.partnerships[number_]
        woman, man, _, ended = couple_
        assert ended is None
        couple_[3] = (now, ENDED_BY_SEPARATION)
        attempts = variant(3, varint(woman))
        self.queue = [other for other in self.queue if other.payload != attempts]
        k = sum(1 for other, value in self.partnerships.items() if other < number_ and value[0] == woman)
        key = (SEPARATION_BUDGET, self.key_of(woman), k, self.key_of(man))
        draw = variant(4, *(varint(word) for word in key))
        subjects = [variant(0, varint(woman)), variant(0, varint(man))]
        entry = self._entry(now, subjects, [plan, draw], variant(9, varint(number_)))
        self.separations.append((now, number_, entry, event.plan, key))

    # ------------------------------------------------------------------ market

    def kinship(self, x: int, y: int) -> Fraction:
        """`φ(x, y)` of the genetics specification 2.7 over all known ancestors, living or
        archived (missing parents count 0, founders are unrelated): the recursion of
        `spec_reference.Pedigree` on the SimIds, the later SimId the younger. Exact."""
        pair = (min(x, y), max(x, y))
        if pair not in self.kinships:
            self.kinships[pair] = self.pedigree.phi(x, y)
        return self.kinships[pair]

    def phi_float(self, x: int, y: int) -> float:
        """φ as f64: a sum of powers of two, exact."""
        phi = self.kinship(x, y)
        value = float(phi)
        assert Fraction(value) == phi
        return value

    def born_of(self, sim: int) -> int:
        return self.sims[sim][0] if sim in self.sims else self.archive[sim][0]

    def parent_keys(self, sim: int):
        """(the mother's lineage key, the father's) of a sim born in the run, or None (a
        founder)."""
        parents = self.sims[sim][7] if sim in self.sims else self.archive[sim][9]
        if parents is None:
            return None
        return self.key_of(parents[0]), self.key_of(parents[1])

    def phenotype_parts(self, sim: int, trait: PhenotypeTrait, now: int) -> dict:
        """The phenotype of the trait of a living sim at `now` and its parts
        (`phenotype_parts`): its genome, its lineage key, its parents' keys, its age `now −
        born` in seconds."""
        born, key, _, person = self.sims[sim][:4]
        return phenotype_parts(self.seed, trait, person, key, self.parent_keys(sim), now - born)

    def phenotypes(self, sim: int, now: int) -> list:
        """(weight, z) of each present similarity term (cognition, then height) for a living
        sim at `now`."""
        return [(weight, self.phenotype_parts(sim, trait, now)["z"]) for _, trait, weight in self.data.similarity_traits()]

    def sex_of(self, sim: int) -> bytes:
        return self.sims[sim][2] if sim in self.sims else self.archive[sim][4]

    def _market_round(self, now: int, plan: bytes) -> None:
        """A round at `now` (design note of the market, section 3), `n = now / MONTH`: the
        single men in the market age, held for the round; the single women in it who
        search (MARKET_SEARCH); their order (MARKET_ORDER); per searcher in turn her
        candidates (PAIRING_RANK, skipping `φ ≥ 1/8`, up to `k`), the utility and the
        choice; a pairing if the chosen man has no open partnership now."""
        data = self.data
        n = now // MONTH
        assert now == n * MONTH and n >= 1
        adult = data.market_age * YEAR

        def single_in_market(sim: int, sex: bytes) -> bool:
            born, _, sim_sex = self.sims[sim][:3]
            return sim_sex == sex and self.open_partnership(sim) is None and now - born >= adult

        men = [sim for sim in sorted(self.sims) if single_in_market(sim, MALE)]
        women = [sim for sim in sorted(self.sims) if single_in_market(sim, FEMALE)]
        if not women:
            return
        considered, searchers = [], []
        for woman in women:
            age = now - self.sims[woman][0]
            p = search_chance(age, data)
            word = words(chacha_key(self.seed, MARKET_SEARCH, self.key_of(woman), n, 0))[0]
            u = unit(word)
            # A chance from exp keeps the margin; 0 and 1 cannot be moved by libm.
            assert p in (0.0, 1.0) or abs(u - p) > DRAW_MARGIN, f"a search hangs on the last bits of {p!r}"
            searched = bernoulli(word, p)
            considered.append((woman, age, p, u, searched, age < 18 * YEAR))
            if searched:
                searchers.append(woman)
        order = ranked(self.seed, MARKET_ORDER, WORLD_KEY, n, [self.key_of(woman) for woman in searchers])
        by_key = {self.key_of(woman): woman for woman in searchers}
        boys = [sim for sim in sorted(self.sims) if self.sims[sim][2] == MALE and now - self.sims[sim][0] < adult]
        record = {"t": now, "n": n, "men": men, "considered": considered, "boys": boys, "choices": []}
        self.rounds.append(record)
        for key in order:
            record["choices"].append(self._choose(by_key[key], men, now, n, plan))

    def _choose(self, woman: int, men: list, now: int, n: int, plan: bytes) -> dict:
        """Her candidates, the utilities and her choice (design note of the market, section
        3, steps 4 to 6); the record of it."""
        data = self.data
        her_key, her_born = self.key_of(woman), self.sims[woman][0]
        by_key = {self.key_of(man): man for man in men}
        walked, kept = [], []
        for key in ranked(self.seed, PAIRING_RANK, her_key, n, list(by_key)):
            if len(kept) == data.market_candidates:
                break
            man = by_key[key]
            phi = self.kinship(woman, man)
            skipped = phi >= Fraction(data.kinship_limit)
            walked.append((man, phi, skipped))
            if not skipped:
                kept.append(man)
        record = {"woman": woman, "walked": walked, "candidates": [], "chosen": None, "outcome": "no candidate", "terms": []}
        if not kept:
            return record
        # Her phenotypes once (design note of the similarity, section 2), each candidate's
        # once per candidate; the similarity terms only for the named traits.
        hers = self.phenotypes(woman, now)
        record["phenotypes"] = {woman: [z for _, z in hers]}
        best = None
        for index, man in enumerate(kept):
            gap = as_years(now - self.sims[man][0]) - as_years(now - her_born)
            phi = self.phi_float(woman, man)
            g = noise_of(self.seed, her_key, n, self.key_of(man))
            his = self.phenotypes(man, now)
            record["phenotypes"][man] = [z for _, z in his]
            similarity = [(weight, z_her - z_his) for (weight, z_her), (_, z_his) in zip(hers, his)]
            terms = penalties(gap, phi, data, similarity)
            u = utility_of(terms, g)
            assert u == utility(gap, phi, g, data, similarity)
            assert math.isfinite(u), "market.utility"
            record["candidates"].append((man, gap, phi, g, u))
            record["terms"].append(terms)
            # The largest; on a tie the earlier in the list (the smaller rank).
            if best is None or u > record["candidates"][best][4]:
                best = index
        top = record["candidates"][best][4]
        assert all(abs(top - u) > UTILITY_MARGIN for i, (*_, u) in enumerate(record["candidates"]) if i != best), (
            "a choice hangs on the last bits of the utility"
        )
        man = kept[best]
        record["chosen"] = man
        if self.open_partnership(man) is not None:
            # Chosen earlier in this round: no second try (11.2).
            record["outcome"] = "taken"
            return record
        draw = variant(4, varint(PAIRING_NOISE), varint(her_key), varint(n), varint(self.key_of(man)))
        record["outcome"] = "paired"
        record["partnership"] = self.next_partnership_id
        record["entry"] = self._partner(woman, man, [plan, variant(3, string(PARTNER_MARKET_RULE)), draw], now)
        return record

    def arrange_rejection(self, woman: int, man: int):
        """The reject code of `ArrangePartnership { woman, man }` submitted now, or None
        (ADR-0004 section 4; design note of the market, section 4): the existing checks in
        their order, each for both (unknown, not alive, the sexes, already partnered), then
        `PartnerTooYoung` (either under the market age in the second the input acts, the
        stamp plus one), then `PartnersTooCloselyRelated` (`φ ≥ 1/8`)."""
        acts = self.closed + 1
        pair = (woman, man)
        if any(sim not in self.sims and sim not in self.archive for sim in pair):
            return UNKNOWN_SIM
        if any(sim not in self.sims for sim in pair):
            return SIM_NOT_ALIVE
        if self.sims[woman][2] != FEMALE or self.sims[man][2] != MALE:
            return PARTNERS_OF_WRONG_SEX
        if any(self.open_partnership(sim) is not None for sim in pair):
            return ALREADY_PARTNERED
        if any(acts - self.sims[sim][0] < self.data.market_age * YEAR for sim in pair):
            return PARTNER_TOO_YOUNG
        if self.kinship(woman, man) >= Fraction(self.data.kinship_limit):
            return PARTNERS_TOO_CLOSELY_RELATED
        return None

    def try_arrange(self, woman: int, man: int):
        """`ArrangePartnership` submitted now: its reject code (nothing changes, no sequence
        number, ADR-0004 section 4), or None when it is accepted and applied."""
        code = self.arrange_rejection(woman, man)
        if code is None:
            self.arrange(woman, man)
        return code

    # ------------------------------------------------------------------ fertility

    def key_of(self, sim: int) -> int:
        """The lineage key of a living or archived sim."""
        return self.sims[sim][1] if sim in self.sims else self.archive[sim][3]

    def wish_of(self, woman: int) -> int:
        """Her wish (genetics specification 11.3, design note section 3): `wish(u, shape,
        tilt)` with `u` the draw FERTILITY_WISH at her lineage key; pure, not held. The
        draw keeps `DRAW_MARGIN` from every threshold."""
        data = self.data
        u = wish_unit(self.seed, self.key_of(woman))
        assert all(abs(u - acc) > DRAW_MARGIN for acc in wish_thresholds(data.wish_shape, data.wish_tilt)), u
        return wish(u, data.wish_shape, data.wish_tilt)

    def theta_of(self, sim: int) -> float:
        """`θ_i` of a sim: the curve of its sex with `z_p` = 0 (no trait of the puberty
        timing in these data, 11.1)."""
        sex = self.sims[sim][2] if sim in self.sims else self.archive[sim][4]
        return theta_i(self.data.puberty_curve[sex], 0.0)

    def menarche_of(self, woman: int) -> float:
        """Her menarche (5.6, design note section 3): `menarche(θ_i, δ)` with `δ` the draw
        FERTILITY_MENARCHE at her lineage key; the form `puberty` only, pure, not held."""
        form = self.data.fertility
        assert form.kind == "puberty"
        return menarche(self.theta_of(woman), menarche_delta(self.seed, self.key_of(woman), form), form)

    def partner_of(self, woman: int):
        """Her partner in the open partnership, or None."""
        open_ = [p[1] for p in self.partnerships.values() if p[0] == woman and p[3] is None]
        assert len(open_) <= 1
        return open_[0] if open_ else None

    def factors(self, woman: int, man: int, t: int) -> tuple:
        """`(F_w, F_m)` at `t` (design note section 5): her factor at her age from her
        menarche (the window does not read it), his at his age from his `θ_i`."""
        form = self.data.fertility
        her_age = as_years(t - self.sims[woman][0])
        his_age = as_years(t - self.sims[man][0])
        her_menarche = self.menarche_of(woman) if form.kind == "puberty" else 0.0
        return female_factor(her_age, her_menarche, form), male_factor(his_age, self.theta_of(man), form)

    def attempt_chance(self, woman: int, man: int, t: int) -> float:
        """The chance of her attempt at `t`: `chance(adult_rate, F_w, F_m)`."""
        f_w, f_m = self.factors(woman, man, t)
        return chance(self.data.adult_rate, f_w, f_m)

    def series_allowed(self, woman: int, s: int) -> bool:
        """Whether her series may be planned at `s` now (design note section 4): an open
        partnership, not pregnant, `births < wish`, `age(w, s) < E`."""
        her = self.sims[woman]
        return (
            self.partner_of(woman) is not None
            and woman not in self.pregnancies
            and her[6] < self.wish_of(woman)
            and as_years(s - her[0]) < self.data.fertility_end
        )

    def _plan_series(self, woman: int, s: int, by: int, now: int) -> None:
        """Her monthly series from `s` on, planned by the entry `by`, if the rule allows it
        at this moment."""
        if not self.series_allowed(woman, s):
            return
        payload = variant(3, varint(woman))
        self._schedule(s, GENERAL, (SIM_SUBJECT, woman), ATTEMPT_KIND, payload, MONTH, by, now)

    def pending_series(self, woman: int):
        """The time of her pending series, or None."""
        times = [event.time for event in self.queue if event.kind == ATTEMPT_KIND and event.subject == (SIM_SUBJECT, woman)]
        assert len(times) <= 1
        return times[0] if times else None

    def _attempt(self, event: Event, now: int, plan: bytes) -> None:
        woman = event.subject[1]
        her = self.sims[woman]
        man = self.partner_of(woman)
        him = self.sims[man]
        p = self.attempt_chance(woman, man, now)
        draw = (CONCEPTION_ATTEMPT, her[1], now // MONTH, him[1])
        word = words(chacha_key(self.seed, *draw))[0]
        assert abs(unit(word) - p) > DRAW_MARGIN, f"an attempt hangs on the last bits of its chance {p!r}"
        self.attempts.append((now, woman, man, p, bernoulli(word, p)))
        if not bernoulli(word, p):
            # Her last attempt: the next one would be at or after the end of her fertility.
            if as_years(now + MONTH - her[0]) >= self.data.fertility_end:
                self.queue.remove(event)
            return
        self.queue.remove(event)
        conception_number = her[5]
        her[5] += 1
        # The twins (design note of the twins, sections 1 and 2): the draw at the key of
        # the first child decides one child, identical or fraternal twins.
        first = child_key(her[1], him[1], conception_number, 0)
        u = multiple_unit(self.seed, first)
        kind = multiple_kind(u, self.data.twins)
        keys = [first] if kind == SINGLE else [first, child_key(her[1], him[1], conception_number, 1)]
        random = lambda key: variant(4, *(varint(word) for word in key))
        causes = [plan, random(draw), random((MULTIPLE_BIRTH, first, 0, 0))]
        subjects = [variant(0, varint(woman)), variant(0, varint(man))]
        children, entries = [], []
        for index, key in enumerate(keys):
            # Per child in index order its conception entry (section 3), the same causes
            # for both.
            entries.append(self._entry(now, subjects, causes, variant(6, varint(key))))
            if kind == IDENTICAL and index == 1:
                # Identical twins share the zygote of child 0 and its two meioses.
                _, sex, person = children[0]
                children.append((key, sex, copy_genome(person)))
                continue
            sex, person = self._zygote(key, woman, man, entries[-1], now)
            children.append((key, sex, person))
        self.multiples.append((now, woman, man, conception_number, u, kind, [(key, sex) for key, sex, _ in children]))
        self.pregnancies[woman] = (man, conception_number, children, now)
        months = self.data.pregnancy_months
        payload = variant(4, varint(woman))
        # One birth, planned by the conception entry of child 0.
        self._schedule(now + months * MONTH, BIRTH, (SIM_SUBJECT, woman), BIRTH_KIND, payload, None, entries[0], now)

    def _zygote(self, genome_key: int, woman: int, man: int, conception: int, now: int) -> tuple:
        """(sex, genome) of the zygote under `genome_key`, with its two entries
        `core:meiosis` (her side, then his), caused by the conception entry
        `conception` and the rule."""
        # The meiosis of the independent reference (tools/meiosis_reference.py, written
        # from the genetics specification 2.3 by another model).
        data = self.data
        person, records = meiosis_reference.child_with_mutation(
            self.seed, genome_key, self.sims[woman][3], self.sims[man][3], data.map_lengths, data.sex_ratio, data.genome_map, data.rates
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
            payload = variant(7, varint(genome_key), segments, chromosome, mutations)
            self._entry(now, [variant(0, varint(parent))], causes, payload)
        sex = MALE if "y" in person["paternal_sex"] else FEMALE
        return sex, person

    def _birth(self, event: Event, now: int, plan: bytes) -> None:
        mother = event.subject[1]
        father, conception_number, children, _ = self.pregnancies.pop(mother)
        # Per child in index order: the next SimId and its creation entry, one after the
        # other (design note of the twins, sections 3 and 4).
        born = []
        for key, sex, person in children:
            child = self.next_sim_id
            self.next_sim_id += 1
            subjects = [variant(0, varint(child)), variant(0, varint(mother)), variant(0, varint(father))]
            entry = self._entry(now, subjects, [plan, variant(3, string("core:gestation"))], variant(8, varint(key)))
            afflictions = self.afflictions_of(key, sex, person, now)
            self.sims[child] = [now, key, sex, person, afflictions, 0, 0, (mother, father, conception_number), 0]
            self.pedigree.add(child, mother, father)
            born.append((child, entry))
        # Then per child in index order its first birthday and its deaths, by its own
        # creation entry.
        for child, entry in born:
            self._plan_birthday(child, now + YEAR, entry, now)
            self._plan_deaths(child, entry, now)
        # She bore them alive; then the series after a birth (design note of the fertility
        # in the world, sections 4 and 6), if her partnership is open, planned by the
        # creation entry of child 0.
        self.sims[mother][6] += len(children)
        pause = max(self.data.pause_months * MONTH, 1)
        self._plan_series(mother, month_start_at_or_after(now + pause), born[0][1], now)

    def arrange(self, woman: int, man: int) -> None:
        """`ArrangePartnership` of two living sims, submitted right now; it must be
        accepted (`try_arrange` for one that may be rejected)."""
        code = self.arrange_rejection(woman, man)
        assert code is None, f"ArrangePartnership is rejected: {REJECT_NAMES[code]}"
        sequence = len(self.inputs) + 1
        self.inputs.append(signed(self.closed) + varint(sequence) + variant(2, varint(woman), varint(man)))
        self.input_pairings.append((self.closed + 1, woman, man))
        self._partner(woman, man, [variant(2, varint(sequence))], self.closed + 1)

    def _start(self, founders: list) -> None:
        now = 0
        by_input = variant(2, varint(1))
        started = self._entry(now, [], [by_input], variant(0, varint(len(founders))))
        self._schedule(YEAR - 1, OBSERVER, (WORLD_SUBJECT, 0), YEAR_END_KIND, variant(0), YEAR, started, now)
        # The rounds of the market, right after the year end series (seq 2): the first at the
        # first month start after the start, then every month (design note of the market,
        # section 2).
        if self.market:
            self._schedule(month_start_after(now), GENERAL, (WORLD_SUBJECT, 0), MARKET_ROUND_KIND, MARKET_ROUND_PAYLOAD, MONTH, started, now)
        for index, (born, sex, genes) in enumerate(founders):
            sim = self.next_sim_id
            self.next_sim_id += 1
            key = founder_key(self.tag, index)
            subject = variant(0, varint(sim))
            added = self._entry(now, [subject], [by_input], variant(1, signed(born), lineage_key(key)))
            person = founder_genome(sex, genes, self.data.loci)
            # A founder's origin is the start (B19).
            afflictions = self.afflictions_of(key, sex, person, now)
            self.sims[sim] = [born, key, sex, person, afflictions, 0, 0, None, 0]
            self.pedigree.add(sim)
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
        assert payload[0] < 0x80
        self.entry_kinds.append(payload[0])
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
            self.ran.append((now, event.kind))
            if event.kind == YEAR_END_KIND:
                # The next occurrence is in the queue before the observer looks.
                event.time += event.period
                self.queue.append(event)
                self.year_hashes.append(hash64(self.projection(now - 1)))
                self.census_hashes.append(hash64(self.census(now - 1)))
            elif event.kind == MARKET_ROUND_KIND:
                # Periodic: the next round is in the queue before this one runs.
                event.time += event.period
                self.queue.append(event)
                self._market_round(now, plan)
            elif event.kind == ATTEMPT_KIND:
                event.time += event.period
                self.queue.append(event)
                self._attempt(event, now, plan)
            elif event.kind == BIRTH_KIND:
                self._birth(event, now, plan)
            elif event.kind == SEPARATION_KIND:
                self._separate(event, now, plan)
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
        born, key, sex, person, afflictions, conceptions, births, parents, partnerships = self.sims.pop(sim)
        codes = [SENESCENCE, ACCIDENT] + [
            affliction.disease.code for affliction in afflictions if affliction.exposure is not None
        ]
        code = next(code for code in codes if event.payload == death_payload(sim, str_key(code)))
        draw = variant(4, varint(DEATH_BUDGET), varint(key), varint(str_key(code)), varint(0))
        self._entry(now, [variant(0, varint(sim))], [plan, draw], variant(4, string(code)))
        self.queue = [other for other in self.queue if other.subject != (SIM_SUBJECT, sim)]
        for number_, couple_ in self.partnerships.items():
            if couple_[3] is None and sim in couple_[:2]:
                couple_[3] = (now, ENDED_BY_DEATH)
                # The partnership ends, and with it her series of attempts and its pending
                # separation (design note of the separation, section 5).
                attempts = variant(3, varint(couple_[0]))
                separation = (PARTNERSHIP_SUBJECT, number_)
                self.queue = [
                    other
                    for other in self.queue
                    if other.payload != attempts and not (other.kind == SEPARATION_KIND and other.subject == separation)
                ]
        # Her pregnancy ends with her.
        self.pregnancies.pop(sim, None)
        # The afflictions stay as they were at the death.
        self.archive[sim] = (
            born, now, str_key(code), key, sex, person, afflictions, conceptions, births, parents, partnerships
        )

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
            + varint(births)
            + varint(partnerships)
            + parents_bytes(parents)
            for sim, (born, key, sex, person, afflictions, conceptions, births, parents, partnerships) in sorted(
                self.sims.items()
            )
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
            + varint(births)
            + varint(partnerships)
            + parents_bytes(parents)
            for sim, (born, left, cause, key, sex, person, afflictions, conceptions, births, parents, partnerships) in sorted(
                self.archive.items()
            )
        ]
        pregnancies = [
            varint(mother) + pregnancy_bytes(*value) for mother, value in sorted(self.pregnancies.items())
        ]
        return (
            varint(PROJECTION_VERSION)
            + some(varint(self.seed))
            + some(world_params(self.switch))
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

    def census(self, closed=None) -> bytes:
        """The census of the null twin (test rule 8; design note of the market, section 7):
        the projection without the queue and its next insertion number, so the domain
        state only (seed, switch, clock, the counters of SimId and PartnershipId, sims,
        partnerships, pregnancies, archive). A round that changes nothing changes none of
        it."""
        projection = self.projection(closed)
        queue = seq([event.queue_bytes() for event in self.ordered_queue()]) + varint(self.next_insertion)
        assert projection.count(queue) == 1
        return projection.replace(queue, b"")

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


def start_rejection(founders: list, couples, data: Data):
    """The reject code of the couples of `Start`, or None (design note of the market,
    section 4): a couple naming no founder (`unknown_founder`), of the wrong sexes, a
    founder in two couples (`already_partnered`), then a founder of a couple under the
    market age at the start (second 0: `0 − born < 16 · YEAR`, `partner_too_young`).
    Founders are unrelated (φ 0)."""
    paired = set()
    for woman, man in couples:
        if not (0 <= woman < len(founders) and 0 <= man < len(founders)):
            return UNKNOWN_FOUNDER
        if founders[woman][1] != FEMALE or founders[man][1] != MALE:
            return PARTNERS_OF_WRONG_SEX
        if woman in paired or man in paired:
            return ALREADY_PARTNERED
        paired.update((woman, man))
    for woman, man in couples:
        if any(0 - founders[index][0] < data.market_age * YEAR for index in (woman, man)):
            return PARTNER_TOO_YOUNG
    return None


def check_market(world: World) -> None:
    """The load checks of the market (design note of the market, section 6; ADR-0018
    section 7; ADR-0006 section 5) on the state of `world`, as the loader would see its save
    with its input log; a broken one is `Inconsistent` there, an assertion here:

    - exactly one `market_round` series: period MONTH, at the first month start after the
      clock (after the start: `max(clock, 0)`, so MONTH right after Start), subject the
      world, planned by `core:started` at 0;
    - each partnership was formed by an accepted input (a couple of Start at 0, an
      ArrangePartnership in the second after its stamp) or in a round second (`formed` a
      positive multiple of MONTH) by the market; in one second the partnerships of inputs
      come before those of the round (PartnershipId order);
    - both were single then: an earlier partnership of either ended before `formed`;
    - both were at least 16 at `formed`, and `φ < 1/8`."""
    rounds = [event for event in world.queue if event.kind == MARKET_ROUND_KIND]
    if world.market:
        assert len(rounds) == 1, rounds
        (series,) = rounds
        assert series.time == month_start_after(max(world.closed, 0)) and series.period == MONTH
        assert series.subject == (WORLD_SUBJECT, 0) and series.cls == GENERAL and series.payload == MARKET_ROUND_PAYLOAD
        assert series.plan == (1, 0)
    else:
        assert not rounds
    pending = list(world.input_pairings)
    origin = {}
    for number_, (woman, man, formed, _) in sorted(world.partnerships.items()):
        if pending and pending[0] == (formed, woman, man):
            pending.pop(0)
            origin[number_] = "input"
        else:
            assert formed > 0 and formed % MONTH == 0, (number_, formed)
            origin[number_] = "market"
        assert all(formed - world.born_of(sim) >= world.data.market_age * YEAR for sim in (woman, man)), number_
        assert world.kinship(woman, man) < Fraction(world.data.kinship_limit), number_
        assert world.sex_of(woman) == FEMALE and world.sex_of(man) == MALE
    assert not pending, pending
    numbers = sorted(world.partnerships)
    for first in numbers:
        for later in numbers:
            if later <= first:
                continue
            a, b = world.partnerships[first], world.partnerships[later]
            if a[2] == b[2]:
                assert not (origin[first] == "market" and origin[later] == "input"), (first, later)
            if set(a[:2]) & set(b[:2]):
                assert a[3] is not None and a[3][0] < b[2], (first, later)


def robust_seconds(seconds_of):
    """The seconds of a death that do not hang on the last bit of libm: `seconds_of(nudge)`
    gives the same with the budget, exp and ln moved by one ulp either way, so the pinned
    libm of the core (within an ulp of the correctly rounded value) gives this second."""
    nominal = seconds_of((0, 0, 0))
    for nudge in itertools.product((-1, 0, 1), repeat=3):
        assert seconds_of(nudge) == nominal, f"a due second hangs on the last bit: {nudge}"
    return nominal


def check_separations(world: World) -> None:
    """The load checks of the separation (design note of the separation, section 6;
    ADR-0018 section 7; ADR-0006 section 5) on the state of `world`, as the loader would
    see its save; a broken one is `Inconsistent` there, an assertion here:

    - `partnerships` of each woman (living or archived) is the number of partnerships whose
      woman she is; a man's is 0;
    - per partnership, with `due` as in section 3 (`k` from the partnerships of the same
      woman with a smaller PartnershipId): the first of {`due` if `due` <= the clock, the
      first death of the two} decides, a tie going to the separation; `ended` is that
      Ending, or None if neither has come;
    - a pending `separation` exists exactly for each open partnership with a `due`, at
      `due`, subject that partnership; none else;
    - a series of attempts needs an open partnership (the check that stays)."""
    people = {sim: (value[2], value[8]) for sim, value in world.sims.items()}
    people.update({sim: (value[4], value[10]) for sim, value in world.archive.items()})
    for sim, (sex, count) in people.items():
        mine = sum(1 for value in world.partnerships.values() if value[0] == sim)
        assert count == (mine if sex == FEMALE else 0) and (sex == FEMALE or mine == 0), (sim, count, mine)
    pending = [event for event in world.queue if event.kind == SEPARATION_KIND]
    for number_, (woman, man, formed, ended) in world.partnerships.items():
        due = world.separation_due(number_)
        deaths = [world.archive[sim][1] for sim in (woman, man) if sim in world.archive]
        first_death = min(deaths) if deaths else None
        came = due is not None and due <= world.closed
        if came and (first_death is None or due <= first_death):
            want = (due, ENDED_BY_SEPARATION)
        elif first_death is not None:
            want = (first_death, ENDED_BY_DEATH)
        else:
            want = None
        assert ended == want, (number_, ended, want)
        events = [event for event in pending if event.subject == (PARTNERSHIP_SUBJECT, number_)]
        if ended is None and due is not None:
            assert [(e.time, e.payload) for e in events] == [(due, variant(5, varint(number_)))], number_
        else:
            assert not events, number_
    assert all(event.subject[0] == PARTNERSHIP_SUBJECT and event.subject[1] in world.partnerships for event in pending)
    for event in world.queue:
        if event.kind == ATTEMPT_KIND:
            assert world.partner_of(event.subject[1]) is not None, event.subject


def pregnancy_bytes(father: int, conception_number: int, children: list, conceived: int) -> bytes:
    """Pregnancy { father: SimId, conception: u32, children: Vec<Unborn>, conceived:
    SimTime } with Unborn { lineage_key, sex, genome }, the children in index order (design
    note of the twins, section 4)."""
    unborn = [lineage_key(key) + sex + genome_of(person) for key, sex, person in children]
    assert len(unborn) in (1, 2)
    return varint(father) + varint(conception_number) + seq(unborn) + signed(conceived)


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
    for name, data in (("repository", REPOSITORY), ("fork", FORK_DATA)):
        form = data.fertility
        window = f" from {form.start!r} until {form.until!r}" if form.kind == "window" else ""
        print(
            f"fertility_{name} form {form.kind}{window} end {data.fertility_end!r} adult_rate {data.adult_rate!r}"
            f" pause_months {data.pause_months} pregnancy_months {data.pregnancy_months}"
            f" wish_tilt {data.wish_tilt!r} wish_shape {data.wish_shape!r} twins {data.twins!r}"
        )
    print(f"life_values_fork {FORK_LIFE_VALUES}")
    print(f"separation_rate repository {REPOSITORY.separation_rate!r} fork {FORK_DATA.separation_rate!r}")
    print(
        f"tags FERTILITY_MENARCHE {FERTILITY_MENARCHE} FERTILITY_WISH {FERTILITY_WISH} MULTIPLE_BIRTH {MULTIPLE_BIRTH}"
        f" SEPARATION_BUDGET {SEPARATION_BUDGET}"
    )
    print(f"event kind separation {SEPARATION_KIND} class {GENERAL} subject partnership {PARTNERSHIP_SUBJECT}")
    print(
        f"tags PAIRING_RANK {PAIRING_RANK} MARKET_SEARCH {MARKET_SEARCH} MARKET_ORDER {MARKET_ORDER}"
        f" PAIRING_NOISE {PAIRING_NOISE}"
    )
    print(
        f"event kind market_round {MARKET_ROUND_KIND} class {GENERAL} subject world {WORLD_SUBJECT}"
        f" payload {MARKET_ROUND_PAYLOAD.hex()} period {MONTH}"
    )
    print(f"reject partner_too_young {PARTNER_TOO_YOUNG} partners_too_closely_related {PARTNERS_TOO_CLOSELY_RELATED}")
    for name, data in (("repository", REPOSITORY), ("fork", FORK_DATA)):
        print(
            f"market_{name} age {data.market_age} c {data.market_height!r} m {data.market_center!r}"
            f" w {data.market_width!r} rho {data.market_minor!r} k {data.market_candidates}"
            f" w_a {data.pairing_weight_age!r} mu_a {data.pairing_gap!r} limit {data.kinship_limit!r}"
            f" cousin_odds {data.cousin_odds!r} K {kinship_weight(data)!r}"
        )
    # Pure values of the market on the repository's data, for unit tests: a woman born at 0
    # at `t = age · YEAR` seconds (17.999 · YEAR is no whole second: rounded down, and the
    # last second before 18 as well).
    for label, t in [
        ("16", 16 * YEAR),
        ("17.5", 35 * YEAR // 2),
        ("17.999", math.floor(Fraction(17_999, 1000) * YEAR)),
        ("18*YEAR-1", 18 * YEAR - 1),
        ("18", 18 * YEAR),
        ("25.23", 2523 * YEAR // 100),
        ("40", 40 * YEAR),
        ("60", 60 * YEAR),
    ]:
        print(f"market_search_chance age {label} t {t} a {as_years(t)!r} p {search_chance(t, REPOSITORY)!r}")
    for gap, phi, noise in [(4.69, 0.0, 0.0), (0.0, 0.0, 1.5), (3.0, 0.0625, 0.0), (10.0, 0.03125, -2.0)]:
        print(f"market_utility gap {gap!r} phi {phi!r} noise {noise!r} U {utility(gap, phi, noise, REPOSITORY)!r}")
    clan_keys = [founder_key("test:clan", index) for index in range(6)]
    order = ranked(0xFF, MARKET_ORDER, WORLD_KEY, 7, clan_keys)
    print(
        "market_order seed 0xff month 7 founders 0-5 of test:clan: "
        + " ".join(f"{key:016x} (founder {clan_keys.index(key)})" for key in order)
    )
    candidates = ranked(0xFF, PAIRING_RANK, clan_keys[0], 7, clan_keys[1:])
    print(
        "market_candidates seed 0xff month 7 seeker founder 0 of test:clan, founders 1-5: "
        + " ".join(f"{key:016x} (founder {clan_keys.index(key)})" for key in candidates)
    )
    print(
        "market_ranks MARKET_ORDER "
        + " ".join(f"{rank(0xFF, MARKET_ORDER, WORLD_KEY, 7, key):016x}" for key in clan_keys)
        + " PAIRING_RANK "
        + " ".join(f"{rank(0xFF, PAIRING_RANK, clan_keys[0], 7, key):016x}" for key in clan_keys[1:])
    )
    # The similarity terms (design note of the similarity, sections 1 and 2): the weights of
    # the life course data, the named traits (none in the repository's data), and pure values
    # of the utility with both terms, with one and without (a term left out, not added as 0).
    print(
        f"similarity_repository w_K {REPOSITORY.pairing_weight_cognition!r} w_H {REPOSITORY.pairing_weight_height!r}"
        f" cognition_trait {REPOSITORY.cognition_trait} height_trait {REPOSITORY.height_trait}"
    )
    w_k, w_h = REPOSITORY.pairing_weight_cognition, REPOSITORY.pairing_weight_height
    for gap, phi, noise, d_k, d_h in [(4.69, 0.0, 0.0, 1.0, 1.0), (0.0, 0.0, 1.5, -0.5, 2.0), (3.0, 0.0625, 0.0, 0.3, -0.7), (10.0, 0.03125, -2.0, 1.25, 0.0)]:
        both = utility(gap, phi, noise, REPOSITORY, [(w_k, d_k), (w_h, d_h)])
        cognition = utility(gap, phi, noise, REPOSITORY, [(w_k, d_k)])
        height = utility(gap, phi, noise, REPOSITORY, [(w_h, d_h)])
        print(
            f"similarity_utility gap {gap!r} phi {phi!r} noise {noise!r} dK {d_k!r} dH {d_h!r}"
            f" penalties {penalties(gap, phi, REPOSITORY, [(w_k, d_k), (w_h, d_h)])!r}"
            f" U both {both!r} cognition_only {cognition!r} height_only {height!r} neither {utility(gap, phi, noise, REPOSITORY)!r}"
        )
    print(f"empty_projection {EMPTY_PROJECTION.hex()}")
    print(f"empty_save {EMPTY_SAVE.hex()}")
    # The constants of this model against the code lists of tests/golden.
    golden = ROOT / "tests" / "golden"
    lines = {tuple(line.split()) for path in ("rng_tags.txt", "event_codes.txt", "log_codes.txt") for line in (golden / path).read_text().splitlines()}
    wanted = {
        ("tag", str(PAIRING_RANK), "PAIRING_RANK"),
        ("tag", str(MARKET_SEARCH), "MARKET_SEARCH"),
        ("tag", str(MARKET_ORDER), "MARKET_ORDER"),
        ("tag", str(PAIRING_NOISE), "PAIRING_NOISE"),
        ("tag", str(SEPARATION_BUDGET), "SEPARATION_BUDGET"),
        ("tag", str(ENV_SHARED), "ENV_SHARED"),
        ("tag", str(ENV_INDIVIDUAL), "ENV_INDIVIDUAL"),
        ("kind", "market_round", str(MARKET_ROUND_KIND)),
        ("kind", "separation", str(SEPARATION_KIND)),
        ("subject", "world", str(WORLD_SUBJECT)),
        ("class", "general", str(GENERAL)),
        ("payload", "5", "core:partnered"),
        ("cause", "3", "rule"),
        ("cause", "4", "random"),
    } | {("reject", str(code), name) for code, name in REJECT_NAMES.items()}
    missing = sorted(wanted - lines)
    print(f"code lists (rng_tags, event_codes, log_codes) against the constants: {'ok' if not missing else f'MISSING {missing}'}")
    print("step hazard checks (disease_age, disease_hazard_used): ok")
    print("closed form checks (senescence_age, accident_age against mortality.rs W7.1): ok")

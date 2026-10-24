#!/usr/bin/env python3
"""Reference bytes for the postcard pins in crates/sim-core/tests/formats.rs.

Written from the postcard wire format specification (https://postcard.jamesmunns.com/
wire-format), independently of the Rust code: unsigned integers are LEB128 varints,
signed integers are zigzag encoded first, strings and sequences carry a varint length,
an enum variant is its index as a varint followed by its fields, a struct is its fields
in declaration order, a float is its eight bytes in little endian. No dependencies.

Usage: python3 tools/postcard_reference.py
"""


import struct


def varint(value: int) -> bytes:
    assert value >= 0
    out = bytearray()
    while True:
        byte = value & 0x7F
        value >>= 7
        if value:
            out.append(byte | 0x80)
        else:
            out.append(byte)
            return bytes(out)


def signed(value: int) -> bytes:
    """i64: zigzag, then varint."""
    return varint((value << 1) ^ (value >> 63) if value >= 0 else ((-value) << 1) - 1)


def string(text: str) -> bytes:
    data = text.encode("utf-8")
    return varint(len(data)) + data


def seq(items: list) -> bytes:
    return varint(len(items)) + b"".join(items)


def variant(index: int, *fields: bytes) -> bytes:
    return varint(index) + b"".join(fields)


# Enums without fields are their variant index. Sex: female = 0, male = 1.
FEMALE, MALE = variant(0), variant(1)

# The locus `core:disease_ar` of the walking skeleton (segment 0, slot 0 of the autosomes):
# 0 the reference allele, 1 the dummy allele. Every other slot holds 0 in these tests.
WILD, DUMMY = 0, 1


def genotype(maternal: int, paternal: int) -> tuple:
    """The alleles at `core:disease_ar`, from the mother and from the father."""
    return (maternal, paternal)


def founder_alleles(genes) -> bytes:
    """Vec<LocusAlleles>, LocusAlleles { locus: DataId, copies: Vec<u8> }: only a locus
    with an alternative allele is listed; a u8 is one raw byte. `genes` is the pair of
    alleles at `core:disease_ar` (from `genotype`) or a list of (locus, copies), the
    copies maternal then paternal (docs/GLOSSAR.md `copies`)."""
    if isinstance(genes, tuple):
        genes = [] if genes == (0, 0) else [("core:disease_ar", list(genes))]
    return seq([string(locus) + varint(len(copies)) + bytes(copies) for locus, copies in genes])


def genome(sex: bytes, genes: tuple) -> bytes:
    """Genome { maternal: Autosomes, paternal: Autosomes, maternal_x: XChromosome,
    paternal_sex: PaternalSex, mt: Mitochondria }. Autosomes([u64; 3]) is three varints
    without a length, `core:disease_ar` bit 0 of the first; XChromosome(u16) a varint; the
    sex chromosome from the father X(XChromosome) = variant 0, Y(YChromosome(u8)) =
    variant 1 with one raw byte; Mitochondria(u8) one raw byte."""
    maternal, paternal = genes

    def autosomes(bit: int) -> bytes:
        return varint(bit) + varint(0) + varint(0)

    from_father = variant(0, varint(0)) if sex == FEMALE else variant(1, bytes([0]))
    return autosomes(maternal) + autosomes(paternal) + varint(0) + from_father + bytes([0])


def genome_of(person: dict) -> bytes:
    """Genome as in `genome`, from a genome in the form of tools/meiosis_reference.py
    (`autosomes_maternal`, `autosomes_paternal`: 8 segments of 24 slots; `x_maternal`;
    `x_paternal` or `paternal_sex` {"x" or "y": slots}; `mt`). Autosome segment s, slot i
    is bit 24 s + i of the three words; X, Y and mt slot i is bit i (the layout the
    meiosis tests of sim-core read the reference vectors with)."""

    def autosomes(rows) -> bytes:
        words = [0, 0, 0]
        for segment, row in enumerate(rows):
            for slot, allele in enumerate(row):
                at = 24 * segment + slot
                words[at // 64] |= allele << (at % 64)
        return b"".join(varint(word) for word in words)

    def bits(slots) -> int:
        return sum(allele << slot for slot, allele in enumerate(slots))

    if "x_paternal" in person:
        name, from_father = "x", person["x_paternal"]
    else:
        ((name, from_father),) = person["paternal_sex"].items()
    if name == "x":
        second = variant(0, varint(bits(from_father)))
    else:
        second = variant(1, bytes([bits(from_father)]))
    return (
        autosomes(person["autosomes_maternal"])
        + autosomes(person["autosomes_paternal"])
        + varint(bits(person["x_maternal"]))
        + second
        + bytes([bits(person["mt"])])
    )


def lineage_key(key: int) -> bytes:
    """LineageKey(u64): the plain number, like every full-range u64."""
    return varint(key)


def flag(on: bool) -> bytes:
    """A bool is one byte, 0 or 1."""
    return b"\x01" if on else b"\x00"


def nutrition_year(year: int, index: float) -> bytes:
    """NutritionYear { year: i64, index: f64 } (ADR-0013)"""
    return signed(year) + f64(index)


def world_params(hereditary_diseases: bool, nutrition=()) -> bytes:
    """WorldParams { hereditary_diseases: bool, nutrition: [NutritionYear] } (ADR-0013)"""
    return flag(hereditary_diseases) + seq([nutrition_year(y, n) for y, n in nutrition])


def param_change(hereditary_diseases: bool) -> bytes:
    """ParamChange::HereditaryDiseases(bool), variant 0"""
    return variant(0, flag(hereditary_diseases))


def param_change_nutrition(year: int, index: float) -> bytes:
    """ParamChange::NutritionYear(NutritionYear), variant 1 (ADR-0013)"""
    return variant(1, nutrition_year(year, index))


def f64(value: float) -> bytes:
    """A float is eight bytes, little endian."""
    return struct.pack("<d", value)


def exposure(value) -> bytes:
    """Option<Exposure>, Exposure { used: FiniteF64, since: Option<SimTime> }: the hazard
    used up in the closed periods of the switch and the time the current one began. The
    value is None or a pair (used, since) with since None or a time."""
    if value is None:
        return b"\x00"
    used, since = value
    return b"\x01" + f64(used) + (b"\x00" if since is None else b"\x01" + signed(since))


def affliction(disease: int, onset: float, exposure_) -> bytes:
    """Affliction { disease: u64 (the str_key of its code), onset: FiniteF64 (years),
    exposure: Option<Exposure> } (None for a visible disease)."""
    return varint(disease) + f64(onset) + exposure(exposure_)


def afflictions(items: list) -> bytes:
    """Vec<Affliction> in the order of the diseases in the data; each item a triple
    (disease, onset, exposure) as for `affliction`."""
    return seq([affliction(*item) for item in items])


def founder(born: int, sex: bytes, genes: tuple) -> bytes:
    """Founder { born: SimTime, sex: Sex, alleles: Vec<LocusAlleles> }"""
    return signed(born) + sex + founder_alleles(genes)


def couple(woman: int, man: int) -> bytes:
    """Couple { woman: u32, man: u32 }: positions in the list of founders."""
    return varint(woman) + varint(man)


def capacity(value) -> bytes:
    """`capacity: Option<u64>`, the last field of `Start` (design note of the emigration,
    section 1): the option tag 0 for none, else 1 and the plain number."""
    return b"\x00" if value is None else b"\x01" + varint(value)


def pool_start_none() -> bytes:
    """`pool: Option<PoolStart>`, the last field of `Start` since scenario format 5 (design
    note of the arrival, section 3): the option tag 0 for a town of founders."""
    return b"\x00"


# Strand: maternal = 0, paternal = 1. SexChromosome: x = 0, y = 1.
MATERNAL, PATERNAL = variant(0), variant(1)
X, Y_CHROMOSOME = variant(0), variant(1)


def pregnancy(father: int, conception: int, children: list, conceived: int) -> bytes:
    """Pregnancy { father: SimId, conception: u32, children: Vec<Unborn>, conceived:
    SimTime }, Unborn { lineage_key, sex, genome } (projection version 14, design note of
    the twins section 4): `children` one or two (key, sex, genes) in index order."""
    unborn = [lineage_key(key) + sex + genome(sex, genes) for key, sex, genes in children]
    return varint(father) + varint(conception) + seq(unborn) + signed(conceived)


def mutation(locus: str, old: int, new: int) -> bytes:
    """Mutation { locus: DataId, old: u8, new: u8 }"""
    return string(locus) + bytes([old, new])


def segment_meiosis(segment: int, strand: bytes, crossovers: list) -> bytes:
    """SegmentMeiosis { segment: u8, start_strand: Strand, crossovers: Vec<FiniteF64> }"""
    return bytes([segment]) + strand + seq([f64(c) for c in crossovers])


def ending(at: int, by: int) -> bytes:
    """Ending { at: SimTime, by: EndedBy }, EndedBy: Death = 0, Separation = 1 (design note
    of the separation, section 2), Emigration = 2 (design note of the emigration, section
    5)."""
    return signed(at) + variant(by)


def partnership(woman: int, man: int, formed: int, ended) -> bytes:
    """Partnership { woman: SimId, man: SimId, formed: SimTime, ended: Option<Ending> }
    (projection version 15): `ended` None or (at, by) as for `ending`."""
    return varint(woman) + varint(man) + signed(formed) + (b"\x00" if ended is None else b"\x01" + ending(*ended))


def household(formed: int, ended) -> bytes:
    """Household { formed: SimTime, ended: Option<SimTime> } (projection version 16, design
    note of the households, section 2): `ended` None or the time."""
    return signed(formed) + (b"\x00" if ended is None else b"\x01" + signed(ended))


def household_fields(household_id: int, moved_out: bool, carer) -> bytes:
    """The fields a sim holds after `partnerships` (design note of the households, section
    2): `household: HouseholdId`, `moved_out: bool`, `carer: Option<SimId>` (an archived
    sim holds the first two only)."""
    return varint(household_id) + flag(moved_out) + (b"\x00" if carer is None else b"\x01" + varint(carer))


# Input::Start (variant 0) { world_seed: u64, founder_tag: DataId, world_params:
# WorldParams, founders: [Founder], couples: [Couple], capacity: Option<u64>, pool:
# Option<PoolStart> }: no causes of death and no fertility, they come from the data
# (ADR-0017 sections 2 and 4; design note of the fertility in the world, section 1); the
# capacity (design note of the emigration, section 1), none here; the pool last (design note
# of the arrival, section 3), none here.
SAMPLE_INPUT = variant(
    0,
    varint(0xFF),
    string("test:fork_no_density"),
    world_params(False),
    seq([founder(-946_080_000, FEMALE, genotype(WILD, DUMMY))]),
    seq([]),
    capacity(None),
    pool_start_none(),
)

# Input::ArrangePartnership (variant 2) { woman: SimId, man: SimId }
SAMPLE_ARRANGE = variant(2, varint(1), varint(2))

# LogPayload::Partnered (variant 5) { partnership: PartnershipId }
SAMPLE_PARTNERED_PAYLOAD = variant(5, varint(1))

# LogPayload::Separated (variant 9, the last) { partnership: PartnershipId } (design note
# of the separation, section 4).
SAMPLE_SEPARATED_PAYLOAD = variant(9, varint(1))

# Event::Separation (variant 5, the last) { partnership: PartnershipId } in the queue
# (design note of the separation, section 3; kind `separation` 6).
SAMPLE_SEPARATION_EVENT = variant(5, varint(1))

# A partnership ended by separation and one ended by death (design note of the
# separation, section 2): the option tag, the time, the variant of EndedBy.
SAMPLE_PARTNERSHIP_SEPARATED = partnership(1, 2, 0, (86_400, 1))
SAMPLE_PARTNERSHIP_DIED = partnership(1, 2, 0, (86_400, 0))
SAMPLE_PARTNERSHIP_OPEN = partnership(1, 2, 0, None)

# LogPayload::Conception (variant 6) { lineage_key } and Meiosis (variant 7) { genome_key,
# segments: Vec<SegmentMeiosis>, sex_chromosome, mutations: Vec<Mutation> }: the keys are
# plain u64.
SAMPLE_CONCEPTION_PAYLOAD = variant(6, varint(0x0123456789ABCDEF))
SAMPLE_MEIOSIS_PAYLOAD = variant(
    7,
    varint(0x0123456789ABCDEF),
    seq([segment_meiosis(0, PATERNAL, [0.25, 0.75]), segment_meiosis(8, MATERNAL, [])]),
    Y_CHROMOSOME,
    seq([mutation("core:disease_ar", 0, 1)]),
)

# Cause::Random (variant 4) { tag, subject, occasion, detail }: four plain u64.
SAMPLE_RANDOM_CAUSE = variant(
    4, varint(4), varint(0xCFD3D4551FB363C2), varint(0x0123456789ABCDEF), varint(0)
)

# LogPayload::Death (variant 4) { cause: Code }: any code; the world writes
# `core:senescence`, `core:accident` or the code of a disease.
SAMPLE_DEATH_PAYLOAD = variant(4, string("test:base"))

# Input::ForkOverride (variant 1) { changes: [ParamChange] }: the switch goes on.
SAMPLE_OVERRIDE = variant(1, seq([param_change(True)]))

# InputStamp { t: SimTime, seq: InputSeq }
SAMPLE_STAMP = signed(5) + varint(1)

# InputRecord { t: SimTime, seq: InputSeq, input: Input }
SAMPLE_RECORD = signed(-1) + varint(1) + SAMPLE_INPUT

# LogEntry { id, t, subjects, causes, payload }; Subject::Sim = 0;
# Cause: Event = 0, Plan = 1 { by, planned_at }, Input = 2, Rule = 3; LogPayload::Birthday = 2
SAMPLE_ENTRY = (
    varint(7)
    + signed(86_400)
    + seq([variant(0, varint(42))])
    + seq(
        [
            variant(1, varint(3), signed(0)),
            variant(3, string("core:gompertz_hazard")),
            variant(0, varint(5)),
            variant(2, varint(1)),
        ]
    )
    + variant(2, varint(1))
)

# InputStamp with a negative time and the largest sequence number.
NEGATIVE_STAMP = signed(-5) + varint((1 << 53) - 1)

# InputRejected { code: RejectCode::AlreadyStarted (variant 0) }
REJECTED = variant(0)

# The two reject codes of the market at the end of the list (design note of the market,
# section 4): PartnerTooYoung (21), then PartnersTooCloselyRelated (22).
REJECTED_PARTNER_TOO_YOUNG = variant(21)
REJECTED_PARTNERS_TOO_CLOSELY_RELATED = variant(22)

# Event::MarketRound (variant 6, the last, no fields) in the queue (design note of the
# market, section 2; kind `market_round` 7).
SAMPLE_MARKET_ROUND_EVENT = variant(6)

# The causes of a pairing of the market (design note of the market, section 3, step 6): the
# plan of the round (by `core:started`, entry 1, at 0), Rule (variant 3) with the code
# `core:partner_market`, Random (variant 4) with PAIRING_NOISE (21), her key, the month
# index, his key.
SAMPLE_MARKET_CAUSES = seq(
    [
        variant(1, varint(1), signed(0)),
        variant(3, string("core:partner_market")),
        variant(4, varint(21), varint(0xCFD3D4551FB363C2), varint(7), varint(0x0123456789ABCDEF)),
    ]
)

# Event::LeaveHome (variant 7, the last) { sim: SimId } in the queue (design note of the
# households, section 3; kind `leave_home` 8).
SAMPLE_LEAVE_HOME_EVENT = variant(7, varint(1))

# LogPayload::HouseholdFormed (variant 10) { household: HouseholdId } and LogPayload::Moved
# (variant 11, the last) { household: HouseholdId } (design note of the households, section
# 3).
SAMPLE_HOUSEHOLD_FORMED_PAYLOAD = variant(10, varint(1))
SAMPLE_MOVED_PAYLOAD = variant(11, varint(1))

# The causes of a move after a partnership: Event (variant 0) the entry `core:partnered`,
# Rule (variant 3) `core:household`.
SAMPLE_HOUSEHOLD_CAUSES = seq([variant(0, varint(5)), variant(3, string("core:household"))])

# A household open and one ended; a sim's household fields (household 3, moved out, carer
# sim 2) and those of a dependent child alone after the orphan rule (household 1, not moved
# out, no carer).
SAMPLE_HOUSEHOLD_OPEN = household(0, None)
SAMPLE_HOUSEHOLD_ENDED = household(0, 86_400)
SAMPLE_HOUSEHOLD_FIELDS = household_fields(3, True, 2)
SAMPLE_HOUSEHOLD_FIELDS_ORPHAN = household_fields(1, False, None)
# `social_parents: Vec<SimId>` after `parents` (design note of the households, part 2,
# section 2): empty, and a foster mother 2 then her partner 7.
SAMPLE_SOCIAL_PARENTS_EMPTY = seq([])
SAMPLE_SOCIAL_PARENTS = seq([varint(2), varint(7)])
SAMPLE_SOCIAL_PARENTS_LARGE = seq([varint(300)])

# The emigration by soft capacity (design note of the emigration): `Start` with the
# capacity 3 and with the largest one; Event::Emigration (variant 8, the last) { capacity:
# u64 } in the queue (kind `emigration` 9, class 950); LogPayload::Emigrated (variant 12, the
# last) { household: HouseholdId }; its causes (the plan of the event by `core:started`, entry
# 1 at 0; Rule `core:capacity`; Random with EMIGRATION (24), the smallest lineage key of the
# household's members of 18 or more, the year, detail 0); a partnership ended by emigration
# (EndedBy::Emigration = 2); the reason an archived sim left, LeftBy::Death { cause } = 0
# and LeftBy::Emigration = 1 (no fields); the reject code CapacityZero (23, the last).
SAMPLE_INPUT_CAPACITY = variant(
    0,
    varint(0xFF),
    string("test:fork_no_density"),
    world_params(False),
    seq([founder(-946_080_000, FEMALE, genotype(WILD, DUMMY))]),
    seq([]),
    capacity(3),
    pool_start_none(),
)
SAMPLE_CAPACITY_NONE = capacity(None)
SAMPLE_CAPACITY_3 = capacity(3)
SAMPLE_CAPACITY_MAX = capacity((1 << 64) - 1)
SAMPLE_EMIGRATION_EVENT = variant(8, varint(3))
SAMPLE_EMIGRATED_PAYLOAD = variant(12, varint(1))
SAMPLE_EMIGRATION_CAUSES = seq(
    [
        variant(1, varint(1), signed(0)),
        variant(3, string("core:capacity")),
        variant(4, varint(24), varint(0xCFD3D4551FB363C2), varint(1), varint(0)),
    ]
)
SAMPLE_PARTNERSHIP_EMIGRATED = partnership(1, 2, 0, (2 * 31_449_600 - 1, 2))
SAMPLE_LEFT_BY_DEATH = variant(0, varint(0x0123456789ABCDEF))
SAMPLE_LEFT_BY_EMIGRATION = variant(1)
REJECTED_CAPACITY_ZERO = variant(23)

# The reject code of relatives in direct line (ADR-0018 section 8, addendum of 2026-10-24):
# PartnersInDirectLine (36), the last of the list, after ShockOverlaps (35).
REJECTED_PARTNERS_IN_DIRECT_LINE = variant(36)

# SimDuration at its lower bound, -1 000 000 years of 364 days.
MIN_DURATION = signed(-1_000_000 * 364 * 86_400)

# The largest ID and the full-range seed, to pin the varint widths.
MAX_ID = varint((1 << 53) - 1)
MAX_U64 = varint((1 << 64) - 1)

# The first founder of `test:fork_no_density` (ADR-0001 section 4): the value that
# tools/rng_reference.py prints first under `founders`.
SAMPLE_LINEAGE_KEY = lineage_key(0xCFD3D4551FB363C2)

if __name__ == "__main__":
    for name, data in [
        ("sample_input", SAMPLE_INPUT),
        ("sample_override", SAMPLE_OVERRIDE),
        ("sample_random_cause", SAMPLE_RANDOM_CAUSE),
        ("sample_death_payload", SAMPLE_DEATH_PAYLOAD),
        ("affliction_lethal_off", affliction(0x0123456789ABCDEF, 0.0, (0.0, None))),
        ("affliction_lethal_on", affliction(0x0123456789ABCDEF, 18.0, (0.25, 5))),
        ("affliction_visible", affliction(0x0123456789ABCDEF, 25.5, None)),
        ("sample_arrange", SAMPLE_ARRANGE),
        ("sample_partnered_payload", SAMPLE_PARTNERED_PAYLOAD),
        ("sample_separated_payload", SAMPLE_SEPARATED_PAYLOAD),
        ("sample_separation_event", SAMPLE_SEPARATION_EVENT),
        ("sample_partnership_separated", SAMPLE_PARTNERSHIP_SEPARATED),
        ("sample_partnership_died", SAMPLE_PARTNERSHIP_DIED),
        ("sample_partnership_open", SAMPLE_PARTNERSHIP_OPEN),
        ("couple", couple(0, 1)),
        ("sample_conception_payload", SAMPLE_CONCEPTION_PAYLOAD),
        ("sample_meiosis_payload", SAMPLE_MEIOSIS_PAYLOAD),
        ("sample_stamp", SAMPLE_STAMP),
        ("sample_record", SAMPLE_RECORD),
        ("sample_entry", SAMPLE_ENTRY),
        ("negative_stamp", NEGATIVE_STAMP),
        ("rejected", REJECTED),
        ("rejected_partner_too_young", REJECTED_PARTNER_TOO_YOUNG),
        ("rejected_partners_too_closely_related", REJECTED_PARTNERS_TOO_CLOSELY_RELATED),
        ("sample_market_round_event", SAMPLE_MARKET_ROUND_EVENT),
        ("sample_market_causes", SAMPLE_MARKET_CAUSES),
        ("sample_leave_home_event", SAMPLE_LEAVE_HOME_EVENT),
        ("sample_household_formed_payload", SAMPLE_HOUSEHOLD_FORMED_PAYLOAD),
        ("sample_moved_payload", SAMPLE_MOVED_PAYLOAD),
        ("sample_household_causes", SAMPLE_HOUSEHOLD_CAUSES),
        ("sample_household_open", SAMPLE_HOUSEHOLD_OPEN),
        ("sample_household_ended", SAMPLE_HOUSEHOLD_ENDED),
        ("sample_household_fields", SAMPLE_HOUSEHOLD_FIELDS),
        ("sample_household_fields_orphan", SAMPLE_HOUSEHOLD_FIELDS_ORPHAN),
        ("sample_social_parents_empty", SAMPLE_SOCIAL_PARENTS_EMPTY),
        ("sample_social_parents", SAMPLE_SOCIAL_PARENTS),
        ("sample_social_parents_large", SAMPLE_SOCIAL_PARENTS_LARGE),
        ("min_duration", MIN_DURATION),
        ("max_id", MAX_ID),
        ("max_u64", MAX_U64),
        ("alleles_wild_dummy", founder_alleles(genotype(WILD, DUMMY))),
        ("alleles_wild_wild", founder_alleles(genotype(WILD, WILD))),
        ("genome_female_wild_dummy", genome(FEMALE, genotype(WILD, DUMMY))),
        ("genome_male_dummy_wild", genome(MALE, genotype(DUMMY, WILD))),
        ("sample_lineage_key", SAMPLE_LINEAGE_KEY),
        ("sample_input_capacity_3", SAMPLE_INPUT_CAPACITY),
        ("sample_capacity_none", SAMPLE_CAPACITY_NONE),
        ("sample_capacity_3", SAMPLE_CAPACITY_3),
        ("sample_capacity_max", SAMPLE_CAPACITY_MAX),
        ("sample_emigration_event", SAMPLE_EMIGRATION_EVENT),
        ("sample_emigrated_payload", SAMPLE_EMIGRATED_PAYLOAD),
        ("sample_emigration_causes", SAMPLE_EMIGRATION_CAUSES),
        ("sample_partnership_emigrated", SAMPLE_PARTNERSHIP_EMIGRATED),
        ("sample_left_by_death", SAMPLE_LEFT_BY_DEATH),
        ("sample_left_by_emigration", SAMPLE_LEFT_BY_EMIGRATION),
        ("rejected_capacity_zero", REJECTED_CAPACITY_ZERO),
        ("rejected_partners_in_direct_line", REJECTED_PARTNERS_IN_DIRECT_LINE),
    ]:
        print(f"{name} {data.hex()}")

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

# The locus `core:dummy` of the walking skeleton (segment 0, slot 0 of the autosomes):
# 0 the reference allele, 1 the dummy allele. Every other slot holds 0 in these tests.
WILD, DUMMY = 0, 1


def genotype(maternal: int, paternal: int) -> tuple:
    """The alleles at `core:dummy`, from the mother and from the father."""
    return (maternal, paternal)


def founder_alleles(genes: tuple) -> bytes:
    """Vec<LocusAlleles>, LocusAlleles { locus: DataId, copies: Vec<u8> }: only a locus
    with an alternative allele is listed; a u8 is one raw byte."""
    if genes == (0, 0):
        return seq([])
    return seq([string("core:dummy") + varint(2) + bytes(genes)])


def genome(sex: bytes, genes: tuple) -> bytes:
    """Genome { maternal: Autosomes, paternal: Autosomes, maternal_x: XChromosome,
    paternal_sex: PaternalSex, mt: Mitochondria }. Autosomes([u64; 3]) is three varints
    without a length, `core:dummy` bit 0 of the first; XChromosome(u16) a varint; the
    sex chromosome from the father X(XChromosome) = variant 0, Y(YChromosome(u8)) =
    variant 1 with one raw byte; Mitochondria(u8) one raw byte."""
    maternal, paternal = genes

    def autosomes(bit: int) -> bytes:
        return varint(bit) + varint(0) + varint(0)

    from_father = variant(0, varint(0)) if sex == FEMALE else variant(1, bytes([0]))
    return autosomes(maternal) + autosomes(paternal) + varint(0) + from_father + bytes([0])


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


# AppliesTo: everyone = 0, dummy_homozygous = 1.
APPLIES_TO = {"everyone": 0, "dummy_homozygous": 1}


def death_cause(code: str, per_year: float, applies_to: str = "everyone") -> bytes:
    """DeathCause { cause: Code, per_year: FiniteF64, applies_to: AppliesTo }"""
    return string(code) + f64(per_year) + variant(APPLIES_TO[applies_to])


def exposure(value) -> bytes:
    """Option<Exposure>, Exposure { before: SimDuration, since: Option<SimTime> }; the
    value is None or a pair (before, since) with since None or a time."""
    if value is None:
        return b"\x00"
    before, since = value
    return b"\x01" + signed(before) + (b"\x00" if since is None else b"\x01" + signed(since))


def founder(born: int, sex: bytes, genes: tuple) -> bytes:
    """Founder { born: SimTime, sex: Sex, alleles: Vec<LocusAlleles> }"""
    return signed(born) + sex + founder_alleles(genes)


def couple(woman: int, man: int) -> bytes:
    """Couple { woman: u32, man: u32 }: positions in the list of founders."""
    return varint(woman) + varint(man)


def fertility(per_month: float, from_years: int, until_years: int, pregnancy_months: int) -> bytes:
    """Fertility { per_month: FiniteF64, from_years: u32, until_years: u32, pregnancy_months: u32 }"""
    return f64(per_month) + varint(from_years) + varint(until_years) + varint(pregnancy_months)


# Strand: maternal = 0, paternal = 1. SexChromosome: x = 0, y = 1.
MATERNAL, PATERNAL = variant(0), variant(1)
X, Y_CHROMOSOME = variant(0), variant(1)


def pregnancy(father: int, key: int, conception: int, sex: bytes, genes: tuple, conceived: int) -> bytes:
    """Pregnancy { father: SimId, lineage_key, conception: u32, sex, genome, conceived: SimTime }"""
    return varint(father) + varint(key) + varint(conception) + sex + genome(sex, genes) + signed(conceived)


def mutation(locus: str, old: int, new: int) -> bytes:
    """Mutation { locus: DataId, old: u8, new: u8 }"""
    return string(locus) + bytes([old, new])


def segment_meiosis(segment: int, strand: bytes, crossovers: list) -> bytes:
    """SegmentMeiosis { segment: u8, start_strand: Strand, crossovers: Vec<FiniteF64> }"""
    return bytes([segment]) + strand + seq([f64(c) for c in crossovers])


def partnership(woman: int, man: int, formed: int, ended) -> bytes:
    """Partnership { woman: SimId, man: SimId, formed: SimTime, ended: Option<SimTime> }"""
    return varint(woman) + varint(man) + signed(formed) + (b"\x00" if ended is None else b"\x01" + signed(ended))


# Input::Start (variant 0) { world_seed: u64, founder_tag: DataId, world_params:
# WorldParams, death_causes: [DeathCause], founders: [Founder], couples: [Couple] }
SAMPLE_INPUT = variant(
    0,
    varint(0xFF),
    string("test:fork_no_density"),
    world_params(False),
    seq([death_cause("test:base", 0.02)]),
    seq([founder(-946_080_000, FEMALE, genotype(WILD, DUMMY))]),
    seq([]),
    fertility(0.05, 18, 45, 10),
)

# Input::ArrangePartnership (variant 2) { woman: SimId, man: SimId }
SAMPLE_ARRANGE = variant(2, varint(1), varint(2))

# LogPayload::Partnered (variant 5) { partnership: PartnershipId }
SAMPLE_PARTNERED_PAYLOAD = variant(5, varint(1))

# LogPayload::Conception (variant 6) { lineage_key } and Meiosis (variant 7) { genome_key,
# segments: Vec<SegmentMeiosis>, sex_chromosome, mutations: Vec<Mutation> }: the keys are
# plain u64.
SAMPLE_CONCEPTION_PAYLOAD = variant(6, varint(0x0123456789ABCDEF))
SAMPLE_MEIOSIS_PAYLOAD = variant(
    7,
    varint(0x0123456789ABCDEF),
    seq([segment_meiosis(0, PATERNAL, [0.25, 0.75]), segment_meiosis(8, MATERNAL, [])]),
    Y_CHROMOSOME,
    seq([mutation("core:dummy", 0, 1)]),
)

# Cause::Random (variant 4) { tag, subject, occasion, detail }: four plain u64.
SAMPLE_RANDOM_CAUSE = variant(
    4, varint(4), varint(0xCFD3D4551FB363C2), varint(0x0123456789ABCDEF), varint(0)
)

# LogPayload::Death (variant 4) { cause: Code }
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
        ("death_cause", death_cause("test:base", 0.02)),
        ("death_cause_dummy", death_cause("test:dummy_disease", 0.3, "dummy_homozygous")),
        ("sample_arrange", SAMPLE_ARRANGE),
        ("sample_partnered_payload", SAMPLE_PARTNERED_PAYLOAD),
        ("couple", couple(0, 1)),
        ("fertility", fertility(0.05, 18, 45, 10)),
        ("sample_conception_payload", SAMPLE_CONCEPTION_PAYLOAD),
        ("sample_meiosis_payload", SAMPLE_MEIOSIS_PAYLOAD),
        ("sample_stamp", SAMPLE_STAMP),
        ("sample_record", SAMPLE_RECORD),
        ("sample_entry", SAMPLE_ENTRY),
        ("negative_stamp", NEGATIVE_STAMP),
        ("rejected", REJECTED),
        ("min_duration", MIN_DURATION),
        ("max_id", MAX_ID),
        ("max_u64", MAX_U64),
        ("alleles_wild_dummy", founder_alleles(genotype(WILD, DUMMY))),
        ("alleles_wild_wild", founder_alleles(genotype(WILD, WILD))),
        ("genome_female_wild_dummy", genome(FEMALE, genotype(WILD, DUMMY))),
        ("genome_male_dummy_wild", genome(MALE, genotype(DUMMY, WILD))),
        ("sample_lineage_key", SAMPLE_LINEAGE_KEY),
    ]:
        print(f"{name} {data.hex()}")

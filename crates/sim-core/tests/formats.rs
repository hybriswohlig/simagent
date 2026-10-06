//! Formats of IDs, log entries and inputs (ADR-0004): JSON is the human-readable form
//! and the command schema, postcard the canonical binary form. Both round trip, both
//! reject what the types forbid, and no JSON number leaves the JavaScript-safe range.
//!
//! The tests named `formats_roundtrip_*` together are acceptance test 9 of ADR-0004.
//! The postcard pins come from `tools/postcard_reference.py`, which is written from the
//! postcard wire format specification and not from a run of this code.

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use sim_core::data_id::{Code, DataId};
use sim_core::finite::FiniteF64;
use sim_core::genome::{Allele, Genotype, Sex, SexChromosome, Strand};
use sim_core::ids::{EventId, InputSeq, MAX_JS_SAFE, PartnershipId, SimId};
use sim_core::input::{
    AppliesTo, ArrangePartnership, Couple, DeathCause, Fertility, ForkOverride, Founder, Input,
    InputRecord, InputRejected, InputStamp, RejectCode, Start,
};
use sim_core::lineage::{GenomeKey, LineageKey};
use sim_core::log::{Cause, Causes, DrawRef, LogEntry, LogPayload, PlanRef, Subject};
use sim_core::params::{ParamChange, WorldParams};
use sim_core::time::{MAX_ABS_SECS, SimDuration, SimTime};
use sim_core::trace::{TraceEntry, TracePayload};
use sim_core::world::{LoadError, SaveError};

/// A fertility under which nobody ever conceives.
fn barren() -> Fertility {
    Fertility {
        per_month: FiniteF64::try_new(0.0).expect("finite"),
        from_years: 18,
        until_years: 45,
        pregnancy_months: 10,
    }
}

/// `EventId` is opaque (ADR-0004 section 6): outside the core it only comes from serde.
fn event(n: u64) -> EventId {
    serde_json::from_value(Value::from(n)).expect("valid event id")
}

fn input_seq(n: u64) -> InputSeq {
    InputSeq::try_from(n).expect("valid input seq")
}

fn data_id(text: &str) -> DataId {
    DataId::new(text).expect("valid data id")
}

fn hex(text: &str) -> Vec<u8> {
    assert!(text.len().is_multiple_of(2), "{text}");
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex digits"))
        .collect()
}

fn sim(n: u64) -> SimId {
    SimId::try_from(n).expect("valid sim id")
}

fn sample_entry() -> LogEntry {
    LogEntry {
        id: event(7),
        t: SimTime::from_secs(86_400),
        subjects: vec![Subject::Sim(sim(42))],
        causes: Causes::new(Cause::Plan(PlanRef {
            by: event(3),
            planned_at: SimTime::from_secs(0),
        }))
        .with(Cause::Rule(
            Code::new("core:gompertz_hazard").expect("valid"),
        ))
        .with(Cause::Event(event(5)))
        .with(Cause::Input(input_seq(1))),
        payload: LogPayload::Birthday { age_years: 1 },
    }
}

fn sample_input() -> Input {
    Input::Start(Start {
        world_seed: 0xff,
        founder_tag: data_id("test:fork_no_density"),
        world_params: WorldParams {
            hereditary_diseases: false,
        },
        death_causes: vec![DeathCause {
            cause: Code::new("test:base").expect("valid"),
            per_year: FiniteF64::try_new(0.02).expect("finite"),
            applies_to: AppliesTo::Everyone,
        }],
        founders: vec![Founder {
            born: SimTime::from_secs(-946_080_000),
            sex: Sex::Female,
            genotype: Genotype {
                maternal: Allele::Wild,
                paternal: Allele::Dummy,
            },
        }],
        couples: Vec::new(),
        fertility: Fertility {
            per_month: FiniteF64::try_new(0.05).expect("finite"),
            from_years: 18,
            until_years: 45,
            pregnancy_months: 10,
        },
    })
}

/// The draw of the death budget of the first founder of `test:fork_no_density`, with an
/// occasion that shows every hex digit.
fn sample_draw() -> DrawRef {
    DrawRef {
        tag: 4,
        subject: 0xcfd3_d455_1fb3_63c2,
        occasion: 0x0123_4567_89ab_cdef,
        detail: 0,
    }
}

/// The override block of a fork that switches the hereditary diseases on.
fn sample_override() -> Input {
    Input::ForkOverride(ForkOverride {
        changes: vec![ParamChange::HereditaryDiseases(true)],
    })
}

/// The first woman and the first man of a world paired.
fn sample_arrange() -> Input {
    Input::ArrangePartnership(ArrangePartnership {
        woman: sim(1),
        man: sim(2),
    })
}

/// Every `Input` variant. A new variant fails to compile here until it is in the list.
fn all_inputs() -> Vec<Input> {
    let inputs = vec![sample_input(), sample_override(), sample_arrange()];
    for input in &inputs {
        match input {
            Input::Start(_) | Input::ForkOverride(_) | Input::ArrangePartnership(_) => {}
        }
    }
    inputs
}

fn json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("serializes")
}

fn from_json<T: DeserializeOwned>(text: &str) -> Result<T, serde_json::Error> {
    serde_json::from_str(text)
}

/// Encodes and decodes with postcard. The value must survive: its JSON form (which
/// shows every field) is the same before and after, and so are the bytes.
fn postcard_round_trip<T: Serialize + DeserializeOwned>(value: &T) -> Vec<u8> {
    let bytes = postcard::to_allocvec(value).expect("encodes");
    let back: T = postcard::from_bytes(&bytes).expect("decodes");
    assert_eq!(json(&back), json(value));
    assert_eq!(postcard::to_allocvec(&back).expect("encodes"), bytes);
    bytes
}

const ENTRY_JSON: &str = r#"{"id":7,"t":86400,"subjects":[{"sim":42}],"causes":[{"plan":{"by":3,"planned_at":0}},{"rule":"core:gompertz_hazard"},{"event":5},{"input":1}],"payload":{"core:birthday":{"age_years":1}}}"#;
const STAMP_JSON: &str = r#"{"t":5,"seq":1}"#;

// tools/postcard_reference.py
const ENTRY_POSTCARD: &str =
    "0780c60a01002a040103000314636f72653a676f6d706572747a5f68617a617264000502010201";
const INPUT_POSTCARD: &str = "00ff0114746573743a666f726b5f6e6f5f64656e73697479000109746573743a626173657b14ae47e17a943f0001ffa3a08607000001009a9999999999a93f122d0a";
const OVERRIDE_POSTCARD: &str = "01010001";
const STAMP_POSTCARD: &str = "0a01";
const RECORD_POSTCARD: &str = "010100ff0114746573743a666f726b5f6e6f5f64656e73697479000109746573743a626173657b14ae47e17a943f0001ffa3a08607000001009a9999999999a93f122d0a";
const NEGATIVE_STAMP_POSTCARD: &str = "09ffffffffffffff0f";
const REJECTED_POSTCARD: &str = "00";
const MIN_DURATION_POSTCARD: &str = "ffffe3e3cda60e";
const MAX_ID_POSTCARD: &str = "ffffffffffffff0f";
const MAX_U64_POSTCARD: &str = "ffffffffffffffffff01";

const INPUT_JSON: &str = r#"{"start":{"world_seed":"00000000000000ff","founder_tag":"test:fork_no_density","world_params":{"hereditary_diseases":false},"death_causes":[{"cause":"test:base","per_year":0.02,"applies_to":"everyone"}],"founders":[{"born":-946080000,"sex":"female","genotype":{"maternal":"wild","paternal":"dummy"}}],"couples":[],"fertility":{"per_month":0.05,"from_years":18,"until_years":45,"pregnancy_months":10}}}"#;

fn sample_stamp() -> InputStamp {
    InputStamp {
        t: SimTime::from_secs(5),
        seq: input_seq(1),
    }
}

#[test]
fn log_entry_json_form_is_pinned() {
    assert_eq!(json(&sample_entry()), ENTRY_JSON);
    let back: LogEntry = from_json(ENTRY_JSON).expect("parses");
    assert_eq!(json(&back), ENTRY_JSON);
}

#[test]
fn input_json_form_is_pinned() {
    assert_eq!(json(&sample_input()), INPUT_JSON);
    assert_eq!(
        from_json::<Input>(INPUT_JSON).expect("parses"),
        sample_input()
    );
}

const OVERRIDE_JSON: &str = r#"{"fork_override":{"changes":[{"core:hereditary_diseases":true}]}}"#;

/// The override block (ADR-0001 section 6): a list of pairs of a world parameter, named
/// by its data ID, and the new value.
#[test]
fn override_form_is_pinned() {
    assert_eq!(json(&sample_override()), OVERRIDE_JSON);
    assert_eq!(
        from_json::<Input>(OVERRIDE_JSON).expect("parses"),
        sample_override()
    );
    assert_eq!(
        postcard_round_trip(&sample_override()),
        hex(OVERRIDE_POSTCARD)
    );
    let off = ParamChange::HereditaryDiseases(false);
    assert_eq!(json(&off), r#"{"core:hereditary_diseases":false}"#);
    assert_eq!(postcard_round_trip(&off), [0, 0]);
    let params = WorldParams {
        hereditary_diseases: true,
    };
    assert_eq!(json(&params), r#"{"hereditary_diseases":true}"#);
    assert_eq!(postcard_round_trip(&params), [1]);
    for rejected in [
        r#"{"fork_override":{}}"#,
        r#"{"fork_override":{"changes":[{"core:hereditary_diseases":true}],"extra":0}}"#,
        r#"{"fork_override":{"changes":[{"core:unknown":true}]}}"#,
        r#"{"fork_override":{"changes":[{"core:hereditary_diseases":1}]}}"#,
        r#"{"fork_override":{"changes":[{"hereditary_diseases":true}]}}"#,
        r#"{"fork_override":{"changes":["core:hereditary_diseases"]}}"#,
    ] {
        assert!(from_json::<Input>(rejected).is_err(), "{rejected}");
    }
    for rejected in [
        "{}",
        r#"{"hereditary_diseases":1}"#,
        r#"{"hereditary_diseases":true,"x":0}"#,
    ] {
        assert!(from_json::<WorldParams>(rejected).is_err(), "{rejected}");
    }
    // A start without world parameters is no start: nothing is filled in.
    let without = INPUT_JSON.replace(r#""world_params":{"hereditary_diseases":false},"#, "");
    assert_ne!(without, INPUT_JSON);
    assert!(from_json::<Input>(&without).is_err());
}

/// A conception of the child with a lineage key that shows every hex digit.
fn sample_conception() -> LogPayload {
    LogPayload::Conception {
        lineage_key: lineage_key_of(0x0123_4567_89ab_cdef),
    }
}

/// The meiosis of a father who passes on his paternal strand and a Y.
fn sample_meiosis() -> LogPayload {
    LogPayload::Meiosis {
        genome_key: GenomeKey::of_zygote(lineage_key_of(0x0123_4567_89ab_cdef)),
        start_strand: Strand::Paternal,
        sex_chromosome: SexChromosome::Y,
    }
}

/// A lineage key read from JSON: the only way to get one that was not built (ADR-0001).
fn lineage_key_of(value: u64) -> LineageKey {
    from_json(&format!(r#""{value:016x}""#)).expect("a key")
}

/// Conception (ADR-0001, ADR-0004 section 7, B18): the fertility of a scenario, the
/// entry of a conception and the meiosis of a parent, with the keys as hex in JSON.
/// Values from tools/postcard_reference.py.
#[test]
fn conception_forms_are_pinned() {
    let fertility = Fertility {
        per_month: FiniteF64::try_new(0.05).expect("finite"),
        from_years: 18,
        until_years: 45,
        pregnancy_months: 10,
    };
    let fertility_json =
        r#"{"per_month":0.05,"from_years":18,"until_years":45,"pregnancy_months":10}"#;
    assert_eq!(json(&fertility), fertility_json);
    assert_eq!(
        from_json::<Fertility>(fertility_json).expect("parses"),
        fertility
    );
    assert_eq!(
        postcard_round_trip(&fertility),
        hex("9a9999999999a93f122d0a")
    );

    let conception_json = r#"{"core:conception":{"lineage_key":"0123456789abcdef"}}"#;
    assert_eq!(json(&sample_conception()), conception_json);
    assert_eq!(
        from_json::<LogPayload>(conception_json).ok(),
        Some(sample_conception())
    );
    assert_eq!(
        postcard_round_trip(&sample_conception()),
        hex("06ef9bafcdf8acd19101")
    );
    let meiosis_json = r#"{"core:meiosis":{"genome_key":"0123456789abcdef","start_strand":"paternal","sex_chromosome":"y"}}"#;
    assert_eq!(json(&sample_meiosis()), meiosis_json);
    assert_eq!(
        from_json::<LogPayload>(meiosis_json).ok(),
        Some(sample_meiosis())
    );
    assert_eq!(
        postcard_round_trip(&sample_meiosis()),
        hex("07ef9bafcdf8acd191010101")
    );
    assert_eq!(json(&Strand::Maternal), r#""maternal""#);
    assert_eq!(json(&SexChromosome::X), r#""x""#);

    for rejected in [
        r#"{"per_month":0.05,"from_years":18,"until_years":45}"#,
        r#"{"per_month":"0.05","from_years":18,"until_years":45,"pregnancy_months":10}"#,
        r#"{"per_month":0.05,"from_years":-1,"until_years":45,"pregnancy_months":10}"#,
        r#"{"per_month":1e999,"from_years":18,"until_years":45,"pregnancy_months":10}"#,
        r#"{"per_month":0.05,"from_years":18,"until_years":45,"pregnancy_months":10,"twins":0}"#,
    ] {
        assert!(from_json::<Fertility>(rejected).is_err(), "{rejected}");
    }
    for rejected in [
        r#"{"core:meiosis":{"genome_key":"0123456789abcdef","start_strand":"Paternal","sex_chromosome":"y"}}"#,
        r#"{"core:meiosis":{"genome_key":"0123456789abcdef","start_strand":"paternal","sex_chromosome":"Y"}}"#,
        r#"{"core:meiosis":{"genome_key":"0123456789abcdef","start_strand":"paternal"}}"#,
        r#"{"core:conception":{"lineage_key":81985529216486895}}"#,
    ] {
        assert!(from_json::<LogPayload>(rejected).is_err(), "{rejected}");
    }
    // A start without fertility is no start: nothing is filled in.
    let without = INPUT_JSON.replace(
        r#","fertility":{"per_month":0.05,"from_years":18,"until_years":45,"pregnancy_months":10}"#,
        "",
    );
    assert_ne!(without, INPUT_JSON);
    assert!(from_json::<Input>(&without).is_err());
}

/// Partnerships (ADR-0004 sections 1, 4 and 7): the couples of `Start` by the places of
/// the founders, the input that pairs two sims by ID, and the creation entry of a
/// partnership. All values from tools/postcard_reference.py.
#[test]
fn partnership_forms_are_pinned() {
    let couple = Couple { woman: 0, man: 1 };
    let couple_json = r#"{"woman":0,"man":1}"#;
    assert_eq!(json(&couple), couple_json);
    assert_eq!(from_json::<Couple>(couple_json).expect("parses"), couple);
    assert_eq!(postcard_round_trip(&couple), hex("0001"));

    let arrange_json = r#"{"arrange_partnership":{"woman":1,"man":2}}"#;
    assert_eq!(json(&sample_arrange()), arrange_json);
    assert_eq!(
        from_json::<Input>(arrange_json).expect("parses"),
        sample_arrange()
    );
    assert_eq!(postcard_round_trip(&sample_arrange()), hex("020102"));

    let partnered = LogPayload::Partnered {
        partnership: PartnershipId::try_from(1).expect("valid"),
    };
    let partnered_json = r#"{"core:partnered":{"partnership":1}}"#;
    assert_eq!(json(&partnered), partnered_json);
    assert_eq!(
        from_json::<LogPayload>(partnered_json).ok(),
        Some(partnered.clone())
    );
    assert_eq!(postcard_round_trip(&partnered), hex("0501"));

    for rejected in [
        r#"{"woman":0}"#,
        r#"{"woman":0,"man":1,"since":0}"#,
        r#"{"woman":-1,"man":1}"#,
        r#"{"woman":4294967296,"man":1}"#,
    ] {
        assert!(from_json::<Couple>(rejected).is_err(), "{rejected}");
    }
    for rejected in [
        r#"{"arrange_partnership":{"woman":1}}"#,
        r#"{"arrange_partnership":{"woman":0,"man":2}}"#,
        r#"{"arrange_partnership":{"woman":1,"man":"2"}}"#,
        r#"{"arrange_partnership":{"woman":1,"man":2,"since":0}}"#,
    ] {
        assert!(from_json::<Input>(rejected).is_err(), "{rejected}");
    }
    assert!(from_json::<LogPayload>(r#"{"core:partnered":{"partnership":0}}"#).is_err());
    // A start without couples is no start: the list is required, even when empty.
    let without = INPUT_JSON.replace(r#","couples":[]"#, "");
    assert_ne!(without, INPUT_JSON);
    assert!(from_json::<Input>(&without).is_err());
}

/// Death (ADR-0001, ADR-0004 section 7): the causes in the input (one for everyone, one
/// for the homozygous only), the entry of a death and the draw it names as its cause,
/// with all four numbers as hex in JSON.
#[test]
fn death_forms_are_pinned() {
    let cause = DeathCause {
        cause: Code::new("test:base").expect("valid"),
        per_year: FiniteF64::try_new(0.02).expect("finite"),
        applies_to: AppliesTo::Everyone,
    };
    let cause_json = r#"{"cause":"test:base","per_year":0.02,"applies_to":"everyone"}"#;
    assert_eq!(json(&cause), cause_json);
    assert_eq!(from_json::<DeathCause>(cause_json).expect("parses"), cause);
    assert_eq!(
        postcard_round_trip(&cause),
        hex("09746573743a626173657b14ae47e17a943f00")
    );
    let disease = DeathCause {
        cause: Code::new("test:dummy_disease").expect("valid"),
        per_year: FiniteF64::try_new(0.3).expect("finite"),
        applies_to: AppliesTo::DummyHomozygous,
    };
    let disease_json =
        r#"{"cause":"test:dummy_disease","per_year":0.3,"applies_to":"dummy_homozygous"}"#;
    assert_eq!(json(&disease), disease_json);
    assert_eq!(
        from_json::<DeathCause>(disease_json).expect("parses"),
        disease
    );
    assert_eq!(
        postcard_round_trip(&disease),
        hex("12746573743a64756d6d795f64697365617365333333333333d33f01")
    );
    for rejected in [
        r#"{"cause":"test:base","applies_to":"everyone"}"#,
        r#"{"per_year":0.02,"applies_to":"everyone"}"#,
        r#"{"cause":"test:base","per_year":0.02}"#,
        r#"{"cause":"test:base","per_year":"0.02","applies_to":"everyone"}"#,
        r#"{"cause":"test:base","per_year":null,"applies_to":"everyone"}"#,
        r#"{"cause":"test:base","per_year":-0.0,"applies_to":"everyone"}"#,
        r#"{"cause":"test:base","per_year":1e999,"applies_to":"everyone"}"#,
        r#"{"cause":"Base","per_year":0.02,"applies_to":"everyone"}"#,
        r#"{"cause":"test:base","per_year":0.02,"applies_to":"all"}"#,
        r#"{"cause":"test:base","per_year":0.02,"applies_to":"DummyHomozygous"}"#,
        r#"{"cause":"test:base","per_year":0.02,"applies_to":1}"#,
        r#"{"cause":"test:base","per_year":0.02,"applies_to":"everyone","applies":"all"}"#,
    ] {
        assert!(from_json::<DeathCause>(rejected).is_err(), "{rejected}");
    }

    let random = Cause::Random(sample_draw());
    let random_json = r#"{"random":{"tag":"0000000000000004","subject":"cfd3d4551fb363c2","occasion":"0123456789abcdef","detail":"0000000000000000"}}"#;
    assert_eq!(json(&random), random_json);
    assert_eq!(
        json(&from_json::<Cause>(random_json).expect("parses")),
        random_json
    );
    assert_eq!(
        postcard_round_trip(&random),
        hex("0404c2c7cdfdd18af5e9cf01ef9bafcdf8acd1910100")
    );
    for rejected in [
        r#"{"random":{"tag":4,"subject":"cfd3d4551fb363c2","occasion":"0123456789abcdef","detail":"0000000000000000"}}"#,
        r#"{"random":{"tag":"0000000000000004","subject":"cfd3d4551fb363c2","occasion":"0123456789abcdef"}}"#,
        r#"{"random":{"tag":"0000000000000004","subject":"cfd3d4551fb363c2","occasion":"0123456789abcdef","detail":"0000000000000000","seed":"0000000000000001"}}"#,
    ] {
        assert!(from_json::<Cause>(rejected).is_err(), "{rejected}");
    }

    let died = LogPayload::Death {
        cause: Code::new("test:base").expect("valid"),
    };
    let died_json = r#"{"core:death":{"cause":"test:base"}}"#;
    assert_eq!(json(&died), died_json);
    assert_eq!(from_json::<LogPayload>(died_json).ok(), Some(died.clone()));
    assert_eq!(postcard_round_trip(&died), hex("0409746573743a62617365"));
}

#[test]
fn input_stamp_form_is_pinned() {
    assert_eq!(json(&sample_stamp()), STAMP_JSON);
    assert_eq!(
        from_json::<InputStamp>(STAMP_JSON).expect("parses"),
        sample_stamp()
    );
}

fn sample_record() -> InputRecord {
    InputRecord {
        t: SimTime::from_secs(-1),
        seq: input_seq(1),
        input: sample_input(),
    }
}

/// ADR-0004 section 6: a trace entry has no EventId; time, sim and payload.
#[test]
fn trace_entry_form_is_pinned() {
    let entry = TraceEntry {
        t: SimTime::from_secs(5),
        sim: sim(2),
        payload: TracePayload::BirthdayPlanned {
            next: SimTime::from_secs(-1),
        },
    };
    let text = r#"{"t":5,"sim":2,"payload":{"core:birthday_planned":{"next":-1}}}"#;
    assert_eq!(json(&entry), text);
    assert_eq!(from_json::<TraceEntry>(text).ok(), Some(entry.clone()));
    // time 5 (zigzag 10), sim 2, variant 0, next -1 (zigzag 1)
    assert_eq!(postcard_round_trip(&entry), [10, 2, 0, 1]);
    let unknown = text.replace(r#""sim":2"#, r#""sim":2,"x":0"#);
    assert!(from_json::<TraceEntry>(&unknown).is_err());
    assert!(from_json::<TraceEntry>(&text.replace(r#""sim":2"#, r#""sim":0"#)).is_err());
}

/// ADR-0004 section 4: one line of `input.ndjson` per accepted input.
#[test]
fn input_record_form_is_pinned() {
    let text = format!(r#"{{"t":-1,"seq":1,"input":{INPUT_JSON}}}"#);
    assert_eq!(json(&sample_record()), text);
    assert_eq!(from_json::<InputRecord>(&text).ok(), Some(sample_record()));
    assert_eq!(postcard_round_trip(&sample_record()), hex(RECORD_POSTCARD));
    let unknown = text.replace(r#""seq":1,"#, r#""seq":1,"unknown":0,"#);
    assert!(from_json::<InputRecord>(&unknown).is_err(), "{unknown}");
    let zero_seq = text.replace(r#""seq":1,"#, r#""seq":0,"#);
    assert!(from_json::<InputRecord>(&zero_seq).is_err(), "{zero_seq}");
}

#[test]
fn postcard_forms_are_pinned() {
    assert_eq!(postcard_round_trip(&sample_entry()), hex(ENTRY_POSTCARD));
    assert_eq!(postcard_round_trip(&sample_input()), hex(INPUT_POSTCARD));
    assert_eq!(postcard_round_trip(&sample_stamp()), hex(STAMP_POSTCARD));
    assert_eq!(postcard_round_trip(&sim(MAX_JS_SAFE)), hex(MAX_ID_POSTCARD));
    // Binary keeps the plain u64: a full-range seed is a 10-byte varint, not text.
    let full_seed = Input::Start(Start {
        world_seed: u64::MAX,
        founder_tag: data_id("a:b"),
        world_params: WorldParams {
            hereditary_diseases: false,
        },
        death_causes: Vec::new(),
        founders: Vec::new(),
        couples: Vec::new(),
        fertility: barren(),
    });
    let mut expected = vec![0];
    expected.extend(hex(MAX_U64_POSTCARD));
    // The tag, the switch off (one byte 0), no causes of death, no founders and no
    // couples (three lengths 0), then the fertility: eight bytes of 0.0 and the
    // varints 18, 45 and 10.
    expected.extend([3, b'a', b':', b'b', 0, 0, 0, 0]);
    expected.extend([0; 8]);
    expected.extend([18, 45, 10]);
    assert_eq!(postcard_round_trip(&full_seed), expected);
}

/// The genome of the Durchstich: in JSON the names, in postcard the variant index
/// (tools/postcard_reference.py).
#[test]
fn genome_forms_are_pinned() {
    for (sex, name, index) in [
        (Sex::Female, "\"female\"", 0_u8),
        (Sex::Male, "\"male\"", 1),
    ] {
        assert_eq!(json(&sex), name);
        assert_eq!(from_json::<Sex>(name).expect("parses"), sex);
        assert_eq!(postcard_round_trip(&sex), [index]);
    }
    for (allele, name, index) in [
        (Allele::Wild, "\"wild\"", 0_u8),
        (Allele::Dummy, "\"dummy\"", 1),
    ] {
        assert_eq!(json(&allele), name);
        assert_eq!(from_json::<Allele>(name).expect("parses"), allele);
        assert_eq!(postcard_round_trip(&allele), [index]);
    }
    let mixed = Genotype {
        maternal: Allele::Wild,
        paternal: Allele::Dummy,
    };
    let mixed_json = r#"{"maternal":"wild","paternal":"dummy"}"#;
    assert_eq!(json(&mixed), mixed_json);
    assert_eq!(from_json::<Genotype>(mixed_json).expect("parses"), mixed);
    assert_eq!(postcard_round_trip(&mixed), hex("0001"));
    let other = Genotype {
        maternal: Allele::Dummy,
        paternal: Allele::Wild,
    };
    assert_eq!(postcard_round_trip(&other), hex("0100"));

    for rejected in ["\"Female\"", "\"other\"", "0", "null"] {
        assert!(from_json::<Sex>(rejected).is_err(), "{rejected}");
        assert!(from_json::<Allele>(rejected).is_err(), "{rejected}");
    }
    for rejected in [
        r#"{"maternal":"wild"}"#,
        r#"{"paternal":"wild"}"#,
        r#"{"maternal":"wild","paternal":"dummy","third":"wild"}"#,
        r#"{"maternal":"wild","paternal":"recessive"}"#,
        r#"{"maternal":"wild","maternal":"wild","paternal":"dummy"}"#,
    ] {
        assert!(from_json::<Genotype>(rejected).is_err(), "{rejected}");
    }
    // A variant index that does not exist.
    assert!(postcard::from_bytes::<Sex>(&[2]).is_err());
    assert!(postcard::from_bytes::<Allele>(&[2]).is_err());
    assert!(postcard::from_bytes::<Genotype>(&[0, 2]).is_err());
}

/// A lineage key is a full-range u64 (ADR-0004 section 3): 16 hex digits in JSON, the
/// plain number in postcard. The key is the first founder of ADR-0001 `key_rules`.
#[test]
fn lineage_key_forms_are_pinned() {
    let key = LineageKey::founder(&data_id("test:fork_no_density"), 0);
    assert_eq!(json(&key), "\"cfd3d4551fb363c2\"");
    assert_eq!(
        from_json::<LineageKey>("\"cfd3d4551fb363c2\"").expect("parses"),
        key
    );
    assert_eq!(postcard_round_trip(&key), hex("c2c7cdfdd18af5e9cf01"));
    for rejected in [
        "14975886013442810818",
        "\"cfd3d4551fb363c\"",
        "\"CFD3D4551FB363C2\"",
        "\"0xcfd3d4551fb363c2\"",
        "{\"0\":\"cfd3d4551fb363c2\"}",
    ] {
        assert!(from_json::<LineageKey>(rejected).is_err(), "{rejected}");
    }
}

#[test]
fn postcard_round_trips() {
    let stamp = InputStamp {
        t: SimTime::from_secs(-5),
        seq: input_seq(MAX_JS_SAFE),
    };
    assert_eq!(postcard_round_trip(&stamp), hex(NEGATIVE_STAMP_POSTCARD));
    let rejected = InputRejected {
        code: RejectCode::AlreadyStarted,
    };
    assert_eq!(postcard_round_trip(&rejected), hex(REJECTED_POSTCARD));
    assert_eq!(
        postcard_round_trip(&SimDuration::from_secs(-MAX_ABS_SECS)),
        hex(MIN_DURATION_POSTCARD)
    );
    let id = data_id("core:height");
    let mut expected = vec![11];
    expected.extend(b"core:height");
    assert_eq!(postcard_round_trip(&id), expected);
}

#[test]
fn formats_roundtrip_rejects_ids_outside_one_to_js_safe() {
    for text in ["0", "9007199254740992", "18446744073709551615"] {
        assert!(from_json::<SimId>(text).is_err(), "SimId {text}");
        assert!(from_json::<EventId>(text).is_err(), "EventId {text}");
        assert!(from_json::<InputSeq>(text).is_err(), "InputSeq {text}");
    }
    assert!(from_json::<Subject>(r#"{"sim":0}"#).is_err());
    assert_eq!(
        from_json::<SimId>("9007199254740991").ok(),
        Some(sim(MAX_JS_SAFE))
    );
    for value in [0u64, MAX_JS_SAFE + 1] {
        let bytes = postcard::to_allocvec(&value).expect("encodes");
        assert!(
            postcard::from_bytes::<SimId>(&bytes).is_err(),
            "postcard {value}"
        );
        assert!(
            postcard::from_bytes::<EventId>(&bytes).is_err(),
            "postcard {value}"
        );
        assert!(
            postcard::from_bytes::<InputSeq>(&bytes).is_err(),
            "postcard {value}"
        );
    }
    let bytes = postcard::to_allocvec(&MAX_JS_SAFE).expect("encodes");
    let largest: EventId = postcard::from_bytes(&bytes).expect("the largest id");
    assert_eq!(json(&largest), "9007199254740991");
    assert_eq!(
        postcard::from_bytes::<InputSeq>(&bytes).ok(),
        Some(input_seq(MAX_JS_SAFE))
    );
    assert_eq!(
        json(&from_json::<EventId>("9007199254740991").expect("the largest id")),
        "9007199254740991"
    );
    // An ID is a JSON integer: no string, float, negative number, null or array.
    for text in [r#""7""#, "7.0", "7.5", "-1", "null", "[7]", "true"] {
        assert!(from_json::<EventId>(text).is_err(), "EventId {text}");
        assert!(from_json::<SimId>(text).is_err(), "SimId {text}");
        assert!(from_json::<InputSeq>(text).is_err(), "InputSeq {text}");
    }
    assert_eq!(json(&from_json::<EventId>("7").expect("valid")), "7");
}

#[test]
fn formats_roundtrip_rejects_malformed_hex() {
    let with_seed = |seed: &str| {
        from_json::<Input>(&INPUT_JSON.replace("00000000000000ff", seed)).map(|input| match input {
            Input::Start(start) => start.world_seed,
            Input::ForkOverride(_) | Input::ArrangePartnership(_) => panic!("a start"),
        })
    };
    assert_eq!(with_seed("00000000000000ff").ok(), Some(0xff));
    for bad in [
        "0000000000000ff",
        "000000000000000ff",
        "00000000000000FF",
        "0x000000000000ff",
        "+00000000000000f",
        "",
        "00000000 00000ff",
        "ff",
    ] {
        assert!(with_seed(bad).is_err(), "{bad:?}");
    }
    // A JSON number is not the hex form either.
    let number = INPUT_JSON.replace(r#""00000000000000ff""#, "255");
    assert!(from_json::<Input>(&number).is_err(), "{number}");
    assert_eq!(sim_core::hex_u64::parse("ffffffffffffffff"), Some(u64::MAX));
    assert_eq!(sim_core::hex_u64::parse("fffffffffffffff"), None);
}

#[test]
fn formats_roundtrip_rejects_empty_causes() {
    let empty = ENTRY_JSON.replace(
        r#"[{"plan":{"by":3,"planned_at":0}},{"rule":"core:gompertz_hazard"},{"event":5},{"input":1}]"#,
        "[]",
    );
    assert!(from_json::<LogEntry>(&empty).is_err());
    assert!(from_json::<Causes>("[]").is_err());
    let bytes = postcard::to_allocvec(&Vec::<Cause>::new()).expect("encodes");
    assert!(postcard::from_bytes::<Causes>(&bytes).is_err());
}

#[test]
fn formats_roundtrip_rejects_unknown_fields_and_variants() {
    let cases = [
        INPUT_JSON.replace(r#""founders""#, r#""extra":1,"founders""#),
        INPUT_JSON.replace(r#"{"born""#, r#"{"age":1,"born""#),
        // No silent defaults: a founder without sex or genotype is no founder.
        INPUT_JSON.replace(r#","sex":"female""#, ""),
        INPUT_JSON.replace(r#","genotype":{"maternal":"wild","paternal":"dummy"}"#, ""),
        INPUT_JSON.replace(r#""sex":"female""#, r#""sex":"unknown""#),
        INPUT_JSON.replace(r#"{"start""#, r#"{"begin""#),
        r#"{"start":{"world_seed":"00000000000000ff","founders":[]}}"#.to_owned(),
    ];
    for text in &cases {
        assert!(from_json::<Input>(text).is_err(), "{text}");
    }
    let entry_cases = [
        ENTRY_JSON.replace(r#""payload""#, r#""extra":0,"payload""#),
        ENTRY_JSON.replace("core:birthday", "core:wedding"),
        ENTRY_JSON.replace(r#"{"sim":42}"#, r#"{"household":42}"#),
        ENTRY_JSON.replace(r#"{"event":5}"#, r#"{"random":5}"#),
        ENTRY_JSON.replace(r#""age_years":1"#, r#""age_years":1,"x":2"#),
    ];
    for text in &entry_cases {
        assert!(from_json::<LogEntry>(text).is_err(), "{text}");
    }
}

#[test]
fn times_and_codes_are_checked_when_read() {
    let too_late = MAX_ABS_SECS + 1;
    for text in [too_late.to_string(), (-too_late).to_string()] {
        assert!(from_json::<SimTime>(&text).is_err(), "SimTime {text}");
        assert!(
            from_json::<SimDuration>(&text).is_err(),
            "SimDuration {text}"
        );
    }
    assert_eq!(
        from_json::<SimTime>(&MAX_ABS_SECS.to_string()).ok(),
        Some(SimTime::from_secs(MAX_ABS_SECS))
    );
    for bad in [r#""Core:height""#, r#""height""#, r#""core:""#] {
        assert!(from_json::<DataId>(bad).is_err(), "{bad}");
        assert!(from_json::<Code>(bad).is_err(), "{bad}");
    }
}

/// ADR-0001: `founder_tag` is a data ID of the scenario, not a number.
#[test]
fn the_founder_tag_is_a_data_id() {
    let with_tag = |tag: &str| from_json::<Input>(&INPUT_JSON.replace("test:fork_no_density", tag));
    assert!(with_tag("test:fork_no_density").is_ok());
    for bad in ["0123456789abcdef", "Test:x", "fork_no_density", ""] {
        assert!(with_tag(bad).is_err(), "{bad:?}");
    }
    let number = INPUT_JSON.replace(r#""test:fork_no_density""#, "7");
    assert!(from_json::<Input>(&number).is_err(), "{number}");
}

/// ADR-0004 section 3: every u64 that can exceed 2^53 - 1 is a hex string in JSON.
#[test]
fn formats_roundtrip_no_json_number_leaves_the_js_safe_range() {
    fn check(value: &Value, path: &str) {
        match value {
            Value::Number(n) => {
                let ok = n.as_u64().is_some_and(|v| v <= MAX_JS_SAFE)
                    || n.as_i64().is_some_and(|v| v.unsigned_abs() <= MAX_JS_SAFE);
                assert!(ok, "{path} = {n}");
            }
            Value::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    check(item, &format!("{path}[{i}]"));
                }
            }
            Value::Object(map) => {
                for (key, item) in map {
                    check(item, &format!("{path}.{key}"));
                }
            }
            Value::Null | Value::Bool(_) | Value::String(_) => {}
        }
    }
    let extreme_entry = LogEntry {
        id: event(MAX_JS_SAFE),
        t: SimTime::from_secs(-MAX_ABS_SECS),
        subjects: vec![Subject::Sim(sim(MAX_JS_SAFE))],
        causes: Causes::new(Cause::Plan(PlanRef {
            by: event(MAX_JS_SAFE),
            planned_at: SimTime::from_secs(MAX_ABS_SECS),
        }))
        .with(Cause::Input(input_seq(MAX_JS_SAFE)))
        .with(Cause::Event(event(MAX_JS_SAFE))),
        payload: LogPayload::Birthday {
            age_years: u32::MAX,
        },
    };
    let extreme_stamp = InputStamp {
        t: SimTime::from_secs(MAX_ABS_SECS),
        seq: input_seq(MAX_JS_SAFE),
    };
    let extreme_input = Input::Start(Start {
        world_seed: u64::MAX,
        founder_tag: data_id("test:fork_no_density"),
        world_params: WorldParams {
            hereditary_diseases: false,
        },
        death_causes: Vec::new(),
        founders: vec![Founder {
            born: SimTime::from_secs(MAX_ABS_SECS),
            sex: Sex::Male,
            genotype: Genotype {
                maternal: Allele::Dummy,
                paternal: Allele::Dummy,
            },
        }],
        couples: Vec::new(),
        fertility: barren(),
    });
    // A lineage key is a full-range number as well.
    let added = serde_json::to_value(&all_payloads()[1]).expect("json");
    check(&added, "founder added");
    assert_eq!(
        added["core:founder_added"]["lineage_key"],
        Value::from("cfd3d4551fb363c2")
    );
    check(
        &serde_json::to_value(&extreme_entry).expect("json"),
        "entry",
    );
    let input = serde_json::to_value(&extreme_input).expect("json");
    check(&input, "input");
    assert_eq!(
        input["start"]["world_seed"],
        Value::from("ffffffffffffffff"),
        "the full-range seed is written as hex"
    );
    check(&serde_json::to_value(extreme_stamp).expect("json"), "stamp");
}

fn all_payloads() -> Vec<LogPayload> {
    let payloads = vec![
        LogPayload::Started { founders: 3 },
        LogPayload::FounderAdded {
            born: SimTime::from_secs(-86_400),
            lineage_key: LineageKey::founder(&data_id("test:fork_no_density"), 0),
        },
        LogPayload::Birthday { age_years: 30 },
        LogPayload::ParamChanged {
            change: ParamChange::HereditaryDiseases(true),
        },
        LogPayload::Death {
            cause: Code::new("test:base").expect("valid"),
        },
        LogPayload::Partnered {
            partnership: PartnershipId::try_from(1).expect("valid"),
        },
        sample_conception(),
        sample_meiosis(),
    ];
    // A new variant fails to compile here until it is added to the list above.
    for payload in &payloads {
        match payload {
            LogPayload::Started { .. }
            | LogPayload::FounderAdded { .. }
            | LogPayload::Birthday { .. }
            | LogPayload::ParamChanged { .. }
            | LogPayload::Death { .. }
            | LogPayload::Partnered { .. }
            | LogPayload::Conception { .. }
            | LogPayload::Meiosis { .. } => {}
        }
    }
    payloads
}

fn all_reject_codes() -> Vec<RejectCode> {
    let codes = vec![
        RejectCode::AlreadyStarted,
        RejectCode::ClockNotAtStart,
        RejectCode::FounderNotBornBeforeStart,
        RejectCode::TooManyFounders,
        RejectCode::NotStarted,
        RejectCode::EmptyOverride,
        RejectCode::OverrideParamTwice,
        RejectCode::DeathCauseTwice,
        RejectCode::DeathRateOutOfRange,
        RejectCode::UnknownSim,
        RejectCode::SimNotAlive,
        RejectCode::PartnersOfWrongSex,
        RejectCode::AlreadyPartnered,
        RejectCode::UnknownFounder,
        RejectCode::FertilityOutOfRange,
    ];
    for code in &codes {
        match code {
            RejectCode::AlreadyStarted
            | RejectCode::ClockNotAtStart
            | RejectCode::FounderNotBornBeforeStart
            | RejectCode::TooManyFounders
            | RejectCode::NotStarted
            | RejectCode::EmptyOverride
            | RejectCode::OverrideParamTwice
            | RejectCode::DeathCauseTwice
            | RejectCode::DeathRateOutOfRange
            | RejectCode::UnknownSim
            | RejectCode::SimNotAlive
            | RejectCode::PartnersOfWrongSex
            | RejectCode::AlreadyPartnered
            | RejectCode::UnknownFounder
            | RejectCode::FertilityOutOfRange => {}
        }
    }
    codes
}

fn all_causes() -> Vec<Cause> {
    let causes = vec![
        Cause::Event(event(1)),
        Cause::Plan(PlanRef {
            by: event(1),
            planned_at: SimTime::from_secs(0),
        }),
        Cause::Input(input_seq(1)),
        Cause::Rule(Code::new("core:x").expect("valid")),
        Cause::Random(sample_draw()),
    ];
    for cause in &causes {
        match cause {
            Cause::Event(_)
            | Cause::Plan(_)
            | Cause::Input(_)
            | Cause::Rule(_)
            | Cause::Random(_) => {}
        }
    }
    causes
}

/// ADR-0004 test 9: every variant survives JSON and postcard.
#[test]
fn formats_roundtrip_every_variant() {
    for payload in all_payloads() {
        let text = json(&payload);
        assert_eq!(from_json::<LogPayload>(&text).ok(), Some(payload.clone()));
        postcard_round_trip(&payload);
    }
    for cause in all_causes() {
        let text = json(&cause);
        assert_eq!(json(&from_json::<Cause>(&text).expect("parses")), text);
        postcard_round_trip(&cause);
    }
    for input in all_inputs() {
        let text = json(&input);
        assert_eq!(from_json::<Input>(&text).ok(), Some(input.clone()));
        postcard_round_trip(&input);
    }
    for code in all_reject_codes() {
        let rejected = InputRejected { code };
        let text = json(&rejected);
        assert_eq!(from_json::<InputRejected>(&text).ok(), Some(rejected));
        postcard_round_trip(&rejected);
    }
}

/// `{"variant":{...}}` with one more field in the inner object.
fn with_unknown_inner_field(value: &Value) -> String {
    let mut value = value.clone();
    let inner = value
        .as_object_mut()
        .and_then(|outer| outer.values_mut().next())
        .and_then(Value::as_object_mut)
        .expect("a struct variant");
    inner.insert("unknown".to_owned(), Value::from(0));
    value.to_string()
}

/// ADR-0004 test 9: one unknown field per variant and per struct is rejected.
#[test]
fn formats_roundtrip_rejects_an_unknown_field_everywhere() {
    for payload in all_payloads() {
        let text = with_unknown_inner_field(&serde_json::to_value(&payload).expect("json"));
        assert!(from_json::<LogPayload>(&text).is_err(), "{text}");
    }
    let plan = serde_json::to_value(&all_causes()[1]).expect("json");
    let text = with_unknown_inner_field(&plan);
    assert!(from_json::<Cause>(&text).is_err(), "{text}");
    let text = with_unknown_inner_field(&serde_json::to_value(sample_input()).expect("json"));
    assert!(from_json::<Input>(&text).is_err(), "{text}");
    // Each struct: accepted as is, rejected with one more field.
    let fields = r#","unknown":0}"#;
    let stamp = r#"{"t":0,"seq":1}"#;
    let rejected = r#"{"code":"already_started"}"#;
    let plan = r#"{"by":1,"planned_at":0}"#;
    let founder = r#"{"born":0,"sex":"male","genotype":{"maternal":"dummy","paternal":"dummy"}}"#;
    let extend = |text: &str| text.replace('}', fields);
    assert!(from_json::<InputStamp>(stamp).is_ok());
    assert!(from_json::<InputStamp>(&extend(stamp)).is_err());
    assert!(from_json::<InputRejected>(rejected).is_ok());
    assert!(from_json::<InputRejected>(&extend(rejected)).is_err());
    assert!(from_json::<PlanRef>(plan).is_ok());
    assert!(from_json::<PlanRef>(&extend(plan)).is_err());
    assert!(from_json::<Founder>(founder).is_ok());
    assert!(from_json::<Founder>(&extend(founder)).is_err());
    for text in [r#"{"event":5,"input":1}"#, r#"{"sim":1,"sim":2}"#] {
        assert!(
            from_json::<Cause>(text).is_err() && from_json::<Subject>(text).is_err(),
            "{text}"
        );
    }
    assert!(from_json::<RejectCode>(r#""never_heard_of""#).is_err());
}

/// The first postcard byte of a variant is its index (all indices stay below 128).
fn postcard_index<T: Serialize>(value: &T) -> u8 {
    postcard::to_allocvec(value).expect("encodes")[0]
}

fn json_name<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value).expect("json") {
        Value::String(name) => name,
        Value::Object(map) if map.len() == 1 => map.keys().next().expect("one key").clone(),
        other => panic!("not an enum form: {other}"),
    }
}

fn line<T: Serialize>(kind: &str, value: &T) -> String {
    format!("{kind} {} {}\n", postcard_index(value), json_name(value))
}

#[test]
fn log_codes_are_stable() {
    let payloads = all_payloads();
    let causes = all_causes();
    let Subject::Sim(_) = Subject::Sim(sim(1));
    let rejects = all_reject_codes();

    let mut actual = String::new();
    actual.extend(payloads.iter().map(|p| line("payload", p)));
    actual.extend(causes.iter().map(|c| line("cause", c)));
    actual.push_str(&line("subject", &Subject::Sim(sim(1))));
    actual.extend(all_inputs().iter().map(|i| line("input", i)));
    actual.extend(rejects.iter().map(|r| line("reject", r)));
    // ADR-0006 test 8: the codes of saving and loading.
    let saves = [SaveError::LogNotTaken, SaveError::YearHashesMismatch];
    for code in &saves {
        match code {
            SaveError::LogNotTaken | SaveError::YearHashesMismatch => {}
        }
    }
    let loads = [
        LoadError::BadMagic,
        LoadError::ForeignFormatVersion,
        LoadError::ForeignCoreVersion,
        LoadError::ForeignDataHash,
        LoadError::ForeignProjectionVersion,
        LoadError::Malformed,
        LoadError::InvalidQueue,
        LoadError::Inconsistent,
    ];
    for code in &loads {
        match code {
            LoadError::BadMagic
            | LoadError::ForeignFormatVersion
            | LoadError::ForeignCoreVersion
            | LoadError::ForeignDataHash
            | LoadError::ForeignProjectionVersion
            | LoadError::Malformed
            | LoadError::InvalidQueue
            | LoadError::Inconsistent => {}
        }
    }
    actual.extend(saves.iter().map(|s| line("save", s)));
    actual.extend(loads.iter().map(|l| line("load", l)));
    // ADR-0004 section 6: the kinds of trace entries.
    let traces = [TracePayload::BirthdayPlanned {
        next: SimTime::from_secs(0),
    }];
    for trace in &traces {
        match trace {
            TracePayload::BirthdayPlanned { .. } => {}
        }
    }
    actual.extend(traces.iter().map(|t| line("trace", t)));
    // The genome of the Durchstich: part of inputs and saves.
    let sexes = [Sex::Female, Sex::Male];
    for sex in &sexes {
        match sex {
            Sex::Female | Sex::Male => {}
        }
    }
    let alleles = [Allele::Wild, Allele::Dummy];
    for allele in &alleles {
        match allele {
            Allele::Wild | Allele::Dummy => {}
        }
    }
    actual.extend(sexes.iter().map(|s| line("sex", s)));
    actual.extend(alleles.iter().map(|a| line("allele", a)));
    // The world parameters an override block can name.
    let params = [ParamChange::HereditaryDiseases(true)];
    for param in &params {
        match param {
            ParamChange::HereditaryDiseases(_) => {}
        }
    }
    actual.extend(params.iter().map(|p| line("param", p)));
    // On whom a cause of death acts: part of `Start` and of saves.
    let targets = [AppliesTo::Everyone, AppliesTo::DummyHomozygous];
    for target in &targets {
        match target {
            AppliesTo::Everyone | AppliesTo::DummyHomozygous => {}
        }
    }
    actual.extend(targets.iter().map(|t| line("applies_to", t)));

    let golden = include_str!("../../../tests/golden/log_codes.txt");
    let pinned: String = golden
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| format!("{l}\n"))
        .collect();
    assert_eq!(actual, pinned, "log codes are never renumbered (ADR-0004)");
}

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
use sim_core::ids::{EventId, InputSeq, MAX_JS_SAFE, SimId};
use sim_core::input::{Founder, Input, InputRecord, InputRejected, InputStamp, RejectCode, Start};
use sim_core::log::{Cause, Causes, LogEntry, LogPayload, PlanRef, Subject};
use sim_core::time::{MAX_ABS_SECS, SimDuration, SimTime};

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
        founders: vec![Founder {
            born: SimTime::from_secs(-946_080_000),
        }],
    })
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
const INPUT_POSTCARD: &str = "00ff0114746573743a666f726b5f6e6f5f64656e7369747901ffa3a08607";
const STAMP_POSTCARD: &str = "0a01";
const RECORD_POSTCARD: &str = "010100ff0114746573743a666f726b5f6e6f5f64656e7369747901ffa3a08607";
const NEGATIVE_STAMP_POSTCARD: &str = "09ffffffffffffff0f";
const REJECTED_POSTCARD: &str = "00";
const MIN_DURATION_POSTCARD: &str = "ffffe3e3cda60e";
const MAX_ID_POSTCARD: &str = "ffffffffffffff0f";
const MAX_U64_POSTCARD: &str = "ffffffffffffffffff01";

const INPUT_JSON: &str = r#"{"start":{"world_seed":"00000000000000ff","founder_tag":"test:fork_no_density","founders":[{"born":-946080000}]}}"#;

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
        founders: Vec::new(),
    });
    let mut expected = vec![0];
    expected.extend(hex(MAX_U64_POSTCARD));
    expected.extend([3, b'a', b':', b'b', 0]);
    assert_eq!(postcard_round_trip(&full_seed), expected);
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
        from_json::<Input>(&INPUT_JSON.replace("00000000000000ff", seed)).map(|input| {
            let Input::Start(start) = input;
            start.world_seed
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
        INPUT_JSON.replace(r#"{"born""#, r#"{"sex":1,"born""#),
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
        founders: vec![Founder {
            born: SimTime::from_secs(MAX_ABS_SECS),
        }],
    });
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
        },
        LogPayload::Birthday { age_years: 30 },
    ];
    // A new variant fails to compile here until it is added to the list above.
    for payload in &payloads {
        match payload {
            LogPayload::Started { .. }
            | LogPayload::FounderAdded { .. }
            | LogPayload::Birthday { .. } => {}
        }
    }
    payloads
}

fn all_reject_codes() -> Vec<RejectCode> {
    let codes = vec![
        RejectCode::AlreadyStarted,
        RejectCode::ClockNotAtStart,
        RejectCode::FounderNotBornBeforeStart,
    ];
    for code in &codes {
        match code {
            RejectCode::AlreadyStarted
            | RejectCode::ClockNotAtStart
            | RejectCode::FounderNotBornBeforeStart => {}
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
    ];
    for cause in &causes {
        match cause {
            Cause::Event(_) | Cause::Plan(_) | Cause::Input(_) | Cause::Rule(_) => {}
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
    let Input::Start(_) = sample_input();
    let text = json(&sample_input());
    assert_eq!(from_json::<Input>(&text).ok(), Some(sample_input()));
    postcard_round_trip(&sample_input());
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
    let founder = r#"{"born":0}"#;
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
    assert!(from_json::<RejectCode>(r#""not_started""#).is_err());
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
    let Input::Start(_) = sample_input();
    let rejects = all_reject_codes();

    let mut actual = String::new();
    actual.extend(payloads.iter().map(|p| line("payload", p)));
    actual.extend(causes.iter().map(|c| line("cause", c)));
    actual.push_str(&line("subject", &Subject::Sim(sim(1))));
    actual.push_str(&line("input", &sample_input()));
    actual.extend(rejects.iter().map(|r| line("reject", r)));

    let golden = include_str!("../../../tests/golden/log_codes.txt");
    let pinned: String = golden
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| format!("{l}\n"))
        .collect();
    assert_eq!(actual, pinned, "log codes are never renumbered (ADR-0004)");
}

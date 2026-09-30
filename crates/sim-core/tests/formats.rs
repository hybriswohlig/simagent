//! Formats of IDs, log entries and inputs (ADR-0004): JSON is the human-readable form
//! and the command schema, postcard the canonical binary form. Both round trip, both
//! reject what the types forbid, and no JSON number leaves the JavaScript-safe range.

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use sim_core::data_id::{Code, DataId};
use sim_core::ids::{EventId, InputSeq, MAX_JS_SAFE, SimId};
use sim_core::input::{Founder, Input, InputRejected, InputStamp, RejectCode, Start};
use sim_core::log::{Cause, Causes, LogEntry, LogPayload, PlanRef, Subject};
use sim_core::time::{MAX_ABS_SECS, SimDuration, SimTime};

fn event(n: u64) -> EventId {
    EventId::try_from(n).expect("valid event id")
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
        .with(Cause::Input(InputSeq::try_from(1).expect("valid"))),
        payload: LogPayload::Birthday { age_years: 1 },
    }
}

fn sample_input() -> Input {
    Input::Start(Start {
        world_seed: 0xff,
        founder_tag: u64::MAX,
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

fn postcard_round_trip<T: Serialize + DeserializeOwned>(value: &T) -> Vec<u8> {
    let bytes = postcard::to_allocvec(value).expect("encodes");
    let back: T = postcard::from_bytes(&bytes).expect("decodes");
    assert_eq!(postcard::to_allocvec(&back).expect("encodes"), bytes);
    bytes
}

const ENTRY_JSON: &str = r#"{"id":7,"t":86400,"subjects":[{"sim":42}],"causes":[{"plan":{"by":3,"planned_at":0}},{"rule":"core:gompertz_hazard"},{"event":5},{"input":1}],"payload":{"core:birthday":{"age_years":1}}}"#;
const INPUT_JSON: &str = r#"{"start":{"world_seed":"00000000000000ff","founder_tag":"ffffffffffffffff","founders":[{"born":-946080000}]}}"#;

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
fn postcard_round_trips() {
    postcard_round_trip(&sample_entry());
    let bytes = postcard_round_trip(&sample_input());
    // Binary keeps the plain u64: the full-range tag is a 10-byte varint, not text.
    assert!(bytes.len() < 32, "{bytes:?}");
    postcard_round_trip(&InputStamp {
        t: SimTime::from_secs(-5),
        seq: InputSeq::try_from(MAX_JS_SAFE).expect("valid"),
    });
    postcard_round_trip(&InputRejected {
        code: RejectCode::AlreadyStarted,
    });
    postcard_round_trip(&DataId::new("core:height").expect("valid"));
    postcard_round_trip(&SimDuration::from_secs(-MAX_ABS_SECS));
}

#[test]
fn ids_outside_one_to_js_safe_are_rejected() {
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
    }
}

#[test]
fn hex_u64_needs_exactly_sixteen_lowercase_digits() {
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
    assert_eq!(sim_core::hex_u64::parse("ffffffffffffffff"), Some(u64::MAX));
    assert_eq!(sim_core::hex_u64::parse("fffffffffffffff"), None);
}

#[test]
fn causes_are_never_empty() {
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
fn unknown_fields_and_variants_are_rejected() {
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

/// ADR-0004 section 3: every u64 that can exceed 2^53 - 1 is a hex string in JSON.
#[test]
fn no_json_number_leaves_the_js_safe_range() {
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
        .with(Cause::Input(
            InputSeq::try_from(MAX_JS_SAFE).expect("valid"),
        )),
        payload: LogPayload::Birthday {
            age_years: u32::MAX,
        },
    };
    let extreme_input = Input::Start(Start {
        world_seed: u64::MAX,
        founder_tag: u64::MAX,
        founders: vec![Founder {
            born: SimTime::from_secs(MAX_ABS_SECS),
        }],
    });
    check(
        &serde_json::to_value(&extreme_entry).expect("json"),
        "entry",
    );
    check(
        &serde_json::to_value(&extreme_input).expect("json"),
        "input",
    );
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

fn all_causes() -> Vec<Cause> {
    let causes = vec![
        Cause::Event(event(1)),
        Cause::Plan(PlanRef {
            by: event(1),
            planned_at: SimTime::from_secs(0),
        }),
        Cause::Input(InputSeq::try_from(1).expect("valid")),
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
fn every_variant_round_trips() {
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
    postcard_round_trip(&sample_input());
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
fn an_unknown_field_is_rejected_everywhere() {
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
    let RejectCode::AlreadyStarted = RejectCode::AlreadyStarted;

    let mut actual = String::new();
    actual.extend(payloads.iter().map(|p| line("payload", p)));
    actual.extend(causes.iter().map(|c| line("cause", c)));
    actual.push_str(&line("subject", &Subject::Sim(sim(1))));
    actual.push_str(&line("input", &sample_input()));
    actual.push_str(&line("reject", &RejectCode::AlreadyStarted));

    let golden = include_str!("../../../tests/golden/log_codes.txt");
    let pinned: String = golden
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| format!("{l}\n"))
        .collect();
    assert_eq!(actual, pinned, "log codes are never renumbered (ADR-0004)");
}

//! The hand logs of `tools/story_reference.py --check`, and three scratch runs.
//! Expected numbers are the Python reference.

use serde_json::{Value, json};

use super::{Report, Story, check, parse_events};

const LIE_DRAW: &str = "0000000000000024";
const REFUTE_DRAW: &str = "0000000000000025";
/// speaker, hearer, third, avoided person, source, origin.
const FIRST: [i64; 6] = [1, 2, 3, 4, 1, 1];

fn sentence() -> Value {
    json!({"tat": {"doer": 4, "kind": 1, "other": 2, "day": 3}})
}

fn other_sentence() -> Value {
    json!({"tat": {"doer": 4, "kind": 0, "other": 2, "day": 3}})
}

fn aimed_at_jonas() -> Value {
    json!({"tat": {"doer": 4, "kind": 1, "other": 3, "day": 3}})
}

fn line(id: i64, second: i64, sim: i64, causes: Value, payload: Value) -> Value {
    json!({
        "id": id,
        "t": second,
        "subjects": [{"sim": sim}],
        "causes": causes,
        "payload": payload,
    })
}

fn event(id: i64) -> Value {
    json!([{"event": id}])
}

fn plan(by: i64, at: i64) -> Value {
    json!([{"plan": {"by": by, "planned_at": at}}])
}

fn draw(tag: &str, subject: i64, occasion: i64, detail: i64) -> Value {
    json!([{
        "random": {
            "tag": tag,
            "subject": format!("{subject:016x}"),
            "occasion": format!("{occasion:016x}"),
            "detail": format!("{detail:016x}"),
        }
    }])
}

fn told(
    id: i64,
    second: i64,
    speaker: i64,
    hearer: i64,
    statement: Value,
    lie: bool,
    cause: Value,
) -> Value {
    line(
        id,
        second,
        speaker,
        cause,
        json!({"core:told": {"to": hearer, "statement": statement, "certainty": 900, "lie": lie}}),
    )
}

fn believed(
    id: i64,
    second: i64,
    hearer: i64,
    statement: Value,
    source: i64,
    origin: i64,
    cause: Value,
) -> Value {
    line(
        id,
        second,
        hearer,
        cause,
        json!({
            "core:believed": {
                "statement": statement,
                "certainty": 733,
                "basis": "gehört",
                "source": source,
                "origin": origin,
                "replaced": false,
            }
        }),
    )
}

fn bond(
    id: i64,
    second: i64,
    sim: i64,
    other: i64,
    before: i64,
    after: i64,
    cause: Value,
) -> Value {
    line(
        id,
        second,
        sim,
        cause,
        json!({
            "core:bond_changed": {
                "other": other,
                "before": [400, before, 400, 0, 200, 0],
                "after": [400, after, 400, 0, 200, 0],
            }
        }),
    )
}

fn company(id: i64, second: i64, sim: i64, avoided: Option<i64>, cause: Value) -> Value {
    line(
        id,
        second,
        sim,
        cause,
        json!({
            "core:company": {
                "objects": [{
                    "object": 1,
                    "liked": null,
                    "company": 0,
                    "avoided": avoided,
                    "avoid": 8000,
                }]
            }
        }),
    )
}

fn refuted(id: i64, second: i64, hearer: i64, statement: Value, source: i64, by: i64) -> Value {
    line(
        id,
        second,
        hearer,
        draw(REFUTE_DRAW, by, second, hearer),
        json!({"core:refuted": {"by": by, "statement": statement, "source": source}}),
    )
}

fn started(id: i64, second: i64) -> Value {
    line(
        id,
        second,
        1,
        json!([{"input": 1}]),
        json!({"core:started": {"founders": 4}}),
    )
}

fn full_log(lie: bool) -> Vec<Value> {
    let told_cause = if lie {
        draw(LIE_DRAW, 1, 1000, 2)
    } else {
        event(12)
    };
    vec![
        started(10, 100),
        company(11, 500, 2, Some(4), plan(10, 100)),
        bond(12, 1000, 2, 8, 100, 50, plan(10, 900)),
        told(13, 1000, 1, 2, sentence(), lie, told_cause),
        believed(14, 1000, 2, sentence(), 1, 1, event(13)),
        line(
            15,
            1000,
            2,
            event(14),
            json!({"core:unbelieved": {"statement": other_sentence()}}),
        ),
        bond(16, 1000, 2, 4, 450, 370, event(15)),
        told(17, 4000, 2, 3, other_sentence(), false, plan(16, 1000)),
        told(18, 5000, 2, 3, sentence(), false, plan(16, 1000)),
        company(20, 8000, 2, Some(4), plan(16, 7000)),
        refuted(21, 8500, 2, other_sentence(), 1, 5),
        bond(22, 8500, 2, 1, 500, 380, event(21)),
        refuted(23, 9000, 2, sentence(), 1, 5),
        bond(24, 9000, 2, 1, 500, 380, event(23)),
    ]
}

fn later_log() -> Vec<Value> {
    vec![
        started(10, 100),
        told(
            11,
            1000,
            1,
            2,
            aimed_at_jonas(),
            true,
            draw(LIE_DRAW, 1, 1000, 2),
        ),
        believed(12, 1000, 2, aimed_at_jonas(), 1, 1, event(11)),
        bond(13, 1000, 2, 4, 450, 430, event(12)),
        told(14, 5000, 2, 3, aimed_at_jonas(), false, plan(13, 1000)),
        believed(15, 5000, 3, aimed_at_jonas(), 2, 1, event(14)),
        bond(16, 5000, 3, 4, 450, 370, event(15)),
        company(17, 8000, 3, Some(4), plan(16, 7000)),
        refuted(18, 9000, 3, aimed_at_jonas(), 2, 4),
        bond(19, 9000, 3, 2, 500, 380, event(18)),
    ]
}

fn disordered_log() -> Vec<Value> {
    vec![
        told(10, 1000, 1, 2, sentence(), true, draw(LIE_DRAW, 1, 1000, 2)),
        believed(11, 1000, 2, sentence(), 1, 1, event(10)),
        bond(12, 1000, 2, 4, 450, 370, event(11)),
        company(13, 2000, 2, Some(4), plan(12, 1000)),
        refuted(14, 3000, 2, sentence(), 1, 5),
        bond(15, 3000, 2, 1, 500, 380, event(14)),
        told(16, 4000, 2, 3, sentence(), false, plan(12, 1000)),
    ]
}

fn refuted_too_early() -> Vec<Value> {
    vec![
        told(10, 1000, 1, 2, sentence(), true, draw(LIE_DRAW, 1, 1000, 2)),
        believed(11, 1000, 2, sentence(), 1, 1, event(10)),
        bond(12, 1000, 2, 4, 450, 370, event(11)),
        told(13, 5000, 2, 3, sentence(), false, plan(12, 1000)),
        refuted(14, 6000, 2, sentence(), 1, 5),
        bond(15, 6000, 2, 1, 500, 380, event(14)),
        company(16, 8000, 2, Some(4), plan(12, 7000)),
    ]
}

fn avoidance_log() -> Vec<Value> {
    vec![
        started(10, 100),
        told(11, 1000, 1, 2, sentence(), false, plan(10, 900)),
        believed(12, 1000, 2, sentence(), 1, 1, event(11)),
        bond(13, 1000, 2, 4, 450, 370, event(12)),
        company(14, 2000, 2, Some(4), plan(13, 1000)),
        told(15, 5000, 2, 3, sentence(), false, plan(13, 1000)),
        company(16, 8000, 2, Some(4), plan(13, 7000)),
        refuted(17, 9000, 2, sentence(), 1, 5),
        bond(18, 9000, 2, 1, 500, 380, event(17)),
    ]
}

fn ndjson(entries: &[Value]) -> String {
    let mut text = String::new();
    for entry in entries {
        text.push_str(&serde_json::to_string(entry).expect("entry"));
        text.push('\n');
    }
    text
}

fn id_of(entry: &Value) -> i64 {
    entry["id"].as_i64().expect("id")
}

fn expect(entries: &[Value], hops: usize, chains: usize, missing: &[&str]) -> Report {
    let report = check(&ndjson(entries)).expect("hand log");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(report.hops, hops);
    assert_eq!(report.chains.len(), chains);
    if chains == 0 {
        assert_eq!(report.groups.len(), 1);
        assert_eq!(report.groups[0].missing.as_slice(), missing);
    } else {
        assert!(missing.is_empty());
        assert!(report.groups.is_empty());
    }
    report
}

fn assert_chain(story: &Story, ids: [i64; 7], lie: bool, roles: [i64; 6]) {
    assert_eq!(
        [
            story.told,
            story.believed,
            story.told_on,
            story.lowered,
            story.avoided,
            story.refuted,
            story.trust_loss,
        ],
        ids.map(Some)
    );
    let links = [ids[0], ids[1], ids[2], ids[4], ids[5], ids[6]];
    let mut ordered = links;
    ordered.sort_unstable();
    assert_eq!(links, ordered);
    assert_eq!(story.lie, lie);
    assert_eq!(
        [
            story.speaker,
            story.hearer,
            story.third,
            story.avoided_person,
            story.source,
            story.origin
        ],
        roles.map(Some)
    );
}

const CHAIN: [i64; 7] = [13, 14, 18, 16, 20, 23, 24];

#[test]
fn full() {
    let report = expect(&full_log(false), 1, 1, &[]);
    assert_chain(&report.chains[0], CHAIN, false, FIRST);
}

#[test]
fn direct_cause() {
    let mut direct: Vec<Value> = full_log(false)
        .into_iter()
        .filter(|entry| id_of(entry) != 15)
        .collect();
    for entry in &mut direct {
        if id_of(entry) == 16 {
            entry["causes"] = event(14);
        }
    }
    let report = expect(&direct, 1, 1, &[]);
    assert_chain(&report.chains[0], CHAIN, false, FIRST);
}

#[test]
fn missing_told() {
    let mut kept = Vec::new();
    for mut entry in full_log(false) {
        let id = id_of(&entry);
        if entry["payload"].get("core:told").is_some() || id == 14 || id == 15 {
            continue;
        }
        if id == 16 {
            entry["causes"] = plan(12, 1000);
        }
        kept.push(entry);
    }
    expect(
        &kept,
        0,
        0,
        &[
            "told",
            "believed",
            "told_on",
            "avoided",
            "refuted",
            "trust_loss",
        ],
    );
}

#[test]
fn missing_believed() {
    let mut kept = Vec::new();
    for mut entry in full_log(false) {
        let id = id_of(&entry);
        if id == 14 || id == 15 {
            continue;
        }
        if id == 16 {
            entry["causes"] = plan(13, 1000);
        }
        kept.push(entry);
    }
    expect(
        &kept,
        0,
        0,
        &["believed", "told_on", "avoided", "refuted", "trust_loss"],
    );
}

#[test]
fn missing_told_on() {
    let kept: Vec<Value> = full_log(false)
        .into_iter()
        .filter(|entry| id_of(entry) != 18)
        .collect();
    expect(&kept, 0, 0, &["told_on"]);
}

#[test]
fn missing_avoided() {
    let mut log = full_log(false);
    for entry in &mut log {
        if id_of(entry) == 20 {
            entry["payload"]["core:company"]["objects"][0]["avoided"] = Value::Null;
        }
    }
    expect(&log, 1, 0, &["avoided"]);
}

#[test]
fn missing_refuted() {
    let kept: Vec<Value> = full_log(false)
        .into_iter()
        .filter(|entry| id_of(entry) != 23 && id_of(entry) != 24)
        .collect();
    expect(&kept, 1, 0, &["refuted", "trust_loss"]);
}

#[test]
fn missing_trust_loss() {
    let kept: Vec<Value> = full_log(false)
        .into_iter()
        .filter(|entry| id_of(entry) != 24)
        .collect();
    expect(&kept, 1, 0, &["trust_loss"]);
}

#[test]
fn two_hop() {
    let log = vec![
        started(10, 100),
        told(13, 1000, 1, 2, sentence(), false, plan(10, 900)),
        believed(14, 1000, 2, sentence(), 1, 1, event(13)),
        told(18, 5000, 2, 3, sentence(), false, plan(14, 1000)),
    ];
    expect(&log, 1, 0, &["avoided", "refuted", "trust_loss"]);
}

#[test]
fn lie_origin() {
    let lied = full_log(true);
    assert!(lied.iter().all(|entry| {
        !(id_subject(entry) == Some(1) && entry["payload"].get("core:believed").is_some())
    }));
    let report = expect(&lied, 1, 1, &[]);
    assert_chain(&report.chains[0], CHAIN, true, FIRST);
}

fn id_subject(entry: &Value) -> Option<i64> {
    entry["subjects"][0]["sim"].as_i64()
}

#[test]
fn trust_stays_above() {
    let mut log = full_log(false);
    for entry in &mut log {
        if id_of(entry) == 16 {
            entry["payload"]["core:bond_changed"]["after"][1] = json!(430);
        }
    }
    let report = expect(&log, 1, 0, &["avoided"]);
    assert_eq!(report.groups[0].example.note, "bond 16 leaves trust at 430");
}

#[test]
fn company_before_bond() {
    let kept: Vec<Value> = full_log(false)
        .into_iter()
        .filter(|entry| id_of(entry) != 20)
        .collect();
    expect(&kept, 1, 0, &["avoided"]);
}

#[test]
fn seen_basis() {
    let mut log = full_log(false);
    for entry in &mut log {
        if id_of(entry) == 14 {
            entry["payload"]["core:believed"]["basis"] = json!("gesehen");
        }
    }
    expect(
        &log,
        0,
        0,
        &["believed", "told_on", "avoided", "refuted", "trust_loss"],
    );
}

#[test]
fn trust_loss_not_in_source() {
    let mut log = full_log(false);
    for entry in &mut log {
        if id_of(entry) == 24 {
            entry["payload"]["core:bond_changed"]["other"] = json!(4);
        }
    }
    expect(&log, 1, 0, &["trust_loss"]);
}

#[test]
fn two_passings() {
    let mut extra = full_log(false);
    let at = extra
        .iter()
        .position(|entry| id_of(entry) == 18)
        .expect("told on");
    extra.insert(
        at + 1,
        told(19, 6000, 2, 6, sentence(), false, plan(16, 1000)),
    );
    let report = expect(&extra, 2, 2, &[]);
    assert_chain(
        &report.chains[0],
        [13, 14, 18, 16, 20, 23, 24],
        false,
        FIRST,
    );
    assert_chain(
        &report.chains[1],
        [13, 14, 19, 16, 20, 23, 24],
        false,
        [1, 2, 6, 4, 1, 1],
    );
}

#[test]
fn told_on_too_late() {
    expect(
        &disordered_log(),
        1,
        0,
        &["avoided", "refuted", "trust_loss"],
    );
}

#[test]
fn refuted_before_avoided() {
    expect(&refuted_too_early(), 1, 0, &["refuted", "trust_loss"]);
}

#[test]
fn avoidance_before_told_on() {
    let report = expect(&avoidance_log(), 1, 1, &[]);
    assert_chain(
        &report.chains[0],
        [11, 12, 15, 13, 16, 17, 18],
        false,
        FIRST,
    );
}

#[test]
fn later_hearer() {
    let report = expect(&later_log(), 1, 1, &[]);
    assert_chain(
        &report.chains[0],
        [11, 12, 14, 16, 17, 18, 19],
        true,
        [1, 3, 3, 4, 2, 1],
    );
}

#[test]
fn broken_middle_line() {
    let sample = serde_json::to_string(&full_log(false)[0]).expect("sample");
    let text = format!("{sample}\nNOT JSON\n{sample}\n");
    let err = parse_events(&text, "bad").expect_err("middle line");
    assert!(err.contains("bad:2"), "{err}");
}

#[test]
fn cut_off_last_line() {
    let sample = serde_json::to_string(&full_log(false)[0]).expect("sample");
    let text = format!("{sample}\n{{\"id\":");
    let (entries, warnings) = parse_events(&text, "cut").expect("cut-off is a warning");
    assert_eq!(entries.len(), 1);
    assert!(
        warnings.iter().any(|warning| warning.contains("cut-off")),
        "{warnings:?}"
    );
}

#[test]
fn seed_07() {
    let report = check(include_str!("../fixtures/story/seed-07.ndjson")).expect("seed 7");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(report.hops, 5);
    assert!(report.groups.is_empty());
    assert_eq!(report.chains.len(), 1);
    assert_chain(
        &report.chains[0],
        [3688, 3689, 4517, 3690, 4526, 4988, 4989],
        true,
        [4, 2, 1, 3, 4, 4],
    );
}

#[test]
fn seed_08() {
    let report = check(include_str!("../fixtures/story/seed-08.ndjson")).expect("seed 8");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(report.hops, 4);
    assert!(report.groups.is_empty());
    assert_eq!(report.chains.len(), 1);
    assert_chain(
        &report.chains[0],
        [4525, 4526, 4599, 4527, 4642, 6101, 6102],
        true,
        [3, 1, 2, 4, 3, 3],
    );
}

#[test]
fn seed_11() {
    let report = check(include_str!("../fixtures/story/seed-11.ndjson")).expect("seed 11");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(report.hops, 0);
    assert!(report.chains.is_empty());
    assert_eq!(report.groups.len(), 1);
    assert_eq!(report.groups[0].missing.as_slice(), ["told_on", "avoided"]);
    assert_eq!(report.groups[0].count, 1);
    let example = &report.groups[0].example;
    assert_eq!(example.told, Some(11297));
    assert_eq!(example.believed, Some(11298));
    assert_eq!(example.told_on, None);
    assert_eq!(example.lowered, None);
    assert_eq!(example.avoided, None);
    assert_eq!(example.refuted, Some(12437));
    assert_eq!(example.trust_loss, Some(12438));
    assert_eq!(example.note, "bond 11299 leaves trust at 620");
}

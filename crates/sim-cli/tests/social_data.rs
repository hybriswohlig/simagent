//! ADR-0030 section 6 and ADR-0031: `data/social/social.json` loads through the reader with the
//! numbers of the ADRs, enters the data hash as `tools/data_reference.py` computes it, and each
//! rule of the core and of the reader rejects a broken file with its code.

use serde_json::{Value, json};
use sim_cli::data::{core_data, social_from};
use sim_core::social::data::{BondValue, Condition, Occasion, Step, Who, memory_kinds};

const SOCIAL: &str = include_str!("../../../data/social/social.json");

/// The data hash of the repository's data from `tools/data_reference.py` (ADR-0007), with the
/// social data after the houses; without them it is 0a08eab115b75093. The feelings of ADR-0032
/// moved it from 01ce3a4b0ac58e6f.
const DATA_HASH: u64 = 0x57e6_320f_6113_787a;

/// The repository's data, for the kinds of furniture of its houses.
fn repository() -> &'static sim_core::data::Data {
    static DATA: std::sync::OnceLock<sim_core::data::Data> = std::sync::OnceLock::new();
    DATA.get_or_init(|| core_data().expect("data"))
}

fn file() -> Value {
    serde_json::from_str(SOCIAL).expect("the file")
}

/// The message of the reader for the file after `change`.
fn refused(change: impl FnOnce(&mut Value)) -> String {
    let mut social = file();
    change(&mut social);
    let error = social_from(&social.to_string(), repository().houses().expect("houses"))
        .expect_err("refused");
    error.to_string()
}

#[test]
fn the_repository_social_data_loads_with_the_numbers_of_the_adrs() {
    let social = social_from(SOCIAL, repository().houses().expect("houses")).expect("loads");
    let limits = &social.limits;
    // The limits 32, 32, 8, 3600, 34, 16 of ADR-0030 section 6 and the others of both ADRs.
    assert_eq!(
        (limits.memories, limits.bonds, limits.keep_on_move_out),
        (32, 32, 8)
    );
    assert_eq!(
        (
            limits.pair_gap_seconds,
            limits.entries_per_day,
            limits.talk_entries
        ),
        (3600, 34, 16)
    );
    assert_eq!(
        (limits.beliefs, limits.deeds, limits.deed_window_days),
        (32, 64, 30)
    );
    assert_eq!(
        (
            limits.min_age,
            limits.talk_delay_seconds,
            limits.talks_per_day
        ),
        (6, 600, 8)
    );
    // The six kinds of memory in the order of their numbers.
    let ids: Vec<_> = social.memory.kinds.iter().map(|k| k.id.clone()).collect();
    assert_eq!(ids, memory_kinds());
    assert_eq!(ids[1].as_str(), "core:child_born");
    assert_eq!(social.memory.recall_step, 300);
    assert_eq!(social.memory.relative_phi_eighths, 1);
    // The start values of a bond by role, ADR-0030 section 2.
    let start = &social.bonds.start;
    assert_eq!(start.partner, [700, 700, 500, 500, 600, 0]);
    assert_eq!(start.parent, [650, 650, 600, 0, 600, 0]);
    assert_eq!(start.child, [700, 600, 300, 0, 600, 0]);
    assert_eq!(start.sibling, [550, 550, 400, 0, 500, 0]);
    assert_eq!(start.kin, [500, 500, 450, 0, 300, 0]);
    assert_eq!(start.housemate, [400, 450, 400, 0, 200, 0]);
    assert_eq!(
        (social.bonds.familiarity_fade, social.bonds.grudge_fade),
        (5, 10)
    );
    // The six topics of ADR-0030 section 3: their conditions and the steps of their outcomes.
    let topics = &social.topics;
    assert_eq!(topics.len(), 6);
    let conditions: Vec<Condition> = topics.iter().map(|topic| topic.condition).collect();
    assert_eq!(
        conditions,
        [
            Condition::Always,
            Condition::AffectionFrom(500),
            Condition::Always,
            Condition::Gossip,
            Condition::CommonMemory,
            Condition::GrudgeFrom(300),
        ]
    );
    // Necken: both under 60.
    assert_eq!((topics[2].min_age, topics[2].max_age), (6, 59));
    let step = |who, value, delta| Step { who, value, delta };
    assert_eq!(
        topics[0].steps,
        [step(Who::Both, BondValue::Familiarity, 40)]
    );
    assert_eq!(
        topics[1].steps,
        [
            step(Who::Target, BondValue::Affection, 30),
            step(Who::Target, BondValue::Respect, 20)
        ]
    );
    assert_eq!(topics[2].steps, [step(Who::Target, BondValue::Grudge, 60)]);
    assert!(topics[3].steps.is_empty());
    assert_eq!(topics[4].steps, [step(Who::Both, BondValue::Affection, 20)]);
    assert_eq!(
        topics[5].steps,
        [
            step(Who::Target, BondValue::Grudge, -200),
            step(Who::Target, BondValue::Trust, 60)
        ]
    );
    // The kinds of furniture each topic may take place at, by their data ids.
    let furniture: Vec<Vec<&str>> = (topics.iter())
        .map(|topic| topic.furniture.iter().map(|id| id.as_str()).collect())
        .collect();
    let (table, sofa, tv, toys) = (
        "core:dining_table",
        "core:sofa",
        "core:tv",
        "core:toy_chest",
    );
    assert_eq!(
        furniture,
        [
            vec![table, sofa, tv],
            vec![table, sofa],
            vec![table, sofa, toys],
            vec![table, sofa],
            vec![table, sofa, tv],
            vec![table, sofa],
        ]
    );
    // The choice and the beliefs, ADR-0030 section 4, ADR-0031 sections 2 to 5.
    let choice = &social.choice;
    assert_eq!((choice.company, choice.avoid), (8000, 8000));
    assert_eq!(
        (choice.company_min_sum, choice.avoid_below_trust),
        (600, 400)
    );
    let beliefs = &social.beliefs;
    assert_eq!((beliefs.lie_ppm, beliefs.refute_ppm), (50_000, 200_000));
    assert_eq!(
        (
            beliefs.heard_cap,
            beliefs.heard_floor,
            beliefs.heard_divisor
        ),
        (900, 500, 1500)
    );
    assert_eq!((beliefs.confirm_step, beliefs.confirm_cap), (50, 900));
    assert_eq!((beliefs.lie_grudge, beliefs.lie_certainty), (700, 900));
    assert_eq!((beliefs.refute_trust, beliefs.refute_grudge), (-120, 150));
    assert_eq!(
        (
            beliefs.heard_aimed_trust,
            beliefs.heard_aimed_grudge,
            beliefs.heard_other_trust
        ),
        (-80, 100, -20)
    );
    assert_eq!(beliefs.inferred_certainty, 900);
    assert_eq!(beliefs.deeds.len(), 3);
    // The feelings, ADR-0032 sections 1 to 4 and 6: rates per day, the temper's return, the table
    // of occasions in the order of the ADR, the band 600 and 3/2, the contagion 700, 150, 699.
    let feelings = &social.feelings;
    assert_eq!(feelings.rates.all(), [300, 300, 150, 400]);
    assert_eq!(feelings.mood_return, 100);
    let band = &feelings.band;
    assert_eq!(
        (band.min_affection, band.numerator, band.denominator),
        (600, 3, 2)
    );
    let contagion = &feelings.contagion;
    assert_eq!(
        (contagion.min_anger, contagion.step, contagion.cap),
        (700, 150, 699)
    );
    let rows: Vec<(Occasion, [i16; 4])> = (feelings.table.iter())
        .map(|row| (row.occasion, row.steps()))
        .collect();
    assert_eq!(
        rows,
        [
            (Occasion::TalkEveryday, [40, 0, 0, 0]),
            (Occasion::TalkPraise, [150, 0, 0, 0]),
            (Occasion::TalkTease, [0, 100, 0, 0]),
            (Occasion::TalkRecall, [100, 0, 0, 0]),
            (Occasion::TalkApology, [50, -200, 0, 0]),
            (Occasion::Heard, [0, 200, 100, 0]),
            (Occasion::Refuted, [0, 150, 0, 0]),
            (Occasion::Birth, [800, 0, 0, 0]),
            (Occasion::PartnerLost, [0, 0, 1000, 200]),
            (Occasion::RelativeDied, [0, 0, 800, 100]),
            (Occasion::PartnershipBegan, [600, 0, 0, 0]),
            (Occasion::Separated, [0, 300, 600, 0]),
            (Occasion::NeedEmpty, [0, 150, 0, 100]),
        ]
    );
    // An occasion without a row is all 0 (the mild heard deed and gossip have none).
    let mut thin = feelings.clone();
    thin.table.retain(|row| row.occasion != Occasion::Birth);
    assert_eq!(thin.steps(Occasion::Birth), [0; 4]);
    assert_eq!(feelings.steps(Occasion::Birth), [800, 0, 0, 0]);
}

/// The data hash covers the social data; `tools/data_reference.py` encodes and checks the file
/// itself and gives this number.
#[test]
fn the_social_data_are_in_the_data_hash() {
    let data = core_data().expect("data");
    assert_eq!(data.hash(), DATA_HASH);
    assert_eq!(data.social().expect("the social data").limits.memories, 32);
}

#[test]
fn the_reader_and_the_core_reject_a_broken_file() {
    let code = |message: String, code: &str| {
        assert!(message.contains(code), "{code} not in {message}");
    };
    // The reader: an unknown field, a missing one, a wrong type.
    code(
        refused(|f| f["limits"]["extra"] = json!(1)),
        "unknown field",
    );
    code(
        refused(|f| drop(f["limits"].as_object_mut().expect("o").remove("bonds"))),
        "missing field",
    );
    code(
        refused(|f| f["limits"]["memories"] = json!("many")),
        "invalid type",
    );
    code(
        refused(|f| f["topics"][0]["condition"] = json!("sometimes")),
        "unknown variant",
    );
    code(refused(|f| f["extra"] = json!(1)), "unknown field");
    // The core's checks, in the order of its codes.
    code(
        refused(|f| f["limits"]["memories"] = json!(0)),
        "social.limits",
    );
    code(
        refused(|f| f["limits"]["memories"] = json!(65)),
        "social.limits",
    );
    code(
        refused(|f| f["limits"]["keep_on_move_out"] = json!(33)),
        "social.limits",
    );
    code(
        refused(|f| f["limits"]["min_age"] = json!(5)),
        "social.limits",
    );
    code(
        refused(|f| f["limits"]["talk_delay_seconds"] = json!(59)),
        "social.limits",
    );
    code(
        refused(|f| f["limits"]["pair_gap_seconds"] = json!(599)),
        "social.limits",
    );
    code(
        refused(|f| f["limits"]["talk_entries"] = json!(35)),
        "social.limits",
    );
    code(
        refused(|f| f["memory"]["recall_step"] = json!(0)),
        "social.memory",
    );
    code(
        refused(|f| f["memory"]["relative_phi_eighths"] = json!(9)),
        "social.memory",
    );
    code(
        refused(|f| f["memory"]["kinds"][0]["id"] = json!("core:chat")),
        "social.memory_kind",
    );
    code(
        refused(|f| drop(f["memory"]["kinds"].as_array_mut().expect("a").pop())),
        "social.memory_kind",
    );
    code(
        refused(|f| f["memory"]["kinds"][1]["strength"] = json!(0)),
        "social.memory_kind",
    );
    code(
        refused(|f| f["memory"]["kinds"][1]["strength"] = json!(1001)),
        "social.memory_kind",
    );
    code(
        refused(|f| f["memory"]["kinds"][1]["fade"] = json!(1001)),
        "social.memory_kind",
    );
    code(
        refused(|f| f["memory"]["kinds"][1]["valence"] = json!(-1001)),
        "social.memory_kind",
    );
    code(
        refused(|f| f["bonds"]["start"]["kin"][5] = json!(1001)),
        "social.bonds",
    );
    code(
        refused(|f| f["bonds"]["grudge_fade"] = json!(1001)),
        "social.bonds",
    );
    code(refused(|f| f["topics"] = json!([])), "social.topics");
    code(
        refused(|f| f["topics"][0]["condition"] = json!("gossip")),
        "social.topics",
    );
    code(
        refused(|f| f["topics"][1]["id"] = json!("core:everyday")),
        "social.topics",
    );
    code(
        refused(|f| f["topics"][2]["max_age"] = json!(131)),
        "social.topics",
    );
    code(
        refused(|f| f["topics"][2]["min_age"] = json!(60)),
        "social.topics",
    );
    code(
        refused(|f| f["topics"][1]["condition"] = json!({"affection_from": 1001})),
        "social.topics",
    );
    code(
        refused(|f| f["topics"][0]["steps"][0]["delta"] = json!(1001)),
        "social.topics",
    );
    code(
        refused(|f| f["topics"][0]["furniture"] = json!(["core:sofa", "core:sofa"])),
        "social.topics",
    );
    code(
        refused(|f| f["choice"]["company"] = json!(0)),
        "social.choice",
    );
    code(
        refused(|f| f["choice"]["avoid"] = json!(100_000_001)),
        "social.choice",
    );
    code(
        refused(|f| f["choice"]["avoid_below_trust"] = json!(1001)),
        "social.choice",
    );
    code(
        refused(|f| f["beliefs"]["heard_cap"] = json!(1001)),
        "social.beliefs",
    );
    code(
        refused(|f| f["beliefs"]["heard_divisor"] = json!(0)),
        "social.beliefs",
    );
    code(
        refused(|f| f["beliefs"]["lie_ppm"] = json!(1_000_001)),
        "social.beliefs",
    );
    code(
        refused(|f| f["beliefs"]["refute_trust"] = json!(-1001)),
        "social.beliefs",
    );
    code(
        refused(|f| f["beliefs"]["deeds"] = json!([])),
        "social.beliefs",
    );
    code(
        refused(|f| f["beliefs"]["deeds"] = json!(["core:lies", "core:lies"])),
        "social.beliefs",
    );
    // The feelings: the reader first (a field out of place or of the wrong type), then the core.
    code(
        refused(|f| f["feelings"]["extra"] = json!(1)),
        "unknown field",
    );
    code(
        refused(|f| drop(f["feelings"].as_object_mut().expect("o").remove("band"))),
        "missing field",
    );
    code(
        refused(|f| f["feelings"]["table"][0]["occasion"] = json!("quarrel")),
        "unknown variant",
    );
    code(
        refused(|f| f["feelings"]["table"][0]["hope"] = json!(1)),
        "unknown field",
    );
    code(
        refused(|f| f["feelings"]["rates"]["joy"] = json!(10_001)),
        "social.feelings",
    );
    code(
        refused(|f| f["feelings"]["rates"]["fear"] = json!(10_001)),
        "social.feelings",
    );
    code(
        refused(|f| f["feelings"]["mood_return"] = json!(10_001)),
        "social.feelings",
    );
    code(
        refused(|f| f["feelings"]["table"][1]["occasion"] = json!("talk_everyday")),
        "social.feelings",
    );
    code(
        refused(|f| f["feelings"]["table"][0]["joy"] = json!(1001)),
        "social.feelings",
    );
    code(
        refused(|f| f["feelings"]["band"]["min_affection"] = json!(1001)),
        "social.feelings",
    );
    code(
        refused(|f| f["feelings"]["band"]["denominator"] = json!(0)),
        "social.feelings",
    );
    code(
        refused(|f| f["feelings"]["contagion"]["step"] = json!(1001)),
        "social.feelings",
    );
    code(
        refused(|f| f["feelings"]["contagion"]["cap"] = json!(700)),
        "social.feelings",
    );
}

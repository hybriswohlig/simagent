//! The genetics card of a run (ADR-0021 section 5, Grok G2 to G5, L1 to L5) equals the one
//! `tools/golden_reference.py` computes from its own model: of every person of the dummy run
//! (`tests/golden/dummy_card.json`) and of two persons of each case of parents in the run of
//! the town (`tests/golden/town_card.json`: pool persons with no, one and two parents in the
//! run, an archived parent, town children of two pool persons, of mixed parents and of two
//! town persons), each at its second shown; the parts add up to the phenotype (genetics
//! specification 4.2). Every code, flag, whole number and the shape exactly; the parts and
//! `z` within 1e-12 of the larger of 1 and the golden's magnitude: the reference rounds
//! `exp`, `ln` and `cos` correctly, the pinned libm may differ in the last bit (class P of
//! the known answers, ADR-0003), and the card is no state. The reference refuses a `z`
//! within 1e-9 percentile points of a threshold, so the sentences cannot differ.

use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};
use sim_core::ids::SimId;
use sim_core::presentation::card::Parts;
use sim_core::world::World;

const DUMMY_CARD: &str = include_str!("../../../tests/golden/dummy_card.json");
const TOWN_CARD: &str = include_str!("../../../tests/golden/town_card.json");

/// `ours` and `golden` are the same JSON, their fractional numbers within 1e-12 of the
/// larger of 1 and the golden's magnitude.
fn close(ours: &Value, golden: &Value) -> bool {
    match (ours, golden) {
        (Value::Number(a), Value::Number(b)) if a.is_f64() || b.is_f64() => {
            let (a, b) = (a.as_f64().expect("a number"), b.as_f64().expect("a number"));
            let scale = if b.abs() > 1.0 { b.abs() } else { 1.0 };
            (a - b).abs() <= 1e-12 * scale
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| close(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, a)| b.get(key).is_some_and(|b| close(a, b)))
        }
        _ => ours == golden,
    }
}

/// The world at the end of `scenario` (a file of `tests/scenarios`), run by the binary.
fn run(scenario: &str) -> World {
    let run = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("card")
        .join(scenario);
    if run.exists() {
        std::fs::remove_dir_all(&run).expect("removes the old run");
    }
    let scenario = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/scenarios")
        .join(scenario);
    let status = Command::new(env!("CARGO_BIN_EXE_simagent"))
        .args(["run".as_ref(), "--scenario".as_ref(), scenario.as_os_str()])
        .args(["--out".as_ref(), run.as_os_str()])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("runs");
    assert!(status.success());
    let bytes = std::fs::read(run.join("state.save")).expect("the save");
    World::load(&bytes, sim_cli::data::core_data().expect("data"))
        .expect("loads")
        .0
}

/// Every person of `golden` has the card the golden names, and its parts add up to `z`.
fn matches(world: &World, golden: &str, persons: usize) {
    let golden: Value = serde_json::from_str(golden).expect("JSON");
    let golden = golden.as_array().expect("a list");
    assert_eq!(golden.len(), persons);
    for person in golden {
        let n = person["sim"].as_u64().expect("a sim");
        let card = world
            .genetics_card(SimId::try_from(n).expect("an id"))
            .expect("a card");
        let ours = json!({"sim": n, "traits": card.traits});
        assert!(close(&ours, person), "sim {n}: {ours} against {person}");
        for row in &card.traits {
            let sum = match row.parts {
                Parts::Split {
                    parents,
                    passing,
                    childhood,
                    own,
                } => ((parents + passing) + childhood) + own,
                Parts::Inherited {
                    inherited,
                    childhood,
                    own,
                    ..
                } => (inherited + childhood) + own,
            };
            assert!((sum - row.z).abs() < 1e-12, "sim {n}: {sum} {}", row.z);
        }
    }
}

#[test]
fn the_dummy_runs_card_is_the_references() {
    let world = run("dummy.json");
    matches(&world, DUMMY_CARD, 13);
    assert!(
        world
            .genetics_card(SimId::try_from(999).expect("an id"))
            .is_none()
    );
}

#[test]
fn the_towns_card_is_the_references() {
    matches(&run("town.json"), TOWN_CARD, 15);
}

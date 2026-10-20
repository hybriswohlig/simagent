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
use sim_core::time::SimTime;
use sim_core::world::{World, YearOut};

const DUMMY_CARD: &str = include_str!("../../../tests/golden/dummy_card.json");
const TOWN_CARD: &str = include_str!("../../../tests/golden/town_card.json");
const HUNGER_HEIGHTS: &str = include_str!("../../../tests/golden/card_hunger.json");
const DISEASES: &str = include_str!("../../../tests/golden/card_diseases.json");

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
    run_until(scenario, None)
}

/// The world of `scenario` at `until` (its own `until` if `None`), run by the binary.
fn run_until(scenario: &str, until: Option<i64>) -> World {
    let name = until.map_or_else(|| scenario.to_owned(), |t| format!("{scenario}@{t}"));
    let run = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("card")
        .join(name);
    if run.exists() {
        std::fs::remove_dir_all(&run).expect("removes the old run");
    }
    let scenario = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/scenarios")
        .join(scenario);
    let mut command = Command::new(env!("CARGO_BIN_EXE_simagent"));
    command
        .args(["run".as_ref(), "--scenario".as_ref(), scenario.as_os_str()])
        .args(["--out".as_ref(), run.as_os_str()]);
    if let Some(t) = until {
        command.args(["--until".to_owned(), t.to_string()]);
    }
    let status = command
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

/// The height lines (Grok L5, L6) of every person of the hunger run (the dummy with hunger in
/// years 2 and 3) equal the reference's (`tests/golden/card_hunger.json`) in the middle of
/// year 2, with year 3 still ahead, and at the end: the loss from the hunger years up to the
/// second shown, the loss of age, the mean of both parents' target heights.
#[test]
fn the_hunger_runs_heights_are_the_references() {
    let golden: Value = serde_json::from_str(HUNGER_HEIGHTS).expect("JSON");
    let snapshots = golden.as_array().expect("a list");
    assert_eq!(snapshots.len(), 2);
    for snapshot in snapshots {
        let t = snapshot["t"].as_i64().expect("a second");
        let world = run_until("card_hunger.json", Some(t));
        assert_eq!(world.closed_through().as_secs(), t);
        for person in snapshot["persons"].as_array().expect("persons") {
            let n = person["sim"].as_u64().expect("a sim");
            let card = world
                .genetics_card(SimId::try_from(n).expect("an id"))
                .expect("a card");
            let height = card.height.expect("height lines");
            let ours = json!({
                "sim": n,
                "hunger_cm": height.hunger_cm,
                "age_cm": height.age_cm,
                "parents_target_cm": height.parents_target_cm,
            });
            assert!(close(&ours, person), "{t} sim {n}: {ours} against {person}");
        }
    }
}

/// The dummy run in this process to `until`, and its forks with the override "hereditary
/// diseases on" stamped `T_5 − 1 s` (as `tests/golden.rs` builds the golden fork) and
/// `T_7 − 1 s`.
fn dummy_and_forks() -> (World, World, World) {
    let scenario: sim_cli::files::Scenario =
        sim_cli::files::parse_strict(include_str!("../../../tests/scenarios/dummy.json"))
            .expect("the scenario parses");
    let started = || {
        let mut world = World::new(sim_cli::data::core_data().expect("data"));
        world
            .submit(sim_core::input::Input::Start(scenario.start.clone()))
            .expect("accepted");
        world
    };
    let mut run = started();
    run.advance_until(&mut YearOut::default(), scenario.until, &mut |_| false);
    let fork = |year: i64| {
        let mut fork = started();
        fork.advance_until(
            &mut YearOut::default(),
            SimTime::from_secs(year * 31_449_600 - 1),
            &mut |_| false,
        );
        let block = sim_core::input::ForkOverride {
            changes: vec![sim_core::params::ParamChange::HereditaryDiseases(true)],
        };
        fork.submit(sim_core::input::Input::ForkOverride(block))
            .expect("accepted");
        fork.advance_until(&mut YearOut::default(), scenario.until, &mut |_| false);
        fork
    };
    (run, fork(5), fork(7))
}

/// The disease lines (Grok G4, L7, M1, M4) of the dummy run (switch off), its forks (on from
/// year 5, and from year 7, after sim 3 left with the switch off) and the town equal the
/// reference's (`tests/golden/card_diseases.json`) exactly: copies, statuses, years and ages
/// are whole values.
#[test]
fn the_disease_lines_are_the_references() {
    let golden: Value = serde_json::from_str(DISEASES).expect("JSON");
    let (dummy, fork, fork_7) = dummy_and_forks();
    let runs = golden.as_array().expect("a list");
    assert_eq!(runs.len(), 4);
    // The town to the golden's second, in its own directory (another test runs it to its end).
    let town = run_until("town.json", runs[3]["t"].as_i64());
    for snapshot in runs {
        let world = match snapshot["run"].as_str().expect("a name") {
            "dummy" => &dummy,
            "dummy_fork" => &fork,
            "dummy_fork_7" => &fork_7,
            "town" => &town,
            other => panic!("unknown run {other}"),
        };
        assert_eq!(
            Some(world.closed_through().as_secs()),
            snapshot["t"].as_i64()
        );
        for person in snapshot["persons"].as_array().expect("persons") {
            let n = person["sim"].as_u64().expect("a sim");
            let card = world
                .genetics_card(SimId::try_from(n).expect("an id"))
                .expect("a card");
            let ours =
                json!({"sim": n, "diseases_off": card.diseases_off, "diseases": card.diseases});
            assert_eq!(&ours, person, "{} sim {n}", snapshot["run"]);
        }
    }
}

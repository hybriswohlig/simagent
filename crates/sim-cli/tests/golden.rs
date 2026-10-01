//! Goldens (ADR-0003 6.3): one set for every target and both profiles. The scenario
//! `tests/scenarios/dummy.json` must give the year hashes, the log hash and the save in
//! `tests/golden/`, in this process, in two fresh processes, and after loading the
//! golden save. A difference between targets, between debug and release or between
//! two runs is a determinism bug that is searched, never blessed.
//!
//! The goldens of this scenario are computed by `tools/golden_reference.py`,
//! independently of the Rust code.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;
use sim_cli::files::{self, Manifest, Scenario};
use sim_core::canonical::YearHash;
use sim_core::input::Input;
use sim_core::log::{LogEntry, LogPayload, log_hash};
use sim_core::time::{EventClass, EventKind, Payload, RunStats, SimTime};
use sim_core::version::{CORE_VERSION, EMPTY_DATA_HASH};
use sim_core::world::{EventPayload, World, YearOut};

const SCENARIO: &str = include_str!("../../../tests/scenarios/dummy.json");
const GOLDEN: &str = include_str!("../../../tests/golden/dummy.json");
const GOLDEN_SAVE: &[u8] = include_bytes!("../../../tests/golden/dummy.save");
/// Day 100 of year 3, where the golden save was taken.
const SAVE_AT: i64 = 3 * 31_449_600 + 100 * 86_400;

/// `tests/golden/<scenario>.json` (ADR-0003 6.3).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Golden {
    core_version: u32,
    #[serde(with = "sim_core::hex_u64")]
    data_hash: u64,
    year_hashes: Vec<YearHash>,
    #[serde(with = "sim_core::hex_u64")]
    log_hash: u64,
}

fn golden() -> Golden {
    serde_json::from_str(GOLDEN).expect("the golden parses")
}

fn scenario() -> Scenario {
    files::parse_strict(SCENARIO).expect("the scenario parses")
}

fn started() -> World {
    let mut world = World::new();
    world
        .submit(Input::Start(scenario().start))
        .expect("accepted");
    world
}

struct Run {
    world: World,
    out: YearOut,
    log: Vec<LogEntry>,
}

fn run_to(mut world: World, mut out: YearOut, mut log: Vec<LogEntry>, until: SimTime) -> Run {
    log.extend(world.take_log());
    world.advance_until(&mut out, until, &mut |_| false);
    log.extend(world.take_log());
    Run { world, out, log }
}

fn whole_run() -> Run {
    run_to(started(), YearOut::default(), Vec::new(), scenario().until)
}

#[test]
fn golden_run_matches() {
    let golden = golden();
    assert_eq!(golden.core_version, CORE_VERSION);
    assert_eq!(golden.data_hash, EMPTY_DATA_HASH);
    let run = whole_run();
    assert_eq!(run.out.year_hashes, golden.year_hashes);
    assert_eq!(log_hash(&run.log), golden.log_hash);
    assert_eq!(golden.year_hashes.len(), 10);
}

/// ADR-0003 6.3: the golden save is what this code writes at that point, and loading
/// it and running on gives the golden hashes.
#[test]
fn golden_save_is_reproduced_and_continues() {
    let golden = golden();
    let part = run_to(
        started(),
        YearOut::default(),
        Vec::new(),
        SimTime::from_secs(SAVE_AT),
    );
    let saved = part.world.save(&part.out.year_hashes).expect("saves");
    assert!(
        saved == GOLDEN_SAVE,
        "the save differs from tests/golden/dummy.save"
    );

    let (loaded, years) = World::load(GOLDEN_SAVE, EMPTY_DATA_HASH).expect("the golden loads");
    assert_eq!(years, golden.year_hashes[..3]);
    let resumed = YearOut {
        census: Vec::new(),
        year_hashes: years,
    };
    let rest = run_to(loaded, resumed, part.log, scenario().until);
    assert_eq!(rest.out.year_hashes, golden.year_hashes);
    assert_eq!(log_hash(&rest.log), golden.log_hash);
    assert_eq!(rest.world.state_hash(), whole_run().world.state_hash());
}

fn repo_file(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

/// B2, ADR-0003 6.2: two runs in two fresh processes agree with each other and with
/// the golden.
#[test]
fn golden_run_matches_in_two_processes() {
    let golden = golden();
    let base = Path::new(env!("CARGO_TARGET_TMPDIR")).join("golden");
    if base.exists() {
        fs::remove_dir_all(&base).expect("removes the old directory");
    }
    let scenario = repo_file("tests/scenarios/dummy.json");
    let dirs = [base.join("first"), base.join("second")];
    for dir in &dirs {
        let status = Command::new(env!("CARGO_BIN_EXE_simagent"))
            .args(["run", "--scenario"])
            .arg(&scenario)
            .arg("--out")
            .arg(dir)
            .status()
            .expect("runs simagent");
        assert!(status.success());
        let text = fs::read_to_string(dir.join(files::MANIFEST)).expect("reads");
        let manifest: Manifest = files::parse_strict(text.trim_end()).expect("a manifest");
        assert_eq!(manifest.year_hashes, golden.year_hashes);
        assert_eq!(manifest.log_hash, golden.log_hash);
        assert_eq!(manifest.core_version, golden.core_version);
        assert_eq!(manifest.data_hash, golden.data_hash);
    }
    for file in [
        files::MANIFEST,
        files::SCENARIO,
        files::INPUT_LOG,
        files::EVENTS,
        files::CENSUS,
        files::SAVE,
    ] {
        let read = |dir: &PathBuf| fs::read(dir.join(file)).expect("reads");
        assert!(
            read(&dirs[0]) == read(&dirs[1]),
            "{file} differs between two runs"
        );
    }
}

/// Every event payload and every log payload, by name. A new variant fails to compile
/// in the match until it has a line here.
fn event_kinds() -> Vec<(&'static str, EventClass, EventKind)> {
    let sim = sim_core::ids::SimId::try_from(1).expect("valid");
    let all = [
        ("year_end", EventPayload::YearEnd),
        ("birthday", EventPayload::Birthday { sim }),
    ];
    all.into_iter()
        .map(|(name, payload)| {
            match payload {
                EventPayload::YearEnd | EventPayload::Birthday { .. } => {}
            }
            (name, payload.class(), payload.kind())
        })
        .collect()
}

fn log_kind(payload: &LogPayload) -> &'static str {
    match payload {
        LogPayload::Started { .. } => "core:started",
        LogPayload::FounderAdded { .. } => "core:founder_added",
        LogPayload::Birthday { .. } => "core:birthday",
    }
}

const LOG_KINDS: [&str; 3] = ["core:started", "core:founder_added", "core:birthday"];

/// The kinds that a run did not exercise (ADR-0003 6.3).
fn uncovered(stats: &RunStats, log: &[LogEntry]) -> Vec<String> {
    let mut missing = Vec::new();
    for (name, class, kind) in event_kinds() {
        if stats.events_of(class, kind) == 0 {
            missing.push(format!("event {name}"));
        }
    }
    for kind in LOG_KINDS {
        if !log.iter().any(|entry| log_kind(&entry.payload) == kind) {
            missing.push(format!("log {kind}"));
        }
    }
    missing
}

/// ADR-0003 6.3: in the golden scenarios every event kind runs and every log kind
/// appears at least once.
#[test]
fn golden_scenarios_cover_every_kind() {
    let run = whole_run();
    assert_eq!(uncovered(run.world.stats(), &run.log), Vec::<String>::new());
}

/// The canary of the coverage check (ADR-0003 K10): a scenario without founders misses
/// the birthday event and two log kinds, and the check says so.
#[test]
fn the_coverage_check_reports_missing_kinds() {
    let mut empty = scenario();
    empty.start.founders.clear();
    let mut world = World::new();
    world.submit(Input::Start(empty.start)).expect("accepted");
    let run = run_to(world, YearOut::default(), Vec::new(), empty.until);
    assert_eq!(
        uncovered(run.world.stats(), &run.log),
        [
            "event birthday",
            "log core:founder_added",
            "log core:birthday"
        ]
    );
}

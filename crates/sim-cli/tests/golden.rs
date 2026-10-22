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
use std::collections::BTreeMap;

use sim_core::canonical::YearHash;
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::finite::FiniteF64;
use sim_core::input::{ForkOverride, Input, PoolStart, Start};
use sim_core::log::{LogEntry, LogPayload, log_hash};
use sim_core::params::{ParamChange, WorldParams};
use sim_core::time::{EventClass, EventKind, Payload, RunStats, SimTime};
use sim_core::version::CORE_VERSION;

/// The data hash of the repository's data, the definitions with the pool and its
/// frequencies, from `tools/data_reference.py` (ADR-0007).
const DATA_HASH: u64 = 0x63e4_b3ea_3dcc_1fe6;
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
    let mut world = World::new(sim_cli::data::core_data().expect("data"));
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

/// The year of the fork point of the golden fork, and the fork: the dummy run with the
/// override "hereditary diseases on", submitted with the stamp `T_5 - 1 s` (ADR-0001
/// section 6), and a hunger of one year at the same stamp, which hits year 5 (ADR-0023).
const FORK_YEAR: usize = 5;

fn fork_run() -> Run {
    let fork_stamp = SimTime::from_secs(5 * 31_449_600 - 1);
    let mut part = run_to(started(), YearOut::default(), Vec::new(), fork_stamp);
    let block = ForkOverride {
        changes: vec![ParamChange::HereditaryDiseases(true)],
    };
    let stamp = part
        .world
        .submit(Input::ForkOverride(block))
        .expect("accepted");
    assert_eq!(stamp.t, fork_stamp);
    // A hunger of one year at the same stamp (ADR-0023, addendum before S1b): year 5.
    let shock = sim_core::input::TriggerShock {
        shock: sim_core::data_id::DataId::new("core:hunger_one_year").expect("valid"),
    };
    part.world
        .submit(Input::TriggerShock(shock))
        .expect("accepted");
    run_to(part.world, part.out, part.log, scenario().until)
}

/// The golden scenario of the arrival (`tests/golden/arrival.json`, from
/// `tools/arrival_reference.py`, ADR-0020 section 5): the pool `pool_c` on the documents it
/// was extracted from, those of the pool run `father_dies` without `pool_frequencies`
/// (`tests/golden/father_dies`, whose data hash is the pool's `frozen_hash`, design note of
/// the pool's records, section 6), a town founded from the pool with the seed 0xff,
/// `n_start` 50 and one immigrant household a year, the hereditary diseases on, to the end
/// of year 13. Its scenario file comes with the scenario format 5 of the CLI; until then the
/// start is built here as the design note draws it: the households of occasion 0 until they
/// bring 50 persons.
const ARRIVAL_GOLDEN: &str = include_str!("../../../tests/golden/arrival.json");
const ARRIVAL_END: i64 = 14 * 31_449_600 - 1;

/// The frequencies `pool_c` was made with (`tests/golden/pool/pool_frequencies.json`, its
/// wrapped form, each `value`), as `Data::with_pool` takes them.
fn pool_frequencies() -> BTreeMap<DataId, FiniteF64> {
    let file: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/golden/pool/pool_frequencies.json"
    ))
    .expect("json");
    file["pool_frequencies"]
        .as_object()
        .expect("the entries")
        .iter()
        .map(|(id, entry)| {
            let q = entry["value"].as_f64().expect("a value");
            (
                DataId::new(id).expect("id"),
                FiniteF64::try_new(q).expect("finite"),
            )
        })
        .collect()
}

fn arrival_data() -> Data {
    let definitions = sim_cli::data::definitions_from(
        include_str!("../../../tests/golden/father_dies/calibration/demography.json"),
        include_str!("../../../tests/golden/father_dies/calibration/genetics.json"),
        include_str!("../../../tests/golden/father_dies/genetics/genome_map.json"),
    )
    .expect("definitions");
    Data::with_pool(
        definitions,
        include_bytes!("../../../tests/golden/pool/pool_c.bin"),
        pool_frequencies(),
    )
    .expect("checked")
}

/// The `Start` of the arrival: a town of 50 from `pool_c` with immigration, its pool drawn.
fn arrival_start(data: &Data) -> Start {
    Start {
        world_seed: 0xff,
        founder_tag: DataId::new("test:arrival").expect("id"),
        world_params: WorldParams {
            hereditary_diseases: true,
            nutrition: Vec::new(),
        },
        founders: Vec::new(),
        couples: Vec::new(),
        capacity: None,
        pool: PoolStart::drawn(data, 0xff, 50, true),
    }
}

fn arrival_run() -> Run {
    let data = arrival_data();
    let start = arrival_start(&data);
    let mut world = World::new(data);
    world.submit(Input::Start(start)).expect("accepted");
    run_to(
        world,
        YearOut::default(),
        Vec::new(),
        SimTime::from_secs(ARRIVAL_END),
    )
}

/// The CLI resolves a scenario file's pool on the data it is given: the households of
/// `PoolStart::drawn`, every other field as written (design note of the map-4 switch,
/// section 13). The `Start` it gives is the arrival's, which the golden pins.
#[test]
fn the_cli_resolves_a_pool_scenario_to_the_arrivals_start() {
    let text = format!(
        r#"{{"format_version":5,"start":{{"world_seed":"00000000000000ff","founder_tag":"test:arrival","world_params":{{"hereditary_diseases":true,"nutrition":[]}},"founders":[],"couples":[],"capacity":null,"pool":{{"n_start":50,"immigration":true}}}},"until":{ARRIVAL_END}}}"#
    );
    let file: sim_cli::files::ScenarioFile = serde_json::from_str(&text).expect("a scenario");
    let resolved = sim_cli::resolve(file, || Ok(arrival_data())).expect("resolved");
    assert_eq!(resolved.format_version, 5);
    assert_eq!(resolved.until, SimTime::from_secs(ARRIVAL_END));
    assert_eq!(resolved.start, arrival_start(&arrival_data()));
    assert!(
        resolved
            .start
            .pool
            .is_some_and(|pool| !pool.households.is_empty())
    );
}

#[test]
fn golden_arrival_matches() {
    let golden: Golden = serde_json::from_str(ARRIVAL_GOLDEN).expect("the golden parses");
    assert_eq!(golden.core_version, CORE_VERSION);
    assert_eq!(golden.data_hash, arrival_data().hash());
    let run = arrival_run();
    assert_eq!(run.out.year_hashes, golden.year_hashes);
    assert_eq!(log_hash(&run.log), golden.log_hash);
    assert_eq!(golden.year_hashes.len(), 14);
}

/// The golden of the fork (tools/golden_reference.py), and ADR-0001
/// `fork_point_hash_equals_parent` on it: the years before the fork point have the
/// hashes of the parent, every year from the fork point on another one.
#[test]
fn golden_fork_matches() {
    let text = fs::read_to_string(repo_file("tests/golden/dummy_fork.json"))
        .expect("the golden of the fork");
    let golden_fork: Golden = serde_json::from_str(&text).expect("the golden parses");
    assert_eq!(golden_fork.core_version, CORE_VERSION);
    assert_eq!(golden_fork.data_hash, DATA_HASH);
    let fork = fork_run();
    assert_eq!(fork.out.year_hashes, golden_fork.year_hashes);
    assert_eq!(log_hash(&fork.log), golden_fork.log_hash);

    let parent = golden();
    assert_eq!(golden_fork.year_hashes.len(), parent.year_hashes.len());
    assert_eq!(
        golden_fork.year_hashes[..FORK_YEAR],
        parent.year_hashes[..FORK_YEAR]
    );
    for (ours, theirs) in golden_fork.year_hashes[FORK_YEAR..]
        .iter()
        .zip(&parent.year_hashes[FORK_YEAR..])
    {
        assert_eq!(ours.year, theirs.year);
        assert_ne!(ours.hash, theirs.hash, "year {}", ours.year);
    }
    assert_ne!(golden_fork.log_hash, parent.log_hash);
    // The entry of the override. It plans the death of sim 1 of `core:disease_ar` (cystic
    // fibrosis on the repository's data, ADR-0017 section 3) for second 1 506 119 448, at
    // about 78; she dies of senescence in year 26 first (second 836 172 484, at 56.6), as
    // in the run (tools/golden_reference.py). After it come the onsets of sims 1 and 3,
    // reached at their birth and never reported while the switch was off, then the shock's
    // `core:shock_triggered` and its one `core:param_changed` for year 5.
    assert_eq!(fork.log.len(), whole_run().log.len() + 5);
}

/// The pedigrees of the golden run and of its fork after thirty years are the goldens
/// (tools/golden_reference.py, step 10): in the fork, with the hereditary diseases on from
/// year 5, sims 1 and 3 show their disease.
#[test]
fn golden_pedigrees_match() {
    for (run, file) in [
        (whole_run(), "tests/golden/dummy_pedigree.json"),
        (fork_run(), "tests/golden/dummy_fork_pedigree.json"),
    ] {
        let golden = fs::read_to_string(repo_file(file)).expect("the golden pedigree");
        let pedigree = serde_json::to_string(&run.world.pedigree()).expect("json") + "\n";
        assert_eq!(pedigree, golden, "{file}");
    }
}

#[test]
fn golden_run_matches() {
    let golden = golden();
    assert_eq!(golden.core_version, CORE_VERSION);
    assert_eq!(golden.data_hash, DATA_HASH);
    let run = whole_run();
    assert_eq!(run.out.year_hashes, golden.year_hashes);
    assert_eq!(log_hash(&run.log), golden.log_hash);
    assert_eq!(golden.year_hashes.len(), 30);
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

    let (loaded, years) = World::load(GOLDEN_SAVE, sim_cli::data::core_data().expect("data"))
        .expect("the golden loads");
    assert_eq!(years, golden.year_hashes[..3]);
    let resumed = YearOut {
        census: Vec::new(),
        year_hashes: years,
        trace: Vec::new(),
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

/// The town from the pool of the repository's data (`tests/scenarios/town.json`, format 5
/// with `pool` `{n_start 100, immigration true}`) through the binary: the CLI resolves the
/// pool by `PoolStart::drawn` on the built-in data, and the run gives the golden
/// `tests/golden/town.json` of `tools/golden_reference.py` (core version, data hash, the 30
/// year hashes, the log hash). The end-to-end test of the pool resolution.
#[test]
fn golden_town_matches_through_the_binary() {
    let golden: Golden = serde_json::from_str(
        &fs::read_to_string(repo_file("tests/golden/town.json")).expect("the golden of the town"),
    )
    .expect("the golden parses");
    assert_eq!(golden.core_version, CORE_VERSION);
    assert_eq!(golden.data_hash, DATA_HASH);
    assert_eq!(golden.year_hashes.len(), 30);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("golden_town");
    if dir.exists() {
        fs::remove_dir_all(&dir).expect("removes the old directory");
    }
    let status = Command::new(env!("CARGO_BIN_EXE_simagent"))
        .args(["run", "--scenario"])
        .arg(repo_file("tests/scenarios/town.json"))
        .arg("--out")
        .arg(&dir)
        .status()
        .expect("runs simagent");
    assert!(status.success());
    let text = fs::read_to_string(dir.join(files::MANIFEST)).expect("reads");
    let manifest: Manifest = files::parse_strict(text.trim_end()).expect("a manifest");
    assert_eq!(manifest.core_version, golden.core_version);
    assert_eq!(manifest.data_hash, golden.data_hash);
    assert_eq!(manifest.year_hashes, golden.year_hashes);
    assert_eq!(manifest.log_hash, golden.log_hash);
    // The resolved scenario holds the households `PoolStart::drawn` gives on the built-in
    // data for the town's seed, size and immigration.
    let resolved: Scenario = files::parse_strict(
        fs::read_to_string(dir.join(files::SCENARIO))
            .expect("reads")
            .trim_end(),
    )
    .expect("a resolved scenario");
    let data = sim_cli::data::core_data().expect("data");
    let drawn = PoolStart::drawn(&data, 1, 100, true).expect("the data hold a pool");
    assert!(!drawn.households.is_empty());
    assert_eq!(resolved.start.pool, Some(drawn));
    assert_eq!(resolved.start.world_seed, 1);
    assert!(resolved.start.founders.is_empty());
}

/// The CLI's message for a scenario with a pool on data without one: here the map 3
/// fixture, which holds no pool (the built-in data hold one since the switch to map 4).
#[test]
fn a_pool_scenario_on_data_without_a_pool_is_invalid() {
    let text = r#"{"format_version":5,"start":{"world_seed":"00000000000000ff","founder_tag":"test:tiny","world_params":{"hereditary_diseases":false,"nutrition":[]},"founders":[],"couples":[],"capacity":null,"pool":{"n_start":10,"immigration":false}},"until":31449599}"#;
    let file: sim_cli::files::ScenarioFile = serde_json::from_str(text).expect("a scenario");
    let without_pool = || {
        sim_cli::data::data_from(
            include_str!("../../../data/calibration/demography.json"),
            include_str!("../../../tests/golden/map3/calibration/genetics.json"),
            include_str!("../../../tests/golden/map3/genetics/genome_map.json"),
        )
    };
    let error = sim_cli::resolve(file, without_pool).expect_err("no pool to draw from");
    assert_eq!(
        error.to_string(),
        "error: scenario_invalid: the data hold no pool"
    );
}

/// Every event payload by name. The names and codes must be the `kind` lines of
/// `tests/golden/event_codes.txt` (see `the_coverage_lists_match_the_code_registries`),
/// so a kind that is registered there cannot be missing here.
fn event_kinds() -> Vec<(&'static str, EventClass, EventKind)> {
    let sim = sim_core::ids::SimId::try_from(1).expect("valid");
    let all = [
        ("year_end", EventPayload::YearEnd),
        ("birthday", EventPayload::Birthday { sim }),
        ("death", EventPayload::Death { sim, cause: 0 }),
        (
            "conception_attempt",
            EventPayload::ConceptionAttempt { woman: sim },
        ),
        ("birth", EventPayload::Birth { mother: sim }),
        (
            "separation",
            EventPayload::Separation {
                partnership: sim_core::ids::PartnershipId::try_from(1).expect("valid"),
            },
        ),
        ("market_round", EventPayload::MarketRound),
        ("leave_home", EventPayload::LeaveHome { sim }),
        ("emigration", EventPayload::Emigration { capacity: 1 }),
        ("immigration", EventPayload::Immigration { persons: 1 }),
        (
            "disease_onset",
            EventPayload::DiseaseOnset { sim, disease: 0 },
        ),
    ];
    all.into_iter()
        .map(|(name, payload)| {
            match payload {
                EventPayload::YearEnd
                | EventPayload::Birthday { .. }
                | EventPayload::Death { .. }
                | EventPayload::ConceptionAttempt { .. }
                | EventPayload::Birth { .. }
                | EventPayload::Separation { .. }
                | EventPayload::MarketRound
                | EventPayload::LeaveHome { .. }
                | EventPayload::Emigration { .. }
                | EventPayload::Immigration { .. }
                | EventPayload::DiseaseOnset { .. } => {}
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
        LogPayload::ParamChanged { .. } => "core:param_changed",
        LogPayload::Death { .. } => "core:death",
        LogPayload::Partnered { .. } => "core:partnered",
        LogPayload::Conception { .. } => "core:conception",
        LogPayload::Meiosis { .. } => "core:meiosis",
        LogPayload::Birth { .. } => "core:birth",
        LogPayload::Separated { .. } => "core:separated",
        LogPayload::HouseholdFormed { .. } => "core:household_formed",
        LogPayload::Moved { .. } => "core:moved",
        LogPayload::Emigrated { .. } => "core:emigrated",
        LogPayload::Arrived { .. } => "core:arrived",
        LogPayload::DiseaseOnset { .. } => "core:disease_onset",
        LogPayload::ShockTriggered { .. } => "core:shock_triggered",
    }
}

const LOG_KINDS: [&str; 16] = [
    "core:started",
    "core:founder_added",
    "core:birthday",
    "core:param_changed",
    "core:death",
    "core:partnered",
    "core:conception",
    "core:meiosis",
    "core:birth",
    "core:separated",
    "core:household_formed",
    "core:moved",
    "core:emigrated",
    "core:arrived",
    "core:disease_onset",
    "core:shock_triggered",
];

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
/// appears at least once. The scenarios are the dummy run, its fork at year 5 and the
/// arrival; only the arrival has a pool, so only it has immigrants, and only the fork has
/// a shock (ADR-0023, addendum before S1b).
#[test]
fn golden_scenarios_cover_every_kind() {
    let run = whole_run();
    assert_eq!(
        uncovered(run.world.stats(), &run.log),
        [
            "event immigration",
            "event disease_onset",
            "log core:param_changed",
            "log core:arrived",
            "log core:disease_onset",
            "log core:shock_triggered"
        ],
        "the dummy run has no override, no shock and no pool, its switch is off, and its \
         only disease begins at birth"
    );
    let fork = fork_run();
    let arrival = arrival_run();
    let mut log = run.log;
    log.extend(fork.log);
    log.extend(arrival.log);
    let in_neither: Vec<String> = uncovered(fork.world.stats(), &log)
        .into_iter()
        .filter(|kind| uncovered(arrival.world.stats(), &log).contains(kind))
        .collect();
    assert_eq!(in_neither, Vec::<String>::new());
}

/// The two fields behind `prefix` in the lines of a code registry in `tests/golden/`.
fn registered(registry: &str, prefix: &str) -> Vec<(String, String)> {
    registry
        .lines()
        .filter_map(|line| {
            let mut parts = line.split(' ');
            if parts.next() != Some(prefix) {
                return None;
            }
            Some((parts.next()?.to_owned(), parts.next()?.to_owned()))
        })
        .collect()
}

/// The lists the coverage check requires are exactly the registries of codes, which
/// their own golden tests pin to the enums: every registered event kind and every
/// registered log kind must occur in a golden scenario.
#[test]
fn the_coverage_lists_match_the_code_registries() {
    // `kind <name> <code>`
    let event_codes = include_str!("../../../tests/golden/event_codes.txt");
    let kinds: Vec<(String, String)> = event_kinds()
        .into_iter()
        .map(|(name, _, kind)| (name.to_owned(), kind.0.to_string()))
        .collect();
    assert_eq!(kinds, registered(event_codes, "kind"));
    assert_eq!(kinds.len(), 11);
    // `payload <index> <name>`
    let log_codes = include_str!("../../../tests/golden/log_codes.txt");
    let logs: Vec<String> = registered(log_codes, "payload")
        .into_iter()
        .map(|(_, name)| name)
        .collect();
    assert_eq!(logs, LOG_KINDS);
    assert_eq!(logs.len(), 16);
}

/// Each required kind is really required: a run that lacks exactly one of them is
/// reported with exactly that one.
#[test]
fn every_required_kind_is_checked() {
    let run = fork_run();
    let mut log = run.log.clone();
    log.extend(arrival_run().log);
    for missing in LOG_KINDS {
        let without: Vec<LogEntry> = log
            .iter()
            .filter(|entry| log_kind(&entry.payload) != missing)
            .cloned()
            .collect();
        // The fork has no pool: its own events never include the immigration; its only
        // disease begins at birth, so it plans no onset.
        assert_eq!(
            uncovered(run.world.stats(), &without),
            [
                "event immigration".to_owned(),
                "event disease_onset".to_owned(),
                format!("log {missing}")
            ]
        );
    }
    // A world that never ran: no event fired, every event kind is reported.
    let idle = World::new(sim_cli::data::core_data().expect("data"));
    let events: Vec<String> = event_kinds()
        .into_iter()
        .map(|(name, _, _)| format!("event {name}"))
        .collect();
    assert_eq!(uncovered(idle.stats(), &log), events);
}

/// The canary of the coverage check (ADR-0003 K10): a scenario without founders, and so
/// without couples, misses the events and entries of sims, and the check says so.
#[test]
fn the_coverage_check_reports_missing_kinds() {
    let mut empty = scenario();
    empty.start.founders.clear();
    empty.start.couples.clear();
    let mut world = World::new(sim_cli::data::core_data().expect("data"));
    world.submit(Input::Start(empty.start)).expect("accepted");
    let run = run_to(world, YearOut::default(), Vec::new(), empty.until);
    assert_eq!(
        uncovered(run.world.stats(), &run.log),
        [
            "event birthday",
            "event death",
            "event conception_attempt",
            "event birth",
            "event separation",
            "event leave_home",
            "event immigration",
            "event disease_onset",
            "log core:founder_added",
            "log core:birthday",
            "log core:param_changed",
            "log core:death",
            "log core:partnered",
            "log core:conception",
            "log core:meiosis",
            "log core:birth",
            "log core:separated",
            "log core:household_formed",
            "log core:moved",
            "log core:emigrated",
            "log core:arrived",
            "log core:disease_onset",
            "log core:shock_triggered"
        ]
    );
}

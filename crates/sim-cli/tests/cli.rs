//! The command line runner as a process (ADR-0004 sections 4, 6 and 8, acceptance
//! tests 1, 2 and 4; ADR-0003 acceptance tests of `sim-cli`).
//!
//! The pinned contents of the run directory of scenario "tiny" (seed 0xff, one founder
//! born one second before the start, one year) come from the ADRs and from
//! `tools/state_reference.py`, not from a run of the program.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use serde_json::Value;
use sim_core::input::Input;
use sim_core::log::LogHasher;
use sim_core::time::SimTime;
use sim_core::world::{World, YearOut};

const Y: i64 = 31_449_600;

const TINY_SCENARIO: &str = r#"{"format_version":1,"start":{"world_seed":"00000000000000ff","founder_tag":"test:tiny","founders":[{"born":-1}]},"until":31449599}"#;

/// Three founders, ten years.
const TEN_YEARS: &str = r#"{
  "format_version": 1,
  "start": {
    "world_seed": "0000000000000001",
    "founder_tag": "test:cli",
    "founders": [{"born": -943488000}, {"born": -794880000}, {"born": -1}]
  },
  "until": 314495999
}"#;

const FILES: [&str; 6] = [
    "manifest.json",
    "scenario.resolved.json",
    "input.ndjson",
    "events.ndjson",
    "census.csv",
    "state.save",
];

/// A fresh directory for one test under cargo's temporary directory.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("cli")
        .join(name);
    if dir.exists() {
        fs::remove_dir_all(&dir).expect("removes the old scratch directory");
    }
    fs::create_dir_all(&dir).expect("creates the scratch directory");
    dir
}

fn text(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn arg(path: &Path) -> &str {
    path.to_str().expect("a UTF-8 path")
}

struct Outcome {
    ok: bool,
    stdout: String,
    stderr: String,
}

fn simagent(args: &[&str]) -> Outcome {
    let output = Command::new(env!("CARGO_BIN_EXE_simagent"))
        .args(args)
        .output()
        .expect("runs simagent");
    Outcome {
        ok: output.status.success(),
        stdout: String::from_utf8(output.stdout).expect("UTF-8"),
        stderr: String::from_utf8(output.stderr).expect("UTF-8"),
    }
}

fn must(args: &[&str]) {
    let outcome = simagent(args);
    assert!(
        outcome.ok,
        "simagent {args:?} failed:\n{}\n{}",
        outcome.stdout, outcome.stderr
    );
}

/// The first line of stderr of a failing call: `error: <code>` and details.
fn error_of(args: &[&str]) -> String {
    let outcome = simagent(args);
    assert!(!outcome.ok, "simagent {args:?} should fail");
    outcome.stderr.lines().next().unwrap_or("").to_owned()
}

fn run_scenario(dir: &Path, scenario: &str, name: &str) -> PathBuf {
    let file = dir.join(format!("{name}.scenario.json"));
    fs::write(&file, scenario).expect("writes the scenario");
    let out = dir.join(name);
    must(&["run", "--scenario", arg(&file), "--out", arg(&out)]);
    out
}

fn assert_same_run(left: &Path, right: &Path) {
    for file in FILES {
        let (a, b) = (left.join(file), right.join(file));
        assert!(
            fs::read(&a).expect("reads") == fs::read(&b).expect("reads"),
            "{file} differs between {} and {}",
            left.display(),
            right.display()
        );
    }
}

/// ADR-0004 sections 4 and 8: the run directory of "tiny", file by file.
#[test]
fn run_writes_the_run_directory() {
    let dir = scratch("run_directory");
    let out = run_scenario(&dir, TINY_SCENARIO, "tiny");
    assert_eq!(
        text(&out.join("scenario.resolved.json")),
        format!("{TINY_SCENARIO}\n")
    );
    assert_eq!(
        text(&out.join("input.ndjson")),
        concat!(
            r#"{"format_version":1,"core_version":1,"data_hash":"2d06800538d394c2","build_commit":"unknown"}"#,
            "\n",
            r#"{"t":-1,"seq":1,"input":{"start":{"world_seed":"00000000000000ff","founder_tag":"test:tiny","founders":[{"born":-1}]}}}"#,
            "\n",
            r#"{"end":31449599}"#,
            "\n",
        )
    );
    assert_eq!(
        text(&out.join("events.ndjson")),
        concat!(
            r#"{"id":1,"t":0,"subjects":[],"causes":[{"input":1}],"payload":{"core:started":{"founders":1}}}"#,
            "\n",
            r#"{"id":2,"t":0,"subjects":[{"sim":1}],"causes":[{"input":1}],"payload":{"core:founder_added":{"born":-1}}}"#,
            "\n",
            r#"{"id":3,"t":31449599,"subjects":[{"sim":1}],"causes":[{"plan":{"by":2,"planned_at":0}}],"payload":{"core:birthday":{"age_years":1}}}"#,
            "\n",
        )
    );
    assert_eq!(
        text(&out.join("census.csv")),
        "year,living,lived_secs\n0,1,31449600\n"
    );
    // Year hash and log hash from tools/state_reference.py.
    assert_eq!(
        text(&out.join("manifest.json")),
        concat!(
            r#"{"format_version":1,"core_version":1,"data_hash":"2d06800538d394c2","build_commit":"unknown","world_seed":"00000000000000ff","closed_through":31449599,"year_hashes":[{"year":0,"hash":"6710517d804b1d20"}],"log_hash":"3233ead4da597110"}"#,
            "\n",
        )
    );
    let save: String = fs::read(out.join("state.save"))
        .expect("reads")
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_eq!(
        save,
        "53494d4147454e540101c2a9cec6d380a0832d0101ff01fe87ff1d0202fe8ffe3b0003010100fe8ffe3b000100018088ff1d04010101000203fe87ff1d01000401010100ff0109746573743a74696e7901010100a0baac82d8af948867"
    );
    let mut entries: Vec<_> = fs::read_dir(&out)
        .expect("lists")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .into_string()
                .expect("UTF-8")
        })
        .collect();
    entries.sort();
    let mut expected: Vec<String> = FILES.iter().map(|name| (*name).to_owned()).collect();
    expected.sort();
    assert_eq!(entries, expected);
}

/// ADR-0004 test 1: a replay only from `input.ndjson` gives the same year hashes and
/// the same log hash, here the same run directory byte for byte.
#[test]
fn replay_from_input_log_matches_scenario_run() {
    let dir = scratch("replay");
    let original = run_scenario(&dir, TEN_YEARS, "original");
    let replayed = dir.join("replayed");
    let log = original.join("input.ndjson");
    let manifest = original.join("manifest.json");
    must(&[
        "run",
        "--replay",
        arg(&log),
        "--manifest",
        arg(&manifest),
        "--out",
        arg(&replayed),
    ]);
    assert_same_run(&original, &replayed);
    let manifest: Value = serde_json::from_str(&text(&manifest)).expect("json");
    assert_eq!(manifest["year_hashes"].as_array().expect("array").len(), 10);
    assert_eq!(text(&original.join("events.ndjson")).lines().count(), 34);
    // Without the manifest the replay runs as well; it only has nothing to compare with.
    let unchecked = dir.join("unchecked");
    must(&["run", "--replay", arg(&log), "--out", arg(&unchecked)]);
    assert_same_run(&original, &unchecked);
}

/// Every `Input` variant as the command schema writes it. A new variant fails to
/// compile here until it has a line.
fn commands() -> Vec<(Input, &'static str)> {
    let start = r#"{"start":{"world_seed":"0000000000000001","founder_tag":"test:cli","founders":[{"born":-943488000},{"born":-794880000},{"born":-1}]}}"#;
    let commands = vec![(serde_json::from_str::<Input>(start).expect("parses"), start)];
    for (input, _) in &commands {
        match input {
            Input::Start(_) => {}
        }
    }
    commands
}

/// ADR-0004 test 2: the same inputs directly through `submit` and as JSON through the
/// command schema of the CLI give the same hashes.
#[test]
fn command_schema_matches_direct_call() {
    let end = SimTime::from_secs(10 * Y - 1);
    let mut world = World::new();
    let mut hasher = LogHasher::new();
    let mut out = YearOut::default();
    for (input, _) in commands() {
        world.submit(input).expect("accepted");
        hasher.update(&world.take_log());
    }
    world.advance_until(&mut out, end, &mut |_| false);
    hasher.update(&world.take_log());

    let dir = scratch("command_schema");
    let lines: Vec<&str> = commands().iter().map(|(_, line)| *line).collect();
    let start: Value = serde_json::from_str(lines[0]).expect("json");
    let scenario = serde_json::json!({
        "format_version": 1,
        "start": start["start"],
        "until": end.as_secs(),
    });
    let run = run_scenario(&dir, &scenario.to_string(), "run");
    let manifest: Value = serde_json::from_str(&text(&run.join("manifest.json"))).expect("json");
    assert_eq!(
        manifest["year_hashes"],
        serde_json::to_value(&out.year_hashes).expect("json")
    );
    assert_eq!(
        manifest["log_hash"],
        Value::from(format!("{:016x}", hasher.digest()))
    );
    assert_eq!(
        fs::read(run.join("state.save")).expect("reads"),
        world.save(&out.year_hashes).expect("saves")
    );
}

/// ADR-0004 test 4: saving in the middle of a year, loading in a fresh process and
/// running on gives the run directory of the uninterrupted run.
#[test]
fn save_load_continue_in_a_fresh_process() {
    let dir = scratch("resume");
    let whole = run_scenario(&dir, TEN_YEARS, "whole");
    let scenario = dir.join("whole.scenario.json");
    for (name, stops) in [
        ("mid_year", vec![3 * Y + 100 * 86_400]),
        ("year_end", vec![4 * Y - 1]),
        ("first_second", vec![0]),
        ("three_parts", vec![Y / 2, 6 * Y, 9 * Y + 5]),
    ] {
        let parts = dir.join(name);
        let first = stops[0].to_string();
        must(&[
            "run",
            "--scenario",
            arg(&scenario),
            "--out",
            arg(&parts),
            "--until",
            &first,
        ]);
        for stop in &stops[1..] {
            must(&[
                "run",
                "--resume",
                "--out",
                arg(&parts),
                "--until",
                &stop.to_string(),
            ]);
        }
        // Without `--until` a resumed run goes to the end of its scenario.
        must(&["run", "--resume", "--out", arg(&parts)]);
        assert_same_run(&whole, &parts);
    }
}

fn replace_in(path: &Path, from: &str, to: &str) {
    let before = text(path);
    assert!(before.contains(from), "{from:?} not in {}", path.display());
    fs::write(path, before.replacen(from, to, 1)).expect("writes");
}

/// ADR-0003 `replay_rejects_foreign_core_version`, and the data hash likewise.
#[test]
fn replay_rejects_foreign_core_version() {
    let dir = scratch("foreign");
    let original = run_scenario(&dir, TINY_SCENARIO, "tiny");
    let log = original.join("input.ndjson");
    let out = dir.join("out");
    replace_in(&log, r#""core_version":1"#, r#""core_version":2"#);
    assert_eq!(
        error_of(&["run", "--replay", arg(&log), "--out", arg(&out)]),
        "error: replay_foreign_core_version: the input log is of core version 2, this is 1"
    );
    replace_in(&log, r#""core_version":2"#, r#""core_version":1"#);
    replace_in(&log, "2d06800538d394c2", "2d06800538d394c3");
    assert_eq!(
        error_of(&["run", "--replay", arg(&log), "--out", arg(&out)]),
        "error: replay_foreign_data_hash: the input log has data hash 2d06800538d394c3, this is 2d06800538d394c2"
    );
    assert!(!out.exists(), "a rejected replay writes nothing");
}

/// ADR-0003 `replay_reports_first_divergent_year`.
#[test]
fn replay_reports_first_divergent_year() {
    let dir = scratch("diverged");
    let original = run_scenario(&dir, TEN_YEARS, "original");
    let manifest_path = original.join("manifest.json");
    let manifest: Value = serde_json::from_str(&text(&manifest_path)).expect("json");
    let year = |n: usize| {
        manifest["year_hashes"][n]["hash"]
            .as_str()
            .expect("hex")
            .to_owned()
    };
    // Years 3 and 7 differ from what the run produces; the replay names year 3.
    replace_in(&manifest_path, &year(3), "0123456789abcdef");
    replace_in(&manifest_path, &year(7), "fedcba9876543210");
    let out = dir.join("out");
    let log = original.join("input.ndjson");
    assert_eq!(
        error_of(&[
            "run",
            "--replay",
            arg(&log),
            "--manifest",
            arg(&manifest_path),
            "--out",
            arg(&out),
        ]),
        format!(
            "error: replay_diverged: year 3 has hash {}, the manifest says 0123456789abcdef",
            year(3)
        )
    );
}

/// A manifest with another number of years than the replay produces is a divergence
/// too: the run it describes ended elsewhere.
#[test]
fn replay_compares_the_number_of_years() {
    let dir = scratch("years");
    let original = run_scenario(&dir, TEN_YEARS, "original");
    let manifest_path = original.join("manifest.json");
    let manifest: Value = serde_json::from_str(&text(&manifest_path)).expect("json");
    let hash = manifest["year_hashes"][9]["hash"].as_str().expect("hex");
    let last = format!(r#",{{"year":9,"hash":"{hash}"}}"#);
    replace_in(&manifest_path, &last, "");
    let log = original.join("input.ndjson");
    assert_eq!(
        error_of(&[
            "run",
            "--replay",
            arg(&log),
            "--manifest",
            arg(&manifest_path),
            "--out",
            arg(&dir.join("out")),
        ]),
        "error: replay_diverged: the replay has 10 year hashes, the manifest 9"
    );
}

/// A session that advances in several calls writes every census row once. A replay
/// with more than one input does that; until a second input exists, only this test
/// drives a session that way.
#[test]
fn a_session_writes_each_census_row_once() {
    use sim_cli::files::Scenario;
    use sim_cli::session::Session;
    let dir = scratch("session");
    let scenario: Scenario = serde_json::from_str(TINY_SCENARIO).expect("parses");
    let rejected = |code: String| panic!("rejected: {code}");
    let first = Input::Start(scenario.start.clone());
    let mut session =
        Session::start(&dir, SimTime::from_secs(-1), first, None, rejected).expect("starts");
    for limit in [5, Y - 1, Y, 2 * Y - 1, 3 * Y - 1] {
        session
            .advance(SimTime::from_secs(limit))
            .expect("advances");
    }
    session.finish(&scenario).expect("finishes");
    assert_eq!(
        text(&dir.join("census.csv")),
        "year,living,lived_secs\n0,1,31449600\n1,1,31449600\n2,1,31449600\n"
    );
    assert_eq!(text(&dir.join("events.ndjson")).lines().count(), 5);
}

/// ADR-0004 section 4: a replay ends with an error if a stamp differs or an input is
/// rejected.
#[test]
fn replay_rejects_a_changed_input_log() {
    let dir = scratch("changed_log");
    let original = run_scenario(&dir, TINY_SCENARIO, "tiny");
    let log = original.join("input.ndjson");
    let pristine = text(&log);
    // A failed replay may leave what it wrote; each attempt gets its own directory.
    let attempts = std::cell::Cell::new(0);
    let replay = |changed: &str| {
        fs::write(&log, changed).expect("writes");
        attempts.set(attempts.get() + 1);
        let out = dir.join(format!("attempt{}", attempts.get()));
        error_of(&["run", "--replay", arg(&log), "--out", arg(&out)])
    };
    let lines: Vec<&str> = pristine.lines().collect();
    let with_lines = |records: &[&str]| {
        let mut all = vec![lines[0]];
        all.extend_from_slice(records);
        all.push(lines[2]);
        all.join("\n") + "\n"
    };
    assert_eq!(
        replay(&pristine.replacen(r#""seq":1"#, r#""seq":2"#, 1)),
        "error: replay_stamp_mismatch: the log has t -1 seq 2, the replay stamped t -1 seq 1"
    );
    let second_start = lines[1].replacen(r#""seq":1"#, r#""seq":2"#, 1);
    assert_eq!(
        replay(&with_lines(&[lines[1], &second_start])),
        "error: replay_input_rejected: input 2 at t -1 was rejected: already_started"
    );
    // A replay follows the log: it runs to the stamp of a record before it applies it.
    // `Start` after the clock moved is rejected (ADR-0004 section 4).
    assert_eq!(
        replay(&pristine.replacen(r#""t":-1"#, r#""t":5"#, 1)),
        "error: replay_input_rejected: input 1 at t 5 was rejected: clock_not_at_start"
    );
    assert!(!dir.join("attempt3").exists(), "nothing was written");
    // The record as a list of values instead of an object with field names.
    let positional = r#"[-1,1,{"start":["00000000000000ff","test:tiny",[[-1]]]}]"#;
    assert_eq!(
        replay(&with_lines(&[positional])),
        "error: input_log_invalid: line 2 is not a record of the input log"
    );
    let nested_positional = lines[1].replacen(r#"{"born":-1}"#, "[-1]", 1);
    assert_eq!(
        replay(&with_lines(&[&nested_positional])),
        "error: input_log_invalid: line 2 is not a record of the input log"
    );
    let unknown = lines[1].replacen(r#""seq":1"#, r#""seq":1,"extra":0"#, 1);
    assert_eq!(
        replay(&with_lines(&[&unknown])),
        "error: input_log_invalid: line 2 is not a record of the input log"
    );
    assert_eq!(
        replay(&format!("{}\n{}\n", lines[0], lines[1])),
        "error: input_log_invalid: the last line is not {\"end\":t}"
    );
    assert_eq!(
        replay(""),
        "error: input_log_invalid: line 1 is not the header of an input log"
    );
    assert_eq!(
        replay(&pristine.replacen(r#""format_version":1"#, r#""format_version":2"#, 1)),
        "error: input_log_invalid: line 1 is not the header of an input log"
    );
    assert_eq!(
        replay(&pristine.replacen(r#"{"end":31449599}"#, r#"{"end":-5}"#, 1)),
        "error: input_log_invalid: the end -5 lies before the last input at -1"
    );
    // The pristine log still replays.
    fs::write(&log, &pristine).expect("writes");
    let out = dir.join("out");
    must(&["run", "--replay", arg(&log), "--out", arg(&out)]);
    assert_same_run(&original, &out);
}

#[test]
fn bad_scenarios_and_directories_are_rejected() {
    let dir = scratch("rejected");
    let out = dir.join("out");
    let run = |scenario: &str| {
        let file = dir.join("scenario.json");
        fs::write(&file, scenario).expect("writes");
        error_of(&["run", "--scenario", arg(&file), "--out", arg(&out)])
    };
    assert_eq!(
        run(&TINY_SCENARIO.replacen(r#""until""#, r#""extra":1,"until""#, 1)),
        "error: scenario_invalid: not a scenario of format version 1"
    );
    assert_eq!(
        run(&TINY_SCENARIO.replacen(r#""format_version":1"#, r#""format_version":2"#, 1)),
        "error: scenario_invalid: not a scenario of format version 1"
    );
    assert_eq!(
        run(&TINY_SCENARIO.replacen(r#"{"born":-1}"#, "[-1]", 1)),
        "error: scenario_invalid: not a scenario of format version 1"
    );
    assert_eq!(
        run(&TINY_SCENARIO.replacen("00000000000000ff", "ff", 1)),
        "error: scenario_invalid: not a scenario of format version 1"
    );
    assert_eq!(
        run(&TINY_SCENARIO.replacen(r#"{"born":-1}"#, r#"{"born":0}"#, 1)),
        "error: input_rejected: founder_not_born_before_start"
    );
    assert_eq!(
        run(&TINY_SCENARIO.replacen("31449599", "-2", 1)),
        "error: scenario_invalid: until -2 lies before the start"
    );
    assert!(!out.exists(), "a rejected scenario writes nothing");
    // A run never writes into a directory that already holds something.
    fs::create_dir_all(&out).expect("creates");
    fs::write(out.join("note.txt"), "mine").expect("writes");
    assert_eq!(
        run(TINY_SCENARIO),
        "error: out_dir_not_empty: the output directory already has content"
    );
    assert_eq!(text(&out.join("note.txt")), "mine");
}

#[test]
fn resume_checks_the_save_and_the_limit() {
    let dir = scratch("resume_errors");
    let run = run_scenario(&dir, TINY_SCENARIO, "tiny");
    assert_eq!(
        error_of(&["run", "--resume", "--out", arg(&run), "--until", "5"]),
        "error: until_in_the_past: the run stands at 31449599, until is 5"
    );
    let save = run.join("state.save");
    let mut bytes = fs::read(&save).expect("reads");
    bytes[9] = 2;
    fs::write(&save, &bytes).expect("writes");
    assert_eq!(
        error_of(&["run", "--resume", "--out", arg(&run), "--until", "99999999"]),
        "error: save_rejected: foreign_core_version"
    );
    fs::remove_file(&save).expect("removes");
    assert!(
        error_of(&["run", "--resume", "--out", arg(&run), "--until", "99999999"])
            .starts_with("error: io: ")
    );
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 20_000,
        rng_seed: RngSeed::Fixed(0x5c4e_d0f1),
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    /// ADR-0003 `float_json_roundtrip_bit_exact`: every finite f64 survives serde_json
    /// there and back bit for bit (feature `float_roundtrip`, AT5).
    #[test]
    fn float_json_roundtrip_bit_exact(bits in any::<u64>()) {
        let value = f64::from_bits(bits);
        prop_assume!(value.is_finite());
        let text = serde_json::to_string(&value).expect("writes");
        let back: f64 = serde_json::from_str(&text).expect("reads");
        prop_assert_eq!(back.to_bits(), value.to_bits(), "{}", text);
    }
}

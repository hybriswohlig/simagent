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

const TINY_SCENARIO: &str = r#"{"format_version":1,"start":{"world_seed":"00000000000000ff","founder_tag":"test:tiny","world_params":{"hereditary_diseases":false,"nutrition":[]},"death_causes":[],"founders":[{"born":-1,"sex":"female","alleles":[{"locus":"core:dummy","copies":[0,1]}]}],"couples":[],"fertility":{"per_month":0.0,"from_years":18,"until_years":45,"pregnancy_months":10}},"until":31449599}"#;

/// Three founders, ten years.
const TEN_YEARS: &str = r#"{
  "format_version": 1,
  "start": {
    "world_seed": "0000000000000001",
    "founder_tag": "test:cli",
    "world_params": {"hereditary_diseases": false, "nutrition": []},
    "death_causes": [],
    "founders": [
      {"born": -943488000, "sex": "female", "alleles": [{"locus": "core:dummy", "copies": [0, 1]}]},
      {"born": -794880000, "sex": "male", "alleles": [{"locus": "core:dummy", "copies": [1, 0]}]},
      {"born": -1, "sex": "male", "alleles": [{"locus": "core:dummy", "copies": [1, 1]}]}
    ],
    "couples": [],
    "fertility": {"per_month": 0.0, "from_years": 18, "until_years": 45, "pregnancy_months": 10}
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
            r#"{"format_version":1,"core_version":19,"data_hash":"beeea5b3dd6a1fe9","build_commit":"unknown"}"#,
            "\n",
            r#"{"t":-1,"seq":1,"input":{"start":{"world_seed":"00000000000000ff","founder_tag":"test:tiny","world_params":{"hereditary_diseases":false,"nutrition":[]},"death_causes":[],"founders":[{"born":-1,"sex":"female","alleles":[{"locus":"core:dummy","copies":[0,1]}]}],"couples":[],"fertility":{"per_month":0.0,"from_years":18,"until_years":45,"pregnancy_months":10}}}}"#,
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
            r#"{"id":2,"t":0,"subjects":[{"sim":1}],"causes":[{"input":1}],"payload":{"core:founder_added":{"born":-1,"lineage_key":"5ed9498f7efac998"}}}"#,
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
            r#"{"format_version":1,"core_version":19,"data_hash":"beeea5b3dd6a1fe9","build_commit":"unknown","world_seed":"00000000000000ff","closed_through":31449599,"year_hashes":[{"year":0,"hash":"e5f704bed88f0564"}],"log_hash":"6bed3b6ab04c2db0"}"#,
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
        "53494d4147454e540113e9bfa8ebbdb6a9f7be010a01ff0101000000010000000000000000122d0afe87ff1d020102fe8ffe3b0003010100fe8ffe3b000100018088ff1d040101019893ebf7f7b1d2ec5e00000000010000000000000000000000000203fe87ff1d01000401010100ff0109746573743a74696e79000000010100010a636f72653a64756d6d79020001000000000000000000122d0a0100e48abcc4ed97c1fbe501"
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
    let start = r#"{"start":{"world_seed":"0000000000000001","founder_tag":"test:cli","world_params":{"hereditary_diseases":false,"nutrition":[]},"death_causes":[],"founders":[{"born":-943488000,"sex":"female","alleles":[{"locus":"core:dummy","copies":[0,1]}]},{"born":-794880000,"sex":"male","alleles":[{"locus":"core:dummy","copies":[1,0]}]},{"born":-1,"sex":"male","alleles":[{"locus":"core:dummy","copies":[1,1]}]}],"couples":[],"fertility":{"per_month":0.0,"from_years":18,"until_years":45,"pregnancy_months":10}}}"#;
    let block = r#"{"fork_override":{"changes":[{"core:hereditary_diseases":true}]}}"#;
    let parse = |line: &str| serde_json::from_str::<Input>(line).expect("parses");
    let commands = vec![
        (parse(start), start),
        (parse(block), block),
        (parse(ARRANGE), ARRANGE),
    ];
    for (input, _) in &commands {
        match input {
            Input::Start(_) | Input::ForkOverride(_) | Input::ArrangePartnership(_) => {}
        }
    }
    commands
}

/// The woman and the first man of `TEN_YEARS` paired, in the command schema.
const ARRANGE: &str = r#"{"arrange_partnership":{"woman":1,"man":2}}"#;

/// ADR-0004 tests 1 and 2 with a partnership: a run stopped at the end of year 0 gets
/// an `ArrangePartnership` in the command schema at the stop (`run --resume --input`)
/// and runs on. The replay only from `input.ndjson` gives the same run directory, and
/// the same inputs directly through `submit` give the same hashes and save.
#[test]
fn a_partnership_at_a_stop_replays() {
    let dir = scratch("arrange");
    let scenario = dir.join("ten.scenario.json");
    fs::write(&scenario, TEN_YEARS).expect("writes the scenario");
    let run = dir.join("run");
    let stop = (Y - 1).to_string();
    let end = 10 * Y - 1;
    must(&[
        "run",
        "--scenario",
        arg(&scenario),
        "--out",
        arg(&run),
        "--until",
        &stop,
    ]);
    let inputs = dir.join("arrange.json");
    fs::write(&inputs, format!("[{ARRANGE}]")).expect("writes the inputs");
    let until = end.to_string();
    must(&[
        "run",
        "--resume",
        "--out",
        arg(&run),
        "--input",
        arg(&inputs),
        "--until",
        &until,
    ]);
    let log = text(&run.join("input.ndjson"));
    assert_eq!(
        log.lines().nth(2),
        Some(format!(r#"{{"t":{stop},"seq":2,"input":{ARRANGE}}}"#).as_str())
    );
    assert!(text(&run.join("events.ndjson")).contains(
        r#""subjects":[{"sim":1},{"sim":2}],"causes":[{"input":2}],"payload":{"core:partnered":{"partnership":1}}}"#
    ));
    let replayed = dir.join("replayed");
    must(&[
        "run",
        "--replay",
        arg(&run.join("input.ndjson")),
        "--manifest",
        arg(&run.join("manifest.json")),
        "--out",
        arg(&replayed),
    ]);
    assert_same_run(&run, &replayed);

    let scenario: Value = serde_json::from_str(TEN_YEARS).expect("json");
    let start: Input =
        serde_json::from_value(serde_json::json!({"start": scenario["start"]})).expect("a start");
    let arrange: Input = serde_json::from_str(ARRANGE).expect("an input");
    let mut world = World::new(sim_cli::data::core_data().expect("data"));
    let mut hasher = LogHasher::new();
    let mut out = YearOut::default();
    world.submit(start).expect("accepted");
    world.advance_until(&mut out, SimTime::from_secs(Y - 1), &mut |_| false);
    world.submit(arrange).expect("accepted");
    world.advance_until(&mut out, SimTime::from_secs(end), &mut |_| false);
    hasher.update(&world.take_log());
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

/// An input at a stop that the core rejects ends with its code and changes nothing in
/// the run directory; so does a file that is no list of inputs.
#[test]
fn a_rejected_input_at_a_stop_changes_nothing() {
    let dir = scratch("arrange_rejected");
    let scenario = dir.join("ten.scenario.json");
    fs::write(&scenario, TEN_YEARS).expect("writes the scenario");
    let run = dir.join("run");
    let stop = (Y - 1).to_string();
    must(&[
        "run",
        "--scenario",
        arg(&scenario),
        "--out",
        arg(&run),
        "--until",
        &stop,
    ]);
    let before: Vec<Vec<u8>> = FILES
        .iter()
        .map(|file| fs::read(run.join(file)).expect("reads"))
        .collect();
    let until = (2 * Y - 1).to_string();
    for (content, error) in [
        (
            r#"[{"arrange_partnership":{"woman":1,"man":9}}]"#,
            "error: input_rejected: unknown_sim",
        ),
        (
            r#"[{"arrange_partnership":{"woman":2,"man":3}}]"#,
            "error: input_rejected: partners_of_wrong_sex",
        ),
        (
            r#"{"arrange_partnership":{"woman":1,"man":2}}"#,
            "error: input_invalid: not a list of inputs in the command schema",
        ),
    ] {
        let inputs = dir.join("inputs.json");
        fs::write(&inputs, content).expect("writes the inputs");
        assert_eq!(
            error_of(&[
                "run",
                "--resume",
                "--out",
                arg(&run),
                "--input",
                arg(&inputs),
                "--until",
                &until,
            ]),
            error,
            "{content}"
        );
        let after: Vec<Vec<u8>> = FILES
            .iter()
            .map(|file| fs::read(run.join(file)).expect("reads"))
            .collect();
        assert!(after == before, "{content}");
    }
}

/// ADR-0004 test 2: the same inputs directly through `submit` and as JSON through the
/// command schema of the CLI give the same hashes. The start goes in as a scenario, the
/// override block as the override of a fork at year 5.
#[test]
fn command_schema_matches_direct_call() {
    let end = SimTime::from_secs(10 * Y - 1);
    let fork_stamp = SimTime::from_secs(5 * Y - 1);
    let commands = commands();
    let [(start, start_line), (block, block_line)] = &commands[..2] else {
        panic!("a start and an override block");
    };
    let mut world = World::new(sim_cli::data::core_data().expect("data"));
    let mut hasher = LogHasher::new();
    let mut out = YearOut::default();
    world.submit(start.clone()).expect("accepted");
    world.advance_until(&mut out, fork_stamp, &mut |_| false);
    hasher.update(&world.take_log());
    world.submit(block.clone()).expect("accepted");
    world.advance_until(&mut out, end, &mut |_| false);
    hasher.update(&world.take_log());

    let dir = scratch("command_schema");
    let start_json: Value = serde_json::from_str(start_line).expect("json");
    let scenario = serde_json::json!({
        "format_version": 1,
        "start": start_json["start"],
        "until": end.as_secs(),
    });
    let run = run_scenario(&dir, &scenario.to_string(), "run");
    let block_json: Value = serde_json::from_str(block_line).expect("json");
    let override_file = dir.join("override.json");
    let changes = block_json["fork_override"]["changes"].to_string();
    fs::write(&override_file, changes).expect("writes");
    let fork = dir.join("fork");
    must(&[
        "fork",
        arg(&run),
        "--from-year",
        "5",
        "--override",
        arg(&override_file),
        "--out",
        arg(&fork),
    ]);
    let manifest: Value = serde_json::from_str(&text(&fork.join("manifest.json"))).expect("json");
    assert_eq!(
        manifest["year_hashes"],
        serde_json::to_value(&out.year_hashes).expect("json")
    );
    assert_eq!(
        manifest["log_hash"],
        Value::from(format!("{:016x}", hasher.digest()))
    );
    assert_eq!(
        fs::read(fork.join("state.save")).expect("reads"),
        world.save(&out.year_hashes).expect("saves")
    );
    // The input log of the fork holds both inputs as the command schema writes them.
    let log = text(&fork.join("input.ndjson"));
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(
        lines[1..3],
        [
            format!(r#"{{"t":-1,"seq":1,"input":{start_line}}}"#),
            format!(
                r#"{{"t":{},"seq":2,"input":{block_line}}}"#,
                fork_stamp.as_secs()
            ),
        ]
    );
}

/// The mini pedigree of the Durchstich (HANDOFF §9 step 5): `simagent export` writes
/// every sim of a run with its parents, its birth and its death, and every partnership,
/// into `pedigree.json`. The dummy scenario after ten years: the founders 1 to 3 (sims
/// 2 and 3 have died), the three children of sims 1 and 2, and their partnership, which
/// ended with the death of sim 2. Values from tools/golden_reference.py.
#[test]
fn export_writes_the_pedigree() {
    let dir = scratch("export");
    let scenario = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/scenarios/dummy.json");
    let run = dir.join("dummy");
    must(&["run", "--scenario", arg(&scenario), "--out", arg(&run)]);
    let before: Vec<Vec<u8>> = FILES
        .iter()
        .map(|file| fs::read(run.join(file)).expect("reads"))
        .collect();
    must(&["export", arg(&run)]);
    // The alleles at `core:dummy` (maternal, paternal) from tools/golden_reference.py,
    // in the form of a founder.
    let founder = |sim: u64,
                   key: &str,
                   sex: &str,
                   genes: (u8, u8),
                   born: i64,
                   died: &str,
                   cause: &str| {
        format!(
            r#"{{"sim":{sim},"lineage_key":"{key}","sex":"{sex}","alleles":[{{"locus":"core:dummy","copies":[{},{}]}}],"born":{born},"died":{died},"cause_of_death":{cause},"mother":null,"father":null}}"#,
            genes.0, genes.1
        )
    };
    let child = |sim: u64, key: &str, sex: &str, born: i64| {
        format!(
            r#"{{"sim":{sim},"lineage_key":"{key}","sex":"{sex}","alleles":[{{"locus":"core:dummy","copies":[1,0]}}],"born":{born},"died":null,"cause_of_death":null,"mother":1,"father":2}}"#
        )
    };
    let persons = [
        founder(
            1,
            "67a2a1a58cf8f47a",
            "female",
            (1, 1),
            -943_488_000,
            "null",
            "null",
        ),
        founder(
            2,
            "d14e686636ce6fd8",
            "male",
            (1, 0),
            -794_880_000,
            "125649014",
            r#""test:base""#,
        ),
        founder(
            3,
            "431e7431023db46a",
            "male",
            (1, 1),
            -1,
            "91095303",
            r#""test:base""#,
        ),
        child(4, "82a371d4d7ec74d0", "male", 41_126_400),
        child(5, "633a53f95f45349a", "female", 91_929_600),
        child(6, "3b95377839225a93", "female", 118_540_800),
    ];
    let expected = format!(
        r#"{{"persons":[{}],"partnerships":[{{"partnership":1,"woman":1,"man":2,"formed":0,"ended":125649014}}]}}"#,
        persons.join(",")
    ) + "\n";
    assert_eq!(text(&run.join("pedigree.json")), expected);
    // The run itself stays as it was.
    let after: Vec<Vec<u8>> = FILES
        .iter()
        .map(|file| fs::read(run.join(file)).expect("reads"))
        .collect();
    assert!(after == before);
    assert_eq!(
        error_of(&["export", arg(&dir.join("nothing"))])
            .split(':')
            .next(),
        Some("error")
    );
    // Review finding: a `pedigree.json` that links to the save. The export replaces the
    // link with its own file and leaves the save as it was.
    let pedigree = run.join("pedigree.json");
    fs::remove_file(&pedigree).expect("removes");
    std::os::unix::fs::symlink(run.join("state.save"), &pedigree).expect("links");
    must(&["export", arg(&run)]);
    assert_eq!(fs::read(run.join("state.save")).expect("reads"), before[5]);
    let kind = fs::symlink_metadata(&pedigree).expect("exists").file_type();
    assert!(!kind.is_symlink());
    assert_eq!(text(&pedigree), expected);
    // The same for the leftover of an interrupted export.
    let leftover = run.join("pedigree.json.tmp");
    std::os::unix::fs::symlink(run.join("state.save"), &leftover).expect("links");
    must(&["export", arg(&run)]);
    assert_eq!(fs::read(run.join("state.save")).expect("reads"), before[5]);
    assert_eq!(text(&pedigree), expected);
    // No temporary file stays behind.
    let mut names: Vec<String> = fs::read_dir(&run)
        .expect("lists")
        .map(|entry| {
            let name = entry.expect("lists").file_name();
            name.into_string().expect("names")
        })
        .collect();
    names.sort();
    let mut wanted: Vec<String> = FILES.iter().map(|file| (*file).to_owned()).collect();
    wanted.push("pedigree.json".to_owned());
    wanted.sort();
    assert_eq!(names, wanted);
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
    replace_in(&log, r#""core_version":19"#, r#""core_version":20"#);
    assert_eq!(
        error_of(&["run", "--replay", arg(&log), "--out", arg(&out)]),
        "error: replay_foreign_core_version: the input log is of core version 20, this is 19"
    );
    replace_in(&log, r#""core_version":20"#, r#""core_version":19"#);
    replace_in(&log, "beeea5b3dd6a1fe9", "c3f04a38869c20d7");
    assert_eq!(
        error_of(&["run", "--replay", arg(&log), "--out", arg(&out)]),
        "error: replay_foreign_data_hash: the input log has data hash c3f04a38869c20d7, this is beeea5b3dd6a1fe9"
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
            "error: replay_diverged: year 3 has hash {}, the manifest says 0123456789abcdef (repro package: {})",
            year(3),
            arg(&out.join("repro.json"))
        )
    );
    // The replay ended there (ADR-0003 1.4): the years 0 to 3 are written, nothing later.
    assert_eq!(text(&out.join("census.csv")).lines().count(), 1 + 4);
    assert_eq!(text(&out.join("events.ndjson")).lines().count(), 4 + 4 * 3);
}

/// The manifest also carries the log hash of the original run; a replay compares it at
/// the end (ADR-0004 test 1: the same year hashes and the same log hash).
#[test]
fn replay_compares_the_log_hash() {
    let dir = scratch("log_hash");
    let original = run_scenario(&dir, TINY_SCENARIO, "tiny");
    let manifest_path = original.join("manifest.json");
    replace_in(&manifest_path, "6bed3b6ab04c2db0", "6bed3b6ab04c2db1");
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
        "error: replay_diverged: the log hash is 6bed3b6ab04c2db0, the manifest says 6bed3b6ab04c2db1"
    );
}

/// A limit the run cannot reach is refused before anything is written: a time before
/// the clock, or one in the last year of the time range, where the next year end could
/// no longer be planned (ADR-0002).
#[test]
fn until_outside_the_run_is_rejected() {
    let dir = scratch("until");
    let scenario = dir.join("scenario.json");
    fs::write(&scenario, TINY_SCENARIO).expect("writes");
    let out = dir.join("out");
    let run = |until: &str| {
        error_of(&[
            "run",
            "--scenario",
            arg(&scenario),
            "--out",
            arg(&out),
            "--until",
            until,
        ])
    };
    assert_eq!(
        run("-2"),
        "error: until_in_the_past: the run stands at -1, until is -2"
    );
    // 1 000 000 years of 31 449 600 seconds, less one year, is the last limit.
    assert_eq!(
        run("31449568550401"),
        "error: until_beyond_horizon: until 31449568550401 is later than 31449568550400, one year before the end of the time range"
    );
    assert_eq!(
        run("31449599999999"),
        "error: until_beyond_horizon: until 31449599999999 is later than 31449568550400, one year before the end of the time range"
    );
    assert_eq!(
        run("31449600000001"),
        "error: usage: --until 31449600000001 is not a time in seconds"
    );
    assert!(!out.exists(), "nothing was written");
    let far = TINY_SCENARIO.replacen("31449599", "31449568550401", 1);
    fs::write(&scenario, far).expect("writes");
    assert_eq!(
        error_of(&["run", "--scenario", arg(&scenario), "--out", arg(&out)]),
        "error: until_beyond_horizon: until 31449568550401 is later than 31449568550400, one year before the end of the time range"
    );
    assert!(!out.exists(), "nothing was written");
    // Also when this call would stop earlier: the run could never be resumed to its end.
    assert_eq!(
        error_of(&[
            "run",
            "--scenario",
            arg(&scenario),
            "--out",
            arg(&out),
            "--until",
            "5"
        ]),
        "error: until_beyond_horizon: until 31449568550401 is later than 31449568550400, one year before the end of the time range"
    );
    assert!(!out.exists(), "nothing was written");
    // A resumed run has the same limits and stays untouched when refused.
    let done = run_scenario(&dir, TINY_SCENARIO, "done");
    let before = text(&done.join("input.ndjson"));
    assert_eq!(
        error_of(&[
            "run",
            "--resume",
            "--out",
            arg(&done),
            "--until",
            "31449568550401"
        ]),
        "error: until_beyond_horizon: until 31449568550401 is later than 31449568550400, one year before the end of the time range"
    );
    assert_eq!(text(&done.join("input.ndjson")), before);
}

/// A run that was interrupted while it wrote leaves lines in its files that the save
/// does not know. Resuming it would write those years twice; it is refused instead.
#[test]
fn resume_refuses_a_directory_that_does_not_match_its_save() {
    let dir = scratch("interrupted");
    let scenario = dir.join("scenario.json");
    fs::write(&scenario, TEN_YEARS).expect("writes");
    let out = dir.join("out");
    let mid = (3 * Y + 100 * 86_400).to_string();
    must(&[
        "run",
        "--scenario",
        arg(&scenario),
        "--out",
        arg(&out),
        "--until",
        &mid,
    ]);
    let snapshot = |name: &str| fs::read(out.join(name)).expect("reads");
    let pristine: Vec<Vec<u8>> = FILES.iter().map(|name| snapshot(name)).collect();
    let restore = || {
        for (name, bytes) in FILES.iter().zip(&pristine) {
            fs::write(out.join(name), bytes).expect("writes");
        }
    };
    let resume = || error_of(&["run", "--resume", "--out", arg(&out)]);
    let unchanged = |tampered: &str, with: &[u8]| {
        for (name, bytes) in FILES.iter().zip(&pristine) {
            let expected: &[u8] = if *name == tampered { with } else { bytes };
            assert!(
                snapshot(name) == expected,
                "{name} was changed by a refused resume"
            );
        }
    };

    // One more complete event line, as a crashed run would leave it.
    let events = text(&out.join("events.ndjson"));
    let extra = r#"{"id":15,"t":103161600,"subjects":[{"sim":2}],"causes":[{"plan":{"by":12,"planned_at":85708800}}],"payload":{"core:birthday":{"age_years":29}}}"#;
    let longer = format!("{events}{extra}\n");
    fs::write(out.join("events.ndjson"), &longer).expect("writes");
    assert_eq!(
        resume(),
        "error: run_dir_inconsistent: events.ndjson does not match the log hash of the manifest"
    );
    unchanged("events.ndjson", longer.as_bytes());
    restore();

    // One more census row.
    let census = text(&out.join("census.csv")) + "3,3,94348800\n";
    fs::write(out.join("census.csv"), &census).expect("writes");
    assert_eq!(
        resume(),
        "error: run_dir_inconsistent: census.csv has 4 rows, the save has 3 completed years"
    );
    unchanged("census.csv", census.as_bytes());
    restore();

    // A manifest of another moment of the run.
    replace_in(
        &out.join("manifest.json"),
        &format!(r#""closed_through":{mid}"#),
        r#""closed_through":5"#,
    );
    assert_eq!(
        resume(),
        "error: run_dir_inconsistent: manifest.json does not describe the save"
    );
    restore();

    // A scenario file of another format version.
    replace_in(
        &out.join("scenario.resolved.json"),
        r#""format_version":1"#,
        r#""format_version":2"#,
    );
    assert_eq!(
        resume(),
        "error: scenario_invalid: not a scenario of format version 1"
    );
    restore();

    // The pristine directory resumes and ends like the uninterrupted run.
    must(&["run", "--resume", "--out", arg(&out)]);
    let whole = run_scenario(&dir, TEN_YEARS, "whole");
    assert_same_run(&whole, &out);
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
    use sim_cli::session::{Compare, Session};
    let dir = scratch("session");
    let scenario: Scenario = serde_json::from_str(TINY_SCENARIO).expect("parses");
    let rejected = |code: String| panic!("rejected: {code}");
    let first = Input::Start(scenario.start.clone());
    let mut session = Session::start(
        &dir,
        SimTime::from_secs(-1),
        first,
        Compare::Nothing,
        rejected,
    )
    .expect("starts");
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
    // A log that does not begin with a start, and a start that is not the first input.
    let block = r#"{"t":-1,"seq":1,"input":{"fork_override":{"changes":[{"core:hereditary_diseases":true}]}}}"#;
    assert_eq!(
        replay(&with_lines(&[block, lines[1]])),
        "error: input_log_invalid: the first input is not a start"
    );
    assert!(!dir.join("attempt4").exists(), "nothing was written");
    // The record as a list of values instead of an object with field names.
    let positional = r#"[-1,1,{"start":["00000000000000ff","test:tiny",[[-1]]]}]"#;
    assert_eq!(
        replay(&with_lines(&[positional])),
        "error: input_log_invalid: line 2 is not a record of the input log"
    );
    let founder = r#"{"born":-1,"sex":"female","alleles":[{"locus":"core:dummy","copies":[0,1]}]}"#;
    assert!(lines[1].contains(founder));
    for positional in [
        r#"[-1,"female",{"maternal":"wild","paternal":"dummy"}]"#,
        r#"{"born":-1,"sex":"female","alleles":[["core:dummy",[0,1]]]}"#,
    ] {
        let nested_positional = lines[1].replacen(founder, positional, 1);
        assert_eq!(
            replay(&with_lines(&[&nested_positional])),
            "error: input_log_invalid: line 2 is not a record of the input log",
            "{positional}"
        );
    }
    // A founder without sex or alleles is no founder: nothing is filled in.
    for missing in [
        r#"{"born":-1,"alleles":[{"locus":"core:dummy","copies":[0,1]}]}"#,
        r#"{"born":-1,"sex":"female"}"#,
        r#"{"born":-1}"#,
    ] {
        let incomplete = lines[1].replacen(founder, missing, 1);
        assert_eq!(
            replay(&with_lines(&[&incomplete])),
            "error: input_log_invalid: line 2 is not a record of the input log",
            "{missing}"
        );
    }
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
        run(&TINY_SCENARIO.replacen(r#"{"born":-1,"sex":"female","#, r#"{"born":-1,"#, 1)),
        "error: scenario_invalid: not a scenario of format version 1"
    );
    assert_eq!(
        run(&TINY_SCENARIO.replacen(
            r#"{"locus":"core:dummy","copies":[0,1]}"#,
            r#"["core:dummy",[0,1]]"#,
            1
        )),
        "error: scenario_invalid: not a scenario of format version 1"
    );
    assert_eq!(
        run(&TINY_SCENARIO.replacen(r#""copies":[0,1]"#, r#""copies":[0,256]"#, 1)),
        "error: scenario_invalid: not a scenario of format version 1"
    );
    // Alleles the genome map does not allow are a rejected start (genetics
    // specification 2.1): a value other than 0 and 1, a locus it does not have.
    assert_eq!(
        run(&TINY_SCENARIO.replacen(r#""copies":[0,1]"#, r#""copies":[0,2]"#, 1)),
        "error: input_rejected: founder_copies"
    );
    assert_eq!(
        run(&TINY_SCENARIO.replacen(r#""locus":"core:dummy""#, r#""locus":"core:nothing""#, 1)),
        "error: input_rejected: founder_unknown_locus"
    );
    assert_eq!(
        run(&TINY_SCENARIO.replacen("00000000000000ff", "ff", 1)),
        "error: scenario_invalid: not a scenario of format version 1"
    );
    assert_eq!(
        run(&TINY_SCENARIO.replacen(r#"{"born":-1,"#, r#"{"born":0,"#, 1)),
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
    assert_eq!(bytes[9], 19, "the core version in the header of the save");
    bytes[9] = 20;
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

/// ADR-0016 `a_diverged_replay_writes_a_repro_package`: a replay that differs from its
/// manifest leaves `repro.json`; `simagent repro` reproduces the difference, and a package
/// with the true hashes passes. No body value is read, so the clamp at 6 plays no part.
#[test]
fn a_diverged_replay_writes_a_repro_package() {
    let dir = scratch("repro_diverged");
    let original = run_scenario(&dir, TEN_YEARS, "original");
    let manifest_path = original.join("manifest.json");
    let manifest: Value = serde_json::from_str(&text(&manifest_path)).expect("json");
    let year = |n: usize| {
        manifest["year_hashes"][n]["hash"]
            .as_str()
            .expect("hex")
            .to_owned()
    };
    let (true_2, true_3) = (year(2), year(3));
    replace_in(&manifest_path, &true_3, "0123456789abcdef");
    let out = dir.join("out");
    let log = original.join("input.ndjson");
    let _ = error_of(&[
        "run",
        "--replay",
        arg(&log),
        "--manifest",
        arg(&manifest_path),
        "--out",
        arg(&out),
    ]);
    let package_path = out.join("repro.json");
    let package: Value = serde_json::from_str(&text(&package_path)).expect("json");
    assert_eq!(package["format_version"], 1);
    assert_eq!(package["kind"], "replay_diverged");
    assert_eq!(package["core_version"], manifest["core_version"]);
    assert_eq!(package["data_hash"], manifest["data_hash"]);
    assert_eq!(package["world_seed"], manifest["world_seed"]);
    assert_eq!(package["checkpoint"]["year"], 2);
    assert_eq!(package["checkpoint"]["hash"], true_2.as_str());
    assert_eq!(package["at"]["year"], 3);
    assert_eq!(package["invariant"], Value::Null);
    assert_eq!(package["expected"].as_array().expect("list").len(), 4);
    assert_eq!(package["expected"][3]["hash"], "0123456789abcdef");
    // The inputs are those of the run.
    let records = text(&log).lines().count() - 2;
    assert_eq!(
        package["input_log"].as_array().expect("list").len(),
        records
    );
    // The one command reproduces it; with the true hash the package passes.
    assert!(
        error_of(&["repro", arg(&package_path)])
            .starts_with("error: repro_reproduced: year 3 has hash"),
    );
    replace_in(
        &package_path,
        "\"hash\":\"0123456789abcdef\"",
        &format!("\"hash\":\"{true_3}\""),
    );
    must(&["repro", arg(&package_path)]);
    // An unknown field is no package.
    replace_in(
        &package_path,
        "\"format_version\":1",
        "\"format_version\":1,\"x\":0",
    );
    assert!(error_of(&["repro", arg(&package_path)]).starts_with("error: repro_invalid:"));
}

/// ADR-0016 `repro_packages_pass`: every package under `tests/repro/` is a regression
/// test; `simagent repro` must pass for each (none so far).
#[test]
fn repro_packages_pass() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/repro");
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };
    for entry in entries {
        let path = entry.expect("entry").path();
        if path.extension().is_some_and(|ext| ext == "json") {
            must(&["repro", arg(&path)]);
        }
    }
}

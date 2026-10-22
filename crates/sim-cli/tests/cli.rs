//! The command line runner as a process (ADR-0004 sections 4, 6 and 8, acceptance
//! tests 1, 2 and 4; ADR-0003 acceptance tests of `sim-cli`).
//!
//! The pinned contents of the run directory of scenario "tiny" (seed 0xff, one founder
//! born one second before the start, one year) come from the ADRs and from the world model
//! of `tools/world_reference.py` on the repository's data, not from a run of the program.

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

const TINY_SCENARIO: &str = r#"{"format_version":5,"start":{"world_seed":"00000000000000ff","founder_tag":"test:tiny","world_params":{"hereditary_diseases":false,"nutrition":[]},"founders":[{"born":-1,"sex":"female","alleles":[{"locus":"core:disease_ar","copies":[0,1]}]}],"couples":[],"capacity":null,"pool":null},"until":31449599}"#;

/// Three founders, ten years.
const TEN_YEARS: &str = r#"{
  "format_version": 5,
  "start": {
    "world_seed": "0000000000000001",
    "founder_tag": "test:cli",
    "world_params": {"hereditary_diseases": false, "nutrition": []},
    "founders": [
      {"born": -943488000, "sex": "female", "alleles": [{"locus": "core:disease_ar", "copies": [0, 1]}]},
      {"born": -794880000, "sex": "male", "alleles": [{"locus": "core:disease_ar", "copies": [1, 0]}]},
      {"born": -1, "sex": "male", "alleles": [{"locus": "core:disease_ar", "copies": [1, 1]}]}
    ],
    "couples": [],
    "capacity": null,
    "pool": null
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
            r#"{"format_version":5,"core_version":46,"data_hash":"b1f5c0054302363d","build_commit":"unknown"}"#,
            "\n",
            r#"{"t":-1,"seq":1,"input":{"start":{"world_seed":"00000000000000ff","founder_tag":"test:tiny","world_params":{"hereditary_diseases":false,"nutrition":[]},"founders":[{"born":-1,"sex":"female","alleles":[{"locus":"core:disease_ar","copies":[0,1]}]}],"couples":[],"capacity":null,"pool":null}}}"#,
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
            r#"{"id":3,"t":0,"subjects":[{"sim":1}],"causes":[{"input":1},{"rule":"core:household"}],"payload":{"core:household_formed":{"household":1}}}"#,
            "\n",
            r#"{"id":4,"t":31449599,"subjects":[{"sim":1}],"causes":[{"plan":{"by":2,"planned_at":0}}],"payload":{"core:birthday":{"age_years":1}}}"#,
            "\n",
        )
    );
    assert_eq!(text(&out.join("census.csv")), census(&["0,1,31449600,1"]));
    // Year hash and log hash from the world model of tools/world_reference.py on the
    // repository's data.
    assert_eq!(
        text(&out.join("manifest.json")),
        concat!(
            r#"{"format_version":5,"core_version":46,"data_hash":"b1f5c0054302363d","build_commit":"unknown","world_seed":"00000000000000ff","closed_through":31449599,"year_hashes":[{"year":0,"hash":"b42c32b1197db934"}],"log_hash":"6d23128b21469cb5"}"#,
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
        "53494d4147454e54022ebdec8898d480f0fab1011201ff01010000fe87ff1d020102068088ff1d0002060180a8a702fe8ffe3b0007010100fe8ffe3b000100018088ff1dfe9fedd7040006070100e4daacea1300040201a0bb8ca1c58498a01c00b8f4e5ff8c1f0005020185d2bc82f8b2ead44d00080101019893ebf7f7b1d2ec5e00c08f80af8684a8a82be0849480e4ee92801b8480d68680bcf482c0018f8a80dced80b8aa34c095ce80a8b9b8902686c0d6808084d68a200000000000000000010000000000000101000000000006010004fe87ff1d01000200020002000501010100ff0109746573743a74696e790000010100010f636f72653a646973656173655f61720200010000000100b4f2f6cb91d68c96b401"
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
    assert_eq!(text(&original.join("events.ndjson")).lines().count(), 37);
    // Without the manifest the replay runs as well; it only has nothing to compare with.
    let unchecked = dir.join("unchecked");
    must(&["run", "--replay", arg(&log), "--out", arg(&unchecked)]);
    assert_same_run(&original, &unchecked);
}

/// Every `Input` variant as the command schema writes it. A new variant fails to
/// compile here until it has a line.
fn commands() -> Vec<(Input, &'static str)> {
    let start = r#"{"start":{"world_seed":"0000000000000001","founder_tag":"test:cli","world_params":{"hereditary_diseases":false,"nutrition":[]},"founders":[{"born":-943488000,"sex":"female","alleles":[{"locus":"core:disease_ar","copies":[0,1]}]},{"born":-794880000,"sex":"male","alleles":[{"locus":"core:disease_ar","copies":[1,0]}]},{"born":-1,"sex":"male","alleles":[{"locus":"core:disease_ar","copies":[1,1]}]}],"couples":[],"capacity":null,"pool":null}}"#;
    let block = r#"{"fork_override":{"changes":[{"core:hereditary_diseases":true}]}}"#;
    let parse = |line: &str| serde_json::from_str::<Input>(line).expect("parses");
    let rule = r#"{"set_world_param":{"change":{"core:hereditary_diseases":true}}}"#;
    let commands = vec![
        (parse(start), start),
        (parse(block), block),
        (parse(ARRANGE), ARRANGE),
        (parse(rule), rule),
    ];
    for (input, _) in &commands {
        match input {
            Input::Start(_)
            | Input::ForkOverride(_)
            | Input::ArrangePartnership(_)
            | Input::SetWorldParam(_) => {}
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

/// ADR-0023 section 5, the lever of the switch: world rules at stops inside the years,
/// on at `2.5 · Y − 1` and off at `6.5 · Y − 1` (`run --resume --input`). The census row
/// of the year of each rule shows the new state: `diseases_on` and
/// `affected.core:disease_ar` are 1 in the rows 2 to 5 and 0 from row 6 on; every other
/// column and every row of the run without rules (the switch off throughout) is the
/// twin's. Sim 3 (two copies, onset 0) lives with 4 years of exposure: the fork test
/// shows him alive with 5. Turning on writes his onset (path 3) caused by the rule, as
/// the fork does; the replay from `input.ndjson` gives the same run.
#[test]
fn world_rules_move_the_census_from_their_year() {
    let dir = scratch("world_rules");
    let scenario = dir.join("ten.scenario.json");
    fs::write(&scenario, TEN_YEARS).expect("writes the scenario");
    let (run, twin) = (dir.join("run"), dir.join("twin"));
    let end = (10 * Y - 1).to_string();
    let twin_args = [
        "run",
        "--scenario",
        arg(&scenario),
        "--out",
        arg(&twin),
        "--until",
        &end,
    ];
    must(&twin_args);
    let (on, off) = (2 * Y + Y / 2 - 1, 6 * Y + Y / 2 - 1);
    let first = on.to_string();
    must(&[
        "run",
        "--scenario",
        arg(&scenario),
        "--out",
        arg(&run),
        "--until",
        &first,
    ]);
    let rule = |value: bool| {
        format!(r#"{{"set_world_param":{{"change":{{"core:hereditary_diseases":{value}}}}}}}"#)
    };
    for (value, until) in [(true, off.to_string()), (false, end.clone())] {
        let inputs = dir.join(format!("rule_{value}.json"));
        fs::write(&inputs, format!("[{}]", rule(value))).expect("writes the inputs");
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
    }
    let log = text(&run.join("input.ndjson"));
    assert_eq!(
        log.lines().skip(2).take(2).collect::<Vec<_>>(),
        [
            format!(r#"{{"t":{on},"seq":2,"input":{}}}"#, rule(true)),
            format!(r#"{{"t":{off},"seq":3,"input":{}}}"#, rule(false)),
        ]
    );
    assert!(text(&run.join("events.ndjson")).contains(&format!(
        r#""t":{},"subjects":[{{"sim":3}}],"causes":[{{"input":2}}],"payload":{{"core:disease_onset":{{"disease":"core:disease_ar","since_year":-1,"age_years":0}}}}"#,
        on + 1
    )));
    let (ours, theirs) = (
        text(&run.join("census.csv")),
        text(&twin.join("census.csv")),
    );
    let header: Vec<&str> = ours.lines().next().expect("a header").split(',').collect();
    assert_eq!(theirs.lines().next(), ours.lines().next());
    let switched = ["diseases_on", "affected.core:disease_ar"];
    for (row, (our, their)) in ours.lines().zip(theirs.lines()).skip(1).enumerate() {
        for ((name, value), twin_value) in header.iter().zip(our.split(',')).zip(their.split(',')) {
            let expected = match (switched.contains(name), row) {
                (true, 2..=5) => "1",
                (true, _) => "0",
                (false, _) => twin_value,
            };
            assert_eq!((name, value), (name, expected), "year {row}");
        }
    }
    assert_eq!(ours.lines().count(), 11);
    assert_eq!(theirs.lines().count(), 11);
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
        "format_version": 5,
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

/// `simagent kinship` prints φ of two persons of a run (step 10): the golden run after
/// thirty years, pairs from tools/golden_reference.py (`pedigree_kinship_*`); an unknown
/// person is an error.
#[test]
fn kinship_prints_the_coefficient() {
    let dir = scratch("kinship");
    let scenario = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/scenarios/dummy.json");
    let run = dir.join("dummy");
    must(&["run", "--scenario", arg(&scenario), "--out", arg(&run)]);
    for (a, b, phi) in [
        ("1", "8", "0.25"),
        ("8", "10", "0.25"),
        ("1", "2", "0.0"),
        ("3", "3", "0.5"),
        ("13", "13", "0.5"),
        ("2", "13", "0.125"),
        ("10", "13", "0.125"),
        ("8", "7", "0.0"),
    ] {
        let outcome = simagent(&["kinship", arg(&run), a, b]);
        assert!(outcome.ok, "{a} and {b}: {}", outcome.stderr);
        assert_eq!(outcome.stdout, format!("{phi}\n"), "{a} and {b}");
    }
    // Review finding: an ID with a leading hyphen is an unknown person too, in either place.
    for (a, b) in [
        ("14", "1"),
        ("1", "0"),
        ("x", "1"),
        ("-1", "1"),
        ("1", "-x"),
    ] {
        assert!(
            error_of(&["kinship", arg(&run), a, b]).starts_with("error: unknown_sim: "),
            "{a} and {b}"
        );
    }
}

/// `simagent measure` (ADR-0020 section 3) prints the measurement of the living of a run as
/// one line of JSON: the dummy run at its end, 38 traits and 121 additive loci of map 4
/// (tools/golden_reference.py, `dummy run measure_json`); and the error `no_living` once
/// nobody lives: a single founder of 100, after 60 years.
#[test]
fn measure_prints_the_measurement() {
    let dir = scratch("measure");
    let scenario = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/scenarios/dummy.json");
    let run = dir.join("dummy");
    must(&["run", "--scenario", arg(&scenario), "--out", arg(&run)]);
    let outcome = simagent(&["measure", arg(&run)]);
    assert!(outcome.ok, "{}", outcome.stderr);
    assert_eq!(
        outcome.stdout,
        "{\"format_version\":1,\"clock\":943487999,\"living\":6,\"traits\":[{\"trait\":\"core:androgen\",\"mean\":-0.14679216352924515,\"sigma\":1.0671873729054746},{\"trait\":\"core:bald\",\"mean\":0.2622871928201795,\"sigma\":1.3768926368215257},{\"trait\":\"core:beard\",\"mean\":-0.6667895802604935,\"sigma\":0.8539125638299665},{\"trait\":\"core:body_fat\",\"mean\":0.3867569977417232,\"sigma\":1.247219128924647},{\"trait\":\"core:body_hair\",\"mean\":-0.4771276551419548,\"sigma\":0.5335936864527374},{\"trait\":\"core:breast\",\"mean\":0.40927060156829403,\"sigma\":0.9758187445536298},{\"trait\":\"core:chest_depth\",\"mean\":0.7353318250377071,\"sigma\":0.4056031174545983},{\"trait\":\"core:cognition\",\"mean\":-1.3809113105505701,\"sigma\":1.462494064565354},{\"trait\":\"core:curl\",\"mean\":0.33565827520978636,\"sigma\":0.7453559924999299},{\"trait\":\"core:disease_liability\",\"mean\":-1.982200444988875,\"sigma\":0.9999999999999998},{\"trait\":\"core:estrogen\",\"mean\":1.0178995525111871,\"sigma\":0.5773502691896257},{\"trait\":\"core:face_01\",\"mean\":0.501466838329042,\"sigma\":0.5482928049865328},{\"trait\":\"core:face_02\",\"mean\":-0.16927472646517186,\"sigma\":0.8403454977302823},{\"trait\":\"core:face_03\",\"mean\":-0.14302538269876594,\"sigma\":0.5968086423264627},{\"trait\":\"core:face_04\",\"mean\":-0.5180076748081298,\"sigma\":0.6587551391323891},{\"trait\":\"core:face_05\",\"mean\":-0.6628248877111406,\"sigma\":0.4256530929707379},{\"trait\":\"core:face_06\",\"mean\":0.4924670633234169,\"sigma\":0.5482928049865327},{\"trait\":\"core:face_07\",\"mean\":0.18648304625717702,\"sigma\":0.3258535799336192},{\"trait\":\"core:face_08\",\"mean\":-0.4193351416214594,\"sigma\":0.5482928049865328},{\"trait\":\"core:face_09\",\"mean\":-0.34736610751397895,\"sigma\":0.5143091374736493},{\"trait\":\"core:face_10\",\"mean\":0.14418410373074012,\"sigma\":0.45407109086084246},{\"trait\":\"core:face_11\",\"mean\":0.007316692082697858,\"sigma\":0.6394985014316559},{\"trait\":\"core:face_12\",\"mean\":0.3264253810321409,\"sigma\":0.5302646089977677},{\"trait\":\"core:fat_distribution\",\"mean\":0.12426980992141852,\"sigma\":0.7861650943380502},{\"trait\":\"core:freckle\",\"mean\":0.9099022524436888,\"sigma\":0.5773502691896257},{\"trait\":\"core:grey\",\"mean\":-0.11401798288376123,\"sigma\":0.372677996249965},{\"trait\":\"core:growth\",\"mean\":0.1534419972834016,\"sigma\":1.0671873729054746},{\"trait\":\"core:hair_darkness\",\"mean\":-0.5306492337691556,\"sigma\":1.2583057392117916},{\"trait\":\"core:hair_density\",\"mean\":0.30933393331833364,\"sigma\":0.74535599249993},{\"trait\":\"core:head_size\",\"mean\":0.848524828545953,\"sigma\":0.47206696794228487},{\"trait\":\"core:hip_width\",\"mean\":1.0314383807071488,\"sigma\":0.6209312003399052},{\"trait\":\"core:leg_ratio\",\"mean\":0.04011587210319742,\"sigma\":0.5558402048550762},{\"trait\":\"core:longevity\",\"mean\":0.8215502945759688,\"sigma\":1.3437096247164249},{\"trait\":\"core:muscle\",\"mean\":-0.5617963717573727,\"sigma\":0.9753916592266352},{\"trait\":\"core:pigment\",\"mean\":0.5764439222352774,\"sigma\":2.1343747458109497},{\"trait\":\"core:puberty\",\"mean\":-0.9733756656083598,\"sigma\":0.816496580927726},{\"trait\":\"core:shoulder_width\",\"mean\":-0.44425347699640844,\"sigma\":0.5335936864527374},{\"trait\":\"core:skin_age\",\"mean\":-0.8766530836729082,\"sigma\":0.816496580927726}],\"loci\":[{\"locus\":\"core:androgen_01\",\"mean_dose\":0.8333333333333334},{\"locus\":\"core:breast_02\",\"mean_dose\":1.0},{\"locus\":\"core:cognition_04\",\"mean_dose\":0.6666666666666666},{\"locus\":\"core:curl_01\",\"mean_dose\":0.8333333333333334},{\"locus\":\"core:disease_liability_06\",\"mean_dose\":0.5},{\"locus\":\"core:face_06_01\",\"mean_dose\":1.5},{\"locus\":\"core:freckle_01\",\"mean_dose\":1.1666666666666667},{\"locus\":\"core:growth_03\",\"mean_dose\":0.8333333333333334},{\"locus\":\"core:longevity_01\",\"mean_dose\":1.5},{\"locus\":\"core:pigment_03\",\"mean_dose\":1.0},{\"locus\":\"core:skin_age_02\",\"mean_dose\":0.6666666666666666},{\"locus\":\"core:pigment_11\",\"mean_dose\":1.5},{\"locus\":\"core:growth_11\",\"mean_dose\":1.1666666666666667},{\"locus\":\"core:cognition_12\",\"mean_dose\":1.5},{\"locus\":\"core:androgen_02\",\"mean_dose\":0.6666666666666666},{\"locus\":\"core:body_fat_01\",\"mean_dose\":0.8333333333333334},{\"locus\":\"core:cognition_05\",\"mean_dose\":1.0},{\"locus\":\"core:curl_02\",\"mean_dose\":1.5},{\"locus\":\"core:disease_liability_07\",\"mean_dose\":0.5},{\"locus\":\"core:face_01_01\",\"mean_dose\":1.5},{\"locus\":\"core:face_09_01\",\"mean_dose\":0.6666666666666666},{\"locus\":\"core:freckle_02\",\"mean_dose\":1.8333333333333333},{\"locus\":\"core:growth_04\",\"mean_dose\":1.6666666666666667},{\"locus\":\"core:hair_darkness_01\",\"mean_dose\":1.0},{\"locus\":\"core:longevity_02\",\"mean_dose\":0.6666666666666666},{\"locus\":\"core:pigment_04\",\"mean_dose\":1.3333333333333333},{\"locus\":\"core:puberty_01\",\"mean_dose\":0.6666666666666666},{\"locus\":\"core:pigment_12\",\"mean_dose\":0.6666666666666666},{\"locus\":\"core:growth_12\",\"mean_dose\":0.6666666666666666},{\"locus\":\"core:cognition_13\",\"mean_dose\":0.3333333333333333},{\"locus\":\"core:androgen_03\",\"mean_dose\":0.8333333333333334},{\"locus\":\"core:body_fat_02\",\"mean_dose\":1.5},{\"locus\":\"core:chest_depth_01\",\"mean_dose\":1.6666666666666667},{\"locus\":\"core:cognition_06\",\"mean_dose\":0.8333333333333334},{\"locus\":\"core:disease_liability_08\",\"mean_dose\":1.1666666666666667},{\"locus\":\"core:face_04_01\",\"mean_dose\":0.5},{\"locus\":\"core:face_12_01\",\"mean_dose\":1.3333333333333333},{\"locus\":\"core:growth_05\",\"mean_dose\":1.0},{\"locus\":\"core:hair_darkness_02\",\"mean_dose\":0.5},{\"locus\":\"core:hip_width_01\",\"mean_dose\":1.3333333333333333},{\"locus\":\"core:longevity_03\",\"mean_dose\":1.6666666666666667},{\"locus\":\"core:pigment_05\",\"mean_dose\":1.6666666666666667},{\"locus\":\"core:puberty_02\",\"mean_dose\":0.0},{\"locus\":\"core:pigment_13\",\"mean_dose\":0.16666666666666666},{\"locus\":\"core:growth_13\",\"mean_dose\":1.0},{\"locus\":\"core:cognition_14\",\"mean_dose\":0.5},{\"locus\":\"core:androgen_04\",\"mean_dose\":1.6666666666666667},{\"locus\":\"core:bald_01\",\"mean_dose\":1.1666666666666667},{\"locus\":\"core:cognition_07\",\"mean_dose\":1.5},{\"locus\":\"core:disease_liability_01\",\"mean_dose\":0.5},{\"locus\":\"core:face_07_01\",\"mean_dose\":1.1666666666666667},{\"locus\":\"core:grey_01\",\"mean_dose\":1.0},{\"locus\":\"core:growth_06\",\"mean_dose\":0.8333333333333334},{\"locus\":\"core:longevity_04\",\"mean_dose\":1.0},{\"locus\":\"core:muscle_01\",\"mean_dose\":0.16666666666666666},{\"locus\":\"core:pigment_06\",\"mean_dose\":0.6666666666666666},{\"locus\":\"core:puberty_03\",\"mean_dose\":0.8333333333333334},{\"locus\":\"core:pigment_14\",\"mean_dose\":0.8333333333333334},{\"locus\":\"core:growth_14\",\"mean_dose\":0.3333333333333333},{\"locus\":\"core:cognition_15\",\"mean_dose\":0.6666666666666666},{\"locus\":\"core:androgen_05\",\"mean_dose\":0.8333333333333334},{\"locus\":\"core:bald_02\",\"mean_dose\":1.1666666666666667},{\"locus\":\"core:body_hair_01\",\"mean_dose\":0.6666666666666666},{\"locus\":\"core:cognition_08\",\"mean_dose\":0.6666666666666666},{\"locus\":\"core:disease_liability_02\",\"mean_dose\":0.5},{\"locus\":\"core:face_02_01\",\"mean_dose\":0.8333333333333334},{\"locus\":\"core:face_10_01\",\"mean_dose\":1.1666666666666667},{\"locus\":\"core:grey_02\",\"mean_dose\":0.8333333333333334},{\"locus\":\"core:growth_07\",\"mean_dose\":1.3333333333333333},{\"locus\":\"core:hair_density_01\",\"mean_dose\":1.1666666666666667},{\"locus\":\"core:longevity_05\",\"mean_dose\":0.6666666666666666},{\"locus\":\"core:muscle_02\",\"mean_dose\":1.3333333333333333},{\"locus\":\"core:pigment_07\",\"mean_dose\":0.8333333333333334},{\"locus\":\"core:puberty_04\",\"mean_dose\":1.3333333333333333},{\"locus\":\"core:shoulder_width_01\",\"mean_dose\":0.6666666666666666},{\"locus\":\"core:pigment_15\",\"mean_dose\":1.3333333333333333},{\"locus\":\"core:growth_15\",\"mean_dose\":1.6666666666666667},{\"locus\":\"core:cognition_16\",\"mean_dose\":2.0},{\"locus\":\"core:androgen_06\",\"mean_dose\":1.3333333333333333},{\"locus\":\"core:cognition_01\",\"mean_dose\":0.5},{\"locus\":\"core:disease_liability_03\",\"mean_dose\":1.6666666666666667},{\"locus\":\"core:face_05_01\",\"mean_dose\":0.3333333333333333},{\"locus\":\"core:fat_distribution_01\",\"mean_dose\":1.1666666666666667},{\"locus\":\"core:growth_08\",\"mean_dose\":0.16666666666666666},{\"locus\":\"core:hair_density_02\",\"mean_dose\":1.1666666666666667},{\"locus\":\"core:leg_ratio_01\",\"mean_dose\":1.0},{\"locus\":\"core:longevity_06\",\"mean_dose\":0.5},{\"locus\":\"core:pigment_08\",\"mean_dose\":1.3333333333333333},{\"locus\":\"core:puberty_05\",\"mean_dose\":1.1666666666666667},{\"locus\":\"core:pigment_16\",\"mean_dose\":0.3333333333333333},{\"locus\":\"core:growth_16\",\"mean_dose\":0.6666666666666666},{\"locus\":\"core:cognition_09\",\"mean_dose\":1.3333333333333333},{\"locus\":\"core:androgen_07\",\"mean_dose\":0.8333333333333334},{\"locus\":\"core:beard_01\",\"mean_dose\":1.1666666666666667},{\"locus\":\"core:cognition_02\",\"mean_dose\":0.16666666666666666},{\"locus\":\"core:disease_liability_04\",\"mean_dose\":1.0},{\"locus\":\"core:estrogen_01\",\"mean_dose\":1.3333333333333333},{\"locus\":\"core:face_08_01\",\"mean_dose\":0.5},{\"locus\":\"core:growth_01\",\"mean_dose\":0.5},{\"locus\":\"core:longevity_07\",\"mean_dose\":1.6666666666666667},{\"locus\":\"core:pigment_01\",\"mean_dose\":1.3333333333333333},{\"locus\":\"core:puberty_06\",\"mean_dose\":1.0},{\"locus\":\"core:pigment_09\",\"mean_dose\":1.3333333333333333},{\"locus\":\"core:growth_09\",\"mean_dose\":1.5},{\"locus\":\"core:cognition_10\",\"mean_dose\":0.6666666666666666},{\"locus\":\"core:androgen_08\",\"mean_dose\":0.8333333333333334},{\"locus\":\"core:beard_02\",\"mean_dose\":0.16666666666666666},{\"locus\":\"core:breast_01\",\"mean_dose\":0.6666666666666666},{\"locus\":\"core:cognition_03\",\"mean_dose\":1.0},{\"locus\":\"core:disease_liability_05\",\"mean_dose\":0.16666666666666666},{\"locus\":\"core:estrogen_02\",\"mean_dose\":1.6666666666666667},{\"locus\":\"core:face_03_01\",\"mean_dose\":0.8333333333333334},{\"locus\":\"core:face_11_01\",\"mean_dose\":1.0},{\"locus\":\"core:growth_02\",\"mean_dose\":2.0},{\"locus\":\"core:head_size_01\",\"mean_dose\":1.8333333333333333},{\"locus\":\"core:longevity_08\",\"mean_dose\":1.5},{\"locus\":\"core:pigment_02\",\"mean_dose\":1.3333333333333333},{\"locus\":\"core:skin_age_01\",\"mean_dose\":0.3333333333333333},{\"locus\":\"core:pigment_10\",\"mean_dose\":1.0},{\"locus\":\"core:growth_10\",\"mean_dose\":0.8333333333333334},{\"locus\":\"core:cognition_11\",\"mean_dose\":1.5}]}\n"
    );
    let alone = r#"{"format_version":5,"start":{"world_seed":"0000000000000002","founder_tag":"test:alone","world_params":{"hereditary_diseases":false,"nutrition":[]},"founders":[{"born":-3144960000,"sex":"female","alleles":[]}],"couples":[],"capacity":null,"pool":null},"until":1886975999}"#;
    let gone = run_scenario(&dir, alone, "alone");
    assert!(
        error_of(&["measure", arg(&gone)]).starts_with("error: no_living: "),
        "nobody lives"
    );
}

/// The pedigree (HANDOFF §9 steps 5 and 10): `simagent export` writes every person of a
/// run with its biological and social parents, its birth and death, household, carer and
/// hereditary diseases, every partnership with its end and kinship, and every household
/// into `pedigree.json`, each person with its first name and surname (ADR-0021 section 3).
/// The dummy scenario after thirty years: the golden `tests/golden/dummy_export.json` from
/// tools/golden_reference.py (its pedigree is `dummy_pedigree.json`, the core's).
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
    let expected =
        text(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/dummy_export.json"));
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
    replace_in(&log, r#""core_version":46"#, r#""core_version":47"#);
    assert_eq!(
        error_of(&["run", "--replay", arg(&log), "--out", arg(&out)]),
        "error: replay_foreign_core_version: the input log is of core version 47, this is 46"
    );
    replace_in(&log, r#""core_version":47"#, r#""core_version":46"#);
    replace_in(&log, "b1f5c0054302363d", "c3f04a38869c20d7");
    assert_eq!(
        error_of(&["run", "--replay", arg(&log), "--out", arg(&out)]),
        "error: replay_foreign_data_hash: the input log has data hash c3f04a38869c20d7, this is b1f5c0054302363d"
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
    // A manifest of another format version is not this program's.
    let foreign = dir.join("foreign.json");
    fs::write(
        &foreign,
        text(&manifest_path).replacen(r#""format_version":5"#, r#""format_version":6"#, 1),
    )
    .expect("writes");
    assert_eq!(
        error_of(&[
            "run",
            "--replay",
            arg(&original.join("input.ndjson")),
            "--manifest",
            arg(&foreign),
            "--out",
            arg(&dir.join("foreign_out")),
        ]),
        "error: manifest_invalid: not a manifest of format version 5"
    );
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
    // `Start` with its three households, then three birthdays a year.
    assert_eq!(text(&out.join("events.ndjson")).lines().count(), 7 + 4 * 3);
}

/// The manifest also carries the log hash of the original run; a replay compares it at
/// the end (ADR-0004 test 1: the same year hashes and the same log hash).
#[test]
fn replay_compares_the_log_hash() {
    let dir = scratch("log_hash");
    let original = run_scenario(&dir, TINY_SCENARIO, "tiny");
    let manifest_path = original.join("manifest.json");
    replace_in(&manifest_path, "6d23128b21469cb5", "6bed3b6ab04c2db1");
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
        "error: replay_diverged: the log hash is 6d23128b21469cb5, the manifest says 6bed3b6ab04c2db1"
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

    // A census of an older version: the three columns only.
    let old = text(&out.join("census.csv"))
        .lines()
        .map(|line| line.split(',').take(3).collect::<Vec<_>>().join(",") + "\n")
        .collect::<String>();
    fs::write(out.join("census.csv"), &old).expect("writes");
    assert_eq!(
        resume(),
        "error: run_dir_inconsistent: census.csv does not start with the header of this version"
    );
    unchanged("census.csv", old.as_bytes());
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

    // A manifest of another format version.
    replace_in(
        &out.join("manifest.json"),
        r#""format_version":5"#,
        r#""format_version":6"#,
    );
    assert_eq!(
        resume(),
        "error: manifest_invalid: not a manifest of format version 5"
    );
    restore();

    // A scenario file of another format version.
    replace_in(
        &out.join("scenario.resolved.json"),
        r#""format_version":5"#,
        r#""format_version":6"#,
    );
    assert_eq!(
        resume(),
        "error: scenario_invalid: not a scenario of format version 5"
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
        census(&["0,1,31449600,1", "1,1,31449600,1", "2,1,31449600,1"])
    );
    assert_eq!(text(&dir.join("events.ndjson")).lines().count(), 6);
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
    let founder =
        r#"{"born":-1,"sex":"female","alleles":[{"locus":"core:disease_ar","copies":[0,1]}]}"#;
    assert!(lines[1].contains(founder));
    for positional in [
        r#"[-1,"female",{"maternal":"wild","paternal":"dummy"}]"#,
        r#"{"born":-1,"sex":"female","alleles":[["core:disease_ar",[0,1]]]}"#,
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
        r#"{"born":-1,"alleles":[{"locus":"core:disease_ar","copies":[0,1]}]}"#,
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
        replay(&pristine.replacen(r#""format_version":5"#, r#""format_version":6"#, 1)),
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
        "error: scenario_invalid: not a scenario of format version 5"
    );
    // Format 4 is no longer read: under format 5 founders draw the alleles they do not name.
    assert_eq!(
        run(&TINY_SCENARIO.replacen(r#""format_version":5"#, r#""format_version":4"#, 1)),
        "error: scenario_invalid: a scenario of format version 4 is no longer read (format 5 draws the alleles founders do not name)"
    );
    // `pool` and `capacity` are required (`null` for none).
    for missing in [r#","pool":null"#, r#""capacity":null,"#] {
        assert_eq!(
            run(&TINY_SCENARIO.replacen(missing, "", 1)),
            "error: scenario_invalid: not a scenario of format version 5",
            "{missing}"
        );
    }
    // The built-in data hold the pool: the scenario resolves, and the core rejects a
    // start with founders and a pool.
    assert_eq!(
        run(&TINY_SCENARIO.replacen(
            r#""pool":null"#,
            r#""pool":{"n_start":10,"immigration":false}"#,
            1
        )),
        "error: input_rejected: start_pool_and_founders"
    );
    assert_eq!(
        run(&TINY_SCENARIO.replacen(r#""format_version":5"#, r#""format_version":6"#, 1)),
        "error: scenario_invalid: not a scenario of format version 5"
    );
    assert_eq!(
        run(&TINY_SCENARIO.replacen(r#"{"born":-1,"sex":"female","#, r#"{"born":-1,"#, 1)),
        "error: scenario_invalid: not a scenario of format version 5"
    );
    assert_eq!(
        run(&TINY_SCENARIO.replacen(
            r#"{"locus":"core:disease_ar","copies":[0,1]}"#,
            r#"["core:disease_ar",[0,1]]"#,
            1
        )),
        "error: scenario_invalid: not a scenario of format version 5"
    );
    assert_eq!(
        run(&TINY_SCENARIO.replacen(r#""copies":[0,1]"#, r#""copies":[0,256]"#, 1)),
        "error: scenario_invalid: not a scenario of format version 5"
    );
    // Alleles the genome map does not allow are a rejected start (genetics
    // specification 2.1): a value other than 0 and 1, a locus it does not have.
    assert_eq!(
        run(&TINY_SCENARIO.replacen(r#""copies":[0,1]"#, r#""copies":[0,2]"#, 1)),
        "error: input_rejected: founder_copies"
    );
    assert_eq!(
        run(&TINY_SCENARIO.replacen(
            r#""locus":"core:disease_ar""#,
            r#""locus":"core:nothing""#,
            1
        )),
        "error: input_rejected: founder_unknown_locus"
    );
    assert_eq!(
        run(&TINY_SCENARIO.replacen("00000000000000ff", "ff", 1)),
        "error: scenario_invalid: not a scenario of format version 5"
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
    assert_eq!(bytes[9], 46, "the core version in the header of the save");
    bytes[9] = 47;
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
    assert_eq!(package["format_version"], 5);
    assert_eq!(package["kind"], "replay_diverged");
    assert_eq!(package["core_version"], manifest["core_version"]);
    assert_eq!(package["data_hash"], manifest["data_hash"]);
    assert_eq!(package["world_seed"], manifest["world_seed"]);
    // The last closed year is the one that differs, with the hash the run produced.
    assert_eq!(package["checkpoint"]["year"], 3);
    assert_eq!(package["checkpoint"]["hash"], true_3.as_str());
    assert_eq!(package["expected"][2]["hash"], true_2.as_str());
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
    // A package whose replay stops before the year that differed does not pass: fewer
    // year hashes than expected.
    let stopped = dir.join("stopped.json");
    let mut short: Value = serde_json::from_str(&text(&package_path)).expect("json");
    let end_of_2 = short["at"]["t"].as_i64().expect("t") - 364 * 86_400;
    short["at"]["t"] = Value::from(end_of_2);
    short["at"]["year"] = Value::from(2);
    fs::write(&stopped, short.to_string()).expect("writes");
    assert!(
        error_of(&["repro", arg(&stopped)])
            .starts_with("error: repro_reproduced: the replay has 3 year hashes")
    );
    // A diverged replay without its expected hashes is no package.
    let mut bare: Value = serde_json::from_str(&text(&package_path)).expect("json");
    bare.as_object_mut().expect("object").remove("expected");
    fs::write(&stopped, bare.to_string()).expect("writes");
    assert!(error_of(&["repro", arg(&stopped)]).starts_with("error: repro_invalid:"));
    // A package of another format version.
    fs::write(
        &stopped,
        text(&package_path).replacen("\"format_version\":5", "\"format_version\":6", 1),
    )
    .expect("writes");
    assert_eq!(
        error_of(&["repro", arg(&stopped)]),
        "error: repro_invalid: not a repro package of format 5"
    );
    // An unknown field is no package.
    replace_in(
        &package_path,
        "\"format_version\":5",
        "\"format_version\":5,\"x\":0",
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

/// `census.csv` of the repository's data (ADR-0020 section 7): the three columns of every
/// run and `body_evaluated` as given, then the clamp counts of the 21 parameters of the
/// contract with unit `z` or `cm` and the held curve; then births, arrivals, emigrations
/// and the deaths per cause (ADR-0021, addendum 2026-10-21): senescence, accident and the
/// five lethal diseases of `data/calibration/genetics.json` in its order; then the
/// switch, the affected per disease (all six in the data's order), the heights of the
/// women and men of at least 25 and of exactly 25 (count and mean in mm) and the mean
/// inbreeding of the newborns. Here all 0: a founder of `Start` is no arrival, the switch
/// is off, the founder is a carrier under 25 and nobody is born.
fn census(rows: &[&str]) -> String {
    let header = "year,living,lived_secs,body_evaluated,clamped_height_cm,clamped_leg_ratio_z,\
                  clamped_head_size_z,clamped_body_fat_z,clamped_fat_distribution_z,\
                  clamped_muscle_z,clamped_shoulder_width_z,clamped_chest_depth_z,\
                  clamped_hip_width_z,clamped_face_axis_01,clamped_face_axis_02,\
                  clamped_face_axis_03,clamped_face_axis_04,clamped_face_axis_05,\
                  clamped_face_axis_06,clamped_face_axis_07,clamped_face_axis_08,\
                  clamped_face_axis_09,clamped_face_axis_10,clamped_face_axis_11,\
                  clamped_face_axis_12,curve_clamped,births,arrivals,emigrations,\
                  deaths.core:senescence,deaths.core:accident,deaths.core:disease_ar,\
                  deaths.core:disease_ad_late,deaths.core:disease_ad_partial,\
                  deaths.core:disease_xr,deaths.core:disease_liability,diseases_on,\
                  affected.core:disease_ar,affected.core:disease_ad_late,\
                  affected.core:disease_ad_partial,affected.core:disease_xr,\
                  affected.core:disease_mt,affected.core:disease_liability,adults_female,\
                  adult_height_mm_female,adults_male,adult_height_mm_male,age25_female,\
                  age25_height_mm_female,age25_male,age25_height_mm_male,inbreeding_e4";
    let mut text = format!("{header}\n");
    for row in rows {
        text.push_str(row);
        text.push_str(&",0".repeat(48));
        text.push('\n');
    }
    text
}

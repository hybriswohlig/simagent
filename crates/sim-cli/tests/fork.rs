//! `simagent fork` as a process (ADR-0001 section 6, ADR-0004 section 8). A fork is a
//! new run from t0 with the inputs of the parent before the fork point; the year hashes
//! before the fork point must equal the parent's. Overrides arrive with the first world
//! parameter (step 5).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};
use sim_core::log::{LogEntry, log_hash};

const Y: i64 = 31_449_600;

/// Three founders, ten years.
const TEN_YEARS: &str = r#"{
  "format_version": 1,
  "start": {
    "world_seed": "0000000000000001",
    "founder_tag": "test:cli",
    "world_params": {"hereditary_diseases": false},
    "founders": [
      {"born": -943488000, "sex": "female", "genotype": {"maternal": "wild", "paternal": "dummy"}},
      {"born": -794880000, "sex": "male", "genotype": {"maternal": "dummy", "paternal": "wild"}},
      {"born": -1, "sex": "male", "genotype": {"maternal": "dummy", "paternal": "dummy"}}
    ]
  },
  "until": 314495999
}"#;

/// The files of a run directory besides the manifest.
const FILES: [&str; 5] = [
    "scenario.resolved.json",
    "input.ndjson",
    "events.ndjson",
    "census.csv",
    "state.save",
];

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("fork")
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

/// Runs simagent: `Ok` or the first line of stderr.
fn simagent(args: &[&str]) -> Result<(), String> {
    let output = Command::new(env!("CARGO_BIN_EXE_simagent"))
        .args(args)
        .output()
        .expect("runs simagent");
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8(output.stderr).expect("UTF-8");
    Err(stderr.lines().next().unwrap_or("").to_owned())
}

/// The ten-year run in `dir/parent`.
fn parent(dir: &Path) -> PathBuf {
    fs::create_dir_all(dir).expect("creates the directory");
    let file = dir.join("scenario.json");
    fs::write(&file, TEN_YEARS).expect("writes the scenario");
    let out = dir.join("parent");
    assert_eq!(
        simagent(&["run", "--scenario", arg(&file), "--out", arg(&out)]),
        Ok(())
    );
    out
}

fn fork(parent: &Path, from_year: &str, out: &Path) -> Result<(), String> {
    simagent(&[
        "fork",
        arg(parent),
        "--from-year",
        from_year,
        "--out",
        arg(out),
    ])
}

/// The override block "hereditary diseases on" as a file in `dir`.
fn override_file(dir: &Path, block: &str) -> PathBuf {
    let file = dir.join("override.json");
    fs::write(&file, block).expect("writes the override");
    file
}

const SWITCH_ON: &str = r#"[{"core:hereditary_diseases":true}]"#;

fn fork_with(parent: &Path, from_year: &str, block: &Path, out: &Path) -> Result<(), String> {
    simagent(&[
        "fork",
        arg(parent),
        "--from-year",
        from_year,
        "--override",
        arg(block),
        "--out",
        arg(out),
    ])
}

fn hashes(manifest: &Value) -> Vec<String> {
    (0..manifest["year_hashes"].as_array().expect("array").len())
        .map(|year| year_hash(manifest, year))
        .collect()
}

/// Changes the age in the birthday entry of sim 1 in `year` of the run in `dir` and
/// writes the log hash of the changed events into its manifest: a run directory that
/// is consistent in itself and is not the run of its input log.
fn tamper_with_the_birthday_of(dir: &Path, year: i64) {
    let events = dir.join("events.ndjson");
    let at = format!(r#""t":{},"subjects":[{{"sim":1}}]"#, year * Y);
    let lines: Vec<String> = text(&events)
        .lines()
        .map(|line| {
            if line.contains(&at) {
                assert!(line.contains(r#""age_years":"#), "{line}");
                line.replace(r#""age_years":"#, r#""age_years":1"#)
            } else {
                line.to_owned()
            }
        })
        .collect();
    let before = manifest(dir)["log_hash"].as_str().expect("hex").to_owned();
    let entries: Vec<LogEntry> = lines
        .iter()
        .map(|line| serde_json::from_str(line).expect("an entry"))
        .collect();
    let after = format!("{:016x}", log_hash(&entries));
    assert_ne!(before, after, "an entry of year {year} changed");
    fs::write(&events, lines.join("\n") + "\n").expect("writes");
    replace_in(&dir.join("manifest.json"), &before, &after);
}

fn manifest(run: &Path) -> Value {
    serde_json::from_str(&text(&run.join("manifest.json"))).expect("json")
}

fn replace_in(path: &Path, from: &str, to: &str) {
    let before = text(path);
    assert!(before.contains(from), "{from:?} not in {}", path.display());
    fs::write(path, before.replacen(from, to, 1)).expect("writes");
}

fn year_hash(manifest: &Value, year: usize) -> String {
    manifest["year_hashes"][year]["hash"]
        .as_str()
        .expect("hex")
        .to_owned()
}

/// ADR-0001 `fork_without_override_equals_original`: all year hashes and the log hash
/// are those of the parent, here every file of the run directory; the manifest differs
/// by the block `fork` only (ADR-0004 section 8).
#[test]
fn fork_without_override_equals_original() {
    let dir = scratch("equal");
    let parent = parent(&dir);
    let parent_manifest = manifest(&parent);
    assert_eq!(
        parent_manifest["year_hashes"]
            .as_array()
            .expect("array")
            .len(),
        10
    );
    // Year 9 is the last year that starts inside the parent: the latest fork point.
    for from_year in [1_u64, 5, 9] {
        let out = dir.join(format!("fork{from_year}"));
        assert_eq!(fork(&parent, &from_year.to_string(), &out), Ok(()));
        for file in FILES {
            assert!(
                fs::read(parent.join(file)).expect("reads")
                    == fs::read(out.join(file)).expect("reads"),
                "{file} differs in the fork from year {from_year}"
            );
        }
        let mut forked = manifest(&out);
        let block = forked
            .as_object_mut()
            .expect("an object")
            .remove("fork")
            .expect("the manifest of a fork has the block fork");
        assert_eq!(
            block,
            json!({
                "parent_log_hash": parent_manifest["log_hash"],
                "from_year": from_year,
                "dropped_inputs": 0
            })
        );
        assert_eq!(forked, parent_manifest, "from year {from_year}");
    }
    assert!(parent_manifest.get("fork").is_none(), "a run is no fork");
}

/// ADR-0001 section 6: X >= 1, and year X starts at the end of the parent at the latest:
/// `T_X` is a second the parent has run. A refused fork writes nothing.
#[test]
fn the_fork_point_lies_inside_the_parent() {
    let dir = scratch("range");
    let parent = parent(&dir);
    let out = dir.join("out");
    assert_eq!(
        fork(&parent, "0", &out),
        Err(
            "error: fork_point_out_of_range: from-year 0: a fork starts at year 1 or later"
                .to_owned()
        )
    );
    // The parent ends at the last second of year 9; year 10 starts one second later.
    for year in ["10", "11"] {
        assert_eq!(
            fork(&parent, year, &out),
            Err(format!(
                "error: fork_point_out_of_range: from-year {year}: the parent run ends at {}, before year {year} starts",
                10 * Y - 1
            ))
        );
    }
    assert_eq!(
        fork(&parent, "9223372036854775807", &out),
        Err(format!(
            "error: fork_point_out_of_range: from-year 9223372036854775807: the parent run ends at {}, before year 9223372036854775807 starts",
            10 * Y - 1
        ))
    );
    assert!(!out.exists(), "a refused fork writes nothing");
    // A parent that ended at the first second of year 3 can fork from year 3, not from
    // year 4; one second shorter, not from year 3 either.
    let scenario = dir.join("scenario.json");
    let shorter = dir.join("shorter");
    let until = (3 * Y - 1).to_string();
    assert_eq!(
        simagent(&[
            "run",
            "--scenario",
            arg(&scenario),
            "--out",
            arg(&shorter),
            "--until",
            &until
        ]),
        Ok(())
    );
    assert_eq!(fork(&shorter, "2", &dir.join("shorter2")), Ok(()));
    assert_eq!(
        fork(&shorter, "3", &out),
        Err(format!(
            "error: fork_point_out_of_range: from-year 3: the parent run ends at {}, before year 3 starts",
            3 * Y - 1
        ))
    );
    let short = dir.join("short");
    let until = (3 * Y).to_string();
    assert_eq!(
        simagent(&[
            "run",
            "--scenario",
            arg(&scenario),
            "--out",
            arg(&short),
            "--until",
            &until
        ]),
        Ok(())
    );
    assert_eq!(fork(&short, "3", &dir.join("short3")), Ok(()));
    assert_eq!(
        fork(&short, "4", &out),
        Err(format!(
            "error: fork_point_out_of_range: from-year 4: the parent run ends at {}, before year 4 starts",
            3 * Y
        ))
    );
}

/// ADR-0001 section 6: the year hashes of the years 0 to X - 1 must equal those in the
/// manifest of the parent, else `fork_diverged_before_fork_point`. Later years are not
/// compared: there the fork may differ.
#[test]
fn fork_compares_the_years_before_the_fork_point() {
    let dir = scratch("diverged");
    let parent = parent(&dir);
    let path = parent.join("manifest.json");
    let pristine = manifest(&parent);
    // Year 7 lies after the fork point 5: no finding.
    replace_in(&path, &year_hash(&pristine, 7), "fedcba9876543210");
    assert_eq!(fork(&parent, "5", &dir.join("later")), Ok(()));
    assert_eq!(
        fork(&parent, "7", &dir.join("at7")),
        Ok(()),
        "year 7 is not before year 7"
    );
    assert_eq!(
        fork(&parent, "8", &dir.join("at8")),
        Err(format!(
            "error: fork_diverged_before_fork_point: year 7 has hash {}, the manifest of the parent says fedcba9876543210",
            year_hash(&pristine, 7)
        ))
    );
    // Years 3 and 4 differ: the fork names the first and ends there.
    replace_in(&path, &year_hash(&pristine, 3), "0123456789abcdef");
    replace_in(&path, &year_hash(&pristine, 4), "0123456789abcdee");
    let out = dir.join("out");
    assert_eq!(
        fork(&parent, "5", &out),
        Err(format!(
            "error: fork_diverged_before_fork_point: year 3 has hash {}, the manifest of the parent says 0123456789abcdef",
            year_hash(&pristine, 3)
        ))
    );
    assert_eq!(text(&out.join("census.csv")).lines().count(), 1 + 4);
    assert!(
        !out.join("manifest.json").exists(),
        "a broken fork has no manifest"
    );
}

/// ADR-0001 section 6: another core version or another data hash than the parent's ends
/// the fork with a message; so does a parent without an orderly end.
#[test]
fn fork_rejects_a_foreign_or_incomplete_parent() {
    let dir = scratch("foreign");
    let parent = parent(&dir);
    let out = dir.join("out");
    let log = parent.join("input.ndjson");
    replace_in(&log, r#""core_version":3"#, r#""core_version":4"#);
    assert_eq!(
        fork(&parent, "5", &out),
        Err(
            "error: fork_foreign_core_version: the parent run is of core version 4, this is 3"
                .to_owned()
        )
    );
    replace_in(&log, r#""core_version":4"#, r#""core_version":3"#);
    replace_in(&log, "2d06800538d394c2", "2d06800538d394c3");
    assert_eq!(
        fork(&parent, "5", &out),
        Err("error: fork_foreign_data_hash: the parent run has data hash 2d06800538d394c3, this is 2d06800538d394c2".to_owned())
    );
    replace_in(&log, "2d06800538d394c3", "2d06800538d394c2");
    assert_eq!(fork(&parent, "5", &dir.join("restored")), Ok(()));

    // The resolved scenario must start as the input log does.
    let scenario_path = parent.join("scenario.resolved.json");
    replace_in(&scenario_path, "0000000000000001", "0000000000000002");
    assert_eq!(
        fork(&parent, "5", &out),
        Err("error: run_dir_inconsistent: the resolved scenario of the parent does not start as its input log".to_owned())
    );
    replace_in(&scenario_path, "0000000000000002", "0000000000000001");
    fs::write(&scenario_path, "{}\n").expect("writes");
    assert_eq!(
        fork(&parent, "5", &out),
        Err("error: scenario_invalid: not a scenario of format version 1".to_owned())
    );
    fs::copy(
        dir.join("restored").join("scenario.resolved.json"),
        &scenario_path,
    )
    .expect("copies");
    assert_eq!(fork(&parent, "5", &dir.join("restored_again")), Ok(()));

    // The input log of the parent begins with its start.
    let log_text = text(&log);
    let mut lines: Vec<&str> = log_text.lines().collect();
    let block = r#"{"t":-1,"seq":1,"input":{"fork_override":{"changes":[{"core:hereditary_diseases":true}]}}}"#;
    lines.insert(1, block);
    fs::write(&log, lines.join("\n") + "\n").expect("writes");
    assert_eq!(
        fork(&parent, "5", &out),
        Err("error: input_log_invalid: the first input is not a start".to_owned())
    );
    fs::write(&log, &log_text).expect("writes");

    // The manifest must belong to the input log and have a hash for every year before
    // the fork point.
    let manifest_path = parent.join("manifest.json");
    let whole = text(&manifest_path);
    let mut short: Value = serde_json::from_str(&whole).expect("json");
    short["year_hashes"]
        .as_array_mut()
        .expect("array")
        .truncate(4);
    fs::write(
        &manifest_path,
        serde_json::to_string(&short).expect("json") + "\n",
    )
    .expect("writes");
    assert_eq!(
        fork(&parent, "5", &out),
        Err("error: manifest_invalid: the manifest of the parent has 4 year hashes, the fork point needs 5".to_owned())
    );
    assert_eq!(fork(&parent, "4", &dir.join("four")), Ok(()));
    fs::write(&manifest_path, &whole).expect("writes");
    // Review finding: the manifest names the parent in the fork, so it must describe the
    // run it lies in: version, data hash, seed and the hash of its log.
    let foreign = "error: manifest_invalid: the manifest of the parent names another core version, data hash or world seed than its run";
    for (from, to) in [
        (r#""core_version":3"#, r#""core_version":4"#),
        (
            r#""data_hash":"2d06800538d394c2""#,
            r#""data_hash":"2d06800538d394c3""#,
        ),
        (
            r#""world_seed":"0000000000000001""#,
            r#""world_seed":"0000000000000002""#,
        ),
    ] {
        replace_in(&manifest_path, from, to);
        assert_eq!(fork(&parent, "5", &out), Err(foreign.to_owned()), "{to}");
        fs::write(&manifest_path, &whole).expect("writes");
    }
    let log_hash = manifest(&parent)["log_hash"]
        .as_str()
        .expect("hex")
        .to_owned();
    replace_in(&manifest_path, &log_hash, "0123456789abcdef");
    assert_eq!(
        fork(&parent, "5", &out),
        Err(format!(
            "error: manifest_invalid: the manifest of the parent has log hash 0123456789abcdef, its events.ndjson gives {log_hash}"
        ))
    );
    fs::write(&manifest_path, &whole).expect("writes");
    let events_path = parent.join("events.ndjson");
    let events = text(&events_path);
    let without_last: Vec<&str> = events.lines().collect();
    fs::write(
        &events_path,
        without_last[..without_last.len() - 1].join("\n") + "\n",
    )
    .expect("writes");
    let message = fork(&parent, "5", &out).expect_err("events without their last line");
    assert!(
        message.starts_with(&format!(
            "error: manifest_invalid: the manifest of the parent has log hash {log_hash}, its events.ndjson gives "
        )),
        "{message}"
    );
    fs::write(&events_path, &events).expect("writes");
    assert_eq!(fork(&parent, "5", &dir.join("whole_again")), Ok(()));
    let end = (10 * Y - 1).to_string();
    replace_in(
        &manifest_path,
        &format!(r#""closed_through":{end}"#),
        r#""closed_through":5"#,
    );
    assert_eq!(
        fork(&parent, "5", &out),
        Err(format!(
            "error: manifest_invalid: the manifest of the parent ends at 5, its input log at {end}"
        ))
    );
    fs::write(&manifest_path, "{}\n").expect("writes");
    assert_eq!(
        fork(&parent, "5", &out),
        Err("error: manifest_invalid: not a manifest".to_owned())
    );
    fs::remove_file(&manifest_path).expect("removes");
    let message = fork(&parent, "5", &out).expect_err("no manifest");
    assert!(message.starts_with("error: io: "), "{message}");
    // A log without an input, and one whose start is not stamped before the fork point.
    fs::write(&manifest_path, &whole).expect("writes");
    let pristine_log = text(&log);
    let log_lines: Vec<&str> = pristine_log.lines().collect();
    fs::write(&log, [log_lines[0], log_lines[2]].join("\n") + "\n").expect("writes");
    assert_eq!(
        fork(&parent, "5", &out),
        Err("error: input_log_invalid: the input log has no input".to_owned())
    );
    let late_start = log_lines[1].replacen(r#""t":-1"#, &format!(r#""t":{}"#, 5 * Y - 1), 1);
    fs::write(
        &log,
        [log_lines[0], &late_start, log_lines[2]].join("\n") + "\n",
    )
    .expect("writes");
    assert_eq!(
        fork(&parent, "5", &out),
        Err(
            "error: input_log_invalid: the input log has no input before the fork point".to_owned()
        )
    );
    fs::write(&log, &pristine_log).expect("writes");
    fs::remove_file(&manifest_path).expect("removes");
    // A run that was not ended in order has no end line.
    let lines = text(&log);
    let without_end: Vec<&str> = lines.lines().take(2).collect();
    fs::write(&log, without_end.join("\n") + "\n").expect("writes");
    assert_eq!(
        fork(&parent, "5", &out),
        Err(r#"error: input_log_invalid: the last line is not {"end":t}"#.to_owned())
    );
    assert!(!out.exists(), "a refused fork writes nothing");
}

/// ADR-0001 section 6: the inputs of the parent with a stamp before `T_X - 1 s` are
/// taken over, the others are dropped and counted in the manifest. The dummy world
/// accepts no input after `Start`, so the second record here is one the parent never
/// applied: dropped it does no harm, taken over it is rejected.
#[test]
fn inputs_from_the_fork_point_on_are_dropped() {
    let dir = scratch("dropped");
    let parent = parent(&dir);
    let log = parent.join("input.ndjson");
    let pristine = text(&log);
    let lines: Vec<&str> = pristine.lines().collect();
    assert_eq!(lines.len(), 3, "header, start, end");
    let with_second = |t: i64| {
        let second = lines[1].replacen(r#""t":-1,"seq":1"#, &format!(r#""t":{t},"seq":2"#), 1);
        assert_ne!(second, lines[1]);
        fs::write(
            &log,
            [lines[0], lines[1], &second, lines[2]].join("\n") + "\n",
        )
        .expect("writes");
    };
    // Stamp T_5 - 1 s: it would take effect from T_5 on, so a fork from year 5 drops it.
    with_second(5 * Y - 1);
    let out = dir.join("at_the_point");
    assert_eq!(fork(&parent, "5", &out), Ok(()));
    assert_eq!(manifest(&out)["fork"]["dropped_inputs"], json!(1));
    assert_eq!(
        text(&out.join("input.ndjson")),
        pristine,
        "the fork has the start only"
    );
    assert_eq!(fork(&parent, "1", &dir.join("early")), Ok(()));
    assert_eq!(
        manifest(&dir.join("early"))["fork"]["dropped_inputs"],
        json!(1)
    );
    // A fork from year 6 takes it over; the world rejects a second start.
    assert_eq!(
        fork(&parent, "6", &dir.join("later")),
        Err(format!(
            "error: replay_input_rejected: input 2 at t {} was rejected: already_started",
            5 * Y - 1
        ))
    );
    // One second earlier it lies before the fork point of year 5.
    with_second(5 * Y - 2);
    assert_eq!(
        fork(&parent, "5", &dir.join("before")),
        Err(format!(
            "error: replay_input_rejected: input 2 at t {} was rejected: already_started",
            5 * Y - 2
        ))
    );
}

/// Review finding: a fork that is continued stays a fork. `--resume` writes the manifest
/// again and keeps the block `fork`.
#[test]
fn a_resumed_fork_keeps_its_fork_block() {
    let dir = scratch("resumed");
    let parent = parent(&dir);
    let out = dir.join("fork");
    assert_eq!(fork(&parent, "5", &out), Ok(()));
    let block = manifest(&out)["fork"].clone();
    assert_eq!(block["from_year"], json!(5));
    let later = (11 * Y - 1).to_string();
    assert_eq!(
        simagent(&["run", "--resume", "--out", arg(&out), "--until", &later]),
        Ok(())
    );
    let resumed = manifest(&out);
    assert_eq!(resumed["year_hashes"].as_array().expect("array").len(), 11);
    assert_eq!(resumed["fork"], block);
    // A run that is no fork gets none by a resume.
    assert_eq!(
        simagent(&["run", "--resume", "--out", arg(&parent), "--until", &later]),
        Ok(())
    );
    assert!(manifest(&parent).get("fork").is_none());
}

/// Review finding: the fork names its parent by the hash of the parent's log. A parent
/// directory with the events and the log hash of another run passes the comparison of
/// the years before the fork point. As long as there is no override and the world accepts
/// no input after the start, a fork is its parent, so its log hash must be the parent's.
#[test]
fn a_fork_has_the_log_of_its_parent() {
    let dir = scratch("provenance");
    let parent = parent(&dir);
    // Another run: the third founder is born one second earlier.
    let other_scenario = dir.join("other.json");
    fs::write(
        &other_scenario,
        TEN_YEARS.replace(r#"{"born": -1,"#, r#"{"born": -2,"#),
    )
    .expect("writes");
    let other = dir.join("other");
    assert_eq!(
        simagent(&[
            "run",
            "--scenario",
            arg(&other_scenario),
            "--out",
            arg(&other)
        ]),
        Ok(())
    );
    let (own_hash, other_hash) = (
        manifest(&parent)["log_hash"]
            .as_str()
            .expect("hex")
            .to_owned(),
        manifest(&other)["log_hash"]
            .as_str()
            .expect("hex")
            .to_owned(),
    );
    assert_ne!(own_hash, other_hash);
    fs::copy(other.join("events.ndjson"), parent.join("events.ndjson")).expect("copies");
    replace_in(&parent.join("manifest.json"), &own_hash, &other_hash);
    let out = dir.join("out");
    // The other run differs from its first entries on: the events before the fork point
    // are not those of the fork.
    let refused = fork(&parent, "5", &out).expect_err("another run");
    assert!(
        refused.starts_with(
            "error: fork_parent_inconsistent: the events before the fork point have log hash "
        ),
        "{refused}"
    );
    assert!(
        !out.join("manifest.json").exists(),
        "no manifest names a wrong parent"
    );
}

/// ADR-0001 `fork_point_hash_equals_parent` through the CLI: with the override "hereditary
/// diseases on" at year 5 the year hashes 0 to 4 are those of the parent and those from
/// year 5 on are not. The override is an input of the fork, stamped `T_5 - 1 s`; its entry
/// stands at T_5 before every event of that second.
#[test]
fn fork_point_hash_equals_parent() {
    let dir = scratch("override");
    let parent = parent(&dir);
    let block = override_file(&dir, SWITCH_ON);
    let out = dir.join("fork");
    assert_eq!(fork_with(&parent, "5", &block, &out), Ok(()));
    let (ours, theirs) = (manifest(&out), manifest(&parent));
    let (ours_hashes, theirs_hashes) = (hashes(&ours), hashes(&theirs));
    assert_eq!(ours_hashes.len(), 10);
    assert_eq!(ours_hashes[..5], theirs_hashes[..5]);
    for year in 5..10 {
        assert_ne!(ours_hashes[year], theirs_hashes[year], "year {year}");
    }
    assert_ne!(ours["log_hash"], theirs["log_hash"]);
    assert_eq!(
        ours["fork"],
        json!({
            "parent_log_hash": theirs["log_hash"],
            "from_year": 5,
            "dropped_inputs": 0
        })
    );

    let fork_stamp = 5 * Y - 1;
    let inputs = text(&out.join("input.ndjson"));
    let input_lines: Vec<&str> = inputs.lines().collect();
    assert_eq!(input_lines.len(), 4, "header, start, override, end");
    assert_eq!(
        input_lines[2],
        format!(
            r#"{{"t":{fork_stamp},"seq":2,"input":{{"fork_override":{{"changes":[{{"core:hereditary_diseases":true}}]}}}}}}"#
        )
    );
    let (ours_events, theirs_events) = (
        text(&out.join("events.ndjson")),
        text(&parent.join("events.ndjson")),
    );
    let ours_lines: Vec<&str> = ours_events.lines().collect();
    let theirs_lines: Vec<&str> = theirs_events.lines().collect();
    assert_eq!(ours_lines.len(), theirs_lines.len() + 1);
    // Four entries of the start and three birthdays in each of the years 0 to 4.
    assert_eq!(ours_lines[..19], theirs_lines[..19]);
    assert_eq!(
        ours_lines[19],
        format!(
            r#"{{"id":20,"t":{},"subjects":[],"causes":[{{"input":2}}],"payload":{{"core:param_changed":{{"change":{{"core:hereditary_diseases":true}}}}}}}}"#,
            5 * Y
        )
    );
    // The birthday of sim 1 in that second follows with the next EventId.
    assert!(
        ours_lines[20].starts_with(&format!(
            r#"{{"id":21,"t":{},"subjects":[{{"sim":1}}]"#,
            5 * Y
        )),
        "{}",
        ours_lines[20]
    );
    // The census reads only times: it is that of the parent.
    assert_eq!(
        text(&out.join("census.csv")),
        text(&parent.join("census.csv"))
    );

    // The fork replays from its own input log, with its own manifest.
    let replayed = dir.join("replayed");
    assert_eq!(
        simagent(&[
            "run",
            "--replay",
            arg(&out.join("input.ndjson")),
            "--manifest",
            arg(&out.join("manifest.json")),
            "--out",
            arg(&replayed),
        ]),
        Ok(())
    );
    for file in ["events.ndjson", "census.csv", "state.save", "input.ndjson"] {
        assert!(
            fs::read(out.join(file)).expect("reads")
                == fs::read(replayed.join(file)).expect("reads"),
            "{file} differs in the replay of the fork"
        );
    }
    assert_eq!(hashes(&manifest(&replayed)), ours_hashes);
    assert_eq!(manifest(&replayed)["log_hash"], ours["log_hash"]);

    // The stamp of the second input is compared as that of the first: a log that names
    // another sequence number or another second for the override does not replay.
    let replay_of = |name: &str, from: &str, to: &str| {
        let log = dir.join(format!("{name}.ndjson"));
        assert!(inputs.contains(from), "{from}");
        fs::write(&log, inputs.replacen(from, to, 1)).expect("writes");
        simagent(&["run", "--replay", arg(&log), "--out", arg(&dir.join(name))])
    };
    let record = format!(r#"{{"t":{fork_stamp},"seq":2,"#);
    assert_eq!(
        replay_of(
            "other_seq",
            &record,
            &format!(r#"{{"t":{fork_stamp},"seq":3,"#)
        ),
        Err(format!(
            "error: replay_stamp_mismatch: the log has t {fork_stamp} seq 3, the replay stamped t {fork_stamp} seq 2"
        ))
    );
    // A record stamped before the clock of the replay is applied where the clock stands.
    assert_eq!(
        replay_of("other_second", &record, r#"{"t":-2,"seq":2,"#),
        Err(
            "error: replay_stamp_mismatch: the log has t -2 seq 2, the replay stamped t -1 seq 2"
                .to_owned()
        )
    );
}

/// ADR-0001 section 6: an empty override block gives no input, so the fork is the parent.
#[test]
fn an_empty_override_is_no_override() {
    let dir = scratch("empty_override");
    let parent = parent(&dir);
    let block = override_file(&dir, "[]");
    let out = dir.join("fork");
    assert_eq!(fork_with(&parent, "5", &block, &out), Ok(()));
    for file in FILES {
        assert!(
            fs::read(parent.join(file)).expect("reads") == fs::read(out.join(file)).expect("reads"),
            "{file} differs in the fork with an empty override"
        );
    }
    assert_eq!(manifest(&out)["log_hash"], manifest(&parent)["log_hash"]);
}

/// The override file is a list of world parameters with their new values and nothing
/// else; what the core rejects ends the fork with the code of the core.
#[test]
fn a_bad_override_ends_the_fork() {
    let dir = scratch("bad_override");
    let parent = parent(&dir);
    let out = dir.join("fork");
    let invalid = "error: override_invalid: not a list of world parameters with their new values";
    for block in [
        "",
        "{}",
        r#"{"core:hereditary_diseases":true}"#,
        r#"[{"core:unknown":true}]"#,
        r#"[{"core:hereditary_diseases":1}]"#,
        r#"[{"core:hereditary_diseases":true,"core:hereditary_diseases":false}]"#,
        r#"{"changes":[{"core:hereditary_diseases":true}]}"#,
    ] {
        let file = override_file(&dir, block);
        assert_eq!(
            fork_with(&parent, "5", &file, &out),
            Err(invalid.to_owned()),
            "{block}"
        );
        assert!(!out.exists(), "a refused override writes nothing: {block}");
    }
    let missing = dir.join("no_such_file.json");
    let refused = fork_with(&parent, "5", &missing, &out).expect_err("no file");
    assert!(refused.starts_with("error: io: "), "{refused}");
    assert!(!out.exists());
    // A parameter twice: the rule of the core, asked before anything is written.
    let twice = override_file(
        &dir,
        r#"[{"core:hereditary_diseases":true},{"core:hereditary_diseases":false}]"#,
    );
    assert_eq!(
        fork_with(&parent, "5", &twice, &out),
        Err("error: override_rejected: override_param_twice".to_owned())
    );
    assert!(!out.exists(), "a rejected override writes nothing");
    // With the block corrected the same output directory serves.
    let block = override_file(&dir, SWITCH_ON);
    assert_eq!(fork_with(&parent, "5", &block, &out), Ok(()));
}

/// A fork can be forked again. From a later year its override is taken over and the
/// new fork is its parent; from an earlier year the override is dropped (it would act
/// from the new fork point on), the manifest counts it, and the run is the original.
#[test]
fn a_fork_of_a_fork_keeps_or_drops_the_override() {
    let dir = scratch("fork_of_fork");
    let original = parent(&dir);
    let block = override_file(&dir, SWITCH_ON);
    let first = dir.join("first");
    assert_eq!(fork_with(&original, "5", &block, &first), Ok(()));

    let later = dir.join("later");
    assert_eq!(fork(&first, "7", &later), Ok(()));
    for file in FILES {
        assert!(
            fs::read(first.join(file)).expect("reads")
                == fs::read(later.join(file)).expect("reads"),
            "{file} differs in the fork from year 7"
        );
    }
    assert_eq!(manifest(&later)["fork"]["dropped_inputs"], json!(0));
    assert_eq!(
        manifest(&later)["fork"]["parent_log_hash"],
        manifest(&first)["log_hash"]
    );

    // The override is stamped T_5 - 1 s: a fork from year 5 or earlier drops it.
    for from_year in ["5", "3"] {
        let earlier = dir.join(format!("earlier{from_year}"));
        assert_eq!(fork(&first, from_year, &earlier), Ok(()));
        let block = &manifest(&earlier)["fork"];
        assert_eq!(block["dropped_inputs"], json!(1), "from year {from_year}");
        assert_eq!(block["parent_log_hash"], manifest(&first)["log_hash"]);
        assert_eq!(hashes(&manifest(&earlier)), hashes(&manifest(&original)));
        assert_eq!(
            manifest(&earlier)["log_hash"],
            manifest(&original)["log_hash"]
        );
        assert_eq!(
            text(&earlier.join("events.ndjson")),
            text(&original.join("events.ndjson"))
        );
    }
    // From year 6 the override is kept: stamped before T_6 - 1 s.
    let kept = dir.join("kept");
    assert_eq!(fork(&first, "6", &kept), Ok(()));
    assert_eq!(manifest(&kept)["fork"]["dropped_inputs"], json!(0));
    assert_eq!(hashes(&manifest(&kept)), hashes(&manifest(&first)));
}

/// What ties the hash by which a fork names its parent to the run of the fork: the
/// events before the fork point are those of the parent, also when the fork overrides;
/// and a fork that overrides nothing and drops nothing has the whole log of its parent.
#[test]
fn the_events_before_the_fork_point_are_those_of_the_parent() {
    let dir = scratch("events_before");
    let block = override_file(&dir, SWITCH_ON);

    // A parent whose events differ from its run in year 2, before the fork point.
    let early = parent(&dir.join("early_dir"));
    tamper_with_the_birthday_of(&early, 2);
    for (name, with_override) in [("early_plain", false), ("early_override", true)] {
        let out = dir.join(name);
        let result = if with_override {
            fork_with(&early, "5", &block, &out)
        } else {
            fork(&early, "5", &out)
        };
        let refused = result.expect_err("the events of year 2 are not the parent's");
        assert!(
            refused.starts_with(
                "error: fork_parent_inconsistent: the events before the fork point have log hash "
            ),
            "{refused}"
        );
        assert!(!out.join("manifest.json").exists(), "{name}");
    }

    // A parent whose events differ in year 7, after the fork point: a fork without
    // override is its parent and is refused, a fork with override may differ from there.
    let late = parent(&dir.join("late_dir"));
    let run_hash = manifest(&late)["log_hash"]
        .as_str()
        .expect("hex")
        .to_owned();
    tamper_with_the_birthday_of(&late, 7);
    let tampered_hash = manifest(&late)["log_hash"]
        .as_str()
        .expect("hex")
        .to_owned();
    assert_eq!(
        fork(&late, "5", &dir.join("late_plain")),
        Err(format!(
            "error: fork_parent_inconsistent: a fork without override is its parent: its log hash is {run_hash}, the manifest of the parent says {tampered_hash}"
        ))
    );
    assert!(!dir.join("late_plain").join("manifest.json").exists());
    let out = dir.join("late_override");
    assert_eq!(fork_with(&late, "5", &block, &out), Ok(()));
    assert_eq!(
        manifest(&out)["fork"]["parent_log_hash"],
        json!(tampered_hash)
    );
    // The fork point itself counts as after: an entry at T_5 may differ.
    let at = parent(&dir.join("at_dir"));
    tamper_with_the_birthday_of(&at, 5);
    assert_eq!(
        fork_with(&at, "5", &block, &dir.join("at_override")),
        Ok(())
    );
    let before = parent(&dir.join("before_dir"));
    tamper_with_the_birthday_of(&before, 4);
    assert!(fork_with(&before, "5", &block, &dir.join("before_override")).is_err());
}

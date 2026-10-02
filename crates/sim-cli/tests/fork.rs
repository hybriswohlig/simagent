//! `simagent fork` as a process (ADR-0001 section 6, ADR-0004 section 8). A fork is a
//! new run from t0 with the inputs of the parent before the fork point; the year hashes
//! before the fork point must equal the parent's. Overrides arrive with the first world
//! parameter (step 5).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

const Y: i64 = 31_449_600;

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
    // Year 10 starts one second after the end of the parent: the latest fork point.
    for from_year in [1_u64, 5, 10] {
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

/// ADR-0001 section 6: X >= 1, and year X starts at the end of the parent at the latest.
/// A refused fork writes nothing.
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
    assert_eq!(
        fork(&parent, "11", &out),
        Err(format!(
            "error: fork_point_out_of_range: from-year 11: the parent run ends at {}, before year 11 starts",
            10 * Y - 1
        ))
    );
    assert_eq!(
        fork(&parent, "9223372036854775807", &out),
        Err(format!(
            "error: fork_point_out_of_range: from-year 9223372036854775807: the parent run ends at {}, before year 9223372036854775807 starts",
            10 * Y - 1
        ))
    );
    assert!(!out.exists(), "a refused fork writes nothing");
    // A parent that ended inside year 3 can fork from year 3, not from year 4.
    let short = dir.join("short");
    let scenario = dir.join("scenario.json");
    let until = (3 * Y + 5).to_string();
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
            3 * Y + 5
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
    replace_in(&log, r#""core_version":1"#, r#""core_version":2"#);
    assert_eq!(
        fork(&parent, "5", &out),
        Err(
            "error: fork_foreign_core_version: the parent run is of core version 2, this is 1"
                .to_owned()
        )
    );
    replace_in(&log, r#""core_version":2"#, r#""core_version":1"#);
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

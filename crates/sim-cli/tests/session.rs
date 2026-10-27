//! The run directory under the server's way of writing (ADR-0021 sections 1 and 2): the
//! lock of a run, stops the run goes on from, a crash between two files reported on the
//! next open, never a silently wrong run, and a run left between two stops back at its
//! last one on the server's start.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::{Holder, free};

use serde_json::Value;
use sim_cli::error::CliError;
use sim_cli::files::{self, Scenario};
use sim_cli::lock::{RunLock, lock_path};
use sim_cli::session::{self, Recovered, Session};
use sim_core::ids::SimId;
use sim_core::input::{ArrangePartnership, Input};
use sim_core::time::SimTime;

/// Three founders, ten years (as in `cli.rs`).
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

const Y: i64 = 31_449_600;
const FILES: [&str; 6] = [
    "manifest.json",
    "scenario.resolved.json",
    "input.ndjson",
    "events.ndjson",
    "census.csv",
    "state.save",
];

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("session")
        .join(name);
    if dir.exists() {
        fs::remove_dir_all(&dir).expect("removes the old scratch directory");
    }
    fs::create_dir_all(&dir).expect("creates the scratch directory");
    dir
}

/// `simagent args`: success, or the first line of its error.
fn simagent(args: &[&std::ffi::OsStr]) -> Result<(), String> {
    let output = Command::new(env!("CARGO_BIN_EXE_simagent"))
        .args(args)
        .output()
        .expect("runs simagent");
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8(output.stderr).expect("UTF-8"))
    }
}

/// A run of the ten-year scenario to the end of year 3, in `dir`.
fn three_years(base: &Path, name: &str) -> PathBuf {
    let scenario = base.join("ten.json");
    fs::write(&scenario, TEN_YEARS).expect("written");
    let dir = base.join(name);
    let until = (4 * Y - 1).to_string();
    simagent(&[
        "run".as_ref(),
        "--scenario".as_ref(),
        scenario.as_os_str(),
        "--out".as_ref(),
        dir.as_os_str(),
        "--until".as_ref(),
        until.as_ref(),
    ])
    .expect("runs");
    dir
}

fn resume_to(dir: &Path, until: i64) -> Result<(), String> {
    let until = until.to_string();
    simagent(&[
        "run".as_ref(),
        "--resume".as_ref(),
        "--out".as_ref(),
        dir.as_os_str(),
        "--until".as_ref(),
        until.as_ref(),
    ])
}

fn code(error: &str) -> &str {
    error
        .trim_start_matches("error: ")
        .split(':')
        .next()
        .unwrap_or("")
}

/// A lock held by another process keeps every command that writes the run out of it.
#[test]
fn a_locked_run_is_not_opened() {
    let base = scratch("locked");
    let dir = three_years(&base, "run");
    assert_eq!(lock_path(&dir).expect("a path"), base.join("run.lock"));
    let held = Holder::hold(&base.join("run.lock"));
    let error = resume_to(&dir, 6 * Y - 1).expect_err("locked");
    assert_eq!(code(&error), "run_locked", "{error}");
    let export = simagent(&["export".as_ref(), dir.as_os_str()]).expect_err("locked");
    assert_eq!(code(&export), "run_locked", "{export}");
    let fork = base.join("fork");
    let forked = simagent(&[
        "fork".as_ref(),
        dir.as_os_str(),
        "--from-year".as_ref(),
        "1".as_ref(),
        "--out".as_ref(),
        fork.as_os_str(),
    ])
    .expect_err("its parent is locked");
    assert_eq!(code(&forked), "run_locked", "{forked}");
    drop(held);
    resume_to(&dir, 6 * Y - 1).expect("free");
    // In this process too: a second lock of the same run is refused. Last, since nothing
    // can tell when a lock of this process is free again (`common`).
    let _first = RunLock::acquire(&dir).expect("free again");
    let second = RunLock::acquire(&dir).expect_err("held by the first");
    assert_eq!(second.code, "run_locked");
}

/// A link to a run takes the run's own lock; a new run, a replay and a fork into a locked
/// directory are refused; a lock ends with its process.
#[cfg(unix)]
#[test]
fn a_lock_follows_the_run_not_its_name() {
    let base = scratch("lock-identity");
    let dir = three_years(&base, "run");
    let alias = base.join("alias");
    std::os::unix::fs::symlink(&dir, &alias).expect("a link");
    assert_eq!(
        lock_path(&alias).expect("a path"),
        lock_path(&dir).expect("a path")
    );
    let lock_file = lock_path(&dir).expect("a path");
    let held = Holder::hold(&lock_file);
    assert_eq!(
        RunLock::acquire(&alias).expect_err("the same run").code,
        "run_locked"
    );
    drop(held);
    // A run not made yet is named by its parent's real path: below a link to the parent
    // it takes the lock it takes below the parent.
    let parent = base.join("parent");
    fs::create_dir(&parent).expect("made");
    let linked = base.join("linked");
    std::os::unix::fs::symlink(&parent, &linked).expect("a link");
    assert_eq!(
        lock_path(&linked.join("new")).expect("a path"),
        lock_path(&parent.join("new")).expect("a path")
    );
    // Every command that writes a new directory takes its lock first.
    let target = base.join("target");
    let lock = Holder::hold(&lock_path(&target).expect("a path"));
    let scenario = base.join("ten.json");
    let until = (2 * Y - 1).to_string();
    let run = simagent(&[
        "run".as_ref(),
        "--scenario".as_ref(),
        scenario.as_os_str(),
        "--out".as_ref(),
        target.as_os_str(),
        "--until".as_ref(),
        until.as_ref(),
    ]);
    assert_eq!(code(&run.expect_err("locked")), "run_locked");
    let replay = simagent(&[
        "run".as_ref(),
        "--replay".as_ref(),
        dir.join(files::INPUT_LOG).as_os_str(),
        "--out".as_ref(),
        target.as_os_str(),
    ]);
    assert_eq!(code(&replay.expect_err("locked")), "run_locked");
    let fork = simagent(&[
        "fork".as_ref(),
        dir.as_os_str(),
        "--from-year".as_ref(),
        "1".as_ref(),
        "--out".as_ref(),
        target.as_os_str(),
    ]);
    assert_eq!(code(&fork.expect_err("locked")), "run_locked");
    drop(lock);
    assert!(
        free(&lock_path(&target).expect("a path")),
        "free after the holder died"
    );
    // A process that holds the lock and dies lets it go (flock, as `File::try_lock`).
    let holder = Holder::hold(&lock_file);
    assert_eq!(RunLock::acquire(&dir).expect_err("held").code, "run_locked");
    assert!(!free(&lock_file), "held");
    drop(holder);
    assert!(free(&lock_file), "free after the holder died");
    resume_to(&dir, 6 * Y - 1).expect("and the run goes on");
}

/// Stops of the session in every order the server may take: a fresh run's first stop,
/// two stops in a row, a stop before the end, inputs after a reopen; each leaves a run the
/// command line goes on from.
#[test]
fn every_order_of_stops_leaves_a_run_to_go_on_from() {
    let base = scratch("stops");
    let dir = three_years(&base, "run");
    let scenario: Scenario = files::parse_strict(
        fs::read_to_string(dir.join(files::SCENARIO))
            .expect("read")
            .trim_end(),
    )
    .expect("a scenario");
    {
        let mut session = Session::resume(&dir).expect("resumes");
        session.checkpoint(&scenario).expect("a stop at once");
        session.checkpoint(&scenario).expect("and another");
        session.reopen().expect("reopens");
        session.reopen().expect("twice is nothing");
        let mut session = session.submit_at_stop(vec![pair(1, 2)]).expect("accepted");
        session.checkpoint(&scenario).expect("stops");
        session
            .advance(SimTime::from_secs(5 * Y - 1))
            .expect("advances");
        session.checkpoint(&scenario).expect("stops");
        session.finish(&scenario).expect("ends after a stop");
    }
    let log = fs::read_to_string(dir.join(files::INPUT_LOG)).expect("read");
    assert_eq!(log.matches("\"end\"").count(), 1, "{log}");
    assert_eq!(log.lines().count(), 4, "{log}");
    resume_to(&dir, 6 * Y - 1).expect("goes on");
    // A fresh run stopped at once is a run too.
    let fresh = base.join("fresh");
    {
        let first = Input::Start(scenario.start.clone());
        let mut session = Session::start(
            &fresh,
            SimTime::from_secs(-1),
            first,
            sim_cli::session::Compare::Nothing,
            |code| CliError::new("scenario_rejected", code),
        )
        .expect("starts");
        session.checkpoint(&scenario).expect("stops");
    }
    resume_to(&fresh, 2 * Y - 1).expect("goes on");
}

/// Stops from the session (the server's way) give the directory `run --resume --until`
/// gives: the same bytes in every file.
#[test]
fn checkpoints_give_the_run_the_command_line_gives() {
    let base = scratch("checkpoints");
    let by_session = three_years(&base, "session");
    let by_cli = three_years(&base, "cli");
    let scenario: Scenario = files::parse_strict(
        fs::read_to_string(by_session.join(files::SCENARIO))
            .expect("read")
            .trim_end(),
    )
    .expect("a scenario");
    {
        let mut session = Session::resume(&by_session).expect("resumes");
        session.reopen().expect("nothing to reopen");
        for until in [5 * Y - 1, 5 * Y - 1 + 28 * 86_400, 7 * Y - 1] {
            session.reopen().expect("reopens");
            session
                .advance(SimTime::from_secs(until))
                .expect("advances");
            session.checkpoint(&scenario).expect("stops");
        }
    }
    resume_to(&by_cli, 7 * Y - 1).expect("runs");
    for file in FILES {
        assert_eq!(
            fs::read(by_session.join(file)).expect("read"),
            fs::read(by_cli.join(file)).expect("read"),
            "{file}"
        );
    }
    // And the run goes on from a checkpoint like from any stop.
    resume_to(&by_session, 8 * Y - 1).expect("resumes");
}

/// The bytes of every file of a run directory.
fn contents(dir: &Path) -> Vec<Vec<u8>> {
    FILES
        .iter()
        .map(|file| fs::read(dir.join(file)).expect("read"))
        .collect()
}

fn pair(woman: u64, man: u64) -> Input {
    Input::ArrangePartnership(ArrangePartnership {
        woman: SimId::try_from(woman).expect("an id"),
        man: SimId::try_from(man).expect("an id"),
    })
}

/// A rejected input writes nothing (ADR-0021 section 2): not after the open, not after a
/// stop; a list of inputs with a rejected one leaves no session to go on with.
#[test]
fn a_rejected_input_writes_nothing() {
    let base = scratch("rejected");
    let dir = three_years(&base, "run");
    let scenario: Scenario = files::parse_strict(
        fs::read_to_string(dir.join(files::SCENARIO))
            .expect("read")
            .trim_end(),
    )
    .expect("a scenario");
    let stopped = contents(&dir);
    let rejected = |code: String| CliError::new("input_rejected", code);
    {
        let mut session = Session::resume(&dir).expect("resumes");
        let error = session.submit(pair(2, 1), rejected).expect_err("rejected");
        assert_eq!(
            (error.code, error.detail.as_str()),
            ("input_rejected", "partners_of_wrong_sex")
        );
        assert!(contents(&dir) == stopped, "rejected after the open");
        // A stop with nothing new writes the same bytes again.
        session.checkpoint(&scenario).expect("stops");
        assert!(contents(&dir) == stopped, "a stop with nothing new");
        session.submit(pair(2, 1), rejected).expect_err("rejected");
        assert!(contents(&dir) == stopped, "rejected after a stop");
        let error = session
            .submit_at_stop(vec![pair(1, 2), pair(2, 1)])
            .err()
            .expect("rejected");
        assert_eq!(error.detail, "partners_of_wrong_sex");
        assert!(contents(&dir) == stopped, "a list with a rejected input");
    }
    resume_to(&dir, 5 * Y - 1).expect("goes on");
}

/// A damage to a run directory: its name, the damage, the code of the command line, the
/// code of `Session::resume` (the server's open).
type Case = (&'static str, fn(&Path), &'static str, &'static str);

fn cut(dir: &Path, file: &str, keep: impl FnOnce(usize) -> usize) {
    let bytes = fs::read(dir.join(file)).expect("read");
    fs::write(dir.join(file), &bytes[..keep(bytes.len())]).expect("written");
}

/// The input log with one field of its header changed.
fn header_field(dir: &Path, field: &str, change: impl FnOnce(&Value) -> Value) {
    let text = fs::read_to_string(dir.join(files::INPUT_LOG)).expect("read");
    let (header, rest) = text.split_once('\n').expect("a header");
    let mut header: Value = serde_json::from_str(header).expect("JSON");
    let changed = change(&header[field]);
    assert_ne!(header[field], changed, "{field}");
    header[field] = changed;
    fs::write(dir.join(files::INPUT_LOG), format!("{header}\n{rest}")).expect("written");
}

fn remove(dir: &Path, file: &str) {
    fs::remove_file(dir.join(file)).expect("removed");
}

/// A crash between two of the files, each case.
fn damages() -> [Case; 23] {
    [
        (
            // The ten-year scenario has its one input in its start record: an extra
            // record repeats it, which no save holds twice.
            "input written, save old",
            |dir| {
                let text = fs::read_to_string(dir.join(files::INPUT_LOG)).expect("read");
                let mut lines: Vec<&str> = text.lines().collect();
                let end = lines.pop().expect("an end line");
                let extra = lines.last().expect("a record").to_string();
                lines.push(&extra);
                lines.push(end);
                fs::write(dir.join(files::INPUT_LOG), lines.join("\n") + "\n").expect("written");
            },
            "run_dir_inconsistent",
            "run_dir_inconsistent",
        ),
        (
            "input log without its end",
            |dir| {
                let text = fs::read_to_string(dir.join(files::INPUT_LOG)).expect("read");
                let mut lines: Vec<&str> = text.lines().collect();
                lines.pop();
                fs::write(dir.join(files::INPUT_LOG), lines.join("\n") + "\n").expect("written");
            },
            "run_dir_inconsistent",
            "run_dir_inconsistent",
        ),
        (
            "input log with its header only",
            |dir| {
                let text = fs::read_to_string(dir.join(files::INPUT_LOG)).expect("read");
                let header = text.lines().next().expect("a header");
                fs::write(dir.join(files::INPUT_LOG), format!("{header}\n")).expect("written");
            },
            "run_dir_inconsistent",
            "run_dir_inconsistent",
        ),
        (
            "input log of another core",
            |dir| {
                header_field(dir, "core_version", |v| {
                    (v.as_u64().expect("a number") + 1).into()
                })
            },
            "run_dir_inconsistent",
            "run_dir_inconsistent",
        ),
        (
            "input log of other data",
            |dir| header_field(dir, "data_hash", |_| "0000000000000000".into()),
            "run_dir_inconsistent",
            "run_dir_inconsistent",
        ),
        (
            "input log missing",
            |dir| remove(dir, files::INPUT_LOG),
            "io",
            "io",
        ),
        (
            "manifest cut off",
            |dir| cut(dir, files::MANIFEST, |n| n / 2),
            "manifest_invalid",
            "manifest_invalid",
        ),
        (
            "manifest missing",
            |dir| remove(dir, files::MANIFEST),
            "io",
            "io",
        ),
        (
            "manifest new, save old",
            |dir| {
                let text = fs::read_to_string(dir.join(files::MANIFEST)).expect("read");
                let later = text.replacen("\"closed_through\":", "\"closed_through\":1", 1);
                fs::write(dir.join(files::MANIFEST), later).expect("written");
            },
            "run_dir_inconsistent",
            "run_dir_inconsistent",
        ),
        (
            "save cut off",
            |dir| cut(dir, files::SAVE, |n| n / 2),
            "save_rejected",
            "save_rejected",
        ),
        ("save missing", |dir| remove(dir, files::SAVE), "io", "io"),
        (
            "events written, manifest old",
            |dir| {
                let text = fs::read_to_string(dir.join(files::EVENTS)).expect("read");
                let last = text.lines().last().expect("an entry").to_owned();
                files::append(&dir.join(files::EVENTS), &format!("{last}\n")).expect("appended");
            },
            "run_dir_inconsistent",
            "run_dir_inconsistent",
        ),
        (
            "events without their last entry",
            |dir| {
                let text = fs::read_to_string(dir.join(files::EVENTS)).expect("read");
                let mut lines: Vec<&str> = text.lines().collect();
                lines.pop();
                fs::write(dir.join(files::EVENTS), lines.join("\n") + "\n").expect("written");
            },
            "run_dir_inconsistent",
            "run_dir_inconsistent",
        ),
        (
            "events cut off in a line",
            |dir| cut(dir, files::EVENTS, |n| n - 3),
            "events_invalid",
            "events_invalid",
        ),
        (
            // The last entry whole but its line end missing: the next entry would join it.
            "events cut off before the last line end",
            |dir| cut(dir, files::EVENTS, |n| n - 1),
            "events_invalid",
            "events_invalid",
        ),
        (
            "events missing",
            |dir| remove(dir, files::EVENTS),
            "io",
            "io",
        ),
        (
            // The command line reads the scenario first.
            "scenario cut off",
            |dir| cut(dir, files::SCENARIO, |n| n / 2),
            "scenario_invalid",
            "run_dir_inconsistent",
        ),
        (
            "scenario of another world",
            |dir| {
                let text = fs::read_to_string(dir.join(files::SCENARIO)).expect("read");
                let other = text.replacen(
                    "\"world_seed\":\"0000000000000001\"",
                    "\"world_seed\":\"0000000000000002\"",
                    1,
                );
                assert_ne!(other, text, "the seed is replaced");
                fs::write(dir.join(files::SCENARIO), other).expect("written");
            },
            "run_dir_inconsistent",
            "run_dir_inconsistent",
        ),
        (
            "scenario missing",
            |dir| remove(dir, files::SCENARIO),
            "io",
            "io",
        ),
        (
            "census row cut off",
            |dir| cut(dir, files::CENSUS, |n| n - 2),
            "run_dir_inconsistent",
            "run_dir_inconsistent",
        ),
        (
            "census row without its last cell",
            |dir| {
                let text = fs::read_to_string(dir.join(files::CENSUS)).expect("read");
                let shorter: Vec<&str> = (text.lines())
                    .map(|line| line.rsplit_once(',').map_or(line, |(kept, _)| kept))
                    .collect();
                let rows = shorter[1..].join("\n");
                let text = format!("{}\n{rows}\n", text.lines().next().expect("a header"));
                fs::write(dir.join(files::CENSUS), text).expect("written");
            },
            "run_dir_inconsistent",
            "run_dir_inconsistent",
        ),
        (
            "census cell no number",
            |dir| {
                let text = fs::read_to_string(dir.join(files::CENSUS)).expect("read");
                let (kept, _) = text.trim_end().rsplit_once(',').expect("a cell");
                fs::write(dir.join(files::CENSUS), format!("{kept},x\n")).expect("written");
            },
            "run_dir_inconsistent",
            "run_dir_inconsistent",
        ),
        (
            "census missing",
            |dir| remove(dir, files::CENSUS),
            "io",
            "io",
        ),
    ]
}

/// A crash between two of the files: each case, as the next open reports it.
#[test]
fn an_interrupted_write_is_reported_on_the_next_open() {
    let base = scratch("crash");
    for (i, (case, damage, by_cli, by_session)) in damages().into_iter().enumerate() {
        let dir = three_years(&base, &format!("case{i}"));
        damage(&dir);
        let damaged = (FILES.iter())
            .map(|file| fs::read(dir.join(file)).ok())
            .collect::<Vec<_>>();
        let refused = Session::resume(&dir).err().expect(case);
        assert_eq!(refused.code, by_session, "{case}: {}", refused.detail);
        let error = resume_to(&dir, 6 * Y - 1).expect_err(case);
        assert_eq!(code(&error), by_cli, "{case}: {error}");
        // A refused open leaves the damage as it found it.
        let after = (FILES.iter())
            .map(|file| fs::read(dir.join(file)).ok())
            .collect::<Vec<_>>();
        assert!(after == damaged, "{case}: the open wrote");
    }
}

/// What a step writes before its stop, by the real session: an input at the stop, if any,
/// then the clock one year on; the process ends before the stop.
fn interrupted_step(dir: &Path, input: Option<Input>) {
    let session = Session::resume(dir).expect("resumes");
    let mut session = match input {
        Some(input) => session.submit_at_stop(vec![input]).expect("accepted"),
        None => session,
    };
    session.reopen().expect("reopens");
    session
        .advance(SimTime::from_secs(5 * Y - 1))
        .expect("advances");
}

/// The lines of `file` after its first `kept` bytes; a last line without its line end
/// counts.
fn lines_after(dir: &Path, file: &str, kept: usize) -> usize {
    let bytes = fs::read(dir.join(file)).expect("read");
    let rest = &bytes[kept.min(bytes.len())..];
    let ends = rest.iter().filter(|b| **b == b'\n').count();
    ends + usize::from(rest.last().is_some_and(|b| *b != b'\n'))
}

/// A run that a process left between two stops goes back to its last stop on the server's
/// start (ADR-0021 section 2, addendum 2026-10-27): every file as the stop wrote it, the
/// lines after it counted, and the run goes on.
#[test]
fn an_interrupted_step_goes_back_to_its_last_stop() {
    let base = scratch("recover");
    // A case: its name, what the interrupted step left, the inputs after the stop.
    type Step = (&'static str, fn(&Path), usize);
    let steps: [Step; 6] = [
        (
            "a year without its stop",
            |dir| interrupted_step(dir, None),
            0,
        ),
        (
            "an input and a year",
            |dir| interrupted_step(dir, Some(pair(1, 2))),
            1,
        ),
        (
            "events cut off in a line",
            |dir| {
                interrupted_step(dir, None);
                cut(dir, files::EVENTS, |n| n - 3);
            },
            0,
        ),
        (
            "a census row cut off",
            |dir| {
                interrupted_step(dir, None);
                cut(dir, files::CENSUS, |n| n - 2);
            },
            0,
        ),
        (
            "the end line written, the manifest old",
            |dir| {
                interrupted_step(dir, None);
                let end = files::LogEnd {
                    end: SimTime::from_secs(5 * Y - 1),
                };
                files::append(&dir.join(files::INPUT_LOG), &(files::json(&end) + "\n"))
                    .expect("appended");
            },
            0,
        ),
        (
            // The input log written anew when the step opened it, cut off in its last
            // record; the step wrote nothing else.
            "the input log cut off while it was opened",
            |dir| {
                let text = fs::read_to_string(dir.join(files::INPUT_LOG)).expect("read");
                let (open, _end) = text.trim_end().rsplit_once('\n').expect("an end line");
                fs::write(dir.join(files::INPUT_LOG), &open[..open.len() - 4]).expect("written");
            },
            0,
        ),
    ];
    for (i, (case, step, inputs)) in steps.into_iter().enumerate() {
        let dir = three_years(&base, &format!("case{i}"));
        let stopped = contents(&dir);
        let size = |file: &str| fs::read(dir.join(file)).expect("read").len();
        let (events_at_stop, census_at_stop) = (size(files::EVENTS), size(files::CENSUS));
        step(&dir);
        assert!(contents(&dir) != stopped, "{case}: the step wrote nothing");
        let expected = Recovered {
            closed_through: SimTime::from_secs(4 * Y - 1),
            events: lines_after(&dir, files::EVENTS, events_at_stop),
            census: lines_after(&dir, files::CENSUS, census_at_stop),
            inputs,
        };
        let recovered = session::recover(&dir).expect(case);
        assert_eq!(recovered, Some(expected), "{case}");
        assert!(contents(&dir) == stopped, "{case}: not the last stop");
        resume_to(&dir, 6 * Y - 1).expect(case);
    }
}

/// Only what a step leaves after the last stop is cut: a damage to what the stop wrote is
/// refused with the code of the open, the files as they were; the four damages that leave
/// the stop whole go back to it.
#[test]
fn only_what_a_step_leaves_is_cut() {
    let base = scratch("recover-damages");
    let whole = [
        "input written, save old",
        "input log without its end",
        "input log with its header only",
        "events written, manifest old",
    ];
    for (i, (case, damage, _, by_session)) in damages().into_iter().enumerate() {
        let dir = three_years(&base, &format!("case{i}"));
        let stopped = contents(&dir);
        damage(&dir);
        let damaged = (FILES.iter())
            .map(|file| fs::read(dir.join(file)).ok())
            .collect::<Vec<_>>();
        let recovered = session::recover(&dir);
        if whole.contains(&case) {
            let recovered = recovered.expect(case).expect(case);
            assert_eq!(recovered.closed_through, SimTime::from_secs(4 * Y - 1));
            assert!(contents(&dir) == stopped, "{case}: not the last stop");
        } else {
            let refused = recovered.err().expect(case);
            assert_eq!(refused.code, by_session, "{case}: {}", refused.detail);
            let after = (FILES.iter())
                .map(|file| fs::read(dir.join(file)).ok())
                .collect::<Vec<_>>();
            assert!(after == damaged, "{case}: the recovery wrote");
        }
    }
}

/// A run at its stop is no case for the recovery: nothing is written.
#[test]
fn a_run_at_its_stop_is_left_alone() {
    let base = scratch("recover-whole");
    let dir = three_years(&base, "run");
    let stopped = contents(&dir);
    assert_eq!(session::recover(&dir).expect("opens"), None);
    assert!(contents(&dir) == stopped, "the recovery wrote");
}

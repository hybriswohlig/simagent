//! The run directory under the server's way of writing (ADR-0021 sections 1 and 2): the
//! lock of a run, stops the run goes on from, and a crash between two files reported on
//! the next open, never a silently wrong run.

use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::Command;

use sim_cli::files::{self, Scenario};
use sim_cli::lock::{RunLock, lock_path};
use sim_cli::session::Session;
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
    let held = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(base.join("run.lock"))
        .expect("opens");
    held.try_lock().expect("locks");
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
    // In this process too: a second lock of the same run is refused.
    drop(held);
    let first = RunLock::acquire(&dir).expect("free again");
    let second = RunLock::acquire(&dir).expect_err("held by the first");
    assert_eq!(second.code, "run_locked");
    drop(first);
    resume_to(&dir, 6 * Y - 1).expect("free");
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
    let held = RunLock::acquire(&dir).expect("free");
    assert_eq!(
        RunLock::acquire(&alias).expect_err("the same run").code,
        "run_locked"
    );
    drop(held);
    // Every command that writes a new directory takes its lock first.
    let target = base.join("target");
    let lock = RunLock::acquire(&target).expect("free");
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
    // A process that holds the lock and dies lets it go (flock, as `File::try_lock`).
    let lock_file = lock_path(&dir).expect("a path");
    let mut holder = Command::new("perl")
        .args(["-e", "use Fcntl qw(:flock); open(my $f, '>>', $ARGV[0]) or die; flock($f, LOCK_EX) or die; $| = 1; print qq(held\n); sleep 60"])
        .arg(&lock_file)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("perl runs");
    let mut line = String::new();
    std::io::BufRead::read_line(
        &mut std::io::BufReader::new(holder.stdout.take().expect("its output")),
        &mut line,
    )
    .expect("reads");
    assert_eq!(line, "held\n");
    assert_eq!(RunLock::acquire(&dir).expect_err("held").code, "run_locked");
    holder.kill().expect("killed");
    holder.wait().expect("ended");
    RunLock::acquire(&dir).expect("free after the holder died");
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
        let _lock = RunLock::acquire(&dir).expect("free");
        let mut session = Session::resume(&dir).expect("resumes");
        session.checkpoint(&scenario).expect("a stop at once");
        session.checkpoint(&scenario).expect("and another");
        session.reopen().expect("reopens");
        session.reopen().expect("twice is nothing");
        let pair =
            sim_core::input::Input::ArrangePartnership(sim_core::input::ArrangePartnership {
                woman: sim_core::ids::SimId::try_from(1).expect("an id"),
                man: sim_core::ids::SimId::try_from(2).expect("an id"),
            });
        session.submit_at_stop(vec![pair]).expect("accepted");
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
        let _lock = RunLock::acquire(&fresh).expect("free");
        let first = sim_core::input::Input::Start(scenario.start.clone());
        let mut session = Session::start(
            &fresh,
            SimTime::from_secs(-1),
            first,
            sim_cli::session::Compare::Nothing,
            |code| sim_cli::error::CliError::new("scenario_rejected", code),
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
        let _lock = RunLock::acquire(&by_session).expect("free");
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

/// A damage to a run directory: its name, the damage, the code the next open reports.
type Case<'a> = (&'a str, &'a dyn Fn(&Path), &'a str);

/// A crash between two of the files: each case, as the next open reports it.
#[test]
fn an_interrupted_write_is_reported_on_the_next_open() {
    let base = scratch("crash");
    let cases: [Case; 11] = [
        (
            "input written, save old",
            &|dir| {
                let text = fs::read_to_string(dir.join(files::INPUT_LOG)).expect("read");
                let mut lines: Vec<&str> = text.lines().collect();
                let end = lines.pop().expect("an end line");
                let extra = lines.last().expect("a record").to_string();
                lines.push(&extra);
                lines.push(end);
                fs::write(dir.join(files::INPUT_LOG), lines.join("\n") + "\n").expect("written");
            },
            "run_dir_inconsistent",
        ),
        (
            "input log without its end",
            &|dir| {
                let text = fs::read_to_string(dir.join(files::INPUT_LOG)).expect("read");
                let mut lines: Vec<&str> = text.lines().collect();
                lines.pop();
                fs::write(dir.join(files::INPUT_LOG), lines.join("\n") + "\n").expect("written");
            },
            "run_dir_inconsistent",
        ),
        (
            "manifest cut off",
            &|dir| {
                let text = fs::read(dir.join(files::MANIFEST)).expect("read");
                fs::write(dir.join(files::MANIFEST), &text[..text.len() / 2]).expect("written");
            },
            "manifest_invalid",
        ),
        (
            "save cut off",
            &|dir| {
                let bytes = fs::read(dir.join(files::SAVE)).expect("read");
                fs::write(dir.join(files::SAVE), &bytes[..bytes.len() / 2]).expect("written");
            },
            "save_rejected",
        ),
        (
            "events written, manifest old",
            &|dir| {
                let text = fs::read_to_string(dir.join(files::EVENTS)).expect("read");
                let last = text.lines().last().expect("an entry").to_owned();
                files::append(&dir.join(files::EVENTS), &format!("{last}\n")).expect("appended");
            },
            "run_dir_inconsistent",
        ),
        (
            "save missing",
            &|dir| {
                fs::remove_file(dir.join(files::SAVE)).expect("removed");
            },
            "io",
        ),
        (
            "manifest new, save old",
            &|dir| {
                let text = fs::read_to_string(dir.join(files::MANIFEST)).expect("read");
                let later = text.replacen("\"closed_through\":", "\"closed_through\":1", 1);
                fs::write(dir.join(files::MANIFEST), later).expect("written");
            },
            "run_dir_inconsistent",
        ),
        (
            // The command line reads the scenario first; `Session::resume` reports the same
            // file as run_dir_inconsistent (below).
            "scenario cut off",
            &|dir| {
                let text = fs::read(dir.join(files::SCENARIO)).expect("read");
                fs::write(dir.join(files::SCENARIO), &text[..text.len() / 2]).expect("written");
            },
            "scenario_invalid",
        ),
        (
            "scenario missing",
            &|dir| {
                fs::remove_file(dir.join(files::SCENARIO)).expect("removed");
            },
            "io",
        ),
        (
            "census row cut off",
            &|dir| {
                let text = fs::read_to_string(dir.join(files::CENSUS)).expect("read");
                fs::write(dir.join(files::CENSUS), &text[..text.len() - 2]).expect("written");
            },
            "run_dir_inconsistent",
        ),
        (
            "census missing",
            &|dir| {
                fs::remove_file(dir.join(files::CENSUS)).expect("removed");
            },
            "io",
        ),
    ];
    for (i, (case, damage, expected)) in cases.into_iter().enumerate() {
        let dir = three_years(&base, &format!("case{i}"));
        // The ten-year scenario has its one input in its start record: an extra record
        // repeats it, which no save holds twice.
        damage(&dir);
        let error = resume_to(&dir, 6 * Y - 1).expect_err(case);
        assert_eq!(code(&error), expected, "{case}: {error}");
    }
    // The session itself, as the server opens a run, checks the scenario too.
    let dir = three_years(&base, "scenario");
    let text = fs::read(dir.join(files::SCENARIO)).expect("read");
    fs::write(dir.join(files::SCENARIO), &text[..text.len() / 2]).expect("written");
    let refused = Session::resume(&dir).err().expect("refused");
    assert_eq!(refused.code, "run_dir_inconsistent");
}

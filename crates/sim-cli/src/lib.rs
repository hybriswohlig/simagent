//! `simagent`: the command line runner. It loads scenarios, drives the core through
//! its command schema and writes the run directory. It never decides gameplay
//! (ADR-0000); every number in its files comes from the core.

pub mod error;
pub mod files;
pub mod session;

use std::ffi::OsString;
use std::path::Path;

use clap::{Arg, ArgAction, ArgGroup, Command};
use sim_core::input::{Input, InputRecord};
use sim_core::time::{SimDuration, SimTime, calendar, year_start};
use sim_core::version::{CORE_VERSION, EMPTY_DATA_HASH};

use crate::error::CliError;
use crate::files::{ForkInfo, LogEnd, LogHeader, Manifest, Scenario};
use crate::session::{Compare, Session};

/// Arguments through clap's builder API: its derive macros set their own lint levels
/// and collide with the workspace `forbid` lints (ADR-0000).
fn command() -> Command {
    let run = Command::new("run")
        .about("Runs a scenario, replays an input log or continues a run")
        .arg(Arg::new("scenario").long("scenario").value_name("FILE"))
        .arg(Arg::new("replay").long("replay").value_name("INPUT_NDJSON"))
        .arg(Arg::new("resume").long("resume").action(ArgAction::SetTrue))
        .group(
            ArgGroup::new("source")
                .args(["scenario", "replay", "resume"])
                .required(true),
        )
        .arg(Arg::new("out").long("out").value_name("DIR").required(true))
        .arg(
            Arg::new("until")
                .long("until")
                .value_name("SECONDS")
                .allow_negative_numbers(true)
                .conflicts_with("replay"),
        )
        .arg(
            Arg::new("manifest")
                .long("manifest")
                .value_name("MANIFEST_JSON")
                .requires("replay"),
        );
    let fork = Command::new("fork")
        .about("Runs a parent run again from t0 and checks the years before the fork point")
        .arg(Arg::new("parent").value_name("PARENT_DIR").required(true))
        .arg(
            Arg::new("from-year")
                .long("from-year")
                .value_name("YEAR")
                .allow_negative_numbers(true)
                .required(true),
        )
        .arg(Arg::new("out").long("out").value_name("DIR").required(true));
    Command::new("simagent")
        .version(env!("CARGO_PKG_VERSION"))
        .subcommand_required(true)
        .subcommand(run)
        .subcommand(fork)
}

/// Runs the program with these arguments. A usage error ends the process through clap.
pub fn run_cli(args: impl IntoIterator<Item = OsString>) -> Result<(), CliError> {
    let matches = command().get_matches_from(args);
    let run = match matches.subcommand() {
        Some(("run", run)) => run,
        Some(("fork", fork)) => {
            let text = |name: &str| fork.get_one::<String>(name).map_or("", String::as_str);
            let year = text("from-year");
            let from_year: i64 = year
                .parse()
                .map_err(|_| CliError::new("usage", format!("--from-year {year} is not a year")))?;
            return fork_run(Path::new(text("parent")), from_year, Path::new(text("out")));
        }
        _ => return Err(CliError::new("usage", "unknown subcommand")),
    };
    let text = |name: &str| run.get_one::<String>(name).map(String::as_str);
    let out = Path::new(text("out").unwrap_or_default());
    let until = match text("until") {
        Some(secs) => Some(parse_time(secs).ok_or_else(|| {
            CliError::new("usage", format!("--until {secs} is not a time in seconds"))
        })?),
        None => None,
    };
    if let Some(scenario) = text("scenario") {
        run_scenario(Path::new(scenario), out, until)
    } else if let Some(log) = text("replay") {
        replay(Path::new(log), text("manifest").map(Path::new), out)
    } else {
        resume(out, until)
    }
}

fn parse_time(text: &str) -> Option<SimTime> {
    SimTime::try_from(text.parse::<i64>().ok()?).ok()
}

fn before_the_start(until: SimTime) -> bool {
    until.as_secs() < SimTime::START.as_secs() - 1
}

fn run_scenario(file: &Path, out: &Path, until: Option<SimTime>) -> Result<(), CliError> {
    let invalid = |detail: &str| CliError::new("scenario_invalid", detail);
    let scenario: Scenario = files::parse_strict(&files::read_text(file)?)
        .filter(|scenario: &Scenario| scenario.format_version == files::FORMAT_VERSION)
        .ok_or_else(|| invalid("not a scenario of format version 1"))?;
    if before_the_start(scenario.until) {
        let detail = format!("until {} lies before the start", scenario.until.as_secs());
        return Err(CliError::new("scenario_invalid", detail));
    }
    let before_start = SimTime::START.as_secs() - 1;
    let stamp =
        SimTime::try_from(before_start).unwrap_or_else(|_| panic!("invariant cli.start_stamp"));
    // Both limits are checked before anything is written.
    session::check_limit(stamp, scenario.until)?;
    session::check_limit(stamp, until.unwrap_or(scenario.until))?;
    let first = Input::Start(scenario.start.clone());
    let rejected = |code: String| CliError::new("input_rejected", code);
    let mut session = Session::start(out, stamp, first, Compare::Nothing, rejected)?;
    session.advance(until.unwrap_or(scenario.until))?;
    session.finish(&scenario)
}

fn resume(dir: &Path, until: Option<SimTime>) -> Result<(), CliError> {
    let scenario: Scenario = files::parse_strict(&files::read_text(&dir.join(files::SCENARIO))?)
        .filter(|scenario: &Scenario| scenario.format_version == files::FORMAT_VERSION)
        .ok_or_else(|| CliError::new("scenario_invalid", "not a scenario of format version 1"))?;
    let mut session = Session::resume(dir)?;
    let limit = until.unwrap_or(scenario.until);
    session::check_limit(session.closed_through(), limit)?;
    // Nothing has been written up to here: a refused resume leaves the run as it was.
    session.reopen_input_log()?;
    session.advance(limit)?;
    session.finish(&scenario)
}

/// Whose log is read, for the messages about a foreign version.
struct Source {
    subject: &'static str,
    foreign_core_version: &'static str,
    foreign_data_hash: &'static str,
}

const REPLAYED_LOG: Source = Source {
    subject: "the input log",
    foreign_core_version: "replay_foreign_core_version",
    foreign_data_hash: "replay_foreign_data_hash",
};

const PARENT_RUN: Source = Source {
    subject: "the parent run",
    foreign_core_version: "fork_foreign_core_version",
    foreign_data_hash: "fork_foreign_data_hash",
};

/// The records and the end line of an input log of this core version and data hash.
/// The whole log is read and checked before anything runs.
fn read_input_log(log: &Path, source: &Source) -> Result<(Vec<InputRecord>, LogEnd), CliError> {
    let invalid = |detail: String| CliError::new("input_log_invalid", detail);
    let text = files::read_text(log)?;
    let mut lines: Vec<&str> = text.lines().collect();
    let header: LogHeader = lines
        .first()
        .and_then(|line| files::parse_strict(line))
        .ok_or_else(|| invalid("line 1 is not the header of an input log".to_owned()))?;
    if header.format_version != files::FORMAT_VERSION {
        return Err(invalid(
            "line 1 is not the header of an input log".to_owned(),
        ));
    }
    let subject = source.subject;
    if header.core_version != CORE_VERSION {
        let detail = format!(
            "{subject} is of core version {}, this is {CORE_VERSION}",
            header.core_version
        );
        return Err(CliError::new(source.foreign_core_version, detail));
    }
    if header.data_hash != EMPTY_DATA_HASH {
        let detail = format!(
            "{subject} has data hash {:016x}, this is {EMPTY_DATA_HASH:016x}",
            header.data_hash
        );
        return Err(CliError::new(source.foreign_data_hash, detail));
    }
    let end: LogEnd = lines
        .pop()
        .filter(|_| !lines.is_empty())
        .and_then(files::parse_strict)
        .ok_or_else(|| invalid("the last line is not {\"end\":t}".to_owned()))?;
    let mut records = Vec::new();
    for (line, number) in lines[1..].iter().zip(2..) {
        let record: InputRecord = files::parse_strict(line)
            .ok_or_else(|| invalid(format!("line {number} is not a record of the input log")))?;
        records.push(record);
    }
    let last = records.last().map_or(SimTime::START, |record| record.t);
    if end.end < last {
        return Err(invalid(format!(
            "the end {} lies before the last input at {}",
            end.end.as_secs(),
            last.as_secs()
        )));
    }
    Ok((records, end))
}

fn read_manifest(path: &Path) -> Result<Manifest, CliError> {
    files::parse_strict(files::read_text(path)?.trim_end())
        .ok_or_else(|| CliError::new("manifest_invalid", "not a manifest"))
}

/// ADR-0004 section 4: a new world, per record `advance_until(t)` and `submit`, at the
/// end `advance_until(end)`.
fn replay(log: &Path, manifest: Option<&Path>, out: &Path) -> Result<(), CliError> {
    let invalid = |detail: String| CliError::new("input_log_invalid", detail);
    let (records, end) = read_input_log(log, &REPLAYED_LOG)?;
    let compare = match manifest {
        Some(path) => Compare::Replay(read_manifest(path)?),
        None => Compare::Nothing,
    };
    let mut records = records.into_iter();
    let Some(first) = records.next() else {
        return Err(invalid("the input log has no input".to_owned()));
    };
    // When a second input exists, this pattern no longer compiles. Then a test must
    // replay a log with two accepted inputs: the stamp check of `replay_input` cannot
    // fail before that, because every input after `Start` is rejected.
    let Input::Start(start) = &first.input;
    let scenario = Scenario {
        format_version: files::FORMAT_VERSION,
        start: start.clone(),
        until: end.end,
    };
    session::check_limit(first.t, end.end)?;
    let (t, seq) = (first.t, first.seq.get());
    let rejected = |code: String| session::replay_rejected(t, seq, &code);
    let mut session = Session::start(out, t, first.input.clone(), compare, rejected)?;
    session.check_stamp(t, seq)?;
    for record in records {
        if record.t > session.closed_through() {
            session.advance(record.t)?;
        }
        session.replay_input(record)?;
    }
    session.advance(end.end)?;
    session.finish(&scenario)
}

/// ADR-0001 section 6: a fork is a new run from t0 with the resolved scenario and the
/// seed of the parent. The inputs of the parent with a stamp before `T_X - 1 s` are
/// taken over; the year hashes 0 to X - 1 must equal those in the manifest of the
/// parent, and the run goes on to the end of the parent. An override is submitted at
/// `T_X - 1 s`; it arrives with the first world parameter. Without it and without
/// dropped inputs the fork is exactly the parent.
fn fork_run(parent: &Path, from_year: i64, out: &Path) -> Result<(), CliError> {
    let (records, end) = read_input_log(&parent.join(files::INPUT_LOG), &PARENT_RUN)?;
    let manifest = read_manifest(&parent.join(files::MANIFEST))?;
    let scenario: Scenario = files::parse_strict(&files::read_text(&parent.join(files::SCENARIO))?)
        .filter(|scenario: &Scenario| scenario.format_version == files::FORMAT_VERSION)
        .ok_or_else(|| CliError::new("scenario_invalid", "not a scenario of format version 1"))?;
    if manifest.closed_through != end.end {
        let detail = format!(
            "the manifest of the parent ends at {}, its input log at {}",
            manifest.closed_through.as_secs(),
            end.end.as_secs()
        );
        return Err(CliError::new("manifest_invalid", detail));
    }
    let out_of_range = |detail: String| {
        CliError::new(
            "fork_point_out_of_range",
            format!("from-year {from_year}: {detail}"),
        )
    };
    if from_year < 1 {
        return Err(out_of_range("a fork starts at year 1 or later".to_owned()));
    }
    // Year X may start one second after the end of the parent at the latest.
    let second = SimDuration::from_secs(1);
    let last_year = calendar(end.end + second).year;
    let years = u32::try_from(from_year)
        .ok()
        .filter(|_| from_year <= last_year);
    let Some(years) = years else {
        return Err(out_of_range(format!(
            "the parent run ends at {}, before year {from_year} starts",
            end.end.as_secs()
        )));
    };
    let hashes = manifest.year_hashes.len();
    if u64::try_from(hashes).is_ok_and(|hashes| hashes < u64::from(years)) {
        let detail = format!(
            "the manifest of the parent has {hashes} year hashes, the fork point needs {years}"
        );
        return Err(CliError::new("manifest_invalid", detail));
    }
    let fork_stamp = year_start(from_year) - second;
    let (kept, dropped): (Vec<InputRecord>, Vec<InputRecord>) = records
        .into_iter()
        .partition(|record| record.t < fork_stamp);
    let mut kept = kept.into_iter();
    let Some(first) = kept.next() else {
        return Err(CliError::new(
            "input_log_invalid",
            "the input log has no input before the fork point",
        ));
    };
    let Input::Start(start) = &first.input;
    if *start != scenario.start {
        return Err(CliError::new(
            "run_dir_inconsistent",
            "the resolved scenario of the parent does not start as its input log",
        ));
    }
    session::check_limit(first.t, end.end)?;
    let info = ForkInfo {
        parent_log_hash: manifest.log_hash,
        from_year: years,
        dropped_inputs: u64::try_from(dropped.len())
            .unwrap_or_else(|_| panic!("invariant cli.count_range")),
    };
    let compare = Compare::Fork {
        parent: manifest,
        info,
    };
    let (t, seq) = (first.t, first.seq.get());
    let rejected = |code: String| session::replay_rejected(t, seq, &code);
    let mut session = Session::start(out, t, first.input.clone(), compare, rejected)?;
    session.check_stamp(t, seq)?;
    for record in kept {
        if record.t > session.closed_through() {
            session.advance(record.t)?;
        }
        session.replay_input(record)?;
    }
    // The session compares year by year up to the fork point. An override will split
    // the run there: advance to `fork_stamp`, submit it, go on.
    session.advance(end.end)?;
    session.finish(&scenario)
}

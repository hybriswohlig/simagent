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
use sim_core::time::SimTime;
use sim_core::version::{CORE_VERSION, EMPTY_DATA_HASH};

use crate::error::CliError;
use crate::files::{LogEnd, LogHeader, Manifest, Scenario};
use crate::session::Session;

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
    Command::new("simagent")
        .version(env!("CARGO_PKG_VERSION"))
        .subcommand_required(true)
        .subcommand(run)
}

/// Runs the program with these arguments. A usage error ends the process through clap.
pub fn run_cli(args: impl IntoIterator<Item = OsString>) -> Result<(), CliError> {
    let matches = command().get_matches_from(args);
    let Some(("run", run)) = matches.subcommand() else {
        return Err(CliError::new("usage", "unknown subcommand"));
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
    let first = Input::Start(scenario.start.clone());
    let rejected = |code: String| CliError::new("input_rejected", code);
    let mut session = Session::start(out, stamp, first, None, rejected)?;
    session.advance(until.unwrap_or(scenario.until))?;
    session.finish(&scenario)
}

fn resume(dir: &Path, until: Option<SimTime>) -> Result<(), CliError> {
    let scenario: Scenario = files::parse_strict(&files::read_text(&dir.join(files::SCENARIO))?)
        .ok_or_else(|| CliError::new("scenario_invalid", "not a scenario of format version 1"))?;
    let mut session = Session::resume(dir)?;
    let limit = until.unwrap_or(scenario.until);
    if limit < session.closed_through() {
        let detail = format!(
            "the run stands at {}, until is {}",
            session.closed_through().as_secs(),
            limit.as_secs()
        );
        return Err(CliError::new("until_in_the_past", detail));
    }
    // Nothing has been written up to here: a refused resume leaves the run as it was.
    session.reopen_input_log()?;
    session.advance(limit)?;
    session.finish(&scenario)
}

/// ADR-0004 section 4: a new world, per record `advance_until(t)` and `submit`, at the
/// end `advance_until(end)`. The whole log is read and checked before anything runs.
fn replay(log: &Path, manifest: Option<&Path>, out: &Path) -> Result<(), CliError> {
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
    if header.core_version != CORE_VERSION {
        let detail = format!(
            "the input log is of core version {}, this is {CORE_VERSION}",
            header.core_version
        );
        return Err(CliError::new("replay_foreign_core_version", detail));
    }
    if header.data_hash != EMPTY_DATA_HASH {
        let detail = format!(
            "the input log has data hash {:016x}, this is {EMPTY_DATA_HASH:016x}",
            header.data_hash
        );
        return Err(CliError::new("replay_foreign_data_hash", detail));
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
    let expected = match manifest {
        Some(path) => {
            let manifest: Manifest = files::parse_strict(&files::read_text(path)?)
                .ok_or_else(|| CliError::new("manifest_invalid", "not a manifest"))?;
            Some(manifest.year_hashes)
        }
        None => None,
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
    let (t, seq) = (first.t, first.seq.get());
    let rejected = |code: String| session::replay_rejected(t, seq, &code);
    let mut session = Session::start(out, t, first.input.clone(), expected, rejected)?;
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

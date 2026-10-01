//! One run: the world, what its observers wrote, the log hash, and the run directory
//! that grows with every call (ADR-0004 section 6).

use std::fs;
use std::path::{Path, PathBuf};

use sim_core::canonical::YearHash;
use sim_core::input::{Input, InputRecord};
use sim_core::log::{LogEntry, LogHasher};
use sim_core::time::SimTime;
use sim_core::version::{CORE_VERSION, EMPTY_DATA_HASH};
use sim_core::world::{World, YearOut};

use crate::error::CliError;
use crate::files::{self, LogEnd, LogHeader, Manifest, Scenario};

/// Set at build time by whoever builds a release; informative only (ADR-0003 1.4).
pub const BUILD_COMMIT: &str = match option_env!("SIMAGENT_BUILD_COMMIT") {
    Some(commit) => commit,
    None => "unknown",
};

pub struct Session {
    world: World,
    out: YearOut,
    hasher: LogHasher,
    dir: PathBuf,
    /// Census rows already in `census.csv`.
    census_written: usize,
    /// Year hashes of the original run, for a replay with a manifest.
    expected: Option<Vec<YearHash>>,
}

pub fn header() -> LogHeader {
    LogHeader {
        format_version: files::FORMAT_VERSION,
        core_version: CORE_VERSION,
        data_hash: EMPTY_DATA_HASH,
        build_commit: BUILD_COMMIT.to_owned(),
    }
}

/// ADR-0004 section 4: a replay ends if an input of the log is rejected.
pub fn replay_rejected(t: SimTime, seq: u64, code: &str) -> CliError {
    CliError::new(
        "replay_input_rejected",
        format!("input {seq} at t {} was rejected: {code}", t.as_secs()),
    )
}

fn record_line(record: &InputRecord) -> String {
    files::json(record) + "\n"
}

impl Session {
    /// A new run in a directory that does not exist yet or is empty. The first input
    /// is applied at the stamp `first_t` before anything is written: a rejected scenario
    /// leaves no trace. `rejected` turns the reason code into the caller's error.
    pub fn start(
        dir: &Path,
        first_t: SimTime,
        first: Input,
        expected: Option<Vec<YearHash>>,
        rejected: impl FnOnce(String) -> CliError,
    ) -> Result<Session, CliError> {
        let occupied = fs::read_dir(dir).is_ok_and(|mut entries| entries.next().is_some());
        if occupied {
            return Err(CliError::new(
                "out_dir_not_empty",
                "the output directory already has content",
            ));
        }
        let mut world = World::new();
        if first_t > world.closed_through() {
            world.advance_until(&mut YearOut::default(), first_t, &mut |_| false);
        }
        world
            .submit(first)
            .map_err(|error| rejected(files::code(&error.code)))?;
        fs::create_dir_all(dir).map_err(|error| CliError::io(dir, &error))?;
        let mut session = Session {
            world,
            out: YearOut::default(),
            hasher: LogHasher::new(),
            dir: dir.to_path_buf(),
            census_written: 0,
            expected,
        };
        let mut log = files::json(&header()) + "\n";
        for record in session.world.input_log() {
            log.push_str(&record_line(record));
        }
        files::write(&session.path(files::INPUT_LOG), log.as_bytes())?;
        files::write(&session.path(files::CENSUS), b"year,living,lived_secs\n")?;
        files::write(&session.path(files::EVENTS), b"")?;
        session.drain()?;
        Ok(session)
    }

    /// Continues the run in `dir` from its save, in a fresh process. The log hash runs
    /// over the whole log, so the entries written so far are read back (ADR-0004
    /// section 8).
    pub fn resume(dir: &Path) -> Result<Session, CliError> {
        let bytes = files::read(&dir.join(files::SAVE))?;
        let (world, year_hashes) = World::load(&bytes, EMPTY_DATA_HASH)
            .map_err(|error| CliError::new("save_rejected", files::code(&error)))?;
        let mut hasher = LogHasher::new();
        let events = files::read_text(&dir.join(files::EVENTS))?;
        for (line, number) in events.lines().zip(1..) {
            let entry: LogEntry = files::parse_strict(line).ok_or_else(|| {
                CliError::new(
                    "events_invalid",
                    format!("line {number} is not a log entry"),
                )
            })?;
            hasher.update(core::slice::from_ref(&entry));
        }
        Ok(Session {
            world,
            out: YearOut {
                census: Vec::new(),
                year_hashes,
            },
            hasher,
            dir: dir.to_path_buf(),
            census_written: 0,
            expected: None,
        })
    }

    /// The input log of an orderly run ends with `{"end":t}`. Before a resumed run
    /// writes anything else, the log is open again: header and records, no end line.
    pub fn reopen_input_log(&self) -> Result<(), CliError> {
        let mut log = files::json(&header()) + "\n";
        for record in self.world.input_log() {
            log.push_str(&record_line(record));
        }
        files::write(&self.path(files::INPUT_LOG), log.as_bytes())
    }

    /// Compares the stamp of the input applied last with the recorded one.
    pub fn check_stamp(&self, t: SimTime, seq: u64) -> Result<(), CliError> {
        let Some(applied) = self.world.input_log().last() else {
            return Err(CliError::new(
                "replay_stamp_mismatch",
                "no input was applied",
            ));
        };
        if (applied.t, applied.seq.get()) == (t, seq) {
            return Ok(());
        }
        Err(CliError::new(
            "replay_stamp_mismatch",
            format!(
                "the log has t {} seq {seq}, the replay stamped t {} seq {}",
                t.as_secs(),
                applied.t.as_secs(),
                applied.seq.get()
            ),
        ))
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    pub fn closed_through(&self) -> SimTime {
        self.world.closed_through()
    }

    /// Applies one input of a replay and compares its stamp with the recorded one.
    pub fn replay_input(&mut self, record: InputRecord) -> Result<(), CliError> {
        let (t, seq) = (record.t, record.seq.get());
        self.world
            .submit(record.input)
            .map_err(|rejected| replay_rejected(t, seq, &files::code(&rejected.code)))?;
        if let Some(applied) = self.world.input_log().last() {
            files::append(&self.path(files::INPUT_LOG), &record_line(applied))?;
        }
        self.drain()?;
        self.check_stamp(t, seq)
    }

    /// Runs to `limit`, which is not before the clock, and writes what the run produced.
    pub fn advance(&mut self, limit: SimTime) -> Result<(), CliError> {
        self.world
            .advance_until(&mut self.out, limit, &mut |_| false);
        self.drain()?;
        self.compare_with_the_manifest()
    }

    /// Hands the new log entries and census rows to the files.
    fn drain(&mut self) -> Result<(), CliError> {
        let log = self.world.take_log();
        self.hasher.update(&log);
        let lines: String = log.iter().map(|entry| files::json(entry) + "\n").collect();
        files::append(&self.path(files::EVENTS), &lines)?;
        let rows: String = self.out.census[self.census_written..]
            .iter()
            .map(|row| format!("{},{},{}\n", row.year, row.living, row.lived_secs))
            .collect();
        self.census_written = self.out.census.len();
        files::append(&self.path(files::CENSUS), &rows)
    }

    /// ADR-0003 1.4: a replay ends at the first year whose hash differs.
    fn compare_with_the_manifest(&self) -> Result<(), CliError> {
        let Some(expected) = &self.expected else {
            return Ok(());
        };
        for (got, want) in self.out.year_hashes.iter().zip(expected) {
            if got != want {
                return Err(CliError::new(
                    "replay_diverged",
                    format!(
                        "year {} has hash {:016x}, the manifest says {:016x}",
                        got.year, got.hash, want.hash
                    ),
                ));
            }
        }
        Ok(())
    }

    /// The orderly end: the end line, the manifest, the save, the scenario.
    pub fn finish(self, scenario: &Scenario) -> Result<(), CliError> {
        if let Some(expected) = &self.expected
            && expected.len() != self.out.year_hashes.len()
        {
            return Err(CliError::new(
                "replay_diverged",
                format!(
                    "the replay has {} year hashes, the manifest {}",
                    self.out.year_hashes.len(),
                    expected.len()
                ),
            ));
        }
        let end = LogEnd {
            end: self.world.closed_through(),
        };
        files::append(&self.path(files::INPUT_LOG), &(files::json(&end) + "\n"))?;
        let manifest = Manifest {
            format_version: files::FORMAT_VERSION,
            core_version: CORE_VERSION,
            data_hash: EMPTY_DATA_HASH,
            build_commit: BUILD_COMMIT.to_owned(),
            world_seed: scenario.start.world_seed,
            closed_through: self.world.closed_through(),
            year_hashes: self.out.year_hashes.clone(),
            log_hash: self.hasher.digest(),
        };
        let manifest = files::json(&manifest) + "\n";
        files::write(&self.path(files::MANIFEST), manifest.as_bytes())?;
        let save = self
            .world
            .save(&self.out.year_hashes)
            .map_err(|error| CliError::new("save_failed", files::code(&error)))?;
        files::write(&self.path(files::SAVE), &save)?;
        let resolved = files::json(scenario) + "\n";
        files::write(&self.path(files::SCENARIO), resolved.as_bytes())
    }
}

//! One run: the world, what its observers wrote, the log hash, and the run directory
//! that grows with every call (ADR-0004 section 6).

use std::fs;
use std::path::{Path, PathBuf};

use sim_core::body_contract::CLAMPED_KEYS;
use sim_core::input::{Input, InputRecord};
use sim_core::log::{LogEntry, LogHasher};
use sim_core::time::{MAX_ABS_SECS, SimDuration, SimTime, YEAR, calendar, year_start};
use sim_core::version::CORE_VERSION;
use sim_core::world::{CensusRow, World, YearOut};

use crate::error::CliError;
use crate::files::{self, ForkInfo, LogEnd, LogHeader, Manifest, Scenario};

/// Set at build time by whoever builds a release; informative only (ADR-0003 1.4).
pub const BUILD_COMMIT: &str = match option_env!("SIMAGENT_BUILD_COMMIT") {
    Some(commit) => commit,
    None => "unknown",
};

/// What a run compares its year hashes with.
pub enum Compare {
    Nothing,
    /// A replay: every year and at the end the log hash of the original (ADR-0003 1.4).
    Replay(Manifest),
    /// A fork: the years and the events before the fork point with the parent (ADR-0001
    /// section 6).
    Fork {
        parent: Manifest,
        info: ForkInfo,
        /// The start of year X: entries before it are those of the parent.
        fork_point: SimTime,
        /// The log hash over the entries of the parent before the fork point.
        events_before: u64,
        /// The fork overrides nothing and drops no input: it is its parent throughout.
        is_its_parent: bool,
    },
}

pub struct Session {
    world: World,
    out: YearOut,
    hasher: LogHasher,
    /// Over the entries before the fork point of a fork.
    before_fork: LogHasher,
    dir: PathBuf,
    /// Census rows already in `census.csv`.
    census_written: usize,
    /// Year hashes already compared with the manifest.
    years_compared: usize,
    compare: Compare,
    /// The block `fork` of the manifest, if this run is a fork: also after a resume.
    fork: Option<ForkInfo>,
}

/// The hash of the log in `events.ndjson` of a run directory (ADR-0004 section 8).
pub fn log_hash_of_events(dir: &Path) -> Result<u64, CliError> {
    let mut hasher = LogHasher::new();
    read_events_into(dir, &mut hasher, None)?;
    Ok(hasher.digest())
}

/// The same hash over the entries with a time before `before` only.
pub fn log_hash_of_events_before(dir: &Path, before: SimTime) -> Result<u64, CliError> {
    let mut hasher = LogHasher::new();
    read_events_into(dir, &mut hasher, Some(before))?;
    Ok(hasher.digest())
}

fn read_events_into(
    dir: &Path,
    hasher: &mut LogHasher,
    before: Option<SimTime>,
) -> Result<(), CliError> {
    let events = files::read_text(&dir.join(files::EVENTS))?;
    for (line, number) in events.lines().zip(1..) {
        let entry: LogEntry = files::parse_strict(line).ok_or_else(|| {
            CliError::new(
                "events_invalid",
                format!("line {number} is not a log entry"),
            )
        })?;
        if before.is_none_or(|before| entry.t < before) {
            hasher.update(core::slice::from_ref(&entry));
        }
    }
    Ok(())
}

/// A limit a run can reach: not before the clock, and not in the last year of the time
/// range. There the core could not plan the next year end or the next birthday; it
/// would break `time.out_of_range` (ADR-0002).
pub fn check_limit(closed: SimTime, limit: SimTime) -> Result<(), CliError> {
    if limit < closed {
        let detail = format!(
            "the run stands at {}, until is {}",
            closed.as_secs(),
            limit.as_secs()
        );
        return Err(CliError::new("until_in_the_past", detail));
    }
    let horizon = MAX_ABS_SECS - YEAR.as_secs();
    if limit.as_secs() > horizon {
        let detail = format!(
            "until {} is later than {horizon}, one year before the end of the time range",
            limit.as_secs()
        );
        return Err(CliError::new("until_beyond_horizon", detail));
    }
    Ok(())
}

/// The last second of the year that begins or continues right after `closed`.
fn end_of_the_open_year(closed: SimTime) -> SimTime {
    let second = SimDuration::from_secs(1);
    year_start(calendar(closed + second).year + 1) - second
}

pub fn header(data_hash: u64) -> LogHeader {
    LogHeader {
        format_version: files::FORMAT_VERSION,
        core_version: CORE_VERSION,
        data_hash,
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
        compare: Compare,
        rejected: impl FnOnce(String) -> CliError,
    ) -> Result<Session, CliError> {
        let occupied = fs::read_dir(dir).is_ok_and(|mut entries| entries.next().is_some());
        if occupied {
            return Err(CliError::new(
                "out_dir_not_empty",
                "the output directory already has content",
            ));
        }
        let mut world = World::new(crate::data::core_data()?);
        let mut before = YearOut::default();
        // A break-off before the first input writes the package into `dir` too.
        if first_t > world.closed_through() {
            crate::repro::guarded(&mut world, &mut before, dir, None, |world, out| {
                world.advance_until(out, first_t, &mut |_| false);
            })?;
        }
        let pending = first.clone();
        crate::repro::guarded(&mut world, &mut before, dir, Some(&pending), |world, _| {
            world.submit(first)
        })?
        .map_err(|error| rejected(files::code(&error.code)))?;
        fs::create_dir_all(dir).map_err(|error| CliError::io(dir, &error))?;
        let fork = match &compare {
            Compare::Fork { info, .. } => Some(info.clone()),
            Compare::Nothing | Compare::Replay(_) => None,
        };
        let mut session = Session {
            world,
            out: YearOut::default(),
            hasher: LogHasher::new(),
            before_fork: LogHasher::new(),
            dir: dir.to_path_buf(),
            census_written: 0,
            years_compared: 0,
            compare,
            fork,
        };
        let mut log = files::json(&header(session.world.data().hash())) + "\n";
        for record in session.world.input_log() {
            log.push_str(&record_line(record));
        }
        files::write(&session.path(files::INPUT_LOG), log.as_bytes())?;
        files::write(
            &session.path(files::CENSUS),
            (census_header() + "\n").as_bytes(),
        )?;
        files::write(&session.path(files::EVENTS), b"")?;
        session.drain()?;
        Ok(session)
    }

    /// Continues the run in `dir` from its save, in a fresh process. The log hash runs
    /// over the whole log, so the entries written so far are read back (ADR-0004
    /// section 8).
    pub fn resume(dir: &Path) -> Result<Session, CliError> {
        let inconsistent = |detail: String| CliError::new("run_dir_inconsistent", detail);
        let bytes = files::read(&dir.join(files::SAVE))?;
        let (world, year_hashes) = World::load(&bytes, crate::data::core_data()?)
            .map_err(|error| CliError::new("save_rejected", files::code(&error)))?;
        // A run that was interrupted while it wrote has lines in its files that the
        // save does not know. The manifest of the last orderly end tells them apart.
        let manifest: Manifest =
            files::parse_strict(files::read_text(&dir.join(files::MANIFEST))?.trim_end())
                .ok_or_else(|| CliError::new("manifest_invalid", "not a manifest"))?;
        let describes_the_save = manifest.closed_through == world.closed_through()
            && manifest.year_hashes == year_hashes
            && manifest.core_version == CORE_VERSION
            && manifest.data_hash == world.data().hash();
        if !describes_the_save {
            return Err(inconsistent(
                "manifest.json does not describe the save".to_owned(),
            ));
        }
        let mut hasher = LogHasher::new();
        read_events_into(dir, &mut hasher, None)?;
        if hasher.digest() != manifest.log_hash {
            return Err(inconsistent(
                "events.ndjson does not match the log hash of the manifest".to_owned(),
            ));
        }
        let census = files::read_text(&dir.join(files::CENSUS))?;
        if census.lines().next() != Some(census_header().as_str()) {
            return Err(inconsistent(
                "census.csv does not start with the header of this version".to_owned(),
            ));
        }
        let rows = census.lines().count().saturating_sub(1);
        if rows != year_hashes.len() {
            return Err(inconsistent(format!(
                "census.csv has {rows} rows, the save has {} completed years",
                year_hashes.len()
            )));
        }
        Ok(Session {
            world,
            out: YearOut {
                census: Vec::new(),
                year_hashes,
                trace: Vec::new(),
            },
            hasher,
            before_fork: LogHasher::new(),
            dir: dir.to_path_buf(),
            census_written: 0,
            years_compared: 0,
            compare: Compare::Nothing,
            fork: manifest.fork,
        })
    }

    /// The input log of an orderly run ends with `{"end":t}`. Before a resumed run
    /// writes anything else, the log is open again: header and records, no end line.
    fn reopen_input_log(&self) -> Result<(), CliError> {
        let mut log = files::json(&header(self.world.data().hash())) + "\n";
        for record in self.world.input_log() {
            log.push_str(&record_line(record));
        }
        files::write(&self.path(files::INPUT_LOG), log.as_bytes())
    }

    /// Applies inputs at the stop of a resumed run and opens the input log again. All of
    /// them are applied before anything is written: a rejected one ends the run with its
    /// code, and the files stay as they were.
    pub fn submit_at_stop(&mut self, inputs: Vec<Input>) -> Result<(), CliError> {
        let dir = self.dir.clone();
        for input in inputs {
            let pending = input.clone();
            crate::repro::guarded(
                &mut self.world,
                &mut self.out,
                &dir,
                Some(&pending),
                |world, _| world.submit(input),
            )?
            .map_err(|error| CliError::new("input_rejected", files::code(&error.code)))?;
        }
        self.reopen_input_log()?;
        self.drain()
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
        let dir = self.dir.clone();
        let pending = record.input.clone();
        crate::repro::guarded(
            &mut self.world,
            &mut self.out,
            &dir,
            Some(&pending),
            |world, _| world.submit(record.input),
        )?
        .map_err(|rejected| replay_rejected(t, seq, &files::code(&rejected.code)))?;
        if let Some(applied) = self.world.input_log().last() {
            files::append(&self.path(files::INPUT_LOG), &record_line(applied))?;
        }
        self.drain()?;
        self.check_stamp(t, seq)
    }

    /// Applies an input of this run's own (the override of a fork) and records it.
    pub fn submit(
        &mut self,
        input: Input,
        rejected: impl FnOnce(String) -> CliError,
    ) -> Result<(), CliError> {
        let dir = self.dir.clone();
        let pending = input.clone();
        crate::repro::guarded(
            &mut self.world,
            &mut self.out,
            &dir,
            Some(&pending),
            |world, _| world.submit(input),
        )?
        .map_err(|error| rejected(files::code(&error.code)))?;
        if let Some(applied) = self.world.input_log().last() {
            files::append(&self.path(files::INPUT_LOG), &record_line(applied))?;
        }
        self.drain()
    }

    /// Runs to `limit` and writes what the run produced. A replay with a manifest goes
    /// year by year and ends at the first year whose hash differs (ADR-0003 1.4); so
    /// does a fork up to its fork point.
    pub fn advance(&mut self, limit: SimTime) -> Result<(), CliError> {
        check_limit(self.world.closed_through(), limit)?;
        while self.world.closed_through() < limit {
            let year_by_year = self.years_compared < self.years_to_compare();
            let step = if year_by_year {
                limit.min(end_of_the_open_year(self.world.closed_through()))
            } else {
                limit
            };
            let dir = self.dir.clone();
            crate::repro::guarded(&mut self.world, &mut self.out, &dir, None, |world, out| {
                world.advance_until(out, step, &mut |_| false);
            })?;
            self.drain()?;
            self.compare_with_the_manifest()?;
        }
        Ok(())
    }

    /// Hands the new log entries and census rows to the files.
    fn drain(&mut self) -> Result<(), CliError> {
        let log = self.world.take_log();
        self.hasher.update(&log);
        if let Compare::Fork { fork_point, .. } = &self.compare {
            for entry in log.iter().filter(|entry| entry.t < *fork_point) {
                self.before_fork.update(core::slice::from_ref(entry));
            }
        }
        let lines: String = log.iter().map(|entry| files::json(entry) + "\n").collect();
        files::append(&self.path(files::EVENTS), &lines)?;
        let rows: String = self.out.census[self.census_written..]
            .iter()
            .map(|row| census_line(row) + "\n")
            .collect();
        self.census_written = self.out.census.len();
        files::append(&self.path(files::CENSUS), &rows)
    }

    /// How many years from year 0 on are compared: all of a replay, those before the
    /// fork point of a fork.
    fn years_to_compare(&self) -> usize {
        match &self.compare {
            Compare::Nothing => 0,
            Compare::Replay(_) => usize::MAX,
            Compare::Fork { info, .. } => usize::try_from(info.from_year).unwrap_or(usize::MAX),
        }
    }

    /// Compares the year hashes that are new since the last call with the manifest.
    fn compare_with_the_manifest(&mut self) -> Result<(), CliError> {
        let (expected, code, whose) = match &self.compare {
            Compare::Nothing => return Ok(()),
            Compare::Replay(manifest) => (manifest, "replay_diverged", "the manifest"),
            Compare::Fork { parent, .. } => (
                parent,
                "fork_diverged_before_fork_point",
                "the manifest of the parent",
            ),
        };
        let until = self.years_to_compare().min(self.out.year_hashes.len());
        let from = self.years_compared.min(until);
        let produced = &self.out.year_hashes[from..until];
        let wanted = expected.year_hashes.iter().skip(from);
        for (got, want) in produced.iter().zip(wanted) {
            if got != want {
                let detail = format!(
                    "year {} has hash {:016x}, {whose} says {:016x}",
                    got.year, got.hash, want.hash
                );
                // A diverged replay leaves a repro package (ADR-0016); a fork does not.
                if let Compare::Replay(manifest) = &self.compare {
                    let up_to = manifest
                        .year_hashes
                        .iter()
                        .take_while(|hash| hash.year <= got.year)
                        .copied()
                        .collect();
                    let path =
                        crate::repro::diverged(&self.world, &self.out, up_to, &detail, &self.dir)?;
                    return Err(CliError::new(
                        code,
                        format!("{detail} (repro package: {})", path.display()),
                    ));
                }
                return Err(CliError::new(code, detail));
            }
        }
        self.years_compared = self.out.year_hashes.len();
        Ok(())
    }

    /// The orderly end: the end line, the manifest, the save, the scenario.
    pub fn finish(self, scenario: &Scenario) -> Result<(), CliError> {
        if let Compare::Replay(expected) = &self.compare {
            let (got, want) = (self.out.year_hashes.len(), expected.year_hashes.len());
            if got != want {
                let detail = format!("the replay has {got} year hashes, the manifest {want}");
                return Err(CliError::new("replay_diverged", detail));
            }
            let (got, want) = (self.hasher.digest(), expected.log_hash);
            if got != want {
                let detail = format!("the log hash is {got:016x}, the manifest says {want:016x}");
                return Err(CliError::new("replay_diverged", detail));
            }
        }
        // The fork names its parent by the hash of the parent's log. What ties that hash
        // to the history the fork has run: up to the fork point both logs are the same,
        // and a fork that overrides nothing and drops no input is its parent throughout.
        if let Compare::Fork {
            parent,
            events_before,
            is_its_parent,
            ..
        } = &self.compare
        {
            let (got, want) = (self.before_fork.digest(), *events_before);
            if got != want {
                let detail = format!(
                    "the events before the fork point have log hash {got:016x}, those of the parent {want:016x}"
                );
                return Err(CliError::new("fork_parent_inconsistent", detail));
            }
            let (got, want) = (self.hasher.digest(), parent.log_hash);
            if *is_its_parent && got != want {
                let detail = format!(
                    "a fork without override is its parent: its log hash is {got:016x}, the manifest of the parent says {want:016x}"
                );
                return Err(CliError::new("fork_parent_inconsistent", detail));
            }
        }
        let end = LogEnd {
            end: self.world.closed_through(),
        };
        files::append(&self.path(files::INPUT_LOG), &(files::json(&end) + "\n"))?;
        let manifest = Manifest {
            format_version: files::FORMAT_VERSION,
            core_version: CORE_VERSION,
            data_hash: self.world.data().hash(),
            build_commit: BUILD_COMMIT.to_owned(),
            world_seed: scenario.start.world_seed,
            closed_through: self.world.closed_through(),
            year_hashes: self.out.year_hashes.clone(),
            log_hash: self.hasher.digest(),
            fork: self.fork.clone(),
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

/// The header of `census.csv` (ADR-0004 section 8): the three columns of every run, then
/// the bodies of the living (ADR-0020 section 7): evaluations, the clamps per parameter
/// of `CLAMPED_KEYS` and the held growth curves.
pub fn census_header() -> String {
    let mut header = String::from("year,living,lived_secs,body_evaluated");
    for key in CLAMPED_KEYS {
        header.push_str(",clamped_");
        header.push_str(key.as_str());
    }
    header.push_str(",curve_clamped");
    header
}

/// One row of `census.csv` in the order of `census_header`.
pub fn census_line(row: &CensusRow) -> String {
    let mut line = format!(
        "{},{},{},{}",
        row.year, row.living, row.lived_secs, row.body.evaluated
    );
    for count in row.body.clamped {
        line.push_str(&format!(",{count}"));
    }
    line.push_str(&format!(",{}", row.body.curve_clamped));
    line
}

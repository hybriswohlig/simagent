//! The lock of a run directory (ADR-0021 section 1): whoever writes a run holds a lock of
//! the operating system on `<run>.lock` beside its directory (beside, because
//! `Session::start` takes only an empty directory): the server while it has the run open,
//! the command line for the duration of its command. A locked run is not opened. The lock
//! ends with the process, also after a crash; there is no stale lock.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

use crate::error::CliError;

/// A held lock; it ends when the value goes or the process ends.
#[derive(Debug)]
pub struct RunLock {
    _file: File,
}

/// `<run>.lock` beside the directory `dir`, by the directory's real path: a link to a run
/// takes the run's own lock. A run that does not exist yet is named by its parent's real
/// path and its own name.
pub fn lock_path(dir: &Path) -> Result<PathBuf, CliError> {
    let absolute = std::path::absolute(dir).map_err(|e| CliError::io(dir, &e))?;
    let real = match std::fs::canonicalize(&absolute) {
        Ok(real) => real,
        Err(_) => {
            let name = absolute.file_name().unwrap_or_default();
            match absolute.parent().map(std::fs::canonicalize) {
                Some(Ok(parent)) => parent.join(name),
                _ => absolute.clone(),
            }
        }
    };
    let name = real
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            CliError::new("usage", format!("{} names no run directory", dir.display()))
        })?;
    Ok(real.with_file_name(format!("{name}.lock")))
}

impl RunLock {
    /// Takes the lock of the run in `dir`, or refuses with `run_locked` if another
    /// process holds it.
    pub fn acquire(dir: &Path) -> Result<RunLock, CliError> {
        let path = lock_path(dir)?;
        // A new run's directory may not exist yet, nor its parent: `run --out a/b/c`
        // creates them, so the lock beside it needs its parent first.
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| CliError::io(parent, &e))?;
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(|e| CliError::io(&path, &e))?;
        match file.try_lock() {
            Ok(()) => Ok(RunLock { _file: file }),
            Err(TryLockError::WouldBlock) => Err(CliError::new(
                "run_locked",
                format!("{} is in use by another process", dir.display()),
            )),
            Err(TryLockError::Error(error)) => Err(CliError::io(&path, &error)),
        }
    }
}

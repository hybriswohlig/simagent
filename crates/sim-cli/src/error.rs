//! Errors of the command line runner: a stable code and a detail for the reader.

use std::fmt;
use std::path::Path;

/// Printed as `error: <code>: <detail>`. Scripts match on the code.
#[derive(Debug)]
pub struct CliError {
    pub code: &'static str,
    pub detail: String,
}

impl CliError {
    pub fn new(code: &'static str, detail: impl Into<String>) -> CliError {
        CliError {
            code,
            detail: detail.into(),
        }
    }

    pub fn io(path: &Path, error: &std::io::Error) -> CliError {
        CliError::new("io", format!("{}: {error}", path.display()))
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error: {}: {}", self.code, self.detail)
    }
}

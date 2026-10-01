//! `simagent`: see the library crate. A failed command prints `error: <code>: <detail>`
//! and ends with status 1.

use std::process::ExitCode;

fn main() -> ExitCode {
    match sim_cli::run_cli(std::env::args_os()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

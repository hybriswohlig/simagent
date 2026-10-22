//! The browser smoke test of the gate (ADR-0021 section 4 and its acceptance): a fresh run
//! of the gate scenario made by the release binary of this commit up to the end of year 28,
//! `simagent view` over it with `web/dist`, and `web/smoke/smoke.mjs` driving Chromium
//! headless through it (run page, curves, one year, a person, a parent in the pedigree). A
//! canary first: the same drive against a copy of the page whose `main.js` throws must
//! fail. The browser comes from `cargo xtask browser`, which installs the Chromium headless
//! shell of the pinned `playwright-core`; the gate itself downloads nothing.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::{GATE_SCENARIO, Result, Running, cargo, start_view, web};

/// The run the drive opens.
const RUN: &str = "town";
/// The last second of year 28 of the gate scenario, a year before its horizon: the drive
/// steps one year.
const UNTIL: &str = "912038399";
/// The longest a drive may take; its own steps wait at most ten seconds each.
const DRIVE_TIME: Duration = Duration::from_secs(120);

/// `cargo xtask browser [--with-deps]`: the Chromium headless shell of the pinned
/// `playwright-core` (from Playwright's download, not npm), with the system libraries it
/// needs on Linux when asked.
pub fn browser(root: &Path, with_deps: bool) -> Result {
    let dir = root.join("web");
    web::run(
        &dir,
        "npm",
        &["ci", "--ignore-scripts", "--no-audit", "--no-fund"],
    )?;
    let mut args = vec![
        "node_modules/playwright-core/cli.js",
        "install",
        "--only-shell",
    ];
    if with_deps {
        args.push("--with-deps");
    }
    args.push("chromium");
    web::run(&dir, "node", &args)
}

/// The step of the gate; `web/dist` is built by the step `web` before it.
pub fn smoke(root: &Path) -> Result {
    let out = root.join("target").join("xtask-smoke");
    if out.exists() {
        std::fs::remove_dir_all(&out)
            .map_err(|e| format!("cannot remove {}: {e}", out.display()))?;
    }
    let runs = out.join("runs");
    let town = runs.join(RUN).display().to_string();
    cargo(
        root,
        &[
            "run",
            "--quiet",
            "--locked",
            "--release",
            "--package",
            "sim-cli",
            "--",
            "run",
            "--scenario",
            GATE_SCENARIO,
            "--until",
            UNTIL,
            "--out",
            &town,
        ],
    )?;
    let dist = root.join("web").join("dist");
    let broken = out.join("broken");
    copy_dir(&dist, &broken)?;
    let main = broken.join("main.js");
    std::fs::write(&main, "throw new Error(\"canary\");\n")
        .map_err(|e| format!("{}: {e}", main.display()))?;
    if drive(root, &runs, &broken, true).is_ok() {
        return Err("canary: the smoke test passed a page whose main.js throws".to_string());
    }
    drive(root, &runs, &dist, false)
}

/// One drive of the script against `simagent view` over `runs` with the page in `page`;
/// `quiet` for the canary, whose failure is expected and says nothing.
fn drive(root: &Path, runs: &Path, page: &Path, quiet: bool) -> Result {
    let (server, port) = start_view(root, runs, page)?;
    let mut command = Command::new("node");
    command
        .arg("smoke/smoke.mjs")
        .arg(format!("http://127.0.0.1:{port}/"))
        .arg(RUN)
        .current_dir(root.join("web"))
        .env_clear();
    if quiet {
        command.stdout(Stdio::null()).stderr(Stdio::null());
    }
    // Node finds itself on the path and Playwright its browsers below the home directory, or
    // where the CI put them.
    for name in ["PATH", "HOME", "PLAYWRIGHT_BROWSERS_PATH"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    let child = command
        .spawn()
        .map_err(|e| format!("cannot run node: {e}"))?;
    let mut node = Running(child);
    let until = Instant::now() + DRIVE_TIME;
    let status = loop {
        if let Some(status) = node.0.try_wait().map_err(|e| format!("node: {e}"))? {
            break status;
        }
        if Instant::now() > until {
            return Err(format!("the smoke test took longer than {DRIVE_TIME:?}"));
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    drop(server);
    if status.success() {
        Ok(())
    } else {
        Err(format!("the smoke test failed against {}", page.display()))
    }
}

/// A copy of the directory `from` at `to`, files and directories only.
fn copy_dir(from: &Path, to: &Path) -> Result {
    std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    let entries = std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", from.display()))?;
        let (source, target) = (entry.path(), to.join(entry.file_name()));
        let kind = entry
            .file_type()
            .map_err(|e| format!("{}: {e}", source.display()))?;
        if kind.is_dir() {
            copy_dir(&source, &target)?;
        } else if kind.is_file() {
            std::fs::copy(&source, &target).map_err(|e| format!("{}: {e}", source.display()))?;
        } else {
            return Err(format!(
                "{}: neither a file nor a directory",
                source.display()
            ));
        }
    }
    Ok(())
}

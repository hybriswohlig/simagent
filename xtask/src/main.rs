//! Project automation (ADR-0000). `cargo xtask ci` is the gate: the CI, the pre-push
//! hook and every agent run exactly this command.

mod checks;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// Target without std: building the core for it proves the core does not use std.
const NO_STD_TARGET: &str = "wasm32v1-none";
const NO_STD_CRATES: [&str; 2] = ["sim-math", "sim-core"];
/// Environment variables through which compiler flags reach rustc.
const FLAG_VARIABLES: [&str; 3] = [
    "RUSTFLAGS",
    "CARGO_ENCODED_RUSTFLAGS",
    "CARGO_BUILD_RUSTFLAGS",
];

type Result<T = ()> = std::result::Result<T, String>;

fn main() -> ExitCode {
    let command = std::env::args().nth(1);
    let result = match command.as_deref() {
        Some("ci") => gate(true),
        Some("check") => gate(false),
        _ => {
            eprintln!(
                "usage: cargo xtask <command>\n  ci     the gate; requires a clean working tree (CI, pre-push)\n  check  the same gate on a dirty working tree (local iteration)"
            );
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => {
            println!("xtask: gate green");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("xtask: gate red: {message}");
            ExitCode::FAILURE
        }
    }
}

fn gate(require_clean_tree: bool) -> Result {
    let root = workspace_root()?;
    step("location", || check_location(&root))?;
    if require_clean_tree {
        step("clean working tree", || check_clean_tree(&root))?;
    }
    step("compiler flags", || check_compiler_flags(&root))?;
    step("document limits", || check_document_limits(&root))?;
    step("fmt", || cargo(&root, &["fmt", "--all", "--check"]))?;
    step("clippy", || {
        cargo(
            &root,
            &[
                "clippy",
                "--workspace",
                "--all-targets",
                "--locked",
                "--",
                "-D",
                "warnings",
            ],
        )
    })?;
    step("test (debug)", || {
        cargo(&root, &["test", "--workspace", "--locked"])
    })?;
    step("test (release)", || {
        cargo(&root, &["test", "--workspace", "--locked", "--release"])
    })?;
    step("no-std build", || {
        let mut args = vec!["build", "--locked", "--target", NO_STD_TARGET];
        for krate in NO_STD_CRATES {
            args.extend(["--package", krate]);
        }
        cargo(&root, &args)
    })
}

fn step(name: &str, check: impl FnOnce() -> Result) -> Result {
    println!("== {name}");
    check().map_err(|message| format!("{name}: {message}"))
}

fn workspace_root() -> Result<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "xtask has no parent directory".to_string())
}

fn check_location(root: &Path) -> Result {
    let path = root.to_string_lossy();
    if checks::is_icloud_path(&path) {
        return Err(format!(
            "{path} is synced by iCloud Drive; move the repository"
        ));
    }
    Ok(())
}

fn check_clean_tree(root: &Path) -> Result {
    let output = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=all"])
        .current_dir(root)
        .output()
        .map_err(|error| format!("cannot run git: {error}"))?;
    if !output.status.success() {
        return Err("git status failed".to_string());
    }
    let changes = String::from_utf8_lossy(&output.stdout);
    if changes.trim().is_empty() {
        Ok(())
    } else {
        Err(format!(
            "uncommitted changes (commit them or use `cargo xtask check`):\n{changes}"
        ))
    }
}

fn check_compiler_flags(root: &Path) -> Result {
    let mut findings = Vec::new();
    for variable in FLAG_VARIABLES {
        if let Ok(value) = std::env::var(variable) {
            for flag in checks::forbidden_flags_in(&value) {
                findings.push(format!("{flag} in ${variable}"));
            }
        }
    }
    for (variable, value) in std::env::vars() {
        if variable.starts_with("CARGO_TARGET_") && variable.ends_with("_RUSTFLAGS") {
            for flag in checks::forbidden_flags_in(&value) {
                findings.push(format!("{flag} in ${variable}"));
            }
        }
    }
    let config = root.join(".cargo").join("config.toml");
    if let Ok(text) = std::fs::read_to_string(&config) {
        for flag in checks::forbidden_flags_in(&text) {
            findings.push(format!("{flag} in .cargo/config.toml"));
        }
    }
    if findings.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "flags that may change float results: {}",
            findings.join(", ")
        ))
    }
}

fn check_document_limits(root: &Path) -> Result {
    let mut problems = Vec::new();
    let limits = [
        ("CLAUDE.md", checks::CLAUDE_MD_MAX_LINES),
        ("docs/STAND.md", checks::STAND_MD_MAX_LINES),
    ];
    for (name, max) in limits {
        if let Ok(text) = std::fs::read_to_string(root.join(name)) {
            problems.extend(checks::line_limit_violation(name, &text, max));
        }
    }
    let entries =
        std::fs::read_dir(root).map_err(|error| format!("cannot list the root: {error}"))?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("cannot read the root: {error}"))?;
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    for name in checks::unexpected_root_markdown(&names) {
        problems.push(format!("{name} belongs in docs/ (ADR-0000)"));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("; "))
    }
}

fn cargo(root: &Path, args: &[&str]) -> Result {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = Command::new(cargo)
        .args(args)
        .current_dir(root)
        .status()
        .map_err(|error| format!("cannot run cargo: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`cargo {}` failed", args.join(" ")))
    }
}

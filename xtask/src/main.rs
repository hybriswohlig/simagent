//! Project automation (ADR-0000). `cargo xtask ci` is the gate: the CI, the pre-push
//! hook and every agent run exactly this command.

mod arch;
mod canary;
mod checks;
mod scan;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// Target without std: building the core for it proves the core does not use std.
const NO_STD_TARGET: &str = "wasm32v1-none";
const NO_STD_CRATES: [&str; 2] = ["sim-math", "sim-core"];
/// Targets with a determinism promise (ADR-0003) plus the no-std barrier target.
const SUPPORTED_TARGETS: [&str; 4] = [
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "aarch64-apple-darwin",
    NO_STD_TARGET,
];
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
    step("architecture", || check_architecture(&root))?;
    step("scans", || check_scans(&root))?;
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
    step("canaries", || {
        let problems = canary::run(
            &root,
            NO_STD_TARGET,
            |m| member_dir(&root, m),
            &cargo_program(),
        )?;
        if problems.is_empty() {
            Ok(())
        } else {
            Err(format!("\n  {}", problems.join("\n  ")))
        }
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

/// Checks the dependency graph once per supported target. Without `--filter-platform`
/// cargo metadata lists the superset over all platforms, including dependencies behind
/// never-true cfgs such as serde_core's `cfg(any())` coupling to serde_derive.
fn check_architecture(root: &Path) -> Result {
    let mut violations = std::collections::BTreeSet::new();
    for target in SUPPORTED_TARGETS {
        let output = Command::new(cargo_program())
            .args(["metadata", "--format-version", "1", "--locked"])
            .args(["--filter-platform", target])
            .current_dir(root)
            .output()
            .map_err(|error| format!("cannot run cargo metadata: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "cargo metadata failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|error| format!("cannot parse cargo metadata: {error}"))?;
        let built = built_packages(root, target)?;
        for violation in arch::check(&metadata, Some(&built)) {
            violations.insert(format!("[{target}] {violation}"));
        }
    }
    let violations: Vec<String> = violations.into_iter().collect();
    if violations.is_empty() {
        Ok(())
    } else {
        Err(format!("\n  {}", violations.join("\n  ")))
    }
}

/// Names of the packages cargo actually builds for `target` (normal, build and dev
/// edges of every workspace member), from the real feature resolver of `cargo tree`.
fn built_packages(root: &Path, target: &str) -> Result<std::collections::BTreeSet<String>> {
    let output = Command::new(cargo_program())
        .args(["tree", "--workspace", "--locked", "--prefix", "none"])
        .args([
            "--edges",
            "normal,build,dev",
            "--format",
            "{p}",
            "--target",
            target,
        ])
        .current_dir(root)
        .output()
        .map_err(|error| format!("cannot run cargo tree: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo tree failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_string)
        .collect())
}

fn check_scans(root: &Path) -> Result {
    let mut problems = Vec::new();
    for krate in NO_STD_CRATES {
        for file in rust_files(&member_dir(root, krate).join("src"))? {
            for line in scan::extern_std_outside_test(&read(&file)?) {
                problems.push(format!(
                    "{}:{line}: extern crate std outside #[cfg(test)]",
                    relative(root, &file)
                ));
            }
        }
    }
    for file in rust_files(&member_dir(root, "sim-core").join("src"))? {
        let is_hash_module = file.file_name().is_some_and(|name| name == "hash.rs");
        if !is_hash_module && scan::mentions_identifier(&read(&file)?, "xxhash_rust") {
            problems.push(format!(
                "{}: xxhash_rust outside sim-core/src/hash.rs (ADR-0003 S5)",
                relative(root, &file)
            ));
        }
    }
    let mut reference: Option<(&str, String)> = None;
    for member in arch::MEMBERS {
        let dir = member_dir(root, member);
        for file in rust_files(&dir)? {
            for attribute in scan::guarded_allows(&read(&file)?) {
                problems.push(format!(
                    "{}: {attribute} (the lint stays forbid)",
                    relative(root, &file)
                ));
            }
        }
        if !scan::inherits_workspace_lints(&read(&dir.join("Cargo.toml"))?) {
            problems.push(format!(
                "{member}: Cargo.toml does not inherit the workspace lints"
            ));
        }
        match (
            scan::common_block(&read(&dir.join("clippy.toml"))?),
            &reference,
        ) {
            (None, _) => problems.push(format!("{member}: clippy.toml has no common block")),
            (Some(block), None) => reference = Some((member, block)),
            (Some(block), Some((first, common))) if block != *common => problems.push(format!(
                "{member}: common block of clippy.toml differs from {first}"
            )),
            (Some(_), Some(_)) => {}
        }
    }
    for lint in scan::lints_not_forbidden(&read(&root.join("Cargo.toml"))?) {
        problems.push(format!("{lint} is not forbid in [workspace.lints]"));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!("\n  {}", problems.join("\n  ")))
    }
}

fn member_dir(root: &Path, member: &str) -> PathBuf {
    if member == "xtask" {
        root.join("xtask")
    } else {
        root.join("crates").join(member)
    }
}

/// All `.rs` files below `dir`, sorted, without `target` directories.
fn rust_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        let entries = std::fs::read_dir(&current)
            .map_err(|error| format!("cannot list {}: {error}", current.display()))?;
        for entry in entries {
            let path = entry
                .map_err(|error| format!("cannot read {}: {error}", current.display()))?
                .path();
            if path.is_dir() && path.file_name().is_some_and(|name| name != "target") {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn read(path: &Path) -> Result<String> {
    std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn cargo_program() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string())
}

fn cargo(root: &Path, args: &[&str]) -> Result {
    let status = Command::new(cargo_program())
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

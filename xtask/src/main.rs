//! Project automation (ADR-0000). `cargo xtask ci` is the gate: the CI, the pre-push
//! hook and every agent run exactly this command.

mod arch;
mod budget;
mod canary;
mod checks;
mod contract;
mod kat;
mod release_run;
mod results;
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
        Some("kat-record") => return report("kat-record", kat_record()),
        Some("melanin-chart") => return report("melanin-chart", melanin_chart()),
        Some("kat-verify-reference") => {
            return report("kat-verify-reference", kat_verify_reference());
        }
        _ => {
            eprintln!(
                "usage: cargo xtask <command>\n  ci                    the gate; requires a clean working tree (CI, pre-push)\n  check                 the same gate on a dirty working tree (local iteration)\n  kat-record            write the libm bits of the class P known-answer tests (bless)\n  kat-verify-reference  recompute the reference columns with tools/kat_reference.py\n  melanin-chart         write the colours of contracts/body.json to target/reports/"
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

fn report(command: &str, result: Result) -> ExitCode {
    match result {
        Ok(()) => {
            println!("xtask: {command} done");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("xtask: {command} failed: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Directory of the known-answer files (ADR-0003 section 3), relative to the root.
const KAT_DIR: &str = "tests/golden/kat";

/// The known-answer files as (function name, path, text), sorted by name.
fn kat_files(root: &Path) -> Result<Vec<(String, PathBuf, String)>> {
    let dir = root.join(KAT_DIR);
    let entries = std::fs::read_dir(&dir)
        .map_err(|error| format!("cannot read {}: {error}", dir.display()))?;
    let mut files = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|error| format!("cannot read {}: {error}", dir.display()))?
            .path();
        if path.extension().is_some_and(|extension| extension == "csv") {
            let name = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or_else(|| format!("{}: not a UTF-8 name", path.display()))?
                .to_string();
            let text = read(&path)?;
            files.push((name, path, text));
        }
    }
    files.sort();
    Ok(files)
}

/// ADR-0003 section 3.3 in the gate: every class P row is recorded and within its bound.
fn check_kat_files(root: &Path) -> Result<Vec<String>> {
    let files = kat_files(root)?;
    let mut problems = Vec::new();
    for (function, _) in kat::CLASS_P {
        if !files.iter().any(|(name, _, _)| name == function) {
            problems.push(format!("{KAT_DIR}/{function}.csv is missing"));
        }
    }
    for (name, _, text) in &files {
        for finding in kat::violations(name, text) {
            problems.push(format!("{KAT_DIR}/{finding}"));
        }
    }
    Ok(problems)
}

/// `cargo xtask kat-record`: evaluates the class P rows with the pinned libm (through
/// the example `kat_eval` of sim-math) and writes the bits. Nothing is written if any
/// row of any file breaks its bound: that is a finding, not a tolerance question.
fn kat_record() -> Result {
    let root = workspace_root()?;
    let mut recorded = Vec::new();
    let mut problems = Vec::new();
    for (name, path, text) in kat_files(&root)? {
        let inputs = kat::class_p_inputs(&text);
        if inputs.is_empty() {
            continue;
        }
        let mut args = vec![
            "run",
            "--quiet",
            "--locked",
            "--package",
            "sim-math",
            "--example",
            "kat_eval",
            "--",
            name.as_str(),
        ];
        args.extend(inputs.iter().map(String::as_str));
        let output = Command::new(cargo_program())
            .args(&args)
            .current_dir(&root)
            .output()
            .map_err(|error| format!("cannot run cargo: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "kat_eval {name} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let actual: Vec<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_string)
            .collect();
        match kat::record(&name, &text, &actual) {
            Ok(new_text) => recorded.push((path, new_text)),
            Err(findings) => problems.extend(findings),
        }
    }
    if !problems.is_empty() {
        return Err(format!("nothing written\n  {}", problems.join("\n  ")));
    }
    for (path, text) in recorded {
        std::fs::write(&path, text)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
        println!("recorded {}", relative(&root, &path));
    }
    Ok(())
}

/// `cargo xtask kat-verify-reference`: recomputes the reference columns with the
/// Python tools (interpreter from `KAT_PYTHON`, default `python3`; needs
/// `tools/requirements.txt`) and compares them with the files. Not part of the gate:
/// the CI needs no Python.
fn kat_verify_reference() -> Result {
    let root = workspace_root()?;
    let python = std::env::var("KAT_PYTHON").unwrap_or_else(|_| "python3".to_string());
    for tool in ["tools/kat_reference.py", "tools/rng_reference.py"] {
        let status = Command::new(&python)
            .args(["-B", tool, "--check"])
            .current_dir(&root)
            .status()
            .map_err(|error| format!("cannot run {python}: {error}"))?;
        if !status.success() {
            return Err(format!(
                "{tool} --check failed ({status}), see its output above"
            ));
        }
    }
    Ok(())
}

fn gate(require_clean_tree: bool) -> Result {
    let root = workspace_root()?;
    step("location", || check_location(&root))?;
    if require_clean_tree {
        step("clean working tree", || check_clean_tree(&root))?;
    }
    step("compiler flags", || check_compiler_flags(&root))?;
    step("document limits", || check_document_limits(&root))?;
    step("result changes", || check_result_changes(&root))?;
    step("architecture", || check_architecture(&root))?;
    step("contract check", || check_contract(&root))?;
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
    })?;
    step("gate run (release binary): goldens and log budget", || {
        check_gate_run(&root)
    })
}

/// The scenario of the gate run (ADR-0004 section 8, ADR-0001 section 7): the dummy of
/// step 2, from step 5 on the walking skeleton. Its golden has the same file name.
const GATE_SCENARIO: &str = "tests/scenarios/dummy.json";
const GATE_GOLDEN: &str = "tests/golden/dummy.json";

/// Runs the gate scenario once with the release binary of `sim-cli`, which is built
/// without the features of the dev-dependencies, and checks the run twice: its manifest
/// against the golden (ADR-0001 section 7) and its log entries per person-year from
/// `events.ndjson` and `census.csv` (ADR-0004 section 8).
fn check_gate_run(root: &Path) -> Result {
    let out = root.join("target").join("xtask-gate-run");
    if out.exists() {
        std::fs::remove_dir_all(&out)
            .map_err(|error| format!("cannot remove {}: {error}", out.display()))?;
    }
    let out_arg = out.display().to_string();
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
            "--out",
            &out_arg,
        ],
    )?;
    let mut found = release_run::differences(
        &read(&out.join("manifest.json"))?,
        &read(&root.join(GATE_GOLDEN))?,
    )?;
    let entry_times = budget::entry_times(&read(&out.join("events.ndjson"))?)?;
    let census = budget::census_rows(&read(&out.join("census.csv"))?)?;
    found.extend(budget::violations(&entry_times, &census));
    if found.is_empty() {
        Ok(())
    } else {
        Err(format!("\n  {}", found.join("\n  ")))
    }
}

/// `cargo xtask melanin-chart`: the colour tables of the contract as
/// `target/reports/melanin-chart.html` (ADR-0005 section 5).
fn melanin_chart() -> Result {
    let root = workspace_root()?;
    let body = read(&root.join("contracts/body.json"))?;
    let chart = contract::melanin_chart(&body)
        .map_err(|findings| format!("\n  {}", findings.join("\n  ")))?;
    let dir = root.join("target").join("reports");
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
    let path = dir.join("melanin-chart.html");
    std::fs::write(&path, chart)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    println!("wrote {}", relative(&root, &path));
    Ok(())
}

/// ADR-0005 section 7: `contracts/body.json` is the single source of the visible body
/// values. Checked here: its schema with the rules of the units and of the melanin table,
/// and its changelog.
fn check_contract(root: &Path) -> Result {
    let body = read(&root.join("contracts/body.json"))?;
    let mut found = contract::schema_findings(&body);
    found.extend(contract::changelog_findings(
        &body,
        &read(&root.join("contracts/body.changelog.md"))?,
    ));
    if found.is_empty() {
        Ok(())
    } else {
        Err(format!("\n  {}", found.join("\n  ")))
    }
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

/// ADR-0003 6.4 and 6.5: compares the working tree with its merge base with
/// `origin/main`. On main itself the base is the head, and the rules hold trivially.
fn check_result_changes(root: &Path) -> Result {
    let base = git_text(root, &["merge-base", "HEAD", "origin/main"]).map_err(|error| {
        format!(
            "cannot find the merge base with origin/main ({error}); the rules on result changes need the history (ADR-0003 6.4)"
        )
    })?;
    let base = base.trim();
    let at_base = |path: &str| git_bytes(root, &["show", &format!("{base}:{path}")]).ok();
    let text = |bytes: Option<Vec<u8>>| {
        bytes
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .unwrap_or_default()
    };
    let mut before = results::Results {
        core_version: results::parse_core_version(&text(at_base(CORE_VERSION_FILE))),
        toolchain: text(at_base("rust-toolchain.toml")),
        locked: results::locked_versions(&text(at_base("Cargo.lock"))),
        ..results::Results::default()
    };
    let listed = git_text(root, &["ls-tree", "-r", "--name-only", base, GOLDEN_DIR])?;
    for path in listed.lines() {
        if let Some(bytes) = at_base(path) {
            before.goldens.insert(path.to_string(), bytes);
        }
    }
    let read_or_empty = |path: &str| std::fs::read_to_string(root.join(path)).unwrap_or_default();
    let mut after = results::Results {
        core_version: results::parse_core_version(&read_or_empty(CORE_VERSION_FILE)),
        toolchain: read_or_empty("rust-toolchain.toml"),
        locked: results::locked_versions(&read_or_empty("Cargo.lock")),
        ..results::Results::default()
    };
    for file in all_files(&root.join(GOLDEN_DIR))? {
        let bytes = std::fs::read(&file)
            .map_err(|error| format!("cannot read {}: {error}", file.display()))?;
        after.goldens.insert(relative(root, &file), bytes);
    }
    let found = results::violations(&before, &after);
    if found.is_empty() {
        Ok(())
    } else {
        Err(format!("\n  {}", found.join("\n  ")))
    }
}

const CORE_VERSION_FILE: &str = "crates/sim-core/src/version.rs";
const GOLDEN_DIR: &str = "tests/golden";

fn git_bytes(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|error| format!("cannot run git: {error}"))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn git_text(root: &Path, args: &[&str]) -> Result<String> {
    git_bytes(root, args).map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

/// Every file below `dir`, in a fixed order.
fn all_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        let entries = std::fs::read_dir(&current)
            .map_err(|error| format!("cannot list {}: {error}", current.display()))?;
        for entry in entries {
            let path = entry
                .map_err(|error| format!("cannot list {}: {error}", current.display()))?
                .path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
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
fn built_packages(
    root: &Path,
    target: &str,
) -> Result<std::collections::BTreeSet<(String, String)>> {
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
    let mut built = std::collections::BTreeSet::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if line.trim().is_empty() {
            continue;
        }
        built.insert(
            arch::parse_tree_line(line)
                .ok_or_else(|| format!("cannot parse the cargo tree line {line:?}"))?,
        );
    }
    Ok(built)
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
    let mut core_sources = Vec::new();
    for file in rust_files(&member_dir(root, "sim-core").join("src"))? {
        core_sources.push((relative(root, &file), read(&file)?));
    }
    problems.extend(scan::core_identifier_findings(&core_sources));
    problems.extend(scan::core_time_findings(&core_sources));
    problems.extend(scan::tag_findings(
        &core_sources,
        &read(&root.join("tests/golden/rng_tags.txt"))?,
    ));
    let mut workspace_sources = Vec::new();
    for member in arch::MEMBERS {
        for file in rust_files(&member_dir(root, member))? {
            workspace_sources.push((relative(root, &file), read(&file)?));
        }
    }
    problems.extend(scan::generator_findings(&workspace_sources));
    problems.extend(check_kat_files(root)?);
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

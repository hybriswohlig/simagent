//! Project automation (ADR-0000). `cargo xtask ci` is the gate: the CI, the pre-push
//! hook and every agent run exactly this command.

mod arch;
mod budget;
mod canary;
mod checks;
mod codegen;
mod contract;
mod contract_base;
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
        Some("codegen") => {
            return report(
                "codegen",
                workspace_root().and_then(|root| codegen(&root, &contract_base(&root)?)),
            );
        }
        Some("contract-check") => {
            return report(
                "contract-check",
                workspace_root().and_then(|root| check_contract(&root, &contract_base(&root)?)),
            );
        }
        Some("kat-verify-reference") => {
            return report("kat-verify-reference", kat_verify_reference());
        }
        _ => {
            eprintln!(
                "usage: cargo xtask <command>\n  ci                    the gate; requires a clean working tree (CI, pre-push)\n  check                 the same gate on a dirty working tree (local iteration)\n  kat-record            write the libm bits of the class P known-answer tests (bless)\n  kat-verify-reference  recompute the reference columns with tools/kat_reference.py\n  melanin-chart         write the colours of contracts/body.json to target/reports/\n  codegen               write the types of contracts/body.json into sim-core, and its hash\n  contract-check        the checks of contracts/body.json alone (part of the gate)"
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
    step("contract check", || {
        check_contract(&root, &contract_base(&root)?)
    })?;
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
    let body = read(&root.join(BODY_FILE))?;
    let chart = contract::melanin_chart(&body).map_err(listed)?;
    let dir = root.join("target").join("reports");
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
    let path = dir.join("melanin-chart.html");
    std::fs::write(&path, chart)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    println!("wrote {}", relative(&root, &path));
    Ok(())
}

const BODY_FILE: &str = "contracts/body.json";
const BODY_CHANGELOG_FILE: &str = "contracts/body.changelog.md";
const BODY_HASHES_FILE: &str = "contracts/body.hashes.json";

fn listed(findings: Vec<String>) -> String {
    format!("\n  {}", findings.join("\n  "))
}

/// The names of the functions in the hand-written files of the contract module: no
/// parameter may have such a key, its getter would define the name twice.
fn taken_names(root: &Path) -> Result<Vec<String>> {
    let generated = root.join(codegen::GENERATED_FILE);
    let dir = generated.parent().unwrap_or(root);
    let mut names = Vec::new();
    for file in rust_files(dir)? {
        if file != generated {
            names.extend(scan::function_names(&read(&file)?));
        }
    }
    Ok(names)
}

/// The types of the contract `body` as `cargo xtask codegen` writes them: generated and
/// then formatted in `target/codegen`, never in place.
fn generated_types(root: &Path, body: &str) -> Result<String> {
    let hash = sim_core::hash::stable_hash64(body.as_bytes());
    let source = codegen::generate(body, hash, &taken_names(root)?).map_err(listed)?;
    let manifest = read(&root.join("Cargo.toml"))?;
    let edition = codegen::workspace_edition(&manifest)
        .ok_or("Cargo.toml: expected exactly one line edition = \"...\"")?;
    codegen::format(&root.join("target").join("codegen"), edition, &source)
}

/// The contract files at the merge base with `origin/main` (ADR-0005 section 7, check 3);
/// a file that the base does not have is `None`.
fn contract_base(root: &Path) -> Result<contract_base::Base> {
    let base = git_text(root, &["merge-base", "HEAD", "origin/main"]).map_err(|error| {
        format!(
            "cannot find the merge base with origin/main ({error}); the contract check compares with it (ADR-0005 section 7)"
        )
    })?;
    let base = base.trim();
    let at_base = |path: &str| -> Result<Option<String>> {
        let listed = git_text(root, &["ls-tree", "--name-only", base, "--", path])?;
        if listed.trim().is_empty() {
            return Ok(None);
        }
        git_text(root, &["show", &format!("{base}:{path}")]).map(Some)
    };
    Ok(contract_base::Base {
        body: at_base(BODY_FILE)?,
        hashes: at_base(BODY_HASHES_FILE)?,
    })
}

/// `cargo xtask codegen` (ADR-0005 section 2): writes the generated types of the body
/// contract into `sim-core` and the hash of the contract file, under its version, into
/// `contracts/body.hashes.json`. A contract with findings changes nothing, and neither
/// does a contract that would get another hash under a version of the merge base.
fn codegen(root: &Path, base: &contract_base::Base) -> Result {
    let body = read(&root.join(BODY_FILE))?;
    let types = generated_types(root, &body)?;
    let version = contract::version(&body).ok_or("body.json has no contract_version")?;
    let hashes_path = root.join(BODY_HASHES_FILE);
    let hashes = if hashes_path.exists() {
        read(&hashes_path)?
    } else {
        "{}".to_string()
    };
    let hash = sim_core::hash::stable_hash64(body.as_bytes());
    let hashes = codegen::with_hash(&hashes, &version, hash)?;
    let rewritten = contract_base::hash_findings(base.hashes.as_deref(), &hashes);
    if !rewritten.is_empty() {
        return Err(format!(
            "{}\n  nothing written: raise contract_version in {BODY_FILE} first",
            listed(rewritten)
        ));
    }
    let types_path = root.join(codegen::GENERATED_FILE);
    for (path, text) in [(&hashes_path, &hashes), (&types_path, &types)] {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
        }
        std::fs::write(path, text)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
        println!("wrote {}", relative(root, path));
    }
    Ok(())
}

/// ADR-0005 section 7: `contracts/body.json` is the single source of the visible body
/// values. Checked here: its schema with the rules of the units and of the melanin table
/// (check 1), its hash and its changelog (check 2), the step from the contract of the
/// merge base (check 3), and that the generated types in `sim-core` are the ones of this
/// file (check 4).
fn check_contract(root: &Path, base: &contract_base::Base) -> Result {
    let body = read(&root.join(BODY_FILE))?;
    let mut found = contract::schema_findings(&body);
    let valid = found.is_empty();
    found.extend(contract::changelog_findings(
        &body,
        &read(&root.join(BODY_CHANGELOG_FILE))?,
    ));
    let hashes = read(&root.join(BODY_HASHES_FILE))?;
    if let Some(version) = contract::version(&body) {
        found.extend(codegen::hash_findings(
            &hashes,
            &version,
            sim_core::hash::stable_hash64(body.as_bytes()),
        ));
    }
    if valid {
        found.extend(contract_base::base_findings(base, &body, &hashes));
    }
    if found.is_empty() {
        let fresh = generated_types(root, &body)?;
        let checked_in = read(&root.join(codegen::GENERATED_FILE))?;
        found.extend(codegen::stale_finding(&fresh, &checked_in));
    }
    if found.is_empty() {
        Ok(())
    } else {
        Err(listed(found))
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
    let output = git_command(root)
        .args(["status", "--porcelain", "--untracked-files=all"])
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

/// The variables by which git finds a repository apart from the directory it runs in
/// (`git rev-parse --local-env-vars`). Inside a hook git exports some of them for the
/// repository of the hook.
const LOCAL_GIT_ENV: [&str; 15] = [
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CONFIG",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "GIT_OBJECT_DIRECTORY",
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_GRAFT_FILE",
    "GIT_INDEX_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_REPLACE_REF_BASE",
    "GIT_PREFIX",
    "GIT_SHALLOW_FILE",
    "GIT_COMMON_DIR",
];

/// git for the repository that `dir` lies in, whatever the environment names.
fn git_command(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command.current_dir(dir);
    let _ = LOCAL_GIT_ENV;
    command
}

fn git_bytes(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = git_command(root)
        .args(args)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The hand-written part of the contract module.
    const CONTRACT_MODULE_FILE: &str = "crates/sim-core/src/body_contract/mod.rs";

    /// A copy of the files that the step contract check reads, below `target/`, with the
    /// text of `file` changed by `change`.
    fn contract_copy(name: &str, file: &str, change: impl Fn(&str) -> String) -> PathBuf {
        let root = workspace_root().expect("the workspace root");
        let copy = root.join("target").join("contract-step-tests").join(name);
        for path in [
            BODY_FILE,
            BODY_CHANGELOG_FILE,
            BODY_HASHES_FILE,
            codegen::GENERATED_FILE,
            CONTRACT_MODULE_FILE,
            "Cargo.toml",
        ] {
            let mut text = read(&root.join(path)).expect(path);
            if path == file {
                let changed = change(&text);
                assert_ne!(changed, text, "{name}: the change changes nothing");
                text = changed;
            }
            let target = copy.join(path);
            std::fs::create_dir_all(target.parent().expect("a file in a directory"))
                .expect("directory");
            std::fs::write(&target, text).expect(path);
        }
        copy
    }

    /// A merge base with the contract files as the working tree has them.
    fn unchanged_base() -> contract_base::Base {
        let root = workspace_root().expect("the workspace root");
        contract_base::Base {
            body: Some(read(&root.join(BODY_FILE)).expect("contract")),
            hashes: Some(read(&root.join(BODY_HASHES_FILE)).expect("hashes")),
        }
    }

    /// The pre-push hook runs these tests with `GIT_DIR` of the pushing repository in the
    /// environment. A git command that inherits it works on that repository instead of
    /// the one in its directory (it happened: `git init` of a test set `core.bare` there).
    #[test]
    fn git_finds_the_repository_by_the_directory_alone() {
        let root = workspace_root().expect("the workspace root");
        let command = git_command(&root);
        assert_eq!(command.get_current_dir(), Some(root.as_path()));
        let removed: Vec<String> = command
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .map(|(name, _)| name.to_string_lossy().into_owned())
            .collect();
        assert_eq!(removed, {
            let mut sorted = LOCAL_GIT_ENV.map(str::to_string).to_vec();
            sorted.sort();
            sorted
        });
        assert_eq!(command.get_envs().count(), LOCAL_GIT_ENV.len());
        // The list is the one of the git that runs here: a variable it does not know
        // would stay in the environment.
        let listed = git_text(&root, &["rev-parse", "--local-env-vars"]).expect("git");
        for name in listed.lines() {
            assert!(
                LOCAL_GIT_ENV.contains(&name),
                "{name} is not in LOCAL_GIT_ENV"
            );
        }
        assert!(listed.lines().any(|name| name == "GIT_DIR"));
    }

    /// Runs git in `dir` without the hooks and the identity of the machine.
    fn git(dir: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args([
                "-c",
                "user.name=test",
                "-c",
                "user.email=test@example.invalid",
            ])
            .args([
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
            ])
            .args(args)
            .current_dir(dir)
            .output()
            .expect("git runs");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// A git repository below `target/` whose only commit holds `files` of the copy of
    /// the contract files, with `origin/main` at that commit.
    fn contract_repo(name: &str, files: &[&str]) -> PathBuf {
        let repo = contract_copy(name, "", str::to_string);
        let _ = std::fs::remove_dir_all(repo.join(".git"));
        git(&repo, &["init", "-q"]);
        for file in files {
            git(&repo, &["add", "--", file]);
        }
        git(&repo, &["commit", "-q", "-m", "base"]);
        git(&repo, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
        repo
    }

    /// Replaces the text of `file` in `repo` by `change` of it.
    fn rewrite(repo: &Path, file: &str, change: impl Fn(&str) -> String) {
        let path = repo.join(file);
        let text = read(&path).expect(file);
        std::fs::write(&path, change(&text)).expect(file);
    }

    /// The base of the contract check is the commit of `origin/main`, not the working
    /// tree; a file that this commit does not have is none.
    #[test]
    fn the_contract_base_is_the_merge_base_with_origin_main() {
        let repo = contract_repo("base", &[BODY_FILE, BODY_HASHES_FILE]);
        let committed = unchanged_base();
        rewrite(&repo, BODY_FILE, |text| format!("{text}\n"));
        rewrite(&repo, BODY_HASHES_FILE, |_| "{}\n".to_string());
        let base = contract_base(&repo).expect("a base");
        assert_eq!(base.body, committed.body);
        assert_eq!(base.hashes, committed.hashes);

        let repo = contract_repo("base-without-contract", &["Cargo.toml"]);
        let base = contract_base(&repo).expect("a base");
        assert_eq!((base.body, base.hashes), (None, None));

        let repo = contract_repo("base-without-hashes", &[BODY_FILE]);
        let base = contract_base(&repo).expect("a base");
        assert_eq!((base.body, base.hashes), (committed.body, None));

        // Without origin/main there is no base, and the step says so.
        git(&repo, &["update-ref", "-d", "refs/remotes/origin/main"]);
        let refused = contract_base(&repo).err().expect("no base");
        assert!(
            refused.starts_with("cannot find the merge base with origin/main ("),
            "{refused}"
        );
    }

    /// ADR-0005 section 7, check 3, the whole step: a changed contract needs the next
    /// version, and `cargo xtask codegen` never rewrites a hash entry of the base.
    #[test]
    fn a_changed_contract_goes_through_codegen_and_the_step() {
        let files = [
            BODY_FILE,
            BODY_CHANGELOG_FILE,
            BODY_HASHES_FILE,
            codegen::GENERATED_FILE,
            "Cargo.toml",
        ];
        let repo = contract_repo("change", &files);
        let base = contract_base(&repo).expect("a base");
        let snapshot = |repo: &Path| files.map(|file| read(&repo.join(file)).expect(file));
        let committed = snapshot(&repo);

        // Unchanged: the step passes and codegen writes the same bytes.
        assert_eq!(check_contract(&repo, &base), Ok(()));
        assert_eq!(codegen(&repo, &base), Ok(()));
        assert_eq!(snapshot(&repo), committed);

        // A changed description under the old version: codegen writes nothing.
        let describe =
            |text: &str| text.replacen("\"description\": \"", "\"description\": \"The ", 1);
        rewrite(&repo, BODY_FILE, describe);
        let changed = read(&repo.join(BODY_FILE)).expect("contract");
        assert_ne!(changed, committed[0]);
        let refused = codegen(&repo, &base).expect_err("a hash of the base");
        assert!(
            refused.contains("of the merge base was changed or removed")
                && refused.ends_with(
                    "nothing written: raise contract_version in contracts/body.json first"
                ),
            "{refused}"
        );
        assert_eq!(snapshot(&repo)[1..], committed[1..]);
        // Written by hand, the hash and the types pass checks 2 and 4, not check 3.
        let hash = sim_core::hash::stable_hash64(changed.as_bytes());
        let version = contract::version(&changed).expect("a version");
        rewrite(&repo, BODY_HASHES_FILE, |_| {
            format!("{{\"{version}\": \"{hash:016x}\"}}\n")
        });
        let types = generated_types(&repo, &changed).expect("types");
        rewrite(&repo, codegen::GENERATED_FILE, |_| types.clone());
        let refused = check_contract(&repo, &base).expect_err("check 3");
        assert_eq!(
            refused,
            format!(
                "\n  contract_version {version} after {version} of the merge base: a correction needs {}\n  body.hashes.json: the entry {version} of the merge base was changed or removed: a changed contract needs a new version",
                next_versions(&version).join(" or ")
            )
        );

        // With the next version, its changelog section and codegen, the step passes.
        let next = next_versions(&version)[0].clone();
        rewrite(&repo, BODY_HASHES_FILE, |_| committed[2].clone());
        rewrite(&repo, codegen::GENERATED_FILE, |_| committed[3].clone());
        rewrite(&repo, BODY_FILE, |text| {
            text.replacen(
                &format!("\"contract_version\": \"{version}\""),
                &format!("\"contract_version\": \"{next}\""),
                1,
            )
        });
        rewrite(&repo, BODY_CHANGELOG_FILE, |text| {
            format!(
                "{text}\n## {next}\n\n- Neu: nichts\n- Geändert: eine Beschreibung\n- Stillgelegt: nichts\n"
            )
        });
        assert_eq!(codegen(&repo, &base), Ok(()));
        assert_eq!(check_contract(&repo, &base), Ok(()));
        let written = snapshot(&repo);
        assert_ne!(written[2], committed[2]);
        assert_ne!(written[3], committed[3]);
        assert!(
            written[2].contains(&format!("\"{next}\": ")),
            "{}",
            written[2]
        );
    }

    /// The versions one step after `version` (ADR-0005 section 6): PATCH + 1, then the
    /// breaking step, MINOR + 1 below 1.0.0.
    fn next_versions(version: &str) -> Vec<String> {
        let numbers: Vec<u64> = version
            .split('.')
            .map(|part| part.parse().expect("a number"))
            .collect();
        let [major, minor, patch] = numbers[..] else {
            panic!("three numbers");
        };
        if major == 0 {
            vec![
                format!("0.{minor}.{}", patch + 1),
                format!("0.{}.0", minor + 1),
            ]
        } else {
            vec![
                format!("{major}.{minor}.{}", patch + 1),
                format!("{major}.{}.0", minor + 1),
                format!("{}.0.0", major + 1),
            ]
        }
    }

    /// The functions of `body_contract/mod.rs` are taken, the generated ones are not
    /// read, and the generator gets the names: a parameter with the key `with` stops it.
    #[test]
    fn the_functions_of_the_contract_module_are_taken() {
        let root = workspace_root().expect("the workspace root");
        let names = taken_names(&root).expect("names");
        assert!(names.contains(&"with".to_string()), "{names:?}");
        for generated in ["get", "slot", "spec", "height_cm"] {
            assert!(!names.contains(&generated.to_string()), "{generated}");
        }

        let copy = contract_copy("taken", "", str::to_string);
        assert_eq!(taken_names(&copy), Ok(names));
        let body = read(&copy.join(BODY_FILE)).expect("contract");
        assert!(generated_types(&copy, &body).is_ok());
        let first_key = body
            .split("\"key\": \"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("a first key");
        let renamed = body.replacen(&format!("\"key\": \"{first_key}\""), "\"key\": \"with\"", 1);
        assert_eq!(
            generated_types(&copy, &renamed),
            Err(
                "\n  parameter with: the body contract module has a function of this name"
                    .to_string()
            )
        );
        // Without the hand-written file the name is free.
        std::fs::remove_file(copy.join(CONTRACT_MODULE_FILE)).expect("removed");
        assert_eq!(taken_names(&copy), Ok(Vec::new()));
        assert!(generated_types(&copy, &renamed).is_ok());
    }

    /// ADR-0005 `contract_check_passes_on_repo`, the whole step.
    #[test]
    fn the_contract_step_passes_on_the_files_of_the_repo() {
        let copy = contract_copy("clean", "", str::to_string);
        assert_eq!(check_contract(&copy, &unchanged_base()), Ok(()));
    }

    /// The step runs every check of ADR-0005 section 7 that exists so far: each broken
    /// file is refused for its own reason.
    #[test]
    fn the_contract_step_runs_every_check() {
        let with_newline = |text: &str| format!("{text}\n");
        type Change<'a> = &'a dyn Fn(&str) -> String;
        let cases: [(&str, &str, Change, &str); 7] = [
            (
                "schema",
                BODY_FILE,
                &|text| text.replacen("\"format_version\": 1", "\"format_version\": 2", 1),
                "body.json: format_version must be 1",
            ),
            (
                "changelog",
                BODY_CHANGELOG_FILE,
                &|_| "# Changelog\n".to_string(),
                "body.changelog.md has no section ## ",
            ),
            (
                "hash-of-a-changed-contract",
                BODY_FILE,
                &with_newline,
                ": a changed contract needs a new version",
            ),
            (
                "hash-file",
                BODY_HASHES_FILE,
                &|_| "{}\n".to_string(),
                "body.hashes.json has no entry for ",
            ),
            (
                "stale-types",
                codegen::GENERATED_FILE,
                &with_newline,
                "generated.rs is stale: run cargo xtask codegen and commit the result",
            ),
            (
                "edition",
                "Cargo.toml",
                &|text| text.replace("edition = ", "edition="),
                "Cargo.toml: expected exactly one line edition = \"...\"",
            ),
            // The edition of the manifest reaches the formatter: one it does not know.
            (
                "edition-of-the-manifest",
                "Cargo.toml",
                &|text| text.replace("edition = \"", "edition = \"1"),
                "rustfmt refuses the generated source:",
            ),
        ];
        for (name, file, change, expected) in cases {
            let copy = contract_copy(name, file, change);
            let refused = check_contract(&copy, &unchanged_base()).expect_err(name);
            assert!(refused.contains(expected), "{name}: {refused}");
        }
    }
}

//! The web build of the gate (ADR-0021 section 4). It removes `web/dist`, scans `web/src`
//! (no symbolic link, no other host, `Math` only under `web/src/render/`), checks that the
//! running Node has the major of `web/.nvmrc`, installs the locked packages without scripts
//! (`npm ci`), compiles `web/src` with the pinned `tsc` into `web/dist`, writes the page
//! with its import map there and copies the vendored Preact modules beside it, and runs
//! `web/test` with `node --test` on it. `tsc` and the tests run under Node's permission
//! model with an empty environment: `tsc` reads `web/` and writes `web/dist`; the tests read
//! only `web/package.json`, `web/dist` and `web/test`, write nothing, start no process or
//! worker and build no code from strings. A canary test proves that on every run. Node 24
//! cannot close the network: the tests read no file that is not public, but what the
//! network itself reaches (a metadata service of the CI, say) they reach. So does every
//! Rust test and build script of this gate, which runs the repository's code with the
//! network, the environment and every file; the sandbox keeps the web tests below that
//! level, it does not make the gate safe for code nobody reviewed.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::Result;

/// The import map of the page: the specifier, the module in `web/dist`, and the file below
/// `web/node_modules` it is copied from.
pub const VENDOR: [(&str, &str, &str); 3] = [
    ("preact", "vendor/preact.mjs", "preact/dist/preact.mjs"),
    (
        "preact/hooks",
        "vendor/hooks.mjs",
        "preact/hooks/dist/hooks.mjs",
    ),
    (
        "preact/jsx-runtime",
        "vendor/jsx-runtime.mjs",
        "preact/jsx-runtime/dist/jsxRuntime.mjs",
    ),
];
/// The one directory of `web/src` whose code may compute (SVG geometry, the sRGB rule
/// ADR-0005 section 5 gives to readers).
const RENDER: &str = "web/src/render/";

/// The page, `web/dist/index.html`. It is written here and not kept in `web/`, so its
/// import map is exactly [`VENDOR`] and it loads nothing but `./main.js`.
pub fn page() -> String {
    let imports: serde_json::Map<String, serde_json::Value> = (VENDOR.iter())
        .map(|(name, path, _)| ((*name).to_owned(), serde_json::json!(format!("./{path}"))))
        .collect();
    let map = serde_json::json!({ "imports": imports });
    format!(
        "<!doctype html>\n<html lang=\"de\">\n  <head>\n    <meta charset=\"utf-8\">\n    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n    <title>SimAgent</title>\n    <script type=\"importmap\">{map}</script>\n    <script type=\"module\" src=\"./main.js\"></script>\n  </head>\n  <body>\n    <main id=\"app\"></main>\n  </body>\n</html>\n"
    )
}

/// Whether `node --version` printed the major of `.nvmrc`.
pub fn node_major(printed: &str, nvmrc: &str) -> Result {
    let major = printed
        .trim()
        .strip_prefix('v')
        .and_then(|v| v.split('.').next());
    if major == Some(nvmrc.trim()) {
        Ok(())
    } else {
        Err(format!(
            "node_version: node {}, web/.nvmrc wants major {}",
            printed.trim(),
            nvmrc.trim()
        ))
    }
}

/// The ways a page reaches the network; only [`API`] may use them (its `getJson` and
/// `postJson`).
const NETWORK: [&str; 6] = [
    "fetch",
    "XMLHttpRequest",
    "WebSocket",
    "EventSource",
    "sendBeacon",
    "importScripts",
];
const API: &str = "web/src/api.ts";

/// Whether `text` holds `word` as a word (not inside a longer identifier such as `myMath`).
fn names(text: &str, word: &str) -> bool {
    let identifier =
        |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$');
    text.match_indices(word).any(|(i, word)| {
        let before = text[..i].chars().next_back();
        let after = text[i + word.len()..].chars().next();
        !identifier(before) && !identifier(after)
    })
}

/// A file of `web/src` names no other host (`://`, or a string that starts with `//`),
/// imports nothing at run time, reaches the network only in [`API`] and uses `Math` only
/// under [`RENDER`]. A tripwire for review, not a proof (ADR-0021 section 4).
pub fn source_findings(path: &str, text: &str) -> Vec<String> {
    let mut findings = Vec::new();
    let quoted_host = ["\"//", "'//", "`//"].iter().any(|q| text.contains(q));
    if text.contains("://") || quoted_host {
        findings.push(format!(
            "{path}: names another host (web/ talks only to its own server)"
        ));
    }
    if text.contains("import(") {
        findings.push(format!(
            "{path}: imports at run time (web/ loads only its own modules)"
        ));
    }
    if path != API && NETWORK.iter().any(|word| names(text, word)) {
        findings.push(format!("{path}: reaches the network outside {API}"));
    }
    if !path.starts_with(RENDER) && names(text, "Math") {
        findings.push(format!(
            "{path}: uses Math outside {RENDER} (web/ computes nothing, ADR-0021)"
        ));
    }
    findings
}

/// Every file below `dir`, sorted; a symbolic link is an error.
fn files(root: &Path, dir: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        let path = entry.path();
        if kind.is_symlink() {
            return Err(format!(
                "{}: a symbolic link in web/",
                crate::relative(root, &path)
            ));
        } else if kind.is_dir() {
            found.extend(files(root, &path)?);
        } else {
            found.push(path);
        }
    }
    found.sort();
    Ok(found)
}

/// `web`, `web/src` and `web/test` are directories of the repository, no symbolic links.
fn unlinked(root: &Path) -> Result {
    for dir in ["web", "web/src", "web/test"] {
        let kind = (root.join(dir).symlink_metadata()).map_err(|e| format!("{dir}: {e}"))?;
        if kind.is_symlink() {
            return Err(format!("{dir}: a symbolic link in web/"));
        }
    }
    Ok(())
}

/// The scans over `root/web/src`; `web`, `web/src` and `web/test` are no symbolic link and
/// hold none.
pub fn scan(root: &Path) -> Result<Vec<String>> {
    let web = root.join("web");
    unlinked(root)?;
    let mut findings = Vec::new();
    for file in files(root, &web.join("src"))? {
        let text = crate::read(&file)?;
        findings.extend(source_findings(&crate::relative(root, &file), &text));
    }
    files(root, &web.join("test"))?;
    Ok(findings)
}

/// The codes of the repository without a text in `web/` (see [`code_text_findings`]).
pub fn code_texts(root: &Path) -> Result<Vec<String>> {
    let codes = crate::read(&root.join(LOG_CODES))?;
    Ok(code_text_findings(&codes, &crate::read(&root.join(TEXTS))?))
}

/// The stable codes of the log, the inputs and saving.
const LOG_CODES: &str = "tests/golden/log_codes.txt";
/// The player texts of `web/`.
const TEXTS: &str = "web/src/text.ts";

/// Every code of [`LOG_CODES`] has a text in [`TEXTS`], a key `"<type> <name>":` at the start
/// of a line of the table `CODES` (ADR-0021 section 4); a retired code too.
pub fn code_text_findings(codes: &str, texts: &str) -> Vec<String> {
    let table = (texts.split_once("export const CODES"))
        .and_then(|(_, rest)| rest.split_once("\n};"))
        .map_or("", |(table, _)| table);
    let keys: BTreeSet<&str> = (table.lines())
        .filter_map(|line| line.trim_start().strip_prefix('"')?.split_once("\":"))
        .map(|(key, _)| key)
        .collect();
    (codes.lines())
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| {
            let words: Vec<&str> = line.split_whitespace().collect();
            let words = words.strip_prefix(&["retired"]).unwrap_or(&words);
            match words {
                [kind, _, name, ..] => format!("{kind} {name}"),
                _ => line.to_owned(),
            }
        })
        .filter(|code| !keys.contains(code.as_str()))
        .map(|code| format!("{TEXTS}: no text for the code `{code}` of {LOG_CODES}"))
        .collect()
}

/// The flags of Node's permission model for a run that reads `reads` and nothing else.
pub fn sandbox(reads: &[&Path]) -> Vec<String> {
    let mut args = vec!["--permission".to_owned()];
    args.extend(
        reads
            .iter()
            .map(|path| format!("--allow-fs-read={}", path.display())),
    );
    args.push("--disallow-code-generation-from-strings".to_owned());
    args
}

/// Runs Node in `dir` with `args` and an environment of `PATH` alone.
fn node(dir: &Path, args: &[String]) -> Result {
    let status = Command::new("node")
        .args(args)
        .current_dir(dir)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .status()
        .map_err(|e| format!("cannot run node: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`node {}` failed", args.join(" ")))
    }
}

/// The canary of the sandbox of the tests: each thing the tests may not do is denied.
pub fn canary(web: &Path) -> String {
    let dist = web.join("dist");
    let outside = web.join("node_modules/typescript/package.json");
    format!(
        "import {{ test }} from \"node:test\";
import assert from \"node:assert/strict\";
import {{ readFileSync, writeFileSync }} from \"node:fs\";
import {{ execSync }} from \"node:child_process\";
import {{ Worker }} from \"node:worker_threads\";
const denied = {{ code: \"ERR_ACCESS_DENIED\" }};
test(\"no write\", () => assert.throws(() => writeFileSync({write:?}, \"x\"), denied));
test(\"no read outside\", () => assert.throws(() => readFileSync({outside:?}), denied));
test(\"no process\", () => assert.throws(() => execSync(\"true\"), denied));
test(\"no worker\", () => assert.throws(() => new Worker(new URL(import.meta.url)), denied));
test(\"no code from strings\", () => assert.throws(() => new Function(\"return 1\"), EvalError));
test(\"no environment\", () => {{
  const kept = Object.keys(process.env).filter((k) => ![\"PATH\", \"__CF_USER_TEXT_ENCODING\", \"NODE_TEST_WORKER_ID\"].includes(k));
  assert.deepEqual(kept, []);
}});
test(\"the output is readable\", () => {{ readFileSync({page:?}); }});
",
        write = dist.join("canary").display().to_string(),
        outside = outside.display().to_string(),
        page = dist.join("index.html").display().to_string(),
    )
}

pub(crate) fn run(dir: &Path, program: &str, args: &[&str]) -> Result {
    let status = Command::new(program)
        .args(args)
        .current_dir(dir)
        .status()
        .map_err(|e| format!("cannot run {program}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`{program} {}` failed", args.join(" ")))
    }
}

/// Writes a file of `web/dist` that `tsc` must not have written.
fn add(to: &Path, contents: &[u8]) -> Result {
    if to.exists() {
        return Err(format!("{}: tsc wrote it already", to.display()));
    }
    let parent = to.parent().ok_or("a file without a directory")?;
    std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    std::fs::write(to, contents).map_err(|e| format!("{}: {e}", to.display()))
}

/// `cargo xtask web` and the gate step "web".
pub fn build(root: &Path) -> Result {
    let web = root.join("web");
    let dist = web.join("dist");
    // Before anything is removed: below a linked `web`, `dist` would be another's.
    unlinked(root)?;
    // Then the old output, so that a failed build leaves none behind.
    if dist.symlink_metadata().is_ok() {
        std::fs::remove_dir_all(&dist).map_err(|e| format!("{}: {e}", dist.display()))?;
    }
    let mut findings = scan(root)?;
    findings.extend(code_texts(root)?);
    if !findings.is_empty() {
        return Err(format!("\n  {}", findings.join("\n  ")));
    }
    let printed = Command::new("node")
        .arg("--version")
        .output()
        .map_err(|e| {
            format!("node_version: cannot run node ({e}); install the major of web/.nvmrc")
        })?;
    let printed = String::from_utf8_lossy(&printed.stdout);
    node_major(&printed, &crate::read(&web.join(".nvmrc"))?)?;
    run(
        &web,
        "npm",
        &[
            "ci",
            "--ignore-scripts",
            "--no-audit",
            "--no-fund",
            "--prefer-offline",
        ],
    )?;
    // Node's permission model takes a directory only if it exists when Node starts.
    std::fs::create_dir(&dist).map_err(|e| format!("{}: {e}", dist.display()))?;
    let mut tsc = sandbox(&[&web]);
    tsc.push(format!("--allow-fs-write={}", dist.display()));
    tsc.extend(["node_modules/typescript/bin/tsc", "-p", "tsconfig.json"].map(str::to_owned));
    node(&web, &tsc)?;
    add(&dist.join("index.html"), page().as_bytes())?;
    for (_, to, from) in VENDOR {
        let from = web.join("node_modules").join(from);
        let module = std::fs::read(&from).map_err(|e| format!("{}: {e}", from.display()))?;
        add(&dist.join(to), &module)?;
    }
    let (package, test_dir) = (web.join("package.json"), web.join("test"));
    let tests: Vec<String> = (files(root, &test_dir)?.iter())
        .map(|file| crate::relative(&test_dir, file))
        .filter(|file| file.ends_with(".test.mjs"))
        .collect();
    if tests.is_empty() {
        return Err("web/test holds no *.test.mjs".to_owned());
    }
    node(&test_dir, &test_run(&[&test_dir, &dist, &package], &tests))?;
    let canaries = root.join("target/xtask-canaries/web");
    std::fs::create_dir_all(&canaries).map_err(|e| format!("{}: {e}", canaries.display()))?;
    let file = canaries.join("sandbox.test.mjs");
    std::fs::write(&file, canary(&web)).map_err(|e| format!("{}: {e}", file.display()))?;
    let reads: [&Path; 4] = [&canaries, &dist, &test_dir, &package];
    let canary_run = test_run(&reads, &["sandbox.test.mjs".to_owned()]);
    node(&canaries, &canary_run).map_err(|e| format!("the sandbox of the web tests leaks: {e}"))
}

/// The arguments of `node --test` in the directory `reads[0]` over `files` relative to it.
/// The runner looks for its files with the first `--allow-fs-read` only (Node 24.19), so
/// that one is the directory it runs in.
pub fn test_run(reads: &[&Path], files: &[String]) -> Vec<String> {
    let mut args = sandbox(reads);
    args.extend(["--test-isolation=none", "--test"].map(str::to_owned));
    args.extend(files.iter().cloned());
    args
}

#[cfg(test)]
#[path = "web_tests.rs"]
mod tests;

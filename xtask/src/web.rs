//! The web build of the gate (ADR-0021 section 4): scans first (the import map names
//! exactly the vendored Preact modules, nothing in `web/` reaches another host, and only
//! `web/src/render/` computes with `Math.`); then the running Node has the major of
//! `web/.nvmrc`, `npm ci` installs the locked packages without scripts, the pinned `tsc`
//! compiles `web/src` into a fresh `web/dist` beside `web/static` and the vendored modules,
//! and `node --test` runs `web/test` on it.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::Result;

/// The import map of `web/static/index.html`: the specifier, the module in `web/dist`, and
/// the file below `web/node_modules` it is copied from.
pub const VENDOR: [(&str, &str, &str); 3] = [
    ("preact", "./vendor/preact.mjs", "preact/dist/preact.mjs"),
    (
        "preact/hooks",
        "./vendor/hooks.mjs",
        "preact/hooks/dist/hooks.mjs",
    ),
    (
        "preact/jsx-runtime",
        "./vendor/jsx-runtime.mjs",
        "preact/jsx-runtime/dist/jsxRuntime.mjs",
    ),
];
/// The one directory of `web/src` whose code may compute (SVG geometry, the sRGB rule
/// ADR-0005 section 5 gives to readers).
const RENDER: &str = "web/src/render/";
const IMPORT_MAP: &str = "<script type=\"importmap\">";

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

/// The page holds one import map, exactly [`VENDOR`], and refers only to its own files
/// (`./`, no `..`).
pub fn import_map_findings(html: &str) -> Vec<String> {
    let mut findings = Vec::new();
    let maps = html.matches("type=\"importmap\"").count();
    let map = (html.split_once(IMPORT_MAP))
        .and_then(|(_, after)| after.split_once("</script>"))
        .map(|(map, _)| map);
    let expected: serde_json::Map<String, serde_json::Value> = (VENDOR.iter())
        .map(|(name, path, _)| ((*name).to_owned(), serde_json::json!(path)))
        .collect();
    let expected = serde_json::json!({ "imports": expected });
    match map.map(serde_json::from_str::<serde_json::Value>) {
        Some(Ok(found)) if maps == 1 && found == expected => {}
        _ => findings.push(format!(
            "web/static/index.html: the import map is not exactly {expected}"
        )),
    }
    for attribute in ["src=\"", "href=\""] {
        for (_, rest) in html.match_indices(attribute).map(|(i, _)| html.split_at(i)) {
            let value = rest[attribute.len()..].split('"').next().unwrap_or("");
            if !value.starts_with("./") || value.contains("..") {
                findings.push(format!(
                    "web/static/index.html: {attribute}{value}\" is not a file of web/dist"
                ));
            }
        }
    }
    findings
}

/// A file of `web/src` or `web/static` names no other host, and computes (`Math.`) only
/// under [`RENDER`].
pub fn source_findings(path: &str, text: &str) -> Vec<String> {
    let mut findings = Vec::new();
    if text.contains("://") {
        findings.push(format!(
            "{path}: names another host (web/ talks only to its own server)"
        ));
    }
    if path.starts_with("web/src/") && !path.starts_with(RENDER) && text.contains("Math.") {
        findings.push(format!(
            "{path}: uses Math. outside {RENDER} (web/ computes nothing, ADR-0021)"
        ));
    }
    findings
}

/// Every file below `dir`, sorted.
fn files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        if path.is_dir() {
            found.extend(files(&path)?);
        } else {
            found.push(path);
        }
    }
    found.sort();
    Ok(found)
}

/// The scans over the sources of `root/web`.
pub fn scan(root: &Path) -> Result<Vec<String>> {
    let web = root.join("web");
    let mut findings = import_map_findings(&crate::read(&web.join("static/index.html"))?);
    for dir in ["src", "static"] {
        for file in files(&web.join(dir))? {
            let text = crate::read(&file)?;
            findings.extend(source_findings(&crate::relative(root, &file), &text));
        }
    }
    Ok(findings)
}

fn run(dir: &Path, program: &str, args: &[&str]) -> Result {
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

fn copy(from: &Path, to: &Path) -> Result {
    let parent = to.parent().ok_or("a file without a directory")?;
    std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    std::fs::copy(from, to).map_err(|e| format!("{} to {}: {e}", from.display(), to.display()))?;
    Ok(())
}

/// `cargo xtask web` and the gate step "web".
pub fn build(root: &Path) -> Result {
    let findings = scan(root)?;
    if !findings.is_empty() {
        return Err(format!("\n  {}", findings.join("\n  ")));
    }
    let web = root.join("web");
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
    let dist = web.join("dist");
    if dist.exists() {
        std::fs::remove_dir_all(&dist).map_err(|e| format!("{}: {e}", dist.display()))?;
    }
    run(
        &web,
        "node",
        &["node_modules/typescript/bin/tsc", "-p", "tsconfig.json"],
    )?;
    let statics = web.join("static");
    for file in files(&statics)? {
        let relative = file.strip_prefix(&statics).map_err(|e| e.to_string())?;
        copy(&file, &dist.join(relative))?;
    }
    for (_, to, from) in VENDOR {
        let to = to
            .strip_prefix("./")
            .ok_or("a vendored module outside web/dist")?;
        copy(&web.join("node_modules").join(from), &dist.join(to))?;
    }
    let tests: Vec<String> = (files(&web.join("test"))?.iter())
        .map(|file| crate::relative(&web, file))
        .filter(|file| file.ends_with(".test.mjs"))
        .collect();
    if tests.is_empty() {
        return Err("web/test holds no *.test.mjs".to_owned());
    }
    let mut args = vec!["--test"];
    args.extend(tests.iter().map(String::as_str));
    run(&web, "node", &args)
}

#[cfg(test)]
#[path = "web_tests.rs"]
mod tests;

//! The npm guard (ADR-0021 section 4): `web/` takes npm packages only from its allowlist,
//! each with an allowed licence, without an install script and from the registry, and it
//! pins the major version of Node. `cargo xtask web-deps` and the gate step "web
//! dependencies" run it.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use crate::Result;

/// The licences a package may carry (SPDX identifiers, ADR-0021 section 4).
pub const LICENSES: [&str; 6] = [
    "MIT",
    "ISC",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "0BSD",
];
/// Where every package comes from.
const REGISTRY: &str = "https://registry.npmjs.org/";
const ALLOWLIST_FORMAT: &str = "simagent.web.allowlist";

/// The four files of `web/` the guard reads, as text.
pub struct Files<'a> {
    pub nvmrc: &'a str,
    pub package: &'a str,
    pub lockfile: &'a str,
    pub allowlist: &'a str,
}

/// One problem: its code and what it names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub code: &'static str,
    pub detail: String,
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}

fn problem(code: &'static str, detail: String) -> Problem {
    Problem { code, detail }
}

fn parse(name: &str, text: &str) -> Result<Value> {
    serde_json::from_str(text).map_err(|e| format!("web/{name}: {e}"))
}

/// Every problem of the files: the Node pin, the lockfile version, the root, then the
/// packages in the order of the lockfile's keys; an error only if a file is no JSON or the
/// allowlist has another format.
pub fn check(files: &Files) -> Result<Vec<Problem>> {
    let package = parse("package.json", files.package)?;
    let lockfile = parse("package-lock.json", files.lockfile)?;
    let allowlist = allowed_packages(&parse("allowlist.json", files.allowlist)?)?;
    let mut problems = node_pin(files.nvmrc, &package, &lockfile);
    if lockfile["lockfileVersion"] != 3 {
        problems.push(problem(
            "lockfile_version",
            format!("lockfileVersion {}, not 3", lockfile["lockfileVersion"]),
        ));
    }
    let entries = lockfile["packages"]
        .as_object()
        .ok_or("web/package-lock.json: no packages")?;
    let root = entries.get("").unwrap_or(&Value::Null);
    problems.extend(root_problems(&package, root, &allowlist));
    for (key, entry) in entries.iter().filter(|(key, _)| !key.is_empty()) {
        problems.extend(entry_problems(key, entry, &allowlist));
    }
    Ok(problems)
}

/// The allowlist: package name to licence.
fn allowed_packages(allowlist: &Value) -> Result<BTreeMap<String, String>> {
    let fits = allowlist["format"] == ALLOWLIST_FORMAT && allowlist["version"] == 1;
    let listed = allowlist["packages"].as_array().filter(|_| fits);
    let listed = listed.ok_or(format!(
        "web/allowlist.json: not {ALLOWLIST_FORMAT} version 1"
    ))?;
    let mut packages = BTreeMap::new();
    for entry in listed {
        let (Some(name), Some(license)) = (entry["name"].as_str(), entry["license"].as_str())
        else {
            return Err(format!(
                "web/allowlist.json: an entry without name or license: {entry}"
            ));
        };
        if packages
            .insert(name.to_owned(), license.to_owned())
            .is_some()
        {
            return Err(format!("web/allowlist.json: {name} twice"));
        }
    }
    Ok(packages)
}

/// `.nvmrc` holds one major version; `engines.node` of the package and of the lockfile's
/// root entry is that major with `.x`.
fn node_pin(nvmrc: &str, package: &Value, lockfile: &Value) -> Vec<Problem> {
    let major = nvmrc.trim();
    if major.is_empty() || !major.bytes().all(|b| b.is_ascii_digit()) {
        return vec![problem(
            "node_pin",
            format!("web/.nvmrc holds {major:?}, not a major version"),
        )];
    }
    let pin = format!("{major}.x");
    let engines = [
        ("package.json", &package["engines"]["node"]),
        (
            "package-lock.json",
            &lockfile["packages"][""]["engines"]["node"],
        ),
    ];
    (engines.into_iter())
        .filter(|(_, node)| node.as_str() != Some(pin.as_str()))
        .map(|(file, node)| {
            problem(
                "node_pin",
                format!("web/{file}: engines.node {node}, not {pin:?} after web/.nvmrc"),
            )
        })
        .collect()
}

/// The kinds of dependency a manifest declares.
const DEPENDENCIES: [&str; 4] = [
    "dependencies",
    "devDependencies",
    "optionalDependencies",
    "peerDependencies",
];
/// The scripts npm runs when it installs the root package.
const INSTALL_SCRIPTS: [&str; 7] = [
    "preinstall",
    "install",
    "postinstall",
    "prepublish",
    "preprepare",
    "prepare",
    "postprepare",
];

/// The root: what `package.json` declares is on the allowlist and is what the lockfile's
/// root entry declares (`npm ci` installs the lockfile), and neither installs with a script.
fn root_problems(
    package: &Value,
    root: &Value,
    allowlist: &BTreeMap<String, String>,
) -> Vec<Problem> {
    let mut problems = Vec::new();
    for kind in DEPENDENCIES {
        let declared = package[kind].as_object().into_iter().flatten();
        for (name, _) in declared.filter(|(name, _)| !allowlist.contains_key(*name)) {
            problems.push(problem(
                "unlisted_package",
                format!("web/package.json {kind}: {name}"),
            ));
        }
        if package[kind] != root[kind] {
            problems.push(problem(
                "manifest_mismatch",
                format!(
                    "{kind}: package.json {}, lockfile {}",
                    package[kind], root[kind]
                ),
            ));
        }
    }
    let scripts = package["scripts"].as_object().into_iter().flatten();
    for (name, _) in scripts.filter(|(name, _)| INSTALL_SCRIPTS.contains(&name.as_str())) {
        problems.push(problem(
            "install_script",
            format!("web/package.json scripts.{name}"),
        ));
    }
    if set(root, "hasInstallScript") {
        problems.push(problem("install_script", "the lockfile's root".to_owned()));
    }
    problems
}

/// A flag of the lockfile counts as set unless it is absent or `false`: a malformed value
/// fails closed.
fn set(entry: &Value, flag: &str) -> bool {
    !matches!(entry.get(flag), None | Some(Value::Bool(false)))
}

/// A name npm publishes: an optional `@scope/`, then one segment of letters, digits, `-`,
/// `.`, `_` and `~` that does not start with `.` or `_`; so no `/`, `%` or `..` reaches the
/// tarball's path.
fn publishable_name(name: &str) -> bool {
    let segment = |part: &str| {
        let first = part.bytes().next();
        first.is_some_and(|b| b != b'.' && b != b'_')
            && (part.bytes()).all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
    };
    match name.strip_prefix('@').map(|scoped| scoped.split_once('/')) {
        Some(Some((scope, base))) => segment(scope) && segment(base),
        Some(None) => false,
        None => segment(name),
    }
}

/// A version npm publishes, as far as the path goes: letters, digits, `.`, `-` and `+`,
/// starting with a digit.
fn publishable_version(version: &str) -> bool {
    (version.bytes().next()).is_some_and(|b| b.is_ascii_digit())
        && (version.bytes()).all(|b| b.is_ascii_alphanumeric() || b".-+".contains(&b))
}

/// The tarball of a package in the registry, as npm resolves it.
fn tarball(name: &str, version: &str) -> String {
    let base = name.rsplit('/').next().unwrap_or(name);
    format!("{REGISTRY}{name}/-/{base}-{version}.tgz")
}

fn entry_problems(key: &str, entry: &Value, allowlist: &BTreeMap<String, String>) -> Vec<Problem> {
    // An alias keeps the real name in `name`; otherwise the last segment of the path is it.
    let last = key.rsplit("node_modules/").next().unwrap_or(key);
    let name = entry["name"].as_str().unwrap_or(last);
    let version = entry["version"].as_str().unwrap_or("?");
    let at = format!("{name}@{version} ({key})");
    let mut problems = Vec::new();
    let license = entry["license"].as_str();
    match allowlist.get(name) {
        None => problems.push(problem("unlisted_package", at.clone())),
        Some(listed) if license != Some(listed.as_str()) => problems.push(problem(
            "license_changed",
            format!("{at}: {}, the allowlist says {listed}", entry["license"]),
        )),
        Some(_) => {}
    }
    if !license.is_some_and(license_allowed) {
        problems.push(problem(
            "license_not_allowed",
            format!("{at}: {}", entry["license"]),
        ));
    }
    if set(entry, "hasInstallScript") {
        problems.push(problem("install_script", at.clone()));
    }
    // The tarball is the package's own, so the name is what installs: a link or a bundled
    // package has none in the registry, and a name or version with a path in it could
    // point the tarball elsewhere.
    let resolved = entry["resolved"].as_str().unwrap_or("");
    let integrity = entry["integrity"].as_str().unwrap_or("");
    let local = set(entry, "link") || set(entry, "inBundle");
    let named = publishable_name(name) && publishable_version(version);
    if local || !named || resolved != tarball(name, version) || !integrity.starts_with("sha512-") {
        problems.push(problem(
            "not_from_the_registry",
            format!(
                "{at}: resolved {resolved:?}, integrity {integrity:?}, link {}, inBundle {}",
                entry["link"], entry["inBundle"]
            ),
        ));
    }
    problems
}

/// Whether an SPDX expression passes: every identifier of an `AND` and one side of an `OR`
/// on [`LICENSES`]; `AND` binds tighter than `OR`, parentheses group. Anything else
/// (`WITH`, `+`, `LicenseRef-`, a stray word) fails.
pub fn license_allowed(expression: &str) -> bool {
    let spaced = expression.replace('(', " ( ").replace(')', " ) ");
    let mut tokens = spaced.split_whitespace().peekable();
    let passes = or_expression(&mut tokens);
    passes.is_some_and(|passes| passes && tokens.next().is_none())
}

type Tokens<'a> = std::iter::Peekable<std::str::SplitWhitespace<'a>>;

fn or_expression(tokens: &mut Tokens) -> Option<bool> {
    let mut passes = and_expression(tokens)?;
    while tokens.next_if_eq(&"OR").is_some() {
        let side = and_expression(tokens)?;
        passes = passes || side;
    }
    Some(passes)
}

fn and_expression(tokens: &mut Tokens) -> Option<bool> {
    let mut passes = atom(tokens)?;
    while tokens.next_if_eq(&"AND").is_some() {
        let part = atom(tokens)?;
        passes = passes && part;
    }
    Some(passes)
}

fn atom(tokens: &mut Tokens) -> Option<bool> {
    match tokens.next()? {
        "(" => {
            let inner = or_expression(tokens)?;
            tokens.next_if_eq(&")").map(|_| inner)
        }
        ")" | "AND" | "OR" | "WITH" => None,
        // An SPDX identifier: letters, digits, `.` and `-`.
        identifier => (identifier.bytes())
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
            .then(|| LICENSES.contains(&identifier)),
    }
}

/// The files by which npm, or another package manager, would install packages.
const MANIFESTS: [&str; 7] = [
    "package.json",
    "package-lock.json",
    "npm-shrinkwrap.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "bun.lock",
    "bun.lockb",
];
/// The only manifests the repository may hold.
const OWN_MANIFESTS: [&str; 2] = ["web/package.json", "web/package-lock.json"];

/// A manifest anywhere but in [`OWN_MANIFESTS`] (`npm-shrinkwrap.json` in `web/` would even
/// win over the lockfile); `paths` relative to the root, with `/`.
pub fn stray_manifests(paths: &[String]) -> Vec<Problem> {
    (paths.iter())
        .filter(|path| {
            let name = path.rsplit('/').next().unwrap_or(path);
            MANIFESTS.contains(&name) && !OWN_MANIFESTS.contains(&path.as_str())
        })
        .map(|path| problem("another_manifest", path.clone()))
        .collect()
}

/// The directories the walk leaves out: git's, cargo's, the private docs and the packages
/// npm installed for `web/`.
const NOT_WALKED: [&str; 4] = [".git", "target", "docs", "web/node_modules"];

/// The files below `root`, relative to it, without [`NOT_WALKED`]; symbolic links are not
/// followed.
fn files_below(root: &Path, dir: &Path, found: &mut Vec<String>) -> Result {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        let path = entry.path();
        let relative = crate::relative(root, &path);
        if !kind.is_dir() {
            found.push(relative);
        } else if !NOT_WALKED.contains(&relative.as_str()) {
            files_below(root, &path, found)?;
        }
    }
    Ok(())
}

/// `cargo xtask web-deps` and the gate step: the files of `root/web`, and no other
/// manifest in the repository.
pub fn command(root: &Path) -> Result {
    let web = root.join("web");
    let read = |name: &str| crate::read(&web.join(name));
    let (nvmrc, package) = (read(".nvmrc")?, read("package.json")?);
    let (lockfile, allowlist) = (read("package-lock.json")?, read("allowlist.json")?);
    let files = Files {
        nvmrc: &nvmrc,
        package: &package,
        lockfile: &lockfile,
        allowlist: &allowlist,
    };
    let mut problems = check(&files)?;
    let mut paths = Vec::new();
    files_below(root, root, &mut paths)?;
    // The order of a directory listing depends on the file system.
    paths.sort();
    problems.extend(stray_manifests(&paths));
    if problems.is_empty() {
        Ok(())
    } else {
        let lines: Vec<String> = problems.iter().map(Problem::to_string).collect();
        Err(format!("\n  {}", lines.join("\n  ")))
    }
}

#[cfg(test)]
#[path = "web_deps_tests.rs"]
mod tests;

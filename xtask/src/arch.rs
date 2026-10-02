//! Architecture test over `cargo metadata` (ADR-0000; ADR-0003 AT1 to AT5 grow here as
//! their dependencies arrive). Every rule is data in this file: changing it changes a
//! guard and belongs in its own commit with a five-line report (ADR-0000).

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Exact list of workspace members. A new crate needs an ADR and an entry here.
pub const MEMBERS: [&str; 4] = ["sim-math", "sim-core", "sim-cli", "xtask"];

/// Allowed normal and build edges between workspace members.
pub const EDGES: [(&str, &[&str]); 4] = [
    ("sim-math", &[]),
    ("sim-core", &["sim-math"]),
    ("sim-cli", &["sim-core", "sim-math"]),
    // The gate tool computes with the pinned math: the colour rule of the body contract
    // linearizes sRGB (ADR-0005 section 5) and must not use the float methods of std.
    ("xtask", &["sim-math"]),
];

/// Positive list for the normal and build closure of `sim-core` (ADR-0000).
pub const CORE_ALLOWED: [&str; 19] = [
    "sim-core",
    "sim-math",
    "libm",
    "xxhash-rust",
    // serde for log entries, inputs and saves (ADR-0004); the derive macro and its
    // compile-time dependencies run on the host, not in the core.
    "serde",
    "serde_core",
    "serde_derive",
    "proc-macro2",
    "quote",
    "syn",
    "unicode-ident",
    // postcard for the canonical encoding (ADR-0003 AT3); cobs is a fixed dependency of
    // postcard (COBS framing, unused by to_allocvec) and brings thiserror.
    "postcard",
    "cobs",
    "thiserror",
    "thiserror-impl",
    // ChaCha8 for the addressed random draws (ADR-0001 section 2). rand_core, ppv-lite86
    // and zerocopy are the fixed dependencies of rand_chacha 0.10.0; no getrandom.
    "rand_chacha",
    "rand_core",
    "ppv-lite86",
    "zerocopy",
];

/// Further packages allowed in the normal and build closure of the tool crates.
pub const TOOL_ALLOWED: [&str; 11] = [
    "sim-cli",
    "xtask",
    "serde_json",
    "serde_core",
    "itoa",
    "memchr",
    "zmij",
    // clap for the arguments of sim-cli, builder API only, features `std` only
    // (ADR-0000). clap_builder, clap_lex and anstyle are its fixed dependencies.
    "clap",
    "clap_builder",
    "clap_lex",
    "anstyle",
];

/// Packages allowed only in the test graph (reached through dev-dependencies): proptest
/// with default features off and only `std` (step 2c). rand and getrandom are banned in
/// the normal graph but allowed here: every proptest runs with a fixed seed (ADR-0001).
/// rand_chacha and rand_core stand here for the older versions that proptest brings;
/// the versions of the core are on the core list.
pub const DEV_ALLOWED: [&str; 13] = [
    "proptest",
    "rand",
    "rand_core",
    "rand_chacha",
    "rand_xorshift",
    "getrandom",
    "libc",
    "cfg-if",
    "num-traits",
    "autocfg",
    "bitflags",
    "regex-syntax",
    "unarray",
];

/// Resolved features allowed per package in the closure of `sim-core`; a package that
/// is not listed must resolve without features (ADR-0000 B1: feature unification).
pub const CORE_FEATURES_ALLOWED: [(&str, &[&str]); 10] = [
    ("xxhash-rust", &["xxh3"]),
    // simd is the default of both in rand_chacha 0.10.0 (ADR-0001 section 7). std on
    // ppv-lite86 arrives only through the test graph (proptest); it changes the choice
    // of SIMD path, not the values. The wasm32v1-none build proves the core without it.
    ("ppv-lite86", &["simd", "std"]),
    ("zerocopy", &["simd"]),
    ("postcard", &["alloc"]),
    ("serde", &["alloc", "derive", "serde_derive"]),
    // std arrives through feature unification with serde_json (xtask, test graph); it
    // only adds impls for std types. The wasm32v1-none build proves the core without it.
    ("serde_core", &["alloc", "result", "std"]),
    ("serde_derive", &["default"]),
    ("proc-macro2", &["default", "proc-macro"]),
    ("quote", &["default", "proc-macro"]),
    (
        "syn",
        &[
            "clone-impls",
            "default",
            "derive",
            "parsing",
            "printing",
            "proc-macro",
        ],
    ),
];

/// Dependencies pinned by ADR-0003 (AT1 to AT3): package, the only member that may
/// depend on it directly, the exact version requirement, and exactly the features it
/// enables. Default features must be off.
pub const PINNED: [(&str, &str, &str, &[&str]); 4] = [
    ("libm", "sim-math", "=0.2.16", &[]),
    ("xxhash-rust", "sim-core", "=0.8.19", &["xxh3"]),
    ("postcard", "sim-core", "=1.1.3", &["alloc"]),
    ("rand_chacha", "sim-core", "=0.10.0", &[]),
];

/// Crates that must not declare Cargo features of their own (ADR-0000).
pub const NO_OWN_FEATURES: [&str; 2] = ["sim-math", "sim-core"];

/// Packages that must never appear in the normal or build closure of any member.
pub const NEVER: [&str; 9] = [
    "rand",
    "getrandom",
    "bevy",
    "bevy_ecs",
    "rayon",
    "tokio",
    "hashbrown",
    "wgpu",
    "winit",
];

/// Packages that `cargo metadata` resolves but cargo never builds, with the reason.
/// cargo metadata lists an optional dependency as soon as a weak feature
/// (`dep?/feature`) names it. Only these packages may be missing from the build set;
/// every other difference between metadata and `cargo tree` is a violation.
pub const NOT_BUILT: [(&str, &str); 1] = [(
    "embedded-io",
    "optional dependency of postcard, named only by the weak feature `embedded-io-04?/alloc`",
)];

/// All violations of the architecture rules in `metadata` (output of
/// `cargo metadata --format-version 1`). `built` is the set of package names that
/// `cargo tree` actually builds for the target; it must equal the resolve graph except
/// for the packages on `NOT_BUILT`, which then drop out of every closure. `None`
/// checks the metadata as it is.
pub fn check(metadata: &Value, built: Option<&BTreeSet<(String, String)>>) -> Vec<String> {
    let mut graph = match Graph::parse(metadata) {
        Ok(graph) => graph,
        Err(message) => return vec![format!("cargo metadata: {message}")],
    };
    let mut violations = Vec::new();
    if let Some(built) = built {
        check_build_set(&graph, built, &mut violations);
        graph.drop_not_built(built);
    }
    check_members(&graph, &mut violations);
    let mut normal_union = BTreeSet::new();
    for &member in &graph.members {
        let name = graph.name(member);
        check_member_edges(&graph, member, &mut violations);
        let closure = graph.closure(&[member]);
        for &package in &closure {
            check_closure_package(name, graph.name(package), &mut violations);
        }
        normal_union.extend(closure);
    }
    if built.is_some() {
        check_one_version(&graph, &normal_union, &mut violations);
    }
    for &member in &graph.members {
        check_dev_graph(&graph, member, &normal_union, &mut violations);
    }
    if let Some(core) = graph.member_named("sim-core") {
        let mut core_closure = graph.closure(&[core]);
        core_closure.insert(core);
        for package in core_closure {
            check_core_features(
                graph.name(package),
                graph.resolved(package),
                &mut violations,
            );
        }
    }
    check_pinned(&graph, &mut violations);
    check_no_rename(&graph, &mut violations);
    check_serde_json(&graph, &mut violations);
    for (id, declared) in &graph.declared {
        let name = graph.name(id);
        if NO_OWN_FEATURES.contains(&name) && !declared.is_empty() {
            violations.push(format!(
                "{name} declares own features ({}); the core has none (ADR-0000)",
                declared.join(", ")
            ));
        }
    }
    violations
}

fn check_members(graph: &Graph, violations: &mut Vec<String>) {
    let actual: BTreeSet<&str> = graph.members.iter().map(|id| graph.name(id)).collect();
    for name in &actual {
        if !MEMBERS.contains(name) {
            violations.push(format!(
                "unknown workspace member {name}: a new crate needs an ADR and an entry in xtask/src/arch.rs"
            ));
        }
    }
    for name in MEMBERS {
        if !actual.contains(name) {
            violations.push(format!("workspace member {name} is missing"));
        }
    }
}

fn check_member_edges(graph: &Graph, member: &str, violations: &mut Vec<String>) {
    let name = graph.name(member);
    let allowed: &[&str] = EDGES
        .iter()
        .find(|(from, _)| *from == name)
        .map_or(&[], |(_, to)| *to);
    for &(dep, normal) in graph.edges(member) {
        let dep_name = graph.name(dep);
        if normal && graph.members.contains(&dep) && !allowed.contains(&dep_name) {
            violations.push(format!("edge {name} -> {dep_name} is not allowed (EDGES)"));
        }
    }
}

fn check_closure_package(member: &str, package: &str, violations: &mut Vec<String>) {
    if NEVER.contains(&package) {
        violations.push(format!(
            "{package} is in the closure of {member} (never list)"
        ));
    }
    let core_member = member == "sim-core" || member == "sim-math";
    if core_member && !CORE_ALLOWED.contains(&package) {
        violations.push(format!(
            "{package} is in the closure of {member} but not on the core positive list"
        ));
    } else if !core_member && !CORE_ALLOWED.contains(&package) && !TOOL_ALLOWED.contains(&package) {
        violations.push(format!(
            "{package} is in the closure of {member} but not on the positive list"
        ));
    }
}

fn check_dev_graph(
    graph: &Graph,
    member: &str,
    normal_union: &BTreeSet<&str>,
    violations: &mut Vec<String>,
) {
    let dev_roots: Vec<&str> = graph
        .edges(member)
        .iter()
        .filter(|(_, normal)| !normal)
        .map(|(dep, _)| *dep)
        .collect();
    let mut dev_graph = graph.closure(&dev_roots);
    dev_graph.extend(dev_roots);
    for package in dev_graph {
        let package_name = graph.name(package);
        let known = normal_union.contains(package) || graph.members.contains(&package);
        if !known && !DEV_ALLOWED.contains(&package_name) {
            violations.push(format!(
                "{package_name} is a dev dependency of {} but not on the dev list",
                graph.name(member)
            ));
        }
    }
}

fn check_core_features(package: &str, resolved: &[&str], violations: &mut Vec<String>) {
    let allowed: &[&str] = CORE_FEATURES_ALLOWED
        .iter()
        .find(|(name, _)| *name == package)
        .map_or(&[], |(_, features)| *features);
    for feature in resolved {
        if !allowed.contains(feature) {
            violations.push(format!(
                "feature {feature} of {package} is resolved in the core closure"
            ));
        }
    }
}

/// AT5 (ADR-0003 5.7): `serde_json` only with the feature `float_roundtrip`, so that
/// every finite f64 survives writing and reading bit for bit. The rule that carries is
/// the first: a member can only call serde_json if it declares it, and then its own
/// declaration puts the feature into every build of that member. The resolved feature
/// list is a second look only: cargo metadata gives one list per package, the union
/// over all kinds of dependencies. That serde_json stays out of the core is the job of
/// the core positive list.
fn check_serde_json(graph: &Graph, violations: &mut Vec<String>) {
    const PACKAGE: &str = "serde_json";
    const FEATURE: &str = "float_roundtrip";
    for &member in &graph.members {
        let declares_without = graph.specs.get(member).is_some_and(|specs| {
            specs
                .iter()
                .any(|spec| spec.name == PACKAGE && !spec.features.contains(&FEATURE))
        });
        if declares_without {
            violations.push(format!(
                "{} declares {PACKAGE} without the feature {FEATURE} (ADR-0003 AT5)",
                graph.name(member)
            ));
        }
    }
    for id in graph.edges.keys() {
        if graph.name(id) == PACKAGE && !graph.resolved(id).contains(&FEATURE) {
            violations.push(format!(
                "{PACKAGE} resolves without the feature {FEATURE} (ADR-0003 AT5)"
            ));
        }
    }
}

/// The scans and the positive lists find crates by their name. A member that renames a
/// dependency would name it in the code as something else.
fn check_no_rename(graph: &Graph, violations: &mut Vec<String>) {
    for &member in &graph.members {
        for spec in graph.specs.get(member).into_iter().flatten() {
            if let Some(alias) = spec.rename {
                violations.push(format!(
                    "{} renames its dependency {} to {alias}; the scans find crates by name (ADR-0000)",
                    graph.name(member),
                    spec.name
                ));
            }
        }
    }
}

/// Checks the manifests of the workspace members only: third-party crates may declare
/// libm themselves (num-traits does, optionally); what they change in the build shows
/// up in the resolved features of the core closure.
fn check_pinned(graph: &Graph, violations: &mut Vec<String>) {
    for (id, specs) in &graph.specs {
        if !graph.members.contains(id) {
            continue;
        }
        let dependent = graph.name(id);
        for spec in specs {
            let Some((package, owner, requirement, features)) =
                PINNED.iter().find(|(package, ..)| *package == spec.name)
            else {
                continue;
            };
            if dependent != *owner {
                violations.push(format!(
                    "{package} may only be a direct dependency of {owner}, not of {dependent} (ADR-0003)"
                ));
            }
            if spec.requirement != *requirement {
                violations.push(format!(
                    "{dependent} requires {package} as {:?}; it must be {requirement:?} (ADR-0003)",
                    spec.requirement
                ));
            }
            if spec.default_features {
                violations.push(format!(
                    "{dependent} must use {package} with default-features = false (ADR-0003)"
                ));
            }
            let mut declared = spec.features.clone();
            declared.sort_unstable();
            let mut expected = features.to_vec();
            expected.sort_unstable();
            if declared != expected {
                violations.push(format!(
                    "{dependent} enables features {declared:?} of {package}; it must enable exactly {expected:?} (ADR-0003)"
                ));
            }
        }
    }
}

/// Name and version from a line of `cargo tree --prefix none --format {p}`:
/// `name vX.Y.Z`, optionally followed by a path or `(*)`.
pub fn parse_tree_line(line: &str) -> Option<(String, String)> {
    let mut tokens = line.split_whitespace();
    let name = tokens.next()?;
    let version = tokens.next()?.strip_prefix('v')?;
    let is_name = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    let starts_with_digit = version.starts_with(|c: char| c.is_ascii_digit());
    (is_name && starts_with_digit).then(|| (name.to_string(), version.to_string()))
}

/// The build set and the resolve graph must name the same packages in the same
/// versions, except for the packages on `NOT_BUILT`. A build set that is empty, cut
/// short or parsed from an unexpected output therefore never passes.
fn check_build_set(
    graph: &Graph,
    built: &BTreeSet<(String, String)>,
    violations: &mut Vec<String>,
) {
    let resolved: BTreeSet<(&str, &str)> = graph
        .edges
        .keys()
        .map(|id| (graph.name(id), graph.version(id)))
        .collect();
    for &(name, version) in &resolved {
        let listed = NOT_BUILT.iter().any(|(package, _)| *package == name);
        match (graph.is_built(name, version, built), listed) {
            (false, false) => violations.push(format!(
                "{name} {version} is in cargo metadata but cargo tree does not build it, and it is not on the not-built list"
            )),
            (true, true) => violations.push(format!(
                "{name} {version} is on the not-built list but cargo tree builds it"
            )),
            (true, false) | (false, true) => {}
        }
    }
    for (name, version) in built {
        if !resolved.contains(&(name.as_str(), version.as_str())) {
            violations.push(format!(
                "cargo tree builds {name} {version}, which cargo metadata does not list"
            ));
        }
    }
}

/// The positive lists, the never list and the feature list of the normal closures go by
/// package name, so no name may be in them in two versions. A further version that only
/// the test graph has is the business of the dev list.
fn check_one_version(graph: &Graph, normal_union: &BTreeSet<&str>, violations: &mut Vec<String>) {
    let mut versions_of: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for id in normal_union {
        versions_of
            .entry(graph.name(id))
            .or_default()
            .insert(graph.version(id));
    }
    for (name, versions) in versions_of {
        if versions.len() > 1 {
            let versions: Vec<&str> = versions.into_iter().collect();
            violations.push(format!(
                "{name} is built in two versions ({}); the lists go by package name",
                versions.join(", ")
            ));
        }
    }
}

/// A dependency as declared in a package manifest.
struct Spec<'a> {
    name: &'a str,
    /// The name in the code if the manifest renames the dependency.
    rename: Option<&'a str>,
    requirement: &'a str,
    default_features: bool,
    features: Vec<&'a str>,
}

/// The parts of `cargo metadata` the rules need, keyed by package id.
struct Graph<'a> {
    names: BTreeMap<&'a str, &'a str>,
    versions: BTreeMap<&'a str, &'a str>,
    declared: BTreeMap<&'a str, Vec<&'a str>>,
    /// Dependency edges; `true` for normal or build, `false` for dev only.
    edges: BTreeMap<&'a str, Vec<(&'a str, bool)>>,
    resolved: BTreeMap<&'a str, Vec<&'a str>>,
    /// Declared dependencies per package (only needed for `PINNED`).
    specs: BTreeMap<&'a str, Vec<Spec<'a>>>,
    members: Vec<&'a str>,
}

impl<'a> Graph<'a> {
    fn parse(metadata: &'a Value) -> Result<Self, String> {
        let mut graph = Graph {
            names: BTreeMap::new(),
            versions: BTreeMap::new(),
            declared: BTreeMap::new(),
            edges: BTreeMap::new(),
            resolved: BTreeMap::new(),
            specs: BTreeMap::new(),
            members: Vec::new(),
        };
        for package in array(metadata, "packages")? {
            let id = string(package, "id")?;
            graph.names.insert(id, string(package, "name")?);
            graph.versions.insert(id, string(package, "version")?);
            let declared = package
                .get("features")
                .and_then(Value::as_object)
                .map(|features| features.keys().map(String::as_str).collect())
                .unwrap_or_default();
            graph.declared.insert(id, declared);
            let mut specs = Vec::new();
            let declared_deps = package.get("dependencies").and_then(Value::as_array);
            for dep in declared_deps.into_iter().flatten() {
                specs.push(Spec {
                    name: string(dep, "name")?,
                    rename: dep.get("rename").and_then(Value::as_str),
                    requirement: string(dep, "req")?,
                    default_features: dep
                        .get("uses_default_features")
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                    features: array(dep, "features")?
                        .iter()
                        .filter_map(Value::as_str)
                        .collect(),
                });
            }
            graph.specs.insert(id, specs);
        }
        for member in array(metadata, "workspace_members")? {
            graph
                .members
                .push(member.as_str().ok_or("workspace member is not a string")?);
        }
        let resolve = metadata.get("resolve").ok_or("no resolve graph")?;
        for node in array(resolve, "nodes")? {
            let id = string(node, "id")?;
            let mut edges = Vec::new();
            for dep in array(node, "deps")? {
                let kinds = array(dep, "dep_kinds")?;
                let normal = kinds
                    .iter()
                    .any(|kind| kind.get("kind").and_then(Value::as_str) != Some("dev"));
                edges.push((string(dep, "pkg")?, normal));
            }
            graph.edges.insert(id, edges);
            let resolved = array(node, "features")?
                .iter()
                .filter_map(Value::as_str)
                .collect();
            graph.resolved.insert(id, resolved);
        }
        Ok(graph)
    }

    /// Drops the edges to packages on `NOT_BUILT` that are indeed not built, so they
    /// drop out of every closure.
    fn drop_not_built(&mut self, built: &BTreeSet<(String, String)>) {
        let mut dropped = BTreeSet::new();
        for &id in self.edges.keys() {
            let name = self.name(id);
            let listed = NOT_BUILT.iter().any(|(package, _)| *package == name);
            if listed && !self.is_built(name, self.version(id), built) {
                dropped.insert(id);
            }
        }
        for edges in self.edges.values_mut() {
            edges.retain(|(dep, _)| !dropped.contains(dep));
        }
    }

    fn is_built(&self, name: &str, version: &str, built: &BTreeSet<(String, String)>) -> bool {
        built
            .iter()
            .any(|(built_name, built_version)| built_name == name && built_version == version)
    }

    fn version(&self, id: &str) -> &'a str {
        self.versions
            .get(id)
            .copied()
            .unwrap_or("<unknown version>")
    }

    fn name(&self, id: &str) -> &'a str {
        self.names.get(id).copied().unwrap_or("<unknown package>")
    }

    fn member_named(&self, name: &str) -> Option<&'a str> {
        self.members
            .iter()
            .copied()
            .find(|id| self.name(id) == name)
    }

    fn edges(&self, id: &str) -> &[(&'a str, bool)] {
        self.edges.get(id).map_or(&[], Vec::as_slice)
    }

    fn resolved(&self, id: &str) -> &[&'a str] {
        self.resolved.get(id).map_or(&[], Vec::as_slice)
    }

    /// Packages reachable from `roots` over normal and build edges, without the roots.
    fn closure(&self, roots: &[&'a str]) -> BTreeSet<&'a str> {
        let mut seen = BTreeSet::new();
        let mut stack: Vec<&'a str> = roots.to_vec();
        while let Some(id) = stack.pop() {
            for &(dep, normal) in self.edges(id) {
                if normal && seen.insert(dep) {
                    stack.push(dep);
                }
            }
        }
        for root in roots {
            seen.remove(root);
        }
        seen
    }
}

fn array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>, String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("field {key} is missing or not an array"))
}

fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("field {key} is missing or not a string"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Builds a minimal `cargo metadata` document. Packages whose name is in `MEMBERS`
    /// become workspace members unless `members` overrides that.
    struct Fixture {
        packages: Vec<(String, Vec<String>)>,
        deps: Vec<(String, String, Option<&'static str>)>,
        features: Vec<(String, Vec<String>)>,
        /// Declared dependencies: (from, to, requirement, default features, features).
        specs: Vec<(String, String, String, bool, Vec<String>)>,
        /// Renamed dependencies: (from, to, name in the code).
        renames: Vec<(String, String, String)>,
        members: Option<Vec<String>>,
    }

    impl Fixture {
        /// The real shape of the workspace after step 1b.
        fn workspace() -> Self {
            let mut fixture = Fixture {
                packages: Vec::new(),
                deps: Vec::new(),
                features: Vec::new(),
                specs: Vec::new(),
                renames: Vec::new(),
                members: None,
            };
            for name in MEMBERS.into_iter().chain([
                "libm",
                "xxhash-rust",
                "serde_json",
                "serde_core",
                "itoa",
                "memchr",
                "zmij",
            ]) {
                fixture = fixture.package(name, &[]);
            }
            fixture
                .dep("sim-math", "libm", None)
                .spec("sim-math", "libm", "=0.2.16", false, &[])
                .dep("sim-core", "xxhash-rust", None)
                .spec("sim-core", "xxhash-rust", "=0.8.19", false, &["xxh3"])
                .resolved("xxhash-rust", &["xxh3"])
                .dep("sim-core", "sim-math", None)
                .dep("sim-cli", "sim-core", None)
                .dep("xtask", "serde_json", None)
                .spec("xtask", "serde_json", "1", true, &["float_roundtrip"])
                .resolved("serde_json", &["default", "float_roundtrip", "std"])
                .dep("serde_json", "serde_core", None)
                .dep("serde_json", "itoa", None)
                .dep("serde_json", "memchr", None)
                .dep("serde_json", "zmij", None)
        }

        fn package(mut self, name: &str, own_features: &[&str]) -> Self {
            let features = own_features.iter().map(|f| (*f).to_string()).collect();
            self.packages.push((name.to_string(), features));
            self
        }

        fn spec(
            mut self,
            from: &str,
            to: &str,
            requirement: &str,
            default_features: bool,
            features: &[&str],
        ) -> Self {
            let features = features.iter().map(|f| (*f).to_string()).collect();
            self.specs.push((
                from.to_string(),
                to.to_string(),
                requirement.to_string(),
                default_features,
                features,
            ));
            self
        }

        fn renamed(mut self, from: &str, to: &str, alias: &str) -> Self {
            self.renames
                .push((from.to_string(), to.to_string(), alias.to_string()));
            self
        }

        fn dep(mut self, from: &str, to: &str, kind: Option<&'static str>) -> Self {
            self.deps.push((from.to_string(), to.to_string(), kind));
            self
        }

        fn resolved(mut self, name: &str, features: &[&str]) -> Self {
            let features = features.iter().map(|f| (*f).to_string()).collect();
            self.features.push((name.to_string(), features));
            self
        }

        fn members(mut self, names: &[&str]) -> Self {
            self.members = Some(names.iter().map(|n| (*n).to_string()).collect());
            self
        }

        fn build(&self) -> Value {
            // A key is `name` (version 0.1.0) or `name@version`.
            let split = |key: &str| -> (String, String) {
                match key.split_once('@') {
                    Some((name, version)) => (name.to_string(), version.to_string()),
                    None => (key.to_string(), "0.1.0".to_string()),
                }
            };
            let id = |key: &str| {
                let (name, version) = split(key);
                format!("id+{name}#{version}")
            };
            let packages: Vec<Value> = self
                .packages
                .iter()
                .map(|(name, own)| {
                    let own: serde_json::Map<String, Value> =
                        own.iter().map(|f| (f.clone(), json!([]))).collect();
                    let dependencies: Vec<Value> = self
                        .specs
                        .iter()
                        .filter(|(from, ..)| from == name)
                        .map(|(from, to, requirement, default_features, features)| {
                            let rename = self
                                .renames
                                .iter()
                                .find(|(f, t, _)| f == from && t == to)
                                .map(|(_, _, alias)| alias.clone());
                            json!({
                                "name": to,
                                "req": requirement,
                                "uses_default_features": default_features,
                                "features": features,
                                "rename": rename,
                                "kind": null
                            })
                        })
                        .collect();
                    let (plain, version) = split(name);
                    json!({
                        "name": plain,
                        "version": version,
                        "id": id(name),
                        "features": own,
                        "dependencies": dependencies
                    })
                })
                .collect();
            let nodes: Vec<Value> = self
                .packages
                .iter()
                .map(|(name, _)| {
                    let deps: Vec<Value> = self
                        .deps
                        .iter()
                        .filter(|(from, _, _)| from == name)
                        .map(|(_, to, kind)| {
                            json!({
                                "name": split(to).0.replace('-', "_"),
                                "pkg": id(to),
                                "dep_kinds": [{ "kind": kind, "target": null }]
                            })
                        })
                        .collect();
                    let features: Vec<String> = self
                        .features
                        .iter()
                        .filter(|(n, _)| n == name)
                        .flat_map(|(_, f)| f.clone())
                        .collect();
                    json!({ "id": id(name), "deps": deps, "features": features })
                })
                .collect();
            let members: Vec<String> = match &self.members {
                Some(names) => names.iter().map(|n| id(n)).collect(),
                None => MEMBERS.iter().map(|n| id(n)).collect(),
            };
            json!({
                "packages": packages,
                "workspace_members": members,
                "resolve": { "nodes": nodes }
            })
        }
    }

    fn violations(fixture: Fixture) -> Vec<String> {
        check(&fixture.build(), None)
    }

    #[test]
    fn packages_that_are_never_built_are_ignored() {
        let fixture = Fixture::workspace()
            .package("embedded-io", &[])
            .dep("sim-core", "embedded-io", None)
            .resolved("embedded-io", &["alloc"]);
        let metadata = fixture.build();
        assert!(
            !check(&metadata, None).is_empty(),
            "without the build set the phantom is a violation"
        );
        let built = built_workspace();
        assert_eq!(check(&metadata, Some(&built)), Vec::<String>::new());
    }

    /// Every package of the workspace fixture, as `cargo tree` would list them.
    fn built_workspace() -> BTreeSet<(String, String)> {
        let names = [
            "libm",
            "xxhash-rust",
            "serde_json",
            "serde_core",
            "itoa",
            "memchr",
            "zmij",
        ];
        MEMBERS.into_iter().chain(names).map(built).collect()
    }

    /// A package of the fixture as `cargo tree` lists it: `name` or `name@version`.
    fn built(key: &str) -> (String, String) {
        let (name, version) = key.split_once('@').unwrap_or((key, "0.1.0"));
        (name.to_string(), version.to_string())
    }

    fn assert_build_set_reports(
        fixture: Fixture,
        built: &BTreeSet<(String, String)>,
        needle: &str,
    ) {
        let found = check(&fixture.build(), Some(built));
        assert!(
            found.iter().any(|v| v.contains(needle)),
            "expected a violation mentioning {needle:?}, got {found:?}"
        );
    }

    #[test]
    fn the_workspace_with_its_build_set_is_clean() {
        let found = check(&Fixture::workspace().build(), Some(&built_workspace()));
        assert_eq!(found, Vec::<String>::new());
    }

    #[test]
    fn an_empty_build_set_is_a_violation() {
        assert_build_set_reports(
            Fixture::workspace(),
            &BTreeSet::new(),
            "sim-core 0.1.0 is in cargo metadata but cargo tree does not build it",
        );
    }

    #[test]
    fn a_package_missing_from_the_build_set_must_be_on_the_not_built_list() {
        let fixture = Fixture::workspace()
            .package("left-pad", &[])
            .dep("sim-core", "left-pad", None);
        assert_build_set_reports(
            fixture,
            &built_workspace(),
            "left-pad 0.1.0 is in cargo metadata but cargo tree does not build it",
        );
    }

    #[test]
    fn a_never_package_cannot_hide_behind_the_build_set() {
        let fixture = Fixture::workspace()
            .package("rand", &[])
            .dep("sim-core", "rand", None);
        assert_build_set_reports(
            fixture,
            &built_workspace(),
            "rand 0.1.0 is in cargo metadata",
        );
    }

    #[test]
    fn a_not_built_entry_that_is_built_is_stale() {
        let fixture =
            Fixture::workspace()
                .package("embedded-io", &[])
                .dep("sim-core", "embedded-io", None);
        let mut built_set = built_workspace();
        built_set.insert(built("embedded-io"));
        assert_build_set_reports(
            fixture,
            &built_set,
            "embedded-io 0.1.0 is on the not-built list but cargo tree builds it",
        );
    }

    #[test]
    fn the_build_set_must_come_from_the_same_graph() {
        let mut built_set = built_workspace();
        built_set.insert(built("left-pad@1.0.0"));
        assert_build_set_reports(
            Fixture::workspace(),
            &built_set,
            "cargo tree builds left-pad 1.0.0, which cargo metadata does not list",
        );
    }

    /// A built version that the graph does not have, of a package the graph knows.
    #[test]
    fn a_built_version_must_be_in_the_graph() {
        let mut built_set = built_workspace();
        built_set.remove(&built("xxhash-rust"));
        built_set.insert(built("xxhash-rust@9.9.9"));
        assert_build_set_reports(
            Fixture::workspace(),
            &built_set,
            "cargo tree builds xxhash-rust 9.9.9, which cargo metadata does not list",
        );
    }

    /// The comparison is by name and version: an allowed name does not cover a second
    /// version that cargo resolves but does not build.
    #[test]
    fn an_unbuilt_second_version_must_be_on_the_not_built_list() {
        let fixture = Fixture::workspace().package("xxhash-rust@9.9.9", &[]).dep(
            "serde_json",
            "xxhash-rust@9.9.9",
            None,
        );
        assert_build_set_reports(
            fixture,
            &built_workspace(),
            "xxhash-rust 9.9.9 is in cargo metadata but cargo tree does not build it",
        );
    }

    /// The test graph may hold another version of a core package: proptest brings its
    /// own rand_chacha next to the one the core pins. The dev list names it, and the two
    /// versions never meet in a normal build.
    #[test]
    fn a_second_version_only_in_the_test_graph_is_fine() {
        let fixture = Fixture::workspace()
            .package("proptest", &[])
            .package("rand_chacha@0.9.0", &[])
            .dep("sim-core", "proptest", Some("dev"))
            .dep("proptest", "rand_chacha@0.9.0", None)
            .package("libm@9.9.9", &[])
            .dep("proptest", "libm@9.9.9", None);
        let mut built_set = built_workspace();
        built_set.extend([built("proptest"), built("rand_chacha@0.9.0")]);
        // rand_chacha is on the dev list: fine. libm is not: its second version, which
        // exists only in the test graph, is reported by the dev list, not as a duplicate.
        built_set.insert(built("libm@9.9.9"));
        let found = check(&fixture.build(), Some(&built_set));
        assert_eq!(
            found,
            vec!["libm is a dev dependency of sim-core but not on the dev list".to_string()]
        );
    }

    /// A second version that is built would pass every list by its name alone.
    #[test]
    fn two_built_versions_of_one_package_are_a_violation() {
        let fixture = Fixture::workspace().package("xxhash-rust@9.9.9", &[]).dep(
            "serde_json",
            "xxhash-rust@9.9.9",
            None,
        );
        let mut built_set = built_workspace();
        built_set.insert(built("xxhash-rust@9.9.9"));
        assert_build_set_reports(
            fixture,
            &built_set,
            "xxhash-rust is built in two versions (0.1.0, 9.9.9)",
        );
    }

    /// Both versions of a not-built package drop out, as embedded-io under postcard.
    #[test]
    fn two_unbuilt_versions_on_the_not_built_list_are_fine() {
        let fixture = Fixture::workspace()
            .package("embedded-io@0.4.0", &[])
            .package("embedded-io@0.6.1", &[])
            .dep("sim-core", "embedded-io@0.4.0", None)
            .dep("sim-core", "embedded-io@0.6.1", None);
        let found = check(&fixture.build(), Some(&built_workspace()));
        assert_eq!(found, Vec::<String>::new());
    }

    /// A built package and its unbuilt second version on the not-built list: only the
    /// unbuilt one leaves the closures.
    #[test]
    fn a_built_version_stays_in_the_closure_next_to_an_unbuilt_one() {
        let fixture = Fixture::workspace()
            .package("embedded-io@0.4.0", &[])
            .package("embedded-io@0.6.1", &[])
            .dep("sim-core", "embedded-io@0.4.0", None)
            .dep("sim-core", "embedded-io@0.6.1", None);
        let mut built_set = built_workspace();
        built_set.insert(built("embedded-io@0.6.1"));
        let found = check(&fixture.build(), Some(&built_set));
        assert!(
            found.iter().any(|v| v
                .contains("embedded-io 0.6.1 is on the not-built list but cargo tree builds it")),
            "{found:?}"
        );
        assert!(
            found
                .iter()
                .any(|v| v.contains("embedded-io is in the closure of sim-core")),
            "{found:?}"
        );
    }

    #[test]
    fn cargo_tree_lines_give_name_and_version() {
        let pair = |name: &str, version: &str| Some((name.to_string(), version.to_string()));
        assert_eq!(parse_tree_line("syn v3.0.6"), pair("syn", "3.0.6"));
        assert_eq!(parse_tree_line("syn v3.0.6 (*)"), pair("syn", "3.0.6"));
        assert_eq!(
            parse_tree_line("serde_derive v1.0.229 (proc-macro)"),
            pair("serde_derive", "1.0.229")
        );
        assert_eq!(
            parse_tree_line("sim-core v0.1.0 (/home/x/simagent/crates/sim-core)"),
            pair("sim-core", "0.1.0")
        );
        for bad in [
            "",
            "syn",
            "syn 3.0.6",
            "syn vendored",
            "syn v",
            "warning: something happened",
            "warning: v1.0.0 is yanked",
            "some/path v1.0.0",
            "v1.0.0",
        ] {
            assert_eq!(parse_tree_line(bad), None, "{bad:?}");
        }
    }

    /// ADR-0001 canary: rand_chacha only in sim-core, exactly `=0.10.0`, without default
    /// features and without any feature that 0.10.0 offers.
    #[test]
    fn rand_chacha_is_pinned_without_features() {
        let with = |from: &str, requirement: &str, default_features: bool, features: &[&str]| {
            Fixture::workspace()
                .package("rand_chacha", &[])
                .dep(from, "rand_chacha", None)
                .spec(from, "rand_chacha", requirement, default_features, features)
        };
        let clean = with("sim-core", "=0.10.0", false, &[]);
        assert_eq!(violations(clean), Vec::<String>::new());
        assert_reports(
            with("sim-core", "=0.10.0", false, &["serde"]),
            "sim-core enables features [\"serde\"] of rand_chacha; it must enable exactly [] (ADR-0003)",
        );
        assert_reports(
            with("sim-core", "=0.10.0", true, &[]),
            "sim-core must use rand_chacha with default-features = false (ADR-0003)",
        );
        assert_reports(
            with("sim-core", "0.10", false, &[]),
            "sim-core requires rand_chacha as \"0.10\"; it must be \"=0.10.0\" (ADR-0003)",
        );
        assert_reports(
            with("sim-math", "=0.10.0", false, &[]),
            "rand_chacha may only be a direct dependency of sim-core, not of sim-math (ADR-0003)",
        );
    }

    /// Review finding: the scans look for crates by name; a renamed dependency of a
    /// member would be invisible to them.
    #[test]
    fn a_renamed_dependency_of_a_member_is_reported() {
        let plain = || {
            Fixture::workspace()
                .package("rand_chacha", &[])
                .dep("sim-core", "rand_chacha", None)
                .spec("sim-core", "rand_chacha", "=0.10.0", false, &[])
        };
        assert_eq!(violations(plain()), Vec::<String>::new());
        assert_eq!(
            violations(plain().renamed("sim-core", "rand_chacha", "chacha")),
            vec![
                "sim-core renames its dependency rand_chacha to chacha; the scans find crates by name (ADR-0000)"
                    .to_string()
            ]
        );
        // What a third-party crate renames is its own business.
        let foreign = Fixture::workspace()
            .spec("serde_json", "itoa", "1", true, &[])
            .renamed("serde_json", "itoa", "fmt");
        assert_eq!(violations(foreign), Vec::<String>::new());
    }

    /// AT5: a member that declares serde_json enables float_roundtrip.
    #[test]
    fn serde_json_needs_float_roundtrip_where_declared() {
        let fixture = Fixture::workspace()
            .dep("sim-cli", "serde_json", None)
            .spec("sim-cli", "serde_json", "1", true, &[]);
        assert_reports(
            fixture,
            "sim-cli declares serde_json without the feature float_roundtrip (ADR-0003 AT5)",
        );
        // A dev dependency counts as well.
        let fixture = Fixture::workspace()
            .dep("sim-core", "serde_json", Some("dev"))
            .spec("sim-core", "serde_json", "1", true, &["std"]);
        assert_reports(
            fixture,
            "sim-core declares serde_json without the feature float_roundtrip (ADR-0003 AT5)",
        );
    }

    /// AT5: what cargo resolves has the feature, whoever asked for it.
    #[test]
    fn serde_json_must_resolve_with_float_roundtrip() {
        let mut fixture = Fixture::workspace();
        fixture.features.retain(|(name, _)| name != "serde_json");
        let fixture = fixture.resolved("serde_json", &["default", "std"]);
        assert_reports(
            fixture,
            "serde_json resolves without the feature float_roundtrip (ADR-0003 AT5)",
        );
    }

    fn assert_reports(fixture: Fixture, needle: &str) {
        let found = violations(fixture);
        assert!(
            found.iter().any(|v| v.contains(needle)),
            "expected a violation mentioning {needle:?}, got {found:?}"
        );
    }

    #[test]
    fn the_workspace_as_built_is_clean() {
        assert_eq!(violations(Fixture::workspace()), Vec::<String>::new());
    }

    #[test]
    fn a_never_package_is_reported_for_any_member() {
        let fixture =
            Fixture::workspace()
                .package("hashbrown", &[])
                .dep("serde_json", "hashbrown", None);
        assert_reports(fixture, "hashbrown");
    }

    #[test]
    fn the_core_closure_follows_the_positive_list() {
        // libm is on the core list since step 2a; num-traits is not.
        let fixture =
            Fixture::workspace()
                .package("num-traits", &[])
                .dep("sim-math", "num-traits", None);
        assert_reports(
            fixture,
            "num-traits is in the closure of sim-math but not on the core positive list",
        );
    }

    #[test]
    fn a_build_dependency_counts_like_a_normal_one() {
        let fixture = Fixture::workspace()
            .package("cc", &[])
            .dep("sim-core", "cc", Some("build"));
        assert_reports(fixture, "cc");
    }

    #[test]
    fn an_unlisted_tool_dependency_is_reported() {
        let fixture = Fixture::workspace()
            .package("regex", &[])
            .dep("xtask", "regex", None);
        assert_reports(fixture, "regex");
    }

    #[test]
    fn edges_between_members_follow_the_matrix() {
        let fixture = Fixture::workspace().dep("sim-core", "sim-cli", None);
        assert_reports(fixture, "sim-core -> sim-cli");
        // The gate tool computes with the pinned math (the colour rule of the body
        // contract, ADR-0005 section 5), so it may use sim-math and nothing else yet.
        let with_math = Fixture::workspace().dep("xtask", "sim-math", None);
        assert_eq!(violations(with_math), Vec::<String>::new());
        for member in ["sim-core", "sim-cli"] {
            let fixture = Fixture::workspace().dep("xtask", member, None);
            assert_eq!(
                violations(fixture),
                vec![format!("edge xtask -> {member} is not allowed (EDGES)")]
            );
        }
        let backwards = Fixture::workspace().dep("sim-math", "xtask", None);
        assert_reports(backwards, "edge sim-math -> xtask is not allowed (EDGES)");
    }

    #[test]
    fn resolved_features_in_the_core_closure_are_reported() {
        let fixture = Fixture::workspace().resolved("sim-math", &["std"]);
        assert_reports(fixture, "std");
    }

    #[test]
    fn core_crates_declare_no_features() {
        let mut fixture = Fixture::workspace();
        fixture.packages.retain(|(name, _)| name != "sim-core");
        let fixture = fixture.package("sim-core", &["fast"]);
        assert_reports(fixture, "fast");
    }

    #[test]
    fn the_member_list_is_exact() {
        let fixture = Fixture::workspace().package("sim-viewer", &[]).members(&[
            "sim-math",
            "sim-core",
            "sim-cli",
            "xtask",
            "sim-viewer",
        ]);
        assert_reports(fixture, "sim-viewer");
    }

    #[test]
    fn dev_dependencies_need_their_own_list() {
        // proptest is on the dev list since step 2c; criterion is not.
        let fixture = Fixture::workspace().package("criterion", &[]).dep(
            "sim-core",
            "criterion",
            Some("dev"),
        );
        assert_reports(
            fixture,
            "criterion is a dev dependency of sim-core but not on the dev list",
        );
    }

    fn without_libm_spec(fixture: Fixture) -> Fixture {
        let mut fixture = fixture;
        fixture.specs.retain(|(_, to, ..)| to != "libm");
        fixture
    }

    #[test]
    fn libm_may_only_be_a_dependency_of_sim_math() {
        let fixture = Fixture::workspace().dep("sim-core", "libm", None).spec(
            "sim-core",
            "libm",
            "=0.2.16",
            false,
            &[],
        );
        assert_reports(fixture, "libm may only be a direct dependency of sim-math");
    }

    #[test]
    fn libm_is_pinned_exactly() {
        let fixture =
            without_libm_spec(Fixture::workspace()).spec("sim-math", "libm", "0.2", false, &[]);
        assert_reports(fixture, "=0.2.16");
    }

    #[test]
    fn libm_default_features_are_off() {
        let fixture =
            without_libm_spec(Fixture::workspace()).spec("sim-math", "libm", "=0.2.16", true, &[]);
        assert_reports(fixture, "default-features");
    }

    #[test]
    fn libm_enables_no_features() {
        let fixture = without_libm_spec(Fixture::workspace()).spec(
            "sim-math",
            "libm",
            "=0.2.16",
            false,
            &["arch"],
        );
        assert_reports(fixture, "arch");
    }

    fn without_spec(fixture: Fixture, package: &str) -> Fixture {
        let mut fixture = fixture;
        fixture.specs.retain(|(_, to, ..)| to != package);
        fixture
    }

    #[test]
    fn xxhash_rust_may_only_be_a_dependency_of_sim_core() {
        let fixture = Fixture::workspace()
            .dep("sim-cli", "xxhash-rust", None)
            .spec("sim-cli", "xxhash-rust", "=0.8.19", false, &["xxh3"]);
        assert_reports(
            fixture,
            "xxhash-rust may only be a direct dependency of sim-core",
        );
    }

    #[test]
    fn xxhash_rust_enables_exactly_xxh3() {
        let missing = without_spec(Fixture::workspace(), "xxhash-rust").spec(
            "sim-core",
            "xxhash-rust",
            "=0.8.19",
            false,
            &[],
        );
        assert_reports(missing, "must enable exactly");
        let extra = without_spec(Fixture::workspace(), "xxhash-rust").spec(
            "sim-core",
            "xxhash-rust",
            "=0.8.19",
            false,
            &["xxh3", "xxh64"],
        );
        assert_reports(extra, "must enable exactly");
    }

    #[test]
    fn resolved_xxhash_features_beyond_xxh3_are_reported() {
        let fixture = Fixture::workspace().resolved("xxhash-rust", &["xxh3", "std"]);
        assert_reports(fixture, "feature std of xxhash-rust");
    }

    #[test]
    fn third_party_libm_declarations_are_left_to_the_feature_check() {
        let fixture = Fixture::workspace().package("num-traits", &[]).spec(
            "num-traits",
            "libm",
            "^0.2.0",
            true,
            &[],
        );
        let found = violations(fixture);
        assert!(found.iter().all(|v| !v.contains("num-traits")), "{found:?}");
    }

    #[test]
    fn resolved_libm_features_are_reported() {
        let fixture = Fixture::workspace().resolved("libm", &["arch"]);
        assert_reports(fixture, "feature arch of libm");
    }

    #[test]
    fn dev_dependencies_do_not_count_as_normal_ones() {
        // A dev-only package must not be reported as a core closure violation, only
        // as an unlisted dev dependency.
        let fixture =
            Fixture::workspace()
                .package("proptest", &[])
                .dep("sim-core", "proptest", Some("dev"));
        let found = violations(fixture);
        assert!(
            found.iter().all(|v| !v.contains("closure of sim-core")),
            "dev dependency leaked into the normal closure: {found:?}"
        );
    }
}

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
    ("xtask", &[]),
];

/// Positive list for the normal and build closure of `sim-core` (ADR-0000).
pub const CORE_ALLOWED: [&str; 3] = ["sim-core", "sim-math", "libm"];

/// Further packages allowed in the normal and build closure of the tool crates.
pub const TOOL_ALLOWED: [&str; 7] = [
    "sim-cli",
    "xtask",
    "serde_json",
    "serde_core",
    "itoa",
    "memchr",
    "zmij",
];

/// Packages allowed only in the test graph (reached through dev-dependencies).
pub const DEV_ALLOWED: [&str; 0] = [];

/// Resolved features allowed per package in the closure of `sim-core`; a package that
/// is not listed must resolve without features (ADR-0000 B1: feature unification).
pub const CORE_FEATURES_ALLOWED: [(&str, &[&str]); 0] = [];

/// Dependencies pinned by ADR-0003 (AT1 to AT3): package, the only member that may
/// depend on it directly, the exact version requirement, and the features it may enable.
/// Default features must be off.
pub const PINNED: [(&str, &str, &str, &[&str]); 1] = [("libm", "sim-math", "=0.2.16", &[])];

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

/// All violations of the architecture rules in `metadata` (output of
/// `cargo metadata --format-version 1`).
pub fn check(metadata: &Value) -> Vec<String> {
    let graph = match Graph::parse(metadata) {
        Ok(graph) => graph,
        Err(message) => return vec![format!("cargo metadata: {message}")],
    };
    let mut violations = Vec::new();
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

fn check_pinned(graph: &Graph, violations: &mut Vec<String>) {
    for (id, specs) in &graph.specs {
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
            for feature in &spec.features {
                if !features.contains(feature) {
                    violations.push(format!(
                        "{dependent} enables feature {feature} of {package} (ADR-0003)"
                    ));
                }
            }
        }
    }
}

/// A dependency as declared in a package manifest.
struct Spec<'a> {
    name: &'a str,
    requirement: &'a str,
    default_features: bool,
    features: Vec<&'a str>,
}

/// The parts of `cargo metadata` the rules need, keyed by package id.
struct Graph<'a> {
    names: BTreeMap<&'a str, &'a str>,
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
            declared: BTreeMap::new(),
            edges: BTreeMap::new(),
            resolved: BTreeMap::new(),
            specs: BTreeMap::new(),
            members: Vec::new(),
        };
        for package in array(metadata, "packages")? {
            let id = string(package, "id")?;
            graph.names.insert(id, string(package, "name")?);
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
                members: None,
            };
            for name in MEMBERS.into_iter().chain([
                "libm",
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
                .dep("sim-core", "sim-math", None)
                .dep("sim-cli", "sim-core", None)
                .dep("xtask", "serde_json", None)
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
            let id = |name: &str| format!("id+{name}#0.1.0");
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
                        .map(|(_, to, requirement, default_features, features)| {
                            json!({
                                "name": to,
                                "req": requirement,
                                "uses_default_features": default_features,
                                "features": features,
                                "kind": null
                            })
                        })
                        .collect();
                    json!({
                        "name": name,
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
                                "name": to.replace('-', "_"),
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
        check(&fixture.build())
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
        let fixture =
            Fixture::workspace()
                .package("proptest", &[])
                .dep("sim-core", "proptest", Some("dev"));
        assert_reports(fixture, "proptest");
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

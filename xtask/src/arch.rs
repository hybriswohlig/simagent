//! Architecture test over `cargo metadata` (ADR-0000; ADR-0003 AT1 to AT5 grow here as
//! their dependencies arrive). Every rule is data in this file: changing it changes a
//! guard and belongs in its own commit with a five-line report (ADR-0000).

use serde_json::Value;

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
pub const CORE_ALLOWED: [&str; 2] = ["sim-core", "sim-math"];

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
pub fn check(_metadata: &Value) -> Vec<String> {
    Vec::new()
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
        members: Option<Vec<String>>,
    }

    impl Fixture {
        /// The real shape of the workspace after step 1b.
        fn workspace() -> Self {
            let mut fixture = Fixture {
                packages: Vec::new(),
                deps: Vec::new(),
                features: Vec::new(),
                members: None,
            };
            for name in
                MEMBERS
                    .into_iter()
                    .chain(["serde_json", "serde_core", "itoa", "memchr", "zmij"])
            {
                fixture = fixture.package(name, &[]);
            }
            fixture
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
                    json!({ "name": name, "id": id(name), "features": own })
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
        let fixture = Fixture::workspace()
            .package("libm", &[])
            .dep("sim-math", "libm", None);
        assert_reports(fixture, "libm");
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

//! Source and config scans of the gate (ADR-0000, ADR-0003). Text based for now; the
//! token based scans S1 to S5 of ADR-0003 arrive with the types they inspect (step 2).

/// Lints that must stay at `forbid`, with the table they live in.
pub const FORBID_LINTS: [(&str, &str); 5] = [
    ("rust", "unsafe_code"),
    ("rust", "private_interfaces"),
    ("clippy", "disallowed_types"),
    ("clippy", "disallowed_methods"),
    ("clippy", "as_conversions"),
];

/// Lint names and groups that an `allow` or `expect` attribute must never mention.
pub const GUARDED_NAMES: [&str; 8] = [
    "unsafe_code",
    "private_interfaces",
    "disallowed_types",
    "disallowed_methods",
    "as_conversions",
    "clippy::style",
    "clippy::restriction",
    "clippy::all",
];

/// Line numbers (1 based) of `extern crate std` that are not directly preceded by
/// `#[cfg(test)]`.
pub fn extern_std_outside_test(_source: &str) -> Vec<usize> {
    Vec::new()
}

/// Attributes that `allow` or `expect` a guarded lint, as found in the source.
pub fn guarded_allows(_source: &str) -> Vec<String> {
    Vec::new()
}

/// The text between `# BEGIN COMMON` and `# END COMMON` of a clippy.toml.
pub fn common_block(_clippy_toml: &str) -> Option<String> {
    None
}

/// Lints of `FORBID_LINTS` that are not set to "forbid" in the root Cargo.toml.
pub fn lints_not_forbidden(_root_cargo_toml: &str) -> Vec<String> {
    Vec::new()
}

/// True if a member Cargo.toml inherits the workspace lints (`[lints]` with
/// `workspace = true`).
pub fn inherits_workspace_lints(_member_cargo_toml: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extern_std_is_allowed_only_under_cfg_test() {
        let source = "#![no_std]\nextern crate alloc;\n\n#[cfg(test)]\nextern crate std;\n";
        assert!(extern_std_outside_test(source).is_empty());
        let bad = "#![no_std]\nextern crate std;\n// extern crate std; in a comment\n";
        assert_eq!(extern_std_outside_test(bad), vec![2]);
        let renamed = "#[cfg(test)]\nextern crate alloc;\nextern crate std as s;\n";
        assert_eq!(extern_std_outside_test(renamed), vec![3]);
    }

    #[test]
    fn guarded_allows_are_found() {
        assert!(guarded_allows("#[allow(dead_code)]\nfn f() { x.expect(\"a\"); }").is_empty());
        let found = guarded_allows(
            "#[allow(clippy::disallowed_types)]\nfn f() {}\n#![expect(\n    clippy::as_conversions\n)]\n#[cfg_attr(test, allow(unsafe_code))]\n#[allow(clippy::style)]",
        );
        assert_eq!(found.len(), 4, "{found:?}");
    }

    #[test]
    fn the_common_block_is_extracted() {
        let text = "# head\n# BEGIN COMMON x\na = 1\n# END COMMON\nb = 2\n";
        assert_eq!(common_block(text).as_deref(), Some("a = 1\n"));
        assert!(common_block("a = 1\n").is_none());
    }

    #[test]
    fn lint_levels_must_stay_forbid() {
        let good = "[workspace.lints.rust]\nunsafe_code = \"forbid\"\nprivate_interfaces = \"forbid\"\n\n[workspace.lints.clippy]\ndisallowed_types = \"forbid\"\ndisallowed_methods = \"forbid\"\nas_conversions = \"forbid\"\n";
        assert!(lints_not_forbidden(good).is_empty());
        let lowered = good.replace("as_conversions = \"forbid\"", "as_conversions = \"deny\"");
        assert_eq!(
            lints_not_forbidden(&lowered),
            vec!["clippy::as_conversions".to_string()]
        );
        let moved = good.replace(
            "[workspace.lints.clippy]\ndisallowed_types = \"forbid\"\n",
            "[workspace.lints.clippy]\n",
        );
        let moved = moved.replace(
            "[workspace.lints.rust]\n",
            "[workspace.lints.rust]\ndisallowed_types = \"forbid\"\n",
        );
        assert_eq!(
            lints_not_forbidden(&moved),
            vec!["clippy::disallowed_types".to_string()]
        );
    }

    #[test]
    fn members_inherit_the_workspace_lints() {
        assert!(inherits_workspace_lints(
            "[package]\nname = \"a\"\n\n[lints]\nworkspace = true\n"
        ));
        assert!(!inherits_workspace_lints("[package]\nname = \"a\"\n"));
        assert!(!inherits_workspace_lints("[lints]\nworkspace = false\n"));
        assert!(!inherits_workspace_lints(
            "[lints.rust]\nunsafe_code = \"allow\"\n"
        ));
    }
}

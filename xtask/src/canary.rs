//! Canaries (ADR-0000, ADR-0003): deliberate violations that must turn red with the
//! expected diagnostic on every gate run. A typo in a clippy.toml silently disables an
//! entry and a lowered lint level lets a local `allow` through; a canary notices both.
//! Each canary is a tiny standalone crate under `target/xtask-canaries/`, built with the
//! real `[workspace.lints]` of the root Cargo.toml and the real clippy.toml of the
//! member it stands for.

use std::path::Path;
use std::process::Command;

/// Float methods that only `sim_math` may provide (ADR-0003). Owned by the canary, not
/// derived from clippy.toml, so a missing entry there is noticed. Second field: the
/// arguments of the call.
pub const FLOAT_METHODS: [(&str, &str); 38] = [
    ("exp", ""),
    ("exp2", ""),
    ("exp_m1", ""),
    ("ln", ""),
    ("log", "2.0"),
    ("log2", ""),
    ("log10", ""),
    ("ln_1p", ""),
    ("powf", "2.0"),
    ("powi", "2"),
    ("sqrt", ""),
    ("cbrt", ""),
    ("hypot", "2.0"),
    ("sin", ""),
    ("cos", ""),
    ("tan", ""),
    ("sin_cos", ""),
    ("asin", ""),
    ("acos", ""),
    ("atan", ""),
    ("atan2", "2.0"),
    ("sinh", ""),
    ("cosh", ""),
    ("tanh", ""),
    ("asinh", ""),
    ("acosh", ""),
    ("atanh", ""),
    ("floor", ""),
    ("ceil", ""),
    ("round", ""),
    ("round_ties_even", ""),
    ("trunc", ""),
    ("fract", ""),
    ("mul_add", "2.0, 3.0"),
    ("rem_euclid", "2.0"),
    ("div_euclid", "2.0"),
    ("min", "2.0"),
    ("max", "2.0"),
];

/// How a canary is checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    /// `cargo clippy -- -D warnings` on the host.
    Clippy,
    /// `cargo build` for the target without std.
    BuildNoStd,
}

/// One deliberate violation and the diagnostics it must produce.
#[derive(Debug)]
pub struct Canary {
    pub name: String,
    /// Workspace member whose clippy.toml the canary uses.
    pub member: &'static str,
    pub tool: Tool,
    pub source: String,
    /// Each text must occur exactly `count` times in the diagnostics.
    pub expect: Vec<(String, usize)>,
}

/// The canaries of the gate.
pub fn canaries() -> Vec<Canary> {
    Vec::new()
}

/// The `[lints...]` tables for a canary crate, taken from the `[workspace.lints...]`
/// tables of the root Cargo.toml.
pub fn lints_for_canary(_root_cargo_toml: &str) -> String {
    String::new()
}

/// Violations of the expectations of `canary`, given whether the build failed and the
/// diagnostics it printed. Empty if the canary behaved as expected.
pub fn judge(_canary: &Canary, _failed: bool, _diagnostics: &str) -> Vec<String> {
    Vec::new()
}

/// Builds every canary and returns the ones that did not turn red as expected.
pub fn run(
    root: &Path,
    no_std_target: &str,
    member_dir: impl Fn(&str) -> std::path::PathBuf,
) -> Result<Vec<String>, String> {
    let _ = (root, no_std_target, &member_dir, Command::new("true"));
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named<'a>(all: &'a [Canary], name: &str) -> &'a Canary {
        all.iter()
            .find(|canary| canary.name == name)
            .unwrap_or_else(|| panic!("no canary named {name}"))
    }

    #[test]
    fn every_member_has_a_method_canary_for_every_float_method() {
        let all = canaries();
        for member in ["sim-math", "sim-core", "sim-cli", "xtask"] {
            let canary = named(&all, &format!("methods-{member}"));
            assert_eq!(canary.member, member);
            assert_eq!(canary.tool, Tool::Clippy);
            for (method, _) in FLOAT_METHODS {
                for ty in ["f64", "f32"] {
                    let text = format!("use of a disallowed method `{ty}::{method}`");
                    assert!(
                        canary.expect.contains(&(text.clone(), 1)),
                        "{member}: missing {text}"
                    );
                }
            }
        }
    }

    #[test]
    fn core_members_pull_in_std_for_the_method_canary() {
        let all = canaries();
        for member in ["sim-math", "sim-core"] {
            let source = &named(&all, &format!("methods-{member}")).source;
            assert!(source.contains("#![no_std]") && source.contains("extern crate std;"));
        }
    }

    #[test]
    fn the_guard_canaries_exist() {
        let all = canaries();
        let barrier = named(&all, "no-std-barrier");
        assert_eq!(barrier.tool, Tool::BuildNoStd);
        assert!(
            barrier
                .expect
                .iter()
                .any(|(text, _)| text.contains("can't find crate for `std`"))
        );
        for name in [
            "types-sim-core",
            "types-xtask",
            "as-conversions",
            "allow-under-forbid",
            "unsafe-code",
        ] {
            named(&all, name);
        }
    }

    #[test]
    fn workspace_lints_become_package_lints() {
        let root = "[workspace]\nmembers = []\n\n[workspace.lints.rust]\nunsafe_code = \"forbid\"\n\n[workspace.lints.clippy]\nas_conversions = \"forbid\"\n\n[profile.release]\noverflow-checks = true\n";
        assert_eq!(
            lints_for_canary(root),
            "[lints.rust]\nunsafe_code = \"forbid\"\n\n[lints.clippy]\nas_conversions = \"forbid\"\n\n"
        );
    }

    #[test]
    fn judging_counts_exact_occurrences() {
        let canary = Canary {
            name: "demo".to_string(),
            member: "sim-core",
            tool: Tool::Clippy,
            source: String::new(),
            expect: vec![("disallowed method `f64::exp`".to_string(), 1)],
        };
        let one = "error: use of a disallowed method `f64::exp`\n";
        assert!(judge(&canary, true, one).is_empty());
        assert!(
            !judge(&canary, false, one).is_empty(),
            "a canary that builds is broken"
        );
        assert!(!judge(&canary, true, "error: something else\n").is_empty());
        let two = format!("{one}{one}");
        assert!(!judge(&canary, true, &two).is_empty());
    }
}

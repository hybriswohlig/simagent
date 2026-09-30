//! Canaries (ADR-0000, ADR-0003): deliberate violations that must turn red with the
//! expected diagnostic on every gate run. A typo in a clippy.toml silently disables an
//! entry and a lowered lint level lets a local `allow` through; a canary notices both.
//! Each canary is a tiny standalone crate under `target/xtask-canaries/`, built with the
//! real `[workspace.lints]` of the root Cargo.toml and the real clippy.toml of the
//! member it stands for.

use std::path::{Path, PathBuf};
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

/// Workspace members without std; their canaries pull std in on purpose, otherwise a
/// missing float method would fail with E0599 instead of the guard under test.
const NO_STD_MEMBERS: [&str; 2] = ["sim-math", "sim-core"];

/// Types banned in the core crates, as clippy reports them (canonical paths).
const CORE_TYPES: [(&str, &str); 9] = [
    (
        "std::collections::HashMap",
        "std::collections::HashMap<u8, u8>",
    ),
    ("std::collections::HashSet", "std::collections::HashSet<u8>"),
    ("std::time::Instant", "std::time::Instant"),
    ("std::time::SystemTime", "std::time::SystemTime"),
    (
        "std::collections::hash_map::DefaultHasher",
        "std::collections::hash_map::DefaultHasher",
    ),
    (
        "std::collections::hash_map::RandomState",
        "std::collections::hash_map::RandomState",
    ),
    (
        "core::hash::BuildHasherDefault",
        "core::hash::BuildHasherDefault<u8>",
    ),
    (
        "alloc::collections::BinaryHeap",
        "alloc::collections::BinaryHeap<u8>",
    ),
    ("f32", "f32"),
];

/// Types banned in the tool crates.
const TOOL_TYPES: [(&str, &str); 5] = [
    (
        "std::collections::HashMap",
        "std::collections::HashMap<u8, u8>",
    ),
    ("std::collections::HashSet", "std::collections::HashSet<u8>"),
    (
        "std::collections::hash_map::DefaultHasher",
        "std::collections::hash_map::DefaultHasher",
    ),
    (
        "std::collections::hash_map::RandomState",
        "std::collections::hash_map::RandomState",
    ),
    (
        "core::hash::BuildHasherDefault",
        "core::hash::BuildHasherDefault<u8>",
    ),
];

/// The canaries of the gate.
pub fn canaries() -> Vec<Canary> {
    let mut all = Vec::new();
    for member in ["sim-math", "sim-core", "sim-cli", "xtask"] {
        all.push(method_canary(member));
    }
    for member in ["sim-math", "sim-core"] {
        all.push(type_canary(member, &CORE_TYPES));
    }
    for member in ["sim-cli", "xtask"] {
        all.push(type_canary(member, &TOOL_TYPES));
    }
    all.push(Canary {
        name: "no-std-barrier".to_string(),
        member: "sim-core",
        tool: Tool::BuildNoStd,
        source: "#![no_std]\nextern crate std;\npub fn probe() {}\n".to_string(),
        expect: vec![("can't find crate for `std`".to_string(), 1)],
    });
    all.push(Canary {
        name: "as-conversions".to_string(),
        member: "sim-core",
        tool: Tool::Clippy,
        source: "#![no_std]\npub fn probe(x: f64) -> i64 {\n    x as i64\n}\n".to_string(),
        expect: vec![(
            "using a potentially dangerous silent `as` conversion".to_string(),
            1,
        )],
    });
    all.push(Canary {
        name: "allow-under-forbid".to_string(),
        member: "sim-core",
        tool: Tool::Clippy,
        source: "#![no_std]\n#[allow(clippy::disallowed_types)]\npub fn probe() {}\n".to_string(),
        expect: vec![(
            "allow(clippy::disallowed_types) incompatible with previous forbid".to_string(),
            1,
        )],
    });
    all.push(Canary {
        name: "unsafe-code".to_string(),
        member: "sim-core",
        tool: Tool::Clippy,
        source: "#![no_std]\npub fn probe() {\n    unsafe {}\n}\n".to_string(),
        expect: vec![("usage of an `unsafe` block".to_string(), 1)],
    });
    all
}

fn prelude(member: &str) -> &'static str {
    if NO_STD_MEMBERS.contains(&member) {
        "#![no_std]\nextern crate alloc;\nextern crate std;\n"
    } else {
        ""
    }
}

/// Calls every banned float method once per line, for f64 and f32.
fn method_canary(member: &'static str) -> Canary {
    let mut source = format!("{}pub fn probe(x: f64, y: f32) {{\n", prelude(member));
    let mut expect = Vec::new();
    for (variable, ty) in [("x", "f64"), ("y", "f32")] {
        for (method, arguments) in FLOAT_METHODS {
            source.push_str(&format!("    let _ = {variable}.{method}({arguments});\n"));
            expect.push((format!("use of a disallowed method `{ty}::{method}`"), 1));
        }
    }
    source.push_str("}\n");
    Canary {
        name: format!("methods-{member}"),
        member,
        tool: Tool::Clippy,
        source,
        expect,
    }
}

/// Uses every banned type once per line.
fn type_canary(member: &'static str, types: &[(&str, &str)]) -> Canary {
    let mut source = format!("{}pub fn probe() {{\n", prelude(member));
    let mut expect = Vec::new();
    for (index, (reported, written)) in types.iter().enumerate() {
        source.push_str(&format!("    let _v{index}: Option<{written}> = None;\n"));
        expect.push((format!("use of a disallowed type `{reported}`"), 1));
    }
    source.push_str("}\n");
    Canary {
        name: format!("types-{member}"),
        member,
        tool: Tool::Clippy,
        source,
        expect,
    }
}

/// The `[lints...]` tables for a canary crate, taken from the `[workspace.lints...]`
/// tables of the root Cargo.toml.
pub fn lints_for_canary(root_cargo_toml: &str) -> String {
    let mut lints = String::new();
    let mut inside = false;
    for line in root_cargo_toml.lines() {
        if line.starts_with('[') {
            inside = line.starts_with("[workspace.lints");
        }
        if inside {
            lints.push_str(&line.replacen("[workspace.lints", "[lints", 1));
            lints.push('\n');
        }
    }
    lints
}

/// Violations of the expectations of `canary`, given whether the build failed and the
/// diagnostics it printed. Empty if the canary behaved as expected.
pub fn judge(canary: &Canary, failed: bool, diagnostics: &str) -> Vec<String> {
    let mut problems = Vec::new();
    if !failed {
        problems.push(format!(
            "canary {} built without error: its guard does not fire",
            canary.name
        ));
    }
    for (text, count) in &canary.expect {
        let found = diagnostics.matches(text.as_str()).count();
        if found != *count {
            problems.push(format!(
                "canary {}: expected {count} x `{text}`, found {found}",
                canary.name
            ));
        }
    }
    problems
}

/// Builds every canary and returns the ones that did not turn red as expected.
pub fn run(
    root: &Path,
    no_std_target: &str,
    member_dir: impl Fn(&str) -> PathBuf,
    cargo: &str,
) -> Result<Vec<String>, String> {
    let base = root.join("target").join("xtask-canaries");
    let lints = lints_for_canary(&read(&root.join("Cargo.toml"))?);
    let mut problems = Vec::new();
    for canary in canaries() {
        let dir = base.join(&canary.name);
        std::fs::create_dir_all(&dir)
            .map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
        let manifest = format!(
            "[package]\nname = \"canary-{}\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n[lib]\npath = \"lib.rs\"\n\n[workspace]\n\n{lints}",
            canary.name
        );
        write(&dir.join("Cargo.toml"), &manifest)?;
        write(&dir.join("lib.rs"), &canary.source)?;
        let clippy_toml = read(&member_dir(canary.member).join("clippy.toml"))?;
        write(&dir.join("clippy.toml"), &clippy_toml)?;
        let args: Vec<&str> = match canary.tool {
            Tool::Clippy => vec!["clippy", "--quiet", "--", "-D", "warnings"],
            Tool::BuildNoStd => vec!["build", "--quiet", "--target", no_std_target],
        };
        let output = Command::new(cargo)
            .args(&args)
            .current_dir(&dir)
            .env("CARGO_TARGET_DIR", base.join("target"))
            .output()
            .map_err(|error| format!("cannot run cargo for canary {}: {error}", canary.name))?;
        let diagnostics = String::from_utf8_lossy(&output.stderr);
        problems.extend(judge(&canary, !output.status.success(), &diagnostics));
    }
    Ok(problems)
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|error| format!("cannot write {}: {error}", path.display()))
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

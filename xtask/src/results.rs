//! The rules on result changes between a pull request and its merge base with
//! `origin/main` (ADR-0003 6.4 and 6.5). They are pure functions over two views of the
//! repository; `main.rs` fills the views from git and from the working tree.

use serde_json::Value;
use std::collections::BTreeMap;

/// What the rules look at, at one commit.
#[derive(Clone, Debug, Default)]
pub struct Results {
    /// `CORE_VERSION` in `crates/sim-core/src/version.rs`; `None` if there is none.
    pub core_version: Option<u32>,
    /// Every file below `tests/golden/`, by its path relative to the repository root.
    pub goldens: BTreeMap<String, Vec<u8>>,
    /// The content of `rust-toolchain.toml`.
    pub toolchain: String,
    /// The versions `Cargo.lock` resolves for the packages of `LOCKED_PACKAGES`.
    pub locked: BTreeMap<String, String>,
}

/// Packages whose update must not change results in the same pull request (ADR-0003 6.5).
pub const LOCKED_PACKAGES: [&str; 3] = ["postcard", "xxhash-rust", "serde_json"];

/// `CORE_VERSION` as written in `version.rs`.
pub fn parse_core_version(source: &str) -> Option<u32> {
    source.lines().find_map(|line| {
        line.trim()
            .strip_prefix("pub const CORE_VERSION: u32 = ")?
            .strip_suffix(';')?
            .parse()
            .ok()
    })
}

/// The versions of `LOCKED_PACKAGES` in the text of a `Cargo.lock`. A package that is
/// locked in two versions gets both, so that a change of either shows.
pub fn locked_versions(cargo_lock: &str) -> BTreeMap<String, String> {
    let quoted = |line: &str, key: &str| {
        line.strip_prefix(key)?
            .strip_prefix(" = \"")?
            .strip_suffix('"')
            .map(str::to_string)
    };
    let mut versions: BTreeMap<String, String> = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in cargo_lock.lines() {
        if let Some(name) = quoted(line, "name") {
            current = LOCKED_PACKAGES.contains(&name.as_str()).then_some(name);
        } else if let (Some(version), Some(name)) = (quoted(line, "version"), current.take()) {
            versions
                .entry(name)
                .and_modify(|known| *known = format!("{known}, {version}"))
                .or_insert(version);
        }
    }
    versions
}

/// Year hashes by year and the log hash of a scenario golden, if `bytes` is one.
fn scenario_hashes(bytes: &[u8]) -> Option<(BTreeMap<i64, String>, String)> {
    let golden: Value = serde_json::from_slice(bytes).ok()?;
    let log_hash = golden.get("log_hash")?.as_str()?.to_string();
    let mut years = BTreeMap::new();
    for entry in golden.get("year_hashes")?.as_array()? {
        years.insert(
            entry.get("year")?.as_i64()?,
            entry.get("hash")?.as_str()?.to_string(),
        );
    }
    Some((years, log_hash))
}

/// True if an existing result of the golden `path` differs between `base` and `head`.
fn result_changed(path: &str, base: &[u8], head: &[u8]) -> bool {
    if base == head {
        return false;
    }
    if path.ends_with(".json") {
        // A scenario golden: later years may be added, existing hashes must stay.
        if let (Some((base_years, base_log)), Some((head_years, head_log))) =
            (scenario_hashes(base), scenario_hashes(head))
        {
            return base_log != head_log
                || base_years
                    .iter()
                    .any(|(year, hash)| head_years.get(year) != Some(hash));
        }
    }
    if path.ends_with(".csv") || path.ends_with(".txt") {
        // Known answers and code lists: lines may be added, existing lines must stay.
        let (base, head) = (String::from_utf8_lossy(base), String::from_utf8_lossy(head));
        return base
            .lines()
            .any(|line| !head.lines().any(|kept| kept == line));
    }
    true
}

/// The goldens of `base` whose existing results differ in `head`: a year hash or the
/// log hash of a scenario golden, the bytes of a golden save, an existing line of a
/// known-answer file. New files, new years and new lines are no change.
pub fn changed_results(base: &Results, head: &Results) -> Vec<String> {
    base.goldens
        .iter()
        .filter(|(path, before)| match head.goldens.get(*path) {
            Some(after) => result_changed(path, before, after),
            None => true,
        })
        .map(|(path, _)| path.clone())
        .collect()
}

/// ADR-0003 6.4 and 6.5: the violations of `head` against `base`.
pub fn violations(base: &Results, head: &Results) -> Vec<String> {
    let mut found = Vec::new();
    let changed = changed_results(base, head);
    match (base.core_version, head.core_version) {
        // The first commit with a core version has nothing to compare with.
        (None, _) => {}
        (Some(before), None) => found.push(format!(
            "CORE_VERSION is gone, the base has {before} (ADR-0003 6.4)"
        )),
        (Some(before), Some(now)) => {
            let next = before.saturating_add(1);
            if changed.is_empty() {
                if now != before && now != next {
                    found.push(format!(
                        "CORE_VERSION is {now}; without a golden change it must be the base {before} or {next} (ADR-0003 6.4)"
                    ));
                }
            } else if now != next {
                for path in &changed {
                    found.push(format!(
                        "{path} changed, so CORE_VERSION must be {next} (base {before} + 1), it is {now} (ADR-0003 6.4)"
                    ));
                }
            }
        }
    }
    // 6.5: an update decides nothing about results.
    let results_touched = base.goldens != head.goldens || base.core_version != head.core_version;
    if results_touched {
        const RULE: &str =
            "the same pull request must not change goldens or CORE_VERSION (ADR-0003 6.5)";
        if !base.toolchain.is_empty() && base.toolchain != head.toolchain {
            found.push(format!("rust-toolchain.toml changed; {RULE}"));
        }
        for package in LOCKED_PACKAGES {
            if let (Some(before), Some(now)) = (base.locked.get(package), head.locked.get(package))
                && before != now
            {
                found.push(format!(
                    "{package} changed from {before} to {now} in Cargo.lock; {RULE}"
                ));
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOLDEN: &str = r#"{
  "core_version": 1,
  "data_hash": "2d06800538d394c2",
  "year_hashes": [
    { "year": 0, "hash": "00000000000000a0" },
    { "year": 1, "hash": "00000000000000a1" }
  ],
  "log_hash": "00000000000000ff"
}
"#;

    fn base() -> Results {
        let mut results = Results {
            core_version: Some(4),
            toolchain: "[toolchain]\nchannel = \"1.96.0\"\n".to_string(),
            ..Results::default()
        };
        let files: [(&str, &[u8]); 4] = [
            ("tests/golden/dummy.json", GOLDEN.as_bytes()),
            ("tests/golden/dummy.save", b"SIMAGENT\x01\x01"),
            (
                "tests/golden/kat/ceil.csv",
                b"# bits\n3ff0,3ff0,E\n4000,4000,E\n",
            ),
            ("tests/golden/log_codes.txt", b"payload 0 core:started\n"),
        ];
        for (path, bytes) in files {
            results.goldens.insert(path.to_string(), bytes.to_vec());
        }
        for (package, version) in [
            ("postcard", "1.1.3"),
            ("xxhash-rust", "0.8.19"),
            ("serde_json", "1.0.151"),
        ] {
            results
                .locked
                .insert(package.to_string(), version.to_string());
        }
        results
    }

    fn with(change: impl FnOnce(&mut Results)) -> Results {
        let mut results = base();
        change(&mut results);
        results
    }

    fn replace(results: &mut Results, path: &str, from: &str, to: &str) {
        let text = String::from_utf8(results.goldens[path].clone()).expect("UTF-8");
        assert!(text.contains(from), "{from:?} not in {path}");
        results
            .goldens
            .insert(path.to_string(), text.replacen(from, to, 1).into_bytes());
    }

    #[test]
    fn the_core_version_is_read_from_the_source() {
        let source =
            "//! doc\n\n/// Rises.\npub const CORE_VERSION: u32 = 17;\n\npub const X: u64 = 3;\n";
        assert_eq!(parse_core_version(source), Some(17));
        assert_eq!(
            parse_core_version("pub const CORE_VERSION: u32 = 1;"),
            Some(1)
        );
        assert_eq!(
            parse_core_version("// pub const CORE_VERSION: u32 = 9;\n"),
            None
        );
        assert_eq!(parse_core_version("pub const CORE_VERSION: u32 = x;"), None);
        assert_eq!(parse_core_version(""), None);
        // Review finding: a second line of that form (in a block comment, in an inner
        // module) would let the gate read another number than the core uses.
        for decoy in [
            "/*\npub const CORE_VERSION: u32 = 9;\n*/\npub const CORE_VERSION: u32 = 1;\n",
            "mod old {\n    pub const CORE_VERSION: u32 = 9;\n}\npub const CORE_VERSION: u32 = 1;\n",
            "pub const CORE_VERSION: u32 = 1;\npub const CORE_VERSION: u32 = 1;\n",
        ] {
            assert_eq!(parse_core_version(decoy), None, "{decoy}");
        }
    }

    #[test]
    fn locked_versions_come_from_the_lockfile() {
        let lock = concat!(
            "version = 4\n\n",
            "[[package]]\nname = \"postcard\"\nversion = \"1.1.3\"\nsource = \"registry\"\n\n",
            "[[package]]\nname = \"serde\"\nversion = \"1.0.229\"\n\n",
            "[[package]]\nname = \"serde_json\"\nversion = \"1.0.151\"\n\n",
            "[[package]]\nname = \"xxhash-rust\"\nversion = \"0.8.19\"\n",
        );
        let versions = locked_versions(lock);
        assert_eq!(versions.len(), 3);
        assert_eq!(versions["postcard"], "1.1.3");
        assert_eq!(versions["serde_json"], "1.0.151");
        assert_eq!(versions["xxhash-rust"], "0.8.19");
    }

    #[test]
    fn nothing_changed_is_no_violation() {
        assert_eq!(violations(&base(), &base()), Vec::<String>::new());
        assert_eq!(changed_results(&base(), &base()), Vec::<String>::new());
    }

    /// Additions are no result change: a new golden, a new known-answer line, a new code
    /// in a code list, and anything in a comment line. The core version may stay or rise
    /// by one.
    #[test]
    fn additions_are_no_result_change() {
        let head = with(|r| {
            r.goldens.insert(
                "tests/golden/village.json".to_string(),
                GOLDEN.as_bytes().to_vec(),
            );
            r.goldens.insert(
                "tests/golden/village.save".to_string(),
                b"SIMAGENT".to_vec(),
            );
            replace(
                r,
                "tests/golden/kat/ceil.csv",
                "4000,4000,E\n",
                "4000,4000,E\n4008,4008,E\n",
            );
            // Review finding: a comment line is no result.
            replace(
                r,
                "tests/golden/kat/ceil.csv",
                "# bits\n",
                "# generated by another tool\n# with two comment lines\n",
            );
            replace(
                r,
                "tests/golden/log_codes.txt",
                "core:started\n",
                "core:started\npayload 1 core:x\n",
            );
        });
        assert_eq!(changed_results(&base(), &head), Vec::<String>::new());
        assert_eq!(violations(&base(), &head), Vec::<String>::new());
        let bumped = with(|r| r.core_version = Some(5));
        assert_eq!(
            violations(&base(), &bumped),
            Vec::<String>::new(),
            "a result change outside the goldens"
        );
    }

    /// ADR-0003 6.4 and K8: a changed golden needs exactly base + 1. A scenario that
    /// runs longer keeps its year hashes, but its log hash is the hash of a longer log:
    /// that is a changed log hash and counts (review finding: a new year never comes
    /// without it).
    #[test]
    fn a_changed_golden_needs_the_next_core_version() {
        type Change = dyn Fn(&mut Results);
        let cases: [(&str, &Change); 8] = [
            ("tests/golden/dummy.json", &|r| {
                replace(
                    r,
                    "tests/golden/dummy.json",
                    "{ \"year\": 1, \"hash\": \"00000000000000a1\" }",
                    "{ \"year\": 1, \"hash\": \"00000000000000a1\" },\n    { \"year\": 2, \"hash\": \"00000000000000a2\" }",
                );
                replace(
                    r,
                    "tests/golden/dummy.json",
                    "00000000000000ff",
                    "00000000000000fd",
                );
            }),
            ("tests/golden/log_codes.txt", &|r| {
                replace(
                    r,
                    "tests/golden/log_codes.txt",
                    "payload 0 core:started",
                    "payload 0 core:begun",
                )
            }),
            ("tests/golden/dummy.json", &|r| {
                replace(
                    r,
                    "tests/golden/dummy.json",
                    "00000000000000a1",
                    "00000000000000b1",
                )
            }),
            ("tests/golden/dummy.json", &|r| {
                replace(
                    r,
                    "tests/golden/dummy.json",
                    "00000000000000ff",
                    "00000000000000fe",
                )
            }),
            ("tests/golden/dummy.json", &|r| {
                replace(
                    r,
                    "tests/golden/dummy.json",
                    "    { \"year\": 0, \"hash\": \"00000000000000a0\" },\n",
                    "",
                )
            }),
            ("tests/golden/dummy.save", &|r| {
                r.goldens.insert(
                    "tests/golden/dummy.save".to_string(),
                    b"SIMAGENT\x01\x02".to_vec(),
                );
            }),
            ("tests/golden/kat/ceil.csv", &|r| {
                replace(r, "tests/golden/kat/ceil.csv", "3ff0,3ff0,E", "3ff0,3ff1,E")
            }),
            ("tests/golden/dummy.save", &|r| {
                r.goldens.remove("tests/golden/dummy.save");
            }),
        ];
        for (path, change) in cases {
            let unchanged_version = with(change);
            assert_eq!(
                changed_results(&base(), &unchanged_version),
                vec![path.to_string()]
            );
            assert_eq!(
                violations(&base(), &unchanged_version),
                vec![format!(
                    "{path} changed, so CORE_VERSION must be 5 (base 4 + 1), it is 4 (ADR-0003 6.4)"
                )]
            );
            let bumped = with(|r| {
                change(r);
                r.core_version = Some(5);
            });
            assert_eq!(violations(&base(), &bumped), Vec::<String>::new(), "{path}");
            let too_far = with(|r| {
                change(r);
                r.core_version = Some(6);
            });
            assert_eq!(violations(&base(), &too_far).len(), 1, "{path}");
        }
    }

    #[test]
    fn the_core_version_moves_by_at_most_one() {
        for version in [Some(3), Some(6), Some(0), None] {
            let head = with(|r| r.core_version = version);
            let found = violations(&base(), &head);
            assert_eq!(found.len(), 1, "{version:?}: {found:?}");
            assert!(found[0].contains("CORE_VERSION"), "{found:?}");
        }
        // The first commit that has a core version: nothing to compare with.
        let before = with(|r| r.core_version = None);
        assert_eq!(violations(&before, &base()), Vec::<String>::new());
    }

    /// ADR-0003 6.5 and K11: an update of the toolchain or of a pinned package must not
    /// come with a golden change or a new core version.
    #[test]
    fn an_update_must_not_change_results() {
        let update_only = with(|r| {
            r.locked.insert("postcard".to_string(), "1.1.4".to_string());
        });
        assert_eq!(violations(&base(), &update_only), Vec::<String>::new());
        let toolchain_only =
            with(|r| r.toolchain = "[toolchain]\nchannel = \"1.97.0\"\n".to_string());
        assert_eq!(violations(&base(), &toolchain_only), Vec::<String>::new());
        let with_golden = with(|r| {
            r.locked.insert("postcard".to_string(), "1.1.4".to_string());
            r.core_version = Some(5);
            replace(
                r,
                "tests/golden/dummy.json",
                "00000000000000a1",
                "00000000000000b1",
            );
        });
        assert_eq!(
            violations(&base(), &with_golden),
            vec![
                "postcard changed from 1.1.3 to 1.1.4 in Cargo.lock; the same pull request must not change goldens or CORE_VERSION (ADR-0003 6.5)"
                    .to_string()
            ]
        );
        let with_version = with(|r| {
            r.toolchain = "[toolchain]\nchannel = \"1.97.0\"\n".to_string();
            r.core_version = Some(5);
        });
        assert_eq!(
            violations(&base(), &with_version),
            vec![
                "rust-toolchain.toml changed; the same pull request must not change goldens or CORE_VERSION (ADR-0003 6.5)"
                    .to_string()
            ]
        );
        // A new golden next to an update is a golden change as well.
        let with_new_golden = with(|r| {
            r.locked
                .insert("serde_json".to_string(), "1.0.152".to_string());
            r.goldens.insert(
                "tests/golden/new.json".to_string(),
                GOLDEN.as_bytes().to_vec(),
            );
        });
        assert_eq!(violations(&base(), &with_new_golden).len(), 1);
    }
}
